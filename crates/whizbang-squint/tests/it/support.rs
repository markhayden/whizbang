//! What the tests build their images from: pictures painted pixel by pixel in whole
//! numbers, so every machine paints the same one.

use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use whizbang_core::artifact::{Image, ImageRole, Origin, Step};
use whizbang_core::identity::{Binding, DecodeLimits, Mapping, Projection, Topology};
use whizbang_squint::view::{Crop, Request, view};

pub fn limits() -> DecodeLimits {
    DecodeLimits::default()
}

/// Red, green, blue and alpha for each pixel, row by row.
pub fn paint(width: u32, height: u32, colour: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            rgba.extend_from_slice(&colour(x, y));
        }
    }
    rgba
}

/// The pixels as a PNG file.
pub fn png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut file = Vec::new();
    PngEncoder::new(&mut file)
        .write_image(rgba, width, height, ExtendedColorType::Rgba8)
        .unwrap();
    file
}

/// A picture file as an image artifact in the space stated.
pub fn image_in(file: Vec<u8>, projection: Projection, topology: Topology) -> Image {
    let origin = Origin::new(Step::new("fixture", "1")).unwrap();
    Image::new(
        file,
        projection,
        topology,
        ImageRole::Picture,
        origin,
        &limits(),
    )
    .unwrap()
}

/// A flat picture, painted.
pub fn picture(width: u32, height: u32, colour: impl Fn(u32, u32) -> [u8; 4]) -> Image {
    let file = png(width, height, &paint(width, height, colour));
    image_in(file, Projection::Flat, Topology::Bounded)
}

/// A sample with no pattern to it. The fixed vectors were computed apart from this crate
/// with the same function, so it must not change.
pub fn noise(x: u32, y: u32, channel: u32, seed: u32) -> u8 {
    let mut v = x.wrapping_mul(73_856_093)
        ^ y.wrapping_mul(19_349_663)
        ^ channel.wrapping_mul(83_492_791)
        ^ seed.wrapping_mul(2_654_435_761);
    v ^= v >> 13;
    v = v.wrapping_mul(0x5bd1_e995);
    v ^= v >> 15;
    (v >> 8) as u8
}

/// A colour of noise at each pixel; its alpha is noise too when `alpha` is set, and
/// opaque otherwise.
pub fn noisy(seed: u32, alpha: bool) -> impl Fn(u32, u32) -> [u8; 4] {
    move |x, y| {
        [
            noise(x, y, 0, seed),
            noise(x, y, 1, seed),
            noise(x, y, 2, seed),
            if alpha { noise(x, y, 3, seed) } else { 255 },
        ]
    }
}

/// The image's canonical pixels.
pub fn rgba_of(image: &Image) -> Vec<u8> {
    image.pixels(&limits()).unwrap().rgba().to_vec()
}

/// The one crop of a request with no second image.
pub fn look(image: &Image, bbox: [f64; 4]) -> Crop {
    let mut crops = view(
        &Request {
            image,
            also: None,
            bbox,
        },
        &limits(),
    )
    .unwrap();
    assert_eq!(crops.len(), 1);
    crops.remove(0)
}

/// The box of a crop's binding: left, top, right, bottom, in its source's pixels.
pub fn cut_of(binding: &Binding) -> [f64; 4] {
    let Mapping::Crop {
        left,
        top,
        right,
        bottom,
    } = binding.mapping()
    else {
        panic!("{:?} is not a crop", binding.mapping())
    };
    [*left, *top, *right, *bottom]
}
