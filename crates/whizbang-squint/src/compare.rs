//! `change_map` and `edge_similarity`: two flat images in one grid compared on structure
//! (plan `docs/plans/m2c-milestone-2.md`, task X1, item 9). Both images are brought to one
//! working size and their luminance lightly blurred; edges are gradient magnitudes, agreement
//! the normalised cross-correlation of the two edge maps, and the change map the absolute
//! difference of the locally contrast-normalised luminances.
//!
//! Ported from squint at the commit `docs/sources.toml` pins:
//! `crates/squint-tools/src/builtin/compare.rs` and its manifests
//! `crates/squint-tools/manifests/change_map.toml` and `edge_similarity.toml`, in their
//! arithmetic and order, over the luminance of `align` ([`crate::align`], squint's
//! `align/gray.rs`). Squint refused the same frame id twice; here the two images must be two
//! different pictures, since an image's identity is its pixels. Both are experimental
//! tools (`SPEC.md` section 8): each runs only when a run names it. The records are
//! `docs/reference/tools/change_map.md` and `edge_similarity.md`.

use image::RgbImage;
use image::imageops::{self, FilterType};
use serde_json::{Value as Json, json};
use whizbang_core::artifact::{self, Artifact, ArtifactError, ArtifactType, Image, ImageRole};
use whizbang_core::artifact::{Origin, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Context, Count, DescriptorError, Draft, Failure, InputPort, Kind,
    OutputPort, PreparedCall, Produced, ResultKind,
};
use whizbang_core::identity::{
    Binding, CanonicalPixels, DecodeLimits, Direction, Mapping, Projection, Topology,
};

use crate::align::gray::Gray;

/// The change map's name.
pub const CHANGE_MAP: &str = "change_map";
/// The edge similarity's name.
pub const EDGE_SIMILARITY: &str = "edge_similarity";
/// The versions a result's origin records: squint's manifests'.
pub const CHANGE_MAP_VERSION: &str = "1.1.0";
pub const EDGE_SIMILARITY_VERSION: &str = "1.0.0";
/// The version of each action's descriptor.
pub const ACTION_VERSION: &str = "1.0.0";
/// The name of the diagnostic in which each action states what it measured ([`report`]).
pub const CHANGE_MAP_REPORT: &str = "change_map.json";
pub const EDGE_SIMILARITY_REPORT: &str = "edge_similarity.json";

/// The longer side both images are brought to before comparing.
const WORKING_SIDE: u32 = 768;
/// A normalised residual beyond this counts as changed; one at this saturates the map.
const CHANGED: f32 = 1.0;
const SATURATION: f32 = 3.0;

/// Why a comparison gave nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompareError {
    /// The request cannot be met as it was made; the sentence is for whoever made it.
    Invalid(String),
    /// An image could not be read, or the result could not be made.
    Failed(String),
}

impl std::fmt::Display for CompareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompareError::Invalid(why) => write!(f, "invalid request: {why}"),
            CompareError::Failed(why) => write!(f, "compare failed: {why}"),
        }
    }
}

impl From<ArtifactError> for CompareError {
    fn from(error: ArtifactError) -> CompareError {
        CompareError::Failed(error.to_string())
    }
}

/// One measurement, with squint's name and unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurement {
    pub name: &'static str,
    pub value: f64,
    pub unit: &'static str,
}

/// What `edge_similarity` measured: the correlation of the two edge maps, and the weaker
/// image's mean edge strength.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edges {
    pub similarity: f64,
    pub energy: f64,
}

impl Edges {
    pub fn measurements(&self) -> Vec<Measurement> {
        vec![
            Measurement {
                name: "edge_similarity",
                value: self.similarity,
                unit: "ratio",
            },
            Measurement {
                name: "edge_energy",
                value: self.energy,
                unit: "intensity per pixel",
            },
        ]
    }
}

/// What `change_map` measured: the map, grey, at the working size in `frame`'s grid, and the
/// share of its pixels whose normalised luminance differs by more than one unit.
#[derive(Debug, Clone, PartialEq)]
pub struct Changes {
    pub map: RgbImage,
    pub fraction: f64,
}

impl Changes {
    pub fn measurements(&self) -> Vec<Measurement> {
        vec![Measurement {
            name: "changed_fraction",
            value: self.fraction,
            unit: "ratio",
        }]
    }
}

/// Squint's check that two pictures share one grid: the same aspect ratio within 1% (sizes
/// may differ: an aligned frame is capped).
fn one_grid(frame: &RgbImage, against: &RgbImage) -> Result<(), CompareError> {
    let aspect = |i: &RgbImage| f64::from(i.width()) / f64::from(i.height().max(1));
    if (aspect(frame) / aspect(against) - 1.0).abs() > 0.01 {
        return Err(CompareError::Invalid(format!(
            "frame is {}×{} and against is {}×{}; the images must share one grid",
            frame.width(),
            frame.height(),
            against.width(),
            against.height()
        )));
    }
    Ok(())
}

/// Both images at `frame`'s aspect ratio and the working size, as blurred luminance.
fn working(frame: &RgbImage, against: &RgbImage) -> (Gray, Gray, u32, u32) {
    let (w, h) = frame.dimensions();
    let scale = f64::from(WORKING_SIDE.min(w.max(h))) / f64::from(w.max(h));
    let ww = ((f64::from(w) * scale).round() as u32).max(2);
    let hh = ((f64::from(h) * scale).round() as u32).max(2);
    let fit = |img: &RgbImage| {
        if img.dimensions() == (ww, hh) {
            img.clone()
        } else {
            imageops::resize(img, ww, hh, FilterType::Triangle)
        }
    };
    (
        Gray::from_rgb(&fit(frame)).blur3(),
        Gray::from_rgb(&fit(against)).blur3(),
        ww,
        hh,
    )
}

fn edge_magnitude(g: &Gray) -> Gray {
    let (gx, gy) = g.gradients();
    let mut out = Gray::new(g.width, g.height);
    for ((o, a), b) in out.data.iter_mut().zip(gx.data.iter()).zip(gy.data.iter()) {
        *o = (a * a + b * b).sqrt();
    }
    out
}

/// Normalised cross-correlation of two maps; zero when either has no variation.
fn ncc(a: &Gray, b: &Gray) -> f64 {
    let n = a.data.len() as f64;
    let (ma, mb) = (
        a.data.iter().map(|v| f64::from(*v)).sum::<f64>() / n,
        b.data.iter().map(|v| f64::from(*v)).sum::<f64>() / n,
    );
    let (mut cov, mut va, mut vb) = (0.0f64, 0.0f64, 0.0f64);
    for (x, y) in a.data.iter().zip(b.data.iter()) {
        let (dx, dy) = (f64::from(*x) - ma, f64::from(*y) - mb);
        cov += dx * dy;
        va += dx * dx;
        vb += dy * dy;
    }
    if va <= 1e-12 || vb <= 1e-12 {
        return 0.0;
    }
    (cov / (va * vb).sqrt()).clamp(-1.0, 1.0)
}

/// `edge_similarity` of two pictures in one grid: squint's arithmetic.
pub fn edges(frame: &RgbImage, against: &RgbImage) -> Result<Edges, CompareError> {
    one_grid(frame, against)?;
    let (ga, gb, _, _) = working(frame, against);
    let (ea, eb) = (edge_magnitude(&ga), edge_magnitude(&gb));
    let similarity = ncc(&ea, &eb);
    let energy = |e: &Gray| e.data.iter().map(|v| f64::from(*v)).sum::<f64>() / e.data.len() as f64;
    Ok(Edges {
        similarity,
        energy: energy(&ea).min(energy(&eb)),
    })
}

/// `change_map` of two pictures in one grid: squint's arithmetic.
pub fn changes(frame: &RgbImage, against: &RgbImage) -> Result<Changes, CompareError> {
    one_grid(frame, against)?;
    let (ga, gb, w, h) = working(frame, against);
    let (la, lb) = (ga.lcn(4, 1e-3), gb.lcn(4, 1e-3));
    let mut map = RgbImage::new(w, h);
    let mut changed = 0usize;
    for (p, (x, y)) in map.pixels_mut().zip(la.data.iter().zip(lb.data.iter())) {
        let d = (x - y).abs();
        if d > CHANGED {
            changed += 1;
        }
        let v = ((d / SATURATION).clamp(0.0, 1.0) * 255.0).round() as u8;
        *p = image::Rgb([v, v, v]);
    }
    Ok(Changes {
        map,
        fraction: changed as f64 / f64::from(w * h),
    })
}

/// Squint's sentence of an edge similarity, the two images named as the caller names them.
pub fn edges_summary(edges: &Edges, frame: &str, against: &str) -> String {
    format!(
        "Edge similarity of {frame} and {against}: {:.3} (1 is the same structure; the weaker \
         frame's edge energy is {:.4}).",
        edges.similarity, edges.energy
    )
}

/// Squint's sentence of a change map, the map named as `map`.
pub fn changes_summary(changes: &Changes, frame: &str, against: &str, map: &str) -> String {
    format!(
        "Change map of {frame} against {against} ({map}, in {frame}'s grid): {:.1}% of pixels \
         differ in structure; bright is different, dark the same.",
        changes.fraction * 100.0
    )
}

/// What an action states of what it measured, as JSON: its measurements.
pub fn report(measurements: &[Measurement]) -> Json {
    json!({
        "measurements": measurements.iter().map(|m| json!({
            "name": m.name, "value": m.value, "unit": m.unit,
        })).collect::<Vec<_>>(),
    })
}

/// The image's pixels as squint loaded them: red, green and blue, the alpha dropped.
pub fn rgb(image: &Image, limits: &DecodeLimits) -> Result<RgbImage, CompareError> {
    if image.space().projection() != Projection::Flat {
        return Err(CompareError::Invalid(
            "a panorama is not compared: change_map and edge_similarity take flat images"
                .to_owned(),
        ));
    }
    let pixels = image.pixels(limits)?;
    let rgb: Vec<u8> = pixels
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    RgbImage::from_raw(pixels.width(), pixels.height(), rgb)
        .ok_or_else(|| CompareError::Failed("the pixels do not make an image".to_owned()))
}

/// Two images of a call, read and held to being two different flat pictures.
fn pair(
    frame: &Image,
    against: &Image,
    limits: &DecodeLimits,
) -> Result<(RgbImage, RgbImage), CompareError> {
    if frame.identity().pixels == against.identity().pixels {
        return Err(CompareError::Invalid(
            "frame and against must be two different images".to_owned(),
        ));
    }
    Ok((rgb(frame, limits)?, rgb(against, limits)?))
}

/// `edge_similarity` of two image artifacts.
pub fn edge_similarity(
    frame: &Image,
    against: &Image,
    limits: &DecodeLimits,
) -> Result<Edges, CompareError> {
    let (a, b) = pair(frame, against, limits)?;
    edges(&a, &b)
}

/// The change map of two image artifacts, and the map as an image bound onto `frame` by a
/// crop of the whole of it (ADR 0001), so a place on the map maps back to `frame`.
pub fn change_map(
    frame: &Image,
    against: &Image,
    limits: &DecodeLimits,
) -> Result<(Changes, Image, Binding), CompareError> {
    let (a, b) = pair(frame, against, limits)?;
    let changes = changes(&a, &b)?;
    let fault = |error: &dyn std::fmt::Display| CompareError::Failed(error.to_string());
    let rgba: Vec<u8> = changes
        .map
        .pixels()
        .flat_map(|pixel| [pixel.0[0], pixel.0[1], pixel.0[2], 255])
        .collect();
    let (w, h) = changes.map.dimensions();
    let shown = CanonicalPixels::from_rgba(w, h, rgba).map_err(|e| fault(&e))?;
    let origin = Origin::new(
        Step::new(CHANGE_MAP, CHANGE_MAP_VERSION)
            .input("frame", frame.id().0)
            .input("against", against.id().0),
    )?;
    let file = crate::view::png(&shown).map_err(|e| fault(&e))?;
    let made = Image::new(
        file,
        Projection::Flat,
        Topology::Bounded,
        ImageRole::Picture,
        origin,
        limits,
    )?;
    if made.identity().pixels != shown.digest() {
        return Err(CompareError::Failed(
            "the map's file does not read back as its pixels".to_owned(),
        ));
    }
    let space = frame.space();
    let binding = Binding::new(
        made.identity().pixels,
        made.space(),
        frame.identity().pixels,
        space,
        Direction::SourceToDestination,
        Mapping::Crop {
            left: 0.0,
            top: 0.0,
            right: f64::from(space.width()),
            bottom: f64::from(space.height()),
        },
    )
    .map_err(|e| fault(&e))?;
    Ok((changes, made, binding))
}

/// One of the two tools as an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Which {
    ChangeMap,
    EdgeSimilarity,
}

/// `change_map` or `edge_similarity` as an action: it takes `frame` and `against`; the change
/// map returns its map at `map`; each states its measurements in its run's diagnostic
/// ([`CHANGE_MAP_REPORT`], [`EDGE_SIMILARITY_REPORT`]): a port returns artifacts, and a
/// measurement is none (`docs/carry-over.md`).
pub struct Compare {
    descriptor: ActionDescriptor,
    which: Which,
}

impl Compare {
    fn draft(name: &str, purpose: &str) -> Draft {
        ActionDescriptor::draft(name, ACTION_VERSION, Kind::Primitive, purpose)
            .input(InputPort::required(
                "frame",
                ArtifactType::Image,
                "The image whose grid the comparison is made in, usually the source.",
            ))
            .input(InputPort::required(
                "against",
                ArtifactType::Image,
                "The image compared with it, usually the candidate aligned onto it.",
            ))
            .results(ResultKind::Measurements)
            .concurrent(true)
    }

    /// `change_map`.
    pub fn change_map() -> Result<Compare, DescriptorError> {
        let draft = Compare::draft(
            CHANGE_MAP,
            "Shows where two images in one grid differ in structure: a grey map, bright where the locally normalised luminance disagrees.",
        )
        .output(OutputPort::counted(
            "map",
            ArtifactType::Image,
            "The map, in the grid of frame: bright is different, dark the same.",
            Count::between(1, 1),
        ));
        Ok(Compare {
            descriptor: draft.build()?,
            which: Which::ChangeMap,
        })
    }

    /// `edge_similarity`.
    pub fn edge_similarity() -> Result<Compare, DescriptorError> {
        let draft = Compare::draft(
            EDGE_SIMILARITY,
            "Scores how much two images in one grid share the same edges: 1 for the same structure, near 0 for unrelated ones.",
        );
        Ok(Compare {
            descriptor: draft.build()?,
            which: Which::EdgeSimilarity,
        })
    }
}

impl Action for Compare {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let limits = DecodeLimits::default();
        let name = self.descriptor.name().to_owned();
        let failed = |why: &dyn std::fmt::Display| Failure::error(&name, &why.to_string());
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
        let (frame, against) = (at("frame")?, at("against")?);
        let refused = |error: CompareError| match error {
            CompareError::Invalid(why) => Failure::invalid(&why),
            other => failed(&other),
        };
        let mut produced = Produced::nothing();
        let (measurements, report_name) = match self.which {
            Which::EdgeSimilarity => {
                let edges = edge_similarity(&frame, &against, &limits).map_err(refused)?;
                (edges.measurements(), EDGE_SIMILARITY_REPORT)
            }
            Which::ChangeMap => {
                let (changes, map, _) = change_map(&frame, &against, &limits).map_err(refused)?;
                produced = produced.with("map", map);
                (changes.measurements(), CHANGE_MAP_REPORT)
            }
        };
        produced = produced.with_measurements(report(&measurements));
        let said = report(&measurements).to_string();
        let kept = context.diagnostics().write(report_name, said.as_bytes());
        kept.map_err(|why| failed(&why))?;
        Ok(produced)
    }
}
