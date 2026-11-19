//! The wire of fal's filler: a request written in pieces for the gate, and an answer read
//! to its pixels.
//!
//! **The request** is one JSON object, `{"image_url":"data:image/png;base64,<picture>",
//! "mask_url":"data:image/png;base64,<mask>","sync_mode":true}`, the fields of the model's
//! input that fal's page names (read 2026-11-18). Each picture is a piece of its own, the
//! PNG `Piece::picture` writes, which the sender writes as the base64 of exactly its bytes;
//! the words before and between them are of the class of the picture they lead into, and
//! the words after the mask are of the mask's, so the request needs `source_images` and
//! `derived_images` and nothing else.
//!
//! **The answer** is `{"image":{"url":"data:image/...;base64,...", ...}}`. Any text in it
//! that holds `://` is an address of a host, and the answer is refused whole: with
//! `sync_mode` none is to come, and none is followed.

use image::{RgbaImage, imageops};
use serde_json::Value;
use whizbang_core::capability::{Fault, FaultKind, FillRequest, Painted, Picture, Support};
use whizbang_core::egress::{DataClass, Piece, fitted};
use whizbang_core::identity::{CanonicalPixels, DecodeLimits, identify};

use super::NOT_WRITTEN;

/// What a fault says of an answer of 200 that is no answer of the model's.
pub const NOT_AN_ANSWER: &str = "the provider's answer is not an answer of the model";
/// What it says of an answer that holds the address of a host.
pub const AN_ADDRESS: &str =
    "the provider's answer holds the address of a host, and nothing is fetched from one";
/// What it says of an answer whose picture is not of the size sent.
pub const WRONG_SIZE: &str = "the provider's picture is not of the size it was sent";

/// A request as written: its pieces, and the size it was sent at.
pub(crate) struct Written {
    pub(crate) pieces: Vec<Piece>,
    pub(crate) sent: (u32, u32),
}

/// The binary mask of a support: white where the model is to paint, black elsewhere,
/// every pixel opaque.
fn mask_of(support: &Support) -> Vec<u8> {
    support
        .plane()
        .iter()
        .flat_map(|&sample| {
            let v = if sample == Support::PAINT { 255 } else { 0 };
            [v, v, v, 255]
        })
        .collect()
}

/// A picture of these four bytes a pixel.
fn picture_of(width: u32, height: u32, rgba: Vec<u8>) -> Result<Picture, Fault> {
    let unfit = |_| Fault::new(FaultKind::Rejected, NOT_WRITTEN).not_sent();
    let pixels = CanonicalPixels::from_rgba(width, height, rgba).map_err(|_| unfit(()))?;
    Picture::new(pixels).map_err(|_| unfit(()))
}

/// Pixels resized with this filter.
fn resized(
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    to: (u32, u32),
    filter: imageops::FilterType,
) -> Option<Vec<u8>> {
    let source = RgbaImage::from_raw(width, height, rgba)?;
    Some(imageops::resize(&source, to.0, to.1, filter).into_raw())
}

/// The request's picture and mask, fitted to `long_side`: the picture by Lanczos3, the
/// mask by a triangle and then cut at half way back to black and white, as rascal scaled
/// a binary mask (`fill_max_side`).
fn fitted_pair(request: &FillRequest, long_side: u32) -> Result<(Picture, Picture), Fault> {
    let image = request.image();
    let (width, height) = (image.width(), image.height());
    let mask = mask_of(request.support());
    let to = fitted(width, height, long_side);
    if to == (width, height) {
        return Ok((image.clone(), picture_of(width, height, mask)?));
    }
    let unfit = || Fault::new(FaultKind::Rejected, NOT_WRITTEN).not_sent();
    let rgba = resized(
        width,
        height,
        image.rgba().to_vec(),
        to,
        imageops::FilterType::Lanczos3,
    )
    .ok_or_else(unfit)?;
    let mut mask =
        resized(width, height, mask, to, imageops::FilterType::Triangle).ok_or_else(unfit)?;
    for pixel in mask.chunks_mut(4) {
        let v = if pixel[0] >= 128 { 255 } else { 0 };
        pixel.copy_from_slice(&[v, v, v, 255]);
    }
    Ok((picture_of(to.0, to.1, rgba)?, picture_of(to.0, to.1, mask)?))
}

/// The request written in pieces, fitted to `long_side`.
pub(crate) fn request(request: &FillRequest, long_side: u32) -> Result<Written, Fault> {
    let (picture, mask) = fitted_pair(request, long_side)?;
    let unfit = |_| Fault::new(FaultKind::Rejected, NOT_WRITTEN).not_sent();
    let pieces = vec![
        Piece::of(
            DataClass::SourceImages,
            b"{\"image_url\":\"data:image/png;base64,".to_vec(),
        ),
        Piece::picture(DataClass::SourceImages, &picture).map_err(unfit)?,
        Piece::of(
            DataClass::DerivedImages,
            b"\",\"mask_url\":\"data:image/png;base64,".to_vec(),
        ),
        Piece::picture(DataClass::DerivedImages, &mask).map_err(unfit)?,
        Piece::of(DataClass::DerivedImages, b"\",\"sync_mode\":true}".to_vec()),
    ];
    Ok(Written {
        pieces,
        sent: (picture.width(), picture.height()),
    })
}

/// Whether any text in the value, a key or a string, holds `://`.
fn holds_an_address(value: &Value) -> bool {
    match value {
        Value::String(text) => text.contains("://"),
        Value::Array(items) => items.iter().any(holds_an_address),
        Value::Object(fields) => fields
            .iter()
            .any(|(key, value)| key.contains("://") || holds_an_address(value)),
        _ => false,
    }
}

/// The bytes of a data address of a picture: `data:image/<type>;base64,<data>`.
fn data_of(address: &str) -> Option<Vec<u8>> {
    let rest = address.strip_prefix("data:image/")?;
    let (media, data) = rest.split_once(";base64,")?;
    let plain = !media.is_empty()
        && media
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'-' || b == b'.');
    if !plain {
        return None;
    }
    unbase64(data)
}

/// The base64 of RFC 4648, with its padding, read strictly: any other byte, a length
/// that is not a multiple of four, or padding anywhere but the end, is none.
pub(crate) fn unbase64(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let value = |c: u8| -> Option<u32> {
        Some(u32::from(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        }))
    };
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let groups = bytes.len() / 4;
    for (at, group) in bytes.chunks(4).enumerate() {
        let padding = group.iter().rev().take_while(|&&c| c == b'=').count();
        if padding > 2 || (padding > 0 && at + 1 != groups) {
            return None;
        }
        let mut all = 0u32;
        for (i, &c) in group[..4 - padding].iter().enumerate() {
            all |= value(c)? << (18 - 6 * i);
        }
        out.extend_from_slice(&all.to_be_bytes()[1..4 - padding]);
    }
    Some(out)
}

/// An answer of 200 read to the pixels of the request's size: its picture, of the size
/// sent, scaled back when the request was scaled to be sent.
pub(crate) fn painted(
    body: &[u8],
    request: &FillRequest,
    written: &Written,
) -> Result<Painted, Fault> {
    // An answer of 200 was charged: a fault after it costs the price, its bound.
    let malformed = |why| Err(Fault::new(FaultKind::Malformed, why).answered(None));
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return malformed(NOT_AN_ANSWER);
    };
    if holds_an_address(&value) {
        return malformed(AN_ADDRESS);
    }
    let Some(bytes) = value["image"]["url"].as_str().and_then(data_of) else {
        return malformed(NOT_AN_ANSWER);
    };
    let Ok(identified) = identify(&bytes, &DecodeLimits::default()) else {
        return malformed(NOT_AN_ANSWER);
    };
    let pixels = identified.pixels;
    if (pixels.width(), pixels.height()) != written.sent {
        return malformed(WRONG_SIZE);
    }
    let size = (request.image().width(), request.image().height());
    let rgba = if written.sent == size {
        pixels.rgba().to_vec()
    } else {
        let back = resized(
            written.sent.0,
            written.sent.1,
            pixels.rgba().to_vec(),
            size,
            imageops::FilterType::Lanczos3,
        );
        match back {
            Some(rgba) => rgba,
            None => return malformed(NOT_AN_ANSWER),
        }
    };
    let rgb = rgba
        .chunks(4)
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    match Painted::from_rgb(size.0, size.1, rgb) {
        Ok(painted) => Ok(painted),
        Err(_) => malformed(NOT_AN_ANSWER),
    }
}
