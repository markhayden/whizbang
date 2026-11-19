//! `outputs::…`: what an action hands back is held to everything the descriptor says of
//! each output port, before anything is published (plan m3, task F1, item 5): the image
//! a mask is on, the image another is derived from, the rebind of a mask onto a derived
//! image, the action its origin names, and the executor's decode limits. Each relation
//! is refused when it does not hold, and the same call with it holding completes.

use std::sync::Arc;

use whizbang_core::artifact::{
    self, Artifact, ArtifactId, ArtifactType, Derivation, Image, ImageRole, Mask, Source, UseCase,
};
use whizbang_core::contract::{
    Action, ActionDescriptor, Call, Context, Engine, Executor, Failure, InputPort, Kind,
    NoBackends, Outcome, OutputPort, PortRef, PreparedCall, Produced, Registry,
};
use whizbang_core::identity::{
    Binding, CanonicalPixels, DecodeLimits, Digest, Projection, Topology, encode,
};
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::scratch::Scratch;

use crate::artifact::support::{image_by, limits, made_by, on, photo, rect, rect_mask_by};

type Body = Box<dyn Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync>;

struct Scripted(ActionDescriptor, Body);

impl Action for Scripted {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        (self.1)(call, context)
    }
}

/// A store with the photograph and a mask on it, and an executor over it.
struct World {
    _dir: Scratch,
    store: Arc<Store>,
    image: Image,
    mask: Mask,
}

impl World {
    fn new(label: &str) -> World {
        let dir = Scratch::new(label);
        let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap());
        let image = image_by(photo(), ImageRole::Picture, "import");
        let mask = rect_mask_by(image.binding(), "import");
        let mut publication = store.begin().unwrap();
        artifact::put(&mut publication, &Artifact::from(image.clone())).unwrap();
        artifact::put(&mut publication, &Artifact::from(mask.clone())).unwrap();
        publication.publish().unwrap();
        World {
            _dir: dir,
            store,
            image,
            mask,
        }
    }

    fn engine(&self, action: ActionDescriptor, body: Body) -> Engine {
        let mut registry = Registry::new();
        registry.register(Arc::new(Scripted(action, body))).unwrap();
        Engine::over(
            registry,
            Arc::new(NoBackends),
            self.store.clone(),
            Digest::of_bytes(b""),
        )
    }

    fn pixels(&self) -> CanonicalPixels {
        self.image.pixels(&limits()).unwrap()
    }

    fn holds(&self, artifact: ArtifactType, id: ArtifactId) -> bool {
        artifact::get(&*self.store, artifact, id, &limits()).is_ok()
    }
}

/// A mask on this binding whose core is the box from `x0, y0` to `x1, y1`, its origin
/// naming `action`. A mask's identity is of its planes and its binding, not its origin.
fn box_by(binding: Binding, (x0, y0, x1, y1): (u32, u32, u32, u32), action: &str) -> Mask {
    let core = rect(x0, y0, x1, y1);
    Mask::from_core(
        binding,
        UseCase::Custom,
        core,
        Source::default(),
        made_by(action),
    )
    .unwrap()
}

/// Runs the action once with the world's image and mask: the outcome.
fn ran(world: &World, action: ActionDescriptor, body: Body) -> Outcome {
    let engine = world.engine(action.clone(), body);
    let mut call = Call::new(action.name());
    for input in action.inputs() {
        let id = match input.artifact() {
            ArtifactType::Image => world.image.id(),
            _ => world.mask.id(),
        };
        call = call.input(input.name(), id);
    }
    engine.submit(&call).unwrap().outcome().clone()
}

fn refused(outcome: &Outcome, said: &str) {
    match outcome {
        Outcome::Error { kind, message } => {
            assert_eq!(kind, "output", "{message}");
            assert!(message.contains(said), "{message}");
        }
        other => panic!("not refused: {other:?}"),
    }
}

fn completed(outcome: &Outcome) {
    assert_eq!(outcome.name(), "completed", "{outcome:?}");
}

/// An image of these pixels whose origin names `action`.
fn image_of(pixels: &CanonicalPixels, action: &str) -> Image {
    Image::new(
        encode::png(pixels).unwrap(),
        Projection::Flat,
        Topology::Bounded,
        ImageRole::Picture,
        made_by(action),
        &limits(),
    )
    .unwrap()
}

/// The photograph with the pixel at this place, counted row by row, made white.
fn changed_at(pixels: &CanonicalPixels, at: usize) -> CanonicalPixels {
    let mut rgba = pixels.rgba().to_vec();
    rgba[at * 4..at * 4 + 3].copy_from_slice(&[255, 255, 254]);
    CanonicalPixels::from_rgba(pixels.width(), pixels.height(), rgba).unwrap()
}

/// A pixel the world's mask weighs over 0 (inside its box from 2, 1 to 5, 4), and one it
/// weighs nothing.
const INSIDE: usize = 2 * 8 + 3;
const OUTSIDE: usize = 0;

/// `finds`: an image in, a mask on it out.
fn finds() -> ActionDescriptor {
    ActionDescriptor::draft("finds", "1.0.0", Kind::Primitive, "A test action.")
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The image.",
        ))
        .output(OutputPort::one("mask", ArtifactType::Mask, "A mask.").on(PortRef::input("image")))
        .build()
        .unwrap()
}

/// `pastes`: an image and a mask in, an image derived from the first out.
fn pastes() -> ActionDescriptor {
    ActionDescriptor::draft("pastes", "1.0.0", Kind::Primitive, "A test action.")
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The image.",
        ))
        .input(InputPort::required("mask", ArtifactType::Mask, "The mask.").on("image"))
        .output(
            OutputPort::one("image", ArtifactType::Image, "The composite.")
                .derived_from(PortRef::input("image")),
        )
        .build()
        .unwrap()
}

/// `fills`: an image and a mask in; the composite and the mask rebound onto it out.
fn fills() -> ActionDescriptor {
    ActionDescriptor::draft("fills", "1.0.0", Kind::Primitive, "A test action.")
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The image.",
        ))
        .input(InputPort::required("mask", ArtifactType::Mask, "The mask.").on("image"))
        .output(
            OutputPort::one("image", ArtifactType::Image, "The composite.")
                .derived_from(PortRef::input("image")),
        )
        .output(
            OutputPort::one("mask", ArtifactType::Mask, "The mask, rebound.")
                .on(PortRef::input("image"))
                .rebound_onto("image"),
        )
        .build()
        .unwrap()
}

#[test]
fn a_mask_on_another_image_than_its_port_names_is_refused() {
    let world = World::new("outputs-on");
    let elsewhere = rect_mask_by(on("another picture"), "finds");
    let id = elsewhere.id();
    let outcome = ran(
        &world,
        finds(),
        Box::new(move |_, _| Ok(Produced::nothing().with("mask", elsewhere.clone()))),
    );
    refused(
        &outcome,
        "the output mask is on another image than its port names",
    );
    assert!(!world.holds(ArtifactType::Mask, id));
    let on_it = box_by(world.image.binding(), (0, 0, 2, 2), "finds");
    completed(&ran(
        &world,
        finds(),
        Box::new(move |_, _| Ok(Produced::nothing().with("mask", on_it.clone()))),
    ));
}

#[test]
fn an_image_not_derived_from_the_one_its_port_names_is_refused() {
    // Changed where the mask the call gave weighs nothing, and another picture of the
    // size: each refused, and neither committed. Changed only where the mask weighs
    // over 0, it completes.
    let world = World::new("outputs-derived");
    let pixels = world.pixels();
    let outside = image_of(&changed_at(&pixels, OUTSIDE), "pastes");
    let other = image_of(&changed_at(&changed_at(&pixels, OUTSIDE), INSIDE), "pastes");
    for wrong in [outside, other] {
        let id = wrong.id();
        let outcome = ran(
            &world,
            pastes(),
            Box::new(move |_, _| Ok(Produced::nothing().with("image", wrong.clone()))),
        );
        refused(
            &outcome,
            "the output image is not derived from the image its port names",
        );
        assert!(!world.holds(ArtifactType::Image, id));
    }
    let inside = image_of(&changed_at(&pixels, INSIDE), "pastes");
    completed(&ran(
        &world,
        pastes(),
        Box::new(move |_, _| Ok(Produced::nothing().with("image", inside.clone()))),
    ));
}

#[test]
fn a_mask_not_rebound_onto_the_image_its_port_names_is_refused() {
    // A composite made through the mask, handed back with the mask still on the image
    // it was made from: refused. With the mask rebound through the checked derivation,
    // it completes; and the image and the mask the call gave, handed back unchanged,
    // complete too, the mask keeping its binding.
    let world = World::new("outputs-rebound");
    let pixels = world.pixels();
    let derived = changed_at(&pixels, INSIDE);
    let composite = image_of(&derived, "fills");
    let mut recorded = world.mask.clone();
    recorded
        .record(whizbang_core::artifact::Step::new("fills", "1"))
        .unwrap();
    let derivation = Derivation::checked(&pixels, &derived, &world.mask).unwrap();
    let rebound = recorded.rebind(&derivation).unwrap();
    let (image, kept) = (composite.clone(), recorded.clone());
    let outcome = ran(
        &world,
        fills(),
        Box::new(move |_, _| {
            Ok(Produced::nothing()
                .with("image", image.clone())
                .with("mask", kept.clone()))
        }),
    );
    refused(
        &outcome,
        "the output mask is not on the image it is rebound onto",
    );
    let image = composite.clone();
    completed(&ran(
        &world,
        fills(),
        Box::new(move |_, _| {
            Ok(Produced::nothing()
                .with("image", image.clone())
                .with("mask", rebound.clone()))
        }),
    ));
    let (image, mask) = (world.image.clone(), world.mask.clone());
    completed(&ran(
        &world,
        fills(),
        Box::new(move |_, _| {
            Ok(Produced::nothing()
                .with("image", image.clone())
                .with("mask", mask.clone()))
        }),
    ));
}

#[test]
fn an_output_whose_origin_names_another_action_is_refused() {
    let world = World::new("outputs-origin");
    // A mask the store does not hold: one it holds keeps the file it has, whose origin
    // was held when it was committed.
    let made = rect_mask_by(on("a picture of its own"), "someone-else");
    let id = made.id();
    let outcome = ran(
        &world,
        finds(),
        Box::new(move |_, _| Ok(Produced::nothing().with("mask", made.clone()))),
    );
    refused(&outcome, "an artifact whose origin does not name it");
    assert!(!world.holds(ArtifactType::Mask, id));
}

#[test]
fn an_output_past_the_decode_limits_is_refused_before_publication() {
    // The executor's limits are 40 pixels; the mask handed back is 8 by 6.
    let world = World::new("outputs-limits");
    let made = rect_mask_by(on("elsewhere"), "makes");
    let id = made.id();
    let action = ActionDescriptor::draft("makes", "1.0.0", Kind::Primitive, "A test action.")
        .output(OutputPort::one("mask", ArtifactType::Mask, "A mask."))
        .build()
        .unwrap();
    let small = DecodeLimits {
        max_pixels: 40,
        ..DecodeLimits::default()
    };
    let engine = world
        .engine(
            action,
            Box::new(move |_, _| Ok(Produced::nothing().with("mask", made.clone()))),
        )
        .with_decode_limits(small);
    let outcome = engine
        .submit(&Call::new("makes"))
        .unwrap()
        .outcome()
        .clone();
    refused(&outcome, "not read within the executor's decode limits");
    assert!(!world.holds(ArtifactType::Mask, id));
}

#[test]
fn a_stored_artifact_neither_given_nor_made_by_a_call_it_made_is_held_to_its_origin() {
    // The store holds a mask on the image that another action made. Handed back by
    // `finds`, which was not given it and called nothing that made it, it is refused:
    // the record would list another action's mask as this one's output. One the store
    // holds that `finds` made before, as a run made again hands back, completes; and so
    // does the mask the call gave, handed back as it was (`fills`, above).
    let world = World::new("outputs-stored");
    let other = box_by(world.image.binding(), (0, 0, 2, 2), "someone-else");
    let again = box_by(world.image.binding(), (1, 1, 3, 3), "finds");
    let mut publication = world.store.begin().unwrap();
    artifact::put(&mut publication, &Artifact::from(other.clone())).unwrap();
    artifact::put(&mut publication, &Artifact::from(again.clone())).unwrap();
    publication.publish().unwrap();
    assert_ne!(other.id(), again.id());
    let outcome = ran(
        &world,
        finds(),
        Box::new(move |_, _| Ok(Produced::nothing().with("mask", other.clone()))),
    );
    refused(
        &outcome,
        "an artifact the store holds already, made by another action",
    );
    // The same mask with an origin of its own that names `finds` is the same artifact:
    // what the record would list is the file the store keeps.
    let disguised = box_by(world.image.binding(), (0, 0, 2, 2), "finds");
    let outcome = ran(
        &world,
        finds(),
        Box::new(move |_, _| Ok(Produced::nothing().with("mask", disguised.clone()))),
    );
    refused(
        &outcome,
        "an artifact the store holds already, made by another action",
    );
    completed(&ran(
        &world,
        finds(),
        Box::new(move |_, _| Ok(Produced::nothing().with("mask", again.clone()))),
    ));
}
