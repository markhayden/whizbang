#!/usr/bin/env bash
# A syntax error in a script is a failure, whatever bash runs it. The bash that ships with
# macOS (3.2) runs a script up to its first syntax error and then exits 0 if the script has
# set an `EXIT` trap, so three things stand in the way: scripts/syntax.sh parses every
# script before any test runs; a script that sets such a trap first parses itself; and
# scripts/tests/run.sh passes a test file only when it printed its last line.
source "$(dirname "$0")/lib.sh"

syntax="$root/scripts/syntax.sh"
run="$root/scripts/tests/run.sh"
guard_text="has a syntax error; refusing to run"

# The shells to try: the one running this test and, where it is another one, /bin/bash.
shells=("$BASH")
if [ -x /bin/bash ] && [ "$(/bin/bash -c 'echo $BASH_VERSION')" != "$BASH_VERSION" ]; then
  shells+=(/bin/bash)
fi
major() { "$1" -c 'echo ${BASH_VERSINFO[0]}'; }

# --- The problem itself: a trap, then a syntax error later in the file.
mkdir "$tmp/problem"
write_problem() { # <file> <first line>
  printf '%s\n' '#!/usr/bin/env bash' "$2" 'set -euo pipefail' "trap 'true' EXIT" \
    'echo "it ran"' 'if true; then' '  echo "inside"' 'done' 'echo "the end"' >"$1"
}
write_problem "$tmp/problem/unguarded.sh" ':'
write_problem "$tmp/problem/guarded.sh" \
  '"$BASH" -n "${BASH_SOURCE[0]}" || { echo "fixture: ${BASH_SOURCE[0]} '"$guard_text"'" >&2; exit 2; }'
for sh in "${shells[@]}"; do
  status=0
  "$sh" "$tmp/problem/unguarded.sh" >"$tmp/out" 2>&1 || status=$?
  grep -q -F "syntax error" "$tmp/out" || fail "$sh: the fixture has a syntax error"
  # This is the fault. If bash 3 ever stops doing it, this test says so.
  if [ "$(major "$sh")" = 3 ]; then
    [ "$status" = 0 ] || fail "$sh: bash 3 was expected to exit 0 here (got $status)"
    grep -q -x -F "it ran" "$tmp/out" || fail "$sh: bash 3 was expected to run the lines before the error"
  fi
  # With the first line that parses the file, nothing runs and the status is 2.
  expect_fail "$sh: a script that parses itself first" "$guard_text" "$sh" "$tmp/problem/guarded.sh"
  "$sh" "$tmp/problem/guarded.sh" >"$tmp/out" 2>&1 || status=$?
  [ "$status" = 2 ] || fail "$sh: a script that parses itself first exits 2 (got $status)"
  grep -q -F "it ran" "$tmp/out" && fail "$sh: nothing of a broken script runs"
  # The check over a folder catches the unguarded one.
  expect_fail "$sh: the syntax check" "unguarded.sh has a syntax error" "$sh" "$syntax" "$tmp/problem"
  grep -q -F "guarded.sh has a syntax error" "$tmp/out" || fail "$sh: every broken script is named"
  grep -q -F "2 of 2 shell scripts" "$tmp/out" || { cat "$tmp/out" >&2; fail "$sh: the count of broken scripts"; }
done

# --- Scripts are found by extension or by their first line, in any folder below.
mkdir -p "$tmp/found/hooks" "$tmp/found/a folder/deep"
printf '#!/usr/bin/env bash\necho ok\n' >"$tmp/found/good.sh"
printf 'echo ok\n' >"$tmp/found/hooks/no-first-line.sh"
printf '#!/usr/bin/env bash\necho ok\n' >"$tmp/found/hooks/good-hook"
printf 'not a script: fi ) done (\n' >"$tmp/found/notes.txt"
printf '#!/usr/bin/env python3\nprint("fi"))\n' >"$tmp/found/other-language"
expect_pass "a folder of sound scripts" "$syntax" "$tmp/found"
grep -q -F "ok (3 shell scripts" "$tmp/out" || { cat "$tmp/out" >&2; fail "three scripts are found"; }
# A new script with a syntax error, of each kind.
printf '#!/usr/bin/env bash\nif true; then\n' >"$tmp/found/hooks/new-hook"
expect_fail "a new hook with no extension" "hooks/new-hook has a syntax error" "$syntax" "$tmp/found"
rm "$tmp/found/hooks/new-hook"
printf '#!/bin/sh\ndone\n' >"$tmp/found/hooks/new-sh-hook"
expect_fail "a new hook for sh" "hooks/new-sh-hook has a syntax error" "$syntax" "$tmp/found"
rm "$tmp/found/hooks/new-sh-hook"
printf 'echo "sourced, no first line"\n}\n' >"$tmp/found/a folder/deep/new part.sh"
expect_fail "a new sourced file in a folder with a space" "a folder/deep/new part.sh has a syntax error" \
  "$syntax" "$tmp/found"
rm "$tmp/found/a folder/deep/new part.sh"
expect_pass "the folder is sound again" "$syntax" "$tmp/found"
expect_fail "a folder with no script" "no shell script" "$syntax" "$tmp/found/a folder"
expect_fail "a folder that does not exist" "no such folder" "$syntax" "$tmp/absent"

# --- This repo's scripts: all sound, and a planted error anywhere among them is found.
expect_pass "this repo's scripts" "$syntax"
cp -R scripts "$tmp/scripts"
for f in hooks/pre-commit hooks/commit-msg hooks/pre-push scan.sh tests/lib.sh tests/scan/common.sh \
  tests/scan/odd.sh tests/test-heavy.sh; do
  [ -f "$tmp/scripts/$f" ] || fail "this repo has scripts/$f"
  cp "$tmp/scripts/$f" "$tmp/kept"
  printf '\nif true; then\n' >>"$tmp/scripts/$f"
  expect_fail "an error planted in scripts/$f" "scripts/$f has a syntax error" "$syntax" "$tmp/scripts"
  grep -q -F "1 of " "$tmp/out" || { cat "$tmp/out" >&2; fail "only scripts/$f is broken"; }
  cp "$tmp/kept" "$tmp/scripts/$f"
done

# --- Every script here that sets an EXIT trap parses itself first, and so refuses to run
# when it is broken. tests/lib.sh does it for every test file that sources it.
guarded=(check-crate-graph.sh docs.sh heavy.sh scan.sh)
for name in "${guarded[@]}"; do
  # The error goes in before the last line: bash 3 would have run everything above it.
  sed '$d' "scripts/$name" >"$tmp/$name"
  printf 'if true; then\n' >>"$tmp/$name"
  tail -n 1 "scripts/$name" >>"$tmp/$name"
  chmod +x "$tmp/$name"
  for sh in "${shells[@]}"; do
    status=0
    "$sh" "$tmp/$name" >"$tmp/out" 2>&1 || status=$?
    [ "$status" = 2 ] || { cat "$tmp/out" >&2; fail "$sh: a broken $name exits 2 (got $status)"; }
    grep -q -F "$name $guard_text" "$tmp/out" || { cat "$tmp/out" >&2; fail "$sh: a broken $name says it refuses"; }
    [ "$(grep -c . "$tmp/out")" -le 3 ] || { cat "$tmp/out" >&2; fail "$sh: a broken $name runs nothing"; }
  done
done
# A test file that sources lib.sh, directly or through another sourced file.
mkdir "$tmp/chain"
lib="$root/scripts/tests/lib.sh"
printf '%s\n' "source \"$lib\"" 'echo "it ran"' 'done' 'finished' >"$tmp/chain/direct.sh"
printf '%s\n' "source \"$lib\"" 'echo "the middle file ran"' >"$tmp/chain/middle.sh"
printf '%s\n' "source \"$tmp/chain/middle.sh\"" 'echo "it ran"' 'done' 'finished' >"$tmp/chain/through.sh"
printf '%s\n' "source \"$lib\"" 'echo "it ran"' 'if true; then' >"$tmp/chain/broken-middle.sh"
printf '%s\n' "source \"$tmp/chain/broken-middle.sh\"" 'echo "it ran"' 'finished' >"$tmp/chain/outer.sh"
for sh in "${shells[@]}"; do
  for f in direct.sh through.sh outer.sh; do
    status=0
    "$sh" "$tmp/chain/$f" >"$tmp/out" 2>&1 || status=$?
    [ "$status" = 2 ] || { cat "$tmp/out" >&2; fail "$sh: a broken test file ($f) exits 2 (got $status)"; }
    grep -q -F "$guard_text" "$tmp/out" || { cat "$tmp/out" >&2; fail "$sh: a broken test file ($f) says it refuses"; }
    grep -q -e "it ran" -e "finished" "$tmp/out" && { cat "$tmp/out" >&2; fail "$sh: nothing of a broken test file ($f) runs"; }
  done
done
# The rule, for scripts to come: a line that sets an EXIT trap comes after the line that
# parses the file. (A test file gets both from lib.sh.)
while IFS= read -r -d '' f; do
  trap_line=$(grep -n -E '^[[:space:]]*trap .*EXIT' "$f" | head -n 1 | cut -d: -f1) || true
  [ -n "$trap_line" ] || continue
  guard_line=$(grep -n -F '"$BASH" -n' "$f" | head -n 1 | cut -d: -f1) || true
  [ -n "$guard_line" ] && [ "$guard_line" -lt "$trap_line" ] ||
    fail "$f sets an EXIT trap without parsing itself first (see the top of scripts/scan.sh)"
done < <(find scripts -type f -print0)

# --- run.sh: the syntax check comes before any test, and a test file passes only when it
# printed its last line.
mkdir "$tmp/tests"
printf 'source "%s/scripts/tests/lib.sh"\necho "a passing test"\nfinished\n' "$root" >"$tmp/tests/test-a-pass.sh"
expect_pass "a folder of passing tests" "$run" "$tmp/tests"
grep -q -E "^ok +$tmp/tests/test-a-pass.sh$" "$tmp/out" || { cat "$tmp/out" >&2; fail "a passing test is reported ok"; }
# A test that exits 0 before its end, as bash 3 does at a syntax error.
printf 'source "%s/scripts/tests/lib.sh"\necho "half of a test"\nexit 0\nfinished\n' "$root" >"$tmp/tests/test-b-early.sh"
expect_fail "a test that exits 0 early" "exited 0 before its last line" "$run" "$tmp/tests"
grep -q -E "^FAIL +$tmp/tests/test-b-early.sh$" "$tmp/out" || { cat "$tmp/out" >&2; fail "the early test is reported failed"; }
grep -q -E "^ok +$tmp/tests/test-b-early.sh$" "$tmp/out" && { cat "$tmp/out" >&2; fail "the early test is not reported ok"; }
grep -q -E "^ok +$tmp/tests/test-a-pass.sh$" "$tmp/out" || { cat "$tmp/out" >&2; fail "the other test still passes"; }
# The last line must be the last: output after it means the file went on.
printf 'source "%s/scripts/tests/lib.sh"\nfinished\necho "more"\n' "$root" >"$tmp/tests/test-b-early.sh"
expect_fail "a test that prints after its last line" "exited 0 before its last line" "$run" "$tmp/tests"
# A test that fails is reported with its output, as before.
printf 'source "%s/scripts/tests/lib.sh"\nfail "a reason"\nfinished\n' "$root" >"$tmp/tests/test-b-early.sh"
expect_fail "a failing test" "failed: a reason" "$run" "$tmp/tests"
rm "$tmp/tests/test-b-early.sh"
# A syntax error in any file of the folder: no test runs at all.
mkdir "$tmp/tests/parts"
printf 'echo "a part"\nif true; then\n' >"$tmp/tests/parts/new.sh"
expect_fail "a syntax error beside the tests" "parts/new.sh has a syntax error" "$run" "$tmp/tests"
grep -q -F "no test was run" "$tmp/out" || { cat "$tmp/out" >&2; fail "run.sh says no test was run"; }
grep -q -E "^ok " "$tmp/out" && { cat "$tmp/out" >&2; fail "no test is reported ok after a syntax error"; }
rm -r "$tmp/tests/parts"
expect_pass "the folder passes again" "$run" "$tmp/tests"
expect_fail "a folder with no test" "no test" "$run" "$tmp/found"

# --- The scan tests run their parts as files of their own, and hold them to the same rule.
mkdir "$tmp/parts"
printf 'source "%s/scripts/tests/lib.sh"\necho "a passing part"\nfinished\n' "$root" >"$tmp/parts/a.sh"
expect_pass "parts that reach their end" bash scripts/tests/test-scan.sh "$tmp/parts"
printf 'source "%s/scripts/tests/lib.sh"\necho "half of a part"\nexit 0\nfinished\n' "$root" >"$tmp/parts/b.sh"
expect_fail "a part that exits 0 early" "parts/b.sh exited 0 before its last line" \
  bash scripts/tests/test-scan.sh "$tmp/parts"

finished
