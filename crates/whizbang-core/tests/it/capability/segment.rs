//! Segment (`SPEC.md` section 4; ADR 0006, "Segment"): one image and a batch of requests,
//! each a phrase, a point or a box; instances with planes, boxes and scores for each.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use whizbang_core::capability::{
    Asked, Declaration, Fault, FaultKind, INSTANCE_LIMIT, Instance, PHRASE_LIMIT, QUERY_LIMIT,
    Query, SegmentHandle, SegmentRequest, SegmentResponse, Segmenter, Unfit, feature,
};
use whizbang_core::contract::{Cancellation, ModelIdentity};

use super::fill::picture;
use crate::artifact::support::{HEIGHT, WIDTH, rect};

const W: f64 = WIDTH as f64;
const H: f64 = HEIGHT as f64;

fn out_of_range(error: Unfit) -> (&'static str, String) {
    match error {
        Unfit::OutOfRange { what, found, .. } => (what, found),
        other => panic!("not out of range: {other:?}"),
    }
}

// --- what is asked ----------------------------------------------------------------------------

#[test]
fn a_query_is_a_phrase_a_point_or_a_box() {
    assert_eq!(
        Query::phrase("a dog").unwrap().asked(),
        Asked::Phrase("a dog")
    );
    assert_eq!(
        Query::point(1.5, 2.0).unwrap().asked(),
        Asked::Point { x: 1.5, y: 2.0 }
    );
    assert_eq!(
        Query::boxed(1.0, 2.0, 3.0, 4.5).unwrap().asked(),
        Asked::Box {
            left: 1.0,
            top: 2.0,
            right: 3.0,
            bottom: 4.5
        }
    );
    assert_eq!(
        Query::phrase("").unwrap_err(),
        Unfit::Empty { what: "the phrase" }
    );
    Query::phrase(&"p".repeat(PHRASE_LIMIT)).unwrap();
    assert!(matches!(
        Query::phrase(&"p".repeat(PHRASE_LIMIT + 1)),
        Err(Unfit::TooLarge { limit: 512, .. })
    ));
    // A coordinate is a number.
    for (x, y) in [(f64::NAN, 1.0), (1.0, f64::INFINITY), (-1.0, 1.0)] {
        assert_eq!(out_of_range(Query::point(x, y).unwrap_err()).0, "the point");
    }
    // A box has area.
    for (l, t, r, b) in [
        (1.0, 1.0, 1.0, 2.0),
        (1.0, 2.0, 3.0, 2.0),
        (3.0, 1.0, 1.0, 2.0),
        (f64::NAN, 1.0, 2.0, 2.0),
        (-0.5, 1.0, 2.0, 2.0),
        (1.0, 1.0, f64::INFINITY, 2.0),
    ] {
        assert_eq!(
            out_of_range(Query::boxed(l, t, r, b).unwrap_err()).0,
            "the box"
        );
    }
}

fn dog() -> Vec<Query> {
    vec![Query::phrase("a dog").unwrap()]
}

#[test]
fn a_request_is_one_image_and_a_bounded_batch_inside_it() {
    let queries = vec![
        Query::phrase("a dog").unwrap(),
        // The edges of the image are inside it.
        Query::point(W, H).unwrap(),
        Query::boxed(0.0, 0.0, W, H).unwrap(),
    ];
    let request = SegmentRequest::new(picture(), queries.clone(), 8).unwrap();
    assert_eq!(request.image(), &picture());
    assert_eq!(request.queries(), queries);
    assert_eq!(request.max_instances(), 8);

    assert_eq!(
        SegmentRequest::new(picture(), Vec::new(), 8).unwrap_err(),
        Unfit::Empty {
            what: "the batch of requests"
        }
    );
    let many = |count: usize| vec![Query::phrase("a dog").unwrap(); count];
    SegmentRequest::new(picture(), many(QUERY_LIMIT), 8).unwrap();
    assert_eq!(
        SegmentRequest::new(picture(), many(QUERY_LIMIT + 1), 8).unwrap_err(),
        Unfit::TooLarge {
            what: "the batch of requests",
            size: 65,
            limit: 64,
        }
    );
    assert!(matches!(
        SegmentRequest::new(picture(), dog(), 0),
        Err(Unfit::OutOfRange { .. })
    ));
    SegmentRequest::new(picture(), dog(), INSTANCE_LIMIT).unwrap();
    assert!(matches!(
        SegmentRequest::new(picture(), dog(), INSTANCE_LIMIT + 1),
        Err(Unfit::OutOfRange { .. })
    ));
    // A point and a box outside the image.
    let outside = vec![Query::point(W + 0.5, 1.0).unwrap()];
    assert_eq!(
        out_of_range(SegmentRequest::new(picture(), outside, 8).unwrap_err()).0,
        "the point"
    );
    let outside = vec![Query::boxed(1.0, 1.0, 2.0, H + 1.0).unwrap()];
    assert_eq!(
        out_of_range(SegmentRequest::new(picture(), outside, 8).unwrap_err()).0,
        "the box"
    );
}

// --- what comes back ----------------------------------------------------------------------------

fn instance() -> Instance {
    Instance::new(WIDTH, HEIGHT, rect(2, 1, 5, 4), [2.0, 1.0, 5.0, 4.0], 0.9).unwrap()
}

#[test]
fn an_instance_is_a_binary_plane_a_box_and_a_score() {
    let instance = instance();
    assert_eq!((instance.width(), instance.height()), (WIDTH, HEIGHT));
    assert_eq!(instance.plane(), rect(2, 1, 5, 4));
    assert_eq!(instance.bounds(), [2.0, 1.0, 5.0, 4.0]);
    assert_eq!(instance.score(), 0.9);

    let with = |plane: Vec<u8>, bounds: [f64; 4], score: f64| {
        Instance::new(WIDTH, HEIGHT, plane, bounds, score).unwrap_err()
    };
    let (plane, bounds) = (rect(2, 1, 5, 4), [2.0, 1.0, 5.0, 4.0]);
    // The plane: its length, its range, and something in it.
    assert!(matches!(
        with(plane[1..].to_vec(), bounds, 0.9),
        Unfit::WrongLength {
            what: "the plane of an instance",
            ..
        }
    ));
    let mut grey = plane.clone();
    grey[0] = 127;
    assert_eq!(
        with(grey, bounds, 0.9),
        Unfit::OutOfRange {
            what: "the plane of an instance",
            at: Some(0),
            found: "127".to_owned(),
            allowed: "0 or 255",
        }
    );
    assert_eq!(
        with(vec![0; plane.len()], bounds, 0.9),
        Unfit::Empty {
            what: "the plane of an instance"
        }
    );
    // The score: a number from 0 to 1.
    for score in [f64::NAN, -0.01, 1.01, f64::INFINITY] {
        assert_eq!(
            out_of_range(with(plane.clone(), bounds, score)),
            ("the score of an instance", score.to_string())
        );
    }
    for score in [0.0, 1.0] {
        Instance::new(WIDTH, HEIGHT, plane.clone(), bounds, score).unwrap();
    }
    // The box: inside the image, with area.
    for bad in [
        [2.0, 1.0, 2.0, 4.0],
        [2.0, 1.0, W + 1.0, 4.0],
        [-1.0, 1.0, 5.0, 4.0],
        [2.0, f64::NAN, 5.0, 4.0],
    ] {
        assert_eq!(
            out_of_range(with(plane.clone(), bad, 0.9)).0,
            "the box of an instance"
        );
    }
    // The size is refused before the plane is looked at.
    assert!(matches!(
        Instance::new(50_000, 1_001, Vec::new(), bounds, 0.9),
        Err(Unfit::TooLarge { .. })
    ));
    assert!(matches!(
        Instance::new(0, 1, Vec::new(), bounds, 0.9),
        Err(Unfit::Empty { .. })
    ));
}

// --- the handle ---------------------------------------------------------------------------------

struct Stub {
    declaration: Declaration,
    answer: Result<SegmentResponse, Fault>,
    sent: AtomicU64,
}

impl Stub {
    fn new(features: &[&str], answers: Vec<Vec<Instance>>) -> Arc<Stub> {
        Arc::new(Stub {
            declaration: Declaration::new(ModelIdentity::new("stub", "segmenter", "1"), features)
                .unwrap(),
            answer: Ok(SegmentResponse::new(answers)),
            sent: AtomicU64::new(0),
        })
    }
}

impl Segmenter for Stub {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn segment(&self, _: &SegmentRequest, _: &Cancellation) -> Result<SegmentResponse, Fault> {
        self.sent.fetch_add(1, Ordering::SeqCst);
        self.answer.clone()
    }
}

const ALL: [&str; 3] = [feature::PHRASE, feature::POINT, feature::BOX];

#[test]
fn the_handle_hands_back_the_instances_of_each_request_in_the_batchs_order() {
    let stub = Stub::new(&ALL, vec![vec![instance(), instance()], Vec::new()]);
    let handle = SegmentHandle::new(stub.clone());
    let queries = vec![
        Query::phrase("a dog").unwrap(),
        Query::point(1.0, 1.0).unwrap(),
    ];
    let request = SegmentRequest::new(picture(), queries, 2).unwrap();
    let response = handle.segment(&request, &Cancellation::new()).unwrap();
    assert_eq!(response.answers().len(), 2);
    assert_eq!(response.answers()[0], [instance(), instance()]);
    assert!(response.answers()[1].is_empty());
    assert_eq!(stub.sent.load(Ordering::SeqCst), 1);
    assert!(format!("{handle:?}").contains("segmenter"));
}

#[test]
fn an_answer_that_does_not_fit_its_request_is_malformed() {
    let ask = |stub: Arc<Stub>, max: u32| {
        let request = SegmentRequest::new(picture(), dog(), max).unwrap();
        let fault = SegmentHandle::new(stub)
            .segment(&request, &Cancellation::new())
            .unwrap_err();
        assert_eq!(fault.kind(), FaultKind::Malformed);
        fault.sentence().to_owned()
    };
    // Not one answer for each request: none, and two for one.
    for answers in [Vec::new(), vec![Vec::new(), Vec::new()]] {
        assert_eq!(
            ask(Stub::new(&ALL, answers), 8),
            "the segmenter's answer does not answer each request once"
        );
    }
    // More instances for a request than were asked for.
    assert_eq!(
        ask(Stub::new(&ALL, vec![vec![instance(), instance()]]), 1),
        "the segmenter's answer holds more instances for a request than were asked for"
    );
    // An instance of another image's size.
    let turned = Instance::new(
        HEIGHT,
        WIDTH,
        vec![255; (WIDTH * HEIGHT) as usize],
        [0.0, 0.0, 1.0, 1.0],
        0.5,
    )
    .unwrap();
    assert_eq!(
        ask(Stub::new(&ALL, vec![vec![turned]]), 8),
        format!("the plane of an instance is {HEIGHT} by {WIDTH}, not {WIDTH} by {HEIGHT}")
    );
}

#[test]
fn each_kind_of_request_needs_its_feature_and_is_not_sent_without_it() {
    let kinds = [
        (feature::PHRASE, Query::phrase("a dog").unwrap()),
        (feature::POINT, Query::point(1.0, 1.0).unwrap()),
        (feature::BOX, Query::boxed(1.0, 1.0, 2.0, 2.0).unwrap()),
    ];
    for (needed, query) in kinds {
        // Every feature but the one this request needs.
        let others: Vec<&str> = ALL.into_iter().filter(|f| *f != needed).collect();
        let stub = Stub::new(&others, vec![Vec::new()]);
        let request = SegmentRequest::new(picture(), vec![query.clone()], 8).unwrap();
        let fault = SegmentHandle::new(stub.clone())
            .segment(&request, &Cancellation::new())
            .unwrap_err();
        assert_eq!(fault.kind(), FaultKind::Unsupported);
        assert_eq!(
            fault.sentence(),
            format!("the backend segmenter does not declare the feature {needed}")
        );
        assert_eq!(stub.sent.load(Ordering::SeqCst), 0);
        // With that feature alone it is sent.
        let stub = Stub::new(&[needed], vec![Vec::new()]);
        let request = SegmentRequest::new(picture(), vec![query], 8).unwrap();
        SegmentHandle::new(stub.clone())
            .segment(&request, &Cancellation::new())
            .unwrap();
        assert_eq!(stub.sent.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn nothing_is_sent_once_the_run_is_cancelled() {
    let stub = Stub::new(&ALL, vec![Vec::new()]);
    let cancellation = Cancellation::new();
    cancellation.cancel();
    let request = SegmentRequest::new(picture(), dog(), 8).unwrap();
    let fault = SegmentHandle::new(stub.clone())
        .segment(&request, &cancellation)
        .unwrap_err();
    assert_eq!(fault.kind(), FaultKind::Cancelled);
    assert_eq!(stub.sent.load(Ordering::SeqCst), 0);
}
