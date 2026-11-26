//! The records of `select`, `plug` and `feather`, `docs/reference/tools/<tool>.md`. Each
//! example calls the operation's function directly, the function its action runs, so the
//! test needs no store and no executor.

use whizbang_core::artifact::{Mask, UseCase};
use whizbang_rascal::examples::{
    feather_blob, feather_wire, plug_holes, plug_openings, select_instances,
};
use whizbang_rascal::feather::feather;
use whizbang_rascal::plug::plug;
use whizbang_rascal::select::{holding, select};

use super::super::grow::{mask_of, plane};
use super::super::record::{picture, tiled, time};
use super::{Page, SIZES, count, hold, timed_head, timed_row};

const W: usize = 96;
const H: usize = 64;

fn never() -> bool {
    false
}

/// One example: its name, what it was asked in words, the masks given and the mask made.
pub struct Example {
    pub name: &'static str,
    pub asked: String,
    pub given: Vec<Mask>,
    pub made: Mask,
}

fn examples(tool: &str) -> Vec<Example> {
    let example = |name, asked: &str, given: Vec<Mask>, made| Example {
        name,
        asked: asked.to_owned(),
        given,
        made,
    };
    match tool {
        "select" => {
            let all = select_instances();
            let kept = vec![all[0].clone(), all[2].clone()];
            let cut = select(&kept, &[[70.0, 0.0, 96.0, 24.0]], UseCase::Remove).unwrap();
            let held = holding(all.clone(), [38.0, 32.0]).unwrap();
            let picked = select(&held, &[], UseCase::Remove).unwrap();
            vec![
                example(
                    "kept",
                    "the first and third, `exclude` \"70 0 96 24\"",
                    kept,
                    cut,
                ),
                example("at", "all three, `at` \"38 32\"", all, picked),
            ]
        }
        "plug" => {
            let (holed, bays) = (plug_holes(), plug_openings());
            let plugged = |mask: &Mask| plug(mask.clone(), 0.1, &never).unwrap().unwrap().0;
            vec![
                example(
                    "holes",
                    "`max_share` 0.1",
                    vec![holed.clone()],
                    plugged(&holed),
                ),
                example(
                    "openings",
                    "`max_share` 0.1",
                    vec![bays.clone()],
                    plugged(&bays),
                ),
            ]
        }
        _ => {
            let (blob, wire) = (feather_blob(), feather_wire());
            let feathered =
                |mask: &Mask, sigma| feather(mask.clone(), sigma, &never).unwrap().unwrap();
            vec![
                example(
                    "blob",
                    "`sigma_px` 3",
                    vec![blob.clone()],
                    feathered(&blob, 3.0),
                ),
                example(
                    "wire",
                    "`sigma_px` 3",
                    vec![wire.clone()],
                    feathered(&wire, 3.0),
                ),
            ]
        }
    }
}

/// The masks given as one picture: their cores together.
fn given_picture(given: &[Mask]) -> Vec<u8> {
    let mut union = given[0].clone();
    if given.len() > 1 {
        union = select(given, &[], UseCase::Remove).unwrap();
    }
    picture(&union)
}

pub fn page(tool: &'static str) -> Page {
    page_of(tool, &examples(tool))
}

fn page_of(tool: &'static str, examples: &[Example]) -> Page {
    let mut files = Vec::new();
    let mut measured = String::from(
        "\n| Example | Asked | Masks given | Core before | Core after | Blend after | Protected after |\n\
         |---|---|---|---|---|---|---|\n",
    );
    for example in examples {
        files.push((
            format!("{}.png", example.name),
            given_picture(&example.given),
        ));
        files.push((format!("{}-made.png", example.name), picture(&example.made)));
        let before: usize = example.given.iter().map(|m| count(m.core())).sum();
        measured.push_str(&format!(
            "| {} | {} | {} | {before} | {} | {} | {} |\n",
            example.name,
            example.asked,
            example.given.len(),
            count(example.made.core()),
            count(example.made.blend()),
            count(example.made.protect()),
        ));
    }
    measured.push('\n');
    Page {
        tool,
        files,
        measured,
    }
}

pub fn timed(tool: &str) -> String {
    let column = match tool {
        "select" => "`select` of two masks and a box",
        "plug" => "`plug`, `max_share` 0.1",
        _ => "`feather`, `sigma_px` 3",
    };
    let mut out = timed_head(&[column]);
    for (width, height) in SIZES {
        let mask = tiled(width, height);
        let took = match tool {
            "select" => {
                let other = mask_of(
                    width,
                    height,
                    plane(width, height, |x, _| x % 256 < 16),
                    &vec![0; width * height],
                );
                let pair = [mask.clone(), other];
                let boxed = [[0.0, 0.0, width as f64 / 2.0, height as f64 / 2.0]];
                time(
                    5,
                    || (),
                    |()| select(&pair, &boxed, UseCase::Remove).unwrap(),
                )
            }
            "plug" => time(5, || mask.clone(), |m| plug(m, 0.1, &never)),
            _ => time(5, || mask.clone(), |m| feather(m, 3.0, &never)),
        };
        out.push_str(&timed_row(width, height, &[took]));
    }
    out.push('\n');
    out
}

#[test]
fn the_records_of_select_plug_and_feather_are_what_they_give() {
    let at = |m: &Mask, x: usize, y: usize| (m.core()[y * W + x], m.blend()[y * W + x]);
    for tool in ["select", "plug", "feather"] {
        let examples = examples(tool);
        hold(&page_of(tool, &examples));
        let [one, two] = &examples[..] else { panic!() };
        match tool {
            "select" => {
                // The box cleared the third oval's top; the second oval is out. At (38, 32)
                // both overlapping ovals hold the pixel, so both are kept.
                assert_eq!((at(&one.made, 76, 20).0, at(&one.made, 76, 30).0), (0, 255));
                assert_eq!(at(&one.made, 50, 34).0, 0);
                assert_eq!(
                    (at(&two.made, 20, 30).0, at(&two.made, 50, 34).0),
                    (255, 255)
                );
                assert_eq!(at(&two.made, 76, 28).0, 0);
            }
            "plug" => {
                // The hole of 16 is filled, the one of 196 is not; the bay is open along a
                // side and stays, the pocket is open across a corner only and is filled.
                assert_eq!((at(&one.made, 21, 31).0, at(&one.made, 36, 28).0), (255, 0));
                assert_eq!((at(&two.made, 62, 22).0, at(&two.made, 62, 42).0), (0, 255));
                assert_eq!(at(&two.made, 60, 40).0, 0);
            }
            _ => {
                // The blend falls off outward and is 0 under the bar; a wire one pixel
                // wide gets a feather nine rows deep on each side.
                assert!(at(&one.made, 61, 32).1 > at(&one.made, 63, 32).1);
                assert_eq!(at(&one.made, 57, 32), (0, 0));
                let column: Vec<usize> = (0..H).filter(|y| at(&two.made, 40, *y).1 != 0).collect();
                assert_eq!(
                    column,
                    [(23..32).collect::<Vec<_>>(), (33..42).collect()].concat()
                );
            }
        }
    }
}
