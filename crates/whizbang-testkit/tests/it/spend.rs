//! The model of the ledger's books, and the table of histories (ADR 0016): every named
//! history gives the balances it must, every generated one keeps every invariant, and a
//! name is written one way and read back strictly.

use std::collections::{BTreeMap, BTreeSet};

use whizbang_testkit::spend::{
    Books, Counted, Event, Folded, GENERATED_EVENTS, How, LARGE, Name, Outcome, Run, Sent, Stage,
    Standing, State, WHOLE_EVENTS, Why, fold, generated, generated_large, generated_tampered,
    generated_whole, histories, run_checked, seal,
};

/// The seed of the generated histories. A history that breaks an invariant is printed
/// with it and with its number.
const SEED: u64 = 0x1B00_2026_1114;

/// How many histories are generated, of each of the two shapes.
const GENERATED: u64 = 4096;

/// How many books the ledger alone wrote are each changed by one name.
const WHOLE: u64 = 2048;

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
    // from 43 those of its review rounds.
    assert!(table.len() >= 64, "{}", table.len());
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
    assert_eq!(generated_tampered(SEED, 7), generated_tampered(SEED, 7));
}

/// What became of each one name changed in books the ledger wrote alone, counted.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Tally {
    states: u64,
    removals: u64,
    damages: u64,
    plants: u64,
    /// The reservation the name names is frozen, and so the store needs its owner.
    frozen: u64,
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
    // In no epoch can more be reserved than before, and no other reservation counts
    // for less; only a turn can change another at all, by ending its correction.
    ensure(
        after.balance().left <= before.balance().left,
        "more can be reserved",
    )?;
    for epoch in before.resets.keys() {
        let (was, is) = (before.balance_of(*epoch), after.balance_of(*epoch));
        ensure(is.left <= was.left, "more is left in an epoch")?;
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
    // A name of a reservation: it is frozen, whatever it was, and the store needs
    // its owner in every epoch. No single name made, lost or damaged leaves a set
    // the ledger could have written: limits (a) and (d) take more than one.
    let is = after.reservations.get(&run);
    let Some(is) = is.filter(|is| frozen(is) && !is.reachable) else {
        return Err(format!("{run} is not frozen: {is:#?}"));
    };
    ensure(
        after.balance().needs_owner,
        "the store has no need of its owner",
    )?;
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
    for epoch in before.resets.keys() {
        ensure(
            after.needs_owner(*epoch),
            "an epoch has no need of its owner",
        )?;
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
    // more than was left before is never admitted. An admission of nothing, which
    // is always covered, is refused where the newest epoch needs its owner.
    let over = changed.apply(&admit(before.balance().left + 1));
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
        assert_eq!(frozen.balance().left, 0, "{name}");
        assert!(frozen.balance().needs_owner, "{name}");
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
    // amount and the amount again for its closing's place, and nothing is admitted.
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
    assert_eq!(outcomes[6], Outcome::Refused(Why::NeedsOwner));
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
    assert!(balance.needs_owner);
    // A second reset changes nothing of that.
    events.extend([reset(40), admit(2, 1)]);
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(outcomes[8], Outcome::Refused(Why::NeedsOwner));
    assert_eq!((books.balance().counted, books.balance().left), (20, 0));
    // The owner's correction of that reservation does: it falls under the epoch of
    // its hold again, and the newest has its whole allowance.
    events.extend([Event::Adjust { run, micros: 4 }, admit(2, 1)]);
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(outcomes[9..], [Outcome::Done, Outcome::Done]);
    let one = books.reservation(run).unwrap();
    assert_eq!((one.state, one.counts), (State::Adjusted, 4));
    let balance = books.balance();
    assert_eq!((balance.counted, balance.left), (1, 39));
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
        // A reset starts them again where it was the turns; where the name made
        // froze a reservation, only a correction of that reservation does.
        let frozen = |one: &Standing| one.state == State::Frozen;
        let frozen = books.fold().reservations.values().any(frozen);
        let turn = matches!(hand, Event::Remove(_) | Event::Plant(Name::Turn { .. }));
        assert_eq!(frozen, !turn, "{hand:?}");
        let wanted = match frozen {
            true => Outcome::Refused(Why::NeedsOwner),
            false => Outcome::Done,
        };
        assert_eq!(outcomes[13], wanted, "{hand:?}");
        assert_eq!(books.balance().needs_owner, frozen, "{hand:?}");
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
    // Each way a store comes to need its owner: a turn lost, a turn made, a name of
    // no form, damage, and a name for a run that has no reservation. The first two
    // are of the epoch alone; the others freeze a reservation.
    let hands = [
        (Event::Remove(Name::Turn { epoch: 1, turn: 1 }), false),
        (Event::Plant(Name::Turn { epoch: 1, turn: 50 }), false),
        (
            Event::Plant(Name::Stray {
                run: Run(1),
                count: 1,
            }),
            true,
        ),
        (Event::Damage(Name::Reservation(Run(2))), true),
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
        // admissions start again; a frozen reservation stops them still. A run of
        // the epoch that needs its owner sends nothing more either way.
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
    // unknown. The reservation is frozen all the same, and nothing more is
    // admitted or dispatched.
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
        admit(2, 0),
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
    assert!(books.balance().needs_owner && !books.balance().unknown);
    assert_eq!(
        outcomes[6..],
        [
            Outcome::Refused(Why::NeedsOwner),
            Outcome::Refused(Why::Frozen),
        ]
    );
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
            Event::Plant(_) | Event::Damage(_) => hands.push(at),
            Event::Remove(name) => {
                hands.push(at);
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
    // Found by the hunt over other seeds that followed the fourth review round, and
    // not ruled on. An admission of 9 is judged with 10 left. A request is then
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
    // The late one is not: "Limits", (e), has a late admission stand only where
    // nothing more is then admitted, and the checker holds it to that.
    let broke = run_checked(&events).unwrap_err();
    assert!(broke.contains("admitted on a stale balance"), "{broke}");
    // With one more admitted than is left afterwards, nothing more can be, and
    // the checker lets the late one by.
    let mut all = before;
    all.push(late(3, admit(2, 10)));
    let (books, outcomes) = run_checked(&all).unwrap();
    assert_eq!(outcomes[8], Outcome::Done);
    assert_eq!(books.balance().left, 0);
}
