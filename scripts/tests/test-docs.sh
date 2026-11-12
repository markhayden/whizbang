#!/usr/bin/env bash
# The generated reference (scripts/docs.sh): a fresh copy passes the check, a stale or missing
# one fails it, generation is the same every time, and what is not a well-formed description
# is refused. Nothing here builds the binary: descriptions are saved documents, or come from
# a stand-in for cargo that prints one. `just docs-check` is the check against the real build,
# and the first part below holds the chain from there to CI in place.
source "$(dirname "$0")/lib.sh"

docs=scripts/docs.sh

# --- The chain from the real binary to CI ------------------------------------------------

# recipe <name>: the body of a recipe in the justfile, without its indentation.
recipe() {
  awk -v name="$1" '
    /^[^ #]/ { on = (index($0, name ":") == 1) ; next }
    on && /^    / { sub(/^    /, ""); print }' justfile
}
grep -q -E '^check:( [a-z-]+)* docs-check( [a-z-]+)*$' justfile ||
  fail "justfile: check must depend on docs-check"
grep -q -E '^ci:( [a-z-]+)* check( [a-z-]+)*$' justfile ||
  fail "justfile: ci must depend on check"
[ "$(recipe docs-check)" = "scripts/docs.sh --check" ] ||
  fail "justfile: docs-check must run exactly: scripts/docs.sh --check"
# A check that regenerated first would always pass: docs-check depends on nothing, and no
# recipe that check or ci reaches is `docs`.
grep -q -x 'docs-check:' justfile || fail "justfile: docs-check must have no dependencies"
# reached <recipe>: every recipe it depends on, directly or through another, one per line.
reached() {
  local todo=$1 seen="" name
  while [ -n "$todo" ]; do
    name=${todo%% *}
    todo=${todo#"$name"}
    todo=${todo# }
    case " $seen " in *" $name "*) continue ;; esac
    seen="$seen $name"
    # shellcheck disable=SC2046  # one word per dependency
    todo="$todo $(awk -v name="$name" '
      /^[a-z][a-z0-9-]*( [a-z]+)*:/ { head = $0; sub(/:.*/, "", head); split(head, w, " ")
        if (w[1] == name) { sub(/^[^:]*:/, ""); print } }' justfile | tr '\n' ' ')"
    todo=$(echo $todo)
  done
  echo "$seen" | tr ' ' '\n' | grep -v -x -e "$1" -e ''
}
reached check | grep -q -x 'docs-check' || fail "justfile: reached does not see docs-check under check"
for top in check ci; do
  if reached "$top" | grep -q -x 'docs'; then
    fail "justfile: $top must not reach docs: a check that regenerates first cannot fail"
  fi
done
[ "$(recipe docs)" = "scripts/docs.sh" ] ||
  fail "justfile: docs must run exactly: scripts/docs.sh"
grep -q -E '^ +run: just ci$' .github/workflows/ci.yml ||
  fail ".github/workflows/ci.yml must have a step that runs: just ci"

# A stand-in for cargo, first on the path. It accepts only the command the real description
# comes from, prints the file named by FAKE_DESCRIBE_OUT and exits with FAKE_DESCRIBE_EXIT.
mkdir "$tmp/bin"
cat >"$tmp/bin/cargo" <<'SH'
#!/usr/bin/env bash
if [ "$*" != "run --quiet -p whizbang -- describe --json --pretty" ]; then
  echo "stand-in cargo: unexpected arguments: $*" >&2
  exit 97
fi
cat "$FAKE_DESCRIBE_OUT"
exit "${FAKE_DESCRIBE_EXIT:-0}"
SH
chmod +x "$tmp/bin/cargo"
# with_binary <file> <exit> <command...>: run the command with the stand-in for cargo.
with_binary() {
  local file=$1 code=$2
  shift 2
  PATH="$tmp/bin:$PATH" FAKE_DESCRIBE_OUT=$file FAKE_DESCRIBE_EXIT=$code "$@"
}

committed=docs/reference/description.json
jq '. + {doors: []}' "$committed" >"$tmp/changed.json"
: >"$tmp/nothing"

# Without --description the description comes from the binary, not from the committed copy.
expect_pass "the binary prints the committed description" with_binary "$committed" 0 "$docs" --check
expect_fail "the binary prints a changed description" "docs/reference is stale" \
  with_binary "$tmp/changed.json" 0 "$docs" --check
expect_fail "the binary fails" "could not run" with_binary "$committed" 3 "$docs" --check
expect_fail "the binary prints nothing" "not a description" with_binary "$tmp/nothing" 0 "$docs" --check

# Writing takes the same path, and a failure writes nothing: never an empty file.
expect_pass "write from the binary" with_binary "$tmp/changed.json" 0 "$docs" --out "$tmp/frombin"
cmp -s "$tmp/changed.json" "$tmp/frombin/description.json" || fail "the written description is not the binary's"
expect_fail "write when the binary fails" "could not run" \
  with_binary "$committed" 3 "$docs" --out "$tmp/failed"
expect_fail "write when the binary prints nothing" "not a description" \
  with_binary "$tmp/nothing" 0 "$docs" --out "$tmp/failed"
[ ! -e "$tmp/failed" ] || fail "a failed generation wrote something"
expect_fail "a failed write over a good copy" "not a description" \
  with_binary "$tmp/nothing" 0 "$docs" --out "$tmp/frombin"
cmp -s "$tmp/changed.json" "$tmp/frombin/description.json" || fail "a failed generation changed a good copy"
[ -s "$tmp/frombin/actions.md" ] || fail "a failed generation emptied the page"

# --- Generation ----------------------------------------------------------------------------

# Odd spacing and key order: description.json is a copy, not a reformatting.
printf '{"version":"9.9.9",   "name": "whizbang",\n "actions":[ ],"description_version":1}\n' >"$tmp/empty.json"
cat >"$tmp/two.json" <<'JSON'
{
  "description_version": 1,
  "name": "whizbang",
  "version": "9.9.9",
  "actions": [
    {"name": "zebra_map", "purpose": "Listed first: 2 images in, 1 map out (50% faster)."},
    {"name": "apple-2", "purpose": "Listed second."}
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
# The page names the command it is made from, as the script runs it: with --pretty.
made_from='<!-- Generated by `just docs` from `whizbang describe --json --pretty`. Never edit it by hand. -->'
[ "$(head -n 1 "$tmp/ref/actions.md")" = "$made_from" ] || fail "the page's first line is not: $made_from"
[ "$(head -n 1 docs/reference/actions.md)" = "$made_from" ] || fail "the committed page's first line is not: $made_from"
grep -q -x -F '# Regenerate the reference (docs/reference/) from `whizbang describe --json --pretty`.' justfile ||
  fail "the justfile does not say the reference is made from describe --json --pretty"
# The smoke of the release binary runs the form the reference is made from.
grep -q -x -F '    target/release/whizbang describe --json --pretty > /dev/null' justfile ||
  fail "just smoke does not run describe --json --pretty"

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

# A stale copy fails, and the failure names the file, the folder that was checked and the fix.
expect_fail "a stale description.json" "$tmp/ref is stale" \
  "$docs" --check --description "$tmp/two.json" --out "$tmp/ref"
expect_fail "names the stale file" "description.json" \
  "$docs" --check --description "$tmp/two.json" --out "$tmp/ref"
expect_fail "names the fix" "just docs" \
  "$docs" --check --description "$tmp/two.json" --out "$tmp/ref"
if grep -q -F "docs/reference is stale" "$tmp/out"; then
  fail "the failure names docs/reference when another folder was checked"
fi

# Only the page is stale: someone edited it by hand.
echo "an edit by hand" >>"$tmp/ref/actions.md"
expect_fail "a page edited by hand" "actions.md" \
  "$docs" --check --description "$tmp/empty.json" --out "$tmp/ref"

# A missing file is stale too.
rm "$tmp/ref/actions.md"
expect_fail "a missing page" "actions.md is missing" \
  "$docs" --check --description "$tmp/empty.json" --out "$tmp/ref"
expect_fail "a missing folder" "$tmp/nowhere is stale" \
  "$docs" --check --description "$tmp/empty.json" --out "$tmp/nowhere"
[ ! -e "$tmp/nowhere" ] || fail "the check wrote to the folder it checks"

# The folder holds the two generated files and the tool records under tools/, nothing else.
# A file that nothing generates has its own message: regenerating would not remove it.
mkdir -p "$tmp/again/tools"
echo "a record" >"$tmp/again/tools/some_tool.md"
expect_pass "tool records beside the generated files" \
  "$docs" --check --description "$tmp/empty.json" --out "$tmp/again"
echo "by hand" >"$tmp/again/notes.md"
expect_fail "a file nothing generates" "notes.md is not generated" \
  "$docs" --check --description "$tmp/empty.json" --out "$tmp/again"
rm "$tmp/again/notes.md"
: >"$tmp/again/.hidden"
expect_fail "a hidden file nothing generates" ".hidden is not generated" \
  "$docs" --check --description "$tmp/empty.json" --out "$tmp/again"
if grep -q -F "just docs" "$tmp/out"; then
  fail "a stray file is answered with advice to regenerate"
fi
rm "$tmp/again/.hidden"
newline_name=$(printf 'actions.md\nmore')
: >"$tmp/again/$newline_name"
expect_fail "a file whose name holds a newline" "is not generated" \
  "$docs" --check --description "$tmp/empty.json" --out "$tmp/again"
rm "$tmp/again/$newline_name"
: >"$tmp/again/tools/.keep"
expect_pass "anything under tools/" "$docs" --check --description "$tmp/empty.json" --out "$tmp/again"
rm -r "$tmp/again/tools"
: >"$tmp/again/tools"
expect_fail "tools as a file" "tools is not a folder" \
  "$docs" --check --description "$tmp/empty.json" --out "$tmp/again"

# Inside a repository the folder is listed as git sees it: what is ignored (a file the
# system drops there) is not a stray file; what is untracked and not ignored is.
git init -q "$tmp/repo"
echo ".DS_Store" >"$tmp/repo/.gitignore"
# in_repo <arguments...>: the script, run from inside that repository.
in_repo() { (cd "$tmp/repo" && "$root/$docs" "$@"); }
expect_pass "generate inside a repository" in_repo --description "$tmp/empty.json" --out docs/reference
expect_pass "a fresh copy inside a repository" in_repo --check --description "$tmp/empty.json" --out docs/reference
grep -q -F "docs/reference matches" "$tmp/out" || fail "the folder is not named as the caller gave it"
: >"$tmp/repo/docs/reference/.DS_Store"
expect_pass "an ignored file" in_repo --check --description "$tmp/empty.json" --out docs/reference
: >"$tmp/repo/docs/reference/notes.md"
expect_fail "an untracked file that is not ignored" "docs/reference/notes.md is not generated" \
  in_repo --check --description "$tmp/empty.json" --out docs/reference
if grep -q -F "just docs" "$tmp/out"; then
  fail "a stray file in a repository is answered with advice to regenerate"
fi
rm "$tmp/repo/docs/reference/notes.md"
: >"$tmp/repo/docs/reference/$newline_name"
expect_fail "a newline in a name, inside a repository" "is not generated" \
  in_repo --check --description "$tmp/empty.json" --out docs/reference
rm "$tmp/repo/docs/reference/$newline_name"
git -C "$tmp/repo" add -A
: >"$tmp/repo/docs/reference/tracked.md"
git -C "$tmp/repo" add docs/reference/tracked.md
expect_fail "a tracked file nothing generates" "tracked.md is not generated" \
  in_repo --check --description "$tmp/empty.json" --out docs/reference
git -C "$tmp/repo" rm -q -f --cached docs/reference/tracked.md
rm "$tmp/repo/docs/reference/tracked.md"
# Stale and stray together: the advice to regenerate is given, and the stray file is named.
echo "by hand" >>"$tmp/repo/docs/reference/actions.md"
: >"$tmp/repo/docs/reference/notes.md"
expect_fail "stale and stray" "docs/reference is stale: run \`just docs\`" \
  in_repo --check --description "$tmp/empty.json" --out docs/reference
grep -q -F "notes.md is not generated" "$tmp/out" || fail "stale and stray: the stray file is not named"

# Actions are listed in the description's order (recipes come first there), each with its
# purpose as written.
expect_pass "generate with two actions" "$docs" --description "$tmp/two.json" --out "$tmp/two"
grep -n -F -e '## `zebra_map`' -e '## `apple-2`' "$tmp/two/actions.md" | cut -d: -f2 >"$tmp/order"
printf '## `zebra_map`\n## `apple-2`\n' >"$tmp/want"
cmp -s "$tmp/order" "$tmp/want" || { cat "$tmp/two/actions.md" >&2; fail "the actions are not in the description's order"; }
grep -q -x -F 'Listed first: 2 images in, 1 map out (50% faster).' "$tmp/two/actions.md" || fail "a purpose was changed"
[ "$(grep -c '^#' "$tmp/two/actions.md")" -eq 3 ] || fail "the page has headings other than its title and one per action"
if grep -q -F "registers no actions yet" "$tmp/two/actions.md"; then
  fail "a page with actions says there are none"
fi
expect_pass "the two-action copy is fresh" "$docs" --check --description "$tmp/two.json" --out "$tmp/two"

# The committed reference agrees with its own description.json, and a description whose type
# has changed (one more field) fails against it.
expect_pass "the committed page comes from the committed description" \
  "$docs" --check --description "$committed"
expect_fail "a changed description type against the committed copies" "docs/reference is stale" \
  "$docs" --check --description "$tmp/changed.json"

# --- What is refused -----------------------------------------------------------------------

# refuse <name> <text> <description>: neither written nor checked, and the reason is given.
refuse() {
  printf '%s\n' "$3" >"$tmp/bad.json"
  expect_fail "$1" "not a description" "$docs" --description "$tmp/bad.json" --out "$tmp/bad"
  grep -q -F -e "$2" "$tmp/out" || { cat "$tmp/out" >&2; fail "$1: expected the output to say: $2"; }
  [ ! -e "$tmp/bad" ] || fail "$1: something was written"
  expect_fail "$1, when checking" "not a description" \
    "$docs" --check --description "$tmp/bad.json" --out "$tmp/two"
}
# with <jq filter>: the two-action description, changed.
with() { jq -c "$1" "$tmp/two.json"; }

# The shape.
refuse "not JSON" "" 'not json'
refuse "an empty document" "exactly one JSON document" ''
refuse "two documents" "exactly one JSON document" "$(with .; with .)"
refuse "an array" "an object" '[]'
refuse "no description_version" "description_version" "$(with 'del(.description_version)')"
refuse "a description_version that is text" "description_version" "$(with '.description_version = "1"')"
refuse "a description_version that is not whole" "description_version" "$(with '.description_version = 1.5')"
refuse "a negative description_version" "description_version" "$(with '.description_version = -1')"
refuse "a description_version past 32 bits" "description_version" "$(with '.description_version = 4294967296')"
# Written out, because jq would rewrite these numbers.
refuse "a description_version of 1e400" "description_version" \
  '{"description_version":1e400,"name":"whizbang","version":"9.9.9","actions":[]}'
refuse "a description_version of 20 digits" "description_version" \
  '{"description_version":12345678901234567890,"name":"whizbang","version":"9.9.9","actions":[]}'
# A whole number however it is written, and printed one way by every jq.
for pair in "1.0 1" "1E2 100" "4294967295 4294967295" "0 0"; do
  printf '{"description_version":%s,"name":"whizbang","version":"9.9.9","actions":[]}\n' "${pair% *}" >"$tmp/dv.json"
  expect_pass "a description_version of ${pair% *}" "$docs" --description "$tmp/dv.json" --out "$tmp/dv"
  grep -q -F "(version ${pair#* })." "$tmp/dv/actions.md" ||
    { cat "$tmp/dv/actions.md" >&2; fail "a description_version of ${pair% *} is not printed as ${pair#* }"; }
  cmp -s "$tmp/dv.json" "$tmp/dv/description.json" || fail "description.json was rewritten"
done
refuse "no name" "name" "$(with 'del(.name)')"
refuse "a name that is a number" "name" "$(with '.name = 7')"
refuse "no version" "version" "$(with 'del(.version)')"
refuse "no actions" "actions" "$(with 'del(.actions)')"
refuse "actions that are not a list" "actions" "$(with '.actions = {}')"
refuse "an action that is text" "action 1" "$(with '.actions[0] = "zebra"')"
refuse "an action without a name" "action 2" "$(with 'del(.actions[1].name)')"
refuse "an action without a purpose" "action 2" "$(with 'del(.actions[1].purpose)')"
refuse "a purpose that is a number" "action 1" "$(with '.actions[0].purpose = 3')"

# Names and versions: nothing that could close the code span or start a line of its own.
refuse "a build name with a backtick" "name" "$(with '.name = "whiz`bang"')"
refuse "a version that is not three numbers" "version" "$(with '.version = "1.2"')"
refuse "a version with a newline" "version" "$(with '.version = "1.2.3\n# Forged"')"
refuse "a version with a space" "version" "$(with '.version = "1.2.3 beta"')"
refuse "a version with an empty part" "version" "$(with '.version = "1..3"')"
refuse "a version of 65 characters" "version" "$(with ".version = \"1.2.3-$(printf 'x%.0s' $(seq 1 59))\"")"
refuse "two actions with one name" "more than once" "$(with '.actions[1].name = .actions[0].name')"
grep -q -F "zebra_map" "$tmp/out" || fail "the repeated name is not given"
for name in "" "Zebra" "9lives" "-dash" "two words" 'tick`' "a.b" "a|b" 'new\nline' "café" \
  "$(printf 'x%.0s' $(seq 1 65))"; do
  refuse "the action name '$name'" "action 1" "$(with ".actions[0].name = \"$name\"")"
done
with '.version = "1.2.3-rc.1+build.5"' >"$tmp/pre.json"
expect_pass "a pre-release version with build data" "$docs" --description "$tmp/pre.json" --out "$tmp/pre"

# Purposes: one plain line.
for purpose in "" " Leading space." "Trailing space. " "# A heading" "2. A list item" "- A list item" \
  'Two\nlines.' 'Text\n\n## `forged`\n\nA forged action.' 'A carriage\rreturn.' 'A\ttab.' \
  'A bell\u0007.' 'A <b>tag</b>.' 'An <!-- unclosed comment.' 'A quote > mark.' 'A `tick`.' \
  'A | bar.' 'A [link](https://example.com).' 'An *emphasis*.' 'An _emphasis_.' \
  'A back\\slash.' 'See https://example.com/page for more.' 'See www.example.com for more.' \
  'See WWW.example.com for more.' 'See example.com://x.' 'An &amp; entity.' 'A line separator here.' 'Café.' \
  "$(printf 'A%.0s' $(seq 1 201))"; do
  refuse "the purpose '$purpose'" "action 1" "$(with ".actions[0].purpose = \"$purpose\"")"
done

expect_fail "a description that does not exist" "no such description" \
  "$docs" --description "$tmp/absent.json" --out "$tmp/bad"
expect_fail "an unknown argument" "unknown argument" "$docs" --nope

finished
