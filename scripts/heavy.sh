#!/usr/bin/env bash
# Runs one heavy command at a time on this machine (SPEC.md section 13).
#
#   scripts/heavy.sh <label> <command...>
#
# A second invocation, from any worktree or clone, waits until the first has finished, then
# runs. The exit status is the command's.
#
# The lock is a directory, because creating one is atomic and macOS has no flock(1). It
# holds the holder's process id; a lock whose holder no longer exists is taken over.
# WHIZBANG_HEAVY_LOCK moves the lock (the tests use it); WHIZBANG_HEAVY_POLL is the number
# of seconds between attempts.
set -euo pipefail

if [ $# -lt 2 ]; then
  echo "usage: scripts/heavy.sh <label> <command...>" >&2
  exit 2
fi
label=$1
shift

lock=${WHIZBANG_HEAVY_LOCK:-/tmp/whizbang-heavy-$(id -u).lock}
poll=${WHIZBANG_HEAVY_POLL:-2}

said=""
while ! mkdir "$lock" 2>/dev/null; do
  holder=$(cat "$lock/pid" 2>/dev/null || true)
  if [ -n "$holder" ] && ! kill -0 "$holder" 2>/dev/null; then
    # Read again before removing: another waiter may have taken the stale lock already.
    if [ "$(cat "$lock/pid" 2>/dev/null || true)" = "$holder" ]; then
      echo "heavy: [$label] removing a stale lock left by process $holder" >&2
      rm -rf "$lock"
    fi
    continue
  fi
  if [ "$said" != "$holder" ]; then
    echo "heavy: [$label] waiting for [$(cat "$lock/label" 2>/dev/null || echo "?")] (process ${holder:-?})" >&2
    said=$holder
  fi
  sleep "$poll"
done

release() { rm -rf "$lock"; }
trap release EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
echo $$ >"$lock/pid"
echo "$label" >"$lock/label"

echo "heavy: [$label] running" >&2
status=0
"$@" || status=$?
echo "heavy: [$label] finished with status $status" >&2
exit "$status"
