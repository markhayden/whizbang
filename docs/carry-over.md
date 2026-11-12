# Carry-over

The only backlog. It holds everything from the old repos that was not finished or not ported,
with where to find it and the milestone that owns it. An item leaves this list when it is built
in whizbang or deliberately dropped; a dropped item keeps one line saying why. Milestones are
those of `SPEC.md` section 16.

Old repos are `github.com/markhayden/{rascal,squint,wafflery,table-manners}`, frozen at the tag
`final-before-whizbang`.

Checked on 2026-11-11: no item-level answer for an unreleased evaluation set has been committed
to this repo.

## The freeze

Done on 2026-11-11. The four old repos are tagged `final-before-whizbang` and pinned in
`docs/sources.toml`. What was found and what happened to it is in `docs/plans/m0-inventory.md`.

Still to do in milestone 0, in whizbang: assemble and validate the baseline package from the
tagged squint, and write up the 2026-11-11 study from it.

## Fixes carried into whizbang

| From | Item | Milestone | Done when |
|---|---|---|---|
| whizbang | Check the title and body of a pull request for secrets, listed words and attribution: no scan sees them | 0, with `review-merge` in task S5 | `review-merge` runs both scans and the attribution check over the title and body before a merge |
| whizbang | A pull request can change the workflow that checks it; CI uses the base branch's scan but the pull request's own workflow file | 0, with `review-merge` in task S5 | `review-merge` refuses a change under `.github/` that the reviewer has not read |
| whizbang | The rule for an action's name and purpose (the top of `scripts/docs.sh`) is enforced only when the reference is generated; an action that breaks it can still be registered | 1A, with the execution contract | Registering an action whose name or purpose breaks the rule fails in Rust, with a test |
| whizbang | `scripts/tests/scan/planted.sh` failed once on CI's macOS runner (git 2.56): the working-tree scan did not report the planted key in `with space.txt`, though it reported the files beside it, and a rerun of the same commit passed. 420 runs on another Mac (git 2.54), alone and under load, did not repeat it. The scan handles that file's line like the others, so the loss is most likely inside one `git grep --untracked` call; if so, the real scan can miss a file the same way | 0 | The cause is found and fixed with a test. Until then the test prints, when a place is missing, the git version, the files and their sizes, what git lists and whether a second scan reports it |
| rascal | Say on the terminal when a run was not checked | 4 | The loop's output states checked, unchecked and why |
| rascal | Keep the cause when a check fails | 4 | A failed nested check returns its real error |
| rascal | A check cannot receive the variables it needs | 4 | Covered by nested calls; verified by the loop's acceptance case |
| rascal | An object on or near a seam line should be kept, not removed | Optional investigation | If pursued, it takes the ladder as a change to `seam` |
| rascal | A standing object's base and stand are left behind | Optional investigation | If pursued, it takes the ladder as a change to `remove_prompt` |
| rascal | Thin parts escape the mask | Optional investigation | If pursued, it takes the ladder |
| rascal | Large results spend most of a run encoding images and one large report page | 3 | Measured; thumbnails in reports; result encoding chosen from the measurement |
| rascal | Parallel installs of models sharing a file each download it | 1B | Two installs sharing a file download it once |
| whizbang | No test plants an unknown registry or git source for `cargo deny` (deferred from milestone 0, S1) | 1B | A planted unknown registry or git source fails `just deny` |
| rascal | SAM 3 resizes every candidate to full size before cutting to the instance limit; a low threshold on a large image holds hundreds of full-size planes (R1) | 1D | Candidates are cut to the limit before resizing, with a memory test |
| rascal | A model never shows as out of date: nothing compares an installation with the catalogue, and an adapter change does not change identities (R2) | 1B | An installation records its catalogue and adapter revisions; a mismatch is reported |
| rascal | Uninstall signals a process id read from a file, which may have been reused (R3) | 1B | A process is identified by more than its id before it is signalled |
| rascal | A configured model that is not in the catalogue can read as ready while the run refuses (R4) | 1B | Readiness reports it |
| rascal | A hold is judged live by its process id alone (R5) | 1B | A hold from a dead or recycled process is released |
| rascal | Untested cases around installs: a signal during a download, a reinstall between a call and its retry, a removal racing an install, a corrupt installation record (R6) | 1B | Each has a test |
| rascal | A local CI image that lacks the test runner its recipes use (R14) | Dropped | Whizbang has no local CI image; `just ci` runs the same recipes CI does |
| rascal | The documented per-call cap for a check is one cent, which cannot cover a squint check | 4 | A nested check draws on its parent's reservation, with an acceptance case; the guide gives a real figure |
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
| Every inherited tool that makes a quality claim (`change_map`, `edge_similarity`, each model-based rascal recipe): ported as unverified, then taken up the ladder | 2 and 3, then E |
| `regions`, `classes` and `outline`: wanted, not yet shown to work as intended. Each is brought in as an experimental tool and scored on rungs 1 and 2, on SAM 3 where that works, and on their old fixed-class segmenter (as a catalogue entry) where it does not. Those that pass go on up the ladder | E |
| A fresh, harder source-fidelity confirmation set: new families, unseen by any agent | E, before any rung 4 run |
| Rerun the top Google model after the squint fixes above | E |
| Other runtimes for local models, compared on speed, mask agreement and install size | When a measurement calls for it |
| Holding several models in the worker; concurrent model calls | When a measurement calls for it |
| Speed of local models (R12): a fill model on the CPU took 41 seconds a fill; one segmenter ran once per detected box; repeat runs were no faster, so models may have reloaded each run | 1C and 1D measure it on the GPU with a resident worker; milestone 8 for the fill model |
| Faster transfer between whizbang and the worker | When profiling calls for it |
| Segmentation that labels every pixel | When a tool needs it |
| A paid segmentation service as a backend | When one is wanted |
| Built-in TLS for the engine API | When a deployment needs it |

## Deferred until evaluated

| From | Item | Where it is | Note |
|---|---|---|---|
| squint | `regions`, `classes`, `outline` tools | `tools/python/src/squint_tools/` | See "Evaluations". The 2026-11-11 study found a clear gain only for `regions` on one model, and that `outline` ranked noise above the real change |
| squint | Fixed-class segmenter | `tools/python/MODEL.md` | Comes back as a catalogue entry if any of the three needs it |

## Not ported

| From | Item | Where it is | Why |
|---|---|---|---|
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
