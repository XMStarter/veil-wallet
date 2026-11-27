#!/usr/bin/env bash
# Builds the Monero wallet library that Veil loads, from pinned sources.
#
#   build.sh <work-dir> [--with-daemon] [--windows | --android]
#
# The result is <work-dir>/out/libmonero_wallet2_api_c.so and, with
# --with-daemon, <work-dir>/out/monerod for regtest tests.
#
# Built without --windows or --android, it links against the system's
# libraries, which are then no part of what is built; why is said where
# that build is made, below.
#
# Whatever folder <work-dir> is, the build happens at one path, the same on
# every machine: /srv. The compilers write the paths of source files into
# what they build, some hundreds of them, and the dependency build writes
# the folder it installs into; with the work folder under its own name the
# same sources gave another library in every folder. So the script starts
# itself again in a view of the machine's folders that is its own, in which
# <work-dir> is what lies at /srv, and builds there. Nothing outside that
# view changes: /srv stays what it was for every other program, and the
# files are written into <work-dir> as the user who runs this. It takes a
# Linux that lets an ordinary user have such a view (user namespaces), or
# root. A work folder built in before this was so is emptied and built
# again from nothing; what the dependency build had downloaded stays.
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
#
# With --android the library is built for Android on 64-bit ARM, on Linux,
# into <work-dir>/out/android/arm64-v8a/libmonero_wallet2_api_c.so. Monero's
# dependency build does that too, and brings its own compiler: the Android
# NDK it pins, release 17b. That compiler is from 2018, and three things
# here make up for its age; each is explained where it is done. The build
# has a source tree of its own, <work-dir>/android, so that the one patch
# only it needs touches nothing the other two builds are made from. There
# is no daemon for Android. Besides the build tools above it needs:
#   autoconf automake libtool pkg-config python3 python-is-python3 unzip
set -euo pipefail

# monero_c wraps wallet2 in a C interface and adds Polyseed support. Its
# monero submodule pins the Monero release it is built from.
MONERO_C_REPO="https://github.com/MrCyjaneK/monero_c"
MONERO_C_COMMIT="a59deb815a9c806c09bb434fb9017763e2e944f2" # tag v0.18.5.3-RC1

work="${1:?usage: build.sh <work-dir> [--with-daemon] [--windows | --android]}"
shift
with_daemon=""
windows=""
android=""
for option in "$@"; do
  case "$option" in
    --with-daemon) with_daemon=yes ;;
    --windows) windows=yes ;;
    --android) android=yes ;;
    *)
      echo "error: unknown option $option" >&2
      exit 2
      ;;
  esac
done
if [ -n "$windows" ] && [ -n "$android" ]; then
  echo "error: --windows and --android are two builds; run this once for each" >&2
  exit 2
fi
if [ -n "$android" ] && [ -n "$with_daemon" ]; then
  echo "error: there is no daemon for Android; tests with a daemon run on a desktop" >&2
  exit 2
fi
here="$(cd "$(dirname "$0")" && pwd)"
jobs="$(nproc 2>/dev/null || echo 2)"

mkdir -p "$work"
work="$(cd "$work" && pwd)"

# The one path everything is built at; see the top of this file.
BUILT_AT=/srv
if [ "${VEIL_WALLET_LIB_VIEW:-}" != "$work" ]; then
  if [ ! -d "$BUILT_AT" ]; then
    echo "error: this system has no folder $BUILT_AT, which the build needs as the" >&2
    echo "place it puts the work folder at. As root: mkdir $BUILT_AT" >&2
    exit 1
  fi
  # Root can have a view of its own as it is. Anybody else gets one as the
  # root of a user namespace, and goes back to being themselves in it once
  # the folder is in place.
  if [ "$(id -u)" = 0 ]; then
    enter=(unshare --mount --)
    back=()
  else
    enter=(unshare --user --map-root-user --mount --)
    back=(unshare --user "--map-user=$(id -u)" "--map-group=$(id -g)" --)
  fi
  if ! "${enter[@]}" ${back[@]+"${back[@]}"} true 2>/dev/null; then
    echo "error: this system does not let this user have a view of the folders of its" >&2
    echo "own (unshare, user namespaces), which the build needs so that it happens at" >&2
    echo "the same path on every machine. See the top of $0." >&2
    exit 1
  fi
  # shellcheck disable=SC2016
  VEIL_WALLET_LIB_VIEW="$work" exec "${enter[@]}" bash -c '
    work="$1" at="$2"
    shift 2
    mount --bind "$work" "$at" && exec "$@"
  ' _ "$work" "$BUILT_AT" ${back[@]+"${back[@]}"} "$0" "$work" "$@"
fi
# From here on in the view: the folder as the user knows it is only named
# in what is said to the user.
shown="$work"
work="$BUILT_AT"
if [ ! "$work" -ef "$shown" ]; then
  echo "error: $shown is not what lies at $BUILT_AT; the view was not made" >&2
  exit 1
fi

# How a work folder is built, in a word, which is kept in a file of its
# own (built-how.txt) because check.sh and windows-app.sh read it too: it
# goes into the sum that tells a library built from this checkout from
# any other, with the patches. A folder built another way is
# built again from nothing: under its own path it has that path in what
# was built, and its patches were committed at the time of that build,
# which is in the library too (see the dates below). Only what the
# dependency build downloaded is kept, and put back further down.
BUILT_HOW="$(cat "$here/built-how.txt")"
if [ "$(cat "$work/.built-how" 2>/dev/null)" != "$BUILT_HOW" ]; then
  if [ -d "$work/monero_c" ] || [ -d "$work/android" ] || [ -d "$work/build" ]; then
    echo "$shown was built in another way than this script builds now. Its sources"
    echo "and everything built from them, for every system, are removed and made"
    echo "again, here for one system; what was downloaded for the dependencies stays."
    # What came out as well, for every system: a library built the old
    # way must not be taken for one built this way.
    rm -rf "$work/out"
    for old in "$work" "$work/android"; do
      rm -rf "$old/build" "$old/build-windows" "$old/host-libs" "$old/link-names"
      if [ -d "$old/monero_c/monero/contrib/depends/sources" ] && [ ! -e "$old/depends-sources.kept" ]; then
        mv "$old/monero_c/monero/contrib/depends/sources" "$old/depends-sources.kept"
      fi
      rm -rf "$old/monero_c"
    done
  fi
  echo "$BUILT_HOW" > "$work/.built-how"
fi

# Where the sources are and where things are built. Android has a tree of
# its own; what comes out goes under the same out/ either way.
tree="$work"
if [ -n "$android" ]; then
  tree="$work/android"
  mkdir -p "$tree"
fi
src="$tree/monero_c"

# apply_patches.sh commits the patches it applies, which needs an identity,
# and a date: Monero writes the first nine characters of its commit's name
# into the library as part of its version, and the name of a commit comes
# from its date among other things. With the date of the build, every
# build was another library in those nine characters. Any date does, as
# long as it is always the same one.
export GIT_AUTHOR_NAME="veil build" GIT_AUTHOR_EMAIL="build@invalid"
export GIT_COMMITTER_NAME="veil build" GIT_COMMITTER_EMAIL="build@invalid"
export GIT_AUTHOR_DATE="2000-01-01T00:00:00+00:00" GIT_COMMITTER_DATE="2000-01-01T00:00:00+00:00"

# Three more things a build can take from the machine it runs on, made the
# same here for every build: the language, by which tools order lists and
# write numbers; the time zone; and a store of what was compiled before
# (ccache, which Monero's dependency build makes and Monero's own build
# uses where it finds it), because what a build makes has to come from its
# sources and not from a build before it. None of the three has been seen
# to change what is built; they are closed because they are cheap to close.
export LC_ALL=C TZ=UTC CCACHE_DISABLE=1

# A fourth thing does change what is built: whether the machine has flex.
# Monero's dependency build compiles unbound, whose source comes with the
# reader of its settings file already written out (util/configlexer.c)
# and with the description that file was made from. Where the machine
# has flex, unbound's build makes the file anew with it, and what flex
# writes there is not what came with the source: the library for Android
# built by hosted CI, whose machines have flex, differed from the one
# built in a cloud session, which had none, in fifteen thousand bytes of
# tables and in where everything after them lies. So the build is told
# that there is no flex,
# in the words a machine without one finds for itself (autoconf's ":"),
# and the file that came with the source is the one compiled, everywhere.
# A library built before this on a machine without flex is the library
# this builds, which is why built-how.txt does not name it.
export LEX=:

# And git's own settings on this machine, which decide what a commit made
# here is. A machine set to sign its commits made signed ones of
# monero_c's patches, a signature is part of what a commit's name comes
# from, and so the nine characters in the library were another machine's
# nine characters: that was the whole of the difference between the
# library for Windows built in a cloud session and the same one built on
# the owner's machine, sixteen bytes. Settings for line endings, hooks or
# messages could do the same. So git is told to read none of the
# machine's settings but those it needs to reach the network, which are
# copied into a file of this build's own. Settings handed over in the
# environment (GIT_CONFIG_COUNT) are left alone: a build machine uses
# them for its way to the network, and a person does not set them.
git_settings="$work/.git-settings"
: > "$git_settings"
for scope in --system --global; do
  while IFS= read -r -d '' entry; do
    case "$entry" in
      *$'\n'*) git config --file "$git_settings" --add "${entry%%$'\n'*}" "${entry#*$'\n'}" ;;
      *) git config --file "$git_settings" --add "$entry" true ;;
    esac
  done < <(git config "$scope" -z --get-regexp '^(http|https|url|credential|safe)\.' 2>/dev/null)
done
export GIT_CONFIG_GLOBAL="$git_settings" GIT_CONFIG_SYSTEM=/dev/null

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
# What an earlier way of building had downloaded, back where the
# dependency build looks for it.
if [ -d "$tree/depends-sources.kept" ]; then
  if [ ! -e monero/contrib/depends/sources ]; then
    mv "$tree/depends-sources.kept" monero/contrib/depends/sources
  else
    rm -rf "$tree/depends-sources.kept"
  fi
fi

# Veil's own patches, on top of monero_c's. They stay uncommitted, so that a
# changed patch set can be dropped and applied again: the marker file holds
# the checksum of the set that is applied.
#
# The Android build has one more, for a library monero_c adds to Monero
# (bc-ur, for payment codes shown as pictures, which Veil does not use but
# the library is built with). It asks the compiler to work out the element
# type of a vector from two iterators, which the compiler of 2018 cannot.
# The patch names the type, which means the same to every compiler.
patches=("$here"/patches/monero/*.patch "$here"/patches/monero_c/*.patch)
if [ -n "$android" ]; then
  patches+=("$here"/patches/android/bc-ur/*.patch)
fi
patch_set="$(cat "${patches[@]}" "$here/built-how.txt" | sha256sum | cut -d' ' -f1)"
if [ "$(cat .veil-patched 2>/dev/null)" != "$patch_set" ]; then
  if [ -f .veil-patched ]; then
    echo "Veil's patches changed; applying them again"
    git -C monero checkout -- src
    git checkout -- monero_libwallet2_api_c
    if [ -n "$android" ]; then
      git -C monero/external/bc-ur checkout -- .
    fi
  fi
  for patch in "$here"/patches/monero/*.patch; do
    git -C monero apply --whitespace=nowarn "$patch"
  done
  for patch in "$here"/patches/monero_c/*.patch; do
    git apply --whitespace=nowarn "$patch"
  done
  if [ -n "$android" ]; then
    for patch in "$here"/patches/android/bc-ur/*.patch; do
      git -C monero/external/bc-ur apply --whitespace=nowarn "$patch"
    done
  fi
  echo "$patch_set" > .veil-patched
fi

# Where the dependency build keeps what it downloads: in its own folder,
# unless another is named, which twice.sh does so that two builds download
# once. Every file is checked by its hash wherever it lies.
depends_options=()
beside=()
if [ -n "${VEIL_DEPENDS_SOURCES:-}" ]; then
  depends_options+=("SOURCES_PATH=$VEIL_DEPENDS_SOURCES")
fi

if [ -n "$windows" ]; then
  host=x86_64-w64-mingw32
  depends="$src/monero/contrib/depends"
  # Monero's dependency build: Boost, OpenSSL, ZeroMQ, Unbound and the rest,
  # compiled for Windows from sources it downloads and checks by hash. Monero
  # needs the cross compiler in its POSIX-threads form, which the
  # distribution installs beside the default one.
  make -C "$depends" HOST="$host" -j"$jobs" ${depends_options[@]+"${depends_options[@]}"} \
    mingw32_CC="$host-gcc-posix" mingw32_CXX="$host-g++-posix"

  build="$work/build-windows"
  out="$work/out/windows"
  # Three things the wallet library's own build files leave out in this
  # setup: where the dependencies' libraries are; that Polyseed is linked in
  # statically (without the definition its functions are looked for in a
  # DLL that does not exist); and -static, so that the compiler's own
  # thread library goes into the DLL too and the result needs nothing but
  # Windows itself.
  #
  # And a fourth, for the same library from every build: no time of
  # linking. A program for Windows has a place for it in its first bytes,
  # the linker fills it in unless told not to, and a sum over the file
  # follows from it; two builds differed in those six bytes.
  cmake -S monero_libwallet2_api_c -B "$build" -G Ninja \
    -DCMAKE_TOOLCHAIN_FILE="$depends/$host/share/toolchain.cmake" \
    -DMONERO_FLAVOR=monero -DHOST_ABI="$host" -DOUTPUT_MODE=SHARED \
    -DCMAKE_BUILD_TYPE=Release -DUSE_DEVICE_TREZOR=OFF -DBUILD_TESTS=OFF \
    -DMANUAL_SUBMODULES=1 -DBUILD_DOCUMENTATION=OFF \
    -DCMAKE_SHARED_LINKER_FLAGS="-L$depends/$host/lib -static -Wl,--no-insert-timestamp" \
    -DCMAKE_EXE_LINKER_FLAGS="-L$depends/$host/lib -static -Wl,--no-insert-timestamp" \
    -DCMAKE_C_FLAGS="-DPOLYSEED_STATIC" -DCMAKE_CXX_FLAGS="-DPOLYSEED_STATIC"
  cmake --build "$build" -j"$jobs"
  library="libmonero_wallet2_api_c.dll"
  daemon="monerod.exe"
elif [ -n "$android" ]; then
  host=aarch64-linux-android
  depends="$src/monero/contrib/depends"

  # The compiler in NDK 17b is a program of 2018 that asks for a system
  # library under the name it had then, libtinfo.so.5, which current
  # distributions no longer have. Where it is missing, the one they do have
  # is offered under that name, in a folder of this build's own and to this
  # build's programs only: the compiler uses it to ask what the terminal
  # can show, and nothing of it ends up in what is built.
  host_libs="$tree/host-libs"
  mkdir -p "$host_libs"
  if ! ldconfig -p 2>/dev/null | grep -q 'libtinfo\.so\.5 '; then
    current="$(ldconfig -p 2>/dev/null | awk '/libtinfo\.so\.6 .*x86-64/ { print $NF; exit }')"
    if [ -z "$current" ]; then
      echo "error: the Android compiler needs libtinfo, and this system has neither" >&2
      echo "libtinfo.so.5 nor libtinfo.so.6. On Debian and Ubuntu: apt-get install libtinfo6" >&2
      exit 1
    fi
    ln -sf "$current" "$host_libs/libtinfo.so.5"
  fi
  export LD_LIBRARY_PATH="$host_libs${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

  # Monero's dependency build: the NDK itself, downloaded from its publisher
  # and checked by hash, then Boost, OpenSSL, ZeroMQ, Unbound and the rest,
  # compiled for Android from sources checked the same way.
  make -C "$depends" HOST="$host" -j"$jobs" ${depends_options[@]+"${depends_options[@]}"}

  # monero_c's build files ask the linker for the C++ library by the name
  # newer NDKs give it. In this one it is in two files under other names;
  # this says so to the linker, in a folder of this build's own.
  link_names="$tree/link-names"
  mkdir -p "$link_names"
  echo 'INPUT(-lc++_static -lc++abi)' > "$link_names/libc++.a"

  build="$tree/build"
  out="$work/out/android/arm64-v8a"
  PATH="$depends/$host/native/bin:$PATH" \
    cmake -S monero_libwallet2_api_c -B "$build" -G Ninja \
    -DCMAKE_TOOLCHAIN_FILE="$depends/$host/share/toolchain.cmake" \
    -DMONERO_FLAVOR=monero -DHOST_ABI="$host" -DOUTPUT_MODE=SHARED \
    -DCMAKE_BUILD_TYPE=Release -DUSE_DEVICE_TREZOR=OFF -DBUILD_TESTS=OFF \
    -DMANUAL_SUBMODULES=1 -DBUILD_DOCUMENTATION=OFF \
    -DCMAKE_SHARED_LINKER_FLAGS="-L$link_names"
  PATH="$depends/$host/native/bin:$PATH" cmake --build "$build" -j"$jobs"
  library="libmonero_wallet2_api_c.so"
  daemon=""
else
  # For Linux the library is linked against the system's own Boost,
  # OpenSSL, ZeroMQ, Unbound and the rest, as shared libraries that are no
  # part of what is built: it runs where those are installed in the
  # versions of the machine that built it, and is the same library only
  # on a machine with the same ones. Monero's dependency build, which the
  # other two builds use, does not build on a current distribution for
  # the distribution itself: its Boost is from 2018 and does not compile
  # against a C library of today. Tried on Ubuntu 24.04 in November 2026.
  #
  # One of them is built here and not the system's: Polyseed, as a shared
  # library of its own beside the wallet library. The linker's habit is
  # to write the folder it was built in into whatever needs it, as the
  # place to look for it, and that folder is nowhere outside this build.
  # So the place written is "where this file is" instead, and Polyseed is
  # put next to the wallet library and goes wherever that goes.
  #
  # ARCH=default: left to itself, Monero's build compiles for the
  # processor of the machine it runs on (-march=native), and so does
  # RandomX inside it. The library then differs from one build machine to
  # the next, and on a processor older than the builder's it can end with
  # an illegal instruction. "default" names no processor: the compiler's
  # own baseline for x86-64 plus AES, which is what the builds for
  # Windows and Android get from Monero's dependency build and what
  # Monero's own releases are built with.
  build="$work/build"
  out="$work/out"
  # The word is quoted once more for the shell that runs the linker, which
  # would take it for a variable of its own, and an empty one.
  cmake -S monero_libwallet2_api_c -B "$build" -G Ninja \
    -DARCH=default \
    -DMONERO_FLAVOR=monero -DHOST_ABI=x86_64-linux-gnu -DOUTPUT_MODE=SHARED \
    -DCMAKE_BUILD_TYPE=Release -DUSE_DEVICE_TREZOR=OFF -DBUILD_TESTS=OFF \
    -DMANUAL_SUBMODULES=1 -DBUILD_DOCUMENTATION=OFF \
    -DCMAKE_SKIP_BUILD_RPATH=ON \
    -DCMAKE_SHARED_LINKER_FLAGS="-Wl,-rpath,'\$ORIGIN'" \
    -DCMAKE_EXE_LINKER_FLAGS="-Wl,-rpath,'\$ORIGIN'"
  cmake --build "$build" -j"$jobs"
  beside=("$build/monero_build/external/polyseed/libpolyseed.so.2")
  library="libmonero_wallet2_api_c.so"
  daemon="monerod"
fi

mkdir -p "$out"
cp "$build/$library" "$out/"
# What the library needs next to it, under the name it asks for.
for file in ${beside[@]+"${beside[@]}"}; do
  cp -L "$file" "$out/"
done
# Lets check.sh tell a library built from an older patch set from this one.
echo "$patch_set" > "$out/veil-patches.sha256"
if [ -n "$with_daemon" ]; then
  cmake --build "$build" --target daemon -j"$jobs"
  cp "$build/bin/$daemon" "$out/"
fi
echo "built: $shown${out#"$work"}"
ls -l "$out"
# What to hold against another machine's build of the same checkout.
(cd "$out" && sha256sum "$library" ${beside[@]+"${beside[@]##*/}"})
