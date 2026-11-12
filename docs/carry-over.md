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

The baseline package is assembled from the tagged squint; `docs/baseline/` says what it holds
and how it is identified. Its replay fixtures are converted to transcripts and audited:
`docs/baseline/transcript.md` and `docs/baseline/audit.toml`. The study is written up from
the package in `docs/results/2026-11-tool-study.md`.

## Fixes carried into whizbang

| From | Item | Milestone | Done when |
|---|---|---|---|
| whizbang | The title and body of a pull request are scanned by `kickoff` and `review-merge` for secrets, listed words and attribution lines, but four things are found only by reading: a bare agent or model name in a sentence, a value from the repo's local `.env` (the text is scanned in a folder of its own), a path that starts with `~`, and a `Co-Authored-By` line that follows a leading mark the stripped copy keeps (`1.`, `+`, a table bar, a backtick, `~`, `<!--`, `- [x]`, `_`). For the last, both skills run a `grep` in any letter case over the raw text that lists each such line for reading; nothing refuses it | 1A | A script scans a pull request's title and body for all four, with a test, and both skills use it |
| whizbang | A push made with `GIT_DIR` and `GIT_WORK_TREE` set from another folder runs no hook. Nothing prevents or detects clean-room material published that way on a branch other than `main`, or in a branch name: CI cannot run the clean-room scan, and `review-merge` scans only the head it merges | 1A | Such a push is refused or its material is scanned, with a test; or the owner accepts the gap in a line here |
| whizbang | The rule for an action's name and purpose (the top of `scripts/docs.sh`) is enforced only when the reference is generated; an action that breaks it can still be registered | 1A, with the execution contract | Registering an action whose name or purpose breaks the rule fails in Rust, with a test |
| whizbang | A script edited while it runs: bash reads a script as it goes, so a change to `scripts/scan.sh` during a scan can make the rest of the run a different program. The trap now fails a run that stops early, but not one that goes on wrongly | 1A | The scan and the other long scripts run from a copy or from a function read whole before it starts, with a test that edits the file during a run |
| whizbang | The scan relies on one engine, git grep, for every match. On CI's macOS runner that engine lost matches at random on several threads (the comment above `search` in `scripts/scan.sh`). The scan now searches on one thread, counts that every file it should cover was looked in, runs every search twice and fails when the two differ, and CI loops the search against its known output. None of that finds a match that the engine misses the same way every time | When the two-run check or the canary fires in practice | A second engine: the scan reads each listed file itself (and each blob of a commit) through another matcher and fails unless both report the same places |
| whizbang | The scan's time. Searching on one thread, counting what each search looked in (one `git ls-tree` for every commit; in the working tree a `find` of each level of folders that git does not ignore, with one `git check-ignore` for each level and one for the files, each asked through `xargs` and read by one `perl`, and one `perl` to look up the files on each side) and running every search twice more than doubled it. It grows with commits times paths. In the working tree it no longer grows with what ignored folders hold, since the walk does not enter them; it grows a little with how deep the folders go. Measured on one Mac, before the checks and with them: `just scan` 0.5 to 1.1 seconds; a range of 18 commits 3.8 to 10.1 seconds; the secrets scan over the whole history 14.6 to 32.3 seconds. Measured on one Mac, the whole scan, before the walk left ignored folders out and after: a tree of 4,000 files beside 50,000 ignored ones 2.6 to 1.3 seconds; `just scan` here beside a built `target/` of 1,300 files 1.4 to 1.5 seconds, the few more processes for each level costing more than so small a folder saves. Where nothing is ignored the walk costs more than the one `find` did: folders 30 levels deep 0.32 to 1.3 seconds; 20,000 folders at one level 4.4 to 7.0 seconds; 5,000 untracked folders 2.3 to 2.9 seconds. The scan no longer converts names (it asks git about them as arguments and compares files by device and inode): 500 files with decomposed names take 0.3 seconds and 1,500 take 0.5, as plain names do |
| whizbang | On macOS the working-tree scan refuses a file that is tracked under a decomposed name (an accent after its letter, as a commit made on another system can bring it) when an ignore rule written with the composed name matches it: git's list of tracked files that a rule matches compares the rule with the index's name and misses it, and git's search of untracked files sees the composed name, takes it for ignored and leaves it out. The refusal is right, since the file is not searched; it says only that a file was not looked in. Of 1,512 made-up trees with such names tracked, one was refused for this. To see it: `f=$(printf 'cafe\314\201.txt'); printf 'x\n' >"$f"; git -c core.precomposeunicode=false add -- "$f"; printf 'caf\303\251.txt\n' >.gitignore; just scan`. A range scan of the same commits is not affected | When a repository that holds such a file is scanned on macOS | The scan finds such files itself (the index's names, composed, asked of `git check-ignore`) and searches them by their name in the index, with a test |
| whizbang | The working-tree scan refuses when a tracked folder has since become a repository of its own (`git init` inside it): its tracked files are on the scan's list, and git's search of the working tree does not look inside another repository. The refusal is right and says only that files were not looked in | When it happens in practice | The refusal names this cause, or the scan searches those files by name, with a test |
| rascal | Say on the terminal when a run was not checked | 4 | The loop's output states checked, unchecked and why |
| rascal | Keep the cause when a check fails | 4 | A failed nested check returns its real error |
| rascal | A check cannot receive the variables it needs | 4 | Covered by nested calls; verified by the loop's acceptance case |
| rascal | An object on or near a seam line should be kept, not removed | Optional investigation | If pursued, it takes the ladder as a change to `seam` |
| rascal | A standing object's base and stand are left behind | Optional investigation | If pursued, it takes the ladder as a change to `remove_prompt` |
| rascal | Thin parts escape the mask | Optional investigation | If pursued, it takes the ladder |
| rascal | Large results spend most of a run encoding images and one large report page | 3 | Measured; thumbnails in reports; result encoding chosen from the measurement |
| rascal | Parallel installs of models sharing a file each download it | 1B | Two installs sharing a file download it once |
| whizbang | `grep -q` after a pipe, under `pipefail`: it stops at its first match, the command before it can be killed while writing, and a match then reads as a failure. It failed `scripts/tests/test-skills.sh` once on the Linux runner; that file no longer has one and refuses a new one. Other scripts under `scripts/` still have the pattern, on short inputs | 1A | No script under `scripts/` has a `grep -q` after a pipe, and `scripts/tests/test-syntax.sh` holds them to it |
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
| whizbang | `ci` has no Chrome end-to-end step: there is no playground to drive yet (`SPEC.md` sections 13 and 14) | 5 | `ci` runs the playground's acceptance cases in Chrome on every pull request |
| playground | A missing model cannot be picked in the model list | 5 | It can be picked, and Run explains what is missing |
| whizbang | The baseline gate needs a replay that writes a transcript in format 1 (`docs/baseline/transcript.md`), runs `whizbang_eval::baseline::compare` over every recorded decision and writes the gate receipt | 2 | Every recorded decision compares with no difference, and the receipt is written |
| whizbang | Transcript format 1 compares the bytes of tool images (crops, aligned frames) and the text of tool results. Whether whizbang's `view` and `align` reproduce the old tools' output exactly is not known until they are ported | 2 | They reproduce; or a new format version says what is compared instead, agreed with the owner before the gate runs |
| whizbang | Follow-ups from the review of the scan's census on random trees (file names it mishandled): see pull request #17. Anything that pull request leaves open is listed in this table by that pull request | 1A, first | Pull request #17 is merged, and each thing it left open has its own row here |
| whizbang | `whizbang-eval` turns on `serde_json`'s `arbitrary_precision`, so the comparison keeps a number's text. Once the binary depends on `whizbang-eval`, feature unification turns it on in every crate: every parsed `Value` keeps number text, an object whose only key is `serde_json`'s private number key reads as a number, and serde's buffered paths (untagged and internally tagged enums, `flatten`) misread numbers under it | 2, before the binary depends on `whizbang-eval` | Decided and recorded: the comparison parses in a way that does not reach other crates, or the workspace accepts the feature everywhere and a test covers each buffered path it has |
| whizbang | The 2026-11 study has one repetition for six of seven models. One comparison of thirteen excludes zero under the correction frozen for its run, and does not under a correction across the study (`docs/results/2026-11-tool-study.md`). Nothing has been run again | E | The comparison that excluded zero is repeated on a set no agent has seen, with at least two repetitions per arm, or the claim is withdrawn |
| whizbang | The study's write-up leaves out results by kind of failure: the kinds are the owner's private grouping of a set whose labels are not published | E | The owner decides whether the grouping may be published; if so, `scripts/baseline/tables.py` adds the table |

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
