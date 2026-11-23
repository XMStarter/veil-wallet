#!/usr/bin/env bash
# Builds the Monero wallet library that Veil loads, from pinned sources.
#
#   build.sh <work-dir> [--with-daemon]
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
set -euo pipefail

# monero_c wraps wallet2 in a C interface and adds Polyseed support. Its
# monero submodule pins the Monero release it is built from.
MONERO_C_REPO="https://github.com/MrCyjaneK/monero_c"
MONERO_C_COMMIT="a59deb815a9c806c09bb434fb9017763e2e944f2" # tag v0.18.5.3-RC1

work="${1:?usage: build.sh <work-dir> [--with-daemon]}"
with_daemon="${2:-}"
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

# Veil's own patches, on top of monero_c's.
if [ ! -f .veil-patched ]; then
  for patch in "$here"/patches/monero/*.patch; do
    git -C monero apply --whitespace=nowarn "$patch"
  done
  for patch in "$here"/patches/monero_c/*.patch; do
    git apply --whitespace=nowarn "$patch"
  done
  touch .veil-patched
fi

cmake -S monero_libwallet2_api_c -B "$work/build" -G Ninja \
  -DMONERO_FLAVOR=monero -DHOST_ABI=x86_64-linux-gnu -DOUTPUT_MODE=SHARED \
  -DCMAKE_BUILD_TYPE=Release -DUSE_DEVICE_TREZOR=OFF -DBUILD_TESTS=OFF \
  -DMANUAL_SUBMODULES=1 -DBUILD_DOCUMENTATION=OFF
cmake --build "$work/build" -j"$jobs"

mkdir -p "$work/out"
cp "$work/build/libmonero_wallet2_api_c.so" "$work/out/"
if [ "$with_daemon" = "--with-daemon" ]; then
  cmake --build "$work/build" --target daemon -j"$jobs"
  cp "$work/build/bin/monerod" "$work/out/"
fi
echo "built: $work/out"
ls -l "$work/out"
