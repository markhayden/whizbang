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
acts.
Spec: section 6, "Spend"; section 3, "Storage"

## Context

`SPEC.md` section 6: a paid call reserves its bound when it is admitted; a request is
recorded before it leaves; each attempt is settled; an allowance is reset only by the
owner's command; and what a crash left unsettled is charged at its bound. The plan for
checkpoint 1B decides that balances are a fold of records whose names are made once,
that recovery is pessimistic, that a damaged record freezes one reservation, and that a
person corrects by a new record. It keeps seven invariants ("The ledger's invariants")
and leaves the names and the arithmetic to this record: six review rounds had each
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
core is changed by it. The second part builds the interface and the records in the
core; until it merges, **every sentence below about the store, the executor or the
core says what is to be built, and only a sentence that names a test says what the
head does.** The real fold, the real ledger, the meter, recovery and the commands are
the spend strand's tasks (S1b to S12), each held to the table.

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
dispatched under it; no settlement, closing or old correction lowers it; and the store
takes no admission while it is there. Only a correction the owner makes over it, as it
then is, ends that. The store has the same rule for its turns ("Nothing is published
on a stale balance").

**That is deliberate: tampering or damage stops new work until a person looks.** A
frozen reservation over-counts heavily, and is the owner's to repair.

Two rounds of review each found seven ways past rules written case by case (a
settlement without its guard, a correction counted by its names, a place taken twice).
A rule that says what is allowed, and freezes everything else, has no case to forget.

**A writer who forges a complete, consistent chain is believed.** A set of names that
the recogniser takes is, in the names alone, what the ledger would have made, and the
fold reads names. That, and three more, are the limits of the guarantee, each stated
under "Limits".

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
| `spend/closed/<run>/released`, `spend/closed/<run>/held` | The ledger, in the publication that ends the run; or recovery | That the reservation is closed, and whether the closing released what it held beyond its charges |
| `spend/adjusted/<run>/<place>/<turn>/<seal>/<micros>` | A person's command | A correction: what the reservation is charged, the place and the turn it took, and a seal of the names it was made over |

- `<run>` and `<by>` are run ids (ADR 0005). `<epoch>`, `<n>` and `<micros>` are
  decimal digits with no leading zero, 20 at most; an epoch is 0 for a reservation made
  before any reset and otherwise from 1, a place and a turn are from 1, and an amount is under the largest number there is, which is no amount (ADR
  0007). `<stage>` is `not_sent`, `answered` or `unresolved` (ADR 0010). `<seal>` is
  a digest in lower-case digits of base sixteen, of a fixed length: sixteen in the
  model, whose digest is not a cryptographic one, and in the core the first 32 of a
  SHA-256 (second part). A name is at
  most 200 characters, in the alphabet `Step::make` takes.
- **Nothing secret is in a name:** a run's id is made by the executor, and the rest are
  numbers and fixed words.
- **The grammar is strict and parsed as untrusted.** Each form is written one way and
  read back to the same value; a number with a leading zero, a sign or a space, a
  segment too many or too few, a word of another case and a name of no form are not
  names (`spend::a_name_is_written_one_way_and_read_back_strictly`). The core has one
  parser of it, which every reader of a name under `spend/` uses (second part).

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
| An admission of a nested call | Nothing under `spend/`: its share is taken from its caller's allowance (ADR 0008) |
| A dispatch | The next `place` under the root's reservation, `dispatched/…` at it |
| A settlement | `settling/…`, `settled/…`, the next `turn`; and when the charge is over the bound, the next `place` |
| The end of a run nothing called | `closed/<run>/released` or `closed/<run>/held`, the next `place`, the next `turn` |
| Recovery | The same three for every reservation that is not closed and whose run is not open in this process, in one publication: a turn for each |
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

Every history of the table in which other hands do nothing passes it after every event,
and each name of a full set of books lost alone, each of fifteen names made, and each
name damaged, fails it
(`spend::the_recogniser_takes_what_the_ledger_writes_and_nothing_else`). What it does
not hold is order: which bare place was the closing's, and that no dispatch came after
it. No count depends on it.

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

A settlement's *whole chain* is the row "A settlement" of the recogniser with the
dispatch's `place` there and none of the four damaged. What such settlements say is
*settled*: it lowers nothing in a frozen reservation, and it is the least a correction
may say.

**A frozen reservation counts in every epoch there is,** whatever its holds name: a
hold other hands made can name any epoch, and the real one can be lost (history 67).

**What a dispatch may still take** is *A* less what is known, of an open reservation,
and nothing of any other: nothing is dispatched under a frozen reservation (history
46) or a closed one.

**What one run sent** is known when its tree's reservation is reachable: for each
place whose dispatch names the run, what the place counts for. Under a frozen
reservation it is **unknown, and at most what the reservation's names can mean**: a
dispatch whose name is lost names no run, so a number read from the names could be low
(histories 50 and 63). The fold answers one or the other (`Sent` in the model), and a
caller that shows a run's spend shows which.

**The store's balance** is of one epoch: the allowance its reset set, less the sum of
what the reservations that count in that epoch count for. What is left is never
reported below nothing; by how much the count is over the allowance is the debt, and it
stays counted (history 8). An admission is judged on the newest epoch: it is admitted
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
counts in full again (history 15). Only a correction ends it (history 31).

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

- **a settlement over its bound:** the charge was in that name alone. What is left is
  the amount, its dispatch's bound and the amount again for the place the overrun
  took (history 53: 12 was counted, 25 is);
- **the hold:** the amount was in that name alone. The reservation is frozen at every
  bound and charge its other names say, and takes no dispatch.

A name lost from a reservation that is frozen already takes what it said out of the
sum: the rule is of books that were whole.

Losses of more than one name that leave a reachable set are believed: "Limits" lists
them.

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
reservation that counts in it, and one when the next epoch is there, for the reset that
closed it. **An epoch whose turns are not exactly 1 to that count needs the owner:**
nothing is left in it and no admission is taken, until a reset opens the next epoch,
whose turns start again. So a turn that is lost, a turn that other hands made at the
next number or far ahead, and a name that takes a turn made without one, each stop
admissions and never let one through that judged after it (histories 47 and 61,
`spend::the_turns_of_an_epoch_are_exactly_those_its_publications_took`). What a
reservation counts for is not changed by it.

**A frozen reservation makes the store need its owner too, in every epoch,** and a
reset does not end that: the reservation is still there. The owner's correction of it
does (histories 65 and 67). A reservation that is interrupted and reachable does not
stop admissions; only names the ledger could not have written do. Dispatches under
other, reachable reservations go on against what each has left.

**A reservation falls under the epoch its hold names, for good.** The balance an admission is judged on is the newest
epoch's, so a reset closes the books before it: a reservation of an earlier epoch
counts against that epoch's allowance and no other (history 30), and a correction of
one is recorded and changes nothing the new allowance can spend, downward or upward
(histories 31, 32 and 42).
A run that is still executing when its epoch is closed goes on drawing on its own
reservation, and its settlements and its closing take their turns in that epoch. A
reset takes the next turn of the epoch it closes, so an admission that judged before it
is asked again and falls under the new epoch.

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
come to no more than the reservation's amount less what is known against it. And it
does not bound a request by itself: a child whose parent overran still holds its
share, and its request is refused because **every dispatch is also held to what its
reservation has left by the names** (history 39).

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
and of each reservation, and no admission and no dispatch is published that it does not
cover (`spend::every_generated_history_keeps_every_invariant`,
`spend::two_processes_over_one_store_are_held_by_the_same_names`).

**No dispatch after any closing.** A dispatch is refused when any `closed` name names
its reservation in the generation it is judged on, with its place or without, whatever
else the reservation is (history 48). A closing the ledger made took a place, so a
dispatch that judged before it is refused by the store and judged again (history 49).

**What the store cannot refuse** is a publication that was judged before other hands
changed the books: its names collide with nothing they did. A dispatch judged before a
closing that other hands made without a place is published (history 48, its last
event); so is an admission judged before a turn was lost (history 61), and a dispatch
judged before its place's guard was lost (history 59). In each the books freeze, the
reservation or the store, so nothing is released and nothing more is admitted; but the
one publication stands. "Limits" has it.

### Settlement, closing and recovery

**A run does not end while a request it sent is outstanding.** The tally waits for
every request until its settlement is durable (the plan, "What this plan builds on";
task I3), and the model refuses the end of a run with a request unsettled, late
judgements among them (history 51).

**What the provider charged and what the books say are two things.** Between an answer
and its settlement the charge is in the memory of one process. A crash there leaves a
dispatch with no settlement: recovery counts the reservation in full and releases
nothing (history 52). When the answer was over the bound, no name says by how much,
and the reservation counts for its amount: "Limits".

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
process, by the same rule, in one publication; done twice it makes nothing (histories
26, 27); a run abandoned in a live process is left until that process is gone
(history 37). **Recovery releases only on whole chains,** as the plan's decision has it
("unless whole, readable records show that every request it dispatched was settled"):
it witnessed none of them, and a reservation whose names are not reachable is closed
as `held` (histories 43 and 64). A second process that
recovers while the first still runs a request closes the reservation as `held`; the
first settles its request afterwards, and a charge over the bound is counted
(histories 54 and 56). A run that was deleted and pruned changes nothing here: no name under
`spend/` goes with it (histories 13, 14).

### A person's correction

`adjusted/<run>/<place>/<turn>/<seal>/<micros>` sets what a reservation is charged. It
is the owner's deliberate act (`whizbang spend adjust`, task S12) and is trusted for
what it says. **What it means is stated here and is not an exemption: a correction
writes off what is outstanding under its reservation, and what a place without a whole
settlement says.** It never writes off what settlements with their whole chain say. It
is also the one thing that ends a freeze: the owner accepts the frozen reservation, as
its names then are, at the amount given.

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
| 1. Never under | After every event of every history, each reservation counts for no less than its floor, which the checker works out from the truth alone: for each request really sent, what its settlement recorded, or its bound while none is recorded. The truth is written down as things happen (what was reserved and in which epoch; every request, two at one place among them, with the run that sent it, its bound and what was settled; the names the owner's last correction left) and is never read from the fold. What a correction writes off is left out of the floor by the correction's stated meaning |
| 1, by epoch | Each reservation counts in the epoch it was really reserved in; and no more can be reserved than the allowance the owner really set less the floors of that epoch |
| 1, by run | Each run is counted for no less than the floors of the requests it really sent, or is answered as unknown with a most that is no less (histories 50 and 63) |
| 1, under loss | A reservation that lost one name, with nothing else touched, is frozen and counts for no less than it did; no less than its amount when the name was a settlement over its bound; what is known when it was the hold |
| 2. Debt is not forgotten | What is left is the allowance less the count, or nothing; a tree's signed allowances, in whichever process holds them, never come to more than what was really reserved less what its requests really took; every return is added to the signed balance |
| 3. One closing | A reservation that released has every request settled and recorded; one closed with nothing released is never released later; a damaged one is frozen or corrected |
| 3, no dispatch after a closing | No dispatch is published once a closing names its reservation, whenever it was judged (histories 48, 49) |
| 4. Damage freezes one reservation | A name of a reservation that is damaged, made by other hands or lost changes no reservation but its own, and a damaged one's own counts in full |
| 5. Every amount is in a name | Every name the model makes is in the alphabet, under the limit, and read back by the parser to the same name; the fold of the names read back is the same fold |
| 6. No admission on a stale balance | No admission is published that what is really left of the allowance does not cover, and no dispatch that what was really reserved does not cover, late ones among them; and none as it was judged once the ledger has taken a turn in its epoch since |
| 7. A person corrects by a new record | A correction is a new name; nothing the ledger does removes or replaces one |

What the checker leaves out is the limits under "Limits", and nothing else.

The table has the 38 histories of the plan, in its order, this record's own after
them (39 to 42), and from 43 those of its review rounds. The generated histories are 4,096 of at most 24 events each, from a seed written
in the test, and 4,096 more of another shape: books the ledger alone wrote, then one to
three things other hands do, then one to four things the ledger and its processes do
(`spend::every_generated_history_of_whole_books_then_other_hands_keeps_every_invariant`); one that breaks an invariant is printed with the seed and its number.
Other hands make names of every form in them, at any point between the ledger's own
events and its late judgements: a turn far ahead, a whole settlement, a correction with
the right seal and no place, a whole correction; names are damaged and lost; about one event in four
happens in a second process with its own memory over the same store; and a provider
charges before a settlement is published. They reach every event, every state and
every way a publication ends, late ones among them
(`spend::the_generated_histories_reach_every_event_every_state_and_every_way_a_publication_ends`),
and an event that is refused changes no name (`spend::a_refusal_changes_no_name`).

### What the second part builds, as decided here

- **`Ledger::reserve` cannot return without having made its names.** What it returns
  can only be had from `Step::make`: a ledger that makes none does not compile.
- **`Ledger::settle`,** called inside the publication that ends a run nothing called,
  with the same step: a failure of it is a failure of that publication, the run stays
  admitted, and its caller is told.
- **The parser,** one for every reader, and a reading of reservations that answers
  each entry by itself, sound or damaged with the amount its name states.
- **`Allowance`** as above.
- **A reading of one run's spend** that answers a number or "unknown, at most".
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

### Limits, each stated

- **The model is not the ledger.** It publishes names in memory; no test of it touches
  a store. The real fold is held to it by task S1b, history by history.
- **A crash is whole in the model:** a publication is there or it is not. A crash
  inside a publication is the store's to keep whole, and each ledger task has its own
  sweep.
- **The model has two processes and no more,** each with its own runs and requests,
  and a publication is whole in both.
- **The fold reads every name each time.** What that costs as a store ages is on
  `docs/carry-over.md`, with milestone 2.

**The guarantee has four limits.** The checker (`Books::check` and `run_watched` in
the model) takes its floor from the truth and leaves out these and nothing else:

1. **A complete, consistent chain is believed, whoever made it.** Where other hands
   made or lost names of a reservation and the recogniser still takes the set, the
   fold counts by it. The checker believes it only when a second reading of the names,
   written apart from the recogniser, takes the set too, the stage rule of ADR 0010
   among its rows; a set the fold took and that reading does not is a failure. So a
   `settling` and a `settled` at a charge the stage allows lower their place, and a
   recovery then releases
   (`spend::known_what_the_names_cannot_say_is_not_counted`); a correction with its
   place, its turn and the right seal is a correction; a reset with its guard is an
   allowance. Any part of such a chain without the rest lowers nothing; and a chain
   that took no turn leaves the store needing its owner.
2. **A charge that no name states is not counted.** A provider's charge over the
   bound that is in memory when the process dies, and a settlement over its bound
   whose name is lost, leave the reservation at its amount or more, and under the
   charge when the charge was over that (history 52, the same test, history 53).
   Invariant 1 is of what the records show.
3. **The amount, when every hold is lost** (and only then: with a hold other hands
   made still there, the reservation is held to its epoch and its floor). The reservation counts for what its other
   names say, in every epoch; a request whose dispatch is lost as well has then no
   name that states any amount.
4. **A loss that leaves a reachable set is believed**
   (`spend::known_a_loss_that_leaves_what_the_ledger_could_have_written_is_believed`).
   No loss of one name does. These do: the last places of a reservation, each with
   everything at it (a dispatch that is not settled and its place; a closing and its
   place; a correction and its place); a settlement with its guard, and the place it
   took when it was over its bound and that place is the last; every name of the
   reservation; and a reset with its guard when it is the newest. Each but the
   request also leaves the epoch's turns one more than its publications account for,
   so the store needs its owner, unless the turn is lost with it.

**One more thing is outside the guarantee, and the second review round did not accept
it: what other hands do between a judgement and its publication.** The store refuses a
publication only for a name it makes. A judgement made before a turn was lost, before a
place's guard was lost, or before other hands made a closing without its place, is
published (histories 61, 59 and 48), and an admission or a dispatch then stands that the
books as they were at publication would have refused. After it the reservation or the
store is frozen. Closing this needs the store to refuse a publication for something
other than a name it makes, such as the generation it judged on having changed, which
is not ADR 0007's rule and is not decided here. The checker leaves out a late
publication exactly when other hands did something between its judgement and it.

**And one ruling of that round cannot be met by a fold of names:** that a correction,
once a later name ended it, never stands again. Made and taken away again, the name
leaves the same names the owner corrected ("A person's correction").

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

Not done at this head, and owed before the next round: the generators do not yet make
holds in an epoch up to three past the newest or for runs with no reservation, the
second suite still allows three things from other hands and not six, there is no
third suite that loses each name of each of many whole books, and the frozen sum and
the store's freeze have not been through the mutation check.

## Consequences

- An admission through the ledger makes three names under `spend/`, a paid request
  five (two when it is dispatched, three when it is settled), a closing three, a reset
  three, a correction three: a run with *n* paid requests, 5 *n* + 6. They are never removed, and each adds to what a publication
  writes (`docs/carry-over.md`).
- Every admission, settlement and closing takes a turn, so they are published one at a
  time over one store, and the later of two that judged the same books is judged again: as with the fake of
  ADR 0007, and bounded as there by the caller's signal and deadline.
- The store's balance is never more than the truth, and can be less: an interrupted
  reservation counts in full, an ancestor loses a whole overrun, and a judgement that
  missed a release is refused.
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
