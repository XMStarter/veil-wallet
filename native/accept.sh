#!/usr/bin/env bash
# The acceptance run on the owner's machine, in one command. It does, one
# after the other, and each only when the one before it has passed:
#
#   1. takes the latest commits of the branch that is checked out
#   2. runs the checks as Linux programs          (./check.sh LEVELS)
#   3. runs the same checks as Windows programs   (./check.sh --windows LEVELS)
#   4. starts the app that step 2 has built
#
#   ./accept.sh                       the levels offline and regtest
#   ./accept.sh offline regtest tor   the levels named
#   ./accept.sh --no-app ...          stops after the checks
#   ./accept.sh --no-pull ...         leaves the commits as they are
#
# The levels are those of check.sh. `bridges` has no Windows form and is
# left out of step 3. At the end, and wherever the run stops, it names the
# logs to send.
#
# For the owner's machine, inside WSL. It starts a Monero daemon and, with
# the levels that say so, connects to Tor: never for hosted CI, and never
# for a cloud session. check.sh refuses those levels in hosted CI itself.
set -uo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
cd "$here"

# The whole run is this one function, called on the last line: the shell
# has then read all of the file before it runs any of it, and step 1 may
# replace the file underneath.
main() {
  local pull=1 app=1 pulled=0
  local levels=()
  for word in "$@"; do
    case "$word" in
      --no-pull) pull=0 ;;
      --no-app) app=0 ;;
      # Given by this script to itself, once it has taken new commits.
      --pulled) pulled=1 ;;
      -h | --help | help)
        sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
      -*)
        echo "error: unknown option '$word'" >&2
        exit 2
        ;;
      *) levels+=("$word") ;;
    esac
  done
  if [ "${#levels[@]}" = 0 ]; then levels=(offline regtest); fi

  local logs=()

  if [ "$pull" = 1 ] && [ "$pulled" = 0 ]; then
    step "1 of 4: the latest commits"
    if ! git pull --ff-only; then
      stopped "the latest commits could not be taken; nothing was run"
    fi
    # The commits may have brought a newer version of this script: the
    # rest of the run is that version's.
    local again=(--pulled)
    if [ "$app" = 0 ]; then again+=(--no-app); fi
    exec "$here/accept.sh" "${again[@]}" "${levels[@]}"
  fi
  echo
  echo "commit: $(git log -1 --format='%h %s')"

  step "2 of 4: the checks as Linux programs (${levels[*]})"
  local before
  before="$(newest_log)"
  ./check.sh "${levels[@]}"
  local status=$?
  add_log "$before"
  if [ "$status" != 0 ]; then
    stopped "the checks failed as Linux programs; the app was not started"
  fi

  local for_windows=()
  for level in "${levels[@]}"; do
    if [ "$level" != bridges ]; then for_windows+=("$level"); fi
  done
  if [ "${#for_windows[@]}" = 0 ]; then
    step "3 of 4: nothing to run as Windows programs"
  else
    step "3 of 4: the checks as Windows programs (${for_windows[*]})"
    before="$(newest_log)"
    ./check.sh --windows "${for_windows[@]}"
    status=$?
    add_log "$before"
    if [ "$status" != 0 ]; then
      stopped "the checks failed as Windows programs; the app was not started"
    fi
  fi

  step "the checks passed"
  say_logs

  if [ "$app" = 0 ]; then exit 0; fi
  # The offline level is what builds the app. Without it the app that is
  # there may be an older one, and an older app is worse than none: it
  # shows what was, not what is being accepted.
  case " ${levels[*]} " in
    *" offline "*) ;;
    *)
      echo
      echo "The app is not started: the offline level builds it, and it was not run."
      exit 0
      ;;
  esac
  local built="$here/../app/build/linux/x64/release/bundle/veil"
  if [ ! -x "$built" ]; then
    echo
    echo "The app is not started: it was not built ($built is not there)."
    exit 0
  fi
  step "4 of 4: the app (close its window to end the run)"
  "$built"
}

step() {
  echo
  echo "=================================================================="
  echo "  $1"
  echo "=================================================================="
}

# The log check.sh wrote last.
newest_log() {
  # shellcheck disable=SC2012  # the names are check.sh's own: no spaces
  ls -t "$here"/target/check-logs/*.log 2>/dev/null | head -n 1
}

# Notes the log of the check that has just run, if it wrote one: a check
# that refuses to start writes none, and the newest log is then still the
# one from before, given here.
add_log() {
  local newest
  newest="$(newest_log)"
  if [ -n "$newest" ] && [ "$newest" != "$1" ]; then logs+=("$newest"); fi
}

say_logs() {
  if [ "${#logs[@]}" = 0 ]; then return; fi
  echo
  echo "logs to send:"
  local log
  for log in "${logs[@]}"; do
    if [ -n "$log" ]; then echo "  $log"; fi
  done
}

# Ends the run where it is, saying why and which logs there are.
stopped() {
  step "stopped: $1"
  say_logs
  exit 1
}

main "$@"; exit $?
