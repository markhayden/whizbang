#!/usr/bin/env bash
# The title and body of a pull request, as two files: `scan.sh --pr-text`. Both scans and
# the attribution check, and the four things that were found only by reading: a bare agent
# or model name in a sentence, a value of the repo's local .env, a path that starts with
# `~`, and an attribution line behind a leading mark. A refusal says where, never what.
source "$(dirname "$0")/common.sh"

pr="$tmp/pr"
mkdir "$pr"
title="$pr/title.txt"
body="$pr/body.md"
check() { "$scan" --pr-text "$title" "$body"; }
# text <body line>...: a sound title, and a body of these lines after two of its own, so
# the first line given is line 3 of the body.
text() {
  printf '%s\n' "docs: a title" >"$title"
  printf '%s\n' "What changed." "" "$@" >"$body"
}
names_agent="names an agent or a model"
tilde_path="a path that starts with ~"
mark_credit='Co''-Authored-By: A Person <person@example.com>'
made='Generated ''with [Some Tool](https://example.com/tool)'

# --- A text with none of it passes, and says what it covered.
text "# A heading" "- a point about authors" "* another" "> a quote" "| a | table |" \
  "1. The skills are in \`.claude/skills/kickoff/SKILL.md\` and \`CLAUDE.md\` stays." \
  "- [x] a folder named \`.claude\`, at any depth, and .claude/agents/builder.md" \
  "It takes ~5 seconds, about ~3/4 of what it took; ~~this is struck~~ and a~b is not a path." \
  "#8 is the pull request before. Write to someone@example.com." \
  "disk-usage-report-generator-v2 is a name, not a key."
expect_pass "a text with nothing to refuse" check
says "secrets scan: clean (the title and body of a pull request)"
says "attribution check: clean (the title and body of a pull request)"
says "clean-room scan: 0 matching lines"
grep -a -q -F -e "to read" "$tmp/out" && { cat "$tmp/out" >&2; fail "a sound text lists no line to read"; }
expect_pass "the same, without the clean-room scan" env -u WHIZBANG_WORDLIST "$scan" --secrets-only --pr-text "$title" "$body"
# An empty body is a body.
: >"$body"
expect_pass "an empty body" check

# --- 1. A bare agent or model name in a sentence, in any case, joined or not.
other_names=("Cod""ex" "Open""AI" "Chat""GPT" "Gem""ini" "Co""pilot" "An""thropic" "Dev""in" "Ai""der" "Cur""sor agent")
text "This was written with $N in an afternoon."
expect_fail "a bare name in a sentence" "pull-request/body:3: $names_agent" check
quiet "$N"
text "words" "$(printf '%s' "$N" | tr 'A-Z' 'a-z') did the first draft"
expect_fail "a bare name in small letters" "pull-request/body:4: $names_agent" check
text "the ${N}Code session found it" "and (${N}) agreed"
expect_fail "a joined name" "pull-request/body:3: $names_agent" check
says "pull-request/body:4: $names_agent"
# Each other name, one to a line of one body.
lines=()
for name in "${other_names[@]}"; do lines+=("reviewed by $name before the push"); done
text "${lines[@]}"
expect_fail "the other names in sentences" "pull-request/body:3: $names_agent" check
i=0
while [ "$i" -lt "${#other_names[@]}" ]; do
  says "pull-request/body:$((i + 3)): $names_agent"
  quiet "${other_names[$i]}"
  i=$((i + 1))
done
text "nothing"
printf '%s\n' "docs: what $N found" >"$title"
expect_fail "a bare name in the title" "pull-request/title:1: $names_agent" check
# The folder and the root file are paths; the name beside them is still a name.
text "see .${N}/notes and \`.claude/skills\`"
expect_fail "a folder that is not the one this repo has" "pull-request/body:3: $names_agent" check
text "\`.claude/skills\` were read by $N"
expect_fail "a name beside the folder's" "pull-request/body:3: $names_agent" check

# --- 2. A value of the repo's local .env: the variable is named, the value never printed.
secret_value="value-$(printf '%s' 0918273645)-of-env"
printf 'SOME_TOKEN="%s"\n' "$secret_value" >.env
text "set it to $secret_value and run"
printf '%s\n' "docs: $secret_value" >"$title"
expect_fail "a value of .env in the title and the body" "pull-request/body:3 (the value of SOME_TOKEN in .env)" check
says "pull-request/title:1 (the value of SOME_TOKEN in .env)"
quiet "$secret_value"
# From a folder below the top, with relative names, it is the same repo and the same .env.
mkdir -p below
cp "$title" below/t.txt
cp "$body" below/b.md
(cd below && expect_fail "a value of .env, from a folder below" "pull-request/body:3 (the value of SOME_TOKEN in .env)" \
  "$scan" --pr-text t.txt b.md)
rm -r below .env

# --- 3. A path that starts with ~.
lines=("the list is at ~/private/words.txt" "see \`~/notes/plan.md\`" "(~someone/shared/x)" "~/x" \
  "cd ~/work && ls" "path=~/.config/tool" "[notes](~/notes.md)")
text "${lines[@]}"
expect_fail "paths that start with ~" "pull-request/body:3: $tilde_path" check
i=0
while [ "$i" -lt "${#lines[@]}" ]; do
  says "pull-request/body:$((i + 3)): $tilde_path"
  i=$((i + 1))
done
quiet "words.txt"
quiet "notes"
text "nothing"
printf '%s\n' "docs: move ~/old to the repo" >"$title"
expect_fail "a path that starts with ~, in the title" "pull-request/title:1: $tilde_path" check

# --- 4. An attribution line behind a leading mark: every mark the carry-over row listed,
# the marks the stripped copy of the skills took off, and a cell of a table.
marks=('1. ' '+ ' '| ' '`' '~ ' '<!-- ' '- [x] ' '_' '# ' '#' '## ' '- ' '* ' '   - ' '> # ' '' \
  '| a cell | ' '- [X] ' '12) ' '**' '- [ ] ' '> > ')
# One body for each kind of line, with one line for each mark.
kinds=("$mark_credit" "co""-authored-by: a person" "$made" "Signed-off-by: Mark via $N <m@example.com>")
problems=("$has_credit" "$has_credit" "$credits_agent" "$credits_agent")
k=0
while [ "$k" -lt "${#kinds[@]}" ]; do
  lines=()
  for mark in "${marks[@]}"; do lines+=("$mark${kinds[$k]}"); done
  text "${lines[@]}"
  expect_fail "a line of kind $k after each mark" "pull-request/body:3: ${problems[$k]}" check
  i=0
  while [ "$i" -lt "${#marks[@]}" ]; do
    says "pull-request/body:$((i + 3)): ${problems[$k]}"
    i=$((i + 1))
  done
  quiet "example.com"
  k=$((k + 1))
done
# And none of them without the line: the marks alone are not refused.
lines=()
for mark in "${marks[@]}"; do lines+=("${mark}words about authors"); done
text "${lines[@]}"
expect_pass "the marks before ordinary words" check
text "words" "# ------------------------ >8 ------------------------" "$mark_credit"
expect_fail "a credit after a scissors line" "pull-request/body:5: $has_credit" check
text "nothing"
printf '%s\n' "$mark_credit" >"$title"
expect_fail "a credit as the title" "pull-request/title:1: $has_credit" check

# --- What is not refused but listed for reading: a line that says either phrase anywhere.
text "words" "+ GENERATED ""WITH a tool" "the hook refuses a co""-authored-by line" "plain"
expect_pass "phrases that are not credits" check
says "to read: pull-request/body:4"
says "to read: pull-request/body:5"
grep -a -q -F -e "to read: pull-request/body:3" -e "to read: pull-request/body:6" "$tmp/out" &&
  { cat "$tmp/out" >&2; fail "only the lines with a phrase are listed"; }

# --- Both scans, as over any text: a key (directly after a word character too), a home
# path, an address, a listed word; and the clean-room scan fails closed.
text "token = $key"
expect_fail "a key in the body" "pull-request/body:3" check
quiet "$chars"
text "token_$key"
expect_fail "a key directly after a word character" "pull-request/body:3" check
quiet "$chars"
text "see $home_path" "write to $address"
expect_fail "a home path and an address" "pull-request/body:3" check
says "pull-request/body:4"
quiet "someone"
text "it says $word here" "and $word again"
expect_fail "a listed word" "clean-room scan: 2 matching lines" check
says "secrets scan: clean"
quiet "$word"
text "nothing"
printf '%s\n' "docs: $word" >"$title"
expect_fail "a listed word in the title" "clean-room scan: 1 matching lines" check
quiet "$word"
text "nothing"
expect_fail "no word list" "WHIZBANG_WORDLIST is not set" env -u WHIZBANG_WORDLIST "$scan" --pr-text "$title" "$body"

# --- Everything at once: each is reported, with its place.
printf 'SOME_TOKEN="%s"\n' "$secret_value" >.env
text "by $N" "at ~/hideaway" "1. $mark_credit" "x = $secret_value" "token = $key" "$word"
expect_fail "everything at once" "pull-request/body:3: $names_agent" check
says "pull-request/body:4: $tilde_path"
says "pull-request/body:5: $has_credit"
says "pull-request/body:6 (the value of SOME_TOKEN in .env)"
says "pull-request/body:7"
says "clean-room scan: 1 matching lines"
for never in "$N" "hideaway" "example.com" "$secret_value" "$chars" "$word"; do quiet "$never"; done
rm .env

# --- What cannot be scanned is refused: a missing file, a folder, a NUL byte (UTF-16 is
# not plain text), one file alone.
text "nothing"
expect_fail "a body that is not there" "cannot read" "$scan" --pr-text "$title" "$pr/absent.md"
expect_fail "a title that is a folder" "cannot read" "$scan" --pr-text "$pr" "$body"
expect_fail "one file alone" "usage" "$scan" --pr-text "$title"
printf 'by %s\n' "$N" | iconv -f UTF-8 -t UTF-16 >"$body"
expect_fail "a body in UTF-16" "holds a NUL byte" check
printf 'words\0more\n' >"$body"
expect_fail "a body with a NUL byte" "holds a NUL byte" check
# A last line with no line break after it is a line.
printf 'docs: a title' >"$title"
printf 'What changed.\n\nby %s' "$N" >"$body"
expect_fail "a last line with no line break" "pull-request/body:3: $names_agent" check
printf 'What changed.\n\n1. %s' "$mark_credit" >"$body"
expect_fail "a credit on a last line with no line break" "pull-request/body:3: $has_credit" check
# A name that starts with a dash is a file.
text "by $N"
cp "$title" ./-t
cp "$body" ./-b
expect_fail "files named with a dash" "pull-request/body:3: $names_agent" "$scan" --pr-text -t -b
rm ./-t ./-b
# The files are not changed, and nothing of them is left in the repo.
text "by $N"
sum=$(cat "$title" "$body" | cksum)
expect_fail "a refused text" "$names_agent" check
[ "$(cat "$title" "$body" | cksum)" = "$sum" ] || fail "the scan leaves the two files as they were"
[ -z "$(git status --porcelain)" ] || fail "the scan leaves nothing in the repo"

finished
