//! The gradient-domain solve behind `paste`'s mode `poisson` (`SPEC.md` section 7). The
//! domain is every pixel the mask weighs over 0, which leaves out every protected pixel.
//! Per channel, the discrete Poisson equation on the grid of four neighbours over the
//! domain takes the **fill's gradients** inside it and **the base's values** at every
//! neighbour in the image outside it, protected holes included; a pixel on the image's
//! border has one term fewer, and nothing wraps (`paste` refuses a picture that wraps
//! before it calls in). On a grid of four neighbours, a domain with nothing outside it to
//! anchor to is exactly the whole image, and it is reported instead of solved.
//!
//! Conjugate gradients in 32-bit floats from the fill as the first guess: the residual of
//! the guess is evaluated before every test, the first included, and the solve has
//! converged when `|r| <= tol * max(|b|, 1)` for the true residual `b - A x` (the
//! recurrence's residual gates the test, the true one confirms it); reaching `max_iter` is
//! an error, never a fallback. The solution stays in 32-bit floats, held to `[0, 255]`;
//! the composite blends it by the weight and rounds once.
//!
//! Ported from rascal `crates/rascal-ops/src/poisson.rs` at the commit `docs/sources.toml`
//! pins, the arithmetic unchanged, with one difference, on purpose: the three channels are
//! solved one after another on the caller's thread, where rascal started a thread for
//! each; each channel's arithmetic is its own, so the solution is the same. The solve
//! stops between iterations when told to.

use whizbang_core::artifact::{Mask, effective_weight};

use crate::constants;

/// The solver's numbers: a constant set's values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// The relative tolerance on the residual.
    pub tol: f32,
    /// The ceiling of iterations; reaching it is an error.
    pub max_iter: usize,
}

impl Settings {
    /// The numbers of a set's text.
    pub fn of(set: &str) -> Result<Settings, String> {
        let number = |key: &str| {
            constants::number(set, key).ok_or_else(|| format!("the constant set has no {key}"))
        };
        Ok(Settings {
            tol: number("tol")? as f32,
            max_iter: number("max_iter")? as usize,
        })
    }
}

/// What a solve came to.
#[derive(Debug, Clone, PartialEq)]
pub enum Solve {
    /// The solution: red, green and blue for each pixel, row by row, as 32-bit floats;
    /// every pixel of the domain the solve's, held to `[0, 255]`, and every other the
    /// fill's.
    Solved {
        solution: Vec<f32>,
        /// The pixels of the domain.
        domain_px: u64,
        /// The most iterations a channel took.
        iterations: usize,
    },
    /// The domain has nothing outside it to anchor to.
    Unanchored {
        /// The pixels of that domain.
        component_px: u64,
    },
}

/// No pixel or unknown at this slot.
const NONE: u32 = u32::MAX;

/// The domain as a sparse system: one unknown for each pixel of the domain, its four
/// neighbours in the image (`NONE` off it), the unknown at each (`NONE` outside the
/// domain), and how many neighbours it has in the image (the diagonal).
struct Domain {
    pixels: Vec<u32>,
    in_image: Vec<[u32; 4]>,
    neighbours: Vec<[u32; 4]>,
    degree: Vec<f32>,
}

impl Domain {
    fn of(mask: &Mask) -> Domain {
        let space = mask.space();
        let (width, height) = (space.width() as usize, space.height() as usize);
        let mut index = vec![NONE; width * height];
        let mut pixels = Vec::new();
        for (slot, at) in index.iter_mut().enumerate() {
            if effective_weight(mask.core()[slot], mask.blend()[slot], mask.protect()[slot]) > 0.0 {
                *at = pixels.len() as u32;
                pixels.push(slot as u32);
            }
        }
        let mut in_image = Vec::with_capacity(pixels.len());
        let mut neighbours = Vec::with_capacity(pixels.len());
        let mut degree = Vec::with_capacity(pixels.len());
        for &slot in &pixels {
            let slot = slot as usize;
            let (x, y) = (slot % width, slot / width);
            let around = [
                if x > 0 { (slot - 1) as u32 } else { NONE },
                if x + 1 < width {
                    (slot + 1) as u32
                } else {
                    NONE
                },
                if y > 0 { (slot - width) as u32 } else { NONE },
                if y + 1 < height {
                    (slot + width) as u32
                } else {
                    NONE
                },
            ];
            in_image.push(around);
            neighbours.push(around.map(|q| if q == NONE { NONE } else { index[q as usize] }));
            degree.push(around.iter().filter(|&&q| q != NONE).count() as f32);
        }
        Domain {
            pixels,
            in_image,
            neighbours,
            degree,
        }
    }

    /// `y = A x`: the degree on the diagonal, less the neighbours in the domain.
    fn apply(&self, x: &[f32], y: &mut [f32]) {
        for (i, out) in y.iter_mut().enumerate() {
            let mut v = self.degree[i] * x[i];
            for &q in &self.neighbours[i] {
                if q != NONE {
                    v -= x[q as usize];
                }
            }
            *out = v;
        }
    }

    /// One channel's right-hand side: the fill's Laplacian over the neighbours in the
    /// image, and the base at each neighbour in the image outside the domain.
    fn rhs(&self, base: &[f32], fill: &[f32]) -> Vec<f32> {
        self.pixels
            .iter()
            .enumerate()
            .map(|(i, &slot)| {
                let slot = slot as usize;
                let mut v = 0.0f32;
                for (k, &q) in self.in_image[i].iter().enumerate() {
                    if q != NONE {
                        v += fill[slot] - fill[q as usize];
                        if self.neighbours[i][k] == NONE {
                            v += base[q as usize];
                        }
                    }
                }
                v
            })
            .collect()
    }
}

fn dot(a: &[f32], b: &[f32]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(&x, &y)| f64::from(x) * f64::from(y))
        .sum()
}

fn norm(a: &[f32]) -> f64 {
    dot(a, a).sqrt()
}

/// Why a channel has no solution, or that the solve was told to stop.
enum Stop {
    NotConverged {
        iterations: usize,
        residual: f64,
        threshold: f64,
    },
    /// `p A p <= 0`: not positive definite, which an anchored domain never is.
    Breakdown {
        iterations: usize,
    },
    Stopped,
}

impl Stop {
    fn describe(&self, channel: usize) -> String {
        match self {
            Stop::NotConverged {
                iterations,
                residual,
                threshold,
            } => format!(
                "poisson: channel {channel} not converged after {iterations} iteration{}: residual {residual:.3e} above tol times max(|b|, 1) = {threshold:.3e}",
                if *iterations == 1 { "" } else { "s" }
            ),
            Stop::Breakdown { iterations } => format!(
                "poisson: channel {channel}: conjugate gradients broke down at iteration {iterations} (the system is not positive definite)"
            ),
            Stop::Stopped => "poisson: stopped".to_owned(),
        }
    }
}

/// Conjugate gradients on one channel from the guess `x`; the iterations taken.
fn conjugate_gradients(
    domain: &Domain,
    b: &[f32],
    x: &mut [f32],
    settings: &Settings,
    stopped: &dyn Fn() -> bool,
) -> Result<usize, Stop> {
    let n = x.len();
    let threshold = f64::from(settings.tol) * norm(b).max(1.0);
    let mut ax = vec![0.0f32; n];
    let mut r = vec![0.0f32; n];
    let true_residual = |x: &[f32], ax: &mut [f32], r: &mut [f32]| {
        domain.apply(x, ax);
        for i in 0..n {
            r[i] = b[i] - ax[i];
        }
        norm(r)
    };
    let mut residual = true_residual(x, &mut ax, &mut r);
    // Whether `residual` is the true residual of `x` (at first, and after a restart) or
    // the recurrence's, which only gates the test.
    let mut residual_is_true = true;
    let mut p = r.clone();
    let mut ap = vec![0.0f32; n];
    let mut rr = dot(&r, &r);
    let mut iterations = 0usize;
    loop {
        if residual <= threshold {
            if residual_is_true {
                return Ok(iterations);
            }
            residual = true_residual(x, &mut ax, &mut r);
            if residual <= threshold {
                return Ok(iterations);
            }
            p.copy_from_slice(&r);
            rr = dot(&r, &r);
        }
        if iterations >= settings.max_iter {
            return Err(Stop::NotConverged {
                iterations,
                residual,
                threshold,
            });
        }
        if stopped() {
            return Err(Stop::Stopped);
        }
        domain.apply(&p, &mut ap);
        let pap = dot(&p, &ap);
        if pap <= 0.0 {
            return Err(Stop::Breakdown { iterations });
        }
        let alpha = (rr / pap) as f32;
        for i in 0..n {
            x[i] += alpha * p[i];
            r[i] -= alpha * ap[i];
        }
        let rr_next = dot(&r, &r);
        let beta = (rr_next / rr) as f32;
        for i in 0..n {
            p[i] = r[i] + beta * p[i];
        }
        rr = rr_next;
        residual = rr.sqrt();
        residual_is_true = false;
        iterations += 1;
    }
}

/// The solve over `base` and `fill`, four bytes a pixel each, of the mask's size; the
/// alpha is not read. An error, in a sentence, when a channel does not converge; none
/// when `stopped` says so.
pub fn solve(
    base: &[u8],
    fill: &[u8],
    mask: &Mask,
    settings: &Settings,
    stopped: &dyn Fn() -> bool,
) -> Result<Option<Solve>, String> {
    let space = mask.space();
    let samples = space.width() as usize * space.height() as usize;
    if base.len() != samples * 4 || fill.len() != samples * 4 {
        return Err("poisson: the base, the fill and the mask must be one size".to_owned());
    }
    let domain = Domain::of(mask);
    if domain.pixels.len() == samples {
        return Ok(Some(Solve::Unanchored {
            component_px: domain.pixels.len() as u64,
        }));
    }
    let channel =
        |rgba: &[u8], c: usize| -> Vec<f32> { rgba.chunks(4).map(|p| f32::from(p[c])).collect() };
    let mut solution: Vec<f32> = fill
        .chunks(4)
        .flat_map(|p| [f32::from(p[0]), f32::from(p[1]), f32::from(p[2])])
        .collect();
    let mut iterations = 0usize;
    for c in 0..3 {
        let (base, fill) = (channel(base, c), channel(fill, c));
        let b = domain.rhs(&base, &fill);
        let mut x: Vec<f32> = domain.pixels.iter().map(|&s| fill[s as usize]).collect();
        let taken = match conjugate_gradients(&domain, &b, &mut x, settings, stopped) {
            Ok(taken) => taken,
            Err(Stop::Stopped) => return Ok(None),
            Err(other) => return Err(other.describe(c)),
        };
        iterations = iterations.max(taken);
        for (i, &slot) in domain.pixels.iter().enumerate() {
            solution[slot as usize * 3 + c] = x[i].clamp(0.0, 255.0);
        }
    }
    Ok(Some(Solve::Solved {
        solution,
        domain_px: domain.pixels.len() as u64,
        iterations,
    }))
}
