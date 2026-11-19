//! The 95% Wilson score interval for a binomial proportion. Ported from squint's
//! `crates/squint-eval/src/wilson.rs`, at the commit `docs/sources.toml` pins.

/// The two-sided 95% normal quantile.
pub const Z_95: f64 = 1.959_963_984_540_054;

/// The Wilson interval for `successes` of `trials`; `None` when there are no trials, or
/// more successes than trials.
pub fn wilson(successes: u64, trials: u64, z: f64) -> Option<(f64, f64)> {
    if trials == 0 || successes > trials {
        return None;
    }
    let n = trials as f64;
    let p = successes as f64 / n;
    let z2 = z * z;
    let denominator = 1.0 + z2 / n;
    let centre = (p + z2 / (2.0 * n)) / denominator;
    let half = z * ((p * (1.0 - p) / n) + z2 / (4.0 * n * n)).sqrt() / denominator;
    Some(((centre - half).max(0.0), (centre + half).min(1.0)))
}
