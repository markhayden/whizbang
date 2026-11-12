# Milestone 0 note

Milestone 0, "freeze and skeleton", is complete as of 2026-11-11. Its plan is
`m0-freeze-and-skeleton.md`; this note says what shipped, how long things take, what was
spent, and what the next milestone starts from.

## What shipped

**The skeleton (track S).** A workspace that builds and a workflow that is enforced, not
described.

- `whizbang describe` in three forms, with the reference under `docs/reference/` generated
  from it and checked on every `just check`.
- `just check`: formatting, lints, the crate edges, `cargo deny`, the script tests, the
  reference, and a time limit on the tests. `just ci` adds the advisory database and a smoke
  of the release binary. CI runs it on macOS and Linux, and one aggregate job is the required
  check on `main`.
- The scans: secrets, attribution and the clean-room scan, over the working tree, over every
  commit a push would publish, and over a range. The hooks run them; CI runs the first two
  for every commit of a pull request, with the base branch's copy of the scan.
- The rules (`AGENTS.md`), the permission settings, three skills (`kickoff`, `port-module`,
  `review-merge`), two subagent files and `scripts/worktree.sh`, exercised end to end on one
  small task and corrected from what that showed.

**The freeze (track F).** The four old repos are tagged `final-before-whizbang` and pinned in
`docs/sources.toml`; the images repo is renamed and still answers at its old address; the
coordination repo is retired. What was found and what happened to it is in `m0-inventory.md`.

**The baseline (track B).** The old squint study is kept as a private package, identified in
`docs/baseline/manifest.toml` by one digest. Its replay fixtures are converted to transcripts
in a format milestone 2 can write, and audited: 1,144 decisions, all complete
(`docs/baseline/audit.toml`). The comparison milestone 2's gate will use is in
`crates/whizbang-eval`. The study is written up, aggregates only, in
`docs/results/2026-11-tool-study.md`.

## Pull requests

| # | What |
|---|---|
| 1 | The inventory of the old repos |
| 2 | Check hardening: crate edges, timeouts, timing, deny, release smoke |
| 3 | The pins of the frozen repos and how the freeze went |
| 4 | Scan hardening: what a push would publish, commit by commit |
| 5 | The generated reference and its staleness check |
| 6 | The baseline package's manifest |
| 7 | The rules, the permission settings, the README, the ADR and guide indexes |
| 8 | Transcripts, their audit, and the gate's comparison |
| 9 | A script that stops early fails; the scan searches on one thread |
| 10 | Closed without merging: the hunt for the scan's lost matches on CI |
| 11 | The 2026-11 tool study |
| 12 | The scan counts what a search looked in, runs every search twice, and has a canary in CI |
| 13 | The three skills and the worktree script |
| 14 | `describe --json` prints one line, `--pretty` indents it (the exercise of the workflow) |
| 15 | Corrections to the skills and subagent files from the exercise |
| 16 | The scan's census does not enter ignored folders |
| 17 | Open when this note was written: three kinds of file name the scan's census mishandles (`m0-scan-names`); it has a row in `docs/carry-over.md` |
| 18 | This close-out |

## Timing

Measured on 2026-11-11 at `main` 27d3fc3.

| What | Where | Time |
|---|---|---|
| CI, `check (ubuntu-22.04)` | GitHub, run for 27d3fc3 | 1 min 47 s |
| CI, `check (macos-15)`, with the 30 s search canary | GitHub, run for 27d3fc3 | 3 min 42 s |
| CI, whole run including the aggregate `ci` | GitHub, run for 27d3fc3 | 3 min 53 s |
| `just setup` | Fresh clone, the Mac | under 1 s |
| `just check`, first run, nothing built | Fresh clone, the Mac | 82 s |
| `just check`, second run | Fresh clone, the Mac | 72 s |
| The script tests alone (`scripts/tests/run.sh`) | Fresh clone, the Mac | 72 s |
| The Rust tests alone, compilation not counted | Fresh clone, the Mac | 0.8 s |
| `just scan` | Fresh clone, the Mac | 2 s |
| `just scan-range` over one merge and its 10 commits | Fresh clone, the Mac | 5 s |

The Mac was busy with another agent's test runs while these were taken (load average about
13 on 16 cores), so its numbers are high: the same checks took 14 to 30 s on a quiet machine
earlier in the day. CI is the comparable figure, and it is under the 10 minutes the plan
allows. Almost all of `just check` is now the script tests, which run one after another;
running them side by side is the obvious saving and is not done yet.

## Spend

0 US dollars. Milestone 0 made no model call: the baseline was assembled, converted and
audited from recordings, and no decision needed capturing again.

## What CI and the reviews found that mattered

Each pull request that changed code, a script or the rules was built by one agent and
reviewed by another before the orchestrator merged it. Five things found that way, or by
CI, changed what the milestone delivered.

- **The scan lost matches on CI's macOS runner.** This one was found by CI, not by a
  review: the scan's own test failed three times on that runner, on three unrelated
  branches, and passed each time it was run again. A hunt on the runner (pull request #10)
  found the cause: `git grep` on several threads, in the C locale, with an extended regular
  expression, dropped a match at random with no error, in about one search in five to one
  in thirty depending on how many searches ran at once. The scan could have reported clean
  on a file holding a key. It now searches on one thread, counts the files each search
  looked in, runs every search twice, and a canary on that runner repeats the search for
  30 seconds on every run.
- **A script that failed could exit 0.** On the bash 3.2 that macOS ships, a script with an
  `EXIT` trap exits 0 after a syntax error, an unset variable and several other faults. Every
  script is now parsed before any test runs, the hooks parse the scan before trusting it, and
  each trap fails a run that did not reach its end.
- **The first permission settings allowed arbitrary commands in advance.** A wildcard over
  `just` and over `scripts/` let through any command, including a push. The settings are now
  exact commands, held to an exact list by a test, and the rules say plainly that settings
  reduce prompts and are not the boundary.
- **The study's one clear result is weaker than first reported.** One comparison of thirteen
  excludes zero under the correction frozen for its run, and its lower end is +0.0 under a
  correction across the study. The write-up says so in its first paragraph.
- **The blind set is exposed.** Every item of `source-fidelity/blind-v3` has been seen by an
  agent. It is a regression set now, and a later held-out claim needs a new set.

## Known limits, carried forward

Each has a row in `docs/carry-over.md`.

- A push made from another folder with git pointed at the repo runs no hook; nothing on
  the way to a branch other than `main` then runs the clean-room scan.
- The scan relies on one engine for every match, and its time grows with commits and paths.
- File names the scan's census mishandles (pull request #17, in work).
- What only reading finds in a pull request's title and body.
- The tests on the rules and skills hold some lines exactly and cannot see a new sentence
  that contradicts them; such changes are read in full.

## What milestone 1A starts from

- `main` at the commit tagged `m0`, with CI green and both scans clean.
- The plan `m1a-contracts-and-store.md`, already reviewed in three rounds.
- An empty registry: `whizbang describe` lists no actions, and `crates/whizbang-core` holds
  only the description type.
- The frozen repos at their pins, to port from with the `port-module` skill.
- The baseline package, its transcripts and the comparison, for milestone 2's gate.
