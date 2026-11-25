//! `fill` as an action (plan m3, task F1, item 5): under a filler that repaints every
//! pixel, and under one that answers with the picture it was sent.

use std::sync::Arc;

use whizbang_core::artifact::Derivation;
use whizbang_core::capability::{Bounds, Declaration, Fault, FillRequest, Filler, Painted};
use whizbang_core::contract::{Call, Capabilities, Executor, ModelIdentity};
use whizbang_core::identity::{DecodeLimits, Destination, Mapping};
use whizbang_testkit::capabilities::{FakeFiller, Paint, roster};

use super::paste::{H, W, World, image_of, mask_on, outputs, pixels, weighs_nothing};

/// The outputs of one `fill` of the picture and its mask under this filler: the image and
/// the mask, and the identities of the picture and the mask given.
fn filled(
    label: &str,
    filler: Arc<dyn Filler>,
) -> (
    World,
    whizbang_core::artifact::Image,
    whizbang_core::artifact::Mask,
    whizbang_core::artifact::Image,
    whizbang_core::artifact::Mask,
) {
    let world = World::new(label);
    let base = image_of(&pixels(), "import");
    let mask = mask_on(&base);
    let (image_id, mask_id) = (
        world.put(base.clone().into()),
        world.put(mask.clone().into()),
    );
    let engine = world.engine(
        Capabilities::none().with_fill(filler.clone()),
        Arc::new(roster(None, Some(filler.as_ref()), None)),
    );
    let call = Call::new("fill")
        .input("image", image_id)
        .input("mask", mask_id);
    let outcome = engine.submit(&call).unwrap().outcome().clone();
    let outputs = outputs(&outcome);
    assert_eq!(
        outputs.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(),
        ["image", "mask"]
    );
    let (image, made) = (world.image(outputs[0].1), world.mask(outputs[1].1));
    (world, base, mask, image, made)
}

#[test]
fn pixels_outside_the_support_and_protected_ones_are_the_originals() {
    // The filler repaints every pixel, inside the support and out: the composite keeps
    // the original's four bytes wherever the mask weighs nothing, and the fill's colour
    // in the core.
    let (_world, base, mask, image, _) =
        filled("fill-kept", Arc::new(FakeFiller::new(Paint::Inverse)));
    let limits = DecodeLimits::default();
    let (before, after) = (
        base.pixels(&limits).unwrap(),
        image.pixels(&limits).unwrap(),
    );
    let (before, after) = (before.rgba(), after.rgba());
    for at in 0..(W * H) as usize {
        let (b, a) = (&before[at * 4..at * 4 + 4], &after[at * 4..at * 4 + 4]);
        if weighs_nothing(&mask, at) {
            assert_eq!(a, b, "pixel {at}");
        } else if mask.core()[at] != 0 {
            assert_eq!(a, [255 - b[0], 255 - b[1], 255 - b[2], b[3]], "pixel {at}");
        }
    }
    assert_eq!(image.origin().steps().last().unwrap().action, "fill");
}

#[test]
fn the_mask_is_rebound_onto_the_composite_through_a_checked_derivation() {
    // The mask handed back is on the composite, its binding placing the composite onto
    // the picture by the identity; it has the same planes and an origin whose last steps
    // are this action and the rebind; and it is the mask a derivation checked of the two
    // pictures rebinds.
    let (_world, base, mask, image, made) =
        filled("fill-rebound", Arc::new(FakeFiller::new(Paint::Inverse)));
    let binding = made.binding();
    assert_eq!(binding.source(), image.identity().pixels);
    assert_eq!(
        binding.destination(),
        Destination::Image(base.identity().pixels)
    );
    assert_eq!(*binding.mapping(), Mapping::Identity);
    assert_eq!(
        (made.core(), made.blend(), made.protect()),
        (mask.core(), mask.blend(), mask.protect())
    );
    let steps: Vec<&str> = made
        .origin()
        .steps()
        .iter()
        .map(|s| s.action.as_str())
        .collect();
    assert_eq!(steps, ["fixture", "fill", "rebind"]);
    let limits = DecodeLimits::default();
    let derivation = Derivation::checked(
        &base.pixels(&limits).unwrap(),
        &image.pixels(&limits).unwrap(),
        &mask,
    )
    .unwrap();
    assert_eq!(made.id(), mask.rebind(&derivation).unwrap().id());
}

/// A filler that answers each request with the picture it was sent.
struct Echo(Declaration);

impl Filler for Echo {
    fn declaration(&self) -> &Declaration {
        &self.0
    }

    fn fill(&self, request: &FillRequest, _: &Bounds) -> Result<Painted, Fault> {
        let image = request.image();
        let rgb = image
            .rgba()
            .chunks(4)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();
        Ok(Painted::from_rgb(image.width(), image.height(), rgb).unwrap())
    }
}

#[test]
fn a_fill_answered_with_the_picture_it_was_sent_keeps_the_masks_binding() {
    // Nothing changed, so nothing was derived: the picture and the mask given are handed
    // back as they were.
    let declaration = Declaration::new(ModelIdentity::new("fake", "echo", "1"), &[]).unwrap();
    let (_world, base, mask, image, made) = filled("fill-unchanged", Arc::new(Echo(declaration)));
    assert_eq!(image.id(), base.id());
    assert_eq!(made.id(), mask.id());
    assert_eq!(made.binding(), mask.binding());
}

/// What a fill run keeps for replay (plan m7, task B1, item 7; ADR 0048): the composite
/// cannot be undone, so the run keeps the picture the filler painted as the diagnostic
/// `fill-answer.png`, a canonical image's PNG of its colours with an opaque alpha; the
/// composite made again from the picture, that answer and the feathered mask is the one
/// the run committed. An answer that is not the image's size is not kept.
#[test]
fn a_fill_run_keeps_the_painted_answer_that_a_feathered_mask_was_composited_from() {
    use whizbang_core::artifact::{Image, ImageRole};
    use whizbang_core::capability::Picture;
    use whizbang_core::contract::record;
    use whizbang_core::identity::{Projection, Topology};
    use whizbang_rascal::composite::{Mode, composite_in};
    use whizbang_rascal::fill::{ANSWER, answer_png};

    let world = World::new("fill-answer");
    let base = image_of(&pixels(), "import");
    let mask = mask_on(&base);
    // The mask is feathered: a blend beside its core, which the composite mixes by.
    assert!(mask.blend().iter().any(|blend| (1..255).contains(blend)));
    let (image_id, mask_id) = (
        world.put(base.clone().into()),
        world.put(mask.clone().into()),
    );
    let kept = |filler: Arc<dyn Filler>| {
        let engine = world.engine(
            Capabilities::none().with_fill(filler.clone()),
            Arc::new(roster(None, Some(filler.as_ref()), None)),
        );
        let call = Call::new("fill")
            .input("image", image_id)
            .input("mask", mask_id);
        let finished = engine.submit(&call).unwrap();
        let generation = world.store.snapshot().unwrap();
        let id = finished.run().as_str();
        let stored = record::stored(&generation, id, |entry| world.store.read(entry));
        let listed = stored.unwrap().unwrap().diagnostics().to_vec();
        let name = format!("diagnostics/{ANSWER}");
        let entry = generation.run(id).unwrap().entries().get(&name).cloned();
        let bytes = entry.map(|entry| world.store.read(&entry).unwrap());
        assert_eq!(listed.iter().any(|d| d.name() == name), bytes.is_some());
        (finished.outcome().clone(), bytes)
    };
    let (outcome, bytes) = kept(Arc::new(FakeFiller::new(Paint::Inverse)));
    let bytes = bytes.expect("the answer is kept");
    // The painted answer, as the filler gave it: every channel 255 less the picture's.
    let limits = DecodeLimits::default();
    let given = base.pixels(&limits).unwrap();
    let rgb: Vec<u8> = (given.rgba().chunks(4))
        .flat_map(|p| [255 - p[0], 255 - p[1], 255 - p[2]])
        .collect();
    let painted = Painted::from_rgb(given.width(), given.height(), rgb.clone()).unwrap();
    assert_eq!(bytes, answer_png(&painted).unwrap());
    // It reads as a canonical picture, whose colours are the answer's and whose alpha is
    // opaque.
    let read = Image::new(
        bytes,
        Projection::Flat,
        Topology::Bounded,
        ImageRole::Picture,
        base.origin().clone(),
        &limits,
    )
    .unwrap();
    let read = read.pixels(&limits).unwrap();
    let back: Vec<u8> = (read.rgba().chunks(4))
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    assert_eq!(back, rgb);
    assert!(read.rgba().chunks(4).all(|p| p[3] == 255));
    // The composite made again from it, under the feathered mask, is the run's output.
    let answered = Painted::from_rgb(read.width(), read.height(), back).unwrap();
    let picture = Picture::new(given).unwrap();
    let made = composite_in(&picture, &answered, &mask, Mode::Feather).unwrap();
    let committed = world.image(outputs(&outcome)[0].1);
    assert_eq!(made.pixels(), &committed.pixels(&limits).unwrap());
    // An answer of another size fails the run, and is not kept.
    let (outcome, bytes) = kept(Arc::new(FakeFiller::new(Paint::Size(W + 1, H))));
    assert_eq!(outcome.name(), "error");
    assert!(bytes.is_none());
}
