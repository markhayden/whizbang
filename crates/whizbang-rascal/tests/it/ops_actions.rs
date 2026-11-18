//! `select`, `plug`, `feather` and `render` as actions, run through the executor against a
//! store, one after another as the owner runs them: each commits what its function makes,
//! and a call one cannot take ends the run as invalid.

use std::sync::Arc;

use whizbang_core::artifact::{self, Artifact, ArtifactId, ArtifactType, Mask};
use whizbang_core::contract::{
    Action, Call, Engine, Executor, NoBackends, Outcome, Registry, Value,
};
use whizbang_core::identity::{DecodeLimits, Digest};
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_rascal::feather::Feather;
use whizbang_rascal::plug::Plug;
use whizbang_rascal::render::Render;
use whizbang_rascal::select::Select;
use whizbang_testkit::scratch::Scratch;

use super::grow::plane;
use super::render::{mask_on, picture};

fn engine(store: Arc<Store>) -> Engine {
    let mut registry = Registry::new();
    let actions: [Arc<dyn Action>; 4] = [
        Arc::new(Select::new()),
        Arc::new(Plug::new()),
        Arc::new(Feather::new()),
        Arc::new(Render::new()),
    ];
    for action in actions {
        registry.register(action).unwrap();
    }
    Engine::over(registry, Arc::new(NoBackends), store, Digest::of_bytes(b""))
}

fn put(store: &Store, artifact: Artifact) -> ArtifactId {
    let mut publication = store.begin().unwrap();
    let id = artifact::put(&mut publication, &artifact).unwrap();
    publication.publish().unwrap();
    id
}

/// The one output of a run that completed.
fn one(engine: &Engine, call: &Call) -> ArtifactId {
    let finished = engine.submit(call).unwrap();
    let Outcome::Completed { outputs } = finished.outcome() else {
        panic!("{:?}", finished.outcome());
    };
    let [output] = outputs.as_slice() else {
        panic!("{outputs:?}");
    };
    output.id()
}

fn mask(store: &Store, id: ArtifactId) -> Mask {
    let read = artifact::get(store, ArtifactType::Mask, id, &DecodeLimits::default());
    read.unwrap().into_mask().unwrap()
}

#[test]
fn the_four_run_one_after_another_through_the_executor_and_commit_what_they_make() {
    let dir = Scratch::new("ops");
    let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap());
    let (_, image) = picture(9, 7);
    let first = mask_on(&image);
    // A second instance on the same image: a ring with a hole of one pixel.
    let ring = plane(9, 7, |x, y| {
        (0..3).contains(&x) && (3..6).contains(&y) && (x, y) != (1, 4)
    });
    let second = Mask::from_core(
        image.binding(),
        first.use_case(),
        ring,
        first.source().clone(),
        first.origin().clone(),
    )
    .unwrap();
    let image_id = put(&store, Artifact::Image(image.clone()));
    let (a, b) = (put(&store, first.into()), put(&store, second.into()));
    let engine = engine(store.clone());

    let selected = one(
        &engine,
        &Call::new("select")
            .argument("exclude", Value::text("8 0 9 1"))
            .input("mask", a)
            .input("mask", b),
    );
    let held = mask(&store, selected);
    assert_eq!(held.origin().steps()[0].action, "select");
    let plugged = one(&engine, &Call::new("plug").input("mask", selected));
    let held = mask(&store, plugged);
    assert_eq!(
        held.core()[4 * 9 + 1],
        255,
        "the hole of one pixel is filled"
    );
    let feathered = one(
        &engine,
        &Call::new("feather")
            .argument("sigma_px", Value::Number(1.5))
            .input("mask", plugged),
    );
    assert!(mask(&store, feathered).blend().iter().any(|s| *s != 0));
    let rendered = one(
        &engine,
        &Call::new("render")
            .argument("profile", Value::text("alpha"))
            .input("mask", feathered)
            .input("image", image_id),
    );
    let read = artifact::get(
        &*store,
        ArtifactType::Image,
        rendered,
        &DecodeLimits::default(),
    );
    let Artifact::Image(rendering) = read.unwrap() else {
        panic!("no image");
    };
    assert_eq!(rendering.origin().steps()[0].action, "render");

    // `alpha` without its image, and a box that is not four numbers, end as invalid.
    for call in [
        Call::new("render")
            .argument("profile", Value::text("alpha"))
            .input("mask", feathered),
        Call::new("select")
            .argument("exclude", Value::text("1 2 3"))
            .input("mask", a),
    ] {
        let finished = engine.submit(&call).unwrap();
        assert!(
            matches!(finished.outcome(), Outcome::Invalid { .. }),
            "{:?}",
            finished.outcome()
        );
    }
}
