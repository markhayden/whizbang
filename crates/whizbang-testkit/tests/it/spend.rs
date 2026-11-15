//! The model of the ledger's books, and the table of histories (ADR 0016): every named
//! history gives the balances it must, every generated one keeps every invariant, and a
//! name is written one way and read back strictly.

use std::collections::{BTreeMap, BTreeSet};

use whizbang_testkit::spend::{
    Books, Counted, Event, Folded, GENERATED_EVENTS, How, LARGE, Name, Outcome, Run, Sent, Stage,
    Standing, State, WHOLE_EVENTS, Why, fold, generated, generated_large, generated_paired,
    generated_tampered, generated_turned, generated_whole, histories, run_checked, seal,
};

/// The seed of the generated histories. A history that breaks an invariant is printed
/// with it and with its number.
const SEED: u64 = 0x1B00_2026_1114;

/// How many histories are generated, of each of the two shapes.
const GENERATED: u64 = 4096;

/// How many books the ledger alone wrote are each changed by one name.
const WHOLE: u64 = 2048;

/// How many histories have two admissions that wait together to be confirmed.
const PAIRED: u64 = 1024;

/// How many histories have a turn moved by other hands around a late admission.
const TURNED: u64 = 1536;

/// How many histories have amounts near the largest a name can state.
const LARGE_HISTORIES: u64 = 1024;

fn reset(micros: u64) -> Event {
    Event::Reset { micros }
}

fn admit(run: u32, micros: u64) -> Event {
    Event::Admit {
        run: Run(run),
        micros,
    }
}

fn dispatch(by: u32, micros: u64) -> Event {
    Event::Dispatch {
        by: Run(by),
        micros,
    }
}

fn settle(root: u32, place: u64, micros: u64) -> Event {
    Event::Settle {
        root: Run(root),
        place,
        how: How::Answered(Some(micros)),
    }
}

fn second(event: Event) -> Event {
    Event::Second(Box::new(event))
}

fn late(judged: usize, event: Event) -> Event {
    Event::Stale {
        judged,
        event: Box::new(event),
    }
}

/// Whether other hands do anything in these events.
fn tampered(events: &[Event]) -> bool {
    let hands =
        |event: &Event| matches!(event, Event::Plant(_) | Event::Damage(_) | Event::Remove(_));
    events.iter().any(hands)
}

#[test]
fn every_named_history_gives_the_balances_it_must_and_keeps_every_invariant() {
    let table = histories();
    // Every history that fails is said, not the first alone.
    let broken: Vec<String> = table
        .iter()
        .filter_map(|history| history.verify().err())
        .collect();
    assert!(broken.is_empty(), "{}", broken.join("\n"));
    // The plan's 38, in its order and under its numbers, then this record's own, and
    // from 43 those of its review rounds: 87 with the pull request's review, and 88
    // from the follow-ups of its last round; 89 is task S1b's, with the real fold.
    assert!(table.len() >= 89, "{}", table.len());
    for (at, history) in table.iter().enumerate() {
        let number = format!("{:02} ", at + 1);
        assert!(history.name.starts_with(&number), "{}", history.name);
        assert!(!history.must.is_empty(), "{}", history.name);
    }
}

#[test]
fn every_generated_history_keeps_every_invariant() {
    let mut all = Reached::default();
    for number in 0..GENERATED {
        let events = generated(SEED, number);
        assert!((1..=GENERATED_EVENTS).contains(&events.len()), "{number}");
        if let Err(why) = reached(&events, &mut all) {
            panic!("seed {SEED:#x}, history {number}: {why}\n{events:#?}");
        }
    }
    // Every dispatch that is published is held by the checker to what was really
    // left of its reservation. That is reached, with runs that other runs called,
    // after a run above them had a request settled over its bound, and judged late
    // across what other hands did.
    assert!(all.published > 1_000, "{all:?}");
    assert!(all.by_a_called_run > 300, "{all:?}");
    assert!(all.after_an_overrun > 300, "{all:?}");
    assert!(all.after_an_overrun_of_something > 80, "{all:?}");
    assert!(all.late_across_other_hands > 300, "{all:?}");
    assert!(all.needs_owner > 300, "{all:?}");
    // Every admission that stands is held to the truth and to the reading made
    // after its publication; some are published and not confirmed.
    assert!(all.admitted > 2_000, "{all:?}");
    assert!(all.not_confirmed > 50, "{all:?}");
    // A dispatch that was judged before the store came to need its owner is
    // published where it needs it; none that was judged there is, which is held
    // of every dispatch of every suite.
    assert!(all.judged_before_the_owner_was_needed > 20, "{all:?}");
    // The same seed and number give the same history.
    assert_eq!(generated(SEED, 7), generated(SEED, 7));
    assert_ne!(generated(SEED, 7), generated(SEED + 1, 7));
}

#[test]
fn every_generated_history_of_whole_books_then_other_hands_keeps_every_invariant() {
    let mut frozen = 0;
    let mut all = Reached::default();
    for number in 0..GENERATED {
        let events = generated_tampered(SEED, number);
        assert!((1..=GENERATED_EVENTS).contains(&events.len()), "{number}");
        // It begins with what the ledger alone does: the books before other hands
        // touch them are ones the recogniser takes.
        let first = events
            .iter()
            .position(|event| tampered(std::slice::from_ref(event)));
        let first = first.expect("other hands do something in each");
        let (books, _) = Books::after(&events[..first]);
        for (run, one) in &books.fold().reservations {
            assert!(
                one.reachable,
                "history {number}: {run} before event {first}"
            );
        }
        match reached(&events, &mut all) {
            Ok(books) => {
                let folded = books.fold();
                let is_frozen = |one: &&Standing| one.state == State::Frozen;
                frozen += folded.reservations.values().filter(is_frozen).count();
            }
            Err(why) => panic!("seed {SEED:#x}, tampered history {number}: {why}\n{events:#?}"),
        }
    }
    // Other hands freeze something in a good part of them.
    assert!(frozen > 1000, "{frozen}");
    // Every dispatch that is published is held by the checker to what was really
    // left of its reservation. That is reached with the two shapes the fourth
    // review round gave: a run that another run called, after a run above it had a
    // request settled over its bound, asks late across what other hands did; and
    // asks now, after other hands took away two names or more of the reservation.
    // What the process holds of the tree refuses some of them where the names
    // would not.
    assert!(all.published > 1_500, "{all:?}");
    assert!(all.by_a_called_run > 150, "{all:?}");
    assert!(all.after_an_overrun > 100, "{all:?}");
    assert!(all.after_an_overrun_of_something > 20, "{all:?}");
    assert!(all.late_across_other_hands > 100, "{all:?}");
    assert!(all.after_losses > 50, "{all:?}");
    assert!(all.over_tree > 5, "{all:?}");
    assert!(all.needs_owner > 200, "{all:?}");
    assert!(all.admitted > 4_000, "{all:?}");
    assert!(all.not_confirmed > 30, "{all:?}");
    assert!(all.judged_before_the_owner_was_needed > 50, "{all:?}");
    assert_eq!(generated_tampered(SEED, 7), generated_tampered(SEED, 7));
}

/// What became of each one name changed in books the ledger wrote alone, counted.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Tally {
    states: u64,
    removals: u64,
    damages: u64,
    plants: u64,
    /// The reservation the name names is frozen.
    frozen: u64,
    /// Of the frozen: its one hold states its amount and the name took no turn, so
    /// the store goes on beside it; and the others, where the store needs its owner.
    goes_on: u64,
    stops: u64,
    /// More can be reserved after the change than before: only where a settlement
    /// over its bound was lost, and by no more than it was over (limit (b)).
    more_left: u64,
    /// Of the frozen: a settlement over its bound was lost, and its charge was in
    /// that name alone (limit (b)); the hold was lost, and the amount was in that
    /// name alone (limit (c)).
    charge_lost: u64,
    amount_lost: u64,
    /// No reservation is frozen, and the newest epoch needs its owner: a turn of it
    /// was lost or made.
    needs_owner: u64,
    /// Neither: one of an allowance's own names, or a turn of an epoch that is not
    /// the newest. No more can be reserved in any epoch than before, and no
    /// reservation counts for less.
    no_more: u64,
    /// The fold answers what it answered: a turn whose content is damaged, and a
    /// second reset of an epoch that allows more than the first.
    unchanged: u64,
}

/// A sample of single names other hands could make in these books: every form, at
/// numbers the books have and at numbers far outside them, for one of the
/// reservations there are and for a run that has none. Each books are given a third
/// of them, by their number, so that three books in a row are given them all.
fn plants(books: &Books, number: u64) -> Vec<Name> {
    let third = |made: Vec<Name>| {
        let chosen = |(at, _): &(usize, Name)| at % 3 == (number % 3) as usize;
        let made = made.into_iter().enumerate().filter(chosen);
        made.map(|(_, name)| name).collect::<Vec<Name>>()
    };
    let folded = books.fold();
    let newest = folded.epoch();
    let far = 1_000_000_000_000_000_000u64;
    let most = u64::MAX - 1;
    let allowed = folded.balance().allowed;
    let mut made = vec![
        Name::Turn {
            epoch: newest,
            turn: folded.next_turn(newest),
        },
        Name::Turn {
            epoch: newest,
            turn: folded.next_turn(newest) + 50,
        },
        Name::Turn {
            epoch: newest,
            turn: u64::MAX,
        },
        Name::Turn {
            epoch: newest + 1,
            turn: 1,
        },
        Name::Turn {
            epoch: newest.saturating_sub(1),
            turn: folded.next_turn(newest.saturating_sub(1)),
        },
        Name::Epoch(newest + 1),
        Name::Epoch(newest + 3),
        Name::Epoch(far),
        Name::Reset {
            epoch: newest + 1,
            micros: 50,
        },
        Name::Reset {
            epoch: newest + 3,
            micros: most,
        },
        Name::Reset {
            epoch: newest,
            micros: allowed + 1,
        },
        Name::Reset {
            epoch: newest,
            micros: allowed / 2,
        },
    ];
    // A run that has no reservation: each form alone is a reservation the ledger
    // did not write.
    let none = Run(900);
    made.extend([
        Name::Reservation(none),
        Name::Held {
            epoch: newest,
            run: none,
            micros: 0,
        },
        Name::Held {
            epoch: newest + 3,
            run: none,
            micros: 5,
        },
        Name::Place {
            run: none,
            place: 1,
        },
        Name::Dispatched {
            run: none,
            place: 1,
            by: none,
            micros: 3,
        },
        Name::Settling {
            run: none,
            place: 1,
        },
        Name::Settled {
            run: none,
            place: 1,
            stage: Stage::Answered,
            micros: 7,
        },
        Name::Closed {
            run: none,
            released: true,
        },
        Name::Adjusted {
            run: none,
            place: 1,
            turn: 1,
            seal: 0,
            micros: 0,
        },
        Name::Stray {
            run: none,
            count: 1,
        },
    ]);
    let roots: Vec<Run> = folded.reservations.keys().copied().collect();
    let Some(run) = roots
        .get((number % roots.len().max(1) as u64) as usize)
        .copied()
    else {
        return third(made);
    };
    let one = &folded.reservations[&run];
    let own = one.epoch.unwrap_or(newest);
    let next = books.next_place(run);
    // A place with a request that is not settled, and one with a request that is.
    let sent = |settled: bool| {
        books.names().iter().find_map(|name| match name {
            Name::Dispatched {
                run: of,
                place,
                micros,
                ..
            } if *of == run => {
                let guard = Name::Settling { run, place: *place };
                (books.names().contains(&guard) == settled).then_some((*place, *micros))
            }
            _ => None,
        })
    };
    let (open, ended) = (sent(false), sent(true));
    let (at_open, bound) = open.unwrap_or((next, 0));
    let at_ended = ended.map_or(next, |(place, _)| place);
    let held = |epoch, micros| Name::Held { epoch, run, micros };
    let place = |place| Name::Place { run, place };
    let dispatched = |place, by, micros| Name::Dispatched {
        run,
        place,
        by,
        micros,
    };
    let guard = |place| Name::Settling { run, place };
    let settled = |place, stage, micros| Name::Settled {
        run,
        place,
        stage,
        micros,
    };
    let adjusted = |place, turn, seal, micros| Name::Adjusted {
        run,
        place,
        turn,
        seal,
        micros,
    };
    made.extend([
        // A second hold: of more, of less, of the same in another epoch, far ahead.
        held(own, one.amount + 1),
        held(own, one.amount / 2),
        held(own, most),
        held(own + 1, one.amount),
        held(newest + 3, one.amount),
        held(own.saturating_sub(1), one.amount),
        held(far, 0),
        // A place: the next, one past it, and far.
        place(next),
        place(next + 1),
        place(far),
        place(u64::MAX),
        // A dispatch: at the next place, at a place that has one, by another run,
        // of nothing, far.
        dispatched(next, run, 1),
        dispatched(at_open, run, bound + 1),
        dispatched(at_ended, none, 0),
        dispatched(far, run, most),
        // A guard: at a request that is not settled, at the next place, far.
        guard(at_open),
        guard(next),
        guard(far),
        // A settlement: as the ledger would write one for a request that is not
        // settled, at each stage, without its guard; a second at a place that is
        // settled; at the next place; far.
        settled(at_open, Stage::NotSent, 0),
        settled(at_open, Stage::Unresolved, bound),
        settled(at_open, Stage::Answered, bound + 3),
        settled(at_ended, Stage::Answered, 1_000),
        settled(next, Stage::Answered, 0),
        settled(far, Stage::Unresolved, most),
        // A closing of each kind.
        Name::Closed {
            run,
            released: true,
        },
        Name::Closed {
            run,
            released: false,
        },
        // A correction: sealed as the command would seal one now, naming a turn
        // that is there, with no place; at a place that is there; far.
        adjusted(next, 1, books.seal_for(run, next), 0),
        adjusted(next.saturating_sub(1).max(1), 1, 0, 0),
        adjusted(far, u64::MAX, u64::MAX, most),
        Name::Stray { run, count: 1 },
        Name::Stray { run, count: far },
    ]);
    third(made)
}

/// Holds one changed name to what the record says of it, by what the fold answers.
/// `books` are books the ledger wrote alone, `before` their fold and `after` the
/// fold with the one name changed that `hands` names. The reservation the name
/// names is frozen and the store needs its owner, or the name is the store's own and
/// no more can be reserved in any epoch; and nothing counts for less than it did, or
/// than the truth, but for the two names that are the only place of their amount.
/// Nothing is left out here but what is said.
/// What a settlement that these hands lose was charged over its bound, or nothing.
fn lost_over(books: &Books, hands: &Event) -> u64 {
    let Event::Remove(Name::Settled {
        run,
        place: at,
        micros,
        ..
    }) = hands
    else {
        return 0;
    };
    let bound = books.names().iter().find_map(|other| match other {
        Name::Dispatched {
            run: of,
            place,
            micros,
            ..
        } if of == run && place == at => Some(*micros),
        _ => None,
    });
    micros.saturating_sub(bound.unwrap_or(0))
}

fn one_name_changed(
    books: &Books,
    before: &Folded,
    after: &Folded,
    hands: &Event,
    tally: &mut Tally,
) -> Result<(), String> {
    let (Event::Remove(name) | Event::Damage(name) | Event::Plant(name)) = hands else {
        return Err("not something other hands do".to_owned());
    };
    let ensure = |holds: bool, what: &str| match holds {
        true => Ok(()),
        false => Err(format!("{what}\nbefore: {before:#?}\nafter: {after:#?}")),
    };
    // What a settlement that is lost was charged over its bound: limit (b), the one
    // loss of one name after which more can be reserved, and by no more than that.
    let lost_over = lost_over(books, hands);
    // In no epoch can more be reserved than before, but for that; and no other
    // reservation counts for less; only a turn can change another at all, by ending
    // its correction.
    let more = |was: u64, is: u64| is <= was.saturating_add(lost_over);
    ensure(
        more(before.balance().left, after.balance().left),
        "more can be reserved",
    )?;
    if after.balance().left > before.balance().left {
        tally.more_left += 1;
    }
    for epoch in before.resets.keys() {
        let (was, is) = (before.balance_of(*epoch), after.balance_of(*epoch));
        ensure(more(was.left, is.left), "more is left in an epoch")?;
        ensure(is.allowed <= was.allowed, "more is allowed in an epoch")?;
    }
    let turn = matches!(name, Name::Turn { .. });
    for (run, was) in &before.reservations {
        if name.of() == Some(*run) {
            continue;
        }
        let is = after.reservations.get(run);
        ensure(
            is.is_some_and(|is| is.counts >= was.counts && (turn || is == was)),
            "another reservation changed",
        )?;
    }
    let frozen = |one: &Standing| one.state == State::Frozen;
    let Some(run) = name.of() else {
        // One of the store's own names. No reservation is frozen by it.
        ensure(!after.reservations.values().any(frozen), "frozen")?;
        let damage = matches!(hands, Event::Damage(_));
        match name {
            // A turn's content is not read: it says nothing but that it is taken.
            Name::Turn { .. } if damage => {
                ensure(after == before, "a damaged turn changed the fold")?;
                tally.unchanged += 1;
            }
            Name::Turn { epoch, .. } => {
                ensure(after.needs_owner(*epoch), "its epoch has its turns")?;
                match *epoch == after.epoch() {
                    true => tally.needs_owner += 1,
                    false => tally.no_more += 1,
                }
            }
            _ if after == before => tally.unchanged += 1,
            _ => tally.no_more += 1,
        }
        return Ok(());
    };
    // A name of a reservation: it is frozen, whatever it was. No single name made,
    // lost or damaged leaves a set the ledger could have written: limits (a) and
    // (d) take more than one.
    let is = after.reservations.get(&run);
    let Some(is) = is.filter(|is| frozen(is) && !is.reachable) else {
        return Err(format!("{run} is not frozen: {is:#?}"));
    };
    // Its amount is said to be unknown where it has not exactly one hold, and the
    // store then says that it cannot tell how much is counted.
    ensure(
        is.unknown == is.epoch.is_none(),
        "its amount is known or not",
    )?;
    ensure(
        after.balance().unknown == is.unknown,
        "the store says what is counted, or does not",
    )?;
    // Where its amount is unknown the store needs its owner, in every epoch. Where
    // the name is one that takes a turn, lost or made, the epoch of its hold does.
    // Otherwise its one hold states its amount, it is counted at the most its names
    // can mean, and the store goes on: no epoch needs its owner that did not.
    let damage = matches!(hands, Event::Damage(_));
    let takes_a_turn = matches!(
        name,
        Name::Held { .. } | Name::Settling { .. } | Name::Closed { .. } | Name::Adjusted { .. }
    );
    if is.unknown {
        for epoch in before.resets.keys() {
            ensure(
                after.needs_owner(*epoch),
                "an epoch has no need of its owner",
            )?;
        }
        tally.stops += 1;
    } else if takes_a_turn && !damage {
        let epoch = is.epoch.unwrap_or(after.epoch());
        ensure(after.needs_owner(epoch), "its epoch has its turns")?;
        tally.stops += 1;
    } else {
        for epoch in before.resets.keys() {
            ensure(
                after.needs_owner(*epoch) == before.needs_owner(*epoch),
                "an epoch came to need its owner",
            )?;
        }
        tally.goes_on += 1;
    }
    let none = (0, 0, 0);
    let was = before.reservations.get(&run);
    let (counted, known, amount) = was.map_or(none, |was| (was.counts, was.known(), was.amount));
    let truth = books.really(run);
    let took = truth.map_or(0, |(_, _, took)| took);
    if let Some((epoch, reserved, _)) = truth {
        ensure(is.counts_in(epoch), "not counted in its epoch")?;
        ensure(reserved == amount, "the truth has another amount")?;
    }
    // What it sent over its bound, when the name lost is a settlement's.
    let over = match (hands, name) {
        (
            Event::Remove(_),
            Name::Settled {
                place: at, micros, ..
            },
        ) => {
            let bound = books.names().iter().find_map(|other| match other {
                Name::Dispatched {
                    run: of,
                    place,
                    micros,
                    ..
                } if *of == run && place == at => Some(*micros),
                _ => None,
            });
            micros.saturating_sub(bound.unwrap_or(0))
        }
        _ => 0,
    };
    let hold = matches!((hands, name), (Event::Remove(_), Name::Held { .. }));
    let over = u128::from(over);
    if over > 0 {
        // Limit (b): the charge was in that name alone. What is left is no less
        // than the amount, and no less than before but for what was over the bound.
        tally.charge_lost += 1;
        ensure(is.counts >= u128::from(amount), "under its amount")?;
        ensure(is.counts + over >= counted, "under what it counted")?;
        ensure(is.counts + over >= took, "under the truth")?;
    } else if hold {
        // Limit (c): the amount was in that name alone. Every bound and every
        // charge is still counted.
        tally.amount_lost += 1;
        ensure(is.counts >= known, "under what is known")?;
        ensure(is.counts >= took, "under the truth")?;
    } else {
        ensure(is.counts >= counted, "under what it counted")?;
        ensure(is.counts >= took, "under the truth")?;
        ensure(is.counts >= u128::from(amount), "under its amount")?;
    }
    tally.frozen += 1;
    Ok(())
}

/// The same one changed name, done to the books and held by the checker, and then an
/// admission and a dispatch asked of the ledger for real: nothing is admitted where
/// the store needs its owner, never more than was left before, and nothing is
/// dispatched under the reservation the name names.
fn asked_for_real(
    books: &Books,
    before: &Folded,
    after: &Folded,
    hands: &Event,
) -> Result<(), String> {
    let (Event::Remove(name) | Event::Damage(name) | Event::Plant(name)) = hands else {
        return Err("not something other hands do".to_owned());
    };
    let mut changed = books.clone();
    if changed.checked(hands)? != Outcome::Done {
        return Err("it was not done".to_owned());
    }
    let ensure = |holds: bool, what: &str| match holds {
        true => Ok(()),
        false => Err(format!("{what}\nbefore: {before:#?}\nafter: {after:#?}")),
    };
    ensure(changed.fold() == *after, "the books fold to something else")?;
    let admit = |micros| Event::Admit {
        run: Run(990),
        micros,
    };
    // A refusal changes no name, so each is asked of the same books in turn. One
    // more than was left before is never admitted, but for what a lost settlement
    // was charged over its bound (limit (b)). An admission of nothing, which
    // is always covered, is refused where the newest epoch needs its owner.
    let most = before
        .balance()
        .left
        .saturating_add(lost_over(books, hands));
    let over = changed.apply(&admit(most.saturating_add(1)));
    ensure(
        matches!(over, Outcome::Refused(_)),
        "admitted over what was left",
    )?;
    if after.balance().needs_owner {
        let nothing = changed.apply(&admit(0));
        ensure(
            nothing == Outcome::Refused(Why::NeedsOwner),
            "admitted where the store needs its owner",
        )?;
    }
    // Nothing is dispatched under the reservation the name names; and nothing under
    // any other of an epoch that needs its owner, which is every epoch where a
    // reservation is frozen: a request of nothing among them.
    for by in changed.executing_runs() {
        let root = changed.root_of(by);
        let own = name.of().is_some() && root == name.of();
        let under = root.and_then(|root| after.reservations.get(&root));
        let stopped = |one: &Standing| one.epoch.is_none_or(|epoch| after.needs_owner(epoch));
        if !own && !under.is_some_and(stopped) {
            continue;
        }
        let sent = changed.apply(&dispatch(by.0, 0));
        let refused = match own {
            true => matches!(sent, Outcome::Refused(Why::Frozen | Why::AlreadyClosed)),
            false => matches!(sent, Outcome::Refused(Why::NeedsOwner | Why::AlreadyClosed)),
        };
        ensure(refused, "dispatched where the owner is needed")?;
    }
    ensure(changed.fold() == *after, "a refusal changed the books")?;
    // All that each run can spend, asked on books of their own where a name is lost
    // or made: whatever is published, the checker holds to what was really left.
    let runs = match hands {
        Event::Damage(_) => Vec::new(),
        _ => changed.executing_runs(),
    };
    for by in runs {
        let mut asked = changed.clone();
        let all = asked.spendable(by).unwrap_or(0);
        asked.checked(&dispatch(by.0, all))?;
    }
    // Where the store has no need of its owner, what is left is admitted, and
    // breaks nothing.
    if !after.balance().needs_owner {
        let rest = changed.checked(&admit(after.balance().left))?;
        ensure(rest == Outcome::Done, "what is left is not admitted")?;
    }
    Ok(())
}

/// What other hands do to the books in one of the two tests below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hands {
    /// Every name lost, and every name damaged, each alone.
    LoseOrDamage,
    /// A sample of names made, each alone.
    Make,
}

/// One name changed in each of `states` books the ledger wrote alone, of this seed:
/// every change is held by what the fold answers, and those of one books in sixteen
/// by the checker and by events asked for real as well.
fn each_name_changed(seed: u64, states: u64, what: Hands) -> Result<Tally, String> {
    let mut tally = Tally::default();
    let mut reached: BTreeSet<String> = BTreeSet::new();
    for number in 0..states {
        let events = generated_whole(seed, number);
        let said =
            |why: String| format!("seed {seed:#x}, whole books {number}: {why}\n{events:#?}");
        if !(1..=WHOLE_EVENTS).contains(&events.len()) || tampered(&events) {
            return Err(said("not the ledger's alone".to_owned()));
        }
        // Every invariant is held on the way in one of the two tests: the other is
        // given the same books.
        let (books, _) = match what {
            Hands::LoseOrDamage => run_checked(&events).map_err(said)?,
            Hands::Make => Books::after(&events),
        };
        let before = books.fold();
        // The books are ones the ledger could have written, and need nobody.
        for (run, one) in &before.reservations {
            if !one.reachable || one.state == State::Frozen {
                return Err(said(format!("{run} is not reachable")));
            }
            reached.insert(format!("{:?}", one.state));
            if one.charged > u128::from(one.amount) {
                reached.insert("over its amount".to_owned());
            }
        }
        if before.resets.keys().any(|epoch| before.needs_owner(*epoch)) {
            return Err(said("an epoch needs its owner".to_owned()));
        }
        if before.epoch() > 1 {
            reached.insert("a second epoch".to_owned());
        }
        tally.states += 1;
        let mut hands = Vec::new();
        match what {
            Hands::LoseOrDamage => {
                for name in books.names() {
                    hands.push(Event::Remove(name.clone()));
                    hands.push(Event::Damage(name.clone()));
                }
                tally.removals += books.names().len() as u64;
                tally.damages += books.names().len() as u64;
            }
            Hands::Make => {
                let absent = |name: &Name| !books.names().contains(name);
                let made = plants(&books, number).into_iter().filter(absent);
                hands.extend(made.map(Event::Plant));
                tally.plants += hands.len() as u64;
            }
        }
        // The names with the one changed, folded, and then as they were.
        let (mut names, mut damaged) = (books.names().clone(), books.damaged().clone());
        if !damaged.is_empty() {
            return Err(said("a name is damaged".to_owned()));
        }
        for one in &hands {
            let said = |why: String| said(format!("{one:?}: {why}"));
            let after = match one {
                Event::Remove(name) => {
                    names.remove(name);
                    let after = fold(&names, &damaged);
                    names.insert(name.clone());
                    after
                }
                Event::Damage(name) => {
                    damaged.insert(name.clone());
                    let after = fold(&names, &damaged);
                    damaged.remove(name);
                    after
                }
                Event::Plant(name) => {
                    names.insert(name.clone());
                    let after = fold(&names, &damaged);
                    names.remove(name);
                    after
                }
                _ => return Err(said("not something other hands do".to_owned())),
            };
            one_name_changed(&books, &before, &after, one, &mut tally).map_err(said)?;
            if number % 16 == 0 {
                asked_for_real(&books, &before, &after, one).map_err(said)?;
            }
        }
    }
    let wanted = [
        "Open",
        "Released",
        "Interrupted",
        "Adjusted",
        "over its amount",
        "a second epoch",
    ];
    // Every one of them came to one of the four.
    let all = tally.frozen + tally.needs_owner + tally.no_more + tally.unchanged;
    if all != tally.removals + tally.damages + tally.plants {
        return Err(format!("seed {seed:#x}: {tally:?}"));
    }
    match wanted.iter().find(|one| !reached.contains(**one)) {
        Some(missing) => Err(format!("seed {seed:#x}: no whole books with {missing}")),
        None => Ok(tally),
    }
}

#[test]
fn every_name_of_whole_books_lost_or_damaged_alone_freezes_or_allows_no_more() {
    let tally = each_name_changed(SEED, WHOLE, Hands::LoseOrDamage);
    let tally = tally.unwrap_or_else(|why| panic!("{why}"));
    assert_eq!(tally.states, WHOLE);
    assert!(tally.removals > 20_000, "{tally:?}");
    assert_eq!(tally.removals, tally.damages);
    // The two losses the record names are met: limits (b) and (c).
    assert!(tally.charge_lost > 50, "{tally:?}");
    assert!(tally.amount_lost > 1_000, "{tally:?}");
    assert!(tally.needs_owner > 1_000, "{tally:?}");
    // A frozen reservation whose hold states its amount stops nothing else; one
    // whose amount is unknown, or whose name took a turn, stops the store.
    assert!(tally.goes_on > 10_000, "{tally:?}");
    assert!(tally.stops > 5_000, "{tally:?}");
    assert_eq!(tally.goes_on + tally.stops, tally.frozen, "{tally:?}");
    // The one loss of one name that leaves more to reserve is met, and is always
    // a settlement over its bound.
    assert!(
        tally.more_left > 0 && tally.more_left <= tally.charge_lost,
        "{tally:?}"
    );
    assert_eq!(generated_whole(SEED, 7), generated_whole(SEED, 7));
    assert_ne!(generated_whole(SEED, 7), generated_whole(SEED + 1, 7));
}

#[test]
fn a_name_made_alone_in_whole_books_freezes_or_allows_no_more() {
    let tally = each_name_changed(SEED, WHOLE, Hands::Make);
    let tally = tally.unwrap_or_else(|why| panic!("{why}"));
    assert_eq!(tally.states, WHOLE);
    assert!(tally.plants > 15 * WHOLE, "{tally:?}");
    // No name made alone is one whose amount is lost.
    assert_eq!((tally.charge_lost, tally.amount_lost), (0, 0), "{tally:?}");
    assert!(tally.needs_owner > 1_000, "{tally:?}");
    assert!(tally.goes_on > 1_000, "{tally:?}");
    assert!(tally.stops > 1_000, "{tally:?}");
    assert_eq!(tally.goes_on + tally.stops, tally.frozen, "{tally:?}");
    assert_eq!(tally.more_left, 0, "{tally:?}");
}

#[test]
fn the_generated_histories_reach_every_event_every_state_and_every_way_a_publication_ends() {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for number in 0..GENERATED {
        let events = generated(SEED, number);
        let (books, outcomes) = run_checked(&events).unwrap();
        for (event, outcome) in events.iter().zip(&outcomes) {
            let (second, event) = match event {
                Event::Second(event) => ("second ", &**event),
                event => ("", event),
            };
            let (late, event) = match event {
                Event::Stale { event, .. } => ("late ", &**event),
                event => ("", event),
            };
            let kind = format!("{event:?}");
            let mut words = kind.split([' ', '(', '{']);
            let kind = words.next().unwrap_or_default();
            // What other hands do is told apart by the form of the name.
            let kind = match event {
                Event::Plant(_) | Event::Damage(_) | Event::Remove(_) => {
                    format!("{kind} {}", words.next().unwrap_or_default())
                }
                _ => kind.to_owned(),
            };
            let came = match outcome {
                Outcome::Done | Outcome::Placed(_) => "done".to_owned(),
                Outcome::Refused(why) => {
                    let why = format!("{why:?}");
                    format!("refused {}", why.split(' ').next().unwrap_or_default())
                }
                Outcome::AskedAgain(_) => "asked again".to_owned(),
            };
            seen.insert(format!("{late}{kind}: {came}"));
            if !second.is_empty() {
                seen.insert(format!("{second}{late}{kind}: {came}"));
            }
        }
        let folded = books.fold();
        for one in folded.reservations.values() {
            seen.insert(format!("{:?}", one.state));
            if one.epochs.len() > 1 {
                seen.insert("two holds".to_owned());
            }
            if one.unexplained > 0 {
                seen.insert("a place nothing accounts for".to_owned());
            }
            if one.unknown {
                seen.insert("an amount that is unknown".to_owned());
            }
            if one.state == State::Interrupted && one.corrections > 0 {
                seen.insert("a correction that no longer stands".to_owned());
            }
        }
        let balance = books.balance();
        if balance.debt > 0 {
            seen.insert("in debt".to_owned());
        }
        if balance.needs_owner {
            seen.insert("needs its owner".to_owned());
        }
        if folded.epoch() > 1 {
            seen.insert("a second epoch".to_owned());
        }
    }
    let wanted = [
        "Reset: done",
        "Admit: done",
        "Admit: refused OverAllowance",
        "Admit: refused NeedsOwner",
        "late Admit: asked again",
        "late Admit: done",
        "Child: done",
        "Dispatch: done",
        "Dispatch: refused OverRun",
        "Dispatch: refused AlreadyClosed",
        "Dispatch: refused Frozen",
        "Dispatch: refused NeedsOwner",
        "late Dispatch: done",
        "Settle: done",
        "late Settle: asked again",
        "Settle: refused NotExecuting",
        "Charge: done",
        "Charge: refused AlreadySettled",
        "End: refused ChildrenExecuting",
        "End: refused RequestsOutstanding",
        "late Reset: asked again",
        "End: done",
        "late End: done",
        "late End: asked again",
        "Abandon: done",
        "Crash: done",
        "Recover: done",
        "late Recover: asked again",
        "DeletePrune: done",
        "Adjust: done",
        "Adjust: refused NotCorrectable",
        "Adjust: refused OverCount",
        "Take: done",
        "GiveBack: done",
        "Damage Settled: done",
        "Damage Reset: done",
        "Damage Adjusted: done",
        "Plant Held: done",
        "Plant Dispatched: done",
        "Plant Settled: done",
        "Plant Settling: done",
        "Plant Closed: done",
        "Plant Place: done",
        "Plant Stray: done",
        "Plant Turn: done",
        "Plant Adjusted: done",
        "Plant Epoch: done",
        "Plant Reset: done",
        "Plant Reservation: done",
        "Remove Held: done",
        "Remove Reservation: done",
        "Remove Place: done",
        "Remove Dispatched: done",
        "Remove Settling: done",
        "Remove Settled: done",
        "Remove Closed: done",
        "Remove Adjusted: done",
        "Remove Turn: done",
        "Remove Reset: done",
        "Remove Epoch: done",
        "second Admit: done",
        "second Child: done",
        "second Dispatch: done",
        "second Settle: done",
        "second Charge: done",
        "second End: done",
        "second Crash: done",
        "second Recover: done",
        "second Reset: done",
        "second Adjust: done",
        "second late Admit: asked again",
        "Open",
        "Released",
        "Interrupted",
        "Frozen",
        "Adjusted",
        "in debt",
        "needs its owner",
        "a second epoch",
        "two holds",
        "a place nothing accounts for",
        "an amount that is unknown",
        "a correction that no longer stands",
    ];
    // Five ways are too rare to be met by chance, and are met by name: a correction
    // under what whole settlements say (history 33), one that cannot be held (34),
    // one judged late (41), a dispatch judged late that is asked again and is over
    // what its reservation has left (39), and a place whose guard is taken (below).
    let missing: Vec<&str> = wanted
        .into_iter()
        .filter(|one| !seen.contains(*one))
        .collect();
    assert!(missing.is_empty(), "never reached: {missing:?}\n{seen:#?}");
}

#[test]
fn a_name_is_written_one_way_and_read_back_strictly() {
    let (run, by) = (Run(3), Run(12));
    let forms = [
        (
            Name::Reset {
                epoch: 2,
                micros: 500,
            },
            "spend/reset/2/500",
        ),
        (Name::Epoch(2), "spend/epoch/2"),
        (Name::Turn { epoch: 0, turn: 41 }, "spend/turn/0/41"),
        (Name::Reservation(run), "spend/reservations/7000-3"),
        (
            Name::Held {
                epoch: 0,
                run,
                micros: 0,
            },
            "spend/held/0/7000-3/0",
        ),
        (Name::Place { run, place: 1 }, "spend/place/7000-3/1"),
        (
            Name::Dispatched {
                run,
                place: 2,
                by,
                micros: 250_000,
            },
            "spend/dispatched/7000-3/2/7000-12/250000",
        ),
        (Name::Settling { run, place: 2 }, "spend/settling/7000-3/2"),
        (
            Name::Settled {
                run,
                place: 2,
                stage: Stage::NotSent,
                micros: 0,
            },
            "spend/settled/7000-3/2/not_sent/0",
        ),
        (
            Name::Settled {
                run,
                place: 2,
                stage: Stage::Answered,
                micros: 7,
            },
            "spend/settled/7000-3/2/answered/7",
        ),
        (
            Name::Settled {
                run,
                place: 2,
                stage: Stage::Unresolved,
                micros: 7,
            },
            "spend/settled/7000-3/2/unresolved/7",
        ),
        (
            Name::Closed {
                run,
                released: true,
            },
            "spend/closed/7000-3/released",
        ),
        (
            Name::Closed {
                run,
                released: false,
            },
            "spend/closed/7000-3/held",
        ),
        (
            Name::Adjusted {
                run,
                place: 4,
                turn: 6,
                seal: 0x00AB_CDEF_0123_4567,
                micros: 9,
            },
            "spend/adjusted/7000-3/4/6/00abcdef01234567/9",
        ),
    ];
    for (name, text) in &forms {
        assert_eq!(name.to_string(), *text);
        assert_eq!(Name::parse(text).as_ref(), Some(name), "{text}");
        let of_none = matches!(
            name,
            Name::Reset { .. } | Name::Epoch(_) | Name::Turn { .. }
        );
        assert_eq!(name.of().is_some(), !of_none);
    }
    // The largest amount a name can state is one under the largest number there is,
    // which is no amount (ADR 0007).
    let most = format!("spend/reset/1/{}", u64::MAX - 1);
    assert!(Name::parse(&most).is_some());
    let refused = [
        format!("spend/reset/1/{}", u64::MAX),
        "spend/reset/1/184467440737095516150".to_owned(),
        "spend/reset/0/5".to_owned(),
        "spend/reset/01/5".to_owned(),
        "spend/reset/1/05".to_owned(),
        "spend/reset/1/".to_owned(),
        "spend/reset/1/5/".to_owned(),
        "spend/reset/1/+5".to_owned(),
        "spend/reset/1/5 ".to_owned(),
        "spend/reset/1".to_owned(),
        "spend/turn/1".to_owned(),
        "spend/turn/1/0".to_owned(),
        "spend/turn/01/1".to_owned(),
        "spend/turn/1/2/3".to_owned(),
        "spend/epoch/0".to_owned(),
        "spend/epoch/1/2".to_owned(),
        "spend/adjusted/7000-3/0/6/00abcdef01234567/9".to_owned(),
        "spend/adjusted/7000-3/4/0/00abcdef01234567/9".to_owned(),
        "spend/adjusted/7000-3/4/6/abcdef01234567/9".to_owned(),
        "spend/adjusted/7000-3/4/6/00ABCDEF01234567/9".to_owned(),
        "spend/adjusted/7000-3/4/6/00abcdef0123456g/9".to_owned(),
        "spend/adjusted/7000-3/4/6/+0abcdef01234567/9".to_owned(),
        "spend/adjusted/7000-3/4/6/9".to_owned(),
        "spend/adjusted/7000-3/4/9".to_owned(),
        "spend/place/7000-3/0".to_owned(),
        "spend/place/7000-03/1".to_owned(),
        "spend/place/7001-3/1".to_owned(),
        "spend/place/3/1".to_owned(),
        "spend/settled/7000-3/2/paid/7".to_owned(),
        "spend/settled/7000-3/2/Answered/7".to_owned(),
        "spend/closed/7000-3".to_owned(),
        "spend/closed/7000-3/open".to_owned(),
        "spend/released/7000-3".to_owned(),
        "spend/held/7000-3/5".to_owned(),
        "spend/unknown/7000-3".to_owned(),
        "spend/".to_owned(),
        "spend".to_owned(),
        "Spend/turn/1/1".to_owned(),
        "requests/turn/1/1".to_owned(),
        format!("spend/turn/1/{}", "1".repeat(200)),
    ];
    for text in &refused {
        assert_eq!(Name::parse(text), None, "{text}");
    }
}

#[test]
fn the_fold_reads_names_and_nothing_else_and_one_damaged_name_freezes_one_reservation() {
    // Two reservations, one of them with a request settled under its bound.
    let events = [
        reset(30),
        admit(1, 10),
        admit(2, 10),
        dispatch(1, 5),
        settle(1, 1, 3),
        Event::End { run: Run(1) },
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    assert!(
        outcomes
            .iter()
            .all(|one| !matches!(one, Outcome::Refused(_))),
        "{outcomes:?}"
    );
    let folded = books.fold();
    assert_eq!(folded, fold(books.names(), &BTreeSet::new()));
    assert_eq!(folded.reservations[&Run(1)].state, State::Released);
    assert_eq!(folded.reservations[&Run(1)].counts, 3);
    assert_eq!(folded.balance().left, 17);
    // Each name of the first, damaged alone, freezes it at its amount and leaves the
    // second as it was. Nothing stops the fold.
    let own: Vec<&Name> = books
        .names()
        .iter()
        .filter(|name| name.of() == Some(Run(1)))
        .collect();
    assert_eq!(own.len(), 8, "{own:?}");
    for name in own {
        let damaged = BTreeSet::from([name.clone()]);
        let frozen = fold(books.names(), &damaged);
        assert_eq!(frozen.reservations[&Run(1)].state, State::Frozen, "{name}");
        // The amount, the bound, the charge, and the amount for the closing's place.
        assert_eq!(
            frozen.reservations[&Run(1)].counts,
            10 + 5 + 3 + 10,
            "{name}"
        );
        assert_eq!(
            frozen.reservations[&Run(2)],
            folded.reservations[&Run(2)],
            "{name}"
        );
        // It stops nothing else: the store has no need of its owner, and what is
        // left is what the allowance has beyond 38 counted, which is nothing.
        let balance = frozen.balance();
        assert_eq!(
            (balance.counted, balance.left, balance.debt),
            (38, 0, 8),
            "{name}"
        );
        assert!(!balance.needs_owner, "{name}");
    }
    // A name that belongs to no reservation freezes none.
    let turn = BTreeSet::from([Name::Turn { epoch: 1, turn: 1 }]);
    assert_eq!(fold(books.names(), &turn), folded);
}

/// The seal of these names, none damaged, in the order the store keeps names.
fn sealed(names: &[Name]) -> u64 {
    let names: BTreeSet<&Name> = names.iter().collect();
    seal(names.into_iter().map(|name| (name, false)))
}

/// The books of one reservation written by hand, folded: its state, what is
/// outstanding and what it counts for.
fn folded(names: &[Name], damaged: &[Name]) -> (State, u128, u128) {
    let names: BTreeSet<Name> = names.iter().cloned().collect();
    let damaged: BTreeSet<Name> = damaged.iter().cloned().collect();
    let one = fold(&names, &damaged).reservations[&Run(1)].clone();
    (one.state, one.outstanding, one.counts)
}

#[test]
fn what_the_ledger_could_not_have_written_is_frozen_at_the_most_it_can_mean() {
    let run = Run(1);
    let base = || Books::after(&[reset(30), admit(1, 10), dispatch(1, 4)]).0;
    let with = |plants: &[Name]| {
        let mut books = base();
        for plant in plants {
            books.apply(&Event::Plant(plant.clone()));
        }
        let one = books.reservation(run).unwrap();
        assert_eq!(one.state, State::Frozen, "{plants:?}");
        assert!(!one.reachable, "{plants:?}");
        (one.charged, one.outstanding, one.counts)
    };
    let dispatched = |place, micros| Name::Dispatched {
        run,
        place,
        by: run,
        micros,
    };
    let answered = |place, micros| Name::Settled {
        run,
        place,
        stage: Stage::Answered,
        micros,
    };
    let place = |place| Name::Place { run, place };
    let guard = |place| Name::Settling { run, place };
    let held = |micros| Name::Held {
        epoch: 1,
        run,
        micros,
    };
    let closed = |released| Name::Closed { run, released };
    // A second dispatch at a place: each may be a request, so both bounds.
    assert_eq!(with(&[dispatched(1, 13)]), (0, 17, 27));
    // A settlement where nothing was dispatched: its charge.
    assert_eq!(with(&[answered(2, 20)]), (20, 4, 44));
    assert_eq!(with(&[place(2), guard(2), answered(2, 2)]), (2, 4, 26));
    // A settlement without its guard gives up nothing of the bound; a guard alone,
    // a second hold of less, a name of no form, a closing without its place.
    assert_eq!(with(&[answered(1, 1)]), (1, 4, 15));
    for plant in [
        guard(1),
        held(2),
        Name::Stray { run, count: 1 },
        closed(true),
    ] {
        assert_eq!(with(std::slice::from_ref(&plant)), (0, 4, 14), "{plant}");
    }
    // A dispatch without its place: its bound.
    assert_eq!(with(&[dispatched(2, 3)]), (0, 7, 17));
    // A place that nothing is at: the amount once more.
    assert_eq!(with(&[place(2)]), (0, 4, 24));
    // A guard, and a correction, two past the last place: the amount for each
    // number up to it that holds no dispatch.
    assert_eq!(with(&[guard(3)]), (0, 4, 34));
    let correction = Name::Adjusted {
        run,
        place: 3,
        turn: 1,
        seal: 0,
        micros: 0,
    };
    assert_eq!(with(&[correction]), (0, 4, 34));
    // A hold of more is the amount.
    assert_eq!(with(&[held(25)]), (0, 4, 29));
    // Both closings, each with a place.
    assert_eq!(
        with(&[closed(true), closed(false), place(2), place(3)]).2,
        34
    );
    // A second settlement of a place that is settled: every charge.
    let mut books = base();
    assert_eq!(books.apply(&settle(1, 1, 1)), Outcome::Done);
    assert_eq!(books.reservation(run).unwrap().state, State::Open);
    books.apply(&Event::Plant(answered(1, 7)));
    let one = books.reservation(run).unwrap();
    assert_eq!((one.state, one.charged, one.counts), (State::Frozen, 8, 22));
    // Names that name a run with no hold are a reservation of nothing, frozen, in
    // every epoch: what they say was charged is counted.
    let mut books = base();
    books.apply(&Event::Plant(Name::Settled {
        run: Run(9),
        place: 1,
        stage: Stage::Unresolved,
        micros: 6,
    }));
    let stray = books.reservation(Run(9)).unwrap();
    assert_eq!(
        (stray.state, stray.epoch, stray.counts),
        (State::Frozen, None, 6)
    );
    assert!(stray.counts_in(0) && stray.counts_in(1) && stray.counts_in(7));
    assert_eq!(books.balance().counted, 10 + 6);
    assert!(books.balance().needs_owner);
    // Two resets of one epoch: the smaller allowance.
    books.apply(&Event::Plant(Name::Reset {
        epoch: 1,
        micros: 12,
    }));
    assert_eq!(books.balance().allowed, 12);
    assert_eq!((books.balance().left, books.balance().debt), (0, 4));
}

#[test]
fn the_recogniser_takes_what_the_ledger_writes_and_nothing_else() {
    // After every event of every named history in which other hands do nothing,
    // every reservation is one the recogniser takes.
    let mut whole = 0;
    for history in histories() {
        if tampered(&history.events) {
            continue;
        }
        let mut books = Books::new();
        for event in &history.events {
            books.apply(event);
            for (run, one) in &books.fold().reservations {
                assert!(one.reachable, "{}: {run} after {event:?}", history.name);
            }
            assert!(!books.balance().needs_owner, "{}: {event:?}", history.name);
        }
        whole += 1;
    }
    assert!(whole >= 35, "{whole}");
    // Books with every kind of name the ledger makes: two requests, one settled
    // over its bound, a closing by recovery, a correction.
    let events = [
        reset(30),
        admit(1, 10),
        dispatch(1, 3),
        dispatch(1, 4),
        settle(1, 1, 9),
        Event::Crash,
        Event::Recover,
        Event::Adjust {
            run: Run(1),
            micros: 9,
        },
    ];
    let (books, _) = run_checked(&events).unwrap();
    let one = books.reservation(Run(1)).unwrap();
    assert_eq!(
        (one.state, one.reachable, one.counts),
        (State::Adjusted, true, 9)
    );
    // Each name of it lost alone leaves a set the ledger could not have written;
    // and so does each of these made.
    let own: Vec<Name> = books
        .names()
        .iter()
        .filter(|name| name.of() == Some(Run(1)))
        .cloned()
        .collect();
    assert_eq!(own.len(), 13, "{own:?}");
    for name in &own {
        let mut lost = books.clone();
        assert_eq!(lost.apply(&Event::Remove(name.clone())), Outcome::Done);
        let one = lost.reservation(Run(1)).unwrap();
        assert_eq!((one.reachable, one.state), (false, State::Frozen), "{name}");
    }
    let run = Run(1);
    let made = [
        Name::Stray { run, count: 1 },
        Name::Held {
            epoch: 1,
            run,
            micros: 11,
        },
        Name::Held {
            epoch: 2,
            run,
            micros: 1,
        },
        Name::Place { run, place: 6 },
        Name::Place { run, place: 7 },
        Name::Dispatched {
            run,
            place: 1,
            by: run,
            micros: 1,
        },
        Name::Dispatched {
            run,
            place: 6,
            by: run,
            micros: 1,
        },
        Name::Settling { run, place: 2 },
        Name::Settling { run, place: 3 },
        Name::Settled {
            run,
            place: 2,
            stage: Stage::Answered,
            micros: 1,
        },
        Name::Settled {
            run,
            place: 1,
            stage: Stage::Answered,
            micros: 1,
        },
        Name::Settled {
            run,
            place: 5,
            stage: Stage::Answered,
            micros: 1,
        },
        Name::Closed {
            run,
            released: true,
        },
        Name::Adjusted {
            run,
            place: 5,
            turn: 1,
            seal: 0,
            micros: 0,
        },
        Name::Adjusted {
            run,
            place: 2,
            turn: 1,
            seal: 0,
            micros: 0,
        },
    ];
    for name in &made {
        let mut planted = books.clone();
        planted.apply(&Event::Plant(name.clone()));
        let one = planted.reservation(run).unwrap();
        assert_eq!((one.reachable, one.state), (false, State::Frozen), "{name}");
        assert!(one.counts >= 13, "{name}: {}", one.counts);
    }
    // A correction at a place that is not there, at a dispatch's, or at another
    // correction's, with a place made so that as many hold no dispatch as before.
    for place in [9, 2, 5] {
        let mut planted = books.clone();
        for name in [
            Name::Adjusted {
                run,
                place,
                turn: 1,
                seal: 0,
                micros: 0,
            },
            Name::Place { run, place: 6 },
        ] {
            planted.apply(&Event::Plant(name));
        }
        let one = planted.reservation(run).unwrap();
        assert_eq!(
            (one.reachable, one.state),
            (false, State::Frozen),
            "{place}"
        );
    }
    // Damage is not in a name: it freezes all the same.
    for name in &own {
        let mut hurt = books.clone();
        hurt.apply(&Event::Damage(name.clone()));
        let one = hurt.reservation(run).unwrap();
        assert_eq!((one.reachable, one.state), (false, State::Frozen), "{name}");
    }
}

#[test]
fn a_settlement_is_whole_only_at_a_charge_its_stage_allows() {
    let run = Run(1);
    let base = [
        Name::Reservation(run),
        Name::Held {
            epoch: 0,
            run,
            micros: 30,
        },
        Name::Place { run, place: 1 },
        Name::Dispatched {
            run,
            place: 1,
            by: run,
            micros: 30,
        },
        Name::Settling { run, place: 1 },
    ];
    let settled = |stage, micros| Name::Settled {
        run,
        place: 1,
        stage,
        micros,
    };
    let with = |more: &[Name]| [&base[..], more].concat();
    // Not sent, nothing; unresolved, the bound; answered, any charge under it.
    for (stage, micros) in [
        (Stage::NotSent, 0),
        (Stage::Unresolved, 30),
        (Stage::Answered, 0),
        (Stage::Answered, 2),
        (Stage::Answered, 30),
    ] {
        let found = folded(&with(&[settled(stage, micros)]), &[]);
        assert_eq!(
            found,
            (State::Open, 0, u128::from(micros).max(30)),
            "{stage:?} {micros}"
        );
    }
    // Any other pair is no settlement: the bound stays.
    for (stage, micros) in [
        (Stage::NotSent, 1),
        (Stage::Unresolved, 0),
        (Stage::Unresolved, 29),
        (Stage::Unresolved, 31),
    ] {
        let found = folded(&with(&[settled(stage, micros)]), &[]);
        let most = 30 + 30 + u128::from(micros);
        assert_eq!(found, (State::Frozen, 30, most), "{stage:?} {micros}");
    }
    // The chain without the dispatch's place; with a name of it damaged; with a
    // second dispatch; with a second settlement.
    let whole = with(&[settled(Stage::Answered, 2)]);
    let no_place: Vec<Name> = whole
        .iter()
        .filter(|name| !matches!(name, Name::Place { .. }))
        .cloned()
        .collect();
    assert_eq!(folded(&no_place, &[]), (State::Frozen, 30, 62));
    for hurt in &whole[2..] {
        let found = folded(&whole, std::slice::from_ref(hurt));
        assert_eq!(found, (State::Frozen, 30, 62), "{hurt}");
    }
    let twice = with(&[
        settled(Stage::Answered, 2),
        Name::Dispatched {
            run,
            place: 1,
            by: Run(2),
            micros: 5,
        },
    ]);
    assert_eq!(folded(&twice, &[]), (State::Frozen, 35, 67));
    let two = with(&[settled(Stage::Answered, 2), settled(Stage::Answered, 3)]);
    assert_eq!(folded(&two, &[]), (State::Frozen, 30, 65));
    // An unresolved charge over the bound, with a place as an overrun takes one.
    let over = with(&[
        settled(Stage::Unresolved, 31),
        Name::Place { run, place: 2 },
    ]);
    assert_eq!(folded(&over, &[]).0, State::Frozen);
    // Two settlements that together are over the bound took no place: one that is
    // there is counted at the amount.
    let two_over = with(&[
        settled(Stage::Answered, 20),
        settled(Stage::Answered, 21),
        Name::Place { run, place: 2 },
    ]);
    assert_eq!(
        folded(&two_over, &[]),
        (State::Frozen, 30, 30 + 30 + 41 + 30)
    );
    // What a correction cannot go under is what settlements with their whole chain
    // say, in a frozen reservation too: not one without its guard, with a name
    // damaged, or at a charge its stage does not allow.
    let settled_whole = |names: &[Name], damaged: &[Name]| {
        let names: BTreeSet<Name> = names.iter().cloned().collect();
        let damaged: BTreeSet<Name> = damaged.iter().cloned().collect();
        let one = fold(&names, &damaged).reservations[&run].clone();
        assert_eq!(one.state, State::Frozen);
        one.settled
    };
    let stray = Name::Stray { run, count: 1 };
    let frozen = with(&[settled(Stage::Answered, 2), stray.clone()]);
    assert_eq!(settled_whole(&frozen, &[]), 2);
    let no_guard: Vec<Name> = frozen
        .iter()
        .filter(|name| !matches!(name, Name::Settling { .. }))
        .cloned()
        .collect();
    assert_eq!(settled_whole(&no_guard, &[]), 0);
    assert_eq!(settled_whole(&frozen, &frozen[5..6]), 0);
    let not_allowed = with(&[settled(Stage::NotSent, 2), stray]);
    assert_eq!(settled_whole(&not_allowed, &[]), 0);
}

#[test]
fn a_closing_and_a_correction_count_only_as_the_ledger_writes_them() {
    let run = Run(1);
    let open = [
        Name::Reservation(run),
        Name::Held {
            epoch: 0,
            run,
            micros: 10,
        },
        Name::Place { run, place: 1 },
        Name::Dispatched {
            run,
            place: 1,
            by: run,
            micros: 0,
        },
    ];
    let closing = |released| Name::Closed { run, released };
    let place = |place| Name::Place { run, place };
    // A closing as released with a place that is not settled, its bound nothing.
    let released = [&open[..], &[closing(true), place(2)]].concat();
    assert_eq!(folded(&released, &[]), (State::Frozen, 0, 20));
    let held = [&open[..], &[closing(false), place(2)]].concat();
    assert_eq!(folded(&held, &[]), (State::Interrupted, 0, 10));
    // Both closings, each with a place.
    let both = [&held[..], &[closing(true), place(3)]].concat();
    assert_eq!(folded(&both, &[]).0, State::Frozen);
    // A correction stands with its place, the turn it names, and a seal of exactly
    // the other names.
    let turn = Name::Turn { epoch: 0, turn: 5 };
    let over = sealed(&held);
    let correction = |place, turn, seal, micros| Name::Adjusted {
        run,
        place,
        turn,
        seal,
        micros,
    };
    let stands = [
        &held[..],
        &[correction(3, 5, over, 3), place(3), turn.clone()],
    ]
    .concat();
    assert_eq!(folded(&stands, &[]), (State::Adjusted, 0, 3));
    // Without the turn, without the place, with another seal, damaged, or with any
    // other name of the reservation damaged since: it says nothing.
    let no_turn = [&held[..], &[correction(3, 5, over, 3), place(3)]].concat();
    assert_eq!(folded(&no_turn, &[]), (State::Interrupted, 0, 10));
    let no_place = [&held[..], &[correction(3, 5, over, 3), turn.clone()]].concat();
    assert_eq!(folded(&no_place, &[]), (State::Frozen, 0, 30));
    let other_seal = [
        &held[..],
        &[correction(3, 5, over + 1, 3), place(3), turn.clone()],
    ]
    .concat();
    assert_eq!(folded(&other_seal, &[]), (State::Interrupted, 0, 10));
    for hurt in [&stands[6], &stands[1]] {
        let found = folded(&stands, std::slice::from_ref(hurt));
        assert_eq!(found, (State::Frozen, 0, 30), "{hurt}");
    }
    // A correction that no longer stands still says the most it said.
    let more = [
        &held[..],
        &[correction(3, 5, over + 1, 50), place(3), turn.clone()],
    ]
    .concat();
    assert_eq!(folded(&more, &[]), (State::Interrupted, 0, 50));
    // At a place a dispatch is at, or a guard of a settlement, sealed as it would
    // be; of a reservation that is not closed.
    let but_first: Vec<Name> = held
        .iter()
        .filter(|name| **name != place(1))
        .cloned()
        .collect();
    let at_dispatch = [
        &held[..],
        &[correction(1, 5, sealed(&but_first), 3), turn.clone()],
    ]
    .concat();
    assert_eq!(folded(&at_dispatch, &[]), (State::Frozen, 0, 20));
    let guarded = [&held[..], &[Name::Settling { run, place: 3 }]].concat();
    let at_guard = [
        &guarded[..],
        &[
            correction(3, 5, sealed(&guarded), 3),
            place(3),
            turn.clone(),
        ],
    ]
    .concat();
    assert_eq!(folded(&at_guard, &[]), (State::Frozen, 0, 30));
    let over_open = sealed(&open);
    let not_closed = [
        &open[..],
        &[correction(2, 5, over_open, 3), place(2), turn.clone()],
    ]
    .concat();
    assert_eq!(folded(&not_closed, &[]), (State::Frozen, 0, 20));
    // Two corrections sealed over the same names: each is a name the other was not
    // made over, and neither stands. One made over the other does.
    let pair = [
        &held[..],
        &[
            correction(3, 5, over, 0),
            correction(4, 5, over, 9),
            place(3),
            place(4),
            turn.clone(),
        ],
    ]
    .concat();
    assert_eq!(folded(&pair, &[]), (State::Interrupted, 0, 10));
    let first = [&held[..], &[correction(3, 5, over, 9), place(3)]].concat();
    let over_first = sealed(&first);
    let chain = [
        &first[..],
        &[correction(4, 5, over_first, 2), place(4), turn],
    ]
    .concat();
    assert_eq!(folded(&chain, &[]), (State::Adjusted, 0, 2));
    // A correction that no longer stands on a reservation that released: frozen.
    let settled = [
        &open[..],
        &[
            Name::Settling { run, place: 1 },
            Name::Settled {
                run,
                place: 1,
                stage: Stage::NotSent,
                micros: 0,
            },
            closing(true),
            place(2),
        ],
    ]
    .concat();
    assert_eq!(folded(&settled, &[]), (State::Released, 0, 0));
    let stale = [&settled[..], &[correction(3, 5, 0, 0), place(3)]].concat();
    assert_eq!(folded(&stale, &[]), (State::Frozen, 0, 30));
}

#[test]
fn a_refusal_changes_no_name() {
    let events = [
        reset(5),
        admit(1, 5),
        admit(2, 1),
        dispatch(1, 6),
        Event::Settle {
            root: Run(1),
            place: 1,
            how: How::Unresolved,
        },
        Event::Adjust {
            run: Run(1),
            micros: 0,
        },
        Event::End { run: Run(2) },
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(
        outcomes[2..],
        [
            Outcome::Refused(Why::OverAllowance { left: 0 }),
            Outcome::Refused(Why::OverRun { left: 5 }),
            Outcome::Refused(Why::NoSuchPlace),
            Outcome::Refused(Why::NotCorrectable),
            Outcome::Refused(Why::NotExecuting),
        ]
    );
    assert_eq!(books.names(), books.names_after(2).unwrap());
}

#[test]
fn a_place_whose_guard_is_taken_is_not_settled_again() {
    let events = [
        reset(30),
        admit(1, 10),
        dispatch(1, 4),
        Event::Plant(Name::Settling {
            run: Run(1),
            place: 1,
        }),
        settle(1, 1, 1),
        // A place this process did not dispatch is nobody's to settle.
        Event::Settle {
            root: Run(1),
            place: 2,
            how: How::NotSent,
        },
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(outcomes[4], Outcome::Refused(Why::AlreadySettled));
    assert_eq!(outcomes[5], Outcome::Refused(Why::NoSuchPlace));
    assert_eq!(books.names(), books.names_after(4).unwrap());
    let one = books.reservation(Run(1)).unwrap();
    assert_eq!(
        (one.state, one.outstanding, one.counts),
        (State::Frozen, 4, 14)
    );
}

#[test]
fn a_closing_releases_only_what_is_open_and_whole_and_nothing_is_dispatched_after_it() {
    // A reservation that is damaged while its run executes, with nothing dispatched:
    // its run's end closes it, and the closing says that nothing is released. And so
    // does recovery, of one whose process died.
    let run = Run(1);
    for close in [vec![Event::End { run }], vec![Event::Crash, Event::Recover]] {
        let mut events = vec![
            reset(30),
            admit(1, 10),
            Event::Damage(Name::Reservation(run)),
        ];
        events.extend(close);
        let (books, _) = run_checked(&events).unwrap();
        let held = Name::Closed {
            run,
            released: false,
        };
        let released = Name::Closed {
            run,
            released: true,
        };
        assert!(books.names().contains(&held));
        assert!(!books.names().contains(&released));
    }
    // A reservation that other hands closed while its run executes takes no dispatch;
    // and a closing without the place it takes releases nothing.
    let events = [
        reset(30),
        admit(1, 10),
        Event::Plant(Name::Closed {
            run,
            released: true,
        }),
        dispatch(1, 4),
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(outcomes[3], Outcome::Refused(Why::AlreadyClosed));
    let one = books.reservation(run).unwrap();
    assert_eq!((one.state, one.counts), (State::Frozen, 10));
    // Nothing is dispatched under a reservation that is frozen and not closed.
    let events = [
        reset(30),
        admit(1, 10),
        Event::Plant(Name::Stray { run, count: 1 }),
        dispatch(1, 4),
    ];
    let (_, outcomes) = run_checked(&events).unwrap();
    assert_eq!(outcomes[3], Outcome::Refused(Why::Frozen));
}

#[test]
fn a_correction_never_counts_under_what_whole_settlements_say_whoever_wrote_it() {
    // Interrupted with 4 charged and 5 outstanding. The command refuses a correction
    // under 4; a whole correction that other hands made counts 4 all the same.
    let run = Run(1);
    let mut events = vec![
        reset(30),
        admit(1, 10),
        dispatch(1, 5),
        settle(1, 1, 4),
        dispatch(1, 5),
        Event::Crash,
        Event::Recover,
        Event::Adjust { run, micros: 3 },
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(
        outcomes[7],
        Outcome::Refused(Why::BelowSettled { settled: 4 })
    );
    // The chain of a correction: the name, the place it names and the turn it
    // names. Each alone, and any two, is no correction.
    let place = books.next_place(run);
    let turn = books.fold().next_turn(1);
    let chain = [
        Name::Adjusted {
            run,
            place,
            turn,
            seal: books.seal_for(run, place),
            micros: 1,
        },
        Name::Place { run, place },
        Name::Turn { epoch: 1, turn },
    ];
    for missing in 0..3 {
        let mut part = events.clone();
        let rest = chain.iter().enumerate().filter(|(at, _)| *at != missing);
        part.extend(rest.map(|(_, name)| Event::Plant(name.clone())));
        let (books, _) = run_checked(&part).unwrap();
        let one = books.reservation(run).unwrap();
        assert_ne!(one.state, State::Adjusted, "without {}", chain[missing]);
        assert!(one.counts >= 10, "without {}", chain[missing]);
    }
    events.extend(chain.iter().map(|name| Event::Plant(name.clone())));
    let (books, _) = run_checked(&events).unwrap();
    let one = books.reservation(run).unwrap();
    assert_eq!(
        (one.state, one.settled, one.counts),
        (State::Adjusted, 4, 4)
    );
    assert_eq!(books.balance().left, 26);
}

#[test]
fn a_name_made_or_lost_or_damaged_after_a_correction_ends_it() {
    let run = Run(1);
    let events = [
        reset(30),
        admit(1, 10),
        dispatch(1, 5),
        Event::Crash,
        Event::Recover,
        Event::Adjust { run, micros: 2 },
    ];
    let (books, _) = run_checked(&events).unwrap();
    assert_eq!(books.reservation(run).unwrap().counts, 2);
    let own: Vec<Name> = books
        .names()
        .iter()
        .filter(|name| name.of() == Some(run))
        .cloned()
        .collect();
    let ended = |event: Event| {
        let mut events = events.to_vec();
        events.push(event.clone());
        let (books, _) = run_checked(&events).unwrap_or_else(|why| panic!("{event:?}: {why}"));
        let one = books.reservation(run).unwrap();
        assert_ne!(one.state, State::Adjusted, "{event:?}");
        // In full; but for the hold lost, which was the one name of the amount.
        let hold = matches!(event, Event::Remove(Name::Held { .. }));
        assert!(one.counts >= if hold { 5 } else { 10 }, "{event:?}");
        events
    };
    for name in &own {
        ended(Event::Damage(name.clone()));
        ended(Event::Remove(name.clone()));
    }
    ended(Event::Remove(Name::Turn { epoch: 1, turn: 3 }));
    let stray = Name::Stray { run, count: 1 };
    let mut events = ended(Event::Plant(stray.clone()));
    // The fold is of the names: with the name taken away again they are the names
    // the correction was made over, and it stands.
    events.push(Event::Remove(stray));
    let (books, _) = run_checked(&events).unwrap();
    let one = books.reservation(run).unwrap();
    assert_eq!((one.state, one.counts), (State::Adjusted, 2));
}

/// Every name of these books, lost alone: what the reservation of `run` counted
/// before, and what it is and counts after. `run_checked` holds each loss to the rule
/// that it lowers nothing; this gives the numbers to the test.
fn each_loss(events: &[Event], run: Run) -> Vec<(Name, u128, State, u128)> {
    let (books, _) = run_checked(events).unwrap();
    let before = books.reservation(run).unwrap().counts;
    let mut losses = Vec::new();
    for name in books.names() {
        let mut lost = events.to_vec();
        lost.push(Event::Remove(name.clone()));
        let (after, _) = run_checked(&lost).unwrap_or_else(|why| panic!("{name}: {why}"));
        let one = after.reservation(run).unwrap();
        losses.push((name.clone(), before, one.state, one.counts));
        // No more can be reserved, but for the hold, which is the one name the
        // amount is in.
        if !matches!(name, Name::Held { .. }) {
            assert!(after.balance().left <= books.balance().left, "{name}");
        }
    }
    losses
}

#[test]
fn losing_any_one_name_lowers_nothing() {
    let run = Run(1);
    // Open, with one request settled under its bound and one outstanding.
    let mut events = vec![
        reset(30),
        admit(1, 10),
        dispatch(1, 3),
        dispatch(1, 4),
        settle(1, 1, 2),
    ];
    let open = each_loss(&events, run);
    // Then the second settled at 9 over its bound of 4, and the run ended: released
    // at 11.
    events.push(settle(1, 2, 9));
    events.push(Event::End { run });
    let released = each_loss(&events, run);
    // Then another, interrupted with a request outstanding, and corrected.
    let other = Run(2);
    events.extend([
        admit(2, 10),
        dispatch(2, 6),
        Event::Crash,
        Event::Recover,
        Event::Adjust {
            run: other,
            micros: 1,
        },
    ]);
    let corrected = each_loss(&events, other);
    assert_eq!(run_checked(&events).unwrap().0.balance().left, 30 - 11 - 1);
    for (losses, was) in [(&open, 10), (&released, 11), (&corrected, 1)] {
        assert!(losses.len() > 10, "{}", losses.len());
        for (name, before, state, counts) in losses {
            assert_eq!(*before, was, "{name}");
            let own = name.of() == Some(if was == 1 { other } else { run });
            match name {
                // A name of another reservation, or of none, lowers nothing here.
                _ if !own => assert!(*counts >= was, "{name}"),
                // The hold is the one name the amount is in: what is known is left.
                Name::Held { .. } => assert_eq!(*state, State::Frozen, "{name}"),
                // A charge over its bound was in its settlement's name alone: the
                // bound and the amount are left.
                Name::Settled { micros: 9, .. } => {
                    assert_eq!(
                        (*state, *counts),
                        (State::Frozen, 10 + 7 + 2 + 20),
                        "{name}"
                    );
                }
                _ => {
                    assert_eq!(*state, State::Frozen, "{name}");
                    assert!(*counts >= was && *counts >= 10, "{name}: {counts}");
                }
            }
        }
    }
    // The hold lost: frozen at what its other names say, each place at the larger of
    // its bound and its charge, counted in every epoch, and nothing more is
    // dispatched under it.
    let hold = Name::Held {
        epoch: 1,
        run,
        micros: 10,
    };
    let mut lost = events[..5].to_vec();
    lost.push(Event::Remove(hold));
    lost.push(dispatch(1, 1));
    let (books, outcomes) = run_checked(&lost).unwrap();
    let one = books.reservation(run).unwrap();
    assert_eq!((one.state, one.counts, one.epoch), (State::Frozen, 9, None));
    assert_eq!(outcomes[6], Outcome::Refused(Why::Frozen));
    // A dispatch's name lost: its place is one nothing accounts for, counted at the
    // amount, because its bound was in that name alone.
    let dispatched = Name::Dispatched {
        run,
        place: 2,
        by: run,
        micros: 4,
    };
    let mut lost = events[..5].to_vec();
    lost.push(Event::Remove(dispatched));
    let (books, _) = run_checked(&lost).unwrap();
    let one = books.reservation(run).unwrap();
    assert_eq!(
        (one.state, one.unexplained, one.counts, one.left()),
        (State::Frozen, 1, 10 + 3 + 2 + 10, 0)
    );
    assert_eq!(books.run(run), Sent::Unknown { at_most: 25 });
    // A dispatch and its place lost together, below another place: the number that
    // is missing is counted at the amount.
    let mut lost = events[..5].to_vec();
    for name in [
        Name::Dispatched {
            run,
            place: 1,
            by: run,
            micros: 3,
        },
        Name::Place { run, place: 1 },
    ] {
        lost.push(Event::Remove(name));
    }
    let (books, _) = run_checked(&lost).unwrap();
    let one = books.reservation(run).unwrap();
    assert_eq!((one.state, one.unexplained), (State::Frozen, 1));
    assert_eq!(one.counts, 10 + 4 + 2 + 10);
    // The rule is of books that were whole. A correction the owner made over a name
    // other hands made says what it said for as long as it is there: with that name
    // taken away again and then the correction lost, the reservation is frozen at
    // what its names say, which is less than the correction said.
    let made = Name::Dispatched {
        run,
        place: 9,
        by: run,
        micros: 1000,
    };
    let mut events = vec![
        reset(100),
        admit(1, 10),
        dispatch(1, 5),
        Event::Crash,
        Event::Recover,
        Event::Plant(made.clone()),
        Event::Adjust { run, micros: 1000 },
        Event::Remove(made),
    ];
    let (books, _) = run_checked(&events).unwrap();
    let one = books.reservation(run).unwrap();
    assert_eq!((one.state, one.counts), (State::Interrupted, 1000));
    let correction = |name: &&Name| matches!(name, Name::Adjusted { .. });
    let correction = books.names().iter().find(correction).cloned().unwrap();
    events.push(Event::Remove(correction));
    let (books, _) = run_checked(&events).unwrap();
    let one = books.reservation(run).unwrap();
    assert_eq!((one.state, one.counts), (State::Frozen, 10 + 5 + 20));
}

#[test]
fn known_a_loss_that_leaves_what_the_ledger_could_have_written_is_believed() {
    let run = Run(1);
    let events = [
        reset(30),
        admit(1, 10),
        dispatch(1, 3),
        dispatch(1, 4),
        settle(1, 1, 2),
    ];
    let losing = |names: &[Name]| {
        let mut lost = events.to_vec();
        lost.extend(names.iter().cloned().map(Event::Remove));
        let (books, _) = run_checked(&lost).unwrap();
        books
            .reservation(run)
            .map(|one| (one.state, one.outstanding, one.counts))
    };
    // The last request sent, whole: its dispatch and its place.
    let last = [
        Name::Dispatched {
            run,
            place: 2,
            by: run,
            micros: 4,
        },
        Name::Place { run, place: 2 },
    ];
    assert_eq!(losing(&last), Some((State::Open, 0, 10)));
    // A settlement, whole: its guard and itself. The request reads as outstanding.
    let settlement = [
        Name::Settling { run, place: 1 },
        Name::Settled {
            run,
            place: 1,
            stage: Stage::Answered,
            micros: 2,
        },
    ];
    assert_eq!(losing(&settlement), Some((State::Open, 7, 10)));
    // Every name of the reservation: there is none.
    let (books, _) = run_checked(&events).unwrap();
    let all: Vec<Name> = books
        .names()
        .iter()
        .filter(|name| name.of() == Some(run))
        .cloned()
        .collect();
    assert_eq!(losing(&all), None);
    // In each the store's turns are no longer what its publications took, but for
    // the request, which took none: nothing is admitted until the owner resets.
    let mut lost = events.to_vec();
    lost.extend(settlement.iter().cloned().map(Event::Remove));
    assert!(run_checked(&lost).unwrap().0.balance().needs_owner);
    // The newest reset with its guard: the epoch before it is the newest again, and
    // needs its owner for the turn the reset took in it. With that turn lost too,
    // the books are those of before the reset, and are believed.
    let mut lost = vec![reset(30), admit(1, 10), reset(50)];
    lost.extend([
        Event::Remove(Name::Reset {
            epoch: 2,
            micros: 50,
        }),
        Event::Remove(Name::Epoch(2)),
    ]);
    let balance = run_checked(&lost).unwrap().0.balance();
    assert_eq!((balance.epoch, balance.left), (1, 0));
    assert!(balance.needs_owner);
    lost.push(Event::Remove(Name::Turn { epoch: 1, turn: 2 }));
    let balance = run_checked(&lost).unwrap().0.balance();
    assert_eq!((balance.epoch, balance.allowed, balance.left), (1, 30, 20));
    assert!(!balance.needs_owner);
}

#[test]
fn an_allowance_is_its_resets_only_with_the_guard_and_with_neither_damaged() {
    let names = [
        Name::Reset {
            epoch: 1,
            micros: 30,
        },
        Name::Epoch(1),
    ];
    for name in names {
        for hands in [Event::Damage(name.clone()), Event::Remove(name.clone())] {
            let events = [reset(30), hands, admit(1, 1)];
            let (books, outcomes) = run_checked(&events).unwrap();
            let balance = books.balance();
            assert_eq!((balance.epoch, balance.allowed), (1, 0), "{name}");
            assert_eq!(
                outcomes[2],
                Outcome::Refused(Why::OverAllowance { left: 0 }),
                "{name}"
            );
        }
    }
    // A reset other hands made for an epoch there is not, with no guard: an epoch
    // that allows nothing. With its guard it is a reset like the owner's ("Limits").
    let plant = Event::Plant(Name::Reset {
        epoch: 2,
        micros: 500,
    });
    let (books, _) = run_checked(&[reset(30), plant.clone()]).unwrap();
    assert_eq!((books.balance().epoch, books.balance().allowed), (2, 0));
    let guard = Event::Plant(Name::Epoch(2));
    let (books, _) = run_checked(&[reset(30), plant, guard]).unwrap();
    assert_eq!((books.balance().epoch, books.balance().allowed), (2, 500));
    // A second reset of the owner's epoch says nothing more than the smaller of the
    // two. With the owner's lost, the one other hands made has the guard to itself
    // and is the allowance: a whole chain is believed ("Limits", (a)).
    let more = Event::Plant(Name::Reset {
        epoch: 1,
        micros: 500,
    });
    let (books, _) = run_checked(&[reset(30), more.clone()]).unwrap();
    assert_eq!(books.balance().allowed, 30);
    let lost = Event::Remove(Name::Reset {
        epoch: 1,
        micros: 30,
    });
    let (books, outcomes) = run_checked(&[reset(30), more, lost, admit(1, 400)]).unwrap();
    assert_eq!(outcomes[3], Outcome::Done);
    assert_eq!((books.balance().allowed, books.balance().left), (500, 100));
}

#[test]
fn a_reset_does_not_clear_a_frozen_reservation_and_its_owners_correction_does() {
    // A reservation of 10, frozen by damage and closed by recovery, in the first
    // epoch. The owner resets: it counts in the new epoch as in every other, the
    // amount and the amount again for its closing's place, and all of the new
    // allowance is taken by it. The store has no need of its owner: what the
    // allowance does not cover is refused for that.
    let run = Run(1);
    let mut events = vec![
        reset(10),
        admit(1, 10),
        Event::Crash,
        Event::Damage(Name::Reservation(run)),
        Event::Recover,
        reset(20),
        admit(2, 1),
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(
        outcomes[6],
        Outcome::Refused(Why::OverAllowance { left: 0 })
    );
    let one = books.reservation(run).unwrap();
    assert_eq!(
        (one.state, one.epoch, one.counts),
        (State::Frozen, Some(1), 20)
    );
    let balance = books.balance();
    assert_eq!(
        (
            balance.epoch,
            balance.allowed,
            balance.counted,
            balance.left
        ),
        (2, 20, 20, 0)
    );
    assert!(!balance.needs_owner);
    // A second reset does not clear it either: 20 of the 40 are taken by it, and
    // the rest can be reserved.
    events.extend([reset(40), admit(2, 21), admit(2, 1)]);
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(
        outcomes[8..],
        [
            Outcome::Refused(Why::OverAllowance { left: 20 }),
            Outcome::Done
        ]
    );
    assert_eq!((books.balance().counted, books.balance().left), (21, 19));
    // The owner's correction of that reservation does: it falls under the epoch of
    // its hold again, and the newest has its whole allowance but for the 1.
    events.extend([Event::Adjust { run, micros: 4 }, admit(3, 1)]);
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(outcomes[10..], [Outcome::Done, Outcome::Done]);
    let one = books.reservation(run).unwrap();
    assert_eq!((one.state, one.counts), (State::Adjusted, 4));
    let balance = books.balance();
    assert_eq!((balance.counted, balance.left), (2, 38));
    assert!(!balance.needs_owner);
}

#[test]
fn known_one_hold_made_in_the_place_of_the_ledgers_states_the_epoch() {
    // A reservation of 10 with 10 outstanding. Its hold is lost, other hands make one
    // for it in another epoch, and the turn its admission took is lost too: the
    // names are a set the ledger could have written, and are believed ("Limits",
    // (a)). The reservation falls under the other epoch, and its own has all of its
    // allowance again.
    let run = Run(1);
    let mut events = vec![
        reset(30),
        admit(1, 10),
        dispatch(1, 10),
        Event::Remove(Name::Held {
            epoch: 1,
            run,
            micros: 10,
        }),
        Event::Plant(Name::Held {
            epoch: 4,
            run,
            micros: 10,
        }),
        Event::Remove(Name::Turn { epoch: 1, turn: 1 }),
    ];
    let (books, _) = run_checked(&events).unwrap();
    let one = books.reservation(run).unwrap();
    assert_eq!(
        (one.state, one.reachable, one.epoch, one.counts),
        (State::Open, true, Some(4), 10)
    );
    assert_eq!((books.balance().counted, books.balance().left), (0, 30));
    // Its owner's correction is sealed over that hold, and changes what it is
    // charged, not where.
    events.extend([
        Event::Crash,
        Event::Recover,
        Event::Adjust { run, micros: 10 },
    ]);
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(outcomes[8], Outcome::Done);
    let one = books.reservation(run).unwrap();
    assert_eq!(
        (one.state, one.epoch, one.counts),
        (State::Adjusted, Some(4), 10)
    );
    assert_eq!(books.balance().left, 30);
}

#[test]
fn known_an_admission_judged_before_other_hands_opened_an_epoch_falls_under_the_old_one() {
    // The owner allows 11, and an admission of 10 is judged. Other hands then make
    // the reset of a later epoch, which took no turn in this one, so the store does
    // not refuse the admission: it is published under the epoch it was judged in
    // ("Limits", (e)). That epoch is no longer the newest, and the newest allows
    // nothing: no more is admitted but a call of nothing.
    let later = Event::Plant(Name::Reset {
        epoch: 3,
        micros: 8,
    });
    let events = [
        reset(11),
        later,
        late(1, admit(1, 10)),
        admit(2, 0),
        admit(3, 1),
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(
        outcomes[2..],
        [
            Outcome::Done,
            Outcome::Done,
            Outcome::Refused(Why::OverAllowance { left: 0 }),
        ]
    );
    let one = books.reservation(Run(1)).unwrap();
    assert_eq!((one.epoch, one.counts), (Some(1), 10));
    let balance = books.balance();
    assert_eq!((balance.epoch, balance.allowed), (3, 0));
}

#[test]
fn known_after_the_largest_epoch_a_name_can_say_no_reset_follows() {
    // Other hands open the last epoch there can be. It allows nothing, as any epoch
    // without its reset does, and the owner's reset finds no epoch after it: it is
    // refused, and nothing but a call of nothing is admitted from then on.
    let last = Event::Plant(Name::Epoch(u64::MAX));
    let events = [reset(30), last, reset(5), admit(1, 0), admit(2, 1)];
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(
        outcomes[2..],
        [
            Outcome::Refused(Why::Taken),
            Outcome::Done,
            Outcome::Refused(Why::OverAllowance { left: 0 }),
        ]
    );
    let balance = books.balance();
    assert_eq!((balance.epoch, balance.allowed), (u64::MAX, 0));
}

#[test]
fn a_name_can_name_any_place_and_state_any_amount_and_the_fold_still_ends() {
    // A place at the largest number there is, and a hold of the largest amount, for
    // each of two reservations: the places that hold no dispatch are counted and not
    // walked, and the store's sum stops at the largest number.
    let most = u64::MAX - 1;
    let mut events = vec![reset(30), admit(1, 10), admit(2, 10)];
    for run in [Run(1), Run(2)] {
        events.push(Event::Plant(Name::Place {
            run,
            place: u64::MAX,
        }));
        events.push(Event::Plant(Name::Held {
            epoch: 1,
            run,
            micros: most,
        }));
    }
    let (books, _) = run_checked(&events).unwrap();
    let one = books.reservation(Run(1)).unwrap();
    assert_eq!((one.state, one.unexplained), (State::Frozen, u64::MAX));
    let places = u128::from(u64::MAX) * u128::from(most);
    assert_eq!(one.counts, u128::from(most) + places);
    let balance = books.balance();
    assert_eq!((balance.counted, balance.left), (u128::MAX, 0));
    assert!(balance.needs_owner);
}

#[test]
fn the_turns_of_an_epoch_are_exactly_those_its_publications_took() {
    // Every publication that can change what is left takes one: a reset, in the
    // epoch it closes; an admission; a settlement, under its bound too; a closing;
    // a correction. A dispatch takes none.
    let events = [
        reset(30),
        admit(1, 10),
        dispatch(1, 5),
        settle(1, 1, 3),
        Event::End { run: Run(1) },
        admit(2, 10),
        dispatch(2, 5),
        Event::Crash,
        Event::Recover,
        Event::Adjust {
            run: Run(2),
            micros: 0,
        },
    ];
    let turns = |books: &Books, epoch: u64| {
        let of = |name: &&Name| matches!(name, Name::Turn { epoch: of, .. } if *of == epoch);
        books.names().iter().filter(of).count()
    };
    let mut books = Books::new();
    let mut taken = Vec::new();
    for event in &events {
        books.apply(event);
        taken.push((turns(&books, 0), turns(&books, 1)));
        assert!(!books.balance().needs_owner, "{event:?}");
    }
    let wanted = [
        (1, 0),
        (1, 1),
        (1, 1),
        (1, 2),
        (1, 3),
        (1, 4),
        (1, 4),
        (1, 4),
        (1, 5),
        (1, 6),
    ];
    assert_eq!(taken, wanted);
    // A turn lost, a turn made, a turn made far ahead, and a name that takes a turn
    // made without one: each stops admissions, and the owner's reset starts them.
    let hands = [
        Event::Remove(Name::Turn { epoch: 1, turn: 6 }),
        Event::Remove(Name::Turn { epoch: 1, turn: 2 }),
        Event::Plant(Name::Turn { epoch: 1, turn: 7 }),
        Event::Plant(Name::Turn { epoch: 1, turn: 90 }),
        Event::Plant(Name::Settling {
            run: Run(2),
            place: 1,
        }),
        Event::Plant(Name::Held {
            epoch: 1,
            run: Run(7),
            micros: 0,
        }),
        Event::Plant(Name::Closed {
            run: Run(1),
            released: false,
        }),
    ];
    // A turn lost and another made: as many as are due, and not 1 to their count.
    let mut all = events.to_vec();
    all.extend([
        Event::Remove(Name::Turn { epoch: 1, turn: 2 }),
        Event::Plant(Name::Turn { epoch: 1, turn: 9 }),
        admit(3, 0),
    ]);
    let (_, outcomes) = run_checked(&all).unwrap();
    assert_eq!(outcomes[12], Outcome::Refused(Why::NeedsOwner));
    for hand in hands {
        let mut all = events.to_vec();
        all.extend([hand.clone(), admit(3, 0), reset(5), admit(3, 0)]);
        let (books, outcomes) = run_checked(&all).unwrap();
        assert_eq!(outcomes[11], Outcome::Refused(Why::NeedsOwner), "{hand:?}");
        // A reset starts them again: the new epoch's turns are its own. Where the
        // name made froze a reservation, each of these has one hold that states
        // its amount: it counts in the new epoch too, and stops nothing there.
        let frozen = |one: &&Standing| one.state == State::Frozen;
        let folded = books.fold();
        let frozen: Vec<&Standing> = folded.reservations.values().filter(frozen).collect();
        let turn = matches!(hand, Event::Remove(_) | Event::Plant(Name::Turn { .. }));
        assert_eq!(frozen.is_empty(), turn, "{hand:?}");
        assert!(frozen.iter().all(|one| !one.unknown), "{hand:?}");
        assert_eq!(outcomes[13], Outcome::Done, "{hand:?}");
        let balance = books.balance();
        assert!(!balance.needs_owner, "{hand:?}");
        let counted: u128 = frozen.iter().map(|one| one.counts).sum();
        assert_eq!(balance.counted, counted, "{hand:?}");
    }
    // A recovery that closes two reservations takes two turns.
    let events = [
        reset(30),
        admit(1, 1),
        admit(2, 1),
        Event::Crash,
        Event::Recover,
    ];
    let (books, _) = run_checked(&events).unwrap();
    assert_eq!(turns(&books, 1), 4);
    assert!(!books.balance().needs_owner);
}

#[test]
fn two_processes_over_one_store_are_held_by_the_same_names() {
    // Each is told yes for the last of the allowance: one is admitted.
    let events = [reset(10), admit(1, 10), second(late(1, admit(2, 10)))];
    let (books, outcomes) = run_checked(&events).unwrap();
    let refused = Outcome::Refused(Why::OverAllowance { left: 0 });
    assert_eq!(outcomes[2], Outcome::AskedAgain(Box::new(refused)));
    assert!(books.reservation(Run(2)).is_none());
    // A settlement under its bound by one, and an admission by the other that
    // judged before it: asked again, and admitted on what is left now.
    let events = [
        reset(10),
        admit(1, 5),
        dispatch(1, 5),
        settle(1, 1, 1),
        second(late(3, admit(2, 5))),
    ];
    let (_, outcomes) = run_checked(&events).unwrap();
    assert_eq!(outcomes[4], Outcome::AskedAgain(Box::new(Outcome::Done)));
    // Each runs its own call; a crash of one loses the other's nothing, and the
    // recovery of one closes what the other left and not what itself runs.
    let events = [
        reset(30),
        admit(1, 10),
        second(admit(2, 10)),
        dispatch(1, 4),
        second(dispatch(2, 6)),
        // The first process cannot settle or end what the second sent.
        Event::Settle {
            root: Run(2),
            place: 1,
            how: How::NotSent,
        },
        Event::End { run: Run(2) },
        Event::Crash,
        second(Event::Recover),
        second(settle(2, 1, 5)),
        second(Event::End { run: Run(2) }),
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(outcomes[5], Outcome::Refused(Why::NoSuchPlace));
    assert_eq!(outcomes[6], Outcome::Refused(Why::NotExecuting));
    assert_eq!(outcomes[9..], [Outcome::Done, Outcome::Done]);
    let (first, other) = (
        books.reservation(Run(1)).unwrap(),
        books.reservation(Run(2)).unwrap(),
    );
    assert_eq!((first.state, first.counts), (State::Interrupted, 10));
    assert_eq!((other.state, other.counts), (State::Released, 5));
    assert_eq!(books.balance().left, 15);
}

#[test]
fn known_what_the_names_cannot_say_is_not_counted() {
    let run = Run(1);
    let prefix = [reset(20), admit(1, 10), dispatch(1, 5)];
    // The provider charged 12 over a bound of 5, and the process died before the
    // settlement: no name says 12. The reservation is counted in full, at 10.
    let mut events = prefix.to_vec();
    events.extend([
        Event::Charge {
            root: run,
            place: 1,
            micros: 12,
        },
        Event::Crash,
        Event::Recover,
    ]);
    let (books, _) = run_checked(&events).unwrap();
    let one = books.reservation(run).unwrap();
    assert_eq!((one.state, one.counts), (State::Interrupted, 10));
    // Had the process lived, the answer it holds is what is settled, whatever the
    // caller says of the request.
    events.truncate(4);
    events.push(Event::Settle {
        root: run,
        place: 1,
        how: How::NotSent,
    });
    let (books, _) = run_checked(&events).unwrap();
    assert_eq!(books.reservation(run).unwrap().counts, 12);
    // A whole chain that other hands made is a chain: the settlement and its guard
    // of a request that is really outstanding, at a charge the stage allows, are
    // believed, and recovery then releases, as the plan has it for whole, readable
    // records.
    let chain = [
        Event::Plant(Name::Settling { run, place: 1 }),
        Event::Plant(Name::Settled {
            run,
            place: 1,
            stage: Stage::NotSent,
            micros: 0,
        }),
    ];
    let mut events = prefix.to_vec();
    events.extend(chain.iter().cloned());
    events.extend([Event::Crash, Event::Recover]);
    let (books, _) = run_checked(&events).unwrap();
    let one = books.reservation(run).unwrap();
    assert_eq!((one.state, one.counts), (State::Released, 0));
    // The chain took no turn: the store needs its owner all the same.
    assert!(books.balance().needs_owner);
    // The guard alone lowers nothing, and neither does the settlement alone.
    for alone in &chain {
        let mut one = prefix.to_vec();
        one.extend([alone.clone(), Event::Crash, Event::Recover]);
        let (books, _) = run_checked(&one).unwrap();
        let one = books.reservation(run).unwrap();
        assert_eq!((one.state, one.counts), (State::Frozen, 25));
    }
}

/// A call from inside `parent`, admitted as `child` with this share.
fn child(parent: u32, child: u32, micros: u64) -> Event {
    Event::Child {
        parent: Run(parent),
        child: Run(child),
        micros,
    }
}

#[test]
fn a_store_that_needs_its_owner_starts_no_paid_work_and_still_settles_ends_recovers_and_is_corrected()
 {
    // Two runs of one process, each with a request outstanding, and a third whose
    // process died with one outstanding.
    let before = [
        reset(40),
        admit(1, 10),
        dispatch(1, 4),
        admit(2, 10),
        dispatch(2, 4),
        second(admit(3, 10)),
        second(dispatch(3, 4)),
        second(Event::Crash),
    ];
    // Each way a store comes to need its owner: a turn lost, a turn made, and a
    // name for a run that has no reservation, which no hold states an amount for.
    // The first two are of the epoch alone; the third is a frozen reservation whose
    // amount is unknown, and is of every epoch.
    let hands = [
        (Event::Remove(Name::Turn { epoch: 1, turn: 1 }), false),
        (Event::Plant(Name::Turn { epoch: 1, turn: 50 }), false),
        (
            Event::Plant(Name::Stray {
                run: Run(9),
                count: 1,
            }),
            true,
        ),
    ];
    for (hand, frozen) in hands {
        let mut events = before.to_vec();
        events.push(hand.clone());
        events.extend([
            admit(4, 0),
            dispatch(1, 0),
            dispatch(2, 0),
            settle(1, 1, 3),
            settle(2, 1, 12),
            Event::End { run: Run(1) },
            Event::Recover,
            Event::Adjust {
                run: Run(3),
                micros: 4,
            },
            reset(50),
            admit(4, 0),
            dispatch(2, 0),
        ]);
        let (books, outcomes) =
            run_checked(&events).unwrap_or_else(|why| panic!("{hand:?}: {why}"));
        // No admission, and no dispatch under any reservation, a request of
        // nothing among them.
        assert_eq!(outcomes[9], Outcome::Refused(Why::NeedsOwner), "{hand:?}");
        for at in [10, 11, 19] {
            let refused = matches!(
                outcomes[at],
                Outcome::Refused(Why::NeedsOwner | Why::Frozen)
            );
            assert!(refused, "{hand:?}: {outcomes:?}");
        }
        // What was dispatched is settled, over its bound too; a run ends; recovery
        // closes what a dead process left; the owner corrects; the owner resets.
        let done = outcomes[12..18].iter().all(|one| *one == Outcome::Done);
        assert!(done, "{hand:?}: {outcomes:?}");
        let third = books.reservation(Run(3)).unwrap();
        assert_eq!(
            (third.state, third.counts),
            (State::Adjusted, 4),
            "{hand:?}"
        );
        let second = books.reservation(Run(2)).unwrap();
        assert_eq!(second.charged, 12, "{hand:?}");
        // A reset opens an epoch that needs nobody where it was the turns, and
        // admissions start again; a frozen reservation whose amount is unknown
        // stops them still. A run of the epoch that needs its owner sends nothing
        // more either way.
        let admitted = match frozen {
            true => Outcome::Refused(Why::NeedsOwner),
            false => Outcome::Done,
        };
        assert_eq!(outcomes[18], admitted, "{hand:?}");
    }
    // Where nothing needs the owner, each of them is taken.
    let mut events = before.to_vec();
    events.extend([admit(4, 0), dispatch(1, 0), dispatch(2, 0)]);
    let (_, outcomes) = run_checked(&events).unwrap();
    assert_eq!(
        outcomes[8..],
        [Outcome::Done, Outcome::Placed(2), Outcome::Placed(2)]
    );
    // A frozen reservation whose one hold states its amount is no such case: a
    // name of no form made under it, and a record of it that cannot be read. It is
    // counted at the most its names can mean, nothing is dispatched under it, and
    // the store goes on: a call is admitted, and another run sends its request.
    let frozen = [
        (
            Event::Plant(Name::Stray {
                run: Run(1),
                count: 1,
            }),
            1,
        ),
        (Event::Damage(Name::Reservation(Run(2))), 2),
    ];
    for (hand, run) in frozen {
        let mut events = before.to_vec();
        events.extend([hand.clone(), admit(4, 1), dispatch(1, 0), dispatch(2, 0)]);
        let (books, outcomes) =
            run_checked(&events).unwrap_or_else(|why| panic!("{hand:?}: {why}"));
        assert_eq!(outcomes[9], Outcome::Done, "{hand:?}");
        let sent = |by: u32| match by == run {
            true => Outcome::Refused(Why::Frozen),
            false => Outcome::Placed(2),
        };
        assert_eq!(outcomes[10..], [sent(1), sent(2)], "{hand:?}");
        let one = books.reservation(Run(run)).unwrap();
        // The amount, the bound of 4, and nothing for a place: none is unexplained.
        assert_eq!((one.state, one.counts), (State::Frozen, 14), "{hand:?}");
        let balance = books.balance();
        assert!(!balance.needs_owner, "{hand:?}");
        assert_eq!(balance.counted, 14 + 10 + 10 + 1, "{hand:?}");
    }
}

#[test]
fn a_dispatch_is_held_to_what_is_left_of_its_tree_in_the_memory_of_its_process() {
    // Other hands take away a whole settlement over its bound: itself, its guard,
    // the place it took and the turn it took. What is left is a set the ledger
    // could have written, in an epoch that needs nobody.
    let stripped = |run: u32, charge: u64, turn: u64| {
        let run = Run(run);
        [
            Name::Settled {
                run,
                place: 1,
                stage: Stage::Answered,
                micros: charge,
            },
            Name::Settling { run, place: 1 },
            Name::Place { run, place: 2 },
            Name::Turn { epoch: 1, turn },
        ]
        .map(Event::Remove)
    };
    // A reservation of 10, 8 of it a child's share. The parent's request of 2 is
    // settled at 7: 3 are left of the tree, and the names, stripped, say 8.
    let mut events = vec![
        reset(30),
        admit(1, 10),
        child(1, 2, 8),
        dispatch(1, 2),
        settle(1, 1, 7),
    ];
    events.extend(stripped(1, 7, 2));
    events.extend([
        dispatch(2, 8),
        dispatch(2, 4),
        dispatch(2, 3),
        dispatch(2, 1),
    ]);
    let (books, outcomes) = run_checked(&events).unwrap();
    let one = books.reservation(Run(1)).unwrap();
    assert_eq!((one.state, one.reachable), (State::Open, true));
    assert!(!books.balance().needs_owner);
    assert_eq!(
        outcomes[9..],
        [
            Outcome::Refused(Why::OverTree { left: 3 }),
            Outcome::Refused(Why::OverTree { left: 3 }),
            Outcome::Placed(2),
            Outcome::Refused(Why::OverTree { left: 0 }),
        ]
    );
    assert_eq!(books.spendable(Run(2)), Some(5));
    // The same, judged late on the names of before the overrun, with nothing lost:
    // memory is as it is now, and the store is not even asked.
    let events = [
        reset(30),
        admit(1, 10),
        child(1, 2, 8),
        dispatch(1, 2),
        settle(1, 1, 3),
        late(4, dispatch(2, 8)),
        late(4, dispatch(2, 7)),
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    let asked_again = |outcome| Outcome::AskedAgain(Box::new(outcome));
    assert_eq!(
        outcomes[5],
        asked_again(Outcome::Refused(Why::OverReservation { left: 7 }))
    );
    // Seven fit; the place it chose is the one the overrun took, so it is asked
    // again and placed after it.
    assert_eq!(outcomes[6], asked_again(Outcome::Placed(3)));
    assert_eq!(books.reservation(Run(1)).unwrap().counts, 10);
    // An overrun anywhere in the tree is the tree's: a run two calls down overran,
    // and a run beside it is held to what is left of the whole.
    let mut events = vec![
        reset(60),
        admit(1, 20),
        child(1, 2, 10),
        child(2, 3, 5),
        child(1, 4, 6),
        dispatch(3, 5),
        settle(1, 1, 22),
    ];
    events.extend(stripped(1, 22, 2));
    events.extend([dispatch(4, 1), dispatch(1, 1), dispatch(4, 0)]);
    let (_, outcomes) = run_checked(&events).unwrap();
    assert_eq!(
        outcomes[11..],
        [
            Outcome::Refused(Why::OverTree { left: 0 }),
            Outcome::Refused(Why::OverRun { left: 0 }),
            Outcome::Placed(2),
        ]
    );
    // What a process holds of a tree is lost with the process: nothing of the tree
    // executes after a crash, recovery closes the reservation, and a second process
    // never held it.
    let events = [
        reset(30),
        admit(1, 10),
        child(1, 2, 5),
        second(dispatch(2, 1)),
        Event::Crash,
        dispatch(2, 1),
        Event::Recover,
        dispatch(2, 1),
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    let gone = Outcome::Refused(Why::NotExecuting);
    assert_eq!(
        [&outcomes[3], &outcomes[5], &outcomes[7]],
        [&gone, &gone, &gone]
    );
    assert_eq!(books.reservation(Run(1)).unwrap().state, State::Released);
    // Each tree is held apart: one that is exhausted leaves another its own.
    let events = [
        reset(30),
        admit(1, 5),
        admit(2, 5),
        dispatch(1, 5),
        dispatch(1, 1),
        dispatch(2, 5),
        settle(1, 1, 1),
        dispatch(1, 4),
    ];
    let (_, outcomes) = run_checked(&events).unwrap();
    assert_eq!(
        outcomes[4..],
        [
            Outcome::Refused(Why::OverRun { left: 0 }),
            Outcome::Placed(1),
            Outcome::Done,
            Outcome::Placed(2),
        ]
    );
}

/// The names of one reservation of 10, made by hand: its record, its hold, a place
/// for each number up to `places`, and these.
fn written(places: u64, more: &[Name]) -> Vec<Name> {
    let run = Run(1);
    let mut names = vec![
        Name::Reservation(run),
        Name::Held {
            epoch: 0,
            run,
            micros: 10,
        },
    ];
    names.extend((1..=places.min(16)).map(|place| Name::Place { run, place }));
    names.extend(more.iter().cloned());
    names
}

/// Whether these names of one reservation are a set the ledger could have written,
/// as the fold's recogniser reads them and as the checker's second reading does.
fn read_twice(names: &[Name]) -> (bool, bool) {
    let made: Vec<Event> = names.iter().cloned().map(Event::Plant).collect();
    let (books, _) = Books::after(&made);
    books.read_twice(Run(1)).unwrap()
}

#[test]
fn the_recogniser_replays_what_the_ledger_checked_and_the_second_reading_does_too() {
    let run = Run(1);
    let sent = |place, micros| Name::Dispatched {
        run,
        place,
        by: run,
        micros,
    };
    // A request that is settled: its guard and its settlement, answered.
    let ended = |place, micros| {
        [
            Name::Settling { run, place },
            Name::Settled {
                run,
                place,
                stage: Stage::Answered,
                micros,
            },
        ]
    };
    let held = Name::Closed {
        run,
        released: false,
    };
    let correction = |place| Name::Adjusted {
        run,
        place,
        turn: 1,
        seal: 0,
        micros: 0,
    };
    let both = |takes: bool, places: u64, more: Vec<Name>, what: &str| {
        let names = written(places, &more);
        assert_eq!(read_twice(&names), (takes, takes), "{what}");
        let state = folded(&names, &[]).0;
        assert_eq!(state == State::Frozen, !takes, "{what}: {state:?}");
    };
    // A dispatch of all that is reserved, and one of more.
    both(true, 1, vec![sent(1, 10)], "all of it");
    both(false, 1, vec![sent(1, 11)], "one more than all of it");
    // Of nothing under a reservation of nothing, and of 1: the set the review gave.
    let nothing = |bound| {
        vec![
            Name::Reservation(run),
            Name::Held {
                epoch: 0,
                run,
                micros: 0,
            },
            Name::Place { run, place: 1 },
            sent(1, bound),
        ]
    };
    assert_eq!(read_twice(&nothing(0)), (true, true));
    assert_eq!(read_twice(&nothing(1)), (false, false));
    // A second dispatch fits what the first left.
    both(true, 2, vec![sent(1, 6), sent(2, 4)], "6 and 4");
    both(false, 2, vec![sent(1, 6), sent(2, 5)], "6 and 5");
    // The first settled under its bound. The names do not say when: it is taken to
    // have been settled before the second was sent, which leaves the most.
    let under = |second| [&[sent(1, 6), sent(2, second)][..], &ended(1, 2)[..]].concat();
    both(true, 2, under(8), "settled at 2, then 8");
    both(false, 2, under(9), "settled at 2, then 9");
    // A charge over its bound took the place after it: what was left then is less
    // by the whole charge.
    let over = |third| [&[sent(1, 5), sent(3, third)][..], &ended(1, 7)[..]].concat();
    both(true, 3, over(3), "5 settled at 7, then 3");
    both(false, 3, over(4), "5 settled at 7, then 4");
    // The same charge settled after the second request was sent took the place
    // after that one: 5 were left when it was sent.
    let after = |second| [&[sent(1, 5), sent(2, second)][..], &ended(1, 7)[..]].concat();
    both(true, 3, after(5), "5, then 5, then the first settled at 7");
    both(false, 3, after(6), "5, then 6");
    // Over the whole reservation: a request of nothing still fits, and no other.
    let all = |third| [&[sent(1, 5), sent(3, third)][..], &ended(1, 12)[..]].concat();
    both(true, 3, all(0), "5 settled at 12, then nothing");
    both(false, 3, all(1), "5 settled at 12, then 1");
    // Two charges over their bounds, by 5 and by 1, and one place below the third
    // request: the names do not say which of them took it. The smaller is taken,
    // which leaves 7, though with the larger 3 were left.
    let two = |fourth| {
        let mut names = vec![sent(1, 1), sent(2, 1), sent(4, fourth)];
        names.extend(ended(1, 6));
        names.extend(ended(2, 2));
        names
    };
    both(true, 5, two(7), "over by 5 and by 1, then 7");
    both(false, 5, two(8), "over by 5 and by 1, then 8");
    // A place below a dispatch that nothing over its bound accounts for is a
    // closing's: nothing is dispatched after one.
    both(true, 2, vec![sent(1, 1), held.clone()], "closed after");
    both(false, 2, vec![sent(2, 1), held.clone()], "closed before");
    // A charge over its bound of a request sent after the place accounts for
    // nothing below it.
    let later = [&[sent(2, 1), held.clone()][..], &ended(2, 5)[..]].concat();
    both(false, 3, later, "a closing, then a request over its bound");
    // A place a correction names is the correction's, and nothing is dispatched
    // after one, whatever charges over their bounds there are to stand in for it.
    let corrected = |at| {
        let mut names = vec![sent(1, 1), sent(2, 1), sent(4, 0), held.clone()];
        names.extend(ended(1, 2));
        names.extend(ended(2, 2));
        names.push(correction(at));
        names
    };
    both(true, 7, corrected(7), "a correction after every dispatch");
    both(false, 7, corrected(3), "a correction below a dispatch");
    // A correction follows the closing: its place is not the one after the last
    // dispatch, which then no closing took.
    both(
        false,
        7,
        corrected(5),
        "a correction where the closing must be",
    );
    both(
        true,
        7,
        corrected(6),
        "a correction after a place the closing can have",
    );
    let alone = |at| vec![held.clone(), correction(at)];
    both(false, 2, alone(1), "a correction at the first place");
    both(true, 2, alone(2), "a closing and its correction");
}

#[test]
fn every_sum_of_the_fold_stops_at_the_largest_number_and_is_over_any_allowance() {
    let most = u64::MAX - 1;
    let run = Run(1);
    let answered = |place, micros| Name::Settled {
        run,
        place,
        stage: Stage::Answered,
        micros,
    };
    // A hold of the largest amount and a place at the largest number: each number
    // up to it counts the amount, and the sum is still a number.
    let base = [
        Name::Reset {
            epoch: 1,
            micros: most,
        },
        Name::Epoch(1),
        Name::Reservation(run),
        Name::Held {
            epoch: 1,
            run,
            micros: most,
        },
        Name::Place {
            run,
            place: u64::MAX,
        },
    ];
    let with = |charges: u64| {
        let mut names: BTreeSet<Name> = base.iter().cloned().collect();
        names.extend((1..=charges.min(8)).map(|place| answered(place, most)));
        fold(&names, &BTreeSet::new())
    };
    let places = u128::from(u64::MAX) * u128::from(most);
    let amount = u128::from(most);
    for (charges, counts) in [
        (0, Some(amount + places)),
        // One charge more is still under the largest number; the sum is exact.
        (1, Some(amount + places + amount)),
        (2, Some(amount + places + 2 * amount)),
        // A third is over it: the sum stops there.
        (3, None),
        (8, None),
    ] {
        let folded = with(charges);
        let one = &folded.reservations[&run];
        assert_eq!(one.state, State::Frozen, "{charges}");
        assert_eq!(one.counts, counts.unwrap_or(u128::MAX), "{charges}");
        assert_eq!(one.most(), one.counts, "{charges}");
        assert_eq!(one.counted(), Counted::Exactly(one.counts), "{charges}");
        assert_eq!(one.left(), 0, "{charges}");
        // Over any allowance, the largest among them: nothing is left, the debt is
        // counted, and what a run sent is at most the same.
        let balance = folded.balance();
        assert_eq!(balance.allowed, most, "{charges}");
        assert_eq!((balance.counted, balance.left), (one.counts, 0));
        assert_eq!(balance.debt, one.counts - amount, "{charges}");
        assert_eq!(balance.held, one.counts - one.charged, "{charges}");
        let sent = folded.sent_by(run, run);
        assert_eq!(
            sent,
            Sent::Unknown {
                at_most: one.counts
            }
        );
    }
    // What is known of a reservation is its charges and what is outstanding: each
    // is a sum of amounts, and so is what a run sent.
    let mut names: BTreeSet<Name> = base.iter().cloned().collect();
    for place in 1..=6 {
        names.insert(Name::Dispatched {
            run,
            place,
            by: run,
            micros: most,
        });
        names.insert(answered(place, most));
        names.insert(answered(place + 6, most));
    }
    let folded = fold(&names, &BTreeSet::new());
    let one = &folded.reservations[&run];
    assert_eq!(one.charged, 12 * amount);
    assert_eq!(one.outstanding, 6 * amount);
    assert_eq!(one.known(), 18 * amount);
    assert_eq!(folded.runs[&run].known(), 12 * amount);
    assert_eq!(one.counts, u128::MAX);
    // Two such reservations: the store's sum stops too.
    let other: Vec<Name> = [
        Name::Reservation(Run(2)),
        Name::Held {
            epoch: 1,
            run: Run(2),
            micros: most,
        },
        Name::Place {
            run: Run(2),
            place: u64::MAX,
        },
    ]
    .into();
    names.extend(other);
    let balance = fold(&names, &BTreeSet::new()).balance();
    assert_eq!((balance.counted, balance.left), (u128::MAX, 0));
    assert_eq!(balance.debt, u128::MAX - amount);
    // The ledger at the largest amounts: a reservation of all of it, a request of
    // all of it settled at all of it, and a second reservation refused.
    let events = [
        reset(most),
        admit(1, most),
        child(1, 2, most),
        dispatch(2, most),
        admit(3, 1),
        settle(1, 1, most),
        dispatch(2, 1),
        Event::End { run: Run(2) },
        Event::End { run: Run(1) },
        admit(3, 1),
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(
        [&outcomes[4], &outcomes[6], &outcomes[9]],
        [
            &Outcome::Refused(Why::OverAllowance { left: 0 }),
            &Outcome::Refused(Why::OverRun { left: 0 }),
            &Outcome::Refused(Why::OverAllowance { left: 0 }),
        ]
    );
    let one = books.reservation(Run(1)).unwrap();
    assert_eq!((one.state, one.counts), (State::Released, amount));
}

#[test]
fn known_one_hold_made_in_the_place_of_the_ledgers_states_the_amount_of_a_frozen_reservation_too() {
    // A reservation of 10 with 10 outstanding. Other hands make a hold of nothing
    // for it and take away the ledger's and the dispatch. One hold is there, as
    // there would be had the ledger reserved nothing: no name tells it from the
    // ledger's, so the books say a number where "Limits", (c), has the amount
    // unknown. The reservation is frozen all the same, and nothing is dispatched
    // under it. Its hold states an amount, so the store goes on beside it: the
    // whole allowance of 30 is admitted, where 10 of it are outstanding and no
    // name says so (history 86).
    let run = Run(1);
    let hold = |micros| Name::Held {
        epoch: 1,
        run,
        micros,
    };
    let events = [
        reset(30),
        admit(1, 10),
        dispatch(1, 10),
        Event::Plant(hold(0)),
        Event::Remove(hold(10)),
        Event::Remove(Name::Dispatched {
            run,
            place: 1,
            by: run,
            micros: 10,
        }),
        admit(2, 30),
        dispatch(1, 0),
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    let one = books.reservation(run).unwrap();
    assert_eq!(
        (one.state, one.counted()),
        (State::Frozen, Counted::Exactly(0))
    );
    assert_eq!(books.run(run), Sent::Unknown { at_most: 0 });
    assert_eq!(books.really(run), Some((1, 10, 10)));
    assert!(!books.balance().needs_owner && !books.balance().unknown);
    assert_eq!(
        outcomes[6..],
        [Outcome::Done, Outcome::Refused(Why::Frozen)]
    );
    // 30 counted of 30, and 40 really committed: under by the 10 that were lost.
    let balance = books.balance();
    assert_eq!(
        (balance.allowed, balance.counted, balance.debt),
        (30, 30, 0)
    );
    assert_eq!(books.really(Run(2)), Some((1, 30, 0)));
    let table = named(86);
    assert_eq!(table.balance().counted, 30);
    // With a second hold beside it the books say that they cannot tell.
    let mut events = events.to_vec();
    events.push(Event::Plant(Name::Held {
        epoch: 2,
        run,
        micros: 0,
    }));
    let (books, _) = run_checked(&events).unwrap();
    let one = books.reservation(run).unwrap();
    assert_eq!(one.counted(), Counted::AtLeast(0));
    assert_eq!(books.run(run), Sent::Unbounded);
    assert!(books.balance().unknown);
    // And the store then needs its owner: no amount can be counted for it.
    assert!(books.balance().needs_owner);
}

/// What the histories of a suite reach of a dispatch, counted.
#[derive(Debug, Default)]
struct Reached {
    /// The dispatches that were published, each held by the checker to what was
    /// really left of its reservation; and of them, those of a run another run
    /// called.
    published: u64,
    by_a_called_run: u64,
    /// Those of a run under a run whose request had been settled over its bound;
    /// and of them, those of more than nothing.
    after_an_overrun: u64,
    after_an_overrun_of_something: u64,
    /// Dispatches asked after such an overrun, published or not: judged late with
    /// other hands between the judgement and the publication; and judged now,
    /// after other hands took away two names or more of the tree's reservation.
    late_across_other_hands: u64,
    after_losses: u64,
    /// Those refused for what is left of the tree in memory, and for an epoch that
    /// needs its owner.
    over_tree: u64,
    needs_owner: u64,
    /// The admissions that stand, each held by the checker to the truth and to the
    /// reading made after its publication, and held here to the truth once more;
    /// and of them, those judged late with other hands between the judgement and
    /// the publication, and those confirmed apart from their publication.
    admitted: u64,
    admitted_late_across_other_hands: u64,
    confirmed_apart: u64,
    /// The admissions that were published and not confirmed.
    not_confirmed: u64,
    /// The admissions published with a turn that other hands had taken away: those
    /// that stand, and those that do not.
    filled_and_stands: u64,
    filled_and_not_confirmed: u64,
    /// The dispatches published under a reservation whose admission was published
    /// with such a turn.
    dispatched_after_filling: u64,
    /// The dispatches published where the store needed its owner, each judged
    /// before it came to: the one publication limit (e) leaves a dispatch.
    judged_before_the_owner_was_needed: u64,
    /// The admissions published while another waited to be confirmed.
    waiting_together: u64,
}

/// Runs a history through the checker, as `run_checked` does, holds the two
/// readings of every reservation to one answer after every event, and counts what
/// the history reaches of a dispatch. What it returns is the books, or what broke.
fn reached(events: &[Event], into: &mut Reached) -> Result<Books, String> {
    let mut books = Books::new();
    let mut parents: BTreeMap<Run, Run> = BTreeMap::new();
    let mut overran: BTreeSet<Run> = BTreeSet::new();
    let mut hands: Vec<usize> = Vec::new();
    let mut lost: BTreeMap<Run, u64> = BTreeMap::new();
    let mut turns_lost: BTreeSet<Name> = BTreeSet::new();
    let mut filled: BTreeSet<Run> = BTreeSet::new();
    for (at, event) in events.iter().enumerate() {
        let (mut plain, mut judged) = (event, None);
        // A second process's, a late one, or both: two wrappings at most.
        for _ in 0..2 {
            match plain {
                Event::Second(inner) => plain = inner,
                Event::Stale {
                    judged: when,
                    event: inner,
                } => (judged, plain) = (Some(*when), inner),
                _ => {}
            }
        }
        let gone = |name: &&Name| !books.names().contains(*name);
        let gone: Vec<Name> = turns_lost.iter().filter(gone).cloned().collect();
        // Whether a dispatch falls under an epoch that needs its owner: on the names
        // it was judged on, and on the names as they are.
        let (stopped_then, stopped_now) = match plain {
            Event::Dispatch { by, .. } => (
                books.dispatch_needs_owner(*by, judged),
                books.dispatch_needs_owner(*by, None),
            ),
            _ => (false, false),
        };
        let outcome = books.checked(event)?;
        // The fold's recogniser and the checker's second reading are two writings
        // of one rule: they give one answer of every reservation there is.
        if let Some(run) = books.read_apart().first() {
            let twice = books.read_twice(*run);
            return Err(format!("event {at}: {run} is read as {twice:?}"));
        }
        let last = match &outcome {
            Outcome::AskedAgain(inner) => &**inner,
            other => other,
        };
        let done = !matches!(last, Outcome::Refused(_));
        match plain {
            Event::Child { parent, child, .. } if done => {
                parents.insert(*child, *parent);
            }
            Event::Admit { run, .. } | Event::Publish { run, .. } | Event::Confirm { run } => {
                let waiting = books.waiting_in(false).len() + books.waiting_in(true).len();
                let waits = matches!(plain, Event::Publish { .. }) && done;
                into.waiting_together += u64::from(waits && waiting >= 2);
                let since = |judged: usize| hands.iter().any(|when| (judged..at).contains(when));
                // Published as it was judged, with a turn other hands had taken away.
                let fills = !matches!(plain, Event::Confirm { .. })
                    && !matches!(outcome, Outcome::AskedAgain(_))
                    && books.really(*run).is_some()
                    && gone.iter().any(|name| books.names().contains(name));
                if fills {
                    filled.insert(*run);
                }
                let stands = done && !matches!(plain, Event::Publish { .. });
                if stands {
                    // The truth had room for it, counted in full.
                    let amount = books.really(*run).map_or(0, |(_, amount, _)| amount);
                    if let Some((counted, allowed)) = books.really_counted_with(*run)
                        && amount > 0
                        && counted > allowed
                    {
                        return Err(format!(
                            "event {at}: {run} stands, and really {counted} is counted of {allowed}"
                        ));
                    }
                    into.admitted += 1;
                    into.admitted_late_across_other_hands += u64::from(judged.is_some_and(since));
                    into.confirmed_apart += u64::from(matches!(plain, Event::Confirm { .. }));
                    into.filled_and_stands += u64::from(filled.contains(run));
                }
                if *last == Outcome::Refused(Why::NotConfirmed) {
                    into.not_confirmed += 1;
                    into.filled_and_not_confirmed += u64::from(filled.contains(run));
                }
            }
            Event::Plant(_) | Event::Damage(_) => hands.push(at),
            Event::Remove(name) => {
                hands.push(at);
                if matches!(name, Name::Turn { .. }) && done {
                    turns_lost.insert(name.clone());
                }
                if let Some(run) = name.of().filter(|_| done) {
                    *lost.entry(run).or_default() += 1;
                }
            }
            Event::Settle { root, place, .. } if done => {
                // The bound and the charge, as the names have them now.
                let names = books.names();
                let sent = names.iter().find_map(|name| match name {
                    Name::Dispatched {
                        run,
                        place: at,
                        by,
                        micros,
                    } if run == root && at == place => Some((*by, *micros)),
                    _ => None,
                });
                let charge = names.iter().find_map(|name| match name {
                    Name::Settled {
                        run,
                        place: at,
                        micros,
                        ..
                    } if run == root && at == place => Some(*micros),
                    _ => None,
                });
                if let (Some((by, bound)), Some(charge)) = (sent, charge)
                    && charge > bound
                {
                    overran.insert(by);
                }
            }
            Event::Dispatch { by, micros } => {
                // The runs above it: no tree is deeper than a history is long.
                let (mut above, mut under) = (parents.get(by), false);
                for _ in 0..GENERATED_EVENTS {
                    let Some(run) = above else { break };
                    under |= overran.contains(run);
                    above = parents.get(run);
                }
                // No dispatch that was judged where the store needed its owner is
                // published: one judged again is judged on the names as they are.
                let again = matches!(outcome, Outcome::AskedAgain(_));
                let judged_stopped = if again { stopped_now } else { stopped_then };
                if done && judged_stopped {
                    return Err(format!(
                        "event {at}: dispatched, and judged where the store needed its owner"
                    ));
                }
                into.judged_before_the_owner_was_needed += u64::from(done && stopped_now);
                let root = books.root_of(*by);
                into.dispatched_after_filling +=
                    u64::from(done && root.is_some_and(|root| filled.contains(&root)));
                if done {
                    into.published += 1;
                    into.by_a_called_run += u64::from(parents.contains_key(by));
                    into.after_an_overrun += u64::from(under);
                    into.after_an_overrun_of_something += u64::from(under && *micros > 0);
                }
                let since = |judged: usize| hands.iter().any(|when| (judged..at).contains(when));
                let losses = books.root_of(*by).and_then(|root| lost.get(&root));
                let now = judged.is_none() && losses.is_some_and(|losses| *losses >= 2);
                into.late_across_other_hands += u64::from(under && judged.is_some_and(since));
                into.after_losses += u64::from(under && now);
                into.over_tree += u64::from(matches!(last, Outcome::Refused(Why::OverTree { .. })));
                into.needs_owner += u64::from(*last == Outcome::Refused(Why::NeedsOwner));
            }
            _ => {}
        }
    }
    Ok(books)
}

#[test]
fn every_generated_history_of_amounts_near_the_largest_keeps_every_invariant() {
    let (mut all, mut saturated, mut frozen, mut far, mut large) = (Reached::default(), 0, 0, 0, 0);
    for number in 0..LARGE_HISTORIES {
        let events = generated_large(SEED, number);
        assert!((1..=GENERATED_EVENTS).contains(&events.len()), "{number}");
        let books = match reached(&events, &mut all) {
            Ok(books) => books,
            Err(why) => panic!("seed {SEED:#x}, large history {number}: {why}\n{events:#?}"),
        };
        let folded = books.fold();
        for one in folded.reservations.values() {
            frozen += u64::from(one.state == State::Frozen);
            saturated += u64::from(one.counts == u128::MAX);
            large += u64::from(one.counts > u128::from(u64::MAX));
        }
        let at = |name: &Name| name.place().is_some_and(|place| place > u64::MAX - 4);
        far += u64::from(books.names().iter().any(at));
        // Whatever the amounts, the store's balance is of numbers that are there.
        let balance = folded.balance();
        assert!(balance.debt <= balance.counted, "{number}");
        assert!(
            u128::from(balance.left) <= u128::from(balance.allowed),
            "{number}"
        );
    }
    // Amounts by the scale, places at the last numbers, sums over what 64 bits hold
    // and sums that stop at the largest number: each is met.
    assert_eq!(LARGE, (u64::MAX - 1) / 6);
    assert!(all.published > 150, "{all:?}");
    assert!(all.after_an_overrun > 5, "{all:?}");
    assert!(frozen > 200, "{frozen}");
    assert!(far > 50, "{far}");
    assert!(large > 100, "{large}");
    assert!(saturated > 10, "{saturated}");
    assert_eq!(generated_large(SEED, 7), generated_large(SEED, 7));
    assert_eq!(generated_large(SEED, 8), generated_large(SEED, 8));
}

#[test]
fn known_an_admission_judged_before_a_settlement_that_other_hands_took_away_whole_is_published() {
    // Found by the hunt over other seeds that followed the fourth review round. An
    // admission of 9 is judged with 10 left. A request is then
    // settled at 12 over its bound of 5, which takes the turn the admission chose.
    // Other hands take that settlement away whole: itself, its guard, the place it
    // took and its turn. The turn is free again, so the store takes the admission;
    // the names are a set the ledger could have written, in an epoch that needs
    // nobody, and one more is admitted after it.
    let run = Run(1);
    let whole = [
        Name::Settled {
            run,
            place: 1,
            stage: Stage::Answered,
            micros: 12,
        },
        Name::Settling { run, place: 1 },
        Name::Place { run, place: 2 },
        Name::Turn { epoch: 1, turn: 2 },
    ];
    let mut events = vec![reset(20), admit(1, 10), dispatch(1, 5), settle(1, 1, 12)];
    events.extend(whole.map(Event::Remove));
    let before = events.clone();
    events.extend([late(3, admit(2, 9)), admit(3, 1)]);
    let (books, outcomes) = Books::after(&events);
    assert_eq!(outcomes[8..], [Outcome::Done, Outcome::Done]);
    let balance = books.balance();
    assert_eq!((balance.counted, balance.left), (20, 0));
    assert!(!balance.needs_owner);
    // 12 were really charged: 22 against an allowance of 20. That is the charge
    // that was lost ("Limits", (b) and (d)), and an admission judged after the
    // loss is let by: the same books, and the same names made.
    assert_eq!(books.really(run), Some((1, 10, 12)));
    let mut now = before.clone();
    now.extend([admit(2, 9), admit(3, 1)]);
    let (judged_now, _) = run_checked(&now).unwrap();
    assert_eq!(judged_now.names(), books.names());
    // So is the late one: it is no worse than one judged on the names as other
    // hands left them ("Limits", (e)), and the checker holds it as it holds that
    // one, to what is really left.
    let (judged_late, _) = run_checked(&events).unwrap();
    assert_eq!(judged_late.names(), books.names());
    // With one more admitted than is left afterwards, nothing more can be.
    let mut all = before;
    all.push(late(3, admit(2, 10)));
    let (books, outcomes) = run_checked(&all).unwrap();
    assert_eq!(outcomes[8], Outcome::Done);
    assert_eq!(books.balance().left, 0);
}

#[test]
fn known_a_late_admission_whose_turn_fills_the_place_of_one_that_was_lost_is_published() {
    // Found by the hunt once limit (e) was restated. An admission is judged on an
    // empty epoch and chooses its first turn. A call is admitted, which takes that
    // turn, and its request is settled, which takes the second. Other hands take
    // away the settlement and its guard, and the first turn: the epoch has one
    // turn, the second, for one hold, and needs its owner. An admission judged now
    // is refused. The late one is published, and its turn is the one that was
    // missing: the epoch's turns are 1 and 2 for two holds, and it needs nobody
    // any more.
    let run = Run(1);
    let lost = [
        Name::Settled {
            run,
            place: 1,
            stage: Stage::Answered,
            micros: 6,
        },
        Name::Settling { run, place: 1 },
        Name::Turn { epoch: 1, turn: 1 },
    ];
    let before = |allowance: u64| {
        let mut events = vec![
            reset(allowance),
            admit(1, 6),
            dispatch(1, 6),
            settle(1, 1, 6),
        ];
        events.extend(lost.clone().map(Event::Remove));
        events
    };
    let mut now = before(20);
    now.push(admit(2, 2));
    let (books, outcomes) = run_checked(&now).unwrap();
    assert_eq!(outcomes[7], Outcome::Refused(Why::NeedsOwner));
    assert!(books.balance().needs_owner);
    // It is published, and that is all limit (e) gives it. The process that made it
    // reads the names again: they are sound, with 8 counted of 20, and it stands.
    // The checker holds it to the truth, in which 6 were taken of 20, and to that
    // reading, with no excuse.
    let mut events = before(20);
    events.extend([late(1, admit(2, 2)), dispatch(2, 2)]);
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(outcomes[7..], [Outcome::Done, Outcome::Placed(1)]);
    let balance = books.balance();
    assert_eq!((balance.counted, balance.left), (8, 12));
    assert!(!balance.needs_owner);
    assert!(books.confirmed(Run(2)));
    assert_eq!(books.really_counted_with(Run(2)), Some((8, 20)));
    // With an allowance of 7 the same reading is over it, 8 counted of 7: the
    // admission is not confirmed. Its reservation is closed, which releases the 2,
    // and nothing runs under it.
    let mut events = before(7);
    events.extend([late(1, admit(2, 2)), dispatch(2, 2), admit(3, 1)]);
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(
        outcomes[7..],
        [
            Outcome::Refused(Why::NotConfirmed),
            Outcome::Refused(Why::NotExecuting),
            Outcome::Done,
        ]
    );
    let one = books.reservation(Run(2)).unwrap();
    assert_eq!((one.state, one.counts), (State::Released, 0));
    assert!(!books.confirmed(Run(2)));
    let balance = books.balance();
    assert_eq!((balance.counted, balance.left), (7, 0));
}

#[test]
fn every_generated_history_of_turns_moved_around_a_late_admission_keeps_every_invariant() {
    let mut all = Reached::default();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for number in 0..TURNED {
        let events = generated_turned(SEED, number);
        assert!((1..=GENERATED_EVENTS).contains(&events.len()), "{number}");
        if let Err(why) = reached(&events, &mut all) {
            panic!("seed {SEED:#x}, turned history {number}: {why}\n{events:#?}");
        }
        // The turns that are moved: lost before the other is made and after it, in
        // the newest epoch of the time and in another.
        let (_, outcomes) = Books::after(&events);
        // The newest epoch, as the owner's resets left it.
        let mut newest = 0;
        let mut first = None;
        for (event, outcome) in events.iter().zip(&outcomes) {
            let done = !matches!(outcome, Outcome::Refused(_));
            let moved = match event {
                Event::Remove(Name::Turn { epoch, .. }) if done => Some(("lost", *epoch)),
                Event::Plant(Name::Turn { epoch, .. }) => Some(("made", *epoch)),
                _ => None,
            };
            if let Some((what, epoch)) = moved {
                let order = first.get_or_insert(what);
                let whose = if epoch == newest {
                    "the newest"
                } else {
                    "another"
                };
                seen.insert(format!("a turn {what} in {whose} epoch, {order} first"));
            }
            let plain = match event {
                Event::Second(inner) => inner,
                other => other,
            };
            newest += u64::from(matches!(plain, Event::Reset { .. }) && *outcome == Outcome::Done);
            let kind = match plain {
                Event::Stale { event, .. } => match **event {
                    Event::Admit { .. } => "late Admit",
                    Event::Publish { .. } => "late Publish",
                    _ => continue,
                },
                Event::Confirm { .. } => "Confirm",
                _ => continue,
            };
            let came = match outcome {
                Outcome::Done => "done".to_owned(),
                Outcome::AskedAgain(_) => "asked again".to_owned(),
                other => format!("{other:?}"),
            };
            seen.insert(format!("{kind}: {came}"));
        }
    }
    let wanted = [
        "a turn lost in the newest epoch, lost first",
        "a turn lost in the newest epoch, made first",
        "a turn made in the newest epoch, lost first",
        "a turn made in the newest epoch, made first",
        "a turn lost in another epoch, lost first",
        "a turn lost in another epoch, made first",
        "a turn made in another epoch, lost first",
        "a turn made in another epoch, made first",
        "late Admit: done",
        "late Admit: asked again",
        "late Admit: Refused(NotConfirmed)",
        "late Publish: done",
        "late Publish: asked again",
        "Confirm: done",
        "Confirm: Refused(NotConfirmed)",
        "Confirm: Refused(NotPublished)",
    ];
    let missing: Vec<&str> = wanted
        .into_iter()
        .filter(|one| !seen.contains(*one))
        .collect();
    assert!(missing.is_empty(), "never reached: {missing:?}\n{seen:#?}");
    // An admission is published with a turn that other hands took away, and the
    // reading made after it decides: some stand, on books that read sound and
    // within the allowance, and requests are then sent under them, each held to
    // what was really reserved; more do not, and nothing runs under those. Every
    // one that stands is held to the truth, late ones across what other hands did
    // and ones confirmed apart from their publication among them.
    assert!(all.admitted > 3_000, "{all:?}");
    assert!(all.admitted_late_across_other_hands > 200, "{all:?}");
    assert!(all.confirmed_apart > 50, "{all:?}");
    assert!(all.not_confirmed > 500, "{all:?}");
    assert!(all.filled_and_stands > 200, "{all:?}");
    assert!(all.filled_and_not_confirmed > 500, "{all:?}");
    assert!(all.dispatched_after_filling > 400, "{all:?}");
    assert!(all.published > 1_500, "{all:?}");
    assert_eq!(generated_turned(SEED, 7), generated_turned(SEED, 7));
}

fn publish(run: u32, micros: u64) -> Event {
    Event::Publish {
        run: Run(run),
        micros,
    }
}

fn confirm(run: u32) -> Event {
    Event::Confirm { run: Run(run) }
}

fn turn(epoch: u64, turn: u64) -> Name {
    Name::Turn { epoch, turn }
}

/// The names these books have that they had not after the first `events` events.
fn made_since(books: &Books, events: usize) -> Vec<String> {
    let before = books.names_after(events).unwrap();
    let made = books.names().difference(before);
    made.map(Name::to_string).collect()
}

#[test]
fn an_admission_stands_only_when_it_is_confirmed_on_a_reading_made_after_its_publication() {
    let not_confirmed = Outcome::Refused(Why::NotConfirmed);
    let not_executing = Outcome::Refused(Why::NotExecuting);
    let state = |books: &Books, run: u32| {
        let one = books.reservation(Run(run)).unwrap();
        (one.state, one.counts)
    };
    // The fifth review round's trace (history 76), with the turn lost before the
    // other is made and after it. A reservation of 10 is all the allowance, and all
    // of it is sent. An admission of 10 that judged before it is published once
    // other hands have moved a turn: the names are sound, and read 20 against 10.
    // It is not confirmed. What it published was the record, the hold and the turn;
    // what backing it out publishes is the closing, its place and its turn.
    let moved = [Event::Remove(turn(1, 1)), Event::Plant(turn(1, 2))];
    for order in [[0, 1], [1, 0]] {
        let mut trace = vec![reset(10), admit(1, 10), dispatch(1, 10)];
        trace.extend(order.map(|at| moved[at].clone()));
        trace.extend([late(1, admit(2, 10)), dispatch(2, 10)]);
        let (books, outcomes) = run_checked(&trace).unwrap();
        assert_eq!(
            outcomes[5..],
            [not_confirmed.clone(), not_executing.clone()]
        );
        assert_eq!(
            made_since(&books, 5),
            [
                "spend/turn/1/1",
                "spend/turn/1/3",
                "spend/reservations/7000-2",
                "spend/held/1/7000-2/10",
                "spend/place/7000-2/1",
                "spend/closed/7000-2/released",
            ]
        );
        assert_eq!(state(&books, 2), (State::Released, 0));
        assert!(!books.confirmed(Run(2)));
        let balance = books.balance();
        assert_eq!((balance.counted, balance.debt, balance.left), (10, 0, 0));
        // The truth: 10 were reserved and sent of 10, and the second counts nothing.
        assert_eq!(books.really_counted_with(Run(2)), Some((10, 10)));
    }

    // Each of the three things the reading must show, alone.
    // Its own reservation as the ledger wrote it, and open. Other hands close it
    // whole, with a place and a turn, between its publication and the reading:
    // the names are sound and within the allowance, and it does not stand. It
    // cannot be closed a second time: it stays as it is, counted in full.
    let closed = [
        Name::Closed {
            run: Run(1),
            released: false,
        },
        Name::Place {
            run: Run(1),
            place: 1,
        },
        turn(1, 2),
    ];
    let mut events = vec![reset(20), publish(1, 10)];
    events.extend(closed.map(Event::Plant));
    events.extend([confirm(1), dispatch(1, 1)]);
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(
        outcomes[5..],
        [not_confirmed.clone(), not_executing.clone()]
    );
    assert!(!books.balance().needs_owner);
    assert_eq!(state(&books, 1), (State::Interrupted, 10));
    assert!(made_since(&books, 5).is_empty());
    // An epoch that needs nobody. A turn of its epoch is lost: 22 counted of 30,
    // and the turns are two for three publications.
    let events = [
        reset(30),
        admit(1, 10),
        dispatch(1, 5),
        settle(1, 1, 12),
        Event::Remove(turn(1, 2)),
        late(3, admit(2, 10)),
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(outcomes[5], not_confirmed);
    let balance = books.balance();
    assert_eq!((balance.counted, balance.debt), (12, 0));
    assert!(balance.needs_owner);
    assert_eq!(state(&books, 2), (State::Released, 0));
    // And so for the newest epoch, where it fell under an older one. The owner
    // resets after the judgement, and other hands move the turn the reset took:
    // the admission is published under the epoch it judged, whose turns are then
    // two for a hold and a reset, with 10 counted of 10. It stands there, as one
    // made before the reset would. With a turn made far ahead in the newest
    // epoch, which then needs its owner, it does not.
    let under = [
        reset(10),
        reset(20),
        Event::Remove(turn(1, 1)),
        Event::Plant(turn(1, 2)),
    ];
    let mut events = under.to_vec();
    events.extend([late(1, admit(1, 10)), dispatch(1, 10)]);
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(outcomes[4..], [Outcome::Done, Outcome::Placed(1)]);
    assert_eq!(books.reservation(Run(1)).unwrap().epoch, Some(1));
    let balance = books.balance();
    assert_eq!((balance.epoch, balance.counted, balance.left), (2, 0, 20));
    let mut events = under.to_vec();
    events.extend([Event::Plant(turn(2, 5)), late(1, admit(1, 10))]);
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(outcomes[5], not_confirmed);
    assert!(!books.fold().needs_owner(1));
    assert!(books.fold().needs_owner(2));
    assert_eq!(state(&books, 1), (State::Released, 0));
    // And with the turn the reset took lost and none made, the epoch it fell under
    // needs its owner and the newest does not: it does not stand either.
    let mut events = under[..3].to_vec();
    events.push(late(1, admit(1, 10)));
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(outcomes[3], not_confirmed);
    assert!(books.fold().needs_owner(1));
    assert!(!books.fold().needs_owner(2));
    // Within the allowance, with its own reservation counted in full: the trace
    // above. A call of nothing adds nothing, and stands in a store that is in debt
    // already, as it is admitted there when it is judged.
    let events = [
        reset(10),
        admit(1, 10),
        dispatch(1, 5),
        settle(1, 1, 12),
        admit(2, 0),
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(outcomes[4], Outcome::Done);
    assert_eq!(books.balance().debt, 2);
    assert!(books.confirmed(Run(2)));

    // A reservation that is not what the ledger wrote is closed with nothing
    // released, by the one rule of a closing.
    let second_hold = Name::Held {
        epoch: 1,
        run: Run(1),
        micros: 3,
    };
    let events = [
        reset(20),
        publish(1, 10),
        Event::Plant(second_hold),
        confirm(1),
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(outcomes[3], not_confirmed);
    assert_eq!(
        made_since(&books, 3),
        [
            "spend/turn/1/2",
            "spend/place/7000-1/1",
            "spend/closed/7000-1/held",
        ]
    );
    assert_eq!(books.reservation(Run(1)).unwrap().state, State::Frozen);

    // Nothing is done under an admission before it is confirmed: no request, no
    // call from inside it, no end. It counts in full meanwhile. It is confirmed
    // once, by the process that published it.
    let events = [
        reset(20),
        publish(1, 10),
        dispatch(1, 1),
        child(1, 2, 1),
        Event::End { run: Run(1) },
        second(confirm(1)),
        confirm(1),
        confirm(1),
        dispatch(1, 1),
        child(1, 2, 1),
    ];
    let mut counted = Vec::new();
    let (books, outcomes) = whizbang_testkit::spend::run_watched(&events, |_, books, _| {
        counted.push(books.balance().counted);
        Ok(())
    })
    .unwrap();
    assert_eq!(
        outcomes[1..],
        [
            Outcome::Done,
            not_executing.clone(),
            not_executing.clone(),
            not_executing.clone(),
            Outcome::Refused(Why::NotPublished),
            Outcome::Done,
            Outcome::Refused(Why::NotPublished),
            Outcome::Placed(1),
            Outcome::Done,
        ]
    );
    assert_eq!(counted[1..6], [10; 5]);
    assert!(books.confirmed(Run(1)));
    // Published as it was judged, an admission is asked again like any other when
    // the store refuses it: two that judged the same books chose one turn.
    let events = [reset(10), publish(1, 10), late(1, publish(2, 10))];
    let (_, outcomes) = run_checked(&events).unwrap();
    let over = Outcome::Refused(Why::OverAllowance { left: 0 });
    assert_eq!(outcomes[2], Outcome::AskedAgain(Box::new(over)));

    // Other hands between the publication and the reading: the turn the admission
    // took is lost, and it is not confirmed. Its closing takes the turn again.
    let events = [
        reset(20),
        publish(1, 10),
        Event::Remove(turn(1, 1)),
        confirm(1),
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(outcomes[3], not_confirmed);
    assert_eq!(
        made_since(&books, 3),
        [
            "spend/turn/1/1",
            "spend/place/7000-1/1",
            "spend/closed/7000-1/released",
        ]
    );

    // Recovery passes over an admission that waits in the process that recovers,
    // and closes one that waits in another, as it closes any: the reading then
    // finds it closed, and it does not stand.
    let events = [reset(20), publish(1, 10), Event::Recover, confirm(1)];
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(outcomes[2..], [Outcome::Done, Outcome::Done]);
    assert_eq!(state(&books, 1), (State::Open, 10));
    let events = [
        reset(20),
        publish(1, 10),
        second(Event::Recover),
        confirm(1),
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(outcomes[2..], [Outcome::Done, not_confirmed.clone()]);
    assert_eq!(state(&books, 1), (State::Released, 0));

    // A crash between the publication and the reading leaves a reservation that
    // nothing ran under, counted in full. Recovery closes it as it closes any
    // reservation whose process is gone: nothing was dispatched under it, so all
    // of it is released (history 25). Here the names read 20 against 10 until then.
    let mut events = vec![reset(10), admit(1, 10), dispatch(1, 10)];
    events.extend(moved.clone());
    events.extend([late(1, publish(2, 10)), Event::Crash]);
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(outcomes[5], Outcome::Done);
    assert_eq!(state(&books, 2), (State::Open, 10));
    assert_eq!((books.balance().counted, books.balance().debt), (20, 10));
    assert!(books.executing_runs().is_empty());
    events.extend([confirm(2), Event::Recover, admit(3, 0)]);
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(
        outcomes[7..],
        [
            Outcome::Refused(Why::NotPublished),
            Outcome::Done,
            Outcome::Done
        ]
    );
    assert_eq!(state(&books, 1), (State::Interrupted, 10));
    assert_eq!(state(&books, 2), (State::Released, 0));
    assert_eq!((books.balance().counted, books.balance().debt), (10, 0));

    // Two admissions, each published before either is confirmed, in two processes:
    // each reading has the other's names in it. The first to read is backed out,
    // and the second then reads the first as released, and stands; in either
    // order, never both. (Two that read before either closing is published are
    // both backed out: the model reads one at a time.)
    for first in [1, 2] {
        let last = 3 - first;
        let by = |run: u32, event: Event| if run == 2 { second(event) } else { event };
        let mut events = vec![reset(10), publish(1, 10)];
        events.extend(moved.clone());
        events.extend([
            second(late(1, publish(2, 10))),
            by(first, confirm(first)),
            by(last, confirm(last)),
            by(first, dispatch(first, 10)),
            by(last, dispatch(last, 10)),
        ]);
        let (books, outcomes) = run_checked(&events).unwrap();
        assert_eq!(
            outcomes[4..],
            [
                Outcome::Done,
                not_confirmed.clone(),
                Outcome::Done,
                not_executing.clone(),
                Outcome::Placed(1),
            ]
        );
        assert_eq!(state(&books, first), (State::Released, 0));
        assert_eq!(state(&books, last), (State::Open, 10));
        assert_eq!((books.balance().counted, books.balance().left), (10, 0));
    }
}

#[test]
fn every_generated_history_of_two_admissions_that_wait_together_keeps_every_invariant() {
    let mut all = Reached::default();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for number in 0..PAIRED {
        let events = generated_paired(SEED, number);
        assert!((1..=GENERATED_EVENTS).contains(&events.len()), "{number}");
        if let Err(why) = reached(&events, &mut all) {
            panic!("seed {SEED:#x}, paired history {number}: {why}\n{events:#?}");
        }
        // What happens from the moment two wait together until both were asked.
        let (_, outcomes) = Books::after(&events);
        let (mut waiting, mut asked, mut stand) = (0, 0, 0);
        let mut between: Vec<&str> = Vec::new();
        for (event, outcome) in events.iter().zip(&outcomes) {
            let (second, plain) = match event {
                Event::Second(inner) => (true, &**inner),
                other => (false, other),
            };
            let plain = match plain {
                Event::Stale { event, .. } => &**event,
                other => other,
            };
            let last = match outcome {
                Outcome::AskedAgain(inner) => &**inner,
                other => other,
            };
            match plain {
                Event::Publish { .. } if *last == Outcome::Done => waiting += 1,
                _ if waiting < 2 || asked == 2 => {}
                Event::Crash if second => between.push("the second process dies"),
                Event::Crash => between.push("the first process dies"),
                Event::Recover if second => between.push("the second process recovers"),
                Event::Recover => between.push("the first process recovers"),
                Event::Plant(_) | Event::Remove(_) | Event::Damage(_) => {
                    between.push("other hands act");
                }
                Event::Confirm { .. } => {
                    asked += 1;
                    stand += u64::from(*last == Outcome::Done);
                    between.push(match last {
                        Outcome::Done => "one is confirmed",
                        Outcome::Refused(Why::NotConfirmed) => "one is not confirmed",
                        Outcome::Refused(Why::NotPublished) => "one is no longer waiting",
                        _ => "a confirmation ends otherwise",
                    });
                    if asked == 1 {
                        between.push(if second {
                            "the second process reads first"
                        } else {
                            "the first process reads first"
                        });
                    }
                }
                _ => {}
            }
        }
        if waiting >= 2 && asked == 2 {
            seen.extend(between);
            seen.insert(match stand {
                2 => "both stand",
                1 => "one of the two stands",
                _ => "neither stands",
            });
        }
    }
    let wanted = [
        "the first process dies",
        "the second process dies",
        "the first process recovers",
        "the second process recovers",
        "other hands act",
        "one is confirmed",
        "one is not confirmed",
        "one is no longer waiting",
        "the first process reads first",
        "the second process reads first",
        "both stand",
        "one of the two stands",
        "neither stands",
    ];
    let missing: Vec<&str> = wanted
        .into_iter()
        .filter(|one| !seen.contains(one))
        .collect();
    assert!(missing.is_empty(), "never reached: {missing:?}\n{seen:#?}");
    assert!(all.waiting_together > 200, "{all:?}");
    assert!(all.confirmed_apart > 300, "{all:?}");
    assert!(all.not_confirmed > 150, "{all:?}");
    assert!(all.admitted > 1_500, "{all:?}");
    assert_eq!(generated_paired(SEED, 7), generated_paired(SEED, 7));
}

/// The history of the table under this number, run through the checker: the books it
/// leaves, having given all it must.
fn named(number: u32) -> Books {
    let prefix = format!("{number:02} ");
    let table = histories();
    let history = table.iter().find(|one| one.name.starts_with(&prefix));
    let history = history.unwrap_or_else(|| panic!("no history {number}"));
    history.verify().unwrap_or_else(|why| panic!("{why}"))
}

#[test]
fn an_admission_is_confirmed_only_where_its_reservation_is_the_record_and_one_hold_and_nothing_else()
 {
    // History 80, (B). Between the publication and the reading other hands make a
    // place and a dispatch of 3 under the reservation. The names are a set the
    // ledger could have written, and the reservation reads as open, within its
    // allowance, in an epoch that needs nobody: but nothing runs under an admission
    // before it is confirmed, so no such names are the ledger's, and it does not
    // stand.
    let books = named(80);
    assert!(!books.confirmed(Run(1)));
    assert!(books.executing_runs().is_empty());
    assert!(!books.balance().needs_owner);
    assert_eq!(
        made_since(&books, 4),
        [
            "spend/turn/1/2",
            "spend/place/7000-1/2",
            "spend/closed/7000-1/held",
        ]
    );
    // Each other name that can name the run, made alone between the publication
    // and the reading, and none of them leaves an admission that stands. With no
    // name made it stands.
    let place = |place| Name::Place { run: Run(1), place };
    let sent = Name::Dispatched {
        run: Run(1),
        place: 1,
        by: Run(1),
        micros: 0,
    };
    let more = [
        vec![place(1)],
        vec![place(1), sent.clone()],
        vec![sent],
        vec![Name::Settling {
            run: Run(1),
            place: 1,
        }],
        vec![Name::Stray {
            run: Run(1),
            count: 1,
        }],
        vec![Name::Held {
            epoch: 1,
            run: Run(1),
            micros: 3,
        }],
        vec![Name::Closed {
            run: Run(1),
            released: true,
        }],
    ];
    for names in more {
        let mut events = vec![reset(20), publish(1, 10)];
        events.extend(names.iter().cloned().map(Event::Plant));
        events.push(confirm(1));
        let (books, outcomes) =
            run_checked(&events).unwrap_or_else(|why| panic!("{names:?}: {why}"));
        let last = outcomes.last().unwrap();
        assert_eq!(*last, Outcome::Refused(Why::NotConfirmed), "{names:?}");
        assert!(!books.confirmed(Run(1)), "{names:?}");
        // Backed out, it counts for no less than it reserved.
        assert!(books.reservation(Run(1)).unwrap().counts >= 10, "{names:?}");
    }
    let (books, outcomes) = run_checked(&[reset(20), publish(1, 10), confirm(1)]).unwrap();
    assert_eq!(outcomes[2], Outcome::Done);
    assert!(books.confirmed(Run(1)));
}

#[test]
fn one_record_that_cannot_be_read_freezes_its_reservation_at_its_worst_and_stops_nothing_else() {
    // History 81, (J): the plan's invariant 4. One dispatch's record cannot be
    // read. Its reservation of 10 counts 25: the amount, the bound of 5, and the
    // amount again for the place its closing took. A call of 5, which the rest of
    // the allowance of 30 covers, is admitted and sends its request: 30 counted of
    // 30, and nothing left for a call of 1.
    let books = named(81);
    let balance = books.balance();
    assert_eq!(
        (balance.allowed, balance.counted, balance.left, balance.debt),
        (30, 30, 0, 0)
    );
    assert!(!balance.needs_owner && !balance.unknown);
    assert!(books.confirmed(Run(2)));
    // Nothing is dispatched under the frozen reservation itself; another run sends,
    // settles and ends beside it; and recovery goes on.
    let damaged = Name::Dispatched {
        run: Run(1),
        place: 1,
        by: Run(1),
        micros: 5,
    };
    let mut events = vec![
        reset(30),
        admit(1, 10),
        dispatch(1, 5),
        second(admit(2, 5)),
        second(dispatch(2, 1)),
        Event::Damage(damaged),
        dispatch(1, 0),
        second(dispatch(2, 0)),
        second(settle(2, 1, 1)),
        second(settle(2, 2, 0)),
        second(Event::End { run: Run(2) }),
        Event::Crash,
        Event::Recover,
        admit(3, 0),
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(outcomes[6], Outcome::Refused(Why::Frozen));
    assert_eq!(outcomes[7], Outcome::Placed(2));
    assert!(outcomes[8..].iter().all(|one| *one == Outcome::Done));
    assert_eq!(books.reservation(Run(2)).unwrap().state, State::Released);
    let first = books.reservation(Run(1)).unwrap();
    assert_eq!((first.state, first.counts), (State::Frozen, 25));
    // It counts its worst in every epoch, whatever the owner resets to, until the
    // owner corrects it.
    events.extend([reset(26), admit(4, 2), admit(4, 1)]);
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(
        outcomes[15],
        Outcome::Refused(Why::OverAllowance { left: 1 })
    );
    assert_eq!(outcomes[16], Outcome::Done);
    assert_eq!(books.balance().counted, 26);
    events.extend([
        Event::Adjust {
            run: Run(1),
            micros: 5,
        },
        admit(5, 25),
    ]);
    let (books, outcomes) = run_checked(&events).unwrap();
    assert!(outcomes[17..].iter().all(|one| *one == Outcome::Done));
    assert_eq!(books.balance().counted, 26);
}

#[test]
fn a_correction_writes_off_a_request_in_flight_and_what_it_then_costs_is_past_the_cap() {
    // History 82, (A). The owner's correction to nothing of a reservation that
    // another process's recovery closed while its request of 10 was in flight; 10
    // more are admitted, sent and settled; the first request is then settled at 10.
    // 20 are counted against 10. At the second admission 10 were counted and 20
    // committed: the correction wrote the first 10 off.
    let books = named(82);
    let balance = books.balance();
    assert_eq!(
        (balance.allowed, balance.counted, balance.debt, balance.left),
        (10, 20, 10, 0)
    );
    assert_eq!(books.really(Run(1)), Some((1, 10, 10)));
    assert_eq!(books.really(Run(2)), Some((1, 10, 10)));
    // History 83, (A2), with one writer at a time. The process dies with the
    // provider's answer; the next recovers; the owner corrects to nothing; 10 more
    // are admitted and spent. No name states the first charge, the reservation
    // stays corrected at nothing, and the books count 10 where the two requests
    // really took 20.
    let books = named(83);
    let balance = books.balance();
    assert_eq!(
        (balance.allowed, balance.counted, balance.debt),
        (10, 10, 0)
    );
    assert_eq!(books.really(Run(1)), Some((1, 10, 10)));
    assert_eq!(books.really(Run(2)), Some((1, 10, 10)));
    let first = books.reservation(Run(1)).unwrap();
    assert_eq!(
        (first.state, first.counts, first.outstanding),
        (State::Adjusted, 0, 10)
    );
    // What is written off is bounded: no more than was outstanding under the
    // corrected reservation. With 4 of the 10 settled, a correction cannot go under
    // 4, and 6 are written off.
    let events = [
        reset(10),
        admit(1, 10),
        dispatch(1, 4),
        settle(1, 1, 4),
        dispatch(1, 6),
        second(Event::Recover),
        second(Event::Adjust {
            run: Run(1),
            micros: 3,
        }),
        second(Event::Adjust {
            run: Run(1),
            micros: 4,
        }),
        second(admit(2, 7)),
        second(admit(2, 6)),
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(
        outcomes[6],
        Outcome::Refused(Why::BelowSettled { settled: 4 })
    );
    assert_eq!(
        outcomes[8],
        Outcome::Refused(Why::OverAllowance { left: 6 })
    );
    assert_eq!(outcomes[9], Outcome::Done);
    assert_eq!(books.balance().counted, 10);
    // And it writes off what a place without a whole settlement says: a request
    // settled at 10 whose place is damaged afterwards is no longer under the least
    // a correction may say. Corrected to nothing, 20 more are admitted: 20 counted
    // of 20 where 30 were charged.
    let events = [
        reset(20),
        admit(1, 10),
        dispatch(1, 10),
        settle(1, 1, 10),
        Event::Damage(Name::Place {
            run: Run(1),
            place: 1,
        }),
        Event::End { run: Run(1) },
        Event::Adjust {
            run: Run(1),
            micros: 0,
        },
        admit(2, 20),
        dispatch(2, 20),
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    assert!(
        outcomes
            .iter()
            .all(|one| !matches!(one, Outcome::Refused(_)))
    );
    let balance = books.balance();
    assert_eq!(
        (balance.allowed, balance.counted, balance.debt),
        (20, 20, 0)
    );
    assert_eq!(books.really(Run(1)), Some((1, 10, 10)));
}

#[test]
fn a_reset_does_not_stop_the_open_reservations_of_the_epoch_it_closes() {
    // History 84, (G). The owner resets to nothing while three reservations of 10
    // are open. Each still sends all it reserved: 30 are dispatched after the
    // reset, under the old epoch, and the new epoch counts nothing.
    let books = named(84);
    let folded = books.fold();
    let (old, new) = (folded.balance_of(1), folded.balance_of(2));
    assert_eq!((old.allowed, old.counted, old.debt), (30, 30, 0));
    assert_eq!((new.allowed, new.counted, new.left), (0, 0, 0));
    assert!(!old.needs_owner && !new.needs_owner);
    assert_eq!(books.executing_runs(), [Run(1), Run(2), Run(3)]);
}

#[test]
fn a_request_of_nothing_is_placed_under_a_reservation_over_its_amount_and_in_a_store_in_debt() {
    // History 85, (H). With no allowance, calls of nothing are admitted. A request
    // of nothing is placed, whatever its reservation and the store already owe: it
    // asks for nothing. Whatever it is charged is over its bound, and is debt.
    let books = named(85);
    let balance = books.balance();
    assert_eq!(
        (balance.allowed, balance.counted, balance.debt),
        (0, 12, 12)
    );
    let first = books.reservation(Run(1)).unwrap();
    assert_eq!((first.amount, first.counts, first.left()), (0, 7, 0));
    assert_eq!(books.spendable(Run(1)), Some(0));
    assert_eq!(books.tree_left(Run(1)), Some(0));
}

#[test]
fn the_largest_number_is_no_amount_at_any_entry_and_nothing_is_published_with_it() {
    // The grammar has no name for an amount of the largest number there is (ADR
    // 0007): a reset, an admission, a dispatch, a provider's charge, a settlement
    // and a correction that state it are each refused, and make no name.
    let none = u64::MAX;
    let most = u64::MAX - 1;
    let before = [reset(most), admit(1, most), dispatch(1, 0)];
    let stated = [
        reset(none),
        admit(2, none),
        publish(2, none),
        dispatch(1, none),
        Event::Charge {
            root: Run(1),
            place: 1,
            micros: none,
        },
        settle(1, 1, none),
        Event::Adjust {
            run: Run(1),
            micros: none,
        },
        late(2, admit(2, none)),
        second(reset(none)),
        child(1, 2, none),
        Event::Take {
            run: Run(1),
            micros: none,
        },
        Event::GiveBack {
            run: Run(1),
            micros: none,
        },
    ];
    for event in stated {
        let mut events = before.to_vec();
        events.push(event.clone());
        let (books, outcomes) =
            run_checked(&events).unwrap_or_else(|why| panic!("{event:?}: {why}"));
        assert_eq!(outcomes[3], Outcome::Refused(Why::NoAmount), "{event:?}");
        assert!(made_since(&books, 3).is_empty(), "{event:?}");
    }
    // One less is an amount, at each.
    let events = [
        reset(most),
        admit(1, most),
        dispatch(1, most),
        settle(1, 1, most),
        Event::Crash,
        Event::Recover,
        reset(most),
    ];
    let (_, outcomes) = run_checked(&events).unwrap();
    assert!(
        outcomes
            .iter()
            .all(|one| !matches!(one, Outcome::Refused(_)))
    );
    // A settlement of an answer the process holds states the answer's charge, which
    // was refused when it was given: the request is settled as the caller says.
    let events = [
        reset(10),
        admit(1, 10),
        dispatch(1, 5),
        Event::Charge {
            root: Run(1),
            place: 1,
            micros: none,
        },
        settle(1, 1, 5),
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(outcomes[3], Outcome::Refused(Why::NoAmount));
    assert_eq!(outcomes[4], Outcome::Done);
    assert_eq!(books.reservation(Run(1)).unwrap().charged, 5);
}

#[test]
fn a_closed_reservation_has_nothing_left_for_a_dispatch_whatever_its_state_reads() {
    // What a dispatch may still take is asked of a reservation as it is folded:
    // nothing of one that any closing names, of whatever state.
    let open = [reset(30), admit(1, 10), dispatch(1, 3)];
    let (books, _) = run_checked(&open).unwrap();
    let standing = books.reservation(Run(1)).unwrap();
    assert_eq!(
        (standing.state, standing.closed, standing.left()),
        (State::Open, false, 7)
    );
    // The answer does not rest on the state alone: the same reservation with a
    // closing named has nothing left.
    let closed = Standing {
        closed: true,
        ..standing
    };
    assert_eq!(closed.left(), 0);
    // Every reservation of every named history, after every event: closed, or of
    // any state but open, it has nothing left.
    for history in histories() {
        let mut books = Books::new();
        for event in &history.events {
            books.apply(event);
            for (run, one) in &books.fold().reservations {
                let none = one.closed || one.state != State::Open;
                assert!(!none || one.left() == 0, "{}: {run}", history.name);
            }
        }
    }
    // Closed as released, as held, and corrected.
    for (events, state) in [
        (vec![Event::Crash, Event::Recover], State::Interrupted),
        (
            vec![settle(1, 1, 1), Event::End { run: Run(1) }],
            State::Released,
        ),
        (
            vec![
                Event::Crash,
                Event::Recover,
                Event::Adjust {
                    run: Run(1),
                    micros: 0,
                },
            ],
            State::Adjusted,
        ),
    ] {
        let mut all = open.to_vec();
        all.extend(events);
        let (books, _) = run_checked(&all).unwrap();
        let one = books.reservation(Run(1)).unwrap();
        assert_eq!((one.state, one.closed, one.left()), (state, true, 0));
    }
}

#[test]
fn the_limits_accepted_with_a_frozen_reservation_that_stops_nothing_else_each_count_exactly_this_much_less()
 {
    // A frozen reservation whose one hold states an amount is counted at the most
    // its names can mean, and the store goes on beside it. Where the names that
    // stated the money are the ones that are gone, the most they can mean is under
    // what was spent, and what then reads as left is admitted. Each is a stated
    // limit, and is under by exactly this.
    //
    // History 86, limit (a): one hold of nothing in the place of the ledger's, the
    // dispatch of 10 lost with it. 30 are counted of 30, 40 are committed: 10 under,
    // which is what was outstanding under the reservation whose hold was replaced.
    let books = named(86);
    let committed = |books: &Books, runs: &[u32]| -> u128 {
        let took = |run: &u32| {
            let (_, amount, took) = books.really(Run(*run)).unwrap();
            took.max(u128::from(amount))
        };
        runs.iter().map(took).sum()
    };
    let balance = books.balance();
    assert_eq!((balance.allowed, balance.counted), (30, 30));
    assert!(!balance.needs_owner && !balance.unknown);
    assert_eq!(committed(&books, &[1, 2]), 40);
    // History 87, limit (b): a settlement at 15 over a bound of nothing, its name
    // lost. 20 are counted of 20, 33 are committed: 13 under, which is the charge
    // less what the frozen reservation's other names can mean.
    let books = named(87);
    let balance = books.balance();
    assert_eq!((balance.allowed, balance.counted), (20, 20));
    assert!(!balance.needs_owner && !balance.unknown);
    assert_eq!(committed(&books, &[1, 2]), 33);
    // Where no amount can be counted at all the store still needs its owner, and
    // nothing is admitted: history 67 (the hold lost, others left in other epochs)
    // and history 70 (a charge over its bound lost with its guard and its place,
    // which leaves the epoch a turn more than its names account for).
    for number in [67, 70] {
        assert!(named(number).balance().needs_owner, "{number}");
    }
    // The exception for an epoch's turns is of that epoch alone. History 88, limits
    // (b) and (d): a settlement at 12 over a bound of 5 is lost whole, the turn it
    // took left behind, after a reset closed its epoch. The old epoch needs its
    // owner and shows 10 where 12 were spent; the new epoch needs nobody, and its
    // whole allowance of 20 is admitted.
    let books = named(88);
    let folded = books.fold();
    let old = folded.balance_of(1);
    assert!(old.needs_owner && !old.unknown);
    assert_eq!((old.allowed, old.counted, old.left), (20, 10, 0));
    let new = folded.balance_of(2);
    assert!(!new.needs_owner && !new.unknown);
    assert_eq!((new.allowed, new.counted, new.left), (20, 20, 0));
    let (_, _, took) = books.really(Run(1)).unwrap();
    assert_eq!(took, 12);
}
