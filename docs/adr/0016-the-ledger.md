# 0016. The ledger: books of names made once, folded, and never under

Status: accepted. Changes ADR 0007 ("The reservation": the ledger's interface, and what
a ledger's names are) and ADR 0008 ("Allowances": what an allowance holds), and names
ADR 0010 (a charge, a bound and a stage) and ADR 0015 (a part in the admission)
Date: 2026-11-14
Amended: 2026-11-14, after the review round ("Review rounds"): what the fold guarantees
and against what; a guard for a reset; a correction made over the names it saw; the
next turn and place; what lowers a count and what it needs; losses, a second process and
a charge no name states, in the model. And after the second round: one rule in the
place of the rules for each case, that a reservation counts by the ledger's arithmetic
only when its names are a set the ledger could have written; a turn for every
publication that changes what is left, in the epoch it changes; a correction that
names its place and its turn and is sealed over the names it was made over; and what a
run sent, known or not. And after the third: what a frozen reservation counts for
is one sum with no credits, in every epoch, and it stops admissions until the owner
acts (superseded by the owner's choice, in the amendment below this one: it stops
them only where its amount is unknown). And after the hunt over other seeds that followed: a reservation with no hold,
or with more than one, falls under no one epoch when its owner has corrected it
either. And after the fourth: the amount of a reservation whose own hold is lost is
unknown, and the books say so; an epoch that needs its owner takes no dispatch
either; a dispatch is held to what its process holds of the tree; every sum of the
fold stops at the largest number; and the recogniser replays what the ledger
checked. And after the fifth: an admission is confirmed on a reading of the names
made after its publication, and nothing runs under it before. And after the sixth:
a dispatch judged before the store came to need its owner may still be published
once, which is limit (e) and is said there.
Amended: 2026-11-14, with the review of the pull request this record came with ("Review
rounds", the last two entries). Changed, by the owner's choice: a frozen reservation
whose hold states its amount stops nothing else, as the plan decided, where the third
and fourth rounds had it stop every admission and dispatch; what that can cost is
limits (a) and (b), in money. Kept as an exception: the store needs its owner where no
amount can be counted. Changed: the first condition of a confirmation is held as it is
worded, by the model and by the checker; the largest number is no amount at any entry
of the model; a closed reservation has nothing left for a dispatch. Added: limit (g),
what a correction writes off; what a reset does not stop; a request of nothing;
histories 80 to 87; two things more in "What the second part builds", and notes for
whoever builds it.
Amended: 2026-11-15, with the second part (task I2's second pull request). Added:
"The seal, in the core" and "A name of no form"; "The second part, as built", which
names the test of each thing the first part said was to be built, and what was
decided while building it. Changed: the trigger table's first row, and a row for a
dispatch made with its place; the turns exception holds one epoch only (limit (d),
history 88); limit (b) says in its own paragraph that it has no bound.
Amended: 2026-11-15, with task S1b: the real fold in the core, held to the model
history by history ("The real fold"); limit (h), a text that is placed nowhere, with
history 89.
Amended: 2026-11-15, with task S2: the real ledger in the core ("The real ledger");
`confirm` and `settle` have no default; a refusal of its own where the books need
their owner at admission; `Folded::stands` holds the amount to the hold.
Amended: 2026-11-15, by the owner's decision on what cost control is for: the generated
suites were made smaller, to about a quarter ("The invariants, and what holds them",
"The real fold" and "The real ledger" have the sizes). What the code does is as it
was; the counts under "Review rounds" are of the suites as they were then.
Amended: 2026-11-15, by the owner's decision that spend control is a guard and a
record, and that its strand is cut down (the plan, "Decisions", the last item). Added:
"The strand cut down", which says what of this record stays built, what is not built,
the two rules of the fold that change so that a reset closes the books, and what
that costs in money. Nothing else in this record is rewritten: where that section
and an earlier sentence differ, that section is the later decision.
Amended: 2026-11-15, with task W1 (ADR 0015, "The binary wired"). Changed where a
section said what waited for it: the binary's executor has the ledger, the bound of a
call and the meter.
Spec: section 6, "Spend"; section 3, "Storage"

## Context

`SPEC.md` section 6: a paid call reserves its bound when it is admitted; a request is
recorded before it leaves; each attempt is settled; an allowance is reset only by the
owner's command; and what a crash left unsettled is charged at its bound. The plan for
checkpoint 1B decides that balances are a fold of records whose names are made once,
that recovery is pessimistic, that a damaged record freezes one reservation, and that a
person corrects by a new record. It keeps seven invariants ("The ledger's invariants")
and leaves the names and the arithmetic to this record. **That decision stands here
as the plan made it:** every other reservation and every unrelated paid call goes on
beside a frozen one. The review rounds of this record had replaced it by a stricter
rule; the owner chose the plan's, with its cost known ("Invariant 4, and what the
owner chose", below). It left the rest to this record: six review rounds had each
found a way to under-count in names and sums written as prose, so they are settled
here with a model that runs.

What there was: `Ledger::reserve` in the admission's step, with `Step::make` for names
made once (ADR 0007); the executor's `reservation/1` at `spend/reservations/<run>`; an
allowance in memory that only nested calls draw on and that nothing is given back to
(ADR 0008); a charge, a bound and three stages (ADR 0010).

What was read, at the commits `docs/sources.toml` pins: squint's
`crates/squint-core/src/ledger.rs` (`Ledger`, `CapLedger`, `Account`, `Hold`,
`Closing`), an account in memory whose hold is settled to its cost, returns the
difference, and records an overrun without failing; and rascal's
`schemas/rascal/ledger-v1.json`, one file rewritten for each change. Neither is ported:
squint's balances live in memory and rascal's in a file that is replaced, and here
every record is a name that is made once.

**This record comes in two parts, as task I2 does.** The first, which it came with, is
the design, a model of it and a table of histories:
`crates/whizbang-testkit/src/spend.rs`, with its tests in
`crates/whizbang-testkit/tests/it/spend.rs`, named below as `spend::…`. Nothing in the
core is changed by it. The second part built the interface and the records in the
core: "The second part, as built" names what is there and the test of each. **Every
other sentence below about the store, the executor or the core says what the design
is, and only a sentence that names a test says what the head does.** The real fold,
the real ledger, the meter, recovery and the commands are the spend strand's tasks
(S1b to S12), each held to the table; until they merge the one ledger is the test
kit's fake, which keeps the names of this record in a store and judges by the
model's fold. (Since then: the real fold, "The real fold", and a real ledger that
reserves and confirms, "The real ledger".)

## Decision

### What the fold guarantees, and against what

The books are names in a store. Another process, a crash, a restored backup or a
careless hand can leave them in any state: names made that the ledger did not make,
names gone, content that cannot be read. The fold cannot stop a writer that has the
filesystem from destroying the books. What it guarantees is this:

**Whatever names are present, absent or damaged, the fold never counts less than the
most that the well-formed, guarded evidence can mean; and anything that is not fully
guarded evidence can only raise a count or freeze, never lower one.**

**One rule holds it: the fold accepts only what the ledger itself could have
written.** The names of a reservation are *reachable* when they are exactly a set that
some sequence of the ledger's own publications makes, with nothing damaged; "What the
ledger can have written" is the table a recogniser holds them to. A reachable
reservation counts by the ledger's arithmetic. Any other is **frozen**, and **no name
in it is trusted to explain, credit or attribute anything**: it counts for the most its
names can mean, by one sum that has no credits, in every epoch there is; nothing is
dispatched under it; and no settlement, closing or old correction lowers it. Only a
correction the owner makes over it, as it then is, ends that. **It stops nothing
else:** an unrelated call that the rest of the allowance covers is admitted, and
another reservation's requests go out (history 81). The plan decided that, and the
owner chose it again when this record's rounds had decided otherwise ("Invariant 4,
and what the owner chose").

**One case is kept apart: where no amount can be counted at all, the store needs its
owner.** A frozen reservation that has no hold, or several, has an amount no name can
be trusted for; and an epoch whose turns are not exactly those its publications took
has lost or gained a name that changes what is left, and no name says which. With
the first, nothing is admitted and nothing dispatched anywhere in the store until the
owner corrects that reservation. With the second, nothing is admitted in that epoch
until the owner resets, and nothing is dispatched under that epoch's reservations;
a reservation of another epoch still sends ("Epochs" has the table). What was dispatched before is still settled, a run
still ends, recovery still closes what a dead process left, and the owner corrects
and resets. This exception was not in the plan as it was written; the plan now
records it beside its decision, as a case to be put to the owner. A frozen
reservation over-counts heavily, and is the owner's to repair. A reset does not
repair it: only the owner's correction of that reservation does.

**One thing a frozen reservation cannot be counted for is an amount that no name
states.** Its amount is in the hold the ledger made. Where that hold is lost, what the
reservation counts for is unknown, and where the names show it the fold says so:
limit (c).

Two rounds of review each found seven ways past rules written case by case (a
settlement without its guard, a correction counted by its names, a place taken twice).
A rule that says what is allowed, and freezes everything else, has no case to forget.

**A writer who forges a complete, consistent chain is believed.** A set of names that
the recogniser takes is, in the names alone, what the ledger would have made, and the
fold reads names. That is the first of the limits of the guarantee, (a) to (h), each
stated under "Limits".

### The books are names

**Every amount the fold needs is in a name.** A name lives in the store's generation,
not in content: it is there when content is damaged, and when the run it speaks of was
deleted and pruned. So the ledger's names say the money, and the content under each is
detail that must agree with its name.

A name under `spend/` is one of these forms, to the character, and no other:

| Name | Made by | Says |
|---|---|---|
| `spend/reservations/<run>` | The executor, in the admission's publication (ADR 0007) | The reservation's record, `reservation/1`: the detail |
| `spend/held/<epoch>/<run>/<micros>` | The ledger, in the same publication | That run's reservation: its amount, and the epoch it falls under |
| `spend/turn/<epoch>/<n>` | Whatever can change what is left of that epoch's allowance: an admission; a settlement, a closing and a correction of a reservation that counts in it; the reset that closes it | Nothing but that the turn `n` of that epoch is taken |
| `spend/reset/<epoch>/<micros>` | The owner's command | The allowance of that epoch |
| `spend/epoch/<epoch>` | The owner's command, in the same publication | Nothing but that the epoch is opened: a reset's guard |
| `spend/place/<run>/<n>` | A dispatch, a settlement over its bound, a closing, a correction | Nothing but that the place `n` under that reservation is taken |
| `spend/dispatched/<run>/<n>/<by>/<micros>` | The meter, before a request leaves | The reservation it draws on (the run at the root of its tree), its place, the run that sent it, and its bound |
| `spend/settling/<run>/<n>` | The meter, when the request ends | Nothing but that the place `n` is settled |
| `spend/settled/<run>/<n>/<stage>/<micros>` | The meter, in the same publication | The stage the request reached, and what it was charged |
| `spend/closed/<run>/released`, `spend/closed/<run>/held` | The ledger, in the publication that ends the run; or recovery; or the process that backs out an admission it could not confirm | That the reservation is closed, and whether the closing released what it held beyond its charges |
| `spend/adjusted/<run>/<place>/<turn>/<seal>/<micros>` | A person's command | A correction: what the reservation is charged, the place and the turn it took, and a seal of the names it was made over |

- `<run>` and `<by>` are run ids (ADR 0005). `<epoch>`, `<n>` and `<micros>` are
  decimal digits with no leading zero, 20 at most; an epoch is 0 for a reservation made
  before any reset and otherwise from 1, a place and a turn are from 1, and an amount is under the largest number there is, which is no amount (ADR
  0007). `<stage>` is `not_sent`, `answered` or `unresolved` (ADR 0010). `<seal>` is
  a digest in lower-case digits of base sixteen, of a fixed length: sixteen in the
  model, whose digest is not a cryptographic one, and in the core the first 32 of a
  SHA-256 ("The seal, in the core", below). A name is at
  most 200 characters, in the alphabet `Step::make` takes.
- **The largest number is refused where it comes in.** No name states it as an amount,
  so nothing that would is published: a reset, an admission, a share, a dispatch, a
  provider's charge, a settlement and a correction that state it are each refused
  before anything is judged, with a reason of their own (`Why::NoAmount` in the model;
  `spend::the_largest_number_is_no_amount_at_any_entry_and_nothing_is_published_with_it`).
- **Nothing secret is in a name:** a run's id is made by the executor, and the rest are
  numbers and fixed words.
- **The grammar is strict and parsed as untrusted.** Each form is written one way and
  read back to the same value; a number with a leading zero, a sign or a space, a
  segment too many or too few, a word of another case and a name of no form are not
  names (`spend::a_name_is_written_one_way_and_read_back_strictly`). The core has one
  parser of it, which every reader of a name under `spend/` uses: `spend::Name`, whose
  `parse` reads exactly what its `Display` writes
  (`spend::name::each_form_of_name_parses_to_its_parts_and_is_written_back_the_same`,
  `spend::name::a_name_that_breaks_the_grammar_in_each_way_is_refused_and_shown_escaped_and_cut`;
  tests named `spend::name::`, `spend::record::` and `spend::reading::` are the
  core's, in `crates/whizbang-core/tests/it/spend/`, and every other `spend::` test
  named in this record is the model's).

**The seal, in the core.** A correction is sealed over every other name of its
reservation, the guard of its own place aside. The seal is the first 32 digits, of
base sixteen and in lower case, of the SHA-256 of those names: each as its text, then
`!` when its content is damaged, then a newline, the lines in the order of their
bytes. A name of no form that belongs to the reservation is among them, as its text,
and is never marked as damaged: its content says nothing
(`spend::name::a_seal_is_the_first_32_digits_of_a_sha_256_of_the_names_in_their_order`,
which holds a fixed vector;
`spend::reading::a_correction_is_given_with_what_its_seal_has_to_be_and_whether_it_is_at_its_place`).

**A name of no form.** A text under `spend/` that the parser refuses is not ignored
where it can be placed, and is placed by the position of its segments alone:

| The text begins | And | It belongs to | What it does |
|---|---|---|---|
| `reservations`, `place`, `dispatched`, `settling`, `settled`, `closed` or `adjusted` | Its second segment is a run id | That run's reservation | It is a name the ledger could not have written: the reservation is frozen (the model's `Stray`). Where the run has no other name, it is a frozen reservation with no hold, and the store needs its owner |
| `held` | Its third segment is a run id | The same | The same |
| `reset` or `epoch` | Its second segment is a number | That epoch | The epoch is there, and its allowance is not whole: it allows nothing, as with a reset that lost its guard, until the owner resets |
| `turn` | Its second segment is a number | That epoch | A turn no publication took: the epoch needs its owner |
| Anything else | | Nothing | It changes no balance, and a fold reports it |

The model has `Stray` for the first two rows and is silent on the rest. The third and
fourth rows are the side that cannot count less: a garbled reset, guard or turn is
read as one more name in its epoch that the ledger did not make, never as nothing,
so a name garbled in place of a real one leaves that epoch allowing nothing or
needing its owner, as the loss of the real name alone would or more. They add no
refusal for the whole store: each stops its own epoch, by the rule there already is
for a reset without its guard and for a turn that no publication took. A segment
elsewhere that happens to be a run id places nothing: a run id is lower-case letters,
digits, `_` and `-`, so nearly any segment is one. The parser answers which row a
text is under (`spend::Belongs`;
`spend::name::a_name_of_no_form_belongs_to_a_reservation_or_an_epoch_by_the_place_of_its_segments`);
the reading sorts the first two rows into their reservation
(`spend::reading::with_one_reservations_content_damaged_the_reading_gives_the_others_and_that_one_as_damaged`),
and what the last three do to a balance is the fold's (task S1b).

**Why two names where one would seem to do.** A name that carries an amount cannot
keep two writers from one place: two dispatches that chose the place `n` with different
bounds have different names. So an amount-bearing name comes with a guard that carries
none (`place`, `settling`, `epoch`), made in the same publication: the store refuses the later
publication whole. The turn and the place are also what keeps a judgement from being
published on books that changed ("Nothing is published on a stale balance").

### What each publication makes

One publication each, all of its names or none. The model publishes exactly these
(`Books::apply`), and each is refused whole when the store holds one of its names:

| Event | Names made |
|---|---|
| A reset | `reset/<epoch+1>/<micros>`, `epoch/<epoch+1>`, and the next `turn` of the epoch it closes |
| An admission of a call nothing called | `reservations/<run>`, `held/<epoch>/<run>/<micros>` in the newest epoch, the next `turn` of that epoch; and the run and its admission (ADR 0007) |
| The confirmation of that admission, where it stands | Nothing: a reading of the names |
| The confirmation, where the admission does not stand | The closing of its reservation, as at the end of a run: `closed/<run>/released`, the next `place`, the next `turn` ("An admission is confirmed before anything runs under it") |
| An admission of a nested call | Nothing under `spend/`: its share is taken from its caller's allowance (ADR 0008) |
| A dispatch | The next `place` under the root's reservation, `dispatched/…` at it |
| A settlement | `settling/…`, `settled/…`, the next `turn`; and when the charge is over the bound, the next `place` |
| The end of a run nothing called | `closed/<run>/released` or `closed/<run>/held`, the next `place`, the next `turn` |
| Recovery | The same three for every reservation that is not closed and whose run is not open in this process, nor waiting there to be confirmed, in one publication: a turn for each |
| A correction | `adjusted/<run>/<place>/<turn>/<seal>/<micros>`, the `place` it names, which is the next, and the `turn` it names, which is the next |
| The end of a nested call, a crash, a deletion with a prune | Nothing under `spend/` |

**The next place** is the lowest number from 1 that no `place`, `dispatched`,
`settling` or `settled` of the reservation names in the judged generation. **The next
turn** of an epoch is the lowest that no `turn` of it names. Where the books are whole
that is one more than their count. A publication under a reservation takes its turn in
the epoch the reservation's hold names: in each, where other hands gave it more than
one hold, and in the newest where it has none
(`spend::the_turns_of_an_epoch_are_exactly_those_its_publications_took`).

### What the ledger can have written

The recogniser (`reachable` in the model) takes the names of one reservation, and
whether each is damaged. They are reachable when every row holds; a correction's seal
is looked at afterwards ("A person's correction").

| Of | The names are reachable only when |
|---|---|
| The admission | There is one `reservations/<run>` and exactly one `held`; no name that names the run is of another form; none is damaged |
| Places | The `place` names are 1 to their count, with no number missing |
| A dispatch | It is the only `dispatched` at its place, and the `place` is there |
| A request not settled | Its place has no `settling` and no `settled` |
| A settlement | Its place has one `dispatched`, one `settled` and the `settling`; and the charge is one its stage allows (ADR 0010): `not_sent`, nothing; `unresolved`, the dispatch's bound; `answered`, any charge |
| A closing | There is at most one; and with `released`, every dispatch has its settlement |
| A correction | The closing is there and is `held`; the place the correction names is there and holds no dispatch; no two corrections name one place |
| Every other place | The places that hold no dispatch are exactly as many as the settlements over their bounds, the closing and the corrections: each of those took one |
| The order of the places | Walking the places from 1, each dispatch's bound is no more than the hold's amount less what was known before it: every request sent before it, at its bound, or at its charge where it was settled under its bound; and, for each place below it that holds no dispatch, the excess of one charge over its bound, of a request sent before that place. No place below a dispatch is one that no such charge accounts for: nothing is dispatched after a closing. And the lowest place a correction names is above the one after the last dispatch: a correction follows the closing, which follows every dispatch |

Every history of the table in which other hands do nothing passes it after every event,
and each name of a full set of books lost alone, each of fifteen names made, and each
name damaged, fails it
(`spend::the_recogniser_takes_what_the_ledger_writes_and_nothing_else`). The same is
held of 500 books that the ledger alone wrote, generated from the seed: no name of
them lost alone, damaged alone or made alone leaves a reservation that the recogniser
takes ("The invariants, and what holds them" has the suite).

**The last row replays what the ledger checked.** A dispatch is published only when
its bound is no more than what its reservation has left, and never after a closing,
so names in which a dispatch did not fit, or follows a closing, are not the ledger's:
a reservation of nothing with a dispatch of 1 under it is frozen (history 74). The
fourth review round found that both readings took that set
(`spend::the_recogniser_replays_what_the_ledger_checked_and_the_second_reading_does_too`).

The places give the order of the dispatches, and of whatever else took a place. **Two
things the names do not order,** and there the replay takes the order that leaves the
most, so that it refuses nothing the ledger wrote:

- **when a request was settled under its bound.** That takes no place. The request
  is counted at its charge from the moment it was sent;
- **which charge over its bound took which place,** where more than one request sent
  before a place was settled over its bound. The smallest excess is taken first.

Each of those orders is one the ledger could have kept: a request can be settled as
soon as it is sent, and a charge over its bound takes the next place whenever it is
settled. So what the replay takes is a set that some order of the ledger's
publications writes, and it takes every such set; where other hands made one, it is
believed as any such set is (limit (a)). What is not held of order is which of the
places above the last dispatch was the closing's, beyond that one of them below every
correction's can be. No count depends on it.

### What a reservation counts for

The fold (`fold` in the model; the real one is task S1b's) reads the names of one
generation and, of content, only whether each name's can be read and agrees with it.

**A place** with a dispatch counts for its settlement's charge when it has one, and
otherwise for the dispatch's bound, which is outstanding. For a reservation:

- *charged* is the sum of what its `settled` names say;
- *outstanding* is the bounds of its dispatches that are not settled;
- *known* is charged plus outstanding;
- *A*, its amount, is what its `held` names.

| The reservation | Counts for | State |
|---|---|---|
| Has a correction that stands | What it says, and never under what is settled | Adjusted |
| Is not reachable | *Most*, below | Frozen |
| Closed as `released` | What is known, which then is what is charged | Released |
| Closed as `held` | The larger of *A* and what is known | Interrupted |
| Not closed | The larger of *A* and what is known | Open |

The first row that fits is the state. So an open reservation counts in full while its
run executes (history 16), a charge over its bound is counted as soon as its
settlement is durable (histories 8 and 11), and a closing releases only in the one row
that says so. A reachable reservation that carries a correction which no longer stands
is interrupted, at the larger of *A*, what is known and what any correction says: a
settlement that arrives after a correction ends it (history 60).

**The most a frozen reservation's names can mean** is one sum, with no credits. *H* is
the largest amount a `held` of it names, or nothing. It counts for

> *H*, and every bound a `dispatched` under it names, and every charge a `settled`
> under it names, and *H* once more for each number from 1 to the highest place any of
> its names names that holds no `dispatched`

or for what a correction of it says, where that is more. No settlement gives up
anything of a bound, guarded or not; no overrun, closing or correction accounts for a
place; a place with a settlement and no dispatch counts *H* and the charge. The third
review round found the credits: a settlement made at the place of a lost dispatch made
the place look accounted for (history 65), and a guarded settlement at a charge its
stage forbids was taken for an overrun that had used a place (history 66). A sum that
gives no credit has none to forge. So a reservation of 10 with one request settled
and closed, whose record is damaged, counts 27 where it released at 3 (history 15).
A name can name any place a number can say and state any amount, so the places that
hold no dispatch are counted and never walked, and a store's sum of what its
reservations count for stops at the largest number
(`spend::a_name_can_name_any_place_and_state_any_amount_and_the_fold_still_ends`).
**So does every other sum of the fold:** what is charged, what is outstanding, what a
run sent, and the frozen sum itself, which as first written wrapped past the largest
number and came to less than two real settlements (history 72). A count that stopped
there is over any allowance: nothing is left, and the debt is counted. No names make
the fold fail
(`spend::every_sum_of_the_fold_stops_at_the_largest_number_and_is_over_any_allowance`).

**Where a frozen reservation has not exactly one hold, what it counts for is
unknown, and at least the sum.** *H* is read from the holds that are there. With no
hold, or with several, the fold cannot tell whether the ledger's is among them, and
other hands can leave holds of nothing in its place: with the ledger's hold and a
dispatch lost and two holds of nothing made, the sum is 0 while 10 are outstanding
(history 69). So the fold answers "unknown, at least" beside the number (`Counted` in
the model); the store's balance says that it cannot tell how much is counted, in
every epoch; and what a run sent under it is unknown, with no most. Where the
ledger's hold is among several, the largest amount named is no less than it and
nothing is lost; the fold cannot know that, and says "at least" all the same. Limit
(c) has what is lost, and what one hold made in the ledger's place does.

A settlement's *whole chain* is the row "A settlement" of the recogniser with the
dispatch's `place` there and none of the four damaged. What such settlements say is
*settled*: it lowers nothing in a frozen reservation, and it is the least a correction
may say.

**A frozen reservation counts in every epoch there is,** whatever its holds name: a
hold other hands made can name any epoch, and the real one can be lost (history 67).
**So does a reservation that has no hold, or more than one, when its owner has
corrected it:** a correction says what a reservation is charged, and not where
(history 68). A corrected reservation that has its one hold falls under that hold's
epoch, as every reachable one does (history 42).

**What a dispatch may still take** is *A* less what is known, of an open reservation,
and nothing of any other: nothing is dispatched under a frozen reservation (history
46) or a closed one. The reading that answers it says nothing is left once any closing
names the reservation, whatever state it reads as
(`spend::a_closed_reservation_has_nothing_left_for_a_dispatch_whatever_its_state_reads`).
**A request of nothing asks for nothing, and is placed where nothing is left:** under
a reservation that is over its amount, by a run in debt, in a store in debt. It takes a
place and is counted for nothing. Whatever it is then charged is over its bound: it is
counted, and it is debt. So in a store with no allowance calls of nothing are
admitted, their requests of nothing are placed, and each charge adds to the debt
(history 85;
`spend::a_request_of_nothing_is_placed_under_a_reservation_over_its_amount_and_in_a_store_in_debt`).
The books are not under by it: a charge over a bound is the provider's, and no bound
keeps a provider to its word. Where the store needs its owner a request of nothing is
refused as every other is ("Epochs"). Nothing is dispatched under a reservation of an epoch that needs
its owner either ("Epochs"; history 70). And a dispatch never takes more than the
process that runs the tree holds as left of it ("An allowance holds a signed
balance"; histories 71 and 73).

**What one run sent** is known when its tree's reservation is reachable: for each
place whose dispatch names the run, what the place counts for. Under a frozen
reservation it is **unknown, and at most what the reservation's names can mean**: a
dispatch whose name is lost names no run, so a number read from the names could be low
(histories 50 and 63). Where the reservation's amount is unknown as well, no most can
be said, and the fold says that (history 69). The fold answers one of the three
(`Sent` in the model), and a caller that shows a run's spend shows which.

**The store's balance** is of one epoch: the allowance its reset set, less the sum of
what the reservations that count in that epoch count for. What is left is never
reported below nothing; by how much the count is over the allowance is the debt, and it
stays counted (history 8). Where a reservation that counts in the epoch has an amount
that is unknown, the balance says that what is counted is the least it can be. An
admission is judged on the newest epoch: it is admitted
when its amount is no more than what is left, so a call of nothing is admitted in a
store with no allowance and a call over nothing is not (history 38).

(`spend::every_named_history_gives_the_balances_it_must_and_keeps_every_invariant`
runs every history named here; each is in `histories()` under its number.)

### Damage, loss and other hands

**A damaged entry freezes the reservation it belongs to and nothing else.** Each name
of a reservation, damaged alone, freezes that reservation at no less than its amount,
leaves every other as it was and never stops the fold; a damaged turn, which belongs to
no reservation, freezes none
(`spend::the_fold_reads_names_and_nothing_else_and_one_damaged_name_freezes_one_reservation`).
Freezing is not a record: it is what the fold finds, every time, so nothing can be
both frozen and released, and a reservation that released and is damaged afterwards
counts in full again (history 15). Only a correction ends it (history 31). And
nothing else is stopped by it: with one record of a reservation of 10 unreadable, that
reservation counts 25, and a call of 5 that the rest of an allowance of 30 covers is
admitted and sends its request (history 81;
`spend::one_record_that_cannot_be_read_freezes_its_reservation_at_its_worst_and_stops_nothing_else`).

**A name that other hands made freezes too,** unless with the names that are there it
makes a set the ledger could have written
(`spend::what_the_ledger_could_not_have_written_is_frozen_at_the_most_it_can_mean`,
`spend::a_settlement_is_whole_only_at_a_charge_its_stage_allows`,
`spend::a_closing_and_a_correction_count_only_as_the_ledger_writes_them`): a
settlement without its guard or at a charge its stage does not allow (histories 43 and
64), a second hold in any epoch and of any amount (45, 46), a second dispatch at a
place (17, 50), a closing without its place (48), a name of no form. Names that
name a run with no hold are a frozen reservation of nothing, counted for what they
say.

**Losing one name of a reservation always freezes it:** no set the ledger writes is
another with one name gone (`spend::losing_any_one_name_lowers_nothing`, which loses
each name of three books in turn). It then counts for no less than it did, but for two
names that are the only place their amount is in:

- **a settlement over its bound** (limit (b)): the charge was in that name alone. What is left is
  the amount, its dispatch's bound and the amount again for the place the overrun
  took (history 53: 12 was counted, 25 is);
- **the hold** (limit (c)): the amount was in that name alone. The reservation is frozen at every
  bound and charge its other names say, takes no dispatch, and says that what it
  counts for is unknown.

A name lost from a reservation that is frozen already takes what it said out of the
sum: the rule is of books that were whole. So it is with any reservation that other
hands touched before: a correction its owner made over a name they made can say more
than the names do once that name and the correction are gone
(`spend::losing_any_one_name_lowers_nothing`, its last case).

Losses of more than one name that leave a reachable set are believed: "Limits", (d),
lists them.

### Epochs: what a reset bounds

A reset makes the next epoch and states its allowance. **It states what may be spent,
and counts only with its guard:** an epoch is there as soon as a `reset` or an `epoch` names
it, and its allowance is its reset's only when it has both and neither is damaged. Two
resets of one epoch, with the guard, allow the smaller. Any other epoch allows nothing,
and is still the newest: a reset whose guard is lost, or one that other hands made
without it, stops admissions until the owner resets again, and never allows more
(history 55,
`spend::an_allowance_is_its_resets_only_with_the_guard_and_with_neither_damaged`).

**Turns are an epoch's.** `spend/turn/<epoch>/<n>` is taken by whatever changes what
is left of that epoch's allowance, and the fold counts what the epoch's turns are
*due*: one for each `held` in it, one for each `settling`, `closed` and `adjusted` of a
reservation with a hold in it, and one when the next epoch is there, for the reset that
closed it. **An epoch whose turns are not exactly 1 to that count needs the owner:**
nothing is left in it and no admission is taken, until a reset opens the next epoch,
whose turns start again. So a turn that is lost, a turn that other hands made at the
next number or far ahead, and a name that takes a turn made without one, each stop
admissions and never let one through that judged after it (histories 47 and 61,
`spend::the_turns_of_an_epoch_are_exactly_those_its_publications_took`). What a
reservation counts for is not changed by it.

**A reset does not clear a frozen reservation:** it is still there, and it counts in
the new epoch as in every other, at the most its names can mean. Only the owner's
correction of that reservation ends that (histories 31 and 65;
`spend::a_reset_does_not_clear_a_frozen_reservation_and_its_owners_correction_does`).
Where its one hold states its amount, that is all it does: what the new allowance has
beyond it can be reserved.

**A frozen reservation whose amount is unknown makes the store need its owner, in
every epoch.** With no hold, or with several, no name can be trusted for what it
counts for, it counts in every epoch for a number that is only the least, and
nothing is admitted until its owner corrects it; a reset does not end that (history
67; `spend::a_store_that_needs_its_owner_starts_no_paid_work_and_still_settles_ends_recovers_and_is_corrected`).
A reservation that is interrupted and reachable does not stop admissions, and nor
does a frozen one whose hold states its amount.

**An epoch that needs its owner takes no dispatch either.** No request that is
judged where the epoch needs its owner is dispatched under a reservation of it, a
request of nothing among them; and a frozen reservation whose amount is unknown makes
every epoch need its owner, so that none is dispatched anywhere. The first three
rounds let dispatches under other reservations
go on. The fourth showed a reservation that read as open with 5 left, after three
names of a charge over its bound were lost, in an epoch whose turns did not add up:
the dispatch did not look at that, and 5 more were sent where nothing was left
(history 70). What stops is new paid work. What was dispatched is still settled, or a
run could never close:

| Where the epoch needs its owner | |
|---|---|
| An admission | Refused |
| A dispatch under a reservation of that epoch, of any amount | Refused. One that was judged before the epoch came to need its owner may still be published, once for each such judgement, all of them within what its tree really has left: limit (e) |
| The settlement of a request that was dispatched | Taken, a charge over its bound too |
| The end of a run | Taken: the reservation is closed as its names allow |
| Recovery | Taken |
| The owner's correction | Taken. One that raises what a reservation counts for in the newest epoch is refused: nothing is left there to hold it |
| The owner's reset | Taken: it opens the next epoch |

(`spend::a_store_that_needs_its_owner_starts_no_paid_work_and_still_settles_ends_recovers_and_is_corrected`.)
A reset opens an epoch whose turns start again, and admissions start with it, unless
a frozen reservation's amount is unknown. **A run of the epoch that needed its owner sends nothing
more:** that epoch's turns are never made right. It settles what it sent, and ends.

**A reservation falls under the epoch its hold names, for good.** The balance an admission is judged on is the newest
epoch's, so a reset closes the books before it: a reservation of an earlier epoch
counts against that epoch's allowance and no other (history 30), and a correction of
one is recorded and changes nothing the new allowance can spend, downward or upward
(histories 32 and 42). That is of a reservation with its one hold that is not frozen.
A frozen one, and one with no hold or several, counts in every epoch ("What a
reservation counts for"): the correction of a frozen one ends what it counted in the
new epoch, and never leaves more there than the reset set (history 31).
A run that is still executing when its epoch is closed goes on drawing on its own
reservation, and its settlements and its closing take their turns in that epoch.
**So a reset does not stop what was admitted before it.** With three reservations of 10
open, a reset to nothing leaves each of them sending all it reserved: 30 are dispatched
after the reset, under the old epoch, and the new epoch counts nothing of them (history
84; `spend::a_reset_does_not_stop_the_open_reservations_of_the_epoch_it_closes`). A
reset sets what may be admitted from then on. What stops a run is its cancellation. A
reset takes the next turn of the epoch it closes, so an admission that judged before it
is asked again and falls under the new epoch.

**The epochs end at the largest number a name can say.** Other hands that open that
epoch leave a store that allows nothing and that no reset can follow: the reset
would name the same epoch, and the store refuses it
(`spend::known_after_the_largest_epoch_a_name_can_say_no_reset_follows`). Nothing is
under-counted by it. What the owner's command says there, and how such a name is
taken away, is the second part's to decide.

### An allowance holds a signed balance

Changes ADR 0008, "Allowances". A run's allowance, in memory, is a signed amount: the
reservation of a run nothing called, or the share a nested call took.

- **Debited before a request:** a paid request's bound is taken from its run's
  allowance when its dispatch is published, and a nested call's share when it is made;
  either is refused when the run can spend less (histories 2 and 4).
- **Given back to:** what a bound held beyond the charge, when the request is settled;
  a share whose call's admission was refused; and a child's remainder, which is its own
  balance when that is over nothing, when the child's end is durable and its parent
  still executes. A remainder that arrives once the parent has entered its commit step
  is dropped: the parent's reservation is closed by its own settlements.
- **In debt where an overrun put it:** a charge over its bound takes the excess from
  the allowance of the run that sent the request and of each run above it, below
  nothing if need be (history 18). **What a run can spend is its balance or nothing,
  never less;** and every return is added to the signed balance, so it pays the debt
  first (histories 8, 19 and 20).
- A parent that calls many children in turn, each using nothing, has all of it at the
  end (history 21).

An allowance is not the books. It is lost in a crash, and it is pessimistic: a run
above an overrun loses the whole excess though a run between them had something left.
What it must never do is hold more than is left: the signed balances of a tree's runs
come to no more than the reservation's amount less what is known against it. And a
run's own balance does not bound a request by itself: a child whose parent overran
still holds its share. So a dispatch is held twice more.

**By the names,** to what its reservation has left (history 39).

**And by what the process holds of the tree.** Beside the runs' balances, the process
that runs a tree keeps the reservation's amount and what the requests sent under it
took of it: each at its bound until it is settled, and at its charge from then on, a
charge over its bound among them, whichever run of the tree sent it. A charge over
its bound counts from the moment the process holds the answer that says it, before
its settlement is durable: the process knows of it already (history 75). A dispatch of
more than the amount less that is refused, whatever the names say and whatever its
run's share is. It is asked at the moment of publication, of a dispatch judged late
as of one judged now: memory is as it is now, where a reading of the names is old.
The fourth review round found the names not to be enough. A dispatch judged before
a charge over its bound was published after other hands took the charge's place away
(history 71); and one judged now was published after they took away the whole
settlement and its turn, which leaves a set the ledger could have written, with 5 left
that were not there (history 73;
`spend::a_dispatch_is_held_to_what_is_left_of_its_tree_in_the_memory_of_its_process`).
What a process holds of a tree is lost with it, as the balances are: nothing of the
tree executes after a crash, and recovery closes the reservation.

### Nothing is published on a stale balance

A judgement is made on one generation and published onto another. The store decides
only that a name a publication makes is not there (ADR 0007). So:

- **Every publication that can change what is left of an epoch's allowance takes that
  epoch's next turn:** an admission, every settlement, a closing, each closing of a
  recovery, a correction, and the reset that closes the epoch. A judgement is
  published only with the turn after the ones it saw, so two that judged the same books
  make the same turn, the store refuses the later one whole, and it is judged again
  (histories 1, 12, 29 and 60). The first version gave a turn only to what could lower
  what is left; a settlement under its bound that ended a correction lowered it and
  took none (history 60).
- **Whatever can lower what is left of a reservation takes its next place:** a
  dispatch, a settlement over its bound, a closing, a correction. So of two dispatches
  that chose a place one is published and the other chosen again (history 3); a
  dispatch that judged before an overrun is judged again and refused (history 39); and
  a reservation is closed once, whoever closes it (histories 28 and 40). A dispatch
  takes no turn: it is held to its reservation, which is counted in full already.
- **The cost is a turn for each settlement.** A run with one admission, *n* paid
  requests and one end takes *n* + 2 turns where the first version took one, and an
  admission that judged before any of them, in the same epoch, is judged again. For one
  person's store on one machine that is accepted; the bound on how often a ledger is
  asked again stays on `docs/carry-over.md` with milestone 2.

The model holds this for every history, generated ones among them, in which an event
is judged on earlier books and published later, in one process or in a second over the
same store. The checker works out from the truth what is really left of the allowance
and of each reservation, and no admission stands and no dispatch is published that it
does not cover (`spend::every_generated_history_keeps_every_invariant`,
`spend::two_processes_over_one_store_are_held_by_the_same_names`).

**No dispatch after any closing.** A dispatch is refused when any `closed` name names
its reservation in the generation it is judged on, with its place or without, whatever
else the reservation is (history 48). A closing the ledger made took a place, so a
dispatch that judged before it is refused by the store and judged again (history 49).

**What the store cannot refuse** is a publication that was judged before other hands
changed the books: its names collide with nothing they did. A dispatch judged before a
closing that other hands made without a place is published (history 48, its last
event); so is an admission judged before a turn was lost (history 61), and a dispatch
judged before its place's guard was lost (history 59). In the first and the last the
reservation is frozen, so nothing more is dispatched under it and its closing
releases nothing; in the second the epoch needs its owner, and nothing more is
admitted in it. Such a
dispatch stands, and is still held to what its process holds of the tree, so it does
not pass what was really reserved (history 71). Such an admission is published and
does not stand by that: the next section. And a dispatch that was judged while the
store was sound is published after the store came to need its owner, by the loss of
another reservation's hold or by the loss of a turn (histories 78 and 79): the refusal is
of a judgement, and that judgement was made before. "Limits", (e), has all three.

### An admission is confirmed before anything runs under it

A turn keeps two admissions that judged the same books from both being published,
and it is a name: other hands can take one away and make another. The fifth review
round did that. A reservation of 10 is all of an allowance of 10, and all of it is
sent. Other hands take away the epoch's first turn and make its second. An admission
of 10 that judged before the first is then published with the turn that is free
again: the turns are 1 and 2 for two holds, the epoch needs nobody, both
reservations are sets the ledger could have written, and the books read 20 counted
against 10. As this record stood the second run's request of 10 was then sent
(history 76). No name that states money was lost or made; the admission's safety
rested on one turn's name.

**So the publication of an admission admits nothing by itself.** When it is durable,
the process that made it reads the names again, as they then are, and folds them. The
admission stands only if, on that reading:

1. **its own reservation is a set the ledger could have written, and open:** the
   record, one hold, and nothing else that names the run;
2. **neither the epoch it fell under nor the newest epoch needs its owner,** which a
   frozen reservation whose amount is unknown makes every epoch do;
3. **the epoch it fell under is not over its allowance, with this reservation counted
   in full:** what is counted there is no more than what is allowed. A call of
   nothing adds nothing to what is counted and is not held to a debt that was there
   before it, as it is not when it is judged (history 38).

**The first is held as it is worded.** A reservation that reads as open, with a place
and a dispatch under it that the names make a set the ledger could have written later,
is not the record and one hold: nothing runs under an admission before it is
confirmed, so no such name is the ledger's, and the admission does not stand (history
80). The pull request's review found the model and the checker asking only that the
reservation was a set the ledger could have written, and open; both now ask this, each
in its own code
(`spend::an_admission_is_confirmed_only_where_its_reservation_is_the_record_and_one_hold_and_nothing_else`).

**Otherwise the process backs it out at once.** It closes its own reservation by the
one rule of a closing ("Settlement, closing and recovery"): the ledger dispatched
nothing under it, so an open reservation with no other name is closed as `released`
and counts for nothing from then on; one that is not what the ledger wrote is closed
as `held` and stays frozen; and one under which other hands made a dispatch that is not
settled is closed as `held` and counts in full (history 80). The closing takes the reservation's next place, which is its first where no other hands made one, and the next turn of its
epoch, as any closing does. The call is refused as not confirmed, a refusal of its
own (`Why::NotConfirmed` in the model), and its run never executes.

**Nothing is done under a reservation before it is confirmed:** no request is
dispatched, no call is admitted from inside its run, the run does not start. Until
then the reservation is open and counts in full, which is what makes the rule safe
for two admissions at once: each reads after its own publication, so of any two the
one published later has the other's names in its reading, and the two never both
stand on books that hold only one of them.

What follows from it:

- **A reservation that cannot be closed stays as it is.** Where other hands made a
  closing for it already, or took its names away, the process publishes nothing; the
  reservation counts for what its names say, the call is refused, and nothing runs.
  The same holds where the store refuses the closing's publication: the reservation
  stays admitted, not executed and counted in full, and the next recovery closes it.
- **A crash between the publication and the reading** leaves a reservation that was
  admitted and never confirmed. Nothing ran under it, and it counts in full, which
  is the most it can, until the next recovery. Recovery cannot tell it from a
  reservation whose run died before it sent anything, because the names are the same
  three; it closes it by the same rule, as `released` when its names are whole
  (history 25), and as `held` when they are not. Recovery passes over an admission
  that waits to be confirmed in the process that recovers: that one is its own to
  confirm or to close.
- **Two admissions that read at once can both be backed out,** each seeing the other
  counted in full and neither seeing the other's closing. That is safe, and it is a
  cost: both callers are refused where one could have run. The model reads one at a
  time, and there the first to read is backed out and the other stands.
- **A reading is of one moment.** What other hands do after it is what they do to any
  books: limits (a) to (d).
- **The cost for an admission that stands** is one more reading of the names and one
  more fold, and no name. One that is backed out makes three names more, and takes a
  place and a turn.

An admission that was judged late and whose reading is sound and within the allowance
stands: a late admission whose turn fills the place of a turn that was lost is
published, and is then confirmed or not as any other
(`spend::known_a_late_admission_whose_turn_fills_the_place_of_one_that_was_lost_is_published`).
What the names leave out, they leave out of that reading too: a charge that was lost
with its settlement is not counted against the admission, which is limits (b) and (d)
and not a matter of when it was judged
(`spend::known_an_admission_judged_before_a_settlement_that_other_hands_took_away_whole_is_published`).

(`spend::an_admission_stands_only_when_it_is_confirmed_on_a_reading_made_after_its_publication`
holds each of the three, alone; what backing out publishes; that nothing is done before
the confirmation; the crash; and the two admissions.)

### Settlement, closing and recovery

**A run does not end while a request it sent is outstanding.** The tally waits for
every request until its settlement is durable (the plan, "What this plan builds on";
task I3), and the model refuses the end of a run with a request unsettled, late
judgements among them (history 51).

**What the provider charged and what the books say are two things.** Between an answer
and its settlement the charge is in the memory of one process. A crash there leaves a
dispatch with no settlement: recovery counts the reservation in full and releases
nothing (history 52). When the answer was over the bound, no name says by how much,
and the reservation counts for its amount: "Limits", (b).

A request is settled by the run that sent it, while that run is open, from what the
process holds of the request and never from a name it reads back, once
(`spend::a_place_whose_guard_is_taken_is_not_settled_again`), by the stage it reached
(ADR 0010): not sent, nothing; answered, the charge the answer gave, or the
bound when it gave none; unresolved, the bound, counted as unresolved (histories 5, 6,
7, 35 and 36).

**One closing, by the run's end or by recovery.** It releases, as `released`, only
when the reservation is open, nothing under it is damaged or unplaced, and every
place under it is settled with its whole chain; a tree charged more than it reserved is closed
at what it was charged (histories 9, 24, 25). A reservation that is closed takes no
dispatch
(`spend::a_closing_releases_only_what_is_open_and_whole_and_nothing_is_dispatched_after_it`). Otherwise it is closed as `held`, and
nothing is released: a dispatch whose settlement failed or whose backend panicked, a
process that died between a dispatch and its settlement (histories 10, 22, 23).
Recovery closes every reservation that is not closed and whose run is not open in this
process, nor waiting there for its admission to be confirmed, by the same rule, in one publication; done twice it makes nothing (histories
26, 27); a run abandoned in a live process is left until that process is gone
(history 37). **Recovery releases only on whole chains,** as the plan's decision has it
("unless whole, readable records show that every request it dispatched was settled"):
it witnessed none of them, and a reservation whose names are not reachable is closed
as `held` (histories 43 and 64). A second process that
recovers while the first still runs a request closes the reservation as `held`; the
first settles its request afterwards, and a charge over the bound is counted
(histories 54 and 56). **So "interrupted" does not say that the process is dead:** a
reservation closed as `held` may have a request in flight in a process that lives,
where another process recovered the store; and where the process is dead, the
provider it asked is not. What that means for a correction is limit (g). A run that was deleted and pruned changes nothing here: no name under
`spend/` goes with it (histories 13, 14).

### A person's correction

`adjusted/<run>/<place>/<turn>/<seal>/<micros>` sets what a reservation is charged. It
is the owner's deliberate act (`whizbang spend adjust`, task S12) and is trusted for
what it says. **What it means is stated here and is not an exemption: a correction
writes off what is outstanding under its reservation, and what a place without a whole
settlement says.** It never writes off what settlements with their whole chain say. It
is also the one thing that ends a freeze: the owner accepts the frozen reservation, as
its names then are, at the amount given.

**The write-off is a cost in money, and the one way past the allowance in which no
other hands act.** What is outstanding under a reservation is a request that left and
has no settlement. It may still be charged. A correction to less than the reservation
counted gives the difference back to the allowance, and it can be admitted and spent
again before that charge arrives: limit (g), with histories 82 and 83.

The command takes it only for a reservation that is closed and is interrupted, frozen
or already corrected; for an amount from what is settled, as the least, to *most*, as
the most; and, where it raises what the reservation counts for in the newest epoch,
only when what is left can hold the difference (histories 33, 34). It takes the
reservation's next place and the next turn, names both, and carries **a seal of every
name the reservation has, each with whether it is damaged**.

**A correction stands** when it is not damaged; its reservation has a closing; the
`place` it names is there, with nothing else at it; the `turn` it names is there, in
the epoch of the reservation's hold; and its seal is that of exactly the reservation's
other names as they are, its own place aside. Then the reservation counts for what it
says, and never under what is settled, whoever wrote it
(`spend::a_correction_never_counts_under_what_whole_settlements_say_whoever_wrote_it`).
It is never told by how many names there are: the first version did that, and a name
of any form could stand in for a guard (histories 57 and 58).

**Otherwise it says nothing.** A correction that other hands made without its place or
its turn is none. Any name of the reservation that is made, lost or damaged after a
correction ends it, a later correction's among them, which then is the one that
stands; and so does damage to the correction itself. The owner corrects again, over the
names as they then are (history 44,
`spend::a_name_made_or_lost_or_damaged_after_a_correction_ends_it`).

**It does not end for good.** The fold is of the names that are there: when a name
made after a correction is taken away again, and nothing else changed, the names are
those the correction was sealed over, and it stands. A fold of names cannot tell that
state from the one the owner corrected, because it is the same state.

### The invariants, and what holds them

| Invariant of the plan | Held by |
|---|---|
| 1. Never under | After every event of every history, each reservation counts for no less than its floor, which the checker works out from the truth alone: for each request really sent, what its settlement recorded, or its bound while none is recorded. The truth is written down as things happen (what was reserved and in which epoch; every request, two at one place among them, with the run that sent it, its bound and what was settled; the names the owner's last correction left) and is never read from the fold. What a correction writes off is left out of the floor by the correction's stated meaning, which is limit (g) |
| 1, by epoch | Each reservation counts in the epoch it was really reserved in; and no more can be reserved than the allowance the owner really set less the floors of that epoch |
| 1, by run | Each run is counted for no less than the floors of the requests it really sent, or is answered as unknown with a most that is no less (histories 50 and 63) |
| 1, under loss | A reservation that lost one name, with nothing else touched, is frozen and counts for no less than it did; no less than its amount when the name was a settlement over its bound; what is known when it was the hold |
| 2. Debt is not forgotten | What is left is the allowance less the count, or nothing; a tree's signed allowances, in whichever process holds them, never come to more than what was really reserved less what its requests really took; every return is added to the signed balance |
| 3. One closing | A reservation that released has every request settled and recorded; one closed with nothing released is never released later; a damaged one is frozen or corrected |
| 3, no dispatch after a closing | No dispatch is published once a closing names its reservation, whenever it was judged (histories 48, 49) |
| 4. Damage freezes one reservation. "A damaged entry freezes the reservation it belongs to, at an amount that can be read without the damaged content. It never stops the fold, recovery, or a call that has nothing to do with it." | A name of a reservation that is damaged, made by other hands or lost changes no reservation but its own, and a damaged one's own counts in full, at the most its names can mean. The fold and recovery go on, and a call the rest of the allowance covers is admitted and sends its request beside it (history 81; `spend::one_record_that_cannot_be_read_freezes_its_reservation_at_its_worst_and_stops_nothing_else`). The exception, where nothing can be read: below |
| 5. Every amount is in a name | Every name the model makes is in the alphabet, under the limit, and read back by the parser to the same name; the fold of the names read back is the same fold |
| 6. No admission on a stale balance | No admission stands, and nothing runs under one, unless the truth had room for it: the allowance the owner really set for its epoch, less every reservation really made in that epoch, each at its amount, or at what its requests really took where that is more, until the ledger released it or its owner corrected it. And unless the reading made after its publication showed its reservation whole and open, no epoch of the two needing its owner, and its epoch within its allowance. No dispatch is published that what was really reserved does not cover, late ones among them. No admission is published as it was judged once the ledger has taken a turn in its epoch since, unless other hands did something between (limit (e)); and no request is sent and no call made under a reservation that was not confirmed |
| 7. A person corrects by a new record | A correction is a new name; nothing the ledger does removes or replaces one |

What the checker leaves out is the limits under "Limits", and nothing else.

**Invariant 4, and what the owner chose.** The third and fourth review rounds of this
record replaced the plan's rule by a stricter one: any frozen reservation made the
store need its owner, and no call was admitted and no request dispatched anywhere
until the owner corrected it. That reversed the plan's decision ("every other
reservation and every unrelated paid call goes on") and the last clause of its
invariant 4, and a plan outranks an ADR. The owner was told what each rule costs and
chose the plan's, on 2026-11-14: one unreadable record does not halt a store.

Every name that can make a store stop was then sorted by what can still be counted.

| What is there | What can be counted | What the store does |
|---|---|---|
| A frozen reservation with exactly one hold: a record that cannot be read; a name of no form; a name made, or lost, of a form that takes no turn (the record, a place, a dispatch, a settlement), where what is left is not a set the ledger could have written | The most its names can mean, which is no less than its amount and every bound and charge still named | Goes on. Nothing is dispatched under that reservation; everything else is admitted and dispatched as the allowance covers it |
| The same, where the name lost is a settlement over its bound | The amount, the bound and the amount again for the place the charge took: less than the charge, where the charge was more | Goes on. Accepted limit (b): history 87 |
| The same, where its one hold is one that other hands made in the place of the ledger's | What that hold states, and what the other names say | Goes on. Accepted limit (a): history 86 |
| Names made under an open reservation that leave a set the ledger could have written: a dispatch made with its place, within what the reservation has left | What the names say: the reservation is not frozen, and the dispatch counts as one the ledger placed | Goes on, and nothing is refused under that reservation. Limit (a), a forged set that the recogniser takes: it is not the first row |
| A frozen reservation with no hold, or with several: the hold lost; a second hold made; a name made for a run that has no reservation | Nothing that can be trusted: "unknown, at least" | The store needs its owner, in every epoch, until that reservation is corrected. Kept as an exception; history 67 |
| An epoch whose turns are not those its publications took: a turn lost or made; a name that takes a turn (a hold, a settlement's guard, a closing, a correction) lost, or made without one | Whatever the name that is gone stated is counted nowhere, and no name says what it was | That epoch needs its owner: no admission until a reset, no dispatch under its reservations. Kept as an exception; histories 61 and 70. **It holds that one epoch and no other:** after a reset, a name lost from the epoch it closed leaves the new epoch admitting in full (limit (d); history 88) |

The first three rows are the owner's choice. In the first, going on does not count
less than was spent. In the second and third it can, and each is an accepted limit,
stated under "Limits" in money with a history that asserts the figure it is under by.
**The last two rows are not the plan's rule and not yet the owner's:** there no worst
case can be counted, so that going on would let the whole of what is unknown be
spent again, and this record keeps the refusal until the owner has ruled on exactly
those cases. With the refusal lifted there too, histories 67 and 70 would read as
they did before the third and fourth rounds: 20 committed against 10, and 17 taken of
a reservation of 10.

The table has the 38 histories of the plan, in its order, this record's own after
them (39 to 42), from 43 those of its review rounds, 68 from the hunt that
followed the third, 69 to 72 the fourth round's, 73 to 75 found while those were
built or ruled with them, 76 the fifth round's, 77 a tree deeper than a generated
history is long, 78 and 79 the sixth round's, one trace in its two forms, and 80 to
87 those of the review of the pull request and of the owner's choice that followed:
names made under an admission before its reading (80), the plan's invariant 4 (81),
what a correction writes off (82 and 83), a reset under open reservations (84),
requests of nothing (85), and the two limits accepted with invariant 4 (86 and 87);
and 88, from the follow-ups of that review: a loss in an epoch a reset has closed.
And 89 is task S1b's: every name of a reservation garbled past reading (limit (h)).
That is 89 named histories. The generated histories, in six suites, are 1,000 of at most 24 events each, from a seed written
in the test, and 1,000 more of another shape: books the ledger alone wrote, then one to
six things other hands do, then one to four things the ledger and its processes do
(`spend::every_generated_history_of_whole_books_then_other_hands_keeps_every_invariant`); one that breaks an invariant is printed with the seed and its number.
One in four of the first shape and one in three of the second begin with a tree whose
parent's request is settled over its bound while its child holds a share; in the
second, other hands at times take away a whole settlement, its guard, its place and
its turn, and a run that another run called then sends a request, judged late or
now. **Every dispatch that is published is held to what was really left of its
reservation, in the truth,** and both tests are held to reaching that: with runs that
other runs called, after a run above them overran, late across what other hands did
and after names of the reservation were lost. 250 more histories, of both shapes,
have amounts near the largest a name can state and places near the largest a name can
name (`spend::every_generated_history_of_amounts_near_the_largest_keeps_every_invariant`).
**A fourth shape moves a turn around a late admission:** 400 histories in which the
ledger alone acts; other hands then take away one turn and make another, the one
before the other and after it, in the newest epoch and in another; and an admission
that was judged before they did, three times in four on the names as they were just
before the lost turn was taken, is published. It is confirmed at once, or published
and confirmed apart with other events between, a crash and a recovery among them;
its run then asks for all it reserved; and the ledger and its processes go on
(`spend::every_generated_history_of_turns_moved_around_a_late_admission_keeps_every_invariant`).
**A fifth shape has two admissions wait together:** 250 histories in which two
admissions are published before either is confirmed, in one process or in two, each
judged now or earlier; a process dies, either process recovers the store, other
hands or the ledger act; and the two are confirmed in either order
(`spend::every_generated_history_of_two_admissions_that_wait_together_keeps_every_invariant`).
**Every admission that stands is held to the truth,** in every suite: the allowance
the owner really set covers every reservation really made in its epoch, each counted
in full. The tests count what they reach of that and are held to it.
Other hands make names of every form in them, at any point between the ledger's own
events and its late judgements: a turn far ahead, a hold in any epoch up to three past
the newest and for a run that has no reservation, a whole settlement, a correction with
the right seal and no place, a whole correction; names are damaged and lost; about one event in four
happens in a second process with its own memory over the same store; and a provider
charges before a settlement is published. They reach every event, every state and
every way a publication ends, late ones among them
(`spend::the_generated_histories_reach_every_event_every_state_and_every_way_a_publication_ends`,
which goes through the first 2,000 of the first shape: the rarest is first met in history 1,731),
and an event that is refused changes no name (`spend::a_refusal_changes_no_name`).

**A third suite changes one name.** 500 books that the ledger and the owner alone
wrote, from the same seed, each have every name lost alone, every name damaged alone,
and a sample of names made alone: of every form, at numbers the books have and at
numbers far outside them, for a reservation that is there and for a run that has
none. That is 8,341 names lost, 8,341 damaged and 8,750 made. Each is held
by what the fold answers, with nothing left out but the two losses in the table; and
those of one books in sixteen are also held by the checker, and followed by an
admission and dispatches asked of the ledger for real
(`spend::every_name_of_whole_books_lost_or_damaged_alone_freezes_or_allows_no_more`,
`spend::a_name_made_alone_in_whole_books_freezes_or_allows_no_more`).

| The one name | What it comes to | How many |
|---|---|---|
| A name of a reservation, lost, damaged or made | The reservation is frozen. It counts for no less than it did and no less than its requests really took, and nothing is dispatched under it. Its amount is said to be unknown exactly where it has not one hold | 15,632 |
| Of those: its one hold states its amount, and the name took no turn or is only damaged | The store goes on: no epoch needs its owner that did not | 9,576 |
| Of those: its amount is unknown, or the name is one that takes a turn and is lost or made (damaged, it takes its turn still) | The store needs its owner: in every epoch where the amount is unknown, and otherwise in the epoch of its hold | 6,056 |
| Of those: a settlement over its bound, lost | Limit (b). No less than the amount, and less than it counted by no more than the charge was over the bound. In 1 of them more can then be reserved than before, by no more than that | 19 |
| Of those: the hold, lost | Limit (c). Every bound and every charge its other names say, and the amount said to be unknown | 1,402 |
| A turn of the newest epoch, lost or made | The store needs its owner: nothing is admitted, and nothing is dispatched under a reservation of that epoch | 2,156 |
| A reset or its guard, lost, damaged or made; a turn of another epoch, lost or made | No reservation is frozen. No more can be reserved in any epoch than before, and no reservation counts for less: the epoch allows what it did or less, or it is not the newest and needs its owner | 4,839 |
| A turn, damaged; a second reset of an epoch that says more than the first | The fold answers what it answered: a turn's content is not read, and of two resets the smaller is the allowance | 2,805 |

**No one name falls under limit (a) or (d).** No publication of the ledger makes one
name of a reservation alone, so no one name made is what the ledger would write next;
and no set the ledger writes is another with one name gone.

### What the second part builds, as decided here

- **`Ledger::reserve` cannot return without having made its names.** What it returns
  can only be had from `Step::make`: a ledger that makes none does not compile.
- **`Ledger::settle`,** called inside the publication that ends a run nothing called,
  with the same step: a failure of it is a failure of that publication, the run stays
  admitted, and its caller is told.
- **The parser,** one for every reader, and a reading of reservations that answers
  each entry by itself, sound or damaged with the amount its name states.
- **`Allowance`** as above, and beside it **what the process holds of a tree's
  reservation:** its amount and what its requests took, kept as each is dispatched,
  answered and settled, and asked at every dispatch at the moment it is published.
- **A reading of one run's spend** that answers a number, "unknown, at most", or
  "unknown" with no most; and **a reading of what a reservation and an epoch count
  for** that answers a number or "unknown, at least".
- **A dispatch's refusals,** each its own: the run's allowance, the reservation by
  its names, a frozen or closed reservation, an epoch that needs its owner, and the
  tree as the process holds it.
- **The confirmation, in `Engine::admit`,** between the admission's publication and
  the start of the run. The publication is as ADR 0007 has it: the run with its
  admission, the request id's record, the reservation and the ledger's names, all or
  none. Once it is durable the executor reads the store's generation again and the
  ledger folds it and answers whether the admission stands (a part of the ledger's
  interface, `Ledger::confirm`, given the run and its amount). Only then is the
  run's thread made, its allowance and the tree's remainder set, and its action
  run. Where it does not stand, the run is ended at once, having done nothing, by
  the publication that ends a run: `Ledger::settle` closes the reservation in it,
  the run's record says it ended as not confirmed, and for a request with an id
  `requests/<id>/ended` is made in it, so that a repeat of the request is answered
  from that ending and the id does not run later. The caller is refused with a
  reason of its own. Where that publication fails, the run stays admitted and is
  never started, and recovery ends it as it ends any run its process left. Recovery
  in a live process passes over a run that waits there to be confirmed. A call from
  inside an action has no reservation and is not confirmed: its parent's was.
- **Sums that stop at the largest number,** in the fold and in every reading of it;
  no name under `spend/` makes a reader fail. A sum is kept in 128 bits, and an amount
  of the largest number of 64 is refused where it comes in.
- **The recogniser replays what the ledger checked,** as the last row of "What the
  ledger can have written" has it: each dispatch fit what its reservation had left
  when it took its place, and none follows a closing. A reading that takes a set
  without the replay takes a dispatch of 1 under a hold of nothing (history 74).
- **Places are counted, never walked.** A name can name any place a number of 64 bits
  can say: the places of a frozen reservation that hold no dispatch are the highest
  place named less the dispatches there are, and no reader loops from 1 to a place.
- **The store refuses a `put` of a name under `requests/` and `spend/`:** those names
  are made, never put, and the pin
  `store::runs::known_a_name_that_was_made_can_still_be_replaced_by_put` is turned
  round. **A ledger's name has no segment that is empty, `.` or `..`.**
- **The content of each record:** JSON on one line with a newline, written one way and
  read strictly, at most 1,024 bytes, each repeating what its name says, so that a
  content that disagrees with its name is damage:

| Name | Kind | Fields, in order |
|---|---|---|
| `held` | `spend-hold/1` | `record`, `epoch`, `run`, `reserved_micros` |
| `turn` | `spend-turn/1` | `record`, `epoch`, `turn`, `by` (`admission`, `settlement`, `closing`, `reset` or `adjustment`), `run` or null |
| `reset` | `spend-reset/1` | `record`, `epoch`, `allowance_micros`, `at_ms` |
| `epoch` | `spend-epoch/1` | `record`, `epoch` |
| `place` | `spend-place/1` | `record`, `run`, `place`, `by` (`dispatch`, `settlement`, `closing` or `adjustment`) |
| `dispatched` | `spend-dispatch/1` | `record`, `run`, `place`, `by`, `request` (its place among the requests of the run that sent it), `bound_micros`, `at_ms` |
| `settling` | `spend-settling/1` | `record`, `run`, `place` |
| `settled` | `spend-settlement/1` | `record`, `run`, `place`, `stage`, `charged_micros`, `over_bound`, `at_ms` |
| `closed` | `spend-closing/1` | `record`, `run`, `released`, `by` (`end` or `recovery`), `charged_micros`, `unresolved_micros`, `at_ms` |
| `adjusted` | `spend-adjustment/1` | `record`, `run`, `place`, `turn`, `seal`, `charged_micros`, `at_ms` |

**Notes for whoever builds the second part.** Four things the model does not show,
because of what it is:

- **The tree's tally is debited before the dispatch is published, and the debit is
  undone when the store refuses it, under one lock.** The model is one thread for each
  process, so it asks the tally, publishes, and debits, with nothing between. A real
  process is not: a call made from inside a run executes on a thread of its own, and
  two runs of one tree can each be about to publish a dispatch. If each asks the tally
  before either debits it, both are let by. So under the tally's lock: ask and debit;
  then publish; and if the publication is refused, give the amount back.
- **`charged_micros` and `unresolved_micros` in `spend-closing/1` can pass 64 bits.**
  Each is a sum over a reservation's settlements, each of which can state nearly the
  largest number. The record's type and its text have to hold a sum that stopped at
  the largest number of 128 bits, or say that it stopped.
- **`run/3` has no kind for a run that ended as not confirmed.** The run's record
  "says it ended as not confirmed" (above), and the record of ADR 0013 has no such
  outcome. Whether that is an error of a new kind, which is `run/4`, or one of the
  kinds there are with the reason in its sentence, is decided before the confirmation
  is built, with the base task that owns the record.
- **Two tests of the model are to be ported to the real fold** (task S1b):
  `spend::a_name_can_name_any_place_and_state_any_amount_and_the_fold_still_ends`, and
  the check that two readings of every reservation, written apart, give one answer
  after every event (`Books::read_apart` in the model, asserted by the generated
  tests).

### The second part, as built

What the list above asked for, as it is in the core (`crates/whizbang-core/src/spend/`
and `src/contract/`), each with the test that shows it. Tests named `ledger::`,
`admit::`, `nested::` and `runs::` are the core's (`tests/it/contract/`,
`tests/it/store/`); `context::` are unit tests beside `Allowance`.

| What | As built | Shown by |
|---|---|---|
| `reserve` cannot return without a name | `Ledger::reserve` returns a `MadeName`, which only `Step::make` gives and which is not copied; the example on `Ledger` does not compile. The fast tests run no examples, so the executor also refuses a `MadeName` that this attempt did not make: a ledger that kept one from an earlier attempt | `ledger::a_ledger_that_hands_back_a_name_it_did_not_make_in_this_attempt_is_refused` |
| A ledger's names | `Step::make` takes a text that `spend::Name` reads and that is not of the form `spend/reservations/<run>`: so no segment is empty, `.` or `..` | `admit::a_ledger_adds_to_the_step_only_names_of_its_own_and_nothing_secret` |
| `settle` | `Ledger::settle(&Closing, &mut Step)`, inside the publication that ends a run nothing called. A failure of it fails that publication: the run stays admitted and is abandoned, and its caller is told. Where the store refuses the publication for a name the ledger made in it (the place or the turn was taken meanwhile) the publication is made again, 1,024 times at most | `ledger::a_settle_that_fails_leaves_the_run_admitted_and_its_caller_told`, `ledger::an_admission_through_the_ledger_makes_its_hold_and_the_next_turn_with_the_reservation` |
| The parser, and the reading | `spend::Name`, `spend::Reading`, `spend::Names` | The `spend::name::` and `spend::reading::` tests named under "The books are names" |
| `Allowance`, signed | Debited before a request and for a share; given back to; in debt where a charge over its bound put it, in the run that sent the request and in each above; a child's remainder handed up when its end is durable, and dropped once its parent has entered its commit step | `context::allowance_tests::an_allowance_in_debt_reports_nothing_to_spend_and_a_return_pays_the_debt_first`, `…::a_charge_over_its_bound_takes_the_excess_from_the_run_and_from_each_run_above`, `…::a_parent_that_calls_twenty_children_in_turn_each_using_nothing_has_all_of_it_at_the_end`, `…::an_amount_given_back_to_an_allowance_whose_run_has_entered_its_commit_step_is_dropped`; through the executor, `nested::a_nested_call_draws_on_the_allowance_of_the_run_that_made_it_and_the_ledger_is_asked_once` |
| What the process holds of a tree | Beside the allowance: the reservation's amount and what its requests took. `Allowance::dispatch(bound, publish)` asks and debits the run and the tree in one step, under the tree's lock, before `publish` runs, and undoes the debit when `publish` refuses, fails or panics. `Dispatched::answered` counts a charge over its bound from the moment the process holds the answer; `Dispatched::settled` gives back or takes the difference | `context::allowance_tests::a_dispatch_is_debited_before_it_is_published_and_the_debit_is_undone_when_it_is_refused`, `…::two_runs_of_one_tree_that_publish_at_once_are_not_both_let_by`, `…::a_dispatch_is_held_to_what_the_process_holds_of_the_tree_whatever_its_runs_share_is` (history 75: an answer counted in memory before its settlement), and through the executor `ledger::a_dispatch_is_refused_where_the_trees_tally_in_memory_does_not_cover_it_though_the_names_do` |
| The readings' shapes | `spend::Counted` (a number; "unknown, at least") and `spend::Sent` (a number; "unknown, at most"; "unknown") | `spend::reading::each_of_the_three_shapes_of_a_reading_is_answered` |
| A dispatch's refusals | `contract::DispatchRefused`: `OverRun`, `OverReservation`, `Frozen`, `Closed`, `NeedsOwner`, `OverTree`, and `NoAmount` and `Failed` beside them. The first and the sixth are the allowance's; the others are what the meter's judgement by the names answers, which here is the fake's | `ledger::a_dispatch_is_refused_with_a_reason_of_its_own_by_each_thing_that_holds_it`, `ledger::a_dispatch_is_refused_where_its_epoch_needs_its_owner`, and the test of the tree above |
| The confirmation | In `Engine::admit`, after the admission's publication: the store is read again and `Ledger::confirm(&Reservation, &Reread)` answers. Only where it stands are the run's allowance and the tree's tally made and the run handed on to be executed. Where it does not, or where the reading fails, the run is ended at once by the publication that ends a run, as an error of the kind `not_confirmed` (ADR 0013), with `settle` in it and the id's ending made, and the caller is handed `CallError::NotConfirmed`, which is no refusal: so the request's id is not looked up a second time, and a repeat is answered with the run that ended. Where the reading failed and the run could not be ended either, the caller is handed that failure. A call from inside an action is not confirmed | `ledger::an_admission_whose_reservation_has_any_name_beside_its_record_and_its_one_hold_is_not_confirmed` (history 80), `ledger::an_admission_whose_epoch_needs_its_owner_is_not_confirmed`, `ledger::an_admission_of_more_than_nothing_whose_epoch_is_over_its_allowance_is_not_confirmed`, `ledger::a_call_of_nothing_in_a_store_in_debt_is_confirmed_and_runs`, `ledger::an_admission_made_beside_a_frozen_reservation_whose_hold_states_its_amount_is_confirmed`, `ledger::a_call_from_inside_an_action_has_no_reservation_and_is_not_confirmed` |
| Sums that stop | `spend::Sum`, of 128 bits; `spend::Amount` is never the largest number of 64, at a name, a record and `Allowance::dispatch` | `spend::reading::sums_of_amounts_near_the_largest_number_stop_at_it`, `spend::name::the_largest_number_is_no_amount` |
| The recogniser's replay | `spend::Names::reachable` | `spend::reading::a_reservation_whose_names_the_ledger_did_not_check_into_place_is_frozen` (history 74), `spend::reading::what_the_ledger_could_not_have_written_is_not_taken` |
| Places counted | `spend::Names::unexplained` is one subtraction | `spend::reading::a_name_that_names_the_largest_place_is_counted_without_walking_to_it` |
| The store's rule for a `put` | `Publication::put` and `Publication::keep` refuse a name under `requests/` and `spend/` (`store::Error::MadeOnly`), before anything is staged, whether the name is there or not | `runs::a_name_that_was_made_under_requests_or_spend_cannot_be_replaced_by_put`, which is the pin `known_a_name_that_was_made_can_still_be_replaced_by_put` turned round |
| The records | `spend::Record`, ten kinds | `spend::record::each_record_is_written_one_way_and_round_trips_to_the_same_bytes`, `spend::record::a_record_with_an_unknown_or_a_missing_field_or_any_other_bytes_is_refused`, `spend::record::a_record_over_its_bound_or_of_another_kind_or_that_says_what_none_can_is_refused`; an entry over the bound is not read: `spend::reading::with_one_reservations_content_damaged_the_reading_gives_the_others_and_that_one_as_damaged` |

**What the notes above came to.**

- *The tally debited before the dispatch is published:* as the table has it. The
  lock is the tree's, and it is not held while the dispatch is published.
- *The sums of a closing:* `charged_micros` and `unresolved_micros` of
  `spend-closing/1` are a `spend::Sum`, written as decimal digits in a string, so a
  sum that stopped is written as the largest number of 128 bits.
- *A run that ended as not confirmed:* decided with ADR 0013, which has it: an error
  whose kind is the word `not_confirmed`, in `run/3` as it is.
- *The two tests for the real fold* are task S1b's.

**Decided while it was built, and not in the first part.**

- **`confirm` and `settle` had defaults,** for a ledger that keeps no books of
  names: such a one confirmed every admission and closed nothing. **Changed with
  task S2: neither has a default.** A ledger that makes holds and left `confirm` out
  compiled, and confirmed every admission on books it never read. Every ledger now
  says what it does at each of the three; one that keeps no books, as a test's may,
  says yes and "nothing" in its own words.
- **What the review of the second part found untested, and what holds it now.** The
  tests of the second part were written after its code, and its review changed one
  rule at a time and found changes that no test saw. Each is held now:
  - *A confirmation that cannot be answered is one that says no.* Where
    `Ledger::confirm` fails, and where the store cannot be read again after the
    admission's publication, nothing runs, the run ends as `not_confirmed` and its
    caller is handed `CallError::NotConfirmed`
    (`ledger::an_admission_whose_confirmation_fails_is_not_confirmed_and_nothing_runs`,
    `ledger::an_admission_whose_store_cannot_be_read_again_is_not_confirmed_and_nothing_runs`).
    Where the run cannot be ended either, it stays admitted, counted in full and
    abandoned: its caller is told `NotConfirmed` with `ended` false where the answer
    was no, and is handed the failure of the reading where there was no answer
    (`ledger::an_admission_that_is_not_confirmed_and_cannot_be_ended_stays_admitted_and_its_caller_is_told`).
  - *A run cancelled before the answer that its admission does not stand* ended as
    cancelled in its record while its caller was told `NotConfirmed`. Decided for
    this record and ADR 0013, which say how such a run ends: as `not_confirmed`,
    whatever its signal says, in its record, in its id's ending and to its caller.
    An accepted cancellation still commits nothing (ADR 0008, "Cancellation", which
    now names the case).
  - *`settle` is held to a name it made in its own step,* as `reserve` is
    (`ledger::a_ledger_that_hands_back_from_a_runs_end_a_name_it_did_not_make_there_is_refused`).
    What is not tested is the end of the 1,024 attempts at a run's end: each attempt
    that the store refuses needs another publication to have taken the closing's
    place or turn first, and 1,024 such pairs on the test kit's disk take longer
    than a test may.
  - *`reserve` reads through the store,* as `confirm` and `settle` do. It read
    through the executor's contents, which an executor that writes is given as the
    same store: nothing changed in what is read, and no test can tell the two apart.
  - *The example on `Ledger` that must not compile is compiled,* by
    `scripts/compile-fail.sh`, which `just ci` runs: it must fail with E0451 on the
    private fields of `MadeName` and with no other error, and must compile once the
    line that writes a `MadeName` is replaced.
  - *The rules of the reading.* A probe reads 6,000 sets of names made from a seed,
    each books a ledger could have written and then changed by other hands, in the
    core and in the model, and compares every field a reservation's names give
    (`spend::probe::every_generated_set_of_names_reads_in_the_core_as_the_model_reads_it`).
    It holds two settlements at one place and the four parts of a whole chain, and
    with it are tests named for what counts less when it is wrong and for what a
    probe cannot compare: a correction under a closing that released or under none,
    two closings, two corrections at one place and a correction at a place that is
    not there or is the closing's
    (`spend::reading::a_correction_is_taken_only_under_a_closing_that_held_at_a_place_of_its_own_above_it`);
    the highest place a dispatch names
    (`…::a_dispatch_at_a_place_whose_guard_is_lost_counts_every_place_below_it`); a
    correction's amount in the most a reservation's names can mean
    (`…::a_correction_that_says_more_than_the_names_do_is_the_most_they_can_mean`);
    the replay's count of a charge over its bound
    (`…::the_replay_counts_a_charge_over_its_bound_before_the_dispatch_above_its_place`);
    the texts of no form in a seal
    (`…::a_seal_is_over_the_texts_of_no_form_too_and_marks_what_is_damaged`); the
    four parts of a correction's place
    (`…::a_correction_is_at_its_place_only_where_the_place_is_sound_and_holds_nothing_else`);
    and an entry that cannot be read, and a record of another run's reservation
    (`…::an_entry_that_cannot_be_read_and_a_record_of_another_runs_reservation_are_damage`).
    One change is no change: the recogniser asks twice that a correction's place
    holds no dispatch, since a correction's place is above the last dispatch, and
    either asking alone refuses the same sets.
  - *A call two levels down is refused over the run that made it,* which a test of
    `nested.rs` showed until a child's remainder began to be handed up
    (`nested::a_call_two_levels_down_is_refused_over_the_run_that_made_it_whatever_the_runs_above_have_left`).
- **A child's remainder is handed up now.** ADR 0008 had nothing given back to an
  allowance. With `Allowance` as this record has it, a call from inside an action
  that ends having used nothing gives its whole share back, so a parent can call
  again with it. The tests of `nested.rs` that held the old rule are restated, and
  what they showed of two calls at once is shown with the first held inside its
  action (`nested::two_calls_at_once_cannot_both_take_the_last_of_the_allowance`).
- **What the fake did where an epoch needs its owner at admission:** it declined as
  over the allowance, with nothing left. **Changed with task S2:** a ledger declines
  there with a reason of its own (`contract::Declined::NeedsOwner`, which the caller
  is handed as `Refusal::NeedsOwner` with the epoch), the fake as the real one
  (`spend::reserve::an_epoch_whose_turns_are_not_its_publications_admits_nothing_until_a_reset`).
- **The fake's allowance is a number in memory,** standing for the epoch before any
  reset while the store names no reset, and a correction never stands for it. Its
  judgement is the model's fold over the names a store holds
  (`whizbang_testkit::ledger::FakeLedger::books`).

### The real fold (task S1b)

`spend::fold` in the core is the fold of this record over the names of one generation
of a store. One reservation's names are read by `spend::reading` (the second part of
task I2: the recogniser, the sums of its places, the most its names can mean, the
seal); the fold is everything that is of more than one reservation: the epochs and
their allowances, the turns due and whether an epoch needs its owner, whether a
correction stands, each reservation's state and what it counts for, the store's
balance, one run's spend, whether an admission stands on a reading, and what the
owner's correction may say. It answers in the reading's shapes: a number, "unknown,
at least", and of a run "unknown, at most" or "unknown". It never fails, whatever is
under `spend/`.

**It is held to the model, history by history.** The tests are in
`crates/whizbang-core/tests/it/spend/fold.rs`, named `spend::fold::…`:

| What is held | Test |
|---|---|
| Each of the 89 named histories is written into a store, event by event: the names an event makes in one publication, as the ledger makes them, each with the record of its kind written through the record's type; where a name is lost or its content damaged, into a new store, the damaged ones with content that is no record, a record that says something else than its name, or another name's record. After every event the store is folded and compared with the model's fold, every field of every reservation, run, epoch and turn, and every reading the real fold works out in its own code (each epoch's balance, what is left of a reservation, what a run sent, whether an admission stands). What each history must give of the books is asserted of the real fold too | `every_named_history_written_into_a_store_folds_to_what_the_model_gives_and_to_what_it_must` |
| Every history of the six generated suites, from the seed of the model's tests: 1,000, 1,000, 250, 400, 250 and the 500 whole books. The names are folded in memory, by the one gathering a store's reading uses (`Reading::of_names`), after every event that changed a name, and compared in the same way. A store publication for each of some 22,000 states does not fit the suite's time | `every_generated_history_folds_to_what_the_model_gives_after_every_event`, `every_generated_history_of_whole_books_then_other_hands_folds_to_what_the_model_gives`, `every_generated_history_of_whole_books_then_other_hands_folds_so_in_the_second_half_too`, `every_generated_history_of_amounts_near_the_largest_folds_to_what_the_model_gives`, `every_generated_history_of_turns_moved_folds_to_what_the_model_gives`, `every_generated_history_of_two_admissions_that_wait_folds_to_what_the_model_gives`, `every_generated_history_of_whole_books_folds_to_what_the_model_gives` |
| The two readings of every reservation, the real recogniser and the one the model's checker wrote apart, give one answer after every event of every history above (`Books::read_apart`, with the real recogniser's answer compared field by field with the model's) | The same tests |
| Every correction a history asks for is judged by the real fold first, on the names as they were, and held to what the model does with it: taken, or refused for the same reason with the same number | The same tests |
| A name can name any place and state any amount, and the fold ends: a place and a turn at the largest number, two holds of the largest amount, and the store's sum stopped at the largest number of 128 bits | `a_name_can_name_any_place_and_state_any_amount_and_the_fold_still_ends` |
| Amounts of one under the largest number do not wrap: three charges of it are three times it | `sums_of_amounts_one_under_the_largest_do_not_wrap` |
| The largest number is no amount: a name that states it, as a hold, a bound, a charge or an allowance, is of no form | `the_largest_number_is_no_amount_and_a_name_that_states_it_is_of_no_form` |
| Room for a correction upward: taken only when what is left of the newest epoch holds the difference, and needing none for a reservation of closed books | `a_correction_upward_is_taken_only_when_what_is_left_can_hold_the_difference` |
| A correction's floor: it counts no less than what whole settlements say, whoever wrote it, and the command is refused below them and over the most | `a_correction_counts_no_less_than_what_whole_settlements_say_and_is_refused_below_them` |
| The next turn is the lowest that is free, with a turn far ahead and with a turn lost | `the_next_turn_is_the_lowest_that_is_free_and_never_one_past_the_highest` |
| An answer that no name states is not in the fold: the bound is counted until the settlement is durable. That a dispatch is refused meanwhile is the tree's tally, in memory, and not the fold's | `an_answer_that_no_name_states_is_not_in_the_fold_which_counts_the_bound_until_it_is_settled` |
| The plan's cases for task S1b, each on names written by hand: 17 counted between, 12 at the end and 17 interrupted; a debt that reads nothing left; 10, not 6; a reset; a child's dispatches; 20 and 27 for a damaged reservation, which stops nothing else; a record that cannot be placed; a correction after an interruption and after a freeze | `reserve_10_dispatch_two_of_5_and_one_settles_at_12_counts_17_then_12_or_17`, `an_allowance_driven_below_nothing_by_an_overrun_reads_nothing_left_and_keeps_the_debt`, `a_request_settled_over_its_bound_under_an_open_reservation_counts_the_amount_while_it_covers`, `a_reset_closes_the_books_before_it`, `dispatches_by_a_child_are_under_its_roots_reservation_and_named_for_the_child`, `a_reservation_whose_content_is_damaged_is_frozen_at_20_and_stops_nothing_else`, `a_reservation_damaged_after_it_released_at_3_counts_27`, `a_record_the_fold_cannot_place_freezes_the_reservation_its_name_gives_and_no_other`, `an_adjustment_after_an_interruption_and_after_a_freeze_counts_what_it_says` |
| Records are read through names only: content under `objects/` that no name holds, left by a publication the store refused, changes nothing; and a record that says something else than its name is damage | `the_fold_reads_records_through_names_only` |
| A text of no form: by how it begins it freezes one reservation, stops one epoch, or is placed nowhere and counted ("A name of no form"; limit (h)) | `a_name_of_no_form_freezes_the_reservation_it_starts_as_a_name_of_or_is_reported_unplaced`, `a_turn_a_reset_or_a_guard_that_does_not_parse_never_allows_more_than_the_name_it_stands_for` |

**Three things the fold decides that the model left open,** each on the side that
cannot count less:

- **What one run sent is of every dispatch that names it,** under whichever
  reservation, as in the model: a dispatch other hands made under another
  reservation can only add to it.
- **A reset or a guard of no form that names epoch 0 is placed nowhere.** No reset
  opens that epoch and nothing is allowed there as it is, so the text spoils
  nothing: it is counted with the texts of limit (h)
  (`a_turn_a_reset_or_a_guard_that_does_not_parse_never_allows_more_than_the_name_it_stands_for`).
- **A turn of no form stops its epoch for good,** as a turn that other hands made
  does: the owner's reset opens the next.

### The real ledger (task S2)

`spend::StoreLedger` in the core is the ledger an executor is given. It keeps
nothing: each answer is the real fold of one generation of the store's names, the
one an admission's step shows or the one read again after the admission's
publication. Its tests are in `crates/whizbang-core/tests/it/spend/reserve.rs`, named
`spend::reserve::…`, each with an executor the test builds with it. An allowance is
the owner's reset, which a test makes as `whizbang spend reset` does (`spend::reset`,
task S9).

| What | As built | Shown by |
|---|---|---|
| An admission | Judged on the newest epoch of the step's generation. Admitted, it makes the hold, which states the amount and the epoch, and the epoch's next turn, in the admission's publication; each record says what its name says and no more. Declined, nothing is published and the request's id stays free | `the_ledger_folds_the_steps_generation_declines_over_the_allowance_and_makes_its_names` |
| A store with no allowance | The epoch before any reset allows nothing, and so does one whose reset says nothing: a call of nothing is admitted there, under a hold of nothing, and one over nothing is refused as over the allowance, with nothing left | `a_store_with_no_allowance_admits_a_call_of_nothing_and_refuses_one_over_nothing` |
| A cost with no bound | Refused by the executor, and the ledger is not asked | `a_call_with_no_bound_is_refused_and_the_ledger_is_not_asked` |
| A preparation | Makes no reservation and no record, and asks the ledger nothing | `prepare_makes_no_reservation_and_no_record` |
| A call from inside an action | Asks the ledger nothing, and has no name under `spend/` | `a_nested_call_asks_the_ledger_nothing` |
| An epoch that needs its owner | An admission is refused with a reason of its own, a call of nothing among them, and nothing is published; a reset opens an epoch that admits again. (As cut down with task S9: a frozen reservation whose amount is unknown counts against no allowance and stops no admission, before a reset or after it. It was: "unless a frozen reservation's amount is unknown, which no reset ends") | `an_epoch_whose_turns_are_not_its_publications_admits_nothing_until_a_reset`, `a_frozen_reservation_whose_amount_no_name_states_counts_nowhere_and_stops_no_admission` |
| A damaged record (invariant 4; history 81) | Its own reservation is frozen at the most its names can mean, 25 for one of 10, and a call the rest of the allowance covers is admitted, confirmed and runs beside it. A reset whose record cannot be read allows nothing | `one_record_that_cannot_be_read_freezes_its_own_reservation_and_stops_no_other_call`, `a_reset_whose_record_cannot_be_read_allows_nothing` |
| The confirmation | `Folded::stands` on the reading made after the publication. Its third condition is of the epoch the reservation's hold names: with a reset between the publication and the reading, an admission whose own epoch is full stands though the newest allows nothing, and one whose own epoch is over its allowance does not though the newest has room. Nor does one whose epoch, or the newest, needs its owner; nor one with a name beside its record and its hold. A call of nothing is confirmed in a store in debt | `an_admission_is_held_to_the_epoch_it_fell_under_when_a_reset_came_before_its_reading`, `an_admission_does_not_stand_where_its_epoch_or_the_newest_needs_its_owner`, `an_admission_stands_only_as_the_record_and_one_hold_and_within_its_allowance`, `a_call_of_nothing_is_confirmed_in_a_store_in_debt` |
| Two admissions at once (the plan's rule 1; history 1) | Two executors over one store are each told yes for the last of the allowance, and are held inside the ledger with their names staged, after the reading a wrong ledger would decide from. Let go together, one is admitted; the other's publication is refused for the turn, it is asked again on the books as they are then, and is declined for what is left. It leaves no name, and its id is free. Under an allowance that covers both, both are admitted, at the epoch's first and second turn. The next turn, the turns due and the epoch a publication takes its turn in (`Folded::next_turn`, `due`, `turn_epochs`) are read after each | `two_executors_each_told_yes_for_the_last_of_the_allowance_end_with_one_admitted`, `two_executors_that_judge_together_under_an_allowance_that_covers_both_are_both_admitted` |
| An admission and the owner's reset at once (history 29) | The reset takes the next turn of the epoch it closes, which an admission that judged before it chose: the admission is refused by the store, judged again, and falls under the new epoch | `an_admission_that_judged_before_the_owners_reset_is_asked_again_under_the_new_epoch` |
| A crash inside an admission (the plan's rule 2) | Power is lost before each of the 24 calls that change the disk, with every choice there is of which directories and which names lose what was not durable: some 2,500 crashes, in six tests that each go through every step and take a sixth of its ways. The call may cost a quarter of a dollar under an allowance the owner's reset made durable before; that store is made once and copied for each crash. The next writer finds the run, the id's record, the reservation, the hold and its turn together, each readable and saying what its name says, the reservation open and a set the ledger could have written, or it finds none of them; and after every one of the crashes the same request runs once, or is answered with the run that is there, which nothing ran. It took the place of the executor's sweep with a ledger of one name (ADR 0007, "Admission"), and holds what that held | `a_crash_at_any_event_of_an_admission_through_the_real_ledger_leaves_all_of_it_or_none_1_of_6` to `…_6_of_6` |
| A run id the books already name | Where the books of the step's generation hold any name of the run the admission was named for, the ledger makes nothing and fails, as the model refuses such an admission (`Why::Taken`): a hold beside what is there would leave a frozen reservation, and with a second hold a store that needs its owner | `an_admission_under_a_run_id_the_books_already_name_is_not_made` |

**`Folded::stands` holds the amount to the hold** (from the review of the fold). It
took the amount as an argument and used it only to tell a call of nothing, so a
caller that passed nothing skipped the debt of its epoch. It now answers no unless
the reservation's one hold states exactly the amount it is asked of. Each of its
conditions has a case written by hand, with a call of nothing and an amount that is
not the hold's
(`spend::fold::an_admission_stands_under_each_of_its_three_conditions_and_only_for_the_amount_its_hold_states`),
and the comparison with the model, after every event of every history, asks it of the
hold's amount and of another.

**One name changed alone, against the real fold.** The model's suite of one name
lost, damaged or made alone is held by the model's fold. A sample of it is held by
the real one: 24 whole books from each of two seeds, every name lost alone, every
name damaged alone, and names of every form made alone, each folded and compared
with the model as after an event of a history; over 2,500 changes and over 750
frozen reservations for each seed
(`spend::fold::one_name_lost_damaged_or_made_alone_in_whole_books_folds_to_what_the_model_gives`,
`…_alone_folds_to_what_the_model_gives_from_another_seed`).

**What it does not do yet.** `Ledger::settle` of the real ledger closes nothing: the
closing of a reservation at its run's end is task S11's, which has the charges of a
run's requests to close with. Until it merges, a reservation the real ledger made
stays open when its run has ended, and when its admission was not confirmed, and
counts in full: over, and never under
(`an_admission_is_held_to_the_epoch_it_fell_under_when_a_reset_came_before_its_reading`,
its last lines). The binary's executor is given the ledger by task W1 (built: ADR
0015, "The binary wired").
(Built since, with task S11: "The reservation settled when its run ends". That
test's last lines now hold that the reservation is closed and released.)

### The meter (task S5)

As the plan has it after the strand was cut down ("The strand cut down"): one pull
request, ordinary tests, no sweep and no race of its own. Tests named `spend::meter::`
are in `crates/whizbang-core/tests/it/spend/meter.rs`, each through an executor built
with the real ledger and the meter.

**`spend::Meter` stands in the meter's place** (ADR 0015, "A wrapper around each
request"; `Engine::with_wrapper(Place::Meter, …)`). It is built with the store its
executor writes and a clock. For each request:

- It asks `spend::request_bound` (task S4) of the declaration of the request's own
  backend and what the run has left. A request that is not paid, to a local backend
  that states no price or to one that says it is free, passes with nothing held and
  nothing recorded (`spend::meter::a_free_request_to_a_local_backend_makes_no_record`).
  A request to a backend with no enforceable bound is refused, whether or not
  anything was reserved for its run
  (`spend::meter::a_request_to_a_backend_with_no_bound_is_refused_by_the_meter_whether_or_not_its_run_reserved`).
  A bounded request of a run nothing was reserved for passes unrecorded: it has no
  reservation to be recorded under (the same test).
- It debits the bound with `Allowance::dispatch`, whose `publish` is the dispatch's
  publication: the next `place` under the root's reservation and `dispatched/…` at
  it, with the time. Only when that is durable is the backend called
  (`spend::meter::the_backend_stopped_at_its_barrier_finds_its_own_dispatch_durable_before_it_was_called`,
  which reads the store's folder with a reader of its own while the backend holds
  the request).
- A request over what its run has left is refused, is not sent and has no dispatch,
  and the refusal is in its run's record with what and why
  (`spend::meter::a_request_over_what_its_run_has_left_is_refused_unsent_with_no_dispatch_and_is_in_its_runs_record`).
  A request the gate refused never reaches the meter and has no dispatch
  (`spend::meter::a_request_the_gate_refused_has_no_dispatch_record`).
- A dispatch whose publication the store refuses, as at its generation limit, sends
  nothing, and its debit is undone
  (`spend::meter::a_dispatch_the_store_refuses_at_its_generation_limit_sends_nothing_and_its_debit_is_undone`).
- Once it has debited it holds something for the request, so a backend's panic under
  it leaves the run abandoned, with the dispatch durable and unsettled: counted at
  its bound
  (`spend::meter::a_backend_that_panics_under_the_meter_leaves_the_run_abandoned_and_its_request_counted_at_its_bound`).

**What it judges by.** The allowance and the tree's tally, under the tree's lock
(`Allowance::dispatch`), and the names of its own reservation, read alone
(`spend::Names::of_run`, which reads the content of no other reservation's names). A
reservation with a closing, or whose names are not a set the ledger could have
written, takes no dispatch,
and neither does one whose names leave less than the bound, whatever the run's
allowance has: a dispatch that did not fit would itself freeze the reservation
("What the ledger can have written", the last row)
(`spend::meter::a_reservation_whose_names_read_as_frozen_or_closed_or_leave_too_little_takes_no_dispatch`,
for all three). It folds nothing, so it does not know an epoch that needs its owner.

**Decided while it was built.**

- **A place taken meanwhile is chosen again,** 64 times at most: another dispatch of
  the tree published first, and the store refused this one whole.
- **Every refusal of the ledger's is recorded as `request_over_allowance`.** A record
  has four kinds for a request that was not sent (ADR 0013), and none for a
  reservation that is closed or frozen, or a store that refused the publication. The
  sentence says which.
- **`request` in a dispatch's record is the request's place among the paid requests
  its run dispatched,** from 0, as the reservation's names count them. A wrapper is
  not told a request's place among all the requests of its run; when it is (task
  S10 needs it for the run's spend lines), the meter writes that. (It is told, and
  writes it, since task S10: "The run's spend lines".)
- **A request sent after its run's action came back does not reach the meter:** the
  handle's tally refuses it first (ADR 0015). `Allowance::dispatch` called by an
  action itself, outside any wrapper, is not refused there (`docs/carry-over.md`).
- **Nothing put the meter in the binary's executor** until task W1, which gave it
  the ledger, the bound of a call and the meter together (ADR 0015, "The binary
  wired").

### Settlement of each attempt (task S5b)

As the plan has it after the strand was cut down: one pull request, ordinary tests.
Tests named `spend::settle::` are in `crates/whizbang-core/tests/it/spend/settle.rs`,
each through an executor built with the real ledger and the meter.

**When a request ends, the meter settles it by the stage it reached** (ADR 0010;
`Fault::stage`, never a fault's kind), in `Open::after`:

| How it ended | Stage | Charged |
|---|---|---|
| An answer that states a charge | `answered` | That charge |
| An answer that states none | `answered` | Its bound |
| A fault that says the request was not sent | `not_sent` | Nothing |
| A fault that says it was answered | `answered` | The charge the fault carries, or the bound |
| A fault that says nothing settled it, or says no stage: a timeout, a cancellation with no answer | `unresolved` | Its bound |

(`spend::settle::a_request_charged_under_its_bound_is_charged_that_and_its_run_spends_the_difference_on_its_next`,
`spend::settle::a_request_that_succeeds_with_no_charge_is_charged_its_bound`, and one
test over a table for the faults and the timeout,
`spend::settle::a_fault_at_each_stage_and_a_timeout_settle_as_not_sent_at_nothing_answered_at_its_charge_and_unresolved_at_its_bound`.)

**A settlement is one publication** ("What each publication makes"): `settling/…`,
`settled/…`, the next `turn` of the epoch the reservation's hold names, and the next
`place` when the charge is over the bound. The next turn is the lowest that no
`spend/turn/<epoch>/…` of the generation names, read from those names alone: after
an admission and three settlements the epoch's turns are those its publications
took (the first test above). A turn or a place taken meanwhile is chosen again, 64
times at most. A reservation with no hold takes no turn: the fold owes none for it.

**Then the allowance** (`Dispatched::answered`, then `Dispatched::settled` once the
settlement is durable). What the bound held beyond the charge goes back to the run
that sent the request, which can spend it on its next (the first test above). A
charge over its bound is settled at the charge, its record marked `over_bound`, and
the excess is taken from the run that sent it and from each run above it, below
nothing: each reports nothing to spend and owes the excess
(`spend::settle::a_charge_over_its_bound_is_settled_at_the_charge_marked_as_over_and_taken_from_its_run_and_its_parent`).

**The cases that came from the tasks that were cut.**

- *A cancellation after dispatch* (was S5c): a run cancelled through
  `Executor::cancel` while its request is with the backend, whose request then ends
  with no answer, is charged the bound as unresolved
  (`spend::settle::a_run_cancelled_after_dispatch_whose_request_ends_with_no_answer_is_charged_the_bound_as_unresolved`).
- *A nested call* (was S7): a parent, a child and a grandchild each send one paid
  request. The store holds one reservation and three dispatches under it, each
  naming the run that sent it, each settled; and the parent sends its own request
  from what the child handed up
  (`spend::settle::a_parent_a_child_and_a_grandchild_each_send_one_request_under_one_reservation_and_the_parent_spends_what_the_child_did_not`).
- *A retry* (was S7): each attempt is a request like any other, with its own
  dispatch and settlement, and one that what is left cannot hold is refused before
  dispatch
  (`spend::settle::a_child_that_retries_a_failed_request_has_an_attempt_for_each_and_a_retry_that_does_not_fit_is_refused_before_dispatch`).

**A settlement that cannot be published** fails the wrapper, so the run is abandoned
(ADR 0015): the dispatch stays with no settlement, counted at its bound in the
process and in the books, and nothing of it is given back
(`spend::settle::a_settlement_whose_publication_fails_leaves_the_dispatch_unsettled_counted_at_its_bound_and_the_run_abandoned`,
at the store's generation limit).

**Not here.** The reservation is not closed when its run ends (task S11), and a
run's record does not list its paid requests (task S10): the meter notes nothing
in the run's place for facts yet. (The closing is built since: the next section.)

### The reservation settled when its run ends (task S11)

As the plan has it after the strand was cut down: one pull request, ordinary tests.
Tests named `spend::close::` are in `crates/whizbang-core/tests/it/spend/close.rs`,
each through an executor built with the real ledger and the meter.

**`StoreLedger::settle` closes the reservation of a run nothing called, in the
publication that ends the run** ("The second part, as built", the row on `settle`).
It reads the names of that one reservation (`spend::Names::of_run`), as the meter
does, and the content of no other's; it folds nothing. It makes the closing, the
reservation's next place, and the next turn of each epoch a hold of it names
("What each publication makes"). What the closing says is what the names say:

| The names of the reservation | The closing | What it counts for afterwards |
|---|---|---|
| A set the ledger could have written, with every dispatch settled with its whole chain | `closed/<run>/released` | What its requests were charged, an unresolved one at its bound among them. The rest is free for the next call |
| The same, charged more than was reserved | `closed/<run>/released` | What it was charged: nothing of it is given back |
| A dispatch with no settlement: a settlement that could not be published, or a backend's panic, in a run it called | `closed/<run>/held` | The larger of its amount and what is known, as interrupted, until the owner's reset |
| Not a set the ledger could have written | `closed/<run>/held` | The most its names can mean, as frozen |
| A closing already names it, or no name under `spend/` names the run | Nothing is made | As before |

- A run that reserved 1,000,000 and was charged 300,000 leaves 700,000: a call of
  750,000 is then refused with 700,000 left, and one of 500,000 runs. The same of a
  run that completed, one whose action failed, one whose action panicked and one
  that was cancelled
  (`spend::close::a_run_that_reserved_ten_and_spent_three_leaves_seven_for_the_next_call_however_it_ended`).
- A run that sent nothing releases all of it
  (`spend::close::a_run_that_spent_nothing_releases_all_of_it`).
- A request that no answer settled has a durable settlement at its bound, so the
  closing releases; the bound stays counted, as unresolved
  (`spend::close::a_run_with_an_unresolved_request_releases_only_what_is_not_unresolved`).
- A tree charged more than it reserved
  (`spend::close::a_tree_charged_more_than_was_reserved_is_closed_at_what_it_was_charged`).
- A child's settlement cannot be published, so the child is abandoned (ADR 0015) and
  its dispatch has no settlement; its root then ends, and the reservation is closed
  as interrupted, counted at all that was reserved, with nothing released
  (`spend::close::a_root_with_a_dispatch_that_has_no_settlement_ends_closed_as_interrupted_with_nothing_released`).
  This is the case that the row on what the meter owes left untested
  (`docs/carry-over.md`): the closing is judged by the names, and a child's
  abandonment marks the child alone.
- The end of a nested call asks the ledger nothing and closes nothing
  (`spend::close::a_nested_calls_end_settles_nothing_in_the_ledger`).
- A reservation that other hands closed while its run executed gets no second
  closing at the run's end, which would release what the first held
  (`spend::close::a_reservation_that_a_closing_already_names_is_left_as_it_is_when_its_run_ends`).

**Decided while it was built.**

- **A reservation with no hold takes no turn at its closing,** as at a settlement
  ("Settlement of each attempt"): since the strand was cut down it counts in no
  epoch, and the fold owes no turn for it, so a turn taken in the newest epoch would
  make that epoch need its owner
  (`spend::close::a_reservation_with_no_hold_is_closed_as_interrupted_and_takes_no_turn`;
  for a settlement, the unit test
  `spend::meter::tests::a_reservation_with_no_hold_takes_no_turn_at_settlement`).
  "In the newest where it has none" ("What each publication makes") is what the
  model and the test kit's fake still do; no reservation the real ledger makes has
  no hold.
- **An admission that is not confirmed is closed by the same call:** the executor
  ends such a run at once, with `settle` in that publication. Nothing was sent under
  it, so where its names are whole all of it is released
  (`spend::reserve::an_admission_is_held_to_the_epoch_it_fell_under_when_a_reset_came_before_its_reading`,
  its last lines).
- **The amount the executor passes (`Closing::reserved_micros`) is not read:** the
  hold's name states the amount, and the closing's sums are the names'.
- **Tests of an admission that need a reservation to go on counting** leave it open:
  a run of those tests does nothing and ends at once, and its end now releases all
  it reserved. `spend::reserve::`, the tests of `admit.rs` and the binary's tests of
  `spend status` use, where they say so, a ledger that reserves and confirms as the
  real one does and closes nothing: the books are then those of a run that is still
  executing, or of one whose process died before its end.

**What it does not close.** A run that is abandoned in a live process has no end
published, and a run whose process died has none either: each reservation stays
open and counts in full until the owner's reset ("The strand cut down"). The
binary's executor had no ledger until task W1, whose test of a store a killed
process left is through the binary (ADR 0015, "The binary wired").

### The run's spend lines (task S10)

As the plan has it after the strand was cut down: one pull request, ordinary tests.
Seeing what each run cost matters more here than enforcing it, so this is the part
of the record a person reads. Tests named `spend::lines::` are in
`crates/whizbang-core/tests/it/spend/lines.rs`, each through an executor built with
the real ledger and the meter.

**Every paid request of a run is a line of `spend.requests` in its record** (ADR
0013, "A paid request"). The meter notes it in the run's place for facts
(`RunFacts::paid`) in `Open::after`, once the request's settlement is durable, from
what it holds of the request: the bound, the stage and the charge it settled at.

| How the request ended | `stage` | `reserved_micros` | `settled_micros` | `unresolved_micros` | `over_bound` |
|---|---|---|---|---|---|
| An answer that states a charge | `answered` | Its bound | The charge | 0 | Whether the charge is over the bound |
| An answer that states none | `answered` | Its bound | Its bound | 0 | No |
| A fault that says it was not sent | `not_sent` | Its bound | 0 | 0 | No |
| Nothing settled it: a timeout, a cancellation with no answer | `unresolved` | Its bound | Its bound | Its bound | No |

- The records of a completed, a failed and a cancelled run each list every paid
  request, in the order they were sent
  (`spend::lines::the_records_of_a_completed_a_failed_and_a_cancelled_run_each_list_every_paid_request`).
- A request settled over its bound is listed at what it was charged and marked as
  over; one whose answer stated no charge is listed at its bound, never at nothing
  (`spend::lines::a_request_settled_over_its_bound_is_listed_at_what_it_was_charged_and_marked_as_over`).
- The sums of a run's lines are what the fold holds for that run (`Folded::runs`):
  charged, unresolved, and nothing outstanding
  (`spend::lines::the_sum_of_a_runs_lines_equals_what_the_fold_holds_for_that_run`).
- A nested call's lines are in the child's record and only there, and the parent's
  record names the child among its steps
  (`spend::lines::a_nested_calls_lines_are_in_the_childs_record_and_the_parents_record_names_the_child`).
- A judge's handle counts its own request and sends it through the same places: its
  paid request is a line as a filler's is
  (`spend::lines::a_paid_request_to_a_judge_is_a_line_as_a_fillers_is`).

**A wrapper is told a request's place** (`Wrapped::request`; ADR 0015, "A wrapper
around each request"): its place among the requests its run sent, from 0, which is
where the record lists the model that answered it (`models`) and what a line names
it by. A free request takes a place and has no line, so the lines of a run that
asked a local segmenter between two paid requests name places 1 and 3 (the third
test above). **The dispatch's record names the same place** as `request`, where it
was the request's place among the paid requests of its run.

**Decided while it was built.**

- **The note is made after the settlement is durable, and only then.** A request
  whose settlement could not be published, and one whose backend panicked, have no
  line: the run is abandoned and has no record from this process. The books hold
  the dispatch, at its bound.
- **A note that is not taken fails nothing.** `RunFacts::paid` answers no when the
  record lists as many as it may, which is as many requests as a run sends, or when
  the run's end has begun, which no request outlasts. The books hold the request
  either way, and `spend status --run` shows it.
- **A request refused before dispatch has no line:** nothing was held for it. It is
  among the run's refusals (ADR 0013).
- **What reads the lines is built already:** `whizbang runs show` prints each line of
  a record (ADR 0013). The binary's executor had no meter until task W1, whose test
  is of that command; with it the command prints each amount in dollars and what
  the run cost (ADR 0015, "The binary wired").

### Limits, each stated

- **The model is not the ledger.** It publishes names in memory; no test of it touches
  a store. The real fold is held to it by task S1b, history by history.
- **A crash is whole in the model:** a publication is there or it is not. A crash
  inside a publication is the store's to keep whole, and each ledger task has its own
  sweep.
- **The model has two processes and no more,** each with its own runs and requests,
  and a publication is whole in both.
- **A confirmation is one event in the model:** the reading and what follows from it.
  Two processes that read at once, each before the other's closing, are not in it;
  nor is a publication of anything else that lands between a confirmation's reading
  and the closing that backs it out. And three processes are not: what a third
  would do between two others is done here by one of the two.
- **The fold reads every name each time.** What that costs as a store ages is on
  `docs/carry-over.md`, with milestone 2.

**The guarantee has eight limits, (a) to (h).** Five are things the fold does not hold,
the sixth is a ruling it cannot meet, the seventh is what the owner's own
correction means, and the eighth is a text the fold can place nowhere.

**What the owner accepted with the plan's rule, first, in money.** A frozen reservation
whose hold states its amount stops nothing else. Where its names still state what was
spent, nothing is under. In two cases they do not, and the store goes on beside a
reservation that counts less than it cost:

| Limit | History | What happens | Counted | Committed | Under by | Bounded by |
|---|---|---|---|---|---|---|
| (a), one hold made in the place of the ledger's | 86 | A reservation of 10 has 10 outstanding. Other hands make a hold of nothing for it and take away the ledger's hold and the dispatch. It is frozen and counts 0. The whole allowance of 30 is then admitted and sent | 30 of 30 | 40 | 10 | What was outstanding under the reservation whose hold was replaced: no more than its amount, and any charge over a bound under it |
| (b), a settlement over its bound whose name is lost | 87 | A reservation of 1 sends a request of nothing that is charged 15: 15 counted, 5 left of 20. The settlement's name is lost. It is frozen and counts 2. The 18 that then read as left are admitted and sent | 20 of 20 | 33 | 13 | The charge less the amount, the bound and the amount again. A charge over a bound has no bound of its own, so this has none |

Before the owner's choice the store took no admission in either, so the same names
were under by the same amount and nothing more was spent on top. Both need other
hands: three names changed in the first, one lost in the second. Each is asserted by
`spend::the_limits_accepted_with_a_frozen_reservation_that_stops_nothing_else_each_count_exactly_this_much_less`,
so that neither can be taken for covered.

- **(a) A forged set that the recogniser takes, and that is consistent in its stages,
  is believed.** Where other hands made names of a reservation and the recogniser
  still takes the set, the fold counts by it. So a `settling` and a `settled` at a
  charge the stage allows lower their place, and a recovery then releases
  (`spend::known_what_the_names_cannot_say_is_not_counted`); a correction with its
  place, its turn and the right seal is a correction
  (`spend::a_correction_never_counts_under_what_whole_settlements_say_whoever_wrote_it`);
  one hold made in the place of the ledger's states the amount and the epoch, and
  still does when the owner corrects that reservation
  (`spend::known_one_hold_made_in_the_place_of_the_ledgers_states_the_epoch`);
  **exactly one hold standing in the place of the ledger's lost hold states the
  reservation's epoch and its amount: no name tells it from the ledger's own.** That
  is so in a reservation that is frozen too, where it can state less than is
  outstanding: a request whose dispatch is lost with the ledger's hold is then not
  counted, and the books say a number
  (`spend::known_one_hold_made_in_the_place_of_the_ledgers_states_the_amount_of_a_frozen_reservation_too`);
  and a
  reset with its guard is an allowance, of a whole later epoch, or of the owner's own
  epoch when the owner's reset is lost
  (`spend::an_allowance_is_its_resets_only_with_the_guard_and_with_neither_damaged`).
  Any part of such a chain without the rest lowers nothing; and a chain that took no
  turn leaves its epoch needing its owner.
  **What this limit can cost has no bound.** A forged reset with its guard is any
  allowance its writer names. A forged settlement at nothing gives up the whole bound
  of a request that was sent, and a forged correction all that a reservation counted
  beyond its whole settlements.
- **(b) A charge that no name states is not counted.** A provider's charge over the
  bound that is in memory when the process dies, and a settlement over its bound
  whose name is lost, leave the reservation at its amount or more, and under the
  charge when the charge was over that (history 52,
  `spend::known_what_the_names_cannot_say_is_not_counted`, history 53). After the
  loss of the name the reservation is frozen with its hold, the store goes on, and
  what the charge was over the frozen sum can be reserved again (history 87, in the
  table above). Invariant 1
  is of what the records show.
  **What this limit can cost has no bound.** A charge over a bound has no bound of
  its own: a provider that charges more than it quoted can charge anything, and a
  charge that no name states is under by all of it beyond what the reservation's
  other names still count.
- **(c) When the ledger's own hold of a reservation is lost, its amount is unknown,
  whatever holds other hands leave in its place.** The amount was in that name
  alone. The reservation counts for what its names say, in every epoch; a request
  whose dispatch is lost as well has then no name that states any amount, and is not
  counted (history 69). The round before had this only where every hold was lost;
  the fourth made two holds of nothing. **Where no hold is left, or more than one,
  the books say that they cannot tell:** the reservation is frozen, what it and the
  store count for is "unknown, at least", and no admission and no dispatch is taken
  until its owner corrects it: the exception to the plan's rule ("Invariant 4, and
  what the owner chose"). Where other hands leave exactly one hold in its
  place, that hold is believed for the amount: limit (a). With the ledger's hold
  still there among others, the largest amount named is no less than its own, and
  nothing is lost.
- **(d) A loss of several names that leaves a set the recogniser takes is believed**
  (`spend::known_a_loss_that_leaves_what_the_ledger_could_have_written_is_believed`).
  No loss of one name does. These do: the last places of a reservation, each with
  everything at it (a dispatch that is not settled and its place; a closing and its
  place; a correction and its place); a settlement with its guard, and the place it
  took when it was over its bound and that place is the last; every name of the
  reservation; and a reset with its guard when it is the newest. Each but the
  request also leaves the epoch's turns one more than its publications account for,
  so the store needs its owner and nothing is dispatched in that epoch, unless the
  turn is lost with it. What is believed is the count. A dispatch that follows is
  still held to what its process holds of the tree, while that process lives
  (history 73).
  **The turns stop one epoch, and a reset opens another.** An epoch whose turns are
  not those its publications took needs its owner, and that is of that epoch alone.
  Reset to 20; a reservation of 10 sends a request of 5 that is settled at 12; the
  owner resets to 20 again. Other hands then take away that settlement, its guard
  and the place it took, and leave its turn. The old epoch has a turn more than its
  names account for and needs its owner; its reservation reads as open, at 10, where
  12 were spent. The new epoch's turns start again and are whole: it reads nothing
  counted and admits its full 20 (history 88;
  `spend::the_limits_accepted_with_a_frozen_reservation_that_stops_nothing_else_each_count_exactly_this_much_less`).
  Nothing is under in the new epoch, whose allowance the owner set after the charge;
  the old epoch's books are under by 2, which is this limit and limit (b).
  **What this limit can cost has no bound either.** Every name of a reservation lost
  is all that it spent, and every name under `spend/` lost is all the store spent and
  its allowance with it: an empty store allows nothing, and counts nothing.
- **(e) One publication whose judgement was made before other hands planted,
  removed or damaged a name may still land.** The store refuses a publication only
  for a name it makes, and each judgement is published once. **"Once" is once for each
  judgement.** Where several threads of a process each hold a judgement made before,
  as the runs of one tree can, each of them may land; those of one tree are together
  held to what that tree really has left, because the process's memory of the tree is
  asked and debited for each as it is published. What that gives a
  dispatch and what it gives an admission are two things.
  **A late dispatch stands.** One judged before a place's guard was lost, or before
  other hands made a closing without its place, is published (histories 59 and 48),
  where the books as they were at publication would have refused it: there the
  reservation is frozen. It stands after a closing only under a reservation that
  other hands touched, and never over what was really reserved, because the
  process's memory of the tree is asked when it is published (history 71).
  **A dispatch judged before the store came to need its owner may still be
  published once.** It is held, in the process's memory, to the tree's real
  remainder, so it spends nothing that its own reservation did not already count in
  full. It is not held to the refusal of an epoch that needs its owner, which a
  judgement made earlier cannot know: the names it makes collide with nothing that
  made the store need its owner, its own reservation untouched among them. **A
  dispatch judged after is refused,** one that is judged again among them. So with
  another reservation's hold lost, or one turn lost, a request of 10 that was judged
  before is placed under a reservation of 10: what is counted is what it was, and
  the next request of that run is refused, for what its run has left when it was
  judged before and for the owner when it is judged then (histories 78 and 79).
  **A late admission may be published, and that is all: it is not confirmed unless
  the books then read sound and within the allowance, and an admission that is not
  confirmed runs nothing** ("An admission is confirmed before anything runs under
  it"). One judged before a turn was lost is published, is not confirmed, and is
  closed with everything released (history 61). So is one whose turn fills the
  place of a turn that other hands took away, where the books then read over the
  allowance (history 76). What the limit leaves of an admission is names, a record,
  a hold and a turn, and then a closing, a place and a turn; and a reservation that
  counts in full from its publication to its closing, which is over and never
  under.
  Where the reading made after its publication is sound and within the allowance,
  a late admission stands, and not by this limit: the names as other hands left
  them, its own among them, are books the ledger could have written, and it is held
  as any admission is, to the truth with limits (a) to (d) in it. So one stands
  that falls under an epoch other hands closed by opening a later one
  (`spend::known_an_admission_judged_before_other_hands_opened_an_epoch_falls_under_the_old_one`);
  one judged before a settlement that other hands then took away whole, with its
  guard, its place and its turn, which is over what was really left by the charge
  that was lost and no more, limits (b) and (d)
  (`spend::known_an_admission_judged_before_a_settlement_that_other_hands_took_away_whole_is_published`);
  and one whose turn fills the place of a turn that was lost, where the books then
  read within the allowance
  (`spend::known_a_late_admission_whose_turn_fills_the_place_of_one_that_was_lost_is_published`).
  The second review round did not accept this limit. For an admission the fifth
  closed it, with no change to what the store refuses. For a dispatch, closing it
  needs the store to refuse a publication for something other than a name it
  makes, such as the generation it judged on having changed, which is not ADR
  0007's rule and is not decided here.
- **(f) A correction stands again when the names return to exactly the set it was
  sealed over.** The ruling of the second round, that a correction never stands
  again once a later name ended it, cannot be met by a fold of names: made and taken
  away again, the name leaves the same names the owner corrected ("A person's
  correction").
- **(g) A correction writes off what is outstanding, and what is outstanding may
  still be charged.** This is the one way past an allowance in which no other hands
  act, and it is the owner's own act. A reservation of 10 is all of an allowance of
  10, and its request of 10 is in flight. A second process recovers the store, which
  closes the reservation as `held`. The owner corrects it to nothing. 10 more are
  admitted, sent and settled. The first process then settles its request at 10: 20
  are counted against 10, and the debt is 10 (history 82). At the second admission
  the books counted 10 while 20 were committed. Where the first process dies with
  the provider's answer, the next process recovers and the owner then corrects, the
  reservation stays corrected at nothing and the
  books count 10 where 20 were charged, for good: no name ever states the first
  charge, which is limit (b) as well (history 83;
  `spend::a_correction_writes_off_a_request_in_flight_and_what_it_then_costs_is_past_the_cap`).
  **It is bounded:** by what was outstanding under the corrected reservation when it
  was corrected, each such request at its bound; by whatever such a request is
  then charged over its bound; and by what the reservation's settlements that are not
  whole say, which the correction's stated meaning writes off as well ("A person's
  correction"). So where other hands damaged a place whose request was settled at 10,
  a correction to nothing writes off 10 that were charged: 20 counted of 20 where 30
  were charged, in the test named above. It never reaches what settlements with
  their whole chain say, which a correction cannot go under.
  **"Interrupted" does not say that nothing is in flight.** Recovery closes every
  reservation whose run is not open in the process that recovers, nor waiting there
  to be confirmed. Where that is
  another process than the one that runs it, the run is alive; and where the process
  is dead, the provider it asked may still charge.
  **How far the store's one writer keeps this away.** A store has one writer at a
  time (`SPEC.md` section 3, "One writer"): a process holds a lock on the store for
  as long as it has it open to write, a second that opens it to write is refused,
  and recovery and a correction are both made by a writer (the plan's task I4: one
  way to a writer, with recovery before it). So while the process that
  runs a request lives and holds the store, no other process can recover it or
  correct a reservation in it, and history 82 does not happen. It is reachable only
  where that lock does not hold, which the store's module names: a file system on
  which the lock does not work, and a lock file removed or replaced under a writer.
  History 83 needs none of that, and is written with one writer at a time. The
  process dies, its lock goes with it, the next writer recovers and the owner
  corrects, as they are meant to; and the request the dead process sent is charged
  all the same. That is the case to expect.

- **(h) A text that is placed nowhere changes no balance.** A text under `spend/`
  that is of no form belongs to a reservation or to an epoch only by how it begins
  ("A name of no form"). One that begins as nothing the ledger writes, or whose run is
  no run id, or whose epoch is no number, belongs to nothing. The fold counts how
  many there are (`Folded::unplaced` in the core), so that whoever shows the books
  says so, and changes no balance for them: the store is not stopped.
  **What it can hide is a name.** A name of the ledger's that is garbled so far that
  neither its run nor its epoch can be read is, to the fold, that name lost. One name
  lost freezes its reservation, as every loss of one name does, or leaves its epoch
  needing its owner. Several garbled so are a loss of several names, which is limit
  (d): where what is left is a set the ledger could have written, it is believed.
  With every name of a reservation and the turn its admission took garbled, nothing
  of it is counted anywhere: a reservation of 10 with 10 outstanding reads as
  nothing, five texts are placed nowhere, and the whole allowance of 30 is admitted
  where 40 are then committed (history 89;
  `spend::fold::every_named_history_written_into_a_store_folds_to_what_the_model_gives_and_to_what_it_must`).
  **It has no bound,** as (d) has none. What tells it from a loss is the count of
  texts placed nowhere, which is not nothing only where other hands were.

**What the checker leaves out, each under one limit.** The checker (`Books::check` and
`Books::checked` in the model) takes its floor from the truth. These are its
exemptions, all of them:

| Limit | What the checker does not hold | Where in the model |
|---|---|---|
| (a) | The count of a reservation where other hands made a name of it and it is not frozen: the fold is believed when a second reading of the names, written apart from the recogniser and with the stage rule of ADR 0010 among its rows, takes the set too. A set the fold took and that reading does not is a failure | `held_to` and `believable` |
| (a) | The count of a reservation that a correction other hands made corrects, when the correction's name, the place and the turn it names are there and its seal is right | `held_to` and `forged_correction` |
| (a) | The epoch of a reservation whose one hold other hands made in the place of the ledger's: it is not held to the epoch it was reserved in, and is counted against the newest allowance only where the fold has it there. And its amount: a request whose dispatch is lost with the ledger's hold is held to nothing, where the reservation is frozen too | `replaced` in `check`; `here` in `really_left`; `amount_lost` and `unnamed` |
| (a) | The allowance: of an epoch after the newest that other hands made whole, reset and guard, nothing is held; and where the owner's reset of the newest epoch is lost, the allowance is the names' | `really_left` |
| (b) | A request whose settlement's name is lost is held to its bound at most; and a provider's charge that was never settled is not in the truth | `Asked::floor` |
| (c) | A request whose dispatch is lost, under a reservation whose own hold, the one the ledger made, is lost, is held to nothing, where no hold is left or several are (with exactly one, the row of (a) above). Nothing else is: a request whose dispatch is still named is held to its floor. And the books are held to saying it: such a reservation, frozen with no hold or with several, answers "unknown, at least", and the store with it; and one that answers so with the ledger's hold still there counts no less than its amount | `amount_lost` and `unnamed`; `check` |
| (d) | The count of a reservation where other hands only lost names of it and it is not frozen: believed when the second reading takes what is left | `held_to` and `believable` |
| (d) | A reservation of which every name is lost is not held at all | `held_to` |
| (d) | Where the newest epoch is lost, the allowance is not held | `really_left` |
| (e) | A dispatch published late is not held to a closing where other hands did something between its judgement and it and touched its reservation; and not to the refusal of an epoch that needs its owner, where on the names it was judged on the epoch did not need it. One judged, or judged again, on names where the epoch needs its owner is held to it. An admission published late, where they did something between, is not held to the turns the ledger took since its judgement: that is of its publication, and of nothing else. Whether it stands has no exemption | `unseen`, `stopped_then` and `between` in `checked` |
| (f) | Nothing. The truth keeps the names the owner's last correction left, and the correction is the owner's again whenever the names are those | `held_to` |
| (g) | While the names of a reservation are those the owner's last correction left: every request under it that has no settlement with its whole chain is left out of its floor, and out of what an admission that stands is held to | `corrected` in `held_to` and in `really_in` |
| (h) | Nothing of its own. In the model a name garbled past reading is that name lost and a name that is placed nowhere made (`Name::Unplaced`), which the fold counts and reads nothing from: what is not held is what the rows of (d) leave out for the loss | `held_to` |

The first row and the seventh are one piece of code, told apart by what the truth wrote
down: a name other hands made, or only names lost. What a correction writes off was not
in the table until the review of the pull request: it is the correction's stated
meaning ("A person's correction"), and it is money that can be spent a second time,
so it is the row of (g). No
dispatch is in it: whatever the names say, what its process holds of the tree
holds each dispatch to what was really reserved. The round before had a run's own
allowance do that, which it does not: a child's share is not what the tree has left.
**Nor is an admission that stands.** It is held to the truth: the allowance the owner
really set for its epoch covers every reservation really made there, each counted
in full until it is released or corrected, this one among them. Limits (a) to (d)
are in that truth as they are in any count, by the rows above, and nothing is left
out for an admission as such. And it is held to the reading made after its
publication, which the checker makes again in its own code: its reservation whole
and open, neither its epoch nor the newest needing its owner, its epoch within its
allowance. An admission that was backed out is held to this: nothing was dispatched
under it, its run does not execute, and its reservation is released or still counts
in full.
Four exemptions were taken out after the third round and two made narrower; the
fourth changed the row of (c) and no other; the fifth took out the two that the row
of (e) gave an admission's money ("Review rounds").

### The strand cut down

By the owner's decision of 2026-11-15 (the plan, "Decisions", the last item), spend
control is a guard and a record. It keeps a mistake from running up a bill, and it
shows what each run cost. It errs toward letting work run, it may count more than was
spent, and it is not held to the cent or proven against a crash. This section says
what of this record stays built, what is not built, the one thing that changes, and
what that costs. The sections above are left as they were written; where one of them
and this section differ, this one is the later decision.

**What stays built,** as merged or in review on that day: the names and their grammar,
the records, the reading of one reservation, the fold, the model and its table of
histories as the fold's test, the real ledger's `reserve` and `confirm` (task S2),
the allowance with what a process holds of a tree, the per-call cap and the hour of
an estimate.

**What is still to be built,** each as one pull request with ordinary tests (the plan's
tasks): a paid request with no enforceable bound is refused (S4); the meter quotes a
request, debits it and makes its dispatch durable before it leaves (S5); each attempt
is settled by its stage (S5b); a reservation is closed when its run ends (S11);
`whizbang spend status` and `reset` (S9); and each run's record lists its paid
requests (S10).

**What is not built.**

- **Recovery's part in the money.** "Recovery closes every reservation that is not
  closed" ("Settlement, closing and recovery") is not built. A reservation is closed
  by its run's end and by nothing else, and no closing whizbang writes is `by`
  `recovery`. A reservation that a dead process left stays open.
- **A person's correction.** No command writes an `adjusted` name (the plan's task
  S12 is cut). The fold still reads one as "A person's correction" has it, and
  limit (g) follows from no command of whizbang's.
- **A dispatch judged against the store's books.** The meter judges a request by
  what its process holds, the run's allowance and the tree's tally, and by the
  names of its own reservation, which it reads to take the next place. It does not
  fold the store for each request. So "An epoch that needs its owner takes no
  dispatch either" ("Epochs") holds in the model and in the test kit's fake, and
  not in the real meter: an admission is refused there, and a run that is already
  executing goes on within its own reservation.
- **Proofs beyond those merged or in review.** No crash sweep, no race at a barrier
  and no generated suite is written for the meter, a settlement, a closing or the
  commands, and the table of histories is not run through an executor. "Each ledger
  task has its own sweep" ("Limits, each stated") is no longer so. What keeps a
  publication whole under a crash is the store, which has its own sweeps.

**What changes: a reset closes the books.** With no correction and no recovery, the
owner's reset is the one way to clear what is counted, so it has to clear all of it.
Two rules of the fold change, in the core and in the model alike, and the plan's
task S9 makes the change:

| Rule | Was | Is |
|---|---|---|
| The epochs a reservation counts in (`counts_in`) | The epoch its one hold names; every epoch where it is frozen, or has no hold or several | Each epoch that one of its holds names, and no other: none where it has no hold |
| When an epoch admits nothing until the owner acts (`needs_owner`) | Its turns are not those its publications took; or any reservation in the store is frozen with an amount that is unknown | Its turns are not those its publications took |

So two paragraphs of "Epochs" end with it: "A reset does not clear a frozen
reservation", and "A frozen reservation whose amount is unknown makes the store need
its owner, in every epoch". A frozen reservation counts where its holds say, at the
most its names can mean, and stops nothing else, whatever its holds are; a reset
opens an epoch in which it does not count. The exception of "Invariant 4, and what
the owner chose" was put to the owner and is ended. Histories 31, 65 and 67, the tests
`spend::a_reset_does_not_clear_a_frozen_reservation_and_its_owners_correction_does`
and
`spend::a_store_that_needs_its_owner_starts_no_paid_work_and_still_settles_ends_recovers_and_is_corrected`,
and any other history whose numbers the two rules move are restated by that task. The
checker's rows "1, by epoch" and "1, under loss" ("The invariants, and what holds
them") then leave out a reservation whose hold is lost, which is limit (c) without
its exception: such a reservation counts against no allowance, and the store goes
on. **What a reset clears is what the names before it say of the epochs before it.**
A hold that names a later epoch, which only other hands make, counts there when a
reset reaches it, and one more reset passes it.

**As built with task S9.** The two rules are changed in the core (`spend/fold.rs`)
and in the model alike, and nothing else of either; the model and the real fold agree
on every named and every generated history (`spend::fold::every_named_history_…` and
`spend::fold::every_generated_history_…`, in the core's tests). What moved, each
restated to hold the new rule and none taken away:

- **Named histories.** 31 and 65 keep their numbers; 31 now also holds, after the
  reset and before the correction, that the frozen reservation counts nothing in the
  new epoch. 45, 67, 68, 69 and 78 move: each is of a frozen reservation with several
  holds or none, which now counts where its holds say and stops nothing. In 67, 10
  are counted of 10 where 20 are committed, which is the third row of the table of
  costs below; in 69 the store goes on with 10 outstanding that no name counts.
- **Tests of the model.**
  `spend::a_reset_does_not_clear_a_frozen_reservation_and_its_owners_correction_does`
  is now
  `spend::a_reset_clears_a_frozen_reservation_and_its_owners_correction_changes_nothing_after_it`.
  `spend::a_store_that_needs_its_owner_starts_no_paid_work_and_still_settles_ends_recovers_and_is_corrected`
  keeps its name and its two cases of an epoch's turns; its third, a frozen
  reservation of unknown amount, is held there as a case that stops nothing. Seven
  more tests held the old rule in one assertion or a few, and hold the new one:
  the two suites of one name changed, `…what_the_ledger_could_not_have_written…`,
  `…the_turns_of_an_epoch…`, `…one_hold_made_in_the_place_of_the_ledgers_states_the_amount_of_a_frozen_reservation_too`,
  `…one_record_that_cannot_be_read…` and `…the_limits_accepted…`.
- **Tests of the core.** Three held the old rule and hold the new one:
  `spend::fold::an_admission_stands_under_each_of_its_three_conditions_and_only_for_the_amount_its_hold_states`
  (an admission beside a reservation of unknown amount stands);
  `spend::reserve::a_frozen_reservation_whose_amount_no_name_states_counts_nowhere_and_stops_no_admission`,
  which was `…_stops_every_admission_and_a_reset_too`; and
  `contract::admit::a_stored_reservation_that_is_not_one_is_refused_by_its_reader_and_stops_no_call_beside_it`,
  which was `…_is_refused_and_nothing_is_admitted`: a reservation's record that
  cannot be read, with no hold beside it, is still refused by whoever reads
  reservations, and the call beside it runs. Two more hold it in an assertion
  each (`spend::fold::a_reservation_whose_content_is_damaged_…` after a reset, and
  `spend::fold::a_name_of_no_form_…` for a run with no reservation).
- **The checker.** Three places. A reservation is held to the epoch it was really
  reserved in only while the hold the ledger made for it is there; where that hold is
  lost, what can still be reserved is held to the fold's own count of it, which is
  nothing in an epoch no hold names; and an amount that is unknown must be said by
  each epoch a hold names, where it was the newest epoch that had to say it. Every
  generated suite keeps every invariant so.
- **Two counts of what the generated histories reach** are lower, because a frozen
  reservation of unknown amount no longer makes a store need its owner: dispatches
  refused for that, 29 where the test asked for more than 75, and dispatches judged
  before the store came to need its owner, 4 where it asked for more than 5
  (`spend::every_generated_history_keeps_every_invariant`).

In the paragraphs above that this section ends, read "the store needs its owner" of a
frozen reservation as no longer so, and "counts in every epoch" as "counts in each
epoch a hold of it names". The table under "The invariants, and what holds them"
that counts what one changed name comes to (15,632 and its parts) is of the rules as
they were: the same suites now hold that a reservation whose amount is unknown stops
no epoch but the one a turn is owed in.

**No state leaves the owner without a way to use the allowance.**

| What the store holds | What it does | The way out |
|---|---|---|
| No allowance | Every paid call is refused | `spend reset --allowance` |
| An open reservation of a run a dead process left | It counts in full in its epoch | A reset: it does not count in the next epoch |
| A reservation closed as interrupted | The same | The same |
| A frozen reservation with its one hold, or with several | It counts in each epoch a hold names, at the most its names can mean | The same |
| A frozen reservation with no hold | It counts nowhere; `spend status` names it | None is needed |
| An epoch whose turns are wrong | No admission in that epoch | A reset: the next epoch's turns start again |
| The largest epoch a name can say, opened by other hands | Nothing is allowed and no reset can follow ("Epochs") | None but a new store. Accepted |
| A hold that other hands made, naming an epoch that no reset has opened yet | It counts in that epoch when a reset opens it, and its turn is one that epoch's publications did not take: the reset does not clear it, because it is not of the books before it | `spend status` names it. The next reset passes that epoch, and it does not count in the one after |
| A store at its generation limit | A reset adds names and is refused there, as every such publication is | Deleting runs makes room where runs' names filled it. The names under `requests/` and `spend/` are never removed, so a store those alone filled has no way out but a new store. Accepted here, as it was for every other publication (`docs/carry-over.md`, the row on what every publication costs, which is milestone 2's) |

**What it costs, in money.** Each is accepted with the decision.

| What happens | Counted | Really spent or owed | Off by |
|---|---|---|---|
| A run reserves 10, spends 3 with every request settled, and its process dies before its end | 10, until a reset | 3 | 7 over. Recovery would have released them. The owner sees it in `spend status` and resets |
| The allowance is 20. A reservation of 10 with 10 outstanding is frozen, or its run's process is dead, and the owner resets to 20 | 0 in the new epoch | 10 may still be charged by the provider | The new 20 can be spent in full beside it: 30 against a limit of 20. A reset forgets what came before it, as it already did for an open reservation (history 84) |
| A reservation of 10 with 10 outstanding loses its hold to other hands | 0, in every epoch | 10 | 10 under, and the 10 can be admitted again. Bounded by what was sent under that reservation. Before, the store stopped until a correction |
| Other hands leave an epoch's turns wrong while a run of that epoch executes | The run's reservation as its names read | What the run goes on to send | The run can send what its own reservation reads as left: 5 where nothing was left, in history 70. Before, the dispatch was refused |

Three more things are accepted with it, which no table can state. Nothing compares the
books with what a provider billed. A fault in the meter, a settlement or a closing
that an ordinary test does not catch can count wrongly in either direction until it
is seen in `spend status` or in a run's record. And the limits (a) to (h) stand as
they were, with nothing built to narrow them.

## Review rounds

**2026-11-14.** The independent review the plan asks for, read-only, with the model and
the table in hand and asked only whether any history can under-count, found seven
traces. Each was first written as a history that failed against the model as it stood,
and the rules above were changed until it passed. The 42 histories before them give the
balances they gave, with one change of events: history 28 names its closing as the
ledger makes one, with its place, because a closing without it now releases nothing.

| Trace | What it showed | Held by | History |
|---|---|---|---|
| 1 | A settlement other hands made, with its guard or without, replaced a real bound, and recovery released on it | A settlement lowers only with its whole chain; recovery releases only on whole chains. A forged complete chain is believed: a stated limit | 43 |
| 2 | A correction erased what was outstanding, and stayed through damage and both closings | A correction's meaning is stated; it is made over the names it saw with a place of its own, and says nothing once they change or it is damaged; never under what is settled | 44 |
| 3 | A second hold moved a reservation out of its epoch, or made room for a dispatch | More than one hold freezes, in every epoch named, at the largest amount; a dispatch is held to the smallest. The checker takes the amount and the epoch from the truth | 45, 46 |
| 4 | A turn far ahead let a judgement made before an overrun be published | The next turn and place are the lowest not taken, never a count | 47 |
| 5 | A closing without its place released, and a dispatch followed it | A closing without its place freezes; no dispatch once any closing is named; the invariant is checked | 48, 49 |
| 6 | A second dispatch at a place moved a run's spend to another run | Every run named at a place is counted for all of it; the truth is kept for each run | 50 |
| 7 | A run ended with a request outstanding; a charge with no settlement and a lost name were outside the model | The end is refused; the provider's charge is an event of its own; a name can be lost | 51, 52, 53 |

Found while these were built: 54 (a second process recovers under a live request) and
55 (a reset without its guard), and by the generated histories, 56 (a request with a
bound of nothing, not settled, was released; seed and number in its name).

Two of the rulings given for this round were not met as worded. The next turn is the lowest not taken and not one past the highest:
with the highest, the trace's own history is still published (47). And a dispatch
judged before a closing that other hands made without its place cannot be refused by
the store; the reservation is frozen in full instead (48).

**2026-11-14, the second round.** The same review, of the record as the first round
left it, found seven more traces outside the limits then stated, each passing the
checker. Rules written case by case were given up for the one rule above. Each trace
was first a history that failed.

| Trace | What it showed | Held by | History |
|---|---|---|---|
| 1 | A correction stood on how many names there were: without its place, and with a name of no form in a guard's stead | A correction names its place and its turn and is sealed over the names themselves | 57 |
| 2 | Names made and lost until the count was right again brought a correction back over a charge recorded since | The same: the seal is of the names, not their number | 58 |
| 3 | One lost place let two requests take one place; the fold and the truth both kept one | Frozen, at every bound and every charge of the place; the truth keeps every request | 59 |
| 4 | What lowered the balance without a turn let a late admission through: a settlement that ended a correction, a lost turn, a settlement made without its guard | Every settlement takes a turn; an epoch whose turns are not those its publications took admits nothing. A judgement made before other hands acted is still published: "Limits" | 60, 61 |
| 5 | A charge over its bound without its guard used up a place that nothing accounted for | Only a guarded charge over its one dispatch's bound accounts for a place | 62 |
| 6 | With a dispatch's name lost its run read as having sent nothing | Under a frozen reservation a run's spend is unknown, at most what the names can mean | 63 |
| 7 | A guarded settlement at a charge its stage does not allow was taken, and released on | The recogniser holds a settlement to its stage | 64 |

What the one rule changed in the histories before it. The balances of 1 to 42 stand,
but for two:

- **History 17:** the reservation with two dispatches at one place counts 13, not 10,
  and 2 is left, not 5. Two dispatches at a place may each be a request (trace 3), so
  both bounds are counted, 5 and 8.
- **History 29:** the second of two ends that judged the same books is asked again and
  then done, where it was done at once: a closing takes a turn. Its balances stand.

Of 43 to 56: a dispatch under a frozen reservation is refused as frozen, not for what
is left (46); a planted turn, hold, closing or correction leaves the store needing its
owner (44, 45, 47, 48); what a run sent under a frozen reservation is unknown (50).

Three things in that round's rulings were not built as worded, each said where it
applies: a place with more than one dispatch counts every bound and charge, not the
largest (history 59 is under at the largest: 10 counted, 11 real); a reservation with
no hold counts in every epoch, not in the newest alone, which is never less; and the
two under "Limits": a correction can stand again, and a judgement made before other
hands acted is published.

**2026-11-14, the third round.** The same review, told of the limits as they then
stood, the two below among them as (e) and (f), found that the recogniser held: no
change of one name to books the ledger wrote both passes it and lowers a count.
Three traces remained, all in what a frozen reservation counts for and where.

| Trace | What it showed | Held by | History |
|---|---|---|---|
| 1 | A settlement made without a guard at the place of a lost dispatch made the place look accounted for | The frozen sum has no credits: every place with no dispatch counts the amount | 65 |
| 2 | A guarded settlement at a charge its stage forbids, over its bound, was taken for an overrun that had used a place | The same | 66 |
| 3 | With the real hold lost and holds other hands made in other epochs left, the reservation counted in those alone; a hold made for another run made the turns of its own epoch look right; and the checker let it by | A frozen reservation counts in every epoch and stops admissions; the checker holds the epoch unless every hold is lost, and works out the allowance from the truth | 67 |

(e) is that a judgement made before other hands acted is published; (f) that a
correction can stand again when the names are again those it was sealed over.

What it changed in the histories before it, each of which has damage or names other
hands made, and each of which now also leaves the store needing its owner:

| History | Counted before | Counted now |
|---|---|---|
| 13 | 12 | 32 |
| 14 | 10 | 20 |
| 15 | 10 | 27 |
| 17 | 13, and 10 for the second | 23, and 44 |
| 43 | 17 | 52 |
| 44 | 10 where it is frozen | 35, and 45 at the end |
| 45 | 10 | 15 |
| 46 | 20 | 57 |
| 48, 50 | 10 | 15 |
| 53 | 15 | 25 |
| 57 | 10 | 35 |
| 58 | 10 | 40 |
| 59 | 25 | 35 |
| 62 | 16 | 31 |
| 63 | 10 | 33 |
| 64 | 10 | 25 |

One rule of the recogniser is new with it: a correction is of a reservation closed as
`held`. A released reservation that carries a correction which no longer stands was
frozen before by a rule of its own, and is now frozen as every other.

**2026-11-14, after the third round: the generators, a hunt over other seeds, and the
mutation check.** What the third round left owed was done. Other hands now make a
hold in any epoch up to three past the newest and for a run that has no reservation;
the second suite lets them do one to six things; and the third suite, which changes
one name of each of 2,048 whole books, is new ("The invariants, and what holds
them"). Then all three suites were run from 48 other seeds: 4,096 histories of each
of the first two shapes and 2,048 whole books for each seed, 5,073,392 single names
changed in all, none of which fell under limit (a) or (d). The run is not kept as a
test.

It found one trace outside the limits. It was first a history that failed.

| Trace | What it showed | Held by | History |
|---|---|---|---|
| 1 | A reservation whose own hold was lost, and that other hands gave holds in other epochs, was frozen and counted in every epoch. Its owner's correction ended the freeze, and it then counted in the epochs of those holds alone, so that its own epoch had room again | A reservation with no hold, or more than one, falls under no one epoch, corrected or not | 68 |

No history before it changed what it gives. A reservation with exactly one hold that
other hands made, corrected by its owner, still falls under that hold's epoch: that
is limit (a), one hold in the place of the ledger's, and the correction is sealed
over it.

The hunt and the mutation check also found the checker wrong in these, and the model
unsafe in three. None changed a rule.

| What | Was | Is |
|---|---|---|
| The allowance where the owner's reset is lost and other hands made one with more (too strict) | Held to the owner's amount, though a reset with its guard is believed under (a) | The names' allowance, while the owner's reset is not there |
| A reservation believed under another epoch, by one hold made in the place of the ledger's (too strict) | Counted against the newest allowance all the same, and held to its own epoch once its owner corrected it | Counted where the fold has it; the epoch is not held, under (a) |
| The epoch of a reservation with every hold lost (an exemption that excused nothing) | Not held | Taken out: such a reservation counts in every epoch, and is held to its own |
| A reservation that read as released after it read as interrupted, where other hands touched it (an exemption that excused nothing) | Not held | Taken out: no one event turns the one into the other |
| A loss from a reservation other hands had touched before (too strict) | Held to count no less than before, though a correction made over their names could say more than the names do | The rule is of books that were whole: the first thing other hands do to a reservation |
| A loss from a reservation that was corrected (too loose) | Held to count no less than nothing | Taken out: held to count no less than it did |
| A late dispatch after other hands did anything at all (too loose) | Held to no closing | Held to none only where they touched its reservation |
| What a dispatch may take under a reservation other hands touched, late or not (an exemption that excused nothing) | Not held | Taken out: a run's allowance in memory holds every dispatch to what was really reserved |
| A late admission after other hands did anything at all (too loose) | Not held | Not held only where no more is then admitted in the epoch it fell under |
| A settlement's name lost and made again as it was (too loose) | Held to its bound | Held to its charge again |
| The places of a frozen reservation that hold no dispatch (the model) | Walked, from 1 to the highest: a name with a place at the largest number would not end | Counted |
| A store's sum of what its reservations count for (the model) | Could pass the largest number | Stops at it |
| A reset after the largest epoch (the model) | Passed the largest number | Refused: the epoch is taken |

The mutation check, of what the third round added and of these: 64 changes of one
rule each, to the frozen sum term by term, the rule for a place, the count of the
places that hold no dispatch, the store's freeze, the epochs a reservation counts in,
the rule that a correction is of a reservation closed as `held`, the checker's
rules, each of its exemptions taken away, and five more faults in the fold and the
ledger to try the checker's rules against. 47 were caught by the tests as they
stood, and three more by tests added for them: a guard and a correction past the
last place, a frozen reservation after a reset, and the newest epoch lost with the
turn its reset took. Two were of an exemption that excused nothing, which was taken
out (a dispatch under a reservation other hands touched, above). The 12 that are left
each make an assertion of the checker looser, which no test can catch while the model
is right. Six of them were shown to matter by a fault put in the model beside them,
which the generated suites catch with the checker as it is and no longer catch with
the looser one: the excuse for an epoch (two), the allowance (three), and the second
reading of a reservation's names. For the other six no such fault was found among
those tried: a later epoch made without its guard held to nothing, a first loss held
to lower nothing, a late dispatch and a late admission excused after anything other
hands did, a reservation released after it was interrupted, and a settlement's name
made again.

**2026-11-14, the fourth round.** The same review, with the limits (a) to (f) and
the checker's exemptions in hand, found that a change of one name still holds, and
three defects in changes of several, one of them in two shapes. Each was first a
history that failed.

| Trace | What it showed | Held by | History |
|---|---|---|---|
| 1 | Two holds of nothing made, then the ledger's hold and its dispatch lost. The reservation was frozen throughout, and the frozen sum, which takes its amount from the holds that are there, came to 0 with a request of 10 outstanding | The amount of a reservation whose own hold is lost is unknown, whatever holds are left: limit (c), made wider. The books say "unknown, at least" where there is not exactly one hold, the store with them, and the checker holds them to saying it | 69 |
| 2a | After a parent's charge over its bound, the record damaged and the place the charge took lost: a child's dispatch of 5 that judged before the charge was published where nothing was left | A dispatch is held to what its process holds of the tree, when it is published | 71 |
| 2b | The same charge lost with its guard and its place: the reservation read as open with 5 left. Its epoch needed its owner, which a dispatch did not look at, and the child's dispatch was placed | An epoch that needs its owner takes no dispatch; and what the process holds of the tree refuses it too | 70 |
| 3 | Amounts and a place near the largest number: the frozen sum passed the largest number, and counted less than two real settlements or stopped the fold | Every sum of the fold stops at the largest number, which is over any allowance | 72 |

The round also noted, inside limit (a), that both readings took a set the ledger
cannot write: a dispatch of 1 under a hold of 0. The recogniser and the second
reading now replay what the ledger checked, each in its own code (history 74).

Found while these were built:

- **History 73.** Trace 2b with the turn the settlement took lost as well. No epoch
  needs its owner then, and the names are a set the ledger could have written: only
  what the process holds of the tree refuses the dispatch.
- **One hold made in the place of the ledger's, in a reservation that is frozen.**
  The books say a number there, and it can be under. The ruling for trace 1 has the
  books say "unknown" where the holds are not exactly one; with exactly one, no name
  shows that it is not the ledger's. It was ruled a part of limit (a), below.
- **A correction below the place a closing can have taken** passed both readings.
  The lowest place a correction names is now held to lie above the one after the
  last dispatch.

**What the fourth round changed in the histories before it.** No balance. Four
histories, each with two holds or with none, give the number they gave as "unknown,
at least":

| History | Was | Is |
|---|---|---|
| 45 | The store counts 15 | Unknown, at least 15 |
| 46 | The reservation counts 57 | Unknown, at least 57 |
| 67 | The store counts 20 | Unknown, at least 20 |
| 68 | The reservation counts 30 while it is frozen | Unknown, at least 30; and 10, known, once its owner has corrected it, as before |

History 39 gives what it gave. Its late dispatch was refused by the store, for the
place the overrun had taken, and asked again; it is now refused by the process's
memory of the tree before the store is asked, and asked again. Either way it is then
refused for what the reservation has left by the names, and the child still holds
its share of 5.

**What an epoch that needs its owner refuses** is in the table under "Epochs". That
reverses what the first three rounds allowed: dispatches under other reservations
went on.

**The generators and the checker.** The second suite begins one history in three
with trace 2's tree, lets other hands take away a whole settlement, and has a run
that another run called send a request, late or now; the first begins one in four
with the tree, half of those in the second process. The suite that changes one name
asks a dispatch of every run that the change stops, and, where a name is lost or
made, asks each run for all it can spend, through the checker.
Both generated tests count what they reach of a dispatch, and are held to it. A new
suite of 1,024 has amounts near the largest. What each reaches is given below, as
it is after the rulings.

**The hunt.** All four suites were then run from 32 other seeds, as a test
that is not kept: 4,096 histories of each of the first
two shapes, 1,024 with large amounts and 2,048 whole books for each seed, with
3,382,752 single names changed in all. The two readings of every reservation gave
one answer after every event, and no single name fell under limit (a) or (d). All
131,072 histories of the first shape passed, and every change of one name. Of the
131,072 of the second shape 26 failed, and 1 of the 32,768 with large amounts: all
27 in one way.

**What the hunt found.** An admission is judged; a request
is then settled, which takes the turn the admission chose; and other hands take that
settlement away whole, with its guard, its place and its turn. The turn is free
again, so the store takes the admission as it was judged. The books it lands on are
a set the ledger could have written, in an epoch that needs nobody, and more is
admitted after it: limit (e) as it then stood said of a late admission that nothing
more is then admitted in its epoch. It is over what was really left by no more than
the charge that was lost with the settlement, exactly as an admission judged after
the loss is, which limits (b) and (d) state. The checker refused the late one, as
admitted on a stale balance.

**Three rulings followed,** on that and on what was found while the traces were
built. They change words and the checker, and one thing in what a process holds.

| Ruled | What changed |
|---|---|
| Limit (e) is restated: a late publication is never worse than a fresh judgement on the names as other hands left them, and afterwards the reservation or the store is frozen, or nothing more is admitted in its epoch, or the books read as they would to a fresh process | The checker excuses a late admission from the turns the ledger took since where an admission of the same amount judged on the names as they are would be published, and holds it to what is really left as it holds that one. `spend::known_an_admission_judged_before_a_settlement_that_other_hands_took_away_whole_is_published` pins it |
| Limit (a) has one thing more: exactly one hold in the place of the ledger's lost hold states the amount as well as the epoch, in a frozen reservation too | Words alone: the statement moved from (c), which is now of holds that are not exactly one. `spend::known_one_hold_made_in_the_place_of_the_ledgers_states_the_amount_of_a_frozen_reservation_too` pins it |
| What the process holds of a tree stays asked beside the names, for a late dispatch and for one judged now, and counts a charge over its bound from the moment the process holds the answer | History 75: the answer says 12 over a bound of 5, no name says it yet, and a child's dispatch of 5 is refused |

**The hunt again, under the checker as ruled:** all four suites from the same
32 seeds, with 3,381,779 single names changed. Every history of the first shape
passes, every one with large amounts, and every change of one name; nothing that
passed before fails. Of the 27 that failed, 24 pass. Three of the 131,072 of the
second shape still fail, in one way, which the restated limit does not cover and
which is not ruled on: the late admission is worse than one judged now. Other hands
took away a settlement and, in the place of the turn it took, a turn that the late
admission had chosen. The epoch then needs its owner, and an admission judged now
is refused; the late one is published, its turn is the one that was missing, and
the epoch needs nobody afterwards. Nothing is over what was really left by more
than a charge that was lost. The checker refuses it, as admitted on a stale balance:
`spend::known_a_late_admission_whose_turn_fills_the_place_of_one_that_was_lost_is_published`
pins the admission and the refusal.

On the seed written in the tests, with the answer counted: of the second suite's
4,096 histories, 2,035 dispatches are published, 175 of them by a run under a run that
overran and 67 of those for more than nothing; 370 are asked late across what other
hands did, and 209 after two names or more of the reservation were lost; 10 are
refused by what the process holds of the tree alone, and 442 because the epoch needs
its owner; and of the suite of 1,024 with large amounts,
381
dispatches are published and 23 reservations count for the largest number there is.

**The mutation check,** of what this round added: 45 changes of one rule
each. To what says that an amount is unknown, and where; to the checker's exemption
for it and its three assertions; to the refusal of a dispatch where an epoch needs
its owner; to what the process holds of a tree, and where it is asked; to each sum
that now stops; to each step of the replay, in the recogniser and in the second
reading; and to the checker's rule that a dispatch that is published was really
covered. 32 were caught by the tests. Eight change nothing that any names can show:
what a process still holds of a tree of which no run executes (three), and sums that
cannot pass the largest number with any names a store could hold (five: a product of
two numbers of 64 bits, and sums of amounts that would take more names than a number
of 64 bits counts). Five make the checker looser, which no test can catch while the
model is right. Four of those were shown to matter by a fault put in the model
beside them, which more of the generated suites catch with the checker as it is: the
three assertions of limit (c), and the rule that a dispatch was really covered. For
the fifth, the exemption of (c) given to any request whose dispatch is lost, no such
fault was found: while the ledger's hold is there, the amount it states covers every
bound that lost dispatches can have had, and the frozen sum has the amount in it.
Three clauses that the check showed to decide nothing were taken out: an open
reservation has one epoch, so asking each of its epochs is asking one; and a place
that a correction names below a dispatch is refused already by the rule that the
closing lies between, in each reading. What the rulings changed, the checker's
excuse for a late admission and the answer counted in memory, was not put through
the check again: history 75 holds the second.

**2026-11-14, the fifth round.** The same review, with the limits and the checker's
exemptions as the fourth round left them, found nothing in what a process holds of a
tree, in a crash and its recovery, the replay, corrections, the sums that stop, epochs
or amounts that are unknown; and one history outside the limits, in which no name
that states money is lost or made. It was first a history that failed. The checker
let it by: only what the history must give refused it.

| Trace | What it showed | Held by | History |
|---|---|---|---|
| 1 | A reservation of 10 is all of the allowance, and all of it is sent. Other hands take away the epoch's first turn and make its second, and an admission of 10 that judged before the first is published with the turn that is free again. The turns are then exactly accounted for, both reservations are sets the ledger could have written, the books read 20 against 10, and the second run's request of 10 was placed. The checker excused the admission because the epoch it fell under had nothing left | An admission is confirmed on a reading of the names made after its publication, and nothing runs under it before. The checker holds every admission that stands to the truth and to that reading, with no exemption | 76 |

The round named two things the generated histories cannot reach. A tree that two
processes share cannot be made in this vocabulary: a call is made from inside a run
of the process that acts. And no generated history is longer than 24 events.
**History 77** is one that is: a tree 24 runs deep, each call made from inside the
one before; the answer to the leaf's request says more than the whole tree
reserved; a run half way up then publishes a request it judged before the leaf's,
and so does the run at the root, judged now; and the process dies. What the process
holds of the tree refuses both requests, where the names have 46 left, and recovery
closes the tree's one reservation with nothing released.

**What the fifth round changed in the histories before it.** One, the only one in
which an admission that was judged late is published: in histories 1, 12, 47 and 60
the store refuses it for its turn, and it is judged again.

| History | Was | Is |
|---|---|---|
| 61 | The admission of 10 that judged before the turn was lost is published, and stands: 22 counted of 20, in an epoch that needs its owner | It is published and not confirmed, because the epoch needs its owner: its reservation is closed, releases the 10 and counts nothing, and 12 are counted. The epoch needs its owner as it did, the next admission is refused as it was, and after the owner's reset the balances are the same |

**The tests that pin a late admission,** each as it now is:

| Test | Was | Is |
|---|---|---|
| `spend::known_a_late_admission_whose_turn_fills_the_place_of_one_that_was_lost_is_published` | Published; the checker refused it as admitted on a stale balance, and the case was not ruled on | Ruled. It is published, and it stands, because the reading made after it is sound, with 8 counted of 20: the checker holds it to the truth and to that reading, and lets it by. With an allowance of 7 the same admission reads 8 of 7 and is not confirmed. The test has both |
| `spend::known_an_admission_judged_before_a_settlement_that_other_hands_took_away_whole_is_published` | Published, and held by the checker as one judged now would be | Unchanged in what it gives. It stands by its confirmation: the reading is sound, with 19 and then 20 counted of 20. It is over the truth by the charge that was lost, which is limits (b) and (d) |
| `spend::known_an_admission_judged_before_other_hands_opened_an_epoch_falls_under_the_old_one` | Published under the old epoch; the checker excused it, because that epoch was no longer the newest | Unchanged in what it gives. It stands by its confirmation: neither its epoch nor the newest needs its owner, and it is 10 of 11. The checker holds it to the truth of its own epoch, with no excuse |

**Two things in the ruling for this round were not built as worded.**

- **"No epoch needs its owner."** An epoch that needs its owner needs it for good:
  its turns are never made right, and the owner's reset opens the next one (histories
  47 and 61, whose last admissions follow such a reset and stand). Held to every
  epoch, no admission would stand in such a store again. What is held is the epoch
  the admission fell under and the newest one, which for an admission judged now are
  one epoch; and a frozen reservation makes every epoch need its owner.
- **"Recovery closes it as `held`."** An admission that was published and never
  confirmed, because its process died between, has the three names of a reservation
  whose run died before it sent anything, and no others: confirming makes no name.
  The plan's history 25 has recovery release that reservation, so recovery releases
  this one. It counts in full from its publication to that recovery, and nothing ran
  under it. Telling the two apart needs a name made by the confirmation, which is a
  cost for every admission and is not decided here.

**The checker.** The two things the row of (e) left out for an admission's money are
gone: the excuse where the epoch it fell under had nothing left or was no longer the
newest, and the comparison with an admission judged now. In their place: the truth
of its own epoch, whichever epoch that is, with each reservation really made there
counted in full, where the check before it counted only what its requests took; the
reading made after its publication, written a second time in the checker; and, of
one that was backed out, that nothing ran under it.

**The generators.** A fourth shape is new ("The invariants, and what holds them"),
and the three generated tests that existed count the admissions that stand and those
that are not confirmed, and are held to reaching both. On the seed written in the
tests: of the fourth shape's 1,536 histories, 1,006 admissions are published with a
turn that other hands had taken away. 291 of them stand, and 585 requests are sent
under those, each held to what was really reserved; 715 are not confirmed. 4,290
admissions stand in all, 297 of them judged late across what other hands did and 73
confirmed apart from their publication, and 730 are published and not confirmed.
Of the first shape's 4,096, 2,556 stand and 96 are not confirmed; of the second's,
5,469 and 58. What the fourth round's counts were, they are.

**The hunt.** All five suites were run from 32 other seeds, as a test that is not
kept: 4,096 histories of each of the first two shapes and of the fourth, 1,024 with
large amounts and 2,048 whole books for each seed, with 3,381,779 single names
changed in all. Nothing failed: every one of the 425,984 histories passed, the two
readings of every reservation gave one answer after every event, and no single name
fell under limit (a) or (d). In them 634,416 admissions stand, each held to the
truth, and 67,320 are published and not confirmed; 85,536 admissions are published
with a turn that other hands had taken away, of which 24,741 stand.

**The mutation check,** of what this round added: 28 changes of one rule each.
Sixteen are to the ledger: the confirmation taken out; its own reservation not
asked for, or asked only to be a set the ledger could have written; the epoch it
fell under not asked; the newest epoch not asked; the allowance not asked, asked of
a call of nothing too, or asked with its own reservation left out of the count; the
closing of an admission that is backed out made `held`, or not made; a run made to
execute when its admission is published, or after it was backed out; recovery
closing an admission that waits in its own process; a crash that keeps what waits;
a second process confirming what the first published; and an admission judged now
not read back. Fifteen were caught by the tests. The last changes nothing the model
can show: an admission judged now is published on the names it judged, so its
reading is those names with its own three and always stands. In a store the
judgement and the publication are two moments, and the reading is always made.
Twelve make the checker looser, which no test can catch while the model is right:
the excuse that was taken out put back; the truth not asked; a reservation counted
in the truth only for what its requests took; the reading not asked, and each of
its three parts; a request or a call under a reservation that was not confirmed;
one that stands and does not execute; one that was backed out and ran something,
or is neither released nor counted in full; and the turns taken since a late
judgement. Each was shown to matter by a fault put in the model beside it, counted
over the four generated suites on the seed in the tests. With the confirmation
taken out the checker refuses 893 histories as it is and 227 with the old excuse
back. With an open reservation counted for what is known of it and not for its
amount, 1,027 are refused, all by the truth with each reservation counted in full,
and none where a reservation counts in the truth for its requests alone; the
generated tests assert that truth a second time, and refuse the same 1,027 where
the checker does not ask it. With the epoch it fell under not asked, 28 are refused
by the reading and none without it or without its part for the epochs; the part
for its own reservation holds 1, and the part for the allowance 11 that the truth
does not. A run that executes from the publication is refused in 358, in 6 by the
rule for a request under a reservation that was not confirmed alone, and in 1
without either. A reservation backed out as `held` in a fold that counts such a one
for what is known is refused in 734, and in 156 without the rule for it. A store
that takes a publication whose turn is taken is refused in 125, and in none without
the rule for turns. A confirmation that says an admission stands and starts no run
is refused in 6,620, and in none without the rule for it.

**2026-11-14, the sixth round.** The same review, of the record as the fifth round
left it, found no history that under-counts, and no admission and no dispatch of
money beyond the limits: the confirmation in every order of publication and reading
it was asked about, recovery and a crash, backing out, epochs and calls of nothing
all held. Its one finding is a rule that this record stated more strongly than a
judgement made earlier can keep.

| Trace | What it showed | Held by | History |
|---|---|---|---|
| 1 | A request of 10 is judged under a reservation of 10 while the store is sound. Another reservation is then damaged, or one turn is lost, and the store needs its owner. The request is published: its names collide with nothing, and what its process holds of the tree covers it. What is counted does not change, since its reservation counted in full already | Limit (e), restated for a dispatch: one judged before the store came to need its owner may still be published once, within what its tree really has left; one judged after is refused. No rule of the fold changed | 78, 79 |

No history before these changed what it gives.

**The checker** holds every dispatch that is published to what its tree really had
left, as it did, and now to this: where the store needed its owner when it was
published, it was published as it was judged, on names where the store did not. One
judged, or judged again, where the store needs its owner is never let by. The
generated tests hold the other side themselves: no dispatch that was judged where
its epoch needed its owner is published. On the seed in the tests 51 dispatches of
the first shape and 97 of the second are published where the store needs its owner,
each judged before.

**The generators.** The round named what they could not make: two admissions that
wait together, with a crash or a recovery between their confirmations. A fifth
shape makes them ("The invariants, and what holds them"). Of its 1,024 histories,
354 admissions are published while another waits; 506 are confirmed apart from
their publication and 270 are not confirmed; and the test is held to reaching both
processes reading first, each of them dying and each recovering between, both
admissions standing, one, and neither. Three processes, and a publication that
lands between a confirmation's reading and the closing that backs it out, are not
in the model's vocabulary, and are said under "Limits".

**The mutation check,** of what this round added: two changes to the checker's new
rule, the rule taken out and the rule asking only that the dispatch was published
as it was judged. Neither can be caught while the model is right. With a ledger
that does not refuse a dispatch where the epoch needs its owner, the checker
refuses 627 generated histories by that rule as it is, none with the rule out, and
58 with the weaker one; the generated tests' own assertion refuses 11 more, in
which the store was sound again by the publication, and all 638 without the
checker's rule. Three faults of the round before were run against the fifth shape:
a crash that keeps what waits and a second process confirming what the first
published both fail its test, and recovery closing an admission that waits in its
own process fails the named test alone.

**The review the plan asks for is met by this round.** Six rounds were made, each of
the record as the round before left it, and the last found no history that
under-counts.

**2026-11-14, the review of the pull request.** A reviewer that did not write this
record read it, the model and the tests against the plan. It found no history that
under-counts, two things that block the merge, and nine to follow.

| Finding | What it showed | What was done | History |
|---|---|---|---|
| Blocks, 1 | This record makes a frozen reservation stop every admission and dispatch, where the plan's decision, its invariant 4 and tasks S1b and S6 have every unrelated call go on. A plan outranks an ADR, and the plan was not amended | Put to the owner, who chose the plan's rule with its cost known. The model, this record and the tests are changed to it; the plan's decision stands, with the choice recorded there. The next entry has what changed | 81, 86, 87 |
| Blocks, 2 | The first condition of a confirmation is worded "the record, one hold, and nothing else that names the run". The model and the checker's copy asked only for a set the ledger could have written, and open: a place and a dispatch made under the reservation before its reading left an admission that stood. No test held either reading | Ruled: as worded. The model and the checker each ask it, in their own code. The history failed first | 80 |
| 1 | What a correction writes off can be charged afterwards, and spent a second time meanwhile | Stated as limit (g), with what bounds it and how far the store's one writer keeps it away | 82, 83 |
| 2 | The plan's boxes for the second part were shorter than this record's list of what it builds | The plan's boxes are brought in line; the recogniser's replay and places counted without walking are added to the list here | |
| 3 | Limits (a) and (d) did not say what they can cost | Each says it has no bound, and why | |
| 4 | Limit (e) said a judgement is published "once" | Once for each judgement; those of one tree together within what it has left | |
| 5 | Nothing said that a reset does not stop reservations that are open | Said under "Epochs" and "Consequences" | 84 |
| 6 | Nothing said what a request of nothing does where nothing is left | Said under "What a reservation counts for" and "Consequences" | 85 |
| 7 | The model published an amount of the largest number at a reset and at a settlement, which no name states | Refused at every entry. The test failed first | |
| 8 | What a dispatch may still take rested on the state alone, in a reading that is public | It answers nothing once a closing names the reservation. The test failed first | |
| 9 | Four things a builder of the second part would have to find out | "Notes for whoever builds the second part" | |

History 80 failed against the model as it stood, as did the tests of findings 7 and 8;
histories 82 to 85 passed when they were written, and say what was so and not said.
History 81 is the plan's rule, and is in the next entry.

**The mutation check,** of what this entry added: nine changes of one rule each. The
confirmation asking only for an open reservation; asking for four names or fewer; the
checker's copy not asking; a closed reservation answering by its state alone; the
largest number not refused; not refused of a dispatch; not refused of a settlement's
charge; a frozen reservation not making the store need its owner, which was then the
rule; and the first with the third. Eight were caught by the tests: the first by history 80 through the checker's
own copy, and with the checker's copy loosened beside it by what the history must
give. The checker's copy loosened alone cannot be caught while the model is right.
The generated suites do not make a place and a dispatch between an admission's
publication and its reading: history 80 and its test are what hold the first
condition.

**2026-11-14, the owner's choice on a frozen reservation.** The first blocking finding
was put to the owner with what each rule costs, and the owner chose the plan's: a
damaged record freezes its reservation and stops nothing else. One line of the model
changed: an epoch needs its owner where its turns are not those its publications took,
or where a frozen reservation's amount is unknown, and no longer wherever a
reservation is frozen. Everything else of the third and fourth rounds stands: the sum
with no credits, in every epoch; "unknown, at least"; a reset that does not clear a
frozen reservation; the confirmation; what a process holds of a tree; turns and
guards. The checker was not changed, and holds every generated history of all six
suites as it did.

What it changed in the histories and the tests before it:

| History or test | Was | Is |
|---|---|---|
| 65 | An admission of 8 beside the frozen reservation is refused because the store needs its owner | Refused because 47 are counted of 20 |
| 72 | A call of nothing is refused because the store needs its owner | Admitted: a call of nothing is admitted in a store over any allowance |
| 74 | The store needs its owner | It does not: the reservation counts 1, and 9 are left of 10 |
| 78 | The other reservation is damaged, and the store needs its owner | The other reservation loses its hold: a damaged one no longer stops a dispatch. What the history shows of limit (e) is the same |
| 81 | Written under the stricter rule: an admission of 5 and one of nothing refused | The plan's rule: the admission of 5 stands and sends 5; 30 counted of 30 |
| `spend::a_store_that_needs_its_owner_starts_no_paid_work_and_still_settles_ends_recovers_and_is_corrected` | Five ways a store came to need its owner, three of them a frozen reservation | The three that remain: a turn lost, a turn made, a name for a run with no reservation. A name of no form and a damaged record are asserted to stop nothing else |
| `spend::a_reset_does_not_clear_a_frozen_reservation_and_its_owners_correction_does` | Nothing is admitted after either reset | The frozen reservation takes 20 of each new allowance, and the rest can be reserved |
| `spend::the_turns_of_an_epoch_are_exactly_those_its_publications_took` | After a reset a frozen reservation still stopped admissions | A reset starts them in each case; the frozen reservation counts in the new epoch |
| `spend::known_one_hold_made_in_the_place_of_the_ledgers_states_the_amount_of_a_frozen_reservation_too` | Nothing more was admitted beside it | The whole allowance is: history 86 |
| The two tests that change one name of whole books | Every name of a reservation changed left the store needing its owner | 40,286 of 65,285 leave the store going on, and in 7 of them, each a settlement over its bound that is lost, more can then be reserved |

Two changes of the new rule were tried against the tests. With any frozen reservation
stopping the store again, ten tests fail, history 81's among them. With the exception
lifted as well, so that a frozen reservation whose amount is unknown stops nothing,
eight fail, and one of them is the first generated suite, by the checker: there the
books count less than was spent outside every stated limit, which is why the
exception is kept until the owner has ruled on it.

Histories 86 and 87 are new, and are the two accepted limits. No other history
changed what it gives: 67, 69 and 70 still end with the store needing its owner,
under the exception.

**Rounds after the review of the pull request.** The independent review, read-only,
with this record, the model, its tests and the amended sections of the plan. Rounds 7
and 8 read the record while it still had the stricter rule and the plan amended to
it; they are kept because what they found of the plan's other sentences was fixed
and stands.

| Round | Under-counts outside the limits | The plan and this record differ | The confirmation differs | Wording |
|---|---|---|---|---|
| 7 | 0 | 8 | 0 | 1 |
| 8 | 0 | 3 | 0 | 2 |
| 9, the first on the owner's choice | 0 | 2 | 0 | 4 |
| 10 | 0 | 2 | 0 | 1 |

Round 9 was also asked to list every history that under-counts and the limit each
falls under, and whether any sentence still stated the stricter rule as in force: it
listed fifteen, each inside a stated limit; found the bound stated for limit (g)
narrower than the correction's stated meaning, which is now said there; and found
three sentences of the stricter rule, which are changed. Round 10 listed sixteen
histories that under-count, each inside a stated limit with its cost covered, found
no sentence of the stricter rule left, and found two sentences on an epoch's
isolation and a correction upward where the plan and this record differed. Every
finding was fixed in the plan or here; no round followed the tenth.

## Consequences

- **The plan's decision on a damaged record, and its invariant 4, stand as the plan
  wrote them,** by the owner's choice: a frozen reservation is counted at the most
  its names can mean and stops nothing else. What that costs is limits (a) and (b),
  which are no longer contained by a store that stops: 10 under in history 86 and 13
  in history 87, the second with no bound. One exception is kept and is to be put to
  the owner: where no amount can be counted, the store needs its owner ("Invariant
  4, and what the owner chose").
- An admission is published and then read back: one more reading of the store and
  one more fold for every call that nothing called, and no more names when it
  stands. One that is backed out, with no name of it touched by other hands, leaves six names, counts for nothing, and has
  taken two turns.
- An admission through the ledger makes three names under `spend/`, a paid request
  five (two when it is dispatched, three when it is settled), a closing three, a reset
  three, a correction three: a run with *n* paid requests, 5 *n* + 6. They are never removed, and each adds to what a publication
  writes (`docs/carry-over.md`).
- Every admission, settlement and closing takes a turn, so they are published one at a
  time over one store, and the later of two that judged the same books is judged again: as with the fake of
  ADR 0007, and bounded as there by the caller's signal and deadline.
- What the store reports as left of an allowance is never more than is really left,
  outside the limits (a) to (h), and can be less: an interrupted
  reservation counts in full, an ancestor loses a whole overrun, and a judgement that
  missed a release is refused. Inside the limits it can be more: by 10 in history 86
  and by 13 in history 87.
- A name of a reservation that is made, lost or damaged freezes it, and stops its
  own dispatches and nothing else, while its one hold states its amount. Where it
  leaves no hold or several, it stops every admission and every dispatch in the store
  until the owner corrects that reservation. A turn that
  is made or lost stops them in its epoch: admissions until the owner resets, and
  the dispatches of that epoch's runs for good. Runs in flight settle and end.
- A reset sets what may be admitted from then on, and stops nothing that was admitted
  before it: a reservation that is open when its epoch is closed still sends all it
  reserved, and the new epoch counts none of it (history 84).
- The owner's correction to less than a reservation counted can put the store past
  its allowance, by what was outstanding under that reservation and is charged
  afterwards: limit (g). With the store's one writer holding, that is a request a
  dead process left with its provider (history 83).
- A request of nothing is placed wherever the store does not need its owner, and what
  it is charged is debt. A store with no allowance, or one in debt, still takes calls
  of nothing and their requests of nothing (history 85).
- Reversing this costs every name under `spend/` a store holds.

## Alternatives

- **A balance file that is rewritten** (rascal). Turned down by the plan: a rewritten
  record is what "made once" excludes, and a crash between two files has no whole
  state.
- **Amounts in content, names as keys.** Turned down: a damaged settlement would lose
  what it charged (history 13).
- **One name for a dispatch and its place.** Turned down: a name with a bound in it
  does not collide with the same place under another bound (history 3).
- **A `frozen` record made by recovery.** Turned down: a reservation damaged after it
  released would be released and frozen at once; found by the fold, it is one or the
  other.
- **One `closed` name, with whether it released in its content.** Turned down: the
  fold would then need content to know what was released. Two forms, of which both
  together freeze, need none.
- **An overrun clamped at nothing.** Turned down: it forgets the debt, and the next
  return is spent twice (history 8).
- **An overrun taken from each run above only as far as the run below could not
  cover.** Turned down for now: it is exact where this is pessimistic, and needs every
  ended child's share kept; the plan's history 18 states the pessimistic amounts.
- **A reset that carries open reservations into its epoch.** Turned down: the owner
  sets what may be spent from now on, and a correction of the old books would then
  move the new allowance (history 31).
- **A turn only for what can lower what is left** (this record's first version).
  Turned down after the second round: a settlement under its bound can end a
  correction, which lowers it (history 60), and "can only raise" had to be argued for
  each publication. A turn for each is one rule.
- **A correction told by the number of names it was made over** (the first round's).
  Turned down: other names can make up the number (histories 57, 58).
