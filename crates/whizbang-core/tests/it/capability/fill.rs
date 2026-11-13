//! Fill (`SPEC.md` section 4; ADR 0006, "Fill"): the support plane's polarity and range,
//! how a mask's planes become it, the layout of the pixels a filler returns, and the
//! handle that holds a request to the declaration and an answer to the request.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use std::time::{Duration, Instant};
use whizbang_core::artifact::{Mask, Source, UseCase};

use whizbang_core::capability::{
    Bounds, Declaration, Fault, FaultKind, FillHandle, FillRequest, Filler, PIXEL_LIMIT,
    PROMPT_LIMIT, Painted, Picture, Support, Unfit, feature,
};
use whizbang_core::contract::{Cancellation, ModelIdentity};
use whizbang_core::identity::CanonicalPixels;

use crate::artifact::support::{HEIGHT, WIDTH, made_by, on, rect};

const PIXELS: usize = (WIDTH * HEIGHT) as usize;

/// A picture of `WIDTH` by `HEIGHT`, every pixel its own colour, opaque.
pub fn picture() -> Picture {
    let mut rgba = Vec::with_capacity(PIXELS * 4);
    for at in 0..PIXELS as u32 {
        rgba.extend([(at * 5) as u8, (at * 3 + 1) as u8, (255 - at) as u8, 255]);
    }
    Picture::new(CanonicalPixels::from_rgba(WIDTH, HEIGHT, rgba).unwrap()).unwrap()
}

fn all(value: u8) -> Vec<u8> {
    vec![value; PIXELS]
}

// --- the support plane ---------------------------------------------------------------------

#[test]
fn the_support_plane_is_255_to_paint_and_0_to_keep() {
    assert_eq!((Support::PAINT, Support::KEEP), (255, 0));
    // A mask with each kind of pixel in its first row: core; core that protect then
    // covered; a blend of 1, the least weight there is; a blend of 255; a blend that
    // protect covered; and nothing.
    let mut mask = Mask::from_core(
        on("photo"),
        UseCase::Remove,
        rect(0, 0, 2, 1),
        Source::default(),
        made_by("test"),
    )
    .unwrap();
    let mut blend = all(0);
    blend[2] = 1;
    blend[3] = 255;
    blend[4] = 200;
    mask.set_blend(blend).unwrap();
    let mut protect = all(0);
    protect[1] = 255;
    protect[4] = 255;
    mask.protect_union(&protect).unwrap();

    let support = Support::of(&mask);
    assert_eq!((support.width(), support.height()), (WIDTH, HEIGHT));
    assert_eq!(&support.plane()[..7], [255, 0, 255, 255, 0, 0, 0]);
    // Everything else is kept: the plane is 255 exactly where the weight is over 0.
    for (at, sample) in support.plane().iter().enumerate() {
        let weight = mask.weight(at as u32 % WIDTH, at as u32 / WIDTH).unwrap();
        assert_eq!(*sample, if weight > 0.0 { 255 } else { 0 }, "sample {at}");
    }
    assert_eq!(support.painted(), 3);
    // The same plane, given as bytes, is the same support.
    assert_eq!(
        Support::from_plane(WIDTH, HEIGHT, support.plane().to_vec()).unwrap(),
        support
    );
}

#[test]
fn a_support_plane_with_a_value_outside_its_range_is_refused() {
    // Every value that is neither 0 nor 255, wherever it is.
    for value in 1..=254u8 {
        let mut plane = all(0);
        plane[5] = 255;
        plane[9] = value;
        assert_eq!(
            Support::from_plane(WIDTH, HEIGHT, plane).unwrap_err(),
            Unfit::OutOfRange {
                what: "the support plane",
                at: Some(9),
                found: value.to_string(),
                allowed: "0 or 255",
            }
        );
    }
    // The first such value is the one named.
    let mut plane = all(255);
    plane[40] = 7;
    plane[3] = 128;
    let error = Support::from_plane(WIDTH, HEIGHT, plane).unwrap_err();
    assert_eq!(
        error.to_string(),
        "the support plane holds 128 at 3: a value is 0 or 255"
    );
}

#[test]
fn a_support_plane_is_as_many_bytes_as_its_image_has_pixels() {
    assert_eq!(
        Support::from_plane(WIDTH, HEIGHT, vec![0; PIXELS - 1]).unwrap_err(),
        Unfit::WrongLength {
            what: "the support plane",
            expected: PIXELS as u64,
            found: PIXELS as u64 - 1,
        }
    );
    // Three bytes a pixel is not a plane.
    assert!(matches!(
        Support::from_plane(WIDTH, HEIGHT, vec![0; PIXELS * 3]),
        Err(Unfit::WrongLength { .. })
    ));
    for (width, height) in [(0, HEIGHT), (WIDTH, 0)] {
        assert_eq!(
            Support::from_plane(width, height, Vec::new()).unwrap_err(),
            Unfit::Empty {
                what: "the support plane"
            }
        );
    }
    // The size is refused before the bytes are looked at: nothing this large need be
    // held to refuse it. Exactly the limit is a size, and what is wrong with no bytes
    // for it is their count; one pixel more is too large.
    assert_eq!(PIXEL_LIMIT, 50_000_000);
    assert_eq!(
        Support::from_plane(50_000, 1_000, Vec::new()).unwrap_err(),
        Unfit::WrongLength {
            what: "the support plane",
            expected: PIXEL_LIMIT,
            found: 0,
        }
    );
    assert_eq!(
        Support::from_plane(50_000_001, 1, Vec::new()).unwrap_err(),
        Unfit::TooLarge {
            what: "the support plane",
            size: PIXEL_LIMIT + 1,
            limit: PIXEL_LIMIT,
        }
    );
    assert!(matches!(
        Support::from_plane(u32::MAX, u32::MAX, Vec::new()),
        Err(Unfit::TooLarge { .. })
    ));
}

// --- the pixels a filler returns ------------------------------------------------------------

#[test]
fn a_fill_is_three_bytes_a_pixel_with_no_alpha() {
    let rgb: Vec<u8> = (0..PIXELS * 3).map(|at| at as u8).collect();
    let painted = Painted::from_rgb(WIDTH, HEIGHT, rgb.clone()).unwrap();
    assert_eq!((painted.width(), painted.height()), (WIDTH, HEIGHT));
    assert_eq!(painted.rgb(), rgb);
    // Four bytes a pixel, a fill that kept an alpha channel, is refused, and so is one.
    assert_eq!(
        Painted::from_rgb(WIDTH, HEIGHT, vec![0; PIXELS * 4]).unwrap_err(),
        Unfit::WrongLength {
            what: "the fill",
            expected: PIXELS as u64 * 3,
            found: PIXELS as u64 * 4,
        }
    );
    assert!(matches!(
        Painted::from_rgb(WIDTH, HEIGHT, vec![0; PIXELS]),
        Err(Unfit::WrongLength { .. })
    ));
    assert_eq!(
        Painted::from_rgb(0, 0, Vec::new()).unwrap_err(),
        Unfit::Empty { what: "the fill" }
    );
    assert_eq!(
        Painted::from_rgb(50_000, 1_000, Vec::new()).unwrap_err(),
        Unfit::WrongLength {
            what: "the fill",
            expected: PIXEL_LIMIT * 3,
            found: 0,
        }
    );
    assert_eq!(
        Painted::from_rgb(1, 50_000_001, Vec::new()).unwrap_err(),
        Unfit::TooLarge {
            what: "the fill",
            size: PIXEL_LIMIT + 1,
            limit: PIXEL_LIMIT,
        }
    );
}

#[test]
fn what_is_printed_of_a_plane_a_fill_or_a_request_is_its_size_and_never_its_bytes() {
    let support = support();
    assert_eq!(
        format!("{support:?}"),
        format!("Support({WIDTH} by {HEIGHT}, 9 to paint)")
    );
    let painted = Painted::from_rgb(WIDTH, HEIGHT, vec![77; PIXELS * 3]).unwrap();
    assert_eq!(
        format!("{painted:?}"),
        format!("Painted({WIDTH} by {HEIGHT})")
    );
    // A request prints its picture's digest, its support's size, and how long its
    // prompt is: not the prompt.
    let request = FillRequest::new(picture(), support, Some("a secret garden")).unwrap();
    let printed = format!("{request:?}");
    assert!(printed.contains("Support(8 by 6, 9 to paint)"), "{printed}");
    assert!(printed.contains("a prompt of 15 bytes"), "{printed}");
    assert!(!printed.contains("garden"), "{printed}");
    assert!(!printed.contains("255, 255"), "{printed}");
    let request = FillRequest::new(picture(), self::support(), None).unwrap();
    assert!(format!("{request:?}").contains("no prompt"));
}

// --- the request ------------------------------------------------------------------------------

fn support() -> Support {
    Support::from_plane(WIDTH, HEIGHT, rect(2, 1, 5, 4)).unwrap()
}

#[test]
fn a_request_is_an_image_a_support_plane_of_its_size_and_maybe_a_prompt() {
    let request = FillRequest::new(picture(), support(), Some("grass")).unwrap();
    assert_eq!(request.image(), &picture());
    assert_eq!(request.support(), &support());
    assert_eq!(request.prompt(), Some("grass"));
    assert_eq!(
        FillRequest::new(picture(), support(), None)
            .unwrap()
            .prompt(),
        None
    );

    // A plane of as many samples and another shape.
    let turned = Support::from_plane(HEIGHT, WIDTH, all(255)).unwrap();
    assert_eq!(
        FillRequest::new(picture(), turned, None).unwrap_err(),
        Unfit::WrongSize {
            what: "the support plane",
            expected: (WIDTH, HEIGHT),
            found: (HEIGHT, WIDTH),
        }
    );
    // Nothing to paint is nothing to ask.
    let nothing = Support::from_plane(WIDTH, HEIGHT, all(0)).unwrap();
    assert_eq!(
        FillRequest::new(picture(), nothing, None).unwrap_err(),
        Unfit::Empty {
            what: "the area to paint"
        }
    );
    assert_eq!(
        FillRequest::new(picture(), support(), Some("")).unwrap_err(),
        Unfit::Empty { what: "the prompt" }
    );
    FillRequest::new(picture(), support(), Some(&"p".repeat(PROMPT_LIMIT))).unwrap();
    assert_eq!(
        FillRequest::new(picture(), support(), Some(&"p".repeat(PROMPT_LIMIT + 1))).unwrap_err(),
        Unfit::TooLarge {
            what: "the prompt, in bytes,",
            size: PROMPT_LIMIT as u64 + 1,
            limit: PROMPT_LIMIT as u64,
        }
    );
}

// --- the handle ---------------------------------------------------------------------------------

/// A filler that answers with what it was built to answer, and counts what it was sent.
struct Stub {
    declaration: Declaration,
    answer: Result<Painted, Fault>,
    sent: AtomicU64,
    /// What the last request's bounds said: cancelled or not, and the deadline.
    seen: Mutex<Option<(bool, Option<Instant>)>>,
}

impl Stub {
    fn new(features: &[&str], answer: Result<Painted, Fault>) -> Arc<Stub> {
        Arc::new(Stub {
            declaration: Declaration::new(ModelIdentity::new("stub", "filler", "1"), features)
                .unwrap(),
            answer,
            sent: AtomicU64::new(0),
            seen: Mutex::new(None),
        })
    }
}

impl Filler for Stub {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn fill(&self, _: &FillRequest, bounds: &Bounds) -> Result<Painted, Fault> {
        self.sent.fetch_add(1, Ordering::SeqCst);
        *self.seen.lock().unwrap() = Some((bounds.is_cancelled(), bounds.deadline()));
        self.answer.clone()
    }
}

fn grey(width: u32, height: u32) -> Painted {
    Painted::from_rgb(width, height, vec![128; (width * height * 3) as usize]).unwrap()
}

#[test]
fn the_handle_sends_a_request_and_hands_back_a_fill_of_its_size() {
    let stub = Stub::new(&[feature::PROMPT], Ok(grey(WIDTH, HEIGHT)));
    let handle = FillHandle::new(stub.clone(), Cancellation::new());
    assert_eq!(handle.declaration().model().model(), "filler");
    let request = FillRequest::new(picture(), support(), Some("grass")).unwrap();
    let painted = handle.fill(&request).unwrap();
    assert_eq!(painted, grey(WIDTH, HEIGHT));
    assert_eq!(stub.sent.load(Ordering::SeqCst), 1);
    // The backend was handed the run's bounds: not cancelled, and no deadline.
    assert_eq!(*stub.seen.lock().unwrap(), Some((false, None)));
    // What `{:?}` prints of a handle is its declaration.
    assert!(format!("{handle:?}").contains("filler"));
}

#[test]
fn a_fill_of_other_dimensions_is_a_malformed_answer() {
    for (width, height) in [(HEIGHT, WIDTH), (WIDTH, HEIGHT + 1), (1, 1)] {
        let stub = Stub::new(&[], Ok(grey(width, height)));
        let request = FillRequest::new(picture(), support(), None).unwrap();
        let fault = FillHandle::new(stub.clone(), Cancellation::new())
            .fill(&request)
            .unwrap_err();
        assert_eq!(fault.kind(), FaultKind::Malformed);
        assert_eq!(
            fault.sentence(),
            format!("the fill is {width} by {height}, not {WIDTH} by {HEIGHT}")
        );
        assert_eq!(stub.sent.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn a_prompt_is_not_sent_to_a_filler_that_declares_none() {
    let stub = Stub::new(&[], Ok(grey(WIDTH, HEIGHT)));
    let handle = FillHandle::new(stub.clone(), Cancellation::new());
    // Any prompt needs the feature, a prompt of one byte too.
    for prompt in ["grass", "g"] {
        let prompted = FillRequest::new(picture(), support(), Some(prompt)).unwrap();
        let fault = handle.fill(&prompted).unwrap_err();
        assert_eq!(fault.kind(), FaultKind::Unsupported);
        assert_eq!(
            fault.sentence(),
            "the model filler of the backend stub does not declare the feature prompt"
        );
        assert_eq!(stub.sent.load(Ordering::SeqCst), 0);
    }
    // Without a prompt the same filler is asked.
    let plain = FillRequest::new(picture(), support(), None).unwrap();
    handle.fill(&plain).unwrap();
    assert_eq!(stub.sent.load(Ordering::SeqCst), 1);
}

#[test]
fn a_handle_is_bound_to_its_runs_cancellation_and_sends_nothing_once_it_is_set() {
    // The action hands the handle no signal: the handle holds the run's, from the
    // moment it was made. Cancelled before the handle was made, or after, nothing is
    // sent.
    let request = FillRequest::new(picture(), support(), None).unwrap();
    let cancelled = Cancellation::new();
    cancelled.cancel();
    let stub = Stub::new(&[], Ok(grey(WIDTH, HEIGHT)));
    let fault = FillHandle::new(stub.clone(), cancelled)
        .fill(&request)
        .unwrap_err();
    assert_eq!(fault.kind(), FaultKind::Cancelled);
    assert_eq!(stub.sent.load(Ordering::SeqCst), 0);

    let run = Cancellation::new();
    let handle = FillHandle::new(stub.clone(), run.clone());
    handle.fill(&request).unwrap();
    assert_eq!(stub.sent.load(Ordering::SeqCst), 1);
    run.cancel();
    assert_eq!(
        handle.fill(&request).unwrap_err().kind(),
        FaultKind::Cancelled
    );
    // A clone of the handle is bound to the same run.
    assert_eq!(
        handle.clone().fill(&request).unwrap_err().kind(),
        FaultKind::Cancelled
    );
    assert_eq!(stub.sent.load(Ordering::SeqCst), 1);
}

#[test]
fn a_handle_hands_the_backend_its_deadline_and_sends_nothing_once_it_has_passed() {
    let request = FillRequest::new(picture(), support(), None).unwrap();
    let stub = Stub::new(&[], Ok(grey(WIDTH, HEIGHT)));
    let later = Instant::now() + Duration::from_secs(3600);
    FillHandle::new(stub.clone(), Cancellation::new())
        .until(later)
        .fill(&request)
        .unwrap();
    assert_eq!(*stub.seen.lock().unwrap(), Some((false, Some(later))));

    let passed = Instant::now()
        .checked_sub(Duration::from_millis(1))
        .unwrap();
    let fault = FillHandle::new(stub.clone(), Cancellation::new())
        .until(passed)
        .fill(&request)
        .unwrap_err();
    assert_eq!(fault.kind(), FaultKind::TimedOut);
    assert_eq!(
        fault.sentence(),
        "the deadline of the run passed before the request was sent"
    );
    assert_eq!(stub.sent.load(Ordering::SeqCst), 1);
}

#[test]
fn a_fillers_fault_is_handed_back_as_it_is() {
    let fault = Fault::new(FaultKind::Unavailable, "overloaded").retry_after_ms(250);
    let stub = Stub::new(&[], Err(fault.clone()));
    let request = FillRequest::new(picture(), support(), None).unwrap();
    assert_eq!(
        FillHandle::new(stub, Cancellation::new())
            .fill(&request)
            .unwrap_err(),
        fault
    );
}
