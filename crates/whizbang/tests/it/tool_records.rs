//! The tool record of the loop `remove_checked`, `docs/reference/tools/remove_checked.md`
//! (`SPEC.md` section 8, "Tool records"; plan m4, task E1, item 4): its two worked
//! examples run here, through the binary's wiring, so the page cannot drift from the
//! loop. They are scripted runs on the fixture of `checked`: the test kit's fake
//! segmenter, a filler that paints the support one colour, and a judge that plays the
//! submissions an example lists. A test makes no live call.
//!
//! The record is the binary's and not `whizbang-rascal`'s, where the other recipes'
//! records are held, because only the binary holds rascal's loop and squint's check
//! together (plan m4, task L1, "Where the tests are").
//!
//! - The example images beside the record, `docs/reference/tools/remove_checked/`, are
//!   pictures of what the loop was given and gave, each pixel three wide and three high:
//!   the committed files are held to them pixel for pixel.
//! - The part between the `measured` marks is the examples in words and numbers, the
//!   same on every run and every machine.
//! - `scripts/tool-record.sh remove_checked` writes both again (`writes::remove_checked`).

use std::path::PathBuf;

use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use serde_json::Value as Json;
use whizbang_core::artifact::{self, ArtifactId, ArtifactType, Mask};
use whizbang_core::contract::record::{Changed, Decision};
use whizbang_core::identity::DecodeLimits;

use crate::checked::{
    Looped, PHRASE, Setup, asking, items, output, passes, pixels, remains, submission,
};
use crate::located::{PLACE, finding, remains_at};
use crate::recipe::PAINT;

const TOOL: &str = "remove_checked";
/// The side of the fixture's picture, and how many times its size a picture shows it.
const SIDE: usize = 64;
const SCALE: usize = 3;

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

/// One example: a name, what the judge is scripted to say, in words, and its submissions.
struct Case {
    name: &'static str,
    said: &'static str,
    answers: Vec<Json>,
    /// The kit: whether its segmenter finds something inside a crop.
    kit: fn(Vec<Json>) -> Setup,
}

/// The three examples: two the loop handles, a retry aimed at the place a finding named
/// and a retry with a wider mask, each then a pass; and one it handles badly, a finding
/// about a protected subject, where it stops and hands back what failed.
fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "located",
            said: "Attempt 1: `RQ1` violated, confident, at a box right of the first mask. \
                   Attempt 2: every item held",
            answers: vec![remains_at(PLACE), passes()],
            kit: finding,
        },
        Case {
            name: "retry",
            said: "Attempt 1: `RQ1` violated, confident, at no place. Attempt 2: every item held",
            answers: vec![remains(), passes()],
            kit: Setup::answering,
        },
        Case {
            name: "protected",
            said: "Attempt 1: `SF1` violated, confident (a protected subject changed), and \
                   `RQ1` violated, confident",
            answers: vec![submission(
                items(&[
                    ("SF1", "violation", "confident"),
                    ("RQ1", "violation", "confident"),
                ]),
                Vec::new(),
            )],
            kit: Setup::answering,
        },
    ]
}

/// What one example came to.
struct Example {
    name: &'static str,
    said: &'static str,
    /// The picture the loop was given, as RGBA rows.
    base: Vec<u8>,
    /// The mask of the first attempt, and the kept attempt's.
    first: Mask,
    kept: Mask,
    /// The kept attempt's picture, as RGBA rows.
    out: Vec<u8>,
    /// Each attempt: its verdict, and what its retry changed.
    attempts: Vec<(Option<Decision>, Option<Changed>)>,
    kept_number: u32,
    verdict: Option<Decision>,
    ended: String,
    /// The requests the segmenter, the filler and the judge were sent.
    requests: (u64, u64, u64),
}

fn mask_of(world: &Looped, id: ArtifactId) -> Mask {
    let reader = world.reader();
    let read = artifact::get(&reader, ArtifactType::Mask, id, &DecodeLimits::default());
    read.unwrap().into_mask().unwrap()
}

fn examples() -> Vec<Example> {
    cases()
        .into_iter()
        .map(|case| {
            // The mask of the first attempt: a loop keeps the files of the attempt it
            // keeps and no other's, so the first attempt is made alone, with one attempt
            // asked, under the same first answer, and its mask is the one kept there.
            let alone = Looped::new(
                &format!("record-{}-first", case.name),
                &(case.kit)(vec![case.answers[0].clone()]),
            );
            let only = alone.run(&asking(&alone, 1));
            assert_eq!(only.outcome().name(), "completed", "{}", case.name);
            let first = mask_of(&alone, output(&only, "mask"));
            let world = Looped::new(&format!("record-{}", case.name), &(case.kit)(case.answers));
            let finished = world.run(&world.call());
            assert_eq!(finished.outcome().name(), "completed", "{}", case.name);
            let attempts = world.attempts(&finished);
            Example {
                name: case.name,
                said: case.said,
                base: pixels(&world, world.image_id()),
                first,
                kept: mask_of(&world, output(&finished, "mask")),
                out: pixels(&world, output(&finished, "image")),
                attempts: (attempts.list.iter())
                    .map(|attempt| (attempt.verdict, attempt.changed.clone()))
                    .collect(),
                kept_number: attempts.kept.expect("an attempt is kept"),
                verdict: finished.verdict().map(|verdict| verdict.decision()),
                ended: attempts.ended,
                requests: world.kit.requests(),
            }
        })
        .collect()
}

/// A picture of RGBA rows of the fixture's size, its alpha left out: RGB, three times
/// the size.
fn image_picture(rgba: &[u8]) -> Vec<u8> {
    scaled(|at| [rgba[at * 4], rgba[at * 4 + 1], rgba[at * 4 + 2]])
}

/// A picture of a mask: its core white, a protected pixel red, its blend in blues, the
/// lighter the more it weighs, and black where it weighs nothing.
fn mask_picture(mask: &Mask) -> Vec<u8> {
    scaled(|at| {
        let (core, blend, protect) = (mask.core()[at], mask.blend()[at], mask.protect()[at]);
        if protect != 0 {
            [200, 40, 40]
        } else if core != 0 {
            [255, 255, 255]
        } else if blend != 0 {
            [
                20,
                40 + blend / 2,
                90 + (u16::from(blend) * 165 / 255) as u8,
            ]
        } else {
            [0, 0, 0]
        }
    })
}

fn scaled(colour: impl Fn(usize) -> [u8; 3]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(SIDE * SIDE * SCALE * SCALE * 3);
    for y in 0..SIDE * SCALE {
        for x in 0..SIDE * SCALE {
            rgb.extend(colour((y / SCALE) * SIDE + x / SCALE));
        }
    }
    rgb
}

fn png(rgb: &[u8]) -> Vec<u8> {
    let side = (SIDE * SCALE) as u32;
    let mut file = Vec::new();
    PngEncoder::new(&mut file)
        .write_image(rgb, side, side, ExtendedColorType::Rgb8)
        .unwrap();
    file
}

fn count(plane: &[u8]) -> usize {
    plane.iter().filter(|sample| **sample != 0).count()
}

/// Whether the mask weighs the pixel at `at` nothing.
fn nothing(mask: &Mask, at: usize) -> bool {
    mask.protect()[at] != 0 || (mask.core()[at] == 0 && mask.blend()[at] == 0)
}

fn word(decision: Option<Decision>) -> &'static str {
    match decision {
        Some(Decision::Pass) => "pass",
        Some(Decision::Fail) => "fail",
        Some(Decision::Unsure) => "unsure",
        None => "not checked",
    }
}

/// What the record's fast test holds: the example pictures by file name, as RGB rows,
/// and the part between the `measured` marks.
struct Page {
    files: Vec<(String, Vec<u8>)>,
    measured: String,
}

fn page_of(examples: &[Example]) -> Page {
    let mut files = Vec::new();
    let mut measured = String::from(
        "\n| Example | What the scripted judge said | Attempts | Kept | Verdict handed back | \
         Why the attempts ended | Requests: segmenter, filler, judge | Core and blend of the \
         first mask | Of the kept mask | Pixels changed | Pixels the kept mask weighs \
         nothing, and the picture's |\n|---|---|---|---|---|---|---|---|---|---|---|\n",
    );
    for example in examples {
        let name = example.name;
        files.push((format!("{name}.png"), image_picture(&example.base)));
        files.push((format!("{name}-first.png"), mask_picture(&example.first)));
        files.push((format!("{name}-mask.png"), mask_picture(&example.kept)));
        files.push((format!("{name}-out.png"), image_picture(&example.out)));
        let each: Vec<String> = (example.attempts.iter())
            .map(|(verdict, changed)| match changed {
                None => word(*verdict).to_owned(),
                Some(Changed::Wider { px }) => {
                    format!("{}, after a wider mask ({px} px)", word(*verdict))
                }
                Some(Changed::Located { crops }) => {
                    let found = crops.iter().filter(|crop| crop.mask.is_some()).count();
                    format!(
                        "{}, after looking into the place named ({} joined, something found in {found})",
                        word(*verdict),
                        crops.len()
                    )
                }
            })
            .collect();
        let all = SIDE * SIDE;
        let pixel = |rgba: &[u8], at: usize| rgba[at * 4..at * 4 + 4].to_vec();
        let changed = (0..all)
            .filter(|at| pixel(&example.base, *at) != pixel(&example.out, *at))
            .count();
        let outside: Vec<usize> = (0..all).filter(|at| nothing(&example.kept, *at)).collect();
        let own = (outside.iter())
            .filter(|at| pixel(&example.base, **at) == pixel(&example.out, **at))
            .count();
        let (segment, fill, judge) = example.requests;
        measured.push_str(&format!(
            "| {name} | {} | {}: {} | {} | {} | {} | {segment}, {fill}, {judge} | {} and {} | {} and {} | {changed} | {own} of {} |\n",
            example.said,
            example.attempts.len(),
            each.join("; then "),
            example.kept_number,
            word(example.verdict),
            example.ended,
            count(example.first.core()),
            count(example.first.blend()),
            count(example.kept.core()),
            count(example.kept.blend()),
            outside.len(),
        ));
    }
    measured.push('\n');
    Page { files, measured }
}

/// Fails unless the committed record and its pictures are what the page says.
fn hold(page: &Page) {
    let again = format!("run scripts/tool-record.sh {TOOL}");
    for (name, rgb) in &page.files {
        let path = tools().join(TOOL).join(name);
        let committed = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let decoded = image::load_from_memory(&committed)
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .to_rgb8();
        assert!(
            decoded.width() as usize == SIDE * SCALE && decoded.into_raw() == *rgb,
            "{name} beside the record of {TOOL} is not a picture of what it gives: {again}"
        );
    }
    let record = std::fs::read_to_string(tools().join(format!("{TOOL}.md"))).unwrap();
    assert!(
        between(&record, "measured") == page.measured,
        "the measurements of the record of {TOOL} are stale: {again}\n{}",
        page.measured
    );
}

mod recipes {
    use super::*;

    #[test]
    fn the_record_of_remove_checked_is_what_it_gives() {
        let examples = examples();
        hold(&page_of(&examples));
        let [located, retry, protected] = &examples[..] else {
            panic!("three examples");
        };
        // A confident report that the thing remains, at a place: the retry looks into
        // the place, what it finds there joins the mask, and the second attempt passes.
        assert_eq!(located.attempts[0], (Some(Decision::Fail), None));
        assert!(matches!(
            &located.attempts[1],
            (Some(Decision::Pass), Some(Changed::Located { crops })) if crops.len() == 1
        ));
        assert_eq!(
            (located.kept_number, located.verdict),
            (2, Some(Decision::Pass))
        );
        // The segmenter was asked twice: about the picture, and inside the place.
        assert_eq!(located.requests, (2, 2, 2));
        assert!(count(located.kept.core()) > count(located.first.core()));
        // The same report at no place widens the mask once, and the second attempt
        // passes and is kept.
        assert_eq!(retry.attempts.len(), 2);
        assert_eq!(retry.attempts[0], (Some(Decision::Fail), None));
        assert!(matches!(
            retry.attempts[1],
            (Some(Decision::Pass), Some(Changed::Wider { .. }))
        ));
        assert_eq!(
            (retry.kept_number, retry.verdict),
            (2, Some(Decision::Pass))
        );
        assert_eq!(retry.requests, (1, 2, 2));
        assert!(count(retry.kept.core()) > count(retry.first.core()));
        // The one it handles badly: a finding about a protected subject ends the
        // attempts, though the thing is reported still there; nothing more is filled,
        // and what is handed back is the attempt that failed, with its verdict.
        assert_eq!(protected.attempts, [(Some(Decision::Fail), None)]);
        assert_eq!(
            (protected.kept_number, protected.verdict),
            (1, Some(Decision::Fail))
        );
        assert_eq!(protected.requests, (1, 1, 1));
        assert!(protected.ended.contains("must be preserved"));
        for example in &examples {
            // What the record says in words: every pixel the kept mask weighs nothing
            // is the picture's own, and every core pixel is the fill's whole colour.
            for at in 0..SIDE * SIDE {
                let (was, is) = (
                    &example.base[at * 4..at * 4 + 4],
                    &example.out[at * 4..at * 4 + 4],
                );
                if nothing(&example.kept, at) {
                    assert_eq!(was, is, "{} at {at}", example.name);
                } else if example.kept.core()[at] != 0 {
                    assert_eq!(is[..3], PAINT, "{} at {at}", example.name);
                }
            }
        }
        // The record names the phrase the examples ask for.
        let record = std::fs::read_to_string(tools().join(format!("{TOOL}.md"))).unwrap();
        assert!(record.contains(&format!("\"{PHRASE}\"")));
    }
}

/// What `scripts/tool-record.sh remove_checked` runs: it writes the record's pictures
/// and the part between its `measured` marks again.
mod writes {
    use super::*;

    #[test]
    #[ignore = "rewrites the committed record; run it with scripts/tool-record.sh"]
    fn remove_checked() {
        let page = page_of(&examples());
        let folder = tools().join(TOOL);
        std::fs::create_dir_all(&folder).unwrap();
        for (name, rgb) in &page.files {
            std::fs::write(folder.join(name), png(rgb)).unwrap();
        }
        let path = tools().join(format!("{TOOL}.md"));
        let record = std::fs::read_to_string(&path).unwrap();
        let old = between(&record, "measured");
        let at = old.as_ptr() as usize - record.as_ptr() as usize;
        let new = format!(
            "{}{}{}",
            &record[..at],
            page.measured,
            &record[at + old.len()..]
        );
        std::fs::write(path, new).unwrap();
    }
}
