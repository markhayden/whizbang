//! What is read from a file's container apart from the decoder (ADR 0001): a PNG's colour
//! profile and orientation chunks, wherever they sit, and a JPEG's components, its colour
//! profile segments and whether its data reaches its end marker. A decoder that cannot
//! read one of these says nothing, and a picture read without it would be another picture.

use super::{IdentityError, Result};

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

/// A colour profile found in a container.
pub(super) enum Found {
    /// The container holds none.
    Nothing,
    /// The profile's bytes.
    Profile(Vec<u8>),
}

/// What a PNG's chunks say beside its samples.
pub(super) struct Png {
    pub(super) profile: Found,
    /// The data of the first `eXIf` chunk whose checksum holds, wherever it sits.
    pub(super) exif: Option<Vec<u8>>,
}

/// True when the bytes begin as a PNG does.
pub(super) fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(PNG_SIGNATURE)
}

/// True when the bytes begin as a JPEG does.
pub(super) fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xff, 0xd8])
}

/// Walks a PNG's chunks. The first `iCCP` chunk is the profile: a chunk whose checksum
/// fails, whose layout is wrong or whose data does not inflate is refused, and one that
/// inflates to more than `max_profile_bytes` is over the limit. A walk that runs off the
/// end of the file stops quietly: the decoder refuses such a file.
pub(super) fn png(bytes: &[u8], max_profile_bytes: u64) -> Result<Png> {
    let mut found = Png {
        profile: Found::Nothing,
        exif: None,
    };
    let mut seen_profile = false;
    let mut at = PNG_SIGNATURE.len();
    while let Some(header) = bytes.get(at..at + 8) {
        let length = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        let name = &header[4..8];
        let Some(end) = at.checked_add(12).and_then(|v| v.checked_add(length)) else {
            break;
        };
        let Some(chunk) = bytes.get(at + 4..end) else {
            break;
        };
        let (covered, stored) = chunk.split_at(4 + length);
        let sound = crc32(covered).to_be_bytes() == stored;
        let data = &covered[4..];
        if name == b"iCCP" && !seen_profile {
            seen_profile = true;
            if !sound {
                return Err(unreadable("its checksum fails"));
            }
            found.profile = png_profile(data, max_profile_bytes)?;
        } else if name == b"eXIf" && found.exif.is_none() && sound {
            found.exif = Some(data.to_vec());
        } else if name == b"IEND" {
            break;
        }
        at = end;
    }
    Ok(found)
}

fn unreadable(why: &str) -> IdentityError {
    IdentityError::Unsupported(format!(
        "the colour profile is present and cannot be read: {why}"
    ))
}

/// The profile an `iCCP` chunk holds: a name of 1 to 79 bytes, a zero byte, the compression
/// method (zero), and a zlib stream.
fn png_profile(data: &[u8], max_profile_bytes: u64) -> Result<Found> {
    let name_end = data
        .iter()
        .position(|byte| *byte == 0)
        .filter(|end| (1..=79).contains(end))
        .ok_or_else(|| unreadable("it has no name"))?;
    let stream = match data.get(name_end + 1..) {
        Some([0, stream @ ..]) => stream,
        _ => return Err(unreadable("its compression method is unknown")),
    };
    let limit = usize::try_from(max_profile_bytes).unwrap_or(usize::MAX);
    match miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(stream, limit) {
        Ok(profile) if profile.is_empty() => Ok(Found::Nothing),
        Ok(profile) => Ok(Found::Profile(profile)),
        Err(error) if error.status == miniz_oxide::inflate::TINFLStatus::HasMoreOutput => {
            Err(IdentityError::Limit(format!(
                "the colour profile inflates to more than the limit of {max_profile_bytes} bytes"
            )))
        }
        Err(_) => Err(unreadable("its data does not inflate")),
    }
}

/// The CRC-32 a PNG chunk carries, over its name and data.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// Walks a JPEG's markers and returns its colour profile. Refuses a file whose data ends
/// before its end marker, a file that is not 8 bits a sample, a file with other than one
/// or three components (four is CMYK, whose reading no rule here defines), and a profile
/// whose segments do not make one whole or pass `max_profile_bytes`.
pub(super) fn jpeg(bytes: &[u8], max_profile_bytes: u64) -> Result<Found> {
    let cut_short = || IdentityError::Image("the JPEG ends before its end marker".to_owned());
    // Each piece of a profile: its number, how many there are, and its bytes.
    let mut pieces: Vec<(u8, u8, &[u8])> = Vec::new();
    let mut at = 2;
    loop {
        // A marker is 0xff, any number of further 0xff, then its code.
        if *bytes.get(at).ok_or_else(cut_short)? != 0xff {
            return Err(IdentityError::Image(
                "the JPEG has data where a marker belongs".to_owned(),
            ));
        }
        while *bytes.get(at).ok_or_else(cut_short)? == 0xff {
            at += 1;
        }
        let code = bytes[at];
        at += 1;
        match code {
            // The end of the image. What follows it is not part of the picture.
            0xd9 => break,
            // Markers that stand alone.
            0x01 | 0xd0..=0xd8 => continue,
            _ => {}
        }
        let size = bytes.get(at..at + 2).ok_or_else(cut_short)?;
        let size = usize::from(u16::from_be_bytes([size[0], size[1]]));
        let segment = bytes.get(at + 2..at + size.max(2)).ok_or_else(cut_short)?;
        at += size.max(2);
        match code {
            // A frame header: every start-of-frame marker but the three that are tables.
            0xc0..=0xcf if !matches!(code, 0xc4 | 0xc8 | 0xcc) => {
                let (precision, components) = match segment {
                    [precision, _, _, _, _, components, ..] => (*precision, *components),
                    _ => return Err(cut_short()),
                };
                if precision != 8 {
                    return Err(IdentityError::Unsupported(format!(
                        "a JPEG of {precision} bits a sample has no canonical pixels"
                    )));
                }
                if !matches!(components, 1 | 3) {
                    return Err(IdentityError::Unsupported(format!(
                        "a JPEG of {components} components (CMYK has four) has no canonical \
                         pixels"
                    )));
                }
            }
            0xe2 => {
                if let Some([number, count, data @ ..]) = segment.strip_prefix(b"ICC_PROFILE\0") {
                    pieces.push((*number, *count, data));
                }
            }
            // A scan: its data runs to the next marker that is not a stuffed zero, a
            // restart or a fill byte.
            0xda => loop {
                match bytes.get(at..at + 2).ok_or_else(cut_short)? {
                    [0xff, 0x00 | 0xd0..=0xd7] => at += 2,
                    [0xff, 0xff] => at += 1,
                    [0xff, _] => break,
                    _ => at += 1,
                }
            },
            _ => {}
        }
    }
    if pieces.is_empty() {
        return Ok(Found::Nothing);
    }
    // The pieces are numbered from 1 and each says how many there are; they may come in
    // any order, and each number comes once.
    let count = pieces[0].1;
    let mut profile = Vec::new();
    for number in 1..=count {
        let mut matching = pieces
            .iter()
            .filter(|(piece, of, _)| *piece == number && *of == count);
        match (matching.next(), matching.next()) {
            (Some((_, _, data)), None) => profile.extend_from_slice(data),
            _ => return Err(unreadable("its segments do not make one whole")),
        }
    }
    if count == 0 || pieces.len() != usize::from(count) {
        return Err(unreadable("its segments do not make one whole"));
    }
    if profile.len() as u64 > max_profile_bytes {
        return Err(IdentityError::Limit(format!(
            "the colour profile is {} bytes, over the limit of {max_profile_bytes}",
            profile.len()
        )));
    }
    if profile.is_empty() {
        return Ok(Found::Nothing);
    }
    Ok(Found::Profile(profile))
}
