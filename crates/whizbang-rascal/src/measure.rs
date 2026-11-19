//! `measure`: the five measures of a seam or a composite (`SPEC.md` section 7), each a
//! definition of the constant set `measures-v1`. Each measure that measures something
//! hands back a series per row, with the rows it kept; the summaries (the mean, the
//! median, the 95th percentile, the largest, and the rows skipped) and drift's means by
//! zone are said in the run's journal. A measure whose inputs are absent is not
//! applicable, and the others go on; one whose every row is skipped measured nothing and
//! hands back no series. None of them is a verdict.
//!
//! **The measures.** *Continuity*: on each row, the mean over the channels of the
//! difference between the two columns beside the line, over the row's mean difference
//! between neighbouring columns. *Tone step*: the mean colour of the band's two step
//! windows (`fan::windows`) in CIE Lab, as the differences of L, chroma and hue, right
//! minus left; the hue is left out where either side's chroma is under `chroma_floor`.
//! *Transition*: the mean Sobel magnitude of the luminance within the band's half-width
//! of the line, over its mean outside twice that. *Texture match*: the same inside the
//! core, over the columns between one and two half-widths out on each side. *Drift*: the
//! mean difference of each channel between `before` and `after`, inside the core, in the
//! blend, and where the mask weighs nothing, which a correct paste leaves at zero.
//!
//! **What it reads.** `after`, the image measured, and `before`, the image before the
//! heal or the paste, are of the mask's size: the mask is on one of them, or on an image
//! of its size whose pixels they share (a composite made in place, `paste`'s). The line
//! is a column on the mask's image, or on either image; without it the band profile's
//! column is taken. A seam's mask is on its cut, so the panorama a seam healed is cut
//! the same way before it is measured, as rascal's frames had to agree.
//!
//! **What it hands back, and how much.** At most six series on `after`'s rows, each of
//! at most one value a row; and at most 39 numbers in the journal, on one line. Nothing grows with what
//! the call asks beyond the image's height.
//!
//! Ported from rascal `crates/rascal-ops/src/measure.rs` and `crates/rascal-ops/src/
//! primitives.rs` (`srgb_to_lab`) at the commit `docs/sources.toml` pins: the arithmetic
//! is rascal's, in 64-bit floats, and its Sobel plane is `fan`'s, which is rascal's. Two
//! differences: rascal read the line's column as a share of the width, and here it is
//! in pixels, as every location here is; and rascal's statuses `measured_nothing` and
//! `not_applicable` are said in the journal, beside a run that completed with the series
//! it made, since a run here has no status of that kind.

use std::sync::Arc;

use whizbang_core::artifact::{ArtifactType, BandProfile, Line, Mask, Origin, Series, Shape, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Count, Failure, InputPort, Kind, OutputPort,
    PortRef, PreparedCall, Produced, RegistrationError, Registry, ResultKind, Value, ValueType,
};
use whizbang_core::identity::CanonicalPixels;

use crate::constants::{self, MEASURES_V1};
use crate::fan::{luminance, sobel_magnitude, windows};
use crate::{operation, paste};

/// The action's name.
pub const NAME: &str = "measure";

/// The action's version: rascal's.
pub const VERSION: &str = "1.0.0";

/// The measures, in the order they run and are listed.
pub const MEASURES: [&str; 5] = [
    "continuity",
    "tone_step",
    "transition",
    "texture_match",
    "drift",
];

/// The series each measure writes, by output port: the port, the series' name and its
/// unit.
pub const SERIES: [(&str, &str, &str); 6] = [
    ("continuity", "continuity", "ratio"),
    ("tone_step_dl", "tone_step.dL", "L"),
    ("tone_step_dchroma", "tone_step.dchroma", "chroma"),
    ("tone_step_dhue", "tone_step.dhue", "degrees"),
    ("transition", "transition", "ratio"),
    ("texture_match", "texture_match", "ratio"),
];

/// What the command says the action does.
pub const ABOUT: &str = "Measure a seam or a composite: continuity, tone step, transition, texture match and \
drift, on images of the mask's size. Each measure's series per row is committed to the store \
and its identity printed; the summaries are printed in the run's journal. None is a verdict.";

/// A measure per row: the rows kept with their values, and how many were skipped.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PerRow {
    pub rows: Vec<u32>,
    pub values: Vec<f64>,
    pub skipped: u32,
}

impl PerRow {
    fn push(&mut self, row: u32, value: Option<f64>) {
        match value {
            Some(v) if v.is_finite() => {
                self.rows.push(row);
                self.values.push(v);
            }
            _ => self.skipped += 1,
        }
    }
}

/// The mean, the median, the percentile (nearest rank) and the largest of some values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Summary {
    pub mean: f64,
    pub median: f64,
    pub p95: f64,
    pub max: f64,
}

/// The summary of `values`, which are not empty.
pub fn summarise(values: &[f64], percentile: f64) -> Summary {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    let rank = |p: f64| sorted[(((p / 100.0) * n as f64).ceil() as usize).clamp(1, n) - 1];
    Summary {
        mean: sorted.iter().sum::<f64>() / n as f64,
        median: if n % 2 == 1 {
            sorted[n / 2]
        } else {
            (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
        },
        p95: rank(percentile),
        max: sorted[n - 1],
    }
}

/// Red, green and blue of the pixel at `(x, y)`.
fn rgb(pixels: &CanonicalPixels, x: u32, y: u32) -> [u8; 3] {
    let at = ((y as usize) * pixels.width() as usize + x as usize) * 4;
    let p = &pixels.rgba()[at..at + 3];
    [p[0], p[1], p[2]]
}

/// Continuity on each row: the mean over the channels of the difference between the
/// columns `column - 1` and `column`, over the row's mean difference between
/// neighbouring columns. A zero denominator skips the row.
pub fn continuity(after: &CanonicalPixels, column: u32) -> PerRow {
    let (w, h) = (after.width(), after.height());
    let mut out = PerRow::default();
    if column == 0 || column >= w || w < 2 {
        out.skipped = h;
        return out;
    }
    let diff = |y: u32, x: u32| -> f64 {
        let (a, b) = (rgb(after, x, y), rgb(after, x + 1, y));
        (0..3)
            .map(|ch| f64::from(a[ch].abs_diff(b[ch])))
            .sum::<f64>()
            / 3.0
    };
    for y in 0..h {
        let at_line = diff(y, column - 1);
        let mean = (0..w - 1).map(|x| diff(y, x)).sum::<f64>() / f64::from(w - 1);
        out.push(y, (mean > 0.0).then(|| at_line / mean));
    }
    out
}

/// CIE Lab (D65) of an sRGB colour: rascal's `srgb_to_lab`.
pub fn srgb_to_lab(rgb: [u8; 3]) -> [f64; 3] {
    let linear = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let (r, g, b) = (linear(rgb[0]), linear(rgb[1]), linear(rgb[2]));
    let x = (0.4124564 * r + 0.3575761 * g + 0.1804375 * b) / 0.95047;
    let y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b;
    let z = (0.0193339 * r + 0.1191920 * g + 0.9503041 * b) / 1.08883;
    let f = |t: f64| {
        if t > 216.0 / 24389.0 {
            t.cbrt()
        } else {
            (24389.0 / 27.0 * t + 16.0) / 116.0
        }
    };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// The signed difference `b - a` of two hues in degrees, in `(-180, 180]`.
pub fn hue_difference(a: f64, b: f64) -> f64 {
    let d = (b - a).rem_euclid(360.0);
    if d > 180.0 { d - 360.0 } else { d }
}

/// Tone step on each row: the differences of L, chroma and hue between the mean Lab
/// colours of the band's right and left step windows; the hue is left out where either
/// side's chroma is under `chroma_floor`.
pub fn tone_step(after: &CanonicalPixels, band: &BandProfile, chroma_floor: f64) -> [PerRow; 3] {
    let (w, h) = (after.width(), after.height());
    let [(_, l0, l1), (_, r0, r1), _, _] = windows(band.column(), band.thin(), band.windows(), w);
    let mut out: [PerRow; 3] = Default::default();
    for y in 0..h {
        let lab_mean = |lo: i64, hi: i64| -> Option<[f64; 3]> {
            if hi <= lo {
                return None;
            }
            let mut sum = [0.0; 3];
            for x in lo..hi {
                let lab = srgb_to_lab(rgb(after, x as u32, y));
                for k in 0..3 {
                    sum[k] += lab[k];
                }
            }
            let n = (hi - lo) as f64;
            Some([sum[0] / n, sum[1] / n, sum[2] / n])
        };
        match (lab_mean(l0, l1), lab_mean(r0, r1)) {
            (Some(l), Some(r)) => {
                let chroma = |lab: [f64; 3]| (lab[1] * lab[1] + lab[2] * lab[2]).sqrt();
                let hue = |lab: [f64; 3]| lab[2].atan2(lab[1]).to_degrees();
                out[0].push(y, Some(r[0] - l[0]));
                out[1].push(y, Some(chroma(r) - chroma(l)));
                let hue_ok = chroma(l) >= chroma_floor && chroma(r) >= chroma_floor;
                out[2].push(y, hue_ok.then(|| hue_difference(hue(l), hue(r))));
            }
            _ => {
                for s in &mut out {
                    s.push(y, None);
                }
            }
        }
    }
    out
}

/// The Sobel magnitude of the luminance of `pixels`, which transition and texture match
/// share.
pub fn sobel_plane(pixels: &CanonicalPixels) -> Vec<f32> {
    sobel_magnitude(&luminance(pixels), pixels.width(), pixels.height())
}

/// The columns `[lo, hi)` within the image whose centres `x + 0.5` lie in
/// `[c + from, c + to)`: the band core's own rule for a pixel's centre.
fn span(c: f64, from: f64, to: f64, width: u32) -> (i64, i64) {
    let lo = (c + from - 0.5).ceil().max(0.0) as i64;
    let hi = ((c + to - 0.5).ceil().max(0.0) as i64).min(i64::from(width));
    (lo, hi.max(lo))
}

/// Transition on each row: the mean Sobel magnitude over `[c - half, c + half)`, over the
/// row's mean outside `[c - 2 half, c + 2 half)`.
pub fn transition(after: &CanonicalPixels, band: &BandProfile, sobel: &[f32]) -> PerRow {
    let (w, h) = (after.width(), after.height());
    let c = f64::from(band.column());
    let mut out = PerRow::default();
    for y in 0..h {
        let half = band.half()[y as usize];
        let at = |x: i64| f64::from(sobel[(y * w) as usize + x as usize]);
        let (i0, i1) = span(c, -half, half, w);
        let (o0, o1) = span(c, -2.0 * half, 2.0 * half, w);
        let inside: Vec<f64> = (i0..i1).map(at).collect();
        let outside: Vec<f64> = (0..o0).chain(o1..i64::from(w)).map(at).collect();
        let value = if inside.is_empty() || outside.is_empty() {
            None
        } else {
            let num = inside.iter().sum::<f64>() / inside.len() as f64;
            let den = outside.iter().sum::<f64>() / outside.len() as f64;
            (den > 0.0).then(|| num / den)
        };
        out.push(y, value);
    }
    out
}

/// Texture match on each row: the mean Sobel magnitude inside the core, over its mean
/// on the columns between one and two half-widths out on each side.
pub fn texture_match(
    after: &CanonicalPixels,
    mask: &Mask,
    band: &BandProfile,
    sobel: &[f32],
) -> PerRow {
    let (w, h) = (after.width(), after.height());
    let c = f64::from(band.column());
    let mut out = PerRow::default();
    for y in 0..h {
        let half = band.half()[y as usize];
        let at = |x: i64| f64::from(sobel[(y * w) as usize + x as usize]);
        let row = &mask.core()[(y * w) as usize..((y + 1) * w) as usize];
        let core: Vec<f64> = (0..w)
            .filter(|x| row[*x as usize] != 0)
            .map(|x| at(i64::from(x)))
            .collect();
        let (l0, l1) = span(c, -2.0 * half, -half, w);
        let (r0, r1) = span(c, half, 2.0 * half, w);
        let beyond: Vec<f64> = (l0..l1).chain(r0..r1).map(at).collect();
        let value = if core.is_empty() || beyond.is_empty() {
            None
        } else {
            let num = core.iter().sum::<f64>() / core.len() as f64;
            let den = beyond.iter().sum::<f64>() / beyond.len() as f64;
            (den > 0.0).then(|| num / den)
        };
        out.push(y, value);
    }
    out
}

/// Drift: the mean difference of each channel inside the core, in the blend (where it
/// weighs something outside the core), and where the mask weighs nothing. A zone without
/// a pixel says zero.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drift {
    pub core: [f64; 3],
    pub blend: [f64; 3],
    pub outside: [f64; 3],
}

/// The drift from `before` to `after` under `mask`, all of one size.
pub fn drift(before: &CanonicalPixels, after: &CanonicalPixels, mask: &Mask) -> Drift {
    let (w, h) = (mask.space().width(), mask.space().height());
    let mut sums = [[0.0f64; 3]; 3];
    let mut counts = [0u64; 3];
    for y in 0..h {
        for x in 0..w {
            let at = (y * w + x) as usize;
            let zone = if mask.weight(x, y).unwrap_or(0.0) <= 0.0 {
                2
            } else if mask.core()[at] != 0 {
                0
            } else {
                1
            };
            let (a, b) = (rgb(before, x, y), rgb(after, x, y));
            for ch in 0..3 {
                sums[zone][ch] += f64::from(a[ch].abs_diff(b[ch]));
            }
            counts[zone] += 1;
        }
    }
    let mean = |zone: usize| -> [f64; 3] {
        let n = counts[zone].max(1) as f64;
        [sums[zone][0] / n, sums[zone][1] / n, sums[zone][2] / n]
    };
    Drift {
        core: mean(0),
        blend: mean(1),
        outside: mean(2),
    }
}

/// The measures a call's `set` names, in its order and once each; or why not.
pub fn measures_of(text: &str) -> Result<Vec<&'static str>, String> {
    let mut set = Vec::new();
    for name in text.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let Some(known) = MEASURES.iter().find(|m| **m == name) else {
            return Err(format!(
                "a measure in set is unknown; the measures are {}",
                MEASURES.join(", ")
            ));
        };
        if !set.contains(known) {
            set.push(*known);
        }
    }
    if set.is_empty() {
        return Err("set names no measure".to_owned());
    }
    Ok(set)
}

/// What a call measured: the series by port, the numbers said, and each measure's word.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Measured {
    /// Each series made, by its port: its name, its unit and its rows.
    pub series: Vec<(&'static str, &'static str, &'static str, PerRow)>,
    /// Each number, its name and its unit.
    pub numbers: Vec<(String, f64, &'static str)>,
    /// Each measure, and what became of it: `ok`, `measured_nothing` or
    /// `not_applicable (why)`.
    pub statuses: Vec<String>,
}

/// The images and the mask a call measures, read and checked.
pub struct Measuring<'a> {
    pub after: &'a CanonicalPixels,
    pub before: Option<&'a CanonicalPixels>,
    pub mask: &'a Mask,
    /// The line's column, in pixels, when the call gave a line.
    pub column: Option<u32>,
    pub chroma_floor: f64,
    pub percentile: f64,
}

impl Measuring<'_> {
    /// Each measure of `set`, as rascal ran them.
    pub fn run(&self, set: &[&str]) -> Measured {
        let mut measured = Measured::default();
        let column = self
            .column
            .or_else(|| self.mask.profile().map(|p| p.band.column()));
        let mut sobel: Option<Vec<f32>> = None;
        for measure in set {
            let made: Vec<(usize, PerRow)> = match *measure {
                "continuity" => match column {
                    Some(c) => vec![(0, continuity(self.after, c))],
                    None => {
                        measured
                            .statuses
                            .push("continuity: not_applicable (no line and no profile)".into());
                        continue;
                    }
                },
                "tone_step" | "transition" | "texture_match" => {
                    let Some(profile) = self.mask.profile() else {
                        measured.statuses.push(format!(
                            "{measure}: not_applicable (the mask has no profile)"
                        ));
                        continue;
                    };
                    let band = &profile.band;
                    match *measure {
                        "tone_step" => {
                            let [dl, dc, dh] = tone_step(self.after, band, self.chroma_floor);
                            vec![(1, dl), (2, dc), (3, dh)]
                        }
                        "transition" => {
                            let plane = sobel.get_or_insert_with(|| sobel_plane(self.after));
                            vec![(4, transition(self.after, band, plane))]
                        }
                        _ => {
                            let plane = sobel.get_or_insert_with(|| sobel_plane(self.after));
                            vec![(5, texture_match(self.after, self.mask, band, plane))]
                        }
                    }
                }
                _ => {
                    let Some(before) = self.before else {
                        measured
                            .statuses
                            .push("drift: not_applicable (no before image)".into());
                        continue;
                    };
                    let d = drift(before, self.after, self.mask);
                    for (zone, values) in
                        [("core", d.core), ("blend", d.blend), ("outside", d.outside)]
                    {
                        for (ch, v) in ["r", "g", "b"].iter().zip(values) {
                            measured
                                .numbers
                                .push((format!("drift.{zone}.{ch}"), v, "levels"));
                        }
                    }
                    measured.statuses.push("drift: ok".into());
                    continue;
                }
            };
            let mut any = false;
            for (index, rows) in made {
                let (port, name, unit) = SERIES[index];
                measured.numbers.push((
                    format!("{name}.rows_skipped"),
                    f64::from(rows.skipped),
                    "rows",
                ));
                if rows.values.is_empty() {
                    continue;
                }
                any = true;
                let s = summarise(&rows.values, self.percentile);
                for (k, v) in [
                    ("mean", s.mean),
                    ("median", s.median),
                    ("p95", s.p95),
                    ("max", s.max),
                ] {
                    measured.numbers.push((format!("{name}.{k}"), v, unit));
                }
                measured.series.push((port, name, unit, rows));
            }
            let word = if any { "ok" } else { "measured_nothing" };
            measured.statuses.push(format!("{measure}: {word}"));
        }
        measured
    }
}

impl Measured {
    /// Whether any measure measured something: rascal's status `ok`. Otherwise nothing
    /// is handed back, as rascal published nothing for another status.
    pub fn ok(&self) -> bool {
        self.statuses.iter().any(|s| s.ends_with(": ok"))
    }

    /// The journal's line.
    pub fn line(&self) -> String {
        let numbers: Vec<String> = self
            .numbers
            .iter()
            .map(|(name, value, unit)| format!("{name} {value} {unit}"))
            .collect();
        format!(
            "measure: {}; {}",
            self.statuses.join("; "),
            if numbers.is_empty() {
                "no numbers".to_owned()
            } else {
                numbers.join(", ")
            }
        )
    }
}

/// The action `measure`.
pub struct Measure {
    descriptor: ActionDescriptor,
}

impl Measure {
    pub fn new() -> Measure {
        let mut draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "Measures a seam or a composite: continuity, tone step, transition, texture match and drift, per row; none is a verdict.",
        )
        .argument(Argument::optional(
            "set",
            "The measures to run, separated by commas: continuity, tone step, transition, texture match and drift, each name of two words joined by an underscore; all five when not given.",
            ValueType::Text,
        ))
        .argument(Argument::with_default(
            constants::ARGUMENT,
            "The constant set the measures read: the chroma floor of a hue, and the percentile.",
            ValueType::choice(&[MEASURES_V1.0]),
            Value::text(MEASURES_V1.0),
        ))
        .input(InputPort::required(
            "after",
            ArtifactType::Image,
            "The image to measure, of the mask's size: the composite or the healed cut.",
        ))
        .input(InputPort::optional(
            "before",
            ArtifactType::Image,
            "The image before the heal or the paste, of the mask's size, for drift.",
        ))
        .input(InputPort::required(
            "mask",
            ArtifactType::Mask,
            "The mask, with its band profile for the band's measures.",
        ))
        .input(InputPort::optional(
            "line",
            ArtifactType::Location,
            "A whole column on the mask's image or on either image, for continuity; the profile's column without it.",
        ));
        for (port, _, unit) in SERIES {
            draft = draft.output(
                OutputPort::counted(
                    port,
                    ArtifactType::Series,
                    &format!(
                        "The measure of this name per row, in {unit}; absent when it was not asked, not applicable or measured nothing."
                    ),
                    Count::between(0, 1),
                )
                .on(PortRef::input("after")),
            );
        }
        // What it measured is handed back beside its series too (plan m3, task Q1; ADR
        // 0035): the numbers `Measured::numbers` holds.
        let draft = draft.results(ResultKind::Measurements).concurrent(true);
        match draft.build() {
            Ok(descriptor) => Measure { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Measure {
    fn default() -> Measure {
        Measure::new()
    }
}

/// Registers `measure`.
pub fn register(registry: &mut Registry) -> Result<(), RegistrationError> {
    registry.register(Arc::new(Measure::new()))
}

/// The number `key` of `measures-v1`.
fn constant(key: &str) -> Result<f64, Failure> {
    constants::number(MEASURES_V1.1, key)
        .ok_or_else(|| Failure::error("constants", &format!("measures-v1 has no {key}")))
}

impl Action for Measure {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let set_name = operation::text(call, constants::ARGUMENT).unwrap_or(MEASURES_V1.0);
        if set_name != MEASURES_V1.0 {
            return Err(Failure::invalid(&format!(
                "{set_name} is not a constant set of measure"
            )));
        }
        let all = MEASURES.join(",");
        let asked = operation::text(call, "set").unwrap_or(&all);
        let set = measures_of(asked).map_err(|why| Failure::invalid(&why))?;
        let (after_id, mask_id) = (
            operation::one(call, "after")?,
            operation::one(call, "mask")?,
        );
        let before_id = operation::given(call, "before").first().copied();
        let line_id = operation::given(call, "line").first().copied();
        let mask = operation::mask(context, mask_id)?;
        let (after_image, after) = paste::picture(context, after_id)?;
        let before = match before_id {
            Some(id) => Some(paste::picture(context, id)?.1),
            None => None,
        };
        let space = mask.space();
        let size = (space.width(), space.height());
        let sized = |p: &CanonicalPixels| (p.width(), p.height()) == size;
        if !sized(after.pixels()) || before.as_ref().is_some_and(|b| !sized(b.pixels())) {
            return Err(Failure::invalid(
                "before, after and the mask must be of one size (cut or window the image as the mask's was first)",
            ));
        }
        let column = match line_id {
            None => None,
            Some(id) => {
                let line = operation::location(context, id)?;
                let on = line.binding().source();
                let images = [
                    Some(mask.image()),
                    Some(after.digest()),
                    before.as_ref().map(|b| b.digest()),
                ];
                if !images.contains(&Some(on)) || line.binding().source_space().width() != size.0 {
                    return Err(Failure::invalid(
                        "the line is on none of the mask's image, after and before",
                    ));
                }
                let Shape::Line {
                    line: Line::Column(c),
                    ..
                } = line.shape()
                else {
                    return Err(Failure::invalid(
                        "measure reads a line that is a whole column",
                    ));
                };
                Some((c.round() as u32).min(size.0.saturating_sub(1)))
            }
        };
        if operation::stopped(context) {
            return Err(operation::stopped_failure(context, NAME));
        }
        let measuring = Measuring {
            after: after.pixels(),
            before: before.as_ref().map(|b| b.pixels()),
            mask: &mask,
            column,
            chroma_floor: constant("chroma_floor")?,
            percentile: constant("percentile")?,
        };
        let measured = measuring.run(&set);
        context.events().progress(&measured.line());
        let numbers: Vec<serde_json::Value> = (measured.numbers.iter())
            .map(|(name, value, unit)| serde_json::json!({"name": name, "value": value, "unit": unit}))
            .collect();
        let handed = serde_json::json!({ "numbers": numbers });
        if !measured.ok() {
            return Ok(Produced::nothing().with_measurements(handed));
        }
        let mut step = Step::new(NAME, VERSION)
            .argument("set", &set.join(","))
            .argument(constants::ARGUMENT, set_name)
            .input("after", after_id.0)
            .input("mask", mask_id.0);
        if let Some(id) = before_id {
            step = step.input("before", id.0);
        }
        if let Some(id) = line_id {
            step = step.input("line", id.0);
        }
        let fail = |error: whizbang_core::artifact::ArtifactError| {
            Failure::error("series", &error.without_paths())
        };
        let origin = Origin::new(step).map_err(fail)?;
        let mut produced = Produced::nothing().with_measurements(handed);
        for (port, name, unit, rows) in measured.series {
            let series = Series::with_rows(
                after_image.binding(),
                name,
                unit,
                1,
                rows.rows,
                rows.values,
                origin.clone(),
            )
            .map_err(fail)?;
            produced = produced.with(port, series);
        }
        Ok(produced)
    }
}

/// What `whizbang rascal measure` takes beside what every operation's command does.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The image to measure, of the mask's size, by its identity in the store.
    #[arg(long)]
    pub after: String,
    /// The image before the heal or the paste, for drift, by its identity in the store.
    #[arg(long)]
    pub before: Option<String>,
    /// The mask, by its identity in the store.
    #[arg(long)]
    pub mask: String,
    /// A whole-column line for continuity, by its identity in the store.
    #[arg(long)]
    pub line: Option<String>,
    /// The measures, separated by commas. Unset: all five.
    #[arg(long)]
    pub set: Option<String>,
}

impl operation::Asked for Args {
    fn action(&self) -> &'static str {
        NAME
    }

    fn arguments(&self) -> Vec<(&'static str, Value)> {
        self.set
            .as_deref()
            .map(|set| vec![("set", Value::text(set))])
            .unwrap_or_default()
    }

    fn inputs(&self) -> Vec<(&'static str, &'static str, &str)> {
        let mut inputs = vec![
            ("after", "--after", self.after.as_str()),
            ("mask", "--mask", self.mask.as_str()),
        ];
        if let Some(before) = &self.before {
            inputs.push(("before", "--before", before.as_str()));
        }
        if let Some(line) = &self.line {
            inputs.push(("line", "--line", line.as_str()));
        }
        inputs
    }
}
