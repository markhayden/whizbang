#!/usr/bin/env bash
# A syntax error in a script is a failure, whatever bash runs it. The bash that ships with
# macOS (3.2) runs a script up to its first syntax error and then exits 0 if the script has
# set an `EXIT` trap, so four things stand in the way: scripts/syntax.sh parses every
# script before any test runs; a script that sets such a trap first parses itself; its
# trap turns an exit of 0 into 70 unless the script reached an exit it meant (which also
# covers the faults no parse finds); and scripts/tests/run.sh passes a test file only when
# it printed its last line.
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
printf '#!/bin/bash\nif true; then\n' >"$tmp/found/hooks/new-bin-bash-hook"
expect_fail "a new hook for /bin/bash" "hooks/new-bin-bash-hook has a syntax error" "$syntax" "$tmp/found"
rm "$tmp/found/hooks/new-bin-bash-hook"
printf 'if true; then\n' >"$tmp/found/new.bash"
expect_fail "a new file that ends in .bash" "new.bash has a syntax error" "$syntax" "$tmp/found"
# A link to a script is checked under the link's name, wherever the script is.
mkdir "$tmp/elsewhere"
mv "$tmp/found/new.bash" "$tmp/elsewhere/target"
ln -s "$tmp/elsewhere/target" "$tmp/found/linked.sh"
expect_fail "a link to a broken script" "linked.sh has a syntax error" "$syntax" "$tmp/found"
rm "$tmp/found/linked.sh"
ln -s "$tmp/elsewhere" "$tmp/found/a-folder-link"
ln -s "$tmp/elsewhere/nothing" "$tmp/found/a-dead-link.sh"
expect_pass "links to a folder and to nothing are not scripts" "$syntax" "$tmp/found"
rm "$tmp/found/a-folder-link" "$tmp/found/a-dead-link.sh"
# A file that cannot be read cannot be checked, so it fails, with or without an extension.
for name in locked.sh locked-hook; do
  printf '#!/usr/bin/env bash\necho ok\n' >"$tmp/found/$name"
  chmod 000 "$tmp/found/$name"
  if [ -r "$tmp/found/$name" ]; then
    echo "skip: an unreadable script; this user can read a file with mode 000"
  else
    expect_fail "a script that cannot be read ($name)" "$name cannot be read" "$syntax" "$tmp/found"
  fi
  rm -f "$tmp/found/$name"
done
expect_pass "the folder is sound again" "$syntax" "$tmp/found"
# A folder whose name starts with a dash is a folder, not an option.
mkdir "$tmp/-dash"
printf 'echo ok\n' >"$tmp/-dash/good.sh"
(cd "$tmp" && expect_pass "a folder named with a dash" "$syntax" -dash)
printf 'if true; then\n' >"$tmp/-dash/bad.sh"
(cd "$tmp" && expect_fail "a broken script in a folder named with a dash" "bad.sh has a syntax error" "$syntax" -dash)
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
# --- Faults that no parse finds. Once the trap is set, bash 3 exits 0 from each of these,
# and any bash exits 0 when a here-document with no end swallows the rest of the file. The
# trap of every script turns that 0 into 70. Each fault goes in on the line after the trap.
printf 'fi\n' >"$tmp/faulty-part"
export FAULTY_PART="$tmp/faulty-part"
fault_names=(unset-variable readonly bad-substitution eval sourced-file here-document)
fault_lines=(
  'echo "$no_such_variable"'
  'readonly fixed=1; fixed=2'
  'value=1; echo "${value.x}"'
  "eval 'if true; then'"
  'source "$FAULTY_PART"'
  "cat <<'NEVER_ENDS' >/dev/null"
)
# An empty array is a fault only where the shell takes it for an unset variable.
for sh in "${shells[@]}"; do
  if ! "$sh" -u -c 'none=(); echo "${none[@]}"' >/dev/null 2>&1; then
    fault_names+=(empty-array)
    fault_lines+=('none=(); echo "${none[@]}"')
    break
  fi
done
with_fault() { # <script> <fault line> <to>: the script with the fault after its EXIT trap
  FAULT="$2" awk '{ print } /^trap .* EXIT$/ && !put { print ENVIRON["FAULT"]; put = 1 }' "$1" >"$3"
  grep -q -F -e "$2" "$3" || fail "$1 has an EXIT trap to put a fault after"
  chmod +x "$3"
}
# faulty <shell> <what> <status wanted, or "not 0"> <command...>
faulty() {
  local sh=$1 what=$2 want=$3 status=0
  shift 3
  "$@" >"$tmp/out" 2>&1 || status=$?
  [ "$status" -ne 0 ] || { cat "$tmp/out" >&2; fail "$sh: $what exits 0"; }
  [ "$want" = "not 0" ] || [ "$status" = "$want" ] || { cat "$tmp/out" >&2; fail "$sh: $what exits $want (got $status)"; }
  grep -q -F "went on" "$tmp/out" && { cat "$tmp/out" >&2; fail "$sh: $what stops there"; }
  [ "$status" != 70 ] || grep -q -F "stopped before its end; failing" "$tmp/out" ||
    { cat "$tmp/out" >&2; fail "$sh: $what says that it stopped before its end"; }
  return 0
}
echo "feat: a message" >"$tmp/message"
mkdir "$tmp/faults"
i=0
while [ "$i" -lt "${#fault_names[@]}" ]; do
  fault=${fault_names[$i]}
  line=${fault_lines[$i]}
  for name in "${guarded[@]}"; do with_fault "scripts/$name" "$line" "$tmp/faults/$name"; done
  printf '%s\n' "source \"$lib\"" "$line" 'echo "went on"' 'finished' >"$tmp/faults/a-test.sh"
  for sh in "${shells[@]}"; do
    # Where the shell itself exits 0 (bash 3, and any bash for the here-document), the 70
    # is the trap's; elsewhere the shell's own failure is kept.
    want="not 0"
    if [ "$(major "$sh")" = 3 ] || [ "$fault" = here-document ]; then want=70; fi
    "$sh" -n "$tmp/faults/scan.sh" 2>/dev/null || fail "$sh: a parse does not find the fault ($fault)"
    faulty "$sh" "scan.sh with a fault ($fault)" "$want" "$sh" "$tmp/faults/scan.sh" --message "$tmp/message"
    faulty "$sh" "docs.sh with a fault ($fault)" "$want" "$sh" "$tmp/faults/docs.sh" --check
    faulty "$sh" "heavy.sh with a fault ($fault)" "$want" \
      env WHIZBANG_HEAVY_LOCK="$tmp/faults/lock" "$sh" "$tmp/faults/heavy.sh" a-label echo "went on"
    [ ! -e "$tmp/faults/lock" ] || fail "$sh: heavy.sh with a fault ($fault) still releases the lock"
    faulty "$sh" "check-crate-graph.sh with a fault ($fault)" "$want" "$sh" "$tmp/faults/check-crate-graph.sh"
    faulty "$sh" "a test file with a fault ($fault)" "$want" "$sh" "$tmp/faults/a-test.sh"
  done
  i=$((i + 1))
done
# The exits that are meant keep their status: 0 from a script that finished, and the
# command's own status from heavy.sh, zero or not.
for sh in "${shells[@]}"; do
  expect_pass "$sh: the scan of a sound message" "$sh" scripts/scan.sh --message "$tmp/message"
  expect_pass "$sh: the crate graph" "$sh" scripts/check-crate-graph.sh
  expect_pass "$sh: heavy.sh around a command that passes" \
    env WHIZBANG_HEAVY_LOCK="$tmp/faults/lock" "$sh" scripts/heavy.sh a-label true
  status=0
  env WHIZBANG_HEAVY_LOCK="$tmp/faults/lock" "$sh" scripts/heavy.sh a-label bash -c 'exit 7' >"$tmp/out" 2>&1 || status=$?
  [ "$status" = 7 ] || fail "$sh: heavy.sh passes on the status of a failing command (got $status)"
  printf '%s\n' "source \"$lib\"" 'exit 0' 'finished' >"$tmp/faults/a-test.sh"
  faulty "$sh" "a test file that exits 0 before its end" 70 "$sh" "$tmp/faults/a-test.sh"
  printf '%s\n' "source \"$lib\"" 'exit 3' 'finished' >"$tmp/faults/a-test.sh"
  faulty "$sh" "a test file that exits 3" 3 "$sh" "$tmp/faults/a-test.sh"
done

# The rule, for scripts to come: a script that sets an EXIT trap parses itself on an
# earlier line, and its trap has the form that turns an unmeant exit of 0 into 70, with
# `done_ok=0` on the line before. (A test file gets all of it from lib.sh.)
trap_test='; [ "$done_ok" = 1 ] || [ "$rc" -ne 0 ] || { echo "'
trap_end=': stopped before its end; failing" >&2; rc=70; }; exit "$rc"'"'"' EXIT'
check_rule() { # <file>: prints what is wrong with it, if anything
  local f=$1 trap_line guard_line
  trap_line=$(grep -n -E '^[[:space:]]*trap .*EXIT' "$f" | head -n 1 | cut -d: -f1) || true
  [ -n "$trap_line" ] || return 0
  guard_line=$(grep -n -F '"$BASH" -n' "$f" | head -n 1 | cut -d: -f1) || true
  [ -n "$guard_line" ] && [ "$guard_line" -lt "$trap_line" ] ||
    echo "$f sets an EXIT trap without parsing itself first (see the top of scripts/scan.sh)"
  [ "$(grep -c -E '^[[:space:]]*trap .*EXIT' "$f")" = 1 ] || echo "$f sets more than one EXIT trap"
  sed -n "${trap_line}p" "$f" | grep -q -F -e "trap 'rc=\$?; " || echo "$f: the EXIT trap does not start by keeping the status"
  { sed -n "${trap_line}p" "$f" | grep -F -e "$trap_test" | grep -q -F -e "$trap_end"; } ||
    echo "$f: the EXIT trap does not turn an unmeant exit of 0 into 70 (see the top of scripts/scan.sh)"
  [ "$(sed -n "$((trap_line - 1))p" "$f")" = "done_ok=0" ] || echo "$f: done_ok=0 is not on the line before the EXIT trap"
}
while IFS= read -r -d '' f; do
  problems=$(check_rule "$f")
  [ -z "$problems" ] || fail "$problems"
done < <(find scripts -type f -print0)
# The rule itself: it finds a plain trap, and a trap with no parse before it.
printf '%s\n' '#!/usr/bin/env bash' '"$BASH" -n "${BASH_SOURCE[0]}" || exit 2' "trap 'rm -rf \"\$tmp\"' EXIT" >"$tmp/plain-trap.sh"
case "$(check_rule "$tmp/plain-trap.sh")" in
  *"does not turn an unmeant exit of 0 into 70"*) ;;
  *) fail "the rule finds a plain trap" ;;
esac
sed -n '/^done_ok=0$/,/^trap /p' scripts/scan.sh >"$tmp/no-parse.sh"
case "$(check_rule "$tmp/no-parse.sh")" in
  *"without parsing itself first"*) ;;
  *) fail "the rule finds a trap with no parse before it" ;;
esac
[ -z "$(check_rule scripts/scan.sh)" ] || fail "the rule passes scripts/scan.sh"

# --- run.sh: the syntax check comes before any test, and a test file passes only when it
# printed its last line.
mkdir "$tmp/tests"
printf 'source "%s/scripts/tests/lib.sh"\necho "a passing test"\nfinished\n' "$root" >"$tmp/tests/test-a-pass.sh"
expect_pass "a folder of passing tests" "$run" "$tmp/tests"
grep -q -E "^ok +$tmp/tests/test-a-pass.sh$" "$tmp/out" || { cat "$tmp/out" >&2; fail "a passing test is reported ok"; }
# A test that exits 0 before its end, as bash 3 does at a syntax error. This one does not
# use lib.sh, whose trap would turn the 0 into 70: the last line is a check of its own.
printf 'echo "half of a test"\nexit 0\necho "finished: $0"\n' >"$tmp/tests/test-b-early.sh"
expect_fail "a test that exits 0 early" "exited 0 before its last line" "$run" "$tmp/tests"
grep -q -E "^FAIL +$tmp/tests/test-b-early.sh$" "$tmp/out" || { cat "$tmp/out" >&2; fail "the early test is reported failed"; }
grep -q -E "^ok +$tmp/tests/test-b-early.sh$" "$tmp/out" && { cat "$tmp/out" >&2; fail "the early test is not reported ok"; }
grep -q -E "^ok +$tmp/tests/test-a-pass.sh$" "$tmp/out" || { cat "$tmp/out" >&2; fail "the other test still passes"; }
# The last line must be the last: output after it means the file went on.
printf 'source "%s/scripts/tests/lib.sh"\nfinished\necho "more"\n' "$root" >"$tmp/tests/test-b-early.sh"
expect_fail "a test that prints after its last line" "exited 0 before its last line" "$run" "$tmp/tests"
# The last line names the file that printed it.
printf 'echo "finished: %s/tests/test-a-pass.sh"\n' "$tmp" >"$tmp/tests/test-b-early.sh"
expect_fail "a test that prints another file's last line" "exited 0 before its last line" "$run" "$tmp/tests"
printf 'echo "finished: test-b-early.sh"\n' >"$tmp/tests/test-b-early.sh"
expect_fail "a test that prints its last line without its path" "exited 0 before its last line" "$run" "$tmp/tests"
# The last line is not enough either: the status counts.
printf 'echo "finished: $0"\nexit 3\n' >"$tmp/tests/test-b-early.sh"
expect_fail "a test that fails after its last line" "FAIL  $tmp/tests/test-b-early.sh" "$run" "$tmp/tests"
grep -q -E "^ok +$tmp/tests/test-b-early.sh$" "$tmp/out" && { cat "$tmp/out" >&2; fail "a test that fails after its last line is not ok"; }
# With lib.sh, exiting 0 before `finished` is a failure by its status too.
printf 'source "%s/scripts/tests/lib.sh"\necho "half of a test"\nexit 0\nfinished\n' "$root" >"$tmp/tests/test-b-early.sh"
expect_fail "a test with lib.sh that exits 0 early" "FAIL  $tmp/tests/test-b-early.sh" "$run" "$tmp/tests"
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
printf 'echo "half of a part"\nexit 0\necho "finished: $0"\n' >"$tmp/parts/b.sh"
expect_fail "a part that exits 0 early" "parts/b.sh exited 0 before its last line" \
  bash scripts/tests/test-scan.sh "$tmp/parts"
printf 'echo "finished: $0"\necho "more"\n' >"$tmp/parts/b.sh"
expect_fail "a part that prints after its last line" "parts/b.sh exited 0 before its last line" \
  bash scripts/tests/test-scan.sh "$tmp/parts"
printf 'echo "finished: %s/parts/a.sh"\n' "$tmp" >"$tmp/parts/b.sh"
expect_fail "a part that prints another part's last line" "parts/b.sh exited 0 before its last line" \
  bash scripts/tests/test-scan.sh "$tmp/parts"
printf 'echo "finished: $0"\nexit 3\n' >"$tmp/parts/b.sh"
expect_fail "a part that fails after its last line" "in $tmp/parts/b.sh:" bash scripts/tests/test-scan.sh "$tmp/parts"
rm "$tmp/parts/b.sh"
expect_pass "the parts pass again" bash scripts/tests/test-scan.sh "$tmp/parts"
mkdir "$tmp/no-parts"
expect_fail "a folder with no part" "no part under $tmp/no-parts" bash scripts/tests/test-scan.sh "$tmp/no-parts"
printf 'echo "sourced by the parts"\n' >"$tmp/no-parts/common.sh"
expect_fail "a folder with common.sh alone" "no part under" bash scripts/tests/test-scan.sh "$tmp/no-parts"

finished
