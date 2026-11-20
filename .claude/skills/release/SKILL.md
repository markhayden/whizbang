---
name: release
description: "Cut a version release: choose the commit, tag a candidate, wait for the prerelease workflow, run the local leg, tag the version, check what was published. For the orchestrator only."
---

# release

For the orchestrator, and nobody else (`AGENTS.md`: only the orchestrator tags, releases and
publishes). It is a checklist: do the steps in order and tick each one. A step that fails
is a refusal: stop, tag nothing more, and say which step and why.

A release is a candidate that passed. The candidate is built once, by the `prerelease`
workflow; it is checked on both runners there and on the owner's Mac by the local leg
(`scripts/prerelease-local.sh`); and the `release` workflow publishes the candidate's own
archives, byte for byte, after its gate (`scripts/release-gate.sh`). Nothing is built for
the version tag.

**What the checks rest on.** The receipt is not signed and a draft candidate can be
changed by whoever may write to the repository, so the gate holds a release to what the
orchestrator did, not against the orchestrator: it is sound because only the orchestrator
tags, writes a release and runs the local leg, and it does not hold the tagged commit to
`main`, which step 2 does by hand. Within one run of `prerelease`, the archive that
`install` and `candidate` take from `build` is held to the `SHA256SUMS` that travels in the
same artifact, which shows it was not damaged on the way and nothing more.

## Inputs

- **Version:** the version to release, three numbers, as the workspace's `Cargo.toml`
  names it. A release of another version needs a pull request that changes it first.
- **Commit:** the full commit on `main` to release. When none is given, it is the tip of
  `origin/main` once step 2 holds for it.
- **Models directory and env file:** `WHIZBANG_MODELS_DIR` and `WHIZBANG_ENV_FILE`, set
  where they last, for the local leg. The env file is the owner's: it is named and never
  opened.

If the version or the commit is in doubt, ask the owner. Do not guess.

## Checklist

- [ ] 1. The commit and the next candidate number. Fetch `main` and every tag. The commit
      is on `origin/main`. The workspace's version at that commit is the version. The next
      number is one more than the highest `v<version>-rc.<n>` there is, on any commit, or
      1; a number is never used twice.
- [ ] 2. Before a tag: CI is green for that commit on both runners, the eight checks, in a
      run whose commit is the commit; and `just scan-range <base> <commit>` passes with the
      private list's variable set as `kickoff` says, where `<base>` is the last version
      tag, or the tag of the last milestone when there is no version tag yet. This is the
      same bar as before a merge, on the commit being tagged.
- [ ] 3. Tag the candidate on that commit and push the tag by its name. A pushed tag is
      never moved and never deleted: a correction is a new commit and the next number.
- [ ] 4. Wait for `prerelease` on the tag: `build` and `install` on `macos-15` and on
      `ubuntu-22.04`, then `candidate`, which makes the draft pre-release with both archives
      and `SHA256SUMS`. Never run `prerelease` again for a tag, whether it failed or
      passed: tag the next number. A second run would build other bytes under the same
      name.
- [ ] 5. The local leg, on the owner's Mac, from a checkout of the candidate's tag, so
      that the leg run is the candidate's own. It replaces the pack in the models directory
      with the one the candidate carries, spends well under one US dollar, and attaches the
      receipt to the candidate only when every check passed. Read what it printed and the
      receipt it wrote.
- [ ] 6. A candidate that failed, at step 4 or step 5, keeps its tag and its draft: nothing
      is deleted and nothing is attached by hand. The fix is a pull request, reviewed like
      any other; then start again at step 1 with the next number. An earlier candidate's
      receipt is never used for a later one: the gate takes only the highest number's.
- [ ] 7. The version tag, `v<version>`, on the same commit as the candidate that passed,
      pushed by its name. The `release` workflow runs the gate and then publishes. If the
      gate refuses, read its reason: the release is not made, and the version tag stays
      where it is; the next version tag is a new version.
- [ ] 8. Check what was published: download the release's files into a new folder, hold
      them to their `SHA256SUMS`, and compare that `SHA256SUMS` with the candidate's, byte
      for byte. The release is marked as a pre-release and holds the two archives and
      `SHA256SUMS`.
- [ ] 9. The record: the candidate's tag and number, the commit, the two runs, the
      receipt's digest and what the leg spent go into the milestone's note or the release's
      record, by a pull request. Hand back in the format below.

## Commands

`<version>` is the version, `<n>` the candidate number, `<commit>` the full commit.

```sh
# 1
git fetch origin main --tags
git merge-base --is-ancestor <commit> origin/main      # exit 0, or stop
git show <commit>:Cargo.toml | sed -n 's/^version = "\(.*\)"$/\1/p' | sed -n '1p'
git tag --list 'v<version>-rc.*'
scripts/tag-version.sh --candidate v<version>-rc.<n> <version>

# 2
gh run list --commit <commit> --json databaseId,workflowName,event,conclusion
just scan-range <base> <commit>

# 3: each of these asks first
git tag -a v<version>-rc.<n> <commit> -m "whizbang <version>, candidate <n>"
git push origin refs/tags/v<version>-rc.<n>

# 4
gh run list --workflow prerelease.yml --commit <commit> --json databaseId,headBranch,status,conclusion
gh run watch <run>
gh release view v<version>-rc.<n> --json isDraft,isPrerelease,assets

# 5: from a checkout of the tag, in a folder of its own; heavy
git worktree add --detach ../whizbang-leg v<version>-rc.<n>
(cd ../whizbang-leg && scripts/prerelease-local.sh v<version>-rc.<n> target/prerelease)
scripts/receipt-check.sh ../whizbang-leg/target/prerelease/prerelease-receipt.json tag=v<version>-rc.<n> commit=<commit>
git worktree remove ../whizbang-leg                    # once the receipt has been read

# 7: each of these asks first
git tag -a v<version> <commit> -m "whizbang <version>"
git push origin refs/tags/v<version>
gh run list --workflow release.yml --commit <commit> --json databaseId,headBranch,status,conclusion
gh run watch <run>

# 8: two new folders
files=$(mktemp -d)
gh release download v<version> --dir "$files/release"
gh release download v<version>-rc.<n> --dir "$files/candidate" --pattern SHA256SUMS
scripts/release-publish.sh check "$files/release" <version>
cmp "$files/release/SHA256SUMS" "$files/candidate/SHA256SUMS"
gh release view v<version> --json isDraft,isPrerelease,assets
rm -rf "$files"
```

## Hand-back format

```text
Release: v<version>, of commit <full commit>, on main
Candidate: v<version>-rc.<n>; earlier candidates of this version: <each number and why it failed, or "none">

Verified
- Before the tag: CI on the commit <the eight checks and their results>; just scan-range <base> <commit>: secrets <result>, attribution <result>, clean-room <result>
- prerelease: run <id>, <result>; build and install on macos-15 and ubuntu-22.04; the candidate is a draft with both archives and SHA256SUMS
- Local leg: <passed | failed at: the check>; receipt attached, SHA-256 <digest>; spent <amount>
- release: run <id>, <result>; the gate <passed | refused: its reason>
- Published: the release's files are their SHA256SUMS, which is the candidate's byte for byte; marked as a pre-release
- Record: <the pull request that adds it>
```

A refusal is handed back as: the version, the commit, the candidate if one was tagged, the
step that failed, what was seen, and who acts next. No tag is moved.
