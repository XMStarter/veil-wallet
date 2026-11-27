# Where the signature of a package for Android lies in it, and whether two
# packages are the same but for their signatures. Not a program: read by
# twice.sh and release.sh (`. signature.sh`). Nothing here makes or checks
# a signature, which is Android's own tools' work; it only finds the bytes
# a signature occupies, so that everything else can be compared.
#
# A package is an archive: its files one after the other, then a list of
# them, then twenty-two bytes that say where the list begins. A signature
# is a block of bytes between the files and the list ("APK Signature
# Scheme" 2 and 3). So signing a package moves the list back and changes
# the four bytes that say where it begins, and touches nothing else: the
# files and the list are what is signed. Two packages built from the same
# sources and signed with different keys, or one of them not signed at
# all, are therefore the same in three places and differ in one:
#
#   the files                     the same
#   the signature                 another, or none
#   the list of the files         the same
#   the last twenty-two bytes     the same but for where the list begins
#
# One more thing differs between a package as the build makes it and the
# same package signed again by Android's signing tool, `apksigner`: the
# tool puts empty bytes after the last file, so that the signature begins
# where a block of 4096 bytes begins. Seen with the tool of 2021 that
# Ubuntu 24.04 packages and with the one of 2025 in Android's own kit;
# signing a package that has them leaves them as they are. Up to 4095
# bytes of nothing after the files are therefore no difference here.
#
# The older way of signing, files of its own inside the archive, is not
# read here: a package with such files differs in its files and is
# reported as different, which is the right answer for a build that asks
# for Android 7 or later and is signed without it.

# A number of `$3` bytes read at byte `$2` of the file `$1`, counted from 0.
number_at() {
  od -An -tu"$3" -j"$2" -N"$3" "$1" | tr -d ' '
}

# The bytes of the file `$1` from byte `$2`, counted from 0, `$3` of them.
bytes_at() {
  tail -c +"$(($2 + 1))" "$1" | head -c "$3"
}

# Of the package `$1`: where its files end, where its list begins, and how
# long it is, as three numbers. Between the first two lies the signature;
# they are the same number for a package that is not signed. Fails if the
# file does not end like an archive without a comment, which is how the
# build and Android's signing tool both write one.
signature_of() {
  local apk="$1" size list block files
  size="$(stat -c %s "$apk")" || return 1
  [ "$size" -ge 22 ] || return 1
  [ "$(number_at "$apk" $((size - 22)) 4)" = 101010256 ] || return 1
  list="$(number_at "$apk" $((size - 6)) 4)"
  [ "$list" -le $((size - 22)) ] || return 1
  files="$list"
  if [ "$list" -ge 32 ] &&
    [ "$(bytes_at "$apk" $((list - 16)) 16 | tr -d '\0')" = "APK Sig Block 42" ]; then
    block="$(number_at "$apk" $((list - 24)) 8)"
    files=$((list - block - 8))
    [ "$files" -ge 0 ] || return 1
  fi
  echo "$files $list $size"
}

# Whether the two packages are the same in everything a signature is made
# over: their files, the list of them, and the end of the archive but for
# the place of the list. True for two copies of one file as well.
same_but_for_the_signature() {
  local one="$1" two="$2" files_one list_one size_one files_two list_two size_two rest
  read -r files_one list_one size_one < <(signature_of "$one") || return 1
  read -r files_two list_two size_two < <(signature_of "$two") || return 1
  [ -n "$size_one" ] && [ -n "$size_two" ] || return 1
  # The files, and after them nothing but empty bytes in the one that
  # goes on for longer, fewer than a block of them.
  local shorter="$files_one" longer="$files_two" which="$two"
  if [ "$files_one" -gt "$files_two" ]; then
    shorter="$files_two" longer="$files_one" which="$one"
  fi
  cmp -s -n "$shorter" "$one" "$two" || return 1
  [ $((longer - shorter)) -lt 4096 ] || return 1
  [ "$(bytes_at "$which" "$shorter" $((longer - shorter)) | tr -d '\0' | wc -c)" = 0 ] || return 1
  # The list, and the end up to where it says where the list begins.
  rest=$((size_one - list_one))
  [ "$rest" = $((size_two - list_two)) ] || return 1
  cmp -s <(bytes_at "$one" "$list_one" $((rest - 6))) <(bytes_at "$two" "$list_two" $((rest - 6))) || return 1
  # The two bytes after it: the length of a comment, which there is none of.
  cmp -s <(bytes_at "$one" $((size_one - 2)) 2) <(bytes_at "$two" $((size_two - 2)) 2)
}

# Whether the package `$1` carries a signature of the kind found here.
is_signed() {
  local files list size
  read -r files list size < <(signature_of "$1") || return 1
  [ -n "$size" ] && [ "$files" != "$list" ]
}

# Writes the number `$1` as `$2` bytes, the lowest first, as an archive
# keeps its numbers.
number_as_bytes() {
  local value="$1" bytes="$2" i
  for ((i = 0; i < bytes; i++)); do
    # shellcheck disable=SC2059
    printf "\\x$(printf '%02x' $((value & 255)))"
    value=$((value >> 8))
  done
}

# Writes the package `$1` without its signature as `$2`: its files, its
# list right after them, and an end that says where the list now begins.
# That file is the same for everybody who builds the same sources,
# whatever key their build signed with, so its checksum is what a build
# can be held against. Android installs no package without a signature.
without_signature() {
  local apk="$1" out="$2" files list size
  read -r files list size < <(signature_of "$apk") || return 1
  [ -n "$size" ] || return 1
  {
    head -c "$files" "$apk" &&
      bytes_at "$apk" "$list" $((size - list - 6)) &&
      number_as_bytes "$files" 4 &&
      bytes_at "$apk" $((size - 2)) 2
  } > "$out"
}
