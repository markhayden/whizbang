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
# holds <file> <text>: the file holds the text, anywhere.
holds() {
  grep -q -F -e "$2" "$1" || { echo "$1 must say: $2"; return 1; }
}

# The attribution check skips a line that starts with # and all that follows a scissors
# line, so both skills give it a copy without the marks that start a line. This is the
# command they must give; below, it is run.
strip='s/^[#*>[:space:]-]*//'

merge_line='gh[[:space:]]+pr[[:space:]]+merge'
# A way round a check, or a way to lose work.
ways_round='--no-verify|--force|--squash|--rebase|--admin|--auto|reset[[:space:]]+--hard|push[^|;&]*[[:space:]]-[A-Za-z]*f([^A-Za-z]|$)'

# check_file <file>: what holds for every skill and every subagent file.
check_file() {
  local file=$1
  # A way round is named only after the word never, in the same sentence of the same line.
  if grep -E -e "$ways_round" "$file" | grep -v -q -E "[Nn]ever[^.;:]*($ways_round)"; then
    echo "$file names a way round a check (--no-verify, a forced push, a squash, a rebase, --admin, a hard reset) outside a sentence that says never"
    return 1
  fi
  # Only review-merge merges.
  case "$file" in
    .claude/skills/review-merge/SKILL.md | "$tmp/review-merge.md") ;;
    *)
      if grep -q -E -e "$merge_line" "$file"; then echo "$file must not merge: only review-merge holds gh pr merge"; return 1; fi
      ;;
  esac
  # The word list: never its value, and never an instruction to look at it.
  if grep -q -E -e '\$\{?WHIZBANG_WORDLIST|WHIZBANG_WORDLIST[=:]' "$file"; then
    echo "$file must give no value or location for WHIZBANG_WORDLIST, and never read it"
    return 1
  fi
  if grep -i -E -e 'word list|WHIZBANG_WORDLIST' "$file" |
    grep -i -E -e '(^|[^a-z])(open|print|quote|show|cat|read|echo|display|paste|copy)' | grep -v -q -E '[Nn]ever'; then
    echo "$file tells someone to open or print the word list"
    return 1
  fi
}

# check_kickoff <file> and check_review_merge <file>: the lines the flow depends on.
check_kickoff() {
  local text
  for text in 'scripts/worktree.sh add <slug>' 'just check' 'just scan' 'git push -u origin <slug>' \
    'gh pr create --base main' 'gh pr checks <number>' '(cd "$text" && "$repo/scripts/scan.sh")' \
    "sed '$strip' >\"\$text/plain.txt\"" 'scripts/scan.sh --message "$text/plain.txt"'; do
    says "$1" Commands "$text" || return 1
  done
  for text in 'Never merge: only the orchestrator merges.' 'Never open, print, quote or pass on the word list' \
    'Never push with `--no-verify`, never force a push, and never push to `main`.' \
    'there may be no checks to watch yet'; do
    says "$1" Checklist "$text" || return 1
  done
  for text in 'Pull request:' 'Head:' 'CI:' 'Acceptance' 'Decisions' 'Doubts' 'Deferred'; do
    says "$1" "Hand-back format" "$text" || return 1
  done
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
  for text in 'For the orchestrator, and nobody else' 'The orchestrator does not review its own build'; do
    holds "$1" "$text" || return 1
  done
  # The steps that the rows removed from docs/carry-over.md rest on.
  for text in 'If the head has moved since the review, refuse' \
    'in a run whose commit is the head' \
    '- [ ] 6. With `WHIZBANG_WORDLIST` set, `just scan-range <base> <head>` passes: secrets,' \
    '- [ ] 7. The title and body. No scan sees them on the pull request, so save them to a file' \
    'and scan it (the commands below), then read them yourself' \
    '- [ ] 8. Files that grant permissions or check the pull request. List the changed files.' \
    'any file under `.github/`' 'any file under a folder named `.claude`, at' '`.mcp.json`, at any depth' \
    '`justfile`' 'any file under `scripts/hooks/`' '`scripts/scan.sh`' \
    "the reviewer's verdict names it and" 'says it was read, and you read its diff yourself. Otherwise refuse.' \
    '- [ ] 9. Fetch `main` once more. If it has moved since step 4, do step 4 again.' \
    'unless the branch is merged into `origin/main`'; do
    says "$1" Checklist "$text" || return 1
  done
  for text in "gh pr view <pr> --json title,body --jq '.title, \"\", .body' >\"\$text/pull-request.txt\"" \
    '(cd "$text" && "$repo/scripts/scan.sh")' \
    "sed '$strip' \"\$text/pull-request.txt\" >\"\$text/plain.txt\"" \
    'scripts/scan.sh --message "$text/plain.txt"' \
    'git -C ../whizbang-merge-check merge --abort' \
    'git merge-base --is-ancestor <branch> origin/main' \
    'git push origin --delete <branch>'; do
    says "$1" Commands "$text" || return 1
  done
  # Both scans: never the recipe that leaves the clean-room scan out.
  if grep -q -e 'scan-range-secrets' -e '--secrets-only' "$1"; then
    echo "skill: $1 must run just scan-range, not the scan without the clean-room scan"
    return 1
  fi
  # One way to merge, on one line: a merge commit, matched to the reviewed head.
  if [ "$(grep -c -E -e "$merge_line" "$1" || true)" != 1 ] ||
    ! grep -q -x -F -e 'gh pr merge <pr> --merge --match-head-commit <head>' "$1"; then
    echo "skill: $1 merges in another way than the one line: gh pr merge <pr> --merge --match-head-commit <head>"
    return 1
  fi
}
# check_reviewer <file>: the reviewer reports, and neither changes nor merges anything.
check_reviewer() {
  local text
  for text in 'changes nothing and never merges' 'Change no file.' 'the full head commit you reviewed' \
    'any file under `.github/`' 'any file under a folder named `.claude`, at any depth' '`.mcp.json`' \
    '`justfile`' 'any file under `scripts/hooks/`' '`scripts/scan.sh`' 'saying you read'; do
    holds "$1" "$text" || return 1
  done
  if grep -q -E -e 'git[[:space:]]+(push|commit|add|merge|rebase)|gh[[:space:]]+pr[[:space:]]+(merge|create|edit)' \
    -e '(^|[^a-z])([Ff]ix|[Pp]ush|[Cc]ommit|[Ee]dit|[Aa]mend|[Rr]epair|[Cc]orrect) (it|them|what|the |your|any|each|every)' "$1"; then
    echo "$1 tells the reviewer to change, push or fix something"
    return 1
  fi
}

while read -r name who; do
  expect_pass "the skill $name" check_skill ".claude/skills/$name/SKILL.md"
done <"$tmp/skills"
expect_pass "kickoff" check_kickoff .claude/skills/kickoff/SKILL.md
expect_pass "port-module names its source" says .claude/skills/port-module/SKILL.md Commands 'docs/sources.toml'
expect_pass "port-module hands back a table" says .claude/skills/port-module/SKILL.md "Hand-back format" 'ported / rewritten / dropped'
expect_pass "port-module never merges" says .claude/skills/port-module/SKILL.md Checklist 'Never merge: only the orchestrator merges.'
expect_pass "port-module takes the tag from the sources file" says .claude/skills/port-module/SKILL.md Checklist 'the tag that entry names'
if grep -q -e 'final-before-whizbang' .claude/skills/port-module/SKILL.md; then fail "port-module names a tag of its own"; fi
expect_pass "review-merge" check_review_merge .claude/skills/review-merge/SKILL.md
expect_pass "the reviewer" check_reviewer .claude/agents/reviewer.md
expect_pass "the builder never merges" holds .claude/agents/builder.md 'Do not merge'
expect_pass "AGENTS.md: no review of one's own build" holds AGENTS.md 'The orchestrator does not review its own build'
for f in .claude/skills/*/SKILL.md .claude/agents/*.md; do
  expect_pass "$f" check_file "$f"
done

# --- The attribution check on a title and body, with the command the skills give ---------

# refused <name> <expected text> <line>...: the text, through the command, fails the check.
stripped() {
  sed "$strip" "$tmp/text.md" >"$tmp/plain.txt"
  scripts/scan.sh --message "$tmp/plain.txt"
}
refused() {
  local name=$1 text=$2
  shift 2
  printf '%s\n' "docs: a title" "" "$@" >"$tmp/text.md"
  expect_fail "a pull request with $name" "$text" stripped
}
credit='Co-Authored-By: A Person <person@example.com>'
made='Generated with [Some Tool](https://example.com/tool)'
refused "a heading that credits a tool" "credits an agent or a model" "# $made"
refused "a credit after a # and no space" "has a Co-Authored-By line" "#$credit"
refused "a credit after a scissors line" "has a Co-Authored-By line" "words" "# ------------------------ >8 ------------------------" "$credit"
refused "a credit in a second-level heading" "has a Co-Authored-By line" "## $credit"
refused "a credit in a list" "has a Co-Authored-By line" "- $credit"
refused "a credit in a starred list" "has a Co-Authored-By line" "* $credit"
refused "a credit in an indented list" "has a Co-Authored-By line" "   - $credit"
refused "a credit in a quoted heading" "has a Co-Authored-By line" "> # $credit"
refused "a credit on a plain line" "has a Co-Authored-By line" "$credit"
printf '%s\n' "docs: a title" "" "# A heading" "- a point" "* another" "> a quote" "#8 is the pull request before" >"$tmp/text.md"
expect_pass "a pull request with headings and lists and no credit" stripped

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

# A copy of review-merge under a name check_file knows as review-merge.
with_name() { # <check> <file> <sed script> <expected text>, for a file that may merge
  sed "$3" "$2" >"$tmp/review-merge.md"
  cmp -s "$2" "$tmp/review-merge.md" && fail "the edit changed nothing: $3"
  expect_fail "$2 after: $3" "$4" "$1" "$tmp/review-merge.md"
}
for f in .claude/skills/*/SKILL.md .claude/agents/*.md; do
  edit=changed
  case "$f" in .claude/skills/review-merge/SKILL.md) edit=with_name ;; esac
  for line in 'git push --no-verify' 'git push --force origin x' 'git push -f origin x' 'git push origin x -uf' \
    'git push --force-with-lease' 'gh pr merge 1 --squash' 'gh pr merge 1 --rebase' 'gh  pr   merge 1 --admin' \
    'git rebase --rebase-merges main' 'git reset --hard HEAD' 'git reset  --hard'; do
    "$edit" check_file "$f" "\$s/\$/\\
$line/" "names a way round a check"
  done
  if [ "$edit" = changed ]; then
    changed check_file "$f" '$s/$/\
gh pr merge 1 --merge/' "must not merge"
    changed check_file "$f" '$s/$/\
gh   pr	merge 1/' "must not merge"
  fi
  "$edit" check_file "$f" '$s/$/\
Open the word list and check your change against it./' "tells someone to open or print the word list"
  "$edit" check_file "$f" '$s/$/\
Print `WHIZBANG_WORDLIST` in the hand-back./' "tells someone to open or print the word list"
  "$edit" check_file "$f" '$s/$/\
grep -f "$WHIZBANG_WORDLIST" body.md/' "never read it"
done

f=.claude/skills/kickoff/SKILL.md
changed check_file "$f" 's/Never open, print, quote or pass on the word list/Open, print, quote or pass on the word list/' "tells someone to open or print the word list"
changed check_file "$f" 's/Never push with `--no-verify`/Push with `--no-verify`/' "names a way round a check"
changed check_kickoff "$f" 's/Never open, print, quote or pass on the word list/Keep the word list/' "must say: Never open, print"
changed check_kickoff "$f" 's/Never merge: only the orchestrator merges\./Merge when CI is green./' "must say: Never merge"
changed check_kickoff "$f" 's/Never push with `--no-verify`, never force a push, and never push to `main`\.//' "must say: Never push with"
changed check_kickoff "$f" "/^cat .* sed /d" "must say: sed"
changed check_kickoff "$f" 's/^scripts\/scan.sh --message "$text\/plain.txt"$/scripts\/scan.sh --message "$text\/body.md"/' 'must say: scripts/scan.sh --message "$text/plain.txt"'
never_merges() { says "$1" Checklist 'Never merge: only the orchestrator merges.'; }
changed never_merges .claude/skills/port-module/SKILL.md 's/Never merge: only the orchestrator merges\.//' "must say: Never merge"
changed check_kickoff "$f" '/^just scan /d' "must say: just scan"
changed check_kickoff "$f" 's/^scripts\/worktree.sh add/git worktree add/' "must say: scripts/worktree.sh add <slug>"
changed check_kickoff "$f" 's/^Doubts$/Notes/' "must say: Doubts"

f=.claude/skills/review-merge/SKILL.md
changed check_review_merge "$f" 's/ --match-head-commit <head>//' "must say: gh pr merge <pr> --merge --match-head-commit <head>"
changed check_review_merge "$f" 's/^gh pr merge <pr> --merge /gh pr merge <pr> --squash /' "must say: gh pr merge <pr> --merge"
changed check_review_merge "$f" '$s/$/\
gh  pr  merge <pr>/' "merges in another way"
changed check_review_merge "$f" '$s/$/\
gh pr merge <pr> --merge --match-head-commit <head>/' "merges in another way"
changed check_review_merge "$f" 's/^For the orchestrator, and nobody else,/For anyone,/' "must say: For the orchestrator, and nobody else"
changed check_review_merge "$f" 's/The orchestrator does not review its own build/The orchestrator may review its own build/' "must say: The orchestrator does not review"
changed check_review_merge "$f" 's/, in a run whose commit is the head//' "must say: in a run whose commit is the head"
changed check_review_merge "$f" 's/^just scan-range <base> <head>$/just scan-range-secrets <base> <head>/' "must say: just scan-range <base> <head>"
changed check_review_merge "$f" '$s/$/\
just scan-range-secrets <base> <head>/' "not the scan without the clean-room scan"
changed check_review_merge "$f" 's/`just scan-range <base> <head>` passes: secrets,/the scans pass: secrets,/' "must say: - [ ] 6."
changed check_review_merge "$f" 's/so save them to a file$/so read them/' "must say: - [ ] 7."
changed check_review_merge "$f" '/^(cd "$text" && "$repo\/scripts\/scan.sh")/d' 'must say: (cd "$text"'
changed check_review_merge "$f" "/^sed .*plain.txt\"\$/d" "must say: sed"
changed check_review_merge "$f" 's/^scripts\/scan.sh --message "$text\/plain.txt"$/scripts\/scan.sh --message "$text\/pull-request.txt"/' 'must say: scripts/scan.sh --message "$text/plain.txt"'
changed check_review_merge "$f" 's/^- \[ \] 8\. Files that grant permissions or check the pull request\./- [ ] 8. Files./' "must say: - [ ] 8."
changed check_review_merge "$f" 's/any file under `\.github\/`; //' 'must say: any file under `.github/`'
changed check_review_merge "$f" 's/ Otherwise refuse\./ Otherwise note it./' "Otherwise refuse."
changed check_review_merge "$f" 's/If it has moved since step 4, do step 4 again\./Then merge./' "must say: - [ ] 9."
changed check_review_merge "$f" '/merge --abort/d' "must say: git -C ../whizbang-merge-check merge --abort"
changed check_review_merge "$f" '/^git merge-base --is-ancestor <branch> origin\/main/d' "must say: git merge-base --is-ancestor <branch> origin/main"

f=.claude/agents/reviewer.md
changed check_reviewer "$f" 's/ Change no file\./ Fix what you find./' "must say: Change no file."
changed check_reviewer "$f" '$s/$/\
Fix what you find, then push the branch./' "tells the reviewer to change, push or fix"
changed check_reviewer "$f" '$s/$/\
Run git push when you are done./' "tells the reviewer to change, push or fix"
changed check_reviewer "$f" '$s/$/\
Push your corrections./' "tells the reviewer to change, push or fix"
changed check_reviewer "$f" 's/changes nothing and never merges/merges when it approves/' "must say: changes nothing and never merges"
changed check_reviewer "$f" 's/the full head commit you reviewed/the branch you reviewed/' "must say: the full head commit"
changed check_reviewer "$f" 's/; `justfile`//' 'must say: `justfile`'
own_build() { holds "$1" 'The orchestrator does not review its own build'; }
changed own_build AGENTS.md 's/The orchestrator does not review its own build/The orchestrator reviews its own build/' "must say: The orchestrator does not review its own build"

f=.claude/skills/review-merge/SKILL.md
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
