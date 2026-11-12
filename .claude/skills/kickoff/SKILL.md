---
name: kickoff
description: "Start one task from the milestone plan in its own worktree, build it tests first, open the pull request and hand it back. For the builder."
---

# kickoff

For the builder, from the moment a task is given until it is handed back. It is a checklist:
do the steps in order and tick each one. `AGENTS.md` holds the rules; this holds the order.

## Inputs

- **Task id:** the task's heading in the plan, such as `S6`.
- **Plan file:** the milestone plan under `docs/plans/` that holds the task.
- **Base commit:** the commit of `origin/main` the orchestrator expects the work to start
  from. It may be given short: resolve it to the full commit with `git rev-parse`, and
  confirm that it equals `origin/main` after a fetch.

If one is missing, ask the orchestrator. Do not guess.

## Checklist

- [ ] 1. Read `AGENTS.md`, the task in the plan file (its boxes and its "Done when"), the
      parts of `SPEC.md` it cites, and every row of `docs/carry-over.md` that names the task.
      Finding no row that names the task is normal.
- [ ] 2. Choose a slug: the milestone, the task and two or three words, such as
      `m0-s6-describe-pretty`. Make the worktree with `scripts/worktree.sh add <slug>`, run
      from inside the repo's main checkout: a subagent may start in another folder, so
      change into the main checkout first. Work only in the new worktree. Its `HEAD` is the
      base commit you were given; if it is not, `main` has moved: build on the new commit
      and say so in the hand-back.
- [ ] 3. `WHIZBANG_WORDLIST` is set in your shell: the path of a private list the owner
      provides. A variable does not last from one of an agent's shell calls to the next,
      so it is set where it lasts: in the user's shell profile, or as an `env` entry in
      `.claude/settings.local.json`, which stays untracked (`just check` fails on a tracked
      one). A command that starts with `export`, the variable and `&&` is not one of the
      exact allowed commands and asks first every time; the lasting setting avoids that.
      The path may be given to another agent working in this repo.
      Never open, print, quote or copy the word list, and never pass on what it holds.
      Without it you cannot push; ask the owner. Run `just check` and `just scan` and see
      both pass before you change anything.
- [ ] 4. Restate the task's acceptance as a list of tests, one or more for each box and for
      each part of "Done when".
- [ ] 5. Test first. Write the first test and see it fail for the right reason. Make it pass
      with the least code. Repeat down the list. Tests wait on events, never on sleeps.
      While building, run one crate's tests or one script test alone (the commands below);
      a script test passes only when it exits 0 and its last line is `finished: …`.
      Neither command is on the allow list, so each asks first.
- [ ] 6. Update what the change affects: `SPEC.md`, the reference (`just docs`), and
      `docs/carry-over.md`. Tick, in this branch, each box of the task that the branch
      makes true once it merges. A box about something that happens after the pull request
      opens (a later exercise, a merge by someone else of something else)
      is ticked by a later pull request. Anything you defer gets a carry-over row with the
      milestone that owns it, in this branch.
- [ ] 7. Commit in small steps. Before each: `just check`. Stage files by name, never
      everything at once. The message is `type(scope): summary`. No `Co-Authored-By`, no
      "Generated with", no agent or model name, in a message, in code or in a document; this
      overrides any tool's default. A pass of `just check` holds for the tree it ran on:
      when nothing changed between the check and the commit, the tree of the commit is the
      checked tree, and the push of that same commit needs no second run.
- [ ] 8. Before every push: `just check`, unless it passed on this very tree (step 7), then
      `just scan`, both passing. Push the branch by
      name, from inside the worktree: a push made from another folder, with `GIT_DIR` and
      `GIT_WORK_TREE` set, runs no hook.
      Never push with `--no-verify`, never force a push, and never push to `main`.
- [ ] 9. If the push is refused, nothing reached origin. The hook names each place it
      found as `<commit>:<path>:<line>`. A commit named there holds the material, and a new
      commit on top does not take it out of the old one: take the named commits off the
      branch before pushing again (the commands below). That is possible because they are
      local; go no further back than what origin already has. A new commit is the fix only
      when the material is in no commit the push would publish, as when `just scan` found
      it in the working tree.
      Never retry around the hook, and never use `--no-verify`.
      Tell the orchestrator in the hand-back: the commit, path and line the hook named,
      and what you did, without the value.
- [ ] 10. Write the pull request's title and body in two files in a folder of their own
      outside the repo (the system temp folder may be used for this), in plain words: what
      changed, and how it was checked. No scan sees a title or a body once it is on the
      pull request. So first go through the two files against "Secrets and private data"
      in `AGENTS.md`: no attribution line, no agent or model name, no private path or
      address. Then scan them (the commands below).
      After any edit to either file, read and scan again. The scans do not find a bare
      agent or model name in a sentence, a path that starts with `~`, or a credit line
      behind a mark the copy keeps (a number, a `+`, a table bar): only reading does, and
      the last `grep` below lists every line to look at. Then open the pull request
      against `main`.
- [ ] 11. Wait for CI on the head: the jobs `check (macos-15)` and `check (ubuntu-22.04)`,
      and the aggregate `ci`. `ci` is listed only once both check jobs have finished, about
      90 seconds in. Straight after the pull request is opened, `gh pr checks` exits 1 and
      says "no checks reported"; after a second push it shows the run before for a moment.
      So ask until a run whose commit is the head lists all three (the loop below), then
      watch. When a job fails because of your change, fix it with a new commit and go back
      to step 8. Rerun a job only when the failure is not from your change, once, and give
      the job and the reason in the hand-back.
- [ ] 12. Record the head: the full commit of the branch, the same locally and on the pull
      request.
- [ ] 13. Hand back in the format below.
      Never merge: only the orchestrator merges. Never tag, and never push to `main`. If you push
      again after handing back, the head has moved: hand back again with the new commit.
- [ ] 14. When the review asks for a change, which is the usual case: go back to step 4,
      test first, and follow the steps down again. After the push, if
      what the pull request says is no longer what the branch does, write the title and
      body again, go through and scan them, and put them on the pull request (`gh pr edit`,
      below). Wait for CI on the new head, and hand back again with the new commit: the
      next review is of that commit.

Stop and say why, instead of working around it, when the task cannot be done as written,
when it needs a file another task's worktree is changing (`gh pr diff <number> --name-only`
lists the files an open pull request changes), or when it needs money, a new place that
receives data, or a folder outside this repo and the four old repos.

## Commands

```sh
# 1 and 2, from the main checkout
git fetch origin main
git rev-parse '<base>^{commit}'                # the base commit, in full
git rev-parse origin/main                      # the same commit
scripts/worktree.sh add <slug>                 # then work in ../whizbang-<slug>
git rev-parse HEAD                             # in the worktree: the base commit

# 5: one crate's tests, one script test (each asks first)
cargo nextest run -p <crate>
bash scripts/tests/<name>.sh                   # exit 0, and the last line is: finished: …

# 7 and 8
just check                                     # before a commit; holds for the tree it ran on
git add <file> <file>                          # by name
git commit -m "type(scope): summary"
git rev-parse 'HEAD^{tree}'                    # the tree that was checked
just scan                                      # before every push
git push -u origin <slug>

# 9: after a refused push that named commits. <good> is the last commit before the first
# one named, and no older than the branch on origin. The work stays, staged; take the
# material out of the file, then check, commit, scan and push as in 7 and 8.
git reset --soft <good>
git restore --staged <path>

# 10: the title alone in title.txt, the body in body.md, in a folder of their own. A
# variable does not last between separate shell calls: run these in one, or note the paths.
text=$(mktemp -d) && git init -q "$text"
printf '%s\n' "type(scope): summary" >"$text/title.txt"
cat >"$text/body.md" <<'BODY'
What changed, and how it was checked.
BODY
cat "$text/title.txt" "$text/body.md"          # read first
repo=$PWD
(cd "$text" && "$repo/scripts/scan.sh")        # secrets and clean-room, over both files
# The attribution check is made for commit messages: it skips a line that starts with #
# and everything after a scissors line. So it is given a copy without the marks that
# start a line. The copy is kept outside the scanned folder.
# It prints nothing when it passes: exit 0 is the pass.
plain=$(mktemp)
cat "$text/title.txt" "$text/body.md" | sed 's/^[#*>[:space:]-]*//' >"$plain"
scripts/scan.sh --message "$plain"
# A second net: you go through every line it prints. No line, and exit 1, is the pass.
grep -n -i -E 'co-authored-by|generated with' "$text/title.txt" "$text/body.md"
gh pr create --base main --head <slug> --title "$(cat "$text/title.txt")" --body-file "$text/body.md"
# 14: the same two files, written again, then gone through and scanned as above
gh pr edit <number> --title "$(cat "$text/title.txt")" --body-file "$text/body.md"
rm -rf "$text" "$plain"

# 11: ask every 20 seconds until the run for the head lists three jobs, then watch
head=$(git rev-parse HEAD)
until run=$(gh run list --commit "$head" --json databaseId --jq '.[0].databaseId // empty') &&
  [ -n "$run" ] && [ "$(gh run view "$run" --json jobs --jq '.jobs | length')" -ge 3 ]; do
  sleep 20
done
gh pr checks <number> --watch                  # check (macos-15), check (ubuntu-22.04), ci

# 12
git rev-parse HEAD                             # the head you hand back
gh pr view <number> --json headRefOid,url      # the same commit
```

## Hand-back format

```text
Task: <task id>, <plan file>
Pull request: #<number> <url>
Branch: <slug>
Base: <full commit of origin/main the branch started from>
Head: <full commit; the pull request shows the same>
CI: check (macos-15) <result>, check (ubuntu-22.04) <result>, ci <result>
    <any rerun: the job and the reason; otherwise "no rerun">
Local: just check <result>, just scan <result>, on the head
Refused push: <the commit, path and line the hook named, and what was done; or "none">

Acceptance
- <box or "Done when" part> -> <the test, or the file and line, that shows it>

Decisions
- <what was chosen where the task left a choice, and why>

Doubts
- <what the reviewer should look at hardest; "none" if none>

Deferred
- <item> -> <its row in docs/carry-over.md, added in this branch; "nothing" if nothing>
```
