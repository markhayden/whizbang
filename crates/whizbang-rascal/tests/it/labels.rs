//! `labels` as an action (plan mE, task T2, item 2; ADR 0053), run through the executor
//! against a store, over the test kit's fake segmenter declaring the features of a
//! fixed-class segmenter: one mask for each class some pixel has, its confidence and its
//! pixels handed back; an answer that labels a pixel twice fails the run; and a segmenter
//! that declares no `labels` is refused before any run exists.

use std::sync::Arc;

use serde_json::json;
use whizbang_core::artifact::{self, ArtifactId, ArtifactType};
use whizbang_core::capability::feature;
use whizbang_core::contract::{
    Call, CallError, Capabilities, Engine, Executor, Outcome, Readiness, Refusal, Registry, Value,
};
use whizbang_core::identity::{DecodeLimits, Digest};
use whizbang_rascal::labels::{CLASSES_MOST, DIAGNOSTIC, Labels, NAME, REQUESTS_MOST, classes_of};
use whizbang_testkit::capabilities::{FakeSegmenter, roster};

use super::grow_action::{World, held};
use super::segment::{plane_of, with_picture};

/// A fixed-class segmenter of the fake's kind: `sky` on the top 10 rows of the 24 by 16
/// picture, `floor` on the 6 below.
fn fixed_class() -> FakeSegmenter {
    FakeSegmenter::new()
        .declaring(&[feature::PHRASE, feature::LABELS])
        .finding("sky", [0, 0, 24, 10])
        .finding("floor", [0, 10, 24, 16])
}

fn engine(world: &World, segmenter: &Arc<FakeSegmenter>) -> Engine {
    let mut registry = Registry::new();
    registry.register(Arc::new(Labels::new())).unwrap();
    let backends = roster(Some(segmenter.as_ref()), None, None);
    Engine::over(
        registry,
        Arc::new(backends),
        world.store(),
        Digest::of_bytes(b""),
    )
    .with_capabilities(Capabilities::none().with_segment(segmenter.clone()))
}

fn call(image: ArtifactId, classes: &str) -> Call {
    Call::new(NAME)
        .argument("classes", Value::text(classes))
        .input("image", image)
}

#[test]
fn every_class_some_pixel_has_is_one_mask_with_its_confidence_handed_back() {
    let world = World::new();
    let (_, id) = with_picture(&world);
    let segmenter = Arc::new(fixed_class());
    // 70 classes: two requests, of 64 phrases and of 6, at one instance each.
    let mut named: Vec<String> = (0..68).map(|n| format!("class {n}")).collect();
    named.insert(3, "floor".to_owned());
    named.push("sky".to_owned());
    let finished = engine(&world, &segmenter)
        .submit(&call(id, &named.join("\n")))
        .unwrap();
    let Outcome::Completed { outputs } = finished.outcome() else {
        panic!("{:?}", finished.outcome());
    };
    assert_eq!(segmenter.requests(), 2);
    // In the order the classes were named, each the pixels of its class.
    let limits = DecodeLimits::default();
    let cores: Vec<Vec<u8>> = (outputs.iter())
        .map(|output| {
            assert_eq!(output.port(), "mask");
            let store = world.store();
            let read = artifact::get(store.as_ref(), ArtifactType::Mask, output.id(), &limits);
            read.unwrap().into_mask().unwrap().core().to_vec()
        })
        .collect();
    let on = |core: &[u8]| {
        core.iter()
            .map(|v| if *v == 0 { 0 } else { 255 })
            .collect::<Vec<u8>>()
    };
    assert_eq!(
        cores.iter().map(|core| on(core)).collect::<Vec<_>>(),
        [plane_of([0, 10, 24, 16]), plane_of([0, 0, 24, 10])]
    );
    // The record's pictures are these masks, each three times its size
    // (`docs/reference/tools/labels.md`).
    for (core, file) in cores.iter().zip(["floor", "sky"]) {
        let path = format!(
            "{}/../../docs/reference/tools/labels/{file}.png",
            env!("CARGO_MANIFEST_DIR")
        );
        let drawn = image::open(path).unwrap().to_luma8();
        assert_eq!(drawn.dimensions(), (72, 48));
        let read: Vec<u8> = (0..16u32)
            .flat_map(|y| (0..24u32).map(move |x| (x, y)))
            .map(|(x, y)| drawn.get_pixel(3 * x + 1, 3 * y + 1)[0])
            .collect();
        assert_eq!(read, on(core), "{file}");
    }
    assert_eq!(
        finished.measurements(),
        Some(&json!({"classes": [
            {"name": "floor", "score": 0.9, "pixels": 144},
            {"name": "sky", "score": 0.9, "pixels": 240},
        ]}))
    );
    let stored = held(&world.root(), world.fs.clone());
    let run = (stored.runs.iter())
        .find(|run| run.id == finished.run().as_str())
        .unwrap();
    let said = String::from_utf8(run.diagnostic(DIAGNOSTIC).unwrap().to_vec()).unwrap();
    assert!(
        said.starts_with("classes 70 present 2\nclass \"floor\" mask sha256:"),
        "{said}"
    );
    // What a call may name.
    assert_eq!(classes_of("sky\nfloor").unwrap(), ["sky", "floor"]);
    for (text, said) in [
        ("", "1 to 255"),
        ("sky\n\nfloor", "1 to 512 bytes"),
        ("sky\nsky", "named twice"),
    ] {
        assert!(classes_of(text).unwrap_err().contains(said), "{text:?}");
    }
    let many: Vec<String> = (0..=CLASSES_MOST).map(|n| n.to_string()).collect();
    assert!(classes_of(&many.join("\n")).is_err());
    assert_eq!(REQUESTS_MOST, 4);
}

#[test]
fn a_pixel_labelled_twice_fails_and_a_segmenter_without_labels_is_refused() {
    let world = World::new();
    let (_, id) = with_picture(&world);
    // Two classes on one pixel is no fixed-class segmenter's answer: nothing is committed.
    let overlapping = Arc::new(fixed_class().finding("wall", [0, 8, 4, 12]));
    let finished = engine(&world, &overlapping)
        .submit(&call(id, "sky\nwall"))
        .unwrap();
    let Outcome::Error { message, .. } = finished.outcome() else {
        panic!("{:?}", finished.outcome());
    };
    assert!(message.contains("one class at most"), "{message}");
    // A segmenter that declares phrases and not labels is not ready for the call, which
    // is refused before any run exists; it is never asked.
    let plain = Arc::new(FakeSegmenter::new().finding("sky", [0, 0, 24, 10]));
    let executor = engine(&world, &plain);
    let prepared = executor.prepare(&call(id, "sky")).unwrap();
    let Readiness::NotReady(missing) = prepared.readiness() else {
        panic!("ready: {:?}", prepared.readiness());
    };
    assert_eq!(missing[0].features(), [feature::PHRASE, feature::LABELS]);
    let runs = held(&world.root(), world.fs.clone()).runs.len();
    let refused = executor.submit(&call(id, "sky"));
    assert!(
        matches!(refused, Err(CallError::Refused(Refusal::NotReady { .. }))),
        "{refused:?}"
    );
    assert_eq!(plain.requests(), 0);
    assert_eq!(held(&world.root(), world.fs.clone()).runs.len(), runs);
}
