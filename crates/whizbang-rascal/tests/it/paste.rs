//! `paste` and `fill` as actions (plan m3, task F1, item 5), run through the executor
//! against a store: a picture of 16 by 12 with some alpha, and a mask on it with a core,
//! a blend on part of its edge, protected pixels inside the core and many pixels it
//! weighs nothing.

use std::sync::Arc;

use whizbang_core::artifact::{
    self, Artifact, ArtifactId, ArtifactType, Image, ImageRole, Mask, Source, UseCase,
};
use whizbang_core::contract::{
    Call, Capabilities, Engine, Executor, NoBackends, Outcome, Registry, Value,
};
use whizbang_core::identity::{
    CanonicalPixels, DecodeLimits, Digest, Projection, Topology, encode,
};
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::scratch::Scratch;

use super::grow::{made_by, plane};

pub const W: u32 = 16;
pub const H: u32 = 12;

/// The picture: every pixel its own colour, some half clear.
pub fn pixels() -> CanonicalPixels {
    let rgba = (0..W * H)
        .flat_map(|at| {
            let (x, y) = (at % W, at / W);
            [
                (x * 15) as u8,
                (y * 20) as u8,
                (x * y) as u8,
                if x == 0 { 128 } else { 255 },
            ]
        })
        .collect();
    CanonicalPixels::from_rgba(W, H, rgba).unwrap()
}

/// An image of these pixels, made by `action`.
pub fn image_of(pixels: &CanonicalPixels, action: &str) -> Image {
    Image::new(
        encode::png(pixels).unwrap(),
        Projection::Flat,
        Topology::Bounded,
        ImageRole::Picture,
        made_by(action),
        &DecodeLimits::default(),
    )
    .unwrap()
}

/// The mask: a core from 4, 3 to 10, 8; a blend one pixel wide left of it; two
/// protected pixels inside the core.
pub fn mask_on(image: &Image) -> Mask {
    let (w, h) = (W as usize, H as usize);
    let core = plane(w, h, |x, y| (4..10).contains(&x) && (3..8).contains(&y));
    let mut mask = Mask::from_core(
        image.binding(),
        UseCase::Remove,
        core,
        Source::default(),
        made_by("fixture"),
    )
    .unwrap();
    let blend = plane(w, h, |x, y| x == 3 && (3..8).contains(&y))
        .iter()
        .map(|v| v / 3)
        .collect();
    mask.set_blend(blend).unwrap();
    let protect = plane(w, h, |x, y| (x, y) == (5, 4) || (x, y) == (8, 6));
    mask.protect_union(&protect).unwrap();
    mask
}

/// Whether the mask weighs the pixel at `at` nothing: outside its core and blend, or
/// protected.
pub fn weighs_nothing(mask: &Mask, at: usize) -> bool {
    mask.protect()[at] != 0 || (mask.core()[at] == 0 && mask.blend()[at] == 0)
}

pub struct World {
    _dir: Scratch,
    pub store: Arc<Store>,
}

impl World {
    pub fn new(label: &str) -> World {
        let dir = Scratch::new(label);
        let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap());
        World { _dir: dir, store }
    }

    pub fn put(&self, artifact: Artifact) -> ArtifactId {
        let mut publication = self.store.begin().unwrap();
        let id = artifact::put(&mut publication, &artifact).unwrap();
        publication.publish().unwrap();
        id
    }

    pub fn engine(
        &self,
        capabilities: Capabilities,
        backends: Arc<dyn whizbang_core::contract::Backends>,
    ) -> Engine {
        let mut registry = Registry::new();
        whizbang_rascal::paste::register(&mut registry).unwrap();
        whizbang_rascal::fill::register(&mut registry).unwrap();
        Engine::over(
            registry,
            backends,
            self.store.clone(),
            Digest::of_bytes(b""),
        )
        .with_capabilities(capabilities)
    }

    pub fn image(&self, id: ArtifactId) -> Image {
        match artifact::get(
            &*self.store,
            ArtifactType::Image,
            id,
            &DecodeLimits::default(),
        )
        .unwrap()
        {
            Artifact::Image(image) => image,
            other => panic!("{other:?}"),
        }
    }

    pub fn mask(&self, id: ArtifactId) -> Mask {
        artifact::get(
            &*self.store,
            ArtifactType::Mask,
            id,
            &DecodeLimits::default(),
        )
        .unwrap()
        .into_mask()
        .unwrap()
    }
}

/// The outputs of a run that completed, by port.
pub fn outputs(outcome: &Outcome) -> Vec<(String, ArtifactId)> {
    let Outcome::Completed { outputs } = outcome else {
        panic!("{outcome:?}");
    };
    outputs
        .iter()
        .map(|o| (o.port().to_owned(), o.id()))
        .collect()
}

#[test]
fn feather_keeps_every_pixel_outside_the_mask_and_every_protected_one() {
    // The fill is the picture with every channel inverted, the alpha too: the
    // composite is the fill's colour in the core, a mix under the blend, and the base's
    // four bytes everywhere the mask weighs nothing; no alpha changes.
    let world = World::new("paste");
    let base_pixels = pixels();
    let base = image_of(&base_pixels, "import");
    let mask = mask_on(&base);
    let inverted: Vec<u8> = base_pixels.rgba().iter().map(|v| 255 - v).collect();
    let fill = image_of(
        &CanonicalPixels::from_rgba(W, H, inverted).unwrap(),
        "import",
    );
    let ids = (
        world.put(base.clone().into()),
        world.put(fill.into()),
        world.put(mask.clone().into()),
    );
    let engine = world.engine(Capabilities::none(), Arc::new(NoBackends));
    let call = Call::new("paste")
        .input("image", ids.0)
        .input("fill", ids.1)
        .input("mask", ids.2);
    let outcome = engine.submit(&call).unwrap().outcome().clone();
    let [(port, made)] = outputs(&outcome).try_into().unwrap();
    assert_eq!(port, "image");
    let made = world.image(made);
    assert_eq!(made.origin().steps().last().unwrap().action, "paste");
    let after = made.pixels(&DecodeLimits::default()).unwrap();
    let (before, after) = (base_pixels.rgba(), after.rgba());
    let mut kept = 0;
    for at in 0..(W * H) as usize {
        let (b, a) = (&before[at * 4..at * 4 + 4], &after[at * 4..at * 4 + 4]);
        assert_eq!(a[3], b[3], "pixel {at}: the alpha");
        if weighs_nothing(&mask, at) {
            assert_eq!(a, b, "pixel {at}");
            kept += 1;
        } else if mask.core()[at] != 0 {
            assert_eq!(&a[..3], &[255 - b[0], 255 - b[1], 255 - b[2]], "pixel {at}");
        } else {
            let blend = mask.blend()[at];
            for c in 0..3 {
                assert_eq!(
                    a[c],
                    whizbang_rascal::composite::mix(b[c], 255 - b[c], blend)
                );
            }
        }
    }
    // 192 pixels: a core of 30, 5 of blend, 2 of them protected.
    assert_eq!(kept, 192 - 30 - 5 + 2);
    // `mode` takes feather alone; a fill of another size is invalid.
    let other = Call::new("paste")
        .argument("mode", Value::text("hard"))
        .input("image", ids.0)
        .input("fill", ids.1)
        .input("mask", ids.2);
    assert!(engine.submit(&other).is_err());
}
