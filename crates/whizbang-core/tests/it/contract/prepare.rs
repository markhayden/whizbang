//! Prepare (`SPEC.md` section 3, ADR 0004): a call checked against its action's
//! descriptor, its inputs found in the store, readiness for these exact arguments, and a
//! fingerprint of everything that was seen. Nothing is written and nothing is sent.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use whizbang_core::artifact::{
    self, Artifact, ArtifactId, ArtifactType, Derivation, ImageRole, Mask,
};
use whizbang_core::contract::{
    Action, ActionDescriptor, Backends, Call, Capability, Engine, Executor, InputPort, Kind,
    ModelIdentity, NoBackends, PreparedCall, Readiness, Refusal, Registry, Unavailable, Value,
};
use whizbang_core::identity::{Binding, Digest, Mapping};
use whizbang_core::store::{Reader, Store};
use whizbang_testkit::fs_shim::ShimFs;
use whizbang_testkit::scratch::Scratch;

use super::fixtures;
use crate::artifact::support::{image_of, line, photo, png, rect_mask};

struct Fake(ActionDescriptor);

impl Action for Fake {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }
}

fn registry() -> Registry {
    let mut registry = Registry::new();
    // An action that takes a location on its image, beside the two fixtures.
    let look = ActionDescriptor::draft("look", "1.0.0", Kind::Primitive, "Looks at a place.")
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The image.",
        ))
        .input(InputPort::required("place", ArtifactType::Location, "The place.").on("image"));
    for draft in [fixtures::seam(), fixtures::remove_prompt(), look] {
        registry
            .register(Arc::new(Fake(draft.build().unwrap())))
            .unwrap();
    }
    registry
}

/// Backends as a test states them: the capabilities on offer, each with its features and
/// the model that answers. It counts how often it was asked.
#[derive(Default)]
struct Offered {
    offers: Vec<(Capability, Vec<&'static str>, ModelIdentity)>,
    asked: AtomicUsize,
}

impl Offered {
    fn with(mut self, capability: Capability, features: &[&'static str], model: &str) -> Self {
        self.offers.push((
            capability,
            features.to_vec(),
            ModelIdentity::new("fake", model, "1"),
        ));
        self
    }
}

impl Backends for Offered {
    fn resolve(
        &self,
        capability: Capability,
        features: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        let Some((_, offered, model)) = self.offers.iter().find(|offer| offer.0 == capability)
        else {
            return Err(Unavailable::new("nothing offers it"));
        };
        match features.iter().find(|f| !offered.contains(&f.as_str())) {
            Some(missing) => Err(Unavailable::new(&format!(
                "{} lacks {missing}",
                model.model()
            ))),
            None => Ok(model.clone()),
        }
    }
}

fn segmenter() -> Offered {
    Offered::default().with(Capability::Segment, &["phrase", "box"], "sam")
}

/// A store on a file system that records every change, holding two photographs of one
/// size and a mask on each.
struct World {
    _dir: Scratch,
    fs: Arc<ShimFs>,
    root: std::path::PathBuf,
    photo: ArtifactId,
    other: ArtifactId,
    mask_on_photo: ArtifactId,
    mask_on_other: ArtifactId,
    /// The mask on the first photograph, moved onto the second by a derivation that was
    /// only stated.
    moved: ArtifactId,
    place_on_photo: ArtifactId,
}

fn other_photo() -> Vec<u8> {
    png(|x, y| [(y * 40) as u8, (x * 30) as u8, 7, 255])
}

fn world() -> World {
    let dir = Scratch::new("prepare");
    let root = dir.path().join("store");
    let fs = Arc::new(ShimFs::new());
    let first = image_of(photo(), ImageRole::Picture);
    let second = image_of(other_photo(), ImageRole::Picture);
    assert_eq!(first.space(), second.space());
    let mask_on_photo = rect_mask(first.binding());
    let mask_on_other = rect_mask(second.binding());
    // The second photograph "derived from" the first: untrue, and nothing here can tell.
    let untrue = Binding::new(
        second.identity().pixels,
        second.space(),
        first.identity().pixels,
        first.space(),
        whizbang_core::identity::Direction::SourceToDestination,
        Mapping::Identity,
    )
    .unwrap();
    let moved = mask_on_photo
        .rebind(&Derivation::stated(untrue).unwrap())
        .unwrap();
    let place = line(first.binding());
    let world = World {
        photo: first.id(),
        other: second.id(),
        mask_on_photo: mask_on_photo.id(),
        mask_on_other: mask_on_other.id(),
        moved: moved.id(),
        place_on_photo: place.id(),
        _dir: dir,
        fs: fs.clone(),
        root: root.clone(),
    };
    let store = Store::open(&root, fs).unwrap();
    let mut publication = store.begin().unwrap();
    for artifact in [
        Artifact::from(first),
        Artifact::from(second),
        Artifact::from(mask_on_photo),
        Artifact::from(mask_on_other),
        Artifact::from(moved),
        Artifact::from(place),
    ] {
        artifact::put(&mut publication, &artifact).unwrap();
    }
    publication.publish().unwrap();
    world
}

impl World {
    fn engine_with(&self, backends: impl Backends + 'static, config: &[u8]) -> Engine {
        let reader = Reader::open(&self.root, self.fs.clone()).unwrap();
        Engine::new(
            registry(),
            Arc::new(backends),
            Arc::new(reader),
            Digest::of_bytes(config),
        )
    }

    fn engine(&self, backends: impl Backends + 'static) -> Engine {
        self.engine_with(backends, b"config")
    }

    /// `remove_prompt` on the first photograph, for a dog.
    fn remove(&self) -> Call {
        Call::new("remove_prompt")
            .argument("prompt", Value::text("a dog"))
            .input("image", self.photo)
    }
}

#[track_caller]
fn refused(result: Result<PreparedCall, Refusal>, what: &[&str]) -> Refusal {
    let refusal = match result {
        Ok(prepared) => {
            panic!("prepared, where a refusal saying {what:?} was expected: {prepared:?}")
        }
        Err(refusal) => refusal,
    };
    let message = refusal.to_string();
    for part in what {
        assert!(message.contains(part), "{part:?} is not in: {message}");
    }
    refusal
}

// --- readiness for these exact arguments -------------------------------------------------

#[test]
fn remove_prompt_with_and_without_a_filler_needs_different_capabilities_and_is_ready_or_not() {
    let world = world();
    let engine = world.engine(segmenter());

    // The mask alone: a segmenter is needed and there is one.
    let alone = engine.prepare(&world.remove()).unwrap();
    let capabilities: Vec<Capability> = alone.needs().iter().map(|n| n.capability()).collect();
    assert_eq!(capabilities, [Capability::Segment]);
    assert_eq!(alone.needs()[0].model().unwrap().model(), "sam");
    assert_eq!(alone.readiness(), &Readiness::Ready);
    assert_eq!(alone.outputs(), ["mask", "found"]);

    // With a filler: a filler is needed as well, and nothing offers one.
    let filled = world.remove().argument("filler", Value::text("bria"));
    let filled = engine.prepare(&filled).unwrap();
    let capabilities: Vec<Capability> = filled.needs().iter().map(|n| n.capability()).collect();
    assert_eq!(capabilities, [Capability::Segment, Capability::Fill]);
    assert!(filled.needs()[1].model().is_none());
    let Readiness::NotReady(missing) = filled.readiness() else {
        panic!("ready without a filler: {filled:?}");
    };
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].capability(), Capability::Fill);
    assert_eq!(missing[0].why(), "nothing offers it");
    assert_eq!(filled.outputs(), ["mask", "result", "found"]);
    assert_ne!(alone.fingerprint(), filled.fingerprint());

    // The same call where a filler is offered is ready.
    let both = world.engine(segmenter().with(Capability::Fill, &[], "bria"));
    let filled = world.remove().argument("filler", Value::text("bria"));
    assert_eq!(
        both.prepare(&filled).unwrap().readiness(),
        &Readiness::Ready
    );
}

#[test]
fn a_capability_without_a_feature_the_action_needs_is_not_ready() {
    let world = world();
    let boxes_only = Offered::default().with(Capability::Segment, &["box"], "tiny");
    let prepared = world.engine(boxes_only).prepare(&world.remove()).unwrap();
    let Readiness::NotReady(missing) = prepared.readiness() else {
        panic!("ready without the phrase feature");
    };
    assert_eq!(missing[0].features(), ["phrase".to_owned()]);
    assert_eq!(missing[0].why(), "tiny lacks phrase");
}

#[test]
fn with_no_backends_nothing_that_needs_a_capability_is_ready() {
    let world = world();
    let engine = world.engine(NoBackends);
    let prepared = engine.prepare(&world.remove()).unwrap();
    assert!(matches!(prepared.readiness(), Readiness::NotReady(_)));
    // An action that needs none is ready with none.
    let seam = Call::new("seam").input("image", world.photo);
    assert_eq!(
        engine.prepare(&seam).unwrap().readiness(),
        &Readiness::Ready
    );
}

// --- the call against the descriptor ------------------------------------------------------

#[test]
fn a_prepared_call_holds_every_argument_with_its_value() {
    let world = world();
    let prepared = world.engine(segmenter()).prepare(&world.remove()).unwrap();
    assert_eq!(prepared.action(), "remove_prompt");
    assert_eq!(prepared.version(), "1.0.0");
    // In the descriptor's order: what was given, then what a default gave. An argument
    // that was left out and has no default has no value and is not listed.
    assert_eq!(
        prepared.arguments(),
        [
            ("prompt".to_owned(), Value::text("a dog")),
            ("strict".to_owned(), Value::Boolean(false)),
        ]
    );
    let inputs: Vec<(&str, ArtifactId)> = prepared
        .inputs()
        .iter()
        .map(|i| (i.port(), i.id()))
        .collect();
    assert_eq!(inputs, [("image", world.photo)]);
}

#[test]
fn a_call_the_descriptor_does_not_allow_is_refused() {
    let world = world();
    let engine = world.engine(segmenter());
    let refusal = refused(
        engine.prepare(&Call::new("vanish").input("image", world.photo)),
        &["no action named vanish"],
    );
    assert!(matches!(refusal, Refusal::UnknownAction { .. }));
    let invalid = |call: Call, what: &[&str]| {
        let refusal = refused(engine.prepare(&call), what);
        assert!(matches!(refusal, Refusal::Invalid { .. }), "{refusal:?}");
    };
    invalid(
        world.remove().argument("colour", Value::text("red")),
        &["remove_prompt", "colour", "is no argument"],
    );
    invalid(
        Call::new("remove_prompt").input("image", world.photo),
        &["the argument prompt", "is required"],
    );
    invalid(
        world.remove().argument("strict", Value::text("yes")),
        &["the argument strict", "a text where a boolean is required"],
    );
    invalid(
        world
            .remove()
            .argument("projection", Value::text("fisheye")),
        &[
            "the argument projection",
            "is not one of flat, equirectangular",
        ],
    );
    invalid(
        world.remove().input("canvas", world.photo),
        &["canvas", "is no input"],
    );
    invalid(
        Call::new("remove_prompt").argument("prompt", Value::text("a dog")),
        &["the input image", "takes 1", "was given 0"],
    );
    invalid(
        world.remove().input("image", world.other),
        &["the input image", "takes 1", "was given 2"],
    );
    // An input port is not an argument, and an argument is not a port.
    invalid(
        world.remove().argument("image", Value::text("photo.png")),
        &["image", "is no argument"],
    );
}

#[test]
fn an_input_the_store_does_not_hold_or_holds_as_another_type_is_refused() {
    let world = world();
    let engine = world.engine(segmenter());
    let nothing = ArtifactId(Digest::of_bytes(b"nothing"));
    let refusal = refused(
        engine.prepare(&Call::new("seam").input("image", nothing)),
        &["the input image", "the store holds no image"],
    );
    assert!(matches!(refusal, Refusal::Input { .. }));
    // A mask where an image is required, and an image where a mask is.
    refused(
        engine.prepare(&Call::new("seam").input("image", world.mask_on_photo)),
        &[
            "the input image",
            "an image is required, and this is a mask",
        ],
    );
    refused(
        engine.prepare(&world.remove().input("protect", world.other)),
        &[
            "the input protect",
            "a mask is required, and this is an image",
        ],
    );
}

// --- which image a mask is on --------------------------------------------------------------

#[test]
fn a_mask_is_accepted_on_its_own_image_and_refused_on_another_of_the_same_size() {
    let world = world();
    let engine = world.engine(segmenter());
    engine
        .prepare(&world.remove().input("protect", world.mask_on_photo))
        .unwrap();
    let refusal = refused(
        engine.prepare(&world.remove().input("protect", world.mask_on_other)),
        &[
            "the input protect",
            "the mask belongs to the image",
            "rebind it",
        ],
    );
    assert!(matches!(refusal, Refusal::Input { .. }));
}

#[test]
fn what_is_not_a_mask_is_held_to_its_image_too() {
    let world = world();
    let engine = world.engine(NoBackends);
    let look = |image: ArtifactId, place: ArtifactId| {
        engine.prepare(
            &Call::new("look")
                .input("image", image)
                .input("place", place),
        )
    };
    look(world.photo, world.place_on_photo).unwrap();
    let refusal = refused(
        look(world.other, world.place_on_photo),
        &[
            "the input place",
            "wrong image",
            "the location belongs to the image",
        ],
    );
    assert!(matches!(refusal, Refusal::Input { .. }));
}

#[test]
fn a_mask_made_by_hand_on_an_image_is_a_mask_on_that_image() {
    // The decision of ADR 0004 on the way round a rebind. A mask that was never rebound,
    // because it was made directly on the second photograph (`Mask::from_core`, or a file
    // brought in by `whizbang store import`), is a statement about that photograph by
    // whoever made it, and is used on it. It was never a mask on the first.
    let world = world();
    let engine = world.engine(segmenter());
    let on_other = Call::new("remove_prompt")
        .argument("prompt", Value::text("a dog"))
        .input("image", world.other)
        .input("protect", world.mask_on_other);
    engine.prepare(&on_other).unwrap();
}

#[test]
fn a_mask_rebound_by_a_stated_derivation_is_still_taken_at_its_word() {
    // The gap ADR 0002 accepted and ADR 0004 narrows but does not close. No call can
    // carry a derivation, so a door cannot ask for a rebind. But a mask that code moved
    // with `Derivation::stated` names the second photograph, and prepare believes it:
    // nothing in a mask says who vouched. The fill composite closes this when it becomes
    // the one thing that makes a derivation (task A8, `docs/carry-over.md`); this test is
    // then inverted, with `rebind::a_derivation_is_trusted_to_be_true`.
    let world = world();
    let engine = world.engine(segmenter());
    let on_other = Call::new("remove_prompt")
        .argument("prompt", Value::text("a dog"))
        .input("image", world.other)
        .input("protect", world.moved);
    engine.prepare(&on_other).unwrap();
    // On the photograph it was moved from, it is refused like any mask on another image.
    refused(
        engine.prepare(&world.remove().input("protect", world.moved)),
        &["the input protect", "the mask belongs to the image"],
    );
}

#[test]
fn a_call_has_no_place_for_a_derivation() {
    // What a door can hand the executor: an action's name, values, and artifacts by
    // identity. A derivation is none of these, so one stated by hand never arrives.
    let call = Call::new("remove_prompt")
        .argument("prompt", Value::text("a dog"))
        .input("image", ArtifactId(Digest::of_bytes(b"an image")));
    let arguments: Vec<&str> = call.arguments().keys().map(String::as_str).collect();
    let inputs: Vec<&str> = call.inputs().keys().map(String::as_str).collect();
    assert_eq!(
        (call.action(), arguments, inputs),
        ("remove_prompt", vec!["prompt"], vec!["image"])
    );
    let _: fn(Call, &str, Value) -> Call = Call::argument;
    let _: fn(Call, &str, ArtifactId) -> Call = Call::input;
    let _: fn(&Mask, &Derivation) -> artifact::Result<Mask> = Mask::rebind;
}

// --- bound to what it saw -------------------------------------------------------------------

#[test]
fn the_same_call_prepared_twice_has_one_fingerprint() {
    let world = world();
    let first = world.engine(segmenter()).prepare(&world.remove()).unwrap();
    let second = world.engine(segmenter()).prepare(&world.remove()).unwrap();
    assert_eq!(first.fingerprint(), second.fingerprint());
    assert_eq!(first, second);
    // A default given by hand is the call it would have been without it.
    let explicit = world.remove().argument("strict", Value::Boolean(false));
    let explicit = world.engine(segmenter()).prepare(&explicit).unwrap();
    assert_eq!(first.fingerprint(), explicit.fingerprint());
}

#[test]
fn the_fingerprint_changes_with_an_argument_an_input_the_config_and_a_model() {
    let world = world();
    let base = world.engine(segmenter()).prepare(&world.remove()).unwrap();
    let differs = |prepared: PreparedCall, what: &str| {
        assert_ne!(base.fingerprint(), prepared.fingerprint(), "{what}");
    };
    let engine = world.engine(segmenter());
    differs(
        engine
            .prepare(
                &Call::new("remove_prompt")
                    .argument("prompt", Value::text("a cat"))
                    .input("image", world.photo),
            )
            .unwrap(),
        "an argument's value",
    );
    differs(
        engine
            .prepare(&world.remove().argument("strict", Value::Boolean(true)))
            .unwrap(),
        "a default replaced",
    );
    differs(
        engine
            .prepare(
                &Call::new("remove_prompt")
                    .argument("prompt", Value::text("a dog"))
                    .input("image", world.other),
            )
            .unwrap(),
        "the input",
    );
    differs(
        engine
            .prepare(&world.remove().input("protect", world.mask_on_photo))
            .unwrap(),
        "one more input",
    );
    differs(
        world
            .engine_with(segmenter(), b"another config")
            .prepare(&world.remove())
            .unwrap(),
        "the config",
    );
    let newer = Offered::default().with(Capability::Segment, &["phrase"], "sam-2");
    differs(
        world.engine(newer).prepare(&world.remove()).unwrap(),
        "the model",
    );
    // A capability that is missing is seen too: ready and not ready are two preparations.
    differs(
        world.engine(NoBackends).prepare(&world.remove()).unwrap(),
        "no model",
    );
    assert_eq!(base.config(), Digest::of_bytes(b"config"));
}

#[test]
fn a_fingerprint_is_this_digest() {
    // A fixed vector: `prepared/1` is hashed as ADR 0004 says, and a change to what is
    // hashed, or to its order, shows here.
    let registry = {
        let mut registry = Registry::new();
        let draft = ActionDescriptor::draft(
            "fake",
            "1.2.3",
            whizbang_core::contract::Kind::Primitive,
            "A fake action.",
        )
        .argument(whizbang_core::contract::Argument::with_default(
            "px",
            "A radius.",
            whizbang_core::contract::ValueType::number(None, None),
            Value::Number(8.0),
        ));
        registry
            .register(Arc::new(Fake(draft.build().unwrap())))
            .unwrap();
        registry
    };
    let world = world();
    let reader = Reader::open(&world.root, world.fs.clone()).unwrap();
    let engine = Engine::new(
        registry,
        Arc::new(NoBackends),
        Arc::new(reader),
        Digest::of_bytes(b"config"),
    );
    let prepared = engine.prepare(&Call::new("fake")).unwrap();
    assert_eq!(
        prepared.fingerprint().to_string(),
        "sha256:0e759b475b959c953931682f551a7c8c2e13a7e596846bf8b52132c500a6619f"
    );
}

// --- no side effects -------------------------------------------------------------------------

/// Every file under the store, with its bytes.
fn tree(root: &Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                let bytes = std::fs::read(&path).unwrap();
                files.push((path, bytes));
            }
        }
    }
    files.sort();
    files
}

#[test]
fn prepare_changes_nothing_in_the_store() {
    let world = world();
    let engine = world.engine(segmenter());
    let events = world.fs.events().len();
    let before = tree(&world.root);
    // Ready, not ready, and refused at each step: none of them writes.
    engine.prepare(&world.remove()).unwrap();
    engine
        .prepare(&world.remove().argument("filler", Value::text("bria")))
        .unwrap();
    engine
        .prepare(&world.remove().input("protect", world.mask_on_photo))
        .unwrap();
    engine.prepare(&Call::new("vanish")).unwrap_err();
    engine.prepare(&Call::new("remove_prompt")).unwrap_err();
    engine
        .prepare(&world.remove().input("protect", world.mask_on_other))
        .unwrap_err();
    // No file was created, written, synced, renamed, removed or locked.
    assert_eq!(
        world.fs.events().len(),
        events,
        "{:?}",
        &world.fs.events()[events..]
    );
    assert_eq!(tree(&world.root), before);
    // And a writer can still open the store: the read took no part in the writer lock.
    drop(engine);
    Store::open_existing(&world.root, world.fs.clone()).unwrap();
}

#[test]
fn prepare_asks_the_backends_what_is_there_and_once_for_each_need() {
    let world = world();
    let offered = Arc::new(segmenter());
    let reader = Reader::open(&world.root, world.fs.clone()).unwrap();
    struct Shared(Arc<Offered>);
    impl Backends for Shared {
        fn resolve(&self, c: Capability, f: &[String]) -> Result<ModelIdentity, Unavailable> {
            self.0.resolve(c, f)
        }
    }
    let engine = Engine::new(
        registry(),
        Arc::new(Shared(offered.clone())),
        Arc::new(reader),
        Digest::of_bytes(b"config"),
    );
    engine.prepare(&world.remove()).unwrap();
    assert_eq!(offered.asked.load(Ordering::SeqCst), 1);
    engine
        .prepare(&world.remove().argument("filler", Value::text("bria")))
        .unwrap();
    assert_eq!(offered.asked.load(Ordering::SeqCst), 3);
    // A call refused before its needs are looked at asks nothing.
    engine.prepare(&Call::new("remove_prompt")).unwrap_err();
    assert_eq!(offered.asked.load(Ordering::SeqCst), 3);
}

#[test]
#[ignore = "for tasks A6 and A8: the fakes that count capability requests and reservations do not exist yet"]
fn prepare_sends_no_capability_request_and_makes_no_reservation() {
    // What part 2 can show is above: no change in the store, and that `Backends`, the
    // only thing prepare is given beside a read of the store, has no way to send a
    // request or to reserve. What it cannot show is a count of zero on the capabilities
    // themselves and on the ledger, because neither has a fake to count with. Task A8
    // brings the fake segmenter, filler and judge; task A6 the fake ledger. Whoever
    // builds them completes this test: prepare `remove_prompt` with a filler against
    // them and assert that each counted no request and the ledger no reservation.
    panic!("not written yet: see the comment");
}

// --- the entry point ---------------------------------------------------------------------------

#[test]
fn submit_refuses_what_prepare_refuses_and_what_is_not_ready() {
    let world = world();
    let engine = world.engine(segmenter());
    assert!(matches!(
        engine.submit(&Call::new("vanish")),
        Err(Refusal::UnknownAction { .. })
    ));
    assert!(matches!(
        engine.submit(&Call::new("remove_prompt")),
        Err(Refusal::Invalid { .. })
    ));
    let filled = world.remove().argument("filler", Value::text("bria"));
    let Err(Refusal::NotReady { action, missing }) = engine.submit(&filled) else {
        panic!("submitted without a filler");
    };
    assert_eq!(action, "remove_prompt");
    assert_eq!(missing[0].capability(), Capability::Fill);
}

#[test]
fn submit_runs_nothing_yet() {
    // A call that is ready is refused all the same, by name: admission and execution are
    // not built. The part of task A5 that runs `echo` replaces this refusal, and this
    // test, with a run.
    let world = world();
    let engine = world.engine(segmenter());
    let refusal = engine.submit(&world.remove()).unwrap_err();
    assert_eq!(refusal, Refusal::NothingRunsYet);
    assert!(refusal.to_string().contains("cannot run an action yet"));
}
