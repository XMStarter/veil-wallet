#!/usr/bin/env bash
# Runs Veil's checks and writes a log of the run.
#
#   ./check.sh offline     format, lint, and the tests that need no network
#   ./check.sh regtest     the wallet on a private chain (starts a Monero daemon)
#   ./check.sh tor         the tests that start the real Tor client
#   ./check.sh stagenet    the command-line tool on stagenet, through Tor
#   ./check.sh bridges     Tor through obfs4 and Snowflake bridges
#   ./check.sh daemons     not a check: starts test daemons in pairs to see why one may not come up
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
if [ "${1:-}" = "--windows" ]; then
  windows=1
  shift
fi

lib_dir="${VEIL_WALLET_LIB_DIR:-$HOME/veil-wallet-lib}"
if [ "$windows" = 1 ]; then
  out_dir="$lib_dir/out/windows"
  wallet_lib="$out_dir/libmonero_wallet2_api_c.dll"
  monerod="$out_dir/monerod.exe"
  cargo_target=(--target x86_64-pc-windows-gnu)
  setup_hint="./check.sh --windows setup"
  # WSL hands a Windows program only the variables named here; /p turns a
  # Linux path into the Windows one.
  export WSLENV="${WSLENV:+$WSLENV:}VEIL_WALLET_LIB/p:VEIL_MONEROD/p:VEIL_REQUIRE_WALLET_LIB:VEIL_DAEMON_ROUNDS:VEIL_TEST_NODES:VEIL_TEST_ONION:CI:GITHUB_ACTIONS"
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
flutter_sha256="25ed8c3c29d51a7353f2a944310b478af8214920639524b19c50a19e6c044c5a"
flutter_dir="${VEIL_FLUTTER_DIR:-$HOME/veil-flutter}"

# Prints the path of the Dart to use, or nothing.
dart_program() {
  if [ -x "$flutter_dir/flutter/bin/dart" ]; then
    echo "$flutter_dir/flutter/bin/dart"
  fi
}

usage() {
  sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//'
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
  wanted="$(cat wallet_lib/patches/monero/*.patch wallet_lib/patches/monero_c/*.patch | sha256sum | cut -d' ' -f1)"
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

level_offline() {
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
    dart_side
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

level_regtest() {
  require_wallet_lib || return 1
  if [ ! -x "$monerod" ]; then
    echo "error: no daemon in $out_dir. Run $setup_hint first." >&2
    return 1
  fi
  export VEIL_MONEROD="$monerod"
  # The wallet alone, then the wallet behind the stand-in for Tor.
  step cargo_for_target test -p veil_wallet --features regtest --locked &&
    step cargo_for_target test -p veil_core --features regtest --locked
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

level_setup() {
  if ! command -v apt-get >/dev/null; then
    echo "error: setup knows Debian and Ubuntu only (on Windows: run it inside WSL2)." >&2
    return 1
  fi
  local packages="git curl ca-certificates build-essential cmake ninja-build pkg-config
    libssl-dev libzmq3-dev libunbound-dev libsodium-dev libboost-all-dev
    libhidapi-dev libusb-1.0-0-dev libudev-dev libreadline-dev libexpat1-dev
    libpgm-dev libnorm-dev obfs4proxy snowflake-client xz-utils unzip"
  if [ "$windows" = 1 ]; then
    # The cross compiler, and what Monero's dependency build asks for.
    packages="git curl ca-certificates build-essential cmake ninja-build pkg-config
      g++-mingw-w64-x86-64 nasm autoconf automake libtool python3 gperf bzip2
      xz-utils bc bsdmainutils patch unzip"
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
  if [ "$windows" = 1 ]; then
    step rustup target add x86_64-pc-windows-gnu &&
      step "$here/wallet_lib/build.sh" "$lib_dir" --windows --with-daemon
  else
    step "$here/wallet_lib/build.sh" "$lib_dir" --with-daemon &&
      install_flutter
  fi
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
  local archive="$flutter_dir/flutter.tar.xz"
  mkdir -p "$flutter_dir" && rm -rf "$flutter_dir/flutter" &&
    curl --proto '=https' --tlsv1.2 -sSf -o "$archive" \
      "https://storage.googleapis.com/flutter_infra_release/releases/stable/linux/flutter_linux_$flutter_version-stable.tar.xz" &&
    echo "$flutter_sha256  $archive" | sha256sum -c - &&
    tar -xf "$archive" -C "$flutter_dir" &&
    rm -f "$archive" &&
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
  echo "rustc:   $(rustc --version 2>/dev/null || echo 'not installed')"
  for level in "$@"; do
    echo
    echo "=== $level"
    ("level_$level")
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
  case "$level" in
    offline) ;;
    regtest | tor | stagenet | bridges | daemons | setup)
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
    *" offline "* | *" regtest "* | *" tor "* | *" stagenet "* | *" daemons "*)
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
log="target/check-logs/$(date -u '+%Y%m%d-%H%M%S')-$prefix$(IFS=-; echo "$*").log"
run_levels "$@" 2>&1 | tee "$log"
status="${PIPESTATUS[0]}"
echo
echo "log: $here/$log"
exit "$status"
