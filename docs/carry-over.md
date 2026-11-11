# Carry-over

The only backlog. It holds everything from the old repos that was not finished or not ported,
with where to find it and the milestone that owns it. An item leaves this list when it is built
in whizbang or deliberately dropped; a dropped item keeps one line saying why. Milestones are
those of `SPEC.md` section 16.

Old repos are `github.com/markhayden/{rascal,squint,wafflery,table-manners}`, frozen at the tag
`final-before-whizbang`.

Checked on 2026-11-11: no item-level answer for an unreleased evaluation set has been committed
to this repo.

## To finish before the freeze (milestone 0)

| Repo | Item | Waiting on |
|---|---|---|
| rascal | Hand check, part B: rerun with the account variable exported | Owner |
| rascal | Checkpoint note, local CI run, tag `m14` | The hand check |
| rascal | Tag `m13` on the command-line path; its in-playground check moves to "Acceptance" below | `m14` |
| rascal | Remove about a dozen merged worktrees; delete the models scratch folder | The tags |
| wafflery | Merge `ui-tidy` (three interface fixes; CI passed) | `m14` tag |
| wafflery | Delete two stale session-grant branches and the merged `wafflery-m14` worktree | — |
| squint | Commit the 13 study matrix files | — |
| squint | Two label corrections agreed with the owner on 2026-11-11, recorded in owner storage only | — |
| squint | Delete the merged `contract-0.6` branch and `squint-m14` worktree | — |
| images repo | Rename to `whizbang-evalsets`; the old name is never reused | — |
| coordination repo | Push its local commits; mark it retired | — |
| all four | Tag `final-before-whizbang`; pin the commits in `docs/sources.toml` | Everything above |
| whizbang | Assemble and validate the baseline package from the tagged squint (`SPEC.md` section 16) | The tags |
| whizbang | Write up the 2026-11-11 study from the package, in `docs/results/` | The package |

## Fixes carried into whizbang

| From | Item | Milestone | Done when |
|---|---|---|---|
| rascal | Say on the terminal when a run was not checked | 4 | The loop's output states checked, unchecked and why |
| rascal | Keep the cause when a check fails | 4 | A failed nested check returns its real error |
| rascal | A check cannot receive the variables it needs | 4 | Covered by nested calls; verified by the loop's acceptance case |
| rascal | An object on or near a seam line should be kept, not removed | Optional investigation | If pursued, it takes the ladder as a change to `seam` |
| rascal | A standing object's base and stand are left behind | Optional investigation | If pursued, it takes the ladder as a change to `remove_prompt` |
| rascal | Thin parts escape the mask | Optional investigation | If pursued, it takes the ladder |
| rascal | Large results spend most of a run encoding images and one large report page | 3 | Measured; thumbnails in reports; result encoding chosen from the measurement |
| rascal | Parallel installs of models sharing a file each download it | 1B | Two installs sharing a file download it once |
| squint | A violation located wholly outside the candidate frame is rejected as an invalid submission | 2 | Such a submission is accepted or repaired, with a test |
| squint | Job requirements omit colour, count and lettering | E | A new set version built with full requirements; sealed sets untouched |
| squint | A test pair for a job that does and does not permit a seasonal cover over a protected subject | E | New fixture from fresh images |
| squint | Small models break the submission format | E | The redesigned format is admitted through its gate |
| squint | Open review follow-ups from its milestone 4 | 2 | Each is fixed, or dropped with a line here, as its module is ported |
| playground | A missing model cannot be picked in the model list | 5 | It can be picked, and Run explains what is missing |

## Evaluations

| Item | Milestone |
|---|---|
| Per-subject comparison on SAM 3, by name and by location | 2 (rungs 1 and 2), then E |
| `regions`, re-tested as an experimental tool | After per-subject comparison |
| A fresh, harder source-fidelity confirmation set: new families, unseen by any agent | E, before any rung 4 run |
| Rerun the top Google model after the squint fixes above | E |
| Other runtimes for local models, compared on speed, mask agreement and install size | When a measurement calls for it |
| Holding several models in the worker; concurrent model calls | When a measurement calls for it |
| Faster transfer between whizbang and the worker | When profiling calls for it |
| Segmentation that labels every pixel | When a tool needs it |
| A paid segmentation service as a backend | When one is wanted |
| Built-in TLS for the engine API | When a deployment needs it |

## Not ported

| From | Item | Where it is | Why |
|---|---|---|---|
| squint | `regions`, `classes`, `outline` tools | `tools/python/src/squint_tools/` | Unproven; per-subject comparison replaces them if it passes the ladder |
| squint | Fixed-class segmenter | `tools/python/MODEL.md` | Generic classes only; no instances, no phrases |
| squint | Six crates on crates.io at 0.1.0 | crates.io | Left as they are; never updated |
| rascal | Three further segmenters and their export scripts | `tools/python/src/rascal_tools/models/`, `export/` | Rank below SAM 3; milestone 8 may bring one back |
| rascal | Compatibility export of schemas | `schemas/copied/` | One repo; nothing to stay compatible with |
| table-manners | One session and budget per tool; the ban on accepting another tool's artifact types; fixtures copied between repos; renders declared as commands | `CONTRACT.md` | Written for separate programs. Its spend, estimate, readiness and install guarantees are kept (`SPEC.md` section 6) |
| table-manners | Separate publishing to crates.io | crates.io, 0.1.0 to 0.6.0 | Versioned with whizbang |
| wafflery | Tool registration, probing and child-process sessions | `src/tools/` | Tools run in the host's process |
| wafflery | WebKit browser tests | `ui/tests/browser/` | Chrome only |
| all | Per-repo SPEC, PLAN, task files and ADRs | Each repo's root and `docs/adr/` | Reference only; whizbang starts a new series |

## Flaky tests seen in the old repos

Not ported as they are; if the behaviour is kept, the test is rewritten to wait on events.

- wafflery: a browser bundle-replay test, about 1 failure in 8 on WebKit.
- rascal: a queued-cancellation test in its MCP crate, timed out once in CI.

## Acceptance checks moved here

- **Milestone 5:** the by-hand check of the remove-and-check loop in the playground, three
  cases: a clean pass, or a retry then a pass; a vague prompt showing its tries and its flag;
  an unset key leaving the run unchecked and saying why.

## Later

- Pixel, the image generation agent, as a third top-level tool.
- Seasonal presets for allowed changes.
- Recording a job's hash before generation.
- A click-to-outline interface in the playground on SAM 3's point requests.
- Holding the `whizbang` name on crates.io, at the first working binary.
