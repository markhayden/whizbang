//! The tool record of `resize`, `docs/reference/tools/resize.md` (`SPEC.md` section 8): its
//! examples run here, byte for byte, and its rung 2 table is made here. `write_the_record`,
//! which `scripts/tool-record.sh resize` runs, writes the images, the table and the times.

use image::RgbImage;
use image::imageops::{self, FilterType};
use whizbang_core::artifact::Image;
use whizbang_squint::resize::resize;

use super::record::{between, replaced, time, tools};
use super::scenes::{self, IDENTITY, Scene};
use super::support::*;

/// A picture with one-pixel detail: a checker of single pixels on the left, lines one
/// pixel wide on the right.
fn fine() -> Image {
    picture(240, 160, |x, y| {
        let v = if x < 120 {
            if (x + y) % 2 == 0 { 255 } else { 0 }
        } else if x % 3 == 0 {
            20
        } else {
            235
        };
        [v, v, v, 255]
    })
}

fn scene(width: u32, height: u32) -> Image {
    let scene = Scene::random(34, 160, f64::from(height) / f64::from(width));
    scene.render(width, (width, height), &IDENTITY, scenes::same)
}

fn files() -> Vec<(&'static str, Vec<u8>)> {
    let (scene, fine) = (scene(256, 192), fine());
    let small = resize(&scene, 96, &limits()).unwrap().image;
    let blurred = resize(&fine, 64, &limits()).unwrap().image;
    vec![
        ("scene.png", scene.file().to_vec()),
        ("scene-96.png", small.file().to_vec()),
        ("fine.png", fine.file().to_vec()),
        ("fine-64.png", blurred.file().to_vec()),
    ]
}

/// The rung 2 table: each picture fitted to each long edge, its size, how far its aspect is
/// from the picture's, and whether its pixels are the triangle filter's, exactly.
fn table() -> String {
    let mut out = String::from(
        "| Picture | Long edge asked | Result | Aspect, picture | Aspect, result | Pixels |\n\
         |---|---|---|---|---|---|\n",
    );
    let pictures = [
        ("noise", picture(250, 170, noisy(5, false))),
        ("noise", picture(97, 255, noisy(6, true))),
        ("flat colour", picture(200, 120, |_, _| [40, 90, 200, 255])),
    ];
    for (name, image) in &pictures {
        let (w, h) = (image.space().width(), image.space().height());
        for long_edge in [64u32, 100, 129] {
            let made = resize(image, long_edge, &limits()).unwrap().image;
            let (rw, rh) = (made.space().width(), made.space().height());
            let rgb: Vec<u8> = rgba_of(image)
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2]])
                .collect();
            let want = imageops::resize(
                &RgbImage::from_raw(w, h, rgb).unwrap(),
                rw,
                rh,
                FilterType::Triangle,
            );
            let got: Vec<u8> = rgba_of(&made)
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2]])
                .collect();
            out.push_str(&format!(
                "| {name}, {w}x{h} | {long_edge} | {rw}x{rh} | {:.4} | {:.4} | {} |\n",
                f64::from(w) / f64::from(h),
                f64::from(rw) / f64::from(rh),
                if got == want.into_raw() {
                    "the triangle filter's, exactly"
                } else {
                    "DIFFER"
                }
            ));
        }
    }
    out
}

fn record() -> String {
    std::fs::read_to_string(tools().join("resize.md")).unwrap()
}

#[test]
fn the_records_examples_are_what_resize_gives() {
    for (name, bytes) in files() {
        let path = tools().join("resize").join(name);
        let committed = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(
            committed == bytes,
            "{name} beside the record is not what resize gives: run scripts/tool-record.sh resize"
        );
    }
    let table = table();
    assert!(!table.contains("DIFFER"), "{table}");
    assert!(
        between(&record(), "measured") == format!("\n{table}\n"),
        "the record's measurements are stale: run scripts/tool-record.sh resize\n{table}"
    );
}

fn timed() -> String {
    let mut out = format!(
        "\nMeasured in a release build on {} {}; each time is the middle one of five runs, in \
         milliseconds, of `resize` to a long edge of 512, the file of the result included.\n\n\
         | Image | `resize` |\n|---|---|\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
    for (width, height) in [(1024u32, 768u32), (2048, 1536), (4096, 3072)] {
        let image = picture(width, height, noisy(9, false));
        let took = time(5, || resize(&image, 512, &limits()));
        out.push_str(&format!(
            "| {width}x{height} | {:.0} |\n",
            took.as_secs_f64() * 1000.0
        ));
    }
    out.push('\n');
    out
}

#[test]
#[ignore = "rewrites the committed record; run it with scripts/tool-record.sh resize"]
fn write_the_record() {
    let folder = tools().join("resize");
    std::fs::create_dir_all(&folder).unwrap();
    for (name, bytes) in files() {
        std::fs::write(folder.join(name), bytes).unwrap();
    }
    let record = replaced(&record(), "measured", &format!("\n{}\n", table()));
    let record = replaced(&record, "timed", &timed());
    std::fs::write(tools().join("resize.md"), record).unwrap();
}
