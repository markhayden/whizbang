//! Small artifacts for the tests, built directly.

use std::io::Cursor;

use whizbang_core::artifact::{
    Artifact, ArtifactId, BandProfile, Image, ImageRole, Line, Location, Mask, Origin, Plan,
    Series, Shape, Source, Step, UseCase, Windows,
};
use whizbang_core::identity::{
    Binding, CanonicalPixels, DecodeLimits, Digest, PixelDigest, Projection, Space, Topology,
};

pub const WIDTH: u32 = 8;
pub const HEIGHT: u32 = 6;

pub fn limits() -> DecodeLimits {
    DecodeLimits::default()
}

/// A pixel digest for an image these tests never decode.
pub fn picture(name: &str) -> PixelDigest {
    PixelDigest(Digest::of_bytes(name.as_bytes()))
}

/// The pixels of a picture these tests do look at: `WIDTH` by `HEIGHT`, every byte made
/// from the name, alpha included, so that two names are two unrelated pictures.
pub fn pixels(name: &str) -> CanonicalPixels {
    let mut state = name.bytes().fold(7_u32, |state, byte| {
        state.wrapping_mul(31).wrapping_add(u32::from(byte))
    });
    let rgba = (0..WIDTH * HEIGHT * 4)
        .map(|_| {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (state >> 16) as u8
        })
        .collect();
    CanonicalPixels::from_rgba(WIDTH, HEIGHT, rgba).unwrap()
}

/// The picture of [`pixels`] in its own space.
pub fn shown(name: &str) -> Binding {
    Binding::root(pixels(name).digest(), space())
}

/// The picture with every pixel the mask weighs over 0 repainted, each colour channel
/// 255 less what it was, and every other pixel kept: what a composite through the mask
/// could have made of it.
pub fn painted_through(picture: &CanonicalPixels, mask: &Mask) -> CanonicalPixels {
    let mut rgba = picture.rgba().to_vec();
    for (at, pixel) in rgba.chunks_mut(4).enumerate() {
        let (x, y) = (at as u32 % WIDTH, at as u32 / WIDTH);
        if mask.weight(x, y).unwrap() > 0.0 {
            for channel in &mut pixel[..3] {
                *channel = 255 - *channel;
            }
        }
    }
    CanonicalPixels::from_rgba(WIDTH, HEIGHT, rgba).unwrap()
}

pub fn space() -> Space {
    Space::flat(WIDTH, HEIGHT).unwrap()
}

/// The image `name` in its own space.
pub fn on(name: &str) -> Binding {
    Binding::root(picture(name), space())
}

pub fn made_by(action: &str) -> Origin {
    Origin::new(Step::new(action, "1")).unwrap()
}

/// A plane with 255 in the box from `(x0, y0)` up to `(x1, y1)` and 0 elsewhere.
pub fn rect(x0: u32, y0: u32, x1: u32, y1: u32) -> Vec<u8> {
    let mut plane = vec![0; (WIDTH * HEIGHT) as usize];
    for y in y0..y1 {
        for x in x0..x1 {
            plane[(y * WIDTH + x) as usize] = 255;
        }
    }
    plane
}

/// A mask whose core is the box from (2, 1) to (5, 4).
pub fn rect_mask(binding: Binding) -> Mask {
    Mask::from_core(
        binding,
        UseCase::Custom,
        rect(2, 1, 5, 4),
        Source::default(),
        made_by("rect"),
    )
    .unwrap()
}

pub fn line(binding: Binding) -> Location {
    Location::new(
        binding,
        Shape::Line {
            line: Line::Column(4.0),
            width: 3.0,
        },
        made_by("line"),
    )
    .unwrap()
}

pub fn band(binding: Binding, line: ArtifactId, thin: f64) -> BandProfile {
    BandProfile::new(
        binding,
        line,
        4,
        thin,
        (0..HEIGHT)
            .map(|row| thin + f64::from(row) * 0.25)
            .collect(),
        Windows {
            gap: 1,
            step_columns: 3,
            texture_columns: 8,
        },
        "fan-v1",
        made_by("fan"),
    )
    .unwrap()
}

pub fn sigma(binding: Binding, value: f64) -> Series {
    Series::new(
        binding,
        "sigma_px",
        "px",
        1,
        vec![value; HEIGHT as usize],
        made_by("feather"),
    )
    .unwrap()
}

pub fn step(binding: Binding, value: f64) -> Series {
    Series::new(
        binding,
        "tone_ramp.step",
        "levels",
        3,
        (0..3 * HEIGHT).map(|n| value + f64::from(n)).collect(),
        made_by("ramp"),
    )
    .unwrap()
}

/// A mask that holds everything a mask can: a blend, protected pixels, a profile, a
/// prescription, sources and an origin of two steps.
pub fn full_mask(binding: Binding) -> Mask {
    let line = line(binding.clone());
    let mut mask = Mask::from_core(
        binding.clone(),
        UseCase::Seam,
        rect(3, 0, 5, HEIGHT),
        Source {
            locations: vec![line.id()],
            instances: vec![Digest::of_bytes(b"an instance")],
        },
        Origin::new(
            Step::new("profile", "2")
                .argument("thin", "1.5")
                .input("line", line.id().0),
        )
        .unwrap(),
    )
    .unwrap();
    let mut blend = vec![0; (WIDTH * HEIGHT) as usize];
    for y in 0..HEIGHT {
        blend[(y * WIDTH + 2) as usize] = 128;
        blend[(y * WIDTH + 5) as usize] = 64 + y as u8;
    }
    mask.set_blend(blend).unwrap();
    mask.protect_union(&rect(0, 0, 3, 1)).unwrap();
    mask.set_profile(
        band(binding.clone(), line.id(), 1.5),
        sigma(binding.clone(), 0.75),
    )
    .unwrap();
    mask.push_tone_ramp(step(binding, -2.0), 0.5).unwrap();
    mask.record(Step::new("feather", "1").argument("sigma", "0.75"))
        .unwrap();
    mask
}

/// The mask as a rebind onto `binding` leaves it, made by hand: the same planes, use
/// and sources on that binding, with the profile's band and sigma and the ramp's step
/// made on it. It is `full_mask` moved; `line` is the line the first one's band follows.
pub fn full_mask_moved(mask: &Mask, binding: Binding, line: ArtifactId) -> Mask {
    let mut moved = Mask::from_core(
        binding.clone(),
        mask.use_case(),
        mask.core().to_vec(),
        mask.source().clone(),
        made_by("by hand"),
    )
    .unwrap();
    moved.set_blend(mask.blend().to_vec()).unwrap();
    moved.protect_union(mask.protect()).unwrap();
    moved
        .set_profile(
            band(binding.clone(), line, 1.5),
            sigma(binding.clone(), 0.75),
        )
        .unwrap();
    moved.push_tone_ramp(step(binding, -2.0), 0.5).unwrap();
    moved
}

/// A PNG of `WIDTH` by `HEIGHT` whose pixels come from `colour`.
pub fn png(colour: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
    let picture = image::RgbaImage::from_fn(WIDTH, HEIGHT, |x, y| image::Rgba(colour(x, y)));
    let mut bytes = Vec::new();
    picture
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap();
    bytes
}

/// A photograph: every pixel its own colour.
pub fn photo() -> Vec<u8> {
    png(|x, y| [(x * 30) as u8, (y * 40) as u8, (x * y) as u8, 255])
}

pub fn image_of(file: Vec<u8>, role: ImageRole) -> Image {
    Image::new(
        file,
        Projection::Flat,
        Topology::Bounded,
        role,
        made_by("import"),
        &limits(),
    )
    .unwrap()
}

/// A mask's core as a grey picture: what a rendering of it is.
pub fn rendering_of(mask: &Mask) -> Image {
    let core = mask.core().to_vec();
    let file = png(|x, y| {
        let v = core[(y * WIDTH + x) as usize];
        [v, v, v, 255]
    });
    image_of(file, ImageRole::Rendering)
}

pub fn plan() -> Plan {
    let panorama = Binding::root(picture("panorama"), Space::panorama(400, 200).unwrap());
    Plan::new(panorama, 40, made_by("cut")).unwrap()
}

/// One artifact of each of the six types.
pub fn one_of_each() -> Vec<Artifact> {
    let binding = on("photo");
    let line = line(binding.clone());
    vec![
        full_mask(binding.clone()).into(),
        Location::new(
            binding.clone(),
            Shape::Box {
                left: 1.0,
                top: 0.5,
                right: 6.25,
                bottom: 5.0,
            },
            made_by("find"),
        )
        .unwrap()
        .into(),
        image_of(photo(), ImageRole::Picture).into(),
        band(binding.clone(), line.id(), 1.5).into(),
        sigma(binding, 0.75).into(),
        plan().into(),
    ]
}
