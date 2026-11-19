//! The tool records of plan m3, tasks D1, D2 and B1 (`SPEC.md` section 8, "Tool records"): each
//! record's worked examples run here, so a page cannot drift from its tool.
//!
//! What every record shares, as the record of `grow` has it (`super::record`):
//!
//! - the example images beside the record, `docs/reference/tools/<tool>/`, are pictures
//!   of what the tool was given and gave, byte for byte: a mask by `super::record::picture`,
//!   an image by [`image_picture`], three times its size;
//! - the part between the `measured` marks is the examples in numbers, the same on every
//!   run and every machine, and the fast tests hold it;
//! - the part between the `timed` marks is written by `scripts/tool-record.sh <tool>`
//!   alone, which runs the module `writes` below in a release build, and keeps the times
//!   the record holds unless one moved by more than a third (`super::record::settled`).
//!
//! Rung 2 of each is held by the tests that port rascal's vectors, which each record
//! names; they are not run again here.

use std::time::Duration;

use whizbang_core::identity::CanonicalPixels;

use super::record::{MARK, between, millis, png_file, replaced, settled, tools};

mod close;
mod frames;
mod locations;
mod masks;
mod paste;
mod recipes;
mod render;
mod seams;
mod segment;
mod shaping;
mod subtract;
mod tasks;
mod tighten;

/// What a record's fast test holds: the tool, its example images by file name, and the
/// part between its `measured` marks.
pub struct Page {
    pub tool: &'static str,
    pub files: Vec<(String, Vec<u8>)>,
    pub measured: String,
}

/// Fails unless the committed record and its images are what the page says.
pub fn hold(page: &Page) {
    let again = format!("run scripts/tool-record.sh {}", page.tool);
    for (name, bytes) in &page.files {
        let path = tools().join(page.tool).join(name);
        let committed = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(
            committed == *bytes,
            "{name} beside the record of {} is not a picture of what it gives: {again}",
            page.tool
        );
    }
    let record = std::fs::read_to_string(tools().join(format!("{}.md", page.tool))).unwrap();
    assert!(
        between(&record, "measured") == page.measured,
        "the measurements of the record of {} are stale: {again}\n{}",
        page.tool,
        page.measured
    );
}

/// Writes the page's images and its two generated parts. The times stay as they are
/// when `settled` says so.
fn write(page: &Page, timed: &str) {
    let folder = tools().join(page.tool);
    std::fs::create_dir_all(&folder).unwrap();
    for (name, bytes) in &page.files {
        std::fs::write(folder.join(name), bytes).unwrap();
    }
    let path = tools().join(format!("{}.md", page.tool));
    let record = std::fs::read_to_string(&path).unwrap();
    let record = replaced(&record, "measured", &page.measured);
    let times = match settled(between(&record, "timed"), timed) {
        Some(kept) => kept.to_owned(),
        None => timed.replace(MARK, ""),
    };
    std::fs::write(path, replaced(&record, "timed", &times)).unwrap();
}

/// A picture of an image, each pixel three wide and three high, its colours as stored
/// and its alpha left out; where `marked` is set, two thirds of the way to a light amber,
/// to show a location's inside on its image.
pub fn image_picture(pixels: &CanonicalPixels, marked: Option<&[u8]>) -> Vec<u8> {
    const SCALE: usize = 3;
    let (width, height) = (pixels.width() as usize, pixels.height() as usize);
    let mut rows = Vec::new();
    for y in 0..height {
        let mut row = vec![0u8];
        for x in 0..width {
            let at = y * width + x;
            let mut rgb = [0u8; 3];
            rgb.copy_from_slice(&pixels.rgba()[at * 4..at * 4 + 3]);
            if marked.is_some_and(|plane| plane[at] != 0) {
                for (channel, light) in rgb.iter_mut().zip([255u16, 214, 90]) {
                    *channel = ((u16::from(*channel) + 2 * light) / 3) as u8;
                }
            }
            for _ in 0..SCALE {
                row.extend(rgb);
            }
        }
        for _ in 0..SCALE {
            rows.extend(&row);
        }
    }
    png_file(
        (width * SCALE) as u32,
        (height * SCALE) as u32,
        [8, 2],
        None,
        &rows,
    )
}

/// The examples' scene: 96 by 64, a soft gradient from blue to sand with a darker band
/// across its lower third, every pixel opaque.
pub fn scene() -> CanonicalPixels {
    let (width, height) = (96u32, 64u32);
    let rgba = (0..width * height)
        .flat_map(|at| {
            let (x, y) = (at % width, at / width);
            let band = if (40..48).contains(&y) { 40 } else { 0 };
            [
                (60 + x + y) as u8 - band,
                (90 + x / 2 + y) as u8 - band,
                (170 - x / 2) as u8 - band / 2,
                255,
            ]
        })
        .collect();
    CanonicalPixels::from_rgba(width, height, rgba).unwrap()
}

/// How many bytes of a plane are not 0.
pub fn count(plane: &[u8]) -> usize {
    plane.iter().filter(|sample| **sample != 0).count()
}

/// The sizes every record is timed at.
pub const SIZES: [(usize, usize); 3] = [(1024, 768), (4096, 3072), (8000, 6000)];

/// The opening of a part between the `timed` marks: the machine, and a table's head.
pub fn timed_head(columns: &[&str]) -> String {
    let machine = std::process::Command::new("sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_else(|| "an unnamed processor".to_owned());
    format!(
        "\nMeasured on {machine} ({}, {}), in a release build; each time is the middle one \
         of five runs, in milliseconds.\n\n| Size | Megapixels | {} |\n|---|---|{}\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
        columns.join(" | "),
        "---|".repeat(columns.len()),
    )
}

/// One row of a timed table.
pub fn timed_row(width: usize, height: usize, times: &[Duration]) -> String {
    let cells: Vec<String> = times.iter().map(|time| millis(*time)).collect();
    format!(
        "| {width}x{height} | {} | {} |\n",
        super::record::megapixels(width, height),
        cells.join(" | ")
    )
}

/// What `scripts/tool-record.sh <tool>` runs: each writes its record's images and its
/// two generated parts again.
mod writes {
    macro_rules! writes {
        ($($tool:ident => $module:ident),* $(,)?) => {$(
            #[test]
            #[ignore = "rewrites the committed record; run it with scripts/tool-record.sh"]
            fn $tool() {
                let page = super::$module::page(stringify!($tool));
                super::write(&page, &super::$module::timed(stringify!($tool)));
            }
        )*};
    }
    writes! {
        close => close,
        subtract => subtract,
        line => locations,
        rect => locations,
        stroke => locations,
        paste => paste,
        fill => paste,
        mask_rect => tasks,
        protect => tasks,
        segment => segment,
        select => shaping,
        plug => shaping,
        feather => shaping,
        render => render,
        rasterize => masks,
        shrink => masks,
        invert => masks,
        inspect => masks,
        cut => frames,
        put => frames,
        window => frames,
        unwindow => frames,
        tighten => tighten,
        fan => seams,
        profile => seams,
        ramp => seams,
        apply => seams,
        measure => seams,
        remove => recipes,
        remove_prompt => recipes,
        repair => recipes,
        repair_area => recipes,
        seam => recipes,
    }
}
