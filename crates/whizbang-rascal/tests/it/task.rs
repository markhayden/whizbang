//! The task `remove_prompt` (plan m3, task R1, item 5): the shipped recipe, read and
//! checked against this crate's actions, run through the executor over the test kit's
//! fake segmenter and a fake filler that paints the support, on a picture of 64 by 64,
//! with `protect` left out and given. Its last step fills (plan m3, task F1, item 6): the
//! mask handed back is the feathered mask rebound onto the composite.

use std::sync::Arc;

use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use whizbang_core::artifact::{
    self, Artifact, ArtifactId, ArtifactType, Image, ImageRole, Mask, Origin, Source, Step, UseCase,
};
use whizbang_core::contract::{Call, Capabilities, Engine, Executor, Outcome, Registry, Value};
use whizbang_core::identity::{DecodeLimits, Digest, Projection, Topology};
use whizbang_core::store::Store;
use whizbang_rascal::recipes;
use whizbang_testkit::capabilities::{FakeFiller, FakeSegmenter, Paint, roster};

use super::grow_action::World;

const SIDE: u32 = 64;

fn picture() -> Image {
    let mut rgba = Vec::new();
    for y in 0..SIDE {
        for x in 0..SIDE {
            rgba.extend_from_slice(&[(x * 4) as u8, (y * 4) as u8, 90, 255]);
        }
    }
    let mut file = Vec::new();
    PngEncoder::new(&mut file)
        .write_image(&rgba, SIDE, SIDE, ExtendedColorType::Rgba8)
        .unwrap();
    let origin = Origin::new(Step::new("fixture", "1")).unwrap();
    let limits = DecodeLimits::default();
    Image::new(
        file,
        Projection::Flat,
        Topology::Bounded,
        ImageRole::Picture,
        origin,
        &limits,
    )
    .unwrap()
}

fn put(store: &Store, artifact: Artifact) -> ArtifactId {
    let mut publication = store.begin().unwrap();
    let id = artifact::put(&mut publication, &artifact).unwrap();
    publication.publish().unwrap();
    id
}

/// The executor over every action `remove_prompt` calls, and the recipe as the build
/// ships it, with this segmenter.
fn engine(store: Arc<Store>, segmenter: &Arc<FakeSegmenter>) -> Engine {
    let mut registry = Registry::new();
    whizbang_rascal::segment::register(&mut registry).unwrap();
    whizbang_rascal::select::register(&mut registry).unwrap();
    whizbang_rascal::grow::register(&mut registry).unwrap();
    whizbang_rascal::close::register(&mut registry).unwrap();
    whizbang_rascal::plug::register(&mut registry).unwrap();
    whizbang_rascal::subtract::register(&mut registry).unwrap();
    whizbang_rascal::feather::register(&mut registry).unwrap();
    whizbang_rascal::fill::register(&mut registry).unwrap();
    for recipe in recipes::shipped(&registry).unwrap() {
        registry.register(Arc::new(recipe)).unwrap();
    }
    let filler = Arc::new(FakeFiller::new(Paint::Support([250, 10, 10])));
    let backends = roster(Some(segmenter.as_ref()), Some(filler.as_ref()), None);
    Engine::over(registry, Arc::new(backends), store, Digest::of_bytes(b"")).with_capabilities(
        Capabilities::none()
            .with_segment(segmenter.clone())
            .with_fill(filler),
    )
}

fn mask(store: &Store, id: ArtifactId) -> Mask {
    let read = artifact::get(store, ArtifactType::Mask, id, &DecodeLimits::default());
    read.unwrap().into_mask().unwrap()
}

#[test]
fn remove_prompt_completes_with_and_without_protect() {
    let world = World::new();
    let store = world.store();
    let image = picture();
    let id = put(&store, Artifact::from(image.clone()));
    let segmenter = Arc::new(
        FakeSegmenter::new()
            .finding("leather collar", [10, 10, 20, 18])
            .finding("leather collar", [40, 30, 50, 36]),
    );
    let executor = engine(store.clone(), &segmenter);
    let asked = Call::new("remove_prompt")
        .argument("prompt", Value::text("leather collar"))
        .input("image", id);
    let finished = executor.submit(&asked).unwrap();
    let Outcome::Completed { outputs } = finished.outcome() else {
        panic!("{:?}", finished.outcome());
    };
    // The composite and the mask rebound onto it.
    assert_eq!(outputs.len(), 2);
    let without = mask(&store, outputs[1].id());
    // Both instances, grown, closed and feathered: the core holds each box and reaches
    // further down for the shadow; the blend feathers its edge; nothing is protected.
    let at = |m: &Mask, x: u32, y: u32| m.core()[(y * SIDE + x) as usize];
    assert_eq!((at(&without, 15, 14), at(&without, 45, 33)), (255, 255));
    assert_eq!(
        at(&without, 15, 28),
        255,
        "grown 4 and 12 down: rows to 18 + 3 + 12"
    );
    assert_eq!(at(&without, 15, 4), 0);
    assert!(without.blend().iter().any(|b| *b != 0));
    assert!(without.protect().iter().all(|p| *p == 0));
    assert_eq!(segmenter.requests(), 1);
    let names: Vec<String> = without
        .origin()
        .steps()
        .iter()
        .map(|s| s.action.clone())
        .collect();
    // An origin is the stored artifact's: a step that changed nothing made the same
    // identity, which the store holds with the origin it was first committed with. The
    // last steps are the fill's and its rebind onto the composite.
    assert_eq!(names.first().map(String::as_str), Some("select"));
    assert_eq!(names[names.len() - 3..], ["feather", "fill", "rebind"]);

    // With protect: a mask over the first instance's left half, which the mask keeps off.
    let guard = Mask::from_core(
        image.binding(),
        UseCase::Custom,
        (0..SIDE * SIDE)
            .map(|at| {
                if at % SIDE < 15 && at / SIDE < 40 {
                    255
                } else {
                    0
                }
            })
            .collect(),
        Source::default(),
        Origin::new(Step::new("fixture", "1")).unwrap(),
    )
    .unwrap();
    let guard = put(&store, Artifact::from(guard));
    let finished = executor
        .submit(&asked.clone().input("protect", guard))
        .unwrap();
    let Outcome::Completed { outputs } = finished.outcome() else {
        panic!("{:?}", finished.outcome());
    };
    let with = mask(&store, outputs[1].id());
    assert_eq!(at(&with, 12, 14), 0, "kept off protect");
    assert_ne!(
        with.protect()[(14 * SIDE + 16) as usize],
        0,
        "protect grown by 2"
    );
    assert_eq!(at(&with, 45, 33), 255);
    // An override of what decides a step's needs is refused before anything runs: the
    // recipe was admitted with what its steps need.
    let before = segmenter.requests();
    let chosen = asked
        .clone()
        .argument("set", Value::text("segment.backend=another"));
    let refused = executor.submit(&chosen).unwrap();
    let Outcome::Invalid { why } = refused.outcome() else {
        panic!("{:?}", refused.outcome());
    };
    assert!(why.contains("decides what it needs"), "{why}");
    assert_eq!(segmenter.requests(), before);
    let names: Vec<String> = with
        .origin()
        .steps()
        .iter()
        .map(|s| s.action.clone())
        .collect();
    assert!(names.iter().any(|name| name == "subtract"), "{names:?}");
}
