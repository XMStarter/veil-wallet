#!/usr/bin/env bash
# Builds a part of a package twice and says whether the two came out the
# same, byte for byte.
#
#   ./twice.sh core               the core, for the app on Linux
#   ./twice.sh core-android       the core, for the app on Android
#   ./twice.sh transport-android  the transport program the app for Android brings along
#   ./twice.sh wallet             the wallet library, for Linux
#   ./twice.sh wallet-windows     the wallet library, for Windows
#   ./twice.sh wallet-android     the wallet library, for Android
#   ./twice.sh app                the app for Linux, with the core in it built in each place
#   ./twice.sh apk                the app for Android, the same way
#
# Several parts can be given at once. Each is built from the files of this
# checkout as git knows them, changes that are not committed included, in
# two folders that have nothing in common: another path, another depth,
# and for cargo another home. Whatever of the place of a build gets into
# its result shows as a difference. What does not show: another machine,
# another user, another day's compiler packages. The two builds share the
# compilers this machine has.
#
# The folders are made in $VEIL_TWICE_DIR, by default ~/veil-twice, and a
# part's folders are emptied before it is built. They are kept afterwards,
# for a look at what differs.
#
# The wallet library is built at one path on every machine, in a view of
# the folders that its script makes for itself (wallet_lib/build.sh says
# how), so its two builds show what else differs, not the path. The two
# apps take the wallet library and the transport program this machine has
# built already, the same files for both, since those are compared as
# parts of their own; the folders of an app are no checkout, so neither
# app names a commit.
#
# It builds and compares. It starts nothing it has built, no daemon and no
# Tor. It takes long: every part is compiled twice from nothing.
set -uo pipefail

# Read to its end before anything runs: the script may be changed while
# it is running, for hours, and the shell reads a script as it goes.
{

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/.." && pwd)"
work="${VEIL_TWICE_DIR:-$HOME/veil-twice}"
cargo_home="${CARGO_HOME:-$HOME/.cargo}"
export PATH="$cargo_home/bin:$PATH"
# The compilers stay where they are when cargo's home is another.
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"

usage() {
  sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
}

# The two places a part is built in.
place() {
  case "$1" in
    1) echo "$work/$2/one" ;;
    2) echo "$work/$2/the-other/of/two" ;;
  esac
}

# Puts this checkout's files, as git knows them, into the folder `$1`.
checkout_into() {
  mkdir -p "$1" &&
    (cd "$repo" && git ls-files -z --cached --others --exclude-standard |
      while IFS= read -r -d '' file; do
        # A file that was deleted and not yet committed as deleted.
        if [ -e "$file" ]; then printf '%s\0' "$file"; fi
      done | tar --null -T - -cf -) | tar -xf - -C "$1"
}

# Empties the two places of part `$1` and puts the same files into both,
# before anything is built: a file changed while the first build runs
# must not make the second another build.
both_places() {
  rm -rf "${work:?}/$1" &&
    checkout_into "$(place 1 "$1")/veil" &&
    mkdir -p "$(place 2 "$1")" &&
    cp -r "$(place 1 "$1")/veil" "$(place 2 "$1")/veil"
}

# A home for cargo in the folder `$1`, with the packages this machine's
# cargo has fetched already, so that nothing is fetched twice.
cargo_home_in() {
  mkdir -p "$1" &&
    if [ -d "$cargo_home/registry" ]; then
      cp -r "$cargo_home/registry" "$1/registry"
    fi
}

sha256_of() {
  sha256sum "$1" | cut -d' ' -f1
}

# Compares the file `$3` of the two builds of part `$1`, called `$2`.
same_file() {
  local part="$1" name="$2" file="$3" one two
  one="$(place 1 "$part")/$file"
  two="$(place 2 "$part")/$file"
  if [ ! -f "$one" ] || [ ! -f "$two" ]; then
    echo "  MISSING    $name: one of the two builds did not make it"
    return 1
  fi
  if cmp -s "$one" "$two"; then
    echo "  THE SAME   $name  $(stat -c %s "$one") bytes  sha256 $(sha256_of "$one")"
    return 0
  fi
  echo "  DIFFERENT  $name"
  echo "             $one"
  echo "               $(stat -c %s "$one") bytes  sha256 $(sha256_of "$one")"
  echo "             $two"
  echo "               $(stat -c %s "$two") bytes  sha256 $(sha256_of "$two")"
  echo "             bytes that differ: $(cmp -l "$one" "$two" 2>/dev/null | wc -l)"
  return 1
}

# Runs check.sh of the checkout in each place with the levels given, each
# with a home for cargo of its own.
check_in_both() {
  local part="$1" which root
  shift
  both_places "$part" || return 1
  for which in 1 2; do
    root="$(place "$which" "$part")"
    echo
    echo "--- build $which of $part, in $root"
    cargo_home_in "$root/cargo-home" &&
      (cd "$root/veil/native" && CARGO_HOME="$root/cargo-home" "$@") >"$root/build.log" 2>&1
    if [ "$?" != 0 ]; then
      tail -n 30 "$root/build.log"
      echo "error: build $which of $part failed; its log is $root/build.log" >&2
      return 1
    fi
    echo "built"
  done
}

part_core() {
  check_in_both core ./check.sh core &&
    same_file core "the core for Linux" veil/native/target/release/libveil_core.so
}

part_core_android() {
  check_in_both core-android ./check.sh --android core &&
    same_file core-android "the core for Android" \
      veil/native/target/aarch64-linux-android/release/libveil_core.so
}

part_transport_android() {
  local which root ndk
  ndk="${ANDROID_NDK_ROOT:-${VEIL_ANDROID_DIR:-$HOME/veil-android}/sdk/ndk/$(sed -n 's/^android_ndk_version="\(.*\)"$/\1/p' "$here/check.sh")}"
  both_places transport-android || return 1
  for which in 1 2; do
    root="$(place "$which" transport-android)"
    echo
    echo "--- build $which of transport-android, in $root"
    ANDROID_NDK_ROOT="$ndk" "$root/veil/native/transports/build.sh" "$root/transports" \
      >"$root/build.log" 2>&1
    if [ "$?" != 0 ]; then
      tail -n 30 "$root/build.log"
      echo "error: build $which of transport-android failed; its log is $root/build.log" >&2
      return 1
    fi
    echo "built"
  done
  same_file transport-android "the transport program for Android" \
    transports/out/android/arm64-v8a/liblyrebird.so
}

# The wallet library, built by its script with the option `$2` in each
# place; `$3` is the file that comes out, under the script's work folder.
# What Monero's dependency build has downloaded on this machine already is
# used by both, so that neither downloads it again.
wallet_twice() {
  local part="$1" option="$2" file="$3" which root sources tree
  tree="${VEIL_WALLET_LIB_DIR:-$HOME/veil-wallet-lib}"
  if [ "$option" = --android ]; then tree="$tree/android"; fi
  sources="$tree/monero_c/monero/contrib/depends/sources"
  both_places "$part" || return 1
  for which in 1 2; do
    root="$(place "$which" "$part")"
    echo
    echo "--- build $which of $part, in $root"
    (
      if [ -d "$sources" ]; then
        # A copy: the build writes its marks of what it has checked there.
        cp -r "$sources" "$root/sources" && export VEIL_DEPENDS_SOURCES="$root/sources"
      fi
      # shellcheck disable=SC2086
      "$root/veil/native/wallet_lib/build.sh" "$root/wallet-lib" $option
    ) >"$root/build.log" 2>&1
    if [ "$?" != 0 ]; then
      tail -n 30 "$root/build.log"
      echo "error: build $which of $part failed; its log is $root/build.log" >&2
      return 1
    fi
    echo "built"
  done
  same_file "$part" "the wallet library" "wallet-lib/out/$file"
}

part_wallet() {
  wallet_twice wallet "" libmonero_wallet2_api_c.so || return 1
  # For Linux, Polyseed is a file of its own beside the library.
  same_file wallet "Polyseed, beside it" wallet-lib/out/libpolyseed.so.2
}

part_wallet_windows() {
  wallet_twice wallet-windows --windows windows/libmonero_wallet2_api_c.dll
}

part_wallet_android() {
  wallet_twice wallet-android --android android/arm64-v8a/libmonero_wallet2_api_c.so
}

# Compares the folder `$3` of the two builds of part `$1`, called `$2`,
# file by file.
same_tree() {
  local part="$1" name="$2" dir="$3" one two differing
  one="$(place 1 "$part")/$dir"
  two="$(place 2 "$part")/$dir"
  if [ ! -d "$one" ] || [ ! -d "$two" ]; then
    echo "  MISSING    $name: one of the two builds did not make it"
    return 1
  fi
  differing="$(diff -rq --no-dereference "$one" "$two" 2>&1 | sed -e "s|$one|<one>|g" -e "s|$two|<the other>|g")"
  if [ -z "$differing" ]; then
    echo "  THE SAME   $name, all $(find "$one" -type f | wc -l) files"
    return 0
  fi
  echo "  DIFFERENT  $name"
  # shellcheck disable=SC2001
  echo "$differing" | sed 's/^/             /'
  echo "             <one> is $one"
  echo "             <the other> is $two"
  return 1
}

part_app() {
  check_in_both app ./check.sh app &&
    same_tree app "the app for Linux" veil/app/build/linux/x64/release/bundle
}

# A number of `$3` bytes read at byte `$2` of the file `$1`, counted from 0.
number_at() {
  od -An -tu"$3" -j"$2" -N"$3" "$1" | tr -d ' '
}

# Where the signature of the package `$1` lies in it: the first byte of it
# and the first after it, counted from 0. A package is an archive with a
# list of its files at the end, and what it was signed with is a block of
# bytes right in front of that list. Fails if this is no package signed
# that way.
signature_of() {
  local apk="$1" size list block
  size="$(stat -c %s "$apk")"
  # The last 22 bytes say where the list begins, in an archive without a
  # comment, which is what the build makes.
  [ "$(number_at "$apk" $((size - 22)) 4)" = 101010256 ] || return 1
  list="$(number_at "$apk" $((size - 6)) 4)"
  [ "$(dd if="$apk" bs=1 skip=$((list - 16)) count=16 2>/dev/null)" = "APK Sig Block 42" ] || return 1
  block="$(number_at "$apk" $((list - 24)) 8)"
  echo "$((list - block - 8)) $list"
}

# Whether the two packages differ in their signatures and nowhere else.
same_but_for_the_signature() {
  local one="$1" two="$2" where first last
  where="$(signature_of "$one")" || return 1
  [ "$where" = "$(signature_of "$two")" ] || return 1
  # cmp counts from 1.
  first="$(cmp -l "$one" "$two" | head -n 1 | awk '{ print $1 }')"
  last="$(cmp -l "$one" "$two" | tail -n 1 | awk '{ print $1 }')"
  [ -n "$first" ] || return 1
  # shellcheck disable=SC2086
  set -- $where
  [ "$first" -gt "$1" ] && [ "$last" -le "$2" ]
}

# The package is signed where it is built, here with the key Android's
# tools make for trying things, and a signature is not the same twice:
# there is chance in how it is made. So two packages are the same when
# everything in them that is signed is the same, byte for byte, and the
# signature is all they differ in.
part_apk() {
  local apk=veil/app/build/app/outputs/flutter-apk/app-release.apk which one two
  check_in_both apk ./check.sh --android app || return 1
  one="$(place 1 apk)/$apk"
  two="$(place 2 apk)/$apk"
  if same_file apk "the app for Android" "$apk"; then
    return 0
  fi
  if same_but_for_the_signature "$one" "$two"; then
    echo "  THE SAME   all of it that is signed; the two differ in the signature and nowhere else"
    return 0
  fi
  # Which of the files in the package it is.
  for which in 1 2; do
    mkdir -p "$(place "$which" apk)/in-the-package" &&
      unzip -q -o "$(place "$which" apk)/$apk" -d "$(place "$which" apk)/in-the-package" || return 1
  done
  same_tree apk "the files in the package" in-the-package
  return 1
}

if [ "$#" = 0 ]; then
  usage
  exit 2
fi
for part in "$@"; do
  case "$part" in
    core | core-android | transport-android | wallet | wallet-windows | wallet-android | app | apk) ;;
    -h | --help | help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown part '$part'" >&2
      usage >&2
      exit 2
      ;;
  esac
done

mkdir -p "$work"
failed=0
summary=""
echo "Veil built twice: $*"
echo "date:    $(date -u '+%Y-%m-%d %H:%M:%S UTC')"
echo "commit:  $(git -C "$repo" rev-parse --short=12 HEAD 2>/dev/null || echo unknown)$(git -C "$repo" diff --quiet HEAD 2>/dev/null || echo ' (with local changes)')"
echo "in:      $work"
for part in "$@"; do
  echo
  echo "=== $part"
  if ("part_${part//-/_}"); then
    summary+="  THE SAME   $part"$'\n'
  else
    summary+="  DIFFERENT  $part"$'\n'
    failed=1
  fi
done
echo
echo "=== summary"
printf '%s' "$summary"
exit "$failed"
}
