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
        .argument("phrase", Value::text(phrase))
        .input("image", image)
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
                .argument("phrase", "a cat")
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
