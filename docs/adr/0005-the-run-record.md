# 0005. The run record: one fixed shape with every group, written one way

Status: accepted
Date: 2026-11-12
Amended: 2026-11-12, inside checkpoint 1A, with the code that runs an action and writes
its run. Added: "Running an action, and writing its run". Changed: the sentences that said
nothing wrote a record, a journal or a diagnostic yet, and the list of what this record
does not decide.
Amended: 2026-11-12, a second note, after the second review of the record. Added: a
secret is trimmed of the white space at its ends, and of nothing else; the forms of
percent-encoding that are not found; what the pass costs, with a figure; that a line and
a diagnostic cannot be read from JSON except by the two readers; that what a refusal
quotes is cut at 64 characters. Corrected: a secret that ends in a quote had a wrong JSON
form; "one case can break" a JSON body is two; and the consequence that said each of the
eight groups brings a new version, which did not agree with the shared step to `run/2`.
Amended: 2026-11-12, a third note, after the review of running an action. Corrected: a
run's id was said to stay free until its run was published because one process writes a
store; two executors in one process could give two runs one id, and the store merged
them. The count is now the process's, the store refuses to make a run it already holds,
and the executor names the run again. Added: a secret is refused as a diagnostic's name;
a reader's reason is cut at 200 characters, a quoted name at 64; that a refusal inside a
run is not recorded yet; what `--diagnostic` prints; that a record does not give back the
fingerprint of a call whose argument held a secret; a closed pipe is not an error.
Amended: 2026-11-12, a fourth note, with ADR 0007, which admits a run. Changed: a run is
in the store from its admission, in a publication of its own, and its end is a second
publication that adds to it; so "nothing durable says so until step 5" and "a run is in
the store whole or not at all" no longer hold as they were written, and "What `submit`
does" and "What is not built" are written again. `identity.request` is filled, and a
record whose `request` is no request id is refused. `runs list|show` say when a run was
admitted and has not ended.
Amended: 2026-11-12, a fifth note, with the last part of ADR 0007, which reserves. The
record is `run/2`, the shared step this record allowed: `spend` is an object that says
what was reserved for the run, and holds the list of paid requests, which is the empty
one now. Seven lists are empty where eight were. The schema is `run-2.json`. A `run/1`
record is another version, and is refused.
Amended: 2026-11-12, a fifth note, with the second part of ADR 0007. Changed: the end of
a run is published with `Publication::close_run`.
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
`redact.rs`, `journal.rs`), the schema in `crates/whizbang-core/schemas/run-2.json`, and
the tests that hold them to this record in
`crates/whizbang-core/tests/it/contract/record.rs` and `redact.rs`.

### The shape, `run/2`

A record is one JSON object with thirteen fields, always all of them, in this order:

| Field | Group of the spec | What it holds |
|---|---|---|
| `record` | | The kind and version: `run/2` |
| `identity` | Identity | `run`, the run's id; `request`, the caller's request id, or null; `parent`, the run that called this one, or null; `whizbang`, the version that ran it |
| `asked` | What was asked | `action` and `version`; `arguments`, each as its `name` and its `value`; `settings`; `config`, the digest of the config the settings came from |
| `inputs` | Inputs and outputs | Each input as its `port`, its `type`, the identity of the `artifact` and the identity of the `binding` the artifact belongs to |
| `outputs` | Inputs and outputs | Each committed output as its `port`, its `type` and the identity of the `artifact` |
| `outcome` | Outcome | `status`: `completed`, `cancelled`, `error` or `invalid`; `error`, its `kind` and `message`, or null; `invalid`, the reason, or null; `refusals` |
| `time` | Time | `start_ms` and `end_ms`, in milliseconds since the Unix epoch; `durations` |
| `models` | Models | |
| `spend` | Spend | `reserved_micros`, what was reserved for the run when it was admitted, in millionths of a US dollar, or null when nothing was; `requests` |
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

### What is empty in `run/2`, and how it stops being empty

Seven lists hold nothing in this version, and cannot: `asked.settings`, `outcome.refusals`,
`time.durations`, `models`, `egress`, `steps` and `judge`; and an eighth inside `spend`,
`spend.requests`. Each is there, as an empty list, in every record. In the code the
element of each is a type that has no value; in the schema each has at most zero items. A
record with anything in one of them is not a `run/2` record.

**What `run/2` filled, so far.** `spend` was an empty list in `run/1`. It is an object:
`reserved_micros`, the amount of the reservation the run was admitted with (ADR 0007,
"The reservation"), or null when the executor that admitted it had no ledger and nothing
was reserved; and `requests`, the list the spec asks for, of every paid request with what
was reserved for it, what was settled and what is unresolved. No request is dispatched or
settled before the ledger of checkpoint 1B, so that list is the empty one. The amount is
of the run and not of a request, which is why it is not an element of the list. It is
written when the run ends, from what admission reserved; a run that never ends has no
record, and its admission and its reservation say the same amount. The largest amount
there is is no amount (ADR 0007): the types refuse it, and the schema, which has no
maximum in the vocabulary the validator knows, does not.

**What reading does with a `run/1` record.** It is refused as another version, like any
other. A store is not kept before the tag of checkpoint 1A, so nothing is migrated
(`record::a_record_of_another_version_is_another_version_not_damage`).

The shape of what goes in them is not decided here, because it follows from things that
do not exist: the request and response of a capability, the ledger's reservation, a
recipient, a recipe step, a judge's turn. Whatever first fills one of these lists defines
its elements and writes a new version of the record, with its own decision record or an
amendment of this one.

**When a store starts being kept, and what a version costs before and after.** A store is
kept from the tag that closes checkpoint 1A. Before that tag a store written by earlier
code of the checkpoint is discarded, as ADR 0002 says of masks, and the fills of the empty
lists that the checkpoint's own tasks make (request ids and admission; nested calls,
cancellation and publication; the capability interfaces; the two proofs) may share one
step to `run/2`, each adding to it as it lands. `run/2` is frozen at that tag. After it,
every fill is a new version, and whatever reads records goes on reading the frozen
versions.

`identity.parent` is null in every record written today, because no call nests.
`identity.request` is the caller's id for the request that made the run, or null when it
gave none (ADR 0007). They are strings when they are filled, and filling them does not
change the version: a reader of `run/1` already accepted both. A `request` that is there
is a request id, 1 to 128 characters of its alphabet: the types refuse any other text,
and the schema holds it to the two lengths.

### Written one way, and read strictly

A record is written as JSON on one line: no space between tokens, the fields of every
object in the order this record gives them, strings escaped as `serde_json` escapes them,
a number in the shortest form that reads back to the same double, with a point or an
exponent, a whole number without one. One newline follows.

Reading refuses, in this order:

- more than 4 MiB (4,194,304 bytes, `RECORD_MAX_BYTES`): a record is read whole and
  written again to be compared, so what is read is bounded first. From a store, the
  entry's length is looked at and the content is not read at all;
- a record of another version, as another version: the `record` field is looked at first
  and by itself, and a store written by other code is not migrated. What a record says
  its version is can be anything, and the refusal keeps its first 64 characters;
- anything that is not of the shape: a field that is not known; a field left out,
  `request`, `parent`, `error` and `invalid` among them, which may be null and may not be
  absent; a digest, a type or a status that is none; a value under two kinds or under
  none; a number under zero where a time or a length goes;
- a `request` that is a text and is no request id (ADR 0007);
- a record of the right shape that says what no run can: an empty run id, version, action
  name, argument name or port; an `error` without the status `error` or the status
  without it; the same for `invalid`; an output of a run that did not complete; an end
  before the start; a diagnostic whose name is neither `journal` nor `diagnostics/` and a
  plain name (a lower-case letter or a digit, then lower-case letters, digits, `_`, `-`
  or `.`, 64 characters at most), so that `record` itself is not one; diagnostics out of
  the order of their names, or one name twice;
- bytes that are not the bytes this code writes for the record they hold.

A refusal can quote what was read: an unknown field's name, a version. What it quotes is
shown escaped and cut, so nothing stored reaches a terminal as a control character
through a refusal, and a refusal is never as long as what it refuses. A name that is kept
(a version, the id of another run that a record names) is cut at 64 characters
(`QUOTED_LIMIT`). A reader's reason, which holds the name and then where in the bytes it
was met, is cut at 200 (`REASON_LIMIT`), with three dots when there was more. A failure
of the store under a record is said as ADR 0004 says a store's failure:
no path, as text or as `{:?}`, and the store's own error by name.

The last rule is ADR 0002's for an artifact file, and for the same reason: one record has
one form, so two stores that hold the same record hold the same bytes, and the digest the
store names it by means the record.

The record has no identity of its own, and is not hashed into anything. Rascal's operation
record had one, over everything but the duration and a sequence number. A run is named by
its id, and what is stored is named by the digest of its bytes; nothing needs a third
name.

### The schema

`crates/whizbang-core/schemas/run-2.json` is the JSON Schema (2020-12) of a `run/2`
record, written by hand, and the core carries it as text (`RUN_RECORD_SCHEMA`). It and the
types are two statements of one shape, and the tests hold them together in three ways:

- Records written by hand, and records made from a prepared call and an outcome, are read
  by the types and validated against the schema.
- For every kind of thing the shape refuses (an unknown field at the top and in each
  group, each field left out, a digest of the wrong length, a type or status that is
  none, a value under two kinds or none, a number under zero, a name that is empty, each
  list that must be empty holding something), one document is made with that one fault,
  and both the types and the schema must refuse it
  (`record::what_is_not_a_record_is_refused_by_the_types_and_by_the_schema` and the two
  tests beside it). A schema that is loosened in one of these places fails a test.
- What the schema cannot say is listed, and pinned as a document the schema takes and
  the types refuse (`record::what_the_schema_cannot_say_the_types_still_refuse`).

What the schema cannot say, with the vocabulary it uses (no pattern, no rule across two
fields):

- that a digest's 64 characters are hex digits: it holds a digest to its length only;
- the rules across fields: `error` exactly with the status `error`, `invalid` exactly
  with `invalid`, outputs only of a completed run, the end not before the start;
- what a diagnostic's name may be, the order of the diagnostics, and that no name is
  there twice;
- the alphabet of a request id: it holds a `request` to its two lengths only;
- that what was reserved is not the largest amount there is;
- the one way of writing, and the limit on a record's bytes: both are of the bytes, not
  of the document.

The validator is the test kit's (`whizbang_testkit::schema`): the keywords this schema
uses and no others. It reads a schema whole before it holds a document to it, and
refuses a keyword it does not know, a keyword whose value is not of the kind the keyword
takes, and a reference that leads back to itself, wherever in the schema they are; so a
schema never passes for holding less than it says. No schema library is added to the
build.

### Redaction

`Secrets` holds the values that must never be written, and does not print them. The pass
began as squint's, which replaced a secret by `[redacted]` where it stood as it was and
where it stood as a JSON string escapes it. Here:

- **It works on bytes, whatever their encoding.** A body with one byte that is not UTF-8,
  as a worker's error output cut in the middle of a character is, is searched like any
  other (`Secrets::redact_bytes`); a text is redacted as its bytes.
- **Four forms of each secret are searched for:** as it is; as a JSON string escapes it;
  the same with each `/` written `\/`; and percent-encoded, as a URL holds it, with the
  hex digits in either case.
- **Everything is found before anything is replaced.** Every form of every secret is
  looked for over the bytes as they were given; stretches that overlap are one stretch;
  each stretch is replaced once. So a secret that holds another is replaced whole, two
  secrets that overlap leave no tail of either, a secret that is part of the marker
  cannot rewrite the marker, and a secret that ends in a backslash does not leave half
  of its own JSON escape behind.
- **It is one pass.** At each byte, only the forms that start with that byte are
  compared. The cost is the length of the body times the number of forms that share a
  first byte, not times the number of secrets; there is no bound on how many secrets
  there are. That is fine at the counts a config holds, a handful of keys, and it is not
  free at any count: the review of this pass measured about 3 seconds for 1 MiB of JSON
  against 2,000 keys that share a three-byte prefix, where every key's forms are tried
  wherever that first byte stands.
- **A secret is at least 8 bytes** (`SECRET_MIN_BYTES`). A shorter value would match
  inside ordinary words and inside JSON's own escapes: a secret `n` would turn `\n` into
  `\[redacted]`. A shorter value is refused when the secrets are built
  (`SecretRefused`), and then none are built: whoever meant to protect it is told that it
  is not protected. One value is left out without a refusal, the empty one: it is what a
  variable that was never set expands to, and refusing it would make every key that is
  not configured an error.
- **A value is trimmed first, of ASCII white space at its two ends and of nothing
  else.** A value read from a file or a variable often carries a line break, and the key
  with its line break is in no body. A value that is empty once trimmed is left out as
  an empty one is, and the 8 bytes are counted on the trimmed value. Quotes around a
  value are not taken off: whether they are part of the key is known to whoever reads
  the file the value came from, which is the config's business (checkpoint 1B). The
  refusal says how long the value was and never shows it; naming the variable it came
  from is the config's too.

A redacted body is searched as bytes, not as JSON. A body that was JSON is still JSON
after it wherever a secret stood whole in one of its forms. Two cases can break it, both
where a secret is found as it stands across the edge of an escape that is not its own: a
secret that ends in a backslash, in front of the rest of an escape, as `abcdefgh\` in
`abcdefgh\"`, where the string then ends early; and a secret that begins with a letter
an escape can end in, behind a backslash, as `nabcdefgh` in `x\nabcdefgh`, which leaves
the backslash in front of the marker. A test pins the second.

#### What redaction does not find

Redaction is by value and by form. It finds the secrets it was told of, in the four forms
above, and nothing else:

- a key it was not told of: it does not know a key by its shape;
- a secret inside base64, as in a basic authorization header (`user:<key>`, encoded);
- a secret with a character written as a `\u00XX` escape that a JSON writer need not
  have used;
- a secret in UTF-16 or any encoding in which its bytes are not its UTF-8 bytes;
- percent-encoding that is not the one form searched for, in which every byte that is
  not a letter, a digit, `-`, `.`, `_` or `~` is encoded and the hex digits are all of one
  case: the two cases mixed in one value; a reserved byte left plain beside encoded ones;
  a space written `+`; a byte encoded that need not be; a value encoded twice;
- a secret split across two texts, or across two lines of a journal;
- a secret compressed, hashed, or inside an image.

A test pins base64, the `\u00XX` escape, UTF-16 and the five forms of percent-encoding
as getting through, so that this list stays true. They are on `docs/carry-over.md` for checkpoint 1B, where a config first holds a
key and a worker first returns a body.

**By construction, where it is cheap.** A line of a journal is made by a constructor that
takes the secrets (`JournalLine::new`), and a diagnostic by one of two
(`Diagnostic::journal`, from such lines, and `Diagnostic::written`, which takes the
content and the secrets, checks the name, and gives back the bytes to store with the
digest of those bytes). Their fields are not public. Beside these, a line and a diagnostic
come only from reading what was stored, which reading checks: neither type, nor the
record, can be read from JSON by other code. The readers (`read_journal`,
`RunRecord::from_bytes`) use private types of their own for what is stored. That holds by
the absence of an implementation, which no test at run time can show; adding one back
would be a change to this record. So a record cannot list a
diagnostic, and a journal cannot hold a line, that did not pass through redaction; what
is not held by construction is that the bytes then stored are the bytes that were given
back.

In a record, redaction is applied to each text that comes from outside the build, before
the record is encoded: an argument's value when it is a text, an error's kind and
message, and the reason a run was invalid. It is not applied to the encoded JSON, so it
cannot break the record's shape. Names that come from a descriptor, identities, numbers,
booleans and times are not texts from outside and are written as they are.

So "its exact arguments" (the spec) holds except where an argument held a secret: there
the record holds `[redacted]`, because the spec also says a secret is never written. One
thing follows: the fingerprint of the prepared call (ADR 0004) cannot be worked out again
from such a record, since the fingerprint hashed the argument as it was given; and the
record does not store the fingerprint.

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

### Where a record is, in a store

A run's record is the name `record` among the names of that run (the store's
`Publication::put_in`), as a diagnostic reference. The run's admission, `admitted`, is
another of its names and is ADR 0007's; it is not one of the diagnostics a record lists. It is not an output: nothing may take a
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

**What `submit` does, in order.** `Executor::request`, which takes a call that may carry
a request id, first looks the id up (ADR 0007), and then does the same.

1. Prepares the call. A refusal, or a call that is not ready, is answered as ADR 0004
   says: there is no run and nothing is written.
2. Notes the time, names the run, and admits it: one publication of the store makes the
   run (`Publication::create_run`) with its admission, and the record of the request's
   id when it has one (ADR 0007, "Admission"). The store refuses a publication that
   makes a run it already holds, looking inside its publishing step; the executor then
   names the run again and publishes again, up to eight namings, after which the refusal
   is handed back as the store's failure. From here there is a run, durably.
3. Runs the action, with a context of its own.
4. Makes the outcome: completed with no outputs, error, or invalid. The secrets are taken
   out of an error's type and message and of an invalid run's reason here, so what is
   handed back holds none, as what is written holds none.
5. Publishes the end, in one publication that adds to the run and says the run must be
   there (`Publication::close_run`): the journal under `journal`, each diagnostic the
   action wrote under `diagnostics/<name>`, and the record under `record`, each as a
   diagnostic reference of the run.
6. Answers with the run's id and its outcome, after that publication is durable.

So a run is in the store in one of two states, each whole: admitted, with its admission
alone, or ended, with everything; and a run that was reported survives a loss of power.
When the store fails in step 2 before the admission is visible, there is no run. When it
fails in step 5, `submit` answers with the store's failure (ADR 0004's
`CallError::Store`), not with a finished run, and the run stays as it was admitted. The
store's own exception (`Unacknowledged`) applies to each publication: it became visible
and its last sync failed, and then the run is admitted, or ended, and was not reported.

**A run's id** is the time it started, in milliseconds since the Unix epoch, a hyphen, and
a count: `1794519513123-1`. The count is the process's, one for every executor in it, so
two executors over one store never make the same id from the same time; and it goes up
past any id the store holds when the run is named. It is a `RunId` (ADR 0004) by
construction. Runs that started at different milliseconds sort, by id, in the order they
started, while the time has as many digits; within one millisecond the count is compared
as text, so `-10` comes before `-9`.

**One id, one run, held by the store.** That an id is free when a run is named does not
make it free when the run is admitted: anything that writes the store can publish a run
between the two. So it is not left to the naming. Adding names to a run
(`Publication::put_in`) and making a run (`Publication::create_run`) are two operations
of the store; a publication that makes a run is refused with `RunExists`, and publishes
nothing, when the generation it would be published onto holds that run. Two runs are
never merged under one id, whoever named them. And a run that was deleted while it
executed is not made again by its end: that publication is refused with `NoSuchRun`.

**What an action is handed.** A cancellation signal that nothing sets; no deadline; an
event sink, which is the run's journal; an empty allowance; the store the executor reads;
no capability; an executor that refuses every call (`Refusal::NoNestedCalls`), because
calls do not nest yet; and a place for diagnostics.

**Diagnostics.** `Context::diagnostics` takes bytes under a name: a lower-case letter or a
digit, then lower-case letters, digits, `_`, `-` or `.`, 64 characters at most, written
once in a run. The content has the secrets taken out before it is kept, whatever its
encoding. The name is a key in the store, written as it is in the generation file and in
the record: a name that redaction would change, because it is or holds a secret, is
refused, and the refusal does not show it. No other name of a run comes from outside the
build: a port and an argument are named by the action's descriptor, a call that names
any other is refused before a run exists, and the run's id is made by the executor. A progress event's message has the secrets taken out when it is
reported. Both are held in memory until the run ends, and are then stored as step 5 says.

**The time** comes from a clock the executor is built with (`Clock`), so that a test can
fix it and the record of a run can be compared with bytes written by hand.

**The binary.** `whizbang runs list --store <dir>` lists each run with its status, its
action, when it started and how long it took; `whizbang runs show <run> --store <dir>`
shows one record for a person, every group, the empty ones said to be empty; `--json`
prints the record as it is stored, and `--diagnostic <name>` what the run holds under one
name, which is any diagnostic name of the run and also `record`, as the bytes that are
stored: they are not escaped, and the help says so. A reader that stops reading, as
`head` does, is not an error: the command says nothing and exits 0. Both read only. Neither holds a run's id to the rule of a `RunId`: a
run is looked up under whatever the store has it under, and everything read from a store
is printed escaped, except the bytes asked for with `--json` and `--diagnostic`. A run
that was admitted and has not ended is listed and shown as such, from its admission (ADR
0007); one with no record and no admission, or with a record this build does not read,
is listed with the reason. No
command of the binary runs an action yet; the tests run `echo` through the executor the
binary builds (`wiring::runner`).

**What is not built, and what follows from that.**

- Admission is built (ADR 0007): a run is recorded durably when it is assigned, and
  a process that dies while an action runs leaves the run, admitted. It leaves no
  journal and no diagnostic: those are durable only when the run ends, and cannot be
  read while it executes.
- A panic inside an action is not caught: it unwinds through `submit`, and the run stays
  admitted, with no record.
- Nothing can cancel a run, and `submit` returns only when the run is over.
- A refusal inside a run is not recorded. There is one today: a call from inside an
  action is refused (`NoNestedCalls`). The record's `refusals` list has no elements in
  `run/1`, so the action's own error is the only trace of it, if the action reports it.
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
- Filling the empty lists changes the version: once, to `run/2`, for all that the
  tasks of checkpoint 1A fill, and once for each fill after the store is kept ("When a
  store starts being kept", above). Reading then has to choose: read both versions, or
  refuse the old one. Before a store is kept, refusing is free.
- The schema and the types can drift apart where no test compares them. The tests compare
  them on hand-written records, on records made from a prepared call and an outcome, on
  every list that must be empty, and on the records the executor writes.
- Redaction is only as good as the list of secrets the executor is given
  (`Engine::with_secrets`). There is no config yet and nothing reads a key, so the binary
  gives none.
- A record is read whole and written again to be compared, on every read.
- Reversing this costs every stored record. A change to the fields is a new version.

What this record does not decide, and the code does not do: admission, which is ADR
0007's; nested calls,
cancellation, and an action's outputs with their commit step; how a run that takes long
is scheduled and watched; the elements of the empty lists; how a parent is assigned; `runs stats` and `runs export`,
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
