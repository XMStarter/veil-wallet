#!/usr/bin/env bash
# The app for Windows on the owner's machine, in one command, from WSL.
#
# Hosted CI builds the app for Windows and keeps it with its run as a file
# to download, veil-windows.zip: on the pull request, under Checks, in the
# run named "native", under Artifacts. This script takes that file from the
# Windows Downloads folder, unpacks it onto the Windows disk, puts the
# wallet library that was built on this machine next to it, and starts it.
#
#   ./windows-app.sh                 the newest veil-windows*.zip in Downloads
#   ./windows-app.sh FILE.zip        that file
#   ./windows-app.sh --no-start ...  everything but starting the app
#
# Nothing is installed on Windows. The app goes into the folder Veil-app in
# the Windows user's home and replaces what an earlier run put there. The
# wallet is not in that folder (it is in %APPDATA%\Veil) and is not touched.
#
# The wallet library for Windows comes from ./check.sh --windows setup. The
# app this starts is a real wallet on stagenet: it connects to Tor. So it
# is for the owner's machine, never for hosted CI or a cloud session.
set -uo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
lib_dir="${VEIL_WALLET_LIB_DIR:-$HOME/veil-wallet-lib}"
wallet_lib="$lib_dir/out/windows/libmonero_wallet2_api_c.dll"

# What the app's folder has to hold before it can be started; the wallet
# library is added by this script.
parts=(veil.exe veil_core.dll flutter_windows.dll data)

fail() {
  echo
  echo "error: $*" >&2
  exit 1
}

# The Windows user's home as a Linux path. VEIL_WINDOWS_HOME names another
# folder to stand for it, which is how the script is tried without Windows.
windows_home() {
  if [ -n "${VEIL_WINDOWS_HOME:-}" ]; then
    echo "$VEIL_WINDOWS_HOME"
    return 0
  fi
  command -v cmd.exe >/dev/null 2>&1 && command -v wslpath >/dev/null 2>&1 || return 1
  local profile
  # Asked from a Windows folder: cmd.exe complains about a Linux one.
  profile="$(cd /mnt/c 2>/dev/null && cmd.exe /c 'echo %USERPROFILE%' 2>/dev/null | tr -d '\r')"
  [ -n "$profile" ] || return 1
  wslpath -u "$profile"
}

# Unpacks $1 into the folder $2 with whatever this machine has for it.
unpack() {
  if command -v unzip >/dev/null 2>&1; then
    unzip -q "$1" -d "$2"
  elif command -v python3 >/dev/null 2>&1; then
    python3 -m zipfile -e "$1" "$2"
  else
    return 1
  fi
}

main() {
  local start=1 zip=""
  for word in "$@"; do
    case "$word" in
      --no-start) start=0 ;;
      -h | --help | help)
        sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
      -*)
        echo "error: unknown option '$word'" >&2
        exit 2
        ;;
      *) zip="$word" ;;
    esac
  done

  local home
  home="$(windows_home)" ||
    fail "this is not WSL on Windows: the Windows user's home was not found"
  [ -d "$home" ] || fail "the Windows user's home, $home, is not a folder"

  if [ -z "$zip" ]; then
    # The browser adds " (1)" and so on to a name it has used before; the
    # newest of them is the one meant.
    zip="$(ls -t "$home/Downloads"/veil-windows*.zip 2>/dev/null | head -n 1)"
    [ -n "$zip" ] || fail "no veil-windows*.zip in $home/Downloads.
Download it from the pull request on GitHub: Checks, the run \"native\",
Artifacts, veil-windows."
  fi
  [ -f "$zip" ] || fail "$zip is not a file"
  [ -f "$wallet_lib" ] || fail "the wallet library for Windows is not built: $wallet_lib
Run ./check.sh --windows setup first."
  # A library built before Veil's patches changed still loads, and does
  # what a patch has since taken out of it. The build notes which set of
  # patches it was made from, and in which way it was built.
  local built wanted
  built="$(cat "$(dirname "$wallet_lib")/veil-patches.sha256" 2>/dev/null || true)"
  wanted="$(cat "$here"/wallet_lib/patches/monero/*.patch "$here"/wallet_lib/patches/monero_c/*.patch \
    "$here"/wallet_lib/built-how.txt | sha256sum | cut -d' ' -f1)"
  [ "$built" = "$wanted" ] || fail "the wallet library for Windows was built from older patches, or in an older way: $wallet_lib
Run ./check.sh --windows setup again."

  echo "the app as downloaded:  $zip"
  local dest="$home/Veil-app"
  if [ -e "$dest" ]; then
    # Only a folder this script made is removed: one that holds the app.
    [ -f "$dest/veil.exe" ] || [ -z "$(ls -A "$dest" 2>/dev/null)" ] ||
      fail "$dest exists and is not the app; move it away first"
    rm -rf "$dest" || fail "$dest could not be replaced; is the app still running?"
  fi
  mkdir -p "$dest" || fail "$dest could not be made"
  unpack "$zip" "$dest" || fail "$zip could not be unpacked (neither unzip nor python3 did it)"

  local part
  for part in "${parts[@]}"; do
    [ -e "$dest/$part" ] || fail "the download is not the app: $part is missing in it"
  done
  cp "$wallet_lib" "$dest/" || fail "the wallet library could not be put next to the app"

  echo "the wallet library:     $wallet_lib"
  echo "the app, complete:      $dest"
  echo
  echo "This is a real wallet on stagenet: it connects to Tor and to public"
  echo "stagenet nodes. Its wallet is its own, in %APPDATA%\\Veil, not the one"
  echo "of the app started inside WSL. An antivirus may ask about veil_core.dll"
  echo "and libmonero_wallet2_api_c.dll the first time; the app cannot open a"
  echo "wallet until both are allowed."
  if [ "$start" = 0 ]; then
    return 0
  fi
  echo
  echo "starting the app..."
  # Started by Windows itself, from the app's own folder, so that it goes
  # on when this terminal is closed.
  (cd "$dest" && cmd.exe /c start "" veil.exe) ||
    fail "the app could not be started; start $dest/veil.exe by hand"
}

main "$@"
