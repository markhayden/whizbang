# 0016. The ledger: books of names made once, folded, and never under

Status: accepted. Changes ADR 0007 ("The reservation": the ledger's interface, and what
a ledger's names are) and ADR 0008 ("Allowances": what an allowance holds), and names
ADR 0010 (a charge, a bound and a stage) and ADR 0015 (a part in the admission)
Date: 2026-11-14
Amended: 2026-11-14, after the review round ("Review rounds"): what the fold guarantees
and against what; a guard for a reset; a correction made over the names it saw; the
next turn and place; what lowers a count and what it needs; losses, a second process and
a charge no name states, in the model.
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

Four kinds of name lower a count, and each is **lowering evidence**: a settlement, which
gives up what its dispatch's bound held beyond the charge; a closing as `released`,
which gives up what the hold held beyond the charges; a correction; and a reset, which
states what may be spent. Each counts only with its whole chain present and undamaged,
as the sections below say for each. Without it the name lowers nothing, and its
reservation is frozen at the most it can mean.

**A writer who forges a complete, consistent chain is believed.** A settlement with its
guard at a place that was dispatched, a closing with its place, a correction with its
place over the names that are there, a reset with its guard: in the names alone each is
what the ledger itself would have made, and the fold reads names. That is the limit of
the guarantee, and "Limits" says what follows from it.

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
| `spend/turn/<n>` | Whatever can lower what is left: an admission, a settlement over its bound, a reset, a correction | Nothing but that the turn `n` is taken |
| `spend/reset/<epoch>/<micros>` | The owner's command | The allowance of that epoch |
| `spend/epoch/<epoch>` | The owner's command, in the same publication | Nothing but that the epoch is opened: a reset's guard |
| `spend/place/<run>/<n>` | A dispatch, a settlement over its bound, a closing, a correction | Nothing but that the place `n` under that reservation is taken |
| `spend/dispatched/<run>/<n>/<by>/<micros>` | The meter, before a request leaves | The reservation it draws on (the run at the root of its tree), its place, the run that sent it, and its bound |
| `spend/settling/<run>/<n>` | The meter, when the request ends | Nothing but that the place `n` is settled |
| `spend/settled/<run>/<n>/<stage>/<micros>` | The meter, in the same publication | The stage the request reached, and what it was charged |
| `spend/closed/<run>/released`, `spend/closed/<run>/held` | The ledger, in the publication that ends the run; or recovery | That the reservation is closed, and whether the closing released what it held beyond its charges |
| `spend/adjusted/<run>/<over>/<micros>` | A person's command | A correction: what the reservation is charged, and how many names the reservation had when it was made |

- `<run>` and `<by>` are run ids (ADR 0005). `<epoch>`, `<n>` and `<micros>` are
  decimal digits with no leading zero, 20 at most; an epoch is 0 for a reservation made
  before any reset and otherwise from 1, a place, a turn and the count a correction
  was made over are from 1, and an amount is under the largest number there is, which is no amount (ADR
  0007). `<stage>` is `not_sent`, `answered` or `unresolved` (ADR 0010). A name is at
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
| A reset | `reset/<epoch+1>/<micros>`, `epoch/<epoch+1>`, the next `turn` |
| An admission of a call nothing called | `reservations/<run>`, `held/<epoch>/<run>/<micros>` in the newest epoch, the next `turn`; and the run and its admission (ADR 0007) |
| An admission of a nested call | Nothing under `spend/`: its share is taken from its caller's allowance (ADR 0008) |
| A dispatch | The next `place` under the root's reservation, `dispatched/…` at it |
| A settlement | `settling/…`, `settled/…`; and when the charge is over the bound, the next `place` and the next `turn` |
| The end of a run nothing called | `closed/<run>/released` or `closed/<run>/held`, and the next `place` |
| Recovery | The same two for every reservation that is not closed and whose run is not open in this process, in one publication |
| A correction | `adjusted/<run>/<over>/<micros>`, where `over` is how many names name the reservation in the judged generation; the next `place`; the next `turn` |
| The end of a nested call, a crash, a deletion with a prune | Nothing under `spend/` |

"The next" turn is the lowest number from 1 that no `turn` of the judged generation
names, and the next place the lowest that no `place`, `dispatched`, `settling` or
`settled` of that reservation names. It is never a count of names, and never one past
the highest: a turn that other hands made far ahead would then let a judgement made
before it take a number between (history 47). **A gap is treated as free, and the next
publication takes it.** A name that was lost leaves such a gap, and a late judgement
can then take a number once more: that is a limit, stated below.

### What a reservation counts for

The fold (`fold` in the model; the real one is task S1b's) reads the names of one
generation and, of content, only whether each name's can be read and agrees with it.

**A settlement's chain** is one `dispatched` at its place with the `place` it took, one
`settled` with its `settling`, and none of the four damaged. A place counts for:

| The place has | Counts for | Is |
|---|---|---|
| A settlement with its whole chain | What the settlement says | Settled |
| A dispatch and no settlement | The largest bound a `dispatched` there names | Outstanding |
| Anything else: a settlement without its guard, or without the dispatch's place, or with a name damaged; two settlements; two dispatches; a settlement and no dispatch | The larger of the largest bound and all the charges named there | Not settled, and its reservation frozen |

So a settlement that other hands made, or one whose guard is lost, never takes the
count under its dispatch's bound (history 43,
`spend::a_name_that_lowers_without_its_whole_chain_lowers_nothing`). For a reservation:

- *charged* is the sum of what its `settled` names say, and *settled* the part of it
  that settlements with their whole chain say;
- *outstanding* is what its places count for beyond their charges;
- *known* is charged plus outstanding;
- *A*, its amount, is the largest amount a `held` of it names, and *least* the smallest;
- a place is *unexplained* when a `place` name has no `dispatched` or `settled` at it
  and is not one of those that a charge over its bound, a closing or a correction
  took. It may be a dispatch whose name is lost, of any bound up to the amount, so
  each counts for *A* once more: *exposed* is what is known plus that;
- *most* is the larger of *A* and what is exposed.

| The reservation | Counts for | State |
|---|---|---|
| Has a correction that stands ("A person's correction") | What it says, and never under what is settled | Adjusted |
| Has something damaged or that cannot be placed | *Most* | Frozen |
| Closed as `released`, with every place settled | What is known, which then is what is charged | Released |
| Closed as `held` | *Most* | Interrupted |
| Not closed | *Most* | Open |

The first row that fits is the state. So an open reservation counts in full while its
run executes (history 16), a charge over its bound is counted as soon as its
settlement is durable (histories 8 and 11), and a closing releases only in the one row
that says so. **What a dispatch under a reservation may still take is *least* less what
is exposed,** so nothing other hands add can make room for a request (history 46).

**What one run sent** is, for each place, what the place counts for, counted to every
run that a `dispatched` there names: with two dispatches at one place, each run named
is counted for all of it, so that no run is counted for less than it sent (history 50).

**The store's balance** is of one epoch: the allowance its reset set, less the sum of
what the reservations that count in that epoch count for. What is left is never
reported below nothing; by how much the count is over the allowance is the debt, and it
stays counted (history 8). An admission is judged on the newest epoch: it is admitted
when its amount is no more than what is left, so a call of nothing is admitted in a
store with no allowance and a call over nothing is not (history 38).

(`spend::every_named_history_gives_the_balances_it_must_and_keeps_every_invariant`
runs every history named here; each is in `histories()` under its number.)

### Damage, and what cannot be placed

**A damaged entry freezes the reservation it belongs to and nothing else.** Each name
of a reservation, damaged alone, freezes that reservation at no less than its amount,
leaves every other as it was and never stops the fold; a damaged turn, which belongs to
no reservation, freezes none
(`spend::the_fold_reads_names_and_nothing_else_and_one_damaged_name_freezes_one_reservation`).
Freezing is not a record: it is what the fold finds, every time, so nothing can be
both frozen and released, and a reservation that released and is damaged afterwards
counts in full again (history 15). Only a correction ends it (history 31).

**What cannot be placed freezes too,** and is counted at the most it can mean
(`spend::what_cannot_be_placed_is_counted_at_the_most_it_can_mean`):

- a place that is not settled and not simply outstanding, as the table above has it;
  a dispatch without its `place`; a `settling` with nothing behind it (history 17);
- **more or fewer `place` names than the names account for:** one more is an
  unexplained place, one fewer is a guard that is lost, as of a closing or a correction
  that other hands made without the place it takes (histories 44 and 48);
- **no hold, or more than one, in any epoch and of any amount.** With more than one the
  reservation counts in every epoch a hold names, at the largest amount, and a dispatch
  is held to the smallest (histories 45 and 46). With none it is a reservation of
  nothing that counts in every epoch there is, for what its names say was charged and
  is outstanding;
- a hold with no `reservations/<run>`; a name under `spend/` that names the run and is
  of no form;
- both closings; a closing as `released` with a place that is not settled, whatever
  its bound, a bound of nothing among them (history 56);
- a correction that does not stand.

**Losing one name lowers nothing**
(`spend::losing_any_one_name_lowers_nothing`, which loses each name of three books in
turn). A lost guard or dispatch freezes at no less than the reservation counted. Two
names are the only place their amount is in, and losing one of them loses it:

- **a settlement over its bound:** the charge was in that name alone. What is left is
  its dispatch's bound and the place the overrun took, now unexplained: the
  reservation is frozen at no less than its amount, and at the bound and the amount
  together where that is more (history 53: 12 was counted, 15 is);
- **the hold:** the amount was in that name alone. The reservation is frozen at what is
  known against it, counts in every epoch, and takes no dispatch.

### Epochs: what a reset bounds

A reset makes the next epoch and states its allowance. **It is lowering evidence, and
its chain is its guard:** an epoch is there as soon as a `reset` or an `epoch` names
it, and its allowance is its reset's only when it has both and neither is damaged. Two
resets of one epoch, with the guard, allow the smaller. Any other epoch allows nothing,
and is still the newest: a reset whose guard is lost, or one that other hands made
without it, stops admissions until the owner resets again, and never allows more
(history 55,
`spend::an_allowance_is_its_resets_only_with_the_guard_and_with_neither_damaged`).

**A reservation falls under the epoch its hold names, for good.** The balance an admission is judged on is the newest
epoch's, so a reset closes the books before it: a reservation of an earlier epoch
counts against that epoch's allowance and no other (history 30), and a correction of
one is recorded and changes nothing the new allowance can spend, downward or upward
(histories 31, 32 and 42).
A run that is still executing when its epoch is closed goes on drawing on its own
reservation. A reset takes the next turn, so an admission that judged before it is
asked again and falls under the new epoch.

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

- **Whatever can lower what is left of the allowance takes the next turn:** an
  admission, a settlement over its bound, a reset, a correction. Two that judged the
  same books make the same turn, and the store refuses the later one whole; it is
  judged again (histories 1 and 12).
- **Whatever can lower what is left of a reservation takes its next place:** a
  dispatch, a settlement over its bound, a closing, a correction. So of two dispatches that chose a
  place one is published and the other chosen again (history 3); a dispatch that
  judged before an overrun is judged again and refused (history 39); and a reservation
  is closed once, whoever closes it (histories 28 and 40). Two corrections that judged
  the same books take one turn, and the second is judged again (history 41).
- What can only raise what is left (a settlement at or under its bound, a closing that
  releases) takes neither: a judgement that missed it counted too much, and is asked
  again by its caller.

The model holds this for every history, generated ones among them, in which an event
is judged on earlier books and published later, in one process or in a second over the
same store: no admission is published as it was judged once the ledger has taken a
turn since, and no dispatch once the ledger has taken a place under its reservation
since. The checker writes down each turn and place the ledger takes, apart from the
names, and holds late publications to that
(`spend::every_generated_history_keeps_every_invariant`,
`spend::two_processes_over_one_store_are_held_by_the_same_names`).

**No dispatch after any closing.** A dispatch is refused when any `closed` name names
its reservation in the generation it is judged on, with its place or without, whatever
else the reservation is (history 48). A closing the ledger made took a place, so a
dispatch that judged before it is refused by the store and judged again (history 49).
What the store cannot refuse is a dispatch that judged before a closing that other
hands made without a place: its names collide with nothing. It is published, and the
reservation is frozen in full by that closing, before the dispatch and after it, so
nothing is released and nothing is under (history 48, its last event).

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
it witnessed none of them, and a settlement it cannot hold to its guard, its dispatch
and that dispatch's place releases nothing (history 43). A second process that
recovers while the first still runs a request closes the reservation as `held`; the
first settles its request afterwards, and a charge over the bound is counted
(histories 54 and 56). A run that was deleted and pruned changes nothing here: no name under
`spend/` goes with it (histories 13, 14).

### A person's correction

`adjusted/<run>/<over>/<micros>` sets what a reservation is charged. It is the owner's
deliberate act (`whizbang spend adjust`, task S12) and is trusted for what it says.
**What it means is stated here and is not an exemption: a correction writes off what is
outstanding under its reservation, and what a settlement without its whole chain says.**
It never writes off what settlements with their whole chain say.

The command takes it only for a reservation that is closed and is interrupted, frozen
or already corrected; for an amount from what is settled, as the least, to *most*, as
the most; and, where it raises what the reservation counts for in the newest epoch,
only when what is left can hold the difference (histories 33, 34). It makes the
correction over the names the reservation has, with the reservation's next place and
the next turn.

**A correction stands** when it is the one made over the most names, is alone at that
number and is not damaged; the reservation has a closing; and the reservation's names
are exactly as many as the correction was made over, and two more: itself and its
place. Then the reservation counts for what it says, and never under what is settled,
whoever wrote it
(`spend::a_correction_never_counts_under_what_whole_settlements_say_whoever_wrote_it`).

**Otherwise it says nothing, and the reservation is frozen at *most*.** So a correction
that other hands made without its place is none; any name of the reservation that is
made or lost after a correction ends it, a second closing among them; and so does
damage to the correction itself. The owner corrects again, over the names as they then
are (history 44). Damage to another name of a corrected reservation changes no count
of names, and the correction stands: "Limits".

### The invariants, and what holds them

| Invariant of the plan | Held by |
|---|---|
| 1. Never under | After every event of every history, each reservation counts for no less than its floor, which the checker works out from the truth alone: for each request really sent, what its settlement recorded, or its bound while none is recorded. The truth is written down as things happen (what was reserved and in which epoch, each request with the run that sent it, its bound, what the provider charged and what was settled) and is never read from a name or from the fold. What a correction writes off is left out of the floor by the correction's stated meaning, and nothing else of a corrected reservation is |
| 1, by epoch | Each reservation counts in the epoch it was really reserved in; and no more can be reserved than the allowance the owner really set less the floors of that epoch |
| 1, by run | Each run is counted for no less than the floors of the requests it really sent (history 50) |
| 1, under loss | A reservation that lost one name, with nothing else touched, is frozen and counts for no less than it did; no less than its amount when the name was a settlement over its bound; what is known when it was the hold |
| 2. Debt is not forgotten | What is left is the allowance less the count, or nothing; a tree's signed allowances, in whichever process holds them, never come to more than what was really reserved less what its requests really took; every return is added to the signed balance |
| 3. One closing | A reservation that released has every request settled and recorded; one closed with nothing released is never released later; a damaged one is frozen or corrected |
| 3, no dispatch after a closing | No dispatch is published once a closing names its reservation, whenever it was judged (histories 48, 49) |
| 4. Damage freezes one reservation | A name that is damaged, made by other hands or lost changes no reservation but its own, and a damaged one's own counts in full |
| 5. Every amount is in a name | Every name the model makes is in the alphabet, under the limit, and read back by the parser to the same name; the fold of the names read back is the same fold |
| 6. No admission on a stale balance | "Nothing is published on a stale balance" |
| 7. A person corrects by a new record | A correction is a new name; nothing the ledger does removes or replaces one |

What the checker leaves out is listed under "Limits", each with its reason.

The table has the 38 histories of the plan, in its order, this record's own after
them (39 to 42), and from 43 those of its review rounds. The generated histories are 4,096 of at most 24 events each, from a seed written
in the test; one that breaks an invariant is printed with the seed and its number.
Other hands make names of every form in them, a turn far ahead, a whole settlement and
a whole correction among them; names are damaged and lost; about one event in four
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
| `turn` | `spend-turn/1` | `record`, `turn`, `by` (`admission`, `settlement`, `reset` or `adjustment`), `run` or null |
| `reset` | `spend-reset/1` | `record`, `epoch`, `allowance_micros`, `at_ms` |
| `epoch` | `spend-epoch/1` | `record`, `epoch` |
| `place` | `spend-place/1` | `record`, `run`, `place`, `by` (`dispatch`, `settlement`, `closing` or `adjustment`) |
| `dispatched` | `spend-dispatch/1` | `record`, `run`, `place`, `by`, `request` (its place among the requests of the run that sent it), `bound_micros`, `at_ms` |
| `settling` | `spend-settling/1` | `record`, `run`, `place` |
| `settled` | `spend-settlement/1` | `record`, `run`, `place`, `stage`, `charged_micros`, `over_bound`, `at_ms` |
| `closed` | `spend-closing/1` | `record`, `run`, `released`, `by` (`end` or `recovery`), `charged_micros`, `unresolved_micros`, `at_ms` |
| `adjusted` | `spend-adjustment/1` | `record`, `run`, `over`, `charged_micros`, `at_ms` |

### Limits, each stated

- **The model is not the ledger.** It publishes names in memory; no test of it touches
  a store. The real fold is held to it by task S1b, history by history.
- **A crash is whole in the model:** a publication is there or it is not. A crash
  inside a publication is the store's to keep whole, and each ledger task has its own
  sweep.
- **The model has two processes and no more,** each with its own runs and requests,
  and a publication is whole in both.
- **A complete, consistent chain is believed, whoever made it.** A `settling` and a
  `settled` at a place that is dispatched lower that place to what they say, and a
  recovery then releases on them
  (`spend::known_what_the_names_cannot_say_is_not_counted`); a correction with its
  place, over the names that are there, is a correction; a reset with its guard is an
  allowance. A store another program can write whole chains into is not protected by
  the fold. What the fold does hold: any one of those names without the rest lowers
  nothing.
- **A charge that no name states is not counted.** A provider's charge over the bound
  that is in memory when the process dies, and a settlement over its bound whose name
  is lost, leave the reservation at its amount or more, and under the charge when the
  charge was over that (history 52,
  `spend::known_what_the_names_cannot_say_is_not_counted`, history 53). Invariant 1 is of
  what the records show, and the amount is the most they show.
- **Losing two names of one reservation, or a name with others made by other hands, is
  not held.** A settlement and its guard lost together leave a dispatch that reads as
  outstanding at its bound. The fold still freezes on what it can see.
- **A lost guard lets a late judgement through.** A turn, or a name of a place, that is
  lost leaves its number free; a judgement made before it was first taken can take it
  again, and is then published on books that changed. The same holds when the lost
  number is taken again by another publication.
- **A dispatch judged before a closing that other hands made without its place is
  published** (history 48): the store has no name to refuse it by. Nothing is released.
- **A reservation corrected, and then damaged in another name than the correction,
  stays corrected:** damage changes no count of names. Made or lost names end the
  correction; damage to the correction ends it.
- **The fold reads every name each time.** What that costs as a store ages is on
  `docs/carry-over.md`, with milestone 2.

**What the checker leaves out,** each for one of the limits above
(`Books::check` and `run_watched` in the model):

| Left out of | What | Why |
|---|---|---|
| The floor of a reservation, its epoch and its release | A reservation that lost two of the ledger's names, or one and has a name other hands made | Losing two names is not held |
| The floor | A place where other hands made both a settlement and its guard | A complete chain is believed |
| The floor | Under a correction that stands: each request with no recorded settlement, and each whose place has a name that other hands made or that is damaged | The correction's stated meaning |
| The floor | Of a settlement whose name is lost, what it charged beyond its bound | A charge that no name states |
| A run's floor | A request whose `dispatched` name is lost | The run was named there and nowhere else |
| "Never released later" | A reservation one of whose closings other hands made, or lost | A complete chain is believed; a closing that is lost can be made again |
| What can be reserved against the real allowance | A store where other hands made a reset and its guard, or that lost two of the allowance's names, or one and has one other hands made | A complete chain is believed; losing two names is not held |
| "Losing one name lowers nothing" | A reservation with a second loss or a name other hands made; the two names that are the only place of their amount, held to the lesser floors given above | Losing two names is not held; a charge that no name states |
| A late publication held to turns and places taken since | One judged before a guard of its kind was lost, or that took a number that had been lost | A lost guard lets a late judgement through |
| "No dispatch after a closing" | A closing other hands made after the dispatch was judged; a lost guard, as in the row above | The store has no name to refuse it by |
| "Damage counts in full" | A reservation whose correction stands | Corrected and then damaged stays corrected |

The checker makes no exemption for a name that other hands made or damaged as such:
the general "tampered since" exemption of the first version is gone, and the amount, the
epoch and each run's spend are taken from the truth.

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

Two of the rulings given for this round were not met as worded, and are for the next
round to look at. The next turn is the lowest not taken and not one past the highest:
with the highest, the trace's own history is still published (47). And a dispatch
judged before a closing that other hands made without its place cannot be refused by
the store; the reservation is frozen in full instead (48).

## Consequences

- An admission through the ledger makes three names under `spend/`, a paid request
  four, a closing two, a reset three, a correction three. They are never removed, and each adds to what a publication
  writes (`docs/carry-over.md`).
- Every admission through the ledger takes a turn, so admissions over one store are
  published one at a time, and the later of two is judged again: as with the fake of
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
- **A turn for every change to the books.** Turned down: settlements under their
  bounds only raise what is left, and would make every admission wait for every
  request.
