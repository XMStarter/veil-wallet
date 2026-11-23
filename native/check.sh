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

level_offline() {
  if [ -z "${VEIL_WALLET_LIB:-}" ] && [ -f "$wallet_lib" ]; then
    export VEIL_WALLET_LIB="$wallet_lib"
  fi
  if [ -n "${VEIL_WALLET_LIB:-}" ]; then
    echo "wallet library: $VEIL_WALLET_LIB"
  else
    echo "wallet library: not built, the tests that need it are skipped"
  fi
  step cargo fmt --check &&
    step cargo clippy --workspace --all-targets --all-features --locked -- -D warnings &&
    step cargo test --workspace --locked
}

level_regtest() {
  if [ ! -f "$wallet_lib" ] || [ ! -x "$monerod" ]; then
    echo "error: no wallet library and daemon in $lib_dir/out. Run ./check.sh setup first." >&2
    return 1
  fi
  export VEIL_WALLET_LIB="$wallet_lib" VEIL_MONEROD="$monerod" VEIL_REQUIRE_WALLET_LIB=1
  step cargo test -p veil_wallet --features regtest --locked
}

level_tor() {
  # Lifecycle and fail-closed behaviour, then a real bootstrap and a
  # connection to an onion service.
  step cargo test -p veil_tor --locked --lib -- --include-ignored &&
    step cargo test -p veil_tor --locked --test network -- --include-ignored --nocapture
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
  local sudo=""
  if [ "$(id -u)" != 0 ]; then sudo="sudo"; fi
  step $sudo apt-get update &&
    step $sudo env DEBIAN_FRONTEND=noninteractive apt-get install -y \
      git curl ca-certificates build-essential cmake ninja-build pkg-config \
      libssl-dev libzmq3-dev libunbound-dev libsodium-dev libboost-all-dev \
      libhidapi-dev libusb-1.0-0-dev libudev-dev libreadline-dev libexpat1-dev \
      libpgm-dev libnorm-dev || return 1
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
