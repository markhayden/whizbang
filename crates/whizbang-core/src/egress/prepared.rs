//! A picture prepared for a model (plan 2A, task S1; `SPEC.md` section 10, "Egress":
//! "Before sending, image metadata is removed"): fitted to a long edge, then a JPEG at
//! quality 90, or, when any pixel is not opaque, the PNG of `pixels.rs`. Squint
//! prepared its pictures so (`crates/squint-providers/src/prepare.rs`, at the commit
//! `docs/sources.toml` pins): Lanczos3, never larger, JPEG at 90 or PNG with alpha.
//!
//! **What the encoder writes, and what the check knows of it.** The JPEG is what
//! `image`'s baseline encoder writes of pixels alone: the start of the image, its one
//! `APP0` (JFIF, no thumbnail), its tables (`DQT`, `DHT`), one frame (`SOF0`), one
//! scan (`SOS`) and the end of the image, as the last two bytes. The check behind a
//! send reads a part that starts as a JPEG does by those markers and no other
//! ([`is_plain_jpeg`]): any `APP1` to `APP15` (EXIF, XMP, a colour profile), a
//! comment, a second frame or scan, a thumbnail, or a byte after the end is refused.
//! It reads the structure, not the pixels: a JPEG of that structure holds pixels and
//! nothing else, whoever wrote it.

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ExtendedColorType, ImageEncoder, RgbaImage, imageops};

use super::{DataClass, NotAllowed, Piece};
use crate::capability::Picture;

/// The quality of the JPEG a picture is prepared as: squint's `JPEG_QUALITY`.
pub const JPEG_QUALITY: u8 = 90;

/// The size a picture of `width` by `height` is fitted to for a long edge of `long`:
/// the same, when it fits; otherwise the long edge is `long` and the other is scaled
/// to keep the shape, rounded, and at least 1. Never larger.
pub fn fitted(width: u32, height: u32, long: u32) -> (u32, u32) {
    let long = long.max(1);
    let edge = width.max(height);
    if edge <= long {
        return (width, height);
    }
    let scale = |side: u32| {
        let scaled = (u64::from(side) * u64::from(long) + u64::from(edge) / 2) / u64::from(edge);
        u32::try_from(scaled).unwrap_or(long).max(1)
    };
    (scale(width), scale(height))
}

impl Piece {
    /// A picture prepared to be sent, of this class: fitted to `long_edge` with
    /// Lanczos3 (never larger), then the JPEG of its pixels at [`JPEG_QUALITY`], or,
    /// when any pixel is not opaque, the PNG [`Piece::picture`] writes. It is a
    /// picture ([`Piece::is_picture`]), and the check behind a send knows either form.
    ///
    /// Refused, as [`NotAllowed::TooLong`]: a picture the encoder cannot write.
    pub fn prepared(
        class: DataClass,
        picture: &Picture,
        long_edge: u32,
    ) -> Result<Piece, NotAllowed> {
        let (width, height) = (picture.width(), picture.height());
        let (w, h) = fitted(width, height, long_edge);
        let source = RgbaImage::from_raw(width, height, picture.rgba().to_vec())
            .ok_or(NotAllowed::TooLong)?;
        let fitted = if (w, h) == (width, height) {
            source
        } else {
            imageops::resize(&source, w, h, imageops::FilterType::Lanczos3)
        };
        let opaque = fitted
            .as_raw()
            .iter()
            .skip(3)
            .step_by(4)
            .all(|&alpha| alpha == 255);
        if !opaque {
            let fitted = crate::identity::CanonicalPixels::from_rgba(w, h, fitted.into_raw())
                .map_err(|_| NotAllowed::TooLong)?;
            let fitted = Picture::new(fitted).map_err(|_| NotAllowed::TooLong)?;
            return Piece::picture(class, &fitted);
        }
        let rgb = DynamicImage::ImageRgba8(fitted).into_rgb8().into_raw();
        let mut bytes = Vec::new();
        JpegEncoder::new_with_quality(Cursor::new(&mut bytes), JPEG_QUALITY)
            .write_image(&rgb, w, h, ExtendedColorType::Rgb8)
            .map_err(|_| NotAllowed::TooLong)?;
        if !is_plain_jpeg(&bytes) {
            return Err(NotAllowed::TooLong);
        }
        Ok(Piece::of_picture(class, bytes))
    }
}

/// Whether the bytes are a JPEG of the structure the encoder writes, and nothing
/// else: the start of the image; then only `APP0` that is JFIF with no thumbnail,
/// `DQT`, `DHT`, `DRI` and one `SOF0`; one `SOS`; data in which a marker byte is
/// followed only by a stuffed zero or a restart; and the end of the image as the last
/// two bytes. Every length is held against what is left before it is read.
pub fn is_plain_jpeg(bytes: &[u8]) -> bool {
    let Some(mut rest) = bytes.strip_prefix(b"\xff\xd8") else {
        return false;
    };
    let mut frames = 0;
    // The segments before the scan.
    loop {
        let [0xff, marker, high, low, ..] = *rest else {
            return false;
        };
        let length = usize::from(u16::from_be_bytes([high, low]));
        if length < 2 || rest.len() < 2 + length {
            return false;
        }
        let body = &rest[4..2 + length];
        match marker {
            0xe0 => {
                // JFIF: its name, a version, units, two densities, and a thumbnail of
                // no pixels; nothing more.
                let jfif = body.len() == 14
                    && body.starts_with(b"JFIF\0")
                    && body[12] == 0
                    && body[13] == 0;
                if !jfif {
                    return false;
                }
            }
            0xdb | 0xc4 | 0xdd => {}
            0xc0 => frames += 1,
            0xda => {
                if frames != 1 {
                    return false;
                }
                rest = &rest[2 + length..];
                break;
            }
            _ => return false,
        }
        rest = &rest[2 + length..];
    }
    // The scan's data, to the end of the image.
    let Some(data) = rest.strip_suffix(b"\xff\xd9") else {
        return false;
    };
    let mut at = 0;
    while at < data.len() {
        if data[at] == 0xff {
            match data.get(at + 1) {
                Some(0x00 | 0xd0..=0xd7) => at += 2,
                _ => return false,
            }
        } else {
            at += 1;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_size_is_fitted_to_its_long_edge_and_never_made_larger() {
        assert_eq!(fitted(64, 48, 1568), (64, 48));
        assert_eq!(fitted(4000, 3000, 1568), (1568, 1176));
        assert_eq!(fitted(3000, 4000, 1568), (1176, 1568));
        assert_eq!(fitted(10_000, 1, 100), (100, 1));
        assert_eq!(fitted(5, 5, 0), (1, 1));
    }
}
