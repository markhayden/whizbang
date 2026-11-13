//! Prepare (`SPEC.md` section 3, ADR 0004): a call checked against its action's
//! descriptor, its inputs found in the store, readiness for these exact arguments, and a
//! fingerprint of everything that was seen. Nothing is written and nothing is sent.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use whizbang_core::artifact::{
    self, Artifact, ArtifactId, ArtifactType, Derivation, ImageRole, Mask, Source, UseCase,
};
use whizbang_core::capability::{Declaration, Filler, Judge, Roster, Segmenter};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Backends, Call, CallError, Capability, Count, Engine,
    Executor, InputFault, InputPort, Kind, ModelIdentity, NoBackends, PreparedCall, Readiness,
    Refusal, Registry, Requirement, Unavailable, Value, ValueType,
};
use whizbang_core::identity::{Binding, Digest, Direction, Mapping, Projection, Space, Topology};
use whizbang_core::store::{Reader, Store};
use whizbang_testkit::capabilities::{FakeFiller, FakeJudge, FakeSegmenter, Paint};
use whizbang_testkit::fs_shim::ShimFs;
use whizbang_testkit::scratch::Scratch;

use super::fixtures;
use crate::artifact::support::{
    image_of, line, made_by, painted_through, photo, png, rect, rect_mask,
};

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
    for draft in [fixtures::seam(), fixtures::remove_prompt(), look, every()] {
        registry
            .register(Arc::new(Fake(draft.build().unwrap())))
            .unwrap();
    }
    registry
}

/// An action with an argument of every kind, one that never has a value, a second input
/// port that takes several images, and two needs with features.
fn every() -> whizbang_core::contract::Draft {
    ActionDescriptor::draft(
        "every",
        "2.0.0",
        Kind::Primitive,
        "Takes one of everything.",
    )
    .argument(Argument::required(
        "amount",
        "A number.",
        ValueType::number(None, None),
    ))
    .argument(Argument::optional(
        "skipped",
        "Never given, and without a default.",
        ValueType::Text,
    ))
    .argument(Argument::required(
        "count",
        "A whole number.",
        ValueType::integer(None, None),
    ))
    .argument(Argument::with_default(
        "keep",
        "A boolean.",
        ValueType::Boolean,
        Value::Boolean(true),
    ))
    .argument(Argument::required("label", "A text.", ValueType::Text))
    .input(InputPort::required(
        "image",
        ArtifactType::Image,
        "The image.",
    ))
    .input(InputPort::variadic(
        "extras",
        ArtifactType::Image,
        "More images.",
        Count::between(0, 4),
    ))
    .needs(
        Requirement::new(Capability::Segment)
            .feature("phrase")
            .feature("box"),
    )
    .needs(Requirement::new(Capability::Fill).feature("wide"))
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
        _: Option<&str>,
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
    /// The first photograph's mask as it would be on the second: made by hand on the
    /// binding that places the second photograph onto the first by the identity, which
    /// is what a derivation of the second from the first would hold. No derivation made
    /// it, and none could: the two are unrelated.
    moved: ArtifactId,
    place_on_photo: ArtifactId,
    /// The first photograph's pixels under another space of the same size, and a place
    /// stated on that.
    place_in_another_space: ArtifactId,
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
    // The second photograph "derived from" the first: untrue. `Derivation::checked`
    // refuses the two pictures, and the binding can still be written by hand.
    let untrue = Binding::new(
        second.identity().pixels,
        second.space(),
        first.identity().pixels,
        first.space(),
        Direction::SourceToDestination,
        Mapping::Identity,
    )
    .unwrap();
    let moved = Mask::from_core(
        untrue,
        UseCase::Custom,
        rect(2, 1, 5, 4),
        Source::default(),
        made_by("rect"),
    )
    .unwrap();
    let place = line(first.binding());
    let bounded_panorama = Space::new(
        first.space().width(),
        first.space().height(),
        Projection::Equirectangular,
        Topology::Bounded,
    )
    .unwrap();
    let elsewhere = line(Binding::root(first.identity().pixels, bounded_panorama));
    let world = World {
        photo: first.id(),
        other: second.id(),
        mask_on_photo: mask_on_photo.id(),
        mask_on_other: mask_on_other.id(),
        moved: moved.id(),
        place_on_photo: place.id(),
        place_in_another_space: elsewhere.id(),
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
        Artifact::from(elsewhere),
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
fn refused(result: Result<PreparedCall, CallError>, what: &[&str]) -> Refusal {
    let refusal = match result {
        Ok(prepared) => {
            panic!("prepared, where a refusal saying {what:?} was expected: {prepared:?}")
        }
        Err(CallError::Refused(refusal)) => refusal,
        Err(CallError::Store(failure)) => {
            panic!("the store failed, where a refusal saying {what:?} was expected: {failure}")
        }
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
    assert_eq!(alone.needs()[0].features(), ["phrase".to_owned()]);
    assert_eq!(alone.needs()[0].model().unwrap().model(), "sam");
    assert_eq!(alone.readiness(), &Readiness::Ready);
    assert_eq!(alone.outputs(), ["mask", "found"]);

    // With a filler: a filler is needed as well, and nothing offers one.
    let filled = world.remove().argument("filler", Value::text("bria"));
    let filled = engine.prepare(&filled).unwrap();
    let capabilities: Vec<Capability> = filled.needs().iter().map(|n| n.capability()).collect();
    assert_eq!(capabilities, [Capability::Segment, Capability::Fill]);
    assert_eq!(filled.needs()[0].features(), ["phrase".to_owned()]);
    assert!(filled.needs()[1].features().is_empty());
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

/// The backends as ADR 0006 resolves them: two segmenters, `sam` the default, and one
/// filler or two, `another` the default.
fn roster(with_bria: bool) -> Roster {
    let declared = |model: &str, features: &[&str]| {
        Declaration::new(ModelIdentity::new("fake", model, "1"), features).unwrap()
    };
    let roster = Roster::new()
        .offer(
            Capability::Segment,
            "sam",
            declared("sam3", &["phrase", "box"]),
        )
        .unwrap()
        .offer(Capability::Segment, "tiny", declared("owl-tiny", &["box"]))
        .unwrap()
        .offer(Capability::Fill, "another", declared("another-filler", &[]))
        .unwrap()
        .default_for(Capability::Segment, "sam")
        .unwrap()
        .default_for(Capability::Fill, "another")
        .unwrap();
    if with_bria {
        roster
            .offer(Capability::Fill, "bria", declared("bria-erase", &[]))
            .unwrap()
    } else {
        roster
    }
}

#[test]
fn a_filler_named_in_a_call_is_prepared_against_that_filler() {
    // The inverse of the pin this replaced. `remove_prompt` says its filler is chosen by
    // the argument `filler`. Where no backend of that name is offered the call is not
    // ready, for want of "bria": the default filler is not used in its place.
    let world = world();
    let filled = world.remove().argument("filler", Value::text("bria"));
    let prepared = world.engine(roster(false)).prepare(&filled).unwrap();
    let Readiness::NotReady(missing) = prepared.readiness() else {
        panic!("ready, with a filler that is not the one the call named");
    };
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].capability(), Capability::Fill);
    assert_eq!(missing[0].why(), "no fill backend is named \"bria\"");
    assert_eq!(prepared.needs()[1].capability(), Capability::Fill);
    assert_eq!(prepared.needs()[1].choice(), Some("bria"));
    assert!(prepared.needs()[1].model().is_none());

    // Where it is offered, the call is prepared against it, and not against the default.
    let prepared = world.engine(roster(true)).prepare(&filled).unwrap();
    assert_eq!(prepared.readiness(), &Readiness::Ready);
    assert_eq!(prepared.needs()[1].choice(), Some("bria"));
    assert_eq!(prepared.needs()[1].model().unwrap().model(), "bria-erase");
    // The default, named, is the default.
    let named = world.remove().argument("filler", Value::text("another"));
    let prepared = world.engine(roster(true)).prepare(&named).unwrap();
    assert_eq!(
        prepared.needs()[1].model().unwrap().model(),
        "another-filler"
    );
}

#[test]
fn a_call_that_names_no_backend_gets_the_default_and_one_that_names_one_gets_it() {
    let world = world();
    let engine = world.engine(roster(true));
    // `remove_prompt`'s segmenter is chosen by `model`. Left out: the default.
    let default = engine.prepare(&world.remove()).unwrap();
    assert_eq!(default.readiness(), &Readiness::Ready);
    assert_eq!(default.needs()[0].choice(), None);
    assert_eq!(default.needs()[0].model().unwrap().model(), "sam3");

    // Named: that one, and its features are held to what the call needs. `tiny` takes
    // no phrase. The default does, and is not used in its place.
    let tiny = world.remove().argument("model", Value::text("tiny"));
    let prepared = engine.prepare(&tiny).unwrap();
    let Readiness::NotReady(missing) = prepared.readiness() else {
        panic!("ready, with a segmenter that takes no phrase");
    };
    assert_eq!(
        missing[0].why(),
        "the segment backend tiny does not declare the feature \"phrase\""
    );
    assert_eq!(prepared.needs()[0].choice(), Some("tiny"));
    assert!(prepared.needs()[0].model().is_none());

    // A need with no choosing argument is always answered by the default: `every`
    // names none, whatever its text arguments hold.
    let every = Call::new("every")
        .argument("amount", Value::Number(1.0))
        .argument("count", Value::Integer(1))
        .argument("label", Value::text("tiny"))
        .input("image", world.photo);
    let prepared = engine.prepare(&every).unwrap();
    assert_eq!(prepared.needs()[0].choice(), None);
    assert_eq!(prepared.needs()[0].model().unwrap().model(), "sam3");
}

#[test]
fn the_backend_a_choice_resolves_to_is_part_of_what_a_preparation_saw() {
    let world = world();
    let filled = world.remove().argument("filler", Value::text("bria"));
    let without = world.engine(roster(false)).prepare(&filled).unwrap();
    let with = world.engine(roster(true)).prepare(&filled).unwrap();
    assert_ne!(without.fingerprint(), with.fingerprint());
    // The same call against the same backends is the same preparation.
    let again = world.engine(roster(true)).prepare(&filled).unwrap();
    assert_eq!(with.fingerprint(), again.fingerprint());
    // Another choice is another preparation.
    let other = world.remove().argument("filler", Value::text("another"));
    let other = world.engine(roster(true)).prepare(&other).unwrap();
    assert_ne!(with.fingerprint(), other.fingerprint());
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
    // The sentence names the identity that was asked for.
    assert!(
        refusal
            .to_string()
            .ends_with(&format!("the store holds no image {nothing}")),
        "{refusal}"
    );
    // The refusal says which case it is as a type, and names the artifact by identity.
    assert_eq!(
        refusal,
        Refusal::Input {
            action: "seam".to_owned(),
            port: "image".to_owned(),
            artifact: ArtifactType::Image,
            id: nothing,
            fault: InputFault::Missing,
        }
    );
    // A mask where an image is required, and an image where a mask is.
    let refusal = refused(
        engine.prepare(&Call::new("seam").input("image", world.mask_on_photo)),
        &[
            "the input image",
            "an image is required, and this is a mask",
        ],
    );
    assert_eq!(
        refusal,
        Refusal::Input {
            action: "seam".to_owned(),
            port: "image".to_owned(),
            artifact: ArtifactType::Image,
            id: world.mask_on_photo,
            fault: InputFault::WrongType {
                found: "a mask".to_owned()
            },
        }
    );
    refused(
        engine.prepare(&world.remove().input("protect", world.other)),
        &[
            "the input protect",
            "a mask is required, and this is an image",
        ],
    );
}

/// The file the store keeps an artifact's content in.
fn object_of(world: &World, artifact: ArtifactType, id: ArtifactId) -> std::path::PathBuf {
    let reader = Reader::open(&world.root, world.fs.clone()).unwrap();
    let generation = reader.snapshot().unwrap();
    let entry = generation.get(&artifact::name(artifact, id)).unwrap();
    world.root.join("objects").join(entry.digest.hex())
}

#[test]
fn a_store_that_fails_is_not_a_refusal_of_the_call_and_names_no_path() {
    // The store's content for the photograph is gone: the generation names it and the
    // file is not there. That is the store's failure, not a fault of the call, and it
    // comes back as such, with the content's digest and without where the store is.
    let world = world();
    let object = object_of(&world, ArtifactType::Image, world.photo);
    let digest = object.file_name().unwrap().to_str().unwrap().to_owned();
    std::fs::remove_file(&object).unwrap();
    let engine = world.engine(segmenter());
    let Err(CallError::Store(failure)) = engine.prepare(&world.remove()) else {
        panic!("a missing file of the store was not reported as the store's failure");
    };
    let said = CallError::Store(failure.clone()).to_string();
    assert!(said.contains("the store failed"), "{said}");
    assert!(said.contains(&digest), "{said}");
    let root = world.root.to_str().unwrap();
    let parent = world.root.parent().unwrap().to_str().unwrap();
    assert!(!said.contains(root) && !said.contains(parent), "{said}");
    assert!(!said.contains(std::path::MAIN_SEPARATOR), "{said}");
    // Neither does what `{:?}` prints of it: a failure is often logged that way.
    let debugged = format!("{:?}", CallError::Store(failure.clone()));
    assert!(debugged.contains(&digest), "{debugged}");
    assert!(
        !debugged.contains(root) && !debugged.contains(parent),
        "{debugged}"
    );
    assert!(!debugged.contains(std::path::MAIN_SEPARATOR), "{debugged}");
    // Whoever runs the store can still have all of it, and asks for it by name: it is
    // not the error's source, so a chain of causes printed for a caller stops here.
    assert!(failure.cause().to_string().contains(root));
    assert!(std::error::Error::source(&CallError::Store(failure.clone())).is_none());
    assert!(CallError::Store(failure).refusal().is_none());
    // Submit says the same.
    assert!(matches!(
        engine.submit(&world.remove()),
        Err(CallError::Store(_))
    ));
}

#[test]
fn a_refusal_is_the_error_and_not_its_own_cause() {
    // Whatever prints an error and then its chain of causes says a refusal once.
    let world = world();
    let error = world
        .engine(segmenter())
        .prepare(&Call::new("vanish"))
        .unwrap_err();
    assert!(std::error::Error::source(&error).is_none());
    assert_eq!(error.to_string(), error.refusal().unwrap().to_string());
}

/// The current generation's file, edited as JSON.
fn edit_generation(world: &World, edit: impl FnOnce(&mut serde_json::Value)) {
    let reader = Reader::open(&world.root, world.fs.clone()).unwrap();
    let number = reader.snapshot().unwrap().number();
    drop(reader);
    let file = world
        .root
        .join("generations")
        .join(format!("{number}.json"));
    let mut generation: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    edit(&mut generation);
    std::fs::remove_file(&file).unwrap();
    std::fs::write(&file, serde_json::to_vec(&generation).unwrap()).unwrap();
}

#[test]
fn a_diagnostic_is_never_an_input_wherever_it_is_held() {
    let world = world();
    // Under a run's names, as a run's diagnostics are: an artifact is looked for among
    // the names outside runs, so the store holds no such image, though a run holds the
    // image's own bytes as a diagnostic.
    let unpublished = image_of(png(|x, y| [x as u8, y as u8, 200, 255]), ImageRole::Picture);
    let store = Store::open_existing(&world.root, world.fs.clone()).unwrap();
    let mut publication = store.begin().unwrap();
    let name = artifact::name(ArtifactType::Image, unpublished.id());
    let bytes = Artifact::from(unpublished.clone()).to_bytes();
    publication
        .put_in(
            "run-1",
            &name,
            whizbang_core::store::Kind::Diagnostic,
            &bytes,
        )
        .unwrap();
    publication.publish().unwrap();
    drop(store);
    let engine = world.engine(segmenter());
    let refusal = refused(
        engine.prepare(&Call::new("seam").input("image", unpublished.id())),
        &["the input image", "the store holds no image"],
    );
    assert!(matches!(
        refusal,
        Refusal::Input {
            fault: InputFault::Missing,
            ..
        }
    ));
    drop(engine);

    // At an artifact's own name, outside runs: no publication makes such a reference,
    // and a generation file that other hands wrote can. The store refuses to read a
    // diagnostic as an output, and that comes back as the store's failure, with the
    // content's digest and no path. It is not a failure the store recovers from: the
    // call is answered only when the reference is one of an output.
    let name = artifact::name(ArtifactType::Image, world.photo);
    edit_generation(&world, |generation| {
        generation["entries"][&name]["kind"] = serde_json::json!("diagnostic");
    });
    let engine = world.engine(segmenter());
    let Err(CallError::Store(failure)) = engine.prepare(&world.remove()) else {
        panic!("a diagnostic at an artifact's name was not the store's failure");
    };
    let said = failure.to_string();
    assert!(said.contains("is a diagnostic"), "{said}");
    assert!(!said.contains(world.root.to_str().unwrap()), "{said}");
}

#[test]
fn content_that_is_not_the_artifact_it_is_stored_as_is_a_fault_of_the_input() {
    // Under the name of an image the store holds bytes that are no canonical image
    // file. The store read them soundly; the input is not what its port takes.
    let world = world();
    let id = ArtifactId(Digest::of_bytes(b"not an image"));
    let store = Store::open_existing(&world.root, world.fs.clone()).unwrap();
    let mut publication = store.begin().unwrap();
    publication
        .put(&artifact::name(ArtifactType::Image, id), b"not an image")
        .unwrap();
    publication.publish().unwrap();
    drop(store);
    let engine = world.engine(segmenter());
    let refusal = refused(
        engine.prepare(&Call::new("seam").input("image", id)),
        &["the input image", "not a canonical artifact file"],
    );
    let Refusal::Input { fault, .. } = &refusal else {
        panic!("{refusal:?}");
    };
    assert!(matches!(fault, InputFault::NotTheArtifact { .. }));
    assert!(!refusal.to_string().contains(world.root.to_str().unwrap()));
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
    assert!(matches!(
        refusal,
        Refusal::Input {
            artifact: ArtifactType::Mask,
            id,
            fault: InputFault::WrongImage { .. },
            ..
        } if id == world.mask_on_other
    ));
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
fn what_is_not_a_mask_is_held_to_its_images_space_too() {
    // The same canonical pixels under another space of as many samples are not the image
    // at the port: a place stated in a bounded panorama of 8 by 6 is refused on the flat
    // photograph, and the refusal says it is the space that differs.
    let world = world();
    let engine = world.engine(NoBackends);
    let call = Call::new("look")
        .input("image", world.photo)
        .input("place", world.place_in_another_space);
    let refusal = refused(
        engine.prepare(&call),
        &[
            "the input place",
            "wrong image",
            "the location's image is in",
        ],
    );
    assert!(matches!(refusal, Refusal::Input { .. }));
    assert!(!refusal.to_string().contains("belongs to the image"));
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
fn a_mask_made_by_hand_on_an_image_placed_onto_another_is_taken_at_its_makers_word() {
    // The trust boundary (ADR 0006, "Who vouches for a derivation"): a mask is the
    // statement of whoever made it, and what holds is that it is used only on the image
    // its binding's source names. No call can carry a derivation, and no code can make
    // one of two unrelated pictures; but a mask can be made by hand on a binding that
    // places the second photograph onto the first, and prepare accepts it on the second:
    // nothing in a mask says who vouched, and prepare asks for no record that would.
    let world = world();
    // The two photographs are unrelated, and no derivation of one from the other exists.
    let limits = whizbang_core::identity::DecodeLimits::default();
    let first = image_of(photo(), ImageRole::Picture);
    let second = image_of(other_photo(), ImageRole::Picture);
    let refusal = Derivation::checked(
        &first.pixels(&limits).unwrap(),
        &second.pixels(&limits).unwrap(),
        &rect_mask(first.binding()),
    );
    assert!(matches!(
        refusal,
        Err(artifact::ArtifactError::NotDerived(_))
    ));

    let engine = world.engine(segmenter());
    let on_other = Call::new("remove_prompt")
        .argument("prompt", Value::text("a dog"))
        .input("image", world.other)
        .input("protect", world.moved);
    engine.prepare(&on_other).unwrap();
    // On the photograph its binding is placed onto, it is refused like any mask on
    // another image.
    refused(
        engine.prepare(&world.remove().input("protect", world.moved)),
        &["the input protect", "the mask belongs to the image"],
    );
}

#[test]
fn a_mask_made_by_hand_on_a_derivations_binding_is_the_rebound_mask() {
    // Why prepare cannot tell the two apart, and does not try. A derivation holds a
    // binding and adds nothing to it, and an origin is not part of an identity. So the
    // mask made directly with `Mask::from_core` on a derivation's binding has the
    // identity of the mask rebound through that derivation: one artifact, made two
    // ways, and only one way leaves a `rebind` step in its origin.
    let limits = whizbang_core::identity::DecodeLimits::default();
    let first = image_of(photo(), ImageRole::Picture);
    let pixels = first.pixels(&limits).unwrap();
    let mask = rect_mask(first.binding());
    let composite = painted_through(&pixels, &mask);
    let derivation = Derivation::checked(&pixels, &composite, &mask).unwrap();
    let rebound = mask.rebind(&derivation).unwrap();
    let by_hand = Mask::from_core(
        derivation.binding().clone(),
        UseCase::Custom,
        rect(2, 1, 5, 4),
        Source::default(),
        made_by("rect"),
    )
    .unwrap();
    assert_eq!(by_hand.id(), rebound.id());
    let steps = |mask: &Mask| -> Vec<String> {
        let origin = mask.origin().steps();
        origin.iter().map(|step| step.action.clone()).collect()
    };
    assert_eq!(steps(&by_hand), ["rect"]);
    assert_eq!(steps(&rebound), ["rect", "rebind"]);
}

#[test]
fn a_call_has_no_place_for_a_derivation() {
    // What a door can hand the executor: an action's name, values, and artifacts by
    // identity. A derivation is none of these, so none arrives through a call.
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

impl World {
    /// `every` with a value of each kind, inputs at two ports and both needs.
    fn every(&self) -> Call {
        Call::new("every")
            .argument("amount", Value::Number(2.5))
            .argument("count", Value::Integer(-3))
            .argument("label", Value::text("caf\u{e9}"))
            .input("image", self.photo)
            .input("extras", self.other)
            .input("extras", self.photo)
    }
}

#[test]
fn a_fingerprint_with_one_of_everything_is_this_digest() {
    // A second fixed vector, over every part of `prepared/1`: a value of each of the four
    // kinds (one a default, one negative, one not ASCII), an argument with no value,
    // which is not counted, inputs at two ports with two at the second, and two needs
    // with features, one answered by a model and one not.
    //
    // The expected digest was not taken from this code. It was computed by a short
    // script, written from the text of ADR 0004 ("Bound to what it saw") and of ADR 0001
    // (the encoding) alone, with SHA-256 from a standard library; the script is not kept
    // in the repository. Its inputs are the values below: the three identities, which
    // are pinned here so that the vector stands on its own, and the digest of the bytes
    // `config`.
    let world = world();
    assert_eq!(
        world.photo.to_string(),
        "sha256:87d920d7e227cffb6b4ebb586687b6612e5308da901cbe838c5b9d5cfe852420"
    );
    assert_eq!(
        world.other.to_string(),
        "sha256:c14a38efad377322dd81fd90a857e5598ebf532262fe5201d214781ac1263744"
    );
    let prepared = world.engine(segmenter()).prepare(&world.every()).unwrap();
    assert_eq!(
        prepared.arguments(),
        [
            ("amount".to_owned(), Value::Number(2.5)),
            ("count".to_owned(), Value::Integer(-3)),
            ("keep".to_owned(), Value::Boolean(true)),
            ("label".to_owned(), Value::text("caf\u{e9}")),
        ]
    );
    let inputs: Vec<(&str, ArtifactId)> = prepared
        .inputs()
        .iter()
        .map(|i| (i.port(), i.id()))
        .collect();
    assert_eq!(
        inputs,
        [
            ("image", world.photo),
            ("extras", world.other),
            ("extras", world.photo)
        ]
    );
    let needs: Vec<(Capability, &[String], Option<&str>)> = prepared
        .needs()
        .iter()
        .map(|n| (n.capability(), n.features(), n.model().map(|m| m.model())))
        .collect();
    assert_eq!(
        needs,
        [
            (
                Capability::Segment,
                &["phrase".to_owned(), "box".to_owned()][..],
                Some("sam")
            ),
            (Capability::Fill, &["wide".to_owned()][..], None),
        ]
    );
    assert_eq!(
        prepared.fingerprint().to_string(),
        "sha256:a78b0c010a0022b4272b17d8d07f203a709f347c8e3289acf79f5760fca2b636"
    );
}

#[test]
fn the_order_of_inputs_within_a_port_and_the_form_of_a_text_are_seen() {
    let world = world();
    let engine = world.engine(segmenter());
    let base = engine.prepare(&world.every()).unwrap();
    // The same two extras the other way round are another preparation.
    let swapped = Call::new("every")
        .argument("amount", Value::Number(2.5))
        .argument("count", Value::Integer(-3))
        .argument("label", Value::text("caf\u{e9}"))
        .input("image", world.photo)
        .input("extras", world.photo)
        .input("extras", world.other);
    assert_ne!(
        base.fingerprint(),
        engine.prepare(&swapped).unwrap().fingerprint()
    );
    // A text is hashed as it was given: the same word composed and decomposed is two.
    let decomposed = world.every().argument("label", Value::text("cafe\u{301}"));
    assert_ne!(
        base.fingerprint(),
        engine.prepare(&decomposed).unwrap().fingerprint()
    );
}

#[test]
fn a_negative_zero_is_prepared_as_zero() {
    // The fingerprint hashes negative zero as zero (ADR 0001), so the two are one
    // preparation; the prepared argument is zero as well, so that what is bound and what
    // an action is handed do not differ by a sign.
    let world = world();
    let engine = world.engine(segmenter());
    let zero = engine
        .prepare(&world.every().argument("amount", Value::Number(0.0)))
        .unwrap();
    let negative = engine
        .prepare(&world.every().argument("amount", Value::Number(-0.0)))
        .unwrap();
    assert_eq!(zero.fingerprint(), negative.fingerprint());
    let Value::Number(amount) = negative.arguments()[0].1 else {
        panic!("not a number: {:?}", negative.arguments()[0]);
    };
    assert_eq!(amount.to_bits(), 0.0_f64.to_bits());
    assert_eq!(zero, negative);

    // A default of negative zero, left to apply, is prepared as zero too.
    let mut registry = Registry::new();
    let draft = ActionDescriptor::draft("tilt", "1.0.0", Kind::Primitive, "Tilts.").argument(
        Argument::with_default(
            "angle",
            "An angle.",
            ValueType::number(None, None),
            Value::Number(-0.0),
        ),
    );
    registry
        .register(Arc::new(Fake(draft.build().unwrap())))
        .unwrap();
    let engine = Engine::new(
        registry,
        Arc::new(NoBackends),
        Arc::new(Reader::open(&world.root, world.fs.clone()).unwrap()),
        Digest::of_bytes(b"config"),
    );
    let by_default = engine.prepare(&Call::new("tilt")).unwrap();
    let Value::Number(angle) = by_default.arguments()[0].1 else {
        panic!("not a number: {:?}", by_default.arguments()[0]);
    };
    assert_eq!(angle.to_bits(), 0.0_f64.to_bits());
    let given = engine
        .prepare(&Call::new("tilt").argument("angle", Value::Number(0.0)))
        .unwrap();
    assert_eq!(by_default, given);
}

#[test]
fn the_same_artifact_may_be_given_more_than_once() {
    // Decided in ADR 0004: prepare holds a call to its descriptor (the type at each
    // port, how many, which image each belongs to) and does not ask whether the
    // artifacts differ. The same image twice at a port that takes several, and the image
    // of one port again at another, are accepted; each is listed as it was given, and
    // the fingerprint counts each. Whether a repeat makes sense is the action's to say.
    let world = world();
    let engine = world.engine(segmenter());
    let twice = Call::new("every")
        .argument("amount", Value::Number(2.5))
        .argument("count", Value::Integer(-3))
        .argument("label", Value::text("x"))
        .input("image", world.photo)
        .input("extras", world.other)
        .input("extras", world.other)
        .input("extras", world.photo);
    let prepared = engine.prepare(&twice).unwrap();
    let ids: Vec<ArtifactId> = prepared.inputs().iter().map(|i| i.id()).collect();
    assert_eq!(ids, [world.photo, world.other, world.other, world.photo]);
    let once = Call::new("every")
        .argument("amount", Value::Number(2.5))
        .argument("count", Value::Integer(-3))
        .argument("label", Value::text("x"))
        .input("image", world.photo)
        .input("extras", world.other)
        .input("extras", world.photo);
    assert_ne!(
        prepared.fingerprint(),
        engine.prepare(&once).unwrap().fingerprint()
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
    // No file was created, written, synced, renamed or removed, and no lock file was
    // taken. Sharing the generation's file, which each input read does, is not an event
    // of this file system: nothing durable changes by it.
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
        fn resolve(
            &self,
            c: Capability,
            choice: Option<&str>,
            f: &[String],
        ) -> Result<ModelIdentity, Unavailable> {
            self.0.resolve(c, choice, f)
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
fn prepare_sends_no_capability_request() {
    // The fakes count the requests they are sent. The backends prepare asks are built
    // from what those same fakes declare, as a build makes its roster from its
    // adapters; and preparing calls that are ready, not ready and refused sends none
    // of them a request.
    let segmenter = Arc::new(FakeSegmenter::new());
    let filler = Arc::new(FakeFiller::new(Paint::Inverse));
    let judge = Arc::new(FakeJudge::scripted(Vec::new()));
    let roster = Roster::new()
        .offer(
            Capability::Segment,
            "fake",
            Segmenter::declaration(segmenter.as_ref()).clone(),
        )
        .unwrap()
        .offer(
            Capability::Fill,
            "fake",
            Filler::declaration(filler.as_ref()).clone(),
        )
        .unwrap()
        .offer(
            Capability::Judge,
            "fake",
            Judge::declaration(judge.as_ref()).clone(),
        )
        .unwrap()
        .default_for(Capability::Segment, "fake")
        .unwrap();
    let world = world();
    let engine = world.engine(roster);

    let ready = world.remove().argument("filler", Value::text("fake"));
    let prepared = engine.prepare(&ready).unwrap();
    assert_eq!(prepared.readiness(), &Readiness::Ready);
    assert_eq!(
        prepared.needs()[0].model().unwrap().model(),
        "fake-segmenter"
    );
    assert_eq!(prepared.needs()[1].model().unwrap().model(), "fake-filler");
    let not_ready = world.remove().argument("filler", Value::text("bria"));
    let prepared = engine.prepare(&not_ready).unwrap();
    assert!(matches!(prepared.readiness(), Readiness::NotReady(_)));
    engine.prepare(&Call::new("remove_prompt")).unwrap_err();

    assert_eq!(segmenter.requests(), 0);
    assert_eq!(filler.requests(), 0);
    assert_eq!(judge.requests(), 0);
}

#[test]
#[ignore = "for task A6: the fake ledger that counts reservations does not exist yet"]
fn prepare_makes_no_reservation() {
    // The other half of what `prepare_sends_no_capability_request` shows for the
    // capabilities: a count of zero on the ledger. Task A6 brings the fake ledger.
    // Whoever builds it completes this test: prepare `remove_prompt` with a filler
    // against it and assert that the ledger counted no reservation.
    panic!("not written yet: see the comment");
}

// --- the entry point ---------------------------------------------------------------------------

#[test]
fn submit_refuses_what_prepare_refuses_and_what_is_not_ready() {
    let world = world();
    let engine = world.engine(segmenter());
    assert!(matches!(
        engine.submit(&Call::new("vanish")),
        Err(CallError::Refused(Refusal::UnknownAction { .. }))
    ));
    assert!(matches!(
        engine.submit(&Call::new("remove_prompt")),
        Err(CallError::Refused(Refusal::Invalid { .. }))
    ));
    let filled = world.remove().argument("filler", Value::text("bria"));
    let Err(CallError::Refused(Refusal::NotReady { action, missing })) = engine.submit(&filled)
    else {
        panic!("submitted without a filler");
    };
    assert_eq!(action, "remove_prompt");
    assert_eq!(missing[0].capability(), Capability::Fill);
}

#[test]
fn an_executor_that_only_reads_its_store_runs_nothing() {
    // An executor built over a read of a store was given no way to write, so a call
    // that is ready is refused by name and no run exists. Running is in `execute.rs`.
    let world = world();
    let engine = world.engine(segmenter());
    let before = tree(&world.root);
    let refusal = engine.submit(&world.remove()).unwrap_err();
    assert_eq!(refusal, CallError::Refused(Refusal::ReadOnly));
    assert!(refusal.to_string().contains("only reads its store"));
    assert_eq!(tree(&world.root), before);
}
