# Milestone 0: inventory of the old repos

Task F0 of `m0-freeze-and-skeleton.md`. Read-only; taken on 2026-11-11 after a fetch of every
remote. Nothing in an old repo was changed to produce it. Every line has a disposition; none
is "discard", so nothing here needs the owner's agreement before task F2.

"On main" means the commit is an ancestor of that repo's `origin/main`.

## rascal

`main` and `origin/main` are both `c363fd2`. The tree is clean. The owner's survey
(`labs/segment-01/candidates.md`) is committed in `c363fd2`.

| Item | State | Disposition |
|---|---|---|
| Branch `m14-close`, local and remote, 1 commit ahead | In progress in the rascal session: the milestone 14 close | Merges through task F1 |
| Remote branch `m14e-birefnet`, 2 commits not on main | An export, an adapter and a parity note for a segmenter that is deferred | Archive tag `archive/m14e-birefnet`, then delete the branch |
| Remote branch `m14e-hqsam`, 1 commit not on main | The same, for another deferred segmenter | Archive tag `archive/m14e-hqsam`, then delete the branch |
| Worktree `rascal-m14-fix`, at a commit not on main | Clean. Belongs to the rascal session's close work | Left to the rascal session; removed after `m14` |
| Worktrees `rascal-m14-default`, `-m14-hand`, `-m14e-all`, `-m14e-birefnet`, `-m14e-evfsam2`, `-m14e-hqsam`, `-m14e-sam3`, `-m14r` | Each clean and on main; no ignored content beyond build output | Remove after `m14` |
| Worktree under the owner's hand-check folder | Clean, on main | Left alone: the owner's, in use for the hand check |
| Worktrees under the restore drill folder and the lab folder | Clean, on main | Left alone: not part of this cleanup |
| One worktree in a session's scratch space | Clean, on main | Removed by the session that made it |
| Other local branches | All on main | Delete |

## squint

Local `main` is `feb5a98`, 3 commits behind `origin/main` (`f288738`).

| Item | State | Disposition |
|---|---|---|
| 13 untracked files under `evalsets/matrix/` | The study's matrices and configs | Commit (task F3) |
| Remote branch `contract-0.6`, 2 commits not on main | The contract move to 0.6.0; the same change reached main through another branch | Archive tag `archive/contract-0.6`, then delete the branch |
| Worktree `squint-m14` | Clean, on main | Remove |
| A worktree under the owner's hand-check folder | Clean, on main | Left alone |
| Other local branches | All on main | Delete |
| Ignored: `eval-out/` (about 38 GB of recorded runs) | Not in git, and cannot be rebuilt without paying again | Stays in place. It is the source for the baseline package (track B) |
| Ignored: `.env`, `docs/brief.md` | Private | Stay on this machine; never copied into a repo |
| Ignored: evaluation-set images | Fetched by hash from the images repo | Can be fetched again |

## wafflery

`main` and `origin/main` are both `8d823a8`, which includes the `ui-tidy` merge. The tree is
clean.

| Item | State | Disposition |
|---|---|---|
| Local branch `m5-03-session-grant-boundary`, 2 commits not on main, not on the remote | Session-grant validation that never merged | Archive tag `archive/m5-03-session-grant-boundary`, pushed; then delete the branch |
| Local branch `m5-03-stable-session-grants`, 1 commit not on main, not on the remote | Contained in the branch above | Archive tag `archive/m5-03-stable-session-grants`, pushed; then delete the branch |
| Worktrees `wafflery-m14`, `wafflery-ui-tidy` | Clean, on main; ignored content is build output only | Remove |
| A worktree inside the coordination repo, and one under the owner's hand-check folder | Clean, on main | Left alone |
| Ignored: `ui/dist/`, `ui/storybook-static/`, `ui/test-results/` | Build and test output | Can be rebuilt |

## table-manners

`main` and `origin/main` are both `24ba4d5`, at tag `v0.6.0`. Clean; one worktree; every
branch is on main. Nothing to do before the freeze tag.

## Backlog reconciliation

Open items in each repo's task list and checkpoint reviews were compared with
`docs/carry-over.md`. These were missing and have been added there:

- rascal's milestone 14 follow-ups R1 to R6 (R7, a flaky test, was already listed).
- The checker's documented per-call cap of one cent, which cannot cover a squint check.

Items in the old task lists that are unticked but done (old checkpoints and tags) are not
carried over.

## Outcome

Recorded on 2026-11-11, after tasks F1 to F6.

- **Archive tags pushed and verified on the remotes:** `archive/m14e-birefnet` and
  `archive/m14e-hqsam` in rascal; `archive/contract-0.6` in squint;
  `archive/m5-03-session-grant-boundary` and `archive/m5-03-stable-session-grants` in wafflery.
  For each, the tag's commit was read from the remote and shown to contain the commit it
  preserves.
- **rascal:** `m14` and `m13` tagged by the rascal session. The checkpoint file at the freeze
  tag (`docs/reviews/m14-checkpoint.md`) records the second attempt, with part B run on the
  command line and the checker's per-call cap raised from 0.01 to 1.50. On the remote, `m14`
  points at `d8423e0`, which is `main` and the freeze tag, and `m13` at `661bd68`; every
  check run on both commits succeeded.
- **squint:** the 13 matrix and config files merged, and CI is green on the freeze commit; the
  remote branch `contract-0.6` deleted after its archive tag.
- **The images repo** is renamed `whizbang-evalsets`. Before the rename every sealed set was
  fetched into an empty copy (13 sets, 355 images, every hash checked). After it the same
  fetch through the old address gave identical files; no seal or manifest changed; and a
  request that bypasses the cache still succeeds at the old address.
- **The coordination repo** is retired and archived.
- **Freeze tags:** `final-before-whizbang` in all four repos. For each, local `main`, remote
  `main` and the peeled tag are the same commit, CI is green on it, and the tree was clean.

Checked again on 2026-11-11 with `git ls-remote` and a read-only look at the local checkouts.
The cleanup went less far than the dispositions above say, and nothing more is removed now
that the repos are frozen:

- **rascal:** the remote branches `m14e-birefnet` and `m14e-hqsam` still exist, at the same
  commits as their archive tags. They are the only remote branches in the four repos that
  are not on `main`. Local branches remain, all on `main`, and two worktrees beside the checkout.
- **squint:** local branches remain, all on `main`, and one worktree beside the checkout.
- **wafflery:** the local branches `m5-03-session-grant-boundary` and
  `m5-03-stable-session-grants` remain, at the same commits as their archive tags, with
  two worktrees beside the checkout.
- **All four:** merged remote branches were not deleted (rascal 17, squint 8, wafflery 5,
  table-manners 5); each is on `main`.

Three boxes of the plan rest on the orchestrator's record alone, because what they describe
is outside the four repos, and are left unticked until it confirms them: the two label
corrections in owner storage (F3), the local images clone pointing at the new address (F4),
and the retirement line pushed to the coordination repo (F5).

Left as found, by the owner's decision:

- Seven commits pushed on 2026-11-11 carry a co-author line naming a model: three in rascal
  (including the merge `m14` points at) and four in wafflery. History on a public `main` is
  not rewritten and a pushed tag is not moved. Whizbang rejects such lines in its hooks, and
  in CI for every commit of a pull request (task S2).
- One line in a rascal lab note names a private folder. It holds no contents and no secret.
