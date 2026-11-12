//! Image identity: the digest of a file's bytes and the digest of its canonical pixels
//! (ADR 0001).
//!
//! Canonical pixels are the picture as it is shown: upright, 8 bits a channel, in the order
//! red, green, blue, alpha, row by row from the top left, in sRGB, with alpha as stored and
//! not multiplied into the colours.

use std::io::Cursor;

use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageReader, RgbaImage};
use serde::{Deserialize, Serialize};

use super::colour::MatrixProfile;
use super::digest::{Digest, Encoder, FileDigest, PixelDigest};
use super::{IdentityError, Result};

/// What an image is: two digests. Two files that hold the same picture differ in the first
/// and agree in the second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageIdentity {
    /// Of the file's bytes.
    pub file: FileDigest,
    /// Of the canonical pixels.
    pub pixels: PixelDigest,
}

/// The canonical pixels of an image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalPixels {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl CanonicalPixels {
    /// Pixels that are already canonical: four bytes a pixel, row by row.
    pub fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Result<CanonicalPixels> {
        let expected = u64::from(width) * u64::from(height) * 4;
        if expected == 0 || rgba.len() as u64 != expected {
            return Err(IdentityError::Image(format!(
                "{} bytes are not {width}x{height} pixels of four bytes",
                rgba.len()
            )));
        }
        Ok(CanonicalPixels {
            width,
            height,
            rgba,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Red, green, blue and alpha for each pixel, row by row from the top left.
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    /// The canonical pixel digest: the width, the height and the bytes.
    pub fn digest(&self) -> PixelDigest {
        let mut encoder = Encoder::new("pixels/1");
        encoder.unsigned(u64::from(self.width));
        encoder.unsigned(u64::from(self.height));
        encoder.bytes(&self.rgba);
        PixelDigest(encoder.finish())
    }
}

/// Limits checked before any pixel is decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeLimits {
    /// The most pixels an image may have, as stored.
    pub max_pixels: u64,
    /// The most bytes a file may have.
    pub max_bytes: u64,
}

impl Default for DecodeLimits {
    fn default() -> DecodeLimits {
        DecodeLimits {
            max_pixels: 50_000_000,
            max_bytes: 64 * 1024 * 1024,
        }
    }
}

/// What a file's colour profile was taken for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// The file has no ICC profile: its samples are sRGB.
    None,
    /// The file's profile is sRGB: its samples were kept.
    Srgb,
    /// The file's profile is another RGB matrix profile: its samples were converted.
    Converted,
}

/// How the file stored the picture, before it was made canonical. None of this is part of
/// an identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stored {
    /// The width as stored, before the orientation tag is applied.
    pub width: u32,
    /// The height as stored.
    pub height: u32,
    /// The EXIF orientation that was applied, 1 to 8; 1 is none.
    pub orientation: u8,
    /// Whether the file has an alpha channel.
    pub alpha: bool,
    pub profile: Profile,
}

/// An image, identified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identified {
    pub identity: ImageIdentity,
    pub pixels: CanonicalPixels,
    pub stored: Stored,
}

/// Identifies an image from its file's bytes: hashes them, decodes them within the limits,
/// and makes the pixels canonical.
pub fn identify(bytes: &[u8], limits: &DecodeLimits) -> Result<Identified> {
    if bytes.len() as u64 > limits.max_bytes {
        return Err(IdentityError::Limit(format!(
            "{} bytes is over the limit of {} bytes",
            bytes.len(),
            limits.max_bytes
        )));
    }
    let cannot = |what: &str, error: &dyn std::fmt::Display| {
        IdentityError::Image(format!("cannot {what}: {error}"))
    };
    let mut decoder = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| cannot("read", &e))?
        .into_decoder()
        .map_err(|e| cannot("read the header", &e))?;
    let (stored_width, stored_height) = decoder.dimensions();
    let count = u64::from(stored_width) * u64::from(stored_height);
    if count > limits.max_pixels {
        return Err(IdentityError::Limit(format!(
            "{stored_width}x{stored_height} is {count} pixels, over the limit of {}",
            limits.max_pixels
        )));
    }
    // A tag that cannot be read, or that is not 1 to 8, is no tag.
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let profile = decoder
        .icc_profile()
        .map_err(|e| cannot("read the colour profile", &e))?
        .filter(|profile| !profile.is_empty());
    let decoded = DynamicImage::from_decoder(decoder).map_err(|e| cannot("decode", &e))?;
    let alpha = decoded.color().has_alpha();
    let grey = !decoded.color().has_color();
    let mut rgba = straight_rgba(decoded)?;

    let profile = match profile {
        None => Profile::None,
        Some(_) if grey => {
            return Err(IdentityError::Unsupported(
                "a grey image with a colour profile has no canonical pixels".to_owned(),
            ));
        }
        Some(profile) => {
            let profile = MatrixProfile::parse(&profile).map_err(|why| {
                IdentityError::Unsupported(format!("the colour profile cannot be applied: {why}"))
            })?;
            if profile.is_srgb() {
                Profile::Srgb
            } else {
                profile.convert(&mut rgba);
                Profile::Converted
            }
        }
    };

    let mut upright = DynamicImage::ImageRgba8(rgba);
    upright.apply_orientation(orientation);
    let (width, height) = (upright.width(), upright.height());
    let pixels = CanonicalPixels::from_rgba(width, height, upright.into_rgba8().into_raw())?;
    Ok(Identified {
        identity: ImageIdentity {
            file: FileDigest(Digest::of_bytes(bytes)),
            pixels: pixels.digest(),
        },
        pixels,
        stored: Stored {
            width: stored_width,
            height: stored_height,
            orientation: orientation.to_exif(),
            alpha,
            profile,
        },
    })
}

/// The decoded samples as 8-bit RGBA with straight alpha, in stored order. Grey becomes
/// three equal channels; a missing alpha channel is opaque; a 16-bit sample `v` becomes
/// `(v * 255 + 32767) / 65535`, the nearest 8-bit value.
fn straight_rgba(decoded: DynamicImage) -> Result<RgbaImage> {
    let (width, height) = (decoded.width(), decoded.height());
    let narrow = |v: u16| ((u32::from(v) * 255 + 32767) / 65535) as u8;
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    match &decoded {
        DynamicImage::ImageLuma8(image) => {
            for p in image.pixels() {
                rgba.extend_from_slice(&[p.0[0], p.0[0], p.0[0], 255]);
            }
        }
        DynamicImage::ImageLumaA8(image) => {
            for p in image.pixels() {
                rgba.extend_from_slice(&[p.0[0], p.0[0], p.0[0], p.0[1]]);
            }
        }
        DynamicImage::ImageRgb8(image) => {
            for p in image.pixels() {
                rgba.extend_from_slice(&[p.0[0], p.0[1], p.0[2], 255]);
            }
        }
        DynamicImage::ImageRgba8(image) => rgba.extend_from_slice(image.as_raw()),
        DynamicImage::ImageLuma16(image) => {
            for p in image.pixels() {
                let v = narrow(p.0[0]);
                rgba.extend_from_slice(&[v, v, v, 255]);
            }
        }
        DynamicImage::ImageLumaA16(image) => {
            for p in image.pixels() {
                let v = narrow(p.0[0]);
                rgba.extend_from_slice(&[v, v, v, narrow(p.0[1])]);
            }
        }
        DynamicImage::ImageRgb16(image) => {
            for p in image.pixels() {
                rgba.extend_from_slice(&[narrow(p.0[0]), narrow(p.0[1]), narrow(p.0[2]), 255]);
            }
        }
        DynamicImage::ImageRgba16(image) => {
            for p in image.pixels() {
                rgba.extend_from_slice(&p.0.map(narrow));
            }
        }
        other => {
            return Err(IdentityError::Unsupported(format!(
                "{:?} samples have no canonical pixels",
                other.color()
            )));
        }
    }
    RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| IdentityError::Image("the decoded samples do not fill the image".to_owned()))
}
