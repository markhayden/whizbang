//! The tool records of `change_map` and `edge_similarity`, `docs/reference/tools/change_map.md`
//! and `edge_similarity.md` (`SPEC.md` section 8): their examples run here, byte for byte,
//! with what each tool said and measured. `write_the_record`, which `scripts/tool-record.sh
//! change_map` (or `edge_similarity`) runs, writes both records' pictures, sentences and
//! times.

use std::time::Duration;

use whizbang_core::artifact::Image;
use whizbang_squint::compare;

use super::edge_similarity::fixture;
use super::record::{between, replaced, time, tools};
use super::scenes::{self, IDENTITY, Scene};
use super::support::*;

/// The `before` scene with a box made grey at the same luminance (Rec. 601, rounded):
/// its colour taken away, its structure kept.
pub fn desaturated() -> Image {
    let pixels = fixture("before").pixels(&limits()).unwrap();
    let rgba = pixels.rgba().to_vec();
    let width = pixels.width();
    picture(width, pixels.height(), |x, y| {
        let at = ((y * width + x) * 4) as usize;
        let [r, g, b] = [rgba[at], rgba[at + 1], rgba[at + 2]];
        if (24..72).contains(&x) && (24..72).contains(&y) {
            let luma = 0.299 * f64::from(r) + 0.587 * f64::from(g) + 0.114 * f64::from(b);
            let v = luma.round().clamp(0.0, 255.0) as u8;
            [v, v, v, 255]
        } else {
            [r, g, b, 255]
        }
    })
}

/// The change map's examples: the files beside its record, and what it said of each.
fn change_examples() -> (Vec<(&'static str, Vec<u8>)>, String) {
    let (before, after, grey) = (fixture("before"), fixture("after"), desaturated());
    let (added, added_map, _) = compare::change_map(&before, &after, &limits()).unwrap();
    let (greyed, greyed_map, _) = compare::change_map(&before, &grey, &limits()).unwrap();
    let files = vec![
        ("before.png", before.file().to_vec()),
        ("after.png", after.file().to_vec()),
        ("map.png", added_map.file().to_vec()),
        ("desaturated.png", grey.file().to_vec()),
        ("desaturated-map.png", greyed_map.file().to_vec()),
    ];
    let said = format!(
        "\nOf the first: \"{}\"\n\nOf the second: \"{}\"\n\n",
        compare::changes_summary(&added, "raw:source", "aligned:candidate:t1", "the map"),
        compare::changes_summary(&greyed, "raw:source", "aligned:candidate:t1", "the map"),
    );
    (files, said)
}

/// The edge similarity's examples: the files beside its record, and what it said of each.
fn edge_examples() -> (Vec<(&'static str, Vec<u8>)>, String) {
    let names = ["scene", "toned", "other", "before", "after"];
    let files = names
        .iter()
        .map(|name| {
            let file: &'static str = match *name {
                "scene" => "scene.png",
                "toned" => "toned.png",
                "other" => "other.png",
                "before" => "before.png",
                _ => "after.png",
            };
            (file, fixture(name).file().to_vec())
        })
        .collect();
    let mut said = String::from("\n");
    for (frame, against) in [("scene", "toned"), ("scene", "other"), ("before", "after")] {
        let edges = compare::edge_similarity(&fixture(frame), &fixture(against), &limits());
        let line = compare::edges_summary(&edges.unwrap(), frame, against);
        said.push_str(&format!("- \"{line}\"\n"));
    }
    said.push('\n');
    (files, said)
}

fn record(name: &str) -> String {
    std::fs::read_to_string(tools().join(format!("{name}.md"))).unwrap()
}

fn holds(name: &str, files: &[(&str, Vec<u8>)], said: &str) {
    for (file, bytes) in files {
        let path = tools().join(name).join(file);
        let committed = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(
            committed == *bytes,
            "{file} beside the record of {name} is not what it gives: run scripts/tool-record.sh {name}"
        );
    }
    assert_eq!(between(&record(name), "said"), said, "{name}");
}

#[test]
fn the_records_examples_are_what_change_map_and_edge_similarity_give() {
    let (files, said) = change_examples();
    holds("change_map", &files, &said);
    let (files, said) = edge_examples();
    holds("edge_similarity", &files, &said);
}

fn timed(name: &str) -> String {
    let mut out = format!(
        "\nMeasured in a release build on {} {}; each time is the middle one of five runs, in \
         milliseconds, of `{name}` on a scene and its restyled copy{}.\n\n| Image | `{name}` |\n|---|---|\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
        if name == compare::CHANGE_MAP {
            ", the map's file included"
        } else {
            ""
        },
    );
    for (width, height) in [(256u32, 192u32), (512, 384), (1024, 768), (2048, 1536)] {
        let size = (width, height);
        let scene = Scene::random(21, 160, f64::from(height) / f64::from(width));
        let source = scene.render(width, size, &IDENTITY, scenes::same);
        let candidate = scene.render(width, size, &IDENTITY, scenes::restyled);
        let took: Duration = if name == compare::CHANGE_MAP {
            time(5, || compare::change_map(&source, &candidate, &limits()))
        } else {
            time(5, || {
                compare::edge_similarity(&source, &candidate, &limits())
            })
        };
        out.push_str(&format!(
            "| {width}x{height} | {:.0} |\n",
            took.as_secs_f64() * 1000.0
        ));
    }
    out.push('\n');
    out
}

#[test]
#[ignore = "rewrites the committed records; run it with scripts/tool-record.sh change_map"]
fn write_the_record() {
    for (name, (files, said)) in [
        (compare::CHANGE_MAP, change_examples()),
        (compare::EDGE_SIMILARITY, edge_examples()),
    ] {
        let folder = tools().join(name);
        std::fs::create_dir_all(&folder).unwrap();
        for (file, bytes) in &files {
            std::fs::write(folder.join(file), bytes).unwrap();
        }
        let record = replaced(&record(name), "said", &said);
        let record = replaced(&record, "timed", &timed(name));
        std::fs::write(tools().join(format!("{name}.md")), record).unwrap();
    }
}
