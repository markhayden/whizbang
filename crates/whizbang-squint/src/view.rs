//! `view`: shows a region of an image closer. A crop of an image by box, or a square
//! perspective view into a panorama by yaw, pitch and field of view; and the same of a
//! second image when asked. Each crop is a new image, bound onto the image it was cut from
//! (ADR 0001; ADR 0051 for the perspective view), so a location on the crop maps back.
//!
//! Ported from squint, `crates/squint-tools/src/builtin/view.rs` and `raster.rs`. The
//! tool's record is `docs/reference/tools/view.md`.

use whizbang_core::artifact::{self, Artifact, ArtifactError, ArtifactType, Image, ImageRole};
use whizbang_core::artifact::{Origin, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Count, DescriptorError, Failure, InputPort, Kind,
    OutputPort, PreparedCall, Produced, ResultKind, Value, ValueType,
};
use whizbang_core::identity::{
    Binding, CanonicalPixels, DecodeLimits, Direction, IdentityError, Mapping, Perspective,
    Projection, Topology,
};

mod locate;
mod perspective;
mod scale;

pub use locate::{Located, Traced, box_through_homography, box_to_root, points_into_homography};
pub use perspective::{MIN_VIEW, looked, side};
use scale::Region as Cut;

/// The action's name, as an artifact's origin records it.
pub const NAME: &str = "view";
/// The action's version. It changes when the same request would give other pixels.
pub const VERSION: &str = "1";
/// The longer side of a crop is at most this many pixels; a larger cut is scaled down.
pub const MAX_SIDE: u32 = 1024;
/// A cut smaller than this on either side is refused as too small to show anything.
pub const MIN_CROP: u32 = 8;

/// The least field of view a perspective view is asked with, in degrees.
pub const MIN_FOV: f64 = 10.0;
/// The greatest field of view a perspective view is asked with, in degrees.
pub const MAX_FOV: f64 = 120.0;

/// What to show.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    /// The image to look into. It may itself be a crop that `view` returned.
    pub image: &'a Image,
    /// A second image to show the same region of, for comparison. It has the image's
    /// projection.
    pub also: Option<&'a Image>,
    /// The region.
    pub region: Region,
}

/// The region a request shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Region {
    /// Left, top, right and bottom, as fractions of each image, so the same box names the
    /// same region of two images of different sizes. Of a flat image or of a panorama.
    Box([f64; 4]),
    /// A square perspective view into a complete panorama: yaw from -180 up to 180, pitch
    /// from -90 to 90, and a field of view from [`MIN_FOV`] to [`MAX_FOV`], in degrees.
    Perspective(Perspective),
}

/// One crop: the image, and how it sits on the image it was cut from.
#[derive(Debug, Clone, PartialEq)]
pub struct Crop {
    /// The crop, a picture. Its origin names this action, the box and the source image.
    pub image: Image,
    /// The crop placed onto its source: a crop mapping whose box is the whole pixels that
    /// were cut, in the source's pixels.
    pub binding: Binding,
}

/// Why `view` returned nothing.
#[derive(Debug)]
pub enum ViewError {
    /// The request cannot be met as it was made. The text says what to send instead, and
    /// is meant for whoever made the request: a judge model receives it as the tool's
    /// result.
    Invalid(String),
    /// An image could not be read, or a crop could not be made into an artifact.
    Artifact(ArtifactError),
    /// `view` did not make what it should have: the encoder failed, or a crop did not
    /// read back as the pixels it holds.
    Failed(String),
}

impl std::fmt::Display for ViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ViewError::Invalid(why) => write!(f, "invalid request: {why}"),
            ViewError::Artifact(error) => error.fmt(f),
            ViewError::Failed(why) => write!(f, "view failed: {why}"),
        }
    }
}

impl std::error::Error for ViewError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ViewError::Artifact(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ArtifactError> for ViewError {
    fn from(error: ArtifactError) -> ViewError {
        ViewError::Artifact(error)
    }
}

impl From<IdentityError> for ViewError {
    fn from(error: IdentityError) -> ViewError {
        ViewError::Artifact(ArtifactError::Identity(error))
    }
}

fn invalid<T>(why: String) -> Result<T, ViewError> {
    Err(ViewError::Invalid(why))
}

/// The refusal of angles for a flat image. It says what to send instead, because a model
/// told only that the angles do not apply sends the same call again (squint's
/// `FLAT_FRAME`).
const FLAT: &str = "is flat: send bbox [left, top, right, bottom] as fractions of it; yaw, \
                    pitch and fov are for panoramas";

/// Cuts the request's region out of its image, and out of its second image when it names
/// one, in that order. Either both crops are returned or none is.
///
/// A box snaps outward to whole pixels, so a crop never shows part of a pixel. A cut
/// whose longer side is over [`MAX_SIDE`] is scaled down to it; a smaller one is returned
/// pixel for pixel and never enlarged. A perspective view is square, its side worked out
/// from the panorama's width and the field of view ([`side`]).
pub fn view(request: &Request<'_>, limits: &DecodeLimits) -> Result<Vec<Crop>, ViewError> {
    let images = [Some(request.image), request.also];
    if let Some(also) = request.also
        && also.space().projection() != request.image.space().projection()
    {
        return invalid(format!(
            "also is {} and image is {}: the two have one projection",
            projection(also),
            projection(request.image)
        ));
    }
    match request.region {
        Region::Box(bbox) => {
            let fractions = bbox.iter().all(|v| (0.0..=1.0).contains(v));
            if !fractions || bbox[0] >= bbox[2] || bbox[1] >= bbox[3] {
                return invalid(format!(
                    "bbox {bbox:?} must be fractions with left < right and top < bottom"
                ));
            }
        }
        Region::Perspective(asked) => {
            for (image, name) in images.iter().zip(["image", "also"]) {
                let Some(image) = image else { continue };
                let space = image.space();
                if space.projection() == Projection::Flat {
                    return invalid(format!("{name} {FLAT}"));
                }
                if space.topology() != Topology::Wrap {
                    return invalid(format!(
                        "{name} is part of a panorama, and a perspective view looks into a \
                         whole one: send bbox [left, top, right, bottom] as fractions of it"
                    ));
                }
            }
            angles(asked)?;
        }
    }
    if let Some(also) = request.also
        && also.identity().pixels == request.image.identity().pixels
    {
        return invalid("also must be a different image from image".to_owned());
    }
    images
        .into_iter()
        .flatten()
        .map(|image| match request.region {
            Region::Box(bbox) => crop(image, &bbox, limits),
            Region::Perspective(asked) => perspective(image, asked, limits),
        })
        .collect()
}

/// What a refusal calls an image's projection.
fn projection(image: &Image) -> &'static str {
    match image.space().projection() {
        Projection::Flat => "flat",
        Projection::Equirectangular => "a panorama",
    }
}

/// Refuses angles out of their bounds, in squint's words.
fn angles(asked: Perspective) -> Result<(), ViewError> {
    let Perspective { yaw, pitch, fov } = asked;
    if !(-180.0..180.0).contains(&yaw) {
        return invalid(format!("yaw {yaw} outside [-180, 180)"));
    }
    if !(-90.0..=90.0).contains(&pitch) {
        return invalid(format!("pitch {pitch} outside [-90, 90]"));
    }
    if !(MIN_FOV..=MAX_FOV).contains(&fov) {
        return invalid(format!("fov {fov} outside [{MIN_FOV}, {MAX_FOV}]"));
    }
    Ok(())
}

/// One image's perspective view: a picture, bound onto the panorama by ADR 0051's mapping.
fn perspective(
    image: &Image,
    asked: Perspective,
    limits: &DecodeLimits,
) -> Result<Crop, ViewError> {
    let space = image.space();
    let pixels = image.pixels(limits)?;
    let shown = looked(&pixels, asked, side(space.width(), asked.fov))?;
    let origin = Origin::new(
        Step::new(NAME, VERSION)
            .argument("yaw", &asked.yaw.to_string())
            .argument("pitch", &asked.pitch.to_string())
            .argument("fov", &asked.fov.to_string())
            .input("image", image.id().0),
    )?;
    let mapping = Mapping::Perspective {
        yaw: asked.yaw,
        pitch: asked.pitch,
        fov: asked.fov,
    };
    made(
        image,
        &shown,
        Projection::Flat,
        Topology::Bounded,
        origin,
        mapping,
        limits,
    )
}

/// The crop's image, read back and held to its pixels, and its binding onto `image`.
fn made(
    image: &Image,
    shown: &CanonicalPixels,
    projection: Projection,
    topology: Topology,
    origin: Origin,
    mapping: Mapping,
    limits: &DecodeLimits,
) -> Result<Crop, ViewError> {
    let crop = Image::new(
        png(shown)?,
        projection,
        topology,
        ImageRole::Picture,
        origin,
        limits,
    )?;
    // The file is read back through the one decoder every image is read through, and
    // must hold the pixels that were put in it.
    if crop.identity().pixels != shown.digest() {
        return Err(ViewError::Failed(
            "the crop's file does not read back as its pixels".to_owned(),
        ));
    }
    let binding = Binding::new(
        crop.identity().pixels,
        crop.space(),
        image.identity().pixels,
        image.space(),
        Direction::SourceToDestination,
        mapping,
    )?;
    Ok(Crop {
        image: crop,
        binding,
    })
}

/// One image's crop by box. A cut of the whole image keeps its projection and topology,
/// so the whole of a panorama is still a panorama; any other cut is flat.
fn crop(image: &Image, bbox: &[f64; 4], limits: &DecodeLimits) -> Result<Crop, ViewError> {
    let space = image.space();
    let (left, right) = snap(bbox[0], bbox[2], space.width());
    let (top, bottom) = snap(bbox[1], bbox[3], space.height());
    let region = Cut {
        left,
        top,
        right,
        bottom,
    };
    if region.width() < MIN_CROP || region.height() < MIN_CROP {
        return invalid(format!(
            "bbox {bbox:?} covers only {}x{} pixels of the {}x{} image; at least \
             {MIN_CROP}x{MIN_CROP} are needed",
            region.width(),
            region.height(),
            space.width(),
            space.height()
        ));
    }
    let pixels = image.pixels(limits)?;
    let shown = shown(&pixels, [left, top, right, bottom])?;
    let origin = Origin::new(
        Step::new(NAME, VERSION)
            .argument("left", &bbox[0].to_string())
            .argument("top", &bbox[1].to_string())
            .argument("right", &bbox[2].to_string())
            .argument("bottom", &bbox[3].to_string())
            .input("image", image.id().0),
    )?;
    let whole = (left, top, right, bottom) == (0, 0, space.width(), space.height());
    let (projection, topology) = if whole {
        (space.projection(), space.topology())
    } else {
        (Projection::Flat, Topology::Bounded)
    };
    let mapping = Mapping::Crop {
        left: f64::from(left),
        top: f64::from(top),
        right: f64::from(right),
        bottom: f64::from(bottom),
    };
    made(image, &shown, projection, topology, origin, mapping, limits)
}

/// The whole pixels a box of fractions covers on a picture of `width` by `height`, snapped
/// outward: left, top, one past the right and one past the bottom. The box is fractions
/// from 0 to 1, left under right and top under bottom.
pub fn snapped(bbox: &[f64; 4], width: u32, height: u32) -> [u32; 4] {
    let (left, right) = snap(bbox[0], bbox[2], width);
    let (top, bottom) = snap(bbox[1], bbox[3], height);
    [left, top, right, bottom]
}

/// A cut of whole pixels (left, top, one past the right, one past the bottom) as `view`
/// shows it: pixel for pixel, or scaled down to [`MAX_SIDE`] on its longer side when it is
/// longer. The one method `view` has, for the action and for the replay of the baseline.
pub fn shown(pixels: &CanonicalPixels, cut: [u32; 4]) -> Result<CanonicalPixels, ViewError> {
    let region = Cut {
        left: cut[0],
        top: cut[1],
        right: cut[2],
        bottom: cut[3],
    };
    let (width, height) = scale::fit(region.width(), region.height(), MAX_SIDE);
    let rgba = if (width, height) == (region.width(), region.height()) {
        scale::cut(pixels, &region)
    } else {
        scale::downscale(pixels, &region, width, height).ok_or_else(|| {
            ViewError::Failed(format!(
                "a cut of {}x{} pixels is too large to average",
                region.width(),
                region.height()
            ))
        })?
    };
    Ok(CanonicalPixels::from_rgba(width, height, rgba)?)
}

/// The whole pixels that the fractions `low` to `high` of a side of `size` pixels cover,
/// snapped outward: the first pixel, and one past the last. The span holds at least one
/// pixel. The fractions are from 0 to 1 and `low` is under `high`.
///
/// A product of a fraction and a size is not negative and is at most the size, so the
/// cast to a whole number rounds it down exactly; nothing here depends on a machine's
/// mathematics library.
fn snap(low: f64, high: f64, size: u32) -> (u32, u32) {
    let first = ((low * f64::from(size)) as u32).min(size - 1);
    let end = high * f64::from(size);
    let down = end as u32;
    let up = if f64::from(down) < end {
        down + 1
    } else {
        down
    };
    (first, up.clamp(first + 1, size))
}

/// The pixels as a PNG, by the core's one encoder of an image artifact's file
/// (`whizbang_core::identity::encode`).
pub(crate) fn png(pixels: &CanonicalPixels) -> Result<Vec<u8>, ViewError> {
    whizbang_core::identity::encode::png(pixels)
        .map_err(|why| ViewError::Failed(format!("cannot encode the crop: {why}")))
}

/// `view` as an action (ADR 0009): it takes the image at `image`, and a second at
/// `also` when one is given, and either the box as four numbers or the three angles of a
/// perspective view, and returns the crops at `crops`, the first image's first. A request
/// the function refuses as the caller's mistake ends the run as invalid, with the
/// function's sentence.
///
/// The binding of a crop onto its source is not returned: a port returns artifacts,
/// and a binding is none (`docs/carry-over.md`). The crop's origin names the box and
/// the image it was cut from.
pub struct View(ActionDescriptor);

/// The version of the action's descriptor. [`VERSION`] is what a crop's origin records,
/// and is part of every crop's file.
pub const ACTION_VERSION: &str = "1.1.0";

/// The names of the four arguments that are the box, in the order of [`Region::Box`].
pub const SIDES: [&str; 4] = ["left", "top", "right", "bottom"];
/// The names of the three arguments of a perspective view: yaw, pitch and fov.
pub const ANGLES: [&str; 3] = ["yaw", "pitch", "fov"];

impl View {
    pub fn new() -> Result<View, DescriptorError> {
        let side = |name: &str, purpose: &str| {
            Argument::optional(name, purpose, ValueType::number(Some(0.0), Some(1.0)))
        };
        let angle = |name: &str, purpose: &str, low: f64, high: f64| {
            Argument::optional(name, purpose, ValueType::number(Some(low), Some(high)))
        };
        let draft = ActionDescriptor::draft(
            NAME,
            ACTION_VERSION,
            Kind::Primitive,
            "Shows a region of an image closer, by box or as a perspective view into a panorama, and the same of a second image when one is given.",
        )
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The image to look into.",
        ))
        .input(InputPort::optional(
            "also",
            ArtifactType::Image,
            "A second image to show the same region of.",
        ))
        .argument(side("left", "The left edge of the region, as a fraction of the width."))
        .argument(side("top", "The top edge of the region, as a fraction of the height."))
        .argument(side("right", "The right edge of the region, as a fraction of the width."))
        .argument(side("bottom", "The bottom edge of the region, as a fraction of the height."))
        .argument(angle(
            "yaw",
            "A perspective view's degrees from the panorama's middle column, growing to the right; with pitch and fov, not with a box.",
            -180.0,
            180.0,
        ))
        .argument(angle(
            "pitch",
            "A perspective view's degrees above the horizon.",
            -90.0,
            90.0,
        ))
        .argument(angle(
            "fov",
            "A perspective view's field of view across and down, in degrees.",
            MIN_FOV,
            MAX_FOV,
        ))
        .output(OutputPort::counted(
            "crops",
            ArtifactType::Image,
            "The region of each image, the first image's first.",
            Count::between(1, 2),
        ))
        // Each crop's place on its source, as the binding a caller maps through (plan m3,
        // task Q1, item 4; ADR 0035).
        .results(ResultKind::Measurements)
        .concurrent(true);
        Ok(View(draft.build()?))
    }
}

impl Action for View {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let limits = DecodeLimits::default();
        let failed = |why: &dyn std::fmt::Display| Failure::error("view", &why.to_string());
        let at = |port: &str| -> Result<Option<Image>, Failure> {
            let Some(input) = call.inputs().iter().find(|input| input.port() == port) else {
                return Ok(None);
            };
            match artifact::get(context.store(), ArtifactType::Image, input.id(), &limits) {
                Ok(Artifact::Image(image)) => Ok(Some(image)),
                Ok(_) => Err(failed(&"the input is not an image")),
                Err(error) => Err(failed(&error)),
            }
        };
        let Some(image) = at("image")? else {
            return Err(failed(&"the call gives no image"));
        };
        let also = at("also")?;
        let number = |name: &str| {
            let given = call
                .arguments()
                .iter()
                .find(|(argument, _)| argument == name);
            match given {
                Some((_, Value::Number(number))) => Ok(Some(*number)),
                Some(_) => Err(failed(&format!("the call's {name} is not a number"))),
                None => Ok(None),
            }
        };
        let sides = SIDES.map(&number);
        let angles = ANGLES.map(&number);
        let mut bbox = [0.0; 4];
        let mut given = (0, 0);
        for (slot, side) in bbox.iter_mut().zip(sides) {
            if let Some(side) = side? {
                *slot = side;
                given.0 += 1;
            }
        }
        let mut turned = [0.0; 3];
        for (slot, angle) in turned.iter_mut().zip(angles) {
            if let Some(angle) = angle? {
                *slot = angle;
                given.1 += 1;
            }
        }
        let region = match given {
            (4, 0) => Region::Box(bbox),
            (0, 3) => Region::Perspective(Perspective {
                yaw: turned[0],
                pitch: turned[1],
                fov: turned[2],
            }),
            (0, 0) => {
                return Err(Failure::invalid(
                    "give left, top, right and bottom for a crop, or yaw, pitch and fov for a \
                     perspective view",
                ));
            }
            (_, 0) => {
                return Err(Failure::invalid(
                    "left, top, right and bottom go together: give all four for a crop",
                ));
            }
            (0, _) => {
                return Err(Failure::invalid(
                    "yaw, pitch and fov go together: give all three for a perspective view",
                ));
            }
            _ => {
                return Err(Failure::invalid(
                    "give either a box or yaw, pitch and fov, not both",
                ));
            }
        };
        let request = Request {
            image: &image,
            also: also.as_ref(),
            region,
        };
        let crops = view(&request, &limits).map_err(|error| match error {
            ViewError::Invalid(why) => Failure::invalid(&why),
            other => failed(&other),
        })?;
        let mut produced = Produced::nothing();
        let mut placed = Vec::new();
        for crop in crops {
            let binding = serde_json::to_value(&crop.binding).map_err(|e| failed(&e))?;
            placed
                .push(serde_json::json!({"crop": crop.image.id().to_string(), "binding": binding}));
            produced = produced.with("crops", crop.image);
        }
        Ok(produced.with_measurements(serde_json::json!({ "crops": placed })))
    }
}
