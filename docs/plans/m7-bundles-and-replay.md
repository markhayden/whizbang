# Plan: milestone 7, bundles and replay

Draft 4, 2026-11-24, written from `main` at 6c75b6e3 (the tag `m6`, the merge of #300, the
note of milestone 6), after three outside review rounds ("Review rounds", at the end). Milestone 7 is `SPEC.md` section 16, row 7, "Bundles and replay":
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
   not kept: the run commits the painted picture composited under the mask
   (`crates/whizbang-rascal/src/fill.rs`, `pasted`), which cannot be undone. A judge's turns are kept whole as diagnostics
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
Each member's header is fixed (mode 0644, uid and gid 0, no names of users, mtime 0; the
writer takes the time as a parameter, 0 for a bundle and the run's end for "Export run", as
`report.rs` gives it today),
`bundle.json` comes first and every other member follows in the order of its name's bytes,
so the archive's SHA-256 names it, as table-manners' did. The playground's tar writer
(`crates/whizbang/src/report.rs`, `tar_file`) cuts a name at 100 bytes and writes no
`prefix`; a bundle's names can be longer (a run id and a diagnostic's name may each be 64
characters, `contract/run.rs`, `contract/record.rs`, `DIAGNOSTIC_NAME_LIMIT`). So the core's
`bundle` module has a writer of its own that splits a name between ustar's `prefix` (155
bytes) and `name` (100) at a `/`, and refuses a name it cannot so write; and a reader that
joins them. "Export run" moves to the same writer, whose output for its short names is the
same bytes (B1, item 2).

| Member | Holds |
|---|---|
| `bundle.json` | The manifest (below), at most 1 MiB |
| `runs/<run id>/record` | Each step's run record and each of its descendants', as the store holds it, byte for byte |
| `runs/<run id>/journal` | Its journal, as stored |
| `runs/<run id>/diagnostics/<name>` | Each diagnostic it kept: its judge's exchanges, `segment.txt`, and a fill's answer (below) among them |
| `files/sha256-<hex>` | Each canonical file (ADR 0002) of an input, an output or an artifact a kept diagnostic or exchange names, once for each **file digest**: two files of one artifact identity with other origins (an identity leaves the origin out, `artifact/wire.rs`, and the store keeps each run's own file, `artifact/stored.rs`) are two members |

**The manifest.** `{"bundle": 1, "whizbang": <version>, "steps": [...], "artifacts":
[...], "withheld": [...]}`:

- `steps`, one or more, in the order they ran, each a **root run** (a run nothing nested;
  a child travels inside its root's family and is never a step of its own): its run id, its
  action and its version, the digest of the config its settings came from, whether it is
  replayable by a build of this action set and why not (below), and, for each input port,
  either the artifact's identity or **the earlier step's output it took** (`{"step":
  <index>, "port": <name>, "place": <n>}`) when its identity is one an earlier step made. A
  step never names a path.
- `artifacts`: for each file member, its SHA-256, the artifact identity and type it holds,
  and each run and port it is the file of, so every run keeps its own file.
- `withheld`: what the bundle does not carry and why, each with a sentence: an opaque part
  of a judge's exchange (its adapter, digest and length, as ADR 0009 keeps it); a picture a
  stored exchange names that the store does not hold; a diagnostic the store let go of; a
  fill whose answer was not kept (a run made before B1); a check answered from the decision
  cache, whose originating turns are another run's. A bundle with anything withheld still
  bundles; its offline replay says, before it starts, which step cannot be answered and why
  (R1).

**What a bundle may hold, and what replays.** A bundle carries data, never code. A step
whose action is one this build ships (`whizbang describe`) is replayable. A step of a user's
recipe file or of an experimental tool file (`crates/whizbang/src/wiring.rs`, the actions
that come from configured files; `tool_file.rs`, which runs a tool in the pack's worker) is
carried as evidence, marked `not replayable` with its reason, and refused by replay: the
importing store never takes the recipe or the tool from the bundle, and nothing a bundle
holds is ever run as code.

No secret is in a bundle: every member is a file the store already holds with its secrets
taken out (section 3), and B1 adds nothing but the manifest, which holds no value of the env
file, no path and no account. The bundle is not encrypted and holds the person's pictures
and words: the guides say so.

**Reading one trusts nothing.** A bundle is read before anything acts on it, and every check
below passes before the first publication:

- *Bounds*: at most 256 MiB in all, 4,096 members, a manifest of 1 MiB, a member's header
  read and its size checked before its bytes; artifacts decoded one at a time under the
  core's limits (ADR 0001, ADR 0002), so the memory held is one artifact's and the manifest.
- *Names*: each member a plain file, its name relative, with no `..`, no empty part and
  none of a link's or a device's type; no name twice; every member the manifest names there
  and no other.
- *Artifacts*: each file's SHA-256 is the manifest's; its canonical file decodes; the
  identity it decodes to is the identity it is named by.
- *Records and journals*: each read by its own strict reader (`contract/record.rs`, the
  journal's); each diagnostic's bytes match what its record says of it (its digest and
  length) where the record says.
- *The whole*: each step's record agrees with the manifest (run id, action, version, inputs
  and outputs); each record's parent is in the bundle and the families are trees with one
  root each, the steps' roots; every input and output a record names is a member or listed
  as withheld; every link names an earlier step's output that its record committed, at that
  port and place, with the identity the later step's record took. And the two directions of
  a family agree: every step a record lists as a child run is a record of the bundle whose
  parent is that record, with the same action and an ending that agrees with what the parent
  noted; every record with a parent is listed by that parent once; no run is the child of
  two.
- *Version*: an unknown manifest version is refused, naming it.

A refusal names the first fault and says that nothing was imported and nothing ran. Each
rule has an adversarial test (B1, item 3).

**Importing one** brings it into a store in one publication: its artifacts through the
store's import path but **named under the bundle's entry alone**,
`bundles/<digest>/artifacts/<identity>`, never with an ordinary import's name or mark, so
that a bundle's artifacts are the bundle's and an independent import keeps its own name
(`artifact/stored.rs`); and **every other member as an object of the store named under the
bundle's entry**, `bundles/<digest>/manifest`,
`bundles/<digest>/runs/<run id>/record`, `…/journal` and `…/diagnostics/<name>`, so the
store's names hold each one (`store/generation.rs`, a generation names what it keeps) and a
prune keeps them while the entry is there. A bundle's runs are never made runs of the
importing store: no history, stats or ledger counts them, and replay reads them from the
entry. A second import of a bundle already held changes nothing. **A run started from a bundle** (a replay, a chain's step)
reads its inputs through the bundle's names: preparation and admission look an artifact up
by its ordinary name and then, for a run started from that bundle, by the bundle's
(`artifact/stored.rs`, the lookup; `contract/engine.rs`, admission's retention of inputs).
Admission then gives each input it took a name of the run's own in its publication, so the
run holds what it used once the bundle is deleted and pruned, and a later run can take it as
any input. **A bundle is
deleted** by `whizbang bundle delete <digest>` or from the playground, which removes the
entry's names in one publication; its artifacts stay while a run, an example, an ordinary
import or another bundle names them, and a prune removes the rest.

**Which doors.** The command line and the playground. Not the engine API: a bundle carries
a store's files out and brings files in that name runs of another store, which is the kind
of local control section 10 keeps off it, and an app finds its runs by id already. Not the
stdio server: its tools are the description's actions (ADR 0031), and a bundle is not an
action. A door is not added (section 15, "Ask first").

### The record a bundle carries

**`run/7`**, written only where a run has what it adds, as `run/5` and `run/6` were (ADR
0005): `run/6` with two things after it.

- **A model's card**: beside `device`, the card the worker named on its load, at most 64
  printable characters, or null (the carry-over row on the card; the adapter names it,
  `pack/src/whizbang_pack/adapter.py`, `CARD_NAME_MOST`). B1 reads where the card reaches
  the supervisor and the kind `remote` today and carries it into the answer's `Served`
  (`capability/served.rs`); if it reaches neither, ADR 0048 says what of `pack/1` carries it
  (a section around the frames, as ADR 0020 allows, never a new field of the hello).
- **`replayed`**: the run this run replays (its id, and the bundle's digest and step when it
  came from one), and whether offline or live. Every run of a replayed family carries it, a
  child as its root does, so a figure over runs can leave the whole family out.

Every older version reads as it did; nothing in a store is migrated.

### What the store keeps for replay

- **A run keeps what replay needs, but for a fill's answer.** A segmenter's answer is
  rebuilt from its run's committed masks and `segment.txt`, and a judge's from its
  exchanges; B1 shows each with a test that rebuilds the answer the fake gave and compares
  it whole. A fill's is not: `fill` composites the painted picture under the mask
  (`crates/whizbang-rascal/src/fill.rs`, `pasted`; `composite.rs`) and commits the composite,
  and a rounded blend cannot be undone. So from B1 on, a fill run keeps the painted picture
  it was answered with as a diagnostic, `fill-answer.png`, written as a canonical image's
  PNG, at most the image's size; a fill run made before B1 has none, and its offline replay
  is `unanswerable`, naming that. If B1's test of the segmenter's answer finds anything not
  rebuilt whole, the segmenter's answer is kept the same way.
- **A bundle's entry and a saved example** each name what they hold, so `store delete` and
  `prune` keep those artifacts, and the uploads' collection does not let them go.
- **The engine API's collected uploads** (the carry-over row): the hourly collection now
  ends in the store's own prune of what no name holds, run through the writer the server
  holds, under the readers' holds the store already honours (ADR 0036): an upload's bytes
  go with its name. B1, item 7.
- **Rolling back.** The store's names take any text (`store/buckets.rs`), so a build
  before B1 opening a store with `bundles/` or `examples/` names reads the store and leaves
  those names alone; it refuses a `run/7` record when it reads one, by its version. ADR 0048
  says what a build before it does with each.

### Replay

**What is replayed.** A root run of an action this build ships that ended (completed,
cancelled, error or invalid; not `interrupted`, and not one whose record lost its
admission), from the store it is in or from a bundle imported into it. A child is replayed
as part of its root, never alone: a child's action may need its place in its recipe (a
`check` given measurements holds them to the runs of its own recipe,
`crates/whizbang-squint/src/measured.rs`, `held_to_their_runs`). Anything else is refused
before it is prepared, naming why.

**The call.** A replay's call is made from the record: its action, its arguments as the
record holds them (defaults applied; arguments are kept whole, with secrets taken out), and
its inputs by identity. Settings are cut at `SETTING_TEXT_LIMIT` with no mark
(`contract/record.rs`, `Setting::new`), so a setting whose text is exactly that long may
have been cut, and a replay that depends on it is refused naming it. **The choices.** A
call names its backends by the config's choice names (aliases), which preparation resolves
(`contract/snapshot.rs`), while a record's models group names each backend's declared
identity (`contract/record.rs`, `Model.backend`), which two aliases may share. So each
choice is taken from the record's resolved settings, whose names hold the alias
(`backends.<capability>.<alias>.…`, `config/settings.rs`), and a choice the run took from
the config's default is written explicitly; a record whose settings do not name exactly one
alias for a capability it asked (an older record, or two aliases offering it) is refused
for live replay naming the capability, and offline replay offers the recorded handles under
the alias the settings name, or under the declared identity when they name none.

**Offline replay is the default, and calls nothing.** `whizbang runs replay <run id>` (or
`--bundle <file> [--step <n>]`) makes a new run of that call through the store's executor,
with **recorded capability handles** given to that one run in place of the configured
backends (R1, item 2): each capability request the replayed run makes is answered from the
original family, in the order the original made them, and from nothing else:

| Capability | Answered from |
|---|---|
| `segment` | The original `segment` run's committed masks, with each one's score, box and predicted IoU from its `segment.txt` |
| `fill` | The painted picture the original `fill` run kept (`fill-answer.png`, from B1 on) |
| `judge` | Its stored exchanges, turn by turn: each request the replay makes is compared with the one recorded, and the recorded answer given |

The recorded handles are offered under the choices so found, so preparation finds them; they name no recipient and no price, so egress has nothing to
send and the ledger nothing to reserve. Nothing of the config is read for a backend, no key
is read and no worker is started. **The decision cache is neither read nor written by a
replay**, offline or live: offline, a check would otherwise be answered by the entry its own
original wrote (`crates/whizbang-squint/src/check.rs`, `cache::find`, before the judge is
asked); live, each repetition is "a fresh trial" (section 9). A check the original answered
from the cache is `unanswerable` offline (its turns are another run's), and asks the judge
afresh live.

A request the original did not make, or made otherwise (another model, another request, a
request past the last recorded), is answered with an error of its own kind, `diverged`. An
action may handle a capability error and go on (a check's second look leaves the earlier
verdict standing, `crates/whizbang-squint/src/check.rs`; the loop turns a failed check into
an unchecked attempt, `crates/whizbang-rascal/src/checked.rs`), so **divergence is the
replay's, not the action's**: the first `diverged` or `unanswerable` answer is noted on the
replay itself, every later request of the family is refused the same way, and the report
is `diverged` (or `unanswerable`) at that first place whatever the actions made of it.

**Judge turns and what they carry.** A stored exchange keeps a turn's text, but not every
fact of its answer that the loop acts on: a turn's output ceiling, which decides whether a
cut answer is tried again (`crates/whizbang-squint/src/check.rs`, `output_ceiling`), is not
written (`crates/whizbang-core/src/capability/written.rs`). From B1 on, an exchange keeps
it; an older exchange whose answer was cut at a ceiling it did not keep is `unanswerable`
offline, naming that.

**Tool files.** A check may call a tool file the config names (`check.rs`,
`dispatch_file`), noted among its settings as `tool_file:<name>` with the file's digest. A
tool file is code, and a replay never takes it from a record or a bundle: offline, a run
whose settings or judge turns name one is refused before it starts, naming the file; and,
since a call of a tool file that failed notes no digest (`check.rs`, its error arm), the
refusal is also made where a tool file is dispatched: a replay's context refuses every
tool-file call, offline, as `unanswerable`, so no file runs whatever the record says. Live,
a run whose family called a tool file is replayed only when the current config names a file
of that name with the digest the record noted; one whose digest was never noted (the call
failed) is refused, naming the file.

**What cannot be answered.** An answer the store cannot give (an opaque part withheld, a
picture not held, a diagnostic gone, a fill with no kept answer) ends the replay before that
request as `unanswerable`, naming what is missing: the carry-over row's "says where it gets
an opaque part from, or that it cannot". Pixel work runs for real: every operation and every
step of a recipe is computed again.

**What it compares**, once the replayed run has ended, family against family: the runs are
matched by their place (a root to a root, each child to the child at the same place among
its parent's steps, a loop's attempt to the attempt of the same number), and a reference to
a run inside what is compared (a step's run, a measurement's producing run) is compared
through that match, never by its id. Then: each run's outcome, its committed outputs by
identity, port and place, each step's action and arguments, its measurements (an action
such as `edge_similarity` commits no artifact and its result is its measurements,
`crates/whizbang-squint/src/compare.rs`) and findings, a check's checklist and verdict, and
a loop's attempts. Numbers are compared exactly: a replay runs the same build on the same
pixels, and a number that differs is a difference. Times, run ids and request ids are not compared. The report
is one of **reproduced** (all the same), **differs** (each difference by place, first
first, at most 256 with a count of the rest), **diverged** or **unanswerable** (as above).

**Live replay** (`--live`) makes the call again with the configured backends, as a new run
under preparation, admission and the allowance: the store's current config, credentials,
egress approvals and prices govern it, and nothing in a record or a bundle is authority for
them. A backend the record names that the config no longer offers refuses the replay,
naming it; a resolved setting that differs from the record's is reported beside the
comparison. What it spends is in its record and the ledger like any run's. It is compared as
offline replay is, and a difference is reported, never an error: "A live judgment (a
model's verdict) is not promised to repeat; only the call is" (the old contract's words);
nor are a model's pixels. A live replay of a step whose input is withheld is refused before
it is prepared.

**Its request id and its provenance.** A replay may be given a request id like any call.
A replay's fingerprint is made from **the replay request itself** (its mode, its source: the
run's id, or the bundle's digest and step, and its options), not from the call rebuilt from
the source, so the executor's durable lookup of a request id (`contract/request.rs`, made
before readiness, `contract/engine.rs`) answers a repeat before the source is read: the same
replay asked again returns its run even after its source run or bundle was deleted or the
config changed, and the same id under an ordinary call, another mode or another source is a
conflict. `replayed` is written with the run's admission and in its record at its
end, in the publications that write them today, never added afterwards.

**Where a replay runs.** It is one more run of the store's one executor (section 3, "One
writer"): from the command line while nothing holds the store, and from the playground
through its host, under the same bound on started runs, the same stated time, the same
status, cancel and Quit as any run there. The executor takes the recorded handles for that
one run at its start (`Executor::start_replay`, R1, item 2), so no second way of running is
made and no other run sees them.

**Not built.** Resuming a run a dead process left (section 6: nothing dispatched is replayed
automatically, and a replay is a new run, never the old one carried on); replay over the
engine API or the stdio server ("Which doors"); a replay that reconstructs a missing input;
a replay of a user's recipe or tool file from a bundle.

### Saved examples

An example is the store's: a small entry `examples/<id>` (a random id of 16 hex digits),
`example/1`, holding a name of at most 80 characters and notes of at most 2,000 that a
person gave, the task, its arguments as the run's record holds them, the backends and models
it named, each input by its identity, and the run it was saved from. It names its inputs,
so they outlive the run's deletion. A run whose call depends on a setting that may have been
cut (a text exactly at `SETTING_TEXT_LIMIT`) cannot be saved as an example, as for replay;
arguments are kept whole. Opening one fills a draft and runs nothing; the draft's choices are
the example's where the current config offers them, and readiness is asked again as for any
draft; a later run from it is an ordinary run under the current config. Examples are made,
renamed and deleted in the playground; `whizbang store list` names them, and `store delete
--example <id>` removes one. At most 1,000 in a store. No examples ship with the binary: a
tool record's worked examples stay in its record (`docs/reference/tools/`).

### Saved-run comparisons

Two ended root runs of one task (the same action), one pinned as the baseline. The host
answers the comparison from the two **families** (`GET /api/compare/{id}/{id}`), as the
result screen reads a run (`crates/whizbang/src/play_result.rs`, which reads the family for
its cost): what each was asked, arguments and settings side by side with each difference
marked, inputs by identity with a changed one marked, outputs by port, outcome, time, and
cost and models over the whole family, with unresolved spend and a family not wholly read
said as such. The pictures are shown with the result's compare component
(`ui/src/components/Compare.tsx`), labelled Baseline and Attempt, side by side, and swiped
or as a difference only when both are the same size. The pin is in the page's address
(`#/compare/<baseline>/<attempt>`) and in the browser's storage for that address, so a
reload keeps it; the store holds no pin. The comparison claims no winner and computes no
score.

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
| Invocation bundles | B1, U2 | `bundle::*` (fast tier): written byte for byte the same twice, read back whole, every refusal of the reader; `runs_bundle::a_bundle_imports_into_an_empty_store_and_survives_its_archive_and_first_store_gone_a_restart_and_a_prune`; `send.spec.ts`'s new case `bundle this run downloads one file that imports into another playground` (Chrome) |
| Journal replay | B1, R1 | `replay::*` (fast tier): an offline replay of a recipe, a loop, a check and a fill through a feathered mask reproduces each with no backend configured, no cache read and nothing spent; a divergence and an unanswerable opaque part each reported; `replay::a_live_replay_spends_as_a_run_and_reports_a_changed_verdict_as_a_difference` |
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
| B1 | Invocation bundles: `bundle/1`, `run/7`, writing, reading, importing, deleting; the card in the record; a fill's kept answer; what the store keeps; ADR 0048 | A0 | Large, in two pull requests: about 1,000 (items 1, 6 and 7: ADR 0048, `run/7`, the card, the fill's answer, the store's keeping), then about 2,300 (format and writer 450, reader with its whole-bundle rules 750, import, entry and deletion 400, commands 250, tests 450 more) |
| R1 | Replay: recorded handles given to one run, offline and live, the cache, the comparison by run match, provenance, `whizbang runs replay`; the run's end shares what it kept; a panic at `Publishing`; ADR 0049 | B1 | Large, in two pull requests: about 1,100 (items 1, 2 and 9: ADR 0049, `Executor::start_replay` and the recorded handles, the end's two rows), then about 2,200 (cache 150, comparison 550, live 250, command 250, provenance 200, tests 800) |
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

**Seams.** B1 and R1 land in two pull requests each, as the table says, the first of each
merged before the second is reviewed. U1 and U2 name where they split if either passes about
2,500 changed lines: U1, saved examples (item 2); U2, replay of one run (item 3). A split is said in the hand-back and the owner is told the count. Nothing
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
   250 seconds, coverage is reviewed first and the slowest tests are sped up, moved to
   another tier or removed where what they guard is covered elsewhere, as section 13 says,
   until five runs are under 250 with no limit raised. The row's second remedy, running the
   tier locally in CI's two parts, is not taken by A0: `SPEC.md` section 13 holds one full
   local run to 300 seconds, and two parts of 300 each would raise that budget, which only
   the owner may do. If the tier cannot be brought under 250 without testing less, A0 stops
   and asks the owner ("Open questions"). The record of the five runs goes in the pull
   request; the row closes with the numbers.
2. **The pack's budget** (carry-over, "The pack's tests' budget"). The pack's run on the
   macOS runner measured over five runs from CI's reports, each test's time listed; the
   slowest cut (a fake's waits, a front's deadlines taken from a test clock where they are
   not), so that the run is under 10 seconds on `macos-15` in five runs. If it cannot be
   without testing less, A0 asks the owner for the budget, with the measurements, as the
   row says.
3. **The Chrome case that timed out once** (carry-over, the row on `run.spec.ts`). The case
   itself already waits on the result's tag (`ui/e2e/run.spec.ts`, the `lane`); the
   `toHaveURL` that timed out is in the helper it calls after each Run, `runWhenReady`
   (`ui/e2e/play.ts`, its wait for `#/run/`), under two tabs and two runs at once. The helper
   waits instead on what the running screen shows, with the address checked after; every other caller of the helper keeps passing. Then it is run 20 times in a row
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

- [ ] Done when: five full checks on the owner's Mac each under 250 seconds of test time,
  or the owner's word on the budget; the pack's run under 10 seconds on
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
2. **The writer** (`crates/whizbang-core/src/bundle/`): a root run's family and, for a
   chain, each step's, read through one snapshot of the store; links found by identity (an
   input whose identity an earlier step's output has; two such outputs refuse the chain,
   naming the step and port; a child run named as a step is refused, naming its root); each
   step marked replayable or not ("What a bundle may hold"); the members and the manifest;
   the ustar with fixed headers, names split between `prefix` and `name` and a name that
   cannot be so written refused; the time a parameter. "Export run" (`report.rs`,
   `tar_file`) moves to this writer, passing the run's end as today, and keeps its bytes
   (`play::export_run_is_the_same_bytes_after_the_writer_moved`).
3. **The reader**: every bound and rule of "Reading one trusts nothing", each with an
   adversarial test, on bundles built in the test (no committed binary fixture but one,
   below); nothing is published until every rule has passed.
4. **The import and the deletion**: one publication of the artifacts with their import mark
   and every other member under `bundles/<digest>/`; a second import changes nothing; a
   bundle's runs are not the store's own (`runs list`, `runs stats` and the ledger do not see
   them); a deletion removes the entry's names in one publication, its artifacts staying
   while anything else names them.
5. **The commands**: `whizbang runs bundle <run id>... --to <file>` (one root run, or a chain
   in the order given, at most 16), written as `store export` writes (not over a file without
   `--overwrite`, not through a link, not inside the store, `crates/whizbang/src/export.rs`);
   `whizbang bundle import <file> --store <dir> [--create]`; `whizbang bundle show <file>`
   (the manifest, its steps and links and what it withholds, reading nothing into a store);
   `whizbang bundle list|delete <digest> --store <dir>`.
6. **`run/7`, and the card** (carry-over, "The run record of a request to a GPU host names
   `device cuda` alone"): the record type gains the card and `replayed`, written only where
   present; the card read where the worker names it, as "The record a bundle carries" says,
   held to 64 printable characters, secrets out; the schema `run-7.json`; every older record
   reads as before. `remote::the_run_records_the_card_the_fake_host_named`,
   `record::a_record_with_a_card_is_run_7_and_one_without_is_written_as_before`.
7. **What the store keeps** (carry-over, "The engine API's hourly collection lets go of the
   names of the uploads"; "What the store keeps for replay"): a fill run keeps its painted
   answer as `fill-answer.png`; a judge's exchange keeps its turn's output ceiling
   (`capability/written.rs`, read back as none for an older exchange); `Store::prune` is run by the writer that holds the store
   after the engine API's hourly collection, so an upload's bytes go with its name; a
   bundle's entry and (for U1) an example's are names that keep what they hold from
   `delete`, `prune` and the collection.
   `fill::a_fill_run_keeps_the_painted_answer_that_a_feathered_mask_was_composited_from`,
   `segment::the_answer_rebuilt_from_a_runs_masks_and_its_diagnostic_is_the_answer_it_was_given`,
   `store::collect::the_hourly_collection_ends_in_a_prune_and_an_uploads_bytes_go_with_its_name`,
   `store::collect::a_run_going_keeps_every_file_a_held_reader_reads`,
   `store::collect::a_bundles_entry_keeps_its_artifacts_from_delete_prune_and_the_collection`,
   `store::collect::a_deleted_bundles_artifacts_go_at_the_next_prune_unless_a_run_names_them`,
   `store::collect::an_artifact_two_bundles_share_stays_until_both_are_deleted`,
   `store::collect::an_ordinary_import_before_or_after_a_bundle_keeps_its_artifact_when_the_bundle_goes`,
   `capability::written::an_exchange_keeps_its_turns_output_ceiling_and_an_older_one_reads_as_none`.
   **The model tier, by the orchestrator, before B1's first pull request merges** (no spend:
   the Mac's GPU): item 7 changes `fill` and item 6 may change what the pack's worker or the
   kind `remote` report, so `scripts/test-models.sh` runs on the branch on the owner's Mac
   under `scripts/heavy.sh`, and where a changed source is one whose digest committed
   evidence holds (`crates/whizbang/tests/it/evidence.rs`), the record is written again by
   its runner's candidate mode (milestone 6, P1, item 5) and committed with the source.
8. **A committed fixture**: `fixtures/bundle/remove-then-check.tar`, a chain of a removal
   and its check made by the fixture kit with fake models, after item 7 (so its fill keeps
   its answer), read by the tests of this build and every later one, so a later change of
   format must keep reading it.

- [ ] Done when: `bundle::the_same_runs_make_the_same_bytes_twice`,
  `bundle::a_bundle_reads_back_whole_with_every_member_checked`,
  `bundle::a_member_named_absolutely_with_dot_dot_twice_or_as_a_link_is_refused_and_nothing_imported`,
  `bundle::a_member_not_named_by_the_manifest_or_a_named_one_missing_is_refused`,
  `bundle::an_artifact_whose_file_digest_or_decoded_identity_differs_is_refused`,
  `bundle::a_diagnostic_whose_bytes_differ_from_its_record_is_refused`,
  `bundle::a_manifest_that_contradicts_a_record_or_a_link_that_contradicts_both_is_refused`,
  `bundle::a_family_with_a_missing_parent_a_cycle_or_two_roots_is_refused`,
  `bundle::a_child_listed_by_one_parent_and_naming_another_or_listed_twice_is_refused`,
  `bundle::two_files_of_one_identity_with_other_origins_travel_and_each_run_keeps_its_own`,
  `bundle::an_unknown_version_is_refused_naming_it`,
  `bundle::a_bundle_over_its_bounds_is_refused_before_it_is_read_whole`,
  `bundle::a_long_member_name_round_trips_through_prefix_and_name_and_one_too_long_is_refused`,
  `bundle::an_opaque_part_a_missing_picture_and_a_cached_check_are_listed_as_withheld`,
  `bundle::a_step_of_a_recipe_or_tool_file_is_carried_as_not_replayable`,
  `bundle::the_manifest_holds_no_path_and_no_secret`,
  `bundle::a_chain_links_an_input_to_the_earlier_output_it_took`,
  `bundle::an_input_two_earlier_outputs_could_be_refuses_the_chain`,
  `runs_bundle::a_bundle_imports_into_an_empty_store_and_survives_its_archive_and_first_store_gone_a_restart_and_a_prune`,
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
2. **Recorded capability handles, given to one run** (`crates/whizbang-core/src/capability/
   recorded.rs`): one for each capability, built from a root's family as the store or an
   imported bundle holds it, offered as the record's backend and model, which answer the
   replayed run's requests in their order from the table in "Replay", and `diverged` or
   `unanswerable` otherwise, the first such answer noted on the replay and every later
   request refused the same way ("Replay", divergence). They name no recipient and no price.
   **The executor takes them
   for that one run**: `Executor::start_replay(source, mode, request id)` makes the call from
   the record ("The call"), prepares and admits it with the recorded handles in place of the
   snapshot's for that run and its descendants alone (`contract/engine.rs`, where admission
   takes the handles from the snapshot), and starts it like any run: the same table of
   started runs and its bound, the same status, cancel, stated time and shutdown. No other
   run sees the handles, and nothing of the config is read for a backend.
3. **The decision cache and the call's limits.** A replay neither reads nor writes the
   decision cache (`crates/whizbang-squint/src/check.rs`, `cache::find`), through a flag the
   run's context carries; a check answered from the cache is `unanswerable` offline. Only a
   root of an action this build ships is replayed; a child, a user's recipe or tool file, an
   `interrupted` run, a setting that may have been cut and (offline) a tool file are
   refused, each naming why; the choices are found from the settings' aliases ("The
   call").
4. **An opaque part** (carry-over, "A run cannot be sent again from what the store holds of
   it"): a judge turn that held one is `unanswerable` offline, naming the part's adapter and
   length; live, the run is made from its call, not from its exchanges, so no part is needed;
   the row closes with that line.
5. **The comparison** (`crates/whizbang-core/src/contract/replay.rs`): of "What it compares",
   family against family, runs matched by place, references to runs compared through the
   match, measurements and findings among what is compared, first difference first, at most 256 with a count of the rest.
6. **Live replay**: the current config's backends, credentials, egress and prices; a backend
   the record names that the config does not offer refuses it; settings that differ from the
   record's reported.
7. **The command**: `whizbang runs replay <run id> | --bundle <file> [--step <n>] [--live]
   [--request-id <id>] [--json]`, against a store (`--store`), the report printed, and the
   exit code 0 for reproduced, 3 for differs, diverged or unanswerable, 1 for a refusal. A
   bundle's replay imports it first (B1, item 4).
8. **Provenance and request ids**: `replayed` is part of the request's fingerprint
   (`contract/request.rs`) and written with the admission and the record, for every run of
   the family; `runs stats` leaves replayed families out by default
   (`--replayed include|exclude|only`), so the figures of ordinary use are not changed by
   replays (`crates/whizbang/src/stats.rs`).
9. **The end's two rows.** (a) *What a run kept is moved into its end* (carry-over, "every
   run's end clones what the run kept"): `taken` (`contract/engine.rs`, the end's
   publication, where it is cloned at each attempt) is held once, shared and immutable
   (`Arc`), so the end and a second end after a panic read the same bytes and none is
   copied.
   `execute::an_end_holds_one_copy_of_what_the_run_kept_and_a_second_end_after_a_panic_writes_it_whole`.
   (b) *A panic at `Publishing`* (carry-over, "a panic in the executor's code while a run's
   end is at `Reached::Publishing`"): readable is not durable (a publication makes the new
   pointer visible by a rename before the root folder is synced, `store/mod.rs`, the
   module's steps 4 and 5), so a snapshot cannot tell; and a generation's number is not this
   publication's (a publication that unwound before its rename leaves the number to the
   next, `store/mod.rs`, where it is chosen under the lock). The end's own evidence is used
   instead: before it publishes, the end holds the digest of the record bytes it writes, which
   no unwinding loses. The store gains `Store::confirm(name, digest)`, which syncs the root
   folder and answers whether the current generation names the run's record with exactly that
   digest; a run's record is written once, by its end, so that answer is this end's. The catch
   calls it; when it answers yes, the run is answered as a durable end is (finishing what is
   left once, as the `Durable` arm does); when it answers no or fails, as today.
   `execute::a_panic_after_the_pointer_was_renamed_and_before_the_sync_is_confirmed_by_a_sync_and_answers_the_durable_end`,
   `execute::a_panic_after_the_sync_answers_the_durable_end` and
   `execute::a_panic_at_publishing_before_the_pointer_was_renamed_answers_as_before`, and
   `execute::a_panic_before_the_rename_then_another_publication_is_not_taken_for_the_end`.

- [ ] Done when: `replay::an_offline_replay_of_remove_prompt_reproduces_it_with_no_backend_configured`,
  `replay::an_offline_replay_of_remove_checked_reproduces_its_attempts_and_verdict_through_the_run_match`,
  `replay::an_offline_replay_of_a_check_reproduces_its_turns_checklist_and_verdict`,
  `replay::an_offline_replay_of_a_recipe_whose_check_takes_measurements_reproduces_it`,
  `replay::an_offline_replay_of_a_fill_through_a_feathered_mask_reproduces_its_picture`,
  `replay::an_offline_replay_sends_nothing_reserves_nothing_reads_no_key_and_starts_no_worker`,
  `replay::a_replay_neither_reads_nor_writes_the_decision_cache`,
  `replay::a_check_answered_from_the_cache_is_unanswerable_offline_and_asks_afresh_live`,
  `replay::a_fill_run_made_before_its_answer_was_kept_is_unanswerable_naming_it`,
  `replay::a_changed_operation_is_reported_as_a_difference_at_its_step`,
  `replay::a_request_the_original_did_not_make_is_diverged_and_nothing_is_called`,
  `replay::a_turn_that_held_an_opaque_part_is_unanswerable_naming_it`,
  `replay::a_child_an_interrupted_run_a_recipe_files_run_and_a_setting_at_its_limit_are_each_refused`,
  `replay::a_check_that_called_a_tool_file_is_refused_offline_and_replayed_live_only_with_the_same_digest`,
  `replay::a_tool_file_call_that_failed_in_the_original_runs_no_file_offline_and_is_refused_live`,
  `replay::a_replay_from_a_bundle_holds_its_inputs_after_the_bundle_is_deleted_and_pruned`,
  `replay::choices_are_found_by_alias_for_an_explicit_choice_a_default_and_two_aliases_of_one_model`,
  `replay::a_changed_measurement_of_an_action_with_no_artifact_output_is_a_difference`,
  `replay::a_divergence_in_a_second_look_or_a_loops_check_is_the_replays_report_whatever_the_action_made_of_it`,
  `replay::an_answer_cut_at_its_ceiling_replays_as_it_ran`,
  `replay::a_live_replay_spends_as_a_run_and_reports_a_changed_verdict_as_a_difference`,
  `replay::a_live_replay_whose_backend_the_config_no_longer_offers_is_refused`,
  `replay::a_request_id_under_another_mode_or_source_is_a_conflict_and_the_same_replay_returns_its_run`,
  `replay::a_repeated_replay_is_answered_after_its_source_was_deleted_and_after_the_config_changed`,
  `replay::every_run_of_a_replayed_family_names_it_and_stats_leaves_the_family_out_by_default`,
  `replay::a_replay_in_a_held_store_is_cancelled_waited_for_and_counted_like_any_run`,
  `runs_bundle::a_bundle_imports_into_an_empty_store_and_survives_its_archive_and_first_store_gone_a_restart_and_a_prune`
  (its replay completed here: the chain replayed offline and reproduced), and item 9's three
  pass; ADR 0049 is merged; the three rows closed with their lines.

## U1. The playground: saved-run comparisons and saved examples (large)

### Goal

A person pins one run as a baseline and compares another of the same task with it, and
saves a run's inputs and settings as a named example that opens as a draft; built as G1's
designs show.

1. **Saved-run comparisons**: the host's `GET /api/compare/{id}/{id}`, from the two families
   ("Saved-run comparisons"), reading them as the result screen does
   (`crates/whizbang/src/play_result.rs`), refused 422 for two tasks or a child run and 409
   for a run that has not ended;
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
  `play_compare::two_paid_loops_compare_cost_and_models_over_their_families_with_unresolved_spend_said`,
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
   no path); `GET /api/bundles`, `GET /api/bundles/{digest}` (its manifest, steps, links
   and what it withholds) and `DELETE /api/bundles/{digest}`. **A bundle's upload takes one
   of the host's two upload places**, as a picture's does (`play/guard.rs`, where only
   `Body::Picture` takes one today), refused 429 after its headers when both are taken, and
   **one bundle is read and imported at a time**, the reader's memory held to one artifact
   and the manifest; so at most two of the host's workers are ever held by uploads, and
   polling, Stop and Quit stay answered (ADR 0041, amended, with the bound in the guard's
   table).
2. **Guided chain replay**: "Start the chain", each step opened as a draft from the bundle's
   step, Continue, Stop; a linked input bound to the new run's output by port and place, a
   changed identity shown; a missing or doubled output stops the chain; the run started from
   a step carries `replayed` with the bundle's digest and the step (the start route takes an
   optional `replay` object, checked against the bundle's entry, and part of the request's
   fingerprint as R1, item 8, makes it); the chain's place in the
   browser's storage.
3. **Replay of a run**: "Replay" on a result, offline through `POST /api/runs/{id}/replay`,
   which calls `Executor::start_replay` (R1, item 2) on the host's one executor, so the run is
   polled, stopped and counted as any run there; and live with the cost reviewed first as any
   paid run; the report as a screen.
4. **What G1's table assigns to U2**, each with its test.
5. **The guide**: `docs/guides/playground.md`, "Bundles", "Replaying a chain", "Replaying a
   run".

- [ ] Done when: `play_chain::a_step_runs_only_when_started_and_continue_binds_the_new_output`,
  `play_chain::a_link_that_finds_no_output_or_two_stops_the_chain`,
  `play_chain::a_run_started_from_a_step_records_the_bundle_and_the_step`,
  `play_bundles::a_bundle_uploaded_is_read_whole_before_anything_is_imported`,
  `play_bundles::a_bundle_over_256_mib_is_refused_413_before_it_is_read_whole`,
  `play_bundles::with_two_bundle_uploads_stalled_a_third_is_refused_429_and_polling_and_stop_are_answered`,
  `play_bundles::a_deleted_bundle_leaves_the_artifacts_a_run_names`,
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
| A run cannot be sent again from what the store holds of it | R1, item 4 |
| The engine API's hourly collection lets go of the names of the uploads no run took | B1, item 7 |
| The Google rows of prices are introductory | N1, item 4: re-owned to E with a line, since no live judge run happens |
| `choice_json`'s `Err(_)` arm | U1, item 4 |
| Home's library lists only the pictures given through the playground | U1, item 3 |
| Every run's end clones what the run kept | R1, item 9 (a) |
| A panic at `Reached::Publishing` is answered as abandoned | R1, item 9 (b) |
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
| B1 | Revert with care: a store written by B1's build may hold `run/7` records, which a build before B1 refuses by version when it reads one, and `bundles/` and `examples/` names, which it leaves alone (the store's names take any text, `store/buckets.rs`). ADR 0048 says this; a revert keeps the record's reader and drops the writers |
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
7. **A new record version and two new store entries.** Older builds refuse a `run/7`
   record by its version and leave the new names alone; the revert notes say so.
8. **Replay touches the executor.** A run's handles chosen at its start, and the decision
   cache bypassed, are new seams in the core. R1's first pull request carries them alone with
   the end's two rows, and its reviewer reads that no ordinary run can be given recorded
   handles and that no replay can reach a configured backend.
9. **A fill's answer doubles what a fill keeps.** Each fill run keeps one more picture of its
   image's size, as a diagnostic under the run's bound; a store's growth from it is said in
   the guide.

## Open questions

None now. One may arise in A0: if the fast tier cannot be brought under 250 seconds on the
owner's Mac without testing less, A0 stops and the owner is asked, "I need a yes or no: may
`just check` run the fast tests in CI's two parts, each held to 300 seconds?" (which raises
the local budget of `SPEC.md` section 13, and so is the owner's).

## What the owner still does

- **G1**: draws and approves the new surfaces in Paper with the orchestrator, and says which
  of the six to leave out, if any. U1 and U2 wait for it; B1 and R1 do not.
- **A0, item 1**: only if the fast tier cannot come under 250 seconds without testing
  less, the owner's yes or no ("Open questions").
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
  `--live` makes the call again under today's config and spends as any run. Neither reads
  the decision cache. A fill run now keeps the picture the filler painted, so that its
  replay can be answered; fills made before this milestone cannot be replayed offline.
- **Saved examples live in the store** and keep their pictures after their run is deleted;
  a comparison's pin lives in the page's address.
- **The run record gains `run/7`**: the card a model ran on, and what a run replays.
- **The tags `m7-core` and `m7`** are made by the orchestrator as above.

## Review rounds

### Review round 1

On draft 1. Verdict: revise. Sixteen findings: 1 blocker, 13 majors, 2 minors. Each was
checked against the code; fifteen were right and taken, one was right in part.

1. **Blocker: a fill's committed image is not its answer.** Right: `fill` composites the
   painted picture under the mask (`crates/whizbang-rascal/src/fill.rs`, `pasted`), and a
   rounded blend cannot be undone. Taken: a fill run keeps its painted answer as a
   diagnostic from B1 on, before the fixture is made; an older fill is `unanswerable`
   offline; a test through a feathered mask; and a test that the segmenter's answer is
   rebuilt whole, or kept the same way.
2. **Major: recorded settings are not a config.** Right (`contract/call.rs`; settings cut at
   256 characters, `contract/record.rs`). Taken: "The call" says what a replay is made from;
   a cut argument refuses it; offline the recorded handles are offered as the record's
   backend and model; live, the current config, credentials, egress and prices govern, and
   nothing in a record or a bundle is authority for them.
3. **Major: a bundle carries no recipe or tool file.** Right (`wiring.rs`, `tool_file.rs`).
   Taken: a bundle carries data, never code; a step of a recipe or tool file is carried as
   `not replayable` and refused by replay.
4. **Major: the decision cache.** Right (`check.rs`, `cache::find` before the judge). Taken:
   no replay reads or writes the cache; a cached check is `unanswerable` offline and asked
   afresh live; tests of both.
5. **Major: a child's replay and run ids in what is compared.** Right
   (`measured.rs`, `held_to_their_runs`). Taken: only roots are replayed; families are
   compared with runs matched by place and references to runs compared through the match.
6. **Major: the playground has one executor and no seam for recorded handles.** Right
   (`play/mod.rs`; admission takes handles from the snapshot, `engine.rs`). Taken:
   `Executor::start_replay` gives one run and its descendants the recorded handles and starts
   it like any run, under the same bound, status, cancel, stated time and shutdown; the
   playground's route calls it.
7. **Major: provenance and request ids.** Right (`request.rs`; `stats.rs` selects records).
   Taken: `replayed` is part of the fingerprint, written with the admission and the record,
   on every run of the family; stats leaves replayed families out by default.
8. **Major: where an imported bundle's journals and diagnostics live.** Right. Taken: every
   member is an object named under `bundles/<digest>/`; a test of import, the archive and
   first store gone, a restart, a prune, then replay.
9. **Major: no whole-bundle checks.** Right. Taken: the reader's rules now hold the
   artifacts' decoded identities, diagnostics against their records, the manifest against
   the records, the families as trees and the links against both, with adversarial tests.
10. **Major: the tar writer cuts names at 100 bytes.** Right (`report.rs`, `tar_file`).
    Taken: names split between `prefix` and `name`, one that cannot be written refused,
    round-trip tests.
11. **Major: a bundle's upload and the host's workers.** Right (`play/guard.rs`, only
    `Body::Picture` takes an upload place). Taken: a bundle's upload takes one of the two
    places, one bundle is imported at a time with bounded memory, and a saturation test.
12. **Major: a bundle cannot be deleted.** Right. Taken: `whizbang bundle delete` and
    `DELETE /api/bundles/{digest}`, with a test that what a run names stays.
13. **Major: cost and models are over a family.** Right (`play_result.rs`). Taken: the
    comparison reads both families, with unresolved spend and an unread family said, and a
    test of two paid loops.
14. **Major: A0's split would raise the local budget.** Right (`SPEC.md` section 13). Taken:
    A0 cuts test time first; the split is the owner's to allow, asked only if needed.
15. **Minor: the Chrome case already waits on the tag.** Right in part: the case does, but
    the `toHaveURL` that timed out is in its helper, `runWhenReady` (`ui/e2e/play.ts`).
    Taken: A0 changes the helper.
16. **Minor: older builds do not refuse the new names.** Right (`store/buckets.rs`). Taken:
    the revert notes and risk 7 say an older build leaves the names alone and refuses a
    `run/7` record by its version.

The review found every milestone-7 carry-over row placed in a task. B1 and R1 are now two
pull requests each, the executor's seam in R1's first.

### Review round 2

On draft 2. Verdict: revise. Round 1's fixes held, but for findings 2, 3 and 12, which
were completed by this round's findings 2, 3 and 7. Eleven findings: 1 blocker, 7 majors, 3
minors. Each was checked against the code; all were right and taken.

1. **Blocker: a snapshot cannot tell that an end is durable.** Right: the pointer is renamed
   before the root is synced (`store/mod.rs`, the module's steps 4 and 5). Taken:
   `Store::confirm` syncs the root and answers whether `CURRENT` names the generation; the
   catch at `Publishing` calls it; tests before the rename, between it and the sync, and
   after.
2. **Major: a backend's identity is not its choice name.** Right (`Model.backend` against
   `snapshot.rs`'s aliases; Copy command resolves them in `play_result.rs`). Taken: choices
   are found from the resolved settings' alias names, a default written explicitly, an
   ambiguous or older record refused for live replay; tests of an explicit choice, a default
   and two aliases of one model.
3. **Major: a shipped check runs tool files.** Right (`check.rs`, `dispatch_file`). Taken: a
   run whose settings name a tool file is refused offline, and replayed live only with a file
   of the same digest; a test.
4. **Major: an exchange does not keep its output ceiling.** Right (`written.rs`;
   `check.rs` acts on it). Taken: from B1 on an exchange keeps it; an older exchange cut at a
   ceiling it did not keep is unanswerable; tests.
5. **Major: measurements were not compared.** Right (`edge_similarity` commits no artifact,
   `compare.rs`). Taken: measurements and findings are compared, numbers exactly; a test.
6. **Major: an action may handle a divergence and go on.** Right (`check.rs`'s second look;
   `checked.rs`'s unchecked attempt). Taken: the first divergence is noted on the replay,
   later requests refused, and the report is the replay's; a test of both cases.
7. **Major: a bundle's artifacts under ordinary import marks cannot be released.** Right
   (`artifact/stored.rs`; prune keeps every named object). Taken: a bundle's artifacts are
   named under its entry alone; tests of two bundles sharing one, and an ordinary import
   before and after.
8. **Major: the model tier.** Right (`SPEC.md` section 13). Taken: the orchestrator runs it
   on B1's first pull request, with evidence written again by candidate mode where a source's
   digest is held.
9. **Minor: arguments are not cut; settings are cut with no mark.** Right. Taken: "The
   call" says so; a setting exactly at its limit is taken as possibly cut.
10. **Minor: Export run's time.** Right (`report.rs`). Taken: the writer takes the time as a
    parameter.
11. **Minor: three stale references in the carry-over table.** Right. Taken.

### Review round 3

On draft 3. Verdict: revise. Round 2's fixes held, but for its findings 1 and 3, and the
wording of its finding 9 under saved examples. Seven findings: 1 blocker, 5 majors, 1 minor.
Each was checked against the code; all were right and taken.

1. **Blocker: a generation's number does not prove this end was published.** Right: the
   number is chosen under the lock from the current pointer (`store/mod.rs`), so a
   publication that unwound before its rename leaves it to the next, and an unwinding
   publication returns nothing. Taken: the end holds the digest of the record it writes;
   `Store::confirm(name, digest)` syncs the root and answers whether the current generation
   names the run's record with that digest; a test where another publication comes between.
2. **Major: a failed tool-file call notes no digest.** Right (`check.rs`, the error arm).
   Taken: a replay's context refuses every tool-file dispatch offline, so no file runs; live,
   a call whose digest was never noted is refused; a test.
3. **Major: a bundle's names are not seen by lookup or admission.** Right
   (`artifact/stored.rs`; admission's retention in `engine.rs`). Taken: a run started from a
   bundle looks its inputs up through the bundle's names, and admission gives each a name of
   the run's own; a test of the bundle deleted and pruned after admission.
4. **Major: one file per identity loses an origin.** Right (`artifact/wire.rs`;
   `artifact/stored.rs` keeps each run's own file; `commit.rs`'s test). Taken: files are
   members by file digest, each mapped to the runs and ports it is the file of; a round trip.
5. **Major: parents and listed children were not reconciled.** Right (`record.rs`'s checks
   are local). Taken: the two directions of a family must agree, a child listed once and by
   its own parent, with action and ending; adversarial tests.
6. **Major: a replay's request id must be answered before its source is read.** Right
   (`request.rs`; the lookup before readiness in `engine.rs`). Taken: a replay's fingerprint
   is made from the replay request, not the rebuilt call; a test after the source was deleted
   and after the config changed.
7. **Minor: saved examples still said arguments are cut.** Right. Taken.

