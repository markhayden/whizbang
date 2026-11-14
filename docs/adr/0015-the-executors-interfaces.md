# 0015. The executor's interfaces: what a strand plugs into, one section for each

Status: accepted. Changes ADR 0007 ("Admission": the executor's parts are asked in the
admission's step; "A preparation handed to admission": a preparation carries the time it
was made, and a request may carry a cap), and names ADR 0004 (`prepared/1` is as it
was) and ADR 0013 (the kinds a record has for a refused call)
Date: 2026-11-14
Amended: 2026-11-14, after the review of task I1. Changed under "A part in the
admission": what a part's failure may hold (`NotJudged`: a failure of the store, a
record that was not read, or a sentence), that it is returned without a second look
for the id, and that the ledger's failure holds the same; and a part that panics.
Amended: 2026-11-14, with task I5. Added: "A policy before a send, and a checked send",
which settles three of the points ADR 0012 left under "For later tasks" (who knows a
piece's class at a send, where a credential lives, and who knows a backend's recipient
when `Sender::send` is given none). Changed, as the review of task I1 asked: the
sentence on how a failure's sentence is cleaned, one more limit of it, and a third
test of a part that panics.
Amended: 2026-11-14, after the review of task I5, before it merged. Changed under "A
policy before a send, and a checked send": the entry of a send is taken before
anything is handed over, and a send that cannot be listed is not made; a sender is
good for its run only; an origin that holds a secret is not sent to; refused sends
take half the list at most; a refusal does not set the count of moves back, and the
check is shown a total; an upstream's credential; and what an executor with no sender
notes. ADR 0013 is amended with it ("Where data went").
Spec: section 3, "The execution contract"; section 6, "Spend"; section 10, "Egress"

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
  **A failure holds a failure of the store, a record that was not read, or a
  sentence, and nothing else** (`NotJudged`): the type has no place for a refusal or
  for any other error of the executor's. Whatever error a part hands back is turned
  into one: the store's stays the store's, a record's stays `CallError::Record`, and
  any other, a refusal among them, is only what it says, returned as `CallError::Part`
  under the name of the part that was asked. Every sentence has the executor's
  secrets taken out while it is whole and is then cut at 512 characters
  (`part::whatever_a_part_fails_with_reaches_the_caller_as_the_parts_failure_with_no_secret_in_it`,
  `part::a_store_failure_inside_a_part_reaches_the_caller_as_the_stores`). The
  ledger's `Declined::Failed` holds the same type and is treated the same way, as
  `CallError::Ledger`
  (`admit::whatever_a_ledger_fails_with_reaches_the_caller_as_the_ledgers_failure_with_no_secret_in_it`,
  `admit::a_store_failure_inside_a_ledger_reaches_the_caller_as_the_stores`).
- **It panics.** The panic is its caller's: of whoever made the request, or, for a
  call from inside an action, of that action, whose run ends as an error as after any
  panic of an action. Either way the publication is dropped with what it staged: the
  store is as it was and publishes for the next, nothing is reserved for the call,
  and the same request is admitted afterwards
  (`part::a_part_that_panics_leaves_the_store_as_it_was_and_the_same_request_is_admitted_after`,
  `part::a_part_that_panics_at_a_call_from_inside_an_action_ends_that_run_and_leaves_no_other`).
  An action that catches the panic of a call it made goes on as if the call had not
  been made: what the call took from its allowance is given back whole, and a second
  call, which needs all of it, is admitted and runs
  (`part::an_action_that_catches_a_parts_panic_at_a_nested_call_keeps_its_full_allowance_and_a_second_call_runs`).

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
A part's failure, like a ledger's, is returned without a second look for the id
(`part::a_parts_failure_is_returned_without_a_second_look_for_the_id`,
`admit::a_ledgers_failure_is_returned_without_a_second_look_for_the_id`).

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
- **A sentence is cleaned by itself:** a secret that is completed only by the words
  the executor puts before it is not found.

### A policy before a send, and a checked send

Task I5. The code is `crates/whizbang-core/src/egress/policy.rs` and `checked.rs`, with
the executor's side in `capability/mod.rs` (`Bounds::sender`), `contract/context.rs`
(`Capabilities`) and `contract/engine.rs` (`Engine::with_ways`); the fakes are
`FakeCheck` in `crates/whizbang-testkit/src/sending.rs` and `FakeFiller::posting`. The
tests are in `crates/whizbang-core/tests/it/contract/send.rs`, named below as
`send::…`.

ADR 0012 decided the types a send is made of and enforced nothing. This section is the
executor's part of the enforcement: **what is asked before a request is handed to a
sender, in which order, and what is noted.** Who may receive what is not decided here:
the policy is a value the executor is given, and the check of an address is the egress
strand's (tasks E2 and E3). The default of each approves nothing, and so nothing the
executor is not told to send is sent.

**The policy** (`DispatchPolicy`) is a value that holds, for at most 64 recipients
(`APPROVED_LIMIT`), the classes of data each may receive and whether each may be
probed. It answers two questions and holds no credential.

- `may_receive(recipient, classes)`: yes when that recipient was approved for every
  one of those classes. A request that holds no data is still a request to the
  recipient, and is approved only for a recipient that may receive some class.
- `may_probe(recipient)`: approved apart from any class of data (ADR 0012). A
  recipient that may be probed and nothing else may receive nothing.
- **A recipient is the one that was approved only when its origin, its account and
  its upstreams are all the same** (ADR 0012, "A recipient"): the same endpoint under
  another account, or one that forwards to one origin more, is approved for nothing.
- **The default, `DispatchPolicy::none()`, approves nothing,** and is what an executor
  has until it is given another.
- What `{:?}` prints is each recipient as a recipient is always printed, its origin
  and its account cut, with its classes.

(`send::the_default_policy_approves_nothing`,
`send::a_policy_approves_a_recipient_for_the_classes_it_was_given_and_no_other`,
`send::a_recipient_is_the_one_approved_only_with_its_origin_its_account_and_its_upstreams`,
`send::probing_is_approved_apart_from_any_class_of_data`,
`send::a_policy_names_a_bounded_number_of_recipients`,
`send::the_policys_debug_shows_recipients_cut_and_no_credential`.)

**The ways out** (`Ways`) are what the executor holds (`Engine::with_ways`): the sender
behind every send, the check (`HopCheck`), the policy, and the credentials of its
recipients, at most 256 (`CREDENTIALS_LIMIT`). A credential is held for one recipient
and is bound to one origin, which is the recipient's own or one of its upstreams; a
second for the same recipient and origin is refused
(`send::a_credential_is_held_for_an_origin_its_recipient_reaches_once_and_within_a_bound`).
With nothing given, there is no sender, the check approves no address and the policy
approves no recipient.

**The sender a backend is handed.** `Sender::send` is given no recipient (ADR 0012,
"For later tasks"), so the executor makes a sender **for each backend of each run**,
where the run's handles are made (`Context::new`, ADR 0006): it knows the recipient
that backend declared, holds that recipient's credentials and no other's, and notes
into that run's place for facts. A backend reaches it through the bounds of its
request, `Bounds::sender()`, and is handed no sender and no credential of its own. A
handle that no executor made hands its backend a sender that sends nothing
(`send::a_handle_that_no_executor_made_has_no_way_out`).

**What is asked before anything is handed over,** for every request the backend makes
of it, the first address and each address an answer pointed to alike, in this order.
The first that says no is the answer, and the sender behind is handed nothing:

1. **The backend declared a recipient.** One that declared itself local sends nothing
   (`send::a_backend_that_names_no_recipient_and_sends_all_the_same_hands_nothing_over`).
2. **The request brings no credential.** `Credential::expose` is public (ADR 0012), so
   a credential lives only inside this sender: a backend is given none, and a request
   that comes with one is not sent.
3. **The address's origin is the recipient's own or one of its upstreams**
   (`Recipient::reaches`). A redirect comes back to the backend as an address, as ADR
   0012 has it, and the request the backend then makes of that address is held to
   this like any other
   (`send::an_origin_that_is_not_the_recipients_or_an_upstream_is_refused_before_the_check_is_asked`).
4. **The policy approves the recipient for the classes the request holds**
   (`Outgoing::needs`). **A piece's class is what whoever built the piece stated,**
   which is ADR 0012's rule and is not second-guessed here: nothing at a send knows a
   source image from a derived one, so a piece that states no class needs all four
   approved (`send::a_piece_with_no_class_needs_every_class_approved`).
5. **The check approves the address** (`HopCheck::look`). It is shown a `Hop`: the
   recipient, the address, the classes, the policy, and how many answers in a row
   just before pointed elsewhere (`Hop::moves`), which is what a limit on a chain of
   redirects is held with. The default, `NoHop`, approves no address
   (`send::a_sender_with_the_default_check_hands_nothing_to_any_origin`); a check that
   approves is shown each origin before the sender behind is handed its request
   (`send::a_check_that_approves_sees_each_origin_before_the_sending_fake_does`).

Then the credential held for that recipient and bound to that origin is attached, or
none: a request to an upstream goes without the recipient's own, and a backend of
another recipient at the same origin is handed none of it. **An upstream is handed a
credential only when the executor was given one bound to that upstream; the
recipient's own is never handed to an upstream**
(`send::a_credential_is_attached_by_the_executor_for_its_own_origin_and_a_backend_brings_none`).
An executor with no sender answers a fault here, of the kind `unavailable` that states
the stage `not_sent`, and sends nothing. It is noted nowhere: nothing was refused and
nothing left (`send::an_executor_with_no_sender_sends_nothing_whatever_approves`).

**With the default policy a backend that names a recipient hands nothing over,** even
behind a check and a sender that would let everything through; and a backend that
names none is asked as before
(`send::with_the_default_policy_a_backend_that_names_a_recipient_hands_nothing_over_and_one_that_names_none_is_asked`).

**What a refusal is.** A refusal at 1 to 5 answers the backend with a fault of the kind
`rejected` that states the stage `not_sent`, in fixed words that quote nothing of the
address. It is noted in the run's place for facts (ADR 0013) twice: among the run's
refusals, as the kind `request_not_approved`, which `run/3` has, with a sentence that
names the origin and the classes; and in where the run's data went, as `refused`. The
sentence is cleaned where it is noted, the executor's secrets taken out while it is
whole and then cut (`send::a_secret_the_executor_knows_is_in_no_refusal_of_a_send`). A
check's reason is a `Denied`, which takes fixed words and no made text
(`send::a_check_that_refuses_hands_nothing_over_and_its_sentence_is_the_refusal`).
**The refusal among the run's refusals is the note that counts:** that list lists a
refusal or counts it (ADR 0008). The entry in where data went is listed when it can
be, and the refusal's sentence says so when it could not.

**Nothing is handed over without its entry in the record.** After the five questions,
and before the request is handed to the sender behind, the origin is noted as `sent`,
with the recipient's account, cut, and the classes (`RunFacts::sending`). The look at
whether the run still takes notes, the look for room and the note are one step under
the place's lock. **A send whose entry cannot be taken is not made:** it is refused as
any other, `rejected` and `not_sent`, in fixed words, and nothing reaches the sender
behind. Three things make it so:

- **The run is over.** A backend lives across runs, and `Bounds` is a value a backend
  can keep, with the sender in it. **A sender is good for its run only:** once the
  run's end has begun its place takes no note, so every send through a sender that
  was kept is refused, after the run and from inside a later run alike, and the later
  run's record and the first run's are as they were
  (`send::a_sender_kept_past_its_run_hands_nothing_over_after_the_run_or_in_a_later_one`).
  A send that races the end of its run is in the run's record or was refused, never
  handed over and not listed
  (`send::a_send_that_races_its_runs_end_is_in_the_record_or_was_refused`).
- **The list is full** (`send::a_send_whose_entry_cannot_be_listed_is_refused_and_nothing_is_handed_over`).
  Refused sends cannot bring that about: they take half the list at most (ADR 0013,
  `EGRESS_REFUSED_LIMIT`), and the other half is kept for what was sent
  (`send::refused_sends_cannot_use_up_the_room_of_a_send`). A refused send past that
  half is among the run's refusals all the same.
- **The origin, as it is written, holds a secret the executor knows.** A record
  writes an origin whole, so a destination that is itself a secret cannot be
  recorded, and is not sent to
  (`send::an_origin_that_holds_a_secret_cannot_be_listed_so_nothing_is_sent_to_it`).
  A refused send to such an origin has no entry in where data went; its refusal,
  with the origin taken out of its sentence, stands for it, and no file of the store
  holds the origin (`send::a_secret_the_executor_knows_is_in_no_refusal_of_a_send`).

**The entry stays whatever came back:** an answer, an address to go on to, a fault,
or a panic of the sender, since what a sender that unwound did with the request is
not known
(`send::a_sender_that_panics_is_listed_as_sent_its_run_ends_as_an_error_and_the_next_run_goes`).
**Only a fault that states the stage `not_sent` takes the note back,** a cancellation
that states it among them; a fault that states no stage is unresolved, and stays
listed
(`send::a_fault_that_states_no_stage_is_listed_as_sent_and_a_cancellation_only_when_it_says_nothing_left`).
So a request that failed at its second hop lists the first, which had the data
(`send::a_send_that_says_nothing_left_is_not_listed_as_sent_and_any_other_fault_is`).
A note that is taken back leaves the entry what the other sends to that origin hold,
and takes the entry out only when it was the only one
(`send::a_note_that_is_taken_back_leaves_what_the_other_sends_to_the_origin_hold`).
Once the run's end has begun a note is not taken back: the record is made, and an
entry may then say `sent` of a request of which nothing left. That over-states, and
never under-states.

**A check that panics** unwinds through the backend and its handle into the action, as
a backend's panic does: nothing was handed over, the request is accounted for, the run
ends as an error, and the next run sends
(`send::a_check_that_panics_hands_nothing_over_and_the_next_run_goes`).

**The fakes.** `FakeCheck` approves or refuses every address as a test tells it to,
keeps what it was shown, the moves in a row and in all among it, and can run a test's hook while it is asked. `FakeFiller`
can be told to send pieces to an address through its bounds' sender before it paints,
following an answer that points elsewhere 8 times at most.

**Limits, each stated.**

- **The gate's place is not built.** The plan gives the policy to "the gate's place in
  I3's order", which refuses a request before its backend is called and before the
  meter sees it. Task I3 is not built yet, so a backend that names a recipient is
  still called under the default policy: what it cannot do is send. Task I3 gives its
  gate this policy (`docs/carry-over.md`), and with it decides what classes a
  capability's request holds before any piece is made.
- **Nothing here probes or downloads.** The policy answers whether a recipient may be
  probed and nothing in the executor asks it yet: the checked probe and the checked
  download are task E3's.
- **`Hop::moves` counts answers in a row that pointed elsewhere; an answered request
  of the same backend between two of them sets it back to nothing; the total for the
  run does not go back** (`Hop::total_moves`). A request that was refused or that
  failed sets neither back
  (`send::a_refusal_between_two_hops_does_not_set_the_moves_back_and_the_total_never_goes_back`).
  Both are counted for one backend of one run, not for one request: a run that sends
  through one handle from two threads at once has one count for both.
- **A send to an origin that holds a secret of a later config is refused only once
  the executor knows that secret.** The executor's secrets are those it was built
  with until task I6 gives it those of each snapshot of the config.
- **A backend that opens a socket of its own is not stopped,** as ADR 0012 says:
  adapters are trusted code.
- **The fault a backend is answered with is the backend's to hand on.** A backend that
  swallows it and answers all the same has still sent nothing.
- **Until the wiring (task W1) the binary's executor has no ways out.**

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
- `Bounds` hands a backend its sender, and `Engine` holds the ways out. The egress
  strand implements `HopCheck` and builds a `DispatchPolicy` from a config, and edits
  neither the executor nor a handle. Nothing stored changes.
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
- **A sender given to a backend when it is built.** Turned down: a sender that notes
  where a run's data went belongs to one run, and a backend is shared by all of them.
- **The check alone, with the policy and the recipient's origins left to it.** Turned
  down: a check that a strand or a test plugs in could then approve an origin the
  recipient does not reach, or a recipient the policy never named. The two questions
  the executor can ask itself it asks itself, before the check.
- **The checked sender follows a redirect itself.** Turned down for now: ADR 0012 hands
  a redirect back as an address, and a backend that goes on to it is checked there.
