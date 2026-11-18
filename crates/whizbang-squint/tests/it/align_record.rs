//! The tool record of `align`, `docs/reference/tools/align.md` (`SPEC.md` section 8): its
//! examples run here, byte for byte, and its rung 2 table is what the test of the synthetic
//! pairs measures (`align::a_known_homography_is_recovered_within_the_records_error_on_the_
//! synthetic_pairs`). `write_the_record`, which `scripts/tool-record.sh align` runs, writes
//! the images, that table, what `align` said of each example, and the times.

use std::time::Duration;

use whizbang_core::artifact::Image;
use whizbang_squint::align::{self, Options};

use super::record::{between, replaced, time, tools};
use super::scenes::{self, IDENTITY, Scene, similarity};
use super::support::*;

const SIZE: (u32, u32) = (192, 144);

/// The two examples: a restyled, turned edit of a scene; and the scene with one block
/// moved inside the frame, which registers just as well.
struct Examples {
    source: Image,
    candidate: Image,
    aligned: Image,
    reshaped: Image,
    reshaped_aligned: Image,
    said: [String; 2],
}

fn examples() -> Examples {
    let scene = Scene::random(21, 160, f64::from(SIZE.1) / f64::from(SIZE.0));
    let source = scene.render(SIZE.0, SIZE, &IDENTITY, scenes::same);
    let turned = similarity(SIZE, 1.03, 0.03, (4.0, -3.0));
    let candidate = scene.render(SIZE.0, SIZE, &turned, scenes::restyled);
    // A block of the middle of the scene drawn again 40 pixels to its left: the structure
    // changed, the camera did not.
    let reshaped = {
        let pixels = rgba_of(&source);
        picture(SIZE.0, SIZE.1, |x, y| {
            let inside = (60..100).contains(&x) && (52..92).contains(&y);
            let sx = if inside { x + 40 } else { x };
            let at = ((y * SIZE.0 + sx) * 4) as usize;
            [pixels[at], pixels[at + 1], pixels[at + 2], 255]
        })
    };
    let run = |image: &Image| align::align(image, &source, &Options::default(), &limits()).unwrap();
    let (first, second) = (run(&candidate), run(&reshaped));
    let said = [
        align::summary(&first, "candidate", "source"),
        align::summary(&second, "candidate", "source"),
    ];
    Examples {
        aligned: first.frame.unwrap().image,
        reshaped_aligned: second.frame.unwrap().image,
        source,
        candidate,
        reshaped,
        said,
    }
}

impl Examples {
    fn files(&self) -> [(&'static str, &[u8]); 5] {
        [
            ("source.png", self.source.file()),
            ("candidate.png", self.candidate.file()),
            ("aligned.png", self.aligned.file()),
            ("reshaped.png", self.reshaped.file()),
            ("reshaped-aligned.png", self.reshaped_aligned.file()),
        ]
    }

    /// The part of the record between the `said` marks: what `align` said of each.
    fn said(&self) -> String {
        format!(
            "\nOf the first: \"{}\"\n\nOf the second: \"{}\"\n\n",
            self.said[0], self.said[1]
        )
    }
}

fn record() -> String {
    std::fs::read_to_string(tools().join("align.md")).unwrap()
}

/// Fails unless the committed record's rung 2 table is this one.
pub fn holds(table: &str) {
    assert!(
        between(&record(), "measured") == format!("\n{table}\n"),
        "the record's measurements are stale: run scripts/tool-record.sh align\n{table}"
    );
}

#[test]
fn the_records_examples_are_what_align_gives() {
    let examples = examples();
    for (name, bytes) in examples.files() {
        let path = tools().join("align").join(name);
        let committed = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(
            committed == bytes,
            "{name} beside the record is not what align gives: run scripts/tool-record.sh align"
        );
    }
    assert_eq!(between(&record(), "said"), examples.said());
}

fn millis(time: Duration) -> String {
    format!("{:.0}", time.as_secs_f64() * 1000.0)
}

fn timed() -> String {
    let mut out = format!(
        "\nMeasured in a release build on {} {}; each time is the middle one of five runs, in \
         milliseconds, of `align` on a scene and its turned, restyled edit, the aligned frame \
         included.\n\n| Image | `align` |\n|---|---|\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
    for (width, height) in [(256u32, 192u32), (512, 384), (1024, 768), (2048, 1536)] {
        let size = (width, height);
        let scene = Scene::random(21, 160, f64::from(height) / f64::from(width));
        let source = scene.render(width, size, &IDENTITY, scenes::same);
        let turn = similarity(size, 1.03, 0.03, (4.0, -3.0));
        let candidate = scene.render(width, size, &turn, scenes::restyled);
        let took = time(5, || {
            align::align(&candidate, &source, &Options::default(), &limits())
        });
        out.push_str(&format!("| {width}x{height} | {} |\n", millis(took)));
    }
    out.push('\n');
    out
}

#[test]
#[ignore = "rewrites the committed record; run it with scripts/tool-record.sh align"]
fn write_the_record() {
    let folder = tools().join("align");
    std::fs::create_dir_all(&folder).unwrap();
    let examples = examples();
    for (name, bytes) in examples.files() {
        std::fs::write(folder.join(name), bytes).unwrap();
    }
    let record = record();
    let table = super::align::table(&super::align::pairs());
    let record = replaced(&record, "measured", &format!("\n{table}\n"));
    let record = replaced(&record, "said", &examples.said());
    let record = replaced(&record, "timed", &timed());
    std::fs::write(tools().join("align.md"), record).unwrap();
}
