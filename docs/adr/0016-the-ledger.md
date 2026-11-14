# 0016. The ledger: books of names made once, folded, and never under

Status: accepted. Changes ADR 0007 ("The reservation": the ledger's interface, and what
a ledger's names are) and ADR 0008 ("Allowances": what an allowance holds), and names
ADR 0010 (a charge, a bound and a stage) and ADR 0015 (a part in the admission)
Date: 2026-11-14
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
| `spend/turn/<n>` | Whatever can lower what is left: an admission, a settlement over its bound, a reset, a correction | Nothing but that the `n`th turn is taken |
| `spend/reset/<epoch>/<micros>` | The owner's command | The allowance of that epoch |
| `spend/place/<run>/<n>` | A dispatch, a settlement over its bound, a closing | Nothing but that the place `n` under that reservation is taken |
| `spend/dispatched/<run>/<n>/<by>/<micros>` | The meter, before a request leaves | The reservation it draws on (the run at the root of its tree), its place, the run that sent it, and its bound |
| `spend/settling/<run>/<n>` | The meter, when the request ends | Nothing but that the place `n` is settled |
| `spend/settled/<run>/<n>/<stage>/<micros>` | The meter, in the same publication | The stage the request reached, and what it was charged |
| `spend/closed/<run>/released`, `spend/closed/<run>/held` | The ledger, in the publication that ends the run; or recovery | That the reservation is closed, and whether the closing released what it held beyond its charges |
| `spend/adjusted/<run>/<n>/<micros>` | A person's command | The `n`th correction: what the reservation is charged |

- `<run>` and `<by>` are run ids (ADR 0005). `<epoch>`, `<n>` and `<micros>` are
  decimal digits with no leading zero, 20 at most; an epoch is 0 for a reservation made
  before any reset and otherwise from 1, a place, a turn and a correction's number are
  from 1, and an amount is under the largest number there is, which is no amount (ADR
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
none (`place`, `settling`), made in the same publication: the store refuses the later
publication whole. The turn and the place are also what keeps a judgement from being
published on books that changed ("Nothing is published on a stale balance").

### What each publication makes

One publication each, all of its names or none. The model publishes exactly these
(`Books::apply`), and each is refused whole when the store holds one of its names:

| Event | Names made |
|---|---|
| A reset | `reset/<epoch+1>/<micros>`, the next `turn` |
| An admission of a call nothing called | `reservations/<run>`, `held/<epoch>/<run>/<micros>` in the newest epoch, the next `turn`; and the run and its admission (ADR 0007) |
| An admission of a nested call | Nothing under `spend/`: its share is taken from its caller's allowance (ADR 0008) |
| A dispatch | The next `place` under the root's reservation, `dispatched/…` at it |
| A settlement | `settling/…`, `settled/…`; and when the charge is over the bound, the next `place` and the next `turn` |
| The end of a run nothing called | `closed/<run>/released` or `closed/<run>/held`, and the next `place` |
| Recovery | The same two for every reservation that is not closed and whose run is not open in this process, in one publication |
| A correction | `adjusted/<run>/<n+1>/<micros>`, the next `turn` |
| The end of a nested call, a crash, a deletion with a prune | Nothing under `spend/` |

"The next" turn is one more than the turns the judged generation names, and the next
place one more than the places it names under that reservation.

### What a reservation counts for

The fold (`fold` in the model; the real one is task S1b's) reads the names of one
generation and, of content, only whether each name's can be read and agrees with it.
For a reservation with the amount *A* of its hold:

- *charged* is the sum of what its `settled` names say;
- *outstanding* is the sum of the bounds of its `dispatched` names whose place has no
  `settled`;
- *known* is charged plus outstanding.

| The reservation | Counts for | State |
|---|---|---|
| Has a correction | What the last correction says, and never under what is charged | Adjusted |
| Has something damaged or that cannot be placed | The larger of *A* and what is known | Frozen |
| Closed as `released` | What is known, which then is what is charged | Released |
| Closed as `held` | The larger of *A* and what is known | Interrupted |
| Not closed | The larger of *A* and what is known | Open |

The first row that fits is the state. So an open reservation counts in full while its
run executes (history 16), a charge over its bound is counted as soon as its
settlement is durable (histories 8 and 11), and a closing releases only in the one row
that says so.

**The store's balance** is of one epoch: the allowance its reset set, less the sum of
what the reservations of that epoch count for. What is left is never reported below
nothing; by how much the count is over the allowance is the debt, and it stays counted
(history 8). An admission is judged on the newest epoch: it is admitted when its
amount is no more than what is left, so a call of nothing is admitted in a store with
no allowance and a call over nothing is not (history 38).

(`spend::every_named_history_gives_the_balances_it_must_and_keeps_every_invariant`
runs every history named here; each is in `histories()` under the number the plan
gives it.)

### Damage, and what cannot be placed

**A damaged entry freezes the reservation it belongs to and nothing else.** Each name
of a reservation, damaged alone, freezes that reservation at its amount, leaves every
other as it was and never stops the fold; a damaged turn or reset, which belongs to no
reservation, freezes none
(`spend::the_fold_reads_names_and_nothing_else_and_one_damaged_name_freezes_one_reservation`).
Freezing is not a record: it is what the fold finds, every time, so nothing can be
both frozen and released, and a reservation that released and is damaged afterwards
counts in full again (history 15). Only a correction ends it (history 31).

**What cannot be placed freezes too,** and is counted at the most it can mean
(`spend::what_cannot_be_placed_is_counted_at_the_most_it_can_mean`):

- a second `dispatched` at one place (the larger bound), a `settled` at a place nothing
  was dispatched at or a second one (every charge), either without its guard, or a
  guard with nothing behind it (history 17);
- no hold, or two (the larger amount, the newer epoch); a hold with no
  `reservations/<run>`; a name under `spend/` that names the run and is of no form;
- both closings, a closing as `released` with a dispatch not settled, and a correction
  of a reservation that is not closed, or corrections not numbered from 1.

Names that name a run with no hold are a reservation of nothing, frozen, in the newest
epoch: what they say was charged is counted. Two resets of one epoch count as the
smaller allowance. None of these is something the ledger's own code writes; they are
what a reader of stored bytes has to do something with, and each errs to counting more.

### Epochs: what a reset bounds

A reset makes the next epoch and states its allowance. **A reservation falls under the
epoch its hold names, for good.** The balance an admission is judged on is the newest
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
  dispatch, a settlement over its bound, a closing. So of two dispatches that chose a
  place one is published and the other chosen again (history 3); a dispatch that
  judged before an overrun is judged again and refused (history 39); and a reservation
  is closed once, whoever closes it (histories 28 and 40). Two corrections that judged
  the same books take one turn, and the second is judged again (history 41).
- What can only raise what is left (a settlement at or under its bound, a closing that
  releases) takes neither: a judgement that missed it counted too much, and is asked
  again by its caller.

The model holds this for every history, generated ones among them, in which an event
is judged on earlier books and published later: no admission and no dispatch is
published where what was left no longer covers it
(`spend::every_generated_history_keeps_every_invariant`).

### Settlement, closing and recovery

A request is settled by the run that sent it, while that run is open, from what the
process holds of the request and never from a name it reads back, once
(`spend::a_place_whose_guard_is_taken_is_not_settled_again`), by the stage it reached
(ADR 0010): not sent, nothing; answered, the charge the answer gave, or the
bound when it gave none; unresolved, the bound, counted as unresolved (histories 5, 6,
7, 35 and 36).

**One closing, by the run's end or by recovery.** It releases, as `released`, only
when the reservation is open, nothing under it is damaged or unplaced, and every
dispatch under it has its settlement; a tree charged more than it reserved is closed
at what it was charged (histories 9, 24, 25). A reservation that is closed takes no
dispatch
(`spend::a_closing_releases_only_what_is_open_and_whole_and_nothing_is_dispatched_after_it`). Otherwise it is closed as `held`, and
nothing is released: a dispatch whose settlement failed or whose backend panicked, a
process that died between a dispatch and its settlement (histories 10, 22, 23).
Recovery closes every reservation that is not closed and whose run is not open in this
process, by the same rule, in one publication; done twice it makes nothing (histories
26, 27); a run abandoned in a live process is left until that process is gone
(history 37). A run that was deleted and pruned changes nothing here: no name under
`spend/` goes with it (histories 13, 14).

### A person's correction

`adjusted/<run>/<n>/<micros>` sets what a reservation is charged. It is taken only for
a reservation that is closed and is interrupted, frozen or already corrected; for an
amount from what its settlements say, as the least, to the larger of its amount and
what is known, as the most; and, where it raises what the reservation counts for in
the newest epoch, only when what is left can hold the difference (histories 33, 34).
The fold takes the last one, and never counts it under what is charged, whoever wrote
it (`spend::a_correction_never_counts_under_what_was_charged_whoever_wrote_it`).

### The invariants, and what holds them

| Invariant of the plan | Held by |
|---|---|
| 1. Never under | After every event of every history, each reservation counts for no less than what was really charged and is really outstanding under it, which the model writes down apart from the names; a corrected one, for no less than what was really charged |
| 2. Debt is not forgotten | What is left is the allowance less the count, or nothing; a tree's signed allowances never come to more than is really left; every return is added to the signed balance |
| 3. One closing | A reservation that released has nothing outstanding and counts what was charged; one closed with nothing released is never released later; a damaged one is frozen or corrected |
| 4. Damage freezes one reservation | A damaged or planted name changes no reservation but its own, and its own counts in full |
| 5. Every amount is in a name | Every name the model makes is in the alphabet, under the limit, and read back by the parser to the same name; the fold of the names read back is the same fold |
| 6. No admission on a stale balance | "Nothing is published on a stale balance" |
| 7. A person corrects by a new record | A correction is a new name; no event removes or replaces one |

The table has the 38 histories of the plan, in its order, and this record's own after
them. The generated histories are 4,096 of at most 24 events each, from a seed written
in the test; one that breaks an invariant is printed with the seed and its number.
They reach every event, every state and every way a publication ends, late ones among
them
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
| `place` | `spend-place/1` | `record`, `run`, `place`, `by` (`dispatch`, `settlement` or `closing`) |
| `dispatched` | `spend-dispatch/1` | `record`, `run`, `place`, `by`, `request` (its place among the requests of the run that sent it), `bound_micros`, `at_ms` |
| `settling` | `spend-settling/1` | `record`, `run`, `place` |
| `settled` | `spend-settlement/1` | `record`, `run`, `place`, `stage`, `charged_micros`, `over_bound`, `at_ms` |
| `closed` | `spend-closing/1` | `record`, `run`, `released`, `by` (`end` or `recovery`), `charged_micros`, `unresolved_micros`, `at_ms` |
| `adjusted` | `spend-adjustment/1` | `record`, `run`, `count`, `charged_micros`, `at_ms` |

### Limits, each stated

- **The model is not the ledger.** It publishes names in memory; no test of it touches
  a store. The real fold is held to it by task S1b, history by history.
- **A crash is whole in the model:** a publication is there or it is not. A crash
  inside a publication is the store's to keep whole, and each ledger task has its own
  sweep.
- **The model's allowances are one process's.** It does not model two executors over
  one store beyond a judgement published late.
- **A correction is trusted to the grammar.** A name of the form of a correction, of a
  closed reservation and numbered in order, is a correction, whoever wrote it: a store
  another program can write to is not protected by the fold.
- **A reservation corrected and damaged afterwards stays corrected.**
- **The fold reads every name each time.** What that costs as a store ages is on
  `docs/carry-over.md`, with milestone 2.
- **The review round the plan asks for is not held yet at this head:** a review of
  this record by the independent reviewing model, read-only, with the model and the
  table in hand, asked only whether any history can under-count. It is recorded here
  when it is held, and a way to under-count that it finds is a new named history.

## Consequences

- An admission through the ledger makes three names under `spend/`, a paid request
  four, a closing two. They are never removed, and each adds to what a publication
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
