//! Nothing but pixels (`SPEC.md` section 10, "Egress": "Before sending, image metadata
//! is removed"; ADR 0019, "Nothing but pixels"; plan 1B, task E6).
//!
//! An image leaves as a [`Picture`]: canonical pixels (ADR 0001), which have no place
//! for metadata, a file name or a path. This is the one encoder that makes the bytes
//! of a piece from one, and the reader that knows its output again.
//!
//! **What the encoder writes** is a PNG of three chunks and no other: `IHDR`, one
//! `IDAT`, `IEND`. Eight bits a sample, red, green, blue and alpha, not interlaced;
//! every row after the byte that says it is not filtered; and the rows stored, not
//! compressed, in blocks of 65,535 bytes, the last of them shorter or as long. There
//! is one such file for a picture, so the reader can say of any bytes whether they
//! are one, and it reads no other PNG as one.

use super::{DataClass, NotAllowed, Piece};
use crate::capability::Picture;

/// What every PNG starts with.
pub(super) const SIGNATURE: [u8; 8] = *b"\x89PNG\r\n\x1a\n";

/// The most bytes of one stored block.
const BLOCK: usize = 65_535;

/// Bytes before the first block: the signature, the header's chunk, the length and
/// the name of the image data's chunk, and the two bytes that start its stream.
const BEFORE: usize = 8 + (12 + 13) + 8 + 2;

/// Bytes after the last block: the stream's checksum, the image data's checksum, and
/// the end's chunk.
const AFTER: usize = 4 + 4 + 12;

/// The checksum of a chunk, over its name and its data (CRC-32, as PNG has it), kept
/// as it goes.
struct Crc(u32);

/// What one byte does to that checksum, for each byte.
const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut byte = 0;
    while byte < 256 {
        let mut sum = byte as u32;
        let mut bit = 0;
        while bit < 8 {
            sum = if sum & 1 == 1 {
                (sum >> 1) ^ 0xEDB8_8320
            } else {
                sum >> 1
            };
            bit += 1;
        }
        table[byte] = sum;
        byte += 1;
    }
    table
};

impl Crc {
    fn new() -> Crc {
        Crc(u32::MAX)
    }

    fn take(&mut self, bytes: &[u8]) {
        for byte in bytes {
            let at = (self.0 ^ u32::from(*byte)) & 0xff;
            self.0 = CRC_TABLE[at as usize] ^ (self.0 >> 8);
        }
    }

    fn sum(&self) -> u32 {
        !self.0
    }
}

/// The checksum of a stream's content (Adler-32, as zlib has it), kept as it goes.
struct Adler {
    low: u32,
    high: u32,
}

impl Adler {
    fn new() -> Adler {
        Adler { low: 1, high: 0 }
    }

    fn take(&mut self, bytes: &[u8]) {
        // 5,552 bytes is the most that can be added before a sum must be reduced to
        // stay within 32 bits.
        for some in bytes.chunks(5552) {
            for byte in some {
                self.low += u32::from(*byte);
                self.high += self.low;
            }
            self.low %= 65_521;
            self.high %= 65_521;
        }
    }

    fn sum(&self) -> u32 {
        (self.high << 16) | self.low
    }
}

/// How many bytes the rows of an image take with one byte before each, and how many
/// its whole file takes. Nothing for a side of no pixels, and for a size whose image
/// data would not fit the one chunk it is written in.
fn sizes(width: u32, height: u32) -> Option<(usize, usize)> {
    if width == 0 || height == 0 {
        return None;
    }
    let row = usize::try_from(width)
        .ok()?
        .checked_mul(4)?
        .checked_add(1)?;
    let rows = row.checked_mul(usize::try_from(height).ok()?)?;
    let stream = rows.checked_add(rows.div_ceil(BLOCK).checked_mul(5)?)?;
    // A chunk of a PNG is 2,147,483,647 bytes at most, and holds the stream between
    // its two bytes of start and its four of checksum.
    i32::try_from(stream.checked_add(6)?).ok()?;
    Some((rows, BEFORE.checked_add(stream)?.checked_add(AFTER)?))
}

/// What the file of an image of this size starts with, up to its first block, when
/// its blocks take `stream` bytes.
fn before(width: u32, height: u32, stream: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(BEFORE);
    out.extend_from_slice(&SIGNATURE);
    out.extend_from_slice(&13u32.to_be_bytes());
    let from = out.len();
    out.extend_from_slice(b"IHDR");
    out.extend_from_slice(&width.to_be_bytes());
    out.extend_from_slice(&height.to_be_bytes());
    // Eight bits a sample; colour with alpha; the one compression and the one
    // filtering PNG has; not interlaced.
    out.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut crc = Crc::new();
    crc.take(out.get(from..).unwrap_or_default());
    out.extend_from_slice(&crc.sum().to_be_bytes());
    out.extend_from_slice(&((stream + 6) as u32).to_be_bytes());
    out.extend_from_slice(b"IDAT");
    // A stream of the one method there is, with no dictionary.
    out.extend_from_slice(&[0x78, 0x01]);
    out
}

/// The five bytes before a stored block of this many bytes: whether it is the last,
/// and its length twice, the second time with every bit turned.
fn block_head(last: bool, length: usize) -> [u8; 5] {
    let [low, high] = (length as u16).to_le_bytes();
    [u8::from(last), low, high, !low, !high]
}

/// What the file ends with, after its last block: the checksum of the rows, the
/// checksum of the image data's chunk, and the chunk that ends a PNG.
fn after(rows: u32, chunk: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(AFTER);
    out.extend_from_slice(&rows.to_be_bytes());
    out.extend_from_slice(&chunk.to_be_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(b"IEND");
    let mut crc = Crc::new();
    crc.take(b"IEND");
    out.extend_from_slice(&crc.sum().to_be_bytes());
    out
}

/// The file of these pixels: `rgba` holds four bytes for each of `width` by `height`
/// pixels, row by row. Nothing for a size [`sizes`] has nothing for, and for pixels
/// that are not of that size.
fn write(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let (rows, whole) = sizes(width, height)?;
    let line = (rows / height as usize).checked_sub(1)?;
    if rgba.len() != rows - height as usize {
        return None;
    }
    let mut raw = Vec::with_capacity(rows);
    for pixels in rgba.chunks(line) {
        raw.push(0);
        raw.extend_from_slice(pixels);
    }
    let mut out = Vec::with_capacity(whole);
    out.extend_from_slice(&before(width, height, whole - BEFORE - AFTER));
    let blocks = raw.len().div_ceil(BLOCK);
    for (at, block) in raw.chunks(BLOCK).enumerate() {
        out.extend_from_slice(&block_head(at + 1 == blocks, block.len()));
        out.extend_from_slice(block);
    }
    let mut adler = Adler::new();
    adler.take(&raw);
    let adler = adler.sum();
    // The chunk's checksum is over its name and its data: from the name, which is
    // six bytes before the first block, to the rows' checksum.
    let mut crc = Crc::new();
    crc.take(out.get(BEFORE - 6..).unwrap_or_default());
    crc.take(&adler.to_be_bytes());
    out.extend_from_slice(&after(adler, crc.sum()));
    Some(out)
}

/// Whether these bytes are a file the encoder writes: for some pixels, exactly the
/// bytes [`write`] gives. A file with one chunk more, one byte more or fewer, a
/// filtered row, a compressed block, blocks cut elsewhere or a wrong checksum is not.
///
/// The size is read from where the encoder writes it, and the length that size gives
/// is held against the bytes' length before anything else is read. Nothing is
/// copied.
pub(super) fn is_encoded(bytes: &[u8]) -> bool {
    let size = |at: usize| {
        let four = bytes.get(at..at + 4)?;
        Some(u32::from_be_bytes(four.try_into().ok()?))
    };
    let (Some(width), Some(height)) = (size(16), size(20)) else {
        return false;
    };
    let Some((rows, whole)) = sizes(width, height) else {
        return false;
    };
    if bytes.len() != whole {
        return false;
    }
    let (start, rest) = bytes.split_at(BEFORE);
    let (stream, end) = rest.split_at(rest.len() - AFTER);
    if start != before(width, height, stream.len()) {
        return false;
    }
    let row = rows / height as usize;
    let blocks = rows.div_ceil(BLOCK);
    let mut adler = Adler::new();
    // How many bytes of the rows came before the block in hand.
    let mut came = 0;
    for (at, block) in stream.chunks(BLOCK + 5).enumerate() {
        let Some((head, stored)) = block.split_at_checked(5) else {
            return false;
        };
        if head != block_head(at + 1 == blocks, stored.len()) {
            return false;
        }
        // Each row starts with the byte that says it is not filtered.
        let first = (row - came % row) % row;
        if stored
            .iter()
            .skip(first)
            .step_by(row)
            .any(|byte| *byte != 0)
        {
            return false;
        }
        adler.take(stored);
        came += stored.len();
    }
    let adler = adler.sum();
    let mut crc = Crc::new();
    crc.take(start.get(BEFORE - 6..).unwrap_or_default());
    crc.take(stream);
    crc.take(&adler.to_be_bytes());
    end == after(adler, crc.sum())
}

impl Piece {
    /// A piece that is the pixels of a picture, of this class: the bytes the encoder
    /// writes of them, which hold no metadata, no file name and no path, because a
    /// picture has none to give. It is the one form in which an image leaves: the
    /// check behind a send refuses an image file in any other.
    ///
    /// Refused, as [`NotAllowed::TooLong`]: a picture whose rows do not fit one
    /// chunk of a PNG, which none within
    /// [`PIXEL_LIMIT`](crate::capability::PIXEL_LIMIT) is.
    pub fn picture(class: DataClass, picture: &Picture) -> Result<Piece, NotAllowed> {
        write(picture.width(), picture.height(), picture.rgba())
            .map(|bytes| Piece::of_picture(class, bytes))
            .ok_or(NotAllowed::TooLong)
    }

    /// A piece of this class that is the picture's canonical pixels as they are, four
    /// samples a pixel, row by row from the top left: what a request of frames carries
    /// a picture as (`pack/1`, ADR 0020). It is made from a picture and from nothing
    /// else, so it holds no metadata, no name and no path; the check behind a send
    /// passes it as pixels, as it passes what the encoder writes.
    pub fn pixels(class: DataClass, picture: &Picture) -> Piece {
        Piece::of_pixels(class, picture.rgba().to_vec())
    }
}

#[cfg(test)]
mod tests {
    //! What no picture reaches: the sizes that have no file, without a byte of
    //! pixels.

    use super::*;

    #[test]
    fn a_size_of_no_pixels_and_a_size_past_one_chunk_have_no_file() {
        assert_eq!(sizes(0, 1), None);
        assert_eq!(sizes(1, 0), None);
        assert_eq!(sizes(1, 1), Some((5, BEFORE + 5 + 5 + AFTER)));
        // 255 rows of 257 bytes fill one block exactly; one row more starts a second.
        assert_eq!(sizes(64, 255), Some((65_535, BEFORE + 65_535 + 5 + AFTER)));
        assert_eq!(sizes(64, 256), Some((65_792, BEFORE + 65_792 + 10 + AFTER)));
        // A chunk of a PNG is 2,147,483,647 bytes at most. The rows of 16,383 by
        // 32,000 pixels and the five bytes of each of their blocks fit it; those of
        // 16,383 by 32,770 do not.
        let rows: usize = 65_533 * 32_000;
        let stream = rows + 5 * rows.div_ceil(BLOCK);
        assert_eq!(sizes(16_383, 32_000), Some((rows, BEFORE + stream + AFTER)));
        assert_eq!(sizes(16_383, 32_770), None);
        assert_eq!(sizes(16_384, 65_535), None);
        assert_eq!(sizes(u32::MAX, u32::MAX), None);
        // The last height of a column three pixels wide whose image data fits, and
        // the first whose does not: its blocks alone would, and the six bytes around
        // them take it over.
        let rows: usize = 13 * 165_178_446;
        assert_eq!(rows + 5 * rows.div_ceil(BLOCK) + 6, 2_147_483_634);
        assert_eq!(
            sizes(3, 165_178_446),
            Some((rows, BEFORE + 2_147_483_628 + AFTER))
        );
        let rows: usize = 13 * 165_178_447;
        assert_eq!(rows + 5 * rows.div_ceil(BLOCK), 2_147_483_646);
        assert_eq!(sizes(3, 165_178_447), None);
    }

    #[test]
    fn pixels_that_are_not_of_the_size_have_no_file() {
        assert!(write(2, 1, &[0; 8]).is_some());
        assert_eq!(write(2, 1, &[0; 7]), None);
        assert_eq!(write(2, 1, &[0; 9]), None);
        assert_eq!(write(0, 1, &[]), None);
    }

    #[test]
    fn the_checksums_are_the_ones_png_and_zlib_name() {
        // The check values both formats give for the nine digits.
        let mut crc = Crc::new();
        crc.take(b"1234");
        crc.take(b"56789");
        assert_eq!(crc.sum(), 0xCBF4_3926);
        let mut adler = Adler::new();
        adler.take(b"123456789");
        assert_eq!(adler.sum(), 0x091E_01DE);
        // And past the point where a sum is reduced.
        let mut adler = Adler::new();
        adler.take(&[255; 6000]);
        let (mut low, mut high) = (1u64, 0u64);
        for _ in 0..6000 {
            low = (low + 255) % 65_521;
            high = (high + low) % 65_521;
        }
        assert_eq!(u64::from(adler.sum()), (high << 16) | low);
    }
}
