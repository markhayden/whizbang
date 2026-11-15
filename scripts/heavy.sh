#!/usr/bin/env bash
# Runs one heavy command at a time on this machine (SPEC.md section 13).
#
#   scripts/heavy.sh <label> <command...>
#
# A second invocation, from any worktree or clone, waits until the first has finished, then
# runs. The exit status is the command's.
#
# The command runs through scripts/thread-guard.sh, which kills it, and fails, when one of
# its processes holds more threads than a limit: a heavy command is where tests run.
#
# The lock is a directory, because creating one is atomic and macOS has no flock(1). It
# holds the holder's process id and label.
#
# - A lock is stale when its process no longer exists, or when it has had no process id for
#   WHIZBANG_HEAVY_GRACE seconds (default 5) of this waiter watching it: its maker died
#   between creating it and writing the id.
# - A stale lock is taken over by renaming it aside, which only one waiter can do, and then
#   reading the process id in what was moved. If that is a live process (another waiter took
#   the lock over first), the lock is put back and this waiter goes on waiting.
# - A lock is released only by the process it names.
#
# What remains: between renaming a live lock aside and putting it back, a third invocation
# could create the lock. That needs three invocations inside the same few milliseconds after
# a holder was killed.
#
# WHIZBANG_HEAVY_LOCK moves the lock and WHIZBANG_HEAVY_POLL is the seconds between
# attempts; WHIZBANG_HEAVY_TEST_HOOK is a command run between deciding a lock is stale and
# moving it. The tests use all three.
# Parse the whole file first: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "heavy: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail

if [ $# -lt 2 ]; then
  echo "usage: scripts/heavy.sh <label> <command...>" >&2
  exit 2
fi
label=$1
shift

lock=${WHIZBANG_HEAVY_LOCK:-/tmp/whizbang-heavy-$(id -u).lock}
poll=${WHIZBANG_HEAVY_POLL:-2}
grace=${WHIZBANG_HEAVY_GRACE:-5}
aside="$lock.stale.$$"

alive() { [ -n "$1" ] && kill -0 "$1" 2>/dev/null; }

# take_over <pid seen>: move the lock aside and delete it, unless what was moved turns out to
# belong to a live process.
take_over() {
  local seen=$1 moved
  [ -z "${WHIZBANG_HEAVY_TEST_HOOK:-}" ] || $WHIZBANG_HEAVY_TEST_HOOK
  rm -rf "$aside"
  # -T would be clearer but is not in the macOS mv; $aside does not exist, so this renames.
  mv "$lock" "$aside" 2>/dev/null || return 0
  moved=$(cat "$aside/pid" 2>/dev/null || true)
  if alive "$moved"; then
    # Not the lock that was judged stale. Put it back for its holder.
    if mkdir "$lock" 2>/dev/null; then
      cp "$aside/pid" "$lock/pid"
      cp "$aside/label" "$lock/label" 2>/dev/null || true
      echo "heavy: [$label] the lock had been taken by process $moved; put it back" >&2
    else
      echo "heavy: [$label] warning: process $moved lost its lock to another invocation" >&2
    fi
  else
    echo "heavy: [$label] removed a stale lock (process ${seen:-unknown})" >&2
  fi
  rm -rf "$aside"
}

said=""
empty_since=""
while ! mkdir "$lock" 2>/dev/null; do
  holder=$(cat "$lock/pid" 2>/dev/null || true)
  if [ -n "$holder" ]; then
    empty_since=""
    if ! alive "$holder"; then
      take_over "$holder"
      continue
    fi
  elif [ -d "$lock" ]; then
    [ -n "$empty_since" ] || empty_since=$SECONDS
    if [ $((SECONDS - empty_since)) -ge "$grace" ]; then
      empty_since=""
      take_over ""
      continue
    fi
  fi
  if [ "$said" != "${holder:-none}" ]; then
    echo "heavy: [$label] waiting for [$(cat "$lock/label" 2>/dev/null || echo "?")] (process ${holder:-?})" >&2
    said=${holder:-none}
  fi
  sleep "$poll"
done
echo $$ >"$lock/pid"
echo "$label" >"$lock/label"

# Only the process the lock names removes it.
release() {
  if [ "$(cat "$lock/pid" 2>/dev/null || true)" = "$$" ]; then
    rm -rf "$lock"
  fi
}
# An exit of 0 that is not one meant to succeed becomes 70: see the top of scripts/scan.sh.
done_ok=0
trap 'rc=$?; release; [ "$done_ok" = 1 ] || [ "$rc" -ne 0 ] || { echo "heavy: stopped before its end; failing" >&2; rc=70; }; exit "$rc"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

echo "heavy: [$label] running" >&2
status=0
"$(dirname "${BASH_SOURCE[0]}")/thread-guard.sh" "$@" || status=$?
echo "heavy: [$label] finished with status $status" >&2
done_ok=1
exit "$status"
}
main "$@"; exit
