# 0015. The executor's interfaces: what a strand plugs into, one section for each

Status: accepted. Changes ADR 0007 ("Admission": the executor's parts are asked in the
admission's step; "A preparation handed to admission": a preparation carries the time it
was made, and a request may carry a cap), and names ADR 0004 (`prepared/1` is as it
was) and ADR 0013 (the kinds a record has for a refused call)
Date: 2026-11-14
Spec: section 3, "The execution contract"; section 6, "Spend"

## Context

Checkpoint 1B builds the policy around a call in strands that work side by side: spend,
config, egress and models. Each needs the executor to call it at one moment of a call's
life. The plan decides that the executor's files are changed by one line of small tasks,
I1 to I7, each of which fixes one interface: the call from the executor, a default that
does nothing, and a fake in the test kit. After them a strand implements an interface
and never edits the executor.

This record holds those interfaces, one section for each task, added one at a time.
**It holds no policy.** What a cap is worth, how old a preparation may be, who may
receive data and what a request costs are the strands'.

What there was for the first of them: admission as ADR 0007 has it. A call that was
prepared and is ready is admitted by one publication, attempted in a loop: the ledger is
asked in each attempt (`Ledger::reserve`), and an attempt the store refuses is followed
by another. A request carries a call, an id, the preparation its caller was shown, a
signal and a deadline. Two tasks need to refuse a call there, before a run exists, for
reasons the executor does not know: the cap of one call (task S3) and the age of a
preparation (task S8).

## Decision

### A part in the admission

Task I1. The code is `crates/whizbang-core/src/contract/part.rs`, with the executor's
side in `engine.rs`, `request.rs` and `prepare.rs`; the fake is
`crates/whizbang-testkit/src/part.rs`. The tests are in
`crates/whizbang-core/tests/it/contract/part.rs`, named below as `part::…`.

**The executor holds a list of parts** (`Engine::with_part`, which adds one after those
it has). A part is an `AdmissionPart`: a name, and one question, `look`.

**When a part is asked.** Inside admission, for every call that was prepared, is not
stale and is ready, **before each attempt at the admission's publication**: after the
publication is begun, and before the ledger is asked. An attempt the store refuses,
because the run's id or a ledger's name was taken meanwhile, is followed by another, and
every part is asked again, with the run that attempt would make: what a part added to a
refused attempt was published by nothing
(`part::a_part_is_asked_before_each_attempt_and_its_record_is_of_the_run_that_was_made`).
A call from inside an action is shown to the parts as any call is, after its share was
taken from its caller's allowance
(`part::a_call_from_inside_an_action_is_shown_to_the_parts_and_its_refusal_is_in_its_callers_record`).
A repeat and a conflict are answered by the look for the id, before anything is
prepared, and no part is asked
(`part::a_requests_cap_and_a_preparations_time_reach_a_part_as_they_were_given`, its
last lines).

**What a part is shown** (`Asking`), as it was given:

| Shown | Is |
|---|---|
| `call()` | The call |
| `request_id()` | The caller's id for the request, when it gave one |
| `expected()` | The preparation the request came with, when it came with one, with the time it was made |
| `cap()` | The cap the request came with, when it came with one |
| `prepared()` | The preparation made at admission, which the run will be admitted with |
| `now_ms()` | The time of the admission, by the executor's clock |
| `parent()` | The run whose action made the call, for a call from inside an action |

and, of the attempt (`PartStep`), the run it would make.

**The time is the admission's.** It is the one reading of the clock an admission makes,
the same number the admission and the run's record give as the run's start, and the time
the preparation made at admission carries. It is not read again for a second attempt: a
part that judges an age judges it once for an admission, at the time the admission
says (`part::a_part_is_asked_before_each_attempt_and_its_record_is_of_the_run_that_was_made`,
under a clock that moves at every reading).

**Parts are asked in the order they were given, and the first that does not return is
the answer:** those after it are not asked
(`part::parts_are_asked_in_the_order_they_were_given_and_the_first_refusal_is_the_answer`).
A part then does one of three things.

- **It returns.** The next part is asked, then the ledger, and the publication is
  published.
- **It refuses** (`PartRefusal`). The call is refused with `Refusal::ByPart`: the
  action, the part's name, a kind and the part's sentence. The publication is dropped
  with everything staged in it: no generation, no run, no name, no file; the ledger was
  not asked, so nothing is reserved; what a call from inside an action took from its
  caller's allowance is given back; and the request's id is not taken by this request
  (`part::a_part_that_refuses_leaves_no_run_no_reservation_and_the_requests_id_free`).
- **It fails** (`PartDeclined::Failed`), or does what a part may not. That is
  `CallError::Part`, with the part's name: not a refusal of the call, and nothing is
  published
  (`part::a_part_that_fails_is_not_a_refusal_of_the_call_and_nothing_is_published`).

**A refusal of its own kind.** A part names itself, and its refusal carries that name:
a caller tells the cap's refusal from another part's by `part`. It also carries a
**kind, which the part chooses among the kinds `run/3` has for a refused call** (ADR
0013): the 27 that are not of a capability request. That kind is what `Refusal::kind`
answers, and what the record of a run whose action made the call lists the refusal
under. `PartRefusal::new` makes none for a kind that is of a capability request, or for
an empty sentence. The sentence is the part's own; the executor takes its secrets out
of it while it is whole and then cuts it at 512 characters, as a record does with every
refusal's
(`part::a_parts_refusal_is_of_a_kind_of_a_call_and_its_sentence_is_redacted_and_then_cut`).
A part's failure has its sentence treated the same way.

**A refusal by a part is a refusal after the look** (ADR 0007, "A refusal after the
look"). Between a request's look for its id and a part's refusal, the same id can be
admitted for another request. So for a request with an id the id is looked up once more
before the refusal is returned: an id that is taken by then answers, as a repeat or a
conflict, and the refusal stands only when it is not
(`part::a_refusal_by_a_part_is_a_refusal_after_the_look_so_the_id_is_looked_up_once_more`).

**A part's records.** `PartStep::put(name, bytes)` adds a record **among the names of
the run the attempt makes**, as `admission/<name>` (`part_record_name`), stored as a
diagnostic reference: readable, never an output. It is in the same publication as the
run, its admission, the id's record and the reservation, so it is in the store with the
run or not at all. Two tests hold that: one refuses each call the publication makes of
the file system in turn, and one loses power before each, under every way the test
kit's disk can lose what was not yet durable
(`part::a_parts_record_is_in_the_store_with_the_run_or_not_at_all_whichever_call_of_the_publication_is_refused`;
`part::admitted_with_a_part_a_crash_at_any_event_leaves_the_parts_record_with_its_run_or_neither_even_steps`
and the test beside it for the odd steps). It belongs to its run: it stays when the run
ends, the run's record does not list it, and deleting the run removes it
(`part::a_parts_record_is_among_the_names_of_the_run_that_was_made_and_goes_with_it`).
What `put` refuses, as the part's failure and with nothing published:

- **a name or a content that holds a secret** the executor was told of. This is looked
  at first, in the name and the bytes as they were given, so a name that is also no
  name is refused for its secret and nothing of it is shown;
- a name that is not one a diagnostic may have (a lower-case letter or a digit, then
  lower-case letters, digits, `_`, `-` or `.`, 64 characters at most), shown escaped
  and cut;
- a record of more than 65,536 bytes (`PART_RECORD_MAX_BYTES`);
- a name given twice in one admission, by one part or by two;
- one more than 16 records for one admission, over all its parts
  (`PART_RECORDS_LIMIT`)

(`part::a_parts_record_that_holds_a_secret_is_refused_and_not_shown`,
`part::two_parts_share_the_bound_on_records_and_cannot_give_one_name_twice`).

Whoever reads a part's record back reads stored bytes: it looks at the entry's length
first and reads strictly, as the plan's rule for stored bytes says. That reader is the
part's own task.

**Two things a part reads, which are the executor's.**

- **A request may carry a cap of its own** (`Request::capped_at`, `Request::cap`): an
  `Estimate`, in millionths of a US dollar. **It is no part of the request's
  fingerprint:** `request/1` is as ADR 0007 gives it, and the same call under the same
  id with another cap is the same request, answered with its run. The executor holds
  no call to a cap and gives none a meaning: nothing, and the largest amount, reach a
  part as they were given. A call from inside an action carries its cap the same way.
- **A preparation carries the time it was made** (`PreparedCall::made_ms`), in
  milliseconds since the Unix epoch, by the clock of the executor that made it. **It is
  beside `prepared/1` and not in it:** the fingerprint is as ADR 0004 gives it, the
  same call prepared at another time has the same fingerprint, and a preparation is not
  stale against the call prepared again for its time alone, nor unequal to it. A preparation made by
  `Executor::prepare` carries the time of that call; the one made at admission carries
  the time of the admission.

(`part::a_requests_cap_and_a_preparations_time_reach_a_part_as_they_were_given`,
`part::a_cap_on_a_call_from_inside_an_action_reaches_a_part_too`. The fixed vectors of
the two fingerprints are held where they were, unchanged:
`admit::the_fingerprint_of_a_request_is_of_the_call_as_it_was_given` and
`prepare::a_fingerprint_is_this_digest` with
`prepare::a_fingerprint_with_one_of_everything_is_this_digest`.)

**With no part, the executor behaves as before.** The tests of ADR 0007 pass unchanged
(`admit::…`), and a part that adds nothing and refuses nothing leaves the store, and
the calls made of the file system, as they are without one
(`part::a_part_that_adds_nothing_and_refuses_nothing_leaves_the_store_as_it_is_without_one`).
The binary's executor has no part until the wiring (task W1).

**The fake.** `FakePart` refuses, fails or adds a record when a test tells it to, keeps
what it was shown each time it was asked (`Seen`), and can run a test's hook while it is
asked.

**Limits, each stated.**

- **A part's refusal has no kind of its own in a run's record.** `run/3` fixes the
  kinds, so a part chooses one of them, and its own name is in the sentence only. A
  part that finds no kind that fits needs a new version of the record
  (`docs/carry-over.md`).
- **The window before the publication is as it was.** Between a part's answer and the
  publication nothing holds still what the part judged on, as nothing holds the config
  (ADR 0007, "The window that is left"). A part whose answer depends on what the store
  holds is not served by this interface: that is the ledger's step.
- **A part that panics** unwinds through admission as a panic in a ledger does: nothing
  is published and the request's caller sees the panic. No test holds it.
- **A part is not asked how often it may be asked again.** That is bounded as the
  ledger's asking is: by the caller's signal and deadline, looked at before each
  attempt.
- **The time is not read again for a later attempt** ("The time is the admission's").

## Consequences

- `Refusal` and `CallError` each have one more kind, and `Request` and `PreparedCall`
  one more thing they carry. Nothing stored changes: no record, no fingerprint, no
  schema.
- Two preparations of one call made at different times are equal as values (`==`), as
  they were: the time is left out of the comparison, so a test that compares two
  preparations is not a test of the clock
  (`prepare::a_negative_zero_is_prepared_as_zero`, unchanged).
- A run may hold names under `admission/` beside `admitted`. A reader of a run's names
  that knew four kinds of name now meets a fifth.
- A strand adds a refusal at admission without editing the executor: tasks S3 and S8.
- Reversing it costs the parts that were written against it.

## Alternatives

- **A new kind of refusal in the executor for each policy** (over the cap, too old).
  Turned down: each would be an edit to the executor's files by a strand, which the
  plan rules out, and each a new kind in the record.
- **One new kind in `run/3` for every part's refusal.** Turned down: `run/3` is defined
  once for the checkpoint, before the strands that fill it (ADR 0013), and a part's
  refusal usually is one of the kinds there already: a preparation that is too old is
  stale.
- **Parts asked once, before the loop.** Turned down: a part's records are staged in a
  publication, and each attempt has a new one, naming a run that may be another.
- **Parts asked after the ledger.** Turned down: a call a part refuses would have been
  judged against the allowance first, and a ledger that counts what it was asked would
  count it.
- **The clock read again for each attempt.** Turned down: an admission would then say
  one time in its record and be judged at another, and every reading added moves the
  times of every test that counts them.
- **A part's records under names of their own outside runs,** as the ledger's are.
  Turned down by the plan: they are among the run's names, and go with the run.
- **The cap as a number in the call's arguments.** Turned down: an argument is in the
  fingerprint of a request, and a cap is not what was asked.
