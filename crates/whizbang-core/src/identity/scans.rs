//! Whether a JPEG's scans hold the data its frame calls for (ADR 0001, "A JPEG's
//! scans"; plan m3, task O5). The decoder fills what a scan lacks and says nothing, so a
//! file cut short inside a scan, or a scan header followed at once by the end marker,
//! would be read as a picture made partly of the decoder's filler. Here every scan is
//! walked through its Huffman codes, block by block, without a coefficient being
//! dequantised or transformed: a code that needs a bit the scan does not hold refuses
//! the file. And when the walk reaches the end marker, every coefficient of every
//! component must have been coded to its last bit (successive approximation ending at
//! zero), which a sequential file does in its scans and a progressive one in its last.
//!
//! Sequential (`SOF0`, `SOF1`) and progressive (`SOF2`) Huffman coding are read; any
//! other coding (arithmetic, lossless, hierarchical) is refused as unsupported, as the
//! decoder cannot read it either. A file of more than [`SCANS_MOST`] scans is refused,
//! so the walk costs a bounded number of passes over the image's blocks.

use super::{IdentityError, Result};

/// The most scans a JPEG may have: a progressive file has about ten.
pub(super) const SCANS_MOST: usize = 256;

fn short() -> IdentityError {
    IdentityError::Image("a JPEG scan does not hold the data its frame calls for".to_owned())
}

fn corrupt(why: &str) -> IdentityError {
    IdentityError::Image(format!("the JPEG's scans cannot be read: {why}"))
}

/// A Huffman table: how many codes of each length, and their symbols in order.
#[derive(Clone)]
struct Table {
    first: [u32; 17],
    count: [u32; 17],
    offset: [usize; 17],
    symbols: Vec<u8>,
}

impl Table {
    fn of(counts: &[u8], symbols: &[u8]) -> Table {
        let mut table = Table {
            first: [0; 17],
            count: [0; 17],
            offset: [0; 17],
            symbols: symbols.to_vec(),
        };
        let (mut code, mut at) = (0u32, 0usize);
        for length in 1..=16 {
            let n = u32::from(counts[length - 1]);
            table.first[length] = code;
            table.count[length] = n;
            table.offset[length] = at;
            code = (code + n) << 1;
            at += n as usize;
        }
        table
    }
}

/// The bits of one scan's entropy-coded data, which ends at the first marker that is
/// neither a stuffed zero nor a restart.
struct Bits<'a> {
    bytes: &'a [u8],
    at: usize,
    byte: u8,
    left: u8,
}

impl Bits<'_> {
    fn bit(&mut self) -> Result<u32> {
        if self.left == 0 {
            let byte = *self.bytes.get(self.at).ok_or_else(short)?;
            // A byte of 0xff is followed by a stuffed zero; anything else there is a
            // marker, and the scan's data has ended.
            if byte == 0xff {
                if self.bytes.get(self.at + 1) != Some(&0x00) {
                    return Err(short());
                }
                self.at += 2;
            } else {
                self.at += 1;
            }
            self.byte = byte;
            self.left = 8;
        }
        self.left -= 1;
        Ok(u32::from(self.byte >> self.left) & 1)
    }

    fn bits(&mut self, n: u32) -> Result<u32> {
        let mut value = 0;
        for _ in 0..n {
            value = (value << 1) | self.bit()?;
        }
        Ok(value)
    }

    fn decode(&mut self, table: &Table) -> Result<u8> {
        let mut code = 0u32;
        for length in 1..=16 {
            code = (code << 1) | self.bit()?;
            if code >= table.first[length] && code - table.first[length] < table.count[length] {
                let index = table.offset[length] + (code - table.first[length]) as usize;
                return table
                    .symbols
                    .get(index)
                    .copied()
                    .ok_or_else(|| corrupt("a Huffman table is short of its symbols"));
            }
        }
        Err(corrupt("a code is in no table"))
    }

    /// Past a restart marker, which must be next once the bits left are let go.
    fn restart(&mut self) -> Result<()> {
        self.left = 0;
        while self.bytes.get(self.at..self.at + 2) == Some(&[0xff, 0xff]) {
            self.at += 1;
        }
        match self.bytes.get(self.at..self.at + 2) {
            Some([0xff, 0xd0..=0xd7]) => {
                self.at += 2;
                Ok(())
            }
            _ => Err(short()),
        }
    }
}

/// One component of the frame.
struct Component {
    id: u8,
    h: u32,
    v: u32,
    /// The coefficients coded to their last bit, one bit each in zigzag order.
    whole: u64,
    /// For a progressive file, which coefficients of each block are not zero so far.
    nonzero: Vec<u64>,
}

struct Frame {
    progressive: bool,
    width: u32,
    height: u32,
    hmax: u32,
    vmax: u32,
    components: Vec<Component>,
}

impl Frame {
    /// A component's blocks across and down, as a scan of it alone codes them.
    fn blocks(&self, c: &Component) -> (u32, u32) {
        let across = (self.width * c.h).div_ceil(self.hmax).div_ceil(8);
        let down = (self.height * c.v).div_ceil(self.vmax).div_ceil(8);
        (across, down)
    }
}

/// What one scan codes.
struct Scan {
    components: Vec<(usize, usize, usize)>,
    ss: usize,
    se: usize,
    ah: u8,
    al: u8,
}

/// Refuses a JPEG whose scans do not hold what its frame calls for; the module's
/// description says how. The bytes start with the start-of-image marker.
pub(super) fn whole(bytes: &[u8]) -> Result<()> {
    let mut dc: [Option<Table>; 4] = Default::default();
    let mut ac: [Option<Table>; 4] = Default::default();
    let mut interval = 0u32;
    let mut frame: Option<Frame> = None;
    let mut scans = 0usize;
    let mut at = 2usize;
    loop {
        while bytes.get(at) == Some(&0xff) && bytes.get(at + 1) == Some(&0xff) {
            at += 1;
        }
        let (Some(0xff), Some(code)) = (bytes.get(at), bytes.get(at + 1).copied()) else {
            return Err(short());
        };
        at += 2;
        match code {
            0xd9 => break,
            0x01 | 0xd0..=0xd8 => continue,
            _ => {}
        }
        let size = bytes.get(at..at + 2).ok_or_else(short)?;
        let size = usize::from(u16::from_be_bytes([size[0], size[1]]));
        let segment = bytes.get(at + 2..at + size.max(2)).ok_or_else(short)?;
        at += size.max(2);
        match code {
            // One frame: its size is the one the limits were checked against.
            0xc0..=0xc2 if frame.is_some() => return Err(corrupt("it has two frames")),
            0xc0..=0xc2 => frame = Some(frame_of(code, segment)?),
            0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf | 0xcc => {
                return Err(IdentityError::Unsupported(
                    "a JPEG coded other than by Huffman's sequential or progressive DCT has no canonical pixels".to_owned(),
                ));
            }
            0xc4 => tables(segment, &mut dc, &mut ac)?,
            0xdd => {
                let [a, b, ..] = segment else {
                    return Err(corrupt("a restart interval is short"));
                };
                interval = u32::from(u16::from_be_bytes([*a, *b]));
            }
            0xda => {
                scans += 1;
                if scans > SCANS_MOST {
                    return Err(IdentityError::Limit(format!(
                        "the JPEG has more than {SCANS_MOST} scans"
                    )));
                }
                let frame = frame
                    .as_mut()
                    .ok_or_else(|| corrupt("a scan comes before the frame"))?;
                let scan = scan_of(segment, frame)?;
                let mut bits = Bits {
                    bytes,
                    at,
                    byte: 0,
                    left: 0,
                };
                walk(frame, &scan, &dc, &ac, interval, &mut bits)?;
                at = bits.at;
                // What follows the last code, to the next marker, is padding.
                loop {
                    match bytes.get(at..at + 2).ok_or_else(short)? {
                        [0xff, 0x00 | 0xd0..=0xd7] => at += 2,
                        [0xff, 0xff] => at += 1,
                        [0xff, _] => break,
                        _ => at += 1,
                    }
                }
            }
            _ => {}
        }
    }
    let frame = frame.ok_or_else(|| corrupt("it has no frame"))?;
    if frame.components.iter().any(|c| c.whole != u64::MAX) {
        return Err(short());
    }
    Ok(())
}

fn frame_of(code: u8, segment: &[u8]) -> Result<Frame> {
    let [_, h0, h1, w0, w1, count, rest @ ..] = segment else {
        return Err(corrupt("its frame header is short"));
    };
    let (height, width) = (
        u32::from(u16::from_be_bytes([*h0, *h1])),
        u32::from(u16::from_be_bytes([*w0, *w1])),
    );
    if height == 0 || width == 0 {
        return Err(IdentityError::Unsupported(
            "a JPEG whose height is given after its scans has no canonical pixels".to_owned(),
        ));
    }
    let mut components = Vec::new();
    for c in rest.as_chunks::<3>().0.iter().take(usize::from(*count)) {
        let (h, v) = (u32::from(c[1] >> 4), u32::from(c[1] & 15));
        if !(1..=4).contains(&h) || !(1..=4).contains(&v) {
            return Err(corrupt("a component's sampling is out of range"));
        }
        components.push(Component {
            id: c[0],
            h,
            v,
            whole: 0,
            nonzero: Vec::new(),
        });
    }
    if components.len() != usize::from(*count) || components.is_empty() {
        return Err(corrupt("its frame header is short"));
    }
    Ok(Frame {
        progressive: code == 0xc2,
        width,
        height,
        hmax: components.iter().map(|c| c.h).max().unwrap_or(1),
        vmax: components.iter().map(|c| c.v).max().unwrap_or(1),
        components,
    })
}

fn tables(
    mut segment: &[u8],
    dc: &mut [Option<Table>; 4],
    ac: &mut [Option<Table>; 4],
) -> Result<()> {
    while let [class, rest @ ..] = segment {
        let counts = rest
            .get(..16)
            .ok_or_else(|| corrupt("a Huffman table is short"))?;
        let n: usize = counts.iter().map(|c| usize::from(*c)).sum();
        let symbols = rest
            .get(16..16 + n)
            .ok_or_else(|| corrupt("a Huffman table is short"))?;
        let (kind, slot) = (class >> 4, usize::from(class & 15));
        if slot > 3 || kind > 1 {
            return Err(corrupt("a Huffman table's class or place is out of range"));
        }
        let table = Some(Table::of(counts, symbols));
        if kind == 0 {
            dc[slot] = table;
        } else {
            ac[slot] = table;
        }
        segment = &rest[16 + n..];
    }
    Ok(())
}

fn scan_of(segment: &[u8], frame: &mut Frame) -> Result<Scan> {
    let [count, rest @ ..] = segment else {
        return Err(corrupt("a scan header is short"));
    };
    let n = usize::from(*count);
    let (each, tail) = (rest.get(..2 * n), rest.get(2 * n..2 * n + 3));
    let (Some(each), Some([ss, se, a])) = (each, tail) else {
        return Err(corrupt("a scan header is short"));
    };
    let mut components = Vec::new();
    for c in each.as_chunks::<2>().0 {
        let index = (frame.components.iter())
            .position(|k| k.id == c[0])
            .ok_or_else(|| corrupt("a scan names a component the frame has not"))?;
        components.push((index, usize::from(c[1] >> 4), usize::from(c[1] & 15)));
    }
    let scan = Scan {
        components,
        ss: usize::from(*ss),
        se: usize::from(*se),
        ah: a >> 4,
        al: a & 15,
    };
    let fits = if frame.progressive {
        let dc = scan.ss == 0 && scan.se == 0;
        let ac = scan.ss >= 1 && scan.ss <= scan.se && scan.se <= 63 && n == 1;
        (dc || ac) && scan.al <= 13
    } else {
        scan.ss == 0 && scan.se == 63 && scan.ah == 0 && scan.al == 0
    };
    if !fits || n == 0 || n > 4 {
        return Err(corrupt(
            "a scan's spectral or successive bounds are out of range",
        ));
    }
    if frame.progressive && scan.ss > 0 {
        let (index, _, _) = scan.components[0];
        if frame.components[index].nonzero.is_empty() {
            let (across, down) = frame.blocks(&frame.components[index]);
            frame.components[index].nonzero = vec![0; (across * down) as usize];
        }
    }
    if scan.al == 0 {
        let band = (scan.ss..=scan.se).fold(0u64, |m, k| m | (1 << k));
        for (index, _, _) in &scan.components {
            frame.components[*index].whole |= band;
        }
    }
    Ok(scan)
}

/// The blocks of a scan, in the order it codes them: each as a component and its block
/// in that component's grid (or `None` for a block an interleaved scan pads with).
fn walk(
    frame: &mut Frame,
    scan: &Scan,
    dc: &[Option<Table>; 4],
    ac: &[Option<Table>; 4],
    interval: u32,
    bits: &mut Bits<'_>,
) -> Result<()> {
    let table = |tables: &[Option<Table>; 4], at: usize| {
        tables[at]
            .clone()
            .ok_or_else(|| corrupt("a scan uses a Huffman table never given"))
    };
    let mut coders = Vec::new();
    for (index, d, a) in &scan.components {
        let needs_dc = scan.ss == 0 && scan.ah == 0;
        let needs_ac = scan.se > 0;
        coders.push((
            *index,
            if needs_dc { Some(table(dc, *d)?) } else { None },
            if needs_ac { Some(table(ac, *a)?) } else { None },
        ));
    }
    let mut eobrun = 0u32;
    let mut units = 0u32;
    let mut unit = |bits: &mut Bits<'_>, eobrun: &mut u32| -> Result<()> {
        if interval > 0 && units > 0 && units.is_multiple_of(interval) {
            bits.restart()?;
            *eobrun = 0;
        }
        units += 1;
        Ok(())
    };
    if scan.components.len() == 1 {
        let (index, dct, act) = &coders[0];
        let (across, down) = frame.blocks(&frame.components[*index]);
        for block in 0..(across * down) as usize {
            unit(bits, &mut eobrun)?;
            let nonzero = frame.components[*index].nonzero.get_mut(block);
            one_block(
                frame.progressive,
                scan,
                dct.as_ref(),
                act.as_ref(),
                nonzero,
                &mut eobrun,
                bits,
            )?;
        }
    } else {
        let across = frame.width.div_ceil(8 * frame.hmax);
        let down = frame.height.div_ceil(8 * frame.vmax);
        for _ in 0..across * down {
            unit(bits, &mut eobrun)?;
            for (index, dct, act) in &coders {
                let c = &frame.components[*index];
                for _ in 0..c.h * c.v {
                    one_block(
                        frame.progressive,
                        scan,
                        dct.as_ref(),
                        act.as_ref(),
                        None,
                        &mut eobrun,
                        bits,
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn never_given() -> IdentityError {
    corrupt("a scan uses a Huffman table never given")
}

/// One block's codes in this scan.
fn one_block(
    progressive: bool,
    scan: &Scan,
    dc: Option<&Table>,
    ac: Option<&Table>,
    nonzero: Option<&mut u64>,
    eobrun: &mut u32,
    bits: &mut Bits<'_>,
) -> Result<()> {
    if scan.ss == 0 {
        if scan.ah == 0 {
            let s = bits.decode(dc.ok_or_else(never_given)?)?;
            if s > 16 {
                return Err(corrupt("a DC difference is too long"));
            }
            bits.bits(u32::from(s))?;
        } else {
            bits.bit()?;
        }
        if progressive {
            return Ok(());
        }
    }
    let table = ac.ok_or_else(never_given)?;
    let (mut k, se) = (scan.ss.max(1), scan.se);
    let mut spare = 0u64;
    let nonzero = nonzero.unwrap_or(&mut spare);
    if !progressive || scan.ah == 0 {
        if *eobrun > 0 {
            *eobrun -= 1;
            return Ok(());
        }
        while k <= se {
            let rs = bits.decode(table)?;
            let (r, s) = (usize::from(rs >> 4), u32::from(rs & 15));
            if s != 0 {
                k += r;
                if k > 63 {
                    return Err(corrupt("a run passes the block's end"));
                }
                bits.bits(s)?;
                *nonzero |= 1 << k;
            } else if r == 15 {
                k += 15;
            } else {
                if progressive {
                    *eobrun = (1 << r) - 1 + bits.bits(r as u32)?;
                }
                break;
            }
            k += 1;
        }
        return Ok(());
    }
    // A refinement of the coefficients coded so far, as libjpeg reads one.
    if *eobrun == 0 {
        while k <= se {
            let rs = bits.decode(table)?;
            let (mut r, s) = (i32::from(rs >> 4), rs & 15);
            if s > 1 {
                return Err(corrupt("a refinement codes more than one bit"));
            }
            if s == 0 && r != 15 {
                *eobrun = (1 << r) + bits.bits(r as u32)?;
                break;
            }
            if s == 1 {
                bits.bit()?;
            }
            while k <= se {
                if *nonzero & (1 << k) != 0 {
                    bits.bit()?;
                } else {
                    r -= 1;
                    if r < 0 {
                        break;
                    }
                }
                k += 1;
            }
            if s == 1 && k <= 63 {
                *nonzero |= 1 << k;
            }
            k += 1;
        }
    }
    if *eobrun > 0 {
        while k <= se {
            if *nonzero & (1 << k) != 0 {
                bits.bit()?;
            }
            k += 1;
        }
        *eobrun -= 1;
    }
    Ok(())
}
