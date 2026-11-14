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
      It travels only in a brief to an agent or in the local environment.
      The path is never written into a hand-back, a pull request, a commit or a tracked file.
      Never open, print, quote or copy the word list, and never pass on what it holds.
      Without it you cannot push; ask the owner. Run `just check` and `just scan` and see
      both pass before you change anything.
- [ ] 4. Restate the task's acceptance as a list of tests, one or more for each box and for
      each part of "Done when".
- [ ] 5. Test first. Write the first test and see it fail for the right reason. Make it pass
      with the least code. Repeat down the list. Tests wait on events, never on sleeps.
      While building, run one crate's tests or one script test alone (the commands below);
      a script test passes only when it exits 0 and its last line is `finished: …`.
      The script test is not on the allow list, so it asks first. `just test-only` takes
      `-p <crate>` or `--package <crate>`, `--lib`, `--test <name>`, `-E <filter>`, `--no-capture`,
      `--no-fail-fast` and filter words; anything else is refused. A test makes a thread
      only through `whizbang_testkit::threads`, and no test is run by a bare `cargo`
      command (`AGENTS.md`, "Gotchas": threads and processes).
- [ ] 6. Update what the change affects: `SPEC.md`, the reference (`just docs`), and
      `docs/carry-over.md`. A change to the spec that alters scope, architecture or
      what a milestone holds is not made inside a task: it needs the review rounds first
      (`AGENTS.md`, "Working with the owner"), so stop and say so. Wording and accuracy,
      to bring the spec in line with what the branch does, belong here. Tick, in this branch, each box of the task that the branch
      makes true once it merges. A box about something that happens after the pull request
      opens (a later exercise, a merge by someone else of something else)
      is ticked by a later pull request. Anything you defer gets a carry-over row with the
      milestone that owns it, in this branch.
- [ ] 7. Commit in small steps. Before each: `just check`. Stage files by name, never
      everything at once. The message is `type(scope): summary`. No `Co-Authored-By`, no
      "Generated with", no agent or model name, in a message, in code or in a document; this
      overrides any tool's default. `just check` runs on the working tree, so
      a pass covers a commit only when it ran on exactly that commit's tree: nothing
      changed between the check and the commit, and after the commit `git status --short`
      prints nothing. The push of that same commit then needs no second run.
- [ ] 8. Before every push: `just check` has passed on exactly the tree being pushed
      (step 7), then `just scan` passes. Push the branch by
      name, from inside the worktree: a push made from another folder runs the hook of
      the working tree git is then acting on (the one `git -C` or `GIT_WORK_TREE` names,
      or else the folder git is run in), and is refused when that is no checkout of this repo.
      Never push with `--no-verify`, never force a push, and never push to `main`.
- [ ] 9. If the push is refused, nothing reached origin. The secrets scan names each place
      it found as `<commit>:<path>:<line>`; the clean-room scan prints a count and names
      nothing. A commit that holds the material keeps it whatever is committed on top, so
      the commits come off the branch before the next push (the commands below). After a
      secrets refusal: the commits it names, and those after them. After a clean-room
      refusal: every commit that is not yet on origin, back to origin's tip of the branch,
      or to the base when the branch was never pushed; find the material by going through
      your own change. Go no further back than what origin already has. Then take the
      material out and commit again. If one of the commits taken off was a merge of
      `main`, the reset has folded what `main` brought into the staged work: commit only
      your own files, drop the rest, and merge `origin/main` again (step 15), so that the
      head still contains `main`. A new commit on top is the fix only when the
      material is in no commit the push would publish, as when `just scan` found it in the
      working tree. The commits taken off stay in the local repository, in its object
      store and reflog, which every worktree shares, until git collects them: never
      bring them back, by the reflog or by a cherry-pick. A real key is reported to the
      owner at once and is rotated: taking it off a branch is not enough.
      Never retry around the hook, and never use `--no-verify`.
      Tell the orchestrator in the hand-back: which scan refused, the commit, path and
      line if it named them, and what you did, without the value.
- [ ] 10. Write the pull request's title and body in two files in a folder of their own
      outside the repo (the system temp folder may be used for this), in plain words: what
      changed, and how it was checked. No scan sees a title or a body once it is on the
      pull request. So first go through the two files against "Secrets and private data"
      in `AGENTS.md`: no attribution line, no agent or model name, no private path or
      address. Then scan them (the commands below).
      After any edit to either file, read and scan again. The scan refuses an agent's
      or a model's name that it knows (but for the folder `.claude` and the file
      `CLAUDE.md` of this repo, named as paths), a path that starts with `~`, a value of the repo's
      local `.env`, and a credit line with nothing before it but marks that are not
      letters (a number, a sign, a table bar, a quote, a backtick, a box); a name it does
      not know is found only by reading, and so is a credit behind a mark made of letters
      (`a. `, `Note: `, `<p>`), which it only lists. It lists each line that says
      "co-authored-by" or "generated with" and is not refused: go through those. Then
      open the pull request against `main`.
- [ ] 11. Wait for CI on the head: the jobs `check (macos-15)` and `check (ubuntu-22.04)`,
      and the aggregate `ci`. `ci` is listed only once both check jobs have finished: the
      Linux job takes about 90 seconds and the macOS job about 3 minutes. Straight after
      the pull request is opened, `gh pr checks` exits 1 and
      says "no checks reported"; after a second push it shows the run before for a moment.
      So ask until a run whose commit is the head lists all three (the loop below), then
      watch. The loop outlasts one shell call of the usual length:
      give the call a longer time limit, or enter the loop again. It gives up after 20
      minutes: hand back then, saying that CI did not start. A pull request that conflicts
      with `main` gets no run at all. When a job fails because of your change, fix it with
      a new commit and go back to step 8. Rerun a job only when the failure is not from
      your change, once, and give the job and the reason in the hand-back.
- [ ] 12. Record the head: the full commit of the branch, the same locally and on the pull
      request.
- [ ] 13. Hand back in the format below.
      Never merge a pull request, and never merge into `main`: only the orchestrator does.
      Never tag, and never push to `main`. If you push
      again after handing back, the head has moved: hand back again with the new commit.
- [ ] 14. When the review asks for a change, which is the usual case: go back to step 4,
      test first, and follow the steps down again. After the push, if what the pull request
      says is no longer what the branch does, write the title and body again, go through
      and scan them, and put them on the pull request (`gh pr edit`, below). Wait for CI on
      the new head, and hand back again with the new commit: the next review is of that
      commit.
- [ ] 15. When the orchestrator says that `main` has moved: merge `origin/main` into your
      branch with a merge commit (this asks first), never with a rebase. On a conflict,
      resolve it only in your own files; when it is in a file you did not change, stop,
      abort the merge, and say why in the hand-back. Then steps 7, 8
      and 11 again: `just check`, `just scan`, the push, CI.
      Hand back the new head: it is reviewed again.

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

# 5: one crate's tests, one script test (the script test asks first)
just test-only -p <crate>
bash scripts/tests/<name>.sh                   # exit 0, and the last line is: finished: …

# 7 and 8
just check                                     # before a commit; holds for the tree it ran on
git add <file> <file>                          # by name
git commit -m "type(scope): summary"
git status --short                             # prints nothing: the commit is what was checked
just scan                                      # before every push
git push -u origin <slug>

# 9: after a refused push. <good> is the last commit before the first one the secrets scan
# named; after a clean-room refusal it is origin's tip of the branch, or the base when the
# branch was never pushed. It is never older than the branch on origin. The work stays,
# staged; take the material out of the file, then check, commit, scan and push as in 7, 8.
git rev-parse origin/<slug>                    # origin's tip of the branch, if it was pushed
git reset --soft <good>
git restore --staged <path>

# 10: the title alone in title.txt, the body in body.md, in a folder of their own. A
# variable does not last between separate shell calls: run these in one, or note the paths.
text=$(mktemp -d)
printf '%s\n' "type(scope): summary" >"$text/title.txt"
cat >"$text/body.md" <<'BODY'
What changed, and how it was checked.
BODY
cat "$text/title.txt" "$text/body.md"          # read first
# One scan of the two files, run from inside the repo: secrets, the values of the local
# .env among them; the clean-room scan; a credit line with nothing but marks before it;
# an agent's or a model's name; a path that starts with ~. It says where, never what.
# Exit 0 is the pass. A line it prints that starts "to read:" is not refused: go through it.
scripts/scan.sh --pr-text "$text/title.txt" "$text/body.md"
gh pr create --base main --head <slug> --title "$(cat "$text/title.txt")" --body-file "$text/body.md"
# 14: the same two files, written again, then gone through and scanned as above
gh pr edit <number> --title "$(cat "$text/title.txt")" --body-file "$text/body.md"
rm -rf "$text"

# 11: ask every 20 seconds until the run for the head lists three jobs, for 20 minutes
# at most. The macOS job takes about 3 minutes: give the shell call a longer time limit,
# or enter the loop again.
head=$(git rev-parse HEAD)
tries=0
until run=$(gh run list --commit "$head" --json databaseId --jq '.[0].databaseId // empty') &&
  [ -n "$run" ] && [ "$(gh run view "$run" --json jobs --jq '.jobs | length')" -ge 3 ]; do
  tries=$((tries + 1))
  [ "$tries" -le 60 ] || { echo "no run with three jobs for $head after 20 minutes"; break; }
  sleep 20
done
gh pr checks <number> --watch                  # check (macos-15), check (ubuntu-22.04), ci

# 12
git rev-parse HEAD                             # the head you hand back
gh pr view <number> --json headRefOid,url      # the same commit

# 15: main has moved, and the orchestrator asked (the merge asks first)
git fetch origin main
git merge --no-ff --no-edit origin/main
git merge --abort                              # only on a conflict outside your own files
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
Refused push: <which scan refused, the commit, path and line if named, and what was done;
    or "none">
Main merged in: <the commit of origin/main merged into the branch; or "no">

Acceptance
- <box or "Done when" part> -> <the test, or the file and line, that shows it>

Decisions
- <what was chosen where the task left a choice, and why>

Doubts
- <what the reviewer should look at hardest; "none" if none>

Deferred
- <item> -> <its row in docs/carry-over.md, added in this branch; "nothing" if nothing>
```
