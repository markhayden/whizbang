//! The model of the ledger's books, and the table of histories (ADR 0016): every named
//! history gives the balances it must, every generated one keeps every invariant, and a
//! name is written one way and read back strictly.

use std::collections::BTreeSet;

use whizbang_testkit::spend::{
    Books, Event, GENERATED_EVENTS, How, Name, Outcome, Run, Sent, Stage, State, Why, fold,
    generated, generated_tampered, histories, run_checked, seal,
};

/// The seed of the generated histories. A history that breaks an invariant is printed
/// with it and with its number.
const SEED: u64 = 0x1B00_2026_1114;

/// How many histories are generated, of each of the two shapes.
const GENERATED: u64 = 4096;

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
    for number in 0..GENERATED {
        let events = generated(SEED, number);
        assert!((1..=GENERATED_EVENTS).contains(&events.len()), "{number}");
        if let Err(why) = run_checked(&events) {
            panic!("seed {SEED:#x}, history {number}: {why}\n{events:#?}");
        }
    }
    // The same seed and number give the same history.
    assert_eq!(generated(SEED, 7), generated(SEED, 7));
    assert_ne!(generated(SEED, 7), generated(SEED + 1, 7));
}

#[test]
fn every_generated_history_of_whole_books_then_other_hands_keeps_every_invariant() {
    let mut frozen = 0;
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
        match run_checked(&events) {
            Ok((books, _)) => {
                let folded = books.fold();
                let is_frozen =
                    |one: &&whizbang_testkit::spend::Standing| one.state == State::Frozen;
                frozen += folded.reservations.values().filter(is_frozen).count();
            }
            Err(why) => panic!("seed {SEED:#x}, tampered history {number}: {why}\n{events:#?}"),
        }
    }
    // Other hands freeze something in a good part of them.
    assert!(frozen > 1000, "{frozen}");
    assert_eq!(generated_tampered(SEED, 7), generated_tampered(SEED, 7));
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
        assert_eq!(frozen.reservations[&Run(1)].counts, 10, "{name}");
        assert_eq!(
            frozen.reservations[&Run(2)],
            folded.reservations[&Run(2)],
            "{name}"
        );
        assert_eq!(frozen.balance().left, 10, "{name}");
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
    assert_eq!(with(&[dispatched(1, 13)]), (0, 17, 17));
    // A settlement where nothing was dispatched: its charge.
    assert_eq!(with(&[answered(2, 20)]), (20, 4, 24));
    assert_eq!(with(&[place(2), guard(2), answered(2, 2)]), (2, 4, 10));
    // A settlement without its guard gives up nothing of the bound; a guard alone,
    // a second hold of less, a name of no form, a closing without its place.
    assert_eq!(with(&[answered(1, 1)]), (1, 3, 10));
    for plant in [
        guard(1),
        held(2),
        Name::Stray { run, count: 1 },
        closed(true),
    ] {
        assert_eq!(with(std::slice::from_ref(&plant)), (0, 4, 10), "{plant}");
    }
    // A dispatch without its place: its bound.
    assert_eq!(with(&[dispatched(2, 3)]), (0, 7, 10));
    // A place that nothing is at: the amount once more.
    assert_eq!(with(&[place(2)]), (0, 4, 14));
    // A hold of more is the amount.
    assert_eq!(with(&[held(25)]), (0, 4, 25));
    // Both closings, each with a place.
    assert_eq!(
        with(&[closed(true), closed(false), place(2), place(3)]).2,
        14
    );
    // A second settlement of a place that is settled: every charge.
    let mut books = base();
    assert_eq!(books.apply(&settle(1, 1, 1)), Outcome::Done);
    assert_eq!(books.reservation(run).unwrap().state, State::Open);
    books.apply(&Event::Plant(answered(1, 7)));
    let one = books.reservation(run).unwrap();
    assert_eq!((one.state, one.charged, one.counts), (State::Frozen, 8, 10));
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
    assert_eq!(books.balance().left, 30 - 10 - 6);
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
            micros: 10,
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
            (State::Open, 0, u128::from(micros).max(10)),
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
        let rest = 30u128.saturating_sub(u128::from(micros));
        assert_eq!(found.0, State::Frozen, "{stage:?} {micros}");
        assert_eq!((found.1, found.2 >= 30), (rest, true), "{stage:?} {micros}");
    }
    // The chain without the dispatch's place; with a name of it damaged; with a
    // second dispatch; with a second settlement.
    let whole = with(&[settled(Stage::Answered, 2)]);
    let no_place: Vec<Name> = whole
        .iter()
        .filter(|name| !matches!(name, Name::Place { .. }))
        .cloned()
        .collect();
    assert_eq!(folded(&no_place, &[]), (State::Frozen, 28, 30));
    for hurt in &whole[2..] {
        let found = folded(&whole, std::slice::from_ref(hurt));
        assert_eq!(found, (State::Frozen, 28, 30), "{hurt}");
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
    assert_eq!(folded(&twice, &[]), (State::Frozen, 35, 37));
    let two = with(&[settled(Stage::Answered, 2), settled(Stage::Answered, 3)]);
    assert_eq!(folded(&two, &[]), (State::Frozen, 25, 30));
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
    assert_eq!(folded(&two_over, &[]), (State::Frozen, 0, 41 + 10));
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
    assert_eq!(folded(&released, &[]), (State::Frozen, 0, 10));
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
    assert_eq!(folded(&no_place, &[]), (State::Frozen, 0, 10));
    let other_seal = [
        &held[..],
        &[correction(3, 5, over + 1, 3), place(3), turn.clone()],
    ]
    .concat();
    assert_eq!(folded(&other_seal, &[]), (State::Interrupted, 0, 10));
    for hurt in [&stands[6], &stands[1]] {
        let found = folded(&stands, std::slice::from_ref(hurt));
        assert_eq!(found, (State::Frozen, 0, 10), "{hurt}");
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
    assert_eq!(folded(&at_dispatch, &[]), (State::Frozen, 0, 10));
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
    assert_eq!(folded(&at_guard, &[]), (State::Frozen, 0, 10));
    let over_open = sealed(&open);
    let not_closed = [
        &open[..],
        &[correction(2, 5, over_open, 3), place(2), turn.clone()],
    ]
    .concat();
    assert_eq!(folded(&not_closed, &[]), (State::Frozen, 0, 10));
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
    assert_eq!(folded(&stale, &[]), (State::Frozen, 0, 10));
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
        (State::Frozen, 4, 10)
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
                    assert_eq!((*state, *counts), (State::Frozen, 3 + 4 + 10), "{name}");
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
    assert_eq!((one.state, one.counts, one.epoch), (State::Frozen, 7, None));
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
        (State::Frozen, 1, 3 + 10, 0)
    );
    assert_eq!(books.run(run), Sent::Unknown { at_most: 13 });
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
    assert_eq!(one.counts, 2 + 4 + 10);
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
        assert_eq!(outcomes[13], Outcome::Done, "{hand:?}");
        assert!(!books.balance().needs_owner, "{hand:?}");
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
        assert_eq!((one.state, one.counts), (State::Frozen, 10));
    }
}
