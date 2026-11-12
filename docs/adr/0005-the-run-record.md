# 0005. The run record: one fixed shape with every group, written one way

Status: accepted
Date: 2026-11-12
Amended: 2026-11-12, inside checkpoint 1A, with the code that runs an action and writes
its run. Added: "Running an action, and writing its run". Changed: the sentences that said
nothing wrote a record, a journal or a diagnostic yet, and the list of what this record
does not decide.
Spec: section 3, "What every run records", "The execution contract", "Storage" and "Metrics
from ordinary runs"

## Context

`SPEC.md` section 3 says every run, whatever its outcome, writes one record with a fixed,
versioned shape, and lists what it always stores in eleven groups: identity, what was
asked, inputs and outputs, outcome, time, models, spend, egress, steps, judge runs and
diagnostics. It is the single source for debugging, audit and metrics, so nothing in it is
optional. Diagnostics are kept for failed and cancelled runs as for completed ones, secrets
are redacted before anything is written, and records are removed only by the store's
delete and prune commands. A metric is a query over these records.

What there is to build it on:

- The store (`crates/whizbang-core/src/store/`): runs, each with names of its own, and two
  kinds of reference, a committed output and a diagnostic. A diagnostic is read like
  anything else and refused where an output is asked for. Everything enters by a
  publication, which is atomic and durable before it is reported.
- A prepared call (ADR 0004): the action, its version, every argument that has a value,
  every input, the config's digest. And an outcome: completed, cancelled, error or invalid.
- Two things the old tools had. Rascal wrote an operation record for each committed
  invocation (`crates/rascal-core/src/workspace/record.rs`): the step, the operation and
  its version, the resolved parameters, the identities of inputs and outputs by port, and
  an identity of its own hashed over all of that. Squint redacted every secret from a text
  before storing it (`crates/squint-core/src/artifacts.rs`, `redact`) and kept a journal of
  typed events as a file it appended to and synced one event at a time
  (`crates/squint-core/src/journal.rs`).

What there is not: a capability request, a ledger, a recipient of data, a recipe step, a
nested call, a judge turn, a config, a request id. Six of the eleven groups record things
that nothing can do yet.

The record's shape is costly to reverse: once a store is kept, every record in it has it.

## Decision

The code of the record is in `crates/whizbang-core/src/contract/` (`record.rs`,
`redact.rs`, `journal.rs`), the schema in `crates/whizbang-core/schemas/run-1.json`, and
the tests that hold them to this record in
`crates/whizbang-core/tests/it/contract/record.rs` and `redact.rs`.

### The shape, `run/1`

A record is one JSON object with thirteen fields, always all of them, in this order:

| Field | Group of the spec | What it holds |
|---|---|---|
| `record` | | The kind and version: `run/1` |
| `identity` | Identity | `run`, the run's id; `request`, the caller's request id, or null; `parent`, the run that called this one, or null; `whizbang`, the version that ran it |
| `asked` | What was asked | `action` and `version`; `arguments`, each as its `name` and its `value`; `settings`; `config`, the digest of the config the settings came from |
| `inputs` | Inputs and outputs | Each input as its `port`, its `type`, the identity of the `artifact` and the identity of the `binding` the artifact belongs to |
| `outputs` | Inputs and outputs | Each committed output as its `port`, its `type` and the identity of the `artifact` |
| `outcome` | Outcome | `status`: `completed`, `cancelled`, `error` or `invalid`; `error`, its `kind` and `message`, or null; `invalid`, the reason, or null; `refusals` |
| `time` | Time | `start_ms` and `end_ms`, in milliseconds since the Unix epoch; `durations` |
| `models` | Models | |
| `spend` | Spend | |
| `egress` | Egress | |
| `steps` | Steps | |
| `judge` | Judge runs | |
| `diagnostics` | Diagnostics | Each diagnostic of the run as its `name` among the run's names in the store, the `digest` of what is stored under it, and its length in `bytes`; in the order of their names |

An argument's value is an object with one field, named for its kind: `number`, `integer`,
`boolean` or `text`. So a number that happens to be whole is still a number, and a record
is read back to the value that was written. Arguments are in the descriptor's order, after
defaults (ADR 0004), and an argument that has no value is not listed. A digest is its text,
`sha256:` and 64 hex digits. A type is an artifact type's name (ADR 0002).

A record is made from a prepared call and an outcome (`RunRecord::of`), with the run's
id, the version, the two times and the run's diagnostics: the action, its version, the
arguments, the inputs and the config's digest are the prepared call's; the status, the
error or the reason, and the outputs are the outcome's. An end that is before the start,
as a clock that went back gives, is written as the start.

### What is empty in `run/1`, and how it stops being empty

Eight lists hold nothing in this version, and cannot: `asked.settings`, `outcome.refusals`,
`time.durations`, `models`, `spend`, `egress`, `steps` and `judge`. Each is there, as an
empty list, in every record. In the code the element of each is a type that has no value;
in the schema each has at most zero items. A record with anything in one of them is not a
`run/1` record.

The shape of what goes in them is not decided here, because it follows from things that
do not exist: the request and response of a capability, the ledger's reservation, a
recipient, a recipe step, a judge's turn. Whatever first fills one of these lists defines
its elements and writes a new version of the record (`run/2`), with its own decision
record or an amendment of this one. Until a store is kept, a new version costs nothing: a
store written by earlier code of the checkpoint is discarded, as ADR 0002 says of masks.

`identity.request` and `identity.parent` are null in every record written today, because
no call carries a request id and no call nests. They are strings when they are filled, and
filling them does not change the version: a reader of `run/1` already accepts both.

### Written one way, and read strictly

A record is written as JSON on one line: no space between tokens, the fields of every
object in the order this record gives them, strings escaped as `serde_json` escapes them,
a number in the shortest form that reads back to the same double, with a point or an
exponent, a whole number without one. One newline follows.

Reading refuses:

- a record of another version, as another version: the `record` field is looked at first
  and by itself, and a store written by other code is not migrated;
- anything that is not of the shape: a field that is not known, a field left out, a
  digest, a type or a status that is none, a value under two kinds or under none;
- a record of the right shape that says what no run can: an `error` without the status
  `error` or the status without it; the same for `invalid`; an output of a run that did not
  complete; an end before the start; diagnostics out of the order of their names, or one
  name twice;
- bytes that are not the bytes this code writes for the record they hold.

The last rule is ADR 0002's for an artifact file, and for the same reason: one record has
one form, so two stores that hold the same record hold the same bytes, and the digest the
store names it by means the record.

The record has no identity of its own, and is not hashed into anything. Rascal's operation
record had one, over everything but the duration and a sequence number. A run is named by
its id, and what is stored is named by the digest of its bytes; nothing needs a third
name.

### The schema

`crates/whizbang-core/schemas/run-1.json` is the JSON Schema (2020-12) of a `run/1`
record, written by hand, and the core carries it as text (`RUN_RECORD_SCHEMA`). It and the
types are two statements of one shape. The tests hold them together: hand-written records
are read by the types and validated against the schema, and each change that the types
refuse is refused by the schema where the schema can say it.

The schema says less than the reading does. It does not hold a digest to its hex digits
(only to its length), the consistency rules of the last section, the order of
diagnostics, or the one way of writing.

The validator is the test kit's (`whizbang_testkit::schema`): the keywords this schema
uses and no others, and a keyword it does not know is an error, so a schema never passes
for holding less than it says. No schema library is added to the build.

### Redaction

`Secrets` holds the values that must never be written. It is squint's pass: every secret
is replaced by `[redacted]` where it stands as it is, and where it stands in the form a
JSON string gives it, so a secret quoted inside a JSON body is found. Two things are
added: a secret that holds another is replaced whole, because the longer is replaced
first; and the type does not print what it holds.

In a record, redaction is applied to each text that comes from outside the build, before
the record is encoded: an argument's value when it is a text, an error's kind and
message, and the reason a run was invalid. It is not applied to the encoded JSON, so it
cannot break the record's shape. Names that come from a descriptor, identities, numbers,
booleans and times are not texts from outside and are written as they are.

So "its exact arguments" (the spec) holds except where an argument held a secret: there
the record holds `[redacted]`, because the spec also says a secret is never written.

Redaction is by value. It takes out the secrets it was told of, and nothing else: it does
not find a key by its shape. What keeps other private things out of a record is the shape:
it has no field for a path, a host name, a user or an environment variable.

Bytes that are not UTF-8, as an image is not, cannot be searched for a text and are kept
as they are (`Secrets::redact_bytes`).

### Where a record is, in a store

A run's record is the name `record` among the names of that run (the store's
`Publication::put_in`), as a diagnostic reference. It is not an output: nothing may take a
record as an input, and the store refuses a diagnostic where an output is asked for. Being
one of the run's names, it goes when the run is deleted and its content at the next prune,
and by nothing else, which is what the spec asks.

Beside it, under the names the record lists in `diagnostics`: the run's journal, at
`journal`, and what the action wrote, each at `diagnostics/<name>` (`RECORD_NAME`,
`JOURNAL_NAME`, `DIAGNOSTICS_PREFIX`). All of them are diagnostic references.

`record::stored` reads the record of a run by whatever id the store holds the run under.
That id is not a `RunId` (ADR 0004, "The run id"): a store can hold runs that other code
made, and such a run is read, or reported as having no record or none this build reads.
A record that names another run than the one it is stored in is refused. `RunId` is not
deserialized from a record: the record's `run` is a text.

### The journal

A journal is the run's progress events, each as a JSON object on a line of its own: `seq`,
its place from 0; `at_ms`, when it was reported; `message`, what was said, with the
secrets taken out. It is read strictly: every line is an event in its place, and the last
ends with a newline. An empty journal is no bytes.

Squint dropped a torn last line when it read a journal, because it appended to a file. A
journal here is published whole or not at all, so a torn line is damage and is not
forgiven. Squint's typed events (a turn sent, a tool called, a repair) are the judge
loop's, and are on `docs/carry-over.md` with it.

### A failed run

Its record has the same thirteen fields. `status` is `error`, `error` holds the type and
the message, and `outputs` is empty: a record with an output and a status other than
`completed` is refused on reading. Its diagnostics are listed as a completed run's are. A
run that found its input invalid is the same with `invalid` and its reason; a cancelled
run has neither.

### Running an action, and writing its run

The code is in `crates/whizbang-core/src/contract/` (`engine.rs`, `registry.rs`,
`context.rs`, `journal.rs`); the tests are in
`crates/whizbang-core/tests/it/contract/execute.rs` and `crates/whizbang/tests/it/runs.rs`.

**An action runs.** `Action::run` is handed the prepared call and a `Context`, and returns
when the action is over: `Produced`, or a `Failure`. A `Failure` is an error, with a type
and a message, or an invalid input, with the reason. `Produced` holds nothing and can only
be made as nothing: an action cannot hand back an artifact until there is a commit step to
publish it in, which is the plan's task for nested calls, cancellation and publication.
An action that does not define `run`, as a fixture that only describes itself, fails with
the type `not_runnable`.

**Two executors.** `Engine::new` is given a read of a store; it prepares calls and
refuses to run one (`Refusal::ReadOnly`), because it was given no way to write.
`Engine::over` is given a store opened to write, which makes its process the store's one
writer for as long as the executor lives; it reads that store and writes its runs to it.

**What `submit` does, in order.**

1. Prepares the call. A refusal, or a call that is not ready, is answered as ADR 0004
   says: there is no run and nothing is written.
2. Names the run and notes the time. From here there is a run.
3. Runs the action, with a context of its own.
4. Makes the outcome: completed with no outputs, error, or invalid. The secrets are taken
   out of an error's type and message and of an invalid run's reason here, so what is
   handed back holds none, as what is written holds none.
5. Publishes, in one publication of the store: the journal under `journal`, each
   diagnostic the action wrote under `diagnostics/<name>`, and the record under `record`,
   each as a diagnostic reference of the run.
6. Answers with the run's id and its outcome, after the publication is durable.

So a run is in the store whole or not at all, whatever its outcome, and a run that was
reported survives a loss of power. When the store fails in step 5, `submit` answers with
the store's failure (ADR 0004's `CallError::Store`), not with a finished run, and the
store holds nothing of the run; the one exception is the store's own (`Unacknowledged`):
the generation became visible and its last sync failed, and then the run is whole and was
not reported as finished.

**A run's id** is the time it started, in milliseconds since the Unix epoch, a hyphen, and
a count: `1794519513123-1`. The count is the executor's count of the runs it has named,
and goes up past any id the store already holds. One process writes a store at a time, and
it is this one, so an id that is free when it is made stays free until the run is
published. It is a `RunId` (ADR 0004) by construction. Runs that started at different
milliseconds sort, by id, in the order they started, while the time has as many digits;
within one millisecond the count is compared as text, so `-10` comes before `-9`.

**What an action is handed.** A cancellation signal that nothing sets; no deadline; an
event sink, which is the run's journal; an empty allowance; the store the executor reads;
no capability; an executor that refuses every call (`Refusal::NoNestedCalls`), because
calls do not nest yet; and a place for diagnostics.

**Diagnostics.** `Context::diagnostics` takes bytes under a name: a lower-case letter or a
digit, then lower-case letters, digits, `_`, `-` or `.`, 64 characters at most, written
once in a run. Text has the secrets taken out before it is kept; bytes that are not UTF-8
are kept as they are. A progress event's message has the secrets taken out when it is
reported. Both are held in memory until the run ends, and are then stored as step 5 says.

**The time** comes from a clock the executor is built with (`Clock`), so that a test can
fix it and the record of a run can be compared with bytes written by hand.

**The binary.** `whizbang runs list --store <dir>` lists each run with its status, its
action, when it started and how long it took; `whizbang runs show <run> --store <dir>`
shows one record for a person, every group, the empty ones said to be empty; `--json`
prints the record as it is stored, and `--diagnostic <name>` one of the run's diagnostics
as it is stored. Both read only. Neither holds a run's id to the rule of a `RunId`: a
run is looked up under whatever the store has it under, and everything read from a store
is printed escaped, except the bytes asked for with `--json` and `--diagnostic`. A run
with no record, or with one this build does not read, is listed with the reason. No
command of the binary runs an action yet; the tests run `echo` through the executor the
binary builds (`wiring::runner`).

**What is not built, and what follows from that.**

- There is no admission. A run exists from step 2, in memory; nothing durable says so
  until step 5. A process that dies while an action runs leaves nothing of the run in the
  store: no record, no journal, no diagnostic. Admission (the plan's next task) records a
  run durably when it is assigned.
- A journal's events are durable only when the run ends, and cannot be read while it
  executes.
- A panic inside an action is not caught: it unwinds through `submit`, and the run has no
  record.
- Nothing can cancel a run, and `submit` returns only when the run is over.
- Diagnostics are held in memory, without a bound, until the run ends.

### The origin does not name the run or the models

`docs/carry-over.md` asked whether an artifact's origin (ADR 0002) should name the run
that made the artifact and the models the run used, as a new version of the origin. It
does not; the run record alone holds them.

- An artifact's identity does not cover its origin, and a store holds one file for one
  identity. The same artifact made by two runs is one artifact with one stored origin,
  which could name one of the runs at most, and which one would depend on which was
  published first. The run record has no such limit: each run's record lists the outputs
  it committed, by identity, so "which runs made this artifact" is a query over records,
  and answers with all of them.
- The models are a property of a run's requests, with a device and whether the answer
  came from a cache. That is the `models` group of the record.
- A new version of the origin changes the bytes of every canonical file.

What the origin keeps is what ADR 0002 gave it: each step's action, version, arguments and
inputs. "Every artifact records what produced it" (the spec's invariant) holds by that.

## Consequences

- Whatever lists, shows, counts or exports runs reads one shape. An export of one JSON
  object per line (the spec's `runs export`) can write the stored bytes as they are.
- Eight groups will each bring a new version when they are first filled. Reading then has
  to choose: read both versions, or refuse the old one. Before a store is kept, refusing
  is free.
- The schema and the types can drift apart where no test compares them. The tests compare
  them on hand-written records, on records made from a prepared call and an outcome, on
  every list that must be empty, and on the records the executor writes.
- Redaction is only as good as the list of secrets the executor is given
  (`Engine::with_secrets`). There is no config yet and nothing reads a key, so the binary
  gives none.
- A record is read whole and written again to be compared, on every read.
- Reversing this costs every stored record. A change to the fields is a new version.

What this record does not decide, and the code does not do: admission; nested calls,
cancellation, and an action's outputs with their commit step; how a run that takes long
is scheduled and watched; the elements of the eight empty lists; how a request id and a parent are assigned; `runs stats` and `runs export`,
which are milestone 2's (`SPEC.md` section 16); labels for evaluation runs; whether a diagnostic body is cut short (squint cut a
long body and recorded its full digest; that is on `docs/carry-over.md` with the judge
loop).

## Alternatives

- **Define the elements of every group now,** from the spec's table (a model's backend,
  version, device and cache flag; a reservation's amounts). Turned down: nothing would
  write them, so nothing would test them, and the capability interfaces and the ledger
  would have to fit shapes chosen before they existed.
- **Leave out the groups nothing fills.** Turned down: the spec says nothing in the record
  is optional, and a reader should not have to tell "none" from "not recorded".
- **Hash the record as ADR 0001 hashes an identity,** in a binary encoding. Turned down:
  the record is read by people and by tools outside whizbang, and nothing is addressed by
  a record's identity.
- **Keep the record outside any run,** where no command removes it. Turned down: the spec
  says records are removed by the store's delete and prune commands. Request-id and spend
  records are the ones that outlive a run.
- **Store the record as an output.** Turned down: an output is what another action may
  take as an input.
- **A journal file appended to and synced event by event,** as squint's was. Turned down:
  the store has one way in, and a file written beside it would be a second owner of
  durable state, outside the writer lock and outside prune.
- **A new version of the origin that names the run and the models.** Turned down above.
- **A schema library in the build.** Turned down: one schema, a few keywords, and the
  check is wanted in tests only.
