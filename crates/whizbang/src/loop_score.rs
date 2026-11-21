//! `whizbang eval loop score` (plan e1, task L1, item 5): what the loop `remove_checked`
//! did on the pictures of `segmentation/segment-01`, scored against their hand-drawn truth
//! masks, from the store alone.
//!
//! **The manifest** (`labs/e-loop/segment-01.toml`, kind `segment-01`) names the set, the
//! repository and revision its pictures are pinned at, and each of its pictures: its id,
//! its group, its phrase, and the paths and SHA-256 of its picture and its truth mask,
//! copied from the set's own `set.toml`. [`Manifest::parse`] holds it to that.
//!
//! **Which runs.** A run of the loop under the judge named, whose request id is
//! `e-loop-<judge>-<repetition>-<item id>` ([`request_id`]), as `scripts/loop-evidence.sh`
//! makes them. Its attempts are read from its record, and each attempt's calls from the
//! records of the runs it lists: the find (`remove_prompt` until `select`), the shape
//! (from `grow` until `feather`), the fill (from `fill`) and the check (`source-fidelity`).
//! A located retry's found mask is the output of its own join; a wider retry's is the one
//! before it. Nothing is run, and nothing is written to the store.
//!
//! **The truth masks** are read from the folder of the pictures (`<folder>/<set>/<path>`),
//! each held to the SHA-256 its entry pins, through no link; they are never imported, and
//! never reach the loop. A pixel is of the object where its first channel is at least a
//! half.
//!
//! **Measures**, every one named for what it is: [`coverage`](crate::edits::coverage),
//! [`iou`](crate::edits::iou) and [`size_ratio`](crate::edits::size_ratio) of an attempt's
//! found mask and shaped mask against the truth; *under-covered* is coverage below
//! [`UNDER`], frozen in the plan before any run, with [`SENSITIVITY`] as sensitivity only.
//! They are statements about masks, not about the finished pictures: those come only from
//! a person's marks of the contact sheet (`--labels`, [`crate::loop_sheet`]).
//!
//! **Denominators.** Every requested run is counted, and every recorded attempt; each
//! measure counts only the attempts that have what it needs (a mask, a check, a completed
//! fill), and those that do not are counted as unavailable for it, by why.
//!
//! **What it writes** (`loop-score/1`) holds aggregates only, over all runs and by group:
//! no item, no run, no picture, no phrase and no word of a judge.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use whizbang_core::artifact::{self, ArtifactId, ArtifactType};
use whizbang_core::contract::record::{ArgumentValue, Changed, RunRecord, Status};
use whizbang_core::contract::{Confidence, Disposition};
use whizbang_core::identity::{DecodeLimits, Digest, identify};
use whizbang_core::store::Reader;
use whizbang_core::store::fs::RealFs;
use whizbang_eval::lab::{digest_of, is_digest};

use crate::harness;

/// The kind a manifest of this command names.
pub const KIND: &str = "segment-01";

/// What the file written is.
pub const SCHEMA: &str = "loop-score/1";

/// Coverage below this is under-covered (plan e1, "Words this plan uses"; frozen).
pub const UNDER: f64 = 0.95;

/// The two thresholds reported beside [`UNDER`], as sensitivity, never as the measure.
pub const SENSITIVITY: [f64; 2] = [0.90, 0.99];

/// A change in coverage of at most this much, either way, is "within".
pub const STEP: f64 = 0.01;

/// The most bytes of a manifest, or of a marks file, that are read.
pub const FILE_LIMIT: u64 = 256 * 1024;

/// The most bytes of one truth mask or picture read from the folder.
pub const PICTURE_LIMIT: u64 = crate::import::MAX_FILE_BYTES;

/// The most entries a manifest holds.
pub const ENTRIES_LIMIT: usize = 200;

/// The loop's action, and the actions its attempts call.
const LOOP: &str = whizbang_rascal::checked::NAME;
const REMOVE: &str = whizbang_rascal::checked::REMOVE;
const CHECK: &str = whizbang_rascal::checked::CHECK;

/// The manifest of the loop's pictures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub kind: String,
    /// The set's id, the folder its files are in under the pictures' folder.
    pub set: String,
    pub images_repo: String,
    pub images_rev: String,
    pub item: Vec<Entry>,
}

/// One picture of the manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub group: String,
    /// What the loop is asked to remove: the set's `prompt`.
    pub phrase: String,
    /// The picture the loop is given, under the set's folder.
    pub image: String,
    pub image_sha256: String,
    /// The hand-drawn truth mask, under the set's folder: read only here, by its pin.
    pub truth: String,
    pub truth_sha256: String,
}

/// A name: a lower-case letter or digit, then lower-case letters, digits and `-`, 64 at
/// most.
fn plain(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && (name.chars()).all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A path of plain parts under a folder.
fn relative(path: &str) -> bool {
    !path.is_empty()
        && path.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && (part.chars()).all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        })
}

impl Manifest {
    /// A manifest, read strictly and checked: its kind; a set of plain parts; the
    /// repository and a revision of 40 hex digits; each entry's id unique and plain, its
    /// group plain, its phrase one line, its picture an `image.png` and its truth a
    /// `truth.png` under the set's folder, each pinned by a SHA-256, and no picture's pin
    /// the pin of any truth mask.
    pub fn parse(text: &str) -> Result<Manifest, String> {
        let manifest: Manifest =
            toml::from_str(text).map_err(|e| format!("the loop's manifest: {e}"))?;
        if manifest.kind != KIND {
            return Err(format!("the loop's manifest is of the kind {KIND}"));
        }
        if !relative(&manifest.set) || !manifest.images_repo.starts_with("github.com/") {
            return Err("the loop's manifest names no set or repository".to_owned());
        }
        let rev = &manifest.images_rev;
        if rev.len() != 40 || !rev.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("the loop's manifest pins no revision of 40 hex digits".to_owned());
        }
        if manifest.item.is_empty() || manifest.item.len() > ENTRIES_LIMIT {
            return Err(format!(
                "the loop's manifest holds 1 to {ENTRIES_LIMIT} entries"
            ));
        }
        let truths: Vec<&str> = (manifest.item.iter())
            .map(|e| e.truth_sha256.as_str())
            .collect();
        let mut seen = std::collections::BTreeSet::new();
        for entry in &manifest.item {
            let named = |why: &str| format!("the loop's manifest: an entry {why}");
            if !plain(&entry.id) || !seen.insert(entry.id.as_str()) {
                return Err(named("has an id that is not plain, or that another has"));
            }
            if !plain(&entry.group) {
                return Err(named("has a group that is not plain"));
            }
            let phrase = &entry.phrase;
            if phrase.trim().is_empty() || phrase.chars().any(char::is_control) {
                return Err(named("has a phrase that is empty or not one line"));
            }
            let image_ok = relative(&entry.image) && entry.image.ends_with("/image.png");
            let truth_ok = relative(&entry.truth) && entry.truth.ends_with("/truth.png");
            if !image_ok || !truth_ok {
                return Err(named(
                    "does not name an image.png and a truth.png under the set's folder",
                ));
            }
            if !is_digest(&entry.image_sha256) || !is_digest(&entry.truth_sha256) {
                return Err(named("pins no SHA-256"));
            }
            if truths.contains(&entry.image_sha256.as_str()) {
                return Err(named("names a truth mask as its picture"));
            }
        }
        Ok(manifest)
    }

    /// A manifest file, at most [`FILE_LIMIT`] bytes, and its SHA-256.
    pub fn read(file: &Path) -> Result<(Manifest, String), String> {
        let bytes = read_limited(file, "the loop's manifest")?;
        let text = std::str::from_utf8(&bytes).map_err(|_| "the loop's manifest is not UTF-8")?;
        Ok((Manifest::parse(text)?, digest_of(&bytes)))
    }

    /// Each group and how many entries it has.
    pub fn groups(&self) -> BTreeMap<String, usize> {
        let mut out = BTreeMap::new();
        for entry in &self.item {
            *out.entry(entry.group.clone()).or_default() += 1;
        }
        out
    }

    /// The bytes of a file of an entry under the pictures' folder, held to its pin.
    pub fn pinned(&self, folder: &Path, path: &str, sha256: &str) -> Result<Vec<u8>, String> {
        let at = format!("{}/{path}", self.set);
        let bytes = whizbang_eval::lab::read_relative(folder, &at, PICTURE_LIMIT)?;
        if digest_of(&bytes) != sha256 {
            return Err(format!(
                "{at}: not the file the loop's manifest pins (another SHA-256)"
            ));
        }
        Ok(bytes)
    }
}

pub(crate) fn read_limited(file: &Path, what: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let opened = std::fs::File::open(file).map_err(|e| format!("{what}: {}", e.kind()))?;
    let mut bytes = Vec::new();
    (opened.take(FILE_LIMIT + 1).read_to_end(&mut bytes))
        .map_err(|e| format!("{what}: {}", e.kind()))?;
    if bytes.len() as u64 > FILE_LIMIT {
        return Err(format!("{what} is at most {FILE_LIMIT} bytes"));
    }
    Ok(bytes)
}

/// The request id of a run of the loop: `e-loop-<judge>-<repetition>-<item id>`.
pub fn request_id(judge: &str, repetition: u32, id: &str) -> String {
    format!("e-loop-{judge}-{repetition}-{id}")
}

/// The repetition and the item a request id is of, under this judge.
fn requested<'a>(request: &'a str, judge: &str) -> Option<(u32, &'a str)> {
    let rest = request.strip_prefix(&format!("e-loop-{judge}-"))?;
    let (repetition, id) = rest.split_once('-')?;
    let plain_number = !repetition.is_empty() && repetition.bytes().all(|b| b.is_ascii_digit());
    let repetition: u32 = repetition.parse().ok().filter(|_| plain_number)?;
    (repetition > 0).then_some((repetition, id))
}

// --- what the store holds ---------------------------------------------------------------------

/// The judge's answer about `RQ1`, the named thing still visible, in a check's findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rq1 {
    /// A confident violation: the named thing is still there.
    Confident,
    /// A tentative violation.
    Tentative,
    /// No violation of `RQ1`.
    None,
}

/// What a record keeps of a check's findings, read for `RQ1`.
pub fn rq1_of(found: &whizbang_core::contract::record::Found) -> Rq1 {
    let violations = (found.counts.iter())
        .filter(|c| c.obligation == "RQ1" && c.disposition == Disposition::Violation);
    let mut answer = Rq1::None;
    for count in violations {
        answer = answer.min(match count.confidence {
            Some(Confidence::Confident) => Rq1::Confident,
            _ => Rq1::Tentative,
        });
    }
    answer
}

/// One attempt of a run, as its record and its calls' records say it.
#[derive(Debug, Clone, PartialEq)]
pub struct AttemptSeen {
    pub number: u32,
    /// The found mask the attempt shaped: its own, or the one before it for a wider retry.
    pub found: Option<ArtifactId>,
    pub shaped: Option<ArtifactId>,
    /// The picture its fill completed with.
    pub filled: Option<ArtifactId>,
    /// The judge's answer about `RQ1`, when the attempt was checked.
    pub rq1: Option<Rq1>,
    /// Why it has no verdict, when it has none.
    pub unchecked: Option<String>,
}

/// One run of the loop under the manifest's request ids.
#[derive(Debug, Clone, PartialEq)]
pub struct RunSeen {
    pub run: String,
    pub item: String,
    pub group: String,
    pub repetition: u32,
    /// `completed`, `error`, `invalid` or `cancelled`.
    pub status: String,
    pub attempts: Vec<AttemptSeen>,
    pub kept: Option<u32>,
    /// Why its attempts ended, as one of a closed set ([`reason_of`]).
    pub reason: String,
}

fn text_argument<'a>(record: &'a RunRecord, name: &str) -> Option<&'a str> {
    (record.asked().arguments.iter()).find_map(|a| match &a.value {
        ArgumentValue::Text(text) if a.name == name => Some(text.as_str()),
        _ => None,
    })
}

fn output_of(record: &RunRecord, port: &str) -> Option<ArtifactId> {
    let completed = record.outcome().status == Status::Completed;
    let found = record.outputs().iter().find(|o| o.port == port);
    found.filter(|_| completed).map(|o| ArtifactId(o.artifact))
}

/// Why a loop's attempts ended, as one of a closed set: each sentence the loop writes,
/// with every number made `N`, is one of these or `other` (as `scripts/loop-evidence.py`
/// reads them). A sentence never reaches the file.
pub fn reason_of(sentence: &str) -> &'static str {
    let plain: String = (sentence.chars())
        .map(|c| if c.is_ascii_digit() { 'N' } else { c })
        .collect();
    let mut squeezed = String::new();
    for c in plain.chars() {
        if !(c == 'N' && squeezed.ends_with('N')) {
            squeezed.push(c);
        }
    }
    let s = squeezed.as_str();
    let starts = |prefix: &str| s.starts_with(prefix);
    if s == "attempt N passed" {
        "passed"
    } else if starts("the findings of attempt N were cut short") {
        "findings_cut_short"
    } else if starts("attempt N changed something that must be preserved") {
        "damaged_something_preserved"
    } else if starts("no finding of attempt N says with confidence") {
        "nothing_says_it_remains"
    } else if s == "the last attempt the plan allows" {
        "last_attempt_of_the_plan"
    } else if s == "a wider mask was already used once" {
        "wider_mask_already_used"
    } else if starts("the mask of attempt N is the one before it") {
        "mask_is_the_one_before"
    } else if starts("one unchecked pass: ") {
        "one_unchecked_pass"
    } else if s == "the check of attempt N could not run" {
        "check_could_not_run"
    } else if s == "the budget ran out after attempt N" {
        "budget_ran_out"
    } else if s == "the deadline passed after attempt N" {
        "deadline_passed"
    } else if starts("the retry ") && s.ends_with("and the attempts before it stand") {
        "retry_could_not_be_made"
    } else if s == whizbang_core::contract::record::ATTEMPTS_NOT_ENDED {
        "never_said"
    } else {
        "other"
    }
}

/// Every run of the loop the store at `root` holds under the manifest's request ids for
/// this judge, each with its attempts as its calls' records say them, in the order of
/// the manifest's entries, then of the repetitions. A run under one of those ids asked
/// of another judge is refused.
pub fn runs_of(root: &Path, manifest: &Manifest, judge: &str) -> Result<Vec<RunSeen>, String> {
    let mut records: BTreeMap<String, RunRecord> = BTreeMap::new();
    harness::each_run(root, &mut |record, id, _| {
        records.insert(id.to_owned(), record.clone());
    })?;
    let mut found = Vec::new();
    for (id, record) in &records {
        if record.asked().action.as_deref() != Some(LOOP) {
            continue;
        }
        let Some(request) = record.identity().request.as_deref() else {
            continue;
        };
        let Some((repetition, item)) = requested(request, judge) else {
            continue;
        };
        let Some(entry) = manifest.item.iter().find(|e| e.id == item) else {
            continue;
        };
        if text_argument(record, "judge") != Some(judge) {
            return Err(
                "a run under this judge's request ids was asked of another judge".to_owned(),
            );
        }
        let mut attempts = Vec::new();
        let mut found_before: Option<ArtifactId> = None;
        let listed = record.attempts().map(|a| a.list.as_slice()).unwrap_or(&[]);
        for attempt in listed {
            let mut seen = AttemptSeen {
                number: attempt.attempt,
                found: None,
                shaped: None,
                filled: None,
                rq1: None,
                unchecked: attempt.unchecked.clone(),
            };
            for run in &attempt.runs {
                let Some(call) = records.get(run) else {
                    continue;
                };
                match call.asked().action.as_deref() {
                    Some(REMOVE) => {
                        let from = text_argument(call, "from");
                        let until = text_argument(call, "until");
                        match (from, until) {
                            (None | Some("select"), Some("select")) => {
                                seen.found = output_of(call, "until_select_mask");
                            }
                            (Some("grow"), Some("feather")) => {
                                seen.shaped = output_of(call, "until_feather_mask");
                            }
                            (Some("fill"), None) => seen.filled = output_of(call, "image"),
                            _ => {}
                        }
                    }
                    Some(CHECK) if attempt.verdict.is_some() => {
                        seen.rq1 = Some(call.findings().map_or(Rq1::None, rq1_of));
                    }
                    _ => {}
                }
            }
            // A wider retry shapes the found mask of the attempt before it; so does a
            // located one that joined nothing.
            let wider = matches!(attempt.changed, Some(Changed::Wider { .. }));
            let shaped_again = attempt.attempt > 1 && seen.shaped.is_some();
            if seen.found.is_none() && (wider || shaped_again) {
                seen.found = found_before;
            }
            if seen.found.is_some() {
                found_before = seen.found;
            }
            attempts.push(seen);
        }
        let ended = record.attempts().map(|a| a.ended.as_str()).unwrap_or("");
        found.push(RunSeen {
            run: id.clone(),
            item: entry.id.clone(),
            group: entry.group.clone(),
            repetition,
            status: record.outcome().status.name().to_owned(),
            attempts,
            kept: record.attempts().and_then(|a| a.kept),
            reason: if listed.is_empty() {
                "no_attempt".to_owned()
            } else {
                reason_of(ended).to_owned()
            },
        });
    }
    let order = |run: &RunSeen| {
        let at = manifest.item.iter().position(|e| e.id == run.item);
        (at, run.repetition)
    };
    found.sort_by_key(order);
    Ok(found)
}

// --- the measures -----------------------------------------------------------------------------

/// What a mask is against the truth.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Against {
    pub coverage: f64,
    pub iou: f64,
    pub size: f64,
}

/// A mask of the store against a truth mask of the same size.
pub fn against(reader: &Reader, mask: ArtifactId, truth: &Truth) -> Result<Against, String> {
    let limits = DecodeLimits::default();
    let read = artifact::get_anywhere(reader, ArtifactType::Mask, mask, &limits);
    let mask = (read.map_err(|e| e.to_string())?)
        .into_mask()
        .map_err(|e| e.to_string())?;
    let space = mask.space();
    if (space.width(), space.height()) != (truth.width, truth.height) {
        return Err("a mask of the loop is not the size of its picture's truth mask".to_owned());
    }
    let mut weights = Vec::with_capacity(truth.pixels.len());
    for y in 0..space.height() {
        for x in 0..space.width() {
            weights.push(mask.weight(x, y).unwrap_or(0.0));
        }
    }
    Ok(Against {
        coverage: crate::edits::coverage(&truth.pixels, &weights)?,
        iou: crate::edits::iou(&truth.pixels, &weights)?,
        size: crate::edits::size_ratio(&truth.pixels, &weights)?,
    })
}

/// A truth mask: its size, and which pixels are of the object.
#[derive(Debug, Clone, PartialEq)]
pub struct Truth {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<bool>,
}

/// A truth mask from its file's bytes: a pixel is of the object where its first channel
/// is at least a half.
pub fn truth_of(bytes: &[u8]) -> Result<Truth, String> {
    let read = identify(bytes, &DecodeLimits::default()).map_err(|e| e.to_string())?;
    let pixels = read.pixels;
    Ok(Truth {
        width: pixels.width(),
        height: pixels.height(),
        pixels: pixels.rgba().chunks(4).map(|p| p[0] >= 128).collect(),
    })
}

/// One attempt with its masks measured.
#[derive(Debug, Clone, PartialEq)]
pub struct AttemptScored {
    pub seen: AttemptSeen,
    pub found: Option<Against>,
    pub shaped: Option<Against>,
}

/// One run with its attempts measured.
#[derive(Debug, Clone, PartialEq)]
pub struct RunScored {
    pub seen: RunSeen,
    pub attempts: Vec<AttemptScored>,
}

/// A person's marks of one completed fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mark {
    /// The named thing is still visible.
    pub visible: bool,
    /// Something else was damaged.
    pub damaged: bool,
}

/// A person's marks, by the run and the attempt of each fill.
pub type Marks = BTreeMap<(String, u32), Mark>;

/// The measures of a mask over attempts: how many had it, and its coverage, IoU and size.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct MaskStats {
    /// The attempts measured: those whose mask exists.
    pub attempts: usize,
    /// Those of the attempts asked about whose mask does not exist.
    pub unavailable: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage_mean: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage_least: Option<f64>,
    /// Coverage below [`UNDER`]: the measure.
    pub under_covered: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub under_covered_share: Option<f64>,
    /// Coverage below 0.90 and 0.99: sensitivity only.
    pub under_090: usize,
    pub under_099: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iou_mean: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_mean: Option<f64>,
}

impl MaskStats {
    fn of(masks: &[Option<Against>]) -> MaskStats {
        let measured: Vec<Against> = masks.iter().flatten().copied().collect();
        let n = measured.len();
        let mean = |f: fn(&Against) -> f64| {
            (n > 0).then(|| measured.iter().map(f).sum::<f64>() / n as f64)
        };
        let under = |at: f64| measured.iter().filter(|a| a.coverage < at).count();
        MaskStats {
            attempts: n,
            unavailable: masks.len() - n,
            coverage_mean: mean(|a| a.coverage),
            coverage_least: (measured.iter().map(|a| a.coverage)).reduce(f64::min),
            under_covered: under(UNDER),
            under_covered_share: (n > 0).then(|| under(UNDER) as f64 / n as f64),
            under_090: under(SENSITIVITY[0]),
            under_099: under(SENSITIVITY[1]),
            iou_mean: mean(|a| a.iou),
            size_mean: mean(|a| a.size),
        }
    }
}

/// The judge's answer about `RQ1` against mask under-coverage, over checked attempts.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Rq1Table {
    /// Checked attempts whose shaped mask exists: the denominator.
    pub checked: usize,
    pub confident_violation_under_covered: usize,
    pub confident_violation_covered: usize,
    pub tentative_violation_under_covered: usize,
    pub tentative_violation_covered: usize,
    pub no_violation_under_covered: usize,
    pub no_violation_covered: usize,
}

/// Mask coverage across a retry: each attempt after the first whose shaped mask exists,
/// against the last earlier attempt whose shaped mask exists.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RetryTable {
    /// The retries compared: the denominator.
    pub compared: usize,
    pub coverage_up: usize,
    pub coverage_within: usize,
    pub coverage_down: usize,
    /// The shaped mask's size, as a multiple of the truth, against the one before.
    pub size_larger: usize,
    pub size_within: usize,
    pub size_smaller: usize,
    /// Retries with no shaped mask, or no earlier one to compare with.
    pub unavailable: usize,
}

/// Whether the kept attempt had the highest shaped-mask coverage of its run.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct KeptTable {
    /// Runs with a kept attempt whose shaped mask exists: the denominator.
    pub runs: usize,
    pub highest: usize,
    pub not_highest: usize,
}

/// What a person's marks say, beside the masks.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Pictures {
    /// The judge's `RQ1` answer against "still visible", over checked attempts with a
    /// completed fill that was marked.
    pub rq1_confident_visible: usize,
    pub rq1_confident_not_visible: usize,
    pub rq1_tentative_visible: usize,
    pub rq1_tentative_not_visible: usize,
    pub rq1_none_visible: usize,
    pub rq1_none_not_visible: usize,
    /// The first completed fill of each run: how many, how many marked, and of those.
    pub first_fills: usize,
    pub first_marked: usize,
    pub first_visible: usize,
    pub first_damaged: usize,
    /// The kept attempt's fill of each run.
    pub kept_fills: usize,
    pub kept_marked: usize,
    pub kept_visible: usize,
    pub kept_damaged: usize,
    /// From one completed fill of a run to the next.
    pub transitions: usize,
    pub visible_to_not: usize,
    pub not_to_visible: usize,
    pub stayed_visible: usize,
    pub stayed_not_visible: usize,
    /// Fills of the above with no mark, counted once each.
    pub unmarked_fills: usize,
}

/// The measures of a group of runs, or of all of them.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Group {
    /// Runs of the loop found under the request ids: every requested run that ran.
    pub runs: usize,
    pub runs_by_outcome: BTreeMap<String, usize>,
    /// Runs that recorded no attempt: a first find, shape or fill that failed.
    pub runs_with_no_attempt: usize,
    pub attempts: usize,
    /// Attempts with a completed fill, and with a check.
    pub filled: usize,
    pub checked: usize,
    /// Attempts without each artefact, by why.
    pub no_shaped_mask: BTreeMap<String, usize>,
    pub no_fill: BTreeMap<String, usize>,
    pub no_check: BTreeMap<String, usize>,
    pub first_found_mask: MaskStats,
    pub first_shaped_mask: MaskStats,
    pub kept_found_mask: MaskStats,
    pub kept_shaped_mask: MaskStats,
    pub rq1_against_under_coverage: Rq1Table,
    pub coverage_across_a_retry: RetryTable,
    /// Runs that ended with the kept attempt's shaped mask under-covered, by why the
    /// attempts ended; and how many such runs.
    pub ended_under_covered: usize,
    pub ended_under_covered_by_reason: BTreeMap<String, usize>,
    pub kept_had_the_highest_coverage: KeptTable,
    /// Only with a person's marks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pictures: Option<Pictures>,
}

/// The file `eval loop score` writes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Score {
    pub schema: String,
    pub judge: String,
    pub set: String,
    pub images_rev: String,
    pub manifest_sha256: String,
    pub entries: usize,
    pub under_covered_below: f64,
    pub sensitivity: Vec<f64>,
    /// Whether a person's marks were read; without them nothing is said of the pictures.
    pub marked: bool,
    pub all: Group,
    pub group: BTreeMap<String, Group>,
}

fn why_no_shaped(attempt: &AttemptSeen) -> &'static str {
    match attempt.unchecked.as_deref() {
        Some(why) if why.starts_with("nothing was found in the places named") => {
            "nothing_joined_in_the_places"
        }
        _ => "its_mask_could_not_be_made",
    }
}

fn why_no_fill(attempt: &AttemptSeen) -> &'static str {
    match (attempt.shaped, attempt.unchecked.as_deref()) {
        (None, _) => "no_shaped_mask",
        (Some(_), Some(why)) if why.starts_with("its shaped mask is the one before it") => {
            "its_mask_is_the_one_before"
        }
        _ => "the_fill_failed",
    }
}

fn why_no_check(attempt: &AttemptSeen) -> &'static str {
    match attempt.filled {
        None => "no_fill",
        Some(_) => "the_check_was_not_made_or_could_not_run",
    }
}

fn add(map: &mut BTreeMap<String, usize>, key: &str) {
    *map.entry(key.to_owned()).or_default() += 1;
}

/// The measures over these runs; with marks, what they say of the pictures too.
pub fn group(runs: &[&RunScored], marks: Option<&Marks>) -> Group {
    let mut g = Group {
        runs: runs.len(),
        ..Group::default()
    };
    let (mut first_found, mut first_shaped, mut kept_found, mut kept_shaped) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut pictures = Pictures::default();
    for run in runs {
        add(&mut g.runs_by_outcome, &run.seen.status);
        if run.attempts.is_empty() {
            g.runs_with_no_attempt += 1;
        }
        g.attempts += run.attempts.len();
        if let Some(first) = run.attempts.first() {
            first_found.push(first.found);
            first_shaped.push(first.shaped);
        }
        let kept = (run.seen.kept).and_then(|k| run.attempts.iter().find(|a| a.seen.number == k));
        if let Some(kept) = kept {
            kept_found.push(kept.found);
            kept_shaped.push(kept.shaped);
        }
        let mut before: Option<Against> = None;
        for (at, attempt) in run.attempts.iter().enumerate() {
            let seen = &attempt.seen;
            if seen.shaped.is_none() {
                add(&mut g.no_shaped_mask, why_no_shaped(seen));
            }
            match seen.filled {
                Some(_) => g.filled += 1,
                None => add(&mut g.no_fill, why_no_fill(seen)),
            }
            match seen.rq1 {
                Some(_) => g.checked += 1,
                None => add(&mut g.no_check, why_no_check(seen)),
            }
            if let (Some(rq1), Some(shaped)) = (seen.rq1, attempt.shaped) {
                let t = &mut g.rq1_against_under_coverage;
                t.checked += 1;
                let under = shaped.coverage < UNDER;
                match (rq1, under) {
                    (Rq1::Confident, true) => t.confident_violation_under_covered += 1,
                    (Rq1::Confident, false) => t.confident_violation_covered += 1,
                    (Rq1::Tentative, true) => t.tentative_violation_under_covered += 1,
                    (Rq1::Tentative, false) => t.tentative_violation_covered += 1,
                    (Rq1::None, true) => t.no_violation_under_covered += 1,
                    (Rq1::None, false) => t.no_violation_covered += 1,
                }
            }
            if at > 0 {
                let t = &mut g.coverage_across_a_retry;
                match (attempt.shaped, before) {
                    (Some(now), Some(then)) => {
                        t.compared += 1;
                        let change = now.coverage - then.coverage;
                        if change > STEP {
                            t.coverage_up += 1;
                        } else if change < -STEP {
                            t.coverage_down += 1;
                        } else {
                            t.coverage_within += 1;
                        }
                        let size = now.size - then.size;
                        if size > STEP * then.size.max(f64::MIN_POSITIVE) {
                            t.size_larger += 1;
                        } else if size < -STEP * then.size.max(f64::MIN_POSITIVE) {
                            t.size_smaller += 1;
                        } else {
                            t.size_within += 1;
                        }
                    }
                    _ => t.unavailable += 1,
                }
            }
            if attempt.shaped.is_some() {
                before = attempt.shaped;
            }
        }
        if let Some(shaped) = kept.and_then(|k| k.shaped) {
            if shaped.coverage < UNDER {
                g.ended_under_covered += 1;
                add(&mut g.ended_under_covered_by_reason, &run.seen.reason);
            }
            let best = (run.attempts.iter())
                .filter_map(|a| a.shaped.map(|s| s.coverage))
                .fold(f64::MIN, f64::max);
            let t = &mut g.kept_had_the_highest_coverage;
            t.runs += 1;
            if shaped.coverage >= best {
                t.highest += 1;
            } else {
                t.not_highest += 1;
            }
        }
        if let Some(marks) = marks {
            pictures_of(run, marks, &mut pictures);
        }
    }
    g.first_found_mask = MaskStats::of(&first_found);
    g.first_shaped_mask = MaskStats::of(&first_shaped);
    g.kept_found_mask = MaskStats::of(&kept_found);
    g.kept_shaped_mask = MaskStats::of(&kept_shaped);
    g.pictures = marks.map(|_| pictures);
    g
}

fn pictures_of(run: &RunScored, marks: &Marks, p: &mut Pictures) {
    let mark = |number: u32| marks.get(&(run.seen.run.clone(), number)).copied();
    let fills: Vec<&AttemptScored> = (run.attempts.iter())
        .filter(|a| a.seen.filled.is_some())
        .collect();
    let mut unmarked = std::collections::BTreeSet::new();
    for attempt in &fills {
        let number = attempt.seen.number;
        if mark(number).is_none() {
            unmarked.insert(number);
        }
        if let (Some(rq1), Some(m)) = (attempt.seen.rq1, mark(number)) {
            match (rq1, m.visible) {
                (Rq1::Confident, true) => p.rq1_confident_visible += 1,
                (Rq1::Confident, false) => p.rq1_confident_not_visible += 1,
                (Rq1::Tentative, true) => p.rq1_tentative_visible += 1,
                (Rq1::Tentative, false) => p.rq1_tentative_not_visible += 1,
                (Rq1::None, true) => p.rq1_none_visible += 1,
                (Rq1::None, false) => p.rq1_none_not_visible += 1,
            }
        }
    }
    if let Some(first) = fills.first() {
        p.first_fills += 1;
        if let Some(m) = mark(first.seen.number) {
            p.first_marked += 1;
            p.first_visible += usize::from(m.visible);
            p.first_damaged += usize::from(m.damaged);
        }
    }
    let kept = (run.seen.kept).and_then(|k| fills.iter().find(|a| a.seen.number == k));
    if let Some(kept) = kept {
        p.kept_fills += 1;
        if let Some(m) = mark(kept.seen.number) {
            p.kept_marked += 1;
            p.kept_visible += usize::from(m.visible);
            p.kept_damaged += usize::from(m.damaged);
        }
    }
    for pair in fills.windows(2) {
        let (Some(a), Some(b)) = (mark(pair[0].seen.number), mark(pair[1].seen.number)) else {
            continue;
        };
        p.transitions += 1;
        match (a.visible, b.visible) {
            (true, false) => p.visible_to_not += 1,
            (false, true) => p.not_to_visible += 1,
            (true, true) => p.stayed_visible += 1,
            (false, false) => p.stayed_not_visible += 1,
        }
    }
    p.unmarked_fills += unmarked.len();
}

// --- the command ------------------------------------------------------------------------------

/// What `eval loop score` reads.
pub struct Scoring<'a> {
    pub manifest: &'a Path,
    /// The folder of the pictures, laid out as their repository is.
    pub set: &'a Path,
    pub root: &'a Path,
    pub judge: &'a str,
    /// A person's marks of the contact sheet, a file outside the repository.
    pub labels: Option<&'a Path>,
}

/// The key a picture of the contact sheet goes by: the first 16 hex digits of the
/// SHA-256 of the sheet's seed, the run and the attempt. It names neither, so the page
/// that shows it says nothing of either, and a marks file is read back by it.
pub fn key(seed: u64, run: &str, attempt: u32) -> String {
    let hex = Digest::of_bytes(format!("{seed}/{run}/{attempt}").as_bytes()).hex();
    hex.chars().take(16).collect()
}

/// A marks file as the contact sheet saves it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarksFile {
    pub marks_version: u32,
    pub seed: u64,
    pub marks: Vec<MarkOf>,
}

/// One picture's marks, by its key.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkOf {
    pub picture: String,
    pub visible: bool,
    pub damaged: bool,
}

/// The marks of a saved file, by the run and attempt of each picture of these runs; a
/// mark whose key is of no completed fill of them is refused.
pub fn marks_of(bytes: &[u8], runs: &[RunSeen]) -> Result<Marks, String> {
    let file: MarksFile =
        serde_json::from_slice(bytes).map_err(|_| "the marks file is not one the sheet saves")?;
    if file.marks_version != 1 {
        return Err("the marks file is of another version".to_owned());
    }
    let mut by_key = BTreeMap::new();
    for run in runs {
        for attempt in run.attempts.iter().filter(|a| a.filled.is_some()) {
            let at = (run.run.clone(), attempt.number);
            by_key.insert(key(file.seed, &run.run, attempt.number), at);
        }
    }
    let mut out = Marks::new();
    for mark in &file.marks {
        let Some(at) = by_key.get(&mark.picture) else {
            return Err(
                "the marks file marks a picture that is no completed fill of these runs".to_owned(),
            );
        };
        out.insert(
            at.clone(),
            Mark {
                visible: mark.visible,
                damaged: mark.damaged,
            },
        );
    }
    Ok(out)
}

/// Scores the loop's runs (the module's description says what).
pub fn score(scoring: &Scoring<'_>) -> Result<Score, String> {
    let (manifest, digest) = Manifest::read(scoring.manifest)?;
    let seen = runs_of(scoring.root, &manifest, scoring.judge)?;
    let reader = Reader::open(scoring.root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let mut truths: BTreeMap<String, Truth> = BTreeMap::new();
    let mut scored = Vec::new();
    for run in &seen {
        let entry = (manifest.item.iter())
            .find(|e| e.id == run.item)
            .ok_or("a run is of no entry")?;
        if !truths.contains_key(&entry.id) {
            let bytes = manifest.pinned(scoring.set, &entry.truth, &entry.truth_sha256)?;
            truths.insert(entry.id.clone(), truth_of(&bytes)?);
        }
        let truth = &truths[&entry.id];
        let measure = |mask: Option<ArtifactId>| -> Result<Option<Against>, String> {
            mask.map(|m| against(&reader, m, truth)).transpose()
        };
        let mut attempts = Vec::new();
        for attempt in &run.attempts {
            attempts.push(AttemptScored {
                found: measure(attempt.found)?,
                shaped: measure(attempt.shaped)?,
                seen: attempt.clone(),
            });
        }
        scored.push(RunScored {
            seen: run.clone(),
            attempts,
        });
    }
    let marks = match scoring.labels {
        Some(file) => Some(marks_of(&read_limited(file, "the marks file")?, &seen)?),
        None => None,
    };
    let all: Vec<&RunScored> = scored.iter().collect();
    let mut groups = BTreeMap::new();
    for name in manifest.groups().keys() {
        let of: Vec<&RunScored> = (scored.iter()).filter(|r| &r.seen.group == name).collect();
        groups.insert(name.clone(), group(&of, marks.as_ref()));
    }
    Ok(Score {
        schema: SCHEMA.to_owned(),
        judge: scoring.judge.to_owned(),
        set: manifest.set.clone(),
        images_rev: manifest.images_rev.clone(),
        manifest_sha256: digest,
        entries: manifest.item.len(),
        under_covered_below: UNDER,
        sensitivity: SENSITIVITY.to_vec(),
        marked: marks.is_some(),
        all: group(&all, marks.as_ref()),
        group: groups,
    })
}

/// A value as TOML: its scalars and lists of scalars first, then each table under a
/// dotted heading, keys in order. What this module writes is only maps, numbers, strings
/// of plain characters, booleans and lists of numbers.
pub fn toml_text<T: Serialize>(value: &T) -> Result<String, String> {
    let json = serde_json::to_value(value).map_err(|e| e.to_string())?;
    let mut out = String::new();
    table(&mut out, &[], &json)?;
    Ok(out)
}

fn scalar(value: &serde_json::Value) -> Result<Option<String>, String> {
    use serde_json::Value as J;
    Ok(match value {
        J::Bool(b) => Some(b.to_string()),
        J::Number(n) => Some(n.to_string()),
        J::String(s) if !s.chars().any(|c| c == '"' || c == '\\' || c.is_control()) => {
            Some(format!("\"{s}\""))
        }
        J::Array(items) => {
            let mut parts = Vec::new();
            for item in items {
                parts.push(scalar(item)?.ok_or("a list of tables cannot be written")?);
            }
            Some(format!("[{}]", parts.join(", ")))
        }
        J::Object(_) => None,
        _ => return Err("a value cannot be written as TOML".to_owned()),
    })
}

fn table(out: &mut String, path: &[String], value: &serde_json::Value) -> Result<(), String> {
    let serde_json::Value::Object(map) = value else {
        return Err("a table is not a map".to_owned());
    };
    let key = |k: &str| {
        let bare = !k.is_empty()
            && k.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if bare {
            k.to_owned()
        } else {
            format!("\"{k}\"")
        }
    };
    if !path.is_empty() {
        let heading: Vec<String> = path.iter().map(|k| key(k)).collect();
        out.push_str(&format!("\n[{}]\n", heading.join(".")));
    }
    for (k, v) in map {
        if let Some(text) = scalar(v)? {
            out.push_str(&format!("{} = {text}\n", key(k)));
        }
    }
    for (k, v) in map {
        if v.is_object() {
            let mut deeper = path.to_vec();
            deeper.push(k.clone());
            table(out, &deeper, v)?;
        }
    }
    Ok(())
}

impl Score {
    /// The file, as TOML, with a heading that says what it is and is not.
    pub fn toml(&self) -> Result<String, String> {
        let body = toml_text(self)?;
        Ok(format!(
            "# What the loop `remove_checked` did on segment-01 (plan e1, task L1, item 5),\n\
             # written by `whizbang eval loop score`. Aggregates only: no item, no run, no\n\
             # picture and no word of a judge. Every mask measure is about the mask, not the\n\
             # finished picture; `pictures` is there only when a person's marks were read.\n\
             {body}"
        ))
    }

    /// Writes the file whole, to a new file beside `out` first, then moved into place.
    pub fn write(&self, out: &Path) -> Result<(), String> {
        crate::gating::write_new(out, self.toml()?.as_bytes())
    }

    /// What the command prints: counts only.
    pub fn text(&self) -> String {
        let g = &self.all;
        format!(
            "runs: {}\nattempts: {}\nchecked: {}\nfilled: {}\nretries compared: {}\n\
             ended under-covered: {}\nmarks read: {}\n",
            g.runs,
            g.attempts,
            g.checked,
            g.filled,
            g.coverage_across_a_retry.compared,
            g.ended_under_covered,
            if self.marked { "yes" } else { "no" }
        )
    }
}
