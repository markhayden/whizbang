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
| whizbang | The scan's time. Searching on one thread, counting what each search looked in (one `git ls-tree` for every commit; in the working tree a `find` of each level of folders that git does not ignore, with one `git check-ignore` for each level and one for the files) and running every search twice more than doubled it. It grows with commits times paths. In the working tree it no longer grows with what ignored folders hold, since the walk does not enter them; it grows a little with how deep the folders go. Measured on one Mac, before the checks and with them: `just scan` 0.5 to 1.1 seconds; a range of 18 commits 3.8 to 10.1 seconds; the secrets scan over the whole history 14.6 to 32.3 seconds. Measured on one Mac, the whole scan, before the walk left ignored folders out and after: a tree of 4,000 files beside 50,000 ignored ones 2.6 to 1.3 seconds; `just scan` here beside a built `target/` of 1,300 files 1.4 to 1.5 seconds, the few more processes for each level costing more than so small a folder saves | When the pre-push scan of an ordinary pull request's commits takes more than about 30 seconds, or `just scan` more than about 10 | One of: the census once for each distinct tree rather than each commit; one `git ls-tree` process for many commits; the second engine of the row above, in place of the second run |
| whizbang | An image whose colour profile is not an RGB matrix profile (one that works through lookup tables alone, a grey or CMYK profile, a profile on a grey image) is refused, and so is a JPEG of four components (CMYK): ADR 0001 gives them no canonical pixels. Colour information a PNG carries outside an ICC profile (its gamma, chromaticity, sRGB and coding-independent chunks) is not read, and such a file is taken as sRGB | 2, when squint's image loading is ported | Each has a canonical reading in a new ADR, with vectors; or the refusal and the reading as sRGB are kept, with a line here saying why |
| squint | What `crates/squint-core/src/image.rs` holds beside identity: finding the projection from metadata and the warning for an undeclared 2:1 image, the checks on a path before it is read, the expected and pinned digests of a job's image, the digest of the pixels as stored, and image preparation for providers | 2 | Each is ported with its module, or dropped with a line here |
| squint | A location (a box) on a crop mapped back to the crop's root, with clipping and the partial flag, from `crates/squint-core/src/frames/mod.rs`. ADR 0001's bindings map points; `view` returns a crop and needs a location on it mapped back | 1A, with A10 | A box on a crop of a crop maps to its root through the bindings, with a test |
| squint | The rest of what `crates/squint-core/src/frames/` holds beside points: polygons and views as locations, their validity rules, footprints cut at the seam, locations through a homography or a rotation, and the perspective view into a panorama, which is not one of ADR 0001's mappings | 2 | Locations map through every kind of binding; the view mapping has its ADR and its kind in `binding/1` |
| rascal | What `crates/rascal-core/src/frame.rs` and `mapping.rs` hold beside the mappings: the strip width chosen from a fraction of the width, and the mapping of locations with splitting at the seam and clipping | 3 | Ported with `cut`, `put` and the operations that take locations |
| whizbang | For a lossy file, the pinned decoder's output is part of what `pixels/1` means (ADR 0001). Nothing yet carries a store from one decoder version to the next: no record of which pixel version an identity was made under beyond the `pixels/1` tag, and no way to re-identify stored images | Before the first change of the pinned `image`, `zune-jpeg` or `zune-core` version | The JPEG vectors pass unchanged under the new version; or a new pixels version is introduced by an ADR, with how identities made under the old one are recognised and migrated |
| whizbang | A JPEG is refused when its markers do not reach the end marker, and that is all that is checked. A JPEG that has its end marker and too little before it is decoded as far as it goes, with the decoder's filler for the rest, and is given an identity: a scan with too little data, and also a scan header followed directly by the end marker. The review of this check found that the decoder's strict mode does not report these either, so turning it on is not the fix | 2, when squint's image loading is ported | A JPEG whose scans do not hold the data their frame calls for is refused, by counting what is decoded or by another decoder's report, with tests for a short scan and for an empty one; or the limit stays, with a line here saying why |
| whizbang | A point a mapping computed is brought back onto an edge only when it is at most 1e-9 of a pixel past it (the fixed slack in `land`, `crates/whizbang-core/src/identity/space.rs`). A point on the edge of a heavily magnified crop of a crop can come back about 6e-9 of a pixel outside, and is refused | 2, when `view` returns crops of crops to a judge | The slack is scaled to the mapping, or such a point is shown to land, with a test on a heavily magnified crop of a crop |
| whizbang | The shipped build compiles a colour management library (`moxcms`, with `pxfm`) that the decode path never calls: `image` 0.25.10 depends on it without a feature to turn it off | When `image` makes it optional, or at the next change of the pinned version | The feature is turned off and the two crates leave `Cargo.lock`; or a line here says why they stay |
| rascal | Left behind by the port of the workspace's publication (`crates/rascal-core/src/workspace/`): the registry's operation records (`registry.rs`, `commit.rs`); cancellation honoured before a commit and never after | 1A: the operation records with A5, cancellation with A7 | Each is built by its task, or dropped with a line here |
| rascal | The lock that marks a staging directory as in use, and the rule that a staging directory younger than five seconds is never taken for abandoned (`crates/rascal-core/src/workspace/staging.rs`) | Dropped | Whizbang removes staging only under the writer lock, which one process holds for as long as it writes, so no writer can meet the staging of another that is alive, and nothing is judged by its age |
| rascal | Repair's repaired generation: an entry whose payload is missing is dropped from a new generation (`crates/rascal-core/src/workspace/repair.rs`). The other half of repair, collecting the files nothing names, is `whizbang store prune` | Dropped | The spec has no repair command, and the store does not rewrite what it holds to hide a loss: reading the entry reports the missing file by its digest, and deleting the run that names it removes the reference |
| whizbang | A writer's own reads are not held against its own prune. A reader in another value or process shares the generation it reads, and prune leaves that generation and what it names; `Store::snapshot` shares nothing, so a process that writes a store, deletes runs and prunes while one of its own threads still reads an older generation takes that generation's content away | 1A, with nested calls and publication (task A7), where one process first runs actions and holds the store | A run executing in the writing process keeps reading what it read through a deletion and a prune, with a test; or the executor refuses a prune while runs execute |
| whizbang | The crash tests of publication, creation and deletion lose unsynced names one directory at a time; only the tests of prune also lose them one name at a time (`Lost::Names`). Every mask at every event of a publication multiplies its cases by two to the power of the names a crash finds not yet durable; that was not built or timed | 1A, at the exit check (task A12) | Publication's crash tests run under `Lost::Names` as well: every mask in a slower tier, or a seeded sample in the fast one |
| whizbang | `whizbang store list`, `delete` and `prune` are told where the store is by `--store <dir>` on every call; there is no default location and no setting | 1A, with the first command that runs an action against a store (task A9) | The store's location has one source that every command shares, and the store commands use it |
| whizbang | "Never removed" holds for spend and request-id records only by how the store is built: no command removes a name that belongs to no run. A publication can still give such a name other content (`Publication::put` replaces), and the content it had is then named by nothing and goes at the next prune | 1A, with request ids and admission (task A6), which first publishes such records | Each record is published under a name that is never used twice, and a test puts records in a store, deletes every run and prunes through the real commands, and reads every record back |
| whizbang | The writer lock and a reader's hold are the standard library's file locks, `flock` on Linux and macOS (`crates/whizbang-core/src/store/fs.rs` says what follows from that). A store on a network file system is not supported: there the lock may fail or exclude nobody, and on Linux over NFS prune fails, since it takes a file opened only to read alone. And `LOCK` must never be removed or replaced while a writer holds it: the next writer locks the new file and is admitted beside the first. Only a replacement at the moment the lock is taken is caught | When a store on a network file system is asked for, or before `whizbang serve` holds a store for days (milestone 5), whichever is first | The store refuses a file system on which its locks do not hold, or is tested on it; and a writer notices that its `LOCK` is no longer the file at that name before it publishes, with a test |
| rascal | What `crates/rascal-core/src/mask.rs` holds beside the record: the counts per zone, the box of the pixels with any weight, whether a mask is empty, and the inspect report | 3, or the first operation that needs one (task A9, `grow`) | Each is ported with the operation that reads it, or dropped with a line here |
| rascal | Two rules of `crates/rascal-core/src/location.rs` that `location/1` (ADR 0002) does not hold: a polygon whose edges cross is not refused, and nothing ties together the parts of one shape that was split at a panorama's seam (its `group`) | 3, with the operations that take locations | Each is held by the type, with a test, or dropped with a line here |
| rascal | What `crates/rascal-core/src/artifact.rs` says of each type to a caller: its purpose in a sentence, whether it has a preview, and how it leaves a store | 1A, with the action descriptor (task A5), whose ports name the types | `whizbang describe` says it of each type; or it is dropped with a line here |
| whizbang | An artifact's origin (ADR 0002) holds each step's action, version, arguments and inputs. It does not hold the run that made the artifact or the models the run used: neither has a type yet | 1A, with the run record (task A5) | The origin names the run and the models, as a new version of the origin, with a test; or the run record alone holds them, with a line here saying why |
| whizbang | A mask leaves a store only as the file the store holds under `objects/`: `whizbang store import` brings a canonical artifact file in, and no command writes one out by type and identity | 3, with rascal's doors (its `export` and `import` tools, rascal ADR 0022) | A command and a tool export an artifact as its canonical file, with a test that the file imports into another store with the same identity |
| squint | Left behind by the port of the artifact store (`crates/squint-core/src/artifacts.rs`, `journal.rs`): the run journal, appended and synced one event at a time, whose torn last line is dropped on reading; secrets redacted from a text body before it is stored; a long body cut short with its full digest recorded | 1A, with the run record (task A5), for the journal and redaction; 2, with the judge loop, for cut bodies | The run record's diagnostics are written through the store and redacted first, with tests; cut bodies are kept or dropped when the judge loop is ported |
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
| rascal | The workspace's typed registry of frames, locations, artifacts and plans, with its local ids and the refusal of a frame id bound to a second identity | `crates/rascal-core/src/workspace/registry.rs`, `commit.rs` | An artifact is stored under its type and its identity (ADR 0002), so there is no local id to give out or to bind twice; frames are replaced by bindings (ADR 0001) |
| rascal | The mask as a directory (`mask.toml`, three PNG planes, the profile's files) and the tar bundle a mask travelled in, with its manifest and frame chain | `crates/rascal-core/src/mask.rs`, `bundle.rs`, its ADR 0022 | Replaced by the one canonical file of ADR 0002, which holds a mask whole and is the same bytes in every store |
| rascal | Identities hashed from canonical JSON, the contract's names for the types (`rascal:Mask`) and the one schema version they shared | `crates/rascal-core/src/canonical.rs`, `artifact.rs`, its ADR 0016 | Identities are hashed as ADR 0001 hashes them; each type's kind carries its own version (ADR 0002); one repo has no second tool to tell its types from |
| rascal | Frame ids (`raw:<W>x<H>:<projection>`, `cut:<image-id>:<plan-id>`, `crop:<n>`), the frame table and its parent chain, and the check of a frame record against its parent | `crates/rascal-core/src/frame.rs` | Replaced by bindings (ADR 0001), which name images by digest and are checked when built and when read; nothing is named by a local id |
| squint | Frame ids (`raw:<image>`, `aligned:<image>:<transform>`, `crop:<call>:<n>`) and the per-run frame registry | `crates/squint-core/src/frames/mod.rs` | Replaced by bindings (ADR 0001). What a judge model calls a frame in a conversation is a name for a binding, and belongs to the judge loop |
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
