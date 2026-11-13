# 0007. Request ids and admission: a run exists from the step that records its request

Status: accepted
Date: 2026-11-12
Amended: 2026-11-12, with the second pull request of task A6. Added: "The estimate" and
"A preparation handed to admission", which are built; the paragraph of "Admission" that
listed the second part now lists only what is still to come, the ledger and the
reservation. The task is delivered in three pull requests, not two.
Spec: section 3, "The execution contract", "Storage" and "What every run records";
section 6, "Spend"

## Context

`SPEC.md` section 3 says a caller may give a request id; that ids are scoped to the store
and bound to a fingerprint of the submitted request; that admission records the id, the
fingerprint, the run and the reservation together, durably; that a repeated id is looked
up before any readiness or preparation check, the same request returning the existing run
and a different request under the same id being a conflict; and that the record outlives
a restart and the pruning of its run, so that a retry after a dropped connection never
repeats paid work. "Storage" says request-id records are never deleted by the store's
delete and prune commands.

What there was to build it on:

- The executor of ADR 0005: `submit` prepared a call, ran its action, and published the
  run whole, in one publication, when it was over. Nothing durable existed while a run
  executed, and a process that died left nothing of it.
- The store: names outside runs, which no command removes; a publication that makes a
  run and is refused when the run exists (`Publication::create_run`); and the row of
  `docs/carry-over.md` that said "never removed" held only by how the store is built,
  because `Publication::put` replaces what a name had.
- The fingerprint of a prepared call (`prepared/1`, ADR 0004), which holds the config's
  digest and the models, and so cannot be the fingerprint an id is bound to: the spec
  wants the same request answered with its run after the config changed, without
  preparing anything.

Task A6 of the plan for checkpoint 1A is delivered in parts. This record decides
all of them, and says of each sentence below which part builds it. **The first part, which this
record came with, builds:** the id, the fingerprint of a request, the two records, the
look before prepare, the admission that makes the run and the id's record in one step,
the end of a run as a second step, and what a repeat is answered with. **The second part
builds:** the estimate, and a prepared call handed to admission and checked again.
**The third part builds:** the ledger and the reservation. Until it merges, "Admission"
below is true without its paragraph on what is still to come, and the lines that
paragraph names are on `docs/carry-over.md`.

The code is in `crates/whizbang-core/src/contract/` (`request.rs`, `engine.rs`) and
`crates/whizbang-core/src/store/mod.rs`; the tests that hold it to this record are in
`crates/whizbang-core/tests/it/contract/admit.rs`, `store/runs.rs`, and
`crates/whizbang/tests/it/runs.rs` and `store.rs`.

## Decision

### The id

A `RequestId` is text the caller chose: 1 to 128 of the ASCII letters, the digits, `_`,
`-`, `.` and `:`. Letter case counts. A UUID, a ULID and a hash in hex are ids.

It is untrusted, and it is written as it is in three places: a name in the store, the
two records below, and the run record's `identity.request`. So:

- it is held to that alphabet where it is made (`RequestId::parse`), and nothing in it
  needs escaping in a name, on a command line or in JSON. A refusal shows what was
  refused escaped, its first 64 characters at most, and its length when it was longer;
- an id that holds a secret the executor was told of (one that redaction would change)
  is refused before anything is looked up (`Refusal::SecretInRequestId`), and the refusal
  does not show it. Redaction is by value (ADR 0005): an id that holds a key the
  executor was not told of is stored;
- a run record whose `request` is a text that is no request id is refused on reading,
  and the record's schema holds it to the two lengths (the alphabet is one more thing
  the schema cannot say).

Ids are scoped to the store: the same id in two stores is two ids.

### The fingerprint of a request, and what "the same request" means

Kind `request/1`, in the encoding ADR 0001 gives every identity, with the tags of ADR
0004 (`b`, `u`, `f`, `d`). The fields, in order:

1. the action's name (`b`);
2. the count of the arguments the call gives (`u`); then each, in the order of their
   names as bytes, as its name (`b`) and its value, written as `prepared/1` writes a
   value: its kind (`u`), then itself, negative zero as zero;
3. the count of the ports the call gives artifacts at (`u`); then each, in the order of
   their names as bytes, as its name (`b`), the count of its artifacts (`u`), and each
   artifact's identity (`d`) in the order the call gave them.

It is of the call alone. No descriptor, store, config or model is read to make it, so it
exists before anything is prepared and does not change when any of those does. Two
fixed vectors pin it; their digests were computed by a script written from this text,
apart from the code.

So **the same request is the same call as it was given**, and not "a call that would be
prepared the same":

- the identical call under the same id after the config changed, a model changed, an
  input was deleted or the action was unregistered is the same request, and is answered
  with the run it made;
- a default given by hand and the same default left out are one preparation (ADR 0004)
  and two requests: the second under the first's id is a conflict. So are a backend
  named and the same backend left to the default, which ADR 0006 already counts as two
  preparations of one model. A test pins the first
  (`admit::known_a_default_given_by_hand_is_another_request_than_the_default_left_out`);
- the action's version is not in it: the same call after an upgrade is the same request.

The fingerprint hashes a text argument as it was given. A record does not give back a
secret that an argument held, and a fingerprint is not a way to read one; it is a way to
confirm a guess of the whole call.

### Two records, and where they are

Both are JSON on one line with a newline, fields in the order given, and both are read
as ADR 0005 reads a run record: at most 1,024 bytes (`REQUEST_RECORD_MAX_BYTES`), looked
at in the store's entry before the content is read; the kind first and by itself; an
unknown field, a field left out and a field twice refused; then what the shape cannot
hold; then the bytes compared with what this code writes for what they hold. Neither can
be read from JSON by other code, and neither can be made except by the executor.

**The admission, `admission/1`:** `record`; `run`, a run id; `request`, a request id or
null; `fingerprint`, of the request; `prepared`, the fingerprint of the call as it was
prepared when it was admitted; `action` and `version`, held to the rules of ADR 0003;
`start_ms`. Every run has one, among its own names, as `admitted`
(`ADMITTED_NAME`). A run that was given an id has the same bytes a second time, under
the name `requests/<id>/admitted`, which belongs to no run.

**The ending, `ending/1`:** `record`; `run`; `request`; `status`, one of the four words
of a run record; `end_ms`. Only a run that was given an id has one, under
`requests/<id>/ended`, outside runs. It holds the word and no more: an error's message
and a reason are free text, and go when the run is deleted.

All three names are diagnostic references: readable, and refused where an output is
asked for. An id has no `/`, so the names of two ids never meet, and neither is under
`artifacts/`.

No schema is published for the two. They are the executor's own, read only by this
code; the run record is what other tools read.

**Why they survive delete and prune.** `store delete` removes the names of runs and no
others, and `store prune` removes content that no name is left for. The two names of an
id belong to no run. A test puts records in a store, deletes every run and prunes with
the real commands, and reads every record back
(`store::the_records_of_request_ids_are_left_by_delete_and_prune_and_still_answer`).

**Why they are not given other content.** The store has a new way to publish a name
outside runs: `Publication::create`, which makes the name. A publication that makes a
name the store already holds is refused, whole, with `Error::NameExists`; the store
looks inside its publishing step, at the generation that is current then, as it does for
`create_run`. Both records of an id are made, never put. What is not covered:
`Publication::put` still replaces any name, a made one too, and nothing in the store
stops other code from putting a name under `requests/`; no code of whizbang does. A test
pins that (`store::runs::known_a_name_that_was_made_can_still_be_replaced_by_put`).

### The look, before prepare

`Executor::request` takes a `Request`: a call and, if the caller gives one, an id.
Without an id it is `Executor::submit`. With one, in order:

1. An id that holds a secret is refused.
2. The id is looked up in the current generation, by its name. Nothing else has
   happened: the action has not been found, no input read, no backend asked.
3. Found, with the same fingerprint: the answer is `Answer::Repeat`, the run the first
   request made and where that run is now (below). Nothing is prepared, nothing runs,
   nothing is written.
4. Found, with another fingerprint: `Refusal::Conflict`, with the id and the run the
   first request made. Neither fingerprint is shown.
5. Not found: the call is prepared and admitted as below.

A test's backends panic when a call is prepared, and its registry is empty; the repeat
is answered all the same
(`admit::the_same_request_again_after_the_config_changed_returns_the_first_run_and_prepares_nothing`).
An executor built over a read of a store looks up too, and answers a repeat; a new id it
refuses with `ReadOnly` after preparing, as it refuses any call.

**What is read is not trusted.** A record found under an id's name is bounded, read
strictly, and held to name that id; an ending, to name that id and that run; a run's
record, as ADR 0005 reads it. One that fails is `CallError::Record`, which says which
record and why, with what it quotes escaped and cut, and no path. It is not a refusal of
the call and not a failure of the store's files, and nothing runs: the id is neither
free nor answered until the record is put right.

### Admission

A call that was prepared and is ready is admitted by **one publication**, which:

- makes the run (`create_run`), with its admission under `admitted`;
- for a request with an id, makes the name `requests/<id>/admitted` with the same bytes.

The store decides both in its publishing step. A run id that was taken meanwhile is
refused with `RunExists`, and the run is named again, eight namings at most (ADR 0005).
An id that was admitted meanwhile is refused with `NameExists`: nothing of this request
was published, its action does not run, and the request is answered by looking the id
up again, as a repeat or a conflict. So **one id makes one run**, whatever two executors
over one store saw before they published; a test holds two requests at a barrier, after
each has looked and prepared, and gets one run
(`admit::two_simultaneous_requests_with_one_id_produce_one_run`).

A crash anywhere in that publication leaves the run and the id's record together or
neither: a publication is one replaced pointer (the store's own guarantee), and a test
loses power before each of its steps, under four of the ways a disk can lose what was
not yet durable (two fixed choices of which names survive among them, not every choice)
(`admit::a_crash_inside_the_admission_step_leaves_no_record_or_all_of_it`).

The action runs only after the publication is durable. A refusal, and a call that is
not ready, happen before it and record no id: the id stays free for the request once it
can run.

**The third part of the task adds to this step,** and until it merges none of this
paragraph is built. The reservation, of the estimate below, is a record the ledger
writes into the same publication, under a name
outside runs that is made once; the ledger of this checkpoint is a fake that takes part
in the step and counts, and the real one is checkpoint 1B's. The admission and the run
record then name what was reserved, which is the shared step to `run/2` (ADR 0005).
What happens to a reservation when its run fails, or when the process dies between
admission and the end, is settlement, and is checkpoint 1B's (`SPEC.md` section 6:
after a crash, what was dispatched and not settled is charged at its bound).

### The estimate

Prepare returns, with readiness, an `Estimate`: the most the call may cost, in
millionths of a US dollar. It is the sum of what the backends state as the bound of each
capability the call needs and something answers (`Backends::bound`, asked with the
capability and the model that would answer); a need that nothing answers adds nothing,
and a sum past the largest amount is the largest amount. It is answered from what is
configured, like resolution: nothing is sent and nothing is written.

No backend has a price before the ledger of checkpoint 1B, so `Backends::bound` answers
nothing unless an implementation says otherwise, and every estimate the binary can make
today is zero. A test's backends state a bound. Which of a backend's numbers is an
enforceable bound, and the refusal of a paid request that has none (`SPEC.md` section
6), are checkpoint 1B's.

The estimate is not in the fingerprint `prepared/1`, which stays as ADR 0004 gives it:
its two fixed vectors are as they were. Admission compares the estimate beside it.

### A preparation handed to admission

A `Request` may come with the preparation its caller was shown
(`Request::expecting`). It is the `PreparedCall` itself, not its fingerprint alone, so
that a refusal can say what differs. After the look for the id, the call is prepared
again, under the config and the backends as they are then, and the request is refused
with `Refusal::Stale`, before any run exists and with nothing published, when the two
differ. `Stale` says how, as the first of:

- `Call`: the preparation is of another action, version or arguments;
- `Inputs`: of other inputs, or the same in another order;
- `Config`: the config's digest changed;
- `Model`, with the capability: another model would answer it, or none would;
- `Estimate`: the same models under the same config state another bound;
- `Refused`, with the refusal: the call no longer prepares at all, as when an input is
  not in the store it is admitted against.

Each of the input, the config and the model has a test of its own, which also finds the
store as it was, no action run, and the request's id still free
(`admit::a_preparation_is_refused_at_admission_when_the_input_changed` and the three
beside it).

What follows from the rest of the design, and is stated so that it is not taken for a
gap:

- **An input cannot change under a preparation.** It is named by the digest of its
  content (ADR 0004). What can happen is that the caller's preparation is of other
  inputs, or that the input is no longer readable. Missing from the store, it is
  `Stale::Refused`. With its content damaged or gone from the disk under a name the
  store still holds, preparing again fails as the store's failure (`CallError::Store`),
  which is not a judgement of the preparation; no run is made then either.
- **Staleness is said before readiness.** A call whose model was taken away since is
  refused as stale, not as not ready. A preparation that was not ready and still is not
  is refused as not ready.
- **The look comes first.** The same request again under its id is answered with its
  run even when the preparation it comes with has gone stale
  (`admit::a_repeat_is_answered_before_its_preparation_is_looked_at`).
- **A request without a preparation is still taken.** `Executor::submit`, and a
  `Request` that expects nothing, are prepared once, at admission, and run if ready.
  Nothing can be stale where nothing was shown. Whether a door must always hand over a
  preparation is the door's decision.
- **The config is read when a call is prepared,** each time, through `ConfigSource`,
  and so at admission (`Engine::with_config`). An executor built with a digest alone
  has a config that never changes, which is what the binary has until there is a config.
- **The window that is left.** Between the second preparation and the admission's
  publication nothing holds the config or the backends still. What is recorded with the
  run is the second preparation: the admission's `prepared` and the run record are of
  what was seen at admission.

**What it costs.** Admission prepares again, and prepare decodes every image input (ADR
0004). One input of 50,000,000 pixels, the default limit, measured on the machine this
was written on, in the release profile: 0.64 seconds to prepare, and 0.70 seconds for a
request that came with that preparation, which prepares again and publishes twice; in
the unoptimised profile, 9.8 seconds each
(`admit::measure_what_preparing_and_admitting_one_large_input_costs`, ignored). The
cost stays: the check of an input at admission is what makes an input that is gone a
refusal before a run exists, and keeping decoded pixels from a preparation to its
admission would hold some 200 MB for every preparation a caller has not yet acted on.

### The end of a run is a second publication

When the action is over, one publication adds the journal, the diagnostics and the
record to the run, and, for a request with an id, makes `requests/<id>/ended`. It says
the run must be there (`Publication::extend_run`): a run that was deleted while it
executed is not made again from its last names; the publication is refused with
`NoSuchRun` and the caller is told of the store's failure. The run is reported after
this publication is durable, as before.

So a run is in the store in one of two states, each whole: **admitted**, with its
admission and nothing else, or **ended**, with its admission, journal, diagnostics and
record. A test fails the store at every step of both publications and finds only those
(`execute::a_run_whose_publication_fails_is_the_stores_failure_and_is_never_half_ended`).

ADR 0005 said nothing durable exists while a run executes, and that a run is in the
store whole or not at all. Both are amended there: a run now exists, durably, from its
admission, and "whole" is one of the two states.

### What a repeat is answered with

`Answer::Repeat` holds the run's id, when it was admitted, and a `RunState`:

| The first run | State | From |
|---|---|---|
| Completed, failed, invalid or cancelled, and still in the store | `Ended`, with the outcome | The run's record. It is the outcome the first answer held: the secrets were taken out of both alike |
| Executing in this process | `NotEnded { here: true }` | The run has an admission and no record, and this executor's store made the run since it was opened (`Store::made_here`) |
| Admitted by a process that is gone | `NotEnded { here: false }` | The same, and this writer did not make the run. One process writes a store, so nobody is working on it and it will never end |
| Deleted, after it ended | `Removed { ended: Some(status) }` | The id's ending |
| Deleted, before it ended | `Removed { ended: None }` | No ending |

No run is cancelled yet, so `Ended` with `Cancelled` is read and never written.

What `NotEnded` does not tell apart, each pinned by a test or a row:

- `here: true` is also what a run is whose end could not be written (the store failed),
  and whose action panicked: a panic is not caught yet (`docs/carry-over.md`, with
  nested calls, cancellation and publication), it unwinds through `request`, and the run
  stays admitted until the process ends. Both are answered without running anything.
- It is also what a run is whose admission became visible and was not confirmed durable
  (the store's `Unacknowledged`): the caller was told of the store's failure, the action
  never started, and the id is taken all the same. The store cannot take a visible
  generation back, so the request is answered with that run and is not run under its id.
- An executor that only reads a store says `here: false` of every run that has not
  ended, because it cannot know: another process may be executing it.
- A repeat does not wait for a run that is executing. Status and cancel while a run
  executes are the next task's.

A conflict is a refusal (`Refusal::Conflict`), not an outcome: no run exists for the
second request.

A request is never run a second time under its id, whatever happened to its run. A
caller that wants an interrupted or deleted run done again sends it under a new id.

### What `runs list` and `runs show` do with a run that has not ended

`runs list` shows it as `admitted, not ended`, with its action, version and start, from
its admission. `runs show` prints the admission's fields and says there is no record;
`--json` is refused, saying why; `--diagnostic admitted` prints the admission as it is
stored. After a crash the run is listed the same way until it is deleted: nothing marks
it as interrupted, because nothing in the store says which process made it. A run that
other hands made, with neither a record nor an admission, is listed as `no record`, as
before.

## Consequences

- A run costs two publications, and so two generations, where it cost one. A prune
  removes the first.
- A process that dies while an action runs now leaves the run, admitted, and its id's
  record: the retry is answered, and the paid work is not repeated. What it does not
  leave is the journal and the diagnostics the action had written: those are still held
  in memory until the end (`docs/carry-over.md`, with the next task).
- The records of ids grow without bound: two small names for every request that gave an
  id, for the life of the store. Nothing removes one. That is what the spec asks; a
  command to forget ids older than some age would be a change to it.
- A store written before this record has runs without an admission. They are read, and
  listed as before. A store is not kept before the tag of checkpoint 1A (ADR 0005).
- `Contents` gained `read_record`, which reads an entry of either kind, for the
  executor's own records. Nothing that reads an artifact uses it, so a diagnostic is
  still never an input (ADR 0004).
- `Executor` has a third method, `request`. `submit` is unchanged for its callers.
- `Backends` has a second question, the bound, with an answer of nothing by default, so
  no implementation had to change.
- Reversing this costs every stored admission and ending, and every answer a caller
  was promised.

What this record does not decide: settlement, and what is done with the reservation of a
run that did not end (checkpoint 1B); status and cancel while a run executes, nested
calls and an action's outputs (the next task); a command that lists or forgets request
ids.

## Alternatives

- **Bind the id to the fingerprint of the prepared call.** Turned down: it holds the
  config and the models, so the identical request after a config change would be a
  conflict, and it cannot be had without preparing, which the spec forbids before the
  look.
- **Apply the descriptor's defaults before hashing a request.** Turned down: it reads
  the registry before the look, and an upgrade that changes a default would turn a
  repeat into a conflict.
- **Keep the id's record inside the run.** Turned down: it goes with the run.
- **One record that is rewritten when the run ends.** Turned down: a name that is given
  other content is exactly what "never removed" must exclude, and the old content is
  pruned.
- **No ending; answer a repeat for a deleted run with "deleted" alone.** Turned down:
  "what happened to the run" is how it ended, and a caller that lost its connection
  wants to know whether the work was done.
- **Keep an error's message in the ending.** Turned down: whoever deleted the run
  deleted what it said.
- **Decide "one id, one run" by looking before publishing.** Turned down: two executors
  over one store can each look, find nothing and publish. The store decides.
- **Hash the id into its name.** Turned down: the alphabet already makes an id one safe
  segment, and a name that shows the id can be found by a person.
- **Mark an interrupted run when the next writer opens the store.** Turned down for
  now: it is a write on open, by whatever command opened it, and settlement
  (checkpoint 1B) is what needs to act on such a run.
- **A schema for the two records.** Turned down: nothing outside this code reads them.
