#!/usr/bin/env bash
# A milestone's note names the tests that show what it claims (docs/plans/*-note.md). A
# test that was renamed or removed since would leave the note pointing at nothing, with
# no check failing. So every test a note cites must exist.
#
# A cited test is text between backticks that is a path of lower-case parts joined by `::`
# whose last part is three or more words joined by `_`, as in
# `store::a_second_writer_is_refused`. It exists when some file under crates/ holds
# `fn <last part>(`. A path whose last part is shorter (`whizbang_testkit::threads`) is a
# module or a function, and is not looked for.
source "$(dirname "$0")/lib.sh"

# cited <file>: the last part of each test the file cites, one a line, each once.
cited() {
  { grep -o -E '`[a-z0-9_]+(::[a-z0-9_]+)+`' "$1" || true; } |
    tr -d '`' | sed 's/.*:://' | { grep -E '^[a-z0-9]+(_[a-z0-9]+){2,}$' || true; } | sort -u
}

# missing <file>: each cited test that no file under crates/ defines.
missing() {
  local name
  cited "$1" | while IFS= read -r name; do
    grep -r -F -e "fn $name(" crates >/dev/null || echo "$name"
  done
}

# A Chrome case (plan m5) is cited as `<file>.spec.ts: <title>`, which may run across a
# line break. It exists when ui/e2e/<file>.spec.ts holds `test("<title>"`.

# chrome_cited <file>: each Chrome case the file cites, one a line, each once.
chrome_cited() {
  tr '\n' ' ' <"$1" | tr -s ' ' |
    { grep -o -E '`[a-z]+\.spec\.ts: [^`]+`' || true; } | tr -d '`' | sort -u
}

# chrome_missing <file> [<root>]: each cited Chrome case that is not under <root>/ui/e2e/.
chrome_missing() {
  local span file title root=${2:-.}
  chrome_cited "$1" | while IFS= read -r span; do
    file=${span%%.spec.ts: *}
    title=${span#*.spec.ts: }
    { [ -f "$root/ui/e2e/$file.spec.ts" ] &&
      grep -F -e "test(\"$title\"" "$root/ui/e2e/$file.spec.ts" >/dev/null; } || echo "$span"
  done
}

# The check of Chrome cases, on a tree and a note written here: one case that exists, cited
# across a line, one whose title is not there, and one whose file is not there.
mkdir -p "$tmp/tree/ui/e2e"
cat >"$tmp/tree/ui/e2e/run.spec.ts" <<'SPEC'
test("a rascal task runs from a picture and words to a result", async ({ page }) => {});
SPEC
cat >"$tmp/chrome.md" <<'NOTE'
Held by `run.spec.ts: a rascal task runs from a picture
and words to a result`, by `run.spec.ts: a case nobody wrote` and by `gone.spec.ts: a case`.
NOTE
[ "$(chrome_cited "$tmp/chrome.md" | wc -l | tr -d ' ')" = 3 ] || fail "three Chrome cases are cited, one across a line"
[ "$(chrome_missing "$tmp/chrome.md" "$tmp/tree" | tr '\n' '|')" = "gone.spec.ts: a case|run.spec.ts: a case nobody wrote|" ] ||
  fail "a cited Chrome case that does not exist is not reported, or one that exists is"

# The check itself, on a note written here: one test that exists, one that does not, and a
# path that is no test.
cat >"$tmp/note.md" <<'NOTE'
Held by `contract::text::a_field_name_is_an_action_name_without_dashes` and by
`store::this_test_was_never_written_by_anyone`; threads come from `whizbang_testkit::threads`.
NOTE
grep -r -F -e 'fn a_field_name_is_an_action_name_without_dashes(' crates >/dev/null ||
  fail "the test this file uses as one that exists is gone: name another"
[ "$(cited "$tmp/note.md" | wc -l | tr -d ' ')" = 2 ] || fail "two tests are cited, and a module is not one"
[ "$(missing "$tmp/note.md")" = "this_test_was_never_written_by_anyone" ] ||
  fail "a cited test that does not exist is not reported, or one that exists is"

# Every note there is.
found=0
for note in docs/plans/*-note.md; do
  [ -f "$note" ] || continue
  found=$((found + 1))
  gone=$(missing "$note")
  [ -z "$gone" ] || fail "$note cites tests that no file under crates/ defines: $(echo "$gone" | tr '\n' ' ')"
  gone=$(chrome_missing "$note")
  [ -z "$gone" ] || fail "$note cites Chrome cases that ui/e2e/ does not hold: $(echo "$gone" | tr '\n' '|')"
done
[ "$found" -gt 0 ] || fail "no note under docs/plans"
# The note of checkpoint 1A matches each clause of its row in the spec to tests: it cites many.
[ "$(cited docs/plans/m1a-note.md | wc -l | tr -d ' ')" -ge 40 ] ||
  fail "docs/plans/m1a-note.md cites fewer than 40 tests: its table of clauses is gone or is no longer read"

# So does the note of checkpoint 1B, for its row and for the controls of section 6.
[ "$(cited docs/plans/m1b-note.md | wc -l | tr -d ' ')" -ge 40 ] ||
  fail "docs/plans/m1b-note.md cites fewer than 40 tests: its tables of clauses are gone or are no longer read"

# And the note of milestone 2 (plan 2C, task F1), for its row.
[ "$(cited docs/plans/m2-note.md | wc -l | tr -d ' ')" -ge 30 ] ||
  fail "docs/plans/m2-note.md cites fewer than 30 tests: its table of clauses is gone or is no longer read"

# And the note of milestone 3 (plan m3, task B1), for its row.
[ "$(cited docs/plans/m3-note.md | wc -l | tr -d ' ')" -ge 45 ] ||
  fail "docs/plans/m3-note.md cites fewer than 45 tests: its table of clauses is gone or is no longer read"

# And the note of milestone 4 (plan m4, task N1), for its row.
[ "$(cited docs/plans/m4-note.md | wc -l | tr -d ' ')" -ge 50 ] ||
  fail "docs/plans/m4-note.md cites fewer than 50 tests: its table of clauses is gone or is no longer read"

# And the note of milestone 5 (plan m5, task N1), for its row: the Chrome cases of each kept
# row of section 7, and the tests behind them and the guard's.
[ "$(chrome_cited docs/plans/m5-note.md | wc -l | tr -d ' ')" -ge 50 ] ||
  fail "docs/plans/m5-note.md cites fewer than 50 Chrome cases: its table of rows is gone or is no longer read"
[ "$(cited docs/plans/m5-note.md | wc -l | tr -d ' ')" -ge 50 ] ||
  fail "docs/plans/m5-note.md cites fewer than 50 tests: its table of clauses is gone or is no longer read"

finished
