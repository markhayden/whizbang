#!/usr/bin/env bash
# The generated reference (scripts/docs.sh): a fresh copy passes the check, a stale or missing
# one fails it, and generation is the same every time. Descriptions are saved documents, so
# nothing here builds the binary; `just docs-check` is the check against the real build.
source "$(dirname "$0")/lib.sh"

docs=scripts/docs.sh

cat >"$tmp/empty.json" <<'JSON'
{
  "description_version": 1,
  "name": "whizbang",
  "version": "9.9.9",
  "actions": []
}
JSON
cat >"$tmp/two.json" <<'JSON'
{
  "description_version": 1,
  "name": "whizbang",
  "version": "9.9.9",
  "actions": [
    {"name": "zebra", "purpose": "Listed first | with a bar and a `tick`."},
    {"name": "apple", "purpose": "Listed second."}
  ]
}
JSON

# No actions: both files are written, and the page says so.
expect_pass "generate with no actions" "$docs" --description "$tmp/empty.json" --out "$tmp/ref"
cmp -s "$tmp/empty.json" "$tmp/ref/description.json" ||
  fail "description.json is not the description, byte for byte"
grep -q -F "registers no actions yet" "$tmp/ref/actions.md" || fail "the empty page does not say so"
grep -q -F "9.9.9" "$tmp/ref/actions.md" || fail "the page does not name the version"
grep -q -F "Never edit" "$tmp/ref/actions.md" || fail "the page does not say it is generated"

# A fresh copy passes the check.
expect_pass "a fresh copy" "$docs" --check --description "$tmp/empty.json" --out "$tmp/ref"

# The same input gives the same bytes, ending in one newline, with no path of this machine.
expect_pass "generate again" "$docs" --description "$tmp/empty.json" --out "$tmp/again"
for f in actions.md description.json; do
  cmp -s "$tmp/ref/$f" "$tmp/again/$f" || fail "$f differs between two generations"
  [ "$(tail -c 1 "$tmp/ref/$f" | wc -l)" -eq 1 ] || fail "$f does not end with a newline"
  [ -n "$(tail -c 2 "$tmp/ref/$f" | head -c 1 | tr -d '\n')" ] || fail "$f ends with a blank line"
  if grep -q -F -e "$root" -e "$tmp" -e "$HOME" "$tmp/ref/$f"; then
    fail "$f holds a path of this machine"
  fi
done

# A stale copy fails, and the failure names the file and the fix.
expect_fail "a stale description.json" "docs/reference is stale" \
  "$docs" --check --description "$tmp/two.json" --out "$tmp/ref"
expect_fail "names the stale file" "description.json" \
  "$docs" --check --description "$tmp/two.json" --out "$tmp/ref"
expect_fail "names the fix" "just docs" \
  "$docs" --check --description "$tmp/two.json" --out "$tmp/ref"

# Only the page is stale: someone edited it by hand.
echo "an edit by hand" >>"$tmp/ref/actions.md"
expect_fail "a page edited by hand" "actions.md" \
  "$docs" --check --description "$tmp/empty.json" --out "$tmp/ref"

# A missing file is stale too.
rm "$tmp/ref/actions.md"
expect_fail "a missing page" "actions.md" \
  "$docs" --check --description "$tmp/empty.json" --out "$tmp/ref"
expect_fail "a missing folder" "docs/reference is stale" \
  "$docs" --check --description "$tmp/empty.json" --out "$tmp/nowhere"
[ ! -e "$tmp/nowhere" ] || fail "the check wrote to the folder it checks"

# Actions are listed in the description's order (recipes come first there), each with its
# purpose as written.
expect_pass "generate with two actions" "$docs" --description "$tmp/two.json" --out "$tmp/two"
grep -n -F -e '## `zebra`' -e '## `apple`' "$tmp/two/actions.md" | cut -d: -f2 >"$tmp/order"
printf '## `zebra`\n## `apple`\n' >"$tmp/want"
cmp -s "$tmp/order" "$tmp/want" || { cat "$tmp/two/actions.md" >&2; fail "the actions are not in the description's order"; }
grep -q -x -F 'Listed first | with a bar and a `tick`.' "$tmp/two/actions.md" || fail "a purpose was changed"
if grep -q -F "registers no actions yet" "$tmp/two/actions.md"; then
  fail "a page with actions says there are none"
fi
expect_pass "the two-action copy is fresh" "$docs" --check --description "$tmp/two.json" --out "$tmp/two"

# The committed reference agrees with its own description.json, and a description whose type
# has changed (one more field) fails against it.
expect_pass "the committed page comes from the committed description" \
  "$docs" --check --description docs/reference/description.json
jq '. + {doors: []}' docs/reference/description.json >"$tmp/changed.json"
expect_fail "a changed description type against the committed copies" "docs/reference is stale" \
  "$docs" --check --description "$tmp/changed.json"

# What is not a description is refused, not rendered.
echo '{"name": "whizbang"}' >"$tmp/bad.json"
expect_fail "a document without actions" "not a description" \
  "$docs" --description "$tmp/bad.json" --out "$tmp/bad"
echo 'not json' >"$tmp/bad.json"
expect_fail "a document that is not JSON" "not a description" \
  "$docs" --description "$tmp/bad.json" --out "$tmp/bad"
expect_fail "an unknown argument" "unknown argument" "$docs" --nope
