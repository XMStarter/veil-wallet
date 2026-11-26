#!/usr/bin/env bash
# The licence texts the app carries for everything its packages are built
# from, in app/assets/licences/. Writes them, or with --check says whether
# they are still right; `./check.sh offline` asks that.
#
#   ./notices.sh [--check]
#
#   core.txt            the Rust packages in Veil's core (veil_notices)
#   veil.txt            Veil's own licence, from ../LICENSE
#   typefaces.txt       the three typefaces, from ../app/fonts
#   android.txt         the libraries an app for Android is built with
#   wallet-library.txt  the wallet library and what it is built from
#   transport.txt       the transport program in the app for Android, and
#                       the Go packages in it
#
# The last two are read from the source those two are built from, which
# is on a machine only once they were built there (wallet_lib/build.sh,
# transports/build.sh). Where it is not, they are left as they are and
# that is said: this script then shows nothing about them.
#
# android.txt is a list written by hand, below. All of those libraries
# are under the Apache License, version 2.0; the list is what
# `./gradlew :app:dependencies --configuration releaseRuntimeClasspath`
# names in app/android besides Flutter's own parts, whose texts Flutter
# puts into the app itself.
set -euo pipefail

check=""
if [ "${1:-}" = "--check" ]; then
  check="--check"
elif [ $# -gt 0 ]; then
  echo "usage: notices.sh [--check]" >&2
  exit 2
fi
here="$(cd "$(dirname "$0")" && pwd)"
cd "$here"
kept="$here/../app/assets/licences"
mkdir -p "$kept"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
status=0

notices() {
  cargo run --quiet --locked -p veil_notices -- $check "$@" || status=1
}

notices

printf 'Veil\t%s\n' "$here/../LICENSE" > "$tmp/veil"
notices --parts "$tmp/veil" "$kept/veil.txt"

fonts="$here/../app/fonts"
{
  printf 'Cormorant Garamond (typeface)\t%s\n' "$fonts/CormorantGaramond-OFL.txt"
  printf 'Jost (typeface)\t%s\n' "$fonts/Jost-OFL.txt"
  printf 'JetBrains Mono (typeface)\t%s\n' "$fonts/JetBrainsMono-OFL.txt"
} > "$tmp/typefaces"
notices --parts "$tmp/typefaces" "$kept/typefaces.txt"

apache="$here/veil_notices/texts/Apache-2.0.txt"
for library in "AndroidX libraries" "Kotlin standard library" "kotlinx.coroutines 1.7.1" \
  "JetBrains annotations 13.0" "ZXing core 3.4.1" "ZXing Android Embedded 4.3.0" \
  "ReLinker 1.4.5" "Guava ListenableFuture 1.0"; do
  printf '%s\t%s\n' "$library" "$apache"
done > "$tmp/android"
notices --parts "$tmp/android" "$kept/android.txt"

wallet="${VEIL_WALLET_LIB_DIR:-$HOME/veil-wallet-lib}"
if [ -f "$wallet/monero_c/monero/LICENSE" ] || [ -f "$wallet/android/monero_c/monero/LICENSE" ]; then
  if ls "$wallet"/monero_c/monero/contrib/depends/sources/boost_* \
    "$wallet"/android/monero_c/monero/contrib/depends/sources/boost_* >/dev/null 2>&1; then
    "$here/wallet_lib/notices.sh" $check "$wallet" || status=1
  else
    echo "wallet-library.txt: left as it is. The source archives of the wallet"
    echo "library's parts are on a machine once it was built there for Windows or"
    echo "for Android, and it was not in $wallet."
  fi
else
  echo "wallet-library.txt: left as it is; there is no wallet library source in $wallet."
fi

transports="${VEIL_TRANSPORT_DIR:-$HOME/veil-transports}"
if [ -f "$transports/out/android/arm64-v8a/liblyrebird.so" ]; then
  "$here/transports/notices.sh" $check "$transports" || status=1
else
  echo "transport.txt: left as it is; there is no transport program built in $transports."
fi

exit "$status"
