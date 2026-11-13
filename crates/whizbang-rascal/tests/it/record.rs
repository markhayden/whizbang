//! The tool record, `docs/reference/tools/grow.md` (`SPEC.md` section 8): its examples
//! run here, and its measurements are made here, so the page cannot drift from the tool.
//!
//! - The example images beside the record are pictures of the masks `grow` was given and
//!   gave, byte for byte.
//! - The part of the record between the `measured` marks is what the fast tests measure:
//!   the examples' counts and the fixed vectors.
//! - `write_the_record`, which `scripts/tool-record.sh grow` runs, writes the images, that
//!   part, and the part between the `timed` marks: times, and the other methods. It
//!   leaves the times as they are when nothing else in that part changed and no time
//!   moved by more than a third (`settled`), as the record of `view` does.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use whizbang_core::artifact::{
    self, Artifact, BandProfile, Line, Location, Mask, Series, Shape, Source, UseCase, Windows,
};
use whizbang_core::contract::{Engine, Executor, NoBackends, Registry};
use whizbang_core::identity::{DecodeLimits, Digest};
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_rascal::grow::{Grow, grow, grown, radius_squared, rows_down};
use whizbang_testkit::scratch::Scratch;

use super::grow::{VECTORS, made_by, mask_of, on, plane, vector_mask, vector_result};
use super::grow_action::call;

fn never() -> bool {
    false
}

// --- pictures of masks ------------------------------------------------------------------------

/// The colours of a picture of a mask: nothing, protected, core, and thirteen weights
/// of blend from faint to nearly whole.
fn palette() -> Vec<u8> {
    let mut colours = vec![30, 36, 48, 217, 72, 59, 255, 255, 255];
    for step in 0..13u32 {
        let mix = |from: u32, to: u32| (from + (to - from) * step / 12) as u8;
        colours.extend([mix(52, 200), mix(72, 215), mix(110, 240)]);
    }
    colours
}

/// Which colour a pixel of a mask is shown in.
fn shade(core: u8, blend: u8, protect: u8) -> u8 {
    match (protect, core, blend) {
        (0, 0, 0) => 0,
        (0, 0, blend) => 3 + ((u32::from(blend) - 1) * 13 / 255) as u8,
        (0, _, _) => 2,
        _ => 1,
    }
}

fn crc(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

/// A picture of a mask, each pixel shown three wide and three high: a PNG of sixteen
/// colours, written here byte by byte and not compressed, so that the same mask is the
/// same file wherever it is written. It is a picture for a page, and no artifact.
fn picture(mask: &Mask) -> Vec<u8> {
    const SCALE: usize = 3;
    let space = mask.space();
    let (width, height) = (space.width() as usize, space.height() as usize);
    let mut rows = Vec::new();
    for y in 0..height {
        let mut row = vec![0u8];
        let shades: Vec<u8> = (0..width)
            .flat_map(|x| {
                let at = y * width + x;
                [shade(mask.core()[at], mask.blend()[at], mask.protect()[at]); SCALE]
            })
            .collect();
        row.extend(
            shades
                .chunks(2)
                .map(|pair| pair[0] << 4 | pair.get(1).copied().unwrap_or(0)),
        );
        for _ in 0..SCALE {
            rows.extend(&row);
        }
    }
    // A zlib stream of stored blocks, and its checksum.
    let mut packed = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = rows.chunks(65_535).collect();
    for (at, block) in blocks.iter().enumerate() {
        packed.push(u8::from(at + 1 == blocks.len()));
        packed.extend((block.len() as u16).to_le_bytes());
        packed.extend((!(block.len() as u16)).to_le_bytes());
        packed.extend(*block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for byte in &rows {
        a = (a + u32::from(*byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    packed.extend((b << 16 | a).to_be_bytes());

    let mut header = Vec::new();
    header.extend(((width * SCALE) as u32).to_be_bytes());
    header.extend(((height * SCALE) as u32).to_be_bytes());
    header.extend([4, 3, 0, 0, 0]);
    let mut file = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    for (kind, data) in [
        (b"IHDR", header),
        (b"PLTE", palette()),
        (b"IDAT", packed),
        (b"IEND", Vec::new()),
    ] {
        file.extend((data.len() as u32).to_be_bytes());
        let body = [&kind[..], &data].concat();
        file.extend(&body);
        file.extend(crc(&body).to_be_bytes());
    }
    file
}

// --- the examples -----------------------------------------------------------------------------

const WIDTH: usize = 96;
const HEIGHT: usize = 64;

/// The first example's mask: a blob, as a segmenter finds one, and a thin protected bar
/// beside it.
fn blob() -> Mask {
    let core = plane(WIDTH, HEIGHT, |x, y| {
        let (dx, dy) = (x as i64 - 40, y as i64 - 22);
        dx * dx * 100 + dy * dy * 256 <= 25_600
    });
    let bar = plane(WIDTH, HEIGHT, |x, y| {
        (59..61).contains(&x) && (8..56).contains(&y)
    });
    mask_of(WIDTH, HEIGHT, core, &bar)
}

/// The second example's mask: a band along column 48 with a feathered edge, a profile
/// and a tone ramp, and a protected patch beside it.
fn band() -> Mask {
    let binding = on(WIDTH as u32, HEIGHT as u32);
    let line = Location::new(
        binding.clone(),
        Shape::Line {
            line: Line::Column(48.0),
            width: 4.0,
        },
        made_by("line"),
    )
    .unwrap();
    let mut mask = Mask::from_core(
        binding.clone(),
        UseCase::Seam,
        plane(WIDTH, HEIGHT, |x, _| (46..50).contains(&x)),
        Source {
            locations: vec![line.id()],
            instances: Vec::new(),
        },
        made_by("profile"),
    )
    .unwrap();
    // Six columns of feather on each side, fading outward.
    let blend = (0..WIDTH * HEIGHT).map(|at| {
        let x = at % WIDTH;
        let out = if x < 46 {
            46 - x
        } else {
            (x + 1).saturating_sub(50)
        };
        if (1..=6).contains(&out) {
            (260 - 40 * out) as u8
        } else {
            0
        }
    });
    mask.set_blend(blend.collect()).unwrap();
    mask.protect_union(&plane(WIDTH, HEIGHT, |x, y| {
        (51..58).contains(&x) && (24..34).contains(&y)
    }))
    .unwrap();
    let series = |name: &str, unit: &str, channels: u32, action: &str| {
        let values = vec![2.0; channels as usize * HEIGHT];
        Series::new(
            binding.clone(),
            name,
            unit,
            channels,
            values,
            made_by(action),
        )
        .unwrap()
    };
    let windows = Windows {
        gap: 1,
        step_columns: 3,
        texture_columns: 8,
    };
    let band = BandProfile::new(
        binding.clone(),
        line.id(),
        48,
        2.0,
        vec![2.0; HEIGHT],
        windows,
        "fan-v1",
        made_by("fan"),
    )
    .unwrap();
    mask.set_profile(band, series("sigma_px", "px", 1, "feather"))
        .unwrap();
    mask.push_tone_ramp(series("tone_ramp.step", "levels", 3, "ramp"), 0.5)
        .unwrap();
    mask
}

struct Example {
    name: &'static str,
    px: f64,
    down_px: Option<f64>,
    before: Mask,
    after: Mask,
}

fn examples() -> [Example; 2] {
    let example = |name, before: Mask, px, down_px| Example {
        name,
        px,
        down_px,
        after: grow(before.clone(), px, down_px, &never)
            .unwrap()
            .unwrap()
            .mask,
        before,
    };
    [
        example("blob", blob(), 6.5, Some(12.0)),
        example("band", band(), 3.0, None),
    ]
}

/// Each image beside the record: its file name and its bytes.
fn files(examples: &[Example]) -> Vec<(String, Vec<u8>)> {
    let pictures = examples.iter().flat_map(|example| {
        [
            (format!("{}.png", example.name), picture(&example.before)),
            (
                format!("{}-grown.png", example.name),
                picture(&example.after),
            ),
        ]
    });
    pictures.collect()
}

fn tools() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/reference/tools")
}

fn marks(name: &str) -> (String, String) {
    (
        format!("<!-- {name}: begin -->\n"),
        format!("<!-- {name}: end -->\n"),
    )
}

/// The text between a pair of marks in the record.
fn between<'a>(record: &'a str, name: &str) -> &'a str {
    let (begin, end) = marks(name);
    let from = record.find(&begin).expect("the opening mark") + begin.len();
    let to = record.find(&end).expect("the closing mark");
    &record[from..to]
}

/// The record with the text between a pair of marks replaced.
fn replaced(record: &str, name: &str, with: &str) -> String {
    let old = between(record, name);
    let at = old.as_ptr() as usize - record.as_ptr() as usize;
    format!("{}{with}{}", &record[..at], &record[at + old.len()..])
}

fn count(plane: &[u8]) -> usize {
    plane.iter().filter(|sample| **sample != 0).count()
}

/// The part of the record between the `measured` marks: the examples in numbers, and
/// the fixed vectors. The same on every run and every machine.
fn measured() -> String {
    let mut out = String::from(
        "\n| Example | `px` | K | `down_px` | Rows down | Core before | Core after | Blend before \
         | Blend after | Protected before and after | Profile and prescriptions after |\n\
         |---|---|---|---|---|---|---|---|---|---|---|\n",
    );
    for example in examples() {
        let (before, after) = (&example.before, &example.after);
        assert_eq!(before.protect(), after.protect());
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            example.name,
            example.px,
            radius_squared(example.px),
            example
                .down_px
                .map_or("none".to_owned(), |down| down.to_string()),
            rows_down(example.down_px.unwrap_or(0.0)),
            count(before.core()),
            count(after.core()),
            count(before.blend()),
            count(after.blend()),
            count(after.protect()),
            if after.profile().is_none() && after.prescriptions().is_empty() {
                "none"
            } else {
                "KEPT"
            },
        ));
    }
    out.push_str(
        "\n| Mask | Size | `px` | K | `down_px` | Rows down | Core pixels after | Core plane |\n\
         |---|---|---|---|---|---|---|---|\n",
    );
    for vector in &VECTORS {
        let (name, width, height, px, down_px, pixels, digest) = *vector;
        let same = vector_result(vector) == (pixels, digest.to_owned());
        out.push_str(&format!(
            "| {name} | {width}x{height} | {px} | {} | {down_px} | {} | {pixels} | {} |\n",
            radius_squared(px),
            rows_down(down_px),
            if same {
                "equal to the reference's"
            } else {
                "DIFFERS"
            },
        ));
    }
    out.push('\n');
    out
}

#[test]
fn the_records_examples_are_what_grow_gives() {
    let examples = examples();
    for (name, bytes) in files(&examples) {
        let path = tools().join("grow").join(&name);
        let committed = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(
            committed == bytes,
            "{name} beside the record is not a picture of what grow gives: run \
             scripts/tool-record.sh grow"
        );
    }
    let at = |mask: &Mask, x: usize, y: usize| {
        let at = y * WIDTH + x;
        (mask.core()[at], mask.blend()[at], mask.protect()[at])
    };

    // What the record says of the first example. The blob spans columns 24 to 56 and
    // rows 12 to 32. By 6.5 pixels K is 42, so it grows 6 whole pixels each way: rows 6
    // to 38 through its middle, and 12 more rows down, to row 50.
    let [blob, band] = &examples;
    let column: Vec<usize> = (0..HEIGHT)
        .filter(|y| at(&blob.after, 40, *y).0 != 0)
        .collect();
    assert_eq!(column, (6..=50).collect::<Vec<_>>());
    let row = |mask: &Mask, y: usize| -> Vec<usize> {
        (0..WIDTH).filter(|x| at(mask, *x, y).0 != 0).collect()
    };
    assert_eq!(row(&blob.before, 22), (24..=56).collect::<Vec<_>>());
    // In the blob's middle row the core reaches column 62, on both sides of the bar,
    // and is not set in the bar's two columns.
    let grown_row: Vec<usize> = (18..=62).filter(|x| *x != 59 && *x != 60).collect();
    assert_eq!(row(&blob.after, 22), grown_row);
    assert_eq!(at(&blob.after, 59, 22), (0, 0, 255));
    assert_eq!(blob.after.protect(), blob.before.protect());
    // A corner is round: at 6 columns beyond the blob's widest row, 2 rows up or down are
    // within the radius (36 + 4 is at most 42) and 3 are not (36 + 9).
    assert_eq!(at(&blob.after, 18, 20).0, 255);
    assert_eq!(at(&blob.after, 18, 19).0, 0);

    // What the record says of the second. The band's core was columns 46 to 49, with a
    // feather of six columns on each side. Grown by 3 it is columns 43 to 52, hard at
    // both edges: the feather is gone, and so are the profile and the tone ramp.
    assert_eq!(row(&band.before, 10), (46..=49).collect::<Vec<_>>());
    assert_eq!(at(&band.before, 45, 10), (0, 220, 0));
    assert_eq!(at(&band.before, 40, 10), (0, 20, 0));
    assert_eq!(row(&band.after, 10), (43..=52).collect::<Vec<_>>());
    assert!(band.after.blend().iter().all(|sample| *sample == 0));
    assert_eq!(band.before.weight(42, 10), Some(100.0 / 255.0));
    assert_eq!(band.after.weight(42, 10), Some(0.0));
    assert_eq!(band.after.weight(43, 10), Some(1.0));
    assert!(band.before.profile().is_some() && band.before.prescriptions().len() == 1);
    assert!(band.after.profile().is_none() && band.after.prescriptions().is_empty());
    // Beside the protected patch the core stops at column 50.
    assert_eq!(row(&band.after, 28), (43..=50).collect::<Vec<_>>());
}

#[test]
fn the_records_measurements_are_what_grow_gives() {
    let fresh = measured();
    assert!(
        !fresh.contains("DIFFERS") && !fresh.contains("KEPT"),
        "{fresh}"
    );
    let record = std::fs::read_to_string(tools().join("grow.md")).unwrap();
    assert!(
        between(&record, "measured") == fresh,
        "the record's measurements are stale: run scripts/tool-record.sh grow\n{fresh}"
    );
}

#[test]
fn a_picture_of_a_mask_is_a_png_of_its_three_planes() {
    // Read back by hand: the size, sixteen colours, and one stored row for each of
    // three rows of a mask one pixel high, protected, core, a faint and a full blend.
    let mut mask = mask_of(4, 1, vec![0, 255, 0, 0], &[255, 0, 0, 0]);
    mask.set_blend(vec![0, 0, 1, 255]).unwrap();
    let file = picture(&mask);
    assert_eq!(file[..8], [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
    assert_eq!(
        file[8..29],
        [
            0, 0, 0, 13, b'I', b'H', b'D', b'R', 0, 0, 0, 12, 0, 0, 0, 3, 4, 3, 0, 0, 0
        ]
    );
    // The check value of a PNG's last chunk, which holds nothing, is known.
    assert_eq!(
        file[file.len() - 12..],
        [0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82]
    );
    let data = file.windows(4).position(|kind| kind == b"IDAT").unwrap() + 4;
    // Three rows of seven bytes: no filter, then twelve shades, two to a byte.
    assert_eq!(file[data..data + 7], [0x78, 0x01, 1, 21, 0, !21, 0xff]);
    let row = [0, 0x11, 0x12, 0x22, 0x33, 0x3f, 0xff];
    assert_eq!(file[data + 7..data + 28], [row, row, row].concat());
    assert_eq!(
        [shade(0, 0, 0), shade(0, 128, 0), shade(255, 0, 0)],
        [0, 9, 2]
    );
}

// --- Times: kept until one moves ---------------------------------------------------------------

/// Around each time in the text `timed` builds, so that the times can be told from the
/// rest of it. It is taken out before anything is written.
const MARK: char = '\u{1}';
/// A time is written again only when it moved by more than this share of the time the
/// record holds: the rule of `view`'s record, with its reasons
/// (`crates/whizbang-squint/tests/it/record.rs`).
const MARGIN: f64 = 1.0 / 3.0;

/// The part of the record between the `timed` marks as it is committed, when the fresh
/// one (its times between [`MARK`]s) differs from it in times alone, and each of those
/// by no more than [`MARGIN`] of the committed time. `None` when anything else differs,
/// or a time moved further: the fresh part is then written.
fn settled<'a>(committed: &'a str, fresh: &str) -> Option<&'a str> {
    let mut rest = committed;
    for (at, piece) in fresh.split(MARK).enumerate() {
        if at % 2 == 0 {
            rest = rest.strip_prefix(piece)?;
            continue;
        }
        let digits = rest
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(rest.len());
        let held: f64 = rest[..digits].parse().ok()?;
        let now: f64 = piece.parse().ok()?;
        if (now - held).abs() > MARGIN * held {
            return None;
        }
        rest = &rest[digits..];
    }
    rest.is_empty().then_some(committed)
}

#[test]
fn the_records_times_are_kept_until_one_moves_by_more_than_a_third() {
    let committed = "| 1024x768 | 12 | 3.0 |\n";
    let fresh = |a: &str, b: &str| format!("| 1024x768 | {MARK}{a}{MARK} | {MARK}{b}{MARK} |\n");
    assert_eq!(settled(committed, &fresh("12", "3.0")), Some(committed));
    assert_eq!(settled(committed, &fresh("16", "2.0")), Some(committed));
    assert_eq!(settled(committed, &fresh("17", "3.0")), None);
    assert_eq!(settled(committed, &fresh("12", "1.9")), None);
    assert_eq!(
        settled(committed, &fresh("12", "3.0").replace("1024", "1025")),
        None
    );
    // The committed record's own times are well formed: kept against themselves.
    let record = std::fs::read_to_string(tools().join("grow.md")).unwrap();
    let part = between(&record, "timed");
    let mut marked = String::new();
    let mut rest = part;
    while let Some(at) = rest.find("| ") {
        let (before, cell) = rest.split_at(at + 2);
        marked.push_str(before);
        let end = cell.find(" |").unwrap_or(cell.len());
        let is_time =
            !cell[..end].is_empty() && cell[..end].chars().all(|c| c.is_ascii_digit() || c == '.');
        if is_time {
            marked.push_str(&format!("{MARK}{}{MARK}", &cell[..end]));
        } else {
            marked.push_str(&cell[..end]);
        }
        rest = &cell[end..];
    }
    marked.push_str(rest);
    assert_eq!(settled(part, &marked), Some(part));
}

// --- What only the script runs: times, and the other methods -----------------------------------

/// The middle of `runs` timings of `work`, each on what `make` made outside the time.
fn time<I, T>(runs: usize, mut make: impl FnMut() -> I, mut work: impl FnMut(I) -> T) -> Duration {
    let mut times: Vec<Duration> = (0..runs)
        .map(|_| {
            let input = make();
            let start = Instant::now();
            let made = std::hint::black_box(work(input));
            let took = start.elapsed();
            drop(made);
            took
        })
        .collect();
    times.sort();
    times[runs / 2]
}

/// A time in milliseconds, between [`MARK`]s.
fn millis(time: Duration) -> String {
    let ms = time.as_secs_f64() * 1000.0;
    if ms < 0.05 {
        // Too short to time, and no number to keep or to compare.
        "under 0.1".to_owned()
    } else if ms < 10.0 {
        format!("{MARK}{ms:.1}{MARK}")
    } else {
        format!("{MARK}{ms:.0}{MARK}")
    }
}

/// A mask of any size for the times: a blob and a thin line in every square of 256
/// pixels, and a protected stripe every 512 rows.
fn tiled(width: usize, height: usize) -> Mask {
    let core = plane(width, height, |x, y| {
        let (u, v) = ((x % 256) as i64, (y % 256) as i64);
        (u - 128) * (u - 128) + (v - 100) * (v - 100) <= 48 * 48 || (u == 200 && v < 180)
    });
    let protect = plane(width, height, |_, y| (300..310).contains(&(y % 512)));
    mask_of(width, height, core, &protect)
}

/// The same job by another method: every offset of the disc stamped from every core
/// pixel. It is the rule as it is written, and costs the core's pixels times the disc's.
fn by_stamping(core: &[u8], width: usize, height: usize, px: f64) -> Vec<u8> {
    let k = radius_squared(px) as i64;
    let reach = k.isqrt();
    let offsets: Vec<(i64, i64)> = (-reach..=reach)
        .flat_map(|dy| (-reach..=reach).map(move |dx| (dx, dy)))
        .filter(|(dx, dy)| dx * dx + dy * dy <= k)
        .collect();
    let mut out = vec![0u8; core.len()];
    for at in (0..core.len()).filter(|at| core[*at] != 0) {
        let (x, y) = ((at % width) as i64, (at / width) as i64);
        for (dx, dy) in &offsets {
            let (x, y) = (x + dx, y + dy);
            if x >= 0 && y >= 0 && x < width as i64 && y < height as i64 {
                out[y as usize * width + x as usize] = 255;
            }
        }
    }
    out
}

/// One row or column of rascal's distance transform: the lower envelope of parabolas.
fn envelope(f: &[f64], out: &mut [f64]) {
    let n = f.len();
    let (mut v, mut z) = (vec![0usize; n], vec![0.0f64; n + 1]);
    let mut k = 0usize;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    let meet = |q: usize, p: usize| -> f64 {
        if f[q].is_infinite() {
            return f64::INFINITY;
        }
        if f[p].is_infinite() {
            return f64::NEG_INFINITY;
        }
        ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * q as f64 - 2.0 * p as f64)
    };
    for q in 1..n {
        if f[q].is_infinite() {
            continue;
        }
        let mut s = meet(q, v[k]);
        while k > 0 && s <= z[k] {
            k -= 1;
            s = meet(q, v[k]);
        }
        if f[v[k]].is_infinite() && k == 0 {
            v[0] = q;
            continue;
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    if f[v[0]].is_infinite() {
        out.fill(f64::INFINITY);
        return;
    }
    let mut k = 0usize;
    for (q, slot) in out.iter_mut().enumerate() {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let d = q as f64 - v[k] as f64;
        *slot = d * d + f[v[k]];
    }
}

/// The same job as rascal did it (`crates/rascal-ops/src/primitives.rs`, `dilate`, at
/// the commit `docs/sources.toml` pins): the exact Euclidean distance transform in
/// doubles, by Felzenszwalb and Huttenlocher's two passes, its distance rounded to a
/// single-precision float and squared again, and compared with the radius squared as
/// doubles multiply it, plus 0.000000001.
fn by_distances(core: &[u8], width: usize, height: usize, px: f64) -> Vec<u8> {
    let mut f: Vec<f64> = core
        .iter()
        .map(|sample| if *sample != 0 { 0.0 } else { f64::INFINITY })
        .collect();
    let (mut line, mut out) = (vec![0.0; height], vec![0.0; height]);
    for x in 0..width {
        for y in 0..height {
            line[y] = f[y * width + x];
        }
        envelope(&line, &mut out);
        for y in 0..height {
            f[y * width + x] = out[y];
        }
    }
    let (mut line, mut out) = (vec![0.0; width], vec![0.0; width]);
    for y in 0..height {
        line.copy_from_slice(&f[y * width..(y + 1) * width]);
        envelope(&line, &mut out);
        f[y * width..(y + 1) * width].copy_from_slice(&out);
    }
    let r2 = px.max(0.0).powi(2);
    let within = |squared: f64| {
        let distance = squared.sqrt() as f32;
        distance.is_finite() && f64::from(distance).powi(2).round() <= r2 + 1e-9
    };
    f.into_iter()
        .map(|d| if within(d) { 255 } else { 0 })
        .collect()
}

#[test]
fn the_other_methods_do_the_same_job_where_the_record_says_they_do() {
    // Small masks, so that the comparison the script makes at size is also held here:
    // stamping is the rule; rascal's distances are the rule except where the radius's
    // square as doubles, with its slack, lets in a whole number the exact square is under.
    let vector = &VECTORS[4];
    let (_, width, height, px, ..) = *vector;
    assert_eq!(px, 3.605_551_275_463_989);
    let core = vector_mask(vector).core().to_vec();
    let rule = grown(&core, width, height, px, 0.0, &never).unwrap();
    assert_eq!(by_stamping(&core, width, height, px), rule);
    let old = by_distances(&core, width, height, px);
    let differ = old.iter().zip(&rule).filter(|(a, b)| a != b).count();
    // Counted apart from the code too: the pixels at a squared distance of exactly 13.
    assert_eq!(differ, 434);
    for px in [0.0, 1.0, 2.5, 7.3, 40.0] {
        let rule = grown(&core, width, height, px, 0.0, &never).unwrap();
        assert_eq!(by_stamping(&core, width, height, px), rule, "px {px}");
        assert_eq!(by_distances(&core, width, height, px), rule, "px {px}");
    }
}

fn megapixels(width: usize, height: usize) -> String {
    format!("{:.1}", (width * height) as f64 / 1e6)
}

fn timed() -> String {
    let machine = std::process::Command::new("sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_else(|| "an unnamed processor".to_owned());
    let mut out = format!(
        "\nMeasured on {machine} ({}, {}), in a release build; each time is the middle one \
         of five runs, in milliseconds.\n\n\
         | Mask | Megapixels | File | `grow`, `px` 8 and `down_px` 4 | `grow`, `px` 1000 \
         | `grow`, `px` 10000 | Reading the mask from its file | A whole run through the executor \
         |\n|---|---|---|---|---|---|---|---|\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
    let limits = DecodeLimits::default();
    for (width, height) in [(1024usize, 768usize), (4096, 3072), (8000, 6000)] {
        let mask = tiled(width, height);
        let file = mask.to_bytes();
        let near = time(5, || mask.clone(), |m| grow(m, 8.0, Some(4.0), &never));
        let far = time(5, || mask.clone(), |m| grow(m, 1000.0, None, &never));
        let furthest = time(5, || mask.clone(), |m| grow(m, 10_000.0, None, &never));
        let reading = time(5, || (), |()| Mask::from_bytes(&file, &limits).unwrap());
        // A run: the mask is in a store on this machine's disk, and the call is
        // prepared, admitted, run and committed. Each run is in a store of its own.
        let run = time(
            5,
            || {
                let dir = Scratch::new("grow-record");
                let store =
                    Arc::new(Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap());
                let mut publication = store.begin().unwrap();
                let id = artifact::put(&mut publication, &Artifact::from(mask.clone())).unwrap();
                publication.publish().unwrap();
                let mut registry = Registry::new();
                registry.register(Arc::new(Grow::new())).unwrap();
                let none = Digest::of_bytes(b"");
                (
                    dir,
                    Engine::over(registry, Arc::new(NoBackends), store, none),
                    id,
                )
            },
            |(dir, engine, id)| {
                let finished = engine.submit(&call(id, 8.0, Some(4.0))).unwrap();
                assert_eq!(finished.outcome().name(), "completed");
                dir
            },
        );
        out.push_str(&format!(
            "| {width}x{height} | {} | {:.2} MB | {} | {} | {} | {} | {} |\n",
            megapixels(width, height),
            file.len() as f64 / 1e6,
            millis(near),
            millis(far),
            millis(furthest),
            millis(reading),
            millis(run),
        ));
    }

    out.push_str(
        "\n| Mask | `px` | Method | Pixels that differ from the rule | Time |\n|---|---|---|---|---|\n",
    );
    let dots = vector_mask(&VECTORS[4]);
    let small = tiled(1024, 768);
    let large = tiled(4096, 3072);
    for (name, mask, px, stamp) in [
        (
            "The vectors' dots, 97x61",
            &dots,
            3.605_551_275_463_989,
            true,
        ),
        ("The mask of the times, 1024x768", &small, 8.0, true),
        ("The mask of the times, 1024x768", &small, 100.0, true),
        ("The mask of the times, 4096x3072", &large, 8.0, true),
        ("The mask of the times, 4096x3072", &large, 1000.0, false),
    ] {
        let space = mask.space();
        let (width, height) = (space.width() as usize, space.height() as usize);
        let core = mask.core();
        let rule = grown(core, width, height, px, 0.0, &never).unwrap();
        let differ = |other: &[u8]| other.iter().zip(&rule).filter(|(a, b)| a != b).count();
        out.push_str(&format!(
            "| {name} | {px} | Two sweeps in whole numbers (the method) | 0 | {} |\n",
            millis(time(
                5,
                || (),
                |()| grown(core, width, height, px, 0.0, &never)
            )),
        ));
        out.push_str(&format!(
            "| | | The distance transform in doubles, as in rascal | {} | {} |\n",
            differ(&by_distances(core, width, height, px)),
            millis(time(5, || (), |()| by_distances(core, width, height, px))),
        ));
        if stamp {
            out.push_str(&format!(
                "| | | The disc stamped from every core pixel | {} | {} |\n",
                differ(&by_stamping(core, width, height, px)),
                millis(time(3, || (), |()| by_stamping(core, width, height, px))),
            ));
        } else {
            out.push_str("| | | The disc stamped from every core pixel | not run | not run |\n");
        }
    }
    out.push('\n');
    out
}

/// Writes the example images and the record's two generated parts again. The times
/// stay as they are when `settled` says so.
#[test]
#[ignore = "rewrites the committed record; run it with scripts/tool-record.sh grow"]
fn write_the_record() {
    let folder = tools().join("grow");
    std::fs::create_dir_all(&folder).unwrap();
    for (name, bytes) in files(&examples()) {
        std::fs::write(folder.join(name), bytes).unwrap();
    }
    let path = tools().join("grow.md");
    let record = std::fs::read_to_string(&path).unwrap();
    let record = replaced(&record, "measured", &measured());
    let fresh = timed();
    let times = match settled(between(&record, "timed"), &fresh) {
        Some(kept) => kept.to_owned(),
        None => fresh.replace(MARK, ""),
    };
    let record = replaced(&record, "timed", &times);
    std::fs::write(path, record).unwrap();
}
