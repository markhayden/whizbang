//! Synthetic scenes with known geometry for `align` (after squint's
//! `crates/squint-testkit/src/images.rs`): a flat scene evaluated analytically at every pixel,
//! so a rendering under a homography carries no resampling error and the truth is exact.
//! Squint's scene used the exponential and the sine, whose last digit a machine's
//! mathematics library may round its own way; this one uses additions, multiplications,
//! divisions and `floor` alone, so every machine paints the same pixels.

use whizbang_core::artifact::Image;

use super::support::picture;

/// SplitMix64, as squint's test kit had it.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[lo, hi)`.
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * ((self.next_u64() >> 11) as f64 / (1u64 << 53) as f64)
    }
}

/// A bump of height 1 at the centre that falls off as the square of `1 / (1 + d²/r²)`.
fn bump(d2: f64, r2: f64) -> f64 {
    let q = 1.0 / (1.0 + d2 / r2);
    q * q
}

/// A triangle-wave grating under a bump: period, direction, amplitude, centre, radius².
type Grating = (f64, [f64; 2], f64, [f64; 2], f64);

/// A flat scene in scene units: x from 0 to 1 across the source, y from 0 to its aspect.
pub struct Scene {
    base: [f64; 3],
    /// Centre, squared radius, and the amplitude of each channel.
    blobs: Vec<([f64; 2], f64, [f64; 3])>,
    gratings: Vec<Grating>,
}

impl Scene {
    pub fn random(seed: u64, blobs: usize, aspect: f64) -> Scene {
        let mut rng = Rng::new(seed);
        let base = [
            rng.range(0.35, 0.65),
            rng.range(0.35, 0.65),
            rng.range(0.35, 0.65),
        ];
        let blobs = (0..blobs)
            .map(|_| {
                let r = rng.range(0.006, 0.12);
                let amp = rng.range(0.2, 0.5) * if rng.range(0.0, 1.0) < 0.5 { -1.0 } else { 1.0 };
                let centre = [rng.range(-0.1, 1.1), rng.range(-0.1, aspect + 0.1)];
                let colour = [
                    amp * rng.range(0.5, 1.0),
                    amp * rng.range(0.5, 1.0),
                    amp * rng.range(0.5, 1.0),
                ];
                (centre, r * r, colour)
            })
            .collect();
        let gratings = (0..4)
            .map(|_| {
                let (dx, dy) = (rng.range(-1.0, 1.0), rng.range(-1.0, 1.0));
                let norm = (dx * dx + dy * dy).sqrt().max(0.1);
                let r = rng.range(0.08, 0.2);
                (
                    rng.range(0.08, 0.3),
                    [dx / norm, dy / norm],
                    rng.range(0.04, 0.1),
                    [rng.range(0.0, 1.0), rng.range(0.0, aspect)],
                    r * r,
                )
            })
            .collect();
        Scene {
            base,
            blobs,
            gratings,
        }
    }

    fn at(&self, x: f64, y: f64) -> [f64; 3] {
        let mut c = self.base;
        for (centre, r2, colour) in &self.blobs {
            let d2 = (x - centre[0]) * (x - centre[0]) + (y - centre[1]) * (y - centre[1]);
            let g = bump(d2, *r2);
            for (channel, amp) in c.iter_mut().zip(colour) {
                *channel += amp * g;
            }
        }
        for (period, dir, amp, centre, r2) in &self.gratings {
            let d2 = (x - centre[0]) * (x - centre[0]) + (y - centre[1]) * (y - centre[1]);
            let t = (x * dir[0] + y * dir[1]) / period;
            let tri = 2.0 * (t - t.floor() - 0.5).abs() - 0.5;
            for channel in &mut c {
                *channel += amp * bump(d2, *r2) * tri;
            }
        }
        c
    }

    /// The scene seen through `h` (source pixels to these pixels), `width` by `height`, of a
    /// source `source_width` wide; the colour at each pixel's centre, through `tone`.
    pub fn render(
        &self,
        source_width: u32,
        (width, height): (u32, u32),
        h: &[f64; 9],
        tone: fn(f64) -> f64,
    ) -> Image {
        let inv = invert(h);
        let scale = f64::from(source_width);
        picture(width, height, |u, v| {
            let (px, py) = (f64::from(u) + 0.5, f64::from(v) + 0.5);
            let (sx, sy) = apply(&inv, px, py);
            let [r, g, b] = self.at(sx / scale, sy / scale).map(|c| to_u8(tone(c)));
            [r, g, b, 255]
        })
    }
}

/// No change of tone.
pub fn same(v: f64) -> f64 {
    v
}

/// A change of tone a restyle might make: darker shadows, lifted highlights, an offset.
pub fn restyled(v: f64) -> f64 {
    let v = v.clamp(0.0, 1.0);
    0.08 + 0.7 * v * v + 0.25 * v
}

fn to_u8(v: f64) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

pub const IDENTITY: [f64; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

pub fn apply(h: &[f64; 9], x: f64, y: f64) -> (f64, f64) {
    let d = h[6] * x + h[7] * y + h[8];
    (
        (h[0] * x + h[1] * y + h[2]) / d,
        (h[3] * x + h[4] * y + h[5]) / d,
    )
}

pub fn invert(m: &[f64; 9]) -> [f64; 9] {
    let det = m[0] * (m[4] * m[8] - m[5] * m[7]) - m[1] * (m[3] * m[8] - m[5] * m[6])
        + m[2] * (m[3] * m[7] - m[4] * m[6]);
    [
        (m[4] * m[8] - m[5] * m[7]) / det,
        (m[2] * m[7] - m[1] * m[8]) / det,
        (m[1] * m[5] - m[2] * m[4]) / det,
        (m[5] * m[6] - m[3] * m[8]) / det,
        (m[0] * m[8] - m[2] * m[6]) / det,
        (m[2] * m[3] - m[0] * m[5]) / det,
        (m[3] * m[7] - m[4] * m[6]) / det,
        (m[1] * m[6] - m[0] * m[7]) / det,
        (m[0] * m[4] - m[1] * m[3]) / det,
    ]
}

pub fn matmul(a: &[f64; 9], b: &[f64; 9]) -> [f64; 9] {
    let mut out = [0.0; 9];
    for i in 0..3 {
        for j in 0..3 {
            out[i * 3 + j] = (0..3).map(|k| a[i * 3 + k] * b[k * 3 + j]).sum();
        }
    }
    out
}

/// A similarity about the centre of a `width` by `height` image: a scale, a turn whose
/// half-angle has the tangent `t` (so the cosine and sine are rational: no library call),
/// then a shift.
pub fn similarity(size: (u32, u32), scale: f64, t: f64, shift: (f64, f64)) -> [f64; 9] {
    let (cx, cy) = (f64::from(size.0) / 2.0, f64::from(size.1) / 2.0);
    let (c, s) = ((1.0 - t * t) / (1.0 + t * t), 2.0 * t / (1.0 + t * t));
    let (a, b) = (scale * c, scale * s);
    [
        a,
        -b,
        -a * cx + b * cy + cx + shift.0,
        b,
        a,
        -b * cx - a * cy + cy + shift.1,
        0.0,
        0.0,
        1.0,
    ]
}

/// `base` after a perspective term about the centre: the bottom row `[gx, gy, 1]` in centred
/// coordinates.
pub fn perspective(size: (u32, u32), base: &[f64; 9], gx: f64, gy: f64) -> [f64; 9] {
    let (cx, cy) = (f64::from(size.0) / 2.0, f64::from(size.1) / 2.0);
    let to_centre = [1.0, 0.0, -cx, 0.0, 1.0, -cy, 0.0, 0.0, 1.0];
    let from_centre = [1.0, 0.0, cx, 0.0, 1.0, cy, 0.0, 0.0, 1.0];
    let p = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, gx, gy, 1.0];
    matmul(base, &matmul(&from_centre, &matmul(&p, &to_centre)))
}
