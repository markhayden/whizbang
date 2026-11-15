//! A reading of the names under `spend/` (ADR 0016): each entry by itself, what the
//! ledger can have written, and the most a reservation's names can mean.

use std::cell::Cell;
use std::sync::Arc;

use whizbang_core::capability::Stage;
use whizbang_core::contract::Reservation;
use whizbang_core::spend::{
    Amount, Counted, EpochWord, Name, Names, PlaceBy, Reading, Record, RunSpend, Seal, Sent, Sum,
};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Kind, Store};
use whizbang_testkit::scratch::Scratch;

use super::{amount, n, run};

const ROOT: &str = "7-1";

fn record() -> Name {
    Name::Reservation(run(ROOT))
}

fn held(epoch: u64, micros: u64) -> Name {
    Name::Held {
        epoch,
        run: run(ROOT),
        micros: amount(micros),
    }
}

fn place(place: u64) -> Name {
    Name::Place {
        run: run(ROOT),
        place: n(place),
    }
}

fn dispatched(place: u64, by: &str, micros: u64) -> Name {
    Name::Dispatched {
        run: run(ROOT),
        place: n(place),
        by: run(by),
        micros: amount(micros),
    }
}

fn settling(place: u64) -> Name {
    Name::Settling {
        run: run(ROOT),
        place: n(place),
    }
}

fn settled(place: u64, stage: Stage, micros: u64) -> Name {
    Name::Settled {
        run: run(ROOT),
        place: n(place),
        stage,
        micros: amount(micros),
    }
}

fn closed(released: bool) -> Name {
    Name::Closed {
        run: run(ROOT),
        released,
    }
}

fn of(names: &[Name]) -> Names {
    Names::of(
        &run(ROOT),
        names.iter().cloned().map(|name| (name, false)),
        [],
    )
}

/// Books the ledger wrote: reserve 10; a request of 5 by the root, settled at 12,
/// which is over its bound and takes the place 2; a request of nothing by a child,
/// not sent; the run interrupted, its closing at the place 4; a correction at 5.
fn whole() -> Vec<Name> {
    let mut names = vec![
        record(),
        held(0, 10),
        place(1),
        dispatched(1, ROOT, 5),
        settling(1),
        settled(1, Stage::Answered, 12),
        place(2),
        place(3),
        dispatched(3, "7-2", 0),
        settling(3),
        settled(3, Stage::NotSent, 0),
        place(4),
        closed(false),
        place(5),
    ];
    let before = of(&names);
    let seal = before.seal_aside(&[&place(5)]);
    names.push(Name::Adjusted {
        run: run(ROOT),
        place: n(5),
        turn: n(6),
        seal,
        micros: amount(12),
    });
    names
}

#[test]
fn the_recogniser_takes_what_the_ledger_writes_and_no_set_with_one_name_lost_or_damaged() {
    // Every step of the ledger's own publications is a set it takes.
    let all = whole();
    for upto in [2, 4, 7, 9, 11, 13, 15] {
        let names = of(&all[..upto]);
        assert!(names.reachable(), "the first {upto} names");
        assert_eq!(names.frozen(), None);
    }
    let names = of(&all);
    assert_eq!(names.count(), 15);
    assert_eq!(names.stated(), Some(amount(10)));
    assert_eq!(names.epoch(), Some(0));
    assert_eq!(names.charged(), Sum::new(12));
    assert_eq!(names.settled(), Sum::new(12));
    assert_eq!(names.outstanding(), Sum::ZERO);
    assert_eq!(names.unsettled(), 0);
    assert_eq!(names.turns_beside_holds(), 4);
    assert!(names.closed() && !names.released());
    assert_eq!(names.next_place(), 6);
    assert_eq!(names.most(), Sum::new(12));
    // No set the ledger writes is another with one name gone, or with one damaged.
    for lost in 0..all.len() {
        let mut less = all.clone();
        let gone = less.remove(lost);
        assert!(!of(&less).reachable(), "without {gone}");
        let hurt = all.iter().cloned().enumerate();
        let hurt = Names::of(&run(ROOT), hurt.map(|(at, name)| (name, at == lost)), []);
        assert!(!hurt.reachable(), "with {gone} damaged");
        assert!(hurt.most() >= Sum::new(10), "with {gone} damaged");
    }
    // A name of no form that belongs to it freezes it too.
    let stray = ["spend/place/7-1/01".to_owned()];
    let with = Names::of(
        &run(ROOT),
        all.iter().cloned().map(|name| (name, false)),
        stray,
    );
    assert!(!with.reachable());
    assert_eq!(with.count(), 16);
}

#[test]
fn what_the_ledger_could_not_have_written_is_not_taken() {
    let base = [record(), held(0, 10)];
    let with = |more: &[Name]| of(&[&base[..], more].concat());
    assert!(with(&[]).reachable());
    let cases: Vec<(&str, Vec<Name>)> = vec![
        ("a dispatch without its place", vec![dispatched(1, ROOT, 5)]),
        (
            "a place missing below",
            vec![place(2), dispatched(2, ROOT, 5)],
        ),
        (
            "two dispatches at one place",
            vec![place(1), dispatched(1, ROOT, 5), dispatched(1, "7-2", 4)],
        ),
        (
            "a settlement without its guard",
            vec![
                place(1),
                dispatched(1, ROOT, 5),
                settled(1, Stage::Answered, 1),
            ],
        ),
        (
            "a guard without its settlement",
            vec![place(1), dispatched(1, ROOT, 5), settling(1)],
        ),
        (
            "a settlement of no dispatch",
            vec![place(1), settling(1), settled(1, Stage::Answered, 1)],
        ),
        (
            "not sent, at a charge",
            vec![
                place(1),
                dispatched(1, ROOT, 5),
                settling(1),
                settled(1, Stage::NotSent, 1),
            ],
        ),
        (
            "unresolved, under its bound",
            vec![
                place(1),
                dispatched(1, ROOT, 5),
                settling(1),
                settled(1, Stage::Unresolved, 4),
            ],
        ),
        ("two closings", vec![place(1), closed(true), closed(false)]),
        ("a closing without its place", vec![closed(true)]),
        (
            "released with a request not settled",
            vec![place(1), dispatched(1, ROOT, 5), place(2), closed(true)],
        ),
        ("a bare place that nothing took", vec![place(1)]),
        (
            "a dispatch after a closing",
            vec![place(1), closed(false), place(2), dispatched(2, ROOT, 5)],
        ),
        // The replay: the second request did not fit what was left when it was sent.
        (
            "a dispatch that did not fit",
            vec![
                place(1),
                dispatched(1, ROOT, 6),
                place(2),
                dispatched(2, ROOT, 5),
            ],
        ),
        ("a second hold", vec![held(1, 10)]),
    ];
    for (what, more) in cases {
        let names = with(&more);
        assert!(!names.reachable(), "{what}");
        assert!(names.frozen().is_some(), "{what}");
    }
    // Without the record, and without a hold.
    assert!(!of(&[held(0, 10)]).reachable());
    assert!(!of(&[record()]).reachable());
}

#[test]
fn a_reservation_whose_names_the_ledger_did_not_check_into_place_is_frozen() {
    // The model's history 74: a dispatch of 1 under a hold of nothing, with its
    // place. Every row of the recogniser but the replay takes it.
    let names = of(&[record(), held(0, 0), place(1), dispatched(1, ROOT, 1)]);
    assert!(!names.reachable());
    assert_eq!(names.frozen(), Some(Counted::Exactly(Sum::new(1))));
    assert_eq!(
        names.sent_by(&run(ROOT)),
        Sent::Unknown {
            at_most: Sum::new(1)
        }
    );
    // The same under a hold that covers it is the ledger's.
    let fits = of(&[record(), held(0, 1), place(1), dispatched(1, ROOT, 1)]);
    assert!(fits.reachable());
    // What the replay does not refuse: a request settled under its bound counts at
    // its charge from the start, and the smallest excess takes the first bare place.
    let orders = of(&[
        record(),
        held(0, 10),
        place(1),
        dispatched(1, ROOT, 10),
        settling(1),
        settled(1, Stage::Answered, 2),
        place(2),
        dispatched(2, ROOT, 8),
    ]);
    assert!(orders.reachable());
}

#[test]
fn a_name_that_names_the_largest_place_is_counted_without_walking_to_it() {
    // A guard at the largest place a number can say, under a reservation of 10 with
    // one request of 4 outstanding. Every number up to it that holds no dispatch may
    // be a dispatch whose name is lost: there are the largest number less one of
    // them, and they are counted by one subtraction.
    let names = of(&[
        record(),
        held(0, 10),
        place(1),
        dispatched(1, ROOT, 4),
        place(u64::MAX),
    ]);
    assert!(!names.reachable());
    assert_eq!(names.unexplained(), u64::MAX - 1);
    let most = 10 + 4 + 10 * u128::from(u64::MAX - 1);
    assert_eq!(names.frozen(), Some(Counted::Exactly(Sum::new(most))));
    assert_eq!(names.next_place(), 2);
    // A settlement, its guard and a correction name a place as a guard does.
    for name in [
        settling(u64::MAX),
        settled(u64::MAX, Stage::Answered, 0),
        Name::Adjusted {
            run: run(ROOT),
            place: n(u64::MAX),
            turn: n(1),
            seal: Seal::over([]),
            micros: amount(0),
        },
    ] {
        let names = of(&[record(), held(0, 10), name]);
        assert_eq!(names.unexplained(), u64::MAX);
    }
}

#[test]
fn sums_of_amounts_near_the_largest_number_stop_at_it() {
    assert_eq!(Sum::LARGEST.plus(Sum::new(1)), Sum::LARGEST);
    assert_eq!(Sum::new(u128::MAX - 1).times(2), Sum::LARGEST);
    assert_eq!(Sum::new(3).less(Sum::new(5)), Sum::ZERO);
    assert!(Sum::LARGEST.stopped() && !Sum::new(u128::MAX - 1).stopped());
    assert_eq!(Sum::parse(&Sum::LARGEST.to_string()), Some(Sum::LARGEST));
    for not in [
        "",
        "01",
        "+1",
        " 1",
        "340282366920938463463374607431768211456",
    ] {
        assert_eq!(Sum::parse(not), None, "{not:?}");
    }
    // The frozen sum of names that state nearly the largest amount at the largest
    // place: it would be more than 128 bits hold, and stops.
    let most = Amount::MOST.micros();
    let mut names = vec![record(), held(0, most), place(u64::MAX)];
    for by in ["7-1", "7-2", "7-3"] {
        names.push(dispatched(u64::MAX, by, most));
    }
    for stage in Stage::ALL {
        names.push(settled(u64::MAX, stage, most));
    }
    let names = of(&names);
    assert_eq!(names.most(), Sum::LARGEST);
    assert_eq!(names.frozen(), Some(Counted::Exactly(Sum::LARGEST)));
    assert_eq!(names.known().micros(), 6 * u128::from(most));
    // Two real settlements of nearly the largest amount are more than 64 bits hold.
    let two = of(&[
        record(),
        held(0, most),
        place(1),
        dispatched(1, ROOT, 0),
        settling(1),
        settled(1, Stage::Answered, most),
        place(2),
        place(3),
        dispatched(3, ROOT, 0),
        settling(3),
        settled(3, Stage::Answered, most),
        place(4),
    ]);
    assert!(two.reachable());
    assert_eq!(two.charged().micros(), 2 * u128::from(most));
    assert!(two.charged().micros() > u128::from(u64::MAX));
}

#[test]
fn each_of_the_three_shapes_of_a_reading_is_answered() {
    // A number: what each run sent under a reachable reservation, and what a frozen
    // reservation with its one hold counts for.
    let sound = of(&[
        record(),
        held(0, 10),
        place(1),
        dispatched(1, ROOT, 5),
        settling(1),
        settled(1, Stage::Unresolved, 5),
        place(2),
        dispatched(2, "7-2", 3),
    ]);
    let spent = |charged: u128, outstanding: u128, unresolved: u128| {
        Sent::Known(RunSpend {
            charged: Sum::new(charged),
            outstanding: Sum::new(outstanding),
            unresolved: Sum::new(unresolved),
        })
    };
    assert_eq!(sound.sent_by(&run(ROOT)), spent(5, 0, 5));
    assert_eq!(sound.sent_by(&run("7-2")), spent(0, 3, 3));
    assert_eq!(sound.sent_by(&run("7-9")), spent(0, 0, 0));
    assert_eq!(sound.frozen(), None);
    // Frozen with one hold: a number, and of a run "unknown, at most".
    let frozen = of(&[record(), held(0, 10), dispatched(1, ROOT, 5)]);
    assert_eq!(frozen.frozen(), Some(Counted::Exactly(Sum::new(15))));
    assert_eq!(
        frozen.sent_by(&run("7-2")),
        Sent::Unknown {
            at_most: Sum::new(15)
        }
    );
    // No hold, and two: "unknown, at least", and of a run "unknown" with no most.
    let none = of(&[record(), place(1), dispatched(1, ROOT, 5)]);
    assert_eq!(none.frozen(), Some(Counted::AtLeast(Sum::new(5))));
    assert_eq!(none.sent_by(&run(ROOT)), Sent::Unbounded);
    assert_eq!(none.stated(), None);
    let two = of(&[
        record(),
        held(0, 0),
        held(3, 7),
        place(1),
        dispatched(1, ROOT, 5),
    ]);
    assert_eq!(two.frozen(), Some(Counted::AtLeast(Sum::new(12))));
    assert_eq!(two.sent_by(&run(ROOT)), Sent::Unbounded);
    assert_eq!(two.epoch(), None);
    assert_eq!(two.epochs().into_iter().collect::<Vec<_>>(), [0, 3]);
    assert_eq!(Counted::AtLeast(Sum::new(12)).least(), Sum::new(12));
}

#[test]
fn a_frozen_reservation_gives_no_credit_for_a_settlement_however_whole() {
    // The model's history 15: reserve 10, two requests of 2 settled at 1 and at 2,
    // closed with everything released; then the reservation's record is damaged. It
    // released at 3. Frozen, no settlement gives up anything of a bound: it counts
    // the amount, both bounds, both charges, and the amount once more for the place
    // its closing took, which is 27.
    let names = [
        record(),
        held(0, 10),
        place(1),
        dispatched(1, ROOT, 2),
        settling(1),
        settled(1, Stage::Answered, 1),
        place(2),
        dispatched(2, ROOT, 2),
        settling(2),
        settled(2, Stage::Answered, 2),
        place(3),
        closed(true),
    ];
    let sound = of(&names);
    assert!(sound.reachable() && sound.released());
    assert_eq!(
        (sound.known(), sound.outstanding()),
        (Sum::new(3), Sum::ZERO)
    );
    let hurt = names.iter().map(|name| (name.clone(), *name == record()));
    let frozen = Names::of(&run(ROOT), hurt, []);
    assert_eq!(frozen.frozen(), Some(Counted::Exactly(Sum::new(27))));
    assert_eq!(frozen.outstanding(), Sum::new(4));
    // What its whole settlements say is still what a correction cannot go under.
    assert_eq!((frozen.settled(), frozen.unsettled()), (Sum::new(3), 0));
}

#[test]
fn a_correction_is_given_with_what_its_seal_has_to_be_and_whether_it_is_at_its_place() {
    let all = whole();
    let names = of(&all);
    let [one] = names.corrections().try_into().unwrap();
    assert!(one.stands_but_for_its_turn());
    assert_eq!((one.place, one.turn, one.micros), (5, 6, amount(12)));
    // Its seal is of the other names, its own place's guard aside: the fixed form.
    let others = all
        .iter()
        .filter(|name| **name != one.name && **name != place(5));
    let texts: Vec<String> = others.map(Name::to_string).collect();
    let over = Seal::over(texts.iter().map(|text| (text.as_str(), false)));
    assert_eq!(one.sealed_over, over);
    // A name made, lost or damaged after it: the seal is no longer the names'.
    let mut more = all.clone();
    more.push(place(6));
    assert!(!of(&more).corrections()[0].stands_but_for_its_turn());
    let less: Vec<Name> = all
        .iter()
        .filter(|name| **name != place(2))
        .cloned()
        .collect();
    assert!(!of(&less).corrections()[0].stands_but_for_its_turn());
    let hurt = all.iter().map(|name| (name.clone(), *name == record()));
    let hurt = Names::of(&run(ROOT), hurt, []);
    assert!(!hurt.corrections()[0].stands_but_for_its_turn());
    // Without its place, or with no closing, it is at no place of its own.
    let bare: Vec<Name> = all
        .iter()
        .filter(|name| **name != place(5))
        .cloned()
        .collect();
    assert!(!of(&bare).corrections()[0].at_its_place);
    let open: Vec<Name> = all
        .iter()
        .filter(|name| **name != closed(false))
        .cloned()
        .collect();
    assert!(!of(&open).corrections()[0].at_its_place);
}

fn reservation_bytes(run: &str, micros: u64) -> Vec<u8> {
    let bytes = format!(
        "{{\"record\":\"reservation/1\",\"run\":\"{run}\",\"request\":null,\"admission\":\
         \"sha256:{}\",\"reserved_micros\":{micros},\"start_ms\":1}}\n",
        "ab".repeat(32)
    )
    .into_bytes();
    assert_eq!(Reservation::from_bytes(&bytes).unwrap().micros(), micros);
    bytes
}

#[test]
fn with_one_reservations_content_damaged_the_reading_gives_the_others_and_that_one_as_damaged() {
    let dir = Scratch::new("spend-reading");
    let store = Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap();
    let mut publication = store.begin().unwrap();
    let mut make = |name: &str, bytes: &[u8]| {
        publication.create(name, Kind::Diagnostic, bytes).unwrap();
    };
    for (id, micros) in [("7-1", 10), ("7-2", 20), ("7-3", 30)] {
        let hold = Record::Hold {
            epoch: 0,
            run: run(id),
            reserved: amount(micros),
        };
        make(&hold.name().to_string(), &hold.to_bytes());
        // The second reservation's record is not one: its content is damaged.
        let record = match id {
            "7-2" => b"{\"record\":\"reservation/1\"}\n".to_vec(),
            _ => reservation_bytes(id, micros),
        };
        make(&Name::Reservation(run(id)).to_string(), &record);
    }
    // The third has a place whose content is the record of another place, and one
    // too long to be a record, which is never read.
    let other = Record::Place {
        run: run("7-3"),
        place: n(9),
        by: PlaceBy::Dispatch,
    };
    make("spend/place/7-3/1", &other.to_bytes());
    let long = vec![b' '; 2000];
    make("spend/place/7-3/2", &long);
    // A turn, sound; and texts of no form: of the first reservation, of an epoch, of
    // nothing.
    let turn = Record::Turn {
        epoch: 0,
        turn: n(1),
        by: whizbang_core::spend::TurnBy::Admission,
        run: Some(run("7-1")),
    };
    make(&turn.name().to_string(), &turn.to_bytes());
    make("spend/closed/7-1/open", b"x");
    make("spend/reset/4/x", b"x");
    make("spend/fake/1", b"x");
    make("requests/not-spend", b"x");
    publication.publish().unwrap();

    let generation = store.snapshot().unwrap();
    let longest = Cell::new(0);
    let reading = Reading::of(&generation, |entry| {
        longest.set(longest.get().max(entry.bytes));
        store.read(entry)
    });
    assert!(longest.get() <= 1024, "an entry over the bound was read");
    let ids: Vec<&str> = reading.reservations.keys().map(|id| id.as_str()).collect();
    assert_eq!(ids, ["7-1", "7-2", "7-3"]);
    let one = &reading.reservations[&run("7-1")];
    assert!(!one.damaged() && !one.reachable(), "a name of no form");
    assert_eq!(one.formless(), ["spend/closed/7-1/open"]);
    // The damaged one is given with the amount its hold's name states, and counts
    // for no less.
    let two = &reading.reservations[&run("7-2")];
    assert!(two.damaged() && two.has_record() && !two.reachable());
    assert_eq!(two.stated(), Some(amount(20)));
    assert_eq!(two.frozen(), Some(Counted::Exactly(Sum::new(20))));
    let three = &reading.reservations[&run("7-3")];
    assert!(three.damaged());
    let hurt: Vec<String> = three
        .all()
        .iter()
        .filter(|(_, hurt)| *hurt)
        .map(|(name, _)| name.to_string())
        .collect();
    assert_eq!(hurt, ["spend/place/7-3/1", "spend/place/7-3/2"]);
    assert_eq!(reading.epochs, [(turn.name(), false)]);
    let formless: Vec<_> = reading
        .formless
        .iter()
        .map(|one| (one.text.as_str(), one.epoch))
        .collect();
    assert_eq!(
        formless,
        [
            ("spend/fake/1", None),
            ("spend/reset/4/x", Some((EpochWord::Reset, 4)))
        ]
    );
    // The one gathering: the same names given as text, with the same damage, read
    // the same; and a text outside `spend/` is left out.
    let hurt = [
        "spend/reservations/7-2",
        "spend/place/7-3/1",
        "spend/place/7-3/2",
    ];
    let texts = generation.entries().keys();
    let apart = Reading::of_names(texts.map(|text| (text.as_str(), hurt.contains(&text.as_str()))));
    assert_eq!(apart, reading);
    assert!(generation.get("requests/not-spend").is_some());
    // A sound store reads sound: the first reservation without its stray name.
    let sound = Names::of(&run("7-1"), one.all().iter().cloned(), []);
    assert!(sound.reachable());
}
