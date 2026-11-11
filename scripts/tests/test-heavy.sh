#!/usr/bin/env bash
# scripts/heavy.sh: two heavy commands queue, a dead holder's lock is taken over, and the
# command's exit status is passed on. Uses its own lock, so a real heavy run is not disturbed.
source "$(dirname "$0")/lib.sh"

heavy=scripts/heavy.sh
export WHIZBANG_HEAVY_LOCK="$tmp/lock"
export WHIZBANG_HEAVY_POLL=0.1

# Each job logs when it starts and ends. Run one at a time, every start is followed by its
# own end; overlapped, two starts would be adjacent.
job() { echo "start $1" >>"$tmp/log"; sleep 1; echo "end $1" >>"$tmp/log"; }
export -f job
export tmp
"$heavy" first bash -c 'job first' >"$tmp/first.out" 2>&1 &
"$heavy" second bash -c 'job second' >"$tmp/second.out" 2>&1 &
wait
[ "$(wc -l <"$tmp/log" | tr -d ' ')" = "4" ] || { cat "$tmp/log"; fail "both jobs ran to the end"; }
awk 'NR % 2 == 1 { if ($1 != "start") exit 1; who = $2 }
     NR % 2 == 0 { if ($1 != "end" || $2 != who) exit 1 }' "$tmp/log" ||
  { cat "$tmp/log"; fail "the two jobs overlapped"; }
cat "$tmp/first.out" "$tmp/second.out" | grep -q -F 'waiting for' ||
  { cat "$tmp/first.out" "$tmp/second.out"; fail "the job that queued says what it waits for"; }
[ ! -e "$tmp/lock" ] || fail "the lock is released when the command ends"

# The command's exit status is the script's, and a failed command still releases the lock.
status=0
"$heavy" failing bash -c 'exit 7' >"$tmp/out" 2>&1 || status=$?
[ "$status" = "7" ] || fail "the exit status is passed on (got $status)"
[ ! -e "$tmp/lock" ] || fail "the lock is released when the command fails"

# A lock left by a process that no longer exists is taken over.
mkdir "$tmp/lock"
bash -c 'echo $$' >"$tmp/lock/pid"
echo "gone" >"$tmp/lock/label"
expect_pass "a stale lock is taken over" "$heavy" after-stale true
grep -q -F 'stale' "$tmp/out" || { cat "$tmp/out"; fail "taking over a stale lock is reported"; }

expect_fail "no command" "usage" "$heavy" only-a-label
