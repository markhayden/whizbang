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
done
[ "$found" -gt 0 ] || fail "no note under docs/plans"
# The note of checkpoint 1A matches each clause of its row in the spec to tests: it cites many.
[ "$(cited docs/plans/m1a-note.md | wc -l | tr -d ' ')" -ge 40 ] ||
  fail "docs/plans/m1a-note.md cites fewer than 40 tests: its table of clauses is gone or is no longer read"

finished
