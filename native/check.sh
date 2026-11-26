#!/usr/bin/env bash
# Runs Veil's checks and writes a log of the run.
#
#   ./check.sh offline     format, lint, and the tests that need no network
#   ./check.sh regtest     the wallet on a private chain (starts a Monero daemon)
#   ./check.sh tor         the tests that start the real Tor client
#   ./check.sh stagenet    the command-line tool on stagenet, through Tor
#   ./check.sh bridges     Tor through obfs4 and Snowflake bridges
#   ./check.sh leaks       the offline tests again, with every network call recorded: none may leave the machine
#   ./check.sh leaks-chain the same for the tests on a private chain (starts a Monero daemon)
#   ./check.sh fuzz        the tests on made-up input again, for a minute each and from a seed of this run's own
#   ./check.sh daemons     not a check: starts test daemons in pairs to see why one may not come up
#   ./check.sh flutter     installs the pinned Flutter and nothing else (what hosted CI runs first)
#   ./check.sh setup       once per machine: packages, Rust, the wallet library
#
# Several levels can be given at once: ./check.sh offline regtest tor
#
# With --windows in front, the same levels are built for Windows and run as
# Windows programs: ./check.sh --windows offline regtest tor stagenet. That
# works inside WSL, which starts Windows programs for a Linux shell; nothing
# has to be installed on the Windows side. `./check.sh --windows setup`
# prepares it: a cross compiler and the wallet library built for Windows.
#
# With --android in front, the code is built for Android on 64-bit ARM and
# nothing is run, because nothing here can run it:
#   ./check.sh --android offline   lint, the core built for Android, the tests compiled
#   ./check.sh --android ndk       installs the pinned Android NDK and nothing else
#   ./check.sh --android setup     once per machine: the NDK, the Android SDK, the wallet library and the
#                                  transport program for Android
#   ./check.sh --android app       the app as one file to install on a phone (an APK)
# The other levels are tests, and tests run on a desktop.
#
# `offline` runs anywhere, hosted CI included. Every other level refuses to
# start in hosted CI: see docs/development.md, "Hosted CI and the hosting
# account". They are for the owner's machine.
#
# The wallet library is looked for in $VEIL_WALLET_LIB_DIR, by default
# ~/veil-wallet-lib, which is where `setup` builds it.
#
# The log goes to target/check-logs/. Send that file when asked for the
# result of a run.
set -uo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
cd "$here"

windows=0
android=0
case "${1:-}" in
  --windows)
    windows=1
    shift
    ;;
  --android)
    android=1
    shift
    ;;
esac

lib_dir="${VEIL_WALLET_LIB_DIR:-$HOME/veil-wallet-lib}"
# Where the transport program the app for Android brings along for Tor
# bridges is built (transports/build.sh).
transport_dir="${VEIL_TRANSPORT_DIR:-$HOME/veil-transports}"
if [ "$android" = 1 ]; then
  out_dir="$lib_dir/out/android/arm64-v8a"
  wallet_lib="$out_dir/libmonero_wallet2_api_c.so"
  monerod=""
  cargo_target=(--target aarch64-linux-android)
  setup_hint="./check.sh --android setup"
elif [ "$windows" = 1 ]; then
  out_dir="$lib_dir/out/windows"
  wallet_lib="$out_dir/libmonero_wallet2_api_c.dll"
  monerod="$out_dir/monerod.exe"
  cargo_target=(--target x86_64-pc-windows-gnu)
  setup_hint="./check.sh --windows setup"
  # WSL hands a Windows program only the variables named here; /p turns a
  # Linux path into the Windows one.
  export WSLENV="${WSLENV:+$WSLENV:}VEIL_WALLET_LIB/p:VEIL_MONEROD/p:VEIL_REQUIRE_WALLET_LIB:VEIL_DAEMON_ROUNDS:VEIL_TEST_NODES:VEIL_TEST_ONION:VEIL_FUZZ_SEED:VEIL_FUZZ_SECONDS:VEIL_FUZZ_CASES:CI:GITHUB_ACTIONS"
else
  out_dir="$lib_dir/out"
  wallet_lib="$out_dir/libmonero_wallet2_api_c.so"
  monerod="$out_dir/monerod"
  cargo_target=()
  setup_hint="./check.sh setup"
fi

# Flutter, which brings Dart, for the app and for the Dart side of the core.
# `setup` puts this exact version here and checks the download against the
# checksum, which is the one published in Flutter's list of releases.
flutter_version="3.47.9"
flutter_sha256_linux="25ed8c3c29d51a7353f2a944310b478af8214920639524b19c50a19e6c044c5a"
flutter_sha256_windows="d8e2b5977e09082c6440f89dc1aa887b94c8fbcc3b39222797046295a3f68a9e"
flutter_dir="${VEIL_FLUTTER_DIR:-$HOME/veil-flutter}"

# The Android NDK: the compiler and libraries that turn the core into code
# for Android. This is the release the pinned Flutter builds apps with, so
# that the core and the app are made with the same one. `setup` and `ndk`
# put it here and check the download against this checksum. Google
# publishes a SHA-1 for the file in its list of SDK packages
# (a7b54a5de87fecd125a17d54f73c446199e72a64); this is the SHA-256 of the
# file with that SHA-1.
android_ndk_release="r28c"
android_ndk_version="28.2.13676358"
android_ndk_sha256="dfb20d396df28ca02a8c708314b814a4d961dc9074f9a161932746f815aa552f"
android_dir="${VEIL_ANDROID_DIR:-$HOME/veil-android}"
# The rest of what building an app for Android takes: Google's Android
# SDK. Its command-line tools are pinned here like the NDK (SHA-1 in the
# same list: e025545c62a8e64c7559119566a569fb1dec5f60); the packages they
# then fetch are checked by the tools themselves against that list. The
# NDK lies inside the SDK, under the name the app's build looks for it
# by, so that there is one copy of it and not a second, downloaded by
# that build.
android_sdk="$android_dir/sdk"
android_ndk="$android_sdk/ndk/$android_ndk_version"
android_tools_release="16111833"
android_tools_sha256="0877a1d048fe4a24efe2eff536ca4223f7adeb58648bb81909d33c446918cfa8"
android_sdk_packages=("platforms;android-36" "build-tools;36.0.0" "platform-tools")
# The oldest Android the core is built for: the oldest the pinned Flutter
# builds apps for.
android_api=24

# Whether this shell runs on Windows itself (Git Bash on a CI runner), as
# opposed to Linux or WSL.
on_windows() {
  case "$(uname -s)" in
    MINGW* | MSYS* | CYGWIN*) return 0 ;;
    *) return 1 ;;
  esac
}

# Prints the path of the Dart to use, or nothing.
dart_program() {
  if [ -x "$flutter_dir/flutter/bin/dart" ]; then
    echo "$flutter_dir/flutter/bin/dart"
  fi
}

usage() {
  sed -n '2,31p' "$0" | sed 's/^# \{0,1\}//'
}

# cargo with the target of this run. An empty array needs this spelling in
# older shells.
cargo_for_target() {
  local command="$1"
  shift
  cargo "$command" ${cargo_target[@]+"${cargo_target[@]}"} "$@"
}

# True where a Linux shell can start Windows programs.
can_run_windows_programs() {
  [ -e /proc/sys/fs/binfmt_misc/WSLInterop ] ||
    [ -e /proc/sys/fs/binfmt_misc/WSLInterop-late ] ||
    grep -qi microsoft /proc/sys/kernel/osrelease 2>/dev/null
}

# True on a CI runner. The same test as native/veil_test_guard.
in_hosted_ci() {
  local name value
  for name in GITHUB_ACTIONS CI; do
    value="${!name:-}"
    value="${value//[[:space:]]/}"
    case "$value" in
      "" | 0 | [Ff][Aa][Ll][Ss][Ee]) ;;
      *) return 0 ;;
    esac
  done
  return 1
}

step() {
  echo
  echo "--- $*"
  "$@"
}

# Says whether the built wallet library carries the patches this checkout
# has: "current", "stale" (built from another patch set), or "missing".
wallet_lib_state() {
  if [ ! -f "$wallet_lib" ]; then
    echo missing
    return
  fi
  local built wanted
  built="$(cat "$out_dir/veil-patches.sha256" 2>/dev/null || true)"
  # The same sum build.sh takes: the Android build has a patch of its own.
  local patches=(wallet_lib/patches/monero/*.patch wallet_lib/patches/monero_c/*.patch)
  if [ "$android" = 1 ]; then
    patches+=(wallet_lib/patches/android/bc-ur/*.patch)
  fi
  wanted="$(cat "${patches[@]}" | sha256sum | cut -d' ' -f1)"
  if [ "$built" = "$wanted" ]; then echo current; else echo stale; fi
}

# For the levels that cannot do without the library and the daemon.
require_wallet_lib() {
  case "$(wallet_lib_state)" in
    current) ;;
    stale)
      echo "error: the wallet library in $out_dir was built from older patches." >&2
      echo "Run $setup_hint again; it rebuilds only what changed." >&2
      return 1
      ;;
    *)
      echo "error: no wallet library in $out_dir. Run $setup_hint first." >&2
      return 1
      ;;
  esac
  export VEIL_WALLET_LIB="$wallet_lib" VEIL_REQUIRE_WALLET_LIB=1
}

# Tells cargo and the crates that compile C where the Android compiler is.
android_env() {
  local bin="$android_ndk/toolchains/llvm/prebuilt/linux-x86_64/bin"
  if [ ! -x "$bin/aarch64-linux-android$android_api-clang" ]; then
    echo "error: the Android NDK is not in $android_ndk." >&2
    echo "Run ./check.sh --android ndk (or --android setup, which also builds the wallet library)." >&2
    return 1
  fi
  export ANDROID_NDK_ROOT="$android_ndk" ANDROID_NDK_HOME="$android_ndk"
  export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$bin/aarch64-linux-android$android_api-clang"
  export CC_aarch64_linux_android="$bin/aarch64-linux-android$android_api-clang"
  export CXX_aarch64_linux_android="$bin/aarch64-linux-android$android_api-clang++"
  export AR_aarch64_linux_android="$bin/llvm-ar"
}

# For Android: the lint, the core as the library an app loads, and every
# test compiled. None of it is run; that takes a phone.
level_offline_android() {
  android_env || return 1
  case "$(wallet_lib_state)" in
    current) echo "wallet library for Android: $wallet_lib" ;;
    stale) echo "wallet library for Android: built from older patches. Run $setup_hint." ;;
    *) echo "wallet library for Android: not built. It is not needed to build the core." ;;
  esac
  step cargo fmt --check &&
    step cargo_for_target clippy --workspace --all-targets --all-features --locked -- -D warnings &&
    step cargo_for_target build -p veil_core --release --locked &&
    step cargo_for_target test --workspace --locked --no-run &&
    echo &&
    echo "Built for Android and not run: there is nothing here to run it on."
}

level_offline() {
  if [ "$android" = 1 ]; then
    level_offline_android
    return
  fi
  if [ -z "${VEIL_WALLET_LIB:-}" ] && [ "$(wallet_lib_state)" = current ]; then
    export VEIL_WALLET_LIB="$wallet_lib"
  fi
  if [ -n "${VEIL_WALLET_LIB:-}" ]; then
    echo "wallet library: $VEIL_WALLET_LIB"
  elif [ "$(wallet_lib_state)" = stale ]; then
    echo "wallet library: built from older patches and not used; the tests that need"
    echo "it are skipped. Run $setup_hint to rebuild it."
  else
    echo "wallet library: not built, the tests that need it are skipped"
  fi
  step cargo fmt --check &&
    step cargo_for_target clippy --workspace --all-targets --all-features --locked -- -D warnings &&
    step cargo_for_target test --workspace --locked --no-fail-fast &&
    notices_side &&
    dart_side &&
    app_side
}

# What tells an app which commit it is built from, so that it can say
# where its source is (Settings, About Veil): the commit's hash, with
# "+changes" where files differ from it, and nothing where this is no
# checkout at all.
built_from() {
  local commit=""
  if commit="$(git -C "$here" rev-parse --short=12 HEAD 2>/dev/null)"; then
    if [ -n "$(git -C "$here" status --porcelain 2>/dev/null)" ]; then
      commit="$commit+changes"
    fi
  fi
  echo "--dart-define=VEIL_COMMIT=$commit"
}

# The licence texts the app carries (app/assets/licences): whether they
# are still those of what the packages are built from. See notices.sh.
# Once, as Linux programs: the texts are the same for every system, and
# the script hands the program paths that a Windows program cannot read.
notices_side() {
  if [ "$windows" = 1 ] || on_windows; then
    return 0
  fi
  step "$here/notices.sh" --check
}

# The Dart side of the core: its own tests, which also load the core as a
# shared library and call it. Nothing in them opens a wallet or starts Tor.
dart_side() {
  if [ "$windows" = 1 ]; then
    echo
    echo "Dart side: not run with --windows; there is no Dart for Windows here."
    return 0
  fi
  local dart
  dart="$(dart_program)"
  if [ -z "$dart" ]; then
    echo
    echo "Dart side: Flutter is not installed in $flutter_dir, its tests are skipped."
    echo "Run ./check.sh setup to install it."
    return 0
  fi
  step cargo build -p veil_core --locked || return 1
  # Packages come from the registry the first time and from the local cache
  # after that. Where cargo is told to stay offline, so is this.
  local offline=()
  if [ "${CARGO_NET_OFFLINE:-}" = true ]; then offline=(--offline); fi
  (
    cd "$here/../dart/veil_bridge" &&
      step "$dart" pub get --enforce-lockfile "${offline[@]}" &&
      step "$dart" format --output=none --set-exit-if-changed . &&
      step "$dart" analyze --fatal-infos &&
      step "$dart" test
  )
}

# The tests that hand made-up input to whatever reads text or bytes from
# outside (`made_up` in their names; `veil_test_guard/src/fuzz.rs` makes the
# input). The offline level runs them too, a few thousand inputs each from
# a seed that never changes. Here each runs for a time, from a seed of this
# run's own, so that every run tries inputs no run has tried. A failure
# names the seed and the number of the input.
level_fuzz() {
  if [ -z "${VEIL_WALLET_LIB:-}" ] && [ "$(wallet_lib_state)" = current ]; then
    export VEIL_WALLET_LIB="$wallet_lib"
  fi
  if [ -n "${VEIL_WALLET_LIB:-}" ]; then
    echo "wallet library: $VEIL_WALLET_LIB"
  else
    echo "wallet library: not built or built from older patches; what it reads is not tried"
  fi
  export VEIL_FUZZ_SEED="${VEIL_FUZZ_SEED:-$(date +%s)}"
  export VEIL_FUZZ_SECONDS="${VEIL_FUZZ_SECONDS:-60}"
  echo "seed:    $VEIL_FUZZ_SEED"
  echo "time:    $VEIL_FUZZ_SECONDS seconds for each test"
  step cargo_for_target test --workspace --locked --no-fail-fast made_up || return 1
  if [ "$windows" = 1 ] || [ -z "$(dart_program)" ]; then
    echo
    echo "The app's tests on made-up input: not run here, for want of Flutter."
    return 0
  fi
  local flutter="$flutter_dir/flutter/bin/flutter" offline=()
  if [ "${CARGO_NET_OFFLINE:-}" = true ]; then offline=(--offline); fi
  (
    cd "$here/../app" &&
      step "$flutter" --no-version-check pub get --enforce-lockfile "${offline[@]}" &&
      step "$flutter" --no-version-check test --no-pub --timeout none test/made_up_test.dart
  )
}

level_regtest() {
  require_wallet_lib || return 1
  if [ ! -x "$monerod" ]; then
    echo "error: no daemon in $out_dir. Run $setup_hint first." >&2
    return 1
  fi
  export VEIL_MONEROD="$monerod"
  # The wallet alone, then the wallet behind the stand-in for Tor.
  step cargo_for_target test -p veil_wallet --features regtest --locked || return
  step cargo_for_target test -p veil_core --features regtest --locked
  local status=$?
  where_it_ended "$status" -p veil_core --features regtest --locked
  return "$status"
}

# The rule that nothing reaches the network except through Tor, checked
# from outside the programs. In the tests Tor is a stand-in on loopback,
# so every address any network call of theirs names has to be this
# machine's own. `strace` records the calls of the tests and of everything
# they start, and `veil-trace-check` (veil_test_guard) reads the record:
# an address that is not loopback, a name looked up, a socket bound to
# every interface, or anything it cannot read fails the level. Linux only:
# the record is the Linux kernel's.
#
# What this does not see: the real Tor client, whose own connections do
# leave the machine and are meant to (the `tor` level's tests are not
# traced); the app's windows and Flutter's own code; Windows and Android.
traced_tests() {
  local name="$1"
  shift
  if [ "$windows" = 1 ]; then
    echo "error: '$name' records the calls of Linux programs; there is no --windows form of it." >&2
    return 1
  fi
  if ! command -v strace >/dev/null; then
    echo "error: strace is not installed. Install it: sudo apt-get install -y strace" >&2
    return 1
  fi
  # Without the wallet library its tests are skipped, and the library is
  # most of what is to be looked at.
  require_wallet_lib || return 1
  export VEIL_REQUIRE_WALLET_LIB=1
  local record="$here/target/check-logs/$name-$(date -u '+%Y%m%d-%H%M%S').strace"
  # Built first and unrecorded: compiling is not what is looked at, and
  # cargo is told to fetch nothing while it is.
  step cargo test "$@" --locked --no-run &&
    step cargo build -p veil_test_guard --bin veil-trace-check --locked || return 1
  echo
  echo "--- the same tests, with their network calls recorded in $record"
  # --seccomp-bpf: the kernel stops a program only at the calls that are
  # recorded and lets every other call through at full speed. Without it
  # each call of every program is stopped, which made the wallet's tests
  # four times slower and the daemon too slow to answer in time.
  # --no-fail-fast: a test that fails does not keep the rest from being
  # recorded.
  # -q and not -qq: how each program ended stays in the record, so that
  # the record alone says whether the test programs passed.
  CARGO_NET_OFFLINE=true strace -f --seccomp-bpf -q -o "$record" \
    -e trace=network,execve,clone,clone3,fork,vfork \
    cargo test "$@" --locked --no-fail-fast
  local status=$?
  if [ "$status" != 0 ]; then
    echo "the tests failed while recorded (exit $status); the record is read all the same" >&2
  fi
  # The daemon of the tests on a private chain is not Veil: left out.
  step "$here/target/debug/veil-trace-check" --ignore monerod "$record" || return 1
  return "$status"
}

level_leaks() {
  traced_tests leaks --workspace
}

level_leaks_chain() {
  if [ ! -x "$monerod" ]; then
    echo "error: no daemon in $out_dir. Run $setup_hint first." >&2
    return 1
  fi
  export VEIL_MONEROD="$monerod"
  traced_tests leaks-chain -p veil_wallet -p veil_core --features regtest
}

# A test that fails makes cargo end with 101, and the log says which test
# and why. Any other ending means the test program itself was ended, by a
# crash or by the system, and then the log says nothing: the tests run
# side by side and their output is held back until each has finished. So
# the same tests are run once more, one after the other and with their
# output, to see which one it ends in. This is a diagnosis and never turns
# a failed run into a passed one: the caller still reports the first
# ending.
where_it_ended() {
  local status="$1"
  shift
  if [ "$status" = 0 ] || [ "$status" = 101 ]; then
    return 0
  fi
  echo
  echo "The test program did not finish: it ended with $status, where a failed"
  echo "test ends with 101. Running the same tests again one at a time, with"
  echo "their output, to see where. The level has failed either way."
  step cargo_for_target test "$@" -- --test-threads=1 --nocapture
  local again=$?
  echo
  echo "one at a time it ended with $again"
}

level_tor() {
  require_wallet_lib || return 1
  # Lifecycle and fail-closed behaviour, a real bootstrap with a connection
  # to an onion service, then a wallet synchronising with a public node.
  step cargo_for_target test -p veil_tor --locked --lib -- --include-ignored &&
    step cargo_for_target test -p veil_tor --locked --test network -- --include-ignored --nocapture &&
    step cargo_for_target test -p veil_core --locked --test network -- --include-ignored --nocapture
}

level_stagenet() {
  require_wallet_lib || return 1
  # The command-line tool creates a wallet and synchronises it with a public
  # stagenet node through Tor. A new wallet scans about a month of blocks,
  # so this takes a while.
  step cargo_for_target test -p veil_cli --locked --test stagenet -- --include-ignored --nocapture
}

level_bridges() {
  if [ "$windows" = 1 ]; then
    echo "error: the bridges level is not available for Windows yet: there are no" >&2
    echo "transport programs for Windows to test with." >&2
    return 1
  fi
  require_wallet_lib || return 1
  local program
  for program in obfs4proxy snowflake-client; do
    if ! command -v "$program" >/dev/null; then
      echo "error: $program is not installed. Run ./check.sh setup." >&2
      return 1
    fi
  done
  # Tor through obfs4 and through Snowflake, a bridge that does not exist,
  # then the command-line tool synchronising a wallet through bridges. One
  # test at a time: each starts its own Tor client.
  step cargo test -p veil_tor --locked --test bridges -- --include-ignored --nocapture --test-threads=1 &&
    step cargo test -p veil_cli --locked --test bridges -- --include-ignored --nocapture
}

level_daemons() {
  if [ ! -x "$monerod" ]; then
    echo "error: no daemon in $out_dir. Run $setup_hint first." >&2
    return 1
  fi
  export VEIL_MONEROD="$monerod"
  # A diagnosis, not part of an acceptance run: a test daemon once failed to
  # come up and the log could not say why. This starts daemons in pairs,
  # many times, and shows how each one that fails ended.
  step cargo_for_target test -p veil_test_guard --locked --lib -- --ignored --nocapture daemons_started_side_by_side_come_up
}

# The app for Android, as the one file a phone installs. The core, built
# here, and the wallet library, built by setup, go inside it.
level_app() {
  if [ "$android" != 1 ]; then
    echo "error: 'app' goes with --android. On a desktop, 'offline' builds the app." >&2
    return 1
  fi
  android_env || return 1
  require_wallet_lib || return 1
  local flutter="$flutter_dir/flutter/bin/flutter"
  if [ ! -x "$flutter" ]; then
    echo "error: Flutter is not installed in $flutter_dir. Run ./check.sh setup." >&2
    return 1
  fi
  if [ ! -x "$android_sdk/cmdline-tools/latest/bin/sdkmanager" ]; then
    echo "error: the Android SDK is not in $android_sdk. Run $setup_hint." >&2
    return 1
  fi
  # Where the app's build takes libraries from to put them in the package.
  local libs="$here/../app/android/app/src/main/jniLibs/arm64-v8a"
  local apk="$here/../app/build/app/outputs/flutter-apk/app-release.apk"
  local offline=()
  if [ "${CARGO_NET_OFFLINE:-}" = true ]; then offline=(--offline); fi
  # The transport program for bridges: built here if it is not yet, which
  # takes half a minute and the network once, and nothing after that.
  step env ANDROID_NDK_ROOT="$android_ndk" "$here/transports/build.sh" "$transport_dir" || return 1
  step cargo_for_target build -p veil_core --release --locked &&
    mkdir -p "$libs" &&
    step cp "$here/target/aarch64-linux-android/release/libveil_core.so" "$wallet_lib" \
      "$transport_dir/out/android/arm64-v8a/liblyrebird.so" "$libs/" || return 1
  (
    cd "$here/../app" &&
      export ANDROID_HOME="$android_sdk" ANDROID_SDK_ROOT="$android_sdk" &&
      step "$flutter" --no-version-check pub get --enforce-lockfile "${offline[@]}" &&
      step "$flutter" --no-version-check build apk --release --no-pub --target-platform android-arm64 \
        "$(built_from)"
  ) || return 1
  echo
  echo "the app for Android: $apk"
  hand_over_apk "$apk"
}

# Under WSL, puts the app into the Windows Downloads folder as veil.apk,
# from where it can be copied to a phone. Elsewhere it says where the file
# is and leaves it there.
hand_over_apk() {
  local apk="$1" profile downloads
  if can_run_windows_programs && command -v cmd.exe >/dev/null 2>&1 && command -v wslpath >/dev/null 2>&1; then
    profile="$(cd /mnt/c 2>/dev/null && cmd.exe /c 'echo %USERPROFILE%' 2>/dev/null | tr -d '\r')"
    if [ -n "$profile" ]; then
      downloads="$(wslpath -u "$profile")/Downloads"
      if [ -d "$downloads" ] && cp "$apk" "$downloads/veil.apk"; then
        echo "copied to the Windows Downloads folder as veil.apk"
      fi
    fi
  fi
  echo "To install: copy the file to the phone and open it there. Android asks"
  echo "once whether apps may be installed from where it was opened."
}

# Downloads the address `$2` into the file `$1`, over HTTPS only. When that
# fails it names the address: curl alone says how the server answered and
# not what it was asked for, and the same address can answer one machine
# and not another.
fetch() {
  curl --proto '=https' --tlsv1.2 -sSf -o "$1" "$2" && return 0
  echo "error: could not download $2" >&2
  echo "Whether it answers from this machine: curl -sSI '$2' | head -n 1" >&2
  return 1
}

level_ndk() {
  if [ "$android" != 1 ]; then
    echo "error: 'ndk' goes with --android: ./check.sh --android ndk" >&2
    return 1
  fi
  install_android_ndk && step rustup target add aarch64-linux-android
}

# Downloads the pinned Android NDK from its publisher, checks it, and
# unpacks it. About 700 megabytes to download and two gigabytes on disk.
install_android_ndk() {
  if [ -x "$android_ndk/toolchains/llvm/prebuilt/linux-x86_64/bin/clang" ] &&
    [ "$(cat "$android_dir/installed-ndk" 2>/dev/null)" = "$android_ndk_release" ]; then
    echo "Android NDK $android_ndk_release: installed in $android_ndk"
    return 0
  fi
  echo
  echo "--- installing the Android NDK $android_ndk_release into $android_ndk"
  local name="android-ndk-$android_ndk_release-linux.zip"
  local archive="$android_dir/$name" unpacked="$android_dir/unpacked-ndk"
  mkdir -p "$android_sdk/ndk" && rm -rf "$android_ndk" "$unpacked" &&
    fetch "$archive" "https://dl.google.com/android/repository/$name" &&
    echo "$android_ndk_sha256  $archive" | sha256sum -c - &&
    unzip -q "$archive" -d "$unpacked" &&
    mv "$unpacked/android-ndk-$android_ndk_release" "$android_ndk" || return 1
  rm -rf "$archive" "$unpacked"
  echo "$android_ndk_release" >"$android_dir/installed-ndk" &&
    echo "Android NDK $android_ndk_release: installed"
}

# The Android SDK: its command-line tools, checked against their checksum,
# and with them the packages an app's build needs. Installing them means
# accepting Google's licence for the Android SDK, which this does in the
# name of whoever runs it, and says so.
install_android_sdk() {
  local manager="$android_sdk/cmdline-tools/latest/bin/sdkmanager"
  if [ -x "$manager" ] &&
    [ "$(cat "$android_dir/installed-sdk" 2>/dev/null)" = "$android_tools_release ${android_sdk_packages[*]}" ]; then
    echo "Android SDK: installed in $android_sdk"
    return 0
  fi
  if ! command -v java >/dev/null; then
    echo "error: the Android SDK's tools need Java, and there is none." >&2
    return 1
  fi
  echo
  echo "--- installing the Android SDK into $android_sdk"
  local name="commandlinetools-linux-${android_tools_release}_latest.zip"
  local archive="$android_dir/$name" unpacked="$android_dir/unpacked-tools"
  mkdir -p "$android_sdk/cmdline-tools" && rm -rf "$android_sdk/cmdline-tools/latest" "$unpacked" &&
    fetch "$archive" "https://dl.google.com/android/repository/$name" &&
    echo "$android_tools_sha256  $archive" | sha256sum -c - &&
    unzip -q "$archive" -d "$unpacked" &&
    mv "$unpacked/cmdline-tools" "$android_sdk/cmdline-tools/latest" || return 1
  rm -rf "$archive" "$unpacked"
  echo "Accepting the licence of the Android SDK for you. Its text:"
  echo "https://developer.android.com/studio/terms"
  # The tool asks once per licence and reads the answers from here.
  yes 2>/dev/null | "$manager" --sdk_root="$android_sdk" --licenses >/dev/null || true
  "$manager" --sdk_root="$android_sdk" "${android_sdk_packages[@]}" >/dev/null || return 1
  echo "$android_tools_release ${android_sdk_packages[*]}" >"$android_dir/installed-sdk" &&
    echo "Android SDK: installed"
}

level_setup() {
  if ! command -v apt-get >/dev/null; then
    echo "error: setup knows Debian and Ubuntu only (on Windows: run it inside WSL2)." >&2
    return 1
  fi
  local packages="git curl ca-certificates build-essential cmake ninja-build pkg-config
    libssl-dev libzmq3-dev libunbound-dev libsodium-dev libboost-all-dev
    libhidapi-dev libusb-1.0-0-dev libudev-dev libreadline-dev libexpat1-dev
    libpgm-dev libnorm-dev obfs4proxy snowflake-client xz-utils unzip strace
    clang libgtk-3-dev liblzma-dev"
  if [ "$windows" = 1 ]; then
    # The cross compiler, and what Monero's dependency build asks for.
    packages="git curl ca-certificates build-essential cmake ninja-build pkg-config
      g++-mingw-w64-x86-64 nasm autoconf automake libtool python3 gperf bzip2
      xz-utils bc bsdmainutils patch unzip"
  fi
  if [ "$android" = 1 ]; then
    # What Monero's dependency build asks for; `python` by that name,
    # which the NDK that build brings along starts its tools with; and
    # Java, which the tools that build an app for Android are written in.
    packages="git curl ca-certificates build-essential cmake ninja-build pkg-config
      autoconf automake libtool python3 python-is-python3 gperf bzip2
      xz-utils bc bsdmainutils patch unzip openjdk-17-jdk-headless"
  fi
  local package missing=""
  for package in $packages; do
    if ! dpkg-query -W -f='${Status}' "$package" 2>/dev/null | grep -q "install ok installed"; then
      missing+=" $package"
    fi
  done
  # A repeated setup, after the patches changed, needs no package manager
  # and so no password, and cannot collide with the system's own updates.
  if [ -z "$missing" ]; then
    echo "system packages: all installed"
  else
    echo "system packages to install:$missing"
    local sudo=""
    if [ "$(id -u)" != 0 ]; then sudo="sudo"; fi
    # shellcheck disable=SC2086
    step $sudo apt-get update &&
      step $sudo env DEBIAN_FRONTEND=noninteractive apt-get install -y $missing || return 1
  fi
  if ! command -v cargo >/dev/null && [ ! -x "$HOME/.cargo/bin/cargo" ]; then
    echo
    echo "--- installing Rust with rustup"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
      sh -s -- -y --profile minimal --default-toolchain none || return 1
  fi
  export PATH="$HOME/.cargo/bin:$PATH"
  # The first cargo call installs the toolchain pinned in rust-toolchain.toml.
  step cargo --version || return 1
  if [ "$android" = 1 ]; then
    install_android_ndk &&
      install_android_sdk &&
      step rustup target add aarch64-linux-android &&
      step "$here/wallet_lib/build.sh" "$lib_dir" --android &&
      step env ANDROID_NDK_ROOT="$android_ndk" "$here/transports/build.sh" "$transport_dir" &&
      install_flutter
  elif [ "$windows" = 1 ]; then
    step rustup target add x86_64-pc-windows-gnu &&
      step "$here/wallet_lib/build.sh" "$lib_dir" --windows --with-daemon
  else
    step "$here/wallet_lib/build.sh" "$lib_dir" --with-daemon &&
      install_flutter
  fi
}

# The app: its format, analysis and tests, and a build for the system this
# runs on. The tests draw the screens without a display; nothing in them
# opens a wallet or starts Tor.
app_side() {
  if [ "$windows" = 1 ] || [ -z "$(dart_program)" ]; then
    # Said once already, by the Dart side.
    return 0
  fi
  local flutter="$flutter_dir/flutter/bin/flutter" dart system=linux
  dart="$(dart_program)"
  if on_windows; then system=windows; fi
  local offline=()
  if [ "${CARGO_NET_OFFLINE:-}" = true ]; then offline=(--offline); fi
  # Packages are fetched once, by the first command, and exactly the
  # versions in the lock file. The tool is also kept from asking its
  # publisher whether a newer Flutter exists: the version is pinned.
  (
    cd "$here/../app" &&
      step "$flutter" --no-version-check pub get --enforce-lockfile "${offline[@]}" &&
      step "$dart" format --output=none --set-exit-if-changed lib test &&
      step "$flutter" --no-version-check analyze --no-pub --fatal-infos &&
      step "$flutter" --no-version-check test --no-pub &&
      step "$flutter" --no-version-check build "$system" --release --no-pub "$(built_from)"
  ) || return 1
  bundle_native_parts
}

# The two features that are for tests only, a wallet that goes straight to
# a daemon on this machine (`regtest`) and listeners that lead to local
# ports instead of into Tor (`stand-in`), must not be something an app can
# be built with. The crates refuse to compile with them in an optimised
# build; this shows that they still do, right after the core for the app
# was built, when everything they need is compiled already.
no_test_features_in_a_release() {
  local spec
  for spec in "veil_wallet regtest" "veil_tor stand-in" "veil_core regtest"; do
    set -- $spec
    echo
    echo "--- cargo build -p $1 --release --features $2 --locked (has to be refused)"
    local said
    if said="$(cargo build -p "$1" --release --features "$2" --locked 2>&1)"; then
      echo "error: $1 was built for release with its test-only feature '$2'." >&2
      return 1
    fi
    # Refused for this reason, and not because something else is broken.
    if ! grep -q "feature is for tests" <<<"$said"; then
      echo "$said"
      echo "error: that build failed, but not for its test-only feature." >&2
      return 1
    fi
    echo "refused, as it has to be"
  done
}

# Puts the core next to the app that was just built, so that the app can be
# started, and the wallet library too where it has been built.
#
# On Linux only where the wallet library has been built: not in hosted CI.
# On Windows always, see bundle_for_windows.
bundle_native_parts() {
  if on_windows; then
    bundle_for_windows
    return
  fi
  if [ -z "${VEIL_WALLET_LIB:-}" ]; then
    return 0
  fi
  local bundle="$here/../app/build/linux/x64/release/bundle"
  step cargo build -p veil_core --release --locked &&
    no_test_features_in_a_release &&
    step cp "$here/target/release/libveil_core.so" "$VEIL_WALLET_LIB" "$bundle/lib/" &&
    echo "the app with its native parts: $bundle/veil"
}

# The app built for Windows, made complete as far as this machine can: the
# folder Flutter has built, the core as a release build, and the three
# libraries of Microsoft's C++ runtime that the app and the core need.
# Flutter's own instructions for handing an app to somebody say to put
# those three next to it, from the system folder of the machine that built
# it: a Windows without them cannot start the app at all.
#
# Hosted CI keeps this folder with its run, and it is what the owner
# starts. The wallet library is not built in hosted CI; windows-app.sh puts
# it next to the app on the owner's machine. Where it is at hand it goes in
# here too.
bundle_for_windows() {
  local bundle="$here/../app/build/windows/x64/runner/Release"
  local system32 runtime
  system32="$(cygpath -u "${SYSTEMROOT:-C:\\Windows}")/System32"
  step cargo build -p veil_core --release --locked &&
    no_test_features_in_a_release &&
    step cp "$here/target/release/veil_core.dll" "$bundle/" || return 1
  for runtime in msvcp140.dll vcruntime140.dll vcruntime140_1.dll; do
    step cp "$system32/$runtime" "$bundle/" || return 1
  done
  if [ -n "${VEIL_WALLET_LIB:-}" ]; then
    step cp "$VEIL_WALLET_LIB" "$bundle/" || return 1
  fi
  echo "the app for Windows: $bundle"
}

level_flutter() {
  install_flutter
}

# Downloads the pinned Flutter, checks it, and unpacks it. Its tools report
# usage to Google unless told not to; they are told not to.
install_flutter() {
  if [ -x "$flutter_dir/flutter/bin/dart" ] &&
    [ "$(cat "$flutter_dir/installed-version" 2>/dev/null)" = "$flutter_version" ]; then
    echo "Flutter $flutter_version: installed in $flutter_dir"
    return 0
  fi
  echo
  echo "--- installing Flutter $flutter_version into $flutter_dir"
  local base="https://storage.googleapis.com/flutter_infra_release/releases/stable"
  local name="linux/flutter_linux_$flutter_version-stable.tar.xz" sha="$flutter_sha256_linux"
  if on_windows; then
    name="windows/flutter_windows_$flutter_version-stable.zip"
    sha="$flutter_sha256_windows"
  fi
  local archive="$flutter_dir/${name##*/}"
  mkdir -p "$flutter_dir" && rm -rf "$flutter_dir/flutter" &&
    fetch "$archive" "$base/$name" &&
    echo "$sha  $archive" | sha256sum -c - || return 1
  case "$archive" in
    *.zip) unzip -q "$archive" -d "$flutter_dir" ;;
    *) tar -xf "$archive" -C "$flutter_dir" ;;
  esac || return 1
  rm -f "$archive"
  # A checkout owned by another user makes git refuse to read it.
  git config --global --add safe.directory "$flutter_dir/flutter" 2>/dev/null
  "$flutter_dir/flutter/bin/flutter" --disable-analytics >/dev/null &&
    "$flutter_dir/flutter/bin/dart" --disable-analytics >/dev/null &&
    echo "$flutter_version" >"$flutter_dir/installed-version" &&
    echo "Flutter $flutter_version: installed, usage reporting off"
}

run_levels() {
  local level status failed=0 summary=""
  echo "Veil checks: $*"
  echo "date:    $(date -u '+%Y-%m-%d %H:%M:%S UTC')"
  echo "commit:  $(git rev-parse --short=12 HEAD 2>/dev/null || echo unknown)$(git diff --quiet HEAD 2>/dev/null || echo ' (with local changes)')"
  echo "system:  $(uname -sr)"
  if [ "$windows" = 1 ]; then
    echo "target:  Windows (x86_64-pc-windows-gnu), run as Windows programs"
  fi
  if [ "$android" = 1 ]; then
    echo "target:  Android (aarch64-linux-android, API $android_api), built and not run"
  fi
  echo "rustc:   $(rustc --version 2>/dev/null || echo 'not installed')"
  for level in "$@"; do
    echo
    echo "=== $level"
    ("level_${level//-/_}")
    status=$?
    if [ "$status" = 0 ]; then
      summary+="  PASS  $level"$'\n'
    else
      summary+="  FAIL  $level (exit $status)"$'\n'
      failed=1
    fi
  done
  echo
  echo "=== summary"
  printf '%s' "$summary"
  return "$failed"
}

if [ "$#" = 0 ]; then
  usage
  exit 2
fi
for level in "$@"; do
  if [ "$android" = 1 ]; then
    case "$level" in
      offline | ndk | setup | app | -h | --help | help) ;;
      *)
        echo "error: with --android there are 'offline', 'ndk', 'setup' and 'app'. '$level' is a" >&2
        echo "test or belongs to a desktop, and nothing built for Android can be run here." >&2
        exit 2
        ;;
    esac
  fi
  case "$level" in
    offline) ;;
    flutter) ;;
    ndk) ;;
    regtest | tor | stagenet | bridges | daemons | setup | app | leaks | leaks-chain | fuzz)
      if in_hosted_ci; then
        echo "refused: '$level' does not run in hosted CI. Only 'offline' does." >&2
        echo "See docs/development.md, \"Hosted CI and the hosting account\"." >&2
        exit 2
      fi
      ;;
    -h | --help | help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown level '$level'" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [ "$windows" = 1 ] && ! can_run_windows_programs; then
  case " $* " in
    *" offline "* | *" regtest "* | *" tor "* | *" stagenet "* | *" daemons "* | *" fuzz "*)
      echo "error: --windows runs the tests as Windows programs, which works inside" >&2
      echo "WSL on a Windows machine. This is not WSL." >&2
      exit 2
      ;;
  esac
fi

export PATH="$HOME/.cargo/bin:$PATH"
mkdir -p target/check-logs
prefix=""
if [ "$windows" = 1 ]; then prefix="windows-"; fi
if [ "$android" = 1 ]; then prefix="android-"; fi
log="target/check-logs/$(date -u '+%Y%m%d-%H%M%S')-$prefix$(IFS=-; echo "$*").log"
run_levels "$@" 2>&1 | tee "$log"
status="${PIPESTATUS[0]}"
echo
echo "log: $here/$log"
exit "$status"
