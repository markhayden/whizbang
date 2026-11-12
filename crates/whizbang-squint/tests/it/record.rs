//! The tool record, `docs/reference/tools/view.md` (`SPEC.md` section 8): its examples
//! run here, and its measurements are made here, so the page cannot drift from the tool.
//!
//! - The example images beside the record are what `view` gives, byte for byte.
//! - The part of the record between the `measured` marks is what the tests measure: its
//!   first table is held by the test of `locate.rs` that measures the known boxes, its
//!   second by the test of `vectors.rs` that makes the fixed vectors' crops. Each makes
//!   its table from what it has already worked out, so nothing is computed twice.
//! - `write_the_record`, which `scripts/tool-record.sh view` runs, writes the images, that
//!   part, and the part between the `timed` marks: times, and the other methods. It
//!   leaves the times as they are when nothing else in that part changed and no time
//!   moved by more than a third (`settled`), so a run that changes nothing writes nothing.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use image::imageops::{self, FilterType};
use whizbang_core::artifact::Image;
use whizbang_core::identity::{Projection, Topology};
use whizbang_squint::view::{Crop, Request, view};

use super::locate::{KNOWN, Measured, measure};
use super::support::*;
use super::vectors::{self, VECTORS};

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 800;

/// The picture the examples look into: a soft ground with a grid, a checker of single
/// pixels, lines one pixel wide, and nested squares. The edited one has a green patch.
fn scene(edited: bool) -> impl Fn(u32, u32) -> [u8; 4] {
    move |x, y| {
        let square = |left: u32, top: u32, side: u32| {
            (left..left + side).contains(&x) && (top..top + side).contains(&y)
        };
        let from_centre = x.abs_diff(1000).max(y.abs_diff(520));
        let [r, g, b] = if edited && square(520, 420, 32) {
            [20, 200, 120]
        } else if square(160, 128, 256) {
            if (x + y) % 2 == 0 { [255; 3] } else { [0; 3] }
        } else if square(480, 128, 256) {
            if x % 4 == 0 {
                [250, 240, 60]
            } else {
                [30, 30, 90]
            }
        } else if from_centre < 200 {
            if (from_centre / 6) % 2 == 0 {
                [200, 60, 60]
            } else {
                [240, 230, 220]
            }
        } else if x % 128 == 0 || y % 128 == 0 {
            [40, 40, 60]
        } else {
            [
                (64 + x * 128 / WIDTH) as u8,
                (64 + y * 128 / HEIGHT) as u8,
                160,
            ]
        };
        [r, g, b, 255]
    }
}

/// The box of the first example: it does not fall on pixel edges, and snaps outward to
/// columns 128 to 768 and rows 96 to 480.
const DETAIL: [f64; 4] = [0.1002, 0.1203, 0.5997, 0.5996];
/// The box of the second example: the whole image, which is over the bound.
const WHOLE: [f64; 4] = [0.0, 0.0, 1.0, 1.0];

struct Examples {
    scene: Image,
    edited: Image,
    /// The first example: the same detail of the scene and of the edited scene.
    detail: Vec<Crop>,
    /// The second example: the whole scene, scaled down.
    whole: Crop,
}

fn examples() -> Examples {
    let scene = picture(WIDTH, HEIGHT, scene(false));
    let edited = picture(WIDTH, HEIGHT, self::scene(true));
    let detail = view(
        &Request {
            image: &scene,
            also: Some(&edited),
            bbox: DETAIL,
        },
        &limits(),
    )
    .unwrap();
    let whole = look(&scene, WHOLE);
    Examples {
        scene,
        edited,
        detail,
        whole,
    }
}

impl Examples {
    /// Each image beside the record: its file name and its bytes.
    fn files(&self) -> [(&'static str, &[u8]); 5] {
        [
            ("scene.png", self.scene.file()),
            ("scene-edited.png", self.edited.file()),
            ("detail.png", self.detail[0].image.file()),
            ("detail-edited.png", self.detail[1].image.file()),
            ("whole.png", self.whole.image.file()),
        ]
    }
}

fn tools() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/reference/tools")
}

/// The text between a pair of marks in the record.
fn between<'a>(record: &'a str, name: &str) -> &'a str {
    let (begin, end) = marks(name);
    let from = record.find(&begin).expect("the opening mark") + begin.len();
    let to = record.find(&end).expect("the closing mark");
    &record[from..to]
}

fn marks(name: &str) -> (String, String) {
    (
        format!("<!-- {name}: begin -->\n"),
        format!("<!-- {name}: end -->\n"),
    )
}

/// The record with the text between a pair of marks replaced.
fn replaced(record: &str, name: &str, with: &str) -> String {
    let old = between(record, name);
    let at = old.as_ptr() as usize - record.as_ptr() as usize;
    format!("{}{with}{}", &record[..at], &record[at + old.len()..])
}

/// A distance in pixels, as the record prints it.
fn pixels(distance: f64) -> String {
    if distance == 0.0 {
        "0".to_owned()
    } else if distance < 0.001 {
        format!("{distance:.0e}")
    } else {
        format!("{distance:.3}")
    }
}

/// The record's first table of measurements: one row for each known box, from what
/// `locate::measure` found. The same on every run and every machine.
pub fn mapping_table(measured: &[Measured]) -> String {
    let mut out = String::from(
        "| Image | Box asked for | Cut, in pixels | Shown at | Source pixels to a crop pixel \
         | Cut outside the box, at most | Corners | Points inside | Marked pixel | In crop pixels |\n\
         |---|---|---|---|---|---|---|---|---|---|\n",
    );
    for (known, m) in KNOWN.iter().zip(measured) {
        out.push_str(&format!(
            "| {}x{} | {:?} | {:?} | {}x{} | {:.3} | {} | {} | {} | {} | {} |\n",
            known.size.0,
            known.size.1,
            known.bbox,
            m.cut,
            m.shown.0,
            m.shown.1,
            m.ratio,
            pixels(m.snapped),
            pixels(m.corners),
            pixels(m.points),
            pixels(m.mark),
            pixels(m.mark_in_crop),
        ));
    }
    out
}

/// The record's second table of measurements: one row for each fixed vector, from the
/// crop `view` gave for it.
pub fn vector_table(crops: &[Crop]) -> String {
    let mut out = String::from(
        "| Vector | Image | Box asked for | Cut, in pixels | Shown at | Samples | Pixel digest |\n\
         |---|---|---|---|---|---|---|\n",
    );
    for (vector, crop) in VECTORS.iter().zip(crops) {
        let space = crop.image.space();
        let same = crop.image.identity().pixels.to_string() == vector.pixels;
        out.push_str(&format!(
            "| {} | {}x{} | {:?} | {:?} | {}x{} | {} | {} |\n",
            vector.name,
            vector.size.0,
            vector.size.1,
            vector.bbox,
            cut_of(&crop.binding),
            space.width(),
            space.height(),
            u64::from(space.width()) * u64::from(space.height()) * 4,
            if same {
                "equal to the reference's"
            } else {
                "DIFFERS"
            },
        ));
    }
    out
}

/// The part of the record between the `measured` marks: the two tables.
fn measured(tables: [&str; 2]) -> String {
    format!("\n{}\n{}\n", tables[0], tables[1])
}

/// Fails unless the committed record's table of that number, among its measurements, is
/// this one: 0 for the known boxes, 1 for the fixed vectors.
pub fn holds(number: usize, table: &str) {
    assert!(!table.contains("DIFFERS"), "{table}");
    let record = std::fs::read_to_string(tools().join("view.md")).unwrap();
    let part = between(&record, "measured");
    let tables: Vec<&str> = part.trim_matches('\n').split("\n\n").collect();
    assert!(
        tables.len() == 2 && part == format!("\n{}\n\n{}\n\n", tables[0], tables[1]),
        "the record's measurements are two tables: run scripts/tool-record.sh view"
    );
    assert!(
        table.strip_suffix('\n') == Some(tables[number]),
        "the record's measurements are stale: run scripts/tool-record.sh view\n{table}"
    );
}

#[test]
fn the_records_examples_are_what_view_gives() {
    let examples = examples();
    for (name, bytes) in examples.files() {
        let path = tools().join("view").join(name);
        let committed = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(
            committed == bytes,
            "{name} beside the record is not what view gives: run scripts/tool-record.sh view"
        );
    }

    // What the record says of the first example. The box snapped outward, and the crop
    // is the source pixel for pixel: the checker's single pixels are whole.
    let detail = &examples.detail[0];
    assert_eq!(cut_of(&detail.binding), [128.0, 96.0, 768.0, 480.0]);
    assert_eq!(
        (detail.image.space().width(), detail.image.space().height()),
        (640, 384)
    );
    let source = rgba_of(&examples.scene);
    let shown = rgba_of(&detail.image);
    for y in 0..384usize {
        let from = ((y + 96) * WIDTH as usize + 128) * 4;
        assert_eq!(
            shown[y * 640 * 4..(y + 1) * 640 * 4],
            source[from..from + 640 * 4]
        );
    }
    // The second crop is the same region of the edited scene, and differs from the first
    // inside the patch only: columns 392 to 424 and rows 324 to 356 of the crop.
    let other = rgba_of(&examples.detail[1].image);
    assert_eq!(cut_of(&examples.detail[1].binding), cut_of(&detail.binding));
    for (at, (a, b)) in shown
        .as_chunks::<4>()
        .0
        .iter()
        .zip(other.as_chunks::<4>().0.iter())
        .enumerate()
    {
        let (x, y) = (at % 640, at / 640);
        let patch = (392..424).contains(&x) && (324..356).contains(&y);
        assert_eq!(a != b, patch, "crop pixel ({x}, {y})");
    }

    // What the record says of the second example: 1280 by 800 is shown at 1024 by 640,
    // and the checker of single pixels, black and white in the source, is neither in the
    // crop: every pixel well inside it is a mix.
    let whole = &examples.whole;
    assert_eq!(
        (whole.image.space().width(), whole.image.space().height()),
        (1024, 640)
    );
    let shown = rgba_of(&whole.image);
    for y in 110..300usize {
        for x in 135..325usize {
            let red = shown[(y * 1024 + x) * 4];
            assert!(red > 0 && red < 255, "crop pixel ({x}, {y}) is {red}");
        }
    }
}

// --- Times: kept until one moves ---------------------------------------------------------

/// Around each time in the text `timed` builds, so that the times can be told from the
/// rest of it. It is taken out before anything is written.
const MARK: char = '\u{1}';
/// A time is written again only when it moved by more than this share of the time the
/// record holds. Other work on the machine moves a time by less than that from run to
/// run, and the record's times show proportions, not a benchmark.
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
    let committed = "\nOn a machine.\n\n| 1024x768 | 0.8 | 12 | 3.0 |\n| x | 12.8% | 33 | 90 |\n\n";
    let fresh = |a: &str, b: &str, c: &str, text: &str| {
        format!(
            "\nOn a machine.\n\n| 1024x768 | 0.8 | {MARK}{a}{MARK} | {MARK}{b}{MARK} |\n\
             | x | {text} | 33 | {MARK}{c}{MARK} |\n\n"
        )
    };
    // The same times, and times that moved by a third at most: the committed part stays.
    assert_eq!(
        settled(committed, &fresh("12", "3.0", "90", "12.8%")),
        Some(committed)
    );
    assert_eq!(
        settled(committed, &fresh("16", "2.0", "61", "12.8%")),
        Some(committed)
    );
    // One time moved further, up or down.
    assert_eq!(settled(committed, &fresh("17", "3.0", "90", "12.8%")), None);
    assert_eq!(settled(committed, &fresh("12", "1.9", "90", "12.8%")), None);
    // Something that is not a time changed: a measured difference, the machine, a row.
    assert_eq!(settled(committed, &fresh("12", "3.0", "90", "12.9%")), None);
    let elsewhere = fresh("12", "3.0", "90", "12.8%").replace("a machine", "another");
    assert_eq!(settled(committed, &elsewhere), None);
    let longer = format!("{}| y |\n", fresh("12", "3.0", "90", "12.8%"));
    assert_eq!(settled(committed, &longer), None);
    let shorter = fresh("12", "3.0", "90", "12.8%");
    assert_eq!(settled(committed, &shorter[..shorter.len() - 1]), None);
    // A number that is not marked as a time is text: 33 for 34 is a change.
    assert_eq!(
        settled(
            committed,
            &fresh("12", "3.0", "90", "12.8%").replace("| 33 |", "| 34 |")
        ),
        None
    );
    // The committed record's own times are well formed: what is there now is kept
    // against itself, each of its times marked.
    assert_eq!(
        settled(
            "| 10 | 2.8 |\n",
            &format!("| {MARK}10{MARK} | {MARK}2.8{MARK} |\n")
        ),
        Some("| 10 | 2.8 |\n")
    );
}

// --- What only the script runs: times, and the other methods -----------------------------

/// The middle of `runs` timings of `work`.
fn time<T>(runs: usize, mut work: impl FnMut() -> T) -> Duration {
    let mut times: Vec<Duration> = (0..runs)
        .map(|_| {
            let start = Instant::now();
            std::hint::black_box(work());
            start.elapsed()
        })
        .collect();
    times.sort();
    times[runs / 2]
}

/// A time in milliseconds, between [`MARK`]s.
fn millis(time: Duration) -> String {
    let ms = time.as_secs_f64() * 1000.0;
    if ms < 10.0 {
        format!("{MARK}{ms:.1}{MARK}")
    } else {
        format!("{MARK}{ms:.0}{MARK}")
    }
}

/// The whole of an image scaled to the size `view` shows it at, by a library's method,
/// and made into an image artifact as `view` makes one, with the same encoder settings:
/// the same job by another method. Returns the pixels.
fn by_library(image: &Image, size: (u32, u32), method: &str) -> Vec<u8> {
    use image::codecs::png::{CompressionType, FilterType as Filter, PngEncoder};
    use image::{ExtendedColorType, ImageEncoder};
    let pixels = image.pixels(&limits()).unwrap();
    let source =
        image::RgbaImage::from_raw(pixels.width(), pixels.height(), pixels.rgba().to_vec())
            .unwrap();
    let scaled = match method {
        "triangle" => imageops::resize(&source, size.0, size.1, FilterType::Triangle),
        "thumbnail" => imageops::thumbnail(&source, size.0, size.1),
        other => panic!("no method {other}"),
    };
    let mut file = Vec::new();
    PngEncoder::new_with_quality(&mut file, CompressionType::Default, Filter::Adaptive)
        .write_image(scaled.as_raw(), size.0, size.1, ExtendedColorType::Rgba8)
        .unwrap();
    let made = image_in(file, Projection::Flat, Topology::Bounded);
    rgba_of(&made)
}

/// The scene, repeated to fill a larger picture.
fn tiled(width: u32, height: u32) -> Image {
    let scene = scene(false);
    picture(width, height, |x, y| scene(x % WIDTH, y % HEIGHT))
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
         | Image | Megapixels | File | `view` of the whole image | Of which reading the image \
         | `view` of a box 256 pixels square |\n|---|---|---|---|---|---|\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
    for (width, height) in [(1024u32, 768u32), (4096, 3072), (8000, 6000)] {
        let image = tiled(width, height);
        let whole = time(5, || look(&image, WHOLE));
        let reading = time(5, || image.pixels(&limits()).unwrap());
        let side = [256.0 / f64::from(width), 256.0 / f64::from(height)];
        let small = time(5, || {
            look(&image, [0.25, 0.25, 0.25 + side[0], 0.25 + side[1]])
        });
        out.push_str(&format!(
            "| {width}x{height} | {:.1} | {:.1} MB | {} | {} | {} |\n",
            f64::from(width) * f64::from(height) / 1e6,
            image.file().len() as f64 / 1e6,
            millis(whole),
            millis(reading),
            millis(small),
        ));
    }

    out.push_str(
        "\n| Picture | Method | Samples that differ from the exact mean | Largest difference, \
         in levels | Mean difference | The whole call |\n|---|---|---|---|---|---|\n",
    );
    let noise = picture(1500, 1100, noisy(13, true));
    let large = tiled(4096, 3072);
    let pictures = [
        ("The scene of the examples, 1280x800", &examples().scene),
        ("Noise with alpha, 1500x1100", &noise),
        ("The scene repeated, 4096x3072", &large),
    ];
    for (name, image) in pictures {
        let crop = look(image, WHOLE);
        let size = (crop.image.space().width(), crop.image.space().height());
        let exact = rgba_of(&crop.image);
        out.push_str(&format!(
            "| {name}, shown at {}x{} | The area mean in whole numbers (the method) | 0% | 0 | 0 \
             | {} |\n",
            size.0,
            size.1,
            millis(time(5, || look(image, WHOLE))),
        ));
        for (method, label) in [
            (
                "triangle",
                "`resize` with the triangle filter, as in squint",
            ),
            ("thumbnail", "`thumbnail`"),
        ] {
            let differences: Vec<u8> = by_library(image, size, method)
                .iter()
                .zip(&exact)
                .map(|(a, b)| a.abs_diff(*b))
                .collect();
            out.push_str(&format!(
                "| | The `image` crate's {label} | {:.1}% | {} | {:.2} | {} |\n",
                100.0 * differences.iter().filter(|d| **d > 0).count() as f64
                    / differences.len() as f64,
                differences.iter().max().unwrap(),
                differences.iter().map(|d| f64::from(*d)).sum::<f64>() / differences.len() as f64,
                millis(time(5, || by_library(image, size, method))),
            ));
        }
    }
    out.push('\n');
    out
}

/// Writes the example images and the record's two generated parts again. The times
/// stay as they are when `settled` says so.
#[test]
#[ignore = "rewrites the committed record; run it with scripts/tool-record.sh view"]
fn write_the_record() {
    let folder = tools().join("view");
    std::fs::create_dir_all(&folder).unwrap();
    for (name, bytes) in examples().files() {
        std::fs::write(folder.join(name), bytes).unwrap();
    }
    let path = tools().join("view.md");
    let record = std::fs::read_to_string(&path).unwrap();
    let known: Vec<Measured> = KNOWN.iter().map(measure).collect();
    let tables = [mapping_table(&known), vector_table(&vectors::crops())];
    let record = replaced(&record, "measured", &measured([&tables[0], &tables[1]]));
    let fresh = timed();
    let times = match settled(between(&record, "timed"), &fresh) {
        Some(kept) => kept.to_owned(),
        None => fresh.replace(MARK, ""),
    };
    let record = replaced(&record, "timed", &times);
    std::fs::write(path, record).unwrap();
}
