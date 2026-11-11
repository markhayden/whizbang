#!/usr/bin/env bash
# scripts/heavy.sh: two heavy commands queue, the exit status is passed on, a lock is only
# released by its holder, and a lock whose holder is gone is taken over. Uses its own lock,
# so a real heavy run is not disturbed. Nothing here sleeps for a fixed time: each step waits
# for a file or a line to appear.
source "$(dirname "$0")/lib.sh"

heavy=scripts/heavy.sh
export WHIZBANG_HEAVY_LOCK="$tmp/lock"
export WHIZBANG_HEAVY_POLL=0.05

# wait_for <what> <command...>: poll until the command succeeds, for at most 20 seconds.
wait_for() {
  local what=$1 i=0
  shift
  until "$@" 2>/dev/null; do
    i=$((i + 1))
    [ "$i" -lt 400 ] || fail "timed out waiting for: $what"
    sleep 0.05
  done
}
# A process id that no longer exists.
dead_pid() { bash -c 'echo $$'; }

# --- Two commands queue. The first holds the lock until the test lets it go; the second is
# started while the first runs, and must not start until the first has ended.
export tmp
"$heavy" first bash -c 'echo "start first" >>"$tmp/log"; until [ -e "$tmp/go" ]; do sleep 0.05; done; echo "end first" >>"$tmp/log"' \
  >"$tmp/first.out" 2>&1 &
wait_for "the first job to start" grep -q -x 'start first' "$tmp/log"
"$heavy" second bash -c 'echo "start second" >>"$tmp/log"; echo "end second" >>"$tmp/log"' \
  >"$tmp/second.out" 2>&1 &
wait_for "the second job to queue" grep -q -F 'waiting for [first]' "$tmp/second.out"
[ "$(cat "$tmp/log")" = "start first" ] || { cat "$tmp/log"; fail "the second job started while the first held the lock"; }
touch "$tmp/go"
wait
[ "$(tr '\n' ',' <"$tmp/log")" = "start first,end first,start second,end second," ] ||
  { cat "$tmp/log"; fail "the two jobs ran one after the other"; }
[ ! -e "$tmp/lock" ] || fail "the lock is released when the command ends"

# --- The command's exit status is the script's, and a failed command releases the lock.
status=0
"$heavy" failing bash -c 'exit 7' >"$tmp/out" 2>&1 || status=$?
[ "$status" = "7" ] || fail "the exit status is passed on (got $status)"
[ ! -e "$tmp/lock" ] || fail "the lock is released when the command fails"

# --- A lock is released only by its holder. Here the lock changes hands while the command
# runs (as it would if another process had wrongly taken it over); the lock must be left.
expect_pass "a command whose lock changed hands" \
  "$heavy" robbed bash -c 'echo 1 >"$WHIZBANG_HEAVY_LOCK/pid"'
[ "$(cat "$tmp/lock/pid" 2>/dev/null)" = "1" ] || fail "a lock that names another process is not removed"
rm -rf "$tmp/lock"

# --- A lock left by a process that no longer exists is taken over.
mkdir "$tmp/lock"
dead_pid >"$tmp/lock/pid"
echo "gone" >"$tmp/lock/label"
expect_pass "a stale lock is taken over" "$heavy" after-stale true
grep -q -F 'stale lock' "$tmp/out" || { cat "$tmp/out"; fail "taking over a stale lock is reported"; }
[ ! -e "$tmp/lock" ] || fail "the lock is released after a takeover"
ls "$tmp" | grep -q 'lock\.stale' && fail "the moved stale lock is deleted"

# --- A lock directory with no process id (its maker died before writing one) is stale once
# it has been seen that way for the grace period, and not before.
mkdir "$tmp/lock"
WHIZBANG_HEAVY_GRACE=3600 "$heavy" patient bash -c 'echo ran >"$tmp/patient"' >"$tmp/patient.out" 2>&1 &
wait_for "the waiter to report the lock" grep -q -F 'waiting for' "$tmp/patient.out"
[ -d "$tmp/lock" ] && [ ! -e "$tmp/patient" ] || fail "a lock with no process id is left alone within the grace period"
rmdir "$tmp/lock"
wait
[ -e "$tmp/patient" ] || fail "the waiter runs once the lock is gone"

mkdir "$tmp/lock"
expect_pass "a lock with no process id, past the grace period" env WHIZBANG_HEAVY_GRACE=0 "$heavy" after-empty true
grep -q -F 'stale lock' "$tmp/out" || { cat "$tmp/out"; fail "taking over an empty lock is reported"; }
[ ! -e "$tmp/lock" ] || fail "the lock is released after taking over an empty one"

# --- The takeover checks what it moved. A waiter that decided the lock was stale, and then
# moved a lock that a live process had taken in the meantime, puts that lock back and waits.
# The hook runs between the decision and the move, which is where the race is.
mkdir "$tmp/lock"
dead_pid >"$tmp/lock/pid"
echo "gone" >"$tmp/lock/label"
sleep 600 &
live=$!
cat >"$tmp/hook.sh" <<HOOK
[ -e "$tmp/hooked" ] && exit 0
touch "$tmp/hooked"
echo $live >"$tmp/lock/pid"
echo "the live one" >"$tmp/lock/label"
HOOK
WHIZBANG_HEAVY_TEST_HOOK="bash $tmp/hook.sh" "$heavy" racer bash -c 'echo ran >"$tmp/racer"' >"$tmp/racer.out" 2>&1 &
wait_for "the racer to put the live lock back" grep -q -F 'put it back' "$tmp/racer.out"
wait_for "the racer to queue behind the live holder" grep -q -F 'waiting for [the live one]' "$tmp/racer.out"
[ "$(cat "$tmp/lock/pid")" = "$live" ] || fail "the live holder's lock is back in place"
[ ! -e "$tmp/racer" ] || fail "the racer did not run while a live process held the lock"
kill "$live"
wait "$live" 2>/dev/null || true
wait
[ -e "$tmp/racer" ] || fail "the racer runs once the live holder is gone"
[ ! -e "$tmp/lock" ] || fail "the lock is released at the end"

expect_fail "no command" "usage" "$heavy" only-a-label
