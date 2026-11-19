//! The record of the detail `tiles`, `docs/reference/tools/tiles.md` (`SPEC.md` section 8):
//! its two worked examples cut here, byte for byte, by `view` with the boxes `check` uses.
//! `write_the_record`, which `scripts/tool-record.sh tiles` runs, writes the images.

use whizbang_core::artifact::Image;
use whizbang_squint::details::TILE_BOXES;

use super::record::tools;
use super::scenes::{self, IDENTITY, Scene};
use super::support::*;

/// A scene of 512 by 384, and the same with a detail of single pixels in its lower right:
/// lines one pixel wide that a look at the whole, scaled down, averages away.
fn pictures() -> [(&'static str, Image); 2] {
    let scene = Scene::random(41, 160, 0.75);
    let scene = scene.render(512, (512, 384), &IDENTITY, scenes::same);
    let fine = picture(512, 384, |x, y| {
        if x > 300 && y > 230 && x < 470 && y < 350 {
            let v = if x % 3 == 0 || y % 4 == 0 { 20 } else { 235 };
            [v, v, v, 255]
        } else {
            [90, 120, 160, 255]
        }
    });
    [("scene", scene), ("fine", fine)]
}

/// Each picture, then its four tiles, as `check` cuts them.
fn files() -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for (name, image) in pictures() {
        out.push((format!("{name}.png"), image.file().to_vec()));
        for (place, bbox) in TILE_BOXES {
            let tile = look(&image, bbox).image;
            out.push((format!("{name}-{place}.png"), tile.file().to_vec()));
        }
    }
    out
}

#[test]
fn the_records_examples_are_what_tiles_cuts() {
    let record = std::fs::read_to_string(tools().join("tiles.md")).unwrap();
    for (name, bytes) in files() {
        let path = tools().join("tiles").join(&name);
        let committed = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(
            committed == bytes,
            "{name} beside the record is not what tiles cuts: run scripts/tool-record.sh tiles"
        );
        assert!(record.contains(&format!("(tiles/{name})")), "{name}");
    }
}

#[test]
#[ignore = "rewrites the committed images; run it with scripts/tool-record.sh tiles"]
fn write_the_record() {
    let folder = tools().join("tiles");
    std::fs::create_dir_all(&folder).unwrap();
    for (name, bytes) in files() {
        std::fs::write(folder.join(name), bytes).unwrap();
    }
}
