# 0007. Request ids and admission: a run exists from the step that records its request

Status: accepted
Date: 2026-11-12
Amended: 2026-11-12, with the second pull request of task A6. Added: "The estimate" and
"A preparation handed to admission", which are built; the paragraph of "Admission" that
listed the second part now lists only what is still to come, the ledger and the
reservation. The task is delivered in three pull requests, not two. And from the review
of the first: a run is the id's only when it holds the id's own admission, so a run id
used again after a delete does not answer for another request; an ending without an
admission is a record this build does not read; text is made an id through the
executor, which looks for a secret before it shows anything; the store forgets a run it
made once the run is closed or deleted. Corrected: a panicked run stays admitted after
the process ends too. Added to the consequences: what every publication costs as ids
accumulate; a repeat answered from a generation not yet confirmed; `.` and `..` as ids.
Amended: 2026-11-12, with the third and last pull request of task A6. Added: "The
reservation", which is built, and which decides what the paragraph on what was still to
come left open: what a reservation is, what it means for a ledger to take part in the
step, the refusals `OverAllowance` and `NoBound`, and that what was reserved is read
through names and never from content. Changed: an admission says what was reserved
(`reserved_micros`), so an admission written by the first two parts is not read; a
bound is an `Estimate`, not a bare number; the crash test tries every choice of what a
disk loses; the run record is `run/2` (ADR 0005). Corrected: a refusal of a stored
admission showed an action's name or version escaped twice; a run that had just become
visible could be answered as made by a process that is gone.
Amended: 2026-11-12, with ADR 0008, which catches a panic in an action. Changed, in
"What a repeat is answered with": a run whose action panicked ends as an error and is
answered as `Ended`; `NotEnded { here: true }` no longer covers it.
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
**The third part builds:** the ledger's interface, a fake ledger and the reservation.
All three are merged with this amendment; what is left to checkpoint 1B is said in "The
reservation" and is on `docs/carry-over.md`.

The code is in `crates/whizbang-core/src/contract/` (`request.rs`, `ledger.rs`,
`engine.rs`), `crates/whizbang-testkit/src/ledger.rs` and
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
- text becomes an id through the executor (`Executor::request_id`), which looks for the
  secrets it was told of in the text as it was given, before the text is parsed: text
  that holds one is refused (`Refusal::SecretInRequestId`) and none of it is shown,
  whether or not it would have been an id. `RequestId::parse` itself knows no secret,
  and its refusal shows up to 64 characters of what it refused: it is for text that
  did not come from a caller. An id that reaches `request` is checked again, before
  anything is looked up. Redaction is by value (ADR 0005): an id that holds a key the
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
prepared when it was admitted; `reserved_micros`, what was reserved for the run in
millionths of a US dollar, or null when the executor had no ledger and nothing was
reserved; `action` and `version`, held to the rules of ADR 0003;
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
   happened: the action has not been found, no input read, no backend asked. The
   generation is the one that is visible, which a publication whose last sync is still
   to come may have made: a repeat can be answered from an admission that is not yet
   confirmed durable, and a loss of power then takes the admission and the answer's
   run with it. The first request was not answered in that case, so no caller was
   promised that run twice.
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
free nor answered until the record is put right. An ending under an id that has no
admission is such a record too: were the id taken as free, its action would run and its
end then be refused.

**Which run is the id's.** The id's record names its run by the run's id, and a run's
id is free again once the run is deleted: a later process, whose count starts again,
can give it to another run. So the run the store holds under that id is taken for the
request's only when its own `admitted` entry has the digest of the id's record, which
were published as the same bytes. Any other run under that id is another request's:
the request's run is gone, and the answer is `Removed`, from the ending
(`admit::a_repeat_is_not_answered_from_another_run_that_took_the_deleted_runs_id`).

### Admission

A call that was prepared and is ready is admitted by **one publication**, which:

- makes the run (`create_run`), with its admission under `admitted`;
- for a request with an id, makes the name `requests/<id>/admitted` with the same bytes;
- for an executor with a ledger, makes the reservation's name, and the names the ledger
  added ("The reservation", below).

The store decides both in its publishing step. A run id that was taken meanwhile is
refused with `RunExists`, and the run is named again, eight namings at most (ADR 0005).
An id that was admitted meanwhile is refused with `NameExists`: nothing of this request
was published, its action does not run, and the request is answered by looking the id
up again, as a repeat or a conflict. So **one id makes one run**, whatever two executors
over one store saw before they published; a test holds two requests at a barrier, after
each has looked and prepared, and gets one run
(`admit::two_simultaneous_requests_with_one_id_produce_one_run`).

A crash anywhere in that publication leaves the run and the id's record together or
neither, and the reservation with them: a publication is one replaced pointer (the
store's own guarantee), and a test loses power before each of its steps, under every
way the test kit's disk can lose what was not yet durable: every name undone, every
name kept, and every choice of which directories, and of which names, lose theirs. How
many directories and names a crash leaves undecided is read from the first crash at
each step, and every mask over them is then a crash of its own
(`admit::a_crash_inside_the_admission_step_leaves_no_record_or_all_of_it`).

The action runs only after the publication is durable. A refusal, and a call that is
not ready, happen before it and record no id: the id stays free for the request once it
can run.

### The reservation

**What it is.** A record, kind `reservation/1`, written and read as the two records
above are: `record`; `run`, the run it was reserved for; `request`, the request's id or
null; `admission`, the digest of the run's admission; `reserved_micros`, the amount, in
millionths of a US dollar; `start_ms`. The amount is the estimate of the preparation
made at admission ("A preparation handed to admission", below), as the backends stated
it then. It is never taken from the preparation a request came with, which is only
compared, and what a backend states later is not looked at again: the record holds the
number it was made with.

Its name is `spend/reservations/<run>`, outside runs, made once (`Publication::create`).
So it is found from the run by the run's id, and from a request id through the id's
record, which names the run; and `store delete` and `store prune` leave it, as the spec
says of spend records
(`store::the_records_of_request_ids_are_left_by_delete_and_prune_and_still_answer`).
A run's id is free again once the run is deleted, and the reservation is not: so the run
that the store holds under the id is the reservation's only when its own `admitted` has
the digest the reservation holds, the rule "Which run is the id's" gives for an id. And
an admission that would make a reservation's name the store already holds is refused
whole by the store; the executor names the run again, as it does for a run's id that
was taken, within the same eight namings
(`admit::a_reservation_left_by_a_deleted_run_keeps_its_name_and_the_new_run_is_named_again`).

The admission says the same amount (`reserved_micros`), in both its copies, which stay
one content; and the run's record says it when the run ends (`spend.reserved_micros`,
`run/2`, ADR 0005). Everything that names the run is made again each time the run is
named again: the admission, and the reservation, which holds the admission's digest.

**Who reserves.** An executor that was given a ledger (`Engine::with_ledger`). Every
call it admits has a reservation, of zero when the estimate is zero. An executor
without one reserves nothing, writes `reserved_micros` as null and makes no name under
`spend/`: that is the binary today, which has no ledger until checkpoint 1B.

**What "takes part in the step" means.** The interface is `Ledger::reserve`, called
inside admission once for every attempt at the admission's publication, after the
publication is begun and before it is published. It is handed the reservation and a
`Step`: the store's generation as the attempt found it, the reservations that
generation names, and a way to add records of the ledger's own to the same publication
(`Step::make`), under names that are made once. Then one of three things:

- It returns. The executor adds the reservation and publishes: the run, the id's
  record, the reservation and what the ledger added become visible together or not at
  all.
- It declines because the allowance does not cover the amount. The call is refused with
  `Refusal::OverAllowance`, which gives the amount asked and what is left. The
  publication is dropped with everything it staged: no generation, no run, no name, no
  file, and the request's id as free as it was
  (`admit::a_call_the_ledger_declines_is_refused_with_no_run_and_its_id_stays_free`).
- It fails, or does what a ledger may not. That is `CallError::Ledger`, not a refusal
  of the call, and nothing is published.

This is the interface the ledger of checkpoint 1B has to satisfy, so one thing about it
is decided here and not left to the fake. A ledger judges on a generation that can be
old by the time the publication is published: another admission over the same store may
have reserved meanwhile. The store decides only what a publication asks it to, that a
name it makes is not there. So a ledger whose answer depends on what was reserved makes
a name that any admission it must not overlook would make too; the store refuses the
later of the two whole, and the executor asks the ledger again, on the generation as it
is then. The fake makes `spend/fake/<n>`, where n is one more than the reservations it
saw. A test holds two admissions, under two ids, at a barrier inside the ledger, after
each was told yes for the last of the allowance: one is published, and the other is
asked again and declined
(`admit::two_simultaneous_admissions_cannot_both_take_the_last_of_the_allowance`).
Being asked again is not bounded by a count: it happens only when the store changed
since the ledger judged, because a name the step's generation already holds is refused
when the ledger makes it.

A ledger's names start with `spend/` and not with `spend/reservations/`, in a plain
alphabet, 200 characters at most; a name or a content that holds a secret the executor
was told of is refused and not shown.

**A cost without a bound.** An estimate is a sum that stops at the largest amount there
is. That value is not an amount: nothing is reserved for it and nothing is added to it.
An executor with a ledger refuses such a call with `Refusal::NoBound`, and the ledger is
not asked; a stored admission, reservation or run record that holds it is refused on
reading
(`admit::a_call_whose_cost_has_no_bound_is_not_admitted_and_the_ledger_is_not_asked`).
An executor without a ledger still admits it: it reserves nothing. The refusal of a paid
request whose backend states no enforceable bound is checkpoint 1B's.

**What was reserved is what the current generation names.** A call to `reserve` is not
a reservation. An attempt that the store refuses, because the run's id was taken
(`RunExists`) or the request's id was (`NameExists`), has already moved its content into
`objects/`: the bytes of a reservation for a run that was never made are in the store,
named by nothing, until a prune removes them. So whatever reads reservations reads them
through names in the current generation and never by going through content. The one
reader, `reservations`, takes a generation and has no other way in, and the fake ledger
keeps no count of its own: it reads the store each time. Three tests hold it. Two
executors racing one id are asked twice and leave two reservations in `objects/` and
one name. A run named four times is asked four times and leaves four and one, the one
naming the final run, with the id's record and the run's own admission the same bytes
naming that run. A run named eight times and given up leaves eight and none
(`admit::two_simultaneous_requests_with_one_id_make_one_reservation`,
`admit::a_run_that_is_named_again_has_one_reservation_which_names_its_final_run`,
`admit::a_run_that_is_named_eight_times_and_given_up_reserves_nothing`).

**What makes none.** Prepare
(`prepare::prepare_makes_no_reservation`, against an executor that holds the ledger); a
stale preparation, each of the three cases, on the ledger's count and on the store; a
repeat; a conflict; a call that is refused or not ready; an executor that only reads its
store (`admit::a_repeat_a_conflict_and_a_request_that_does_not_run_reserve_nothing`).

**What is read is not trusted.** A stored reservation is bounded before it is read,
read strictly, and held to be of the run its name says. One that fails stops the
admission that needed it (`CallError::Record`), with what it quotes escaped and cut and
no path, and nothing is published.

**What is left to settlement, which is checkpoint 1B's.** Nothing here releases,
charges or removes a reservation. When its run completes, fails or is invalid, when its
action panics, and when the process dies between admission and the end, the reservation
stays as it was made, and counts against the fake's allowance for good. An admission
that became visible and was not confirmed durable has its reservation with it, as it
has its id. What 1B needs to settle one is there: the reservation names its run and its
request, holds the admission's digest so that a later run under the same id is not
taken for it, and holds the amount; a run that ended has a record and, with an id, an
ending; a run that did not has neither. Dispatch records, the allowance and its reset,
a nested call drawing on its parent's reservation, and what a failed request cost are
all 1B's (`SPEC.md` section 6).

**What is not covered.** `Publication::put` still replaces any name, a reservation's
too: the store does not refuse a `put` under `requests/` or `spend/`, and no code of
whizbang makes one (`store::runs::known_a_name_that_was_made_can_still_be_replaced_by_put`).
The limit stays until the real ledger, which is the first code with a reason to decide
what else may be written under `spend/`.

### The estimate

Prepare returns, with readiness, an `Estimate`: the most the call may cost, in
millionths of a US dollar. It is the sum of what the backends state as the bound of each
capability the call needs and something answers (`Backends::bound`, asked with the
capability and the model that would answer); a need that nothing answers adds nothing,
and a sum past the largest amount is the largest amount. It is answered from what is
configured, like resolution: nothing is sent and nothing is written.

No backend has a price before the ledger of checkpoint 1B, so `Backends::bound`, whose
answer is an `Estimate` and so carries its unit, answers nothing unless an implementation says otherwise, and every estimate the binary can make
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
  run is the second preparation: the admission's `prepared`, the reservation's amount
  and the run record are of what was seen at admission.

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
the run must be there (`Publication::close_run`, which is `extend_run` for a run's last
names): a run that was deleted while it
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

An executor that writes the store looks a second time before it says `here: false`: its
snapshot is older than its question to the store, and a run that ended in between would
otherwise be said to have no process
(`admit::a_repeat_that_races_the_end_of_its_run_is_not_told_the_run_will_never_end`).
The other edge is the store's: it counts a run as made here from just before the rename
that shows it, and forgets it again if that rename fails, so a run that a snapshot shows
is never one the store has not yet counted
(`store::runs::a_run_is_known_as_made_here_from_the_moment_it_is_visible`). And a run is
forgotten only once its closing is visible, not when `close_run` is called
(`admit::a_run_whose_closing_was_refused_before_it_was_visible_is_still_this_writers`).

What `NotEnded` does not tell apart, each pinned by a test or a row:

- `here: true` is also what a run is whose end could not be written (the store failed):
  the run stays admitted, in that process and after it has ended, when only `here`
  changes, and is answered without running anything. A run whose action panicked is not
  one of these any more: the panic is caught, the run ends as an error with its record,
  and a repeat is answered `Ended` (ADR 0008, "A panic in an action").
- It is also what a run is whose admission became visible and was not confirmed durable
  (the store's `Unacknowledged`): the caller was told of the store's failure, the action
  never started, and the id is taken all the same. The store cannot take a visible
  generation back, so the request is answered with that run and is not run under its id
  (`admit::an_admission_that_was_visible_and_not_confirmed_takes_the_id_and_never_runs`;
  `docs/carry-over.md`).
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
- Every publication writes the whole generation again, and the names of ids are in it:
  the cost of each publication, two for a run, grows with every id the store was ever
  given. The review of the first part measured a generation file of 2.0 MB at 5,000 ids.
  It is on `docs/carry-over.md` to decide before ids are used in volume.
- An id may be `.` or `..`: the alphabet allows both. That is safe because a name lives
  in the generation's JSON and is never a file's name. Anything that writes names out
  as paths has to keep it so (`docs/carry-over.md`).
- The store remembers the runs its writer made and has not closed or deleted
  (`Store::made_here`, `Publication::close_run`), not every run it ever made.
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
  no implementation had to change. Its answer is an `Estimate`, which carries the unit.
- Every admission through a ledger adds two names outside runs, the reservation's and
  the ledger's, which are never removed: with the two of an id, four for a request. They
  add to the cost of every publication named above.
- An admission written by the first two parts of this task has no `reserved_micros` and
  is not read; a `run/1` record is another version. A store is not kept before the tag
  of checkpoint 1A (ADR 0005).
- `CallError` has a fourth kind, the ledger's failure, and `Refusal` two more.
- `runs show` says what was reserved for a run, from its record or, for a run that has
  not ended, from its admission.
- Reversing this costs every stored admission and ending, and every answer a caller
  was promised.

What this record does not decide: settlement, what is done with the reservation of a
run that ended or did not, and the ledger itself (checkpoint 1B); status and cancel while a run executes, nested
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
  The same holds for the reservation until a ledger outside this code reads one.
- **The reservation as a field of the admission and nothing else.** Turned down: an
  admission of a run without an id lives only among the run's names and goes with the
  run, and a spend record must outlive it.
- **A ledger that publishes its own records, before or after the admission.** Turned
  down: a crash between the two leaves a reservation without a run or a run without a
  reservation.
- **A lock around admission in place of a made name.** Turned down: it would hold for
  one executor and not for two over one store, which is the case the store's publishing
  step already decides.
- **Count what the ledger was asked.** Turned down: an attempt the store refuses was
  asked for and reserved nothing.
