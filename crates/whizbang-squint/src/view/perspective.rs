//! The square perspective view into a panorama (ADR 0051): its size, and its pixels, each
//! the panorama sampled bilinearly where the view's mapping puts the pixel's centre.
//!
//! Ported from squint, `crates/squint-tools/src/builtin/view.rs` (`view_side`,
//! `perspective`) and `raster.rs` (`sample`), at the commit `docs/sources.toml` pins: on
//! canonical pixels, alpha sampled as the colours are, and through the core's statement of
//! the mapping, whose functions are `libm`'s, so the same view has the same pixels on every
//! machine.

use whizbang_core::identity::{CanonicalPixels, Perspective};

use super::{MAX_SIDE, ViewError};

/// A perspective view is never smaller than this on a side.
pub const MIN_VIEW: u32 = 256;

/// The side of a perspective view of a panorama `width` pixels wide: about the panorama's
/// own resolution over the field of view, `width × fov / 360` to the nearest pixel, held
/// to [`MIN_VIEW`] at least and [`MAX_SIDE`] at most. Squint's `view_side`.
pub fn side(width: u32, fov: f64) -> u32 {
    let side = (f64::from(width) * fov / 360.0).round();
    if side >= f64::from(MAX_SIDE) {
        MAX_SIDE
    } else if side <= f64::from(MIN_VIEW) {
        MIN_VIEW
    } else {
        side as u32
    }
}

/// The view of a complete panorama's pixels, `side` pixels square: pixel `(u, v)` shows
/// the panorama at the forward mapping of `(u + 0.5, v + 0.5)`.
pub fn looked(
    panorama: &CanonicalPixels,
    view: Perspective,
    side: u32,
) -> Result<CanonicalPixels, ViewError> {
    let basis = view.basis();
    let (width, height) = (panorama.width(), panorama.height());
    let rgba = panorama.rgba();
    let span = f64::from(side);
    let mut out = Vec::with_capacity(side as usize * side as usize * 4);
    for v in 0..side {
        for u in 0..side {
            let (x, y) = basis.to_panorama(
                span,
                f64::from(width),
                f64::from(height),
                f64::from(u) + 0.5,
                f64::from(v) + 0.5,
            );
            out.extend_from_slice(&sample(rgba, width, height, x, y));
        }
    }
    Ok(CanonicalPixels::from_rgba(side, side, out)?)
}

/// The bilinear sample at a point of a panorama, between the centres of the four pixels
/// around it: columns taken round the seam, rows held to the first and the last. Each of
/// the four channels apart, rounded to the nearest level, a half away from zero. Squint's
/// `raster::sample` with its wrap, on four channels.
fn sample(rgba: &[u8], width: u32, height: u32, x: f64, y: f64) -> [u8; 4] {
    let (w, h) = (f64::from(width), f64::from(height));
    let mut fx = (x - 0.5).rem_euclid(w);
    // A remainder of a small negative number can round to the width itself, which is
    // column 0 in a wrapped space, as `Space::place` takes it.
    if fx >= w {
        fx = 0.0;
    }
    let fy = (y - 0.5).clamp(0.0, h - 1.0);
    let x0 = (fx.floor() as u32).min(width - 1);
    let y0 = (fy.floor() as u32).min(height - 1);
    let x1 = (x0 + 1) % width;
    let y1 = (y0 + 1).min(height - 1);
    let (ax, ay) = (fx - fx.floor(), fy - fy.floor());
    let at = |x: u32, y: u32| {
        let i = (y as usize * width as usize + x as usize) * 4;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    };
    let (a, b, c, d) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
    let mut out = [0u8; 4];
    for (k, o) in out.iter_mut().enumerate() {
        let top = f64::from(a[k]) * (1.0 - ax) + f64::from(b[k]) * ax;
        let bottom = f64::from(c[k]) * (1.0 - ax) + f64::from(d[k]) * ax;
        *o = (top * (1.0 - ay) + bottom * ay).round().clamp(0.0, 255.0) as u8;
    }
    out
}
