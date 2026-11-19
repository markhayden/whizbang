//! `tighten`: a rough stroke refined to the defect's edges (`SPEC.md` section 7). The path
//! is resampled; at each sample the luminance is read along the perpendicular over
//! `[-r, r]`, its signed derivative taken, and the best pair of opposite-signed edges
//! inside the stroke chosen; a sample without one stays at the full extent; runs of
//! tightened extents are smoothed and a crossing reverts. The core is the tube between
//! consecutive samples' extents with round joins and caps, so a stroke tightened nowhere is
//! exactly the mask `rasterize` draws for it.
//!
//! Rascal's decisions, kept: ties after the score break toward the midpoint nearest the
//! path, then the narrower pair, then the lower index; the direction at an end is
//! one-sided and the path is never extended past its ends; a zero central difference takes
//! the previous sample's direction, the first such the next nonzero one, and a path with no
//! nonzero direction is refused. The perpendicular is read at `ceil(2r)` equal steps over
//! `[-r, r]`, about a pixel each, so the profile is symmetric about the path.
//!
//! **The numbers** a call does not give are its constant set's (the argument `constants`,
//! ADR 0033), `tighten-v1`.
//!
//! Ported from rascal `crates/rascal-ops/src/tighten.rs` at the commit `docs/sources.toml`
//! pins, the arithmetic unchanged, with these differences, on purpose: the path is in
//! pixels of the image (ADR 0002), as rascal's became after its own step from fractions; a
//! path with no direction and a stroke that covers no pixel, which rascal said did not
//! apply, are refused; and rascal's second output, the series `tightened` of one row for
//! each sample, is not handed back, because a series here has one row for each row of its
//! image, and a stroke has as many samples as it is long: [`tighten`] returns the extents,
//! and the action says how many samples it tightened.

use std::sync::Arc;

use whizbang_core::artifact::{ArtifactType, Location, Mask, Origin, Shape, Source, Step, UseCase};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Failure, InputPort, Kind, OutputPort, PortRef,
    PreparedCall, Produced, RegistrationError, Registry, Value, ValueType,
};
use whizbang_core::identity::Point;

use crate::constants::{self, TIGHTEN_V1};
use crate::{operation, paste};

/// The action's name.
pub const NAME: &str = "tighten";

/// The action's version, which a mask it made records in its origin.
pub const VERSION: &str = "1.0.0";

/// The constant sets a call may name.
pub const SETS: [&str; 1] = [TIGHTEN_V1.0];

/// What the command says the action does.
pub const ABOUT: &str = "Tighten a stroke to the defect's edges: at each sample along its path, the two strongest \
opposite-signed luminance edges inside the stroke bound the core; a sample without them \
keeps the stroke's radius. The mask, for a repair, is committed to the store, and its \
identity is printed.";

/// The numbers a call may give, with `tighten-v1`'s values unless it does, and their
/// bounds: rascal's.
pub const TUNABLES: [(&str, &str, f64, f64); 5] = [
    (
        "min_contrast",
        "The least size of the luminance's derivative, in levels per pixel, an edge must have to count.",
        0.0,
        255.0,
    ),
    (
        "min_defect_px",
        "The narrowest defect, in pixels between its two edges.",
        0.0,
        1e4,
    ),
    (
        "max_defect_factor",
        "The widest defect, as a multiple of the stroke's radius (2 is the whole stroke).",
        0.01,
        10.0,
    ),
    (
        "path_sigma",
        "The sigma, in samples, of the smoothing of the tightened extents along the path.",
        0.0,
        1e3,
    ),
    (
        "sample_spacing_px",
        "The spacing of the samples along the path, in pixels.",
        0.25,
        1e3,
    ),
];

/// The five numbers of a call: each given, or its constant set's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tunables {
    pub min_contrast: f64,
    pub min_defect_px: f64,
    pub max_defect_factor: f64,
    pub path_sigma: f64,
    pub sample_spacing_px: f64,
}

impl Tunables {
    /// Each number `given` gives, and the rest from the set's text.
    pub fn of(set: &str, given: impl Fn(&str) -> Option<f64>) -> Result<Tunables, String> {
        let value = |key: &str| {
            given(key)
                .or_else(|| constants::number(set, key))
                .ok_or_else(|| format!("the constant set has no {key}"))
        };
        Ok(Tunables {
            min_contrast: value("min_contrast")?,
            min_defect_px: value("min_defect_px")?,
            max_defect_factor: value("max_defect_factor")?,
            path_sigma: value("path_sigma")?,
            sample_spacing_px: value("sample_spacing_px")?,
        })
    }

    /// Each number by its name, in the order of [`TUNABLES`].
    pub fn named(&self) -> [(&'static str, f64); 5] {
        [
            ("min_contrast", self.min_contrast),
            ("min_defect_px", self.min_defect_px),
            ("max_defect_factor", self.max_defect_factor),
            ("path_sigma", self.path_sigma),
            ("sample_spacing_px", self.sample_spacing_px),
        ]
    }
}

/// The resolved numbers one call runs with, in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub radius: f64,
    pub min_contrast: f64,
    pub min_defect: f64,
    pub max_defect: f64,
    pub path_sigma: f64,
    pub spacing: f64,
}

/// One sample's extent across the path, in pixels along the perpendicular.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extent {
    pub a: f64,
    pub b: f64,
    pub tightened: bool,
}

impl Extent {
    /// The stroke's full radius, untightened.
    pub fn full(radius: f64) -> Extent {
        Extent {
            a: -radius,
            b: radius,
            tightened: false,
        }
    }
}

/// What [`tighten`] made: the core, one byte a pixel, 255 inside; each sample's extent;
/// and the radius it ran with, the stroke's at most the image's diagonal.
#[derive(Debug, Clone, PartialEq)]
pub struct Tightened {
    pub core: Vec<u8>,
    pub extents: Vec<Extent>,
    pub radius: f64,
}

impl Tightened {
    /// How many samples were tightened.
    pub fn tightened(&self) -> usize {
        self.extents.iter().filter(|e| e.tightened).count()
    }
}

/// The path resampled at `spacing` pixels by linear interpolation: the first point, every
/// multiple of the spacing along the cumulative length, and the last point.
pub fn resample(path: &[[f64; 2]], spacing: f64) -> Vec<[f64; 2]> {
    let spacing = spacing.max(1e-6);
    let Some(first) = path.first() else {
        return Vec::new();
    };
    let mut out = vec![*first];
    // The distance travelled since the last sample.
    let mut carried = 0.0;
    for seg in path.windows(2) {
        let (p, q) = (seg[0], seg[1]);
        let len = ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt();
        if len == 0.0 {
            continue;
        }
        let mut along = spacing - carried;
        while along <= len + 1e-9 {
            let t = along / len;
            out.push([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])]);
            along += spacing;
        }
        carried = len - (along - spacing);
    }
    if let Some(last) = path.last()
        && out
            .last()
            .is_some_and(|o| ((o[0] - last[0]).powi(2) + (o[1] - last[1]).powi(2)).sqrt() > 1e-6)
    {
        out.push(*last);
    }
    out
}

/// The unit direction at every sample: the central difference of its neighbours,
/// one-sided at the ends; a zero difference takes the previous sample's direction, the
/// first such the next nonzero one. None when no sample has a nonzero direction.
pub fn directions(samples: &[[f64; 2]]) -> Option<Vec<[f64; 2]>> {
    let n = samples.len();
    if n < 2 {
        return None;
    }
    let raw: Vec<Option<[f64; 2]>> = (0..n)
        .map(|i| {
            let (p, q) = if i == 0 {
                (samples[0], samples[1])
            } else if i == n - 1 {
                (samples[n - 2], samples[n - 1])
            } else {
                (samples[i - 1], samples[i + 1])
            };
            let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
            let len = (dx * dx + dy * dy).sqrt();
            (len > 1e-9).then(|| [dx / len, dy / len])
        })
        .collect();
    let first_nonzero = raw.iter().find_map(|d| *d)?;
    let mut out = Vec::with_capacity(n);
    let mut previous = first_nonzero;
    for d in raw {
        let d = d.unwrap_or(previous);
        out.push(d);
        previous = d;
    }
    Some(out)
}

/// The luminance `0.299 R + 0.587 G + 0.114 B` of each pixel of four bytes, as rascal
/// read it, the alpha left out.
pub fn luminance(rgba: &[u8]) -> Vec<f32> {
    rgba.chunks(4)
        .map(|p| 0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2]))
        .collect()
}

/// Luminance by bilinear interpolation at a point in pixels (pixel `i` is centred at
/// `i + 0.5`), clamped to the image.
pub fn bilinear(plane: &[f32], width: u32, height: u32, x: f64, y: f64) -> f64 {
    let (w, h) = (i64::from(width), i64::from(height));
    let fx = (x - 0.5).clamp(0.0, (w - 1) as f64);
    let fy = (y - 0.5).clamp(0.0, (h - 1) as f64);
    let (x0, y0) = (fx.floor() as i64, fy.floor() as i64);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (tx, ty) = (fx - x0 as f64, fy - y0 as f64);
    let at = |xx: i64, yy: i64| f64::from(plane[(yy * w + xx) as usize]);
    let top = at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx;
    let bottom = at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx;
    top * (1.0 - ty) + bottom * ty
}

/// A sample's luminance along its perpendicular.
pub struct Profile {
    /// The offsets from the path: `ceil(2r) + 1` equal steps from `-r` to `r`.
    pub offsets: Vec<f64>,
    pub values: Vec<f64>,
    /// The step between offsets, in pixels.
    pub step: f64,
}

/// Reads the luminance along the perpendicular `n` through `p` over `[-r, r]` at
/// `ceil(2r)` equal steps, symmetric about the path.
pub fn perpendicular_profile(
    read: &impl Fn(f64, f64) -> f64,
    p: [f64; 2],
    n: [f64; 2],
    radius: f64,
) -> Profile {
    let steps = (2.0 * radius).ceil().max(1.0) as usize;
    let step = 2.0 * radius / steps as f64;
    let offsets: Vec<f64> = (0..=steps).map(|k| -radius + k as f64 * step).collect();
    let values = offsets
        .iter()
        .map(|t| read(p[0] + t * n[0], p[1] + t * n[1]))
        .collect();
    Profile {
        offsets,
        values,
        step,
    }
}

/// The signed first derivative per pixel by central differences over samples `step`
/// apart, one-sided at the ends.
pub fn derivative(values: &[f64], step: f64) -> Vec<f64> {
    let n = values.len();
    let step = if step > 0.0 { step } else { 1.0 };
    (0..n)
        .map(|i| {
            if n < 2 {
                0.0
            } else if i == 0 {
                (values[1] - values[0]) / step
            } else if i == n - 1 {
                (values[n - 1] - values[n - 2]) / step
            } else {
                (values[i + 1] - values[i - 1]) / (2.0 * step)
            }
        })
        .collect()
}

/// The best pair of edges `(a, b)`, `a < b`: derivatives of opposite signs, each at least
/// `min_contrast` in size, `min_defect <= offset[b] - offset[a] <= max_defect`, with the
/// highest `|g(a)| + |g(b)|`; ties break toward the midpoint nearest the path, then the
/// narrower pair, then the lower first index.
pub fn best_pair(g: &[f64], offsets: &[f64], settings: &Settings) -> Option<(usize, usize)> {
    let mut best: Option<((usize, usize), f64, f64, f64)> = None;
    for a in 0..g.len() {
        if g[a].abs() < settings.min_contrast {
            continue;
        }
        for b in a + 1..g.len() {
            if g[b].abs() < settings.min_contrast || g[a].signum() == g[b].signum() {
                continue;
            }
            let width = offsets[b] - offsets[a];
            if width < settings.min_defect - 1e-9 || width > settings.max_defect + 1e-9 {
                continue;
            }
            let score = g[a].abs() + g[b].abs();
            let midpoint = ((offsets[a] + offsets[b]) / 2.0).abs();
            let better = match &best {
                None => true,
                Some((_, s, m, w)) => {
                    score > *s + 1e-9
                        || ((score - s).abs() <= 1e-9
                            && (midpoint < *m - 1e-9
                                || ((midpoint - m).abs() <= 1e-9 && width < *w - 1e-9)))
                }
            };
            if better {
                best = Some(((a, b), score, midpoint, width));
            }
        }
    }
    best.map(|(pair, _, _, _)| pair)
}

/// A Gaussian smoothing of a sequence with this sigma, the kernel cut at three sigmas
/// and renormalised at the ends; a sigma under half a sample, or fewer than two values,
/// changes nothing. Rascal's `smooth_rows`.
pub fn smooth_rows(values: &[f64], sigma: f64) -> Vec<f64> {
    if sigma < 0.5 || values.len() < 2 {
        return values.to_vec();
    }
    let radius = (3.0 * sigma).ceil() as usize;
    let kernel: Vec<f64> = (0..=radius)
        .map(|k| (-(k as f64).powi(2) / (2.0 * sigma * sigma)).exp())
        .collect();
    let n = values.len();
    (0..n)
        .map(|i| {
            let mut sum = 0.0;
            let mut weight = 0.0;
            let lo = i.saturating_sub(radius);
            let hi = (i + radius).min(n - 1);
            for (j, value) in values.iter().enumerate().take(hi + 1).skip(lo) {
                let w = kernel[i.abs_diff(j)];
                sum += w * value;
                weight += w;
            }
            sum / weight
        })
        .collect()
}

/// Smooths `a` and `b` apart over every run of tightened samples; a sample whose smoothed
/// `a >= b` reverts to the full extent. One kernel smooths both, so proper extents never
/// cross by smoothing alone; the reversion is the guard.
pub fn smooth_extents(extents: &mut [Extent], sigma: f64, radius: f64) {
    let n = extents.len();
    let mut start = 0;
    while start < n {
        if !extents[start].tightened {
            start += 1;
            continue;
        }
        let mut end = start;
        while end < n && extents[end].tightened {
            end += 1;
        }
        let a: Vec<f64> = extents[start..end].iter().map(|e| e.a).collect();
        let b: Vec<f64> = extents[start..end].iter().map(|e| e.b).collect();
        let (sa, sb) = (smooth_rows(&a, sigma), smooth_rows(&b, sigma));
        for (i, e) in extents[start..end].iter_mut().enumerate() {
            if sa[i] >= sb[i] {
                *e = Extent::full(radius);
            } else {
                e.a = sa[i];
                e.b = sb[i];
            }
        }
        start = end;
    }
}

/// The core: for each segment between consecutive samples, the pixels whose foot on the
/// segment lies inside it and whose signed offset is within the extents interpolated
/// along it, and a round cap at each sample of its half-extent, centred on its midpoint.
/// None when `stopped` says so, between rows.
pub fn stroke_core(
    samples: &[[f64; 2]],
    directions: &[[f64; 2]],
    extents: &[Extent],
    width: u32,
    height: u32,
    stopped: &dyn Fn() -> bool,
) -> Option<Vec<u8>> {
    let mut core = vec![0u8; width as usize * height as usize];
    if samples.is_empty() {
        return Some(core);
    }
    let reach = extents
        .iter()
        .map(|e| e.a.abs().max(e.b.abs()))
        .fold(0.0_f64, f64::max);
    let (min_x, max_x, min_y, max_y) = samples.iter().fold(
        (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ),
        |acc, p| {
            (
                acc.0.min(p[0]),
                acc.1.max(p[0]),
                acc.2.min(p[1]),
                acc.3.max(p[1]),
            )
        },
    );
    let x_lo = (min_x - reach - 1.0).floor().max(0.0) as u32;
    let x_hi = ((max_x + reach + 1.0).ceil().max(0.0) as u32).min(width);
    let y_lo = (min_y - reach - 1.0).floor().max(0.0) as u32;
    let y_hi = ((max_y + reach + 1.0).ceil().max(0.0) as u32).min(height);
    // The caps: a centre and a radius for each sample, along its own normal.
    let caps: Vec<([f64; 2], f64)> = samples
        .iter()
        .zip(directions)
        .zip(extents)
        .map(|((p, d), e)| {
            let n = [-d[1], d[0]];
            let m = (e.a + e.b) / 2.0;
            ([p[0] + m * n[0], p[1] + m * n[1]], (e.b - e.a) / 2.0)
        })
        .collect();
    for y in y_lo..y_hi {
        if stopped() {
            return None;
        }
        let cy = f64::from(y) + 0.5;
        for x in x_lo..x_hi {
            let cx = f64::from(x) + 0.5;
            let in_cap = caps
                .iter()
                .any(|(c, r)| (cx - c[0]).powi(2) + (cy - c[1]).powi(2) <= r * r + 1e-9);
            let in_tube = !in_cap
                && samples.windows(2).enumerate().any(|(i, seg)| {
                    let (p, q) = (seg[0], seg[1]);
                    let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
                    let len2 = dx * dx + dy * dy;
                    if len2 == 0.0 {
                        return false;
                    }
                    let u = ((cx - p[0]) * dx + (cy - p[1]) * dy) / len2;
                    if !(0.0..=1.0).contains(&u) {
                        return false;
                    }
                    let len = len2.sqrt();
                    // The signed offset along the segment's left normal (-dy, dx).
                    let d = (-dy * (cx - p[0]) + dx * (cy - p[1])) / len;
                    let (ea, eb) = (&extents[i], &extents[i + 1]);
                    let a = ea.a + u * (eb.a - ea.a);
                    let b = ea.b + u * (eb.b - ea.b);
                    d >= a - 1e-9 && d <= b + 1e-9
                });
            if in_cap || in_tube {
                core[(y * width + x) as usize] = 255;
            }
        }
    }
    Some(core)
}

/// Tightens the stroke along `path`, in pixels, with this radius, on an image `width` by
/// `height` of these pixels, four bytes each. The radius is held to the image's diagonal.
/// Refused, in a sentence: a path with no nonzero direction, and a stroke that covers no
/// pixel. None when `stopped` says so.
pub fn tighten(
    path: &[Point],
    radius: f64,
    rgba: &[u8],
    width: u32,
    height: u32,
    tunables: &Tunables,
    stopped: &dyn Fn() -> bool,
) -> Result<Option<Tightened>, String> {
    let diagonal = (f64::from(width).powi(2) + f64::from(height).powi(2)).sqrt();
    let radius = radius.min(diagonal);
    let settings = Settings {
        radius,
        min_contrast: tunables.min_contrast,
        min_defect: tunables.min_defect_px,
        max_defect: tunables.max_defect_factor * radius,
        path_sigma: tunables.path_sigma,
        spacing: tunables.sample_spacing_px,
    };
    let pixels: Vec<[f64; 2]> = path.iter().map(|p| [p.x, p.y]).collect();
    let samples = resample(&pixels, settings.spacing);
    let Some(directions) = directions(&samples) else {
        return Err(
            "the path has no nonzero direction: every sample sits on the same point".to_owned(),
        );
    };
    let lum = luminance(rgba);
    let read = |x: f64, y: f64| bilinear(&lum, width, height, x, y);
    let mut extents: Vec<Extent> = samples
        .iter()
        .zip(&directions)
        .map(|(p, d)| {
            let n = [-d[1], d[0]];
            let profile = perpendicular_profile(&read, *p, n, settings.radius);
            let g = derivative(&profile.values, profile.step);
            match best_pair(&g, &profile.offsets, &settings) {
                Some((a, b)) => Extent {
                    a: profile.offsets[a],
                    b: profile.offsets[b],
                    tightened: true,
                },
                None => Extent::full(settings.radius),
            }
        })
        .collect();
    smooth_extents(&mut extents, settings.path_sigma, settings.radius);
    let Some(core) = stroke_core(&samples, &directions, &extents, width, height, stopped) else {
        return Ok(None);
    };
    if core.iter().all(|sample| *sample == 0) {
        return Err("the stroke covers no pixel of the image".to_owned());
    }
    Ok(Some(Tightened {
        core,
        extents,
        radius,
    }))
}

/// The action `tighten`.
pub struct Tighten {
    descriptor: ActionDescriptor,
}

impl Tighten {
    pub fn new() -> Tighten {
        let mut draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "Tightens a stroke to the defect's edges: at each sample the two strongest opposite edges inside the stroke bound the core.",
        );
        for (name, purpose, min, max) in TUNABLES {
            draft = draft.argument(Argument::optional(
                name,
                purpose,
                ValueType::number(Some(min), Some(max)),
            ));
        }
        let draft = draft
            .argument(Argument::with_default(
                constants::ARGUMENT,
                "The constant set a number is taken from when the call does not give it.",
                ValueType::choice(&SETS),
                Value::text(TIGHTEN_V1.0),
            ))
            .input(
                InputPort::required(
                    "stroke",
                    ArtifactType::Location,
                    "The stroke, a location on the image.",
                )
                .on("image"),
            )
            .input(InputPort::required(
                "image",
                ArtifactType::Image,
                "The image the edges are read from.",
            ))
            .output(
                OutputPort::one(
                    "mask",
                    ArtifactType::Mask,
                    "The tightened stroke as the core of a mask for a repair; no blend and nothing protected.",
                )
                .on(PortRef::input("image")),
            )
            .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Tighten { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Tighten {
    fn default() -> Tighten {
        Tighten::new()
    }
}

/// Registers `tighten`.
pub fn register(registry: &mut Registry) -> Result<(), RegistrationError> {
    registry.register(Arc::new(Tighten::new()))
}

/// What `whizbang rascal tighten` takes beside what every operation's command does.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The stroke, a location on the image, by its identity in the store.
    #[arg(long)]
    pub stroke: String,
    /// The image, by its identity in the store.
    #[arg(long)]
    pub image: String,
    /// The least size of an edge, in luminance levels per pixel. Unset: tighten-v1's, 12.
    #[arg(long)]
    pub min_contrast: Option<f64>,
    /// The narrowest defect, in pixels. Unset: tighten-v1's, 1.
    #[arg(long)]
    pub min_defect_px: Option<f64>,
    /// The widest defect, as a multiple of the radius. Unset: tighten-v1's, 2.
    #[arg(long)]
    pub max_defect_factor: Option<f64>,
    /// The smoothing along the path, in samples. Unset: tighten-v1's, 4.
    #[arg(long)]
    pub path_sigma: Option<f64>,
    /// The spacing of the samples, in pixels. Unset: tighten-v1's, 1.
    #[arg(long)]
    pub sample_spacing_px: Option<f64>,
}

impl operation::Asked for Args {
    fn action(&self) -> &'static str {
        NAME
    }

    fn arguments(&self) -> Vec<(&'static str, Value)> {
        let given = [
            ("min_contrast", self.min_contrast),
            ("min_defect_px", self.min_defect_px),
            ("max_defect_factor", self.max_defect_factor),
            ("path_sigma", self.path_sigma),
            ("sample_spacing_px", self.sample_spacing_px),
        ];
        given
            .into_iter()
            .filter_map(|(name, value)| value.map(|v| (name, Value::Number(v))))
            .collect()
    }

    fn inputs(&self) -> Vec<(&'static str, &'static str, &str)> {
        vec![
            ("stroke", "--stroke", self.stroke.as_str()),
            ("image", "--image", self.image.as_str()),
        ]
    }
}

/// The mask of a tightened stroke on the location's image: for a repair, its source the
/// stroke, its origin one step that names this action, every number it ran with and the
/// set, the stroke and the image.
fn mask_of(location: &Location, core: Vec<u8>, step: Step) -> Result<Mask, Failure> {
    let source = Source {
        locations: vec![location.id()],
        instances: Vec::new(),
    };
    let origin =
        Origin::new(step).map_err(|error| Failure::error("mask", &error.without_paths()))?;
    Mask::from_core(
        location.binding().clone(),
        UseCase::Repair,
        core,
        source,
        origin,
    )
    .map_err(|error| Failure::error("mask", &error.without_paths()))
}

impl Action for Tighten {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let set = operation::text(call, constants::ARGUMENT).unwrap_or(TIGHTEN_V1.0);
        if set != TIGHTEN_V1.0 {
            return Err(Failure::invalid(&format!(
                "{set} is not a constant set of tighten"
            )));
        }
        let tunables = Tunables::of(TIGHTEN_V1.1, |key| operation::number(call, key))
            .map_err(|why| Failure::error("constants", &why))?;
        let stroke_id = operation::one(call, "stroke")?;
        let location = operation::location(context, stroke_id)?;
        let Shape::Stroke { path, radius } = location.shape() else {
            return Err(Failure::invalid(&format!(
                "tighten takes a stroke location, not a {}",
                crate::rasterize::kind(location.shape())
            )));
        };
        let image_id = operation::one(call, "image")?;
        let (_, picture) = paste::picture(context, image_id)?;
        let stopped = || operation::stopped(context);
        let made = tighten(
            path,
            *radius,
            picture.rgba(),
            picture.width(),
            picture.height(),
            &tunables,
            &stopped,
        )
        .map_err(|why| Failure::invalid(&why))?;
        let Some(made) = made else {
            return Err(operation::stopped_failure(context, NAME));
        };
        let mut step = Step::new(NAME, VERSION);
        for (name, value) in tunables.named() {
            step = step.argument(name, &value.to_string());
        }
        let step = step
            .argument(constants::ARGUMENT, set)
            .input("stroke", stroke_id.0)
            .input("image", image_id.0);
        context.events().progress(&format!(
            "tighten: {} of {} samples tightened; a core of {} pixels (radius {} px)",
            made.tightened(),
            made.extents.len(),
            operation::set(&made.core),
            made.radius
        ));
        let mask = mask_of(&location, made.core, step)?;
        Ok(Produced::nothing().with("mask", mask))
    }
}
