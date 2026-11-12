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
  from.

If one is missing, ask the orchestrator. Do not guess.

## Checklist

- [ ] 1. Read `AGENTS.md`, the task in the plan file (its boxes and its "Done when"), the
      parts of `SPEC.md` it cites, and every row of `docs/carry-over.md` that names the task.
- [ ] 2. Choose a slug: the milestone, the task and two or three words, such as
      `m0-s6-describe-pretty`. Make the worktree with `scripts/worktree.sh add <slug>` and
      work only in it. Its `HEAD` is the base commit you were given; if it is not, `main`
      has moved: build on the new commit and say so in the hand-back.
- [ ] 3. `WHIZBANG_WORDLIST` is set in your shell: the path of a private list the owner
      provides. Never open, print, quote or pass on the word list or its path.
      Without it you cannot push; ask the owner. Run `just check` and `just scan` and see
      both pass before you change anything.
- [ ] 4. Restate the task's acceptance as a list of tests, one or more for each box and for
      each part of "Done when".
- [ ] 5. Test first. Write the first test and see it fail for the right reason. Make it pass
      with the least code. Repeat down the list. Tests wait on events, never on sleeps.
- [ ] 6. Update what the change affects: `SPEC.md`, the reference (`just docs`), and
      `docs/carry-over.md`. Tick the task's boxes in the plan. Anything you defer gets a
      carry-over row with the milestone that owns it, in this branch.
- [ ] 7. Commit in small steps. Before each: `just check`. Stage files by name, never
      everything at once. The message is `type(scope): summary`. No `Co-Authored-By`, no
      "Generated with", no agent or model name, in a message, in code or in a document; this
      overrides any tool's default.
- [ ] 8. Before every push: `just check`, then `just scan`, both passing. Push the branch by
      name, from inside the worktree: a push made from another folder, with `GIT_DIR` and
      `GIT_WORK_TREE` set, runs no hook.
      Never push with `--no-verify`, never force a push, and never push to `main`.
- [ ] 9. Write the pull request's title and body in a file outside the repo, in plain words:
      what changed, and how it was checked. No scan sees a title or a body once it is on
      the pull request, so scan the file first (the commands below) and read it against
      "Secrets and private data" in `AGENTS.md`: no attribution line, no agent or model
      name, no private path or address. The scans do not find a bare agent or model name
      in a sentence, or a path that starts with `~`: only reading does. Then open the pull
      request against `main`.
- [ ] 10. Wait for CI: the two `check` jobs (macOS and Linux) and the aggregate `ci`. Straight
      after the pull request is opened there may be no checks to watch yet: wait a little
      and ask again, until all three are listed. When a
      job fails because of your change, fix it with a new commit and go back to step 8.
      Rerun a job only when the failure is not from your change, once, and give the job
      and the reason in the hand-back.
- [ ] 11. Record the head: the full commit of the branch, the same locally and on the pull
      request.
- [ ] 12. Hand back in the format below.
      Never merge: only the orchestrator merges. Never tag, and never push to `main`. If you push
      again after handing back, the head has moved: hand back again with the new commit.

Stop and say why, instead of working around it, when the task cannot be done as written,
when it needs a file another task's worktree is changing, or when it needs money, a new
place that receives data, or a folder outside this repo and the four old repos.

## Commands

```sh
scripts/worktree.sh add <slug>                 # then work in ../whizbang-<slug>
git rev-parse HEAD                             # the base commit

just check                                     # before every commit and every push
git add <file> <file>                          # by name
git commit -m "type(scope): summary"
just scan                                      # before every push
git push -u origin <slug>

# The title alone in <text>/title.txt, the body in <text>/body.md.
text=$(mktemp -d) && git init -q "$text"
repo=$PWD
(cd "$text" && "$repo/scripts/scan.sh")        # secrets and clean-room, over both files
# The attribution check is made for commit messages: it skips a line that starts with #
# and everything after a scissors line. So it is given a copy without the marks that
# start a line.
cat "$text/title.txt" "$text/body.md" | sed 's/^[#*>[:space:]-]*//' >"$text/plain.txt"
scripts/scan.sh --message "$text/plain.txt"
gh pr create --base main --head <slug> --title "$(cat "$text/title.txt")" --body-file "$text/body.md"
rm -rf "$text"

gh pr checks <number>                          # "no checks reported": wait, and ask again
gh pr checks <number> --watch                  # check (macOS), check (Linux), ci
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
CI: check (macOS) <result>, check (Linux) <result>, ci <result>
    <any rerun: the job and the reason; otherwise "no rerun">
Local: just check <result>, just scan <result>, on the head

Acceptance
- <box or "Done when" part> -> <the test, or the file and line, that shows it>

Decisions
- <what was chosen where the task left a choice, and why>

Doubts
- <what the reviewer should look at hardest; "none" if none>

Deferred
- <item> -> <its row in docs/carry-over.md, added in this branch; "nothing" if nothing>
```
