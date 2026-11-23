#!/usr/bin/env bash
# Runs Veil's checks and writes a log of the run.
#
#   ./check.sh offline     format, lint, and the tests that need no network
#   ./check.sh regtest     the wallet on a private chain (starts a Monero daemon)
#   ./check.sh tor         the tests that start the real Tor client
#   ./check.sh stagenet    not available yet
#   ./check.sh setup       once per machine: packages, Rust, the wallet library
#
# Several levels can be given at once: ./check.sh offline regtest tor
#
# `offline` runs anywhere, hosted CI included. Every other level refuses to
# start in hosted CI: see docs/development.md, "Hosted CI and the hosting
# account". `regtest` and `tor` are for the owner's machine.
#
# The wallet library is looked for in $VEIL_WALLET_LIB_DIR, by default
# ~/veil-wallet-lib, which is where `setup` builds it.
#
# The log goes to target/check-logs/. Send that file when asked for the
# result of a run.
set -uo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
cd "$here"

lib_dir="${VEIL_WALLET_LIB_DIR:-$HOME/veil-wallet-lib}"
wallet_lib="$lib_dir/out/libmonero_wallet2_api_c.so"
monerod="$lib_dir/out/monerod"

usage() {
  sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'
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
  built="$(cat "$lib_dir/out/veil-patches.sha256" 2>/dev/null || true)"
  wanted="$(cat wallet_lib/patches/monero/*.patch wallet_lib/patches/monero_c/*.patch | sha256sum | cut -d' ' -f1)"
  if [ "$built" = "$wanted" ]; then echo current; else echo stale; fi
}

# For the levels that cannot do without the library and the daemon.
require_wallet_lib() {
  case "$(wallet_lib_state)" in
    current) ;;
    stale)
      echo "error: the wallet library in $lib_dir/out was built from older patches." >&2
      echo "Run ./check.sh setup again; it rebuilds only what changed." >&2
      return 1
      ;;
    *)
      echo "error: no wallet library in $lib_dir/out. Run ./check.sh setup first." >&2
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
    echo "it are skipped. Run ./check.sh setup to rebuild it."
  else
    echo "wallet library: not built, the tests that need it are skipped"
  fi
  step cargo fmt --check &&
    step cargo clippy --workspace --all-targets --all-features --locked -- -D warnings &&
    step cargo test --workspace --locked
}

level_regtest() {
  require_wallet_lib || return 1
  if [ ! -x "$monerod" ]; then
    echo "error: no daemon in $lib_dir/out. Run ./check.sh setup first." >&2
    return 1
  fi
  export VEIL_MONEROD="$monerod"
  # The wallet alone, then the wallet behind the stand-in for Tor.
  step cargo test -p veil_wallet --features regtest --locked &&
    step cargo test -p veil_core --features regtest --locked
}

level_tor() {
  require_wallet_lib || return 1
  # Lifecycle and fail-closed behaviour, a real bootstrap with a connection
  # to an onion service, then a wallet synchronising with a public node.
  step cargo test -p veil_tor --locked --lib -- --include-ignored &&
    step cargo test -p veil_tor --locked --test network -- --include-ignored --nocapture &&
    step cargo test -p veil_core --locked --test network -- --include-ignored --nocapture
}

level_stagenet() {
  echo "error: nothing to run yet. The stagenet level arrives with the" >&2
  echo "command-line tool, step 1.3 of the build plan in docs/concept.md." >&2
  return 1
}

level_setup() {
  if ! command -v apt-get >/dev/null; then
    echo "error: setup knows Debian and Ubuntu only (on Windows: run it inside WSL2)." >&2
    return 1
  fi
  local packages="git curl ca-certificates build-essential cmake ninja-build pkg-config
    libssl-dev libzmq3-dev libunbound-dev libsodium-dev libboost-all-dev
    libhidapi-dev libusb-1.0-0-dev libudev-dev libreadline-dev libexpat1-dev
    libpgm-dev libnorm-dev"
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
  step cargo --version &&
    step "$here/wallet_lib/build.sh" "$lib_dir" --with-daemon
}

run_levels() {
  local level status failed=0 summary=""
  echo "Veil checks: $*"
  echo "date:    $(date -u '+%Y-%m-%d %H:%M:%S UTC')"
  echo "commit:  $(git rev-parse --short=12 HEAD 2>/dev/null || echo unknown)$(git diff --quiet HEAD 2>/dev/null || echo ' (with local changes)')"
  echo "system:  $(uname -sr)"
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
    regtest | tor | stagenet | setup)
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

export PATH="$HOME/.cargo/bin:$PATH"
mkdir -p target/check-logs
log="target/check-logs/$(date -u '+%Y%m%d-%H%M%S')-$(IFS=-; echo "$*").log"
run_levels "$@" 2>&1 | tee "$log"
status="${PIPESTATUS[0]}"
echo
echo "log: $here/$log"
exit "$status"
