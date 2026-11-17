#!/usr/bin/env bash
# scripts/memory-watch.sh: a command whose processes stay under the limit of resident memory
# runs to its end, its exit status is passed on, and the watch reports the peaks it saw; a
# command that grows over the limit is killed, with everything it started, and the watch
# fails and says why; the limit is 24 GiB unless a variable lowers it; a process the command
# started that outlives it is counted in the report. The stand-in that grows holds 16 MiB
# more each step, up to 256 MiB, against a limit of 64 MiB: nothing here holds more than
# that. Nothing sleeps for a fixed time: each stand-in waits for a file, for 30 seconds at
# most, and the test waits for a file or for a process to be gone.
source "$(dirname "$0")/lib.sh"

watch=scripts/memory-watch.sh
unset WHIZBANG_MEMORY_LIMIT_KIB WHIZBANG_MEMORY_POLL WHIZBANG_MEMORY_REPORT WHIZBANG_MEMORY_TEST_HOOK

command -v python3 >/dev/null || fail "python3 is needed to make a process that grows"

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

# report_value <file> <name>: the value of `name=value` in a report.
report_value() { sed -n "s/^$2=//p" "$1"; }

# --- Under the limit: the command runs to its end, its status is passed on, and the
# report names the default limit, 24 GiB, and peaks above nothing.
expect_pass "a command under the limit" env WHIZBANG_MEMORY_REPORT="$tmp/small.report" "$watch" bash -c 'echo "it ran"'
grep -q -x -F "it ran" "$tmp/out" || { cat "$tmp/out" >&2; fail "the command's output is passed on"; }
[ "$(report_value "$tmp/small.report" limit_kib)" = 25165824 ] ||
  { cat "$tmp/small.report" >&2; fail "the default limit is 24 GiB"; }
grep -q -F "memory-watch: peak" "$tmp/out" || { cat "$tmp/out" >&2; fail "the watch says its peaks when the command ends"; }
status=0
"$watch" bash -c 'exit 7' >"$tmp/out" 2>&1 || status=$?
[ "$status" = 7 ] || { cat "$tmp/out" >&2; fail "the exit status is passed on (got $status)"; }
[ "$(echo "handed on" | "$watch" cat 2>/dev/null)" = "handed on" ] || fail "the command reads the watch's input"

# A command that holds some memory for several counts is seen: its peak is above 1 MiB, it
# is one process, and it has at least one thread.
cat >"$tmp/hold.py" <<'PY'
import os, sys, time
held = bytearray(32 * 1024 * 1024)
for at in range(0, len(held), 4096):
    held[at] = 1
open(sys.argv[1], "w").close()
limit = time.monotonic() + 30
while not os.path.exists(sys.argv[2]) and time.monotonic() < limit:
    time.sleep(0.05)
PY
printf 'echo counted >>"%s"\n' "$tmp/counts" >"$tmp/hook.sh"
WHIZBANG_MEMORY_TEST_HOOK="bash $tmp/hook.sh" WHIZBANG_MEMORY_REPORT="$tmp/hold.report" "$watch" python3 "$tmp/hold.py" "$tmp/hold-up" "$tmp/hold-go" >"$tmp/hold.out" 2>&1 &
holding=$!
wait_for "the holding process to start" test -e "$tmp/hold-up"
# Three counts after it holds its memory, then it may end.
seen=$(grep -c . "$tmp/counts" 2>/dev/null || true)
counted_again() { [ "$(grep -c . "$tmp/counts")" -ge $((${seen:-0} + 3)) ]; }
wait_for "three counts of the holding process" counted_again
touch "$tmp/hold-go"
wait "$holding" || { cat "$tmp/hold.out" >&2; fail "a process under the limit is left to end"; }
peak=$(report_value "$tmp/hold.report" peak_resident_kib)
[ "${peak:-0}" -gt 1024 ] || { cat "$tmp/hold.report" >&2; fail "the peak of a process holding 32 MiB is seen (got $peak)"; }
[ "$(report_value "$tmp/hold.report" most_processes)" = 1 ] || { cat "$tmp/hold.report" >&2; fail "one process is counted as one"; }
[ "$(report_value "$tmp/hold.report" others_seen)" = 0 ] || { cat "$tmp/hold.report" >&2; fail "no other process was seen"; }
[ "$(report_value "$tmp/hold.report" most_threads)" -ge 1 ] || { cat "$tmp/hold.report" >&2; fail "the threads are counted"; }

# --- Over the limit: a stand-in that grows, under a limit of 64 MiB, with a process beside
# it that holds little. The watch kills both and fails, and says why.
cat >"$tmp/grow.py" <<'PY'
import os, sys, time
held = []
for step in range(16):
    block = bytearray(16 * 1024 * 1024)
    for at in range(0, len(block), 4096):
        block[at] = 1
    held.append(block)
    time.sleep(0.05)
limit = time.monotonic() + 30
while time.monotonic() < limit:
    time.sleep(0.05)
print("the growing process reached its end")
PY
cat >"$tmp/beside.py" <<'PY'
import sys, time
open(sys.argv[1], "w").close()
limit = time.monotonic() + 30
while time.monotonic() < limit:
    time.sleep(0.05)
print("the process beside reached its end")
PY
status=0
WHIZBANG_MEMORY_LIMIT_KIB=65536 "$watch" bash -c '
  python3 "$1/beside.py" "$1/beside-up" & echo $! >"$1/beside.pid"
  python3 "$1/grow.py"
  wait' _ "$tmp" >"$tmp/out" 2>&1 || status=$?
[ "$status" = 1 ] || { cat "$tmp/out" >&2; fail "the watch exits 1 when it kills (got $status)"; }
grep -q -F "over the limit of 65536 KiB" "$tmp/out" || { cat "$tmp/out" >&2; fail "the watch says why it killed"; }
grep -q -F "killed the command and everything it started" "$tmp/out" || { cat "$tmp/out" >&2; fail "the watch says what it killed"; }
grep -q -F "reached its end" "$tmp/out" && { cat "$tmp/out" >&2; fail "the family was killed before its end"; }
beside=$(cat "$tmp/beside.pid")
gone() { ! kill -0 "$beside" 2>/dev/null; }
wait_for "the process beside the one that grew to be gone" gone

# --- A process the command started that outlives it is counted, and left to end itself.
cat >"$tmp/after.py" <<'PY'
import os, sys, time
open(sys.argv[1], "w").close()
limit = time.monotonic() + 30
while not os.path.exists(sys.argv[2]) and time.monotonic() < limit:
    time.sleep(0.05)
PY
# The command ends once the watch has counted twice with the process there.
: >"$tmp/counts"
WHIZBANG_MEMORY_TEST_HOOK="bash $tmp/hook.sh" WHIZBANG_MEMORY_REPORT="$tmp/after.report" "$watch" bash -c '
  python3 "$1/after.py" "$1/after-up" "$1/after-go" >/dev/null 2>&1 & echo $! >"$1/after.pid"
  i=0
  until [ -e "$1/after-up" ] && [ "$(grep -c . "$1/counts")" -ge 2 ]; do
    i=$((i + 1)); [ "$i" -lt 400 ] || exit 3
    sleep 0.05
  done' _ "$tmp" >"$tmp/out" 2>&1 || { touch "$tmp/after-go"; cat "$tmp/out" >&2; fail "a command that leaves a process behind still passes"; }
touch "$tmp/after-go"
[ "$(report_value "$tmp/after.report" others_seen)" -ge 1 ] || { cat "$tmp/after.report" >&2; fail "the process the command started was seen"; }
[ "$(report_value "$tmp/after.report" still_running)" = 1 ] || { cat "$tmp/after.report" >&2; fail "the process that outlived the command is counted"; }
after=$(cat "$tmp/after.pid")
gone() { ! kill -0 "$after" 2>/dev/null; }
wait_for "the process that outlived the command to end" gone

# --- A limit or a pause that is not a number is refused before anything runs.
expect_fail "a limit that is not a number" "the limit must be" env WHIZBANG_MEMORY_LIMIT_KIB=lots "$watch" true
expect_fail "a pause that is not a number" "the seconds between two counts" env WHIZBANG_MEMORY_POLL=soon "$watch" true
expect_fail "no command" "usage" "$watch"

finished
