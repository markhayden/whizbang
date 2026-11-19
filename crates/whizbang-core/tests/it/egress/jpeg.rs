//! `jpeg::…`: the structure the check behind a send holds a prepared JPEG to (plan 2C,
//! task F1; the carry-over row on a prepared JPEG known by its markers alone): each
//! table the encoder's own at its quality, and the frame and the scan's header those of
//! a baseline JPEG, exactly.

use whizbang_core::egress::{DataClass, Piece, is_plain_jpeg};

use super::prepare::{patterned, with_segment};

/// A path, as a table made by hand would carry it.
const PATH: &[u8] = b"/srv/someone/private/store/objects/0123456789abcdef";

/// The offset of the first segment of this marker.
fn segment(jpeg: &[u8], marker: u8) -> usize {
    jpeg.windows(2)
        .position(|pair| pair == [0xff, marker])
        .unwrap()
}

/// The same JPEG with the body of the first segment of this marker replaced.
fn with_body(jpeg: &[u8], marker: u8, body: &[u8]) -> Vec<u8> {
    let at = segment(jpeg, marker);
    let length = usize::from(u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]));
    let mut file = jpeg[..at + 2].to_vec();
    file.extend_from_slice(&u16::try_from(body.len() + 2).unwrap().to_be_bytes());
    file.extend_from_slice(body);
    file.extend_from_slice(&jpeg[at + 2 + length..]);
    file
}

fn body_of(jpeg: &[u8], marker: u8) -> Vec<u8> {
    let at = segment(jpeg, marker);
    let length = usize::from(u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]));
    jpeg[at + 4..at + 2 + length].to_vec()
}

#[test]
fn a_table_body_out_of_its_structure_is_refused_by_the_check() {
    let prepared = |w, h| {
        let piece = Piece::prepared(DataClass::SourceImages, &patterned(w, h, false), 1568);
        piece.unwrap().bytes().to_vec()
    };
    let jpeg = prepared(40, 30);
    // Every prepared JPEG is known, whatever its size and pixels: its tables are the
    // encoder's own at its quality.
    for file in [&jpeg, &prepared(256, 7), &prepared(3, 200)] {
        assert!(is_plain_jpeg(file));
    }
    // A table of quantization values that are a path: of the structure (one 8-bit table
    // of 64 values, each above 0), but not one the encoder writes.
    let mut quantized = vec![0x00];
    quantized.extend(PATH.iter().cycle().take(64));
    // A Huffman table whose values are a path, its counts summing to their number.
    let mut huffman = vec![0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    huffman[9] = u8::try_from(PATH.len()).unwrap();
    huffman.extend_from_slice(PATH);
    // The frame and the scan: what a baseline header holds, and a byte more or other.
    let frame = body_of(&jpeg, 0xc0);
    let mut longer_frame = frame.clone();
    longer_frame.extend_from_slice(PATH);
    let mut twelve_bits = frame.clone();
    twelve_bits[0] = 12;
    let scan = body_of(&jpeg, 0xda);
    let mut longer_scan = scan.clone();
    longer_scan.extend_from_slice(PATH);
    let mut progressive = scan.clone();
    let last = progressive.len() - 2;
    progressive[last] = 5;
    let cases = [
        (
            "a quantization table of a path",
            with_body(&jpeg, 0xdb, &quantized),
        ),
        (
            "a second table of a path",
            with_segment(&jpeg, 0xdb, &quantized),
        ),
        (
            "a Huffman table of a path",
            with_body(&jpeg, 0xc4, &huffman),
        ),
        (
            "a restart interval of more bytes",
            with_segment(&jpeg, 0xdd, PATH),
        ),
        (
            "a frame with a path after it",
            with_body(&jpeg, 0xc0, &longer_frame),
        ),
        ("a frame of 12 bits", with_body(&jpeg, 0xc0, &twelve_bits)),
        (
            "a scan header with a path after it",
            with_body(&jpeg, 0xda, &longer_scan),
        ),
        (
            "a scan of part of the spectrum",
            with_body(&jpeg, 0xda, &progressive),
        ),
    ];
    for (what, file) in cases {
        assert!(!is_plain_jpeg(&file), "{what}");
    }
    // A restart interval of its two bytes is still known.
    assert!(is_plain_jpeg(&with_segment(&jpeg, 0xdd, &[0, 8])));
}
