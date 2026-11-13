//! The fake capabilities: a segmenter, a filler that can repaint the whole image, and a
//! judge that plays a scripted transcript. Each counts the requests it was sent.

use std::sync::Arc;

use serde_json::json;
use whizbang_core::capability::{
    Asked, Controls, Fault, FaultKind, FillHandle, FillRequest, Filler, Judge, JudgeHandle,
    JudgeRequest, Message, Opaque, Part, Picture, Query, Role, SegmentHandle, SegmentRequest,
    Segmenter, StopReason, Support, ToolCall, Turn, feature,
};
use whizbang_core::contract::Cancellation;
use whizbang_core::identity::CanonicalPixels;
use whizbang_testkit::capabilities::{FakeFiller, FakeJudge, FakeSegmenter, Paint};

const WIDTH: u32 = 6;
const HEIGHT: u32 = 4;
const PIXELS: usize = (WIDTH * HEIGHT) as usize;

fn picture() -> Picture {
    let mut rgba = Vec::new();
    for at in 0..PIXELS as u32 {
        rgba.extend([
            (at * 9) as u8,
            (at * 7 + 3) as u8,
            (200 - at) as u8,
            255 - at as u8,
        ]);
    }
    Picture::new(CanonicalPixels::from_rgba(WIDTH, HEIGHT, rgba).unwrap()).unwrap()
}

/// A request to paint the first two rows.
fn fill_request(prompt: Option<&str>) -> FillRequest {
    let mut plane = vec![0; PIXELS];
    plane[..2 * WIDTH as usize].fill(255);
    let support = Support::from_plane(WIDTH, HEIGHT, plane).unwrap();
    FillRequest::new(picture(), support, prompt).unwrap()
}

fn now() -> Cancellation {
    Cancellation::new()
}

// --- the filler -----------------------------------------------------------------------------

#[test]
fn the_fake_filler_can_repaint_every_pixel_of_the_image() {
    let filler = Arc::new(FakeFiller::new(Paint::Inverse));
    let request = fill_request(None);
    let painted = FillHandle::new(filler, now()).fill(&request).unwrap();
    assert_eq!((painted.width(), painted.height()), (WIDTH, HEIGHT));
    // Every channel of every pixel is 255 less what it was, inside the support and
    // outside it alike: no pixel is left as it was.
    for (at, pixel) in request.image().rgba().chunks(4).enumerate() {
        let got = &painted.rgb()[at * 3..at * 3 + 3];
        assert_eq!(got, [255 - pixel[0], 255 - pixel[1], 255 - pixel[2]]);
        assert_ne!(got, &pixel[..3], "pixel {at} was not repainted");
    }
}

#[test]
fn the_fake_filler_paints_one_colour_everywhere_or_in_the_support_alone() {
    let request = fill_request(None);
    let flat = FillHandle::new(Arc::new(FakeFiller::new(Paint::Flat([1, 2, 3]))), now())
        .fill(&request)
        .unwrap();
    assert!(flat.rgb().chunks(3).all(|pixel| pixel == [1, 2, 3]));

    let inside = FillHandle::new(Arc::new(FakeFiller::new(Paint::Support([1, 2, 3]))), now())
        .fill(&request)
        .unwrap();
    let support = request.support().plane();
    for (at, pixel) in request.image().rgba().chunks(4).enumerate() {
        let got = &inside.rgb()[at * 3..at * 3 + 3];
        if support[at] == Support::PAINT {
            assert_eq!(got, [1, 2, 3]);
        } else {
            assert_eq!(got, &pixel[..3]);
        }
    }
}

#[test]
fn the_fake_filler_counts_the_requests_it_was_sent() {
    let filler = Arc::new(FakeFiller::new(Paint::Inverse));
    assert_eq!(filler.requests(), 0);
    let handle = FillHandle::new(filler.clone(), now());
    handle.fill(&fill_request(None)).unwrap();
    handle.fill(&fill_request(Some("grass"))).unwrap();
    assert_eq!(filler.requests(), 2);
    // A request the handle did not send is not counted: a handle bound to a run that
    // was cancelled sends nothing.
    let cancelled = Cancellation::new();
    cancelled.cancel();
    FillHandle::new(filler.clone(), cancelled)
        .fill(&fill_request(None))
        .unwrap_err();
    assert_eq!(filler.requests(), 2);
    // One that was sent and failed is.
    let failing = Arc::new(FakeFiller::new(Paint::Fails(Fault::new(
        FaultKind::Unavailable,
        "overloaded",
    ))));
    let fault = FillHandle::new(failing.clone(), now())
        .fill(&fill_request(None))
        .unwrap_err();
    assert_eq!(fault.kind(), FaultKind::Unavailable);
    assert_eq!(failing.requests(), 1);
}

#[test]
fn the_fake_filler_can_answer_in_another_size_and_declare_no_prompt() {
    let wrong = Arc::new(FakeFiller::new(Paint::Size(WIDTH + 1, HEIGHT)));
    let fault = FillHandle::new(wrong.clone(), now())
        .fill(&fill_request(None))
        .unwrap_err();
    assert_eq!(fault.kind(), FaultKind::Malformed);
    assert_eq!(wrong.requests(), 1);

    let filler = FakeFiller::new(Paint::Inverse);
    assert_eq!(filler.declaration().model().backend(), "fake");
    assert_eq!(filler.declaration().model().model(), "fake-filler");
    assert!(filler.declaration().has(feature::PROMPT));
    let plain = Arc::new(FakeFiller::new(Paint::Inverse).declaring(&[]));
    let fault = FillHandle::new(plain.clone(), now())
        .fill(&fill_request(Some("grass")))
        .unwrap_err();
    assert_eq!(fault.kind(), FaultKind::Unsupported);
    assert_eq!(plain.requests(), 0);
}

// --- the segmenter --------------------------------------------------------------------------

#[test]
fn the_fake_segmenter_finds_what_it_was_told_to_find() {
    let segmenter = Arc::new(
        FakeSegmenter::new()
            .finding("a dog", [1, 1, 3, 3])
            .finding("a dog", [4, 0, 6, 2]),
    );
    assert_eq!(segmenter.declaration().model().model(), "fake-segmenter");
    let handle = SegmentHandle::new(segmenter.clone(), now());
    let queries = vec![
        Query::phrase("a dog").unwrap(),
        Query::phrase("a cat").unwrap(),
        Query::point(2.5, 1.2).unwrap(),
        Query::boxed(0.5, 1.0, 2.0, 2.5).unwrap(),
    ];
    let request = SegmentRequest::new(picture(), queries, 8).unwrap();
    let response = handle.segment(&request).unwrap();
    let answers = response.answers();
    assert_eq!(answers.len(), 4);

    // A phrase: the boxes it was told of, in order, the first with the better score.
    assert_eq!(answers[0].len(), 2);
    assert_eq!(answers[0][0].bounds(), [1.0, 1.0, 3.0, 3.0]);
    assert_eq!(answers[0][1].bounds(), [4.0, 0.0, 6.0, 2.0]);
    assert!(answers[0][0].score() > answers[0][1].score());
    let on: Vec<usize> = (0..PIXELS)
        .filter(|at| answers[0][0].plane()[*at] == 255)
        .collect();
    assert_eq!(on, [7, 8, 13, 14]);
    // A phrase it was told nothing of: no instance.
    assert!(answers[1].is_empty());
    // A point: the pixel under it.
    assert_eq!(answers[2].len(), 1);
    assert_eq!(answers[2][0].bounds(), [2.0, 1.0, 3.0, 2.0]);
    // A box: the whole pixels it touches.
    assert_eq!(answers[3][0].bounds(), [0.0, 1.0, 2.0, 3.0]);
    assert!(matches!(request.queries()[3].asked(), Asked::Box { .. }));
}

#[test]
fn the_fake_segmenter_counts_requests_and_keeps_to_the_most_asked_for() {
    let segmenter = Arc::new(
        FakeSegmenter::new()
            .finding("a dog", [1, 1, 3, 3])
            .finding("a dog", [4, 0, 6, 2]),
    );
    let handle = SegmentHandle::new(segmenter.clone(), now());
    assert_eq!(segmenter.requests(), 0);
    let one = SegmentRequest::new(picture(), vec![Query::phrase("a dog").unwrap()], 1).unwrap();
    let response = handle.segment(&one).unwrap();
    assert_eq!(response.answers()[0].len(), 1);
    handle.segment(&one).unwrap();
    // A batch is one request, however many it asks.
    assert_eq!(segmenter.requests(), 2);

    // Declaring phrases alone, it is not sent a point.
    let phrases = Arc::new(FakeSegmenter::new().declaring(&[feature::PHRASE]));
    let point = SegmentRequest::new(picture(), vec![Query::point(1.0, 1.0).unwrap()], 1).unwrap();
    let fault = SegmentHandle::new(phrases.clone(), now())
        .segment(&point)
        .unwrap_err();
    assert_eq!(fault.kind(), FaultKind::Unsupported);
    assert_eq!(phrases.requests(), 0);
    // A box it was told of that is not in the image is the script's mistake, and is
    // said as a fault, not a panic.
    let outside = Arc::new(FakeSegmenter::new().finding("a dog", [0, 0, WIDTH + 1, 1]));
    let fault = SegmentHandle::new(outside, now())
        .segment(&one)
        .unwrap_err();
    assert_eq!(fault.kind(), FaultKind::Malformed);
}

// --- the judge --------------------------------------------------------------------------------

fn text(words: &str) -> Part {
    Part::Text(words.to_owned())
}

fn said(parts: Vec<Part>, stop: StopReason) -> Turn {
    Turn::new(parts, stop, None, None).unwrap()
}

fn asking(words: &str) -> JudgeRequest {
    let user = Message::new(Role::User, vec![text(words)]).unwrap();
    JudgeRequest::new("", vec![user], Vec::new(), Controls::new(256).unwrap()).unwrap()
}

#[test]
fn the_fake_judge_plays_its_transcript_in_order_and_keeps_what_it_was_sent() {
    // A transcript with what a real provider sends: an opaque part, two calls in one
    // turn, a failure, a turn cut off, and a verdict.
    let opaque = Opaque::new(FakeJudge::ADAPTER, "{\"thinking\": \"...\"}").unwrap();
    let call = |id: &str| Part::ToolCall(ToolCall::new(id, "view", json!({})).unwrap());
    let calls = said(
        vec![Part::Opaque(opaque), call("a"), call("b")],
        StopReason::ToolUse,
    );
    let cut = said(vec![text("The sky")], StopReason::MaxTokens);
    let verdict = said(vec![text("yes")], StopReason::EndTurn);
    let overloaded = Fault::new(FaultKind::Unavailable, "overloaded");
    let judge = Arc::new(FakeJudge::scripted(vec![
        Ok(calls.clone()),
        Err(overloaded.clone()),
        Ok(cut.clone()),
        Ok(verdict.clone()),
    ]));
    assert_eq!(judge.declaration().model().backend(), FakeJudge::ADAPTER);
    assert_eq!(judge.remaining(), 4);
    let handle = JudgeHandle::new(judge.clone(), now());

    assert_eq!(handle.send(&asking("one")).unwrap(), calls);
    assert_eq!(handle.send(&asking("two")).unwrap_err(), overloaded);
    assert_eq!(handle.send(&asking("three")).unwrap(), cut);
    assert_eq!(handle.send(&asking("four")).unwrap(), verdict);
    assert_eq!(judge.requests(), 4);
    assert_eq!(judge.remaining(), 0);
    assert_eq!(
        judge.received(),
        [
            asking("one"),
            asking("two"),
            asking("three"),
            asking("four")
        ]
    );

    // Asked past the end of its transcript, it fails, and still counts the request.
    let fault = handle.send(&asking("five")).unwrap_err();
    assert_eq!(fault.kind(), FaultKind::Malformed);
    assert_eq!(fault.sentence(), "the scripted transcript has no turn left");
    assert_eq!(judge.requests(), 5);
}

#[test]
fn the_fake_judge_is_not_sent_what_it_does_not_declare() {
    let judge = Arc::new(
        FakeJudge::scripted(vec![Ok(said(vec![text("yes")], StopReason::EndTurn))]).declaring(&[]),
    );
    let image = Message::new(Role::User, vec![Part::Image(picture())]).unwrap();
    let request =
        JudgeRequest::new("", vec![image], Vec::new(), Controls::new(256).unwrap()).unwrap();
    let fault = JudgeHandle::new(judge.clone(), now())
        .send(&request)
        .unwrap_err();
    assert_eq!(fault.kind(), FaultKind::Unsupported);
    assert_eq!(judge.requests(), 0);
    assert_eq!(judge.remaining(), 1);
}
