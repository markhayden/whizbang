# 0049. Replay

Status: accepted. Builds on ADR 0048 (`run/7`'s `replayed`); changes ADR 0007's admission
record (an admission of a replayed run names the run it replays)
Date: 2026-11-25
Spec: section 3, "The execution contract", "Request identity" and "What every run
records"; section 6, "Spend" ("After a crash, nothing that was dispatched is replayed
automatically"); section 7, rascal's row "Replay from a journal; invocation bundles |
Deferred | Milestone 7"; section 9 (a judge's decision is never read from the cache in
evaluation, "every repetition is a fresh trial"); plan m7, task R1

## Context

Milestone 7 makes a stored run again, from its own store or from a bundle imported into
one, and compares what the new run made with what the old one made
(`docs/plans/m7-bundles-and-replay.md`, "Replay"). The spec names "journal replay" and
says no more.

Rascal is the reference, never the authority. Its `inspect --replay` re-ran an
operation's record on its stored inputs and compared the planes it made, and its
`--replay <recorded run>` answered the fillers, the checks and the pack of a heal from the
recording, "never a call" (rascal `crates/rascal-core/src/replay.rs` and rascal's
`SPEC.md`, the replay of a heal, at the commit `docs/sources.toml` pins). Whizbang keeps
the idea and not the code: a run here is a family of runs under one executor, a model's
answer reaches an action through a capability handle, and the store keeps what each run
recorded (ADR 0048, "What the store keeps for replay").

What a replay needs of a run, the store keeps since B1: a segmenter's answer is rebuilt
from the run's committed masks and `segment.txt`; a fill's painted answer is the
diagnostic `fill-answer.png`; a judge's turns are its exchanges, but for an opaque part's
body and a picture, kept as digests (ADR 0009). A journal is a run's progress events
alone, and nothing replays from it: "journal replay" is a replay from what the store
recorded of the run.

Two reviews of B1 left two questions for this decision. A bundle's manifest says whether
each step is replayable, and the reader takes that as given (`bundle::check`). And
`run-7.json` says a record a dead process left is never `run/7`, so a replayed run that a
killed process left would lose its `replayed`, which `runs stats` needs to leave replayed
families out.

## Decision

### What is replayed

A **root run** (a run nothing nested) of an action this build ships, at the version this
build ships it, that ended (completed, cancelled, error or invalid; not `interrupted`, and
not one whose record lost its admission), from the store it is in or from a bundle the
store holds. A child is replayed as part of its root, never alone: a child's action may
need its place in its recipe (a `check` given measurements holds them to the runs of its
own recipe). A family any of whose runs was interrupted is refused, naming the run: the
steps it lost cannot be answered. Anything else is refused before anything is prepared,
naming why (`Refusal::NotReplayed`, whose kind is `invalid`; a replay is never a call from
inside an action, so no record lists it).

**Whether a run replays is this build's decision, from the records, never a manifest's
claim.** A bundle's manifest says `replayable` and `why` for each step (ADR 0048), and its
reader checks only that the manifest is the one its writer would write. A replay from a
bundle reads the step's records from the bundle's entry and holds them to exactly the
rules above, as for a run of the store: the action and its version against what this
build ships (`Engine::with_shipped`, which the binary gives from `whizbang describe`'s
actions), the ending, the family. A manifest that claims a step replayable when its
records say otherwise changes nothing; one that claims a step not replayable when its
records say it is changes nothing either. `whizbang bundle show` prints what this build
decides of each step, from the records, and says where the manifest's claim differs.

### The call

A replay's call is made from the root's record: its action, its arguments as the record
holds them (defaults applied; arguments are kept whole, with secrets taken out), and its
inputs by identity, in the order the record lists them. A setting's text is cut at
`SETTING_TEXT_LIMIT` with no mark, so a family any of whose settings has a text exactly
that long is refused, naming the setting and its run.

**The choices.** A call names its backends by the config's choice names (aliases), and a
record's models name each backend's declared identity, which two aliases may share. So
each choice is taken from the records' resolved settings, whose names hold the alias
(`backends.<capability>.<alias>.…`). Offline, the recorded handles are offered under the
alias the settings name, mapped to the model that answered that run; a choice the settings
do not name, or a record of none, is answered by the first model of that capability the
family's requests were answered by. Live, a record whose settings do not name exactly one
alias for a capability its family asked is refused, naming the capability, and each choice
the run took from the config's default is written explicitly.

### Offline replay, the default: recorded handles given to one run

`Executor::start_replay(replay)` makes a new run of the call through the store's one
executor and starts it as `Executor::start` starts any run: the same table of started runs
and its bound, the same status, cancel, stated time and shutdown. **Offline, the executor
admits that run, and every run it calls, under one snapshot of its own**, which offers
**recorded capability handles** (`capability/recorded.rs`) and nothing else: the config's
source is never asked, so nothing of the config is read for a backend, no backend is
built, no key is read and no worker is started. The snapshot's digest is the root
record's config's. It records no setting. No other run sees the handles.

The handles answer from the original family, and from nothing else:

| Capability | Answered from |
|---|---|
| `segment` | The original `segment` run's committed masks, with each one's score, box and predicted IoU from its `segment.txt` |
| `fill` | The painted picture the original `fill` run kept (`fill-answer.png`) |
| `judge` | Its stored exchanges, turn by turn |

**In what order.** For each capability, every request the family sent, the runs matched by
place: the root first, then each run's steps in the order its record lists them, at any
depth, and within a run its requests in the order its record's `models` lists them. The
replayed family, made by the same build from the same answers, sends its requests in the
same order; one that does not has diverged. A request is matched to the next recorded
request of its capability and must be **that** request:

- of the model the original's request was answered by (by backend, model and version);
- for `segment`, about the same image (the request's binding is the one the original run's
  `image` input belongs to) with one query;
- for `fill`, of a picture the size of the painted answer;
- for `judge`, the same request: what the replay's request writes, at the stored
  exchange's place, with every opaque part as its digest and length and every picture as
  its digest, is the stored exchange up to its answer, byte for byte.

A recorded request that ended with a fault is answered with that fault, so an action that
handled it handles it again. A segmenter's answer is made only from a run of `segment`
with its one request, and a filler's only from a run of `fill` with its one request: no
other run keeps them.

**The handles name no recipient and no price.** Egress has nothing to send; the estimate
is nothing, so the ledger is not asked and nothing is reserved; the meter has nothing to
charge. The replayed record's `models` name the original's backends and models, its
spend lists no paid request, and its egress is empty.

**Divergence is the replay's, not the action's.** A request the original did not make, or
made otherwise (another model, another request, a request past the last recorded), is
`diverged`. An answer the store cannot give is `unanswerable`: an opaque part withheld
(naming the part's adapter and length), a turn that was not written, a picture not held, a
diagnostic gone, a fill with no kept answer (a run made before B1, naming that), a check
the decision cache answered (whose turns are another run's), a request whose backend's
call unwound. **The first `diverged` or `unanswerable` answer is noted on the replay**,
every later request of the family is refused the same way, and the replay's root keeps
the note as its diagnostic `replay.txt` (`diverged judge request 1: …`), written once
every run under it is over. An action may handle a capability's fault and go on (a
check's second look leaves the earlier verdict standing; the loop turns a failed check
into an unchecked attempt): the replay's report is the first stop whatever the actions
made of it. Each such request is answered as a fault of the kind `unavailable`, which the
record spells as every backend's; its sentence says `diverged` or that the replay cannot
answer, and the note is what the report reads. A new kind of fault would change every
record version's list of how a request ended, for an answer only a replay gives.

**Pixel work runs for real.** Every operation and every step of a recipe is computed
again; only the three capabilities are answered from the record.

### The decision cache and tool files

**A replay neither reads the decision cache nor writes it**, offline or live: the run's
context says it is a replay (`Context::replay`), and `check` makes no key. Offline, a
check would otherwise be answered by the entry its own original wrote; live, each
repetition is "a fresh trial" (section 9). A check the original answered from the cache is
`unanswerable` offline (its turns are another run's), and asks the judge afresh live.

**A tool file is code, and a replay never takes one from a record or a bundle.** Offline,
a family whose settings name a tool file (`tool_file:<name>`, with its digest) is refused
before it starts, naming the file. Since a call of a tool file that failed notes no
digest, the refusal is also made where a tool file is dispatched: in an offline replay,
`check` answers every tool-file call as not called, the replay cannot answer it, and no
file runs. Live, a family that called a tool file is replayed only when the current config
names a file of that name with the digest the record noted; one whose digest was never
noted is refused, naming the file.

### Live replay

`--live` makes the call again with the configured backends, as a new run under
preparation, admission and the allowance: the store's current config, credentials, egress
approvals and prices govern it, and nothing in a record or a bundle is authority for them.
A backend the record names that the config no longer offers refuses the replay, naming
it; a resolved setting that differs from the record's is reported beside the comparison.
What it spends is in its record and the ledger like any run's. A live replay of a step
whose input is withheld is refused before it is prepared. A live judgment is not promised
to repeat, only the call; nor are a model's pixels.

### What it compares

Once the replayed run has ended, family against family (`contract/replay.rs`): runs
matched by place (a root to a root, each child to the child at the same place among its
parent's steps, a loop's attempt to the attempt of the same number), and a reference to a
run inside what is compared (a step's run, a measurement's producing run) compared through
that match, never by its id. Then each run's outcome, its committed outputs by identity,
port and place, each step's action and arguments, its measurements and findings, a check's
checklist and verdict, and a loop's attempts. Numbers are compared exactly: a replay runs
the same build on the same pixels. Times, run ids and request ids are not compared. The
report is **reproduced**, **differs** (each difference by place, first first, at most 256
with a count of the rest), **diverged** or **unanswerable** (from the note).

### Its request id, and its fingerprint

A replay may be given a request id like any call. Its fingerprint is made from **the
replay request itself**, kind `replay/1` (ADR 0001's encoding): its mode, and its source
(a run's id, or a bundle's digest and step). It is never made from the call rebuilt from
the source, so the executor's durable look-up of the id answers a repeat **before the
source is read**: the same replay asked again returns its run after its source run or
bundle was deleted, or the config changed; the same id under an ordinary call (`request/1`),
another mode or another source is a conflict. `whizbang runs replay --bundle <file>` reads
the file's SHA-256 as a stream to make the fingerprint, and imports the bundle only when
the id is not a repeat, so a repeat publishes nothing.

### Provenance, and a replayed run a dead process left

`replayed` (ADR 0048, `run/7`) is noted for every run of a replayed family, a child as its
root does, and written in its record at its end. **It is also written in each run's
admission**, as a field `replayed` of `admission/1` written only for a run of a replayed
family, so every other admission is the bytes it was; the reader takes it when present,
and holds it to name another run and a step a bundle may have.

**A replayed run a killed process left keeps its provenance through its admission.** The
recovery that ends such a run writes the record a dead process leaves (ADR 0013), which is
never `run/7` (`run-7.json`) and holds no `replayed`; its admission, which the store keeps
under the run's names, still says what it replayed. `runs stats` reads the admission of
each interrupted run, and leaves a family out when its root's record or, for an
interrupted root, its admission names a replay. So the figures of ordinary use are never
changed by a replay, whether it ended or was killed. The record of a dead process is not
changed: a `run/7` that has lost groups would need the schema of every lost group again,
for a fact the admission already keeps.

### The end of a run (two rows of the carry-over list)

- **What a run kept is held once.** Its end takes the journal and the diagnostics once,
  as one shared, unchanged list (`Arc`), and every attempt at the end, a second end after a
  panic among them, writes those bytes without copying them.
- **A panic while the end is being published asks the store.** Readable is not durable (a
  publication shows its pointer by a rename before it syncs the root folder), and a
  generation's number is not that publication's (one that unwound before its rename leaves
  the number to the next). So the end keeps the digest of the record it writes, which no
  unwinding loses, and a panic at `Publishing` calls `Store::confirm_record`: the store
  syncs its root and answers whether the current generation names the run's record with
  exactly that digest. A run's record is written once, by its end, so that answer is this
  end's. Yes: the run is answered as a durable end is, and what is left (the caller's
  allowance, telling the parent) is finished once; the store marks the run closed in its
  table of open runs, so it is let go of. No, or no answer: abandoned, as before, and the
  next open reads the store. An abandoned run's marker is put in the table before whoever
  waits for it is told, so a caller that waited finds it marked.

### Where it runs, and what is not built

A replay is one more run of the store's one executor (section 3, "One writer"): from the
command line while nothing holds the store, and from the playground through its host,
under the same bound on started runs and the same status, cancel and Quit as any run
there. Not built: resuming a run a dead process left (section 6: nothing dispatched is
replayed automatically; a replay is a new run, never the old one carried on); replay over
the engine API or the stdio server (ADR 0048, "Which doors"); a replay that reconstructs a
missing input; a replay of a user's recipe or tool file from a bundle.

### As R1 built it

- **The comparison** reads each record's groups as its JSON (the action, its version and
  arguments; its inputs, outputs, outcome, steps, judge, plan, start, findings and
  attempts) with every amount of money left out, and each diagnostic but the journal and
  the note, by its bytes, a text with each run id of either family read as its place.
  Differences are ordered as the runs ended, each run's steps before the run, so the first
  is where the replay first went otherwise.
- **`runs stats`** leaves replayed families out through `--replayed exclude|include|only`
  (`stats::Runs::keeping`); the playground's history and figures read the same `Loaded`, and
  take the choice up with its replay screens (plan m7, task U2).
- **A tool file's call that failed** in the original noted no digest, so a live replay
  cannot know of it from the record; it is answered as the live call answers, and the
  report shows any difference.

## Consequences

- Any ended run of a shipped action can be made again with nothing called and nothing
  spent, and compared with the run it replays; a bundle carries everything that needs.
- The replay path is a seam in the core: one snapshot, given to one run and its family,
  replaces the config's. A reviewer holds it to reach no backend.
- A replayed run's admission is not readable by a build before this decision: such a build
  refuses that admission (its reader takes no unknown field), so it treats the run as one
  whose admission was lost, and a request id used for a replay cannot be looked up by it.
  Every other admission of the store is the bytes it was.
- A divergence is reported as a fault of the kind `unavailable` in the replayed record, and
  told apart by the root's `replay.txt`; a reader of the record alone sees a fault.
- A check replayed offline asks the judge in the order the original did; a change of the
  build that changes what the check sends is `diverged`, which is the point.

## Alternatives

- **A fault kind `diverged`.** It would change the list of how a request ended in every
  record version and schema, for an answer only a replay gives; the note says it.
- **Answers matched per run, by the run's place, instead of per capability in order.** The
  handles a run is handed are made by the snapshot without its run; matching per run would
  thread the run's place through admission. The family-wide order of one build making the
  same requests is the same order, and a request out of it diverges.
- **Trusting the manifest's `replayable`.** A manifest is checked for its spelling, not its
  judgment; a bundle written by another build may say anything. The records are what the
  replay reads, so they are what decides.
- **`replayed` in the interrupted record (`run/7` for a dead process).** It needs the
  schema of every lost group in `run-7.json`; the admission already keeps it.
- **Resuming instead of replaying.** Section 6 forbids replaying anything dispatched
  automatically; a replay is a new run that someone asked for.
