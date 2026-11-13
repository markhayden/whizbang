#!/usr/bin/env bash
# scripts/thread-guard.sh: a command that stays under the limit runs to its end and its exit
# status is passed on; one whose process holds more threads than the limit is killed, with
# everything it started, and the guard fails and says why; so is one whose threads can no
# longer be counted. The limit here is 8 and the
# child that goes over it parks 20 threads: nothing in this test makes more than that, and
# nothing finds out how many a system allows. Nothing sleeps for a fixed time: a parked
# thread waits for an event that never comes, for 30 seconds at most, and the test waits
# for a file or a line to appear.
#
# A count that fails while the command runs is shown with a `ps` of this test's own, first
# on the path, that works until a file appears and then fails in one of four ways.
source "$(dirname "$0")/lib.sh"

guard=scripts/thread-guard.sh
export tmp
# This test may itself run under a guard (`scripts/heavy.sh`): the guards it starts are
# not to take that one for theirs.
unset WHIZBANG_THREAD_GUARD WHIZBANG_THREAD_LIMIT WHIZBANG_THREAD_POLL WHIZBANG_THREAD_HEADROOM WHIZBANG_THREAD_TEST_HOOK

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
with open(sys.argv[1] + ".new", "w") as up:
    up.write(str(os.getpid()))
os.rename(sys.argv[1] + ".new", sys.argv[1])
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
# fails, and says which limit. The pause between two counts is the script's own: were it
# minutes, the process would reach its end, 30 seconds on.
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

# --- What it is set to when nothing sets it: 300 threads, with 0.2 seconds between two counts. The
# command is told (that is how a guard inside a guard knows it is watched).
told=$("$guard" bash -c 'echo "$WHIZBANG_THREAD_GUARD"')
[ "${told#* }" = "300 0.2" ] || fail "the limit is 300 and the pause 0.2 seconds unless set (the command was told: $told)"
grep -q -x -F 'limit=${WHIZBANG_THREAD_LIMIT:-300}' "$guard" || fail "the limit is 300 threads unless set"
grep -q -x -F 'poll=${WHIZBANG_THREAD_POLL:-0.2}' "$guard" || fail "the pause is 0.2 seconds unless set"

# --- A guard inside a guard with the same limit and pause watches nothing: the counts of
# the two are those of one process. With another limit it watches as well.
printf 'echo $PPID >>"%s"\n' "$tmp/watchers" >"$tmp/who.sh"
nested() { # <limit of the inner guard>: how many guards counted the process with four threads
  rm -f "$tmp/watchers" "$tmp/nested-up" "$tmp/nested-go"
  WHIZBANG_THREAD_LIMIT=40 WHIZBANG_THREAD_TEST_HOOK="bash $tmp/who.sh" "$guard" \
    env WHIZBANG_THREAD_LIMIT="$1" "$guard" python3 "$tmp/few.py" "$tmp/nested-up" "$tmp/nested-go" >"$tmp/nested.out" 2>&1 &
  local both=$!
  wait_for "the process under two guards to start" test -e "$tmp/nested-up"
  enough() { [ "$(grep -c . "$tmp/watchers")" -ge 6 ]; }
  wait_for "six counts of the process under two guards" enough
  touch "$tmp/nested-go"
  wait "$both" || true
  sort -u "$tmp/watchers" | grep -c .
}
[ "$(nested 40)" = 1 ] || { cat "$tmp/nested.out" >&2; fail "a guard inside a guard with its limit and pause does not watch"; }
[ "$(nested 30)" = 2 ] || { cat "$tmp/nested.out" >&2; fail "a guard inside a guard with another limit watches as well"; }

# --- A guard takes itself for watched only by a guard above it. The variable alone does
# not switch it off: set by hand to a live process that is beside the guard, or to one
# above it that is no guard (this test), with this guard's own limit and pause, the guard
# still watches, and kills the 20 threads over its limit of 8.
rm -f "$tmp/beside-up" "$tmp/beside-go"
python3 "$tmp/few.py" "$tmp/beside-up" "$tmp/beside-go" >/dev/null 2>&1 &
beside=$!
wait_for "the process beside the guard to start" test -e "$tmp/beside-up"
for named in "$beside" "$$"; do
  rm -f "$tmp/forged-up"
  status=0
  WHIZBANG_THREAD_GUARD="$named 8 0.2" WHIZBANG_THREAD_LIMIT=8 \
    "$guard" python3 "$tmp/parked.py" 20 "$tmp/forged-up" >"$tmp/out" 2>&1 || status=$?
  [ "$status" = 1 ] || { touch "$tmp/beside-go"; cat "$tmp/out" >&2; fail "a guard told by hand that process $named watches it still watches (got $status)"; }
  grep -q -F "over the limit of 8 in one process" "$tmp/out" || { touch "$tmp/beside-go"; cat "$tmp/out" >&2; fail "a guard told by hand that it is watched says what it killed"; }
done
touch "$tmp/beside-go"
wait "$beside" || true

# --- On Linux the command's user may hold only so many processes and threads more. macOS
# has no such limit, and the command's is what the guard's was.
outside=$(ulimit -S -u)
inside=$(WHIZBANG_THREAD_HEADROOM=2000 "$guard" bash -c 'ulimit -S -u')
if [ "$(uname -s)" = "Linux" ]; then
  echo "$inside" | grep -q -E '^[0-9]+$' || fail "on Linux the command's user is limited (got $inside)"
  [ "$inside" -ge 2000 ] && [ "$inside" -lt 200000 ] || fail "on Linux the limit is what the user holds and 2000 more (got $inside)"
  if [ "$outside" != unlimited ] && [ "$outside" -lt 2000 ]; then
    echo "skip: lowering the user's limit; it is already $outside"
  else
    [ "$inside" != "$outside" ] || fail "on Linux the guard lowers the limit (it stayed $outside)"
  fi
else
  [ "$inside" = "$outside" ] || fail "outside Linux the limit is left as it was ($outside, and inside $inside)"
fi
# A count of what the user holds that fails, or comes to nothing, lowers nothing: a limit of
# the headroom alone would be under what the user already holds. Shown with a `ps` of this
# test's that fails when asked for the user's threads and is the real one otherwise.
mkdir "$tmp/nolwp"
cat >"$tmp/nolwp/ps" <<FAKE
#!/bin/sh
for word in "\$@"; do [ "\$word" != "-L" ] || exit 1; done
exec "$(command -v ps)" "\$@"
FAKE
chmod +x "$tmp/nolwp/ps"
blind=$(PATH="$tmp/nolwp:$PATH" WHIZBANG_THREAD_HEADROOM=2000 "$guard" bash -c 'ulimit -S -u')
[ "$blind" = "$outside" ] || fail "a user whose threads cannot be counted keeps the limit it had ($outside, and inside $blind)"
expect_fail "headroom that is no number" "the headroom must be a whole number" env WHIZBANG_THREAD_HEADROOM=lots "$guard" true

# --- A count that fails. A `ps` of this test's, first on the path: the real one, until the
# file $tmp/fault says how to fail.
real_ps=$(command -v ps)
mkdir "$tmp/bin"
cat >"$tmp/bin/ps" <<FAKE
#!/bin/sh
fault=\$(cat "$tmp/fault" 2>/dev/null)
case "\$fault" in
  fails) exit 1 ;;
  header) echo "USER PID TT %CPU STAT PRI STIME UTIME COMMAND"; exit 0 ;;
  partial) "$real_ps" "\$@" | sed -n 1,3p; exit 0 ;;
  nothreads) "$real_ps" "\$@" | awk '{ \$3 = 0; print }'; exit 0 ;;
esac
exec "$real_ps" "\$@"
FAKE
chmod +x "$tmp/bin/ps"
with_fake_ps() { PATH="$tmp/bin:$PATH" "$@"; }

# From the start: the guard cannot count its own threads, says so, and runs nothing.
echo fails >"$tmp/fault"
status=0
with_fake_ps "$guard" touch "$tmp/ran-blind" >"$tmp/out" 2>&1 || status=$?
[ "$status" = 2 ] || { cat "$tmp/out" >&2; fail "a guard that cannot count exits 2 (got $status)"; }
grep -q -F "cannot count the threads of a process on this system" "$tmp/out" || { cat "$tmp/out" >&2; fail "a guard that cannot count says so"; }
[ ! -e "$tmp/ran-blind" ] || fail "a guard that cannot count does not run the command"
echo header >"$tmp/fault"
expect_fail "a ps that prints nothing of the guard" "cannot count the threads" with_fake_ps "$guard" touch "$tmp/ran-blind"
[ ! -e "$tmp/ran-blind" ] || fail "a guard that is not in its own count does not run the command"
rm -f "$tmp/fault"

# While the command runs: the count was good, and then is not. The command is a process
# with a process under it, and neither would end for 30 seconds. The guard kills both,
# fails and says why; it never passes, and it never leaves them running.
cat >"$tmp/pair.sh" <<'PAIR'
python3 "$tmp/few.py" "$tmp/pair-up" "$tmp/never" &
wait
echo "the pair reached its end"
PAIR
faults="fails header partial"
if [ "$(uname -s)" = "Linux" ]; then
  faults="$faults nothreads"
else
  echo "skip: a count in which a process has no thread; this system's ps has a line for each thread and no count"
fi
for fault in $faults; do
  rm -f "$tmp/fault" "$tmp/pair-up" "$tmp/counts"
  echo $$ >"$tmp/guard-of-$fault"
  WHIZBANG_THREAD_TEST_HOOK="bash $tmp/hook.sh" with_fake_ps "$guard" bash "$tmp/pair.sh" >"$tmp/out" 2>&1 &
  blind=$!
  wait_for "the pair to start ($fault)" test -e "$tmp/pair-up"
  counted_once() { [ "$(grep -c . "$tmp/counts" 2>/dev/null)" -ge 1 ]; }
  wait_for "one good count of the pair ($fault)" counted_once
  under=$(cat "$tmp/pair-up")
  echo "$fault" >"$tmp/fault"
  status=0
  wait "$blind" || status=$?
  rm -f "$tmp/fault"
  [ "$status" = 1 ] || { cat "$tmp/out" >&2; fail "a count that fails while the command runs ($fault) fails the guard (got $status)"; }
  grep -q -F "could not be counted, twice running" "$tmp/out" || { cat "$tmp/out" >&2; fail "the guard says the count failed ($fault)"; }
  grep -q -F "reached its end" "$tmp/out" && { cat "$tmp/out" >&2; fail "the pair was killed before its end ($fault)"; }
  pid=$under
  gone() { ! kill -0 "$pid" 2>/dev/null; }
  wait_for "the process under the command to be gone ($fault)" gone
done

# A count that fails once and is good again is not a failure: the command goes on to its
# end. The fault is there for one call of ps only.
cat >"$tmp/bin/ps" <<FAKE
#!/bin/sh
if mv "$tmp/fault" "$tmp/fault-taken" 2>/dev/null; then exit 1; fi
exec "$real_ps" "\$@"
FAKE
rm -f "$tmp/few-up" "$tmp/few-go" "$tmp/counts" "$tmp/fault-taken"
WHIZBANG_THREAD_TEST_HOOK="bash $tmp/hook.sh" with_fake_ps "$guard" python3 "$tmp/few.py" "$tmp/few-up" "$tmp/few-go" >"$tmp/out" 2>&1 &
once=$!
wait_for "the process to start (a count that fails once)" test -e "$tmp/few-up"
echo fails >"$tmp/fault"
wait_for "the failing count to be made" test -e "$tmp/fault-taken"
before=$(grep -c . "$tmp/counts" 2>/dev/null || true)
counted_again() { [ "$(grep -c . "$tmp/counts" 2>/dev/null)" -ge $((${before:-0} + 2)) ]; }
wait_for "two counts after the one that failed" counted_again
touch "$tmp/few-go"
status=0
wait "$once" || status=$?
[ "$status" = 5 ] || { cat "$tmp/out" >&2; fail "a count that fails once does not end the command (got $status)"; }

# --- The recipes that run tests run them through the guard.
grep -q -x -F '    scripts/thread-guard.sh cargo nextest run --workspace' justfile ||
  fail "justfile: the recipe test must run the tests through scripts/thread-guard.sh"
grep -q -x -F '    scripts/thread-guard.sh cargo nextest run "$@"' justfile ||
  fail "justfile: the recipe test-only must run the tests through scripts/thread-guard.sh"
# No recipe and no script, and no test of a script, runs the tests any other way: every
# line that has `cargo`, white space, and `nextest run` or `test` has the guard before it.
# Two files only talk of such a command: this one, and the test of the skills, which holds
# a sentence of a document.
found=$(find justfile scripts -type f ! -path scripts/tests/test-thread-guard.sh ! -path scripts/tests/test-skills.sh \
  -exec grep -n -H -E 'cargo[[:space:]]+(nextest[[:space:]]+run|test)([[:space:]]|$)' {} + || true)
bare=$(printf '%s\n' "$found" | grep -v -E 'thread-guard\.sh[[:space:]]+cargo[[:space:]]+(nextest[[:space:]]+run|test)[[:space:]]|^[^:]*:[0-9]+:[[:space:]]*#|^$' || true)
[ -z "$bare" ] || fail "the tests are run outside scripts/thread-guard.sh: $bare"
[ "$(printf '%s\n' "$found" | grep -c .)" -ge 5 ] || fail "the search for the commands that run tests finds the five there are (found: $found)"
grep -q -F 'scripts/thread-guard.sh cargo nextest run --manifest-path' scripts/tests/test-timeout.sh ||
  fail "scripts/tests/test-timeout.sh must run its fixture's tests through scripts/thread-guard.sh"
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
