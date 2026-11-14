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
    for history in &table {
        if let Err(why) = history.verify() {
            panic!("{why}\n{:#?}", history.events);
        }
    }
    // The plan's 38, in its order and under its numbers, and then this record's own.
    assert!(table.len() >= 38, "{}", table.len());
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
            let (late, event) = match event {
                Event::Stale { event, .. } => ("late ", &**event),
                event => ("", event),
            };
            let kind = format!("{event:?}");
            let kind = kind.split([' ', '(', '{']).next().unwrap_or_default();
            let came = match outcome {
                Outcome::Done | Outcome::Placed(_) => "done".to_owned(),
                Outcome::Refused(why) => {
                    let why = format!("{why:?}");
                    format!("refused {}", why.split(' ').next().unwrap_or_default())
                }
                Outcome::AskedAgain(_) => "asked again".to_owned(),
            };
            seen.insert(format!("{late}{kind}: {came}"));
        }
        for one in books.fold().reservations.values() {
            seen.insert(format!("{:?}", one.state));
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
        "Damage: done",
        "Plant: done",
        "DeletePrune: done",
        "Adjust: done",
        "Adjust: refused NotCorrectable",
        "Adjust: refused BelowSettled",
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
    // Four ways are too rare to be met by chance, and are met by name: a dispatch
    // over what its reservation has left (history 39), a correction that cannot be
    // held (34), one judged late (41), and a place whose guard is taken (below).
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
                count: 1,
                micros: 9,
            },
            "spend/adjusted/7000-3/1/9",
        ),
    ];
    for (name, text) in &forms {
        assert_eq!(name.to_string(), *text);
        assert_eq!(Name::parse(text).as_ref(), Some(name), "{text}");
        assert_eq!(
            name.of().is_some(),
            !matches!(name, Name::Reset { .. } | Name::Turn(_))
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
            count: 1,
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
        (State::Frozen, 1, 6)
    );
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

    // A reservation that other hands closed while its run executes takes no dispatch:
    // what it released stays released.
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
    assert_eq!((one.state, one.counts), (State::Released, 0));
}

#[test]
fn a_correction_never_counts_under_what_was_charged_whoever_wrote_it() {
    // Interrupted with 4 charged and 5 outstanding. The command refuses a correction
    // under 4; a name of that form that other hands made counts 4 all the same.
    let run = Run(1);
    let events = [
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
        Event::Plant(Name::Adjusted {
            run,
            count: 1,
            micros: 1,
        }),
    ];
    let (books, _) = run_checked(&events).unwrap();
    let one = books.reservation(run).unwrap();
    assert_eq!(
        (one.state, one.charged, one.counts),
        (State::Adjusted, 4, 4)
    );
    assert_eq!(books.balance().left, 26);
}
