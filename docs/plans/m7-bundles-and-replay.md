# Plan: milestone 7, bundles and replay

Draft 1, 2026-11-24, written from `main` at 6c75b6e3 (the tag `m6`, the merge of #300, the
note of milestone 6). Milestone 7 is `SPEC.md` section 16, row 7, "Bundles and replay":
"Invocation bundles and journal replay; saved-run comparisons, saved examples and guided
chain replay in the playground". Beside the row it takes section 6's control "Invocation
bundles | Deferred to milestone 7", section 7's rascal row "Replay from a journal; invocation
bundles | Deferred | Milestone 7" and its playground row "Comparisons between saved runs;
saved examples; invocation bundles; guided chain replay | Deferred, required | Milestone 7",
every carry-over row that names milestone 7 (the table "Carry-over rows owned by milestone
7", below), and what `docs/plans/m6-note.md` says milestone 7 owes: those clauses, "replay
from the store", and the rows milestone 6 opened or re-owned for 7.

After milestone 7 come E, then 8 (the owner's decision of 2026-11-24, `SPEC.md` section 16).
This plan takes none of their work: no tool climbs the ladder here, no model is added, and
no local fill or judge is built.

**Words this plan uses.**

- **A bundle** is an invocation bundle: one file that carries a call, or a chain of calls,
  with everything it needs to run again once the store it came from is gone (table-manners'
  `CONTRACT.md`, section 4.9, at the tag `final-before-whizbang`, is the reference, never
  the authority). Its format here is `bundle/1` (B1, ADR 0048).
- **A step** is one call in a bundle: one ended run of the store, its record, its inputs and
  what it kept. A step's run may have children (a recipe's steps, a loop's attempts); they
  travel with it.
- **A chain** is a bundle of two or more steps in which a later step took, as an input, an
  output an earlier step made: a removal sent to "Check an edit", say.
- **Replay** is a stored run made again. **Offline replay** answers every capability
  request from what the run recorded and calls nothing: no model, no provider, no spend
  ("journal replay"; R1). **Live replay** makes the call again for real, as a new run under
  admission and the allowance. Both compare what they made with what the run made.
- **Guided chain replay** is a chain replayed one step at a time in the playground: each
  step opens as an ordinary draft and runs only when the person presses Run (U2).
- **A saved example** is a named starting point a person saved from an ended run: its task,
  its inputs and its settings, held by the store, opened as a draft (U1).
- **A saved-run comparison** is two ended runs of one task, one pinned as the baseline,
  shown side by side: their pictures at one zoom, their inputs and settings with what
  differs marked, how each ended, its time and its cost (U1). It says nothing of which is
  better.
- **The designs** are G1's: the new surfaces drawn in Paper, in the hybrid dark style of
  milestone 5's file, approved by the owner before the playground's tasks build them.

## What the owner decided

1. **The order after milestone 6 is 7, then E, then 8** (2026-11-24, `SPEC.md` section 16).
   Milestone 7 takes nothing of E's (the ladder, the clicked removal's shaping, the gates) or
   8's (fill and judge local and self-hosted, the front's fault forms, its bridge's cancel
   window).
2. **The playground's surfaces are designed in Paper first** (as milestone 5's were,
   `docs/plans/m5-the-playground.md`, "Words this plan uses", "A screen"), in the hybrid
   style, dark screens only, components kept in light too. The orchestrator draws them with
   the owner (G1).
3. **Spend controls stay a guard and a record** (the owner's standing rule, `SPEC.md`
   section 6): nothing new is built to hold spend in this milestone beyond what a replay must
   do to keep the ledger right.
4. **A plan is approved by two or more review rounds** (`SPEC.md` section 14, "Review before
   building"); building starts once they are recorded, and the owner is told.

## What the spec says, and where it is silent

**What it says.** Each of these binds the tasks below.

- Section 3, "The execution contract": request ids bound to a fingerprint and looked up
  before preparation; prepare binds a call to the digests of its inputs, config and models;
  admission reserves the bound before anything is sent; "Outputs and the completed status
  are committed in one step"; "Diagnostic evidence (the journal, intermediate images, a
  failed turn's exchange) is always kept, marked as diagnostic, and can never be used as an
  output".
- Section 3, "Storage": "Everything is kept until deleted"; `store list|delete|prune` remove
  runs "and what only they reference"; "Copies that left the store are out of its reach";
  "One writer"; invariants: protected pixels never change, a rendering cannot be used where
  a mask is required, publication is atomic, every artifact records what produced it, "A
  mask can be exported from one store and imported into another without loss".
- Section 3, "What every run records": every group, always, for every outcome; "Secrets are
  redacted before anything is written"; the models group names "the backend, the model and
  its version or reported identity, the device, and whether the answer came from a cache";
  section 4: the device is "recorded in the run".
- Section 6, "Spend": "A paid request with no enforceable bound is refused before
  dispatch"; "Dispatch is recorded first"; "After a crash, nothing that was dispatched is
  replayed automatically"; "A request whose cost is unknown is charged at its bound".
- Section 6, "Controls from table-manners": "Invocation bundles | Deferred to milestone 7";
  "Artifact export and import | Kept".
- Section 7: rascal's "Replay from a journal; invocation bundles | Deferred | Milestone 7";
  the playground's "Comparisons between saved runs; saved examples; invocation bundles;
  guided chain replay | Deferred, required | Milestone 7"; and, kept at milestone 5, "A saved
  run gives a usable command for a pipeline, from its saved inputs and resolved settings",
  "Saved history owns a snapshot of its inputs", "Send a result to another action; rerun into
  an independent draft".
- Section 9: a judge's decisions are never read from the cache in evaluation, "every
  repetition is a fresh trial"; "What a judge actually saw is recorded as provenance, not
  used as a key".
- Section 10: the playground API is loopback only, behind its guard; "File paths from the
  caller | Inside allowed folders" (milestone 5 took none, and this plan keeps it so);
  the engine API exposes no local control; "Model and tool text escaped before display".
  Egress: default deny; "Every run records where data went".
- Section 13: the fast tier's 300 s and one test's 30 s; the Chrome tier's 240 s, 120 a
  shard; "a budget is not raised without the owner".
- Section 14: a plan's review rounds; "The spec, the reference and the carry-over list are
  updated when the change affects them".
- Section 16, row 7, and the order 6, 7, E, 8.

**Where it is silent, or where the code is short of it.** Each is settled in "Choices this
plan makes", is a task's item, or is an open question; none is settled silently.

1. **What a bundle is.** The spec names it and says no more. The old contract's bundle
   (table-manners `CONTRACT.md`, section 4.9) carried steps, each with its resolved call, its
   original result, its inputs byte for byte and its artifacts' export files, with
   `reference_only` and `unportable` lists, in a fixed ustar named by its SHA-256. Whizbang
   has pieces of it: an artifact's canonical file and `store export`/`store import` (ADR
   0002; `crates/whizbang/src/export.rs`), the playground's "Export run", a tar of a run's
   record, report and outputs (`crates/whizbang/src/report.rs`, `tar_file`), and "Copy
   command" (`crates/whizbang/src/command_of.rs`), which names inputs by their identity in
   one store and so does not travel. Settled in "Bundles".
2. **What "journal replay" replays.** A journal here is a run's progress events alone
   (`contract/journal.rs`, the module's head), and nothing replays from it. Rascal's
   `inspect --replay` re-ran an operation's record on its stored inputs and diffed the
   planes, and its `--replay <recorded run>` answered fillers, checks and the pack from the
   recording, "never a call" (rascal `crates/rascal-core/src/replay.rs`; rascal `SPEC.md`,
   the replay of a heal). Settled in "Replay".
3. **Where a replay gets each model's answer.** A segmenter's answer is in its run: the
   masks are committed outputs, and the diagnostic `segment.txt` holds each mask's score and
   box (`crates/whizbang-rascal/src/segment.rs`, `DIAGNOSTIC`, `line_of`). A fill's answer is
   its run's committed image. A judge's turns are kept whole as diagnostics
   (`contract/journal.rs`, `EXCHANGE_PREFIX`), except an opaque part, whose body is never
   stored (ADR 0009; `docs/carry-over.md`, "A run cannot be sent again from what the store
   holds of it"), and a picture, stored as a digest. The carry-over row asks that "whatever
   resumes or replays a run says where it gets an opaque part from, or that it cannot".
   Settled in "Replay" and R1, item 3.
4. **What a replayed run's record says.** `run/6` has no field naming the run or bundle it
   replays, and a model's entry names its device as one word, `cuda`, not the card
   (`contract/record.rs`, `Model`; `capability/served.rs`, `is_device`; the carry-over row on
   the card). Settled in "The record a bundle carries".
5. **Where a saved example lives.** Drafts live in the browser's storage and are a
   convenience (`ui/src/drafts.ts`, `DRAFTS_KEY`); an example must outlive the run it came
   from and keep its inputs from collection and from `store delete`. Settled in "Saved
   examples".
6. **Whether bundles are offered over the engine API or the stdio server.** The spec lists
   bundles among the interface's controls (section 6) but names no door. Settled in
   "Bundles", "Which doors".
7. **What a server held for months may let go of** (the carry-over row on the engine API's
   collected uploads: "Milestone 7 keeps runs and their bundles for replay, and so decides").
   Settled in "What the store keeps for replay".

## Choices this plan makes

Each is recorded in an ADR written before code depends on it: ADR 0048, "Invocation bundles"
(B1), which also records `run/7`; ADR 0049, "Replay" (R1); and ADR 0041, "The playground
host", amended by U1 and U2 for their routes.

### Bundles

**One file, `bundle/1`, an uncompressed ustar that equal contents make byte for byte.**
Each member's header is fixed (mode 0644, uid and gid 0, no names of users, mtime 0),
`bundle.json` comes first and every other member follows in the order of its name's bytes,
so the archive's SHA-256 names it, as table-manners' did. The playground's existing tar
writer (`report.rs`, `tar_file`) is moved to the core's `bundle` module and made to write
those fixed headers; its one caller keeps its output.

| Member | Holds |
|---|---|
| `bundle.json` | The manifest (below), at most 1 MiB |
| `runs/<run id>/record` | Each step's run record and each of its descendants', as the store holds it, byte for byte |
| `runs/<run id>/journal` | Its journal, as stored |
| `runs/<run id>/diagnostics/<name>` | Each diagnostic it kept (its judge's exchanges and `segment.txt` among them) |
| `artifacts/sha256-<hex>` | The canonical file (ADR 0002) of every input, every output and every artifact a kept diagnostic or exchange names, once each, whatever runs share it |

**The manifest.** `{"bundle": 1, "whizbang": <version>, "steps": [...], "artifacts":
[...], "withheld": [...]}`:

- `steps`, one or more, in the order they ran: each names its run id, its action and its
  version, the digest of the config its settings came from, and, for each input port, either
  the artifact's identity or **the earlier step's output it took** (`{"step": <index>,
  "port": <name>, "place": <n>}`) when its identity is one an earlier step made. A step
  never names a path.
- `artifacts`: each member's identity, type and SHA-256 of its file.
- `withheld`: what the bundle does not carry and why, each with a sentence: an opaque part
  of a judge's exchange (its adapter, digest and length, as ADR 0009 keeps it); a picture a
  stored exchange names that the store does not hold; a diagnostic the store let go of.
  A bundle with anything withheld still bundles; its offline replay says, before it starts,
  which step cannot be answered and why (R1).

No secret is in a bundle: every member is a file the store already holds with its secrets
taken out (section 3), and B1 adds nothing but the manifest, which holds no value of the env
file, no path and no account. The bundle is not encrypted and holds the person's pictures
and words: the guides say so.

**Reading one trusts nothing.** A bundle is read whole before anything acts on it: at most
256 MiB in all, 4,096 members, a manifest of 1 MiB; every member the manifest names is
there and no other is; every artifact's SHA-256 is checked and its canonical file decoded
under the core's limits (ADR 0001, ADR 0002); every record is read by the record's own
strict reader (`contract/record.rs`) and every journal by the journal's; a member name that
is absolute, holds `..`, a link, a device or any type but a plain file is refused; an
unknown manifest version is refused naming it. A refusal names the first fault and says
that nothing was imported and nothing ran.

**Importing one** brings its artifacts into a store through the store's import path, in one
publication, with the import mark S1 gave imports (`whizbang store prune
--unused-imports-older-than`), and the bundle itself as a small entry, `bundles/<digest>`,
which names its artifacts and holds its manifest and its runs' records. A bundle's runs are
never made runs of the importing store: their records are evidence of another store, read
by replay and shown by the playground, and no history, stats or ledger counts them. An
import of a bundle already held changes nothing.

**Which doors.** The command line and the playground. Not the engine API: a bundle carries
a store's files out and brings files in that name runs of another store, which is the kind
of local control section 10 keeps off it, and an app finds its runs by id already. Not the
stdio server: its tools are the description's actions (ADR 0031), and a bundle is not an
action. A door is not added (section 15, "Ask first").

### The record a bundle carries

**`run/7`**, written only where a run has what it adds, as `run/5` and `run/6` were (ADR
0005): `run/6` with two things after it.

- **A model's card**: beside `device`, the card the worker named on its load, at most 64
  printable characters, or null (the carry-over row on the card; the adapter already names
  it, `pack/src/whizbang_pack/adapter.py`, `CARD_NAME_MOST`). The pack's protocol is not
  changed for it: the worker's load diagnostics already carry the card, and the supervisor
  and the kind `remote` read it from there (B1, item 6).
- **`replayed`**: the run this run replays (its id, and the bundle's digest when it came
  from one), whether offline or live, and the step's index in a chain.

Every older version reads as it did; nothing in a store is migrated.

### What the store keeps for replay

- **A run keeps everything replay needs** already: its record, journal, diagnostics and
  outputs are removed only by `store delete` and `prune` (section 3). Nothing new is kept.
- **A bundle's entry and a saved example** each name what they hold, so `store delete` and
  `prune` keep those artifacts, and the uploads' collection does not let them go.
- **The engine API's collected uploads** (the carry-over row): the hourly collection now
  ends in the store's own prune of what no name holds, run through the writer the server
  holds, under the readers' holds the store already honours (ADR 0036): an upload's bytes
  go with its name. B1, item 7.

### Replay

**Offline replay is the default, and calls nothing.** `whizbang runs replay <run id>` (or
`--bundle <file> [--step <n>]`) makes a new run of the same call, with the same arguments,
settings and inputs, through the same executor, with **recorded capability handles** in
place of the configured backends: each capability request the replayed run makes is
answered from the original run's record, in the order the original made them, and from
nothing else:

| Capability | Answered from |
|---|---|
| `segment` | The original `segment` run's committed masks, with each one's score, box and predicted IoU from its `segment.txt` |
| `fill` | The original `fill` run's committed image |
| `judge` | Its stored exchanges, turn by turn: each request the replay makes is compared with the one recorded, and the recorded answer given |

A request the original did not make, or made otherwise (another model, another request, a
request past the last recorded), is answered with an error of its own kind, `diverged`,
which ends the replayed run there and is reported as the first place the two differ. An
answer the store cannot give (an opaque part withheld, a picture not held, a diagnostic
gone) ends it before that request as `unanswerable`, naming what is missing: the carry-over
row's "says where it gets an opaque part from, or that it cannot". Pixel work runs for real:
every operation and every step of a recipe is computed again. Offline replay sends nothing
anywhere, spends nothing (its records hold no paid request), needs no key and no worker.
The replayed run is a run of the store, marked `replayed` (`run/7`), so it is listed,
stored and deletable like any other; its runs are never in a stats figure as ordinary ones
(R1, item 6).

**What it compares**, once the replayed run has ended: its outcome, its committed outputs by
identity, port and place, each step's action and arguments, a check's checklist and
verdict, and a loop's attempts. The report is one of **reproduced** (all the same),
**differs** (each difference by place, first first), **diverged** or **unanswerable** (as
above). A replay of an `interrupted` run, or one whose record lost its admission, is refused
before it starts.

**Live replay** (`--live`) makes the call again with the configured backends, as a new run
under preparation, admission and the allowance, with its own request id when one is given:
no recorded answer is used, and what it spends is in its record and the ledger like any
run's. It is compared as offline replay is, and a difference is reported, never an error:
"A live judgment (a model's verdict) is not promised to repeat; only the call is" (the old
contract's words); nor are a model's pixels. A live replay of a step whose input is withheld
is refused before it is prepared.

**Where a replay may run.** In the store its run is in, or in any store a bundle is imported
into: replay reads the bundle's runs from its entry, never as that store's own. It is one
more call of an action, so it takes the store's one writer (section 3): from the command
line while nothing holds the store, and from the playground through its host.

**Not built.** Resuming a run a dead process left (section 6: nothing dispatched is replayed
automatically, and a replay is a new run, never the old one carried on); replay over the
engine API or the stdio server ("Which doors"); a replay that reconstructs a missing input.

### Saved examples

An example is the store's: a small entry `examples/<id>` (a random id of 16 hex digits),
`example/1`, holding a name of at most 80 characters and notes of at most 2,000 that a
person gave, the task, its arguments, its settings as the run resolved them, each input by
its identity, and the run it was saved from. It names its inputs, so they outlive the run's
deletion. Opening one fills a draft and runs nothing, and readiness is asked again as for
any draft; a later run from it is an ordinary run. Examples are made, renamed and deleted in
the playground; `whizbang store list` names them, and `store delete --example <id>` removes
one. At most 1,000 in a store. No examples ship with the binary: a tool record's worked
examples stay in its record (`docs/reference/tools/`).

### Saved-run comparisons

Two ended runs of one task (the same action), one pinned as the baseline. The host answers
the comparison from the two records (`GET /api/compare/{id}/{id}`): what each was asked,
arguments and settings side by side with each difference marked, inputs by identity with a
changed one marked, outputs by port, outcome, time and cost, the models each used. The
pictures are shown with the result's compare component (`ui/src/components/Compare.tsx`),
labelled Baseline and Attempt, side by side, and swiped or as a difference only when both
are the same size. The pin is in the page's address (`#/compare/<baseline>/<attempt>`) and
in the browser's storage for that address, so a reload keeps it; the store holds no pin.
The comparison claims no winner and computes no score.

### Guided chain replay

A chain is made from History: up to 16 ended runs picked in the order they ran, bundled by
B1 (a later run's input that is an earlier one's output becomes a link; an input two
earlier outputs could be, or a link to a run not picked, refuses the chain naming it). A
chain, or any bundle a person drops on the page, is imported through the host
(`POST /api/bundles`, its bytes; no path), and **Start the chain** opens its first step as
an ordinary draft. Each step runs only when the person presses Run, reviewing it first as
any draft: its cost reviewed before a paid run as always. **Continue** reads how that run
ended and opens the next step, its linked inputs bound to the new run's output by port and
place, and shows where that output's identity differs from the one recorded. A step whose
link finds no output, or two, stops the chain there. A run started from a chain is recorded
`replayed` with its bundle and step. The chain's place (which step, which runs) is the
page's, kept in the browser's storage for its address: closing the tab loses nothing of a
run, and a reopened chain is read again from the runs and the bundle. This is guided
replay, not unattended orchestration: nothing runs two steps on its own.

## The designs (G1), and what they ask of the engine

G1's surfaces, each a screen or a state of one in the Paper file "whizbang playground", on
a page of its own beside "Playground · hybrid", dark screens, the components on
"Components" in both themes:

1. **Compare two runs**: the baseline and the attempt, pictures at one zoom with their modes,
   the asked and resolved settings with differences marked, inputs, outcome, time, cost and
   models; how a baseline is pinned from a result and from History, cleared, and swapped; the
   empty state (one run picked), and the refusal for two tasks.
2. **Saved examples**: "Save as example" on a result (name, notes); where examples are
   listed (Home and the kit workspace); an example opened as a draft, saying which example
   it came from; renaming and deleting; the state of an example whose task this build no
   longer has.
3. **Bundles**: "Bundle this run" on a result; picking runs in History for a chain, in order,
   with a link shown between two runs and the refusals (ambiguous, a link outside the
   chain); a bundle dropped on the page, what it holds and what it withholds.
4. **Guided chain replay**: the chain's steps as a list with their states (to review,
   running, ended, stopped, blocked); a step open for review; Continue, with a changed
   identity shown; Stop; the end of a chain.
5. **Replay of one run**: "Replay" on a result (offline, and live with the cost reviewed),
   and the replay's report: reproduced, differs (by place), diverged, unanswerable.
6. **Home's library** with pictures any door gave (the carry-over row on Home), if the owner
   wants it drawn; otherwise U1 shows them in Home's present design.

G1 ends with a table, "What the design shows that the engine cannot back yet", in milestone
5's form (`docs/plans/m5-the-playground.md`), written into this plan by the orchestrator in
G1's pull request: each element the engine lacks, and whether it is left out, shown in the
engine's terms, or added by a named item of U1 or U2. A change of the design that changes
what U1 or U2 builds beyond that table is a change of this plan, with its review rounds.

## Acceptance: the row's clauses, each with its task and its evidence

| Clause | Task | Shown by |
|---|---|---|
| Invocation bundles | B1, U2 | `bundle::*` (fast tier): written byte for byte the same twice, read back whole, every refusal of the reader; `runs_bundle::a_bundle_of_a_rascal_chain_imports_into_an_empty_store_and_replays_with_the_first_store_gone`; `send.spec.ts`'s new case `bundle this run downloads one file that imports into another playground` (Chrome) |
| Journal replay | R1 | `replay::*` (fast tier): an offline replay of a recipe, a loop and a check reproduces each with no backend configured and nothing spent; a divergence and an unanswerable opaque part each reported; `replay::a_live_replay_spends_as_a_run_and_reports_a_changed_verdict_as_a_difference` |
| Saved-run comparisons in the playground | U1 | `play_compare::*` (fast tier); `compare.spec.ts: a pinned baseline and an attempt of the same task show their pictures, settings and cost side by side, and the pin survives a reload` (Chrome) |
| Saved examples in the playground | U1 | `play_examples::*` (fast tier); `examples.spec.ts: an example saved from a result opens as a draft that runs nothing, after a restart and after its run was deleted` (Chrome) |
| Guided chain replay in the playground | U2 | `play_chain::*` (fast tier); `chain.spec.ts: a chain of a removal and its check replays one ordinary run a step, binds the new output and shows its changed identity` (Chrome) |
| Section 6's control, section 7's rows | B1, R1, U1, U2, N1 | The above, and N1 marks each row kept in `SPEC.md` |

Every clause is shown at the end of the milestone by the tests named, held by
`scripts/tests/test-notes.sh` in the note (N1). None needs a live call.

## How the work is split

Seven tasks. Sizes are changed lines of source, tests and hand-written documents, by
milestone 3's rate (about 2,500 for a large one); generated reference, the built `ui/dist/`
and lock files apart.

| Task | What | Builds on | Size |
|---|---|---|---|
| A0 | The entry check: the fast tier's time, the pack's budget, the Chrome case that timed out once, a request that never connected | `main` | Small: about 500 |
| G1 | The designs, in Paper, approved by the owner; the table of what the engine lacks | `main` (the orchestrator's, beside A0) | No code; a plan amendment of about 100 |
| B1 | Invocation bundles: `bundle/1`, `run/7`, writing, reading, importing; the card in the record; what the store keeps; ADR 0048 | A0 | Large: about 2,400 (format and writer 500, reader 600, import and entry 350, `run/7` and the card 350, store keeping 200, tests 400 more) |
| R1 | Replay: recorded handles, offline and live, the comparison, `whizbang runs replay`; the run's end moves its journal; a panic at `Publishing`; ADR 0049 | B1 | Large: about 2,400 (handles 600, comparison 450, command 250, the end's two rows 300, tests 800) |
| U1 | The playground: saved-run comparisons, saved examples, Home's library and the choice's refusal; their routes and Chrome cases | G1, B1 | Large: about 2,300 (host 600, screens 1,000, tests and Chrome 700) |
| U2 | The playground: bundles, guided chain replay, replay of a run; their routes and Chrome cases | G1, R1, U1 | Large: about 2,300 (host 550, screens 1,000, tests and Chrome 750) |
| N1 | The orchestrator's pass by hand; the note, the boxes, the carry-over rows, the spec's wording owed | U2 | Small: about 500, no code |

**Waves.** Each from `main` once what it builds on has landed.

1. **A0**, and beside it **G1** (the orchestrator and the owner in Paper; no branch of code).
2. **B1**.
3. **R1**, and beside it **U1** once G1 is approved. R1 is the core's `contract/` and the
   binary's `runs.rs` and `wiring.rs`; U1 is `whizbang-serve/src/play/`, the binary's
   `play_*.rs` and `ui/`. They share no source file but `docs/carry-over.md`.
4. **U2**, when R1 and U1 have landed.
5. **N1**.

**Shared files, each with one owner in a wave.** `docs/carry-over.md`: each task changes only
the rows the table "Carry-over rows owned by milestone 7" gives it; the second to land in a
wave merges `main`; N1 writes the rest. `crates/whizbang/src/main.rs`: B1 (the `runs bundle`
and `bundle import` commands), then R1 (`runs replay`). `crates/whizbang-core/src/contract/
record.rs`: B1 alone (`run/7`); R1 sets `replayed` through B1's type. `contract/engine.rs`:
R1 alone. `crates/whizbang-serve/src/play/routes.rs` and `ui/`: U1 in wave 3, U2 in wave 4.
`ui/dist/`: whoever changes `ui/src/` runs `just ui-build`; the second to land runs it again.
`docs/reference/`: whoever changes the description runs `just docs`. This plan's own file: a
task changes only its own section and box; G1 adds its table.

**Seams.** Each task names where it splits if it passes about 2,500 changed lines: B1, the
card in the record (item 6) and what the store keeps (item 7), which then land first; R1, the
end's two rows (item 7), which then land first; U1, saved examples (item 2); U2, replay of
one run (item 4). A split is said in the hand-back and the owner is told the count. Nothing
of a row is dropped to keep the count.

**Review rule.** Every pull request is reviewed whole by a reviewer that did not write it, and
merged by the orchestrator on green CI with `just scan-range` over `base..head`. What each
reviewer reads hardest: B1, that a bundle is read whole and checked before anything is
imported, that no member can name a path outside it, that no secret or path is in the
manifest, and that an older record still reads; R1, that offline replay can reach no backend
and spend nothing (no configured backend is even built), that a divergence never falls back
to a real call, and that a live replay is admitted and charged as any run; U1 and U2, that no
route takes a path, that model and person text is escaped, that every new route is behind
the guard, and that a step of a chain runs only on the person's Run.

**Live runs.** None is planned. No builder makes a provider call, loads a real model, or
deploys to a host. N1's pass by hand uses fakes and the owner's local SAM 3, which spends
nothing ("Spend").

## A0. The entry check (small)

### Goal

The fast tier, the pack's tests and the Chrome tier have room for this milestone's tests
before wide pull requests add to them, and the charge of a request that never connected is
right.

1. **The fast tier's time on the owner's Mac** (carry-over, "The fast tier's time on the
   owner's Mac near its limit"). Five full `just check` runs on the owner's Mac, by the
   orchestrator under `scripts/heavy.sh`, each suite's seconds recorded. If any run passes
   250 seconds, `just check` runs the fast tests in the two parts CI runs, the sweeps and the
   rest, one after the other, each held to the 300 seconds by `scripts/timing.sh` (the row's
   second remedy): `scripts/tests/test-ci-jobs.sh` gains the case that `check` runs both parts
   and fails when either passes its limit. No limit is raised. The record of the five runs
   and the change go in the pull request; the row closes with the numbers.
2. **The pack's budget** (carry-over, "The pack's tests' budget"). The pack's run on the
   macOS runner measured over five runs from CI's reports, each test's time listed; the
   slowest cut (a fake's waits, a front's deadlines taken from a test clock where they are
   not), so that the run is under 10 seconds on `macos-15` in five runs. If it cannot be
   without testing less, A0 asks the owner for the budget, with the measurements, as the
   row says.
3. **The Chrome case that timed out once** (carry-over, the row on `run.spec.ts`). `run.spec.ts:
   each of the five squint use cases runs from its form` waits on what the page shows (the
   result's tag) rather than on the address within a fixed time; then run 20 times in a row
   alone on the macOS runner, as milestone 6's A0 did: one commit adds a step to `e2e
   (macos-15, shard 2)` that runs the case with `--repeat-each 20` and no retry, inside the
   shard's 120 seconds, its run named in the pull request, and the next commit takes it out.
4. **A request that never connected** (carry-over, "A segment request to a GPU host whose
   connection never opened is charged at its bound"). The checked sender is made to say whether
   any byte of the request left (`crates/whizbang-core/src/egress/ways.rs`); the kind `remote`
   settles a request whose connection failed before its first byte at nothing, as `NotSent`,
   and keeps every later failure at its bound (`crates/whizbang-backends/src/remote/mod.rs`,
   where a fault of a request with `sent.dispatched` is made unresolved).
   `remote::wire::a_request_whose_connection_was_refused_settles_at_nothing` and
   `remote::wire::a_request_cut_after_its_first_byte_is_still_charged_at_its_bound`.

- [ ] Done when: five full checks on the owner's Mac each under 250 seconds of test time, or
  `just check` runs the two parts with the script test; the pack's run under 10 seconds on
  `macos-15` in five runs, or the owner's word on its budget; the Chrome case passed 20 runs
  in a row (the run named); item 4's tests pass; the four rows closed with their lines.

## G1. The designs (the orchestrator's, with the owner, in Paper)

### Goal

Approved designs of every new surface, before U1 and U2 build them, in the style milestone
5's screens set.

1. **The surfaces** of "The designs (G1)" above, drawn by the orchestrator in the Paper file
   "whizbang playground", on a new page "Playground · bundles and replay", from the file's
   own tokens and components; new components on "Components" in dark and light.
2. **Read against the engine** on the day, against `main`: the table "What the design shows
   that the engine cannot back yet", written into this plan under G1's section, in milestone
   5's form.
3. **The owner approves** each surface in Paper. Until then U1 and U2 do not start their
   screens; B1 and R1 do not wait.

- [ ] Done when: the owner has approved the six surfaces (or said which to leave out), and
  the table is merged into this plan.

## B1. Invocation bundles (large)

### Goal

Any ended run, or a chain of runs, leaves its store as one file that another store takes in
and can replay with the first store gone; and the records it carries say on which card a
model ran.

1. **ADR 0048, "Invocation bundles"**, written first: "Bundles", "The record a bundle
   carries" and "What the store keeps for replay" of "Choices this plan makes", whole, with
   the old contract's bundle named as reference and each difference from it said
   (`reference_only` becomes `withheld`; no `unportable`, since every artifact type has a
   canonical file, ADR 0002).
2. **The writer** (`crates/whizbang-core/src/bundle/`): a run's family and, for a chain, each
   step's, read through one snapshot of the store; links found by identity (an input whose
   identity an earlier step's output has; two such outputs refuse the chain, naming the step
   and port); the members and the manifest; the ustar with fixed headers. `report.rs`'s tar
   writer moves here and keeps its output for "Export run"
   (`play::export_run_is_the_same_bytes_after_the_writer_moved`).
3. **The reader**: every bound and refusal of "Reading one trusts nothing", each with a test,
   on bundles built in the test (no committed binary fixture but one, below).
4. **The import**: one publication of the artifacts with their import mark and the entry
   `bundles/<digest>`; a second import changes nothing; a bundle's runs are not the store's
   own (`runs list`, `runs stats` and the ledger do not see them).
5. **The commands**: `whizbang runs bundle <run id>... --to <file>` (one run, or a chain in
   the order given, at most 16), written as `store export` writes (not over a file without
   `--overwrite`, not through a link, not inside the store, `crates/whizbang/src/export.rs`);
   `whizbang bundle import <file> --store <dir> [--create]`; `whizbang bundle show <file>`
   (the manifest, its steps and links and what it withholds, reading nothing into a store).
6. **`run/7`, and the card** (carry-over, "The run record of a request to a GPU host names
   `device cuda` alone"): the record type gains the card and `replayed`, written only where
   present; the local supervisor (`whizbang-backends/src/pack/`) and the kind `remote` read
   the card from the worker's load diagnostics, held to 64 printable characters, secrets out;
   the schema `run-7.json`; every older record reads as before.
   `remote::the_run_records_the_card_the_fake_host_named`,
   `record::a_record_with_a_card_is_run_7_and_one_without_is_written_as_before`.
7. **What the store keeps** (carry-over, "The engine API's hourly collection lets go of the
   names of the uploads"): `Store::prune`, run by the writer that holds the store, after the
   engine API's hourly collection, so an upload's bytes go with its name; a bundle's entry
   and (for U1) an example's are names that keep what they hold from `delete`, `prune` and
   the collection. `store::collect::the_hourly_collection_ends_in_a_prune_and_an_uploads_bytes_go_with_its_name`,
   `store::collect::a_run_going_keeps_every_file_a_held_reader_reads`,
   `store::collect::a_bundles_entry_keeps_its_artifacts_from_delete_prune_and_the_collection`.
8. **A committed fixture**: `fixtures/bundle/remove-then-check.tar`, a chain of a removal
   and its check made by the fixture kit with fake models, read by the tests of this build
   and every later one, so a later change of format must keep reading it.

- [ ] Done when: `bundle::the_same_runs_make_the_same_bytes_twice`,
  `bundle::a_bundle_reads_back_whole_with_every_member_checked`,
  `bundle::a_member_named_absolutely_with_dot_dot_or_as_a_link_is_refused_and_nothing_imported`,
  `bundle::a_member_not_named_by_the_manifest_or_a_named_one_missing_is_refused`,
  `bundle::an_artifact_whose_digest_differs_is_refused`,
  `bundle::an_unknown_version_is_refused_naming_it`,
  `bundle::a_bundle_over_its_bounds_is_refused_before_it_is_read_whole`,
  `bundle::an_opaque_part_and_a_missing_picture_are_listed_as_withheld`,
  `bundle::the_manifest_holds_no_path_and_no_secret`,
  `bundle::a_chain_links_an_input_to_the_earlier_output_it_took`,
  `bundle::an_input_two_earlier_outputs_could_be_refuses_the_chain`,
  `runs_bundle::a_bundle_of_a_rascal_chain_imports_into_an_empty_store_and_replays_with_the_first_store_gone`
  (its replay is R1's; B1 lands it with the import alone, and R1 completes it),
  `runs_bundle::a_second_import_changes_nothing_and_its_runs_are_not_the_stores_own`,
  `runs_bundle::the_committed_fixture_reads`, and items 6 and 7's tests pass; ADR 0048 is
  merged; the two rows closed with their lines.

## R1. Replay (large)

### Goal

A stored run, from its own store or from a bundle, is made again offline with nothing
called and nothing spent, or live as an ordinary run, and what it made is compared with what
the run made; and the end of a run neither copies what it kept nor answers a durable end as
abandoned.

1. **ADR 0049, "Replay"**, written first: "Replay" of "Choices this plan makes" whole, with
   rascal's `inspect --replay` and its replay of a heal named as reference.
2. **Recorded capability handles** (`crates/whizbang-core/src/capability/recorded.rs`): one
   for each capability, built from a run's family as the store or a bundle holds it, which
   answer the replayed run's requests in their order from the table in "Replay", and
   `diverged` or `unanswerable` otherwise. They name no recipient and no price, so egress has
   nothing to send and the ledger nothing to reserve. The binary builds them in place of the
   inventory's for an offline replay (`crates/whizbang/src/wiring.rs`): no configured backend
   is built, no worker started, no key read.
3. **An opaque part** (carry-over, "A run cannot be sent again from what the store holds of
   it"): a judge turn that held one is `unanswerable` offline, naming the part's adapter and
   length; live, the run is made from its call, not from its exchanges, so no part is needed;
   the row closes with that line.
4. **The comparison** (`crates/whizbang-core/src/contract/replay.rs`): of "What it compares",
   by place, first difference first, bounded to 256 differences with a count of the rest.
5. **The command**: `whizbang runs replay <run id> | --bundle <file> [--step <n>] [--live]
   [--request-id <id>] [--json]`, against a store (`--store`), the report printed, and the
   exit code 0 for reproduced, 3 for differs, diverged or unanswerable, 1 for a refusal. A
   bundle's replay imports it first (B1, item 4).
6. **The replayed run's record** carries `replayed` (`run/7`). `runs stats` counts replayed
   runs apart (`--replayed include|exclude|only`, excluded by default), so the figures of
   ordinary use are not changed by replays.
7. **The end's two rows.** (a) *What a run kept is moved into its end* (carry-over, "every
   run's end clones what the run kept"): `taken` (`contract/engine.rs`, the end's
   publication, where it is cloned at each attempt) is moved into the first end and kept for
   a second only when a panic needs one: the end holds a shared, immutable copy (`Arc`)
   rather than a clone of the bytes.
   `execute::an_end_holds_one_copy_of_what_the_run_kept_and_a_second_end_after_a_panic_writes_it_whole`.
   (b) *A panic at `Publishing`* (carry-over, "a panic in the executor's code while a run's
   end is at `Reached::Publishing`"): the catch asks the store, through a fresh snapshot,
   whether the run's end became durable, and when it did answers as a durable end does
   (finishing what is left once, as the `Durable` arm does), and otherwise as today.
   `execute::a_panic_at_publishing_after_the_end_became_durable_answers_the_durable_end`
   and `execute::a_panic_at_publishing_before_the_end_was_durable_answers_as_before`.

- [ ] Done when: `replay::an_offline_replay_of_remove_prompt_reproduces_it_with_no_backend_configured`,
  `replay::an_offline_replay_of_remove_checked_reproduces_its_attempts_and_verdict`,
  `replay::an_offline_replay_of_a_check_reproduces_its_turns_checklist_and_verdict`,
  `replay::an_offline_replay_sends_nothing_reserves_nothing_and_starts_no_worker`,
  `replay::a_changed_operation_is_reported_as_a_difference_at_its_step`,
  `replay::a_request_the_original_did_not_make_is_diverged_and_nothing_is_called`,
  `replay::a_turn_that_held_an_opaque_part_is_unanswerable_naming_it`,
  `replay::a_replay_of_an_interrupted_run_is_refused_before_it_starts`,
  `replay::a_live_replay_spends_as_a_run_and_reports_a_changed_verdict_as_a_difference`,
  `replay::a_replayed_run_names_what_it_replays_and_stats_leaves_it_out_by_default`,
  `runs_bundle::a_bundle_of_a_rascal_chain_imports_into_an_empty_store_and_replays_with_the_first_store_gone`
  (completed here), and item 7's three pass; ADR 0049 is merged; the three rows closed with
  their lines.

## U1. The playground: saved-run comparisons and saved examples (large)

### Goal

A person pins one run as a baseline and compares another of the same task with it, and
saves a run's inputs and settings as a named example that opens as a draft; built as G1's
designs show.

1. **Saved-run comparisons**: the host's `GET /api/compare/{id}/{id}`, from the two records
   ("Saved-run comparisons"), refused 422 for two tasks and 409 for a run that has not ended;
   the screen at `#/compare/<baseline>/<attempt>`; "Compare with…" on a result and "Pin as
   baseline" in History; the pin in the address and the browser's storage.
2. **Saved examples**: the entry `example/1` written through the host's writer
   (`POST /api/examples`, from a run's id, a name and notes; `GET /api/examples`;
   `PATCH /api/examples/{id}`; `DELETE /api/examples/{id}`); opening one as a draft;
   `store list` and `store delete --example`; the bound of 1,000.
3. **Home's library** (carry-over, "Home's library lists only the pictures given through the
   playground"): Home lists every picture the store holds as an input of a run or an import,
   whatever door gave it, those with no name a person gave shown by their identity, as G1's
   design shows; with its Chrome case on a store filled from the command line.
4. **A choice's refusal** (carry-over, "`choice_json`'s `Err(_)` arm"): a refusal about the
   choice itself is told apart from the call's missing input
   (`crates/whizbang-serve/src/play/routes.rs`), with a test.
5. **What G1's table assigns to U1**, each with its test.
6. **The guide**: `docs/guides/playground.md`, "Comparing two runs" and "Saved examples".

- [ ] Done when: `play_compare::two_runs_of_one_task_compare_by_settings_inputs_outputs_time_and_cost`,
  `play_compare::two_tasks_are_refused_422_and_a_run_not_ended_409`,
  `play_examples::an_example_keeps_its_inputs_after_its_run_is_deleted_and_pruned`,
  `play_examples::opening_an_example_starts_no_run`,
  `play_examples::a_name_or_notes_holding_markup_are_shown_as_text`,
  `play_examples::past_1000_examples_a_new_one_is_refused_naming_the_bound`,
  `play::a_choice_refused_for_itself_is_told_apart_from_a_missing_input`, and the Chrome cases
  `compare.spec.ts: a pinned baseline and an attempt of the same task show their pictures, settings and cost side by side, and the pin survives a reload`,
  `examples.spec.ts: an example saved from a result opens as a draft that runs nothing, after a restart and after its run was deleted`,
  `home.spec.ts: home lists the pictures of a store filled from the command line` pass, inside
  the Chrome tier's 240 seconds; the two rows closed.

## U2. The playground: bundles, guided chain replay, replay of a run (large)

### Goal

A person bundles a run or a chain, takes a bundle in, replays a chain one ordinary run a
step, and replays one run offline or live with its report; built as G1's designs show.

1. **Bundles in the host**: `GET /api/runs/{id}/bundle` (one run, a download);
   `POST /api/bundles/chain` (run ids in order, at most 16; the chain's file, a download);
   `POST /api/bundles` (a bundle's bytes, at most 256 MiB, read and imported by B1's reader;
   no path); `GET /api/bundles` and `GET /api/bundles/{digest}` (its manifest, steps, links
   and what it withholds). The upload bound joins the guard's table (ADR 0041, amended).
2. **Guided chain replay**: "Start the chain", each step opened as a draft from the bundle's
   step, Continue, Stop; a linked input bound to the new run's output by port and place, a
   changed identity shown; a missing or doubled output stops the chain; the run started from
   a step carries `replayed` with the bundle's digest and the step (the start route takes an
   optional `replay` object, checked against the bundle's entry); the chain's place in the
   browser's storage.
3. **Replay of a run**: "Replay" on a result, offline through `POST /api/runs/{id}/replay`,
   and live with the cost reviewed first as any paid run; the report as a screen.
4. **What G1's table assigns to U2**, each with its test.
5. **The guide**: `docs/guides/playground.md`, "Bundles", "Replaying a chain", "Replaying a
   run".

- [ ] Done when: `play_chain::a_step_runs_only_when_started_and_continue_binds_the_new_output`,
  `play_chain::a_link_that_finds_no_output_or_two_stops_the_chain`,
  `play_chain::a_run_started_from_a_step_records_the_bundle_and_the_step`,
  `play_bundles::a_bundle_uploaded_is_read_whole_before_anything_is_imported`,
  `play_bundles::a_bundle_over_256_mib_is_refused_413_before_it_is_read_whole`,
  `play_bundles::no_bundle_route_takes_a_path`,
  `play_replay::an_offline_replay_from_the_page_spends_nothing_and_shows_reproduced`, and
  the Chrome cases
  `chain.spec.ts: a chain of a removal and its check replays one ordinary run a step, binds the new output and shows its changed identity`,
  `chain.spec.ts: stop leaves the chain and a reopened tab finds its runs`,
  `send.spec.ts: bundle this run downloads one file that imports into another playground`,
  `replay.spec.ts: replay of a result shows reproduced, and a changed one shows where it differs`,
  and `security.spec.ts`'s new case `a bundle holding markup in its words shows them as text`
  pass, inside the Chrome tier's 240 seconds.

## N1. The note (small; the orchestrator's pass first)

1. **The pass by hand**, by the orchestrator in Chrome on the owner's Mac, on a store with
   real runs of the owner's local SAM 3 and the fixture's fakes, spending nothing: each
   surface of G1 used once as a person would; a bundle made in one playground, imported in
   another with the first store moved away, and its chain replayed. What it finds is fixed in
   its own pull request before the note, or written into the carry-over list with an owner.
2. **The note**, `docs/plans/m7-note.md`, in milestone 6's shape: the row's clauses each with
   its evidence (held by `scripts/tests/test-notes.sh`), the timing report by suite and by
   job, the spend (none), what moved to the carry-over list, and what a reader should not
   assume (a live replay repeats the call, not the answer; a bundle holds the person's
   pictures and words unencrypted). The boxes of this plan ticked.
3. **The spec's wording owed**: section 6's "Invocation bundles" and section 7's two rows
   marked kept, milestone 7; section 3's run record names the card; section 11 gains `runs
   bundle`, `runs replay`, `bundle import|show` and `store delete --example`. Each brings the
   spec in line with reviewed pull requests.
4. **The Google rows of prices**: no live run of this milestone asks a Google judge, so the
   row is re-owned with that line to the first live run on or after each date, milestone E's.
5. **The tag `m7`**, by the orchestrator, on the note's merge.

- [ ] Done when: the note is merged and `m7` is tagged.

## Carry-over rows owned by milestone 7, and who takes each

| Row (by its subject in `docs/carry-over.md`) | What happens to it |
|---|---|
| A run cannot be sent again from what the store holds of it | R1, item 3 |
| The engine API's hourly collection lets go of the names of the uploads no run took | B1, item 7 |
| The Google rows of prices are introductory | N1, item 4: re-owned to E with a line, since no live judge run happens |
| `choice_json`'s `Err(_)` arm | U1, item 4 |
| Home's library lists only the pictures given through the playground | U1, item 3 |
| Every run's end clones what the run kept | R1, item 7 (a) |
| A panic at `Reached::Publishing` is answered as abandoned | R1, item 7 (b) |
| The Chrome case `run.spec.ts: each of the five squint use cases runs from its form` | A0, item 3 |
| The fast tier's time on the owner's Mac near its limit | A0, item 1 |
| The run record of a request to a GPU host names `device cuda` alone | B1, item 6 |
| A segment request whose connection never opened is charged at its bound | A0, item 4 |
| The pack's tests' budget | A0, item 2 |

Not this milestone's, and untouched: the front's bridge's cancel window (8); the probe's
wording (the next release); hosting the model's files (no milestone).

## Documentation that changes

- `docs/adr/0048-invocation-bundles.md` (B1), with `run/7`; `docs/adr/0049-replay.md` (R1);
  `docs/adr/0041-the-playground-host.md`, amended for the new routes and the bundle's bound
  (U1, U2).
- `docs/guides/bundles-and-replay.md`, new (B1, R1), under "Written" in
  `docs/guides/README.md`: making, reading and importing a bundle, what it holds and
  withholds, replay offline and live and its report.
- `docs/guides/playground.md`: comparisons, examples (U1); bundles, chains, replay (U2).
- `docs/guides/engine-api.md`, "The uploads it keeps": their bytes now go with their names
  (B1).
- `docs/reference/`: regenerated if the description changes (not expected: no action is
  added).
- `SPEC.md`: N1's wording, item 3. `docs/carry-over.md`: each task's rows, then N1.

## Commit strategy

- One task is one branch and one pull request, tests first, reviewed whole by a reviewer that
  did not write it, merged by the orchestrator with a merge commit once `ci` and both scans
  pass (the sixteen jobs: `check`, `sweeps` and `release` on each runner, `scripts` and `e2e`
  in two shards on each, `secrets`, and the aggregate `ci`). A builder pushes once, when the
  whole task is done, after one full `just check` on the finished tree, and `just e2e` where
  it touches the playground (U1, U2). A0 pushes twice, by design (item 3).
- An ADR is in the pull request of the task that names it, its first commit.
- Every state of `main` between tasks passes `just check`; every door that worked still works.
- **Rollback checkpoints.** The tag `m6`, which exists. `m7-core`, made by the orchestrator
  when B1 and R1 have landed: bundles and replay from the command line. `m7`, on N1's merge.
  A pushed tag is never moved.
- **If a task shows a decision here is wrong,** the decision is changed in this file first, in
  its own pull request; a change of scope or architecture gets the review rounds first. A test
  is never bent to fit.

| Tasks | To undo |
|---|---|
| A0 | Revert the merge: `just check` runs one part again; the remote charge goes back to its bound |
| B1 | Revert with care: a store that imported a bundle holds `bundles/` entries and `run/7` records, which a build before B1 refuses by name and version. ADR 0048 says this; a revert keeps the readers and drops the writers |
| R1 | Revert the merge after U2: replay is gone; replayed runs stay readable as `run/7` records, by B1's reader |
| U1, U2 | Revert the merge: the screens and routes go; examples and bundle entries in a store stay, read by B1's reader and listed by `store list` |
| N1 | A note and the spec's wording |

## Spend

**Estimate: 0 dollars.** No clause of row 7 needs a live call: offline replay calls
nothing by design, live replay is shown against the fixture kit's fakes, and N1's pass uses
the fakes and the owner's local SAM 3 on the Mac's GPU. No builder makes a live call, and
nothing in CI does.

**Cap: 5 dollars.** If a task finds that a clause cannot be shown without a live call, the
orchestrator says why in this plan's section first, runs it from the owner's Mac on an
approved recipient, and stops at 5 dollars, telling the owner. Nothing else is built to hold
spend.

## Dependencies

```
A0 ──► B1 ──► R1 ──────────┐
        │                  ▼
G1 ─────┴──► U1 ───────► U2 ──► N1
(owner's approval)
```

- A0 before B1: the fast tier has room before wide pull requests add to it.
- G1 before U1 and U2: screens are built from approved designs.
- B1 before R1: replay reads a bundle's runs and writes `run/7`.
- B1 before U1: examples are store entries that B1's keeping rules hold.
- R1 and U1 before U2: U2's chain runs replayed steps and builds on U1's routes.

## Risks

1. **Offline replay may not reproduce what it should.** A recipe's pixel work is
   deterministic, but an answer rebuilt from `segment.txt` must give back the same instances
   in the same order (`segment::in_order`), and a judge's request must be compared on what it
   asked, not on bytes that hold a time or a nonce. R1 states what is compared in ADR 0049
   and tests each kind of run; a field found volatile is named there, never skipped silently.
2. **A bundle is a file a person may take from anyone.** It is read whole and checked before
   anything is imported, member names cannot leave it, and its text is escaped wherever
   shown; the reviewer of B1 reads the reader first.
3. **Bundles grow.** A check's exchanges can come to about 33.5 MiB (the carry-over row's
   measure), and a chain holds several runs. The 256 MiB bound refuses more, naming it.
4. **The Chrome tier's time.** U1 and U2 add about eight cases. Each reports its seconds by
   file and stops past about 220 of the 240, or 110 of a shard's 120; then coverage is
   reviewed first, as section 13 says, and no limit is raised without the owner.
5. **The fast tier's time** is at about 246 of 300 on the owner's Mac. A0 makes room first;
   each later task reports its seconds and stops past about 250.
6. **The designs may ask more than the engine has.** G1's table names each element and its
   fate; a change of structure is a change of this plan.
7. **A new record version and two new store entries.** Older builds refuse them by version
   and name; the revert notes say so.

## Open questions

None.

## What the owner still does

- **G1**: draws and approves the new surfaces in Paper with the orchestrator, and says which
  of the six to leave out, if any. U1 and U2 wait for it; B1 and R1 do not.
- **A0, item 2**: only if the pack's tests cannot come under 10 seconds on the macOS runner
  without testing less, the owner's word on their budget, with the measurements.

## What the owner is told, not asked

- **Seven tasks**, four of them large; nothing live, an estimate of 0 dollars and a cap of 5.
- **A bundle is one file** that carries a run or a chain of up to 16 with its pictures,
  records and judge turns, made by `whizbang runs bundle` or the playground, read by
  `whizbang bundle import` or the playground; not offered over the engine API or the stdio
  server.
- **Replay is offline by default**: it calls nothing and spends nothing, answering every
  model from what the run recorded, and says where it differs or what it cannot answer.
  `--live` makes the call again and spends as any run.
- **Saved examples live in the store** and keep their pictures after their run is deleted;
  a comparison's pin lives in the page's address.
- **The run record gains `run/7`**: the card a model ran on, and what a run replays.
- **The tags `m7-core` and `m7`** are made by the orchestrator as above.

## Review rounds
