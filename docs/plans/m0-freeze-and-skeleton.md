# Plan: milestone 0, freeze and skeleton

Draft 4, 2026-11-11. From `SPEC.md` section 16. This file is the task list; there is no
separate one.

## What this milestone delivers

1. The four old repos finished, tagged `final-before-whizbang` and pinned, with nothing lost.
2. A whizbang repo where `just check` and `just scan` pass, CI runs, and an agent starting from
   a fresh clone has its rules, three skills and two subagents.
3. The baseline package, and the 2026-11-11 study written up from it.

Nothing of rascal, squint or wafflery is ported in this milestone.

## Decisions this plan makes

- **Three tracks.** Freeze (F), skeleton (S) and baseline (B). S0 comes first. After it, F and
  S run in parallel. B needs F6 (the tags) and S2 (the scans).
- **Nothing is deleted until it is reachable.** Every branch tip or worktree commit that is not
  an ancestor of its repo's `main` is given an `archive/<name>` tag and pushed before its
  branch or worktree is removed. A dirty file is committed, handed to its owner, or listed as
  deliberately discarded. Nothing is force-deleted to make a check pass.
- **Old repos are verified by their own rules,** not by whizbang's new scan. Whizbang's scan
  applies to whizbang.
- **The baseline package is private** and lives in owner storage at
  `~/whizbang-owner/baseline/2026-11/`. It holds every model's verdict and reasons per item of
  an exposed set. The public repo holds a manifest limited to an allowlist of fields. The
  offline gate of milestone 2 runs on the owner's Mac and leaves a receipt; the comparison
  code itself is tested in public CI on synthetic data.
- **The clean-room scan cannot run in CI,** because its word list never leaves the owner's
  machine. It runs locally before every push and again in `review-merge` before every merge.
- **A walking skeleton.** The skeleton ends with one command that works end to end,
  `whizbang describe --json`, and the generated reference checked against it. Other crates,
  `pack/` and `ui/` are created by the milestone that first needs them.

## Commit strategy

- **The bootstrap (S0) is one commit pushed to `main`,** because the things that would check
  it do not exist yet. It is verified by hand with the commands it adds.
- **After S0, one task is one branch and one pull request.** The builder subagent does the
  work and cannot merge. The reviewer subagent reviews it. The orchestrating session merges,
  with a merge commit, once CI and both scans pass on the exact head it reviewed.
- **`main` is always green.** History on `main` is never rewritten.
- **Commits inside a task** are whatever coherent steps pass the checks. Messages follow
  `type(scope): summary` and name no agent or model.
- **Rollback tag:** `m0`, when the exit check passes.
- **Old repos:** each freeze task is ordinary commits on that repo's `main`. A pushed tag is
  never moved. If a fix is needed after the freeze tag, it is a new commit, the tag
  `final-before-whizbang.1`, and a new pin.

### What undoing each task takes

A revert of a merge undoes a source change. It does not undo anything done outside git.

| Tasks | To undo |
|---|---|
| S0 | Revert the bootstrap commit, after reverting every later task; in practice S0 is corrected forward, not undone |
| S1 to S6 | Revert the merge, after reverting any later task that depends on it. Branch protection, installed hooks and local settings are changed back by hand |
| F1 | Source changes revert normally. A pushed `m13` or `m14` tag stays; a correction is a new tag |
| F2, F3 | Removed branches and worktrees are restored from their `archive/` tags |
| F4 | Rename the images repo back; restore the local remote. The old name must not have been taken |
| F5 | Unarchive the coordination repo |
| F6 | Tags stay. A wrong pin is corrected with a `.1` tag and a new `docs/sources.toml` |
| B1 to B3 | The private package is versioned by folder and never overwritten; a wrong package is superseded by a new version and a new manifest. Recapture spend is not recoverable |

## Track S: the whizbang skeleton

### S0. Bootstrap

One commit to `main`, by the orchestrating session.

- [ ] `Cargo.toml` workspace; `rust-toolchain.toml` at 1.99.1; edition 2024; `unsafe` forbidden;
      `unwrap` and `expect` denied outside tests.
- [ ] `crates/whizbang-core`: the static description type (a list of actions, empty).
- [ ] `crates/whizbang`: `whizbang describe [--json]` and `--version`; one integration test:
      `describe --json` is valid, lists no actions, and is byte-identical across two runs.
- [ ] `justfile`: `setup`, `check`, `scan`, `ci`.
- [ ] `scripts/scan.sh` (first version: secrets patterns over the tree; clean-room counts from
      a word list whose path comes from an environment variable; fails closed when the
      variable is unset or the list is unreadable or empty).
- [ ] `scripts/hooks/`: `pre-commit` (format check), `commit-msg` (rejects attribution),
      `pre-push` (`just scan`). `just setup` installs them and prints what it installed.
- [ ] `AGENTS.md` (first version: commands, boundaries, the no-attribution rule, where the spec
      and plans are) and `CLAUDE.md` importing it.
- [ ] `.claude/agents/builder.md` and `reviewer.md` as checklists: the builder works in a
      worktree and hands back a branch; the reviewer checks a branch against the spec and the
      task; neither merges.
- [ ] `.github/workflows/ci.yml`: `macos-15` and `ubuntu-22.04` jobs that run `just ci`, and
      one aggregate job named `ci` that passes only when both do.
- [ ] After the first green run: branch protection on `main` requiring the `ci` job.

**Done when:** on a fresh clone, `just setup && just check && just scan` pass; the first CI run
is green; a direct push to `main` is then refused.
**Size:** large, and deliberately one unit: none of its parts can be checked without the
others.

### S1. Check hardening

**Depends on:** S0.

- [ ] `scripts/crate-graph.txt` and `scripts/check-crate-graph.sh` with the edges of `SPEC.md`
      section 3; `just check` runs it.
- [ ] `deny.toml` (the old repos' licence allowlist; unknown registries denied).
- [ ] `.config/nextest.toml`: a per-test timeout of 30 seconds in the fast tier.
- [ ] `scripts/timing.sh`: seconds per test suite, compilation excluded; `just check` prints
      the table and fails when test time exceeds 180 seconds.
- [ ] A smoke of the release binary (`cargo build --release`, then `describe`) in `just ci`.
- [ ] `scripts/heavy.sh`: one heavy command at a time on this machine.
- [ ] Tool versions pinned in `just setup`.

**Done when:** `just check` prints a timing table; a branch that adds a forbidden crate edge
fails; a test that sleeps 31 seconds fails by timeout.

### S2. Scan hardening

**Depends on:** S0.

- [ ] The scan covers, for a pushed range, every outgoing commit's changed files, paths and
      message, and the final tree; the names of pushed branches and tags; and the message of
      an annotated tag. What is outgoing is worked out from the refs the destination remote
      already has and the updates being pushed; a commit being on local `main` is not
      evidence it was published. Every commit that becomes newly reachable on the remote is
      scanned. If the remote's state cannot be established, the scan fails.
- [ ] Patterns: provider key shapes; values from a local `.env` if one exists; absolute home
      paths; email addresses.
- [ ] CI runs the secrets scan and the attribution check over every commit of a pull request,
      so a commit made on another machine is still checked.
- [ ] Tests: a planted fake key fails; a key added in one commit and deleted in the next
      fails; a commit with an attribution line fails in CI; a key added and then deleted in commits that are on local `main` but
      not on the remote, pushed under a new branch name, fails; an annotated tag whose message
      holds a planted key, on a clean commit, fails; an empty word list fails; clean-room output is counts only.

**Done when:** each planted case fails, and `just scan` passes on `main`.

### S3. The generated reference

**Depends on:** S0.

- [ ] `just docs` writes `docs/reference/actions.md` and `docs/reference/description.json` from
      `whizbang describe --json`.
- [ ] `just ci` fails when the committed copies differ from a fresh generation.

**Done when:** a branch that changes the description type without regenerating fails CI.

### S4. Rules and documents

**Depends on:** S0.

- [ ] `AGENTS.md` in full, from the old coordination repo's rules cut to one repo: commands;
      where agents may work; the boundaries of `SPEC.md` section 15; secrets and private data;
      precedence between documents; the definition of done; the three roles (builder, reviewer,
      orchestrator) and that only the orchestrator merges; how to work with the owner; gotchas.
- [ ] A fresh-clone checklist at the top: run `just setup`; set the word-list variable; run
      `just scan` and see it pass.
- [ ] `.claude/settings.json`: allow the `just` recipes, `scripts/*`, read-only git and `gh`.
- [ ] `README.md`; `docs/adr/README.md` with the template and when an ADR is needed;
      `docs/guides/README.md` listing the guides to come with their milestones.

**Done when:** the reviewer subagent, given only a fresh clone and the question "what must
pass before you push, and who merges?", answers correctly from the repo.

### S5. Skills

**Depends on:** S4.

- [ ] `.claude/skills/kickoff`, `port-module`, `review-merge`, each a checklist.
- [ ] `review-merge` records the pull request's base and head commits, runs both scans over
      exactly that range, refuses if the head has moved since review, merges, and confirms the
      merged commit.
- [ ] `scripts/worktree.sh`.

**Done when:** each skill names its inputs, its commands and its hand-back format, and
`AGENTS.md` lists all three.

### S6. Exercise the workflow

**Depends on:** S1, S2, S3, S5.

- [ ] One small code-and-test task (add `--pretty` to `describe --json`, with a test) is done
      by the builder following `kickoff`, reviewed by the reviewer, and merged by the
      orchestrator following `review-merge`.
- [ ] One push that must be refused is attempted from the builder's worktree: a commit with a
      planted fake key. The pre-push hook stops it.
- [ ] The three skills and the two subagent files are corrected from what this showed.

**Done when:** the task is merged through a pull request with a green `ci`; the planted push
was refused; the corrections are merged.

### Checkpoint S

- [ ] On a fresh clone on the Mac: `just setup`, `just check`, `just scan` pass.
- [ ] `ci` is green on `main` on both runners, in under 10 minutes.
- [ ] The timing table is in the milestone note.

## Track F: freeze the old repos

Each task runs in the named repo, under that repo's own rules and checks.

### F0. Inventory

Read-only. Before anything in this track changes a repo.

- [ ] For rascal, squint, wafflery and table-manners, after a fetch: local and remote `main`
      commits; dirty tracked files; untracked files; every local and remote branch tip that is
      not an ancestor of `origin/main`; every worktree with its commit and whether it is clean.
- [ ] For every worktree that is a candidate for removal, its ignored contents too: run
      outputs, lab folders, local data and anything else git does not track. Each folder or
      file that could not be rebuilt gets its own line.
- [ ] A disposition for each line: merge, archive tag, hand to its owner, or discard with a
      reason. Written to `docs/plans/m0-inventory.md`.
- [ ] Known at the time of writing, to be confirmed by the inventory: rascal's checkout is
      behind its remote and has an uncommitted edit to `labs/segment-01/candidates.md` that
      belongs to the owner; wafflery has two session-grant branches that are not on `main`;
      squint has an unmerged `origin/contract-0.6` and a worktree on `m14-contract`.
- [ ] Reconcile the backlog: every open item in each repo's task list and checkpoint reviews
      (including rascal's milestone 14 follow-ups R1 to R6) is on `docs/carry-over.md` with a
      milestone, or dropped there with a reason.

**Done when:** the inventory lists a disposition for every line, and the owner has explicitly
agreed to each "discard" line.

### F1. Rascal: close milestones 14 and 13

Carried out by the rascal session; this plan tracks it.

- [ ] The owner's hand check, part B, with the checker's per-call cap raised so a squint check
      fits (the documented cap of one cent cannot cover squint's reservation).
- [ ] "Second attempt" recorded in `docs/reviews/m14-checkpoint.md`; local CI; tag `m14`.
- [ ] Tag `m13` at the commit its review names, with a line that the in-playground check moved
      to whizbang milestone 5.

**Done when:** both tags are on the remote, and every required workflow is green for the exact
commit each tag points at.

### F2. Preserve, then clean up

**Depends on:** F0, F1.

- [ ] Push an `archive/<name>` tag for every non-ancestor tip the inventory marks "archive".
- [ ] Commit or hand back every dirty file the inventory marks so.
- [ ] For ignored contents marked "keep": copy them to the place the inventory names and
      check the copy, or get the owning session's confirmation that it has them.
- [ ] Remove a worktree only when it is clean, its commit is reachable from `main` or a tag
      **on the remote**, and every ignored line for it has its outcome.
- [ ] Delete a branch only when its tip is reachable from `main` or a tag on the remote.

**Done when:** for every removed branch and worktree, the hash of a ref on the remote is read
with `git ls-remote` and the removed commit is shown to be an ancestor of it; and the
inventory shows the outcome of each ignored line.

### F3. Squint: commit the study inputs

- [ ] Pull `main`.
- [ ] Commit the 13 matrix and config files under `evalsets/matrix/`.
- [ ] Record the two agreed label corrections in owner storage, as a versioned corrections
      file beside the owner's labels.

**Done when:** `git status` is clean; CI green for that commit; `just sets-check` passes.

### F4. Rename the images repo

- [ ] Before: list each distinct repository and revision pair the sealed sets pin; record each
      set's seal digest.
- [ ] Rename `wafflery-evalsets` to `whizbang-evalsets`.
- [ ] After: for each distinct pinned revision, fetch that set's images through the **old**
      raw-content address into an empty folder, so every file is really downloaded, and check
      every hash. Check each seal digest is unchanged.
- [ ] If any fetch fails: rename back at once, and stop.
- [ ] Only then update the local clone's remote.

**Done when:** every pinned revision fetched and verified through the old address.
**Sealed manifests are not edited** to point at the new name.

### F5. Retire the coordination repo

- [ ] Commit a first line to its `AGENTS.md` and README (retired; see whizbang), and push it
      with its 8 local commits.
- [ ] Archive it.

### F6. Tag and pin

**Depends on:** F1 to F5, S2.

- [ ] In each of the four repos: local `main` equals `origin/main`; the tree is clean; that
      repo's own checks and scans pass; every required workflow is green for that exact commit
      (a workflow skipped for a documentation-only change is noted as such).
- [ ] Tag `final-before-whizbang` in each and push.
- [ ] `docs/sources.toml` in whizbang: for each repo, its URL, the tag, and the commit the tag
      points at (`git rev-parse <tag>^{commit}`).

**Done when:** each pinned commit equals local `main`, remote `main` and the peeled tag.

### Checkpoint F

- [ ] Four tags on four remotes.
- [ ] `docs/plans/m0-inventory.md` has no line without an outcome.
- [ ] The "To finish before the freeze" table in the carry-over list is empty.

## Track B: the baseline

The recorded material is under squint's `eval-out/`: the run named
`gate-outline-initial-2026-11` and the six named `study-*-2026-11`.

### B1. Assemble the package

**Depends on:** F6, S2.

Two parts, in `~/whizbang-owner/baseline/2026-11/`:

**Replay fixtures (the arm the old harness calls C).**
- [ ] For each decision, the authoritative attempt is the one its `result.json` names.
      Abandoned attempts are kept apart.
- [ ] Per decision: the journal, every stored exchange and tool artifact it references, and
      the prepared images.
- [ ] The input closure: the job and image files, the set's seal, the matrix, descriptor,
      config and prompts, the rules as resolved, the provider catalogue, the tool definitions
      as the arm projected them, the image-preparation version and settings, and the
      dependency lock files.
- [ ] A private inventory with one line per input and the kind of evidence that ties it to
      the historical run:
      - **Recorded digest** (config, prompts, catalogue, set components, tool definitions):
        hash the file the way the old harness did and require it to equal what the run's
        descriptor or journal recorded. A mismatch blocks the package; it is resolved by
        finding the right file, never by recording the new digest.
      - **From a revision** (lock files, code the run did not hash): name the full commit of
        the harness that ran and the path, keep those bytes, and mark the hash as taken at
        assembly time.
      - **Preparation:** match the recorded preparation version and the hashes of the prepared
        images against settings derived from the historical config, catalogue and code.
- [ ] No input is left unexplained. Files gathered at assembly time are kept apart from
      historical ones.

**Study inputs (every arm and repetition).**
- [ ] Each decision's result and report, without image payloads.
- [ ] The labels, the corrections with their version, the family assignments, the scoring
      policy, the bootstrap seed and resample count, the multiple-hypothesis correction, and
      the spend ledgers.

**Provenance, kept as three separate records.**
- [ ] The harness version each run was executed with, read from that run's own descriptor.
- [ ] The squint commit the package was assembled from (the freeze tag).
- [ ] The commit used for any recapture.

**The public manifest,** `docs/baseline/manifest.toml`, holds only: the package version; one
digest over the package; per model and arm, the decision count and spend; the three
provenance records. It holds no item names, file names or verdicts. The digest is computed
the same way every time: over the sorted relative paths and each file's hash.

**The gate receipt** that milestone 2 will write is defined here: the package digest, the
comparison code's version, the whizbang commit and the result.

**Done when:** the manifest's counts match the old runs' own summaries for every model and
arm; every recorded digest matches; and the private inventory has no unexplained input.

### B2. Convert and audit

**Depends on:** B1.

- [ ] A versioned transcript format, independent of any provider, written down in
      `docs/baseline/transcript.md`: per decision, the ordered judge requests (system text,
      history, tool definitions, limits), each response with its content kept whole including
      provider-opaque parts, each tool call tied to its result, the image ids and how each
      image was prepared, repairs, excluded turns, and the terminal outcome.
- [ ] `scripts/baseline/convert.py` turns each replay fixture into a transcript.
- [ ] Every call a judge made gets one of four dispositions, and the audit checks what each
      requires: executed, with its result; rejected, with its result; excluded, never
      dispatched because its turn was excluded; or the final submission, which ends the run
      and has no result message.
- [ ] `scripts/baseline/audit.py` checks every transcript: every referenced file exists and
      its hash matches; no stored file is marked truncated; requests are in order; every call
      satisfies its disposition; the terminal outcome matches the old result.
- [ ] A stored file cut short by the recorder is incomplete. A response the model itself cut
      short at its token limit is complete and is kept as recorded. Nothing is added to a
      history to make the audit pass.
- [ ] A decision whose record is whole is complete, even when its outcome is an error. A
      recorded model error is kept as it is and never run again to get a verdict.
- [ ] A decision whose record is incomplete is listed. It is captured again with the tagged
      squint, stored under `recaptured/`, converted and audited like any other, and excluded
      from the historical study numbers.
- [ ] If a recapture is incomplete for the same reason (the recorder cuts files over 2 MiB),
      recapturing stops. The recorder is fixed in squint, tagged `final-before-whizbang.1` and
      pinned, and the replacement is captured with that.
- [ ] The comparison code that milestone 2 will use. It compares the whole ordered sequence
      of requests, calls and results, and the terminal outcome; any difference not declared
      volatile fails. Its tests run in public CI on synthetic transcripts: an unchanged
      transcript passes; each of these fails: a changed prompt, image, tool definition or
      opaque block; a changed final verdict; a verdict replaced by an error; a changed tool
      name or argument; two calls in the other order; a missing turn; an extra turn. A field
      declared volatile does not fail. Synthetic
      cases also cover an excluded turn holding a call, a final submission, a truncated
      stored file (incomplete) and a response cut at the token limit (complete).

**Done when:** every selected decision has a complete, audited original or a complete,
audited replacement, and the mutation tests pass.
**Spend:** recapture only, limit 20 US dollars across every attempt. If more would be needed,
stop and ask the owner; completeness is not waived. A first look found 1,144 selected results, no truncated or
missing file, and six recorded errors for one model.

### B3. Write up the study

**Depends on:** B2.

- [ ] `scripts/baseline/tables.py` computes every number from the study inputs alone.
- [ ] `docs/results/2026-11-tool-study.md`: what was tested, the method, the table per model
      and arm, the tool effects with corrected intervals, self-disagreement where a second
      repetition exists, the error analysis, spend, and the limits of the result (one
      repetition for six models; an exposed set).
- [ ] Aggregates only: no item names and no item-level verdicts.

**Done when:** rerunning the script reproduces every number in the document.

### Checkpoint B

- [ ] `docs/baseline/manifest.toml` is committed and its digest matches owner storage.
- [ ] Both scans pass on everything track B added to the repo.

## Exit check for milestone 0

- [ ] Checkpoints S, F and B pass.
- [ ] `docs/plans/m0-note.md`: what shipped, the timing table, spend.
- [ ] Tag `m0`.

## Risks

| Risk | Effect | What we do |
|---|---|---|
| The hand check finds a rascal bug | F1 stalls, and F2 and F6 with it | S and the code of B2 proceed. A fix needed to tag is made; anything else goes to the carry-over list |
| The inventory finds work nobody remembers | Cleanup would lose it | Everything gets an archive tag unless the owner says discard |
| The old recordings cannot be turned into complete transcripts | Milestone 2 has no gate | B2 finds out now; recapture within the limit, or ask |
| The rename breaks a sealed set's fetch | Old sets cannot be fetched | F4 tests every pinned revision through the old address and renames back on failure |
| A push from a machine without the word list | Unscanned content reaches the public repo | The hook fails closed; `review-merge` scans the exact head before merging |
| Agents merging with branch protection on | A merge is blocked, or protection is bypassed | One aggregate `ci` job is the only required check; merges go through pull requests |

## Needs the owner

1. The rascal hand check, part B (F1), in the rascal session.
2. A look at the "discard" lines of the inventory (F0).
3. Whether the uncommitted rascal survey in `labs/segment-01/candidates.md` is committed
   before the freeze.
