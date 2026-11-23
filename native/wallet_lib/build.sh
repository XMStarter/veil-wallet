#!/usr/bin/env bash
# Builds the Monero wallet library that Veil loads, from pinned sources.
#
#   build.sh <work-dir> [--with-daemon] [--windows]
#
# The result is <work-dir>/out/libmonero_wallet2_api_c.so and, with
# --with-daemon, <work-dir>/out/monerod for regtest tests.
#
# This is the development and test build: it links against the system's
# libraries. Release builds will use the reproducible toolchain instead.
#
# Needs git, cmake, ninja, a C++ compiler and these Debian/Ubuntu packages:
#   pkg-config libssl-dev libzmq3-dev libunbound-dev libsodium-dev
#   libboost-all-dev libhidapi-dev libusb-1.0-0-dev libudev-dev
#   libreadline-dev libexpat1-dev libpgm-dev libnorm-dev
#
# With --windows the library is built for 64-bit Windows instead, on Linux,
# into <work-dir>/out/windows/libmonero_wallet2_api_c.dll (and monerod.exe).
# The libraries it needs do not come from the system then: Monero's own
# dependency build compiles them from source for Windows, and they end up
# inside the one DLL. That needs the distribution's cross compiler and
# build tools:
#   g++-mingw-w64-x86-64 autoconf automake libtool pkg-config python3 gperf
set -euo pipefail

# monero_c wraps wallet2 in a C interface and adds Polyseed support. Its
# monero submodule pins the Monero release it is built from.
MONERO_C_REPO="https://github.com/MrCyjaneK/monero_c"
MONERO_C_COMMIT="a59deb815a9c806c09bb434fb9017763e2e944f2" # tag v0.18.5.3-RC1

work="${1:?usage: build.sh <work-dir> [--with-daemon] [--windows]}"
shift
with_daemon=""
windows=""
for option in "$@"; do
  case "$option" in
    --with-daemon) with_daemon=yes ;;
    --windows) windows=yes ;;
    *)
      echo "error: unknown option $option" >&2
      exit 2
      ;;
  esac
done
here="$(cd "$(dirname "$0")" && pwd)"
jobs="$(nproc 2>/dev/null || echo 2)"

mkdir -p "$work"
work="$(cd "$work" && pwd)"
src="$work/monero_c"

# apply_patches.sh commits the patches it applies, which needs an identity.
export GIT_AUTHOR_NAME="veil build" GIT_AUTHOR_EMAIL="build@invalid"
export GIT_COMMITTER_NAME="veil build" GIT_COMMITTER_EMAIL="build@invalid"

if [ ! -d "$src/.git" ]; then
  git init -q "$src"
  git -C "$src" remote add origin "$MONERO_C_REPO"
  git -C "$src" fetch -q --depth 1 origin "$MONERO_C_COMMIT"
  git -C "$src" checkout -q FETCH_HEAD
fi
if [ "$(git -C "$src" rev-parse HEAD)" != "$MONERO_C_COMMIT" ]; then
  echo "error: $src is not at the pinned commit $MONERO_C_COMMIT" >&2
  exit 1
fi

cd "$src"
if [ ! -f monero/.patch-applied ]; then
  git submodule update --init --force --depth 1 monero \
    || git submodule update --init --force monero
  ./apply_patches.sh monero
fi

# Veil's own patches, on top of monero_c's. They stay uncommitted, so that a
# changed patch set can be dropped and applied again: the marker file holds
# the checksum of the set that is applied.
patch_set="$(cat "$here"/patches/monero/*.patch "$here"/patches/monero_c/*.patch | sha256sum | cut -d' ' -f1)"
if [ "$(cat .veil-patched 2>/dev/null)" != "$patch_set" ]; then
  if [ -f .veil-patched ]; then
    echo "Veil's patches changed; applying them again"
    git -C monero checkout -- src
    git checkout -- monero_libwallet2_api_c
  fi
  for patch in "$here"/patches/monero/*.patch; do
    git -C monero apply --whitespace=nowarn "$patch"
  done
  for patch in "$here"/patches/monero_c/*.patch; do
    git apply --whitespace=nowarn "$patch"
  done
  echo "$patch_set" > .veil-patched
fi

if [ -n "$windows" ]; then
  host=x86_64-w64-mingw32
  depends="$src/monero/contrib/depends"
  # Monero's dependency build: Boost, OpenSSL, ZeroMQ, Unbound and the rest,
  # compiled for Windows from sources it downloads and checks by hash. Monero
  # needs the cross compiler in its POSIX-threads form, which the
  # distribution installs beside the default one.
  make -C "$depends" HOST="$host" -j"$jobs" \
    mingw32_CC="$host-gcc-posix" mingw32_CXX="$host-g++-posix"

  build="$work/build-windows"
  out="$work/out/windows"
  # Three things the wallet library's own build files leave out in this
  # setup: where the dependencies' libraries are; that Polyseed is linked in
  # statically (without the definition its functions are looked for in a
  # DLL that does not exist); and -static, so that the compiler's own
  # thread library goes into the DLL too and the result needs nothing but
  # Windows itself.
  cmake -S monero_libwallet2_api_c -B "$build" -G Ninja \
    -DCMAKE_TOOLCHAIN_FILE="$depends/$host/share/toolchain.cmake" \
    -DMONERO_FLAVOR=monero -DHOST_ABI="$host" -DOUTPUT_MODE=SHARED \
    -DCMAKE_BUILD_TYPE=Release -DUSE_DEVICE_TREZOR=OFF -DBUILD_TESTS=OFF \
    -DMANUAL_SUBMODULES=1 -DBUILD_DOCUMENTATION=OFF \
    -DCMAKE_SHARED_LINKER_FLAGS="-L$depends/$host/lib -static" \
    -DCMAKE_EXE_LINKER_FLAGS="-L$depends/$host/lib -static" \
    -DCMAKE_C_FLAGS="-DPOLYSEED_STATIC" -DCMAKE_CXX_FLAGS="-DPOLYSEED_STATIC"
  cmake --build "$build" -j"$jobs"
  library="libmonero_wallet2_api_c.dll"
  daemon="monerod.exe"
else
  build="$work/build"
  out="$work/out"
  cmake -S monero_libwallet2_api_c -B "$build" -G Ninja \
    -DMONERO_FLAVOR=monero -DHOST_ABI=x86_64-linux-gnu -DOUTPUT_MODE=SHARED \
    -DCMAKE_BUILD_TYPE=Release -DUSE_DEVICE_TREZOR=OFF -DBUILD_TESTS=OFF \
    -DMANUAL_SUBMODULES=1 -DBUILD_DOCUMENTATION=OFF
  cmake --build "$build" -j"$jobs"
  library="libmonero_wallet2_api_c.so"
  daemon="monerod"
fi

mkdir -p "$out"
cp "$build/$library" "$out/"
# Lets check.sh tell a library built from an older patch set from this one.
echo "$patch_set" > "$out/veil-patches.sha256"
if [ -n "$with_daemon" ]; then
  cmake --build "$build" --target daemon -j"$jobs"
  cp "$build/bin/$daemon" "$out/"
fi
echo "built: $out"
ls -l "$out"
