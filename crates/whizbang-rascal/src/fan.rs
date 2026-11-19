//! `fan`: the band profile along a column line, the seam's half-width row by row
//! (`SPEC.md` section 7). Two rulers per row, the tone step across the line and the
//! texture beside it, decide how far the band may open: wide where the surface is smooth
//! and the two sides differ, `thin` where it is detailed or the sides already agree. From
//! `fan-v2` on, an object inside the opening caps it (the object cap). The windows it
//! sampled are kept in the profile, so `ramp` samples exactly the same columns.
//!
//! **The constant sets** are rascal's four, `fan-v3` the default as at rascal's tag
//! (`constants::FAN`); a call names one by the argument `constants` (ADR 0033), and each
//! tunable a call gives replaces the set's value.
//!
//! Ported from rascal `crates/rascal-ops/src/fan.rs` and the row smoothing, luminance
//! and Sobel magnitude of `primitives.rs`, at the commit `docs/sources.toml` pins, with
//! three differences on purpose: the line's column is in pixels of the image, as every
//! location here is (ADR 0002), where rascal's was a fraction of the frame; the row
//! smoothing's kernel is `feather::exp_neg`, not the platform's `exp`, so a band and its
//! identity are the same on every machine; and where rascal said `fan` did not apply (a
//! window clipped to fewer than two columns) or refused its arguments, the call is
//! refused as invalid, as `select` was in plan 2A.

use std::sync::Arc;

use whizbang_core::artifact::{
    ArtifactType, BandProfile, Line as Along, Origin, Shape, Step, Windows,
};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Failure, InputPort, Kind, OutputPort, PortRef,
    PreparedCall, Produced, RegistrationError, Registry, Value, ValueType,
};
use whizbang_core::identity::{CanonicalPixels, Topology};

use crate::constants::{self, FAN};
use crate::feather::exp_neg;
use crate::grow::text_of;
use crate::{canvas, operation, paste};

/// The action's name.
pub const NAME: &str = "fan";

/// The action's version: rascal's, whose band it gives.
pub const VERSION: &str = "1.2.1";

/// What the command says the action does.
pub const ABOUT: &str = "Measure the band along a column line: a half-width for each row, wide where the surface \
beside the line is smooth and the two sides differ in tone, thin where it is detailed, and \
stopping short of a solid object the opening would cross. The band profile is committed to \
the store, and its identity is printed.";

/// `smooth_rows(v, sigma)`: a Gaussian kernel cut at `3 sigma`, weighed again over the
/// samples it covers at each row; a sigma under 0.5 changes nothing. Rascal's, with
/// [`exp_neg`] for the kernel.
pub fn smooth_rows(values: &[f64], sigma: f64) -> Vec<f64> {
    if sigma < 0.5 || values.len() < 2 {
        return values.to_vec();
    }
    let radius = (3.0 * sigma).ceil() as usize;
    let kernel: Vec<f64> = (0..=radius)
        .map(|k| exp_neg((k as f64).powi(2) / (2.0 * sigma * sigma)))
        .collect();
    let n = values.len();
    (0..n)
        .map(|i| {
            let (mut sum, mut weight) = (0.0, 0.0);
            for (j, value) in values
                .iter()
                .enumerate()
                .take((i + radius).min(n - 1) + 1)
                .skip(i.saturating_sub(radius))
            {
                let w = kernel[i.abs_diff(j)];
                sum += w * value;
                weight += w;
            }
            sum / weight
        })
        .collect()
}

/// The luminance `0.299 R + 0.587 G + 0.114 B` of every pixel, row by row.
pub fn luminance(pixels: &CanonicalPixels) -> Vec<f32> {
    pixels
        .rgba()
        .chunks(4)
        .map(|p| 0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2]))
        .collect()
}

/// The 3 by 3 Sobel magnitude of a plane, divided by 8, its border repeated outward.
pub fn sobel_magnitude(plane: &[f32], width: u32, height: u32) -> Vec<f32> {
    let (w, h) = (i64::from(width), i64::from(height));
    let at = |x: i64, y: i64| plane[(y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) as usize];
    let mut out = vec![0.0f32; plane.len()];
    for y in 0..h {
        for x in 0..w {
            // Differences first, so a flat neighbourhood gives exactly zero.
            let gx = (at(x + 1, y - 1) - at(x - 1, y - 1))
                + 2.0 * (at(x + 1, y) - at(x - 1, y))
                + (at(x + 1, y + 1) - at(x - 1, y + 1));
            let gy = (at(x - 1, y + 1) - at(x - 1, y - 1))
                + 2.0 * (at(x, y + 1) - at(x, y - 1))
                + (at(x + 1, y + 1) - at(x + 1, y - 1));
            out[(y * w + x) as usize] = (gx * gx + gy * gy).sqrt() / 8.0;
        }
    }
    out
}

/// The constants and tunables `fan` measures with, in the units of the spec.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub ref_width: f64,
    pub thin_px: f64,
    pub gap_px: f64,
    pub step_cols: f64,
    pub texture_cols: f64,
    pub tex_low: f64,
    pub tex_high: f64,
    pub lum_floor: f64,
    pub step_min: f64,
    pub step_ref: f64,
    pub wide_max: f64,
    pub row_sigma: f64,
    pub width_sigma: f64,
    pub gate: bool,
    /// The object cap (0 is off): the horizontal luminance step that marks an object's edge.
    pub object_edge: f64,
    /// An edge must also be this many times the row's median step.
    pub object_ratio: f64,
    /// How far short of the edge the band stops, in pixels at `ref_width`.
    pub object_margin_px: f64,
    /// The cap's reach along the rows, as a share of the height.
    pub object_sigma: f64,
    /// The taper beyond the cap's reach, as a share of the height; 0 steps.
    pub object_taper: f64,
    /// The vertical run an edge needs, as a share of the height; 0 takes every one.
    pub object_run: f64,
    /// How many columns the chain of an edge may wander from one row to the next.
    pub object_wander_px: u32,
    /// Whether an object the line runs through is left to the rulers (removed whole)
    /// rather than capped at `thin`.
    pub object_straddle_whole: bool,
}

impl Settings {
    /// The settings of the set named `name`, one of [`FAN`].
    pub fn of(name: &str) -> Result<Settings, String> {
        let Some((_, text)) = FAN.iter().find(|(set, _)| *set == name) else {
            return Err(format!("{name} is not a constant set of fan"));
        };
        let number = |key: &str| {
            constants::number(text, key).ok_or_else(|| format!("{name} has no number {key}"))
        };
        let gate = match constants::value(text, "gate") {
            Some(toml::Value::Boolean(gate)) => gate,
            _ => return Err(format!("{name} has no boolean gate")),
        };
        let straddle = match constants::value(text, "object_straddle") {
            Some(toml::Value::String(said)) if said == "thin" => false,
            Some(toml::Value::String(said)) if said == "whole" => true,
            _ => {
                return Err(format!(
                    "{name}'s object_straddle is neither thin nor whole"
                ));
            }
        };
        let wander = match constants::value(text, "object_wander_px") {
            Some(toml::Value::Integer(wander)) => u32::try_from(wander).ok(),
            _ => None,
        }
        .ok_or_else(|| format!("{name}'s object_wander_px is not a whole number"))?;
        Ok(Settings {
            ref_width: number("ref_width")?,
            thin_px: number("thin_px")?,
            gap_px: number("gap_px")?,
            step_cols: number("step_cols")?,
            texture_cols: number("texture_cols")?,
            tex_low: number("tex_low")?,
            tex_high: number("tex_high")?,
            lum_floor: number("lum_floor")?,
            step_min: number("step_min")?,
            step_ref: number("step_ref")?,
            wide_max: number("wide_max")?,
            row_sigma: number("row_sigma")?,
            width_sigma: number("width_sigma")?,
            gate,
            object_edge: number("object_edge")?,
            object_ratio: number("object_ratio")?,
            object_margin_px: number("object_margin_px")?,
            object_sigma: number("object_sigma")?,
            object_taper: number("object_taper")?,
            object_run: number("object_run")?,
            object_wander_px: wander,
            object_straddle_whole: straddle,
        })
    }
}

/// What `fan` measured, before it is a band profile.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub column: u32,
    pub thin: f64,
    pub half: Vec<f64>,
    /// The half-width before the width's smoothing and clamp.
    pub raw_half: Vec<f64>,
    /// The object cap of each row, or infinity where nothing stood in the way.
    pub cap: Vec<f64>,
    pub windows: Windows,
}

/// A sampling window clipped to fewer than two columns: its name and the columns left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clipped(pub &'static str, pub u32);

/// The four sampling windows as column ranges `[lo, hi)`, from the centre column, `thin`
/// and the scaled geometry, clipped to the image: rascal's `windows`, which `ramp` calls
/// with the profile's own values.
pub fn windows(
    column: u32,
    thin: f64,
    windows: Windows,
    width: u32,
) -> [(&'static str, i64, i64); 4] {
    let c = f64::from(column);
    // The first column at or past the reference edge on each side.
    let edge_l = (c - thin).ceil() as i64;
    let edge_r = (c + thin).ceil() as i64;
    let (gap, step, tex) = (
        i64::from(windows.gap),
        i64::from(windows.step_columns),
        i64::from(windows.texture_columns),
    );
    let clip = |lo: i64, hi: i64| (lo.max(0), hi.min(i64::from(width)));
    let (a, b) = clip(edge_l - gap - step, edge_l - gap);
    let (c0, d) = clip(edge_r + gap, edge_r + gap + step);
    let (e, f) = clip(edge_l - tex, edge_l);
    let (g, h) = clip(edge_r, edge_r + tex);
    [
        ("left step", a, b),
        ("right step", c0, d),
        ("left texture", e, f),
        ("right texture", g, h),
    ]
}

/// The band along the column `column`, in pixels, of `pixels`: rascal's algorithm.
pub fn fan_profile(
    pixels: &CanonicalPixels,
    column: f64,
    s: &Settings,
) -> Result<Profile, Clipped> {
    let (w, h) = (pixels.width(), pixels.height());
    let rgba = pixels.rgba();
    let scale = f64::from(w) / s.ref_width;
    let scaled = |v: f64| ((v * scale).round() as u32).max(1);
    let column = (column.round() as u32).min(w.saturating_sub(1));
    let thin = s.thin_px * scale;
    let geometry = Windows {
        gap: scaled(s.gap_px),
        step_columns: scaled(s.step_cols),
        texture_columns: scaled(s.texture_cols),
    };
    let wins = windows(column, thin, geometry, w);
    // A window keeps at least two columns, or all it asked for when that is fewer.
    for (i, (name, lo, hi)) in wins.into_iter().enumerate() {
        let asked = if i < 2 {
            geometry.step_columns
        } else {
            geometry.texture_columns
        };
        let left = (hi - lo).max(0) as u32;
        if left < asked.min(2) {
            return Err(Clipped(name, left));
        }
    }
    let [(_, sl0, sl1), (_, sr0, sr1), (_, tl0, tl1), (_, tr0, tr1)] = wins;
    let lum = luminance(pixels);
    let sobel = sobel_magnitude(&lum, w, h);
    let mut step = Vec::with_capacity(h as usize);
    let mut texture = Vec::with_capacity(h as usize);
    for y in 0..h {
        let sample = |x: i64, ch: usize| f64::from(rgba[((y * w + x as u32) * 4) as usize + ch]);
        let mean = |lo: i64, hi: i64, ch: usize| {
            (lo..hi).map(|x| sample(x, ch)).sum::<f64>() / (hi - lo) as f64
        };
        // The step: over the channels, the mean of |mean(left) - mean(right)|.
        let st = (0..3)
            .map(|ch| (mean(sl0, sl1, ch) - mean(sr0, sr1, ch)).abs())
            .sum::<f64>()
            / 3.0;
        step.push(st);
        // The texture: the mean Sobel magnitude over both texture windows, over their
        // mean luminance floored at lum_floor, in percent.
        let cols = (tl1 - tl0 + tr1 - tr0) as f64;
        let over = |plane: &[f32]| {
            (tl0..tl1)
                .chain(tr0..tr1)
                .map(|x| f64::from(plane[(y * w + x as u32) as usize]))
                .sum::<f64>()
                / cols
        };
        let mean_l = over(&lum).max(s.lum_floor);
        texture.push(over(&sobel) / mean_l * 100.0);
    }
    let row_sigma = s.row_sigma * f64::from(h);
    let step = smooth_rows(&step, row_sigma);
    let texture = smooth_rows(&texture, row_sigma);
    let wide = thin.max(s.wide_max * f64::from(w));
    let raw_half: Vec<f64> = step
        .iter()
        .zip(&texture)
        .map(|(st, tx)| {
            let smooth = ((s.tex_high - tx) / (s.tex_high - s.tex_low)).clamp(0.0, 1.0);
            let open = if s.gate {
                ((st - s.step_min) / (s.step_ref - s.step_min)).clamp(0.0, 1.0)
            } else {
                1.0
            };
            thin + (wide - thin) * smooth * open
        })
        .collect();
    let smoothed: Vec<f64> = smooth_rows(&raw_half, s.width_sigma * f64::from(h))
        .into_iter()
        .map(|v| v.clamp(thin, wide))
        .collect();
    let at = Place {
        w,
        h,
        column,
        gap: geometry.gap,
        thin,
        scale,
    };
    let cap = object_cap(&lum, &at, &smoothed, s);
    let half = smoothed
        .iter()
        .zip(&cap)
        .map(|(v, c)| v.min(*c).clamp(thin, wide))
        .collect();
    Ok(Profile {
        column,
        thin,
        half,
        raw_half,
        cap,
        windows: geometry,
    })
}

/// Where the cap looks: the image's size, the line, its gap and `thin`, and the scale.
struct Place {
    w: u32,
    h: u32,
    column: u32,
    gap: u32,
    thin: f64,
    scale: f64,
}

/// The vertical run of each strong step: the chains of strong steps up to it and down
/// from it, each within `wander` columns of the last row's, less the one counted twice.
fn vertical_runs(strong: &[bool], ws: usize, hs: usize, wander: u32) -> Vec<u32> {
    let wander = wander as usize;
    let chain = |rows: &mut dyn Iterator<Item = usize>| -> Vec<u32> {
        let mut out = vec![0u32; ws * hs];
        let mut prev: Option<usize> = None;
        for y in rows {
            for x in 0..ws {
                if !strong[y * ws + x] {
                    continue;
                }
                let best = prev.map_or(0, |p| {
                    let lo = x.saturating_sub(wander);
                    let hi = (x + wander).min(ws - 1);
                    (lo..=hi).map(|xx| out[p * ws + xx]).max().unwrap_or(0)
                });
                out[y * ws + x] = best + 1;
            }
            prev = Some(y);
        }
        out
    };
    let up = chain(&mut (0..hs));
    let down = chain(&mut (0..hs).rev());
    up.iter()
        .zip(&down)
        .map(|(u, d)| if *u == 0 { 0 } else { u + d - 1 })
        .collect()
}

/// A running minimum of `values` over `reach` rows each way.
fn running_min(values: &[f64], reach: usize) -> Vec<f64> {
    let n = values.len();
    (0..n)
        .map(|y| {
            let (lo, hi) = (y.saturating_sub(reach), (y + reach).min(n - 1));
            values[lo..=hi]
                .iter()
                .copied()
                .fold(f64::INFINITY, f64::min)
        })
        .collect()
}

/// The object cap, row by row: rascal's `object_cap`. Inside the row's opening and
/// beyond the seam's own gap, a step between two columns of at least `object_edge` and
/// `object_ratio` times the row's median, whose vertical run is at least `object_run`
/// of the height, is an object's edge. One on one side caps the row at its distance less
/// the margin; edges on both sides, or one too close to stop short of, put the object on
/// the line: the row stays thin, or with `object_straddle = "whole"` it and the rows in
/// reach are left to the rulers. The cap reaches `object_sigma` of the height each way as
/// a running minimum, and eases back over `object_taper` beyond. Infinity where nothing
/// stood in the way, and on every row when `object_edge` is 0.
fn object_cap(lum: &[f32], at: &Place, half: &[f64], s: &Settings) -> Vec<f64> {
    let (w, h) = (at.w, at.h);
    let mut cap = vec![f64::INFINITY; h as usize];
    if s.object_edge <= 0.0 || w < 3 {
        return cap;
    }
    let margin = s.object_margin_px * at.scale;
    let exclude = f64::from(at.gap.max(2));
    let c = f64::from(at.column);
    let (hs, ws) = (h as usize, w as usize - 1);
    let mut strong = vec![false; hs * ws];
    for y in 0..hs {
        let row = &lum[y * w as usize..(y + 1) * w as usize];
        let steps: Vec<f64> = row
            .windows(2)
            .map(|p| f64::from((p[1] - p[0]).abs()))
            .collect();
        let mut sorted = steps.clone();
        sorted.sort_by(f64::total_cmp);
        let threshold = s.object_edge.max(s.object_ratio * sorted[sorted.len() / 2]);
        for (x, step) in steps.iter().enumerate() {
            strong[y * ws + x] = *step >= threshold;
        }
    }
    let min_run = (s.object_run * f64::from(h)).round() as u32;
    let runs = vertical_runs(&strong, ws, hs, s.object_wander_px);
    let mut on_line = vec![false; hs];
    for y in 0..hs {
        let reach = half[y];
        let lo = ((c - reach).floor().max(0.0)) as usize;
        let hi = ((c + reach).ceil().min(f64::from(w) - 2.0)) as usize;
        let (mut left, mut right): (Option<f64>, Option<f64>) = (None, None);
        for x in lo..=hi {
            if !strong[y * ws + x] || runs[y * ws + x] < min_run {
                continue;
            }
            let d = (x as f64 + 0.5) - c;
            if d.abs() <= exclude {
                continue;
            }
            if d < 0.0 {
                left = Some(left.map_or(-d, |v| v.min(-d)));
            } else {
                right = Some(right.map_or(d, |v| v.min(d)));
            }
        }
        let nearest = match (left, right) {
            (Some(l), Some(r)) => Some(l.min(r)),
            (Some(d), None) | (None, Some(d)) => Some(d),
            (None, None) => None,
        };
        on_line[y] =
            (left.is_some() && right.is_some()) || nearest.is_some_and(|d| d - margin <= at.thin);
        cap[y] = match nearest {
            Some(_) if on_line[y] => at.thin,
            Some(d) => d - margin,
            None => f64::INFINITY,
        };
    }
    let reach = (s.object_sigma * f64::from(h)).round() as usize;
    let n = cap.len();
    let mut spread = if reach > 0 {
        running_min(&cap, reach)
    } else {
        cap
    };
    if s.object_straddle_whole {
        for (y, out) in spread.iter_mut().enumerate() {
            let (lo, hi) = (y.saturating_sub(reach), (y + reach).min(n - 1));
            if on_line[lo..=hi].iter().any(|b| *b) {
                *out = f64::INFINITY;
            }
        }
    }
    let taper = s.object_taper * f64::from(h);
    if taper <= 0.0 || spread.iter().all(|c| c.is_infinite()) {
        return spread;
    }
    let filled: Vec<f64> = spread
        .iter()
        .zip(half)
        .map(|(c, h)| if c.is_finite() { *c } else { *h })
        .collect();
    let carried = running_min(&filled, (3.0 * taper).ceil() as usize);
    let eased = smooth_rows(&carried, taper);
    spread
        .iter()
        .zip(eased)
        .zip(half)
        .map(|((c, e), h)| {
            if c.is_finite() {
                c.min(e)
            } else if e < *h - 1e-9 {
                e
            } else {
                f64::INFINITY
            }
        })
        .collect()
}

/// The tunables a call may give, each replacing its set's value, with its bounds:
/// rascal's.
const TUNABLES: [(&str, &str, f64, f64); 9] = [
    (
        "object_edge",
        "The object cap: the luminance step between two columns, in levels, that counts as an object's edge; 0 turns the cap off.",
        0.0,
        255.0,
    ),
    (
        "object_margin_px",
        "How far short of an object's edge the band stops, in pixels at the reference width.",
        0.0,
        10_000.0,
    ),
    (
        "tex_low",
        "The texture, in percent a pixel, at and under which the surface is wholly smooth.",
        0.0,
        100.0,
    ),
    (
        "tex_high",
        "The texture at and over which the band stays at its least width.",
        0.0,
        100.0,
    ),
    (
        "wide_max",
        "The widest half-width, as a share of the image's width; 0 keeps every row thin.",
        0.0,
        0.5,
    ),
    (
        "thin_px",
        "The least half-width in pixels at the reference width, scaled by the image's width.",
        0.5,
        10_000.0,
    ),
    (
        "row_sigma",
        "The smoothing of the two rulers along the rows, as a share of the height.",
        0.0,
        1.0,
    ),
    (
        "width_sigma",
        "The smoothing of the half-width along the rows, as a share of the height.",
        0.0,
        1.0,
    ),
    (
        "ref_width",
        "The image width the pixel constants were fitted at.",
        1.0,
        1_000_000.0,
    ),
];

/// The action `fan`.
pub struct Fan {
    descriptor: ActionDescriptor,
}

impl Fan {
    pub fn new() -> Fan {
        let sets: Vec<&str> = FAN.iter().map(|(name, _)| *name).collect();
        let mut draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "Measures the band along a column line: a half-width for each row, wide on smooth surfaces where the sides differ, thin on detail.",
        );
        for (name, purpose, min, max) in TUNABLES {
            let value = if name == "ref_width" {
                ValueType::integer(Some(min as i64), Some(max as i64))
            } else {
                ValueType::number(Some(min), Some(max))
            };
            draft = draft.argument(Argument::optional(name, purpose, value));
        }
        let draft = draft
            .argument(Argument::optional(
                "gate",
                "Whether the tone step across the line gates the opening: a row whose sides agree stays thin.",
                ValueType::Boolean,
            ))
            .argument(Argument::with_default(
                constants::ARGUMENT,
                "The constant set the values a call does not give are taken from.",
                ValueType::choice(&sets),
                Value::text(FAN[0].0),
            ))
            .input(InputPort::required(
                "image",
                ArtifactType::Image,
                "The image, which does not wrap: a cut's canvas, whose middle column is the seam.",
            ))
            .input(
                InputPort::required(
                    "line",
                    ArtifactType::Location,
                    "The line on the image, a whole column.",
                )
                .on("image"),
            )
            .output(
                OutputPort::one("band", ArtifactType::BandProfile, "The band profile.")
                    .on(PortRef::input("image")),
            )
            .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Fan { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Fan {
    fn default() -> Fan {
        Fan::new()
    }
}

/// Registers `fan`.
pub fn register(registry: &mut Registry) -> Result<(), RegistrationError> {
    registry.register(Arc::new(Fan::new()))
}

/// The settings of a call: its set's, with each tunable it gives in place.
pub fn settings_of(call: &PreparedCall) -> Result<(String, Settings), Failure> {
    let set = operation::text(call, constants::ARGUMENT).unwrap_or(FAN[0].0);
    let mut s = Settings::of(set).map_err(|why| Failure::invalid(&why))?;
    for (name, value) in call.arguments() {
        match (name.as_str(), value) {
            ("object_edge", Value::Number(v)) => s.object_edge = *v,
            ("object_margin_px", Value::Number(v)) => s.object_margin_px = *v,
            ("tex_low", Value::Number(v)) => s.tex_low = *v,
            ("tex_high", Value::Number(v)) => s.tex_high = *v,
            ("wide_max", Value::Number(v)) => s.wide_max = *v,
            ("thin_px", Value::Number(v)) => s.thin_px = *v,
            ("row_sigma", Value::Number(v)) => s.row_sigma = *v,
            ("width_sigma", Value::Number(v)) => s.width_sigma = *v,
            ("ref_width", Value::Integer(v)) => s.ref_width = *v as f64,
            ("gate", Value::Boolean(v)) => s.gate = *v,
            _ => {}
        }
    }
    if s.tex_high <= s.tex_low {
        return Err(Failure::invalid(&format!(
            "tex_high ({}) must be over tex_low ({})",
            s.tex_high, s.tex_low
        )));
    }
    Ok((set.to_owned(), s))
}

impl Action for Fan {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let (set, settings) = settings_of(call)?;
        let (image_id, line_id) = (canvas::one(call, "image")?, canvas::one(call, "line")?);
        let (image, picture) = paste::picture(context, image_id)?;
        let line = operation::location(context, line_id)?;
        if line.binding().source() != picture.digest() {
            return Err(Failure::invalid("the line is not on the image"));
        }
        if image.space().topology() != Topology::Bounded {
            return Err(Failure::invalid(
                "the image wraps; fan needs one that does not (cut the panorama first)",
            ));
        }
        let Shape::Line {
            line: Along::Column(column),
            ..
        } = line.shape()
        else {
            return Err(Failure::invalid(
                "fan reads a line that is a whole column; a path is for rasterize",
            ));
        };
        let width = image.space().width();
        let profile = fan_profile(picture.pixels(), *column, &settings).map_err(|clipped| {
            Failure::invalid(&format!(
                "the {} window clips to {} column(s) at column {column} of an image {width} wide; fan needs at least 2",
                clipped.0, clipped.1
            ))
        })?;
        if operation::stopped(context) {
            return Err(operation::stopped_failure(context, NAME));
        }
        let mut step = Step::new(NAME, VERSION);
        for (name, value) in call.arguments() {
            let text = match value {
                Value::Number(v) => text_of(*v),
                Value::Integer(v) => v.to_string(),
                Value::Boolean(v) => v.to_string(),
                Value::Text(_) => continue,
            };
            step = step.argument(name, &text);
        }
        let step = step
            .argument(constants::ARGUMENT, &set)
            .input("image", image_id.0)
            .input("line", line_id.0);
        let widened = profile
            .half
            .iter()
            .filter(|h| **h > profile.thin + 0.5)
            .count();
        let band = Origin::new(step)
            .and_then(|origin| {
                BandProfile::new(
                    image.binding(),
                    line_id,
                    profile.column,
                    profile.thin,
                    profile.half,
                    profile.windows,
                    &set,
                    origin,
                )
            })
            .map_err(|error| Failure::error("band", &error.without_paths()))?;
        context.events().progress(&format!(
            "fan: the band at column {} under {set}: thin {:.1} px, {widened} of {} rows widened",
            band.column(),
            band.thin(),
            band.half().len()
        ));
        Ok(Produced::nothing().with("band", band))
    }
}

/// What `whizbang rascal fan` takes beside what every operation's command does.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The image, which does not wrap, by its identity in the store.
    #[arg(long)]
    pub image: String,
    /// The line on it, a whole column, by its identity in the store.
    #[arg(long)]
    pub line: String,
    /// The constant set: fan-v3 (the default), fan-v1, fan-thin-v1 or fan-v2.
    #[arg(long, value_parser = ["fan-v3", "fan-v1", "fan-thin-v1", "fan-v2"])]
    pub constants: Option<String>,
    /// The widest half-width, as a share of the image's width, from 0 to 0.5.
    #[arg(long)]
    pub wide_max: Option<f64>,
}

impl operation::Asked for Args {
    fn action(&self) -> &'static str {
        NAME
    }

    fn arguments(&self) -> Vec<(&'static str, Value)> {
        let mut arguments = Vec::new();
        if let Some(set) = &self.constants {
            arguments.push((constants::ARGUMENT, Value::text(set)));
        }
        if let Some(wide_max) = self.wide_max {
            arguments.push(("wide_max", Value::Number(wide_max)));
        }
        arguments
    }

    fn inputs(&self) -> Vec<(&'static str, &'static str, &str)> {
        vec![
            ("image", "--image", self.image.as_str()),
            ("line", "--line", self.line.as_str()),
        ]
    }
}
