//! What the tests of `cut`, `put`, `window`, `unwindow`, `apply` and the heal share
//! (plan m3, task O2): a store, the executor over those actions with `fill` and `paste`,
//! pictures of at most 256 by 128, and the outcome of a call.

use std::sync::Arc;

use whizbang_core::artifact::{Artifact, ArtifactId, Image, ImageRole, Mask, Source, UseCase};
use whizbang_core::capability::Filler;
use whizbang_core::contract::{
    Call, Capabilities, Engine, Executor, NoBackends, Outcome, Registry,
};
use whizbang_core::identity::{
    CanonicalPixels, DecodeLimits, Digest, Projection, Topology, encode,
};
use whizbang_testkit::capabilities::roster;

use super::grow::{made_by, plane};
pub use super::paste::{World, outputs};

/// The executor over the five, `fill` and `paste`, with this filler when there is one.
pub fn engine(world: &World, filler: Option<Arc<dyn Filler>>) -> Engine {
    let mut registry = Registry::new();
    whizbang_rascal::cut::register(&mut registry).unwrap();
    whizbang_rascal::put::register(&mut registry).unwrap();
    whizbang_rascal::window::register(&mut registry).unwrap();
    whizbang_rascal::unwindow::register(&mut registry).unwrap();
    whizbang_rascal::apply::register(&mut registry).unwrap();
    whizbang_rascal::fill::register(&mut registry).unwrap();
    whizbang_rascal::paste::register(&mut registry).unwrap();
    let engine = |backends, capabilities| {
        Engine::over(
            registry,
            backends,
            world.store.clone(),
            Digest::of_bytes(b""),
        )
        .with_capabilities(capabilities)
    };
    match filler {
        Some(filler) => engine(
            Arc::new(roster(None, Some(filler.as_ref()), None)),
            Capabilities::none().with_fill(filler.clone()),
        ),
        None => engine(Arc::new(NoBackends), Capabilities::none()),
    }
}

/// The outputs of a call that completed, by port.
pub fn ran(engine: &Engine, call: &Call) -> Vec<(String, ArtifactId)> {
    outputs(engine.submit(call).unwrap().outcome())
}

/// Why a call that ran was found invalid.
pub fn invalid(engine: &Engine, call: &Call) -> String {
    match engine.submit(call).unwrap().outcome() {
        Outcome::Invalid { why } => why.clone(),
        other => panic!("not invalid: {other:?}"),
    }
}

/// Rascal's panorama of the tests of `cut` and `put`: red the column, green the row,
/// blue `7x + 3y`, each modulo 256.
pub fn pano(width: u32, height: u32) -> CanonicalPixels {
    let rgba = (0..width * height)
        .flat_map(|at| {
            let (x, y) = (at % width, at / width);
            [
                (x % 256) as u8,
                (y % 256) as u8,
                ((x * 7 + y * 3) % 256) as u8,
                255,
            ]
        })
        .collect();
    CanonicalPixels::from_rgba(width, height, rgba).unwrap()
}

/// These pixels as an image, a complete panorama or flat.
pub fn image(pixels: &CanonicalPixels, panorama: bool) -> Image {
    let (projection, topology) = if panorama {
        (Projection::Equirectangular, Topology::Wrap)
    } else {
        (Projection::Flat, Topology::Bounded)
    };
    Image::new(
        encode::png(pixels).unwrap(),
        projection,
        topology,
        ImageRole::Picture,
        made_by("import"),
        &DecodeLimits::default(),
    )
    .unwrap()
}

/// The pixels of an image.
pub fn pixels_of(image: &Image) -> CanonicalPixels {
    image.pixels(&DecodeLimits::default()).unwrap()
}

/// A mask on the image whose core is where `core` holds and whose protect is where
/// `protect` holds.
pub fn mask(
    image: &Image,
    core: impl Fn(usize, usize) -> bool,
    protect: impl Fn(usize, usize) -> bool,
) -> Mask {
    let (w, h) = (
        image.space().width() as usize,
        image.space().height() as usize,
    );
    let mut mask = Mask::from_core(
        image.binding(),
        UseCase::Remove,
        plane(w, h, core),
        Source::default(),
        made_by("fixture"),
    )
    .unwrap();
    mask.protect_union(&plane(w, h, protect)).unwrap();
    mask
}

/// Stores the artifact.
pub fn put(world: &World, artifact: impl Into<Artifact>) -> ArtifactId {
    world.put(artifact.into())
}

/// Whether the mask weighs the pixel at `at` nothing.
pub fn nothing(mask: &Mask, at: usize) -> bool {
    mask.protect()[at] != 0 || (mask.core()[at] == 0 && mask.blend()[at] == 0)
}

/// The four bytes of pixel `x`, `y`.
pub fn at(pixels: &CanonicalPixels, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * pixels.width() + x) * 4) as usize;
    pixels.rgba()[i..i + 4].try_into().unwrap()
}
