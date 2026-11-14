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
acts. And after the hunt over other seeds that followed: a reservation with no hold,
or with more than one, falls under no one epoch when its owner has corrected it
either. And after the fourth: the amount of a reservation whose own hold is lost is
unknown, and the books say so; an epoch that needs its owner takes no dispatch
either; a dispatch is held to what its process holds of the tree; every sum of the
fold stops at the largest number; and the recogniser replays what the ledger
checked.
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
takes no admission, and no dispatch under any reservation, while it is there. Only a
correction the owner makes over it, as it then is, ends that. The store has the same
rule for its turns ("Nothing is published on a stale balance").

**That is deliberate: tampering or damage stops all new paid work until a person
looks.** No call is admitted and no request is dispatched, under a reservation that
reads as healthy either. What was dispatched before is still settled, a run still
ends, recovery still closes what a dead process left, and the owner corrects and
resets ("Epochs" has the table). A frozen reservation over-counts heavily, and is the
owner's to repair. A reset does not repair it: only the owner's correction of that
reservation does.

**One thing a frozen reservation cannot be counted for is an amount that no name
states.** Its amount is in the hold the ledger made. Where that hold is lost, what the
reservation counts for is unknown, and where the names show it the fold says so:
limit (c).

Two rounds of review each found seven ways past rules written case by case (a
settlement without its guard, a correction counted by its names, a place taken twice).
A rule that says what is allowed, and freezes everything else, has no case to forget.

**A writer who forges a complete, consistent chain is believed.** A set of names that
the recogniser takes is, in the names alone, what the ledger would have made, and the
fold reads names. That is the first of the limits of the guarantee, (a) to (f), each
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
| The order of the places | Walking the places from 1, each dispatch's bound is no more than the hold's amount less what was known before it: every request sent before it, at its bound, or at its charge where it was settled under its bound; and, for each place below it that holds no dispatch, the excess of one charge over its bound, of a request sent before that place. No place below a dispatch is one that no such charge accounts for: nothing is dispatched after a closing. And the lowest place a correction names is above the one after the last dispatch: a correction follows the closing, which follows every dispatch |

Every history of the table in which other hands do nothing passes it after every event,
and each name of a full set of books lost alone, each of fifteen names made, and each
name damaged, fails it
(`spend::the_recogniser_takes_what_the_ledger_writes_and_nothing_else`). The same is
held of 2,048 books that the ledger alone wrote, generated from the seed: no name of
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
46) or a closed one. Nothing is dispatched under a reservation of an epoch that needs
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

**A frozen reservation makes the store need its owner too, in every epoch. A reset
does not clear a frozen reservation:** it is still there, it counts in the new epoch
as in every other, and no admission is taken. Only the owner's correction of that
reservation does (histories 31, 65 and 67;
`spend::a_reset_does_not_clear_a_frozen_reservation_and_its_owners_correction_does`;
`spend::the_turns_of_an_epoch_are_exactly_those_its_publications_took`). A
reservation that is interrupted and reachable does not
stop admissions; only names the ledger could not have written do.

**An epoch that needs its owner takes no dispatch either.** No request is dispatched
under a reservation of an epoch that needs its owner, a request of nothing among
them; and a frozen reservation makes every epoch need its owner, so that none is
dispatched anywhere. The first three rounds let dispatches under other reservations
go on. The fourth showed a reservation that read as open with 5 left, after three
names of a charge over its bound were lost, in an epoch whose turns did not add up:
the dispatch did not look at that, and 5 more were sent where nothing was left
(history 70). What stops is new paid work. What was dispatched is still settled, or a
run could never close:

| Where the epoch needs its owner | |
|---|---|
| An admission | Refused |
| A dispatch under a reservation of that epoch, of any amount | Refused |
| The settlement of a request that was dispatched | Taken, a charge over its bound too |
| The end of a run | Taken: the reservation is closed as its names allow |
| Recovery | Taken |
| The owner's correction | Taken. One that raises what a reservation counts for in the newest epoch is refused: nothing is left there to hold it |
| The owner's reset | Taken: it opens the next epoch |

(`spend::a_store_that_needs_its_owner_starts_no_paid_work_and_still_settles_ends_recovers_and_is_corrected`.)
A reset opens an epoch whose turns start again, and where no reservation is frozen
admissions start with it. **A run of the epoch that needed its owner sends nothing
more:** that epoch's turns are never made right. It settles what it sent, and ends.

**A reservation falls under the epoch its hold names, for good.** The balance an admission is judged on is the newest
epoch's, so a reset closes the books before it: a reservation of an earlier epoch
counts against that epoch's allowance and no other (history 30), and a correction of
one is recorded and changes nothing the new allowance can spend, downward or upward
(histories 31, 32 and 42).
A run that is still executing when its epoch is closed goes on drawing on its own
reservation, and its settlements and its closing take their turns in that epoch. A
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
one publication stands. A dispatch among them is still held to what its process holds
of the tree, so it does not pass what was really reserved (history 71). "Limits", (e),
has it.

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
them (39 to 42), from 43 those of its review rounds, 68 from the hunt that
followed the third, 69 to 72 the fourth round's, and 73 to 75 found while those were
built or ruled with them. The generated histories are 4,096 of at most 24 events each, from a seed written
in the test, and 4,096 more of another shape: books the ledger alone wrote, then one to
six things other hands do, then one to four things the ledger and its processes do
(`spend::every_generated_history_of_whole_books_then_other_hands_keeps_every_invariant`); one that breaks an invariant is printed with the seed and its number.
One in four of the first shape and one in three of the second begin with a tree whose
parent's request is settled over its bound while its child holds a share; in the
second, other hands at times take away a whole settlement, its guard, its place and
its turn, and a run that another run called then sends a request, judged late or
now. **Every dispatch that is published is held to what was really left of its
reservation, in the truth,** and both tests are held to reaching that: with runs that
other runs called, after a run above them overran, late across what other hands did
and after names of the reservation were lost. 1,024 more histories, of both shapes,
have amounts near the largest a name can state and places near the largest a name can
name (`spend::every_generated_history_of_amounts_near_the_largest_keeps_every_invariant`).
Other hands make names of every form in them, at any point between the ledger's own
events and its late judgements: a turn far ahead, a hold in any epoch up to three past
the newest and for a run that has no reservation, a whole settlement, a correction with
the right seal and no place, a whole correction; names are damaged and lost; about one event in four
happens in a second process with its own memory over the same store; and a provider
charges before a settlement is published. They reach every event, every state and
every way a publication ends, late ones among them
(`spend::the_generated_histories_reach_every_event_every_state_and_every_way_a_publication_ends`),
and an event that is refused changes no name (`spend::a_refusal_changes_no_name`).

**A third suite changes one name.** 2,048 books that the ledger and the owner alone
wrote, from the same seed, each have every name lost alone, every name damaged alone,
and a sample of names made alone: of every form, at numbers the books have and at
numbers far outside them, for a reservation that is there and for a run that has
none. That is 35,089 names lost, 35,089 damaged and 35,793 made. Each is held
by what the fold answers, with nothing left out but the two losses in the table; and
those of one books in sixteen are also held by the checker, and followed by an
admission and dispatches asked of the ledger for real
(`spend::every_name_of_whole_books_lost_or_damaged_alone_freezes_or_allows_no_more`,
`spend::a_name_made_alone_in_whole_books_freezes_or_allows_no_more`).

| The one name | What it comes to | How many |
|---|---|---|
| A name of a reservation, lost, damaged or made | The reservation is frozen and the store needs its owner in every epoch. It counts for no less than it did and no less than its requests really took; nothing is admitted, and nothing is dispatched under it or under any other. Its amount is said to be unknown exactly where it has not one hold | 65,285 |
| Of those: a settlement over its bound, lost | Limit (b). No less than the amount, and less than it counted by no more than the charge was over the bound | 78 |
| Of those: the hold, lost | Limit (c). Every bound and every charge its other names say, and the amount said to be unknown | 5,840 |
| A turn of the newest epoch, lost or made | The store needs its owner: nothing is admitted, and nothing is dispatched under a reservation of that epoch | 8,937 |
| A reset or its guard, lost, damaged or made; a turn of another epoch, lost or made | No reservation is frozen. No more can be reserved in any epoch than before, and no reservation counts for less: the epoch allows what it did or less, or it is not the newest and needs its owner | 20,067 |
| A turn, damaged; a second reset of an epoch that says more than the first | The fold answers what it answered: a turn's content is not read, and of two resets the smaller is the allowance | 11,682 |

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
- **Sums that stop at the largest number,** in the fold and in every reading of it;
  no name under `spend/` makes a reader fail.
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

**The guarantee has six limits, (a) to (f).** Five are things the fold does not hold,
and the sixth is a ruling it cannot meet.

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
  turn leaves the store needing its owner.
- **(b) A charge that no name states is not counted.** A provider's charge over the
  bound that is in memory when the process dies, and a settlement over its bound
  whose name is lost, leave the reservation at its amount or more, and under the
  charge when the charge was over that (history 52,
  `spend::known_what_the_names_cannot_say_is_not_counted`, history 53). Invariant 1
  is of what the records show.
- **(c) When the ledger's own hold of a reservation is lost, its amount is unknown,
  whatever holds other hands leave in its place.** The amount was in that name
  alone. The reservation counts for what its names say, in every epoch; a request
  whose dispatch is lost as well has then no name that states any amount, and is not
  counted (history 69). The round before had this only where every hold was lost;
  the fourth made two holds of nothing. **Where no hold is left, or more than one,
  the books say that they cannot tell:** the reservation is frozen, what it and the
  store count for is "unknown, at least", and no admission and no dispatch is taken
  until its owner corrects it. Where other hands leave exactly one hold in its
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
- **(e) One publication whose judgement was made before other hands planted,
  removed or damaged a name may still land.** The store refuses a publication only
  for a name it makes, and each judgement is published once. **It is never worse
  than a fresh judgement made on the names as other hands left them:** where a fresh
  admission or dispatch on those names would be accepted, under limits (a) to (d),
  the late one is too. **Afterwards the reservation or the store is frozen, or
  nothing more is admitted in the epoch it fell under, or the books read as they
  would to a fresh process.**
  A judgement made before a turn was lost, before a place's guard was lost, or
  before other hands made a closing without its place, is published (histories 61,
  59 and 48), and an admission or a dispatch then stands that the books as they
  were at publication would have refused: there the reservation is frozen or the
  store needs its owner. Nothing is frozen, and no more is admitted in the epoch all
  the same, where an admission falls under an epoch other hands closed by opening a
  later one
  (`spend::known_an_admission_judged_before_other_hands_opened_an_epoch_falls_under_the_old_one`),
  or under an epoch whose reset they took away. And the books read as they would to
  a fresh process where an admission was judged before a settlement that other
  hands then took away whole, with its guard, its place and its turn: the turn it
  chose is free again, and an admission judged on the names as they are would be
  published too, with the same names
  (`spend::known_an_admission_judged_before_a_settlement_that_other_hands_took_away_whole_is_published`).
  What is held of every such publication: a dispatch stands after a closing only
  under a reservation that other hands touched, and never over what was really
  reserved, because the process's memory of the tree is asked when it is published
  (history 71); and an admission stands only in an epoch where nothing more is then
  admitted, because it needs its owner, has nothing left, or is no longer the
  newest, or else where one judged now on the same names would stand, and then it
  is held as that one is, to what is really left. The second review round did not
  accept this limit. Closing it needs the store to refuse a publication for
  something other than a name it makes, such as the generation it judged on having
  changed, which is not ADR 0007's rule and is not decided here.
  The hunt under this limit as restated found one admission that is worse than a
  fresh judgement, and it is not ruled on: one whose turn fills the place of a turn
  that other hands took away ("Review rounds").
- **(f) A correction stands again when the names return to exactly the set it was
  sealed over.** The ruling of the second round, that a correction never stands
  again once a later name ended it, cannot be met by a fold of names: made and taken
  away again, the name leaves the same names the owner corrected ("A person's
  correction").

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
| (e) | A dispatch published late is not held to a closing where other hands did something between its judgement and it and touched its reservation. An admission published late, where they did something between, is not held to the turns the ledger took since its judgement, nor to what is left, where no more is then admitted in the epoch it fell under; and not to those turns, but to what is really left, where an admission of the same amount judged on the names as they are would be published | `unseen`, `excused` and `fresh` in `checked` |
| (f) | Nothing. The truth keeps the names the owner's last correction left, and the correction is the owner's again whenever the names are those | `held_to` |

The first row and the seventh are one piece of code, told apart by what the truth wrote
down: a name other hands made, or only names lost. What a correction writes off is not
in the table: it is the correction's stated meaning ("A person's correction"). No
dispatch is in it either: whatever the names say, what its process holds of the tree
holds each dispatch to what was really reserved. The round before had a run's own
allowance do that, which it does not: a child's share is not what the tree has left.
Four exemptions were taken out after the third round and two made narrower; the
fourth changed the row of (c) and no other ("Review rounds").

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
- A name of a reservation that is made, lost or damaged stops every admission and
  every dispatch in the store until the owner corrects that reservation. A turn that
  is made or lost stops them in its epoch: admissions until the owner resets, and
  the dispatches of that epoch's runs for good. Runs in flight settle and end.
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
