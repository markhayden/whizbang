//! What a request cost, and how far a failed one got (ADR 0010, "How far a request got,
//! and what it cost"): a charge, the three stages a fault can say, and that a fault
//! reads as unresolved when whoever made it said no stage.

use whizbang_core::capability::written::{read_script, write_script};
use whizbang_core::capability::{
    Charge, Fault, FaultKind, Painted, Part, SegmentResponse, Stage, StopReason, Turn, Unfit,
};

fn charge(micros: u64) -> Charge {
    Charge::of_micros(micros as f64).unwrap()
}

fn turn() -> Turn {
    let text = Part::Text("yes".to_owned());
    Turn::new(vec![text], StopReason::EndTurn, None, None).unwrap()
}

fn painted() -> Painted {
    Painted::from_rgb(1, 1, vec![1, 2, 3]).unwrap()
}

#[test]
fn a_charge_cannot_be_made_negative_or_over_the_largest_amount() {
    let two_to_the_64 = 18_446_744_073_709_551_616.0;
    let refused = [
        -1.0,
        -0.000_001,
        f64::NEG_INFINITY,
        f64::NAN,
        f64::INFINITY,
        two_to_the_64,
        f64::MAX,
    ];
    for number in refused {
        let made = Charge::of_micros(number);
        assert!(
            matches!(made, Err(Unfit::OutOfRange { .. })),
            "{number} made {made:?}"
        );
    }
    assert_eq!(
        Charge::of_micros(-3.0).unwrap_err().to_string(),
        "a charge is -3: a value is a finite number of millionths of a US dollar, from 0 to \
         under 2 to the power 64"
    );
    // The largest number under it that the machine can write is a charge, and is not
    // the largest amount.
    let largest = Charge::of_micros(18_446_744_073_709_549_568.0).unwrap();
    assert_eq!(largest.micros(), u64::MAX - 2047);
}

#[test]
fn a_charge_is_a_whole_count_of_the_unit_rounded_up() {
    for (number, micros) in [
        (0.0, 0),
        (0.2, 1),
        (1.0, 1),
        (41.5, 42),
        (9_000_000.0, 9_000_000),
    ] {
        assert_eq!(Charge::of_micros(number).unwrap().micros(), micros);
    }
    assert_eq!(charge(7), charge(7));
    assert_ne!(charge(7), charge(8));
}

#[test]
fn every_stage_has_its_name() {
    let names: Vec<&str> = Stage::ALL.iter().map(|stage| stage.name()).collect();
    assert_eq!(names, ["not_sent", "answered", "unresolved"]);
}

#[test]
fn each_of_the_three_stages_can_be_read_from_a_fault() {
    let fault = || Fault::new(FaultKind::Unavailable, "status 503");
    let not_sent = fault().not_sent();
    assert_eq!(
        (not_sent.stage(), not_sent.charge()),
        (Stage::NotSent, None)
    );
    let known = fault().answered(Some(charge(120)));
    assert_eq!(
        (known.stage(), known.charge()),
        (Stage::Answered, Some(charge(120)))
    );
    let silent = fault().answered(None);
    assert_eq!((silent.stage(), silent.charge()), (Stage::Answered, None));
    let unresolved = fault().unresolved();
    assert_eq!(
        (unresolved.stage(), unresolved.charge()),
        (Stage::Unresolved, None)
    );
    // Stating a stage changes nothing else of the fault.
    let waited = fault().retry_after_ms(1500).answered(Some(charge(1)));
    assert_eq!(waited.kind(), FaultKind::Unavailable);
    assert_eq!(waited.sentence(), "status 503");
    assert_eq!(waited.wait_ms(), Some(1500));
    assert_eq!(waited.to_string(), "unavailable: status 503");
}

#[test]
fn a_wait_stated_after_a_stage_keeps_the_stage_and_the_charge() {
    let fault = || Fault::new(FaultKind::Unavailable, "status 429");
    let answered = fault().answered(Some(charge(55))).retry_after_ms(2000);
    assert_eq!(
        (answered.stage(), answered.charge(), answered.wait_ms()),
        (Stage::Answered, Some(charge(55)), Some(2000))
    );
    let not_sent = fault().not_sent().retry_after_ms(2000);
    assert_eq!(
        (not_sent.stage(), not_sent.charge(), not_sent.wait_ms()),
        (Stage::NotSent, None, Some(2000))
    );
    // In either order the fault is the same fault.
    assert_eq!(
        answered,
        fault().retry_after_ms(2000).answered(Some(charge(55)))
    );
}

#[test]
fn only_an_answered_fault_holds_a_charge() {
    let answered = Fault::new(FaultKind::Rejected, "status 400").answered(Some(charge(9)));
    // Said again as another stage, the charge is gone with the stage it belonged to.
    let unresolved = answered.clone().unresolved();
    assert_eq!(
        (unresolved.stage(), unresolved.charge()),
        (Stage::Unresolved, None)
    );
    let not_sent = answered.clone().not_sent();
    assert_eq!(
        (not_sent.stage(), not_sent.charge()),
        (Stage::NotSent, None)
    );
    // And the stage stated last is the stage.
    let again = not_sent.answered(Some(charge(4)));
    assert_eq!(
        (again.stage(), again.charge()),
        (Stage::Answered, Some(charge(4)))
    );
}

#[test]
fn a_fault_that_states_no_stage_reads_as_unresolved_whatever_its_kind() {
    // ADR 0006's table, kind by kind: every fault made before a fault could say.
    let by_kind = [
        (FaultKind::Cancelled, Stage::Unresolved),
        (FaultKind::Unsupported, Stage::Unresolved),
        (FaultKind::Credential, Stage::Unresolved),
        (FaultKind::Rejected, Stage::Unresolved),
        (FaultKind::Unavailable, Stage::Unresolved),
        (FaultKind::TimedOut, Stage::Unresolved),
        (FaultKind::Malformed, Stage::Unresolved),
        (FaultKind::TooLarge, Stage::Unresolved),
    ];
    let kinds: Vec<FaultKind> = by_kind.iter().map(|(kind, _)| *kind).collect();
    assert_eq!(kinds, FaultKind::ALL);
    for (kind, stage) in by_kind {
        let fault = Fault::new(kind, "as before");
        assert_eq!(fault.stage(), stage, "{}", kind.name());
        assert_eq!(fault.charge(), None, "{}", kind.name());
        // A wait does not say how far the request got.
        assert_eq!(fault.retry_after_ms(5).stage(), stage, "{}", kind.name());
    }
    // An answer that is not fit to hold, turned into a fault, says no stage either.
    let unfit = Unfit::Empty { what: "a turn" };
    assert_eq!(Fault::from(unfit).stage(), Stage::Unresolved);
}

#[test]
fn a_kind_does_not_bind_a_stage_that_is_stated() {
    for kind in FaultKind::ALL {
        for stage in Stage::ALL {
            let fault = Fault::new(kind, "x");
            let stated = match stage {
                Stage::NotSent => fault.not_sent(),
                Stage::Answered => fault.answered(None),
                Stage::Unresolved => fault.unresolved(),
            };
            assert_eq!(stated.stage(), stage, "{} {}", kind.name(), stage.name());
        }
    }
}

#[test]
fn two_faults_that_differ_in_stage_or_in_charge_are_not_equal() {
    let fault = || Fault::new(FaultKind::Unavailable, "status 503");
    assert_ne!(fault().not_sent(), fault().unresolved());
    assert_ne!(fault().answered(None), fault().unresolved());
    assert_ne!(fault().answered(None), fault().answered(Some(charge(0))));
    assert_ne!(
        fault().answered(Some(charge(1))),
        fault().answered(Some(charge(2)))
    );
    assert_eq!(
        fault().answered(Some(charge(1))),
        fault().answered(Some(charge(1)))
    );
    // A stage that is stated and the same stage read where none was are one stage.
    assert_eq!(fault(), fault().unresolved());
    // No kind is read as not sent: only whoever knows that nothing left says so.
    let unsupported = || Fault::new(FaultKind::Unsupported, "no such feature");
    assert_eq!(unsupported(), unsupported().unresolved());
    assert_ne!(unsupported(), unsupported().not_sent());
}

#[test]
fn a_response_made_as_before_carries_no_charge() {
    assert_eq!(painted().charge(), None);
    assert_eq!(SegmentResponse::new(vec![Vec::new()]).charge(), None);
    assert_eq!(turn().charge(), None);
}

#[test]
fn each_response_may_carry_a_charge() {
    let fill = painted().charged(charge(30));
    assert_eq!(fill.charge(), Some(charge(30)));
    assert_eq!(fill.rgb(), painted().rgb());
    assert_ne!(fill, painted());
    let answer = SegmentResponse::new(vec![Vec::new()]).charged(charge(31));
    assert_eq!(answer.charge(), Some(charge(31)));
    assert_ne!(answer, SegmentResponse::new(vec![Vec::new()]));
    let charged = turn().charged(charge(32));
    assert_eq!(charged.charge(), Some(charge(32)));
    assert_eq!(charged.stop(), turn().stop());
    assert_ne!(charged, turn());
    // Stated again, the later charge is the charge.
    assert_eq!(charged.charged(charge(1)).charge(), Some(charge(1)));
    // What an assistant said goes on as a message without it: a charge is of a
    // request, not of what was said.
    assert_eq!(turn().charged(charge(32)).message(), turn().message());
}

#[test]
fn debug_of_a_fault_and_of_a_response_shows_the_stage_and_the_charge() {
    let fault = Fault::new(FaultKind::Rejected, "status 400");
    assert_eq!(
        format!("{fault:?}"),
        "Fault { kind: Rejected, sentence: \"status 400\", retry_after_ms: None, stage: \
         Unresolved, charge: None }"
    );
    assert_eq!(
        format!("{:?}", fault.answered(Some(charge(12)))),
        "Fault { kind: Rejected, sentence: \"status 400\", retry_after_ms: None, stage: \
         Answered, charge: Some(Charge { micros: 12 }) }"
    );
    assert_eq!(format!("{:?}", charge(12)), "Charge { micros: 12 }");
    assert_eq!(format!("{:?}", Stage::NotSent), "NotSent");
    assert_eq!(format!("{:?}", painted()), "Painted(1 by 1, no charge)");
    assert_eq!(
        format!("{:?}", painted().charged(charge(5))),
        "Painted(1 by 1, charged 5)"
    );
    assert_eq!(
        format!("{:?}", SegmentResponse::new(Vec::new()).charged(charge(5))),
        "SegmentResponse { answers: [], charge: Some(Charge { micros: 5 }) }"
    );
    let shown = format!("{:?}", turn().charged(charge(5)));
    assert!(
        shown.ends_with("request_id: None, charge: Some(Charge { micros: 5 }) }"),
        "{shown}"
    );
    assert!(format!("{:?}", turn()).ends_with("request_id: None, charge: None }"));
}

#[test]
fn known_a_turn_and_a_fault_read_back_from_data_carry_no_charge_and_no_stated_stage() {
    // The written form of a script and of an exchange (ADR 0009) has no place for a
    // charge or a stage, and is not changed: what a request cost is the ledger's
    // record. Read back, a turn has no charge and a fault states no stage.
    let script = [
        Ok(turn().charged(charge(77))),
        Err(Fault::new(FaultKind::Unavailable, "status 503").answered(Some(charge(3)))),
        Err(Fault::new(FaultKind::Rejected, "status 400").not_sent()),
    ];
    let written = write_script(&script);
    let text = String::from_utf8(written.clone()).unwrap();
    assert!(
        !text.contains("charge") && !text.contains("stage"),
        "{text}"
    );
    let read = read_script(&written).unwrap();
    assert_eq!(read[0].as_ref().unwrap().charge(), None);
    let fault = read[1].as_ref().unwrap_err();
    assert_eq!((fault.stage(), fault.charge()), (Stage::Unresolved, None));
    let fault = read[2].as_ref().unwrap_err();
    assert_eq!(fault.stage(), Stage::Unresolved);
}
