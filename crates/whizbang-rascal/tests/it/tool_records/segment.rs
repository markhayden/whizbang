//! The record of `segment`, `docs/reference/tools/segment.md`. Its examples are SAM 3's
//! answers, which no fast test can ask for: the model tier asks them
//! (`crates/whizbang/tests/it/model_tier.rs`,
//! `segment_answers_the_records_examples_on_the_mac`) and writes each mask's core as
//! runs into cargo's scratch folder for tests; `scripts/tool-record.sh segment` reads
//! that file and writes the pictures and the tables. The fast test reads each committed
//! picture back to its core and holds the table's pixels and overlaps to it, and the
//! scene to the drawing.

use whizbang_core::artifact::Mask;
use whizbang_core::identity::CanonicalPixels;

use super::super::grow::mask_of;
use super::super::record::{between, picture, tools};
use super::{Page, image_picture};

const W: usize = 192;
const H: usize = 128;

/// The scene, drawn as the model tier draws it (`segment_scene` there): a grey ground,
/// two red discs and a blue square.
pub fn scene() -> CanonicalPixels {
    let rgba = (0..W * H)
        .flat_map(|at| {
            let shape = shape_at(at % W, at / W);
            match shape {
                Some(0 | 1) => [200, 40, 40, 255],
                Some(_) => [40, 70, 190, 255],
                None => [128, 128, 128, 255],
            }
        })
        .collect();
    CanonicalPixels::from_rgba(W as u32, H as u32, rgba).unwrap()
}

/// Which shape drawn holds the pixel: 0 the large disc, 1 the small one, 2 the square.
fn shape_at(x: usize, y: usize) -> Option<usize> {
    let (x, y) = (x as i64, y as i64);
    let disc = |cx: i64, cy: i64, r: i64| (x - cx).pow(2) + (y - cy).pow(2) <= r * r;
    if disc(56, 64, 24) {
        Some(0)
    } else if disc(150, 32, 14) {
        Some(1)
    } else if (104..152).contains(&x) && (72..112).contains(&y) {
        Some(2)
    } else {
        None
    }
}

const SHAPES: [&str; 3] = ["the large disc", "the small disc", "the square"];

/// The shape a core overlaps best, and the overlap: the pixels in both over the pixels in
/// either, to three places.
fn best_match(core: &[u8]) -> (&'static str, String) {
    let best = (0..3)
        .map(|shape| {
            let (mut both, mut either) = (0usize, 0usize);
            for (at, sample) in core.iter().enumerate() {
                let drawn = shape_at(at % W, at / W) == Some(shape);
                both += usize::from(drawn && *sample != 0);
                either += usize::from(drawn || *sample != 0);
            }
            (both as f64 / either as f64, shape)
        })
        .fold((-1.0, 0), |a, b| if b.0 > a.0 { b } else { a });
    (SHAPES[best.1], format!("{:.3}", best.0))
}

/// A core plane read back from a picture `picture` made: each pixel's colour, at the
/// centre of its three by three, is 2 where the core is.
fn core_of(file: &[u8]) -> Vec<u8> {
    let mut at = 8;
    let mut data = Vec::new();
    while at < file.len() {
        let length = u32::from_be_bytes(file[at..at + 4].try_into().unwrap()) as usize;
        if &file[at + 4..at + 8] == b"IDAT" {
            data.extend_from_slice(&file[at + 8..at + 8 + length]);
        }
        at += 12 + length;
    }
    let (mut rows, mut at) = (Vec::new(), 2);
    loop {
        let last = data[at] == 1;
        let length = u16::from_le_bytes([data[at + 1], data[at + 2]]) as usize;
        rows.extend_from_slice(&data[at + 5..at + 5 + length]);
        at += 5 + length;
        if last {
            break;
        }
    }
    let stride = 1 + (W * 3).div_ceil(2);
    (0..W * H)
        .map(|p| {
            let (x, y) = (p % W * 3 + 1, p / W * 3 + 1);
            let byte = rows[y * stride + 1 + x / 2];
            let shade = if x % 2 == 0 { byte >> 4 } else { byte & 15 };
            if shade == 2 { 255 } else { 0 }
        })
        .collect()
}

/// What the model tier wrote: for each example, its line and its masks' lines.
fn answered() -> String {
    let path =
        std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("model-tier/out/segment-record.txt");
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e}; run the model tier's segment_answers_the_records_examples_on_the_mac first",
            path.display()
        )
    })
}

/// The example names and queries, as the model tier asks them.
const ASKED: [(&str, &str); 2] = [
    (
        "phrase",
        "`kind` phrase, `query` \"red circle\", `max_instances` 4",
    ),
    (
        "point",
        "`kind` point, `query` \"128 92\", `max_instances` 1",
    ),
];

pub fn page(tool: &'static str) -> Page {
    let (files, measured, _) = parts();
    Page {
        tool,
        files,
        measured,
    }
}

/// The page's times: what the model tier measured.
pub fn timed(_: &str) -> String {
    parts().2
}

/// The pictures, the measurements and the times, from what the model tier wrote.
fn parts() -> (Vec<(String, Vec<u8>)>, String, String) {
    let text = answered();
    let mut lines = text.lines();
    let scene_line = lines.next().unwrap();
    assert_eq!(
        scene_line,
        format!("scene {}", scene().digest().0),
        "another scene"
    );
    let mut files = vec![("scene.png".to_owned(), image_picture(&scene(), None))];
    let mut measured = String::from(
        "\n| Example | Asked | Instance | Score | Box | Core pixels | Best match drawn | Overlap |\n\
         |---|---|---|---|---|---|---|---|\n",
    );
    let mut timed = String::from(
        "\n| Example | Picture | Instances | Model, installation, device | Seconds for the run |\n\
         |---|---|---|---|---|\n",
    );
    let mut example = ("", "");
    let mut instance = 0;
    for line in lines {
        let words: Vec<&str> = line.split(' ').collect();
        if words[0] == "example" {
            example = *ASKED.iter().find(|(name, _)| *name == words[1]).unwrap();
            instance = 0;
            timed.push_str(&format!(
                "| {} | {W}x{H} | {} | sam3, {}, {} | {} |\n",
                words[1], words[2], words[3], words[4], words[5]
            ));
            continue;
        }
        instance += 1;
        let mut core = Vec::with_capacity(W * H);
        for (at, run) in words[6].split(',').enumerate() {
            let value = if at % 2 == 0 { 0 } else { 255 };
            core.extend(std::iter::repeat_n(value, run.parse().unwrap()));
        }
        let mask: Mask = mask_of(W, H, core, &[0; W * H]);
        let (shape, overlap) = best_match(mask.core());
        files.push((format!("{}-{instance}.png", example.0), picture(&mask)));
        measured.push_str(&format!(
            "| {} | {} | {instance} | {} | {} {} {} {} | {} | {shape} | {overlap} |\n",
            example.0,
            example.1,
            words[1],
            words[2],
            words[3],
            words[4],
            words[5],
            super::count(mask.core()),
        ));
    }
    measured.push('\n');
    timed.push('\n');
    (files, measured, timed)
}

#[test]
fn the_record_of_segment_holds_its_pictures_and_the_scene() {
    let folder = tools().join("segment");
    let scene_file = std::fs::read(folder.join("scene.png")).unwrap();
    assert!(
        scene_file == image_picture(&scene(), None),
        "the scene is not the drawing"
    );
    let record = std::fs::read_to_string(tools().join("segment.md")).unwrap();
    let rows = between(&record, "measured")
        .lines()
        .filter(|l| l.starts_with("| p"));
    let mut seen = 0;
    for row in rows {
        let cells: Vec<&str> = row.split(" | ").collect();
        let file =
            std::fs::read(folder.join(format!("{}-{}.png", &cells[0][2..], cells[2]))).unwrap();
        let core = core_of(&file);
        let mask = mask_of(W, H, core.clone(), &[0; W * H]);
        assert!(picture(&mask) == file, "{row}: not a picture of a core");
        let (shape, overlap) = best_match(&core);
        assert_eq!(
            (cells[5], cells[6], cells[7].trim_end_matches(" |")),
            (
                super::count(&core).to_string().as_str(),
                shape,
                overlap.as_str()
            ),
            "{row}"
        );
        seen += 1;
    }
    assert!(seen >= 2, "{seen} masks in the record");
}
