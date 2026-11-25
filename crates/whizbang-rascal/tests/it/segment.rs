//! `segment` as an action, run through the executor against a store, over the test
//! kit's fake segmenter: one mask for each instance the segmenter answers, with each
//! score and box in the run's diagnostic; no instance is a run that completes with no
//! mask; and a segmenter that cannot take a phrase is refused before any run exists.

use std::sync::Arc;

use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use whizbang_core::artifact::{
    self, Artifact, ArtifactId, ArtifactType, Image, ImageRole, Mask, Origin, Step, UseCase,
};
use whizbang_core::capability::feature;
use whizbang_core::contract::{
    Call, CallError, Capabilities, Engine, Executor, Outcome, Readiness, Refusal, Registry, Value,
};
use whizbang_core::identity::{DecodeLimits, Digest, Projection, Topology};
use whizbang_rascal::segment::{DIAGNOSTIC, NAME, Segment, VERSION};
use whizbang_testkit::capabilities::{FakeSegmenter, roster};

use super::grow_action::{World, held};

const WIDTH: u32 = 24;
const HEIGHT: u32 = 16;

/// A picture of 24 by 16 pixels, painted in whole numbers, as an image artifact.
fn picture() -> Image {
    let mut rgba = Vec::new();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            rgba.extend_from_slice(&[(x * 10) as u8, (y * 15) as u8, 90, 255]);
        }
    }
    let mut file = Vec::new();
    PngEncoder::new(&mut file)
        .write_image(&rgba, WIDTH, HEIGHT, ExtendedColorType::Rgba8)
        .unwrap();
    let origin = Origin::new(Step::new("fixture", "1")).unwrap();
    let limits = DecodeLimits::default();
    let (flat, bounded) = (Projection::Flat, Topology::Bounded);
    Image::new(file, flat, bounded, ImageRole::Picture, origin, &limits).unwrap()
}

/// The store with the picture in it, as `whizbang store import` brings one in, and its
/// identity.
fn with_picture(world: &World) -> (Image, ArtifactId) {
    let image = picture();
    let store = world.store();
    let mut publication = store.begin().unwrap();
    let id = artifact::put(&mut publication, &Artifact::from(image.clone())).unwrap();
    publication.publish().unwrap();
    (image, id)
}

/// The executor over the world's store with `segment` as the build ships it, and this
/// segmenter as the capability's default.
fn engine(world: &World, segmenter: &Arc<FakeSegmenter>) -> Engine {
    let mut registry = Registry::new();
    registry.register(Arc::new(Segment::new())).unwrap();
    let backends = roster(Some(segmenter.as_ref()), None, None);
    Engine::over(
        registry,
        Arc::new(backends),
        world.store(),
        Digest::of_bytes(b""),
    )
    .with_capabilities(Capabilities::none().with_segment(segmenter.clone()))
}

fn call(image: ArtifactId, phrase: &str) -> Call {
    Call::new(NAME)
        .argument("query", Value::text(phrase))
        .input("image", image)
}

/// A call that asks a query of this kind.
fn asking(image: ArtifactId, kind: &str, query: &str) -> Call {
    call(image, query).argument("kind", Value::text(kind))
}

/// The masks a finished run committed, in the order of its outputs.
fn masks_of(world: &World, outputs: &[whizbang_core::contract::Committed]) -> Vec<Mask> {
    let limits = DecodeLimits::default();
    outputs
        .iter()
        .map(|output| {
            let store = world.store();
            artifact::get(store.as_ref(), ArtifactType::Mask, output.id(), &limits)
                .unwrap()
                .into_mask()
                .unwrap()
        })
        .collect()
}

/// The plane of a box of whole pixels: left, top, right, bottom.
fn plane_of(bounds: [u32; 4]) -> Vec<u8> {
    (0..WIDTH * HEIGHT)
        .map(|at| {
            let (x, y) = (at % WIDTH, at / WIDTH);
            let inside = (bounds[0]..bounds[2]).contains(&x) && (bounds[1]..bounds[3]).contains(&y);
            if inside { 255 } else { 0 }
        })
        .collect()
}

#[test]
fn a_phrase_answered_with_three_instances_writes_three_masks() {
    let world = World::new();
    let (image, id) = with_picture(&world);
    let boxes = [[1, 1, 5, 4], [10, 2, 14, 9], [18, 10, 24, 16]];
    let segmenter = Arc::new(
        FakeSegmenter::new()
            .finding("a cat", boxes[0])
            .finding("a cat", boxes[1])
            .finding("a cat", boxes[2]),
    );
    let finished = engine(&world, &segmenter)
        .submit(&call(id, "a cat"))
        .unwrap();
    let Outcome::Completed { outputs } = finished.outcome() else {
        panic!("{:?}", finished.outcome());
    };
    assert_eq!(segmenter.requests(), 1);
    // Three masks, each at the port `mask`, in the order the segmenter answered.
    assert_eq!(outputs.len(), 3);
    let limits = DecodeLimits::default();
    let masks: Vec<Mask> = outputs
        .iter()
        .map(|output| {
            assert_eq!(
                (output.port(), output.artifact()),
                ("mask", ArtifactType::Mask)
            );
            let store = world.store();
            artifact::get(store.as_ref(), ArtifactType::Mask, output.id(), &limits)
                .unwrap()
                .into_mask()
                .unwrap()
        })
        .collect();
    for (at, (mask, bounds)) in masks.iter().zip(boxes).enumerate() {
        // Its core is the instance, on the image's own binding, nothing protected.
        assert_eq!(mask.core(), plane_of(bounds), "{at}");
        assert_eq!(mask.binding(), &image.binding());
        assert!(mask.protect().iter().all(|sample| *sample == 0));
        assert_eq!(mask.use_case(), UseCase::Custom);
        assert_eq!(
            mask.origin().steps(),
            [Step::new(NAME, VERSION)
                .argument("kind", "phrase")
                .argument("query", "a cat")
                .argument("max_instances", "8")
                .argument("instance", &(at + 1).to_string())
                .input("image", id.0)]
        );
    }
    // The diagnostic gives each mask its score and its box, in the outputs' order.
    let stored = held(&world.root(), world.fs.clone());
    let run = stored
        .runs
        .iter()
        .find(|run| run.id == finished.run().as_str())
        .unwrap();
    let said = String::from_utf8(run.diagnostic(DIAGNOSTIC).unwrap().to_vec()).unwrap();
    let expected = format!(
        "instances 3\n\
         mask {} score 0.9 box 1 1 5 4\n\
         mask {} score 0.8 box 10 2 14 9\n\
         mask {} score 0.7 box 18 10 24 16\n",
        masks[0].id().0,
        masks[1].id().0,
        masks[2].id().0
    );
    assert_eq!(said, expected);

    // At most `max_instances`: two of the three, asked for two.
    let finished = engine(&world, &segmenter)
        .submit(&call(id, "a cat").argument("max_instances", Value::Integer(2)))
        .unwrap();
    assert_eq!(finished.outcome().outputs().len(), 2);
}

#[test]
fn no_instance_is_an_outcome_and_writes_no_mask() {
    let world = World::new();
    let (_, id) = with_picture(&world);
    let segmenter = Arc::new(FakeSegmenter::new().finding("a cat", [1, 1, 5, 4]));
    let before = held(&world.root(), world.fs.clone()).masks.len();
    let finished = engine(&world, &segmenter)
        .submit(&call(id, "a dog"))
        .unwrap();
    // The run completed, with nothing committed: finding nothing is an answer.
    assert_eq!(
        finished.outcome(),
        &Outcome::Completed {
            outputs: Vec::new()
        }
    );
    assert_eq!(segmenter.requests(), 1);
    let stored = held(&world.root(), world.fs.clone());
    assert_eq!(stored.masks.len(), before);
    let run = stored
        .runs
        .iter()
        .find(|run| run.id == finished.run().as_str())
        .unwrap();
    assert!(run.outputs.is_empty());
    assert_eq!(run.diagnostic(DIAGNOSTIC), Some(&b"instances 0\n"[..]));
}

#[test]
fn a_backend_lacking_phrase_is_refused_at_prepare() {
    let world = World::new();
    let (_, id) = with_picture(&world);
    let segmenter = Arc::new(
        FakeSegmenter::new()
            .declaring(&[feature::POINT, feature::BOX])
            .finding("a cat", [1, 1, 5, 4]),
    );
    let executor = engine(&world, &segmenter);
    // Prepared, the call is not ready: the one capability it needs, segment with the
    // feature phrase, has no backend that answers it.
    let prepared = executor.prepare(&call(id, "a cat")).unwrap();
    let Readiness::NotReady(missing) = prepared.readiness() else {
        panic!("ready: {:?}", prepared.readiness());
    };
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].features(), [feature::PHRASE]);
    // Sent, it is refused before any run exists, and the segmenter is never asked.
    let runs = held(&world.root(), world.fs.clone()).runs.len();
    let refused = executor.submit(&call(id, "a cat"));
    assert!(
        matches!(refused, Err(CallError::Refused(Refusal::NotReady { .. }))),
        "{refused:?}"
    );
    assert_eq!(segmenter.requests(), 0);
    assert_eq!(held(&world.root(), world.fs.clone()).runs.len(), runs);
}

#[test]
fn a_point_answered_with_three_candidates_writes_three_masks_best_first() {
    let world = World::new();
    let (image, id) = with_picture(&world);
    let boxes = [[2, 2, 6, 6], [1, 1, 8, 8], [0, 0, 12, 10]];
    let segmenter = Arc::new(
        FakeSegmenter::new()
            .finding_at_a_place(boxes[0])
            .finding_at_a_place(boxes[1])
            .finding_at_a_place(boxes[2]),
    );
    let finished = engine(&world, &segmenter)
        .submit(&asking(id, "point", "4 4"))
        .unwrap();
    let Outcome::Completed { outputs } = finished.outcome() else {
        panic!("{:?}", finished.outcome());
    };
    assert_eq!(segmenter.requests(), 1);
    let masks = masks_of(&world, outputs);
    assert_eq!(masks.len(), 3);
    for (mask, bounds) in masks.iter().zip(boxes) {
        assert_eq!(mask.core(), plane_of(bounds));
        assert_eq!(mask.binding(), &image.binding());
    }
    // Best first: the diagnostic's scores fall, in the outputs' order.
    let stored = held(&world.root(), world.fs.clone());
    let run = stored
        .runs
        .iter()
        .find(|run| run.id == finished.run().as_str())
        .unwrap();
    let said = String::from_utf8(run.diagnostic(DIAGNOSTIC).unwrap().to_vec()).unwrap();
    let expected = format!(
        "instances 3\n\
         mask {} score 0.9 box 2 2 6 6\n\
         mask {} score 0.8 box 1 1 8 8\n\
         mask {} score 0.7 box 0 0 12 10\n",
        masks[0].id().0,
        masks[1].id().0,
        masks[2].id().0
    );
    assert_eq!(said, expected);
    // A box is answered the same way; the limit cuts the candidates.
    let finished = engine(&world, &segmenter)
        .submit(&asking(id, "box", "1, 1, 9, 9").argument("max_instances", Value::Integer(2)))
        .unwrap();
    assert_eq!(finished.outcome().outputs().len(), 2);
    // A point outside the picture, or a text that is no point, is refused as invalid
    // before the segmenter is asked.
    for (kind, query) in [
        ("point", "24 3"),
        ("point", "4"),
        ("box", "1 1 30 9"),
        ("box", "3 3 1 1"),
    ] {
        let finished = engine(&world, &segmenter)
            .submit(&asking(id, kind, query))
            .unwrap();
        assert!(
            matches!(finished.outcome(), Outcome::Invalid { .. }),
            "{kind} {query}: {:?}",
            finished.outcome()
        );
    }
    assert_eq!(segmenter.requests(), 2);
}

#[test]
fn a_backend_lacking_point_or_box_is_refused_at_prepare() {
    let world = World::new();
    let (_, id) = with_picture(&world);
    let segmenter = Arc::new(
        FakeSegmenter::new()
            .declaring(&[feature::PHRASE])
            .finding("a cat", [1, 1, 5, 4])
            .finding_at_a_place([1, 1, 5, 4]),
    );
    let executor = engine(&world, &segmenter);
    // A phrase is ready: the backend declares it.
    assert_eq!(
        executor.prepare(&call(id, "a cat")).unwrap().readiness(),
        &Readiness::Ready
    );
    let runs = held(&world.root(), world.fs.clone()).runs.len();
    for (kind, query) in [(feature::POINT, "2 2"), (feature::BOX, "1 1 5 4")] {
        let prepared = executor.prepare(&asking(id, kind, query)).unwrap();
        let Readiness::NotReady(missing) = prepared.readiness() else {
            panic!("ready to ask a {kind}: {:?}", prepared.readiness());
        };
        assert_eq!(missing[0].features(), [kind]);
        let refused = executor.submit(&asking(id, kind, query));
        assert!(
            matches!(refused, Err(CallError::Refused(Refusal::NotReady { .. }))),
            "{refused:?}"
        );
    }
    // Nothing ran, and the segmenter was never asked.
    assert_eq!(segmenter.requests(), 0);
    assert_eq!(held(&world.root(), world.fs.clone()).runs.len(), runs);
}

#[test]
fn a_masks_origin_names_the_query_asked() {
    let world = World::new();
    let (_, id) = with_picture(&world);
    // Each kind finds another core here: a mask's identity is of what it is, so the same
    // core asked twice would be the one mask, with the origin of the first.
    for (kind, query, bounds) in [
        ("phrase", "a cat", [1, 1, 5, 4]),
        ("point", "2 2", [2, 2, 6, 6]),
        ("box", "1 1 5 4", [3, 3, 7, 7]),
    ] {
        let segmenter = Arc::new(
            FakeSegmenter::new()
                .finding("a cat", bounds)
                .finding_at_a_place(bounds),
        );
        let finished = engine(&world, &segmenter)
            .submit(&asking(id, kind, query))
            .unwrap();
        let masks = masks_of(&world, finished.outcome().outputs());
        assert_eq!(masks.len(), 1, "{kind}");
        assert_eq!(
            masks[0].origin().steps(),
            [Step::new(NAME, VERSION)
                .argument("kind", kind)
                .argument("query", query)
                .argument("max_instances", "8")
                .argument("instance", "1")
                .input("image", id.0)]
        );
    }
    assert_eq!(VERSION, "2.1.0");
}

#[test]
fn instances_are_read_in_the_order_the_interface_fixes() {
    // The fake scores a phrase's boxes from 0.9 down by 0.1 and no lower than 0: the
    // tenth and eleventh both score 0, and the eleventh is higher in the picture. They
    // are read best score first, then by the box's top, whatever order they came in.
    let world = World::new();
    let (_, id) = with_picture(&world);
    let mut segmenter = FakeSegmenter::new();
    let mut boxes = Vec::new();
    for at in 0..9 {
        boxes.push([at, 0, at + 1, 1]);
    }
    boxes.push([0, 10, 2, 12]);
    boxes.push([5, 3, 7, 5]);
    for bounds in &boxes {
        segmenter = segmenter.finding("a dot", *bounds);
    }
    let segmenter = Arc::new(segmenter);
    let finished = engine(&world, &segmenter)
        .submit(&call(id, "a dot").argument("max_instances", Value::Integer(16)))
        .unwrap();
    let masks = masks_of(&world, finished.outcome().outputs());
    let mut expected: Vec<[u32; 4]> = boxes[..9].to_vec();
    expected.extend([boxes[10], boxes[9]]);
    let cores: Vec<Vec<u8>> = masks.iter().map(|mask| mask.core().to_vec()).collect();
    let planes: Vec<Vec<u8>> = expected.into_iter().map(plane_of).collect();
    assert_eq!(cores, planes);
    // The instance an origin names is its place in that order.
    let last = masks[10].origin().steps()[0].clone();
    assert_eq!(
        last,
        Step::new(NAME, VERSION)
            .argument("kind", "phrase")
            .argument("query", "a dot")
            .argument("max_instances", "16")
            .argument("instance", "11")
            .input("image", id.0)
    );

    // The order itself: the score, then the top, the left, the right and the bottom,
    // then the predicted IoU, none first and then the higher, then the plane's bytes.
    use std::cmp::Ordering::{Greater, Less};
    use whizbang_rascal::segment::in_order;
    let key = |score: f64, b: [f64; 4], iou: Option<f64>| (score, b, iou, &[0u8][..]);
    let b = [1.0, 2.0, 3.0, 4.0];
    assert_eq!(in_order(key(0.9, b, None), key(0.8, b, None)), Less);
    assert_eq!(
        in_order(
            key(0.5, [0.0, 3.0, 9.0, 9.0], None),
            key(0.5, [9.0, 2.0, 9.0, 9.0], None)
        ),
        Greater
    );
    assert_eq!(
        in_order(
            key(0.5, [1.0, 2.0, 9.0, 9.0], None),
            key(0.5, [2.0, 2.0, 0.0, 0.0], None)
        ),
        Less
    );
    assert_eq!(in_order(key(0.5, b, None), key(0.5, b, Some(0.1))), Less);
    assert_eq!(
        in_order(key(0.5, b, Some(0.9)), key(0.5, b, Some(0.1))),
        Less
    );
    let (one, other) = ([0u8, 255], [255u8, 0]);
    assert_eq!(
        in_order((0.5, b, None, &one[..]), (0.5, b, None, &other[..])),
        Less
    );
}

/// What replay needs of a segmenter's answer is in its run (plan m7, task B1, item 7; ADR
/// 0048): each instance's plane is its mask's core, and its score, its box and its
/// predicted IoU, and what the backend said beside the answer, are in the diagnostic. The
/// answer made again from those is the answer the segmenter gave, whole, in the fixed
/// order, number for number.
#[test]
fn the_answer_rebuilt_from_a_runs_masks_and_its_diagnostic_is_the_answer_it_was_given() {
    use whizbang_core::capability::{Instance, Picture, Query, SegmentHandle, SegmentRequest};
    use whizbang_core::contract::Cancellation;
    use whizbang_rascal::segment::in_order;

    let world = World::new();
    let (image, id) = with_picture(&world);
    // Three instances, out of the fixed order, each with a predicted IoU, and two lines
    // the backend says beside them.
    let boxes = [[10, 2, 14, 9], [1, 1, 5, 4], [18, 10, 24, 16]];
    let segmenter = Arc::new(
        boxes
            .iter()
            .fold(FakeSegmenter::new(), |fake, bounds| {
                fake.finding("a cat", *bounds)
            })
            .saying("3 instances on mps\nas determined", 0.875),
    );
    let finished = engine(&world, &segmenter)
        .submit(&call(id, "a cat"))
        .unwrap();
    let Outcome::Completed { outputs } = finished.outcome() else {
        panic!("{:?}", finished.outcome());
    };
    // The answer the segmenter gives that request, asked again of it, in the fixed order.
    let limits = DecodeLimits::default();
    let picture = Picture::new(image.pixels(&limits).unwrap()).unwrap();
    let request = SegmentRequest::new(picture, vec![Query::phrase("a cat").unwrap()], 8)
        .and_then(|request| request.under(&image.binding()))
        .unwrap();
    let handle = SegmentHandle::new(segmenter.clone(), Cancellation::new());
    let given = handle.segment(&request).unwrap();
    let found = &given.answers()[0];
    let key = |at: usize| {
        let instance = &found[at];
        let iou = given.predicted_iou(0, at);
        (instance.score(), instance.bounds(), iou, instance.plane())
    };
    let mut order: Vec<usize> = (0..found.len()).collect();
    order.sort_by(|one, other| in_order(key(*one), key(*other)));
    let given_whole: Vec<(Instance, Option<f64>)> = (order.iter())
        .map(|at| (found[*at].clone(), given.predicted_iou(0, *at)))
        .collect();

    // The answer rebuilt from the run alone: its committed masks and its diagnostic.
    let stored = held(&world.root(), world.fs.clone());
    let run = stored
        .runs
        .iter()
        .find(|run| run.id == finished.run().as_str())
        .unwrap();
    let said = String::from_utf8(run.diagnostic(DIAGNOSTIC).unwrap().to_vec()).unwrap();
    let masks = masks_of(&world, outputs);
    let mut lines = said.lines();
    assert_eq!(lines.next(), Some("instances 3"));
    let mut rebuilt = Vec::new();
    for mask in &masks {
        let line = lines.next().unwrap();
        let words: Vec<&str> = line.split(' ').collect();
        assert_eq!(words[..2], ["mask", mask.id().0.to_string().as_str()]);
        assert_eq!((words[2], words[4]), ("score", "box"));
        let number = |at: usize| words[at].parse::<f64>().unwrap();
        let bounds = [number(5), number(6), number(7), number(8)];
        let iou = (words.get(9) == Some(&"iou")).then(|| number(10));
        let space = mask.space();
        let instance = Instance::new(
            mask.binding().id(),
            space.width(),
            space.height(),
            mask.core().to_vec(),
            bounds,
            number(3),
        )
        .unwrap();
        rebuilt.push((instance, iou));
    }
    let backend: Vec<&str> = lines
        .map(|line| line.strip_prefix("backend ").unwrap())
        .collect();
    assert_eq!(rebuilt, given_whole);
    assert_eq!(backend.join("\n"), given.diagnostics());
    // And where it was answered is in the run's record, as the response said it.
    let record = run.record.as_ref().unwrap();
    let model = &record.models()[0];
    let served = given.served();
    assert_eq!(model.device.as_deref(), served.device().map(|d| d.as_str()));
    assert_eq!(model.cached, served.from_cache());
}
