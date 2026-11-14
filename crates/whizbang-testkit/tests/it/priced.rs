//! The fakes say what a real backend says of cost and of where its requests go (ADR
//! 0010, ADR 0012): each can be built with a price and a recipient, can report a
//! charge with its answer, and can fail at a named stage.

use std::sync::Arc;

use whizbang_core::capability::{
    Charge, Controls, Declaration, Fault, FaultKind, FillHandle, FillRequest, Filler, Judge,
    JudgeHandle, JudgeRequest, Message, Part, Picture, Price, Query, QuoteKind, Role,
    SegmentHandle, SegmentRequest, Segmenter, Stage, StopReason, Support, TimeLimit, Turn,
};
use whizbang_core::contract::{Backends, Cancellation, Capability, Estimate, Secrets};
use whizbang_core::egress::{Account, Origin, Recipient};
use whizbang_core::identity::CanonicalPixels;
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::capabilities::{FakeFiller, FakeJudge, FakeSegmenter, Paint, roster};

fn picture() -> Picture {
    let rgba = (0..4 * 4 * 4).map(|at| at as u8).collect();
    Picture::new(CanonicalPixels::from_rgba(4, 4, rgba).unwrap()).unwrap()
}

fn fill_request() -> FillRequest {
    let support = Support::from_plane(4, 4, vec![255; 16]).unwrap();
    FillRequest::new(picture(), support, None).unwrap()
}

fn segment_request() -> SegmentRequest {
    SegmentRequest::new(picture(), vec![Query::phrase("a dog").unwrap()], 1).unwrap()
}

fn judge_request() -> JudgeRequest {
    let said = Message::new(Role::User, vec![Part::Text("well?".to_owned())]).unwrap();
    JudgeRequest::new("", vec![said], Vec::new(), Controls::new(64).unwrap()).unwrap()
}

fn yes() -> Turn {
    Turn::new(
        vec![Part::Text("yes".to_owned())],
        StopReason::EndTurn,
        None,
        None,
    )
    .unwrap()
}

fn judge(turns: usize) -> FakeJudge {
    FakeJudge::scripted((0..turns).map(|_| Ok(yes())).collect())
}

fn recipient() -> Recipient {
    let none = Secrets::none();
    Recipient::at("https://api.example.com/v1", &none)
        .unwrap()
        .with_account(Account::new("acct_4f8a1c9e77", &none).unwrap())
}

/// Each kind of price, with the kind of quote it gives and what a roster answers.
fn prices() -> Vec<(Price, QuoteKind, Estimate)> {
    let host = TimeLimit::of_host(60_000).unwrap();
    let own = TimeLimit::own_timeout(60_000).unwrap();
    vec![
        (Price::free(), QuoteKind::Free, Estimate::of_micros(0)),
        (
            Price::at_most(500.0).unwrap(),
            QuoteKind::Bound,
            Estimate::of_micros(500),
        ),
        (
            Price::about(500.0).unwrap(),
            QuoteKind::EstimateOnly,
            Estimate::unbounded(),
        ),
        (
            Price::by_time(20.0, host).unwrap(),
            QuoteKind::Bound,
            Estimate::of_micros(1200),
        ),
        (
            Price::by_time(20.0, own).unwrap(),
            QuoteKind::EstimateOnly,
            Estimate::unbounded(),
        ),
    ]
}

/// One request to each fake, through its handle; what each came to as a charge or a
/// fault.
fn ask_each(
    filler: &Arc<FakeFiller>,
    segmenter: &Arc<FakeSegmenter>,
    judge: &Arc<FakeJudge>,
) -> [Result<Option<Charge>, Fault>; 3] {
    let run = Cancellation::new;
    [
        FillHandle::new(filler.clone(), run())
            .fill(&fill_request())
            .map(|painted| painted.charge()),
        SegmentHandle::new(segmenter.clone(), run())
            .segment(&segment_request())
            .map(|response| response.charge()),
        JudgeHandle::new(judge.clone(), run())
            .send(&judge_request())
            .map(|turn| turn.charge()),
    ]
}

#[test]
fn a_fake_built_with_each_kind_of_price_quotes_it_for_a_request() {
    for (price, kind, bound) in prices() {
        let filler = FakeFiller::new(Paint::Inverse).priced(price);
        let segmenter = FakeSegmenter::new().priced(price);
        let judge = judge(1).priced(price);
        let declarations: [&Declaration; 3] = [
            Filler::declaration(&filler),
            Segmenter::declaration(&segmenter),
            Judge::declaration(&judge),
        ];
        for declaration in declarations {
            assert_eq!(declaration.price(), Some(&price));
            assert_eq!(declaration.quote().map(|quote| quote.kind()), Some(kind));
        }
        // And the roster a build makes of the fakes answers the bound from it.
        let roster = roster(Some(&segmenter), Some(&filler), Some(&judge));
        let asked = [
            (Capability::Fill, declarations[0]),
            (Capability::Segment, declarations[1]),
            (Capability::Judge, declarations[2]),
        ];
        for (capability, declaration) in asked {
            assert_eq!(roster.bound(capability, declaration.model()), bound);
        }
    }
    // A fake that was given no price states none, as before.
    assert_eq!(
        Filler::declaration(&FakeFiller::new(Paint::Inverse)).price(),
        None
    );
    assert_eq!(Segmenter::declaration(&FakeSegmenter::new()).price(), None);
    assert_eq!(Judge::declaration(&judge(0)).price(), None);
}

#[test]
fn a_price_and_a_recipient_stay_when_a_fake_is_told_what_else_to_declare() {
    let price = Price::at_most(500.0).unwrap();
    let filler = FakeFiller::new(Paint::Inverse)
        .priced(price)
        .sending_to(recipient())
        .declaring(&[]);
    let segmenter = FakeSegmenter::new()
        .priced(price)
        .sending_to(recipient())
        .declaring(&["phrase"]);
    let judge = judge(0)
        .priced(price)
        .sending_to(recipient())
        .declaring(&[])
        .as_model("second-judge");
    let declarations: [&Declaration; 3] = [
        Filler::declaration(&filler),
        Segmenter::declaration(&segmenter),
        Judge::declaration(&judge),
    ];
    for declaration in declarations {
        assert_eq!(declaration.price(), Some(&price));
        assert_eq!(declaration.recipient(), Some(&recipient()));
    }
    assert_eq!(declarations[2].model().model(), "second-judge");
}

#[test]
fn a_fake_with_a_recipient_names_it_and_one_without_is_local() {
    let filler = FakeFiller::new(Paint::Inverse).sending_to(recipient());
    let segmenter = FakeSegmenter::new().sending_to(recipient());
    let remote = judge(0).sending_to(recipient());
    for declaration in [
        Filler::declaration(&filler),
        Segmenter::declaration(&segmenter),
        Judge::declaration(&remote),
    ] {
        assert!(!declaration.is_local());
        assert_eq!(declaration.recipient(), Some(&recipient()));
        assert_eq!(
            declaration.recipient().map(|to| to.origin().clone()),
            Some(Origin::of("https://api.example.com", &Secrets::none()).unwrap())
        );
    }
    // With a recipient and no price a fake has no bound; saying it is free gives one.
    let roster = roster(None, Some(&filler), None);
    let model = Filler::declaration(&filler).model();
    assert!(roster.bound(Capability::Fill, model).is_unbounded());
    let free = FakeFiller::new(Paint::Inverse)
        .sending_to(recipient())
        .priced(Price::free());
    let roster = whizbang_testkit::capabilities::roster(None, Some(&free), None);
    assert_eq!(
        roster.bound(Capability::Fill, model),
        Estimate::of_micros(0)
    );
    assert!(Filler::declaration(&FakeFiller::new(Paint::Inverse)).is_local());
    assert!(Segmenter::declaration(&FakeSegmenter::new()).is_local());
    assert!(Judge::declaration(&judge(0)).is_local());
}

#[test]
fn a_fake_reports_the_charge_it_was_told_to_under_its_bound_over_it_or_none() {
    // A bound of 500: a charge under it, one over it, and none at all.
    let price = Price::at_most(500.0).unwrap();
    for charged in [Some(120), Some(900), None] {
        let mut filler = FakeFiller::new(Paint::Inverse).priced(price);
        let mut segmenter = FakeSegmenter::new().priced(price);
        let mut judge = judge(2).priced(price);
        if let Some(micros) = charged {
            filler = filler.charging(micros);
            segmenter = segmenter.charging(micros);
            judge = judge.charging(micros);
        }
        let (filler, segmenter, judge) = (Arc::new(filler), Arc::new(segmenter), Arc::new(judge));
        let expected = charged.map(|micros| Charge::whole(micros).unwrap());
        for answer in ask_each(&filler, &segmenter, &judge) {
            assert_eq!(answer, Ok(expected), "{charged:?}");
        }
        // Each answer of a script is charged, not the first alone.
        let second = JudgeHandle::new(judge.clone(), Cancellation::new())
            .send(&judge_request())
            .unwrap();
        assert_eq!(second.charge(), expected);
    }
}

#[test]
fn a_fake_counts_a_request_once_whether_it_answers_or_fails() {
    let price = Price::at_most(500.0).unwrap();
    let answering = (
        Arc::new(FakeFiller::new(Paint::Inverse).priced(price).charging(7)),
        Arc::new(FakeSegmenter::new().priced(price).charging(7)),
        Arc::new(judge(1).priced(price).charging(7)),
    );
    let failing = (
        Arc::new(
            FakeFiller::new(Paint::Inverse)
                .priced(price)
                .failing_at(Stage::Unresolved),
        ),
        Arc::new(
            FakeSegmenter::new()
                .priced(price)
                .failing_at(Stage::Unresolved),
        ),
        Arc::new(judge(1).priced(price).failing_at(Stage::Unresolved)),
    );
    for (fakes, answers) in [(&answering, true), (&failing, false)] {
        let (filler, segmenter, judge) = fakes;
        for answer in ask_each(filler, segmenter, judge) {
            assert_eq!(answer.is_ok(), answers);
        }
        assert_eq!(
            (filler.requests(), segmenter.requests(), judge.requests()),
            (1, 1, 1)
        );
    }
    // A judge that fails keeps the request and plays no entry of its script.
    assert_eq!(failing.2.remaining(), 1);
    assert_eq!(failing.2.received().len(), 1);
}

#[test]
fn a_fake_told_to_fail_at_each_stage_gives_a_fault_that_says_that_stage() {
    for stage in Stage::ALL {
        let filler = Arc::new(
            FakeFiller::new(Paint::Inverse)
                .charging(40)
                .failing_at(stage),
        );
        let segmenter = Arc::new(FakeSegmenter::new().charging(40).failing_at(stage));
        let judge = Arc::new(judge(1).charging(40).failing_at(stage));
        for answer in ask_each(&filler, &segmenter, &judge) {
            let fault = answer.unwrap_err();
            assert_eq!(fault.stage(), stage);
            assert_eq!(fault.kind(), FaultKind::Unavailable);
            assert_eq!(
                fault.sentence(),
                format!("the fake was told to fail, {}", stage.name())
            );
            // Only an answer says what it cost: the charge the fake was told to report.
            let charge = (stage == Stage::Answered).then(|| Charge::of_micros(40.0).unwrap());
            assert_eq!(fault.charge(), charge);
        }
    }
    // Answered, by a fake that reports no charge, is an answer that says none.
    let silent = Arc::new(FakeFiller::new(Paint::Inverse).failing_at(Stage::Answered));
    let fault = FillHandle::new(silent, Cancellation::new())
        .fill(&fill_request())
        .unwrap_err();
    assert_eq!((fault.stage(), fault.charge()), (Stage::Answered, None));
}

#[test]
fn a_priced_fake_stopped_at_its_barrier_holds_its_request_open_and_sees_the_cancellation() {
    // What the fakes of 1A did at a barrier is unchanged by a price, a charge or a
    // recipient: the request is counted, then held; let go, it is answered with its
    // charge; cancelled meanwhile, it sees so and stops, and what it says of how far
    // the request got is that nothing settles it.
    let barriers = Barriers::new();
    let filler = Arc::new(
        FakeFiller::new(Paint::Inverse)
            .priced(Price::at_most(500.0).unwrap())
            .sending_to(recipient())
            .charging(120)
            .stopping_at(&barriers, "fill"),
    );
    for (turn, cancel) in [(1, false), (2, true)] {
        let run = Cancellation::new();
        let answer = whizbang_testkit::threads::scope(|scope| {
            let handle = FillHandle::new(filler.clone(), run.clone());
            let asking = scope.spawn(move || handle.fill(&fill_request()));
            barriers.arrived("fill", turn);
            assert_eq!(filler.requests(), turn);
            assert!(!asking.is_finished());
            if cancel {
                run.cancel();
            }
            barriers.release("fill");
            asking.join().unwrap()
        });
        let seen = filler.observed();
        assert!(!seen[turn as usize - 1].cancelled_on_arrival());
        assert_eq!(seen[turn as usize - 1].cancelled_on_release(), cancel);
        match answer {
            Ok(painted) => {
                assert!(!cancel);
                assert_eq!(painted.charge(), Some(Charge::of_micros(120.0).unwrap()));
            }
            Err(fault) => {
                assert!(cancel);
                assert_eq!(
                    (fault.kind(), fault.stage(), fault.charge()),
                    (FaultKind::Cancelled, Stage::Unresolved, None)
                );
            }
        }
    }
}

#[test]
fn a_fake_reports_a_charge_past_what_a_float_holds_exactly() {
    let micros = (1_u64 << 53) + 1;
    let filler = Arc::new(FakeFiller::new(Paint::Inverse).charging(micros));
    let painted = FillHandle::new(filler, Cancellation::new())
        .fill(&fill_request())
        .unwrap();
    assert_eq!(painted.charge().map(|charge| charge.micros()), Some(micros));
}

#[test]
fn a_judge_past_the_end_of_its_script_says_that_nothing_settles_the_request() {
    // The fakes state the stage of the faults they make, and do not leave it to be
    // read: here, and for a request cancelled while it was held open (above).
    let judge = Arc::new(judge(0));
    let fault = JudgeHandle::new(judge.clone(), Cancellation::new())
        .send(&judge_request())
        .unwrap_err();
    assert_eq!(
        (fault.kind(), fault.stage(), fault.charge()),
        (FaultKind::Malformed, Stage::Unresolved, None)
    );
    assert_eq!(
        fault,
        Fault::new(
            FaultKind::Malformed,
            "the scripted transcript has no turn left"
        )
        .unresolved()
    );
    assert_eq!(judge.requests(), 1);
}
