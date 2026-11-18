//! `align`: registers one image onto another of the same scene and viewpoint (`SPEC.md`
//! section 8, a control), and the estimator under it: a homography between two flat images,
//! by direct intensity registration on locally contrast-normalised pyramids, with the
//! measurements the tool reports: how much of the image the transform covers, how well the
//! registered pair agrees, and the share of pixels that agree closely. Pure functions over
//! pixels; no randomness, no dependency beyond the image crate.
//!
//! Ported from squint at the commit `docs/sources.toml` pins: the estimator from
//! `crates/squint-tools/src/align/` (`mod.rs`, `flat.rs`, `gray.rs`, `linalg.rs`), in its
//! arithmetic and order, since the baseline gate compares what it produced (plan
//! `docs/plans/m2c-milestone-2.md`, task A2, item 2); the tool from
//! `crates/squint-tools/src/builtin/align.rs` ([`tool`]). The spherical estimator
//! (`sphere.rs`) is not ported: whizbang's `align` registers flat images only
//! (`docs/carry-over.md`). The record is `docs/reference/tools/align.md`.
//!
//! Method: each image becomes a luminance pyramid; every level is locally contrast-normalised so
//! tone, colour and lighting differences between a source and its restyled candidate matter
//! little while structure matters. The coarsest level is searched over a small grid of
//! translations, rotations and scales by normalised cross-correlation, and each level from
//! there refines the transform by robust Gauss-Newton on the intensity residual (a Huber
//! weight tames pixels that genuinely changed). The finest level, capped in size for cost,
//! yields the quality measurements. A pair whose images lack texture is `LowTexture`, one
//! whose refinement does not converge is `NotConverged`, and one whose registered agreement
//! is low is `NotRegistered`; none is given a transform.

mod flat;
mod gray;
mod linalg;
mod tool;

pub use flat::{FlatAlignment, align_flat};
pub use linalg::{Mat3, apply, invert, matmul};
pub use tool::{
    ACTION_VERSION, Align, AlignError, Aligned, Frame, MAX_SIDE, Measurement, NAME, REPORT,
    Transform, VERSION, align, aligned_frame, report, summary,
};

/// What an estimate concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The transform registers the pair: coverage, agreement and inliers all above threshold.
    Registered,
    /// One image has too little structure for any transform to be determined.
    LowTexture,
    /// The refinement did not settle within its iterations or hit a singular system: a numerical
    /// failure that says nothing about the pair's geometry.
    NotConverged,
    /// The registered pair does not agree: different scenes, a different viewpoint, or a
    /// transform outside what `align` represents.
    NotRegistered,
}

/// Measurements of a registered pair, on the locally normalised images at the finest level used.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quality {
    /// The share of the source whose transform lands inside the candidate.
    pub overlap: f64,
    /// Normalised cross-correlation over the overlap, `-1` to `1`.
    pub ncc: f64,
    /// The share of overlapping pixels whose normalised residual is below the inlier threshold.
    pub inlier_fraction: f64,
    /// The weaker of the two images' texture scores (see `Gray::texture`).
    pub texture: f64,
}

/// Tunable thresholds and limits, with the defaults the spike measured.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// The coarsest pyramid level keeps at least this many pixels on its smaller side.
    pub min_side: usize,
    /// Gauss-Newton and the quality measurements run on the finest level whose longer side is
    /// at most this (the full image when smaller).
    pub max_side: usize,
    /// Gauss-Newton iterations per level.
    pub max_iterations: usize,
    /// Huber threshold on the normalised residual.
    pub huber: f32,
    /// A normalised residual below this is an inlier.
    pub inlier_residual: f32,
    /// Below this texture score an image is `LowTexture`.
    pub min_texture: f64,
    /// Registration thresholds.
    pub min_overlap: f64,
    pub min_ncc: f64,
    pub min_inliers: f64,
    /// Local contrast normalisation radius in pixels at every level.
    pub lcn_radius: usize,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            min_side: 24,
            max_side: 1024,
            max_iterations: 40,
            huber: 0.5,
            inlier_residual: 0.5,
            min_texture: 0.004,
            min_overlap: 0.3,
            min_ncc: 0.5,
            min_inliers: 0.5,
            lcn_radius: 4,
        }
    }
}

/// Added to the local variance before normalising: a local contrast below about 3% of full
/// scale (quantisation noise, faint gradients) is damped rather than amplified into structure.
const LCN_EPS: f32 = 1e-3;

/// How a level's refinement ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refinement {
    /// The step fell under the threshold.
    Settled,
    /// The iterations ran out or the step ran away: the pair may simply not fit.
    Exhausted,
    /// A singular system, non-finite parameters or too few samples: a numerical failure that
    /// says nothing about the pair.
    Failed,
}
/// Pixels this close to an image edge are not measured: the blur and the normalisation window
/// are clipped there.
const MARGIN: f64 = 6.0;
/// A normalised source pixel this far from its local mean counts as structure for the inlier
/// measurement.
const STRUCTURE_FLOOR: f64 = 0.5;
