# 0005. The run record: one fixed shape with every group, written one way

Status: accepted
Date: 2026-11-12
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

The code is in `crates/whizbang-core/src/contract/` (`record.rs`, `redact.rs`,
`journal.rs`), the schema in `crates/whizbang-core/schemas/run-1.json`, and the tests that
hold them to this record in `crates/whizbang-core/tests/it/contract/record.rs` and
`redact.rs`.

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

An error's message is otherwise kept as it was given. It can carry the sentence of
something the action called, a decoder's for one (ADR 0004, "No path in what is said"),
and that sentence is kept in the record, since it says what failed.

Redaction is by value. It takes out the secrets it was told of, and nothing else: it does
not find a key by its shape. What keeps other private things out of a record is the shape:
it has no field for a path, a host name, a user or an environment variable. A free text
is the one place the shape does not reach: an action that writes a path into its error's
message has written it into the record. A `CallError` shows no path,
as its text or as `{:?}`; the store's own error does, and an action that quotes one in a
message quotes it without (`store::Error::without_paths`). Nothing holds an action to
that (`docs/carry-over.md`).

Bytes that are not UTF-8, as an image is not, cannot be searched for a text and are kept
as they are (`Secrets::redact_bytes`).

### Where a record is, in a store

A run's record is the name `record` among the names of that run (the store's
`Publication::put_in`), as a diagnostic reference. It is not an output: nothing may take a
record as an input, and the store refuses a diagnostic where an output is asked for. Being
one of the run's names, it goes when the run is deleted and its content at the next prune,
and by nothing else, which is what the spec asks.

Beside it, under the names the record lists in `diagnostics`: the run's journal, at
`journal`, and what the action wrote, each at `diagnostics/<name>`. These names are fixed
here (`RECORD_NAME`, `JOURNAL_NAME`, `DIAGNOSTICS_PREFIX`); nothing writes under them yet.

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
  them on hand-written records, on records made from a prepared call and an outcome, and
  on every list that must be empty.
- Redaction is only as good as the list of secrets it is given. There is no config yet
  and nothing reads a key, so nothing has a secret to give.
- A record is read whole and written again to be compared, on every read.
- Reversing this costs every stored record. A change to the fields is a new version.

What this record does not decide, and the code does not do: running an action and
writing its record through the store, which is the rest of the plan's task for the
execution contract, and with which this record is amended; the elements of the eight
empty lists; how a request id and a parent are assigned; `runs stats` and `runs export`,
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
