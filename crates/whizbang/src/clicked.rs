//! `whizbang eval clicked` (plan mE, task C1, items 1 and 2): whether a clicked removal's
//! mask, shaped as `remove` shapes a location's, covers the thing better than the mask
//! `segment` gives for the click, on the pictures of `segmentation/segment-01`.
//!
//! **The pictures** are the loop's manifest's (`labs/e-loop/segment-01.toml`,
//! [`crate::loop_score::Manifest`]): each picture and its truth mask read from the folder
//! by the pin its entry holds. The picture is brought into the store; the truth mask is
//! never imported and reaches no action.
//!
//! **The click** is one point inside the truth mask, chosen the same way every time
//! ([`click_of`]): the pixel of the object farthest from everything that is not the object,
//! by city-block distance, the picture's border counting as not the object; of pixels
//! equally far, the first in reading order (top row first, then left first). It is asked
//! as `segment`'s query of the kind `point`, `X Y`, that pixel's column and row, as the
//! playground asks a click.
//!
//! **The two masks** of each picture, each a run in the store under a request id of its
//! own ([`request_id`]), so a command stopped part way is run again as it was typed and
//! nothing is asked twice:
//!
//! - *unshaped*: `segment` with that point and one instance, its mask as it is, which is
//!   what a clicked removal paints today (`segment`, then `fill`);
//! - *shaped*: that mask grown, closed, plugged and feathered by the stretch from `grow`
//!   until `feather` of the shipped recipe `remove_prompt`, given it in place of what
//!   `select` found, as the loop shapes its found mask ([`whizbang_rascal::checked`]).
//!   That stretch is `remove`'s own shaping, step for step: the command refuses to run
//!   unless every step of `remove_prompt` from `grow` to `feather` is the step of `remove`
//!   of the same id, operation and settings, bound the same way but for what `grow` takes
//!   ([`shaping_is_removes`]). `remove` itself cannot be run from `grow` without a location
//!   to give it, which a click is not.
//!
//! **The measure** is `eval loop score`'s: coverage of the truth mask, a pixel of the mask
//! where its weight is at least a half ([`crate::loop_score::against`]), and under-covered
//! below [`UNDER`]. A click that found nothing covers nothing, on both sides. A picture
//! whose run did not complete, or whose mask is not the truth mask's size, is unmeasured,
//! and while any is, the rule decides nothing.
//!
//! **The rule, frozen in the plan** (C1, item 2): the clicked removal's recipe is built when
//! the shaped mask is under-covered on at least [`FEWER`] fewer pictures than the unshaped
//! one; otherwise the carry-over row closes with the counts.
//!
//! **What it prints** is aggregates only, over all pictures and by group: no picture, no
//! point, no phrase. It writes nothing but the runs in the store.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use whizbang_core::artifact::ArtifactId;
use whizbang_core::contract::{Answer, Call, Executor, Outcome, Request, RunState, Value};
use whizbang_core::recipe::{FROM, Recipe, UNTIL, from_port};
use whizbang_core::store::Reader;
use whizbang_core::store::fs::RealFs;
use whizbang_rascal::recipes;

use crate::loop_score::{self, Entry, Manifest, Truth};

/// Coverage below this is under-covered: `eval loop score`'s threshold.
pub const UNDER: f64 = loop_score::UNDER;

/// The rule's margin: the recipe is built when the shaped mask is under-covered on at
/// least this many fewer pictures than the unshaped one (plan mE, task C1, item 2).
pub const FEWER: usize = 3;

/// The recipe whose stretch shapes the mask, and the shipped recipe whose shaping it must be.
const SHAPER: &str = whizbang_rascal::checked::REMOVE;

/// The stretch: from the step after `select`, until the last step before `fill`.
const SELECT: &str = "select";
const GROW: &str = "grow";
const FEATHER: &str = "feather";

/// The request id of a picture's run: `e-clicked-<item>-find` for `segment`,
/// `e-clicked-<item>-shape` for the shaping.
pub fn request_id(item: &str, which: &str) -> String {
    format!("e-clicked-{item}-{which}")
}

/// The click inside a truth mask: its column and row (the module's description says how
/// it is chosen). None for a mask with no pixel of the object.
pub fn click_of(truth: &Truth) -> Option<(u32, u32)> {
    let (w, h) = (truth.width as usize, truth.height as usize);
    if truth.pixels.len() != w * h {
        return None;
    }
    // City-block distance to the nearest pixel that is not the object, the outside of
    // the picture counting as not the object: two passes.
    let mut d = vec![0_u32; w * h];
    for y in 0..h {
        for x in 0..w {
            let at = y * w + x;
            if truth.pixels[at] {
                let up = if y > 0 { d[at - w] } else { 0 };
                let left = if x > 0 { d[at - 1] } else { 0 };
                d[at] = up.min(left) + 1;
            }
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let at = y * w + x;
            if truth.pixels[at] {
                let down = if y + 1 < h { d[at + w] } else { 0 };
                let right = if x + 1 < w { d[at + 1] } else { 0 };
                d[at] = d[at].min(down.min(right) + 1);
            }
        }
    }
    let mut best: Option<(u32, usize)> = None;
    for (at, &depth) in d.iter().enumerate() {
        if depth > 0 && best.is_none_or(|(most, _)| depth > most) {
            best = Some((depth, at));
        }
    }
    best.map(|(_, at)| ((at % w) as u32, (at / w) as u32))
}

/// Whether the stretch of `remove_prompt` from `grow` until `feather` is `remove`'s
/// shaping: the same steps in the same order, each of the same operation and settings and
/// bound the same way, but for the mask `grow` takes; and in `remove_prompt` `grow` comes
/// straight after `select`, which is what the stretch is given in its place.
pub fn shaping_is_removes() -> Result<(), String> {
    let read = |text: &str| Recipe::read(text).map_err(|e| e.to_string());
    let (shaper, remove) = (read(recipes::REMOVE_PROMPT)?, read(recipes::REMOVE)?);
    let stretch = |recipe: &Recipe| -> Result<Vec<_>, String> {
        let at = |id: &str| recipe.steps.iter().position(|s| s.id == id);
        match (at(GROW), at(FEATHER)) {
            (Some(from), Some(to)) if from <= to => Ok(recipe.steps[from..=to].to_vec()),
            _ => Err(format!("{} has no steps {GROW} to {FEATHER}", recipe.name)),
        }
    };
    let (ours, theirs) = (stretch(&shaper)?, stretch(&remove)?);
    let differs = || {
        format!(
            "the steps {GROW} to {FEATHER} of {SHAPER} are not remove's; the clicked mask \
             would not be shaped as remove shapes a location's"
        )
    };
    if ours.len() != theirs.len() {
        return Err(differs());
    }
    for (a, b) in ours.iter().zip(&theirs) {
        let same_inputs = a.id == GROW || a.inputs == b.inputs;
        if a.id != b.id || a.op != b.op || a.params != b.params || !same_inputs {
            return Err(differs());
        }
    }
    let grows_the_selection = (shaper.steps.iter())
        .position(|s| s.id == SELECT)
        .is_some_and(|at| shaper.steps.get(at + 1).is_some_and(|s| s.id == GROW));
    if !grows_the_selection {
        return Err(format!(
            "the step {GROW} of {SHAPER} does not come straight after {SELECT}"
        ));
    }
    Ok(())
}

/// One picture made ready: its entry, the picture in the store, its truth mask and its
/// click.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub entry: Entry,
    pub image: ArtifactId,
    pub truth: Truth,
    pub click: (u32, u32),
}

/// Reads every entry's picture and truth mask from the folder by their pins, finds each
/// click, and brings the pictures into the store at `root`, which must be there. Nothing
/// runs; on any refusal nothing more is brought in.
pub fn prepare(
    manifest: &Manifest,
    folder: &Path,
    root: &Path,
    recovery: &whizbang_core::contract::Recovery,
) -> Result<Vec<Prepared>, String> {
    shaping_is_removes()?;
    let mut read = Vec::new();
    for entry in &manifest.item {
        let truth = manifest.pinned(folder, &entry.truth, &entry.truth_sha256)?;
        let truth = loop_score::truth_of(&truth)?;
        let click = click_of(&truth).ok_or("a truth mask has no pixel of the object")?;
        let image = manifest.pinned(folder, &entry.image, &entry.image_sha256)?;
        read.push((entry, image, truth, click));
    }
    let mut prepared = Vec::new();
    for (entry, bytes, truth, click) in read {
        let shown = format!("{}/{}", manifest.set, entry.image);
        let image = crate::import::picture_of_bytes(
            &bytes,
            &shown,
            Some(&entry.image_sha256),
            root,
            recovery.clone(),
        )?;
        prepared.push(Prepared {
            entry: entry.clone(),
            image,
            truth,
            click,
        });
    }
    Ok(prepared)
}

/// What came of one picture's two runs.
#[derive(Debug, Clone, PartialEq)]
pub enum Ran {
    /// The click's mask and its shaped mask.
    Masks {
        unshaped: ArtifactId,
        shaped: ArtifactId,
    },
    /// `segment` completed and found nothing at the click.
    NothingFound,
    /// A run did not complete: which, and why.
    Failed(String),
}

/// Runs a call under a request id and waits: its outputs, or why it did not complete.
fn ran(
    executor: &dyn Executor,
    call: Call,
    id: &str,
) -> Result<Result<Vec<ArtifactId>, String>, String> {
    let id = executor.request_id(id).map_err(|e| e.to_string())?;
    let answer = (executor.request(&Request::new(call).with_id(id))).map_err(|e| e.to_string())?;
    let state = match answer {
        Answer::Ran(finished) => RunState::Ended(finished.outcome().clone()),
        Answer::Repeat(earlier) => earlier.state().clone(),
        Answer::Started(_) => RunState::NotEnded { here: true },
    };
    Ok(match state {
        RunState::Ended(Outcome::Completed { outputs }) => {
            Ok(outputs.iter().map(|output| output.id()).collect())
        }
        RunState::Ended(Outcome::Invalid { why }) => Err(format!("refused: {why}")),
        RunState::Ended(Outcome::Error { kind, message }) => Err(format!("{kind}: {message}")),
        RunState::Ended(Outcome::Cancelled) => Err("cancelled".to_owned()),
        _ => Err("did not end".to_owned()),
    })
}

/// Each picture's two runs, in the manifest's order: `segment` at the click, one
/// instance, under the config's segmenter or `backend`; then, when it found a mask, the
/// shaping.
pub fn run(
    executor: &dyn Executor,
    prepared: &[Prepared],
    backend: Option<&str>,
) -> Result<Vec<Ran>, String> {
    let mut out = Vec::new();
    for one in prepared {
        let (x, y) = one.click;
        let mut find = Call::new(whizbang_rascal::segment::NAME)
            .argument("kind", Value::text("point"))
            .argument("query", Value::text(&format!("{x} {y}")))
            .argument("max_instances", Value::Integer(1))
            .input("image", one.image);
        if let Some(backend) = backend {
            find = find.argument("backend", Value::text(backend));
        }
        let found = match ran(executor, find, &request_id(&one.entry.id, "find"))? {
            Ok(found) => found,
            Err(why) => {
                out.push(Ran::Failed(format!("segment: {why}")));
                continue;
            }
        };
        let Some(&unshaped) = found.first() else {
            out.push(Ran::NothingFound);
            continue;
        };
        let shape = Call::new(SHAPER)
            .argument("prompt", Value::text(&one.entry.phrase))
            .argument(FROM, Value::text(GROW))
            .argument(UNTIL, Value::text(FEATHER))
            .input("image", one.image)
            .input(&from_port(SELECT, "mask"), unshaped);
        let shaped = match ran(executor, shape, &request_id(&one.entry.id, "shape"))? {
            Ok(shaped) => shaped,
            Err(why) => {
                out.push(Ran::Failed(format!("the shaping: {why}")));
                continue;
            }
        };
        match shaped.first() {
            Some(&shaped) => out.push(Ran::Masks { unshaped, shaped }),
            None => out.push(Ran::Failed("the shaping handed back no mask".to_owned())),
        }
    }
    Ok(out)
}

/// The counts of a group of pictures, or of all of them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Counts {
    pub pictures: usize,
    /// Pictures with both masks measured, a click that found nothing among them.
    pub measured: usize,
    /// Pictures whose click found nothing: coverage 0 on both sides.
    pub nothing_found: usize,
    /// Pictures not measured, by why.
    pub unmeasured: BTreeMap<String, usize>,
    pub under_unshaped: usize,
    pub under_shaped: usize,
    pub coverage_sum_unshaped: f64,
    pub coverage_sum_shaped: f64,
}

/// What the rule says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// The shaped mask is under-covered on at least [`FEWER`] fewer pictures: build it.
    Build,
    /// Otherwise: close the carry-over row with the counts.
    Close,
    /// Some picture was not measured: nothing is decided.
    Undecided,
}

impl Decision {
    pub fn word(self) -> &'static str {
        match self {
            Decision::Build => "build",
            Decision::Close => "close",
            Decision::Undecided => "undecided",
        }
    }
}

impl Counts {
    /// The rule over these counts.
    pub fn decision(&self) -> Decision {
        if self.measured < self.pictures {
            Decision::Undecided
        } else if self.under_shaped + FEWER <= self.under_unshaped {
            Decision::Build
        } else {
            Decision::Close
        }
    }

    fn mean(&self, sum: f64) -> Option<f64> {
        (self.measured > 0).then(|| sum / self.measured as f64)
    }
}

/// The tally: over all pictures, and by group.
#[derive(Debug, Clone, PartialEq)]
pub struct Tally {
    pub all: Counts,
    pub group: BTreeMap<String, Counts>,
}

/// Measures each picture's two masks against its truth mask, from the store at `root`.
pub fn score(root: &Path, prepared: &[Prepared], ran: &[Ran]) -> Result<Tally, String> {
    if prepared.len() != ran.len() {
        return Err("a picture has no runs".to_owned());
    }
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let mut all = Counts::default();
    let mut group: BTreeMap<String, Counts> = BTreeMap::new();
    for (one, ran) in prepared.iter().zip(ran) {
        let measured: Result<(f64, f64), String> = match ran {
            Ran::NothingFound => Ok((0.0, 0.0)),
            Ran::Failed(_) => Err("a_run_did_not_complete".to_owned()),
            Ran::Masks { unshaped, shaped } => {
                let a = loop_score::against(&reader, *unshaped, &one.truth)?;
                let b = loop_score::against(&reader, *shaped, &one.truth)?;
                match (a, b) {
                    (Ok(a), Ok(b)) => Ok((a.coverage, b.coverage)),
                    (Err(why), _) | (_, Err(why)) => Err(format!("mask_{why}")),
                }
            }
        };
        let g = group.entry(one.entry.group.clone()).or_default();
        for counts in [&mut all, g] {
            counts.pictures += 1;
            match &measured {
                Ok((unshaped, shaped)) => {
                    counts.measured += 1;
                    counts.nothing_found += usize::from(*ran == Ran::NothingFound);
                    counts.under_unshaped += usize::from(*unshaped < UNDER);
                    counts.under_shaped += usize::from(*shaped < UNDER);
                    counts.coverage_sum_unshaped += unshaped;
                    counts.coverage_sum_shaped += shaped;
                }
                Err(why) => *counts.unmeasured.entry(why.clone()).or_default() += 1,
            }
        }
    }
    Ok(Tally { all, group })
}

impl Tally {
    /// What the command prints: counts and means only.
    pub fn text(&self) -> String {
        let mean = |c: &Counts, sum: f64| {
            c.mean(sum)
                .map_or_else(|| "none".to_owned(), |m| format!("{m:.4}"))
        };
        let a = &self.all;
        let unmeasured: usize = a.unmeasured.values().sum();
        let mut text = format!(
            "pictures: {}\nmeasured: {}\nnothing found at the click: {}\nunmeasured: {}\n",
            a.pictures, a.measured, a.nothing_found, unmeasured
        );
        for (why, n) in &a.unmeasured {
            text.push_str(&format!("unmeasured {why}: {n}\n"));
        }
        text.push_str(&format!(
            "under-covered (coverage below {UNDER}) unshaped: {}\n\
             under-covered (coverage below {UNDER}) shaped: {}\n\
             coverage mean unshaped: {}\ncoverage mean shaped: {}\n",
            a.under_unshaped,
            a.under_shaped,
            mean(a, a.coverage_sum_unshaped),
            mean(a, a.coverage_sum_shaped),
        ));
        for (name, g) in &self.group {
            text.push_str(&format!(
                "group {name}: pictures {} measured {} under-covered unshaped {} shaped {}\n",
                g.pictures, g.measured, g.under_unshaped, g.under_shaped
            ));
        }
        text.push_str(&format!(
            "rule (build when the shaped mask is under-covered on at least {FEWER} fewer \
             pictures than the unshaped one): {}\n",
            a.decision().word()
        ));
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn truth(w: u32, h: u32, inside: impl Fn(u32, u32) -> bool) -> Truth {
        let mut pixels = Vec::new();
        for y in 0..h {
            for x in 0..w {
                pixels.push(inside(x, y));
            }
        }
        Truth {
            width: w,
            height: h,
            pixels,
        }
    }

    #[test]
    fn the_click_is_the_deepest_pixel_first_in_reading_order() {
        // A box of 5 by 3 at (2, 1): its middle row, the first of the deepest.
        let t = truth(10, 6, |x, y| (2..7).contains(&x) && (1..4).contains(&y));
        assert_eq!(click_of(&t), Some((3, 2)));
        // An odd square: its centre.
        let t = truth(9, 9, |x, y| (2..7).contains(&x) && (2..7).contains(&y));
        assert_eq!(click_of(&t), Some((4, 4)));
        // The picture's border is not the object: a mask that fills it is deepest inside.
        let t = truth(3, 3, |_, _| true);
        assert_eq!(click_of(&t), Some((1, 1)));
        // An L: never outside the mask.
        let t = truth(8, 8, |x, y| x < 2 || y >= 6);
        let (x, y) = click_of(&t).unwrap();
        assert!(t.pixels[(y * 8 + x) as usize]);
        assert_eq!(click_of(&truth(4, 4, |_, _| false)), None);
    }

    #[test]
    fn the_shaping_is_removes() {
        shaping_is_removes().unwrap();
    }

    #[test]
    fn the_rule_builds_on_three_fewer_and_decides_nothing_unmeasured() {
        let counts = |under_unshaped, under_shaped, measured| Counts {
            pictures: 20,
            measured,
            under_unshaped,
            under_shaped,
            ..Counts::default()
        };
        assert_eq!(counts(5, 2, 20).decision(), Decision::Build);
        assert_eq!(counts(5, 3, 20).decision(), Decision::Close);
        assert_eq!(counts(2, 0, 20).decision(), Decision::Close);
        assert_eq!(counts(9, 0, 19).decision(), Decision::Undecided);
    }
}
