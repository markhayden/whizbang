# 0015. The executor's interfaces: what a strand plugs into, one section for each

Status: accepted. Changes ADR 0007 ("Admission": the executor's parts are asked in the
admission's step; "Alternatives": a store is recovered when the next writer opens it,
which it turned down for now; "A preparation handed to admission": a preparation carries the time it
was made, and a request may carry a cap, and `ConfigSource` answers a snapshot of the
config or a refusal, where it answered a digest), ADR 0009 (a call that resolved a model
its snapshot does not hold is refused) and ADR 0011 (a kind makes a backend), and names
ADR 0004 (`prepared/1` is as it
was) and ADR 0013 (the kinds a record has for a refused call). Changed by 0019: a
check is shown the pieces a request holds (`Hop::pieces`) and the moves of one request
(`Hop::request_moves`), counted in the bounds each request is handed; and the fourth
question of a send is asked as `DispatchPolicy::why_not`, which says which part of a
recipient is not as it was approved
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
Amended: 2026-11-14, with task I6. Added: "One snapshot of the config". Changed under
"A policy before a send, and a checked send": the limit on a secret of a later config.
ADR 0011 is amended with it ("Kinds", "What a config offers", "Both files, each time").
Amended: 2026-11-14, after the review of task I6, before it merged. Changed under "One
snapshot of the config": the bound is on what snapshots add; a conflict is never
answered with an id that holds a secret, in place of a refusal that took no second
look; two limits on a secret a later snapshot brings; the digest and a rotated key.
Amended: 2026-11-14, with task E3, its second pull request, as the review of the first
asked. Changed under "A policy before a send, and a checked send": what a check is
shown (`Hop::pieces`, `Hop::request_moves`), and that a third count is one request's;
and under "Consequences", the one line in each handle that the egress strand edited.
Amended: 2026-11-14, with task E4. Changed under "A policy before a send, and a checked
send": the fourth question says why (ADR 0019, "The run's recipients").
Amended: 2026-11-14, with the first half of task I7. Added: "An inventory". The
lease, the other half of the task, waits for the wrappers of task I3 and is not here.
Amended: 2026-11-15, with task I3. Added: "A wrapper around each request", with the
gate in its place and what a request is said to hold before any piece is made.
Changed under "A policy before a send, and a checked send" and "An inventory": what
waited for that task.
Amended: 2026-11-15, with task S3. Changed under "One snapshot of the config", "What
waits": the part that holds a call to the cap is built.
Amended: 2026-11-15, with task S8. Changed under "A part in the admission", "Limits":
the kinds the two parts that refuse are recorded under. And, as the review of task S3
asked, under "One snapshot of the config": a snapshot prints whether it holds a cap,
and not the cap.
Amended: 2026-11-15, after the review of task I3. Changed under "A wrapper around
each request": a panic of `Open::after` abandons the run; the tests of a judge and a
segmenter through the places, of a repeat of an abandoned run's request, and of the
gate's sentence; and why the path without a snapshot is left as it is.
Amended: 2026-11-15, with task I4. Added: "One way to a writer, with recovery before
it". ADR 0007 is amended with it (the alternative it turned down for now).
Amended: 2026-11-15, with task I4b. Added: "The interrupted record, and what readers
say of it". ADR 0019 is amended with it ("The run's recipients": the limit that
waited for the recovery).
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
  kinds, so a part chooses one of them, and its own name is in the sentence only. The
  two parts that refuse each found one, and say why it fits where they are written:
  the cap of one call is `over_allowance`, or `invalid` for a cap that is none (task
  S3, `crates/whizbang-core/src/spend/cap.rs`), and the age of a preparation is
  `stale` (task S8, `spend/age.rs`); each has a test of a call from inside an action
  that it refused, in its caller's record. A part that finds no kind that fits needs
  a new version of the record.
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
   (`Outgoing::needs`). It is asked as `DispatchPolicy::why_not`, which answers
   nothing exactly when `may_receive` says yes, and otherwise which part of the
   recipient is not as it was approved: its account, its upstreams or the classes
   (ADR 0019, "The run's recipients"). **A piece's class is what whoever built the piece stated,**
   which is ADR 0012's rule and is not second-guessed here: nothing at a send knows a
   source image from a derived one, so a piece that states no class needs all four
   approved (`send::a_piece_with_no_class_needs_every_class_approved`).
5. **The check approves the address** (`HopCheck::look`). It is shown a `Hop`: the
   recipient, the address, the classes, the policy, how many answers in a row just
   before pointed elsewhere (`Hop::moves`), the pieces the request holds
   (`Hop::pieces`) and how many answers pointed this one request elsewhere
   (`Hop::request_moves`), which is what a limit on a chain of redirects is held with
   (ADR 0019, "Every hop"). The default, `NoHop`, approves no address
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

- **The gate's place was not built with this section, and is now.** The plan gives the
  policy to "the gate's place in I3's order", which refuses a request before its
  backend is called and before the meter sees it. Until task I3 a backend that named a
  recipient was still called under the default policy, and what it could not do was
  send. "A wrapper around each request", below, has the gate and what a request is
  said to hold before any piece is made.
- **Nothing here probes or downloads.** The policy answers whether a recipient may be
  probed and nothing in the executor asks it yet: the checked probe and the checked
  download are task E3's.
- **`Hop::moves` counts answers in a row that pointed elsewhere; an answered request
  of the same backend between two of them sets it back to nothing; the total for the
  run does not go back** (`Hop::total_moves`). A request that was refused or that
  failed sets neither back
  (`send::a_refusal_between_two_hops_does_not_set_the_moves_back_and_the_total_never_goes_back`).
  Both are counted for one backend of one run, not for one request: a run that sends
  through one handle from two threads at once has one count for both. A third count
  is one request's (`Hop::request_moves`, ADR 0019), kept in the bounds that request
  was handed.
- **A send to an origin that holds a secret of a later config is refused only once
  the executor knows that secret.** A run's place for facts is made with every secret
  the executor knew when the run was admitted, those of each snapshot it had taken
  among them ("One snapshot of the config"); a secret a later snapshot brings is not
  known to a run that was admitted before.
- **A backend that opens a socket of its own is not stopped,** as ADR 0012 says:
  adapters are trusted code.
- **The fault a backend is answered with is the backend's to hand on.** A backend that
  swallows it and answers all the same has still sent nothing.
- **Until the wiring (task W1) the binary's executor has no ways out.**

### One snapshot of the config

Task I6. The code is `crates/whizbang-core/src/contract/snapshot.rs`, with the
executor's side in `engine.rs` (`take`, `under`, `admit`, `run_and_end`), `part.rs`
(`Asking::snapshot`) and `egress/checked.rs` (`Ways::behind`); the config's side is
`config/kinds.rs` (`Kind::make`), `config/offers.rs` and `config/settings.rs`; the fake
is `FakeSource` in `crates/whizbang-testkit/src/source.rs`, and `FakeKind` makes fake
backends. The tests are in `crates/whizbang-core/tests/it/contract/snapshot.rs`, named
below as `snapshot::…`, and in `tests/it/config/roster.rs`, as `roster::…`. **The engine's
change was written before most of these tests, not after them;** each decision below was
then held by changing the code and seeing a test fail (the pull request lists the
changes tried).

ADR 0007 gave the executor a `ConfigSource` that answered a digest. A digest says that
the config changed and holds nothing of it. The strands need what the digest is of: the
backends, the cap of one call, the policy, the secrets and the settings a run records,
all of one reading.

**`ConfigSource` has one question, `snapshot()`, and answers a snapshot or a refusal.** A
`Snapshot` is the config's digest together with what the executor uses of the config:

| A snapshot holds | Given by | Used |
|---|---|---|
| The digest | `Snapshot::of` | In the fingerprint of every call prepared under it (ADR 0004), and so in `Stale::Config` |
| The backends | `offering`, an `Offered` | Prepare asks them (`Backends`); a run is handed the one its call resolved |
| The cap of one call | `capped_at` | Shown to the admission's parts; the executor holds no call to it |
| The policy and the credentials | `sending_under`, `with_credential` | Behind the executor's sender and check, for the runs admitted under it |
| The secrets | `with_secrets` | Kept by the executor from then on |
| What makes a run's settings | `recording`, a `Settings` | Made and held to a record's rules at admission, noted when the run starts |

**A digest alone is still a source,** of a snapshot that holds nothing else, and a
snapshot is a source of itself. Under a snapshot that holds no backends the executor
asks and hands over those it was built with (`Engine::over`,
`Engine::with_capabilities`); under one that holds neither a policy nor a credential a
run sends through the ways the executor was given (`Engine::with_ways`); and with no
`Settings` a run records none. So the binary and the tests of checkpoint 1A are as they
were (`snapshot::a_digest_alone_is_a_source_of_a_snapshot_that_holds_nothing_else`).
One helper of `admit.rs`, a source of its own that implemented `digest()`, implements
`snapshot()` instead: three lines, and no test of that file changed.

**When the source is asked.** Once for every call that is prepared
(`Executor::prepare`), and once for every admission of a call that nothing called,
before anything is held for it. The call is prepared under that snapshot, every part of
the admission is shown it at every attempt (`Asking::snapshot`), and the run executes
under it: the source is not asked a second time for one admission
(`snapshot::a_snapshot_is_taken_once_for_an_admission_and_every_part_is_shown_it`). A
source swapped between a preparation and its admission is `Stale::Config`, as ADR 0007
has it (`snapshot::a_source_swapped_between_prepare_and_admission_is_a_stale_config`).

**A run goes on under the snapshot it was admitted with.** The plan's words: a run
"resolves a backend, and is held to a cap and a policy, of that snapshot though the
source was swapped while it ran". The snapshot, the backends made of it for the run and
the settings are carried with the admitted run, in memory, and nowhere else. The run is
handed that snapshot's backend, sends under its policy with its credential, and its
record names its digest, whatever the source answers by then; the next run is admitted
under what the source answers then
(`snapshot::a_run_goes_on_under_the_snapshot_it_was_admitted_with_though_the_source_was_swapped_while_it_ran`).

**A call from inside an action is prepared and admitted under its caller's snapshot,
not a fresh one.** The plan says of the cap that "a nested call is held to the cap
too", and of a run that it goes on under its snapshot; a tree of runs under two configs
would be held to two caps and two policies. The executor in a run's context is this
executor with the run's snapshot as its source (`Engine::under`), so the source is not
asked for a call from inside an action, a part is shown the caller's snapshot, and
`nested.rs` is not changed. A source that refuses while a run executes does not stop
what the run calls
(`snapshot::a_call_from_inside_an_action_is_prepared_under_its_callers_snapshot_too`),
and a run and the run it calls are both refused what their snapshot's policy refuses,
though the source approves it by then
(`snapshot::a_run_and_the_run_it_calls_are_refused_what_their_snapshot_refuses_though_the_source_now_approves_it`).

**A source that refuses is a failure, not a refusal of the call.** It is
`CallError::Config`, with a sentence: the call was not looked at under any config, so
nothing was judged, no run is made, a request's id is not taken, and the next call asks
the source again
(`snapshot::a_source_that_refuses_fails_the_call_with_no_run_and_the_call_after_it_is_put_right_is_admitted`).
As a part's failure and the ledger's, it is returned without a second look for the id
(`snapshot::a_sources_refusal_is_a_failure_and_is_returned_without_a_second_look_for_the_id`).

**A source's reason is fixed words** (`SourceRefused::because` takes a `&'static str`
and nothing else), and names a file as "the config" or "the env file": no key, no value,
no path. The type does not hold that against a source that leaks a made text, so the
executor looks at the whole reason first: one that holds a secret it knows is replaced
with fixed words (`SOURCE_WHY_WITHHELD`), and any other is cut at 256 characters
(`SOURCE_WHY_LIMIT`). The cut comes after the look and adds nothing, so what is left of
a reason that held no secret holds none
(`snapshot::a_sources_reason_that_holds_a_secret_is_replaced_and_a_long_one_is_cut`).
**So a call does not name the key of the file that was wrong.** Task C4's "Done when"
asked for the key, escaped and cut; that is the config reader's own refusal
(`ConfigRefused`, ADR 0011), shown by whatever loads a config directly (`Files::load`),
and the task's words were changed when its source was built.

**A source, a kind or a `Settings` that panics** unwinds to its caller. The snapshot is
taken before anything is held, and the settings are made before anything is taken or
staged for the call, so the store is as it was and publishes for the next, and the same request is
admitted afterwards under its id
(`snapshot::a_source_that_panics_leaves_the_store_as_it_was_and_the_next_request_is_served`,
`snapshot::a_kind_that_panics_while_a_snapshot_is_made_leaves_nothing_behind_and_the_next_request_is_served`,
`snapshot::settings_a_record_would_not_take_fail_the_admission_with_no_run`).

**The secrets are kept for as long as the executor lives.** The executor holds one set,
shared by every clone of it: those it was built with (`Engine::with_secrets`) and those
of every snapshot it has taken, for a preparation as for an admission. None is ever let
go, so a key that was rotated is still taken out of what a later run writes
(`snapshot::a_secret_of_an_earlier_snapshot_is_still_redacted_from_a_later_runs_record`).

- **A run writes without every secret known at its admission:** the union as it was
  when its snapshot was taken. Its admission, its journal, its diagnostics, its record,
  the refusals of the calls it made and its place for facts (ADR 0013) are all made
  with that one set
  (`snapshot::the_keys_of_two_snapshots_are_both_taken_out_of_everything_a_later_run_writes`).
  So an origin that holds a secret of an earlier snapshot cannot be listed, and is not
  sent to
  (`snapshot::an_origin_that_holds_a_secret_of_an_earlier_snapshot_is_not_sent_to_and_is_in_no_file`).
- **The set is bounded at 4,096** (`SECRETS_KEPT_LIMIT`). A snapshot that would bring it
  over is not taken: the call fails as `CallError::Config`, nothing of that snapshot is
  kept, and no earlier secret is dropped to make room. The look and the keeping are one
  step under the set's lock
  (`snapshot::a_snapshot_that_would_bring_more_secrets_than_an_executor_keeps_is_not_taken`).
  **The bound is on what snapshots add.** The secrets an executor is built with are
  not bounded, and all of them are kept: a snapshot that adds none is taken however
  many there are, and one that adds any to a set that is, or would be, over the bound
  is not. Nothing is dropped either way, so it cannot fail open
  (`snapshot::an_executor_built_with_more_secrets_than_it_keeps_is_failed_by_no_snapshot_that_brings_none`).
- **A request's id is looked at again** for a secret once its admission's snapshot is
  taken, before the id is written anywhere: the first look was made before the executor
  knew what that snapshot brings
  (`snapshot::a_request_id_that_holds_a_key_its_admissions_snapshot_brings_is_refused_and_written_nowhere`).
  **A conflict is never answered with an id that holds a secret.** A conflict's answer
  names the id, and a look for the id can come after a snapshot brought a key: the
  admission's own, or one a concurrent call took. So wherever a look finds the id taken
  by another request, the id is looked at with the secrets known then, and one that
  holds a secret is refused as such and not shown
  (`snapshot::a_request_id_taken_while_its_admissions_snapshot_was_read_is_not_shown_when_the_snapshot_brings_its_key`,
  `snapshot::a_conflict_does_not_name_an_id_that_holds_a_key_a_concurrent_admission_brought`).
- **A snapshot whose policy names a recipient at an origin that holds a secret is not
  taken.** Every send to that recipient would be refused where it is made ("A policy
  before a send, and a checked send"), so it is said once, as `CallError::Config` in
  fixed words that show no origin (`SECRET_ORIGIN_APPROVED`). The secret may be the
  snapshot's own or an earlier one's; an upstream that holds one is left to the send,
  and is refused where the config is read when the source over the files knows the
  secret (ADR 0011, "The source over both files")
  (`snapshot::a_snapshot_that_approves_a_recipient_whose_origin_is_a_secret_is_not_taken`).

**The roster and the backends are made from one thing.** A kind no longer declares a
backend: it makes one (`Kind::make`, ADR 0011), and what the roster offers of that
backend is what the backend itself declares (`Made::declaration`). `Offers` keeps each
backend it made under the capability and the name it is offered by, and is the
snapshot's `Offered`: prepare asks it, so a backend that is not ready is answered with
its reason and never by another
(`snapshot::a_backend_that_is_not_ready_is_refused_with_its_reason_and_never_replaced_by_another`,
`roster::what_a_config_offers_holds_each_ready_backend_made_under_its_name_and_no_other`).

- **A run is handed a backend by its name, never by its model:** the name the call
  chose, or else the capability's default. Two backends of one capability that declare
  one model are told apart
  (`snapshot::two_backends_of_one_capability_that_declare_one_model_are_told_apart_by_name`).
- **A call cannot resolve a model its snapshot holds no backend for.** At admission,
  after the call was found ready and before anything is staged, the snapshot is asked
  for the backend under each name the call resolved; one that is not there, that
  declares another model, or that answers another capability, refuses the call as
  `Refusal::NotReady`, with no run
  (`snapshot::a_call_cannot_resolve_a_model_its_snapshot_holds_no_backend_for`). The pin
  of ADR 0009 is turned round:
  `asked::a_call_that_resolved_a_model_its_snapshot_does_not_hold_is_refused_and_not_handed_no_handle`.
  What is left of it, for an executor built with a digest alone, is pinned as
  `asked::known_an_executor_built_with_a_digest_alone_holds_its_backends_apart_from_its_roster`.
- **A backend belongs to its snapshot.** A source that reads a config again makes
  backends again: the runs admitted under one snapshot are answered by its backends
  and by no other's
  (`snapshot::a_backend_made_for_a_snapshot_answers_only_the_runs_admitted_under_it`).
  **The digest does not cover the env file** (ADR 0011, "The digest", where task C4
  decided that this stands): a key that is
  rotated gives a new backend under the same digest, and a call prepared before it is
  not stale
  (`snapshot::known_a_key_rotated_in_the_env_file_gives_a_new_backend_under_the_same_digest_and_no_stale_config`).
- **A kind that makes a backend of another capability** than its table is under makes
  that backend not ready (`NotReady::OtherCapability`)
  (`roster::a_kind_that_makes_a_backend_of_another_capability_makes_its_backend_not_ready`).
- **Two `Offers` are equal when they offer the same:** the roster, each backend that is
  not ready with its reason, and the defaults. The backends they made are not compared.
  A backend has no equality, and each reading makes its own, so two readings of one
  file are equal as values and never share a backend
  (`roster::two_offers_are_equal_when_they_offer_the_same_whatever_backends_they_made`).

**The policy and the credentials of a snapshot** stand behind the executor's sender and
check (`Ways::behind`): a run admitted under the snapshot is held to that policy and
handed those credentials, and the executor's own are not used for it. A snapshot that
was given a credential and no policy sends under the policy that approves nothing
(`snapshot::a_snapshot_prints_no_credential_no_secret_and_no_setting`, its last lines).
The executor's policy and credentials never stand beside a snapshot's: under a snapshot
with a credential alone nothing is sent though the executor's policy approves, and
under one with a policy alone a request goes without the credential the executor holds
(`snapshot::a_snapshots_policy_and_credentials_stand_in_place_of_the_executors_and_never_beside_them`).

**The settings a run records** are asked of the snapshot's `Settings` at admission,
with the names the call resolved and the run's secrets, and each pair is held there to
what a record takes (ADR 0013, "A setting"): at most 96, a name once, a name that holds
no secret, and a value a record writes. One that fails any of these is a failure of the
admission, `CallError::Config`, which names the setting when its name holds no secret
and never its value; nothing is left out in silence and no run is made
(`snapshot::settings_a_record_would_not_take_fail_the_admission_with_no_run`). The
pairs are noted in the run's place for facts when the run starts, and are in its
record (`snapshot::the_settings_of_a_runs_snapshot_are_in_its_record`). A config is a
`Settings` (`Config::settings_of`, ADR 0011).

**What a snapshot prints** is its digest, whether it holds backends, settings and a
cap, its policy as a policy prints itself, and how many secrets it brings
(`snapshot::a_snapshot_prints_no_credential_no_secret_and_no_setting`). The cap's
amount is not printed: the config's reader does not look for a secret in a number
(ADR 0011, "What the reader does not do"), so a cap could spell one.

**The fake.** `FakeSource` answers the snapshot a test gave it, which the test swaps;
refuses or panics when told to; counts how often it was asked; and can run a test's
hook while it is asked.

**What waits.**

- **The source over the config file and its env file is built** (`Files`, ADR 0011,
  "The source over both files"): the digest, the backends, the secrets, the cap, the
  policy and the settings of each reading. It gives a snapshot no credential: a kind
  has no way to hand one over yet (ADR 0011, "What waits").
- **Nothing in the executor holds a call to the cap.** A part is shown it, and task
  S3 built that part (`spend::CallCap`, `crates/whizbang-core/src/spend/cap.rs`, which
  says what it refuses, under which kind of a record, and why no refusal quotes an
  amount; its tests are `cap::…` in `crates/whizbang-core/tests/it/spend/cap.rs`). An
  executor holds calls to the cap only when it is given the part; the binary's is
  given it by its wiring (task W1).
- **The gate's place** is given the policy of the run's snapshot, as the checked
  sender is: both are made of the ways the run executes under ("A wrapper around
  each request").

**Limits, each stated.**

- **The window before the publication is as it was** (ADR 0007, "The window that is
  left"): a config that changes between the snapshot and the publication is not seen
  by that admission. What it leaves is a run under the snapshot it was admitted with,
  which is what is meant.
- **A snapshot's secrets are known only once it is taken.** What a run admitted before
  wrote, or is writing, was cleaned with the secrets known at that run's admission. A
  key that first appears in a later config is not taken out of an earlier run's files,
  nor out of what `status`, `wait` or a repeat later answers of that run.
- **A secret a later snapshot brings is looked for in the origins a running run's
  snapshot approves, when that run makes a call.** If one holds it, the call from
  inside the run fails as `CallError::Config`; the run's own sends are still held to
  the secrets known at its admission.
- **A preparation takes a snapshot's secrets in too,** and may fail on the bound.
- **A backend is made each time a source that reads makes a snapshot,** for a
  preparation as for an admission. A kind's `make` reads nothing and sends nothing
  (ADR 0011), so it is cheap by its own rule; nothing here holds a kind to that.
- **A run's snapshot is in memory alone.** A run a dead process left is recovered from
  its admission (task I4b), which names the digest and not the config.
- **The name of a setting may be longer than a record takes** only from a `Settings`
  that is not a config. The config's reader allowed 155 characters against the
  record's 128, and task C5 held it to the record's bound (ADR 0011, "What the file
  holds"), so no config that is read gives such a name.
- **A reason and a kind's sentence are cleaned by themselves,** as a part's is: a
  secret completed only by words put around it is not found.
- **A run that could not note a setting it was admitted with** would end as an error
  before its action ran. Nothing reaches that: the pairs were held to the same rules,
  with the same secrets, at admission. No test holds it.

### An inventory

Task I7, its first half. The code is `crates/whizbang-core/src/capability/inventory.rs`;
the fake is `FakeInventory` in `crates/whizbang-testkit/src/inventory.rs`. The tests are
in `crates/whizbang-core/tests/it/contract/inventory.rs`, named below as `inventory::…`.
**The lease, the task's other half, is not built:** it stands in a place of task I3's
order, and task I3 is not built ("What waits", below). No file of the executor changed.

A local backend, one that names no recipient (`Declaration::is_local`), answers with a
model that is on this machine or is not, and the models directory (ADR 0017) is where
that is known. Prepare asks `Backends`, which is answered from what is configured. This
section is what stands between the two.

**`Inventory` has one question, `look(model)`,** asked by the name a backend declares
for its model, and two answers (`Installed`):

- **ready, under an installation** (`Installation`): the installation's identity, which
  is a digest and nothing else. Its text is `sha256:` and 64 lower-case hex digits, as
  ADR 0017 writes an installation's identity, so the type has no place for a path, a
  file's name, a time or any text read from a disk
  (`inventory::an_identity_is_a_digest_as_text_and_is_a_version_a_declaration_takes`);
- **not ready, and why** (`Unready`): not installed, out of date, damaged, or not in the
  catalogue. Each is fixed words with nothing in it. **There is no fifth:** an inventory
  that cannot tell, or cannot read what it found, has `Damaged` to say and no way to say
  ready without an identity. What is half installed is not installed or is damaged; which
  of the two is the inventory's own (task M6, ADR 0017).

**A roster over an inventory** (`InstalledRoster::over`) is a `Backends`. It resolves a
call as its roster does (ADR 0006), and what the roster refuses it says as the roster
says it, without asking the inventory
(`inventory::what_the_roster_refuses_is_said_as_the_roster_says_it_and_the_inventory_is_not_asked`).
Then, of the one backend that was picked:

- **One that names a recipient is answered as declared,** and the inventory is not
  asked (`inventory::a_backend_that_names_a_recipient_is_not_asked_of_the_inventory`).
- **A local one is looked up,** by its model's name, and no other model is
  (`inventory::the_inventory_is_asked_about_the_model_of_the_backend_the_call_resolved_and_no_other`).
- **Not ready makes the backend not ready,** with the reason: the call is prepared as
  not ready and is refused at admission as `Refusal::NotReady`, with no run. No other
  backend answers in its place. The sentence names the capability and the backend by
  the name the roster offers it under, and then the reason: nothing of the model
  (`inventory::a_roster_over_the_fake_inventory_prepares_a_model_that_is_not_ready_as_not_ready_with_the_reason`).
- **Ready is answered under the installation's identity, as the model's version:** the
  backend and the model as declared, and the identity's text in place of the version
  the backend declared. That is "the version in what a preparation saw" (the plan; ADR
  0017, "The identity of an installation"), and it is in the preparation's fingerprint
  (`inventory::a_local_model_that_is_ready_is_prepared_under_the_identity_of_its_installation`).
  So an installation replaced, removed or damaged after a call was prepared makes that
  preparation `Stale::Model` at admission, by ADR 0007's rule and with no code of this
  task, and the same installation back makes it good again
  (`inventory::an_installation_replaced_or_removed_under_a_prepared_call_makes_the_preparation_stale`).

**The default, `NoInventory`, knows no model:** every model is not in its catalogue, so
no local backend is ready under it, and a backend that names a recipient is as it was
(`inventory::under_the_default_inventory_no_local_model_is_ready_and_a_backend_that_is_not_local_is_as_it_was`).
**Nothing is put over an inventory unless it is asked for:** a `Roster` alone, what a
config offers (`Offers`) and the binary are as they were, and no test of an earlier
task changed.

**What one request may cost** (`Backends::bound`) is asked with the model as it was
prepared, so a local model comes under its installation's identity, which no
declaration holds. A local backend is therefore matched by its backend and its model,
whatever the version, and the inventory is not asked a second time; a backend that
names a recipient is matched by all three, as a roster matches it. Where two local
backends of one capability declare one backend and one model, the answer is the larger
of theirs: over, never under
(`inventory::a_local_backend_that_states_a_price_is_bound_by_it_under_its_installations_identity`).

**The fake.** `FakeInventory` is told which model is ready, under the installation
whose identity is the digest of the bytes a test gives, and which is not and why; it
can forget a model; and it keeps the name it was asked about, each time. A model it
was told nothing of is not in its catalogue
(`inventory::a_model_the_inventory_was_told_nothing_of_is_not_in_the_catalogue_and_is_never_ready`).

**What waits.**

- **The lease** (the second box of task I7, and the last three parts of its "Done
  when"): a lease on the installation the run was admitted with, taken before a request
  to a local model leaves and let go when it is over. Its place is there, and is
  empty ("A wrapper around each request", `Place::Lease`): what stands in it is not
  built.
- **A run is handed no backend for a local model over an inventory.** An executor hands
  a run the backend that declares the model its call resolved, by backend, model and
  version (ADR 0009, `Capabilities::resolved_for`). The identity of an installation
  is a version no backend declares. So today a call for a local model
  over an inventory is prepared, is ready and is admitted, and its action finds no
  handle; no request reaches a local model that no lease holds
  (`inventory::known_a_run_admitted_for_a_local_model_over_an_inventory_is_handed_no_backend_for_it`).
  The task that builds the lease hands the run its backend, with the identity the
  admission recorded, and turns that pin round (`docs/carry-over.md`).
- **Nothing puts what a config offers over an inventory.** `InstalledRoster` takes a
  `Roster` and is not an `Offered`, so no snapshot of a config holds one, and
  `Snapshot::handed` matches a backend by its version too. `Offers` (ADR 0011) and the
  binary's wiring (task W1) are not changed here.
- **The inventory over the models directory** is task M6's.

**Limits, each stated.**

- **Every local backend is looked up.** A local backend whose model was never one to
  install is not ready over any inventory that does not know it. No backend that ships
  is local yet; the first is checkpoint 1C's.
- **The inventory is asked by the model's name alone.** Two local backends that declare
  one model name are one model to it, whatever backend each declares.
- **An inventory is asked at each preparation and not held still.** Between the answer
  and the admission's publication the installation can change, as the config can (ADR
  0007, "The window that is left"). The lease is what holds a request to the
  installation the admission recorded.
- **An identity is trusted to be an installation's.** The type holds it to a digest; it
  does not know whether an installation of that identity exists.
- **An inventory that panics** unwinds to whoever prepared the call. Prepare holds
  nothing and writes nothing. No test holds it.
- **The sentence is not looked at for a secret here.** It is made of a capability's
  name, a name the roster took, and fixed words, and is handed on as every reason of
  a `Backends` is.

### A wrapper around each request

Task I3. Changes ADR 0006 ("Accounting for requests": what stands between a handle and
its backend) and names ADR 0009, whose tally records each request. Tests named
`around::` are in `crates/whizbang-core/tests/it/contract/around.rs`.

**`Around`, in the core** (`capability::Around`, also at `contract::Around`): what
stands in one place between a handle and its backend. For each request it is told
(`Wrapped`) the run, the run at the root of its tree, the capability, the backend's
declaration and its quote, the classes of data the request is said to hold, and the
run's allowance and place for facts
(`around::three_wrappers_see_a_request_in_the_stated_order_and_none_sees_one_the_handles_own_checks_refused`,
`around::a_wrapper_is_told_the_quote_the_classes_and_the_allowance_of_the_run`).
`Around::before` is called before the request leaves. It may refuse, with a `Stop`: a
fault, which is marked as not sent whatever it said, and a refusal to note, of one of
the four kinds a record has for a request that was not sent (ADR 0013). Otherwise it
answers with what it holds open for the request (`Open`), which is handed how the
request ended, once: the charge an answer stated, or the fault (`Ended`).

**The order is fixed** where a handle sends (`Bounds::send`; a context puts the
wrappers on its handles last, in `Context::new`): the handle's own checks (ADR 0006),
then the tally, then the gate, the lease and the meter, then the backend; and back
through the meter, the lease and the gate before the tally accounts for the request.
So a request is outstanding, and its run and every run above it not terminal, until
the last wrapper has returned
(`around::with_a_wrapper_stopped_after_the_backend_answered_the_run_and_its_parent_are_not_terminal`).
A filler and a segmenter send through `Bounds::send`; a judge's handle counts and
accounts for its request itself, to keep the exchange, and takes it through the same
places by `Bounds::through`. Each of the three is shown passing the places in order,
and stopped before its backend by a gate that refuses
(`around::a_segmentation_and_a_judge_request_each_pass_the_three_places_in_order`,
`around::a_gate_that_refuses_stops_a_segmentation_and_a_judge_request_before_its_backend`;
the first test of this section is a filler's).
A request the handle's own checks refuse is seen by no wrapper. A request the tally
refuses, because the run's action has come back or the run has sent as many as it may,
is seen by none either.

**A refusal.** A wrapper that refuses stops the request before the next place sees it
and before the backend. The fault reaches the action; the refusal is among the run's
refusals, naming no action; and the places before it, which had let the request
through, are told that it did not leave
(`around::a_wrapper_that_refuses_stops_the_request_before_the_next_sees_it_and_before_the_backend`).
The request is in the run's list of requests all the same, as one that ended in that
fault: the tally counted it before any wrapper saw it.

**A failure is not a refusal, and does not account for a request as free.** Two things
leave a request that may have been charged with nobody to say so: `Open::after` fails
once the backend was called (a settlement that could not be published), and the
backend panics under a wrapper that holds something for the request, so that the
wrapper is dropped and never told how the request ended. In both the run is
**abandoned** (ADR 0008, "A run with nothing behind it"): its end is not published,
whatever its action then hands back; its reservation is not closed and nothing of it
is released; and its caller is handed `CallError::Abandoned`, with the failure. The
tally lets go of the request only so that the run can reach that state: no answer of
the request is handed to the action, which is told the request could not be accounted
for, and nothing more is sent for the run
(`around::a_wrapper_that_fails_after_the_backend_answered_leaves_the_run_abandoned_and_not_ended`,
`around::a_backend_that_panics_under_a_wrapper_leaves_the_run_abandoned_and_not_ended`).
A **panic** of `Open::after` comes to the same as its failure: it is caught where the
wrappers are told, the run is marked to be abandoned exactly as for a failure, the
places before it are told how the request ended all the same, and the panic then goes
on, as a backend's does under a wrapper that holds something: it unwinds through the
handle to the action, and whatever the action makes of it the run is abandoned, with
no end published and its reservation open
(`around::a_wrapper_that_panics_after_the_backend_answered_leaves_the_run_abandoned_and_not_ended`).
That holds wherever `after` is called, so also where it is told that a later place
refused the request: what the wrapper held is not accounted for either way
(`around::a_wrapper_that_panics_when_told_a_later_place_refused_leaves_the_run_abandoned`).
The request id of an abandoned run stays taken, by the rule every admitted run has
(ADR 0007): the same request again is answered with that run, as abandoned, and never
runs again, and another request under the id is a conflict
(`around::a_repeat_of_the_request_of_an_abandoned_run_is_answered_with_that_run_and_never_runs_again`).
A wrapper says whether it holds anything for a request (`Open::holds`). One whose
work is all before the request holds nothing (`Nothing`), and a backend's panic under
such a one alone is what it was without it: its run ends as an error that says its
action panicked. That is what the gate is.

**Each place is empty by default, but for the gate's.** `Engine::with_wrapper` puts a
wrapper in a place. The lease's and the meter's stay empty until tasks I7 and S5 fill
them. The gate's, when nothing was put there, holds the gate of the policy the run
executes under: the policy of the run's snapshot where the snapshot holds one, and
otherwise the executor's, which approves nothing unless it was told to ("A policy
before a send, and a checked send"; "One snapshot of the config"). A fake wrapper is
in the test kit (`whizbang_testkit::around::FakeWrapper`): it keeps what it saw, and
refuses, fails or stops at a barrier when a test says so.

**The gate.** A request to a backend that names a recipient goes on only when the
policy approves that recipient for the classes the request is said to hold
(`DispatchPolicy::why_not`, the question the checked sender asks). One it refuses
reaches neither the meter's place nor its backend, so the backend's own count of
requests is none and nothing is handed to any sender; the refusal is among the run's
refusals and in its list of where data went, as a send that was refused; and the
fault states no charge
(`around::with_the_default_policy_a_backend_that_names_a_recipient_is_refused_at_the_gates_place_and_one_that_names_none_is_sent`;
through a config's policy, `gate::a_backend_that_is_refused_at_the_gates_place_is_not_called`).
A backend that names no recipient is local: the gate lets its requests by, and what
such a backend tries to send is refused by its sender, as before. The sentence the
refusal is noted with states the classes the request is known to hold, and where
that is none it says "none known before a piece of it is made": the backend was not
called, so what its pieces would have held is not known. Where the refusal could not
be listed in where the run's data went (the list keeps half its room for what was
sent), the sentence says so, as the checked sender's does
(`around::the_gates_refusal_says_so_when_it_could_not_be_listed_in_where_the_runs_data_went`).

**What a request is said to hold, before any piece is made.** ADR 0012 left open who
knows a source image from a derived one at a handle. Nobody does: a `Picture` has no
class. So the handle states what is certain and no more: **a prompt, where the
request holds one** (a fill with a prompt, a segmentation with a phrase, every
request to a judge), and nothing for a picture. The gate is therefore the first of
two checks and never the only one: it refuses a recipient that no approval names, or
that is not approved for prompts when the request holds one, before anything is
built; and every piece the backend then makes is held to the policy again, with the
class its builder states, by the checked sender. A recipient approved for prompts
and not for source images is still refused a source image there, after its backend
was called: sent a picture and no prompt, the backend is called once, nothing is
handed to the sender, and the refusal names the images
(`gate::a_recipient_approved_for_prompts_only_that_is_sent_a_picture_and_no_prompt_is_refused_after_its_backend_was_called`).

**The bound of a request is its own backend's.** `Wrapped::quote` is the quote of the
backend the handle holds, which is the one the call resolved. Two backends of one
capability that declare one model are told apart there, where `Roster::bound`, which
is asked by a model, answers the largest of theirs. That answer is what an admission
reserves, and it is kept: reserving the largest cannot reserve too little
(`around::the_bound_a_request_is_held_to_is_its_own_backends_and_an_admission_reserves_the_largest`).

**Limits, each stated.**

- **Nothing stands in the lease's or the meter's place.** A paid request is not
  debited, recorded before it leaves or settled until task S5; and a request to a
  local model takes no lease until the rest of task I7.
- **A wrapper that panics in `Around::before`** unwinds as a backend's panic does
  under no wrapper: it is the fault of one action; what the places before it held is
  dropped, and nothing here marks the run. (A panic of `Open::after` abandons the
  run, as above.)
- **A refused request is in the run's list of requests,** as one that ended in a
  fault that says it was not sent, beside its refusal.
- **An executor that was built with held backends and no snapshot hands a run no
  backend for a capability none of them answers,** and admits the call: its action
  finds no handle. The path through a snapshot refuses such a call as not ready
  (`Snapshot::handed`). It is left so for now: an executor built by hand is a test's,
  many of whose actions need a capability and never ask for it. The binary's executor
  is on this path too, today: it is built with no backends at all
  (`crates/whizbang/src/wiring.rs`, `NoBackends`) and no snapshot. That it goes through
  a snapshot is task W1's plan and not yet so; `docs/carry-over.md` has the row that
  holds the binary to the snapshot path, with a test that a call with no backend to
  hand is refused as not ready.
- **The gate knows a prompt and no other class,** as above.

### One way to a writer, with recovery before it

Task I4. Changes ADR 0007, which turned down marking an interrupted run "when the next
writer opens the store" for now, as a write on open by whatever command opened it:
the plan of checkpoint 1B decides it the other way ("What a crashed run is recovered
to"), and this is where the write on open is made. Tests named `recover::` are in
`crates/whizbang-core/tests/it/contract/recover.rs`.

**Recovery is lossy and pessimistic by intent** (the plan, "Decisions"). It
reconstructs nothing of what a dead process did: it ends what was left and counts
what may have been spent at the most it can have been. What makes that cheap is that
it is one publication of names made once, by parts that each look at what is there
and add only what is missing, so that a crash inside it leaves it not done, and done
twice it changes nothing. This section is the frame; what is recovered is each
part's (the interrupted run: task I4b; the money: the spend strand's, if it keeps a
part in the recovery).

**One entry point opens a store to write** (`contract::Opening`, over `Store::open`
and its two siblings, which it mirrors: `open`, `open_in_existing_parent`,
`open_existing`). The store module names nothing above it: the entry point is the
contract's. Opening takes the writer lock and recovers nothing. Whoever then asks for
the writer (`Opening::writer`) is answered once recovery is over, and the first to
ask runs it, on its own thread, before anything is handed out
(`recover::a_fake_parts_records_are_published_before_the_writer_is_handed_out`).

**Recovery is a list of parts** (`RecoveryPart`, given in a `Recovery`, with the clock
that tells the time of the recovery). Each is asked once for a store value, in the
order given, and shown (`Recovering`) the generation the store was found at, a way to
read its entries, whether a run is open in this process, and the time. A part adds
names to the one publication of the recovery: outside runs (`Recovering::make`, which
is the store's `create`) and in a run the store holds (`Recovering::make_in`). A name
is made once: one the generation holds, or that a part made earlier in the same
recovery, is refused, and so is a name in a run the store does not hold, and each
fails the recovery
(`recover::a_part_that_does_not_keep_to_a_name_made_once_fails_the_recovery`). With
nothing added nothing is published; and with no part the store opens as before, but
for the one sync below
(`recover::with_no_part_recovery_publishes_nothing_and_the_store_opens_as_before`,
`recover::recovery_run_twice_changes_nothing_the_second_time`).

**All of the parts' records, or none.** They are one publication, so the store's
rule holds for them: refusing each call of the file system in turn, the store is left
with none of any part's records, or, where the refused call was the last sync, with
all of them
(`recover::all_of_the_parts_records_are_published_or_none_shown_by_refusing_each_call_of_the_publication`).

**An open confirms the generation it finds before it is ready.** The first step of
every recovery is a sync of the store's root (`Store::confirm`), which is the step
that makes a pointer durable: the publication that made the generation may have been
visible and not confirmed (the store's `Unacknowledged`), in a process that then
died, and nothing is built on it before it is durable. A store that cannot be
confirmed hands out no writer, and no part is asked
(`recover::an_open_that_cannot_confirm_the_generation_it_finds_hands_out_no_writer_and_asks_no_part`).

**Three states,** shared by every clone of an `Opening`, which is what "the same
store value" is: initializing, ready, failed. Whoever asks while it is initializing
waits for the answer, and everyone is answered alike from then on: two executors
built at once are both handed the one writer after one recovery, and a third built
while it runs waits
(`recover::two_executors_built_at_once_over_one_store_value_are_both_answered_after_one_recovery_and_a_third_built_while_it_runs_waits`,
`recover::whoever_waits_for_a_recovery_that_fails_is_told_what_the_one_who_ran_it_is_told`).
An executor is built over the writer (`Engine::over`), so building one is asking for
it.

**A recovery that fails** hands out no writer, to anyone, for anything. The store
value is dropped, which lets the lock go, though the opening lives; the store can be
read (`Opening::reader`, which opens it as any reader does); and the failure
(`NoWriter`) is said without a path, in one of two ways:

- **nothing published** (`NothingPublished`), when it failed before its publication
  was visible: a part failed (`RecoveryFailed`: a sentence, cut to 256 characters; a
  failure of the store that a part passes on is taken in its words without paths), a
  part panicked, the generation found could not be confirmed, or the store refused a
  step of the publication. The store is as it was found
  (`recover::after_a_failed_recovery_nobody_is_handed_a_writer_the_reason_is_said_without_a_path_and_a_reader_works`,
  `recover::a_part_that_panics_is_a_failed_recovery`: what the panic carried is not
  said);
- **published and not confirmed** (`NotConfirmed`), when the last sync failed: the
  whole generation is there and stays. The next open confirms it, finds every name
  of the recovery made, publishes nothing more and makes nothing twice
  (`recover::a_recovery_whose_last_sync_fails_leaves_no_writer_and_the_next_open_confirms_what_is_there_and_publishes_nothing_more`).

**At the store's limit on a generation file** a publication that adds names is
refused, recovery's among them (`store::Error::GenerationTooLarge`). That is a fourth
answer, beside ready: the store is kept, and its writer is handed out for deleting
and pruning alone (`Opening::remover`, a `Remover`: `snapshot`, `delete`, `prune`),
which make the file smaller; `Opening::writer` is refused and told why
(`NoWriter::AtTheLimit`), so no executor is built and nothing is admitted. After a
deletion made room the next open recovers
(`recover::with_the_limit_set_low_a_recovery_that_cannot_be_published_leaves_deleting_and_pruning_working_and_an_admission_refused_and_after_a_deletion_the_next_open_recovers`).

**Every writing command of the binary opens through it,** in one place
(`wiring::opening`, in `crates/whizbang/src/wiring.rs`, with the parts of this build
from `wiring::recovery`): the executor of `rascal grow`, `store delete`, `store
prune` and `store import`. `whizbang spend` writes nothing yet. A test holds the
binary's sources to that one place: no file of the binary calls an open of
`Store`, and none but the wiring an open of `Opening`
(`writer::the_binary_opens_a_store_to_write_in_one_place_and_through_the_entry_point`,
in the binary's tests). `store delete` and `store prune` ask for the remover, and
the others for the writer. Each command, run on a store a killed process left,
recovers before it changes anything; after a recovery that failed each is refused
with the reason and a reading command works; and at the limit `store delete` and
`store prune` work, and the others are refused and told why. These are unit tests of
the binary (`crates/whizbang/src/recovering.rs`), which call each command as `main`
does, with a fake part in the recovery where the binary has its own list:
`recovering::store_delete_on_a_store_a_killed_process_left_recovers_before_it_deletes`,
`recovering::store_prune_on_a_store_a_killed_process_left_recovers_before_it_prunes`,
`recovering::store_import_on_a_store_a_killed_process_left_recovers_before_it_imports`,
`recovering::rascal_grow_on_a_store_a_killed_process_left_recovers_before_its_run_is_admitted`,
`recovering::after_a_failed_recovery_every_writing_command_is_refused_with_the_reason_and_a_reading_one_works`,
`recovering::at_the_generation_limit_store_delete_and_prune_work_and_every_other_writing_command_is_refused_and_told_why`.

**A crash inside a recovery.** Power lost before each step of a recovery by two
parts, with every way the test kit's disk can lose what was not yet durable, leaves
the store as it was found or with all of both parts' records; the next open recovers
what is not, and another open after it publishes nothing: each record is there once
and whole (`recover::a_crash_at_any_event_of_a_recovery_followed_by_another_open_leaves_the_parts_records_once_and_whole_part_1_of_4`
to `…_part_4_of_4`, in the group of crash sweeps).

**Limits, each stated.**

- **Nothing holds a caller to the entry point but the binary's test:**
  `Store::open` is public, and a test or a tool that
  opens a store with it is handed a writer with no recovery. The test reads the
  binary's own sources, line by line, for the calls by their names: a call made
  through another name for the type is not found, and the crates the binary calls
  are not read.
- **What the scan of the binary's sources does not find.** It refuses the names
  `Store::open`, `Store>::open` and `Store ::` in a line of code, a `Store` brought
  in under another name, and a glob of the store module. A call through a type
  alias, a trait or a function pointer made elsewhere is not found.
- **`store import --create` on a store that cannot be recovered** says "created a
  store at …" inside its complaint, on purpose: the store was made, by this
  process, before its recovery failed, and the caller is told so with the reason.
- **Tests and the test kit may open a store directly** (`Store::open`): the scan
  reads the binary's sources and nothing else, and a test that builds a store by
  hand, or that wants a writer with no recovery, is not held to the entry point.
- **`Recovering::open_here` is always false through the entry point, today:** a
  recovery runs before this process is handed the writer, so it has made no run.
- **What a command says of a refusal.** A recovery that failed is said in
  `NoWriter`'s words, which hold no path. A store that could not be opened at all
  is said in the store's own sentence, as before this task, which names the store
  by the path the caller gave and is escaped where the binary prints it (ADR 0008,
  "The first door"); of a store that is held the command adds that the holder is
  another opening of the store, in another process or in this one, since a second
  opening in one process is refused like any second writer
  (`recovering::a_store_another_opening_holds_refuses_every_writing_command_in_words_true_of_this_process_and_of_another`).
- **The binary's list of parts** (`wiring::recovery`) held nothing when this task
  was built; task I4b gave it the part that ends an interrupted run.
- **The writer that is handed out is the store,** whole: nothing keeps whoever holds
  it from publishing anything. `Remover` narrows it for the one case that needs it.
- **Recovery runs on the thread of the first to ask,** and is not cancelled: a part
  that never returns is waited for by everyone who asks.
- **Recovery runs once for a store value.** A run this process abandons afterwards
  is left as it is until the process is gone and the next open recovers it (the
  plan, "A run abandoned in a live process").
- **A failed recovery is not tried again by the same opening.** Whoever wants to
  try again opens the store again.
- **A part's sentence is the part's.** A failure of the store that it passes on has
  no path; what it writes itself is cut and not otherwise changed, and a door
  escapes it when it prints it.

### The interrupted record, and what readers say of it

Task I4b. The record is ADR 0013's ("What is lost", `RunRecord::interrupted`); this
is who makes it and when. Tests named `interrupted::` are in
`crates/whizbang-core/tests/it/contract/interrupted.rs`.

**A part of the recovery** (`contract::Interrupted`, a `RecoveryPart`) ends every run
that was admitted, has no record and is not open in this process: a run with the name
`admitted` and without the name `record`, under an id this build gives a run. It adds
the run's record to the recovery's one publication, among the run's own names, and
reconstructs nothing: the record is made from the run's admission alone, and is the
bytes `RunRecord::interrupted` makes of it, with the time of the recovery as the
run's end and the version that recovers as the version
(`interrupted::a_run_a_killed_process_left_is_ended_after_the_next_open_to_write_with_a_record_that_holds_exactly_the_admissions_facts_and_names_every_other_group_as_lost`,
which holds the record to the schema of `run/3`). A child and its parent, both left,
are each ended from its own admission
(`interrupted::a_child_and_its_parent_both_left_are_both_ended_each_from_its_own_admission`).

**For a request with an id, its ending is made** (`requests/<id>/ended`, ADR 0007),
in the same publication: an error, at the time of the recovery. It is made only
where the id's own record is the run's admission, by the digest of its content, as a
look-up holds a run to its id; and not where the id has an ending already.

**A run whose admission cannot be read is ended with what its name gives:** its id,
and `admission` among what is lost. That covers content that is no admission, the
admission of another run, one over the bound of a request's record, content that is
gone from the disk, and an admission that reads and would make a record its own
reader refuses. It is never a failure of the recovery: a store with one damaged run
would have no writer at all. No ending is made for it, since the id was in the
admission; the id stays taken, and the same request and a different request under
it are each answered with the error of a record that is not read
(`CallError::Record`), never as a repeat and never as free. Nothing the recovery
writes or says holds anything of the admission, a marker secret planted in it among
it
(`interrupted::a_run_with_a_damaged_admission_is_ended_from_its_name_alone_and_names_the_admission_as_lost_and_a_marker_secret_in_it_is_in_nothing_the_recovery_wrote`,
`interrupted::a_run_whose_admissions_content_is_gone_from_the_disk_is_ended_from_its_name_alone`).

**What is left alone.** A run that has its record, ended by itself or by an earlier
recovery, so that a recovery run twice makes nothing the second time; a run with no
admission, which other hands made; and a run under an id that is no id of a run
(`interrupted::a_run_that_ended_and_a_run_that_was_never_admitted_are_left_as_they_are_and_recovery_run_twice_changes_nothing`).
And a run of this process: recovery runs once for a store value, before this process
is handed the writer and so before it makes a run, and is not run again by an
opening that is asked again. A run that executes here goes on and ends by itself; a
run this process abandoned stays admitted until the process is gone, and the next
open ends it
(`interrupted::a_run_that_is_executing_in_this_process_and_one_this_process_abandoned_are_left_alone`).
The part also asks the store whether a run is open here (`Recovering::open_here`)
before it ends one; nothing reaches that through the entry point, since no run is
open when a recovery runs.

**The money is not this part's.** A run that was left, or that a wrapper's failure
abandoned (above), keeps its reservation as it was: open, counted in full, nothing
released. Closing it, as held unless every dispatch under it is shown settled (ADR
0016), would be the ledger's own part of the recovery, if the spend strand keeps
one. This part makes no name
under `spend/`
(`interrupted::a_run_that_a_wrappers_failure_abandoned_is_ended_at_the_next_open_and_its_reservation_is_left_open`;
`docs/carry-over.md` has the row that names this test for the spend strand).

**Where a run's data went is lost with the rest.** A run that sent data and was then
left is recovered with `egress` named in `outcome.lost` and an empty list, which is
the clause of task E4 that waited for this
(`recipients::a_run_that_a_process_left_after_it_had_sent_data_is_recovered_as_interrupted_with_egress_named_as_lost_and_an_empty_list`,
through an executor and the sending fake).

**The binary's recovery has this part** (`wiring::recovery`), so every writing
command ends what a killed process left before it changes anything
(`runs::a_run_a_killed_process_left_is_ended_by_the_next_writing_command_and_then_shown_as_interrupted`,
in the binary's tests, through the real binary).

**What readers say of such a run** follows from its having a record, and needed no
code in the executor: a run with a record has ended.

- **A repeat of its request** is answered `Ended`, with the error `interrupted`, by
  `request` and by `start`, by an executor that writes the store and by one that
  only reads it; nothing runs, and another request under the id is a conflict.
- **`status` and `wait`** say the same, and `wait` does not wait: the run is in no
  process's table of open runs.
- **`cancel`** answers `NotAccepted(Ended)`
  (`interrupted::a_repeat_of_the_request_of_an_interrupted_run_is_ended_with_the_error_interrupted_and_status_and_wait_say_the_same_and_cancel_is_not_accepted`).
- **Where the admission could not be read,** the questions by the run's id are
  answered so, and a request under its request id with the error of a record that
  is not read
  (`interrupted::the_questions_by_its_id_of_a_run_whose_admission_could_not_be_read_are_answered_as_interrupted`,
  and the test of a damaged admission above).
- **`runs list`** shows it as ended in error, and says after the status that it was
  interrupted and how much is lost: `error, interrupted (everything but what its
  admission says is lost)`, or `error, interrupted (everything is lost, its
  admission too)`, with `unknown` for an action, a start and a duration that the
  admission would have given. **`runs show`** prints the error, the line `lost:`
  with every group, and `unknown` for each group that is lost (ADR 0013). Both go by
  `outcome.lost` and not by the error's kind, so a run whose action called its own
  error `interrupted` is not said to have lost anything; and both print what an
  admission said escaped
  (`runs::runs_list_and_runs_show_show_an_interrupted_run_as_ended_in_error_interrupted_with_what_is_lost_escaped`,
  in the binary's tests, with `runs::runs_show_says_unknown_for_every_group_a_record_has_lost_and_never_none`).

**A crash inside the recovery that ends a run.** Power lost before each step of it,
with every way the test kit's disk can lose what was not yet durable, leaves the run
as it was left or ended with its record and its request's ending both; the next open
ends it if it was not, with the one record its admission makes, and the open after
that publishes nothing: the run is ended once
(`interrupted::a_crash_at_any_event_of_the_recovery_of_an_interrupted_run_followed_by_another_open_ends_the_run_once_part_1_of_4`
to `…_part_4_of_4`, in the group of crash sweeps).

**Limits, each stated.**

- **Interrupted does not say that the process is dead.** A run is ended because no
  record is there and this process did not make it. One writer holds a store at a
  time, so the process that made it no longer holds the store; whether a backend it
  asked is still working, nothing here knows.
- **A run whose record was published and not confirmed** has its record: it ended
  by itself, and is left as it ended.
- **The end is the time of the recovery,** which can be long after the process
  died: `time.end_ms` less `time.start_ms` is not how long the run worked.
- **An admission that cannot be read today is lost for good in the record,** though
  its bytes stay in the store: a read that failed for a passing reason is not tried
  again, since the run then has its record.

## Consequences

- A store is opened to write through `Opening`, and its first sync is the
  confirmation of the generation it found. A strand adds what it recovers as a
  `RecoveryPart` and edits neither the entry point nor the store (task I4b).
  Nothing stored changes by the frame itself.
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
  neither the executor nor a handle, but for the one line in each handle that hands a
  request its bounds (ADR 0019). Nothing stored changes.
- `ConfigSource` has one method, `snapshot`, in place of `digest`: a source written
  against ADR 0007 changes by that method. `CallError` has one more kind, `Config`.
  `Kind::declare` is `Kind::make`. Nothing stored changes: no record, no fingerprint,
  no schema; a run's `asked.settings` is filled where a snapshot gives settings.
- The config strand builds a `ConfigSource` from the file and edits neither the
  executor nor a handle (tasks C4 and C5).
- A roster may stand over an `Inventory`, and a local model is then prepared under
  its installation's identity. The models strand implements `Inventory` (task M6) and
  edits no file of the core. Nothing stored changes.
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
- **A fresh snapshot for a call from inside an action.** Turned down: the plan holds a
  nested call to its run's cap, and a tree of runs would otherwise be judged under two
  configs, with a parent that was approved to send and a child that was not.
- **A source's refusal as a refusal of the call, naming the key that was wrong.**
  Turned down: no call was judged, so nothing is owed a second look for its id; and a
  sentence made from a file a person wrote is where a pasted key is. The reader's own
  refusal names the key, to whoever loads the config.
- **The roster and the backends as two things a snapshot is given.** Turned down: the
  pin of ADR 0009 would stand, for every source that builds them apart.
- **Secrets dropped when a snapshot no longer holds them,** or the oldest dropped at
  the bound. Turned down: a rotated key is still a key in a record until it is revoked
  everywhere, and a bound that drops fails open.
- **The checked sender follows a redirect itself.** Turned down for now: ADR 0012 hands
  a redirect back as an address, and a backend that goes on to it is checked there.
