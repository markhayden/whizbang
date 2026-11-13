#!/usr/bin/env bash
# scripts/thread-guard.sh: a command that stays under the limit runs to its end and its exit
# status is passed on; one whose process holds more threads than the limit is killed, with
# everything it started, and the guard fails and says why. The limit here is 8 and the
# child that goes over it parks 20 threads: nothing in this test makes more than that, and
# nothing finds out how many a system allows. Nothing sleeps for a fixed time: a parked
# thread waits for an event that never comes, for 30 seconds at most, and the test waits
# for a file or a line to appear.
source "$(dirname "$0")/lib.sh"

guard=scripts/thread-guard.sh
export WHIZBANG_THREAD_POLL=0.2
export tmp

command -v python3 >/dev/null || fail "python3 is needed to make a process with a few threads"

# parked <threads> <file>: a process that parks that many threads, writes the file once
# they all run, and ends after 30 seconds if nothing has killed it, saying so.
cat >"$tmp/parked.py" <<'PY'
import sys, threading
count = int(sys.argv[1])
assert count <= 20, "this test never makes more than 20 threads"
never = threading.Event()
started = threading.Barrier(count + 1)
def park():
    started.wait()
    never.wait(30)
for _ in range(count):
    threading.Thread(target=park, daemon=True).start()
started.wait()
open(sys.argv[2], "w").close()
never.wait(30)
print("the parked process reached its end")
PY

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

# --- Under the limit: the command runs to its end, and its exit status is the guard's.
expect_pass "a command under the limit" "$guard" bash -c 'echo "it ran"'
grep -q -x -F "it ran" "$tmp/out" || { cat "$tmp/out" >&2; fail "the command's output is passed on"; }
status=0
"$guard" bash -c 'exit 7' >"$tmp/out" 2>&1 || status=$?
[ "$status" = 7 ] || { cat "$tmp/out" >&2; fail "the exit status is passed on (got $status)"; }
# What the command reads is what the guard was given.
[ "$(echo "handed on" | "$guard" cat)" = "handed on" ] || fail "the command reads the guard's input"

# A process with a few threads, under a limit above them, is left alone for several counts
# and ends when it is told to.
cat >"$tmp/few.py" <<'PY'
import os, sys, threading, time
done = threading.Event()
for _ in range(4):
    threading.Thread(target=done.wait, args=(30,), daemon=True).start()
open(sys.argv[1], "w").close()
limit = time.monotonic() + 30
while not os.path.exists(sys.argv[2]) and time.monotonic() < limit:
    done.wait(0.05)
sys.exit(5)
PY
status=0
printf 'echo counted >>"%s"\n' "$tmp/counts" >"$tmp/hook.sh"
WHIZBANG_THREAD_LIMIT=8 WHIZBANG_THREAD_TEST_HOOK="bash $tmp/hook.sh" \
  "$guard" python3 "$tmp/few.py" "$tmp/few-up" "$tmp/few-go" >"$tmp/few.out" 2>&1 &
few=$!
wait_for "the process with four threads to start" test -e "$tmp/few-up"
# Counted three times more while it runs, and still there.
before=$(grep -c . "$tmp/counts" 2>/dev/null || true)
counted_again() { [ "$(grep -c . "$tmp/counts")" -ge $((${before:-0} + 3)) ]; }
wait_for "three counts of the process with four threads" counted_again
kill -0 "$few" 2>/dev/null || { cat "$tmp/few.out" >&2; fail "a process under the limit is left running"; }
touch "$tmp/few-go"
wait "$few" || status=$?
[ "$status" = 5 ] || { cat "$tmp/few.out" >&2; fail "a process under the limit ends with its own status (got $status)"; }

# --- Over the limit: 20 threads against a limit of 8. The guard kills the process and
# fails, and says which limit.
status=0
WHIZBANG_THREAD_LIMIT=8 "$guard" python3 "$tmp/parked.py" 20 "$tmp/up" >"$tmp/out" 2>&1 || status=$?
[ -e "$tmp/up" ] || { cat "$tmp/out" >&2; fail "the process parked its threads"; }
[ "$status" = 1 ] || { cat "$tmp/out" >&2; fail "the guard exits 1 when it kills (got $status)"; }
grep -q -E 'thread-guard: process [0-9]+ \(.*\) held 2[0-9] threads, over the limit of 8 in one process' "$tmp/out" ||
  { cat "$tmp/out" >&2; fail "the guard says which process, how many threads and the limit"; }
grep -q -F "killed the command and everything it started" "$tmp/out" || { cat "$tmp/out" >&2; fail "the guard says what it killed"; }
grep -q -F "reached its end" "$tmp/out" && { cat "$tmp/out" >&2; fail "the process was killed before its end"; }

# --- The whole tree goes: the process over the limit is a grandchild, and a process beside
# it, which holds one thread, is killed with it.
cat >"$tmp/family.sh" <<'FAMILY'
python3 "$tmp/few.py" "$tmp/beside-up" "$tmp/never" &
echo $! >"$tmp/beside.pid"
python3 "$tmp/parked.py" 20 "$tmp/deep-up" &
echo $! >"$tmp/deep.pid"
wait
echo "the family reached its end"
FAMILY
status=0
WHIZBANG_THREAD_LIMIT=8 "$guard" bash "$tmp/family.sh" >"$tmp/out" 2>&1 || status=$?
[ "$status" = 1 ] || { cat "$tmp/out" >&2; fail "the guard exits 1 when a grandchild is over the limit (got $status)"; }
grep -q -F "reached its end" "$tmp/out" && { cat "$tmp/out" >&2; fail "the family was killed before its end"; }
for which in beside deep; do
  pid=$(cat "$tmp/$which.pid")
  gone() { ! kill -0 "$pid" 2>/dev/null; }
  wait_for "the process $which the one over the limit to be gone" gone
done

# --- What it refuses before it runs anything.
expect_fail "no command" "usage" "$guard"
expect_fail "a limit that is no number" "the limit must be a whole number" env WHIZBANG_THREAD_LIMIT=many "$guard" true
expect_fail "a limit of none" "the limit must be a whole number" env WHIZBANG_THREAD_LIMIT=0 "$guard" true
expect_fail "a pause that is no number" "must be a number" env WHIZBANG_THREAD_POLL=soon "$guard" true

# --- The recipes that run tests run them through the guard.
grep -q -x -F '    scripts/thread-guard.sh cargo nextest run --workspace' justfile ||
  fail "justfile: the recipe test must run the tests through scripts/thread-guard.sh"
grep -q -x -F '    scripts/thread-guard.sh cargo nextest run "$@"' justfile ||
  fail "justfile: the recipe test-only must run the tests through scripts/thread-guard.sh"
# No recipe and no script runs the tests any other way.
bare=$(grep -n -E 'cargo (nextest run|test)( |$)' justfile scripts/*.sh | grep -v -E 'thread-guard\.sh cargo (nextest run|test) |:[0-9]+: *#' || true)
[ -z "$bare" ] || fail "the tests are run outside scripts/thread-guard.sh: $bare"
grep -q -E '^  scripts/thread-guard\.sh cargo nextest run ' scripts/timing.sh ||
  fail "scripts/timing.sh must run the tests through scripts/thread-guard.sh"
grep -q -F 'thread-guard.sh" "$@"' scripts/heavy.sh ||
  fail "scripts/heavy.sh must run its command through scripts/thread-guard.sh"
# And a heavy command that goes over the limit is killed, and the lock released.
status=0
WHIZBANG_HEAVY_LOCK="$tmp/lock" WHIZBANG_THREAD_LIMIT=8 scripts/heavy.sh over \
  python3 "$tmp/parked.py" 20 "$tmp/heavy-up" >"$tmp/out" 2>&1 || status=$?
[ "$status" = 1 ] || { cat "$tmp/out" >&2; fail "a heavy command over the limit fails (got $status)"; }
grep -q -F "over the limit of 8" "$tmp/out" || { cat "$tmp/out" >&2; fail "the heavy command was killed by the guard"; }
[ ! -e "$tmp/lock" ] || fail "the heavy lock is released after the guard killed its command"

finished
