#!/usr/bin/env bash
# Tests signature.sh, which says whether two packages for Android are the
# same but for their signatures, on archives made up here: a few bytes for
# files, a block where a signature would lie, a few bytes for the list,
# and the twenty-two bytes an archive ends with. They are no packages and
# their blocks are no signatures; only the layout is real. Needs nothing
# of Android's.
#
#   ./signature-test.sh
#
# Part of `./check.sh offline`.
set -uo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=native/signature.sh
. "$here/signature.sh"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# Writes the number `$1` as `$2` bytes, the lowest first.
number() {
  number_as_bytes "$@"
}

# Makes the archive `$1` of the files `$2`, a signature of `$3` bytes
# (none where that is empty), and the list `$4`; with `$5` empty bytes
# after the files, and `$6` bytes that are not empty after those.
archive() {
  local out="$work/$1" files="$2" signature="$3" list="$4" begins
  {
    printf '%s' "$files"
    head -c "${5:-0}" /dev/zero
    head -c "${6:-0}" /dev/zero | tr '\0' 'x'
    if [ -n "$signature" ]; then
      # A block: its length, what is in it, its length again, its name.
      number $((signature + 24)) 8
      head -c "$signature" /dev/urandom
      number $((signature + 24)) 8
      printf 'APK Sig Block 42'
    fi
  } > "$out"
  begins="$(stat -c %s "$out")"
  {
    printf '%s' "$list"
    # The end of an archive: its mark, twelve bytes about the list that
    # are the same here, where the list begins, and no comment.
    number 101010256 4
    printf '\0\0\0\0\1\0\1\0'
    number "${#list}" 4
    number "$begins" 4
    number 0 2
  } >> "$out"
}

failed=0
holds() {
  local wanted="$1" what="$2"
  shift 2
  if "$@"; then got=same; else got=different; fi
  if [ "$got" = "$wanted" ]; then
    echo "  ok      $what: $got"
  else
    echo "  FAILED  $what: $got, and it is $wanted"
    failed=1
  fi
}

archive one.apk "the files of a package" 300 "the list"
archive other-key.apk "the files of a package" 811 "the list"
archive unsigned.apk "the files of a package" "" "the list"
archive other-file.apk "the files of a packagE" 300 "the list"
archive longer.apk "the files of a package, and one more" 300 "the list"
archive other-list.apk "the files of a package" 300 "the lisT"
archive padded.apk "the files of a package" 811 "the list" 4095
archive padded-a-block.apk "the files of a package" 811 "the list" 4096
archive padded-with-something.apk "the files of a package" 811 "the list" 100 1
cp "$work/one.apk" "$work/copy.apk"
# A byte changed inside the signature only.
cp "$work/one.apk" "$work/other-signature.apk"
printf 'x' | dd of="$work/other-signature.apk" bs=1 seek=40 conv=notrunc 2>/dev/null
printf 'no archive at all' > "$work/nothing.apk"

echo "signature.sh, on archives made up for it:"
holds same "the same file twice" same_but_for_the_signature "$work/one.apk" "$work/copy.apk"
holds same "another signature of the same length" same_but_for_the_signature "$work/one.apk" "$work/other-signature.apk"
holds same "another signature of another length" same_but_for_the_signature "$work/one.apk" "$work/other-key.apk"
holds same "signed and not signed" same_but_for_the_signature "$work/one.apk" "$work/unsigned.apk"
holds same "not signed and signed" same_but_for_the_signature "$work/unsigned.apk" "$work/other-key.apk"
holds same "empty bytes after the files, as the signing tool puts them" same_but_for_the_signature "$work/one.apk" "$work/padded.apk"
holds same "the same the other way round" same_but_for_the_signature "$work/padded.apk" "$work/unsigned.apk"
holds different "a whole block of empty bytes after the files" same_but_for_the_signature "$work/one.apk" "$work/padded-a-block.apk"
holds different "bytes after the files that are not empty" same_but_for_the_signature "$work/one.apk" "$work/padded-with-something.apk"
holds different "one byte of a file changed" same_but_for_the_signature "$work/one.apk" "$work/other-file.apk"
holds different "a file more" same_but_for_the_signature "$work/one.apk" "$work/longer.apk"
holds different "one byte of the list changed" same_but_for_the_signature "$work/one.apk" "$work/other-list.apk"
holds different "a file that is no archive" same_but_for_the_signature "$work/one.apk" "$work/nothing.apk"
holds same "a signed archive is seen as signed" is_signed "$work/one.apk"
holds different "an archive without one is not" is_signed "$work/unsigned.apk"
without_signature "$work/one.apk" "$work/one-without.apk"
without_signature "$work/other-key.apk" "$work/other-key-without.apk"
without_signature "$work/unsigned.apk" "$work/unsigned-without.apk"
holds same "a signature taken off leaves the archive as it was made without one" cmp -s "$work/one-without.apk" "$work/unsigned.apk"
holds same "whichever signature it was" cmp -s "$work/other-key-without.apk" "$work/unsigned.apk"
holds same "and taking none off changes nothing" cmp -s "$work/unsigned-without.apk" "$work/unsigned.apk"
exit "$failed"
