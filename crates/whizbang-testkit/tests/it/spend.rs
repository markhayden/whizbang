//! The model of the ledger's books, and the table of histories (ADR 0016): every named
//! history gives the balances it must, every generated one keeps every invariant, and a
//! name is written one way and read back strictly.

use std::collections::BTreeSet;

use whizbang_testkit::spend::{
    Books, Event, GENERATED_EVENTS, How, Name, Outcome, Run, Stage, State, Why, fold, generated,
    histories, run_checked,
};

/// The seed of the generated histories. A history that breaks an invariant is printed
/// with it and with its number.
const SEED: u64 = 0x1B00_2026_1114;

/// How many histories are generated.
const GENERATED: u64 = 4096;

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
    assert!(table.len() >= 55, "{}", table.len());
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
        for one in books.fold().reservations.values() {
            seen.insert(format!("{:?}", one.state));
            if one.epochs.len() > 1 {
                seen.insert("two holds".to_owned());
            }
            if one.unexplained > 0 {
                seen.insert("a place nothing accounts for".to_owned());
            }
        }
        let balance = books.balance();
        if balance.debt > 0 {
            seen.insert("in debt".to_owned());
        }
        if books.fold().epoch() > 1 {
            seen.insert("a second epoch".to_owned());
        }
    }
    let wanted = [
        "Reset: done",
        "Admit: done",
        "Admit: refused OverAllowance",
        "late Admit: asked again",
        "late Admit: done",
        "Child: done",
        "Dispatch: done",
        "Dispatch: refused OverRun",
        "late Dispatch: asked again",
        "late Dispatch: done",
        "Settle: done",
        "late Settle: asked again",
        "Settle: refused NotExecuting",
        "End: refused ChildrenExecuting",
        "Adjust: refused OverCount",
        "late Reset: asked again",
        "End: done",
        "late End: done",
        "Abandon: done",
        "Crash: done",
        "Recover: done",
        "late Recover: asked again",
        "Damage Settled: done",
        "Damage Reset: done",
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
        "Charge: done",
        "Charge: refused AlreadySettled",
        "End: refused RequestsOutstanding",
        "Dispatch: refused AlreadyClosed",
        "Dispatch: refused OverReservation",
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
        "two holds",
        "a place nothing accounts for",
        "DeletePrune: done",
        "Adjust: done",
        "Adjust: refused NotCorrectable",
        "Take: done",
        "GiveBack: done",
        "Open",
        "Released",
        "Interrupted",
        "Frozen",
        "Adjusted",
        "in debt",
        "a second epoch",
    ];
    // Four ways are too rare to be met by chance, and are met by name: a correction
    // under what whole settlements say (history 33), one that cannot be held (34),
    // one judged late (41), and a place whose guard is taken (below).
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
        (Name::Turn(41), "spend/turn/41"),
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
                over: 1,
                micros: 9,
            },
            "spend/adjusted/7000-3/1/9",
        ),
        (Name::Epoch(2), "spend/epoch/2"),
    ];
    for (name, text) in &forms {
        assert_eq!(name.to_string(), *text);
        assert_eq!(Name::parse(text).as_ref(), Some(name), "{text}");
        assert_eq!(
            name.of().is_some(),
            !matches!(name, Name::Reset { .. } | Name::Epoch(_) | Name::Turn(_))
        );
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
        "spend/turn/0".to_owned(),
        "spend/epoch/0".to_owned(),
        "spend/epoch/1/2".to_owned(),
        "spend/adjusted/7000-3/0/9".to_owned(),
        "spend/turn/1/2".to_owned(),
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
        "Spend/turn/1".to_owned(),
        "requests/turn/1".to_owned(),
        format!("spend/turn/{}", "1".repeat(200)),
    ];
    for text in &refused {
        assert_eq!(Name::parse(text), None, "{text}");
    }
}

#[test]
fn the_fold_reads_names_and_nothing_else_and_one_damaged_name_freezes_one_reservation() {
    // Two reservations, one of them with a request settled under its bound.
    let events = [
        Event::Reset { micros: 30 },
        Event::Admit {
            run: Run(1),
            micros: 10,
        },
        Event::Admit {
            run: Run(2),
            micros: 10,
        },
        Event::Dispatch {
            by: Run(1),
            micros: 5,
        },
        Event::Settle {
            root: Run(1),
            place: 1,
            how: How::Answered(Some(3)),
        },
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
    let turn = BTreeSet::from([Name::Turn(1)]);
    assert_eq!(fold(books.names(), &turn), folded);
}

#[test]
fn what_cannot_be_placed_is_counted_at_the_most_it_can_mean() {
    let run = Run(1);
    let base = || {
        let events = [
            Event::Reset { micros: 30 },
            Event::Admit { run, micros: 10 },
            Event::Dispatch { by: run, micros: 4 },
        ];
        Books::after(&events).0
    };
    let with = |plant: Name| {
        let mut books = base();
        books.apply(&Event::Plant(plant));
        books.reservation(run).unwrap()
    };
    // A second dispatch at a place: the larger bound.
    let one = with(Name::Dispatched {
        run,
        place: 1,
        by: run,
        micros: 13,
    });
    assert_eq!(
        (one.state, one.outstanding, one.counts),
        (State::Frozen, 13, 13)
    );
    // A settlement of a place nothing was dispatched at, and one without its guard.
    let one = with(Name::Settled {
        run,
        place: 2,
        stage: Stage::Answered,
        micros: 20,
    });
    assert_eq!(
        (one.state, one.charged, one.counts),
        (State::Frozen, 20, 24)
    );
    let one = with(Name::Settled {
        run,
        place: 1,
        stage: Stage::Answered,
        micros: 1,
    });
    assert_eq!((one.state, one.charged, one.counts), (State::Frozen, 1, 10));
    // A guard with no settlement, a second hold, a stray name, a closing that says it
    // released with a dispatch not settled, and a correction of what is not closed.
    for plant in [
        Name::Settling { run, place: 1 },
        Name::Held {
            epoch: 1,
            run,
            micros: 2,
        },
        Name::Stray { run, count: 1 },
        Name::Closed {
            run,
            released: true,
        },
        Name::Adjusted {
            run,
            over: 1,
            micros: 0,
        },
    ] {
        let one = with(plant.clone());
        assert_eq!((one.state, one.counts), (State::Frozen, 10), "{plant}");
    }
    // A dispatch at a place with no guard; a settlement and both guards with no
    // dispatch; a second settlement of a place that is settled.
    let one = with(Name::Dispatched {
        run,
        place: 2,
        by: run,
        micros: 3,
    });
    assert_eq!(
        (one.state, one.outstanding, one.counts),
        (State::Frozen, 7, 10)
    );
    let mut books = base();
    for plant in [
        Name::Place { run, place: 2 },
        Name::Settling { run, place: 2 },
        Name::Settled {
            run,
            place: 2,
            stage: Stage::Answered,
            micros: 2,
        },
    ] {
        books.apply(&Event::Plant(plant));
    }
    let one = books.reservation(run).unwrap();
    assert_eq!((one.state, one.charged, one.counts), (State::Frozen, 2, 10));
    let mut books = base();
    let settle = Event::Settle {
        root: run,
        place: 1,
        how: How::Answered(Some(1)),
    };
    assert_eq!(books.apply(&settle), Outcome::Done);
    assert_eq!(books.reservation(run).unwrap().state, State::Open);
    books.apply(&Event::Plant(Name::Settled {
        run,
        place: 1,
        stage: Stage::Answered,
        micros: 7,
    }));
    let one = books.reservation(run).unwrap();
    assert_eq!((one.state, one.charged, one.counts), (State::Frozen, 8, 10));
    // A hold of more is the amount; two closings are none that releases.
    let one = with(Name::Held {
        epoch: 1,
        run,
        micros: 25,
    });
    assert_eq!((one.state, one.amount, one.counts), (State::Frozen, 25, 25));
    let mut books = base();
    for released in [true, false] {
        books.apply(&Event::Plant(Name::Closed { run, released }));
    }
    assert_eq!(books.reservation(run).unwrap().state, State::Frozen);
    // Names that name a run with no hold are a reservation of nothing, frozen, in the
    // newest epoch: what they say was charged is counted.
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
fn a_refusal_changes_no_name() {
    let events = [
        Event::Reset { micros: 5 },
        Event::Admit {
            run: Run(1),
            micros: 5,
        },
        Event::Admit {
            run: Run(2),
            micros: 1,
        },
        Event::Dispatch {
            by: Run(1),
            micros: 6,
        },
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
        Event::Reset { micros: 30 },
        Event::Admit {
            run: Run(1),
            micros: 10,
        },
        Event::Dispatch {
            by: Run(1),
            micros: 4,
        },
        Event::Plant(Name::Settling {
            run: Run(1),
            place: 1,
        }),
        Event::Settle {
            root: Run(1),
            place: 1,
            how: How::Answered(Some(1)),
        },
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
    // its run's end closes it, and the closing says that nothing is released.
    let run = Run(1);
    let events = [
        Event::Reset { micros: 30 },
        Event::Admit { run, micros: 10 },
        Event::Damage(Name::Reservation(run)),
        Event::End { run },
    ];
    let (books, _) = run_checked(&events).unwrap();
    assert!(books.names().contains(&Name::Closed {
        run,
        released: false
    }));
    assert!(!books.names().contains(&Name::Closed {
        run,
        released: true
    }));
    // And so does recovery, of one whose process died.
    let events = [
        Event::Reset { micros: 30 },
        Event::Admit { run, micros: 10 },
        Event::Damage(Name::Reservation(run)),
        Event::Crash,
        Event::Recover,
    ];
    let (books, _) = run_checked(&events).unwrap();
    assert!(books.names().contains(&Name::Closed {
        run,
        released: false
    }));
    assert!(!books.names().contains(&Name::Closed {
        run,
        released: true
    }));

    // A reservation that other hands closed while its run executes takes no dispatch;
    // and a closing without the place it takes releases nothing.
    let events = [
        Event::Reset { micros: 30 },
        Event::Admit { run, micros: 10 },
        Event::Plant(Name::Closed {
            run,
            released: true,
        }),
        Event::Dispatch { by: run, micros: 4 },
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(outcomes[3], Outcome::Refused(Why::AlreadyClosed));
    let one = books.reservation(run).unwrap();
    assert_eq!((one.state, one.counts), (State::Frozen, 10));
}

#[test]
fn a_correction_never_counts_under_what_whole_settlements_say_whoever_wrote_it() {
    // Interrupted with 4 charged and 5 outstanding. The command refuses a correction
    // under 4; a whole correction that other hands made, the name over the ten names
    // the reservation has and the place it takes, counts 4 all the same.
    let run = Run(1);
    let mut events = vec![
        Event::Reset { micros: 30 },
        Event::Admit { run, micros: 10 },
        Event::Dispatch { by: run, micros: 5 },
        Event::Settle {
            root: run,
            place: 1,
            how: How::Answered(Some(4)),
        },
        Event::Dispatch { by: run, micros: 5 },
        Event::Crash,
        Event::Recover,
        Event::Adjust { run, micros: 3 },
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    assert_eq!(
        outcomes[7],
        Outcome::Refused(Why::BelowSettled { settled: 4 })
    );
    let own = |name: &&Name| name.of() == Some(run);
    assert_eq!(books.names().iter().filter(own).count(), 10);
    events.push(Event::Plant(Name::Adjusted {
        run,
        over: 10,
        micros: 1,
    }));
    // Without the place it takes it is no correction: frozen, in full.
    let (books, _) = run_checked(&events).unwrap();
    let one = books.reservation(run).unwrap();
    assert_eq!((one.state, one.counts), (State::Frozen, 10));
    events.push(Event::Plant(Name::Place { run, place: 4 }));
    let (books, _) = run_checked(&events).unwrap();
    let one = books.reservation(run).unwrap();
    assert_eq!(
        (one.state, one.settled, one.counts),
        (State::Adjusted, 4, 4)
    );
    assert_eq!(books.balance().left, 26);
}

/// Every name of these books, lost alone: what each reservation counted before, and
/// what it is and counts after. `run_checked` holds each loss to the rule that it
/// lowers nothing; this gives the numbers to the test.
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
        // No other reservation is changed, and no more can be reserved, but for the
        // hold, which is the one name the amount is in.
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
        Event::Reset { micros: 30 },
        Event::Admit { run, micros: 10 },
        Event::Dispatch { by: run, micros: 3 },
        Event::Dispatch { by: run, micros: 4 },
        Event::Settle {
            root: run,
            place: 1,
            how: How::Answered(Some(2)),
        },
    ];
    let open = each_loss(&events, run);
    assert_eq!(open.len(), 4 + 8, "{open:?}");
    // Then the second settled at 9 over its bound of 4, and the run ended: released
    // at 11.
    events.push(Event::Settle {
        root: run,
        place: 2,
        how: How::Answered(Some(9)),
    });
    events.push(Event::End { run });
    let released = each_loss(&events, run);
    assert_eq!(released.len(), 5 + 13, "{released:?}");
    // Then another, interrupted with a request outstanding, and corrected.
    let other = Run(2);
    events.extend([
        Event::Admit {
            run: other,
            micros: 10,
        },
        Event::Dispatch {
            by: other,
            micros: 6,
        },
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
        for (name, before, state, counts) in losses {
            assert_eq!(*before, was, "{name}");
            let own = name.of() == Some(if was == 1 { other } else { run });
            match name {
                // A name of another reservation, or of none, changes nothing here.
                _ if !own => assert_eq!(*counts, was, "{name}"),
                // The hold is the one name the amount is in: what is known is left.
                Name::Held { .. } => assert_eq!(*state, State::Frozen, "{name}"),
                // A charge over its bound was in its settlement's name alone: the
                // bound and the amount are left.
                Name::Settled { micros: 9, .. } => {
                    assert_eq!((*state, *counts), (State::Frozen, 2 + 4 + 10), "{name}");
                }
                _ => {
                    assert_eq!(*state, State::Frozen, "{name}");
                    assert!(*counts >= was && *counts >= 10, "{name}: {counts}");
                }
            }
        }
    }
    // The hold lost: frozen at what is known, counted in every epoch, and nothing
    // more is dispatched under it.
    let hold = Name::Held {
        epoch: 1,
        run,
        micros: 10,
    };
    let mut lost = events[..5].to_vec();
    lost.push(Event::Remove(hold));
    lost.push(Event::Dispatch { by: run, micros: 1 });
    let (books, outcomes) = run_checked(&lost).unwrap();
    let one = books.reservation(run).unwrap();
    assert_eq!((one.state, one.counts, one.epoch), (State::Frozen, 6, None));
    assert_eq!(
        outcomes[6],
        Outcome::Refused(Why::OverReservation { left: 0 })
    );
    // A dispatch's name lost: its place is one nothing accounts for, counted at the
    // amount, because its bound was in that name alone.
    let dispatch = Name::Dispatched {
        run,
        place: 2,
        by: run,
        micros: 4,
    };
    let mut lost = events[..5].to_vec();
    lost.push(Event::Remove(dispatch));
    let (books, _) = run_checked(&lost).unwrap();
    let one = books.reservation(run).unwrap();
    assert_eq!(
        (one.state, one.unexplained, one.counts, one.left()),
        (State::Frozen, 1, 2 + 10, 0)
    );
}

#[test]
fn an_allowance_is_its_resets_only_with_the_guard_and_with_neither_damaged() {
    let reset = Event::Reset { micros: 30 };
    let admit = Event::Admit {
        run: Run(1),
        micros: 1,
    };
    let names = [
        Name::Reset {
            epoch: 1,
            micros: 30,
        },
        Name::Epoch(1),
    ];
    for name in names {
        for hands in [Event::Damage(name.clone()), Event::Remove(name.clone())] {
            let events = [reset.clone(), hands, admit.clone()];
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
    let (books, _) = run_checked(&[reset.clone(), plant.clone()]).unwrap();
    assert_eq!((books.balance().epoch, books.balance().allowed), (2, 0));
    let guard = Event::Plant(Name::Epoch(2));
    let (books, _) = run_checked(&[reset, plant, guard]).unwrap();
    assert_eq!((books.balance().epoch, books.balance().allowed), (2, 500));
}

#[test]
fn two_processes_over_one_store_are_held_by_the_same_names() {
    let second = |event: Event| Event::Second(Box::new(event));
    let late = |judged: usize, event: Event| Event::Stale {
        judged,
        event: Box::new(event),
    };
    let admit = |run: u32, micros: u64| Event::Admit {
        run: Run(run),
        micros,
    };
    // Each is told yes for the last of the allowance: one is admitted.
    let events = [
        Event::Reset { micros: 10 },
        admit(1, 10),
        second(late(1, admit(2, 10))),
    ];
    let (books, outcomes) = run_checked(&events).unwrap();
    let refused = Outcome::Refused(Why::OverAllowance { left: 0 });
    assert_eq!(outcomes[2], Outcome::AskedAgain(Box::new(refused)));
    assert!(books.reservation(Run(2)).is_none());
    // Each runs its own call; a crash of one loses the other's nothing, and the
    // recovery of one closes what the other left and not what itself runs.
    let events = [
        Event::Reset { micros: 30 },
        admit(1, 10),
        second(admit(2, 10)),
        Event::Dispatch {
            by: Run(1),
            micros: 4,
        },
        second(Event::Dispatch {
            by: Run(2),
            micros: 6,
        }),
        // The first process cannot settle or end what the second sent.
        Event::Settle {
            root: Run(2),
            place: 1,
            how: How::NotSent,
        },
        Event::End { run: Run(2) },
        Event::Crash,
        second(Event::Recover),
        second(Event::Settle {
            root: Run(2),
            place: 1,
            how: How::Answered(Some(5)),
        }),
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
    let prefix = [
        Event::Reset { micros: 20 },
        Event::Admit { run, micros: 10 },
        Event::Dispatch { by: run, micros: 5 },
    ];
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
    let settle = Event::Settle {
        root: run,
        place: 1,
        how: How::NotSent,
    };
    events.truncate(4);
    events.push(settle);
    let (books, _) = run_checked(&events).unwrap();
    assert_eq!(books.reservation(run).unwrap().counts, 12);
    // A whole chain that other hands made is a chain: the settlement and its guard
    // of a request that is really outstanding are believed, and recovery then
    // releases, as the plan has it for whole, readable records.
    let mut events = prefix.to_vec();
    events.extend([
        Event::Plant(Name::Settling { run, place: 1 }),
        Event::Plant(Name::Settled {
            run,
            place: 1,
            stage: Stage::NotSent,
            micros: 0,
        }),
        Event::Crash,
        Event::Recover,
    ]);
    let (books, _) = run_checked(&events).unwrap();
    let one = books.reservation(run).unwrap();
    assert_eq!((one.state, one.counts), (State::Released, 0));
    // The guard alone lowers nothing, and neither does the settlement alone.
    for alone in [3, 4] {
        let mut one = prefix.to_vec();
        one.push(events[alone].clone());
        one.extend([Event::Crash, Event::Recover]);
        let (books, _) = run_checked(&one).unwrap();
        let one = books.reservation(run).unwrap();
        assert_eq!((one.state, one.counts), (State::Frozen, 10));
    }
}

#[test]
fn a_name_that_lowers_without_its_whole_chain_lowers_nothing() {
    // Books written by hand: a reservation of 10 with one dispatch of 30 at place 1,
    // so that what its bound holds shows over the amount.
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
    ];
    let settled = |micros: u64| Name::Settled {
        run,
        place: 1,
        stage: Stage::Answered,
        micros,
    };
    let guard = Name::Settling { run, place: 1 };
    let of = |names: &[Name], damaged: &[Name]| {
        let names: BTreeSet<Name> = names.iter().cloned().collect();
        let damaged: BTreeSet<Name> = damaged.iter().cloned().collect();
        let one = fold(&names, &damaged).reservations[&run].clone();
        (one.state, one.outstanding, one.counts)
    };
    let with = |more: &[Name]| [&base[..], more].concat();
    // The whole chain lowers the place to what the settlement says.
    let whole = with(&[guard.clone(), settled(2)]);
    assert_eq!(of(&whole, &[]), (State::Open, 0, 10));
    // Each of these keeps the bound: the dispatch's place lost; a name of the chain
    // damaged; a second dispatch; a second settlement, which also keeps what the two
    // do not add up to.
    let no_place: Vec<Name> = whole
        .iter()
        .filter(|name| !matches!(name, Name::Place { .. }))
        .cloned()
        .collect();
    assert_eq!(of(&no_place, &[]), (State::Frozen, 28, 30));
    for hurt in [&whole[2], &whole[3], &whole[4], &whole[5]] {
        let found = of(&whole, std::slice::from_ref(hurt));
        assert_eq!(found, (State::Frozen, 28, 30), "{hurt}");
    }
    let twice = with(&[
        guard.clone(),
        settled(2),
        Name::Dispatched {
            run,
            place: 1,
            by: Run(2),
            micros: 5,
        },
    ]);
    assert_eq!(of(&twice, &[]), (State::Frozen, 28, 30));
    let two = with(&[guard.clone(), settled(2), settled(3)]);
    assert_eq!(of(&two, &[]), (State::Frozen, 25, 30));

    // A closing as released with a place that is not settled, its bound nothing.
    let free = [
        base[0].clone(),
        base[1].clone(),
        base[2].clone(),
        Name::Dispatched {
            run,
            place: 1,
            by: run,
            micros: 0,
        },
    ];
    let closing = |released| Name::Closed { run, released };
    let place = |place| Name::Place { run, place };
    let released = [&free[..], &[closing(true), place(2)]].concat();
    assert_eq!(of(&released, &[]), (State::Frozen, 0, 10));
    // Both closings, each with a place.
    let both = [
        &free[..2],
        &[closing(true), closing(false), place(1), place(2)],
    ]
    .concat();
    assert_eq!(of(&both, &[]), (State::Frozen, 0, 10));
    // A correction with its place over another count of names than there are; and
    // two at one count, where the names are as many as each says.
    let closed = [&free[..2], &[closing(false), place(1)]].concat();
    let correction = |over, micros| Name::Adjusted { run, over, micros };
    let stands = [&closed[..], &[correction(4, 3), place(2)]].concat();
    assert_eq!(of(&stands, &[]), (State::Adjusted, 0, 3));
    let stale = [&closed[..], &[correction(3, 3), place(2)]].concat();
    assert_eq!(of(&stale, &[]), (State::Frozen, 0, 10));
    let pair = [
        &closed[..],
        &[correction(6, 0), correction(6, 9), place(2), place(3)],
    ]
    .concat();
    assert_eq!(of(&pair, &[]), (State::Frozen, 0, 10));
}
