//! The tasks as shipped recipes, read and checked against this crate's actions and run
//! through the executor on pictures of 64 by 64: `remove_prompt` (plan m3, task R1, item
//! 5) over the test kit's fake segmenter and a fake filler that paints the support, with
//! `protect` left out and given, its last step a fill (task F1, item 6), so the mask
//! handed back is the feathered mask rebound onto the composite; and `remove`,
//! `repair_area`, `mask_rect` and `protect` (task O1, item 3), the first two under a
//! scripted filler.

use std::sync::Arc;

use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use whizbang_core::artifact::{
    self, Artifact, ArtifactId, ArtifactType, Image, ImageRole, Location, Mask, Origin, Shape,
    Source, Step, UseCase,
};
use whizbang_core::contract::{Call, Capabilities, Engine, Executor, Outcome, Registry, Value};
use whizbang_core::identity::{DecodeLimits, Digest, Point, Projection, Topology};
use whizbang_core::store::Store;
use whizbang_rascal::recipes;
use whizbang_testkit::capabilities::{FakeFiller, FakeSegmenter, Paint, roster};

use super::grow_action::World;
use super::shaping::{Shop, count, picture as sized};

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
    // What the other shipped recipes call (plan m3, task O1).
    whizbang_rascal::rasterize::register(&mut registry).unwrap();
    whizbang_rascal::rect::register(&mut registry).unwrap();
    whizbang_rascal::invert::register(&mut registry).unwrap();
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

// --- the tasks of task O1 (plan m3, task O1, item 3): `remove`, `repair_area`, `mask_rect`
// and `protect`, the shipped recipes, run on a picture of 64 by 64 under a scripted filler
// that repaints every pixel: whatever the composite keeps, the paste kept.

fn filling(label: &str) -> Shop {
    Shop::new(label, Some(Arc::new(FakeFiller::new(Paint::Inverse))))
}

fn rgba(image: &Image) -> Vec<u8> {
    image
        .pixels(&DecodeLimits::default())
        .unwrap()
        .rgba()
        .to_vec()
}

/// Every pixel the mask weighs nothing is the original's, and every core pixel the
/// filler's, the inverse; the count of core pixels.
fn held(before: &Image, after: &Image, mask: &Mask) -> usize {
    let (before, after) = (rgba(before), rgba(after));
    let mut core = 0;
    for at in 0..(SIDE * SIDE) as usize {
        let (b, a) = (&before[at * 4..at * 4 + 4], &after[at * 4..at * 4 + 4]);
        let nothing = mask.protect()[at] != 0 || (mask.core()[at] == 0 && mask.blend()[at] == 0);
        if nothing {
            assert_eq!(a, b, "pixel {at} is kept");
        } else if mask.core()[at] != 0 {
            assert_eq!(a, [255 - b[0], 255 - b[1], 255 - b[2], b[3]], "pixel {at}");
            core += 1;
        }
    }
    core
}

/// A location of this shape on the picture, in the store.
fn placed(shop: &Shop, image: &Image, shape: Shape) -> ArtifactId {
    let origin = Origin::new(Step::new("rect", "1")).unwrap();
    shop.put(Artifact::Location(
        Location::new(image.binding(), shape, origin).unwrap(),
    ))
}

fn boxed() -> Shape {
    Shape::Box {
        left: 20.0,
        top: 16.0,
        right: 36.0,
        bottom: 28.0,
    }
}

/// The composite and the mask a recipe that ends in a fill hands back.
fn filled(shop: &Shop, call: &Call) -> (Image, Mask) {
    let outputs = shop.completed(call);
    let ports: Vec<&str> = outputs.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(ports, ["image", "mask"]);
    (shop.image(outputs[0].1), shop.mask(outputs[1].1))
}

#[test]
fn remove_fills_inside_its_mask_and_keeps_every_other_pixel() {
    let shop = filling("remove");
    let image = sized(SIDE, SIDE);
    let id = shop.put(Artifact::Image(image.clone()));
    let location = placed(&shop, &image, boxed());
    let call = Call::new("remove")
        .input("image", id)
        .input("location", location);
    let (composite, mask) = filled(&shop, &call);
    let core = held(&image, &composite, &mask);
    // The box grown by 4, and 12 more rows down for the shadow: rows 12 to 43.
    let at = |x: u32, y: u32| mask.core()[(y * SIDE + x) as usize];
    assert_eq!(
        (at(28, 22), at(28, 43), at(28, 44), at(28, 11)),
        (255, 255, 0, 0)
    );
    assert!(core > 16 * 12, "{core}");
    assert!(mask.blend().iter().any(|b| *b != 0), "feathered");
    // The mask is on the composite, rebound through the fill.
    assert_eq!(mask.image(), composite.identity().pixels);
    assert_eq!(mask.use_case(), UseCase::Remove);
    let steps: Vec<&str> = mask
        .origin()
        .steps()
        .iter()
        .map(|s| s.action.as_str())
        .collect();
    assert_eq!(steps.first().copied(), Some("rasterize"));
    assert!(steps.contains(&"fill"), "{steps:?}");
}

#[test]
fn remove_completes_with_and_without_protect() {
    let shop = filling("remove-protect");
    let image = sized(SIDE, SIDE);
    let id = shop.put(Artifact::Image(image.clone()));
    let location = placed(&shop, &image, boxed());
    let call = Call::new("remove")
        .input("image", id)
        .input("location", location);
    let (_, without) = filled(&shop, &call);
    assert!(without.protect().iter().all(|p| *p == 0));
    // Protect the box's left half: the mask keeps off it and two pixels round it, and
    // every pixel there is the original's.
    let guard = Mask::from_core(
        image.binding(),
        UseCase::Custom,
        (0..SIDE * SIDE)
            .map(|at| if at % SIDE < 28 { 255 } else { 0 })
            .collect(),
        Source::default(),
        Origin::new(Step::new("fixture", "1")).unwrap(),
    )
    .unwrap();
    let guard = shop.put(guard.into());
    let (composite, with) = filled(&shop, &call.clone().input("protect", guard));
    let at = |plane: &[u8], x: u32, y: u32| plane[(y * SIDE + x) as usize];
    assert_eq!(at(with.core(), 24, 22), 0, "kept off protect");
    assert_ne!(at(with.protect(), 29, 22), 0, "protect grown by 2");
    assert_eq!(at(with.core(), 33, 22), 255);
    let core = held(&image, &composite, &with);
    assert!(core < count(without.core()), "{core}");
    let (before, after) = (rgba(&image), rgba(&composite));
    for y in 0..SIDE {
        for x in 0..28 {
            let at = ((y * SIDE + x) * 4) as usize;
            assert_eq!(after[at..at + 4], before[at..at + 4], "({x}, {y})");
        }
    }
}

#[test]
fn repair_area_fills_inside_its_mask_and_keeps_every_other_pixel() {
    let shop = filling("repair-area");
    let image = sized(SIDE, SIDE);
    let id = shop.put(Artifact::Image(image.clone()));
    // A triangle, as drawn and feathered by 3: no growing.
    let triangle = Shape::Polygon {
        points: vec![
            Point::new(10.0, 10.0),
            Point::new(40.0, 10.0),
            Point::new(10.0, 40.0),
        ],
    };
    let location = placed(&shop, &image, triangle.clone());
    let (composite, mask) = filled(
        &shop,
        &Call::new("repair_area")
            .input("image", id)
            .input("location", location),
    );
    let drawn = whizbang_rascal::rasterize::location_core(&triangle, SIDE, SIDE).unwrap();
    assert_eq!(
        mask.core(),
        drawn.as_slice(),
        "the core is the triangle as drawn"
    );
    assert_eq!(mask.use_case(), UseCase::Repair);
    assert!(mask.blend().iter().any(|b| *b != 0), "feathered");
    assert_eq!(held(&image, &composite, &mask), count(&drawn));
}

#[test]
fn mask_rect_makes_the_rectangle_as_a_mask() {
    let shop = Shop::new("mask-rect", None);
    let image = sized(SIDE, SIDE);
    let id = shop.put(Artifact::Image(image.clone()));
    let made = shop.one(
        &Call::new("mask_rect")
            .argument("box", Value::text("20 16 36 28"))
            .input("image", id),
    );
    let mask = shop.mask(made);
    assert_eq!(mask.binding(), &image.binding());
    assert_eq!(mask.use_case(), UseCase::Custom);
    for at in 0..(SIDE * SIDE) as usize {
        let (x, y) = (at as u32 % SIDE, at as u32 / SIDE);
        let inside = (20..36).contains(&x) && (16..28).contains(&y);
        assert_eq!(mask.core()[at] != 0, inside, "({x}, {y})");
    }
    assert!(mask.blend().iter().chain(mask.protect()).all(|s| *s == 0));
    assert_eq!(mask.support(), Some([20, 16, 36, 28]));
    assert_eq!(mask.origin().steps()[0].action, "rasterize");
    // A box that is not one is refused, naming the step.
    let refused = shop.refused(
        &Call::new("mask_rect")
            .argument("box", Value::text("36 16 20 28"))
            .input("image", id),
    );
    assert!(refused.contains("left < right"), "{refused}");
}

#[test]
fn protect_inverts_an_earlier_mask_onto_its_composite() {
    let shop = filling("protect");
    let image = sized(SIDE, SIDE);
    let id = shop.put(Artifact::Image(image.clone()));
    let location = placed(&shop, &image, boxed());
    let outputs = shop.completed(
        &Call::new("remove")
            .input("image", id)
            .input("location", location),
    );
    let (composite, first) = (outputs[0].1, outputs[1].1);
    let earlier = shop.mask(first);
    let made = shop.mask(
        shop.one(
            &Call::new("protect")
                .input("image", composite)
                .input("mask", first),
        ),
    );
    // On the composite; everything the first mask reached protected, the rest the
    // core, no blend.
    assert_eq!(made.image(), shop.image(composite).identity().pixels);
    assert_eq!(made.use_case(), UseCase::Protect);
    for at in 0..(SIDE * SIDE) as usize {
        let reached = earlier.core()[at] != 0 || earlier.blend()[at] != 0;
        assert_eq!(made.protect()[at] != 0, reached, "protect at {at}");
        assert_eq!(made.core()[at] != 0, !reached, "core at {at}");
    }
    assert!(made.blend().iter().all(|b| *b == 0));
    // The first mask on the picture it was made on is not on the composite: refused
    // before anything runs.
    let unbound = shop.put(Artifact::Mask(
        Mask::from_core(
            image.binding(),
            UseCase::Remove,
            earlier.core().to_vec(),
            Source::default(),
            Origin::new(Step::new("fixture", "1")).unwrap(),
        )
        .unwrap(),
    ));
    let why = shop.refused(
        &Call::new("protect")
            .input("image", composite)
            .input("mask", unbound),
    );
    assert!(why.contains("belongs to the image"), "{why}");
    // Subtract given both a second mask and a location is refused.
    let both = shop.refused(
        &Call::new("subtract")
            .input("mask", first)
            .input("subtract", first)
            .input("location", location),
    );
    assert!(both.contains("not both"), "{both}");
}
