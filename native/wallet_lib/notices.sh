#!/usr/bin/env bash
# Writes the licence texts of what the wallet library is built from into
# the file the app carries, app/assets/licences/wallet-library.txt, or
# says whether that file is still right.
#
#   ./notices.sh [--check] <work-dir>
#
# <work-dir> is the folder build.sh built the library in. The texts are
# read from the source there: from monero_c and what it brings along, and
# from the archives Monero's dependency build downloaded, which are there
# once the library was built for Windows or for Android. Nothing is
# downloaded and nothing is built.
#
# Which parts there are is written down below by hand, from the line the
# library is linked with. Where a build for Windows or for Android is in
# <work-dir>, that line is read again and every library on it has to be
# one this script knows: a part that a later Monero adds stops the script
# instead of going unnamed. The library for Linux is left out of that
# reading. It takes Boost, OpenSSL and the like from the machine it is
# built on, as shared libraries that are in no package of Veil's, and
# which those are differs from one machine to the next.
#
# A package with the library inside has to carry these texts: monero_c
# and polyseed are under the GNU Lesser General Public License, which also
# asks for the text of the General Public License it builds on, and the
# others ask that their notices go along. See README.md here.
set -euo pipefail

check=""
if [ "${1:-}" = "--check" ]; then
  check="--check"
  shift
fi
work="${1:?usage: notices.sh [--check] <work-dir>}"
work="$(cd "$work" && pwd)"
here="$(cd "$(dirname "$0")" && pwd)"
out="$here/../../app/assets/licences/wallet-library.txt"

# The source: the tree the libraries for Linux and Windows are built
# from, or the one for Android. Both are the same commit of monero_c.
tree=""
for candidate in "$work/monero_c" "$work/android/monero_c"; do
  if [ -f "$candidate/monero/LICENSE" ]; then
    tree="$candidate"
    break
  fi
done
if [ -z "$tree" ]; then
  echo "error: no monero_c source in $work; build.sh puts it there." >&2
  exit 1
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
list="$tmp/list"
: > "$list"

# part NAME FILE...: these files hold the licence texts of that part.
part() {
  local name="$1" file
  shift
  for file in "$@"; do
    if [ ! -s "$file" ]; then
      echo "error: the licence text of $name is not where it was: $file" >&2
      exit 1
    fi
    printf '%s\t%s\n' "$name" "$file" >> "$list"
  done
}

# archive PREFIX: the one source archive whose name begins with PREFIX,
# from whichever dependency build downloaded it.
archive() {
  local found
  found="$(ls "$work"/monero_c/monero/contrib/depends/sources/"$1"* \
    "$work"/android/monero_c/monero/contrib/depends/sources/"$1"* 2>/dev/null | head -1)"
  if [ -z "$found" ]; then
    echo "error: no source archive $1* in $work. It is there once the library" >&2
    echo "was built for Windows or for Android (build.sh --windows, --android)." >&2
    exit 1
  fi
  echo "$found"
}

# from_archive PREFIX FILE...: unpacks those files of the archive and
# prints where each lies now, and the archive's name without its ending
# on the first line.
unpacked=()
release=""
from_archive() {
  local file top
  file="$(archive "$1")"
  shift
  release="$(basename "$file" | sed -E 's/\.(tar\.(gz|bz2|xz)|tgz)$//')"
  # sed reads the whole list: a reader that stopped at the first line
  # would end tar with a broken pipe, which `pipefail` takes for a failure.
  top="$(tar -tf "$file" | sed -n '1p' | cut -d/ -f1)"
  mkdir -p "$tmp/$release"
  unpacked=()
  local member
  for member in "$@"; do
    tar -xf "$file" -C "$tmp/$release" "$top/$member"
    unpacked+=("$tmp/$release/$top/$member")
  done
}

# The text of the GNU General Public License, version 3, which the Lesser
# one builds on and asks to be handed on with it. ZeroMQ's source carries
# it. Copies of that text differ in their spaces and in how they write
# the address of its publisher, so it is known by its first two lines.
from_archive zeromq- COPYING COPYING.LESSER
zeromq_release="$release"
gpl3="${unpacked[0]}"
zeromq_lesser="${unpacked[1]}"
if ! sed -n '1,2p' "$gpl3" | tr -s ' \n' ' ' \
  | grep -q '^ GNU GENERAL PUBLIC LICENSE Version 3, 29 June 2007 $'; then
  echo "error: $gpl3 is not the text of the GNU General Public License, version 3." >&2
  exit 1
fi

commit="$(git -C "$tree" rev-parse --short=8 HEAD)"
external="$tree/monero/external"

part "Monero wallet library (monero_c $commit)" "$tree/LICENSE" "$gpl3"
part "Monero" "$tree/monero/LICENSE"
part "polyseed" "$external/polyseed/LICENSE" "$gpl3"
part "RandomX" "$external/randomx/LICENSE"
part "LMDB" "$external/db_drivers/liblmdb/LICENSE" "$external/db_drivers/liblmdb/COPYRIGHT"
part "utf8proc" "$external/utf8proc/LICENSE.md"
part "bc-ur" "$external/bc-ur/LICENSE"
part "RapidJSON" "$external/rapidjson/license.txt"

# Easylogging++ lies in Monero's source as two files and no licence text.
# Its header names its authors and the MIT licence; the text of that is
# the one Veil keeps for packages that name it and carry none.
{
  echo "Easylogging++ comes in Monero's source without a licence text of its own."
  echo "Its header says:"
  echo
  sed -n '1,/^#ifndef/p' "$external/easylogging++/easylogging++.h" \
    | grep '^//' | sed -E 's#^// ?##' | sed -e '/^[[:space:]]*$/d'
  echo
  echo "The text of the MIT licence:"
  echo
  cat "$here/../veil_notices/texts/MIT.txt"
} > "$tmp/easylogging.txt"
grep -q "MIT Licence" "$tmp/easylogging.txt" || {
  echo "error: Easylogging++'s header no longer names the MIT licence." >&2
  exit 1
}
part "Easylogging++" "$tmp/easylogging.txt"

from_archive boost_ LICENSE_1_0.txt
part "Boost (${release#boost_})" "${unpacked[@]}"
from_archive openssl- LICENSE.txt
part "OpenSSL ${release#openssl-}" "${unpacked[@]}"
from_archive libsodium- LICENSE
part "libsodium ${release#libsodium-}" "${unpacked[@]}"
from_archive unbound- LICENSE
part "Unbound ${release#unbound-}" "${unpacked[@]}"
part "ZeroMQ ${zeromq_release#zeromq-}" "$zeromq_lesser" "$gpl3"

# Only in the library for Windows.
from_archive hidapi- LICENSE.txt LICENSE-bsd.txt LICENSE-orig.txt LICENSE-gpl3.txt
part "hidapi ${release#hidapi-} (Windows)" "${unpacked[@]}"
from_archive libiconv- COPYING.LIB
part "libiconv ${release#libiconv-} (Windows)" "${unpacked[@]}"

# What the library is linked with, where a build for Windows or for
# Android is at hand: every library on that line has to be one of the
# parts above.
known() {
  case "$1" in
    # Monero's own code, and monero_c's.
    libwallet*.a | libcommon.a | libcryptonote_*.a | libmnemonics.a | librpc_base.a) return 0 ;;
    libmultisig.a | libdevice*.a | libnet.a | libblockchain_db.a | libringct*.a) return 0 ;;
    libcheckpoints.a | libpolyseed_wrapper.a | libcncrypto.a | libblocks.a) return 0 ;;
    libhardforks.a | libversion.a | libepee.a) return 0 ;;
    # What Monero's source brings along.
    liblmdb.a | libbc-ur.a | libpolyseed.a | libutf8proc.a | librandomx.a) return 0 ;;
    libeasylogging.a) return 0 ;;
    # What the dependency build makes.
    libboost_*.a | libzmq.a | libunbound.a | libsodium.a | libssl.a | libcrypto.a) return 0 ;;
    libhidapi.a) return 0 ;;
  esac
  return 1
}
for ninja in "$work/build-windows/build.ninja" "$work/android/build/build.ninja"; do
  [ -f "$ninja" ] || continue
  line="$(grep -E '^build libmonero_wallet2_api_c\.(so|dll)' "$ninja" | head -1)"
  if [ -z "$line" ]; then
    echo "error: $ninja does not say what the library is linked with." >&2
    exit 1
  fi
  for library in $(echo "$line" | tr ' ' '\n' | grep '\.a$' | sed 's#.*/##' | sort -u); do
    if ! known "$library"; then
      echo "error: the library is linked with $library ($ninja)," >&2
      echo "which this script has no licence text for. Add its part here." >&2
      exit 1
    fi
  done
done

cd "$here/.."
cargo run --quiet --locked -p veil_notices -- $check --parts "$list" "$out"
