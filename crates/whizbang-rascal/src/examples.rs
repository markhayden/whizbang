//! The inputs of the worked examples of rascal's tool records, `docs/reference/tools/<tool>.md`
//! (`SPEC.md` section 8): the masks, images, locations, band profiles and plans each example's
//! call takes, made by the constructors the records' own tests build them with
//! (`tests/it/tool_records/`, `tests/it/record.rs`), which take them from here. `whizbang store
//! record-inputs` writes them into a store as canonical artifacts, so a walkthrough of a tool
//! (plan E, task O1) runs its record's examples on the very inputs the record shows: a mask
//! with its protect plane, its band profile and its prescriptions, which a picture of it does
//! not carry (`store import --mask`).
//!
//! Every mask and location is on an image that is written beside it: the examples' scene, a cut
//! of it, a crop of it, or the seam's canvas.

use whizbang_core::artifact::{
    Artifact, BandProfile, Image, ImageRole, Line, Location, Mask, Origin, Plan, Series, Shape,
    Source, Step, UseCase, Windows,
};
use whizbang_core::identity::{
    Binding, CanonicalPixels, DecodeLimits, Point, Projection, Space, Topology, encode,
};

use crate::apply::performed;
use crate::constants::{self, FAN};
use crate::cut::{STRIP_DEFAULT, canvas_of, strip_width};
use crate::fan::{Settings, fan_profile};
use crate::feather::feather;
use crate::profile::band_mask;
use crate::ramp::tone_step;
use crate::window::{Windowed, windowed};

/// The scene's size, which most examples' masks share.
pub const W: usize = 96;
pub const H: usize = 64;
/// The colour of the fills: a brick red.
pub const BRICK: [u8; 3] = [200, 60, 50];
/// Where the crop of `carry`'s examples sits on the scene: left, top, right and bottom.
pub const CROP: [usize; 4] = [30, 16, 70, 48];
/// The seam's canvas: 192 by 64, its seam at column 96.
pub const CANVAS: (u32, u32) = (192, 64);

fn never() -> bool {
    false
}

/// What a fixed example makes: every one is valid, so a failure is a bug here, and these
/// three are the only places this module may panic.
#[allow(clippy::expect_used)]
fn fixed<T, E: std::fmt::Debug>(made: Result<T, E>) -> T {
    made.expect("an example's input is fixed and valid")
}

/// What an operation that is never cancelled makes.
#[allow(clippy::expect_used)]
fn done<T, E: std::fmt::Debug>(made: Result<Option<T>, E>) -> T {
    fixed(made).expect("nothing cancels an example")
}

/// What a fixed example finds: a constant of a shipped set, a band mask's profile.
#[allow(clippy::expect_used)]
fn found<T>(found: Option<T>) -> T {
    found.expect("an example's constant or profile is there")
}

pub fn made_by(action: &str) -> Origin {
    fixed(Origin::new(Step::new(action, "1")))
}

/// A plane `width` by `height`: 255 where `set` holds, 0 elsewhere.
pub fn plane(width: usize, height: usize, set: impl Fn(usize, usize) -> bool) -> Vec<u8> {
    (0..width * height)
        .map(|at| if set(at % width, at / width) { 255 } else { 0 })
        .collect()
}

/// A disc centred at (cx, cy) of radius r.
pub fn disc(cx: i64, cy: i64, r: i64) -> impl Fn(usize, usize) -> bool {
    move |x, y| (x as i64 - cx).pow(2) + (y as i64 - cy).pow(2) <= r * r
}

/// An ellipse centred at (cx, cy) with half-axes a and b.
pub fn oval(cx: i64, cy: i64, a: i64, b: i64) -> impl Fn(usize, usize) -> bool {
    move |x, y| {
        let (dx, dy) = (x as i64 - cx, y as i64 - cy);
        dx * dx * b * b + dy * dy * a * a <= a * a * b * b
    }
}

/// The examples' scene: 96 by 64, a soft gradient from blue to sand with a darker band
/// across its lower third, every pixel opaque.
pub fn scene() -> CanonicalPixels {
    let (width, height) = (W as u32, H as u32);
    let rgba = (0..width * height)
        .flat_map(|at| {
            let (x, y) = (at % width, at / width);
            let band = if (40..48).contains(&y) { 40 } else { 0 };
            [
                (60 + x + y) as u8 - band,
                (90 + x / 2 + y) as u8 - band,
                (170 - x / 2) as u8 - band / 2,
                255,
            ]
        })
        .collect();
    fixed(CanonicalPixels::from_rgba(width, height, rgba))
}

/// The scene with a scratch: a dark line about two pixels wide from (20, 20) to (76, 44).
pub fn scratched() -> CanonicalPixels {
    let scene = scene();
    let mut rgba = scene.rgba().to_vec();
    for at in 0..W * H {
        let (x, y) = ((at % W) as f64 + 0.5, (at / W) as f64 + 0.5);
        // The distance from the segment's line, which runs 56 across and 24 down.
        let along = ((x - 20.0) * 56.0 + (y - 20.0) * 24.0) / (56.0f64 * 56.0 + 24.0 * 24.0);
        let off = ((x - 20.0) * 24.0 - (y - 20.0) * 56.0).abs() / 60.926_184_846_911_27;
        if (0.0..=1.0).contains(&along) && off <= 1.0 {
            rgba[at * 4..at * 4 + 3].copy_from_slice(&[40, 30, 30]);
        }
    }
    fixed(CanonicalPixels::from_rgba(W as u32, H as u32, rgba))
}

/// An image of these pixels in this projection, made by `action`.
fn image_in(pixels: &CanonicalPixels, projection: Projection, topology: Topology) -> Image {
    fixed(Image::new(
        fixed(encode::png(pixels)),
        projection,
        topology,
        ImageRole::Picture,
        made_by("import"),
        &DecodeLimits::default(),
    ))
}

/// A flat image of these pixels.
pub fn image_of(pixels: &CanonicalPixels) -> Image {
    image_in(pixels, Projection::Flat, Topology::Bounded)
}

/// The scene as a complete panorama, which `cut` and `put` take; its left and right edges
/// differ, so the seam shows.
pub fn panorama() -> Image {
    image_in(&scene(), Projection::Equirectangular, Topology::Wrap)
}

/// The root binding of these pixels.
pub fn onto(pixels: &CanonicalPixels) -> Binding {
    Binding::root(
        pixels.digest(),
        fixed(Space::flat(pixels.width(), pixels.height())),
    )
}

/// A mask on these pixels with this core, for removal, and these pixels protected.
pub fn mask_on(pixels: &CanonicalPixels, core: Vec<u8>, protect: &[u8]) -> Mask {
    let mut mask = fixed(Mask::from_core(
        onto(pixels),
        UseCase::Remove,
        core,
        Source::default(),
        made_by("fixture"),
    ));
    fixed(mask.protect_union(protect));
    mask
}

/// A mask on the scene with this core and nothing protected.
pub fn core(set: impl Fn(usize, usize) -> bool) -> Mask {
    mask_on(&scene(), plane(W, H, set), &[0; W * H])
}

fn feathered_by(mask: Mask, sigma: f64) -> Mask {
    done(feather(mask, sigma, &never))
}

// --- grow, close, subtract, select, plug, feather ----------------------------------------------

/// `grow`'s first example: a blob, as a segmenter finds one, and a thin protected bar
/// beside it.
pub fn grow_blob() -> Mask {
    let core = plane(W, H, |x, y| {
        let (dx, dy) = (x as i64 - 40, y as i64 - 22);
        dx * dx * 100 + dy * dy * 256 <= 25_600
    });
    let bar = plane(W, H, |x, y| (59..61).contains(&x) && (8..56).contains(&y));
    mask_on(&scene(), core, &bar)
}

/// The line `grow`'s band is along: column 48, four pixels wide.
pub fn grow_line() -> Location {
    let shape = Shape::Line {
        line: Line::Column(48.0),
        width: 4.0,
    };
    fixed(Location::new(onto(&scene()), shape, made_by("line")))
}

/// `grow`'s second example: a band along column 48 with a feathered edge, a profile and a
/// tone ramp, and a protected patch beside it.
pub fn grow_band() -> Mask {
    let binding = onto(&scene());
    let line = grow_line();
    let mut mask = fixed(Mask::from_core(
        binding.clone(),
        UseCase::Seam,
        plane(W, H, |x, _| (46..50).contains(&x)),
        Source {
            locations: vec![line.id()],
            instances: Vec::new(),
        },
        made_by("profile"),
    ));
    // Six columns of feather on each side, fading outward.
    let blend = (0..W * H).map(|at| {
        let x = at % W;
        let out = if x < 46 {
            46 - x
        } else {
            (x + 1).saturating_sub(50)
        };
        if (1..=6).contains(&out) {
            (260 - 40 * out) as u8
        } else {
            0
        }
    });
    fixed(mask.set_blend(blend.collect()));
    fixed(mask.protect_union(&plane(W, H, |x, y| {
        (51..58).contains(&x) && (24..34).contains(&y)
    })));
    let series = |name: &str, unit: &str, channels: u32, action: &str| {
        let values = vec![2.0; channels as usize * H];
        let made = Series::new(
            binding.clone(),
            name,
            unit,
            channels,
            values,
            made_by(action),
        );
        fixed(made)
    };
    let windows = Windows {
        gap: 1,
        step_columns: 3,
        texture_columns: 8,
    };
    let band = fixed(BandProfile::new(
        binding.clone(),
        line.id(),
        48,
        2.0,
        vec![2.0; H],
        windows,
        "fan-v1",
        made_by("fan"),
    ));
    fixed(mask.set_profile(band, series("sigma_px", "px", 1, "feather")));
    fixed(mask.push_tone_ramp(series("tone_ramp.step", "levels", 3, "ramp"), 0.5));
    mask
}

/// `close`'s first example: two blocks four columns apart, a notch two columns wide bitten
/// from the top of the left one, and a protected patch in the gap.
pub fn close_blocks() -> Mask {
    let core = plane(W, H, |x, y| {
        let block = (16..48).contains(&y) && ((20..44).contains(&x) || (48..72).contains(&x));
        let notch = (28..30).contains(&x) && (16..20).contains(&y);
        block && !notch
    });
    let patch = plane(W, H, |x, y| (44..48).contains(&x) && (30..34).contains(&y));
    mask_on(&scene(), core, &patch)
}

/// The second: two discs of radius 10, four columns apart, feathered by a sigma of 2.
pub fn close_discs() -> Mask {
    feathered_by(
        core(|x, y| disc(34, 32, 10)(x, y) || disc(59, 32, 10)(x, y)),
        2.0,
    )
}

/// The mask both of `subtract`'s examples subtract from: an oval, feathered by a sigma of 2.
pub fn subtract_oval() -> Mask {
    feathered_by(core(oval(48, 32, 24, 16)), 2.0)
}

/// A bar four columns wide from top to bottom, with no blend.
pub fn subtract_bar() -> Mask {
    core(|x, _| (46..50).contains(&x))
}

/// A disc of radius 4, feathered by a sigma of 3: a faint blend nine pixels past its core.
pub fn subtract_faint() -> Mask {
    feathered_by(core(disc(30, 32, 4)), 3.0)
}

/// `select`'s three instance masks, as `segment` commits them: two ovals that overlap, and
/// a third apart.
pub fn select_instances() -> Vec<Mask> {
    vec![
        core(oval(26, 30, 16, 12)),
        core(oval(46, 34, 12, 10)),
        core(oval(76, 28, 12, 14)),
    ]
}

/// `plug`'s first example: a disc with a hole of 16 pixels and one of 196.
pub fn plug_holes() -> Mask {
    core(|x, y| {
        oval(30, 32, 22, 22)(x, y)
            && !((20..24).contains(&x) && (30..34).contains(&y))
            && !((30..44).contains(&x) && (22..36).contains(&y))
    })
}

/// The second: a square with a bay open along its side and a pocket open only across its
/// corner.
pub fn plug_openings() -> Mask {
    core(|x, y| {
        let square = (60..90).contains(&x) && (10..54).contains(&y);
        let bay = (60..66).contains(&x) && (20..26).contains(&y);
        let pocket = (x, y) == (60, 40) || ((61..64).contains(&x) && (41..44).contains(&y));
        square && !bay && !pocket
    })
}

/// `feather`'s first example: a blob with a protected bar across its edge.
pub fn feather_blob() -> Mask {
    let mut blob = core(oval(40, 32, 20, 14));
    fixed(blob.protect_union(&plane(W, H, |x, y| (56..60).contains(&x) && y < 40)));
    blob
}

/// The second: a line one pixel wide.
pub fn feather_wire() -> Mask {
    core(|x, y| y == 32 && (8..88).contains(&x))
}

// --- render, paste, rasterize, shrink, invert, inspect, carry ----------------------------------

/// The mask of both of `render`'s examples, on the scene: an oval feathered by a sigma of
/// 3, with a protected bar across its right edge.
pub fn render_mask() -> Mask {
    let core = plane(W, H, |x, y| {
        let (dx, dy) = (x as i64 - 44, y as i64 - 30);
        dx * dx * 196 + dy * dy * 400 <= 78_400
    });
    let protect = plane(W, H, |x, y| (60..64).contains(&x) && y < 44);
    feathered_by(mask_on(&scene(), core, &protect), 3.0)
}

/// `paste`'s and `fill`'s first mask: a disc of radius 12 at (48, 30), feathered by a sigma
/// of 3.
pub fn paste_disc() -> Mask {
    feathered_by(core(disc(48, 30, 12)), 3.0)
}

/// The second: a block from (20, 12) to (75, 51), with no blend, and rows 28 to 33 of it
/// protected.
pub fn paste_striped() -> Mask {
    let block = plane(W, H, |x, y| (20..76).contains(&x) && (12..52).contains(&y));
    let rows = plane(W, H, |x, y| (16..80).contains(&x) && (28..34).contains(&y));
    mask_on(&scene(), block, &rows)
}

/// A fill of the scene's size, every pixel `rgb`, or every pixel the scene's inverse.
pub fn paste_fill(rgb: Option<[u8; 3]>) -> CanonicalPixels {
    let scene = scene();
    let rgba = scene.rgba().chunks(4).flat_map(|p| match rgb {
        Some([r, g, b]) => [r, g, b, 255],
        None => [255 - p[0], 255 - p[1], 255 - p[2], 255],
    });
    fixed(CanonicalPixels::from_rgba(
        W as u32,
        H as u32,
        rgba.collect(),
    ))
}

/// A disc of radius 14 at (40, 30) on the scene, a protected patch across its right edge,
/// feathered by a sigma of 2: the first mask of `shrink`, `invert` and `inspect`.
pub fn feathered() -> Mask {
    let patch = plane(W, H, |x, y| (50..58).contains(&x) && (26..34).contains(&y));
    feathered_by(
        mask_on(&scene(), plane(W, H, disc(40, 30, 14)), &patch),
        2.0,
    )
}

/// A block against the image's top and left edges.
pub fn cornered() -> Mask {
    core(|x, y| x < 30 && y < 24)
}

/// A disc of radius 20 with a protected disc of radius 6 at its centre.
pub fn holed() -> Mask {
    mask_on(
        &scene(),
        plane(W, H, disc(48, 32, 20)),
        &plane(W, H, disc(48, 32, 6)),
    )
}

/// Points of whole numbers.
pub fn points(at: &[(i32, i32)]) -> Vec<Point> {
    let point = |(x, y): &(i32, i32)| Point {
        x: f64::from(*x),
        y: f64::from(*y),
    };
    at.iter().map(point).collect()
}

/// `rasterize`'s examples: a triangle, and a chevron, whose notch is outside.
pub fn rasterize_shapes() -> [(&'static str, Shape); 2] {
    [
        (
            "triangle",
            Shape::Polygon {
                points: points(&[(12, 56), (48, 6), (84, 56)]),
            },
        ),
        (
            "chevron",
            Shape::Polygon {
                points: points(&[(10, 10), (50, 32), (10, 54), (30, 32)]),
            },
        ),
    ]
}

/// The pixels of a picture inside a box, as an image of their own: what `view` cuts.
pub fn crop_of(pixels: &CanonicalPixels, [left, top, right, bottom]: [usize; 4]) -> Image {
    let width = pixels.width() as usize;
    let mut rgba = Vec::new();
    for y in top..bottom {
        rgba.extend_from_slice(&pixels.rgba()[(y * width + left) * 4..(y * width + right) * 4]);
    }
    let cut = CanonicalPixels::from_rgba((right - left) as u32, (bottom - top) as u32, rgba);
    image_of(&fixed(cut))
}

/// `carry`'s examples: masks on the crop of the scene. A disc in the middle of the crop;
/// and a block against the crop's left and top edges with a protected patch, feathered.
pub fn carry_masks() -> [(&'static str, Mask); 2] {
    let (w, h) = (CROP[2] - CROP[0], CROP[3] - CROP[1]);
    let crop = crop_of(&scene(), CROP);
    let on_crop = |core: Vec<u8>, protect: &[u8]| {
        let mut mask = fixed(Mask::from_core(
            crop.binding(),
            UseCase::Remove,
            core,
            Source::default(),
            made_by("fixture"),
        ));
        fixed(mask.protect_union(protect));
        mask
    };
    let disc = on_crop(plane(w, h, disc(20, 16, 9)), &vec![0; w * h]);
    let mut edged = on_crop(
        plane(w, h, |x, y| x < 14 && y < 10),
        &plane(w, h, |x, y| (4..8).contains(&x) && (3..6).contains(&y)),
    );
    fixed(edged.set_blend(plane(w, h, |x, y| (14..17).contains(&x) && y < 10)));
    [("disc", disc), ("edged", edged)]
}

// --- cut, put, window, unwindow -----------------------------------------------------------------

/// A mask for the seam on these pixels whose core is where `core` holds.
pub fn seam_mask_on(pixels: &CanonicalPixels, core: impl Fn(usize, usize) -> bool) -> Mask {
    let (w, h) = (pixels.width() as usize, pixels.height() as usize);
    fixed(Mask::from_core(
        onto(pixels),
        UseCase::Seam,
        plane(w, h, core),
        Source::default(),
        made_by("fixture"),
    ))
}

/// These pixels with a brick red wherever `paint` holds.
pub fn painted(pixels: &CanonicalPixels, paint: impl Fn(usize, usize) -> bool) -> CanonicalPixels {
    let w = pixels.width() as usize;
    let mut rgba = pixels.rgba().to_vec();
    for at in 0..rgba.len() / 4 {
        if paint(at % w, at / w) {
            rgba[at * 4..at * 4 + 3].copy_from_slice(&BRICK);
        }
    }
    fixed(CanonicalPixels::from_rgba(
        pixels.width(),
        pixels.height(),
        rgba,
    ))
}

/// The strip of `put`'s examples, the default, and the canvas `cut` makes of the scene with it.
pub fn cut_canvas() -> (u32, CanonicalPixels) {
    let s = strip_width(W as u32, STRIP_DEFAULT);
    (s, fixed(canvas_of(&scene(), s)))
}

/// The columns about the canvas's seam that `put`'s second example paints.
pub fn about_the_seam(x: usize, _: usize) -> bool {
    (4..8).contains(&x)
}

/// `window`'s examples: a block in the middle, and one near the top right corner, each the
/// mask and what `window` makes of it.
pub fn windows() -> Vec<(&'static str, Mask, Windowed)> {
    let blocks: [(&'static str, [usize; 4]); 2] =
        [("middle", [40, 26, 52, 36]), ("corner", [84, 2, 94, 10])];
    let flat = fixed(Space::flat(W as u32, H as u32));
    blocks
        .into_iter()
        .map(|(name, [l, t, r, b])| {
            let mask = seam_mask_on(&scene(), |x, y| (l..r).contains(&x) && (t..b).contains(&y));
            let step = Step::new("window", "1.0.0");
            let made = fixed(windowed(&scene(), flat, &mask, None, 0.5, step));
            (name, mask, made)
        })
        .collect()
}

/// An image of a window's crop, in its space.
fn crop_image(pixels: &CanonicalPixels, space: Space) -> Image {
    image_in(pixels, space.projection(), space.topology())
}

// --- fan, profile, ramp, apply, measure ---------------------------------------------------------

/// A canvas `width` by `height` whose seam is its middle column: a warm left half and a
/// cooler, darker right one, each brightening a little downward; from row `detail_from`
/// down, a fine texture whose Sobel gradient is nowhere zero.
pub fn canvas(width: u32, height: u32, detail_from: u32) -> CanonicalPixels {
    let rgba = (0..width * height)
        .flat_map(|at| {
            let (x, y) = (at % width, at / width);
            let base: [i32; 3] = if x < width / 2 {
                [150, 120, 90]
            } else {
                [120, 100, 80]
            };
            let lift = (y * 16 / height) as i32;
            let t = if y >= detail_from {
                (((x * 7 + y * 13) % 5) as i32 - 2) * 6
            } else {
                0
            };
            let c = |v: i32| (v + lift + t).clamp(0, 255) as u8;
            [c(base[0]), c(base[1]), c(base[2]), 255]
        })
        .collect();
    fixed(CanonicalPixels::from_rgba(width, height, rgba))
}

/// The seam's two canvases: smooth, and with a fine texture on its lower half.
pub fn canvases() -> [(&'static str, CanonicalPixels); 2] {
    let (w, h) = CANVAS;
    [("smooth", canvas(w, h, h)), ("detail", canvas(w, h, h / 2))]
}

/// The line of the seam on a canvas: the whole column, two pixels wide, as the task `seam`
/// draws it.
pub fn seam_line(pixels: &CanonicalPixels, column: f64) -> Location {
    let shape = Shape::Line {
        line: Line::Column(column),
        width: 2.0,
    };
    fixed(Location::new(onto(pixels), shape, made_by("line")))
}

/// The band of `pixels` along `column` under `fan-v3`, made into a band profile on them.
pub fn band_of(pixels: &CanonicalPixels, column: f64) -> BandProfile {
    let settings = fixed(Settings::of(FAN[0].0));
    let made = fixed(fan_profile(pixels, column, &settings));
    fixed(BandProfile::new(
        onto(pixels),
        seam_line(pixels, column).id(),
        made.column,
        made.thin,
        made.half,
        made.windows,
        FAN[0].0,
        made_by("fan"),
    ))
}

/// The band mask of `band`, at `profile-v1`'s ratio.
pub fn banded(band: &BandProfile) -> Mask {
    let ratio = found(constants::number(constants::PROFILE_V1.1, "feather_ratio"));
    done(band_mask(
        band,
        ratio,
        Step::new("profile", "1.0.0"),
        &never,
    ))
}

/// The mask with `ramp-v1`'s tone ramp measured on `pixels`.
pub fn ramped(pixels: &CanonicalPixels, mask: &Mask) -> Mask {
    let ramp = constants::RAMP_V1.1;
    let share = found(constants::number(ramp, "share"));
    let sigma = found(constants::number(ramp, "ramp_sigma"));
    let band = &found(mask.profile()).band;
    let values = tone_step(pixels, band, sigma * f64::from(pixels.height()));
    let step = Series::new(
        mask.binding().clone(),
        "tone_ramp.step",
        "levels",
        3,
        values,
        made_by("ramp"),
    );
    let mut out = mask.clone();
    fixed(out.push_tone_ramp(fixed(step), share));
    out
}

// --- tighten --------------------------------------------------------------------------------------

/// `tighten`'s examples: the stroke of `stroke`'s record along the scratch, and one as wide
/// across the scene's plain upper part, where there is no edge.
pub fn tighten_strokes() -> [(&'static str, Vec<Point>, f64); 2] {
    let path = |at: &[(f64, f64)]| at.iter().map(|(x, y)| Point { x: *x, y: *y }).collect();
    [
        ("scratch", path(&[(20.0, 20.0), (76.0, 44.0)]), 6.0),
        ("plain", path(&[(10.0, 8.0), (86.0, 12.0)]), 6.0),
    ]
}

// --- every record's inputs ------------------------------------------------------------------------

/// One input of a record's example: the tool, the example, the port of the example's call
/// that takes it (or what it is to an input that does: "image of mask"), and the artifact.
pub struct Input {
    pub tool: &'static str,
    pub example: &'static str,
    pub port: &'static str,
    pub artifact: Artifact,
}

/// The records whose examples' inputs this module makes, in `describe`'s order.
pub const TOOLS: [&str; 28] = [
    "close",
    "feather",
    "grow",
    "plug",
    "render",
    "select",
    "subtract",
    "paste",
    "cut",
    "put",
    "window",
    "unwindow",
    "apply",
    "line",
    "rect",
    "stroke",
    "rasterize",
    "shrink",
    "invert",
    "carry",
    "tighten",
    "inspect",
    "fan",
    "profile",
    "ramp",
    "measure",
    "mask_rect",
    "protect",
];

/// Every input of every example of the records in [`TOOLS`], in that order, each example's
/// in the order of its call's ports, then what they are on.
pub fn inputs() -> Vec<Input> {
    // The seam's chain is made once, for its five records.
    let seamed = seams();
    let mut out = Vec::new();
    for tool in TOOLS {
        let given = match tool {
            "apply" | "fan" | "profile" | "ramp" | "measure" => seamed
                .iter()
                .filter(|(of, ..)| *of == tool)
                .map(|(_, example, port, artifact)| (*example, *port, artifact.clone()))
                .collect(),
            _ => of(tool),
        };
        for (example, port, artifact) in given {
            out.push(Input {
                tool,
                example,
                port,
                artifact,
            });
        }
    }
    out
}

type Given = (&'static str, &'static str, Artifact);

/// A mask, then the image it is on.
fn masked(example: &'static str, mask: Mask, on: &Image) -> Vec<Given> {
    vec![
        (example, "mask", mask.into()),
        (example, "image of mask", on.clone().into()),
    ]
}

/// The inputs of one record's examples.
fn of(tool: &str) -> Vec<Given> {
    let scene = image_of(&scene());
    let both = |make: &dyn Fn(&'static str) -> Vec<Given>, names: [&'static str; 2]| {
        names.into_iter().flat_map(make).collect::<Vec<_>>()
    };
    let image = |example, image: &Image| vec![(example, "image", image.clone().into())];
    match tool {
        "close" => [("blocks", close_blocks()), ("discs", close_discs())]
            .into_iter()
            .flat_map(|(name, mask)| masked(name, mask, &scene))
            .collect(),
        "feather" => [("blob", feather_blob()), ("wire", feather_wire())]
            .into_iter()
            .flat_map(|(name, mask)| masked(name, mask, &scene))
            .collect(),
        "grow" => {
            let mut out = masked("blob", grow_blob(), &scene);
            out.extend(masked("band", grow_band(), &scene));
            out.push(("band", "line of mask", grow_line().into()));
            out
        }
        "plug" => [("holes", plug_holes()), ("openings", plug_openings())]
            .into_iter()
            .flat_map(|(name, mask)| masked(name, mask, &scene))
            .collect(),
        "render" => both(
            &|name| {
                vec![
                    (name, "mask", render_mask().into()),
                    (name, "image", scene.clone().into()),
                ]
            },
            ["plain", "alpha"],
        ),
        "select" => {
            let all = select_instances();
            let mut out: Vec<Given> = [&all[0], &all[2]]
                .into_iter()
                .map(|mask| ("kept", "mask", mask.clone().into()))
                .collect();
            out.push(("kept", "image of mask", scene.clone().into()));
            out.extend(all.into_iter().map(|mask| ("at", "mask", mask.into())));
            out.push(("at", "image of mask", scene.clone().into()));
            out
        }
        "subtract" => [("bar", subtract_bar()), ("faint", subtract_faint())]
            .into_iter()
            .flat_map(|(name, other)| {
                vec![
                    (name, "mask", subtract_oval().into()),
                    (name, "subtract", other.into()),
                    (name, "image of mask", scene.clone().into()),
                ]
            })
            .collect(),
        "paste" => [
            ("feathered", paste_disc(), Some(BRICK)),
            ("protected", paste_striped(), None),
        ]
        .into_iter()
        .flat_map(|(name, mask, rgb)| {
            vec![
                (name, "image", scene.clone().into()),
                (name, "fill", image_of(&paste_fill(rgb)).into()),
                (name, "mask", mask.into()),
            ]
        })
        .collect(),
        "cut" => both(&|name| image(name, &panorama()), ["default", "half"]),
        "put" => {
            let (s, cut) = cut_canvas();
            let plan = fixed(Plan::new(panorama().binding(), s, made_by("cut")));
            let mask = seam_mask_on(&cut, about_the_seam);
            [
                ("unchanged", cut.clone()),
                ("painted", painted(&cut, about_the_seam)),
            ]
            .into_iter()
            .flat_map(|(name, canvas)| {
                vec![
                    (name, "image", panorama().into()),
                    (name, "canvas", image_of(&canvas).into()),
                    (name, "plan", plan.clone().into()),
                    (name, "mask", mask.clone().into()),
                    (name, "image of mask", image_of(&cut).into()),
                ]
            })
            .collect()
        }
        "window" => windows()
            .into_iter()
            .flat_map(|(name, mask, _)| {
                vec![
                    (name, "image", scene.clone().into()),
                    (name, "mask", mask.into()),
                ]
            })
            .collect(),
        "unwindow" => {
            let (_, _, made) = windows().remove(0);
            let core = made.mask.core().to_vec();
            let width = made.pixels.width() as usize;
            let crop = crop_image(&made.pixels, made.space);
            let inside = painted(&made.pixels, |x, y| core[y * width + x] != 0);
            [
                ("unchanged", crop.clone()),
                ("painted", crop_image(&inside, made.space)),
            ]
            .into_iter()
            .flat_map(|(name, given)| {
                vec![
                    (name, "image", scene.clone().into()),
                    (name, "crop", given.into()),
                    (name, "mask", made.mask.clone().into()),
                    (name, "image of mask", crop.clone().into()),
                ]
            })
            .collect()
        }
        "line" | "rect" | "mask_rect" => {
            let names = match tool {
                "line" => ["column", "path"],
                _ => ["box", "edge"],
            };
            both(&|name| image(name, &scene), names)
        }
        "stroke" => {
            let mut out = image("path", &scene);
            out.extend(image("scratch", &image_of(&scratched())));
            out
        }
        "rasterize" => rasterize_shapes()
            .into_iter()
            .flat_map(|(name, shape)| {
                let location = fixed(Location::new(scene.binding(), shape, made_by("fixture")));
                vec![
                    (name, "location", location.into()),
                    (name, "image", scene.clone().into()),
                ]
            })
            .collect(),
        "shrink" => [("feathered", feathered()), ("cornered", cornered())]
            .into_iter()
            .flat_map(|(name, mask)| masked(name, mask, &scene))
            .collect(),
        "invert" | "inspect" => {
            let second = if tool == "invert" {
                ("cornered", cornered())
            } else {
                ("holed", holed())
            };
            [("feathered", feathered()), second]
                .into_iter()
                .flat_map(|(name, mask)| {
                    vec![
                        (name, "mask", mask.into()),
                        (name, "image", scene.clone().into()),
                    ]
                })
                .collect()
        }
        "carry" => {
            let crop = crop_of(&self::scene(), CROP);
            carry_masks()
                .into_iter()
                .flat_map(|(name, mask)| {
                    vec![
                        (name, "mask", mask.into()),
                        (name, "crop", crop.clone().into()),
                        (name, "image", scene.clone().into()),
                    ]
                })
                .collect()
        }
        "tighten" => {
            let pixels = scratched();
            let on = image_of(&pixels);
            tighten_strokes()
                .into_iter()
                .flat_map(|(name, path, radius)| {
                    let shape = Shape::Stroke { path, radius };
                    let stroke = fixed(Location::new(on.binding(), shape, made_by("stroke")));
                    vec![
                        (name, "stroke", stroke.into()),
                        (name, "image", on.clone().into()),
                    ]
                })
                .collect()
        }
        // `protect` is given a first pass's composite and mask, which a fill makes: what is
        // written is what the first pass takes, the scene and, for `remove`, the box.
        "protect" => {
            let shape = Shape::Box {
                left: 24.0,
                top: 16.0,
                right: 56.0,
                bottom: 44.0,
            };
            let location = fixed(Location::new(scene.binding(), shape, made_by("rect")));
            vec![
                ("block", "image of the first pass", scene.clone().into()),
                ("feathered", "image of the first pass", scene.clone().into()),
                ("feathered", "location of the first pass", location.into()),
            ]
        }
        other => panic!("{other} has no record whose inputs are made here"),
    }
}

/// The inputs of the seam's records, each with its record: each canvas, its line, its band,
/// its band mask, its ramped mask and the working image `apply` makes of it.
fn seams() -> Vec<(&'static str, &'static str, &'static str, Artifact)> {
    let column = f64::from(CANVAS.0 / 2);
    let mut out = Vec::new();
    for (name, pixels) in canvases() {
        let canvas: Artifact = image_of(&pixels).into();
        let line: Artifact = seam_line(&pixels, column).into();
        let band = band_of(&pixels, column);
        let banded = banded(&band);
        let ramped = ramped(&pixels, &banded);
        let working = fixed(performed(&pixels, &ramped)).pixels;
        let (banded, ramped): (Artifact, Artifact) = (banded.into(), ramped.into());
        let given = [
            ("fan", "image", canvas.clone()),
            ("fan", "line", line.clone()),
            ("profile", "band", band.into()),
            ("profile", "image of band", canvas.clone()),
            ("profile", "line of band", line),
            ("ramp", "image", canvas.clone()),
            ("ramp", "mask", banded),
            ("apply", "image", canvas.clone()),
            ("apply", "mask", ramped.clone()),
            ("measure", "after", image_of(&working).into()),
            ("measure", "before", canvas),
            ("measure", "mask", ramped),
        ];
        out.extend(
            given
                .into_iter()
                .map(|(tool, port, artifact)| (tool, name, port, artifact)),
        );
    }
    out
}
