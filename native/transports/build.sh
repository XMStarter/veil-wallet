#!/usr/bin/env bash
# Builds the transport program Veil's app for Android brings along for Tor
# bridges: lyrebird, the Tor Project's program for obfs4, Snowflake and
# WebTunnel, for Android on 64-bit ARM.
#
# Tor does not speak those transports itself; it starts a program that
# does. A desktop has such programs installed. A phone has none, and lets
# an app start only what came in its own package, so the app carries one.
#
#   ./build.sh [WORK_DIR]
#
# WORK_DIR defaults to ~/veil-transports. The result is
# WORK_DIR/out/android/arm64-v8a/liblyrebird.so: a program, named like a
# library because that is the only kind of file Android unpacks from a
# package into a folder whose files may be started.
#
# What it downloads, all of it checked:
#   - Go, the compiler lyrebird is written for, from its publisher, by
#     checksum. It is unpacked inside WORK_DIR and touches nothing else on
#     the machine.
#   - lyrebird's source from the Tor Project, at one commit, by its hash.
#   - The Go packages that source names, through Go's package servers,
#     each by the checksum lyrebird's own go.sum records.
# It needs the Android NDK that `./check.sh --android ndk` installs, for
# the C compiler: on Android a Go program finds names on the network
# through the system's C library, and Snowflake and WebTunnel need names.
#
# lyrebird is under the GNU General Public License, version 3 or later
# (part of its obfs4 is; see LICENSE-GPL3.txt in its source). It is a
# program of its own that Tor starts, not part of Veil's code. Whoever
# hands on a package with it inside has to hand on its licence and say
# where its source is; see README.md here.
set -euo pipefail

work="${1:-$HOME/veil-transports}"

# lyrebird 0.10.0.
lyrebird_repo="https://gitlab.torproject.org/tpo/anti-censorship/pluggable-transports/lyrebird.git"
lyrebird_version="0.10.0"
lyrebird_commit="59ac7e400c384773f5584dbe366516ad04a33d9f"

# Go, in a release lyrebird's go.mod accepts (it asks for 1.26.10 or
# later). The checksum is the one go.dev publishes for this file.
go_version="1.26.11"
go_sha256="9849f4e04ee35303714e34fbc3ac8ab44ea3e448a95aa182f91e3b9b79c710ec"

# The oldest Android the app is built for, as in check.sh.
android_api=24
ndk="${ANDROID_NDK_ROOT:-${ANDROID_NDK_HOME:-}}"
cc="$ndk/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android$android_api-clang"
if [ -z "$ndk" ] || [ ! -x "$cc" ]; then
  echo "error: the Android NDK is not where ANDROID_NDK_ROOT says ($ndk)." >&2
  echo "Run this through ./check.sh --android setup, which installs it and says where it is." >&2
  exit 1
fi

mkdir -p "$work"
work="$(cd "$work" && pwd)"
out="$work/out/android/arm64-v8a"
built="$out/liblyrebird.so"
stamp="$out/veil-transport.txt"
wanted="lyrebird $lyrebird_commit go $go_version api $android_api"
if [ -f "$built" ] && [ "$(cat "$stamp" 2>/dev/null)" = "$wanted" ]; then
  echo "transport program: built in $out"
  exit 0
fi

go_root="$work/go-$go_version"
if [ ! -x "$go_root/go/bin/go" ]; then
  echo "--- installing Go $go_version into $go_root"
  archive="$work/go$go_version.linux-amd64.tar.gz"
  url="https://go.dev/dl/go$go_version.linux-amd64.tar.gz"
  rm -rf "$go_root"
  if ! curl --proto '=https' --tlsv1.2 -sSfL -o "$archive" "$url"; then
    echo "error: could not download $url" >&2
    exit 1
  fi
  echo "$go_sha256  $archive" | sha256sum -c -
  mkdir -p "$go_root"
  tar -xzf "$archive" -C "$go_root"
  rm -f "$archive"
fi

source="$work/lyrebird"
if [ "$(git -C "$source" rev-parse HEAD 2>/dev/null)" != "$lyrebird_commit" ]; then
  echo "--- fetching lyrebird $lyrebird_version"
  rm -rf "$source"
  git init -q "$source"
  git -C "$source" remote add origin "$lyrebird_repo"
  git -C "$source" fetch -q --depth 1 origin "$lyrebird_commit"
  git -C "$source" -c advice.detachedHead=false checkout -q FETCH_HEAD
fi
# A commit's hash covers every file in it: this is the source that was
# pinned, or the build stops here.
if [ "$(git -C "$source" rev-parse HEAD)" != "$lyrebird_commit" ]; then
  echo "error: the lyrebird source is not at commit $lyrebird_commit" >&2
  exit 1
fi

echo "--- building lyrebird $lyrebird_version for Android (arm64-v8a)"
mkdir -p "$out"
rm -f "$built" "$stamp"
# Go as unpacked above and no other; its packages and its cache inside
# WORK_DIR; no usage reports; and go.sum as it stands, so that a package
# whose contents differ from what lyrebird recorded stops the build.
export PATH="$go_root/go/bin:$PATH"
export GOTOOLCHAIN=local GOTELEMETRY=off GOFLAGS=-mod=readonly
export GOPATH="$work/gopath" GOCACHE="$work/gocache"
export GOOS=android GOARCH=arm64 CGO_ENABLED=1 CC="$cc"
# -checklinkname=0: one of Snowflake's packages (anet, which lists the
#   phone's network interfaces where Android no longer lets Go's own way
#   work) reaches into Go's net package by name, which Go's linker
#   refuses unless told to allow it. It is that package's own instruction
#   for building for Android.
# max-page-size: laid out for devices with 16-kilobyte pages, like the
#   app's libraries.
# -trimpath, -buildvcs=false: nothing of this machine's folders or of the
#   state of the checkout goes into the program.
(cd "$source" && go build -trimpath -buildvcs=false \
  -ldflags="-s -w -checklinkname=0 -X main.lyrebirdVersion=$lyrebird_version -extldflags=-Wl,-z,max-page-size=16384" \
  -o "$built" ./cmd/lyrebird)

cp "$source/LICENSE" "$out/lyrebird-LICENSE.txt"
cp "$source/LICENSE-GPL3.txt" "$out/lyrebird-LICENSE-GPL3.txt"
echo "$wanted" >"$stamp"
echo "built: $out"
ls -l "$out"
