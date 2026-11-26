#!/usr/bin/env bash
# Writes the licence texts of the transport program and of what it is
# built from into the file the app carries,
# app/assets/licences/transport.txt, or says whether that file is still
# right.
#
#   ./notices.sh [--check] [WORK_DIR]
#
# WORK_DIR is the folder build.sh built the program in, ~/veil-transports
# if none is given. The program itself says which Go packages are in it
# (`go version -m`), and their source, with the licence texts, lies in
# WORK_DIR where Go put it for the build. Nothing is downloaded and
# nothing is built.
#
# lyrebird is under the GNU General Public License, version 3 or later.
# A package with it inside has to carry that text; the Go packages ask
# the same for theirs. See README.md here.
set -euo pipefail

check=""
if [ "${1:-}" = "--check" ]; then
  check="--check"
  shift
fi
work="${1:-$HOME/veil-transports}"
here="$(cd "$(dirname "$0")" && pwd)"
out="$here/../../app/assets/licences/transport.txt"

program="$work/out/android/arm64-v8a/liblyrebird.so"
source="$work/lyrebird"
go="$(ls "$work"/go-*/go/bin/go 2>/dev/null | head -1)"
if [ ! -f "$program" ] || [ ! -d "$source" ] || [ -z "$go" ]; then
  echo "error: no transport program in $work; build.sh builds it there." >&2
  exit 1
fi
go_root="$(cd "$(dirname "$go")/.." && pwd)"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
list="$tmp/list"
: > "$list"

# texts_in FOLDER: the licence texts and notices at the top of a folder.
texts_in() {
  find "$1" -maxdepth 1 -type f \
    \( -iname 'licen[sc]e*' -o -iname 'copying*' -o -iname 'notice*' \
    -o -iname 'patents*' -o -iname 'copyright*' \) | LC_ALL=C sort
}

# part NAME FOLDER: that part's licence texts lie at the top of FOLDER.
part() {
  local name="$1" folder="$2" file found=""
  while IFS= read -r file; do
    printf '%s\t%s\n' "$name" "$file" >> "$list"
    found=yes
  done < <(texts_in "$folder")
  if [ -z "$found" ]; then
    echo "error: no licence text for $name in $folder" >&2
    exit 1
  fi
}

"$go" version -m "$program" > "$tmp/modules"
go_release="$(sed -n '1s/.*: go//p' "$tmp/modules")"
commit="$(git -C "$source" rev-parse --short=8 HEAD)"

part "lyrebird ($commit)" "$source"
part "Go $go_release" "$go_root"

# A line for each Go package in the program: "dep", its path, its
# version. Where Go keeps its source, a capital letter in the path is
# written as an exclamation mark and the small letter.
packages=0
while read -r kind path version _; do
  case "$kind" in
    dep) ;;
    "=>")
      echo "error: the program was built with a replaced package ($path)," >&2
      echo "which this script does not know where to find." >&2
      exit 1
      ;;
    *) continue ;;
  esac
  folder="$work/gopath/pkg/mod/$(echo "$path" | sed -E 's/([A-Z])/!\L\1/g')@$version"
  if [ ! -d "$folder" ]; then
    echo "error: the source of $path $version is not in $work/gopath." >&2
    exit 1
  fi
  part "$path $version" "$folder"
  packages=$((packages + 1))
done < "$tmp/modules"
if [ "$packages" -lt 10 ]; then
  echo "error: only $packages Go packages were read out of the program;" >&2
  echo "lyrebird is built from several dozen." >&2
  exit 1
fi

cd "$here/.."
cargo run --quiet --locked -p veil_notices -- $check --parts "$list" "$out"
