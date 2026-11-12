---
name: review-merge
description: "Merge one reviewed pull request at the exact commit that was reviewed, after checking the verdict, CI, both scans and the merged result. For the orchestrator only."
---

# review-merge

For the orchestrator, and nobody else, once a reviewer has given a verdict on a pull
request. It is a checklist: do the steps in order and tick each one. A step that fails is a
refusal: stop, merge nothing, and say which step and why.

This is not a review. The orchestrator does not review its own build: if this session wrote
any commit on the branch, the verdict comes from a reviewer that did not.

## Inputs

- **Pull request:** its number.
- **Reviewed head:** the full commit the reviewer named in its verdict.

If the verdict names no commit, or a short one, ask the reviewer for the full commit.

## Checklist

- [ ] 1. The verdict says `may merge at <full head>`, and the commit it names is exactly
      the reviewed head.
      A finding that blocks the merge and is still open, or a verdict for another commit, is a
      refusal. A follow-up does not block: note each one for step 9.
- [ ] 2. The pull request is open, is not a draft, and is against `main`. Its head on GitHub
      is the reviewed head. If the head has moved since the review, refuse: it is reviewed
      again.
- [ ] 3. Fetch `main` and the pull request's head. The fetched head is the reviewed head.
      Record the two commits: **base**, the merge base of `origin/main` and the head, and
      **head**. Every later step uses these two and no branch name.
- [ ] 4. The merged result is tested. Either the head contains the current `origin/main`,
      so CI ran on what `main` will become; or you merge the head into `origin/main` in a
      scratch worktree and `just check` passes there. A merge that applies cleanly is not
      enough, and neither is a green CI from before `main` last moved. A conflict or a
      failure is a refusal: abort the merge if it stopped half-way, remove the scratch
      worktree, and hand back; the builder merges `main` into the branch, and the new head
      is reviewed again.
- [ ] 5. CI is green for the head: the jobs `check (macos-15)` and `check (ubuntu-22.04)`
      and the aggregate `ci`, in a run whose commit is the head. If a job was rerun, note
      the job and the reason; a rerun with no stated reason is a refusal.
- [ ] 6. With `WHIZBANG_WORDLIST` set, `just scan-range <base> <head>` passes: secrets,
      attribution and the clean-room scan, over every commit in the range and the tree of
      the head. CI cannot run the clean-room scan, so this run is the only one.
- [ ] 7. The title and body. No scan sees them on the pull request, so save them to a file
      and scan it (the commands below), then read them yourself: no attribution line, no
      agent or model name, no private path or address, and every claim true of the final
      diff. Reading is the only check for a bare agent or model name in a sentence, a path
      that starts with `~`, a value from the repo's local `.env`, and a credit line behind
      a mark the copy keeps (a number, a `+`, a table bar): the scans of this step do not
      find them, and the last `grep` of this step lists every line to look at.
      You may correct the title or body yourself: a change to the text does not move the
      head. The corrected files get every check of this step again, the two scans, the
      attribution check on the stripped copy and the `grep`, and are gone through again,
      before they are put on the pull request; say what you changed in the hand-back.
      A correction that changes a claim about the code
      goes back to the builder.
- [ ] 8. Files that grant permissions or check the pull request. List the changed files.
      These count, because they grant permissions or are what `just check` and `just ci`
      run: any file under `.github/`; any file under a folder named `.claude`, at
      any depth; `.mcp.json`, at any depth; `justfile`; and any file under `scripts/`.
      For each one that changed: the reviewer's verdict names it and
      says it was read, and you read its diff yourself. Otherwise refuse. Each such file
      gets its own line in the hand-back.
- [ ] 9. Follow-ups are recorded by you; the reviewer only names them. Before merging,
      each follow-up of the verdict is one of two things: fixed on the branch, which gives
      a new head that is reviewed again; or yours to record, as a row in
      `docs/carry-over.md` that your next pull request adds, and that pull request is
      named in the hand-back. Nothing is left only in a verdict.
- [ ] 10. Fetch `main` once more. If it has moved since step 4, do step 4 again.
- [ ] 11. Merge with a merge commit, matched to the head. Never a squash, never a rebase
      merge, never past a failing check.
- [ ] 12. Confirm the merge. The pull request is merged; its merge commit is on
      `origin/main`, has two parents, and its second parent is the head. Watch `ci` on
      `main` for the merge commit; if it fails, tell the owner at once.
- [ ] 13. Clean up, once the merge is confirmed: bring the main checkout up to
      `origin/main`, remove the task's worktree and its local branch, and delete the branch
      on origin (this asks first). Removing a worktree deletes what git ignores in it, such
      as build output; a `.env` there is moved or deleted by hand first. For a worktree
      that `scripts/worktree.sh` did not make, where the branch is not named for the
      folder, use the plain commands below: they hold the same rule, that nothing is
      removed unless the branch is merged into `origin/main`.
      Earlier merged branches that are still on origin may be deleted the same way, each
      only once it is shown to be merged into `origin/main`; each delete asks first.
- [ ] 14. Hand back in the format below.

## Commands

`<pr>` is the number, `<head>` the reviewed head, `<base>` the commit step 3 prints.

```sh
# 2 and 3
gh pr view <pr> --json state,isDraft,baseRefName,headRefOid,title,url
git fetch origin main
git fetch origin refs/pull/<pr>/head
git rev-parse FETCH_HEAD                       # prints <head>, or refuse
git merge-base origin/main <head>              # prints <base>

# 4: exit 0 means the head contains the current main
git merge-base --is-ancestor origin/main <head>
# otherwise, the merged result in a scratch worktree
git worktree add --detach ../whizbang-merge-check origin/main
git -C ../whizbang-merge-check merge --no-ff --no-edit <head>
(cd ../whizbang-merge-check && just check)
git -C ../whizbang-merge-check merge --abort   # only when the merge stopped at a conflict
git worktree remove ../whizbang-merge-check

# 5
gh pr checks <pr>
gh run list --commit <head> --json databaseId,workflowName,event,conclusion,attempt

# 6
just scan-range <base> <head>

# 7: the title in title.txt, the body in body.md, in a folder of their own. A variable
# does not last between separate shell calls: run these in one, or note the paths.
text=$(mktemp -d) && git init -q "$text"
gh pr view <pr> --json title --jq .title >"$text/title.txt"
gh pr view <pr> --json body --jq .body >"$text/body.md"
cat "$text/title.txt" "$text/body.md"          # read first
repo=$PWD
# The checks. After a correction to either file, all of them run again on the two files.
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
# Only after a correction of your own, once the checks above pass on the corrected files.
gh pr edit <pr> --title "$(cat "$text/title.txt")" --body-file "$text/body.md"
rm -rf "$text" "$plain"

# 8
gh pr diff <pr> --name-only
git diff <base> <head> -- .github justfile scripts ':(glob)**/.claude/**' ':(glob)**/.mcp.json'

# 10 and 11
git fetch origin main
gh pr merge <pr> --merge --match-head-commit <head>

# 12
gh pr view <pr> --json state,mergeCommit
git fetch origin main
git merge-base --is-ancestor <merge commit> origin/main
git rev-parse <merge commit>^2                 # prints <head>
gh run list --commit <merge commit> --json databaseId,workflowName,conclusion

# 13, from the main checkout
git pull --ff-only origin main
scripts/worktree.sh remove <slug>
git push origin --delete <branch>              # the branch on origin; this asks first

# 13, for a worktree in <folder> on <branch> that the script did not make
git fetch origin main
git merge-base --is-ancestor <branch> origin/main   # exit 0, or stop: it is not merged
git worktree remove <folder>
git branch -d <branch>
git push origin --delete <branch>

# 13, for an earlier merged branch that is still on origin
git fetch origin
git merge-base --is-ancestor origin/<branch> origin/main   # exit 0, or stop: it is not merged
git push origin --delete <branch>              # this asks first
```

## Hand-back format

```text
Pull request: #<number> <url>
Reviewed head: <full commit>, verdict by <the reviewer>
Base: <full commit>
Merge commit: <full commit>, on main; second parent is the reviewed head

Verified
- Verdict: may merge at the reviewed head; no finding that blocks the merge is open
- Head unmoved: GitHub and the fetched head both at the reviewed head
- Merged result: <the head contains main | just check passed on the merge with main at <commit>>
- CI on the head: check (macos-15) <result>, check (ubuntu-22.04) <result>, ci <result>
  <any rerun: the job and the reason; otherwise "no rerun">
- just scan-range <base> <head>: secrets <result>, attribution <result>, clean-room <result>
- Title and body: scanned and read; <accurate | corrected by the orchestrator: what was
  changed, and checked again | sent back to the builder: why>
- Read: <each changed file that grants permissions or that just check and just ci run
  (.github, a .claude folder, .mcp.json, justfile, scripts), one line each; or "none">
- CI on main for the merge commit: <result>
- Cleaned up: worktree and local branch <branch> removed; the branch on origin deleted
  <any earlier merged branch deleted on origin, by name>

Follow-ups
- <each follow-up of the verdict> -> <fixed on the branch before the reviewed head | the
  pull request of mine that adds its row to docs/carry-over.md; "none" if none>
```

A refusal is handed back as: the pull request, the reviewed head, the step that failed, what
was seen, and who acts next. Nothing is merged.
