//! The one encoder of an image artifact's file (plan m3, task F1; the carry-over row on
//! the encoder inside `whizbang-squint`): pixels written as a PNG of eight bits a channel,
//! grey or red, green, blue and alpha, by `image`'s encoder at its fast compression and
//! adaptive filter, with no chunk but the picture's.
//!
//! An image artifact's file is part of its identity (ADR 0002), so this encoder is held
//! still: `image`, `png` and `fdeflate`, the compressor `png` calls at fast compression,
//! are each pinned exactly in this crate's manifest, and
//! `encode::the_core_encoder_keeps_the_three_pins_and_the_squint_vectors` holds them to the
//! lock file and the crops of `view` to their file digests. It was `whizbang-squint`'s
//! (`view.rs`), with copies in `render` and the binary's `export`.
//!
//! The setting was default compression until 2026-11-19, when the owner chose the fast
//! one that `docs/results/m3-encoding.md` measured: about forty times faster, for files
//! about 1.4 per cent larger (ADR 0002, "The encoder's setting"). The switch gave every
//! image the core writes another file, and so another identity; a store written before
//! it is of the generation format before
//! [`GENERATION_FORMAT`](crate::store::GENERATION_FORMAT), and is refused.

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};

use super::CanonicalPixels;

/// The encoder's setting, by the name `docs/results/m3-encoding.md` measured it under:
/// fast compression and the adaptive filter.
pub const SETTING: &str = "fast_adaptive";

/// The samples of a picture as the encoder takes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channels {
    /// One grey byte a pixel.
    Grey,
    /// Red, green, blue and alpha, four bytes a pixel.
    Rgba,
}

/// The samples, row by row from the top left, as a PNG. Why not, as a sentence: samples
/// that are not of the size, or a size the format cannot hold.
pub fn png_of(
    samples: &[u8],
    width: u32,
    height: u32,
    channels: Channels,
) -> Result<Vec<u8>, String> {
    let (colour, each) = match channels {
        Channels::Grey => (ExtendedColorType::L8, 1),
        Channels::Rgba => (ExtendedColorType::Rgba8, 4),
    };
    // The encoder panics on samples that are not of the size: they are refused first.
    let expected = u64::from(width) * u64::from(height) * each;
    if samples.len() as u64 != expected {
        return Err(format!(
            "{} samples are not the {expected} of a picture of {width} by {height}",
            samples.len()
        ));
    }
    let mut file = Vec::new();
    PngEncoder::new_with_quality(&mut file, CompressionType::Fast, FilterType::Adaptive)
        .write_image(samples, width, height, colour)
        .map_err(|error| format!("cannot write the pixels as a PNG: {error}"))?;
    Ok(file)
}

/// Canonical pixels as a PNG of four channels.
pub fn png(pixels: &CanonicalPixels) -> Result<Vec<u8>, String> {
    png_of(
        pixels.rgba(),
        pixels.width(),
        pixels.height(),
        Channels::Rgba,
    )
}
