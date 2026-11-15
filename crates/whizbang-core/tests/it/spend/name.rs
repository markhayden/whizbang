//! The grammar of the names under `spend/` (ADR 0016, "The books are names").

use whizbang_core::capability::Stage;
use whizbang_core::spend::{Amount, Belongs, EpochWord, Name, Seal};

use super::{amount, n, run};

fn every_form() -> Vec<(Name, &'static str)> {
    let seal = Seal::parse("00112233445566778899aabbccddeeff").unwrap();
    vec![
        (Name::Reservation(run("7-1")), "spend/reservations/7-1"),
        (
            Name::Held {
                epoch: 0,
                run: run("7-1"),
                micros: amount(10),
            },
            "spend/held/0/7-1/10",
        ),
        (
            Name::Turn {
                epoch: 3,
                turn: n(18_446_744_073_709_551_615),
            },
            "spend/turn/3/18446744073709551615",
        ),
        (
            Name::Reset {
                epoch: n(1),
                micros: Amount::MOST,
            },
            "spend/reset/1/18446744073709551614",
        ),
        (Name::Epoch(n(2)), "spend/epoch/2"),
        (
            Name::Place {
                run: run("7-1"),
                place: n(4),
            },
            "spend/place/7-1/4",
        ),
        (
            Name::Dispatched {
                run: run("7-1"),
                place: n(4),
                by: run("7-2"),
                micros: amount(0),
            },
            "spend/dispatched/7-1/4/7-2/0",
        ),
        (
            Name::Settling {
                run: run("7-1"),
                place: n(4),
            },
            "spend/settling/7-1/4",
        ),
        (
            Name::Settled {
                run: run("7-1"),
                place: n(4),
                stage: Stage::NotSent,
                micros: amount(0),
            },
            "spend/settled/7-1/4/not_sent/0",
        ),
        (
            Name::Settled {
                run: run("7-1"),
                place: n(4),
                stage: Stage::Answered,
                micros: amount(12),
            },
            "spend/settled/7-1/4/answered/12",
        ),
        (
            Name::Settled {
                run: run("7-1"),
                place: n(4),
                stage: Stage::Unresolved,
                micros: amount(5),
            },
            "spend/settled/7-1/4/unresolved/5",
        ),
        (
            Name::Closed {
                run: run("7-1"),
                released: true,
            },
            "spend/closed/7-1/released",
        ),
        (
            Name::Closed {
                run: run("7-1"),
                released: false,
            },
            "spend/closed/7-1/held",
        ),
        (
            Name::Adjusted {
                run: run("7-1"),
                place: n(5),
                turn: n(9),
                seal,
                micros: amount(3),
            },
            "spend/adjusted/7-1/5/9/00112233445566778899aabbccddeeff/3",
        ),
    ]
}

#[test]
fn each_form_of_name_parses_to_its_parts_and_is_written_back_the_same() {
    for (name, text) in every_form() {
        assert_eq!(name.to_string(), text);
        assert_eq!(Name::parse(text).unwrap(), name, "{text}");
        assert!(text.len() <= whizbang_core::spend::NAME_LIMIT);
    }
    // The longest name there is, with the longest run ids and numbers, is a name.
    let long = "a".repeat(64);
    let most = u64::MAX - 1;
    let text = format!("spend/dispatched/{long}/{}/{long}/{most}", u64::MAX);
    assert!(text.len() <= 200, "{}", text.len());
    assert_eq!(Name::parse(&text).unwrap().to_string(), text);
}

#[test]
fn a_name_that_breaks_the_grammar_in_each_way_is_refused_and_shown_escaped_and_cut() {
    let seal = "00112233445566778899aabbccddeeff";
    let broken: Vec<(String, &str)> = vec![
        ("spent/held/0/7-1/10".into(), "start with spend/"),
        ("spend/held/00/7-1/10".into(), "no leading zero"),
        ("spend/held/0/7-1/010".into(), "no leading zero"),
        ("spend/held/0/7-1/+10".into(), "decimal digits"),
        ("spend/held/0/7-1/-10".into(), "decimal digits"),
        ("spend/held/0/7-1/ 10".into(), "decimal digits"),
        ("spend/held/0/7-1/1e3".into(), "decimal digits"),
        ("spend/held/0/7-1/".into(), "empty"),
        ("spend/held//7-1/10".into(), "empty"),
        ("spend/held/0/./10".into(), ". or .."),
        ("spend/held/0/../10".into(), ". or .."),
        ("spend/held/0/7-1".into(), "too many or too few"),
        ("spend/held/0/7-1/10/1".into(), "too many or too few"),
        ("spend/Held/0/7-1/10".into(), "none of the forms"),
        ("spend/HELD/0/7-1/10".into(), "none of the forms"),
        ("spend/fake/1".into(), "none of the forms"),
        ("spend/held/0/7_A/10".into(), "run id"),
        // The largest number is no amount, and a number is of 64 bits.
        ("spend/held/0/7-1/18446744073709551615".into(), "no amount"),
        ("spend/held/0/7-1/18446744073709551616".into(), "64 bits"),
        (
            "spend/held/0/7-1/184467440737095516150".into(),
            "20 decimal digits",
        ),
        ("spend/reset/0/10".into(), "from 1"),
        ("spend/epoch/0".into(), "from 1"),
        ("spend/turn/0/0".into(), "from 1"),
        ("spend/place/7-1/0".into(), "from 1"),
        ("spend/settled/7-1/1/Answered/1".into(), "stage"),
        ("spend/settled/7-1/1/sent/1".into(), "stage"),
        ("spend/closed/7-1/Released".into(), "released or held"),
        ("spend/closed/7-1/open".into(), "released or held"),
        (
            format!("spend/adjusted/7-1/1/1/{}/1", seal.to_uppercase()),
            "seal",
        ),
        (format!("spend/adjusted/7-1/1/1/{}/1", &seal[1..]), "seal"),
        (format!("spend/adjusted/7-1/1/1/{seal}0/1"), "seal"),
        (
            format!("spend/place/{}/1", "a".repeat(200)),
            "200 characters",
        ),
    ];
    for (text, why) in &broken {
        let refused = Name::parse(text).expect_err(text);
        assert!(refused.why().contains(why), "{text}: {}", refused.why());
    }
    // What was refused is shown escaped, and cut.
    let hostile = format!("spend/held/0/\n\u{1b}[31m{}/10", "x".repeat(400));
    let said = Name::parse(&hostile).unwrap_err().to_string();
    assert!(!said.contains('\n') && !said.contains('\u{1b}'), "{said}");
    assert!(
        said.contains("\\n") && said.contains("characters)"),
        "{said}"
    );
    assert!(said.len() < 300, "{}", said.len());
}

#[test]
fn a_name_of_no_form_belongs_to_a_reservation_or_an_epoch_by_the_place_of_its_segments() {
    let belongs = |text: &str| Name::parse(text).unwrap_err().belongs().clone();
    let reservation = |id: &str| Belongs::Reservation(run(id));
    // The eight forms of a reservation's names: the run is the third segment of a
    // hold and the second of the others.
    assert_eq!(belongs("spend/held/0/7-1/010"), reservation("7-1"));
    assert_eq!(belongs("spend/held/x/7-1"), reservation("7-1"));
    for word in [
        "reservations",
        "place",
        "dispatched",
        "settling",
        "settled",
        "closed",
        "adjusted",
    ] {
        let text = format!("spend/{word}/7-1/what/ever/else/there/is");
        assert_eq!(belongs(&text), reservation("7-1"), "{text}");
    }
    // A segment elsewhere that could be a run id names no reservation.
    assert_eq!(belongs("spend/held/7-1"), Belongs::Unplaced);
    assert_eq!(belongs("spend/fake/7-1"), Belongs::Unplaced);
    assert_eq!(belongs("spend/7-1/held"), Belongs::Unplaced);
    assert_eq!(belongs("spend/closed/NOT_AN_ID/held"), Belongs::Unplaced);
    assert_eq!(belongs("spend/"), Belongs::Unplaced);
    assert_eq!(belongs("elsewhere/held/0/7-1/1"), Belongs::Unplaced);
    // A reset, a guard and a turn of no form name their epoch.
    let epoch = |word, epoch| Belongs::Epoch { word, epoch };
    assert_eq!(belongs("spend/reset/2/abc"), epoch(EpochWord::Reset, 2));
    assert_eq!(belongs("spend/reset/2"), epoch(EpochWord::Reset, 2));
    assert_eq!(belongs("spend/epoch/2/more"), epoch(EpochWord::Guard, 2));
    assert_eq!(belongs("spend/turn/0/01"), epoch(EpochWord::Turn, 0));
    assert_eq!(belongs("spend/turn/x/1"), Belongs::Unplaced);
    assert_eq!(belongs("spend/turn"), Belongs::Unplaced);
}

#[test]
fn the_largest_number_is_no_amount() {
    assert_eq!(Amount::new(u64::MAX), None);
    assert_eq!(Amount::new(u64::MAX - 1), Some(Amount::MOST));
    assert_eq!(Amount::new(0), Some(Amount::ZERO));
}

#[test]
fn a_seal_is_the_first_32_digits_of_a_sha_256_of_the_names_in_their_order() {
    // The fixed vector: SHA-256 of
    // "spend/held/0/7-1/10\nspend/place/7-1/1!\nspend/reservations/7-1\n".
    let names = [
        ("spend/reservations/7-1", false),
        ("spend/place/7-1/1", true),
        ("spend/held/0/7-1/10", false),
    ];
    let seal = Seal::over(names);
    assert_eq!(seal.to_string(), "9ef3cd339c47246227aa058c772d49ad");
    assert_eq!(Seal::parse(&seal.to_string()), Some(seal));
    // The order they are given in changes nothing; damage and a name more do.
    let mut turned = names;
    turned.reverse();
    assert_eq!(Seal::over(turned), seal);
    let mut sound = names;
    sound[1].1 = false;
    assert_ne!(Seal::over(sound), seal);
    assert_ne!(Seal::over(names.into_iter().take(2)), seal);
}
