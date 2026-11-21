//! `loop_score::…`: `whizbang eval loop score` (plan e1, task L1, item 5), over runs of the
//! loop made through the binary's wiring with the loop's test kit ([`crate::checked`]):
//! a fake segmenter, a filler that paints, a judge that plays the submissions a test lists,
//! on a picture of 64 by 64. The rules of counting are shown on runs written as values.
//! The test of the command through the binary is here too, as `cli::…`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use whizbang::loop_score::{
    self, Against, AttemptScored, AttemptSeen, Mark, Marks, Rq1, RunScored, RunSeen, Scoring,
};
use whizbang_core::artifact::{self, ArtifactId, ArtifactType};
use whizbang_core::contract::{Executor, Request, Value};
use whizbang_core::identity::Digest;
use whizbang_testkit::scratch::Scratch;

use crate::checked::{Looped, Setup, passes, remains};

/// The judge's name in the kit's config, as the loop is asked it.
pub const JUDGE: &str = "plain";

/// The one item of the test's manifest, its group and phrase.
pub const ITEM: &str = "it-01";
pub const GROUP: &str = "whole";

/// The truth: a box around what the fake segmenter finds, wider than the first attempt's
/// shaped mask and inside the wider retry's.
pub const TRUTH: [u32; 4] = [8, 8, 42, 40];

/// A PNG of 64 by 64: white inside the box, black outside; or, without a box, a gradient.
pub fn png(inside: Option<[u32; 4]>) -> Vec<u8> {
    let mut rgba = Vec::new();
    for y in 0..64_u32 {
        for x in 0..64_u32 {
            let pixel = match inside {
                Some([l, t, r, b]) => {
                    let v = if x >= l && x < r && y >= t && y < b {
                        255
                    } else {
                        0
                    };
                    [v, v, v, 255]
                }
                None => [(x * 4) as u8, 200, (y * 4) as u8, 255],
            };
            rgba.extend_from_slice(&pixel);
        }
    }
    let mut out = Vec::new();
    PngEncoder::new(&mut out)
        .write_image(&rgba, 64, 64, ExtendedColorType::Rgba8)
        .unwrap();
    out
}

fn pin(bytes: &[u8]) -> String {
    format!("sha256:{}", Digest::of_bytes(bytes).hex())
}

/// The manifest of one item, and the folder of its pictures laid out as the repository
/// is, in `dir`: the manifest's path and the folder's.
pub fn set_of(dir: &Path, phrase: &str) -> (PathBuf, PathBuf) {
    let folder = dir.join("pictures");
    let items = folder.join("segmentation/segment-01/items").join(ITEM);
    std::fs::create_dir_all(&items).unwrap();
    let (image, truth) = (png(None), png(Some(TRUTH)));
    std::fs::write(items.join("image.png"), &image).unwrap();
    std::fs::write(items.join("truth.png"), &truth).unwrap();
    let manifest = dir.join("segment-01.toml");
    std::fs::write(
        &manifest,
        format!(
            "kind = \"segment-01\"\nset = \"segmentation/segment-01\"\n\
             images_repo = \"github.com/example/pictures\"\n\
             images_rev = \"{}\"\n\n[[item]]\nid = \"{ITEM}\"\ngroup = \"{GROUP}\"\n\
             phrase = \"{phrase}\"\nimage = \"items/{ITEM}/image.png\"\nimage_sha256 = \"{}\"\n\
             truth = \"items/{ITEM}/truth.png\"\ntruth_sha256 = \"{}\"\n",
            "a".repeat(40),
            pin(&image),
            pin(&truth)
        ),
    )
    .unwrap();
    (manifest, folder)
}

/// A store holding one run of the loop under the judge [`JUDGE`] and the request id of
/// the manifest's item, repetition 1, whose judge plays `answers`; and the manifest and
/// pictures beside it.
pub struct Scripted {
    pub world: Looped,
    pub dir: Scratch,
    pub manifest: PathBuf,
    pub folder: PathBuf,
    pub run: String,
}

pub fn scripted(label: &str, answers: Vec<serde_json::Value>) -> Scripted {
    // In memory while the loop runs, then written to the disk for the command's readers.
    let mut setup = Setup::answering(answers);
    setup.dear = true;
    let world = Looped::new(label, &setup);
    let engine = world.engine();
    let id = engine
        .request_id(&loop_score::request_id(JUDGE, 1, ITEM))
        .unwrap();
    let call = world.call().argument("judge", Value::text(JUDGE));
    let answer = engine.request(&Request::new(call).with_id(id)).unwrap();
    let run = match answer {
        whizbang_core::contract::Answer::Ran(finished) => finished.run().as_str().to_owned(),
        other => panic!("{other:?}"),
    };
    drop(engine);
    world.spill();
    let dir = Scratch::new(&format!("{label}-set"));
    let (manifest, folder) = set_of(dir.path(), crate::checked::PHRASE);
    Scripted {
        world,
        dir,
        manifest,
        folder,
        run,
    }
}

impl Scripted {
    pub fn scoring(&self) -> Scoring<'_> {
        Scoring {
            manifest: &self.manifest,
            set: &self.folder,
            root: &self.world.root,
            judge: JUDGE,
            repetitions: 1,
            labels: None,
        }
    }
}

/// The share of the truth's pixels under a mask of the world where its weight is at
/// least a half, worked out here.
fn share_under(world: &Looped, mask: ArtifactId) -> f64 {
    let reader = world.reader();
    let limits = whizbang_core::identity::DecodeLimits::default();
    let read = artifact::get_anywhere(&reader, ArtifactType::Mask, mask, &limits);
    let mask = read.unwrap().into_mask().unwrap();
    let weights: Vec<f32> = (0..64 * 64)
        .map(|at| mask.weight(at % 64, at / 64).unwrap())
        .collect();
    let [l, t, r, b] = TRUTH;
    let (mut inside, mut all) = (0, 0);
    for y in t..b {
        for x in l..r {
            all += 1;
            inside += u32::from(weights[(y * 64 + x) as usize] >= 0.5);
        }
    }
    f64::from(inside) / f64::from(all)
}

#[test]
fn coverage_is_the_share_of_truth_under_the_shaped_mask() {
    let scripted = scripted("score-coverage", vec![remains(), passes()]);
    let manifest = loop_score::Manifest::read(&scripted.manifest).unwrap().0;
    let runs = loop_score::runs_of(&scripted.world.root, &manifest, JUDGE).unwrap();
    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    assert_eq!(
        (run.repetition, run.kept, run.reason.as_str()),
        (1, Some(2), "passed")
    );
    assert_eq!(run.attempts.len(), 2);
    // The first attempt was a confident violation of RQ1, the retry a pass; each has a
    // found mask (the retry's is the first's, widened), a shaped mask and a fill.
    assert_eq!(run.attempts[0].rq1, Some(Rq1::Confident));
    assert_eq!(run.attempts[1].rq1, Some(Rq1::None));
    assert_eq!(run.attempts[1].found, run.attempts[0].found);
    let score = loop_score::score(&scripted.scoring()).unwrap();
    let all = &score.all;
    let first = share_under(&scripted.world, run.attempts[0].shaped.unwrap());
    let kept = share_under(&scripted.world, run.attempts[1].shaped.unwrap());
    assert!(first < loop_score::UNDER && kept > first, "{first} {kept}");
    assert_eq!(all.first_shaped_mask.coverage_mean, Some(first));
    assert_eq!(all.kept_shaped_mask.coverage_mean, Some(kept));
    assert_eq!(all.first_shaped_mask.under_covered, 1);
    // The retry raised the coverage, and the kept attempt had the highest.
    let retry = &all.coverage_across_a_retry;
    assert_eq!(
        (retry.compared, retry.coverage_up, retry.size_larger),
        (1, 1, 1)
    );
    assert_eq!(all.kept_had_the_highest_coverage.highest, 1);
    // The judge's confident violation fell on an under-covered mask.
    assert_eq!(all.rq1_against_under_coverage.checked, 2);
    assert_eq!(
        all.rq1_against_under_coverage
            .confident_violation_under_covered,
        1
    );
    assert_eq!(score.group[GROUP], score.all);
}

#[test]
fn a_truth_mask_is_read_by_its_pin_and_never_imported() {
    let scripted = scripted("score-truth-pin", vec![passes()]);
    let held = whizbang::import::pictures_by_file(&scripted.world.root).unwrap();
    loop_score::score(&scripted.scoring()).unwrap();
    // Nothing was brought into the store: the truth is read from the folder alone.
    assert_eq!(
        whizbang::import::pictures_by_file(&scripted.world.root).unwrap(),
        held
    );
    let truth = png(Some(TRUTH));
    let digest = Digest::of_bytes(&truth);
    assert!(!held.contains_key(&digest));
    // A truth mask that is not the one the manifest pins is refused.
    let file = scripted
        .folder
        .join("segmentation/segment-01/items")
        .join(ITEM)
        .join("truth.png");
    std::fs::write(&file, png(Some([0, 0, 8, 8]))).unwrap();
    let refused = loop_score::score(&scripted.scoring()).unwrap_err();
    assert!(refused.contains("another SHA-256"), "{refused}");
    // And so is one reached through a link.
    std::fs::remove_file(&file).unwrap();
    let aside = scripted.dir.path().join("aside.png");
    std::fs::write(&aside, &truth).unwrap();
    std::os::unix::fs::symlink(&aside, &file).unwrap();
    assert!(loop_score::score(&scripted.scoring()).is_err());
}

#[test]
fn the_output_holds_no_item() {
    let scripted = scripted("score-no-item", vec![remains(), passes()]);
    let score = loop_score::score(&scripted.scoring()).unwrap();
    let text = score.toml().unwrap();
    let manifest = std::fs::read_to_string(&scripted.manifest).unwrap();
    let pins: Vec<&str> = (manifest.lines())
        .filter_map(|l| {
            l.split_once("= \"sha256:")
                .map(|(_, pin)| pin.trim_end_matches('"'))
        })
        .collect();
    let mut held = vec![
        ITEM,
        crate::checked::PHRASE,
        scripted.run.as_str(),
        "MARMALADE",
    ];
    held.extend(pins);
    for word in held {
        assert!(!text.contains(word), "{word}");
    }
    assert!(text.contains("schema = \"loop-score/1\""));
    assert!(text.contains("[group.whole]"));
}

#[test]
fn without_labels_no_claim_about_the_picture_is_written() {
    let runs = [scored(
        "r1",
        vec![attempt(1, Some(0.9), true, Some(Rq1::None))],
        Some(1),
    )];
    let all: Vec<&RunScored> = runs.iter().collect();
    let group = loop_score::group(&all, None);
    assert!(group.pictures.is_none());
    let text = loop_score::toml_text(&group).unwrap();
    for word in ["visible", "damaged", "pictures"] {
        assert!(!text.contains(word), "{word}");
    }
}

// --- the rules of counting, on runs written as values -----------------------------------------

fn id(n: u8) -> ArtifactId {
    ArtifactId(Digest::of_bytes(&[n]))
}

/// An attempt: its number, its shaped mask's coverage when it has one, whether its fill
/// completed, and its check's RQ1 answer when it was checked.
fn attempt(number: u32, shaped: Option<f64>, filled: bool, rq1: Option<Rq1>) -> AttemptScored {
    let measured = |coverage: f64| Against {
        coverage,
        iou: coverage / 2.0,
        size: coverage * 1.5,
    };
    AttemptScored {
        seen: AttemptSeen {
            number,
            found: shaped.map(|_| id(1)),
            shaped: shaped.map(|_| id(number as u8 + 10)),
            filled: filled.then(|| id(number as u8 + 20)),
            rq1,
            unchecked: (rq1.is_none()).then(|| "why".to_owned()),
        },
        found: shaped
            .map(measured)
            .ok_or_else(|| "its_mask_could_not_be_made".to_owned()),
        shaped: shaped
            .map(measured)
            .ok_or_else(|| "its_mask_could_not_be_made".to_owned()),
    }
}

fn scored(run: &str, attempts: Vec<AttemptScored>, kept: Option<u32>) -> RunScored {
    RunScored {
        seen: RunSeen {
            run: run.to_owned(),
            item: ITEM.to_owned(),
            group: GROUP.to_owned(),
            repetition: 1,
            status: "completed".to_owned(),
            attempts: attempts.iter().map(|a| a.seen.clone()).collect(),
            kept,
            reason: "passed".to_owned(),
        },
        attempts,
    }
}

fn of(runs: &[RunScored], marks: Option<&Marks>) -> loop_score::Group {
    let all: Vec<&RunScored> = runs.iter().collect();
    loop_score::group(&all, marks)
}

#[test]
fn rq1_answers_are_counted_against_under_coverage_once_per_checked_attempt() {
    let runs = [
        scored(
            "r1",
            vec![
                attempt(1, Some(0.5), true, Some(Rq1::Confident)),
                attempt(2, Some(0.97), true, Some(Rq1::None)),
            ],
            Some(2),
        ),
        scored(
            "r2",
            vec![
                attempt(1, Some(0.8), true, Some(Rq1::Tentative)),
                // A fill that was not checked: no answer, counted nowhere in the table.
                attempt(2, Some(0.99), true, None),
            ],
            Some(1),
        ),
        scored(
            "r3",
            vec![attempt(1, Some(0.96), true, Some(Rq1::Confident))],
            Some(1),
        ),
    ];
    let g = of(&runs, None);
    let t = &g.rq1_against_under_coverage;
    assert_eq!(t.checked, 4);
    assert_eq!(
        (
            t.confident_violation_under_covered,
            t.confident_violation_covered,
            t.tentative_violation_under_covered,
            t.tentative_violation_covered,
            t.no_violation_under_covered,
            t.no_violation_covered
        ),
        (1, 1, 1, 0, 0, 1)
    );
    assert_eq!((g.attempts, g.checked, g.filled), (5, 4, 5));
    assert_eq!(g.no_check.values().sum::<usize>(), 1);
    // One run ended with its kept attempt under-covered, and its kept was not its best.
    assert_eq!(g.ended_under_covered, 1);
    assert_eq!(g.ended_under_covered_by_reason["passed"], 1);
    let kept = &g.kept_had_the_highest_coverage;
    assert_eq!((kept.runs, kept.highest, kept.not_highest), (3, 2, 1));
}

#[test]
fn a_retry_is_counted_up_within_or_down_by_its_coverage() {
    let runs = [
        scored(
            "r1",
            vec![
                attempt(1, Some(0.50), true, Some(Rq1::Confident)),
                attempt(2, Some(0.80), true, Some(Rq1::Confident)),
                attempt(3, Some(0.805), true, Some(Rq1::None)),
            ],
            Some(3),
        ),
        scored(
            "r2",
            vec![
                attempt(1, Some(0.90), true, Some(Rq1::Confident)),
                attempt(2, Some(0.70), true, Some(Rq1::None)),
            ],
            Some(2),
        ),
    ];
    let t = of(&runs, None).coverage_across_a_retry;
    assert_eq!(t.compared, 3);
    assert_eq!(
        (t.coverage_up, t.coverage_within, t.coverage_down),
        (1, 1, 1)
    );
    assert_eq!((t.size_larger, t.size_within, t.size_smaller), (1, 1, 1));
    assert_eq!(t.unavailable, 0);
}

/// The attempt a retry is when its shaped mask is the one before: shaped, not filled,
/// not checked.
fn unchanged(number: u32, coverage: f64) -> AttemptScored {
    let mut a = attempt(number, Some(coverage), false, None);
    a.seen.unchecked = Some("its shaped mask is the one before it, and nothing was filled".into());
    a
}

#[test]
fn an_unchanged_mask_retry_counts_as_an_attempt_with_no_fill() {
    let runs = [scored(
        "r1",
        vec![
            attempt(1, Some(1.0), true, Some(Rq1::Confident)),
            unchanged(2, 1.0),
        ],
        Some(1),
    )];
    let g = of(&runs, None);
    assert_eq!((g.attempts, g.filled, g.checked), (2, 1, 1));
    assert_eq!(g.no_fill["its_mask_is_the_one_before"], 1);
    assert_eq!(g.no_check["no_fill"], 1);
    assert!(g.no_shaped_mask.is_empty());
    // Its shaped mask exists, so it is compared: the same coverage.
    let t = &g.coverage_across_a_retry;
    assert_eq!((t.compared, t.coverage_within), (1, 1));
    // And it is checked nowhere.
    assert_eq!(g.rq1_against_under_coverage.checked, 1);
}

#[test]
fn a_failed_shape_or_fill_is_unavailable_for_the_measures_that_need_it() {
    let mut no_mask = attempt(2, None, false, None);
    no_mask.seen.unchecked = Some("could not make its mask: an error".into());
    let mut no_fill = attempt(2, Some(0.9), false, None);
    no_fill.seen.unchecked = Some("could not be filled: an error".into());
    let runs = [
        scored(
            "r1",
            vec![attempt(1, Some(0.5), true, Some(Rq1::Confident)), no_mask],
            Some(1),
        ),
        scored(
            "r2",
            vec![attempt(1, Some(0.5), true, Some(Rq1::Confident)), no_fill],
            Some(1),
        ),
    ];
    let g = of(&runs, None);
    assert_eq!(g.no_shaped_mask["its_mask_could_not_be_made"], 1);
    assert_eq!(g.no_fill["no_shaped_mask"], 1);
    assert_eq!(g.no_fill["the_fill_failed"], 1);
    assert_eq!(g.no_check["no_fill"], 2);
    // The retry without a shaped mask cannot be compared; the one with a mask can.
    let t = &g.coverage_across_a_retry;
    assert_eq!((t.compared, t.unavailable, t.coverage_up), (1, 1, 1));
    assert_eq!(g.rq1_against_under_coverage.checked, 2);
}

#[test]
fn a_first_attempt_that_fails_gives_no_picture_measures() {
    let mut failed = scored("r1", Vec::new(), None);
    failed.seen.status = "error".to_owned();
    failed.seen.reason = "no_attempt".to_owned();
    let marks = Marks::new();
    let g = of(&[failed], Some(&marks));
    assert_eq!((g.runs, g.runs_with_no_attempt, g.attempts), (1, 1, 0));
    assert_eq!(g.runs_by_outcome["error"], 1);
    assert_eq!(g.first_shaped_mask.attempts, 0);
    assert_eq!(g.first_shaped_mask.coverage_mean, None);
    let p = g.pictures.unwrap();
    assert_eq!((p.first_fills, p.kept_fills, p.transitions), (0, 0, 0));
}

fn mark(visible: bool, damaged: bool) -> Mark {
    Mark { visible, damaged }
}

#[test]
fn fills_one_and_three_around_an_unchanged_mask_retry_make_one_transition() {
    let runs = [scored(
        "r1",
        vec![
            attempt(1, Some(0.5), true, Some(Rq1::Confident)),
            unchanged(2, 0.5),
            attempt(3, Some(0.9), true, Some(Rq1::None)),
        ],
        Some(3),
    )];
    let marks: Marks = BTreeMap::from([
        (("r1".to_owned(), 1), mark(true, false)),
        (("r1".to_owned(), 3), mark(false, true)),
    ]);
    let p = of(&runs, Some(&marks)).pictures.unwrap();
    assert_eq!((p.transitions, p.visible_to_not), (1, 1));
    assert_eq!(
        (p.not_to_visible, p.stayed_visible, p.stayed_not_visible),
        (0, 0, 0)
    );
    assert_eq!((p.first_fills, p.first_visible, p.first_damaged), (1, 1, 0));
    assert_eq!((p.kept_fills, p.kept_visible, p.kept_damaged), (1, 0, 1));
    assert_eq!(p.unmarked_fills, 0);
}

#[test]
fn labels_add_the_visible_and_damaged_tables() {
    let runs = [
        scored(
            "r1",
            vec![
                attempt(1, Some(0.5), true, Some(Rq1::Confident)),
                attempt(2, Some(0.99), true, Some(Rq1::None)),
            ],
            Some(2),
        ),
        scored(
            "r2",
            vec![attempt(1, Some(0.99), true, Some(Rq1::None))],
            Some(1),
        ),
    ];
    let seen: Vec<RunSeen> = runs.iter().map(|r| r.seen.clone()).collect();
    // The marks file as the sheet saves it: by key, of the pictures looked at. The second
    // run's picture was not looked at.
    let seed = 7_u64;
    let key = |run: &str, n: u32| loop_score::key(seed, run, n);
    let assigned = |run: &str, n: u32| loop_score::Assigned {
        picture: format!("00{n}"),
        key: key(run, n),
        item: ITEM.to_owned(),
        run: run.to_owned(),
        attempt: n,
    };
    let assignment = loop_score::Assignment {
        seed,
        pictures: vec![assigned("r1", 1), assigned("r1", 2), assigned("r2", 1)],
    };
    let sheet = loop_score::sheet_id(seed);
    let file = serde_json::json!({"marks_version": 1, "sheet": sheet, "marks": [
        {"picture": key("r1", 1), "visible": true, "damaged": false},
        {"picture": key("r1", 2), "visible": false, "damaged": false},
    ]});
    let marks = loop_score::marks_of(file.to_string().as_bytes(), &assignment, &seen).unwrap();
    let g = of(&runs, Some(&marks));
    let p = g.pictures.clone().unwrap();
    assert_eq!((p.rq1_confident_visible, p.rq1_none_not_visible), (1, 1));
    assert_eq!((p.first_fills, p.first_marked, p.first_visible), (2, 1, 1));
    assert_eq!((p.kept_fills, p.kept_marked, p.kept_visible), (2, 1, 0));
    assert_eq!((p.transitions, p.visible_to_not), (1, 1));
    assert_eq!(p.unmarked_fills, 1);
    let text = loop_score::toml_text(&g).unwrap();
    assert!(text.contains("[pictures]"), "{text}");
    // Refused: a mark of no picture of the sheet; one picture marked twice; marks saved
    // from another sheet.
    let read = |marks: serde_json::Value, sheet: &str| {
        let file = serde_json::json!({"marks_version": 1, "sheet": sheet, "marks": marks});
        loop_score::marks_of(file.to_string().as_bytes(), &assignment, &seen)
    };
    let one = |run: &str, n: u32| serde_json::json!({"picture": key(run, n), "visible": true, "damaged": false});
    let stray = read(serde_json::json!([one("r9", 1)]), &sheet).unwrap_err();
    assert!(stray.contains("no completed fill"), "{stray}");
    let twice = read(serde_json::json!([one("r1", 1), one("r1", 1)]), &sheet).unwrap_err();
    assert!(twice.contains("twice"), "{twice}");
    let other = read(serde_json::json!([one("r1", 1)]), &loop_score::sheet_id(8)).unwrap_err();
    assert!(other.contains("another sheet"), "{other}");
}

#[test]
fn the_reasons_are_a_closed_set() {
    for (said, reason) in [
        ("attempt 2 passed", "passed"),
        (
            "the last attempt the plan allows",
            "last_attempt_of_the_plan",
        ),
        (
            "attempt 1 changed something that must be preserved, which painting more cannot mend",
            "damaged_something_preserved",
        ),
        (
            "the retry could not be filled, and the attempts before it stand",
            "retry_could_not_be_made",
        ),
        ("MARMALADE", "other"),
    ] {
        assert_eq!(loop_score::reason_of(said), reason, "{said}");
    }
}

/// `cli::…`: the command through the binary.
mod cli {
    use super::*;

    #[test]
    fn eval_loop_score_reads_a_scripted_loop_with_one_retry() {
        let scripted = scripted("cli-loop-score", vec![remains(), passes()]);
        let out = scripted.dir.path().join("score.toml");
        let ran = crate::wired::whizbang(
            &[
                "eval",
                "loop",
                "score",
                "--manifest",
                scripted.manifest.to_str().unwrap(),
                "--set",
                scripted.folder.to_str().unwrap(),
                "--judge",
                JUDGE,
                "--repetitions",
                "1",
                "--out",
                out.to_str().unwrap(),
            ],
            &scripted.world.root,
        );
        assert_eq!(ran.code, Some(0), "{}", ran.err);
        assert!(
            ran.out.contains(
                "requested: 1\nmissing: 0\nruns: 1\nattempts: 2\nchecked: 2\nfilled: 2\n\
                 retries compared: 1\n"
            ),
            "{}",
            ran.out
        );
        let text = std::fs::read_to_string(&out).unwrap();
        let read: toml::Value = toml::from_str(&text).unwrap();
        assert_eq!(read["schema"].as_str(), Some("loop-score/1"));
        assert_eq!(
            read["all"]["coverage_across_a_retry"]["coverage_up"].as_integer(),
            Some(1)
        );
        assert_eq!(read["group"]["whole"]["runs"].as_integer(), Some(1));
        assert!(!text.contains(ITEM) && !text.contains(&scripted.run));
        // Another judge's request ids hold no run.
        let other = scripted.dir.path().join("other.toml");
        let ran = crate::wired::whizbang(
            &[
                "eval",
                "loop",
                "score",
                "--manifest",
                scripted.manifest.to_str().unwrap(),
                "--set",
                scripted.folder.to_str().unwrap(),
                "--judge",
                "dear",
                "--repetitions",
                "1",
                "--out",
                other.to_str().unwrap(),
            ],
            &scripted.world.root,
        );
        assert_eq!(ran.code, Some(0), "{}", ran.err);
        assert!(
            ran.out.starts_with("requested: 1\nmissing: 1\nruns: 0\n"),
            "{}",
            ran.out
        );
    }
}

#[test]
fn the_plan_alone_is_said_and_nothing_runs() {
    // What `remove-checked --plan-only` prints, and what the evidence script reads before
    // its first paid run: the plan, with nothing sent, reserved or recorded.
    let mut setup = Setup::answering(vec![passes()]);
    setup.dear = true;
    let world = Looped::new("plan-only", &setup);
    let engine = world.engine();
    let asked = |cap_judge: &'static str| whizbang::door::RemoveCheckedAsked {
        image: &world.image,
        prompt: crate::checked::PHRASE,
        protect: None,
        backend: None,
        filler: None,
        judge: Some(cap_judge),
        attempts: None,
        check: None,
        request_id: None,
    };
    let said = whizbang::door::remove_checked_plan(&engine, &asked(JUDGE)).unwrap();
    assert!(
        said.starts_with("plan: 3 of 3 attempts asked, checked;"),
        "{said}"
    );
    assert_eq!(world.kit.requests(), (0, 0, 0));
    drop(engine);
    // No run is kept, and the allowance is whole: nothing was reserved.
    let reader = world.reader();
    let generation = reader.snapshot().unwrap();
    assert!(generation.runs().is_empty());
    let folded = whizbang_core::spend::fold::fold(&generation, |entry| reader.read(entry));
    assert_eq!(folded.balance().left.micros(), setup.allowance);
    // Under a cap that covers two attempts, the plan says so.
    let mut setup = Setup::answering(vec![passes()]);
    setup.dear = true;
    setup.cap = Some(format!("{:.6}", crate::checked::bound_of(2) as f64 / 1e6));
    let world = Looped::new("plan-only-cap", &setup);
    let engine = world.engine();
    let said = whizbang::door::remove_checked_plan(&engine, &asked(JUDGE)).unwrap();
    assert!(
        said.starts_with("plan: 2 of 3 attempts asked, checked;"),
        "{said}"
    );
}

#[test]
fn a_missing_repetition_is_requested_and_counted_missing() {
    let scripted = scripted("score-missing", vec![passes()]);
    // Two repetitions asked, one run: one requested run is missing, overall and in its group.
    let scoring = Scoring {
        repetitions: 2,
        ..scripted.scoring()
    };
    let score = loop_score::score(&scoring).unwrap();
    assert_eq!(
        (score.all.requested, score.all.missing, score.all.runs),
        (2, 1, 1)
    );
    let whole = &score.group[GROUP];
    assert_eq!((whole.requested, whole.missing), (2, 1));
    let text = score.toml().unwrap();
    assert!(
        text.contains("requested = 2") && text.contains("missing = 1"),
        "{text}"
    );
    assert_eq!(score.repetitions, 2);
    // Repetitions out of range are refused.
    let none = Scoring {
        repetitions: 0,
        ..scripted.scoring()
    };
    assert!(loop_score::score(&none).is_err());
}

#[test]
fn a_mask_of_another_size_is_unavailable_for_size_and_the_score_goes_on() {
    let scripted = scripted("score-size", vec![remains(), passes()]);
    // The truth mask the manifest pins is of 32 by 32, and the loop's masks of 64 by 64.
    let mut small = Vec::new();
    let mut rgba = Vec::new();
    for _ in 0..32 * 32 {
        rgba.extend_from_slice(&[255, 255, 255, 255]);
    }
    PngEncoder::new(&mut small)
        .write_image(&rgba, 32, 32, ExtendedColorType::Rgba8)
        .unwrap();
    let file = (scripted.folder.join("segmentation/segment-01/items"))
        .join(ITEM)
        .join("truth.png");
    let old = pin(&std::fs::read(&file).unwrap());
    std::fs::write(&file, &small).unwrap();
    let manifest = std::fs::read_to_string(&scripted.manifest).unwrap();
    std::fs::write(&scripted.manifest, manifest.replace(&old, &pin(&small))).unwrap();
    let score = loop_score::score(&scripted.scoring()).unwrap();
    let all = &score.all;
    assert_eq!(all.first_shaped_mask.attempts, 0);
    assert_eq!(all.first_shaped_mask.unavailable_by_reason["size"], 1);
    assert_eq!(all.kept_shaped_mask.unavailable_by_reason["size"], 1);
    assert_eq!(
        all.rq1_against_under_coverage.unavailable_by_reason["size"],
        2
    );
    assert_eq!(all.coverage_across_a_retry.unavailable_by_reason["size"], 1);
    assert_eq!(
        all.kept_had_the_highest_coverage.unavailable_by_reason["size"],
        1
    );
    // What needs no mask is still counted.
    assert_eq!(
        (all.runs, all.attempts, all.checked, all.filled),
        (1, 2, 2, 2)
    );
}

#[test]
fn a_truth_mask_with_no_object_pixel_is_refused() {
    let empty = png(Some([0, 0, 0, 0]));
    let refused = loop_score::truth_of(&empty).unwrap_err();
    assert!(refused.contains("no pixel of the object"), "{refused}");
    assert!(loop_score::truth_of(&png(Some(TRUTH))).is_ok());
}

#[test]
fn every_unavailable_has_a_reason_and_the_sensitivity_counts_have_shares() {
    let mut failed = scored("r2", Vec::new(), None);
    failed.seen.status = "error".to_owned();
    let runs = [
        scored(
            "r1",
            vec![
                attempt(1, Some(0.85), true, Some(Rq1::Confident)),
                attempt(2, None, false, None),
                attempt(3, Some(0.97), true, Some(Rq1::None)),
            ],
            Some(3),
        ),
        failed,
    ];
    let g = of(&runs, None);
    let first = &g.first_shaped_mask;
    assert_eq!((first.attempts, first.unavailable), (1, 1));
    assert_eq!(first.unavailable_by_reason["no_attempt"], 1);
    assert_eq!((first.under_090, first.under_090_share), (1, Some(1.0)));
    assert_eq!((first.under_099, first.under_099_share), (1, Some(1.0)));
    let kept = &g.kept_shaped_mask;
    assert_eq!((kept.under_099, kept.under_099_share), (1, Some(1.0)));
    assert_eq!((kept.under_090, kept.under_090_share), (0, Some(0.0)));
    // The run with no kept attempt is unavailable for every measure of the kept attempt.
    assert_eq!(kept.unavailable_by_reason["no_kept_attempt"], 1);
    let highest = &g.kept_had_the_highest_coverage;
    assert_eq!((highest.runs, highest.unavailable), (1, 1));
    assert_eq!(highest.unavailable_by_reason["no_kept_attempt"], 1);
    // The retry with no shaped mask, and the next compared with the last one there was.
    let retry = &g.coverage_across_a_retry;
    assert_eq!(
        (retry.compared, retry.coverage_up, retry.unavailable),
        (1, 1, 1)
    );
    assert_eq!(retry.unavailable_by_reason["its_mask_could_not_be_made"], 1);
    for map in [
        &first.unavailable_by_reason,
        &kept.unavailable_by_reason,
        &retry.unavailable_by_reason,
        &highest.unavailable_by_reason,
    ] {
        assert!(map.values().sum::<usize>() > 0);
    }
}
