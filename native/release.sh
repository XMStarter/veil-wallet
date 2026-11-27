#!/usr/bin/env bash
# Signs a release of Veil for Android, and says whether two packages are
# the same but for their signatures.
#
#   ./release.sh key               once, on the machine that signs: makes the key for releases
#   ./release.sh sign <package>    signs the package `./check.sh --android release` has built
#   ./release.sh compare <a> <b>   whether two packages differ in their signatures and nowhere else
#   ./release.sh who <package>     what a package says it is, and who signed it
#
# The first two are for the owner's machine. The key for releases is made
# there and stays there: Android takes an update of an app only from the
# key the app was first signed with, so whoever has that key can hand out
# a Veil of their own, and whoever loses it can never update Veil again.
# The key is a file outside this checkout, locked with a password that is
# typed in each time and kept nowhere. Hosted CI is never given either.
#
# The last two are for anybody. A release is built from a commit by
# `VEIL_NETWORK=mainnet ./check.sh --android release`, which writes the
# package without a signature; signing puts a signature between its files
# and the list of them and changes nothing else. So whoever builds the
# same commit has the released package but for its signature, and
# `compare` says so:
#
#   ./release.sh compare downloaded.apk target/packages/veil-<version>-<commit>-unsigned.apk
#
# Who signed a package is the checksum of the signer's certificate. The
# one of the key for releases is in release-key.sha256, once there is a
# key, and wherever a release is published.
#
# Needs Android's signing tool and its tool for reading a package, which
# Ubuntu has as packages: sudo apt install apksigner aapt
set -uo pipefail

{
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=native/signature.sh
. "$here/signature.sh"

# The name Android knows the released app by. A package under any other
# name is a build for trying (app/android/app/build.gradle.kts).
released_as="io.github.xmstarter.veil"
key="${VEIL_RELEASE_KEY:-$HOME/veil-release-key/veil-release.p12}"
key_name="veil"
out_dir="${VEIL_RELEASE_DIR:-$HOME/veil-release}"
recorded_in="$here/release-key.sha256"

usage() {
  sed -n '2,8p' "$0" | sed 's/^# \{0,1\}//'
}

fail() {
  echo "error: $*" >&2
  exit 1
}

# The first program of the given names that is installed, on the path or
# in the Android kit `./check.sh --android setup` installs.
tool() {
  local name found
  for name in "$@"; do
    if found="$(command -v "$name" 2>/dev/null)"; then
      echo "$found"
      return 0
    fi
  done
  for name in "$@"; do
    for found in "${VEIL_ANDROID_DIR:-$HOME/veil-android}"/sdk/build-tools/*/"$name"; do
      if [ -x "$found" ]; then
        echo "$found"
        return 0
      fi
    done
  done
  return 1
}

need_tools() {
  apksigner="$(tool apksigner)" && aapt="$(tool aapt aapt2)" && command -v keytool >/dev/null 2>&1 ||
    fail "this needs Android's signing tools. On Ubuntu: sudo apt install apksigner aapt"
}

# The checksum of the certificate of the one signer of a package, or
# nothing and a failure where the package is not signed, does not hold
# together, or has several signers.
signer_of() {
  local said
  said="$("$apksigner" verify --print-certs "$1" 2>/dev/null)" || return 1
  [ "$(printf '%s\n' "$said" | grep -c 'certificate SHA-256 digest:')" = 1 ] || return 1
  printf '%s\n' "$said" | sed -n 's/^Signer #1 certificate SHA-256 digest: //p'
}

# What the repository names as the key for releases, or nothing.
recorded() {
  if [ -f "$recorded_in" ]; then
    tr -d '[:space:]' < "$recorded_in"
  fi
}

# What a signer is to this repository, in words.
named() {
  local signer="$1" known
  known="$(recorded)"
  if [ -z "$known" ]; then
    echo "the repository names no key for releases yet"
  elif [ "$signer" = "$known" ]; then
    echo "the key for releases"
  else
    echo "NOT the key for releases, which is $known"
  fi
}

# The name and the version a package gives itself.
says_of_itself() {
  "$aapt" dump badging "$1" 2>/dev/null | sed -n "s/^package: name='\([^']*\)' versionCode='\([^']*\)' versionName='\([^']*\)'.*/\1 \2 \3/p"
}

# A password typed in, without showing it. Read from whatever this was
# started with where that is no keyboard, which is how it is tried.
ask_password() {
  local prompt="$1" password
  if [ -t 0 ]; then
    read -rsp "$prompt" password
    echo >&2
  else
    read -r password
  fi
  printf '%s' "$password"
}

make_key() {
  need_tools
  if [ -e "$key" ]; then
    fail "there is a key already: $key
It is not replaced. A release signed with another key is another app to
Android, and nobody who has Veil could update to it."
  fi
  cat <<EOF
This makes the key that every release of Veil for Android is signed with.

  - It is one file, $key, locked with a password.
  - Whoever has both can publish a Veil that every phone takes as an
    update. Whoever loses either can never update Veil again.
  - The password is asked for now, twice, and at every signing. It is
    written nowhere. Take a long one that you do not use elsewhere.

EOF
  local password again
  password="$(ask_password 'A password for the key: ')"
  again="$(ask_password 'The same once more: ')"
  [ "$password" = "$again" ] || fail "the two were not the same. Nothing was made."
  [ "${#password}" -ge 12 ] || fail "that is under twelve characters. Nothing was made."
  mkdir -p "$(dirname "$key")" && chmod 700 "$(dirname "$key")" || fail "cannot make $(dirname "$key")"
  # The password goes to the tool on the line it reads, not among its
  # arguments, where other programs of the machine could see it.
  printf '%s\n%s\n' "$password" "$password" |
    keytool -genkeypair -keystore "$key" -storetype PKCS12 -alias "$key_name" \
      -keyalg RSA -keysize 4096 -sigalg SHA256withRSA -validity 36500 -dname "CN=Veil" >/dev/null 2>&1
  [ -s "$key" ] || fail "the key was not made; keytool said no more than that."
  chmod 600 "$key"
  local certificate
  certificate="$(printf '%s\n' "$password" | keytool -list -keystore "$key" -alias "$key_name" 2>/dev/null |
    sed -n 's/.*(SHA-256): //p' | tr -d ':' | tr 'A-F' 'a-f')"
  [ "${#certificate}" = 64 ] || fail "the key was made but could not be read back: $key"
  cat <<EOF

The key for releases is made: $key

Its certificate, which is no secret and is what a download is known by:

  $certificate

Now, before anything is signed with it:
  1. Copy the file to something that is not this machine and is not
     connected to anything, and keep it where the password is not.
  2. Send the line above to be recorded in the repository
     (native/release-key.sha256).
EOF
}

sign() {
  local package="${1:-}"
  [ -n "$package" ] || fail "which package? ./release.sh sign <package>"
  [ -f "$package" ] || fail "there is no file $package"
  need_tools
  [ -f "$key" ] || fail "there is no key for releases in $key. It is made once, with ./release.sh key"

  local name code version
  read -r name code version < <(says_of_itself "$package")
  [ -n "${version:-}" ] || fail "$package cannot be read as a package for Android"
  if [ "$name" != "$released_as" ]; then
    fail "this package is $name, which is a build for trying. A release is
$released_as, built by: VEIL_NETWORK=mainnet ./check.sh --android release"
  fi

  local out="$out_dir/veil-$version.apk" part
  mkdir -p "$out_dir" || fail "cannot make $out_dir"
  part="$out.part"
  rm -f "$part"
  echo "signing $name, version $version (number $code)"
  echo "  from:   $package"
  echo "  sha256: $(sha256sum "$package" | cut -d' ' -f1)"

  # Signed the way Android has verified since version 7, which is the
  # oldest the app asks for. The newer of the two tools moves the files
  # of a package about unless it is told to leave them, which would make
  # the release another package than the one that was built.
  local keep=()
  if "$apksigner" sign --help 2>&1 | grep -q -- '--alignment-preserved'; then
    keep=(--alignment-preserved true)
  fi
  local password
  password="$(ask_password 'The password of the key for releases: ')"
  if ! printf '%s\n' "$password" | "$apksigner" sign --ks "$key" --ks-key-alias "$key_name" --ks-pass stdin \
    --v1-signing-enabled false --v2-signing-enabled true --v3-signing-enabled true \
    "${keep[@]}" --in "$package" --out "$part" >/dev/null 2>&1; then
    rm -f "$part" "$part.idsig"
    fail "not signed. A wrong password is the usual reason."
  fi
  rm -f "$part.idsig"

  # What was made is looked at before it gets its name.
  local signer known
  if ! signer="$(signer_of "$part")" || [ -z "$signer" ]; then
    rm -f "$part"
    fail "the signed package does not verify. Nothing is kept."
  fi
  if ! same_but_for_the_signature "$package" "$part"; then
    rm -f "$part"
    fail "signing changed more than the signature. Nothing is kept."
  fi
  known="$(recorded)"
  if [ -n "$known" ] && [ "$signer" != "$known" ]; then
    rm -f "$part"
    fail "the key in $key is not the key for releases the repository names.
  this key: $signer
  recorded: $known
Nothing is kept."
  fi
  mv -f "$part" "$out" || fail "cannot write $out"

  echo
  echo "signed: $out"
  echo "  sha256:    $(sha256sum "$out" | cut -d' ' -f1)"
  echo "  signed by: $signer"
  echo "             ($(named "$signer"))"
  echo "  the same as the package it was made from, but for the signature"
  hand_over "$out"
}

# Under WSL, puts the signed package into the Windows Downloads folder as
# well, from where it can be copied to a phone.
hand_over() {
  local file="$1" profile downloads
  if command -v cmd.exe >/dev/null 2>&1 && command -v wslpath >/dev/null 2>&1; then
    profile="$(cd /mnt/c 2>/dev/null && cmd.exe /c 'echo %USERPROFILE%' 2>/dev/null | tr -d '\r')"
    if [ -n "$profile" ]; then
      downloads="$(wslpath -u "$profile")/Downloads"
      if [ -d "$downloads" ] && cp "$file" "$downloads/"; then
        echo "copied to the Windows Downloads folder as ${file##*/}"
      fi
    fi
  fi
}

# One line about who signed a package.
signed_by() {
  local package="$1" signer
  if ! is_signed "$package"; then
    echo "not signed"
  elif [ -z "${apksigner:-}" ]; then
    echo "signed; by whom cannot be said without Android's signing tool (sudo apt install apksigner)"
  elif signer="$(signer_of "$package")" && [ -n "$signer" ]; then
    echo "signed by $signer ($(named "$signer"))"
  else
    echo "signed, and the signature does NOT hold"
  fi
}

compare() {
  local one="${1:-}" two="${2:-}"
  [ -n "$one" ] && [ -n "$two" ] || fail "which two? ./release.sh compare <package> <package>"
  [ -f "$one" ] || fail "there is no file $one"
  [ -f "$two" ] || fail "there is no file $two"
  apksigner="$(tool apksigner)" || apksigner=""
  echo "$one"
  echo "  sha256: $(sha256sum "$one" | cut -d' ' -f1)"
  echo "  $(signed_by "$one")"
  echo "$two"
  echo "  sha256: $(sha256sum "$two" | cut -d' ' -f1)"
  echo "  $(signed_by "$two")"
  echo
  if cmp -s "$one" "$two"; then
    echo "THE SAME   the two are one file"
  elif same_but_for_the_signature "$one" "$two"; then
    echo "THE SAME   but for the signature: the files in the two packages and the list of them are the same, byte for byte"
  else
    echo "DIFFERENT  the two differ in more than their signatures"
    return 1
  fi
}

who() {
  local package="${1:-}" name code version
  [ -n "$package" ] || fail "which package? ./release.sh who <package>"
  [ -f "$package" ] || fail "there is no file $package"
  need_tools
  read -r name code version < <(says_of_itself "$package")
  [ -n "${version:-}" ] || fail "$package cannot be read as a package for Android"
  echo "$package"
  echo "  is:     $name, version $version (number $code)"
  echo "  sha256: $(sha256sum "$package" | cut -d' ' -f1)"
  echo "  $(signed_by "$package")"
}

case "${1:-}" in
  key) make_key ;;
  sign) sign "${2:-}" ;;
  compare) compare "${2:-}" "${3:-}" ;;
  who) who "${2:-}" ;;
  -h | --help | help | "")
    usage
    [ -n "${1:-}" ]
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac
exit
}
