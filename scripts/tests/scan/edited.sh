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
    : >"$BARRIER.reached"
    read -r _ <"$BARRIER"
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

# edited_run <how>: one scan of the working tree, with its file changed at the barrier.
edited_run() {
  local how=$1 rc=0
  cp "$scan" "$here"
  rm -f "$tmp/barrier" "$tmp/barrier.reached"
  mkfifo "$tmp/barrier"
  env -u WHIZBANG_WORDLIST PATH="$tmp/gitbin:$PATH" BARRIER="$tmp/barrier" bash "$here" --secrets-only >"$tmp/out" 2>&1 &
  local pid=$!
  # Opening the pipe to write waits until the stand-in has opened it to read: the scan is
  # then at the barrier.
  exec 3>"$tmp/barrier"
  [ -e "$tmp/barrier.reached" ] || fail "$how: the scan reached the barrier"
  case "$how" in
    untouched) ;;
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
  exec 3>&-
  wait "$pid" || rc=$?
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

finished
