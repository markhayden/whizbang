//! The perspective view into a complete panorama (ADR 0051): a square view that looks along
//! a yaw and a pitch, with a field of view, and the arithmetic that maps its points onto
//! the panorama and back. The mapping of kind 5 in `binding/1` is this arithmetic, and so
//! are the pixels `view` shows: a view's pixel and the point its binding maps it to are one.
//!
//! Ported from squint, `crates/squint-core/src/frames/sphere.rs` (`View`, `direction`,
//! `project`, `equirect_of`, `direction_of_equirect`), at the commit `docs/sources.toml`
//! pins: stated here in pixels, where squint's was in fractions of each image, and with
//! `libm`'s functions, where squint's were the platform's, so that a mapped point and a
//! view's pixels are the same on every machine.
//!
//! The functions a view's every pixel goes through are marked `#[inline]`, so the crate
//! that renders a view builds them at its own level of optimisation.

use std::f64::consts::{FRAC_PI_2, PI};

use super::{IdentityError, Result};

/// A square perspective view, in degrees: where it looks, and how wide.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Perspective {
    /// Degrees from the panorama's middle column, growing to the right.
    pub yaw: f64,
    /// Degrees above the horizon.
    pub pitch: f64,
    /// The field of view across, and down, in degrees.
    pub fov: f64,
}

impl Perspective {
    /// Checks the three against ADR 0051's bounds: each finite, `-180 <= yaw < 180`,
    /// `-90 <= pitch <= 90` and `0 < fov < 180`.
    pub fn check(&self) -> Result<()> {
        let Perspective { yaw, pitch, fov } = *self;
        // A value that is not finite is outside every range below, so each angle is
        // named by its own refusal.
        if !(-180.0..180.0).contains(&yaw) {
            return Err(IdentityError::Binding(format!(
                "yaw {yaw} is not from -180 up to 180"
            )));
        }
        if !(-90.0..=90.0).contains(&pitch) {
            return Err(IdentityError::Binding(format!(
                "pitch {pitch} is not from -90 to 90"
            )));
        }
        if !(fov > 0.0 && fov < 180.0) {
            return Err(IdentityError::Binding(format!(
                "fov {fov} is not between 0 and 180"
            )));
        }
        Ok(())
    }

    /// The view's three directions and the tangent of half its field of view, worked out
    /// once for every point that goes through it.
    pub fn basis(&self) -> Basis {
        let (yaw, pitch) = (self.yaw.to_radians(), self.pitch.to_radians());
        let (sy, cy) = (libm::sin(yaw), libm::cos(yaw));
        let (sp, cp) = (libm::sin(pitch), libm::cos(pitch));
        Basis {
            forward: [cp * cy, cp * sy, sp],
            right: [-sy, cy, 0.0],
            up: [-sp * cy, -sp * sy, cp],
            tan_half: libm::tan(self.fov.to_radians() / 2.0),
        }
    }
}

/// A view's forward, right and up unit vectors, and `tan(fov / 2)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Basis {
    forward: [f64; 3],
    right: [f64; 3],
    up: [f64; 3],
    tan_half: f64,
}

impl Basis {
    /// The point of a panorama `width` by `height` that the point `(x, y)` of a view
    /// `side` pixels square looks at. Its column is from 0 to `width`, both ends included:
    /// the caller takes it round the seam.
    #[inline]
    pub fn to_panorama(&self, side: f64, width: f64, height: f64, x: f64, y: f64) -> (f64, f64) {
        let t = self.tan_half;
        let rightward = (2.0 * x / side - 1.0) * t;
        let upward = (1.0 - 2.0 * y / side) * t;
        let (f, r, u) = (self.forward, self.right, self.up);
        let d = [
            f[0] + rightward * r[0] + upward * u[0],
            f[1] + rightward * r[1] + upward * u[1],
            f[2] + rightward * r[2] + upward * u[2],
        ];
        let length = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let lon = libm::atan2(d[1], d[0]);
        let lat = libm::asin((d[2] / length).clamp(-1.0, 1.0));
        (
            (lon + PI) / (2.0 * PI) * width,
            (FRAC_PI_2 - lat) / PI * height,
        )
    }

    /// The point of a view `side` pixels square at which the point `(x, y)` of a panorama
    /// `width` by `height` is seen, or `None` when it is behind the view: its depth along
    /// the view's forward direction is at most `1e-9`. The point may lie outside the view.
    #[inline]
    pub fn from_panorama(
        &self,
        side: f64,
        width: f64,
        height: f64,
        x: f64,
        y: f64,
    ) -> Option<(f64, f64)> {
        let lon = x / width * 2.0 * PI - PI;
        let lat = FRAC_PI_2 - y / height * PI;
        let cos_lat = libm::cos(lat);
        let d = [
            cos_lat * libm::cos(lon),
            cos_lat * libm::sin(lon),
            libm::sin(lat),
        ];
        let dot = |v: [f64; 3]| d[0] * v[0] + d[1] * v[1] + d[2] * v[2];
        let depth = dot(self.forward);
        if depth <= 1e-9 {
            return None;
        }
        let rightward = dot(self.right) / depth / self.tan_half;
        let upward = dot(self.up) / depth / self.tan_half;
        Some(((rightward + 1.0) / 2.0 * side, (1.0 - upward) / 2.0 * side))
    }
}
