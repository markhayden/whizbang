//! The tool `align`: registers `image` onto `against` with the flat estimator and returns the
//! aligned frame (the image warped into `against`'s grid, lossless), the transform with its
//! quality, and the agreement measurements. A pair that does not register, or that has too
//! little texture, or whose refinement did not converge, has no frame and no transform.
//!
//! Ported from squint, `crates/squint-tools/src/builtin/align.rs` and its manifest
//! `crates/squint-tools/manifests/align.toml`: the measurements, their names and units, the
//! summary's words and the aligned frame's warp, for flat images. Squint named its frame
//! `aligned:<image>:t1` and placed it on `against`'s raw frame; here the frame is an image
//! artifact, bound onto the image it was warped from by a homography (ADR 0001), so a
//! location on it maps back to that image as squint's `to_root` mapped one.

use image::RgbImage;
use whizbang_core::artifact::{self, Artifact, ArtifactError, ArtifactType, Image, ImageRole};
use whizbang_core::artifact::{Origin, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Context, Count, DescriptorError, Failure, InputPort, Kind,
    OutputPort, PreparedCall, Produced,
};
use whizbang_core::identity::{
    Binding, CanonicalPixels, DecodeLimits, Direction, IdentityError, Mapping, Projection, Topology,
};

use super::linalg::{self, Mat3};
use super::{Options, Quality, Status, align_flat};

/// The tool's name.
pub const NAME: &str = "align";
/// The version an aligned frame's origin records: squint's manifest's.
pub const VERSION: &str = "1.1.0";
/// The version of the action's descriptor.
pub const ACTION_VERSION: &str = "1.0.0";
/// The aligned frame's longer side is capped here; its coordinates stay the parent's.
pub const MAX_SIDE: u32 = 1024;

/// One measurement, as squint's envelope carried it.
#[derive(Debug, Clone, PartialEq)]
pub struct Measurement {
    pub name: &'static str,
    pub value: f64,
    pub unit: &'static str,
}

fn measurement(name: &'static str, value: f64, unit: &'static str) -> Measurement {
    Measurement { name, value, unit }
}

/// The registered transform.
#[derive(Debug, Clone, PartialEq)]
pub struct Transform {
    /// `image`'s pixels to `against`'s, row-major, the last entry one.
    pub homography: Mat3,
    /// The same between the two unit squares: what squint's envelope and its frame's
    /// mapping carried.
    pub params: Mat3,
}

/// The aligned frame: the image warped into `against`'s grid, and its binding onto the
/// image it was warped from.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub image: Image,
    pub binding: Binding,
}

/// What `align` found.
#[derive(Debug, Clone, PartialEq)]
pub struct Aligned {
    pub status: Status,
    pub quality: Quality,
    /// Squint's words for the motion and the quality, or for why there is none: the middle
    /// of the summary ([`summary`]).
    pub text: String,
    /// The motion at the image's centre when registered, then the four of the quality.
    pub measurements: Vec<Measurement>,
    pub transform: Option<Transform>,
    pub frame: Option<Frame>,
}

/// Why `align` returned nothing.
#[derive(Debug)]
pub enum AlignError {
    /// The request cannot be met as it was made; the text is for whoever made it.
    Invalid(String),
    /// An image could not be read, or the frame could not be made into an artifact.
    Artifact(ArtifactError),
    /// `align` did not make what it should have.
    Failed(String),
}

impl std::fmt::Display for AlignError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlignError::Invalid(why) => write!(f, "invalid request: {why}"),
            AlignError::Artifact(error) => error.fmt(f),
            AlignError::Failed(why) => write!(f, "align failed: {why}"),
        }
    }
}

impl std::error::Error for AlignError {}

impl From<ArtifactError> for AlignError {
    fn from(error: ArtifactError) -> AlignError {
        AlignError::Artifact(error)
    }
}

impl From<IdentityError> for AlignError {
    fn from(error: IdentityError) -> AlignError {
        AlignError::Artifact(ArtifactError::Identity(error))
    }
}

impl Status {
    /// The status as a word: `registered`, `low_texture`, `not_converged`, `not_registered`.
    pub fn word(self) -> &'static str {
        match self {
            Status::Registered => "registered",
            Status::LowTexture => "low_texture",
            Status::NotConverged => "not_converged",
            Status::NotRegistered => "not_registered",
        }
    }
}

/// The name squint gave the aligned frame of the image it called `image`.
pub fn aligned_frame(image: &str) -> String {
    format!("aligned:{image}:t1")
}

/// Squint's summary of what `align` found, with the two images named as the caller names
/// them.
pub fn summary(aligned: &Aligned, image: &str, against: &str) -> String {
    let text = &aligned.text;
    match aligned.status {
        Status::Registered => format!(
            "{image} registered onto {against}: {text}. The aligned frame {} shares \
             {against}'s grid; compare it with raw:{against}. Registration matches the camera, \
             not the content: a structure reshaped inside the frame registers just as well, \
             so a close fit is no evidence that structure held.",
            aligned_frame(image)
        ),
        Status::NotRegistered => format!(
            "{image} does not register onto {against}: {text}. The pair is not the same scene \
             from the same viewpoint with its geometry intact."
        ),
        Status::LowTexture => format!(
            "{image} and {against} could not be registered: {text}. Nothing follows about \
             their geometry."
        ),
        Status::NotConverged => format!(
            "{image} and {against} could not be registered: registration did not converge \
             ({text}). Nothing follows about their geometry."
        ),
    }
}

/// The image's pixels as squint loaded them: red, green and blue, the alpha dropped.
fn rgb(image: &Image, limits: &DecodeLimits) -> Result<RgbImage, AlignError> {
    let pixels = image.pixels(limits)?;
    let rgb: Vec<u8> = pixels
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    RgbImage::from_raw(pixels.width(), pixels.height(), rgb)
        .ok_or_else(|| AlignError::Failed("the pixels do not make an image".to_owned()))
}

/// Registers `image` onto `against`. Two flat images, different pictures.
pub fn align(
    image: &Image,
    against: &Image,
    options: &Options,
    limits: &DecodeLimits,
) -> Result<Aligned, AlignError> {
    if image.identity().pixels == against.identity().pixels {
        return Err(AlignError::Invalid(
            "image and against must be two different images".to_owned(),
        ));
    }
    for (named, which) in [(image, "image"), (against, "against")] {
        if named.space().projection() != Projection::Flat {
            return Err(AlignError::Invalid(format!(
                "{which} is a panorama: align registers flat images only, so far"
            )));
        }
    }
    let img = rgb(image, limits)?;
    let tgt = rgb(against, limits)?;
    let a = align_flat(&img, &tgt, options);
    let quality = a.quality;
    let text_quality = format!(
        "agreement {:.2}, inliers {:.2}, overlap {:.2}, texture {:.3}",
        quality.ncc, quality.inlier_fraction, quality.overlap, quality.texture
    );
    let mut aligned = Aligned {
        status: a.status,
        quality,
        text: match a.status {
            Status::LowTexture => format!("too little texture ({:.4})", quality.texture),
            _ => text_quality.clone(),
        },
        measurements: Vec::new(),
        transform: None,
        frame: None,
    };
    if a.status == Status::Registered {
        let h = a.homography;
        let (cx, cy) = (f64::from(img.width()) / 2.0, f64::from(img.height()) / 2.0);
        let m = Motion::of(&h, cx, cy);
        // A keystone is named as such: below a quarter of a percent it is resampling noise.
        let keystone = if m.perspective >= 0.0025 {
            format!(", perspective {:.1}%", m.perspective * 100.0)
        } else {
            String::new()
        };
        aligned.text = format!(
            "shift {:.1} px, rotation {:.2}°, scale {:.3}{keystone}; {text_quality}",
            m.shift, m.rotation, m.scale
        );
        aligned.measurements = vec![
            measurement("translation_px", m.shift, "px"),
            measurement("rotation_deg", m.rotation, "deg"),
            measurement("scale", m.scale, "ratio"),
            measurement("perspective", m.perspective, "ratio"),
        ];
        aligned.frame = Some(frame(image, against, &img, &tgt, &h, limits)?);
        aligned.transform = Some(Transform {
            homography: h,
            params: normalise_homography(&h, &img, &tgt),
        });
    }
    aligned.measurements.extend([
        measurement("ncc", quality.ncc, "ratio"),
        measurement("inlier_fraction", quality.inlier_fraction, "ratio"),
        measurement("overlap", quality.overlap, "ratio"),
        measurement("texture", quality.texture, "intensity per pixel"),
    ]);
    if aligned.measurements.iter().any(|m| !m.value.is_finite()) {
        return Err(AlignError::Failed(
            "a measurement is not a finite number".to_owned(),
        ));
    }
    Ok(aligned)
}

/// The aligned frame: `img` resampled into `tgt`'s grid, at most [`MAX_SIDE`] on its longer
/// side, as an image artifact bound onto `image` by the homography that takes the frame's
/// pixels to the image's.
fn frame(
    image: &Image,
    against: &Image,
    img: &RgbImage,
    tgt: &RgbImage,
    h: &Mat3,
    limits: &DecodeLimits,
) -> Result<Frame, AlignError> {
    let side = tgt.width().max(tgt.height()).min(MAX_SIDE);
    let scale = f64::from(side) / f64::from(tgt.width().max(tgt.height()));
    let warped = warp_flat(img, tgt, h, scale)?;
    let rgba: Vec<u8> = warped
        .pixels()
        .flat_map(|pixel| [pixel.0[0], pixel.0[1], pixel.0[2], 255])
        .collect();
    let shown = CanonicalPixels::from_rgba(warped.width(), warped.height(), rgba)?;
    let origin = Origin::new(
        Step::new(NAME, VERSION)
            .input("image", image.id().0)
            .input("against", against.id().0),
    )?;
    let file = crate::view::png(&shown)
        .map_err(|error| AlignError::Failed(format!("cannot encode the frame: {error}")))?;
    let made = Image::new(
        file,
        Projection::Flat,
        Topology::Bounded,
        ImageRole::Picture,
        origin,
        limits,
    )?;
    if made.identity().pixels != shown.digest() {
        return Err(AlignError::Failed(
            "the frame's file does not read back as its pixels".to_owned(),
        ));
    }
    let inverse = linalg::invert(h)
        .ok_or_else(|| AlignError::Failed("the registered homography is singular".to_owned()))?;
    let unscale = [1.0 / scale, 0.0, 0.0, 0.0, 1.0 / scale, 0.0, 0.0, 0.0, 1.0];
    let binding = Binding::new(
        made.identity().pixels,
        made.space(),
        image.identity().pixels,
        image.space(),
        Direction::SourceToDestination,
        Mapping::Homography {
            matrix: linalg::matmul(&inverse, &unscale),
        },
    )?;
    Ok(Frame {
        image: made,
        binding,
    })
}

/// The normalised homography (image `[0,1]²` to target `[0,1]²`) from a pixel one.
fn normalise_homography(h: &Mat3, img: &RgbImage, tgt: &RgbImage) -> Mat3 {
    let (iw, ih) = (f64::from(img.width()), f64::from(img.height()));
    let (tw, th) = (f64::from(tgt.width()), f64::from(tgt.height()));
    let to_pixels = [iw, 0.0, 0.0, 0.0, ih, 0.0, 0.0, 0.0, 1.0];
    let to_unit = [1.0 / tw, 0.0, 0.0, 0.0, 1.0 / th, 0.0, 0.0, 0.0, 1.0];
    let n = linalg::matmul(&to_unit, &linalg::matmul(h, &to_pixels));
    if n[8].abs() > 1e-300 {
        n.map(|v| v / n[8])
    } else {
        n
    }
}

/// A pixel homography read as camera motion at the image centre `(cx, cy)`: the shift of the
/// centre, the rotation and scale of the local linear map there (the Jacobian, so a projective
/// term does not leak into them), and `perspective`, the largest relative change of the
/// projective depth from the centre to a corner (0 for an affine map; a keystone of k gives k).
pub(crate) struct Motion {
    pub(crate) shift: f64,
    pub(crate) rotation: f64,
    pub(crate) scale: f64,
    pub(crate) perspective: f64,
}

impl Motion {
    pub(crate) fn of(h: &Mat3, cx: f64, cy: f64) -> Motion {
        let depth = |x: f64, y: f64| h[6] * x + h[7] * y + h[8];
        let w = depth(cx, cy);
        let (px, py) = linalg::apply(h, cx, cy);
        // d(p)/d(x, y) = (A - p · gᵀ) / w, with A the top-left 2×2 and g = (h6, h7).
        let j = [
            (h[0] - px * h[6]) / w,
            (h[1] - px * h[7]) / w,
            (h[3] - py * h[6]) / w,
            (h[4] - py * h[7]) / w,
        ];
        let perspective = [
            (0.0, 0.0),
            (2.0 * cx, 0.0),
            (0.0, 2.0 * cy),
            (2.0 * cx, 2.0 * cy),
        ]
        .iter()
        .map(|&(x, y)| (depth(x, y) / w - 1.0).abs())
        .fold(0.0, f64::max);
        Motion {
            shift: (px - cx).hypot(py - cy),
            rotation: (j[2] - j[1]).atan2(j[0] + j[3]).to_degrees(),
            scale: (j[0] * j[3] - j[1] * j[2]).abs().sqrt(),
            perspective,
        }
    }
}

/// `img` resampled into `tgt`'s grid, at `scale` of it, through the inverse of `h` (image px →
/// target px); pixels with no source are black.
fn warp_flat(img: &RgbImage, tgt: &RgbImage, h: &Mat3, scale: f64) -> Result<RgbImage, AlignError> {
    let (w, hh) = (
        ((f64::from(tgt.width()) * scale).round() as u32).max(1),
        ((f64::from(tgt.height()) * scale).round() as u32).max(1),
    );
    let inv = linalg::invert(h)
        .ok_or_else(|| AlignError::Failed("the registered homography is singular".to_owned()))?;
    let mut out = RgbImage::new(w, hh);
    for (u, v, p) in out.enumerate_pixels_mut() {
        // Target pixel centre at full resolution, then back into the image.
        let (tx, ty) = ((f64::from(u) + 0.5) / scale, (f64::from(v) + 0.5) / scale);
        let (sx, sy) = linalg::apply(&inv, tx, ty);
        if sx < 0.0 || sy < 0.0 || sx > f64::from(img.width()) || sy > f64::from(img.height()) {
            continue;
        }
        *p = image::Rgb(sample(img, sx, sy));
    }
    Ok(out)
}

/// Bilinear sample at a continuous edge-based coordinate (pixel `i` covers `[i, i+1)`),
/// clamped at the edges. Squint's `raster::sample` without the wrap.
fn sample(img: &RgbImage, x: f64, y: f64) -> [u8; 3] {
    let (w, h) = (f64::from(img.width()), f64::from(img.height()));
    let fx = (x - 0.5).clamp(0.0, w - 1.0);
    let fy = (y - 0.5).clamp(0.0, h - 1.0);
    let x0 = (fx.floor() as u32).min(img.width() - 1);
    let y0 = (fy.floor() as u32).min(img.height() - 1);
    let x1 = (x0 + 1).min(img.width() - 1);
    let y1 = (y0 + 1).min(img.height() - 1);
    let (ax, ay) = (fx - fx.floor(), fy - fy.floor());
    let p = |x: u32, y: u32| img.get_pixel(x, y).0;
    let (a, b, c, d) = (p(x0, y0), p(x1, y0), p(x0, y1), p(x1, y1));
    let mut out = [0u8; 3];
    for (k, o) in out.iter_mut().enumerate() {
        let top = f64::from(a[k]) * (1.0 - ax) + f64::from(b[k]) * ax;
        let bottom = f64::from(c[k]) * (1.0 - ax) + f64::from(d[k]) * ax;
        *o = (top * (1.0 - ay) + bottom * ay).round().clamp(0.0, 255.0) as u8;
    }
    out
}

/// The name of the diagnostic in which the action states what it found
/// ([`report`]): the status, the summary's middle, the measurements and the transform.
pub const REPORT: &str = "align.json";

/// What the action states of what it found, as JSON.
pub fn report(aligned: &Aligned) -> serde_json::Value {
    serde_json::json!({
        "status": aligned.status.word(),
        "text": aligned.text,
        "measurements": aligned.measurements.iter().map(|m| serde_json::json!({
            "name": m.name, "value": m.value, "unit": m.unit,
        })).collect::<Vec<_>>(),
        "params": aligned.transform.as_ref().map(|t| t.params.to_vec()),
    })
}

/// `align` as an action: it takes `image` and `against`, and returns the aligned frame at
/// `aligned` when the pair registers, and nothing otherwise. What it found, registered or
/// not, is in its diagnostic [`REPORT`]: a port returns artifacts, and a measurement is none
/// (`docs/carry-over.md`).
pub struct Align(ActionDescriptor);

impl Align {
    pub fn new() -> Result<Align, DescriptorError> {
        let draft = ActionDescriptor::draft(
            NAME,
            ACTION_VERSION,
            Kind::Primitive,
            "Registers one flat image onto another of the same scene and viewpoint, and returns it warped into the other's frame.",
        )
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The image to register, usually the candidate.",
        ))
        .input(InputPort::required(
            "against",
            ArtifactType::Image,
            "The image to register onto, usually the source: the aligned frame shares its grid.",
        ))
        .output(OutputPort::counted(
            "aligned",
            ArtifactType::Image,
            "The image warped into the other's grid, when the pair registers.",
            Count::between(0, 1),
        ))
        .concurrent(true);
        Ok(Align(draft.build()?))
    }
}

impl Action for Align {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let limits = DecodeLimits::default();
        let failed = |why: &dyn std::fmt::Display| Failure::error(NAME, &why.to_string());
        let at = |port: &str| -> Result<Image, Failure> {
            let Some(input) = call.inputs().iter().find(|input| input.port() == port) else {
                return Err(failed(&format!("the call gives no {port}")));
            };
            match artifact::get(context.store(), ArtifactType::Image, input.id(), &limits) {
                Ok(Artifact::Image(image)) => Ok(image),
                Ok(_) => Err(failed(&"the input is not an image")),
                Err(error) => Err(failed(&error)),
            }
        };
        let (image, against) = (at("image")?, at("against")?);
        let aligned =
            align(&image, &against, &Options::default(), &limits).map_err(|error| match error {
                AlignError::Invalid(why) => Failure::invalid(&why),
                other => failed(&other),
            })?;
        let said = report(&aligned).to_string();
        let kept = context.diagnostics().write(REPORT, said.as_bytes());
        kept.map_err(|why| failed(&why))?;
        let mut produced = Produced::nothing();
        if let Some(frame) = aligned.frame {
            produced = produced.with("aligned", frame.image);
        }
        Ok(produced)
    }
}
