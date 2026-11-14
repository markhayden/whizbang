#!/usr/bin/env bash
# The scan, edited while it runs. Bash reads a script as it goes, so a file changed during
# a run would make the rest of the run another program. The scan is one function, read
# whole before any of it runs (the comment above `main` in scripts/scan.sh), so the run
# finishes as the program it started as, whatever happens to the file meanwhile.
#
# The scan is stopped at a barrier this test holds, not by a sleep: a stand-in for git
# waits, at the first listing of the working tree, for a line from a pipe that only the
# test writes to. By then the scan has read its arguments and its settings and has the
# whole search still to do. The test changes the file, in place, and lets the scan go on.
# What the test waits for is the stand-in's note that it is there, for a limited time.
source "$(dirname "$0")/common.sh"

real_git=$(command -v git)
mkdir "$tmp/gitbin"
cat >"$tmp/gitbin/git" <<SH
#!/usr/bin/env bash
real="$real_git"
SH
cat >>"$tmp/gitbin/git" <<'SH'
if [ -n "${BARRIER:-}" ] && [ ! -e "$BARRIER.reached" ]; then
  case " $* " in *" ls-files "*)
    # LATE and SLOW are for the cases about this test's own waiting: a stand-in that
    # is late to the barrier, and one that is slow between its note and its read.
    [ -z "${BARRIER_LATE:-}" ] || sleep "$BARRIER_LATE"
    : >"$BARRIER.reached"
    [ -z "${BARRIER_SLOW:-}" ] || sleep "$BARRIER_SLOW"
    # Opened to read and to write, which does not wait for the other end; and read for a
    # limited time, so that a stand-in whose test has gone does not stay for ever.
    exec 9<>"$BARRIER"
    read -t "${BARRIER_WAIT:-60}" -r _ <&9 || true
    exec 9>&-
    ;;
  esac
fi
exec "$real" "$@"
SH
chmod +x "$tmp/gitbin/git"

fresh edited
printf 'token = %s\n' "$key" >planted.txt
here="scripts/scan.sh"
cmp -s "$scan" "$here" || fail "the throwaway repository runs a copy of this repo's scan"
size=$(wc -c <"$here" | tr -d ' ')
[ "$size" -gt 40000 ] || fail "the scan is longer than any buffer a shell reads a script through ($size bytes)"

# edited_run <how> [<looks>]: one scan of the working tree, with its file changed at the
# barrier. <looks> is how many times, a tenth of a second apart, the test looks for the
# stand-in's note before it gives up: 600, which is 60 seconds, unless a case says less.
# Each run has a pipe and a note of its own, so that a stand-in left from one run cannot
# be taken for the next.
edited_run() {
  local how=$1 limit=${2:-600} rc=0 barrier="$tmp/barrier.$1"
  cp "$scan" "$here"
  rm -f "$barrier" "$barrier.reached"
  mkfifo "$barrier"
  env -u WHIZBANG_WORDLIST PATH="$tmp/gitbin:$PATH" BARRIER="$barrier" bash "$here" --secrets-only >"$tmp/out" 2>&1 &
  local pid=$!
  # The pipe is opened to read and to write, which does not wait: a scan that ended before
  # the barrier would otherwise leave this test waiting for ever. It stays open until the
  # scan has ended: the stand-in notes that it is at the barrier and only then opens the
  # pipe, so a line written and an end closed in between would be lost, and the stand-in
  # would wait with nobody to write to it. The wait for the note is the only one here
  # with a clock, and the clock is its limit.
  exec 3<>"$barrier"
  local looks=0
  until [ -e "$barrier.reached" ]; do
    looks=$((looks + 1))
    if [ "$looks" -gt "$limit" ] || ! kill -0 "$pid" 2>/dev/null; then
      echo go >&3
      kill "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
      exec 3>&-
      cat "$tmp/out" >&2
      fail "$how: the scan did not reach the barrier (it ended, or the time allowed passed)"
    fi
    sleep 0.1
  done
  case "$how" in
    untouched | slow) ;;
    emptied) : >"$here" ;;
    replaced)
      # Another program of the same length, written over the file in place: every line
      # of it says so and ends the run as a pass.
      awk -v size="$size" 'BEGIN { line = "echo EDITED; exit 0"; for (n = 0; n + length(line) + 1 <= size; n += length(line) + 1) print line }' >"$tmp/other"
      cat "$tmp/other" >"$here"
      ;;
    lengthened) printf 'echo EDITED\nexit 0\n' >>"$here" ;;
    shifted)
      # A line put in at the top moves every byte after it: the shell's place in the file
      # is then in the middle of another line.
      { sed -n '1p' "$scan"; echo "# a line that was not there"; sed '1d' "$scan" | sed 's/secrets scan: found/EDITED/'; } >"$tmp/other"
      cat "$tmp/other" >"$here"
      ;;
  esac
  echo go >&3
  wait "$pid" || rc=$?
  exec 3>&-
  cp "$scan" "$here"
  [ "$rc" = 1 ] || { cat "$tmp/out" >&2; fail "$how: the scan ends as it started, with what it found (exit 1; got $rc)"; }
  says "secrets scan: found a match for a forbidden pattern in:"
  says "planted.txt:1"
  grep -a -q -e "EDITED" -e "stopped before its end" -e "syntax error" "$tmp/out" &&
    { cat "$tmp/out" >&2; fail "$how: nothing of the changed file runs"; }
  return 0
}
# Five runs, one after another, each waited for.
for how in untouched emptied replaced lengthened shifted; do
  edited_run "$how"
done

# The test's own waiting. A stand-in that is slow between its note and its read (one
# second) still gets the line: the pipe is open until the scan has ended. Without that,
# the line was lost and the stand-in waited for ever.
BARRIER_SLOW=1 edited_run slow
# And the limit of the wait trips: with a stand-in that is three seconds late to the
# barrier and five looks allowed, the run fails and says why, and leaves no scan behind.
# The stand-in it leaves reads for one second and ends.
if (BARRIER_LATE=3 BARRIER_WAIT=1 edited_run late 5) >"$tmp/late" 2>&1; then
  cat "$tmp/late" >&2
  fail "a stand-in that is late to the barrier trips the limit of the wait"
fi
grep -q -F "late: the scan did not reach the barrier" "$tmp/late" || { cat "$tmp/late" >&2; fail "the limit of the wait says what happened"; }
cp "$scan" "$here"

finished
