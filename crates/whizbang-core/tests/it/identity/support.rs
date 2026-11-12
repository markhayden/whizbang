//! What the identity tests build their images from: one small picture, the eight ways an
//! orientation tag can store it, colour profiles made by hand, and the containers.

use std::io::Cursor;

use image::codecs::bmp::BmpEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};

pub const WIDTH: u32 = 4;
pub const HEIGHT: u32 = 3;

/// The picture every vector stores: 4 by 3, every pixel a different colour, and no
/// symmetry, so each orientation of it is a different grid.
pub fn picture() -> Vec<[u8; 3]> {
    let mut pixels = Vec::new();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let n = y * WIDTH + x;
            pixels.push([20 * n + 10, 250 - 17 * n, 97 * n % 241 + 5].map(|v| v as u8));
        }
    }
    pixels
}

/// The picture as canonical pixels: RGBA, row-major, opaque.
pub fn picture_rgba() -> Vec<u8> {
    picture()
        .iter()
        .flat_map(|[r, g, b]| [*r, *g, *b, 255])
        .collect()
}

/// A grid of pixels, row-major.
#[derive(Debug, Clone, PartialEq)]
pub struct Grid<P> {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<P>,
}

impl<P: Copy> Grid<P> {
    pub fn new(width: u32, height: u32, pixels: Vec<P>) -> Grid<P> {
        assert_eq!(pixels.len(), (width * height) as usize);
        Grid {
            width,
            height,
            pixels,
        }
    }

    pub fn at(&self, x: u32, y: u32) -> P {
        self.pixels[(y * self.width + x) as usize]
    }

    /// The grid a file stores so that a reader who honours the EXIF orientation tag
    /// `orientation` (1 to 8) shows this one. Written from the tag's definition, not from
    /// any decoder: for each stored pixel, where it is shown.
    pub fn stored_under(&self, orientation: u8) -> Grid<P> {
        let (w, h) = (self.width, self.height);
        let (stored_width, stored_height) = if orientation <= 4 { (w, h) } else { (h, w) };
        let mut pixels = Vec::new();
        for sy in 0..stored_height {
            for sx in 0..stored_width {
                // The shown position of stored pixel (sx, sy).
                let (x, y) = match orientation {
                    1 => (sx, sy),
                    // Mirrored left to right.
                    2 => (w - 1 - sx, sy),
                    // Turned half way round.
                    3 => (w - 1 - sx, h - 1 - sy),
                    // Mirrored top to bottom.
                    4 => (sx, h - 1 - sy),
                    // Mirrored about the diagonal from the top left.
                    5 => (sy, sx),
                    // Shown after a quarter turn clockwise.
                    6 => (w - 1 - sy, sx),
                    // Mirrored about the other diagonal.
                    7 => (w - 1 - sy, h - 1 - sx),
                    // Shown after a quarter turn anticlockwise.
                    8 => (sy, h - 1 - sx),
                    other => panic!("orientation {other}"),
                };
                pixels.push(self.at(x, y));
            }
        }
        Grid::new(stored_width, stored_height, pixels)
    }
}

pub fn picture_grid() -> Grid<[u8; 3]> {
    Grid::new(WIDTH, HEIGHT, picture())
}

/// EXIF metadata that holds an orientation tag and nothing else: a big-endian TIFF header
/// and one directory with one entry.
pub fn exif_orientation(orientation: u16) -> Vec<u8> {
    let mut exif = Vec::new();
    exif.extend_from_slice(b"MM\x00\x2a");
    exif.extend_from_slice(&8u32.to_be_bytes());
    exif.extend_from_slice(&1u16.to_be_bytes());
    // Tag 0x0112 (orientation), type 3 (short), one value.
    exif.extend_from_slice(&0x0112u16.to_be_bytes());
    exif.extend_from_slice(&3u16.to_be_bytes());
    exif.extend_from_slice(&1u32.to_be_bytes());
    exif.extend_from_slice(&orientation.to_be_bytes());
    exif.extend_from_slice(&[0, 0]);
    exif.extend_from_slice(&0u32.to_be_bytes());
    exif
}

/// What a file carries beside its samples.
#[derive(Default)]
pub struct Extras {
    pub exif: Option<Vec<u8>>,
    pub profile: Option<Vec<u8>>,
}

/// A PNG of these samples.
pub fn png(
    width: u32,
    height: u32,
    colour: ExtendedColorType,
    samples: &[u8],
    extras: Extras,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = PngEncoder::new(Cursor::new(&mut bytes));
    if let Some(exif) = extras.exif {
        encoder.set_exif_metadata(exif).unwrap();
    }
    if let Some(profile) = extras.profile {
        encoder.set_icc_profile(profile).unwrap();
    }
    encoder.write_image(samples, width, height, colour).unwrap();
    bytes
}

/// An 8-bit RGB PNG of a grid.
pub fn rgb_png(grid: &Grid<[u8; 3]>, extras: Extras) -> Vec<u8> {
    let samples: Vec<u8> = grid.pixels.iter().flatten().copied().collect();
    png(
        grid.width,
        grid.height,
        ExtendedColorType::Rgb8,
        &samples,
        extras,
    )
}

/// An 8-bit RGBA PNG of a grid.
pub fn rgba_png(grid: &Grid<[u8; 4]>, extras: Extras) -> Vec<u8> {
    let samples: Vec<u8> = grid.pixels.iter().flatten().copied().collect();
    png(
        grid.width,
        grid.height,
        ExtendedColorType::Rgba8,
        &samples,
        extras,
    )
}

/// A 16-bit RGB PNG of these samples.
pub fn rgb16_png(width: u32, height: u32, pixels: &[[u16; 3]]) -> Vec<u8> {
    // The encoder takes the samples in the machine's byte order.
    let samples: Vec<u8> = pixels
        .iter()
        .flatten()
        .flat_map(|sample| sample.to_ne_bytes())
        .collect();
    png(
        width,
        height,
        ExtendedColorType::Rgb16,
        &samples,
        Extras::default(),
    )
}

/// An 8-bit RGB BMP of a grid: another lossless container.
pub fn rgb_bmp(grid: &Grid<[u8; 3]>) -> Vec<u8> {
    let samples: Vec<u8> = grid.pixels.iter().flatten().copied().collect();
    let mut bytes = Vec::new();
    BmpEncoder::new(&mut bytes)
        .encode(&samples, grid.width, grid.height, ExtendedColorType::Rgb8)
        .unwrap();
    bytes
}

/// A JPEG of a grid, at the highest quality.
pub fn rgb_jpeg(grid: &Grid<[u8; 3]>, exif: Option<Vec<u8>>) -> Vec<u8> {
    let samples: Vec<u8> = grid.pixels.iter().flatten().copied().collect();
    let mut bytes = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(Cursor::new(&mut bytes), 100);
    if let Some(exif) = exif {
        encoder.set_exif_metadata(exif).unwrap();
    }
    encoder
        .write_image(&samples, grid.width, grid.height, ExtendedColorType::Rgb8)
        .unwrap();
    bytes
}

// --- colour profiles ----------------------------------------------------------------------

/// The tone curve of a hand-made profile.
#[derive(Debug, Clone)]
pub enum Curve {
    /// The sRGB curve, as a parametric curve of type 3.
    SrgbParametric,
    /// The sRGB curve, sampled into a table of this many entries.
    SrgbTable(usize),
    /// A plain power, as a `curv` element with one entry.
    Gamma(f64),
    /// A parametric curve of this type with these parameters.
    Parametric(u16, Vec<f64>),
    /// A table.
    Table(Vec<u16>),
}

/// The sRGB primaries as an ICC profile holds them: adapted to D50, as fractions of 65536.
pub const SRGB_COLORANTS: [[f64; 3]; 3] = [
    [28578.0 / 65536.0, 14581.0 / 65536.0, 912.0 / 65536.0],
    [25241.0 / 65536.0, 46981.0 / 65536.0, 6362.0 / 65536.0],
    [9376.0 / 65536.0, 3972.0 / 65536.0, 46799.0 / 65536.0],
];

/// A wider gamut than sRGB with the same white: the red, green and blue colorants, each as
/// X, Y and Z.
pub const WIDE_COLORANTS: [[f64; 3]; 3] = [
    [0.51512, 0.24120, -0.00105],
    [0.29198, 0.69225, 0.04189],
    [0.15710, 0.06657, 0.78407],
];

fn fixed(value: f64) -> [u8; 4] {
    ((value * 65536.0).round() as i32).to_be_bytes()
}

fn srgb_decode(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn curve_element(curve: &Curve) -> Vec<u8> {
    let mut out = Vec::new();
    let table = |out: &mut Vec<u8>, entries: &[u16]| {
        out.extend_from_slice(b"curv\0\0\0\0");
        out.extend_from_slice(&(entries.len() as u32).to_be_bytes());
        for entry in entries {
            out.extend_from_slice(&entry.to_be_bytes());
        }
    };
    let parametric = |out: &mut Vec<u8>, kind: u16, parameters: &[f64]| {
        out.extend_from_slice(b"para\0\0\0\0");
        out.extend_from_slice(&kind.to_be_bytes());
        out.extend_from_slice(&[0, 0]);
        for parameter in parameters {
            out.extend_from_slice(&fixed(*parameter));
        }
    };
    match curve {
        Curve::SrgbParametric => parametric(
            &mut out,
            3,
            &[2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.04045],
        ),
        Curve::SrgbTable(entries) => {
            let entries: Vec<u16> = (0..*entries)
                .map(|i| {
                    let v = i as f64 / (*entries - 1) as f64;
                    (srgb_decode(v) * 65535.0).round() as u16
                })
                .collect();
            table(&mut out, &entries);
        }
        // One entry is a power, as a fixed-point number with eight fractional bits.
        Curve::Gamma(gamma) => table(&mut out, &[(gamma * 256.0).round() as u16]),
        Curve::Parametric(kind, parameters) => parametric(&mut out, *kind, parameters),
        Curve::Table(entries) => table(&mut out, entries),
    }
    while out.len() % 4 != 0 {
        out.push(0);
    }
    out
}

/// The tags of a profile: each signature with its element.
pub fn matrix_tags(colorants: &[[f64; 3]; 3], curve: &Curve) -> Vec<([u8; 4], Vec<u8>)> {
    let mut tags = Vec::new();
    for (signature, colorant) in [b"rXYZ", b"gXYZ", b"bXYZ"].iter().zip(colorants) {
        let mut element = b"XYZ \0\0\0\0".to_vec();
        for value in colorant {
            element.extend_from_slice(&fixed(*value));
        }
        tags.push((**signature, element));
    }
    for signature in [b"rTRC", b"gTRC", b"bTRC"] {
        tags.push((*signature, curve_element(curve)));
    }
    tags
}

/// An ICC profile with this colour space signature and these tags, laid out one after the
/// other.
pub fn profile_of(colour_space: &[u8; 4], tags: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut data_offset = 128 + 4 + 12 * tags.len();
    let mut table = (tags.len() as u32).to_be_bytes().to_vec();
    let mut data = Vec::new();
    for (signature, element) in tags {
        table.extend_from_slice(signature);
        table.extend_from_slice(&(data_offset as u32).to_be_bytes());
        table.extend_from_slice(&(element.len() as u32).to_be_bytes());
        data.extend_from_slice(element);
        data_offset += element.len();
    }
    let mut profile = vec![0u8; 128];
    profile[0..4].copy_from_slice(&(data_offset as u32).to_be_bytes());
    // Version 2.4, a display profile, RGB data, connected through XYZ.
    profile[8..12].copy_from_slice(&[2, 0x40, 0, 0]);
    profile[12..16].copy_from_slice(b"mntr");
    profile[16..20].copy_from_slice(colour_space);
    profile[20..24].copy_from_slice(b"XYZ ");
    profile[36..40].copy_from_slice(b"acsp");
    profile.extend_from_slice(&table);
    profile.extend_from_slice(&data);
    profile
}

/// An RGB matrix profile: three colorants and one curve for all three channels.
pub fn matrix_profile(colorants: &[[f64; 3]; 3], curve: &Curve) -> Vec<u8> {
    profile_of(b"RGB ", &matrix_tags(colorants, curve))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn unhex(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2));
    (0..text.len() / 2)
        .map(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}
