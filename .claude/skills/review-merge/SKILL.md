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

- [ ] 1. The verdict says the branch may merge, and says it for exactly the reviewed head.
      A verdict with open findings, or for another commit, is a refusal.
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
      failure is a refusal: the builder merges `main` into the branch, and the new head is
      reviewed again.
- [ ] 5. CI is green for the head: the two `check` jobs (macOS and Linux) and the aggregate
      `ci`, in a run whose commit is the head. If a job was rerun, note the job and the
      reason; a rerun with no stated reason is a refusal.
- [ ] 6. With `WHIZBANG_WORDLIST` set, `just scan-range <base> <head>` passes: secrets,
      attribution and the clean-room scan, over every commit in the range and the tree of
      the head. CI cannot run the clean-room scan, so this run is the only one.
- [ ] 7. The title and body. No scan sees them on the pull request, so save them to a file
      and scan it (the commands below), then read them yourself: no attribution line, no
      agent or model name, no private path or address, and every claim true of the final
      diff. If the text is wrong, the builder corrects it; a change to the text does not
      move the head.
- [ ] 8. Files that grant permissions or check the pull request. List the changed files.
      For each one under `.github/`, and each of `.claude/settings*.json`,
      `.claude/agents/*.md`, `.claude/skills/**` and `.mcp.json`: the reviewer's verdict
      says it was read, and you read its diff yourself. Otherwise refuse. Each such file
      gets its own line in the hand-back.
- [ ] 9. Fetch `main` once more. If it has moved since step 4, do step 4 again.
- [ ] 10. Merge with a merge commit, matched to the head. Never a squash, never a rebase
      merge, never past a failing check.
- [ ] 11. Confirm the merge. The pull request is merged; its merge commit is on
      `origin/main`, has two parents, and its second parent is the head. Watch `ci` on
      `main` for the merge commit; if it fails, tell the owner at once.
- [ ] 12. Clean up: remove the task's worktree and its local branch, and bring the main
      checkout up to `origin/main`.
- [ ] 13. Hand back in the format below.

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
git worktree remove ../whizbang-merge-check

# 5
gh pr checks <pr>
gh run list --commit <head> --json databaseId,workflowName,event,conclusion,attempt

# 6
just scan-range <base> <head>

# 7
text=$(mktemp -d) && git init -q "$text"
gh pr view <pr> --json title,body --jq '.title, "", .body' >"$text/pull-request.txt"
repo=$PWD
(cd "$text" && "$repo/scripts/scan.sh")        # secrets and clean-room, over the text
scripts/scan.sh --message "$text/pull-request.txt"   # the attribution check
rm -rf "$text"

# 8
gh pr diff <pr> --name-only
git diff <base> <head> -- .github .claude .mcp.json

# 9 and 10
git fetch origin main
gh pr merge <pr> --merge --match-head-commit <head>

# 11
gh pr view <pr> --json state,mergeCommit
git fetch origin main
git merge-base --is-ancestor <merge commit> origin/main
git rev-parse <merge commit>^2                 # prints <head>
gh run list --commit <merge commit> --json databaseId,workflowName,conclusion

# 12, from the main checkout
scripts/worktree.sh remove <slug>
git pull --ff-only origin main
```

## Hand-back format

```text
Pull request: #<number> <url>
Reviewed head: <full commit>, verdict by <the reviewer>
Base: <full commit>
Merge commit: <full commit>, on main; second parent is the reviewed head

Verified
- Verdict: may merge, for the reviewed head
- Head unmoved: GitHub and the fetched head both at the reviewed head
- Merged result: <the head contains main | just check passed on the merge with main at <commit>>
- CI on the head: check (macOS) <result>, check (Linux) <result>, ci <result>
  <any rerun: the job and the reason; otherwise "no rerun">
- just scan-range <base> <head>: secrets <result>, attribution <result>, clean-room <result>
- Title and body: scanned and read; <accurate | what was corrected>
- Read: <each changed file under .github/ or that grants permissions, one line each; or
  "no file under .github/ and none that grants permissions">
- CI on main for the merge commit: <result>
- Cleaned up: worktree and local branch <slug> removed

Follow-ups
- <item> -> <its row in docs/carry-over.md, or the pull request that adds it; "none" if none>
```

A refusal is handed back as: the pull request, the reviewed head, the step that failed, what
was seen, and who acts next. Nothing is merged.
