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
  section "$file" Inputs | grep '^- \*\*[A-Z]' >/dev/null ||
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
# After a pipe, no grep here is given -q: it would stop at its first match, the command
# before it would be killed while still writing, and `pipefail` would turn a match into a
# failure. That happened on the Linux runner and not on macOS.
says() {
  section "$1" "$2" | grep -F -e "$3" >/dev/null || { echo "skill: $1, under $2, must say: $3"; return 1; }
}
# holds <file> <text>: the file holds the text, anywhere.
holds() {
  grep -q -F -e "$2" "$1" || { echo "$1 must say: $2"; return 1; }
}

# The one scan of a pull request's title and body (`scripts/scan.sh --pr-text`): both
# scans, and the attribution check made for text. This is the command both skills must
# give, as a line of its own; below, it is run.
pr_scan='scripts/scan.sh --pr-text "$text/title.txt" "$text/body.md"'
# pr_scan_alone <file>: the command is a whole line there, once, and the scan of a pull
# request's text is run in no other way: not without the clean-room scan, and not by the
# commands it replaced, which did not see the values of the local .env.
pr_scan_alone() {
  [ "$(grep -c -x -F -e "$pr_scan" "$1" || true)" = 1 ] ||
    { echo "skill: $1 must say, as one whole line and once: $pr_scan"; return 1; }
  if grep -n -e '--pr-text' "$1" | grep -v -x -E -e "[0-9]+:$(printf '%s' "$pr_scan" | sed 's/[][\.*^$]/\\&/g')" >/dev/null; then
    echo "skill: $1 scans a pull request's text in another way than the one line: $pr_scan"
    return 1
  fi
  if grep -q -e '--secrets-only' -e 'scan.sh --message' -e '"$repo/scripts/scan.sh"' "$1"; then
    echo "skill: $1 must scan a pull request's text with the one line, not without the clean-room scan and not with the commands it replaced"
    return 1
  fi
  # The scan comes before every command that puts the text on a pull request.
  local at put
  at=$(grep -n -x -F -e "$pr_scan" "$1" | cut -d: -f1)
  put=$(grep -n -E -e '^gh[[:space:]]+pr[[:space:]]+(create|edit)([[:space:]]|$)' "$1" | head -n 1 | cut -d: -f1) || true
  [ -n "$put" ] || { echo "skill: $1 has no command that puts the text on a pull request"; return 1; }
  [ "$at" -lt "$put" ] ||
    { echo "skill: $1 must scan the title and body before the first command that puts them on a pull request (gh pr create, gh pr edit)"; return 1; }
}

merge_line='gh[[:space:]]+pr[[:space:]]+merge'
# A way round a check, or a way to lose work.
ways_round='--no-verify|--force|--squash|--rebase|--admin|--auto|reset[[:space:]]+--hard|push[^|;&]*[[:space:]]-[A-Za-z]*f([^A-Za-z]|$)'

# check_file <file>: what holds for every skill and every subagent file.
check_file() {
  local file=$1
  # A way round is named only after the word never, in the same sentence of the same line.
  if grep -E -e "$ways_round" "$file" | grep -v -E "[Nn]ever[^.;:]*($ways_round)" >/dev/null; then
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
    grep -i -E -e '(^|[^a-z])(open|print|quote|show|cat|read|echo|display|paste|copy)' | grep -v -E '[Nn]ever' >/dev/null; then
    echo "$file tells someone to open or print the word list"
    return 1
  fi
}

# own_build_plain <file>: the sentence that says so makes no exception.
own_build_plain() {
  if tr '\n' ' ' <"$1" | grep -o 'The orchestrator does not review its own build[^.]*\.' | grep -i -E 'unless|except' >/dev/null; then
    echo "$1: the sentence about the orchestrator's own build makes an exception"
    return 1
  fi
}

# check_kickoff <file> and check_review_merge <file>: the lines the flow depends on.
check_kickoff() {
  local text
  for text in 'scripts/worktree.sh add <slug>' 'just check' 'just scan' 'git push -u origin <slug>' \
    'gh pr create --base main' 'gh pr checks <number>' "$pr_scan" \
    'It says where, never what.' 'Exit 0 is the pass.' \
    'A line it prints that starts "to read:" is not refused: go through it.' \
    "git rev-parse '<base>^{commit}'" 'git rev-parse origin/main' \
    'just test-only -p <crate>' 'bash scripts/tests/<name>.sh' \
    'git reset --soft <good>' \
    'printf '"'%s\\n'"' "type(scope): summary" >"$text/title.txt"' 'cat >"$text/body.md" <<' \
    'cat "$text/title.txt" "$text/body.md"          # read first' \
    'gh pr edit <number> --title "$(cat "$text/title.txt")" --body-file "$text/body.md"' \
    'until run=$(gh run list --commit "$head" --json databaseId' \
    '.jobs | length'"'"')" -ge 3 ]; do' '[ "$tries" -le 60 ] ||' \
    'git merge --no-ff --no-edit origin/main' 'git status --short' 'git merge --abort' \
    'gh pr checks <number> --watch                  # check (macos-15), check (ubuntu-22.04), ci'; do
    says "$1" Commands "$text" || return 1
  done
  pr_scan_alone "$1" || return 1
  says "$1" Inputs 'It may be given short: resolve it to the full commit with `git rev-parse`' || return 1
  for text in 'Never merge a pull request, and never merge into `main`: only the orchestrator does.' \
    'Never open, print, quote or copy the word list, and never pass on what it holds.' \
    'The path may be given to another agent working in this repo.' \
    "does not last from one of an agent's shell calls to the next" \
    '`.claude/settings.local.json`, which stays untracked' \
    'exact allowed commands and asks first every time' \
    'Never push with `--no-verify`, never force a push, and never push to `main`.' \
    'Finding no row that names the task is normal.' \
    "from inside the repo's main checkout" \
    'The script test is not on the allow list, so it asks first' 'only through `whizbang_testkit::threads`' \
    'makes true once it merges' 'is ticked by a later pull request' \
    "a pass covers a commit only when it ran on exactly that commit's tree" \
    'after the commit `git status --short`' 'has passed on exactly the tree being pushed' \
    'The path is never written into a hand-back, a pull request, a commit or a tracked file.' \
    'everything at once. The message is `type(scope): summary`. No `Co-Authored-By`, no' \
    '"Generated with", no agent or model name, in a message, in code or in a document; this' \
    'commit only' 'resolve it only in your own files; when it is in a file you did not change, stop,' \
    'abort the merge, and say why in the hand-back.' \
    'the clean-room scan prints a count and names' 'every commit that is not yet on origin' \
    'bring them back, by the reflog or by a cherry-pick' 'owner at once and is rotated' \
    'It gives up after 20' 'gets no run at all' 'give the call a longer time limit' \
    'merge `origin/main` into your' 'with a merge commit (this asks first), never with a rebase' \
    '`<commit>:<path>:<line>`' 'Never retry around the hook, and never use `--no-verify`.' \
    'the system temp folder may be used for this' 'After any edit to either file, read' \
    'a name it does' 'not know is found only by reading, and so is a credit behind a mark made of letters' \
    'which it only lists' 'and is not refused: go through those. Then' \
    'So first go through the two files against "Secrets and private data"' \
    'address. Then scan them (the commands below).' \
    'After any edit to either file, read and scan again.' \
    "or a model's name that it knows (but for the folder \`.claude\` and the file" \
    '`CLAUDE.md` of this repo, named as paths)' \
    '`check (macos-15)` and `check (ubuntu-22.04)`' 'says "no checks reported"' \
    'a run whose commit is the head lists all three' \
    'go back to step 4' 'hand back again with the new commit'; do
    says "$1" Checklist "$text" || return 1
  done
  for text in 'Pull request:' 'Head:' 'Acceptance' 'Decisions' 'Doubts' 'Deferred' 'Refused push:' \
    'CI: check (macos-15) <result>, check (ubuntu-22.04) <result>, ci <result>'; do
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
  for text in 'Reviewed head:' 'Base:' 'Merge commit:' 'Verified' 'Read:' 'Follow-ups' \
    '- Verdict: may merge at the reviewed head; no finding that blocks the merge is open' \
    'check (macos-15) <result>, check (ubuntu-22.04) <result>, ci <result>' \
    'corrected by the orchestrator' 'pull request of mine that adds its row to docs/carry-over.md'; do
    says "$1" "Hand-back format" "$text" || return 1
  done
  for text in 'For the orchestrator, and nobody else' 'The orchestrator does not review its own build'; do
    holds "$1" "$text" || return 1
  done
  own_build_plain "$1" || return 1
  # The steps that the rows removed from docs/carry-over.md rest on.
  for text in 'If the head has moved since the review, refuse' \
    '- [ ] 1. The verdict says `may merge at <full head>`, and the commit it names is exactly' \
    'A finding that blocks the merge and is still open, or a verdict for another commit, is a' \
    'A follow-up does not block' \
    '- [ ] 9. Follow-ups are recorded by you; the reviewer only names them.' \
    'a new head that is reviewed again' 'Nothing is left only in a verdict.' \
    'The corrected files get every check of this step again' \
    '`check (macos-15)` and `check (ubuntu-22.04)`' \
    'You may correct the title or body yourself' \
    'A correction that changes a claim about the code goes back to the builder.' \
    'never past a failing check' \
    'your hand-back to the owner is the record of each follow-up' 'before you start other work' \
    'Earlier merged branches that are still on origin may be deleted the same way' \
    'in a run whose commit is the head' \
    '- [ ] 6. With `WHIZBANG_WORDLIST` set, `just scan-range <base> <head>` passes: secrets,' \
    '- [ ] 7. The title and body. No scan sees them on the pull request, so save them to a file' \
    'and scan it (the commands below), then read them yourself' \
    'folder `.claude` and the file `CLAUDE.md` of this repo, named as paths), a path that' \
    'a name it does not know is found only by reading, and so' 'lists. It lists each line that says "co-authored-by" or "generated with" and is not' \
    'refused: go through those.' 'lines it lists, and are gone through again,' \
    'The corrected files get every check of this step again, the scan and the' \
    '- [ ] 8. Files that grant permissions or check the pull request. List the changed files.' \
    'any file under `.github/`' 'any file under a folder named `.claude`, at' '`.mcp.json`, at any depth' \
    '`justfile`' 'any file under `scripts/`' \
    "the reviewer's verdict names it and" 'says it was read, and you read its diff yourself. Otherwise refuse.' \
    '- [ ] 10. Fetch `main` once more. If it has moved since step 4, do step 4 again.' \
    'unless the branch is merged into `origin/main`'; do
    says "$1" Checklist "$text" || return 1
  done
  for text in 'gh pr view <pr> --json title --jq .title >"$text/title.txt"' \
    'gh pr view <pr> --json body --jq .body >"$text/body.md"' \
    'After a correction to either file, it runs again on the two files.' \
    'A variable' 'gh run list --commit <head>' \
    "$pr_scan" 'It says where, never what.' 'Exit 0 is the pass.' \
    'A line it prints that starts "to read:" is not refused: go through it.' \
    'gh pr edit <pr> --title "$(cat "$text/title.txt")" --body-file "$text/body.md"' \
    "git diff <base> <head> -- .github justfile scripts ':(glob)**/.claude/**' ':(glob)**/.mcp.json'" \
    'git merge-base --is-ancestor origin/<branch> origin/main' \
    'git -C ../whizbang-merge-check merge --abort' \
    'git merge-base --is-ancestor <branch> origin/main' \
    'git push origin --delete <branch>'; do
    says "$1" Commands "$text" || return 1
  done
  pr_scan_alone "$1" || return 1
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
    '`justfile`' 'any file under `scripts/`' 'saying you read' \
    'git fetch origin' 'git status --short' 'git rev-parse HEAD' 'git rev-parse origin/<branch>' 'headRefOid' \
    'git merge-base origin/main HEAD' 'git diff origin/main...HEAD' 'git log origin/main..HEAD' \
    'scripts/heavy.sh check just check' 'just scan-range <base> <head>' 'gh pr checks <pr>' \
    'gh run list --commit <head>' \
    "in the builder's worktree" \
    'so you need `WHIZBANG_WORDLIST`' 'Never open, print, quote or copy the word list, and never pass on what it holds.' \
    '**blocks the merge**' '**follow-up**' 'may merge at <full head>' \
    'the orchestrator records it' 'none is left only in your report' \
    '"may not merge at <full head>"' 'Nothing is committed or pushed by you.' \
    'A **follow-up** does not block.' 'without the command that puts the text on the pull' \
    'Both CI commands are needed.' \
    'The path is never written into a report, a pull request, a commit or a tracked file.' \
    'If you wrote any commit on this branch, stop: you cannot review it.' \
    '- **Pull request:**' '- **Worktree:**' "- **The builder's hand-back:**" \
    'Change into it in every shell call' 'is not one of the exact allowed commands and asks first' \
    'which is not on the allow list' 'A scratch clone in the system temp' \
    'While it is still running, wait for it. A red job is a finding that blocks' \
    'that is step 4 of `.claude/skills/review-merge/SKILL.md`' \
    'step 7 of `review-merge`' 'settles it without moving the head' \
    'the one scan of the two files, and each line it' 'lists to read' \
    'what the branch adds' 'a tripwire, not a proof' \
    'is not a false claim' "The pull request's title and body" \
    'the decision behind it is recorded in the pull request body' \
    'the acceptance is the text of the box and any decision recorded in the pull request body' \
    'No work is done twice'; do
    holds "$1" "$text" || return 1
  done
  # `git merge-base` asks a question; `git merge` changes something.
  if grep -q -E -e 'git[[:space:]]+(push|commit|add|merge|rebase)([^-]|$)|gh[[:space:]]+pr[[:space:]]+(merge|create|edit)' \
    -e '(^|[^a-z])([Ff]ix|[Pp]ush|[Cc]ommit|[Ee]dit|[Aa]mend|[Rr]epair|[Cc]orrect) (it|them|what|the |your|any|each|every)' "$1"; then
    echo "$1 tells the reviewer to change, push or fix something"
    return 1
  fi
}

if grep -n -E '\|[[:space:]]*grep[^|]*[[:space:]]-[A-Za-z]*q' "$0" | grep -v -F 'grep -n -E' >"$tmp/quiet"; then
  cat "$tmp/quiet" >&2
  fail "a grep -q after a pipe in $0: with pipefail it fails at random (see says)"
fi

while read -r name who; do
  expect_pass "the skill $name" check_skill ".claude/skills/$name/SKILL.md"
done <"$tmp/skills"
expect_pass "kickoff" check_kickoff .claude/skills/kickoff/SKILL.md
expect_pass "port-module names its source" says .claude/skills/port-module/SKILL.md Commands 'docs/sources.toml'
expect_pass "port-module hands back a table" says .claude/skills/port-module/SKILL.md "Hand-back format" 'ported / rewritten / dropped'
last_step=$(section .claude/skills/kickoff/SKILL.md Checklist | grep -c '^- \[ \] [0-9]*\. ')
expect_pass "port-module finishes with the last step of kickoff" says .claude/skills/port-module/SKILL.md Checklist "Finish with \`kickoff\`, steps 6 to $last_step."
expect_pass "port-module never merges" says .claude/skills/port-module/SKILL.md Checklist 'Never merge a pull request, and never merge into `main`: only the orchestrator does.'
expect_pass "port-module takes the tag from the sources file" says .claude/skills/port-module/SKILL.md Checklist 'the tag that entry names'
if grep -q -e 'final-before-whizbang' .claude/skills/port-module/SKILL.md; then fail "port-module names a tag of its own"; fi
expect_pass "review-merge" check_review_merge .claude/skills/review-merge/SKILL.md
expect_pass "the reviewer" check_reviewer .claude/agents/reviewer.md
expect_pass "the builder never merges" holds .claude/agents/builder.md 'Do not merge a pull request, merge into `main`, push to `main`, tag'
expect_pass "the builder names no agent" holds .claude/agents/builder.md 'Nothing you write names an agent or a model: no commit, no pull request, no code, no'
expect_pass "the builder keeps the path out of what is published" holds .claude/agents/builder.md 'The path is never written into a hand-back, a pull request, a commit or a tracked file.'
expect_pass "the builder pushes a checked tree" holds .claude/agents/builder.md '`just check` has passed on exactly the tree being pushed'
expect_pass "AGENTS.md: no review of one's own build" holds AGENTS.md 'The orchestrator does not review its own build'
for f in .claude/skills/*/SKILL.md .claude/agents/*.md; do
  expect_pass "$f" check_file "$f"
done

# --- The scan of a title and body, with the command the skills give ----------------------
# It is run as the skills run it: from inside the repo, on two files in a folder of their
# own, with a word list (here a made-up one). The cases the skills' own commands refused
# or listed before this scan are all refused now; scripts/tests/scan/pr-text.sh has the
# rest. The lines are put together here, so this file holds none.
pr_dir="$tmp/pr-text"
mkdir "$pr_dir"
printf 'zorblaxquint\n' >"$tmp/words"
# The command names the folder as $text, as the skills do.
scanned() {
  local text=$pr_dir
  WHIZBANG_WORDLIST="$tmp/words" eval "$pr_scan"
}
# refused <name> <expected text> <line>...: the text, through the command, fails the check.
refused() {
  local name=$1 expected=$2
  shift 2
  printf '%s\n' "docs: a title" >"$pr_dir/title.txt"
  printf '%s\n' "$@" >"$pr_dir/body.md"
  expect_fail "a pull request with $name" "$expected" scanned
}
credit='Co''-Authored-By: A Person <person@example.com>'
made='Generated ''with [Some Tool](https://example.com/tool)'
has_credit="has a Co""-Authored-By line"
refused "a heading that credits a tool" "credits an agent or a model" "# $made"
refused "a credit after a # and no space" "$has_credit" "#$credit"
refused "a credit after a scissors line" "$has_credit" "words" "# ------------------------ >8 ------------------------" "$credit"
refused "a credit in a second-level heading" "$has_credit" "## $credit"
refused "a credit in a list" "$has_credit" "- $credit"
refused "a credit in a starred list" "$has_credit" "* $credit"
refused "a credit in an indented list" "$has_credit" "   - $credit"
refused "a credit in a quoted heading" "$has_credit" "> # $credit"
refused "a credit on a plain line" "$has_credit" "$credit"
# Behind a mark the skills' stripped copy kept, a credit was only listed for reading. It is
# refused, and its line named, in any letter case.
for mark in '1. ' '+ ' '| ' '`' '~ ' '<!-- ' '- [x] ' '_'; do
  refused "a credit after [$mark]" "pull-request/body:2: the message $has_credit" "words" "$mark$credit"
  refused "a credit in small letters after [$mark]" "pull-request/body:2: the message $has_credit" "words" "${mark}co""-authored-by: a person"
  refused "a line that credits a tool after [$mark]" "pull-request/body:2: the message credits an agent or a model" "words" "$mark$made"
done
# What reading alone found: a name in a sentence, and a path that starts with ~.
refused "an agent's name in a sentence" "pull-request/body:1: names an agent or a model" "written with Cl""aude this morning"
refused "a path that starts with ~" "pull-request/body:1: a path that starts with ~" "the notes are in ~/notes"
# A phrase that is no credit is listed for reading, and passes.
printf '%s\n' "docs: a title" >"$pr_dir/title.txt"
printf '%s\n' "words" "+ GENERATED ""WITH a tool" >"$pr_dir/body.md"
expect_pass "a phrase that is no credit" scanned
grep -x -F -e "to read: pull-request/body:2" "$tmp/out" >/dev/null || { cat "$tmp/out" >&2; fail "a phrase that is no credit is listed to be read"; }
printf '%s\n' "# A heading" "- a point about authors" "* another" "> a quote" "| a | table |" "#8 is the pull request before" >"$pr_dir/body.md"
expect_pass "a pull request with headings and lists and no credit" scanned
grep -F -e "to read" "$tmp/out" >/dev/null && { cat "$tmp/out" >&2; fail "a text with no phrase lists no line"; }
# Without a word list the command refuses: the skills give it with the clean-room scan.
expect_fail "the command with no word list" "WHIZBANG_WORDLIST is not set" env -u WHIZBANG_WORDLIST bash -c "text='$pr_dir'; $pr_scan"

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
changed check_file "$f" 's/Never open, print, quote or copy the word list, and never pass on what it holds/Open, print, quote or copy the word list, and pass on what it holds/' "tells someone to open or print the word list"
changed check_file "$f" 's/Never push with `--no-verify`/Push with `--no-verify`/' "names a way round a check"
changed check_kickoff "$f" 's/Never open, print, quote or copy the word list, and never pass on what it holds\./Keep the word list./' "must say: Never open, print"
changed check_kickoff "$f" 's/The path may be given to another agent working in this repo\.//' "must say: The path may be given"
changed check_kickoff "$f" 's/, which stays untracked/, which is committed/' "must say: \`.claude/settings.local.json\`, which stays untracked"
changed check_kickoff "$f" 's/check (macos-15)/check (macOS)/g' "must say:"
changed check_kickoff "$f" 's/^CI: check (macos-15) <result>, check (ubuntu-22.04) <result>/CI: check <result>/' "must say: CI: check (macos-15)"
changed check_kickoff "$f" '/^scripts\/scan.sh --pr-text /d' "must say: scripts/scan.sh --pr-text"
changed check_kickoff "$f" 's/^scripts\/scan.sh --pr-text /scripts\/scan.sh --secrets-only --pr-text /' "must say: scripts/scan.sh --pr-text"
changed check_kickoff "$f" '$s/$/\
scripts\/scan.sh --secrets-only --pr-text title body/' "in another way than the one line"
changed check_kickoff "$f" '$s/$/\
scripts\/scan.sh --message "$text\/body.md"/' "not with the commands it replaced"
changed check_kickoff "$f" 's/^# Exit 0 is the pass\. A line it prints that starts "to read:" is not refused: go through it\.$/# Exit 0 is the pass./' 'must say: A line it prints that starts "to read:"'
changed check_kickoff "$f" 's/a name it does$/no name is left/' "must say: a name it does"
# The scan moved to after the command that opens the pull request, or the one that edits it.
awk '$0 == "scripts/scan.sh --pr-text \"$text/title.txt\" \"$text/body.md\"" { held = $0; next } { print } /^gh pr create / { print held }' "$f" >"$tmp/moved.md"
cmp -s "$f" "$tmp/moved.md" && fail "moving the scan after gh pr create changed nothing"
expect_fail "kickoff with the scan after gh pr create" "before the first command that puts them on a pull request" check_kickoff "$tmp/moved.md"
changed check_kickoff "$f" 's/After any edit to either file, read and scan again\./After any edit to either file, read it again./' "must say: After any edit to either file, read and scan again."
changed check_kickoff "$f" 's/address\. Then scan them (the commands below)\./address./' "must say: address. Then scan them"
changed check_kickoff "$f" 's/So first go through the two files against/So look at the two files for/' "must say: So first go through the two files"
changed check_kickoff "$f" 's/and is not refused: go through those\. Then$/and is not refused. Then/' "must say: and is not refused: go through those"
changed check_kickoff "$f" 's/^gh pr edit <number> /gh pr comment <number> /' "must say: gh pr edit <number>"
changed check_kickoff "$f" 's/Never retry around the hook, and never use `--no-verify`\.//' "must say: Never retry around the hook"
changed check_kickoff "$f" '/^until run=/d' "must say: until run="
changed check_kickoff "$f" 's/ -ge 3 \]; do$/ -ge 1 ]; do/' "must say: .jobs | length"
changed check_kickoff "$f" '/"$tries" -le 60/d' 'must say: [ "$tries" -le 60 ]'
changed check_kickoff "$f" 's/has passed on exactly the tree being pushed/has passed/' "must say: has passed on exactly the tree being pushed"
changed check_kickoff "$f" 's/owner at once and is rotated/owner/' "must say: owner at once and is rotated"
changed check_kickoff "$f" 's/, never with a rebase\././' "must say: with a merge commit"
changed check_kickoff "$f" 's/No `Co-Authored-By`, no$/A `Co-Authored-By` when asked, no/' "must say: everything at once."
changed check_kickoff "$f" 's/^      The path is never written into a hand-back/      The path is written into a hand-back/' "must say: The path is never written"
changed check_kickoff "$f" 's/is ticked by a later pull request/is ticked now/' "must say: is ticked by a later pull request"
changed check_kickoff "$f" 's/go back to step 4/open a new pull request/' "must say: go back to step 4"
changed check_kickoff "$f" 's/It may be given short: /It is given in full: /' "must say: It may be given short"
changed check_kickoff "$f" 's/Never merge a pull request, and never merge into `main`: only the orchestrator does\./Merge when CI is green./' "must say: Never merge"
changed check_kickoff "$f" 's/Never push with `--no-verify`, never force a push, and never push to `main`\.//' "must say: Never push with"
never_merges() { says "$1" Checklist 'Never merge a pull request, and never merge into `main`: only the orchestrator does.'; }
changed never_merges .claude/skills/port-module/SKILL.md 's/Never merge a pull request, and never merge into `main`: only the orchestrator does\.//' "must say: Never merge"
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
changed check_review_merge "$f" '/^scripts\/scan.sh --pr-text /d' "must say: scripts/scan.sh --pr-text"
changed check_review_merge "$f" 's/^scripts\/scan.sh --pr-text /(cd "$text" \&\& scripts\/scan.sh) # /' "must say: scripts/scan.sh --pr-text"
changed check_review_merge "$f" '$s/$/\
scripts\/scan.sh --pr-text "$text\/title.txt" "$text\/body.md"/' "as one whole line and once"
changed check_review_merge "$f" 's/a name it does not know is found only by reading/it finds every name/' "must say: a name it does not know"
awk '$0 == "scripts/scan.sh --pr-text \"$text/title.txt\" \"$text/body.md\"" { held = $0; next } { print } /^gh pr edit / { print held }' "$f" >"$tmp/moved.md"
cmp -s "$f" "$tmp/moved.md" && fail "moving the scan after gh pr edit changed nothing"
expect_fail "review-merge with the scan after gh pr edit" "before the first command that puts them on a pull request" check_review_merge "$tmp/moved.md"
changed check_review_merge "$f" 's/^      refused: go through those\.$/      refused./' "must say: refused: go through those."
changed check_review_merge "$f" 's/lines it lists, and are gone through again,$/lines it lists,/' "must say: lines it lists, and are gone through again,"
changed check_review_merge "$f" 's/Nothing is left only in a verdict\.//' "must say: Nothing is left only in a verdict."
changed check_review_merge "$f" '/^gh run list --commit <head> /d' "must say: gh run list --commit <head>"
changed check_review_merge "$f" 's/does not review its own build: if/does not review its own build unless it must: if/' "own build"
changed check_review_merge "$f" '$s/$/\
scripts\/scan.sh --message "$text\/body.md"/' "not with the commands it replaced"
changed check_review_merge "$f" 's/, or a verdict for another commit, is a$/ is a/' "must say: A finding that blocks the merge"
changed check_review_merge "$f" 's/A follow-up does not block/A follow-up blocks/' "must say: A follow-up does not block"
changed check_review_merge "$f" 's/ justfile scripts / justfile scripts\/hooks scripts\/scan.sh /' "must say: git diff <base> <head> -- .github justfile scripts "
changed check_review_merge "$f" 's/; and any file under `scripts\/`\././' 'must say: any file under `scripts/`'
changed check_review_merge "$f" 's/goes back to the builder/is made by the orchestrator too/' "must say: A correction that changes a claim about the code goes back to the builder."
changed check_review_merge "$f" 's/, never past a failing check//' "must say: never past a failing check"
changed check_review_merge "$f" 's/check (macos-15)/check (macOS)/g' "must say:"
changed check_review_merge "$f" 's/^- \[ \] 8\. Files that grant permissions or check the pull request\./- [ ] 8. Files./' "must say: - [ ] 8."
changed check_review_merge "$f" 's/any file under `\.github\/`; //' 'must say: any file under `.github/`'
changed check_review_merge "$f" 's/ Otherwise refuse\./ Otherwise note it./' "Otherwise refuse."
changed check_review_merge "$f" 's/If it has moved since step 4, do step 4 again\./Then merge./' "must say: - [ ] 10."
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
changed check_reviewer "$f" 's/`justfile`; //' 'must say: `justfile`'
changed check_reviewer "$f" 's/; and any file under `scripts\/`//' 'must say: any file under `scripts/`'
changed check_reviewer "$f" '$s/$/\
Run git merge origin\/main in the worktree first./' "tells the reviewer to change, push or fix"
changed check_reviewer "$f" '$s/$/\
Then gh pr edit 1 --title x./' "tells the reviewer to change, push or fix"
changed check_reviewer "$f" 's/\*\*follow-up\*\*/note/' "must say: **follow-up**"
changed check_reviewer "$f" 's/may merge at <full head>/may merge/' "must say: may merge at <full head>"
changed check_reviewer "$f" 's/just scan-range <base> <head>/just scan/' "must say: just scan-range <base> <head>"
changed check_reviewer "$f" 's/^scripts\/heavy.sh check just check/just test/' "must say: scripts/heavy.sh check just check"
changed check_reviewer "$f" 's/is not a false claim/is a false claim/' "must say: is not a false claim"
changed check_reviewer "$f" '/^If you wrote any commit on this branch/d' "must say: If you wrote any commit"
changed check_reviewer "$f" 's/the orchestrator records it/it is recorded somewhere/' "must say: the orchestrator records it"
changed check_reviewer "$f" 's/"may not merge at <full head>"/"may not merge"/' 'must say: "may not merge at <full head>"'
changed check_reviewer "$f" 's/Nothing is committed or pushed by you\./Nothing is committed or pushed by you unless asked./' "must say: Nothing is committed or pushed by you."
changed check_reviewer "$f" 's/^A \*\*follow-up\*\* does not block\.$/A **follow-up** blocks too./' "must say: A **follow-up** does not block."
changed check_reviewer "$f" 's/A red job is a finding that blocks/A red job is noted/' "must say: While it is still running"
changed check_reviewer "$f" 's/so you need `WHIZBANG_WORDLIST`/and need nothing/' "must say: so you need"
own_build() { holds "$1" 'The orchestrator does not review its own build' && own_build_plain "$1"; }
changed own_build AGENTS.md 's/The orchestrator does not review its own build/The orchestrator reviews its own build/' "must say: The orchestrator does not review its own build"
changed own_build AGENTS.md 's/The orchestrator does not review its own build:/The orchestrator does not review its own build unless nobody else can:/' "makes an exception"

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

# --- Five subjects held to exact lines ----------------------------------------------------

# The checks above hold phrases, and a phrase can stay while a sentence beside it takes
# it back. So for a forced push, the git hooks and ways past them, the owner's private
# list, and merging through the API, every line in the rules, the subagent files and the
# skills in which one of the words or command forms below appears must be one of the
# lines listed, whole, after trimming; and each listed line must be there. A new sentence
# that uses one of them needs a change here, made on purpose. What this cannot see: a
# sentence on these subjects that uses none of the words, and every other subject. That
# is why the reviewer and the orchestrator read every change to these files in full.
governed="AGENTS.md .claude/agents/builder.md .claude/agents/reviewer.md .claude/skills/kickoff/SKILL.md .claude/skills/port-module/SKILL.md .claude/skills/review-merge/SKILL.md"
# The words and command forms, in any letter case.
#   a forced push: force, forced, forces and forcing as words of their own (so --force
#     too, and not "enforce"), a push with f in a bundle of short flags, a push of a +
#     refspec
#   the hooks: hook and bypass anywhere in a word; skip, skips, skipped and skipping as
#     words of their own; --no-ver, hooksPath, scripts/hooks, a commit with n in a bundle
#     of short flags
#   the list: word list in one word, two or with a hyphen, forbidden, private list, the
#     variable; and a path, or the word written, beside a hand-back
#   merging through the API: gh api
#   who merges: a line with merge in it and builder or reviewer; may merge; merges
risky='(^|[^[:alpha:]])(force|forced|forces|forcing)([^[:alpha:]]|$)|push[^|;&]*[[:space:]]-[a-z]*f|push[^|;&]*[[:space:]]`?\+[a-z<]|hook|(^|[^[:alpha:]])(skip|skips|skipped|skipping)([^[:alpha:]]|$)|bypass|--no-ver|scripts/hooks|git[[:space:]]+commit[^|;&]*[[:space:]]-[a-z]*n|word[- ]?list|forbidden|private[[:space:]]+list|WHIZBANG_WORDLIST|path[^.]*hand-back|hand-back[^.]*path|written[^.]*hand-back|hand-back[^.]*written|gh[[:space:]]+api|merge.*(builder|reviewer)|(builder|reviewer).*merge|may[[:space:]]+([a-z]+[[:space:]]+)?merge|merges'
# In the rules and the two subagent files, "the list" as well.
the_list='the[[:space:]]+list'
# risky_lines <file> <its name in the list>: every line that touches a subject, trimmed;
# and, as a line that no list can hold, any two-word form split over a line break.
risky_lines() {
  local pattern=$risky
  case "$2" in AGENTS.md | .claude/agents/*) pattern="$risky|$the_list" ;; esac
  { grep -i -E -e "$pattern" "$1" || true; } | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//'
  awk -v files="$2" '
    { line = tolower($0); sub(/[[:space:]]+$/, "", line); sub(/^[[:space:]]+/, "", line) }
    prev != "" && line != "" {
      joined = prev " " line
      split_at = length(prev) + 1
      n = 0
      rest = joined
      offset = 0
      while (match(rest, /(word|private|the)[- ]list|gh api|skip the hook|git commit|git push/)) {
        start = offset + RSTART; end = start + RLENGTH - 1
        if (start <= split_at && end > split_at) {
          found = substr(rest, RSTART, RLENGTH)
          if (found != "the list" || files == "AGENTS.md" || files ~ /^\.claude\/agents\//)
            print "split over a line break: " found
        }
        offset += RSTART + RLENGTH - 1
        rest = substr(rest, RSTART + RLENGTH)
      }
    }
    { prev = line }' "$1"
}
cat >"$tmp/exact" <<'LIST'
AGENTS.md: 1. Run `just setup`. It installs the git hooks and checks the pinned tools
AGENTS.md: 2. Set `WHIZBANG_WORDLIST` to the path of a private word list the owner provides. Set it
AGENTS.md: The path is never written into a hand-back, a pull request, a commit or a tracked file.
AGENTS.md: The list itself is never committed, opened, quoted, printed or copied,
AGENTS.md: | Push | `just check` passed on exactly the tree being pushed (it runs on the working tree, so `git status --short` prints nothing after the commit), then `just scan`: the secrets scan and the clean-room scan. The pre-push hook runs both scans again over every commit the push would publish. Never push with `--no-verify` | Whoever pushes |
AGENTS.md: `main` is never rewritten or force-pushed. A pushed tag is never moved: a correction is a new
AGENTS.md: `main`, the pre-push hook and CI. Anyone who can edit the justfile or a script changes what
AGENTS.md: flag in a bundle of short flags, such as `-cR`, is not caught. The list is repeated in
AGENTS.md: commands, no hooks, no settings further down. `scripts/worktree.sh` is not on the list, so it
AGENTS.md: `WHIZBANG_WORDLIST` is set where it lasts ("On a fresh clone"), not at the front of each
AGENTS.md: and command forms appears: those for a forced push, for the git hooks and ways past them,
AGENTS.md: for the owner's private list and its variable, and for merging through the API. The set
AGENTS.md: unseen. Lines that say who merges are held the same way.
AGENTS.md: - Commit secrets, the word list, or owner-only material such as answers to unreleased
AGENTS.md: - Push with `--no-verify`, or bypass a hook any other way. (added)
AGENTS.md: reading your own change, not by opening the list.
AGENTS.md: - **The pre-push hook fails closed.** Without `WHIZBANG_WORDLIST` set in the shell you push
AGENTS.md: from, the push is refused. That is intended; set the variable, do not bypass the hook.
AGENTS.md: - **CI cannot run the clean-room scan,** because the word list never leaves the owner's
AGENTS.md: - **A worktree on a branch cut before a change to the hooks** runs that branch's older hook.
.claude/agents/builder.md: The same checklist covers what comes after: a push the hook refuses (its step 9), a change
.claude/agents/builder.md: need `WHIZBANG_WORDLIST` in your environment to scan and to push: it is set where it lasts
.claude/agents/builder.md: The path is never written into a hand-back, a pull request, a commit or a tracked file.
.claude/agents/builder.md: Never open, print, quote or copy the word list, and never pass on what it holds.
.claude/agents/reviewer.md: so you need `WHIZBANG_WORDLIST` in your environment. It is set where it lasts (`AGENTS.md`,
.claude/agents/reviewer.md: Never open, print, quote or copy the word list, and never pass on what it holds.
.claude/skills/kickoff/SKILL.md: - [ ] 3. `WHIZBANG_WORDLIST` is set in your shell: the path of a private list the owner
.claude/skills/kickoff/SKILL.md: The path is never written into a hand-back, a pull request, a commit or a tracked file.
.claude/skills/kickoff/SKILL.md: Never open, print, quote or copy the word list, and never pass on what it holds.
.claude/skills/kickoff/SKILL.md: Never push with `--no-verify`, never force a push, and never push to `main`.
.claude/skills/kickoff/SKILL.md: Never retry around the hook, and never use `--no-verify`.
.claude/skills/kickoff/SKILL.md: Tell the orchestrator in the hand-back: which scan refused, the commit, path and
.claude/skills/review-merge/SKILL.md: - [ ] 6. With `WHIZBANG_WORDLIST` set, `just scan-range <base> <head>` passes: secrets,
AGENTS.md: - **Orchestrator:** the session the owner is talking to. Only the orchestrator merges. It
AGENTS.md: - Each acts in its role above: only the orchestrator merges, tags, releases and
AGENTS.md: | `review-merge` | Orchestrator | After the reviewer's verdict: check that it allows the merge and that the head has not moved, test the merged result, run both scans over `base..head`, merge with a merge commit, confirm it, clean up |
AGENTS.md: Changes nothing. Never merges.
AGENTS.md: merges with a merge commit, never a squash and never a rebase merge, at the exact head
AGENTS.md: request or into `main`, never pushes to `main`, never tags. It merges `origin/main` into
AGENTS.md: the branch, opens the pull request and hands back its head commit. Never merges a pull
AGENTS.md: ticks, in its branch, each box of the plan that the branch makes true once it merges; a box
.claude/agents/builder.md: description: "Builds one task from the current milestone plan in a worktree: tests first, then code, then docs. Pushes its branch, opens the pull request and hands back its head commit. Never merges, pushes to main or tags."
.claude/agents/reviewer.md: box ticked in the plan is one the branch makes true once it merges; a tick made on that
.claude/agents/reviewer.md: branch merges.
.claude/agents/reviewer.md: description: "Reviews one branch against the spec and its task. Reports findings; changes nothing and never merges."
.claude/agents/reviewer.md: orchestrator merges by `.claude/skills/review-merge/SKILL.md`, which refuses a verdict that
.claude/agents/reviewer.md: request. A false claim there is a finding that blocks the merge; the builder, or the
.claude/agents/reviewer.md: with the verdict, in these words: "may merge at <full head>" when no finding that blocks
.claude/agents/reviewer.md: the merge is open, and "may not merge at <full head>" otherwise. Change no file. The
.claude/skills/kickoff/SKILL.md: makes true once it merges. A box about something that happens after the pull request
.claude/skills/review-merge/SKILL.md: - [ ] 1. The verdict says `may merge at <full head>`, and the commit it names is exactly
.claude/skills/review-merge/SKILL.md: - Verdict: may merge at the reviewed head; no finding that blocks the merge is open
.claude/skills/review-merge/SKILL.md: worktree, and hand back; the builder merges `main` into the branch, and the new head
LIST
# exact_lines <file> <its name in the list>
# The failure says, for each line that is there and not listed, the word or form that
# makes it a line on one of the subjects, and what to do about it.
exact_lines() {
  local pattern=$risky line
  case "$2" in AGENTS.md | .claude/agents/*) pattern="$risky|$the_list" ;; esac
  risky_lines "$1" "$2" | sort >"$tmp/exact.found"
  awk -v name="$2: " 'index($0, name) == 1 { print substr($0, length(name) + 1) }' "$tmp/exact" | sort >"$tmp/exact.want"
  if ! diff "$tmp/exact.want" "$tmp/exact.found" >"$tmp/exact.diff"; then
    echo "$2: its lines about a forced push, the hooks, the private list, the API or who merges are not exactly those listed in scripts/tests/test-skills.sh"
    { grep '^> ' "$tmp/exact.diff" || true; } | sed 's/^> //' >"$tmp/exact.new"
    while IFS= read -r line; do
      echo "  not listed: $line"
      printf '%s\n' "$line" | { grep -i -o -E -e "$pattern" || echo 'a form split over a line break'; } >"$tmp/exact.word"
      echo "    it is held because of: $(sed -n -e '1s/^[^[:alnum:]+-]*//' -e '1s/[^[:alnum:]]*$//' -e 1p "$tmp/exact.word")"
    done <"$tmp/exact.new"
    { grep '^< ' "$tmp/exact.diff" || true; } | sed 's/^< /  listed, and gone or changed: /'
    echo "  What to do: reword a line that is not listed so that it does not use the word, or,"
    echo "  when the line is meant, add it whole to the list in scripts/tests/test-skills.sh."
    echo "  A listed line that is gone is put back, or taken off the list on purpose."
    return 1
  fi
}
for f in $governed; do
  expect_pass "the exact lines of $f" exact_lines "$f" "$f"
done
[ "$(cut -d: -f1 "$tmp/exact" | sort -u | wc -l | tr -d ' ')" -ge 5 ] || fail "the exact list names fewer files than it should"
# exact_added <file> <line>...: the file with the lines added at its end must fail.
exact_added() {
  local file=$1
  shift
  cp "$file" "$tmp/exact.md"
  printf '%s\n' "$@" >>"$tmp/exact.md"
  expect_fail "$file with the line: $*" "are not exactly those listed" exact_lines "$tmp/exact.md" "$file"
}
# exact_changed <file> <sed script>: the file after the edit must fail.
exact_changed() {
  sed "$2" "$1" >"$tmp/exact.md"
  cmp -s "$1" "$tmp/exact.md" && fail "the edit changed nothing: $2"
  expect_fail "$1 after: $2" "are not exactly those listed" exact_lines "$tmp/exact.md" "$1"
}
for f in $governed; do
  for line in 'Open the word list and check your change against it.' \
    'Never wait on a slow hook, push with --no-verify instead.' \
    'When the branch is yours alone, git push -f origin x is fine.' \
    'git push -fu origin x' 'git push origin +x' 'git push origin `+<slug>`' \
    'A push may use --force-with-lease after a review.' 'Force the branch when it is stale.' \
    'If the hook is slow, skip the hook for that push.' 'If it is slow, skip it.' \
    'look through the private list of the owner for the term' \
    'git -c core.hooksPath=/dev/null push' 'chmod -x scripts/hooks/pre-push' \
    'git commit -n -m x' 'git commit -anm x' 'git commit --no-verif -m x' 'In a hurry, bypass verification.' \
    'Search the wordlist.' 'Search the word-list.' 'Search the Word List.' 'the forbidden-words file' \
    'env | grep WHIZBANG_WORDLIST' 'The path may be written into the hand-back when asked.' \
    'gh api -X PUT repos/o/r/pulls/1/merge' 'Print the word list when a scan fails.' \
    'It may be written in the hand-back.' 'Skipping it is fine when the tree is small.' \
    'A stale branch is forced.' 'The builder may merge its own pull request once CI is green.' \
    'When the orchestrator is away, the reviewer merges.' 'It may also merge a small fix.' \
    'Whoever is free merges it.'; do
    exact_added "$f" "$line"
  done
  # A form of two words, split over a line break.
  exact_added "$f" 'When a scan fails, look in the word' 'list for the cause.'
  exact_added "$f" 'When a scan fails, look in the private' 'list for the cause.'
  exact_added "$f" 'To merge without the page, call gh' 'api with the pull request.'
done
for f in AGENTS.md .claude/agents/builder.md .claude/agents/reviewer.md; do
  exact_added "$f" 'the orchestrator gives you the list'
  exact_added "$f" 'When a scan fails, the orchestrator gives you the' 'list to look in.'
done
exact_changed .claude/skills/kickoff/SKILL.md 's/and never use `--no-verify`\.$/& Unless the scan is slow./'
exact_changed .claude/skills/kickoff/SKILL.md 's/never force a push, and never push to `main`\.$/& A stale branch is the exception./'
exact_changed .claude/agents/builder.md '/^Never open, print, quote or copy the word list/d'
exact_changed .claude/agents/reviewer.md '/^Never open, print, quote or copy the word list/d'
exact_changed .claude/skills/kickoff/SKILL.md '/Never retry around the hook, and never use `--no-verify`\./d'
exact_changed AGENTS.md '/^- Push with `--no-verify`, or bypass a hook any other way/d'
exact_changed AGENTS.md 's/^   The path is never written into a hand-back/   The path is written into a hand-back/'
# What is not a subject passes: the list stays as short as the subjects. A word that only
# holds one of the words inside it is not the word.
for f in $governed; do
  cp "$f" "$tmp/exact.md"
  printf '%s\n' 'Tests wait on events.' 'The carry-over list is the only backlog.' \
    'CI enforces the same checks on both runners.' 'Reinforce the characterisation tests first.' >>"$tmp/exact.md"
  expect_pass "lines on other subjects, $f" exact_lines "$tmp/exact.md" "$f"
done
# The failure names what tripped, and says what to do.
cp AGENTS.md "$tmp/exact.md"
printf '%s\n' 'A test that skips on one platform says so.' >>"$tmp/exact.md"
expect_fail "the message names the word" "it is held because of: skips" exact_lines "$tmp/exact.md" AGENTS.md
expect_fail "the message says what to do" "add it whole to the list" exact_lines "$tmp/exact.md" AGENTS.md
sed '/^Never open, print, quote or copy the word list/d' .claude/agents/builder.md >"$tmp/exact.md"
expect_fail "the message names a listed line that is gone" "listed, and gone or changed: Never open, print" exact_lines "$tmp/exact.md" .claude/agents/builder.md

# --- What "never" rests on ----------------------------------------------------------------

# Two of the listed lines are orders whose "never" is the heading above them, and one is
# the end of a sentence whose sense is in the line before it. So the heading and its
# bullets are held, in order, and so are the sentences those lines end.
# check_never <file>: everything between the heading **Never** and the next heading in the
# rules, whole: each bullet with the lines that continue it, and the blank lines. So a
# sentence added after the bullets, an indented exception under one, a changed
# continuation line, a moved bullet and a renamed heading all fail.
check_never() {
  [ "$(grep -c '^\*\*Never\*\*$' "$1")" = 1 ] || { echo "rules: $1 must have the heading **Never**, once"; return 1; }
  awk '/^\*\*Never\*\*$/ { on = 1; next } on && /^(\*\*[A-Z]|## )/ { exit } on' "$1" >"$tmp/never.found"
  cat >"$tmp/never.want" <<'LIST'
- Add `Co-Authored-By`, "Generated with", or any agent or model name to a commit, a pull
  request's title or body, code or docs. A `Co-Authored-By` line naming a person is refused
  too. This overrides any tool's default. ("Generated with" and the override: added)
- Commit secrets, the word list, or owner-only material such as answers to unreleased
  evaluation sets.
- Edit a sealed evaluation set, its labels or the rules files its seal covers.
- View the items of a held-out evaluation set while it is held out.
- Rewrite history on `main`, or move a pushed tag. (added)
- Push with `--no-verify`, or bypass a hook any other way. (added)
- Change an old repo beyond what the freeze allows, or delete one.

LIST
  diff "$tmp/never.want" "$tmp/never.found" >"$tmp/never.diff" ||
    { echo "rules: what stands under **Never** in $1, up to the next heading, is not what is listed in scripts/tests/test-skills.sh, line for line"; cat "$tmp/never.diff"; return 1; }
}
expect_pass "what stands under Never" check_never AGENTS.md
changed check_never AGENTS.md 's/^\*\*Never\*\*$/**When it helps**/' "must have the heading **Never**, once"
changed check_never AGENTS.md '/^- Push with `--no-verify`, or bypass a hook any other way/d' "is not what is listed"
changed check_never AGENTS.md '/^- Commit secrets, the word list/d' "is not what is listed"
# A line that continues a bullet, changed.
changed check_never AGENTS.md 's/^  evaluation sets\.$/  evaluation sets, unless the owner is away./' "is not what is listed"
# never_edit <name> <awk program>: the rules after the edit must fail.
never_edit() {
  awk "$2" AGENTS.md >"$tmp/never.md"
  cmp -s AGENTS.md "$tmp/never.md" && fail "$1: the edit changed nothing"
  expect_fail "$1" "is not what is listed" check_never "$tmp/never.md"
}
never_edit "a Never bullet moved under Agents may" '/^- Push with `--no-verify`, or bypass a hook any other way/ { next }
  { print } /^\*\*Agents may\*\*$/ { print "- Push with `--no-verify`, or bypass a hook any other way. (added)" }'
never_edit "a bullet added under Never" '/^\*\*Never\*\*$/ { print; print "- Wait for a slow scan."; next } { print }'
never_edit "an exception indented under a bullet" '{ print } /^- Rewrite history on `main`, or move a pushed tag/ { print "  - except to take a secret out" }'
never_edit "a sentence after the bullets" '/^## Secrets and private data$/ { print "These hold unless the orchestrator says otherwise."; print "" } { print }'
never_edit "a new heading that takes the last bullets" '/^- Rewrite history on `main`, or move a pushed tag/ { print "**When the owner agrees**" } { print }'

# sentence <file> <text>: the file holds the text as running words, across its line breaks.
sentence() {
  tr '\n' ' ' <"$1" | tr -s ' ' | grep -F -e "$2" >/dev/null || { echo "$1 must say, as one run of words: $2"; return 1; }
}
kickoff=.claude/skills/kickoff/SKILL.md
no_hook='Push the branch by name, from inside the worktree: a push made from another folder is scanned only when `GIT_DIR` and `GIT_WORK_TREE` both name the worktree, and is refused otherwise.'
# Held to the end of its paragraph: the next words are the next step.
no_value='Tell the orchestrator in the hand-back: which scan refused, the commit, path and line if it named them, and what you did, without the value. - [ ] 10. Write the pull request'
expect_pass "kickoff: why the push is made from the worktree" sentence "$kickoff" "$no_hook"
expect_pass "kickoff: a refused push is told without the value" sentence "$kickoff" "$no_value"
sed 's/a push made from another folder is scanned only$/when the scan is slow, make the push from another folder: it is scanned only/' "$kickoff" >"$tmp/sentence.md"
cmp -s "$kickoff" "$tmp/sentence.md" && fail "the edit to the line before the fragment changed nothing"
expect_fail "kickoff: the line before the fragment rewritten" "must say, as one run of words" sentence "$tmp/sentence.md" "$no_hook"
sed 's/and what you did, without the value\.$/and what you did, with the value./' "$kickoff" >"$tmp/sentence.md"
cmp -s "$kickoff" "$tmp/sentence.md" && fail "the edit to the value changed nothing"
expect_fail "kickoff: told with the value" "must say, as one run of words" sentence "$tmp/sentence.md" "$no_value"
awk '{ print } /and what you did, without the value\.$/ { print "      Include the value when the orchestrator asks for it." }' "$kickoff" >"$tmp/sentence.md"
cmp -s "$kickoff" "$tmp/sentence.md" && fail "the sentence added after the value changed nothing"
expect_fail "kickoff: a sentence added after it" "must say, as one run of words" sentence "$tmp/sentence.md" "$no_value"

# --- The review rule for plans --------------------------------------------------------------

# The owner's rule: two rounds, every blocker and major finding, and nothing of section 15
# replaced. Held as running words in the rules and in the spec.
for text in 'at least two rounds by the independent reviewing model, with every blocker and major finding fixed and the outcome of each round recorded in the plan' \
  'That rule replaces nothing in `SPEC.md` section 15.' \
  'A change to section 15 itself is the owner'"'"'s.' \
  'a model from a different provider than the one that wrote the text, chosen by the owner, and run through its command-line tool in a read-only mode' \
  'An edit to the spec that changes no scope, architecture, boundary or milestone content,' \
  'Any other change to the spec gets the two rounds first.'; do
  expect_pass "the rules: $text" sentence AGENTS.md "$text"
done
for text in 'gets at least two review rounds by the independent reviewing model' \
  'Every blocker and major finding is fixed, and the outcome of each round is recorded in the plan.' \
  'It replaces nothing in section 15' 'a change to section 15 itself is the owner'"'"'s' \
  'any other change gets the two rounds first'; do
  expect_pass "the spec: $text" sentence SPEC.md "$text"
done
# rule_edit <file> <sed script> <text>: the file after the edit no longer says the text.
rule_edit() {
  sed "$2" "$1" >"$tmp/rule.md"
  cmp -s "$1" "$tmp/rule.md" && fail "the edit changed nothing: $2"
  expect_fail "$1 after: $2" "must say, as one run of words" sentence "$tmp/rule.md" "$3"
}
rule_edit AGENTS.md 's/architecture, what a milestone holds), is reviewed before building starts: at least two$/architecture, what a milestone holds), is reviewed before building starts: at least one/' 'at least two rounds by the independent reviewing model'
rule_edit AGENTS.md 's/with every blocker and major finding fixed$/with every blocker fixed/' 'with every blocker and major finding fixed'
rule_edit AGENTS.md 's/^- That rule replaces nothing in `SPEC.md` section 15\./- That rule replaces section 15 where the reviews agree./' 'That rule replaces nothing in `SPEC.md` section 15.'
rule_edit SPEC.md 's/gets at least two review rounds by$/gets one review round by/' 'gets at least two review rounds by the independent reviewing model'
# no_waiver <file>: nothing says a change needs no review.
no_waiver() {
  if tr '\n' ' ' <"$1" | tr -s ' ' | grep -i -E 'needs? no review|without (a |any )?review' >/dev/null; then
    echo "$1 says that something needs no review"
    return 1
  fi
}
for f in $governed SPEC.md; do
  expect_pass "no waiver of review in $f" no_waiver "$f"
done
cp AGENTS.md "$tmp/rule.md"
printf '%s\n' 'A change to the plan that only moves a task needs no' 'review at all.' >>"$tmp/rule.md"
expect_fail "a waiver over a line break" "needs no review" no_waiver "$tmp/rule.md"

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
  section "$file" Commands | grep -F -e '| `scripts/worktree.sh add <slug>` |' >/dev/null ||
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
