#!/usr/bin/env bash
# The skills (.claude/skills/<name>/SKILL.md) are checklists with fixed parts: each names its
# inputs, its checklist, its commands and its hand-back format, under those headings and in
# that order. AGENTS.md lists every skill with who uses it, and scripts/worktree.sh among
# the commands. A few lines the merge depends on are held in review-merge and kickoff.
# What a skill may hold in its frontmatter is in scripts/tests/test-rules.sh.
source "$(dirname "$0")/lib.sh"

# The skills there are, and who uses each.
cat >"$tmp/skills" <<'LIST'
kickoff Builder
port-module Builder
review-merge Orchestrator
LIST

ls .claude/skills >"$tmp/found"
cut -d' ' -f1 "$tmp/skills" | diff - "$tmp/found" >"$tmp/diff" ||
  { cat "$tmp/diff" >&2; fail "the folders under .claude/skills are not the skills listed in scripts/tests/test-skills.sh"; }

# section <file> <heading>: the lines under a `## ` heading.
section() { awk -v h="## $2" '/^## / { on = ($0 == h); next } on' "$1"; }

# check_skill <file>: prints what a skill is missing and fails, or passes.
check_skill() {
  local file=$1 heading
  [ "$(grep '^## ' "$file" | tr '\n' ';')" = "## Inputs;## Checklist;## Commands;## Hand-back format;" ] ||
    { echo "skill: $file must have these sections, in this order, and no other: Inputs, Checklist, Commands, Hand-back format"; return 1; }
  section "$file" Inputs | grep -q '^- \*\*[A-Z]' ||
    { echo "skill: $file names no input (a line that starts: - **Name:**)"; return 1; }
  [ "$(section "$file" Checklist | grep -c '^- \[ \] [0-9]*\. ' || true)" -ge 5 ] ||
    { echo "skill: $file has no checklist of numbered boxes"; return 1; }
  section "$file" Commands | awk '/^```sh$/ { on = 1; next } /^```$/ { on = 0 } on && /^[a-z(]/ { found = 1 } END { exit !found }' ||
    { echo "skill: $file gives no command in a sh block"; return 1; }
  section "$file" "Hand-back format" | awk '/^```text$/ { on = 1; next } /^```$/ { on = 0 } on && /[^ ]/ { found = 1 } END { exit !found }' ||
    { echo "skill: $file gives no hand-back format in a text block"; return 1; }
  # The word list is named by its variable only.
  if grep -q -E 'WHIZBANG_WORDLIST[=:]' "$file"; then
    echo "skill: $file must give no value or location for WHIZBANG_WORDLIST"
    return 1
  fi
}

# says <file> <section> <text>: the section holds the text.
says() {
  section "$1" "$2" | grep -q -F -e "$3" || { echo "skill: $1, under $2, must say: $3"; return 1; }
}
# check_kickoff <file> and check_review_merge <file>: the lines the flow depends on.
check_kickoff() {
  local text
  for text in 'scripts/worktree.sh add <slug>' 'just check' 'just scan' 'git push -u origin <slug>' \
    'gh pr create --base main' 'gh pr checks <number>'; do
    says "$1" Commands "$text" || return 1
  done
  for text in 'Pull request:' 'Head:' 'CI:' 'Acceptance' 'Decisions' 'Doubts' 'Deferred'; do
    says "$1" "Hand-back format" "$text" || return 1
  done
  if grep -q -e 'gh pr merge' "$1"; then echo "skill: $1 must not merge"; return 1; fi
}
check_review_merge() {
  local text
  for text in 'headRefOid' 'git merge-base origin/main <head>' 'git merge-base --is-ancestor origin/main <head>' \
    'just check' 'gh pr checks <pr>' 'just scan-range <base> <head>' 'gh pr diff <pr> --name-only' \
    'gh pr merge <pr> --merge --match-head-commit <head>' 'mergeCommit' 'scripts/worktree.sh remove <slug>'; do
    says "$1" Commands "$text" || return 1
  done
  for text in 'Reviewed head:' 'Base:' 'Merge commit:' 'Verified' 'Read:' 'Follow-ups'; do
    says "$1" "Hand-back format" "$text" || return 1
  done
  for text in 'If the head has moved since the review, refuse' 'The orchestrator does not review its own build'; do
    grep -q -F -e "$text" "$1" || { echo "skill: $1 must say: $text"; return 1; }
  done
  # One way to merge: a merge commit, matched to the reviewed head.
  if grep -e 'gh pr merge' "$1" | grep -v -q -x -F -e 'gh pr merge <pr> --merge --match-head-commit <head>'; then
    echo "skill: $1 merges in another way than: gh pr merge <pr> --merge --match-head-commit <head>"
    return 1
  fi
}

while read -r name who; do
  expect_pass "the skill $name" check_skill ".claude/skills/$name/SKILL.md"
done <"$tmp/skills"
expect_pass "kickoff" check_kickoff .claude/skills/kickoff/SKILL.md
expect_pass "port-module names its source" says .claude/skills/port-module/SKILL.md Commands 'docs/sources.toml'
expect_pass "port-module hands back a table" says .claude/skills/port-module/SKILL.md "Hand-back format" 'ported / rewritten / dropped'
expect_pass "review-merge" check_review_merge .claude/skills/review-merge/SKILL.md

# changed <check> <file> <sed script> <expected text>: the file after the edit must fail.
changed() {
  sed "$3" "$2" >"$tmp/changed.md"
  cmp -s "$2" "$tmp/changed.md" && fail "the edit changed nothing: $3"
  expect_fail "$2 after: $3" "$4" "$1" "$tmp/changed.md"
}
while read -r name who; do
  f=.claude/skills/$name/SKILL.md
  changed check_skill "$f" '/^## Inputs$/d' "must have these sections"
  changed check_skill "$f" '/^## Commands$/d' "must have these sections"
  changed check_skill "$f" 's/^## Hand-back format$/## Report/' "must have these sections"
  changed check_skill "$f" 's/^## Checklist$/## Steps/' "must have these sections"
  changed check_skill "$f" '$s/$/\
## More/' "must have these sections"
  changed check_skill "$f" 's/^- \*\*/- /' "names no input"
  changed check_skill "$f" 's/^- \[ \] /- /' "no checklist of numbered boxes"
  changed check_skill "$f" 's/^```sh$/```/' "gives no command in a sh block"
  changed check_skill "$f" 's/^```text$/```/' "gives no hand-back format in a text block"
  changed check_skill "$f" '$s|$| WHIZBANG_WORDLIST=words|' "no value or location"
done <"$tmp/skills"

f=.claude/skills/kickoff/SKILL.md
changed check_kickoff "$f" '/^just scan /d' "must say: just scan"
changed check_kickoff "$f" 's/^scripts\/worktree.sh add/git worktree add/' "must say: scripts/worktree.sh add <slug>"
changed check_kickoff "$f" 's/^Doubts$/Notes/' "must say: Doubts"
changed check_kickoff "$f" '$s/$/\
gh pr merge 1/' "must not merge"

f=.claude/skills/review-merge/SKILL.md
changed check_review_merge "$f" 's/ --match-head-commit <head>//' "must say: gh pr merge <pr> --merge --match-head-commit <head>"
changed check_review_merge "$f" 's/^gh pr merge <pr> --merge /gh pr merge <pr> --squash /' "must say: gh pr merge <pr> --merge"
changed check_review_merge "$f" '$s/$/\
gh pr merge <pr> --squash/' "merges in another way"
changed check_review_merge "$f" '$s/$/\
gh pr merge <pr> --merge --match-head-commit <head> --admin/' "merges in another way"
changed check_review_merge "$f" '/^just scan-range <base> <head>$/d' "must say: just scan-range <base> <head>"
changed check_review_merge "$f" 's/^just scan-range <base> <head>$/just scan/' "must say: just scan-range <base> <head>"
changed check_review_merge "$f" 's/If the head has moved since the review, refuse/If the head has moved, carry on/' "must say: If the head has moved"
changed check_review_merge "$f" '/^git merge-base --is-ancestor origin\/main <head>$/d' "must say: git merge-base --is-ancestor"
changed check_review_merge "$f" 's/^Merge commit: /Merged: /' "must say: Merge commit:"
changed check_review_merge "$f" 's/^- Read: /- /' "must say: Read:"

# --- AGENTS.md ---------------------------------------------------------------------------

# check_listed <file>: the rules list every skill with who uses it, and the worktree script.
check_listed() {
  local file=$1 name who
  while read -r name who; do
    grep -q -e "^| \`$name\` | $who | [A-Z]" "$file" ||
      { echo "rules: the Skills table must have a row: | \`$name\` | $who | when |"; return 1; }
  done <"$tmp/skills"
  [ "$(section "$file" Skills | grep -c '^| `')" = "$(grep -c . "$tmp/skills")" ] ||
    { echo "rules: the Skills table lists a skill that is not under .claude/skills"; return 1; }
  section "$file" Commands | grep -q -F -e '| `scripts/worktree.sh add <slug>` |' ||
    { echo "rules: the Commands table must have a row for scripts/worktree.sh add <slug>"; return 1; }
}
expect_pass "AGENTS.md lists the skills" check_listed AGENTS.md
while read -r name who; do
  changed check_listed AGENTS.md "/^| \`$name\` |/d" "the Skills table must have a row: | \`$name\`"
done <"$tmp/skills"
changed check_listed AGENTS.md 's/^| `review-merge` | Orchestrator |/| `review-merge` | Builder |/' "the Skills table must have a row"
changed check_listed AGENTS.md 's/^| `kickoff` | Builder |.*$/&\
| `another` | Builder | Never |/' "lists a skill that is not under .claude/skills"
changed check_listed AGENTS.md '/^| `scripts\/worktree.sh add <slug>` |/d' "must have a row for scripts/worktree.sh"

finished
