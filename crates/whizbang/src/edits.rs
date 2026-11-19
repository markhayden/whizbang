//! The synthetic edit set of the edits' rung 2 (plan m3, task B1, item 1; `SPEC.md` section
//! 8, "The ladder": task success, preservation outside the mask, and visual quality), drawn
//! by this program from fixed seeds, and its scoring from a store's runs.
//!
//! **Drawing** ([`draw`], `whizbang eval edits draw`). From the pictures of a published set,
//! each held to the SHA-256 its job pins, [`COUNT`] are chosen by the seed. On each, three
//! defects are drawn, each with its exact mask as truth, and the clean picture is kept as
//! the reference: a red ball, shaded, with a box around it, for `remove` and, named by the
//! phrase [`PHRASE`], for `remove_prompt` (the same picture); a thin light scratch, with a
//! stroke along it, for `repair`; and a dark spot, with a box around it, for `repair_area`.
//! A box and a stroke are written as a location's canonical file, on the picture as `store
//! import` brings it in. Every number comes from the seed through SHA-256 and a SplitMix64
//! sequence, and every shape is drawn with additions, products, divisions and square roots
//! alone, so the same pictures and seed draw the same files on any machine. What is written
//! is the set's manifest (`edits.toml`: each item's ids, digests, and its box, stroke or
//! phrase) and the files; [`lab_files`] makes from the manifest the lab's corpus manifest,
//! and for each recipe a protocol and a matrix whose arms are the recipe and a reference arm
//! holding the clean picture (plan m3, task L1, item 2).
//!
//! **Scoring** ([`score`], `whizbang eval edits score`). Each recipe's cells of a lab round
//! are found in the store by their request ids under the lab's scope; for each completed
//! one, with the picture the recipe was given, its result and its mask:
//!
//! - **preservation**: the count of pixels whose effective weight in the result's mask is
//!   0 and which differ, in any channel, from the picture the recipe was given (0 is
//!   required);
//! - **task success**: the IoU of the result's mask, where its weight is at least a half,
//!   with the drawn defect's mask, and beside it the defect's coverage (the share of the
//!   drawn defect inside that mask) and the mask's size as a multiple of the defect's,
//!   since each recipe grows and feathers its mask on purpose and IoU alone understates a
//!   mask that covers the defect; and the fill's mean error, per channel, inside the drawn
//!   defect's mask against the clean picture, beside the error there of a plain blur fill
//!   of the same mask (each pixel the mask weighs filled with the mean of the pixels it
//!   does not weigh in the smallest square around it that holds any, put back through the
//!   mask's weights).
//!
//! The backends are those the records of the cells' runs, and of the steps' runs under
//! them, say each request went to (`<provider> <model> <version>`, or `none` for a
//! capability no request of the recipe used), and the spend is what every attempt's
//! requests were settled at, failed ones included, with the part no answer settled apart.
//!
//! What is written is aggregates by recipe alone, never an item.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};
use serde::{Deserialize, Serialize};
use whizbang_core::artifact::{
    self, Artifact, ArtifactId, ArtifactType, Location, Origin, Shape, Step,
};
use whizbang_core::contract::Capability;
use whizbang_core::identity::{DecodeLimits, Digest, Point, Projection, identify};
use whizbang_core::store::Reader;
use whizbang_core::store::fs::RealFs;
use whizbang_eval::lab::corpus::{Corpus, Picture};
use whizbang_eval::lab::digest_of;
use whizbang_eval::lab::matrix::{Checks, Matrix};
use whizbang_eval::lab::protocol::{self, Protocol};

use crate::harness::{self, Ended};
use crate::import;
use crate::laboratory;

/// The seed every draw of the set comes from.
pub const SEED: u64 = 20_261_119;

/// How many pictures the set takes.
pub const COUNT: usize = 24;

/// The phrase `remove_prompt` is given: what the ball is.
pub const PHRASE: &str = "a red ball";

/// The recipes scored, in the order the record lists them.
pub const RECIPES: [&str; 4] = ["remove", "remove_prompt", "repair", "repair_area"];

/// The set the edit set is drawn from.
pub const SET: &str = "source-fidelity/scenes-v1";

/// What one fill is forecast at, in millionths of a dollar: Bria Eraser through fal (plan
/// m3, "Spend").
pub const FILL_MICROS: u64 = 40_000;

/// The margin, in pixels, of a box around what it holds.
const PAD: f64 = 2.0;

/// The step the origin of a drawn location names.
const DRAW_STEP: &str = "edits-draw";

/// The most bytes of the set's manifest, or of a lab's file, that are read.
const TEXT_LIMIT: u64 = 4 * 1024 * 1024;

/// The colour of the ball, before its shading.
const BALL: [f64; 3] = [210.0, 38.0, 32.0];

/// The colour of a scratch.
const SCRATCH: [u8; 3] = [236, 232, 224];

/// The colour of a spot, and how much of it covers the picture.
const SPOT: [f64; 3] = [72.0, 48.0, 28.0];
const SPOT_COVER: f64 = 0.75;

/// A picture of a set, as its job pins it: an id the edit set names it by, its path in
/// the set, the SHA-256 its job pins (hex), and its file's bytes.
#[derive(Debug, Clone)]
pub struct Source {
    pub id: String,
    pub path: String,
    pub sha256: String,
    pub bytes: Vec<u8>,
}

/// The set's manifest, `edits.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditSet {
    pub version: u32,
    /// The set the pictures were taken from.
    pub set: String,
    pub seed: u64,
    pub phrase: String,
    #[serde(default)]
    pub source: Vec<SourceEntry>,
    #[serde(default)]
    pub item: Vec<Item>,
}

/// One picture taken: its id, its path in the set, its pin, and the clean picture kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceEntry {
    pub id: String,
    pub path: String,
    /// `sha256:<hex>` of the set's file, as its job pins it.
    pub sha256: String,
    /// The clean picture: the file's pixels as decoded, written as a PNG.
    pub clean: String,
    pub clean_sha256: String,
}

/// One item: a recipe on a picture with a defect drawn on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub id: String,
    pub recipe: String,
    /// The id of the picture it was drawn on.
    pub source: String,
    /// The picture with the defect, which the recipe is given.
    pub picture: String,
    pub picture_sha256: String,
    /// The drawn defect's exact mask: 255 where it was drawn, 0 elsewhere.
    pub truth: String,
    pub truth_sha256: String,
    pub truth_pixels: u64,
    /// The box or the stroke as a location's canonical file, for a recipe that takes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_sha256: Option<String>,
    /// `[left, top, right, bottom]`, in pixels.
    #[serde(rename = "box", default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<[f64; 4]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Stroke>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phrase: Option<String>,
}

/// A stroke along a scratch: its path, in pixels, and its radius.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stroke {
    pub path: Vec<[f64; 2]>,
    pub radius: f64,
}

impl EditSet {
    /// The manifest's text, read strictly.
    pub fn parse(text: &str) -> Result<EditSet, String> {
        let set: EditSet = toml::from_str(text)
            .map_err(|e| whizbang_eval::lab::unreadable("the edit set's manifest", text, &e))?;
        if set.version != 1 {
            return Err(format!(
                "the edit set's manifest: version {} is not 1",
                set.version
            ));
        }
        Ok(set)
    }

    /// The manifest as written.
    pub fn text(&self) -> String {
        let mut body = format!(
            "version = {}\nset = {}\nseed = {}\nphrase = {}\n",
            self.version,
            quote(&self.set),
            self.seed,
            quote(&self.phrase)
        );
        for source in &self.source {
            body.push_str(&format!(
                "\n[[source]]\nid = {}\npath = {}\nsha256 = {}\nclean = {}\nclean_sha256 = {}\n",
                quote(&source.id),
                quote(&source.path),
                quote(&source.sha256),
                quote(&source.clean),
                quote(&source.clean_sha256)
            ));
        }
        for item in &self.item {
            body.push_str(&format!(
                "\n[[item]]\nid = {}\nrecipe = {}\nsource = {}\npicture = {}\n\
                 picture_sha256 = {}\ntruth = {}\ntruth_sha256 = {}\ntruth_pixels = {}\n",
                quote(&item.id),
                quote(&item.recipe),
                quote(&item.source),
                quote(&item.picture),
                quote(&item.picture_sha256),
                quote(&item.truth),
                quote(&item.truth_sha256),
                item.truth_pixels
            ));
            if let (Some(file), Some(sha256)) = (&item.location, &item.location_sha256) {
                body.push_str(&format!(
                    "location = {}\nlocation_sha256 = {}\n",
                    quote(file),
                    quote(sha256)
                ));
            }
            if let Some(bounds) = &item.bounds {
                body.push_str(&format!("box = {}\n", floats(bounds)));
            }
            if let Some(phrase) = &item.phrase {
                body.push_str(&format!("phrase = {}\n", quote(phrase)));
            }
            if let Some(stroke) = &item.stroke {
                let points: Vec<String> = stroke.path.iter().map(|p| floats(p)).collect();
                body.push_str(&format!(
                    "stroke = {{ path = [{}], radius = {:?} }}\n",
                    points.join(", "),
                    stroke.radius
                ));
            }
        }
        format!(
            "# The synthetic edit set of plan m3, task B1, item 1, written by `whizbang eval edits\n\
             # draw` from the pictures of the set it names, each held to its job's pin. Never\n\
             # edited by hand: drawn again from the same pictures and seed, it is the same.\n{body}"
        )
    }
}

/// What a draw made: the manifest, and each file by its path under the set's folder.
#[derive(Debug)]
pub struct Drawn {
    pub set: EditSet,
    pub files: BTreeMap<String, Vec<u8>>,
}

/// The defects, each with the recipes it is drawn for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Defect {
    Ball,
    Scratch,
    Spot,
}

impl Defect {
    const ALL: [Defect; 3] = [Defect::Ball, Defect::Scratch, Defect::Spot];

    fn name(self) -> &'static str {
        match self {
            Defect::Ball => "ball",
            Defect::Scratch => "scratch",
            Defect::Spot => "spot",
        }
    }

    fn recipes(self) -> &'static [&'static str] {
        match self {
            Defect::Ball => &["remove", "remove_prompt"],
            Defect::Scratch => &["repair"],
            Defect::Spot => &["repair_area"],
        }
    }

    /// What a picture with this defect is judged as in the lab, and what it was drawn as.
    fn what(self) -> &'static str {
        match self {
            Defect::Ball => "a shaded red ball",
            Defect::Scratch => "a light scratch",
            Defect::Spot => "a dark spot",
        }
    }
}

/// The numbers a draw takes: a SplitMix64 sequence from the SHA-256 of the seed and the
/// draw's names.
struct Draws(u64);

impl Draws {
    fn new(seed: u64, names: &[&str]) -> Draws {
        let text = format!("{seed}:{}", names.join(":"));
        let hex = Digest::of_bytes(text.as_bytes()).hex();
        Draws(u64::from_str_radix(&hex[..16], 16).unwrap_or(seed))
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number from 0 up to 1.
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1_u64 << 53) as f64
    }

    fn between(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.unit()
    }

    /// A centre on an axis of `size` pixels, `margin` from either end; the middle when
    /// there is no room.
    fn centre(&mut self, size: u32, margin: f64) -> f64 {
        let size = f64::from(size);
        if size - margin > margin {
            self.between(margin, size - margin)
        } else {
            size / 2.0
        }
    }
}

/// A picture being drawn on: red, green, blue and alpha, row by row, and its truth.
struct Canvas {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    truth: Vec<bool>,
}

impl Canvas {
    fn new(width: u32, height: u32, rgba: &[u8]) -> Canvas {
        Canvas {
            width,
            height,
            rgba: rgba.to_vec(),
            truth: vec![false; width as usize * height as usize],
        }
    }

    fn side(&self) -> f64 {
        f64::from(self.width.min(self.height))
    }

    /// The pixels whose centres are within these bounds, clipped to the picture.
    fn span(&self, low: f64, high: f64, size: u32) -> std::ops::Range<u32> {
        let low = low.floor().max(0.0) as u32;
        let high = (high.ceil().max(0.0) as u32).min(size);
        low..high.max(low)
    }

    fn set(&mut self, x: u32, y: u32, colour: [u8; 3]) {
        let at = y as usize * self.width as usize + x as usize;
        self.rgba[at * 4..at * 4 + 3].copy_from_slice(&colour);
        self.truth[at] = true;
    }

    fn get(&self, x: u32, y: u32) -> [u8; 3] {
        let at = (y as usize * self.width as usize + x as usize) * 4;
        [self.rgba[at], self.rgba[at + 1], self.rgba[at + 2]]
    }

    /// The box around the truth, `PAD` pixels out, within the picture.
    fn bounds(&self) -> Option<[f64; 4]> {
        let width = self.width as usize;
        let mut found: Option<[usize; 4]> = None;
        for (at, _) in self.truth.iter().enumerate().filter(|(_, t)| **t) {
            let (x, y) = (at % width, at / width);
            found = Some(match found {
                None => [x, y, x + 1, y + 1],
                Some([x0, y0, x1, y1]) => [x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1)],
            });
        }
        let [x0, y0, x1, y1] = found?.map(|v| v as f64);
        Some([
            (x0 - PAD).max(0.0),
            (y0 - PAD).max(0.0),
            (x1 + PAD).min(f64::from(self.width)),
            (y1 + PAD).min(f64::from(self.height)),
        ])
    }

    /// A red ball, shaded from a light above and to the left.
    fn ball(&mut self, draws: &mut Draws) {
        let side = self.side();
        let radius = (side * draws.between(0.05, 0.09)).max(4.0);
        let margin = radius + side * 0.05 + PAD + 1.0;
        let (cx, cy) = (
            draws.centre(self.width, margin),
            draws.centre(self.height, margin),
        );
        for y in self.span(cy - radius, cy + radius, self.height) {
            for x in self.span(cx - radius, cx + radius, self.width) {
                let dx = (f64::from(x) + 0.5 - cx) / radius;
                let dy = (f64::from(y) + 0.5 - cy) / radius;
                if dx * dx + dy * dy > 1.0 {
                    continue;
                }
                let (hx, hy) = (dx + 0.35, dy + 0.35);
                let lit = hx * hx + hy * hy;
                let light = (1.0 - 0.45 * lit).clamp(0.25, 1.0);
                let shine = (1.0 - lit * 6.0).max(0.0) * 110.0;
                let colour = BALL.map(|c| (c * light + shine).round().clamp(0.0, 255.0) as u8);
                self.set(x, y, colour);
            }
        }
    }

    /// A dark spot: an ellipse that covers three quarters of what is under it.
    fn spot(&mut self, draws: &mut Draws) {
        let side = self.side();
        let rx = (side * draws.between(0.03, 0.06)).max(3.0);
        let ry = rx * draws.between(0.6, 1.0);
        let margin = rx + side * 0.05 + PAD + 1.0;
        let (cx, cy) = (
            draws.centre(self.width, margin),
            draws.centre(self.height, margin),
        );
        for y in self.span(cy - ry, cy + ry, self.height) {
            for x in self.span(cx - rx, cx + rx, self.width) {
                let dx = (f64::from(x) + 0.5 - cx) / rx;
                let dy = (f64::from(y) + 0.5 - cy) / ry;
                if dx * dx + dy * dy > 1.0 {
                    continue;
                }
                let under = self.get(x, y);
                let mut colour = [0_u8; 3];
                for c in 0..3 {
                    let mixed = f64::from(under[c]) * (1.0 - SPOT_COVER) + SPOT[c] * SPOT_COVER;
                    colour[c] = mixed.round().clamp(0.0, 255.0) as u8;
                }
                self.set(x, y, colour);
            }
        }
    }

    /// A thin light scratch along a bent line of three points; the stroke a person would
    /// draw along it, each point moved by up to a third of its radius.
    fn scratch(&mut self, draws: &mut Draws) -> Stroke {
        let side = self.side();
        let length = side * draws.between(0.3, 0.5);
        let half_width = (side * 0.0015).max(1.0);
        let radius = (side * 0.012).max(4.0);
        let (mut ux, mut uy) = (1.0, 0.0);
        for _ in 0..64 {
            let (x, y) = (draws.between(-1.0, 1.0), draws.between(-1.0, 1.0));
            let norm = (x * x + y * y).sqrt();
            if norm > 0.3 && norm <= 1.0 {
                (ux, uy) = (x / norm, y / norm);
                break;
            }
        }
        let bend = length * draws.between(-0.15, 0.15);
        let margin = radius + side * 0.04 + 1.0;
        let reach_x = ux.abs() * length / 2.0 + bend.abs();
        let reach_y = uy.abs() * length / 2.0 + bend.abs();
        let cx = draws.centre(self.width, margin + reach_x);
        let cy = draws.centre(self.height, margin + reach_y);
        let tenth = |v: f64| (v * 10.0).round() / 10.0;
        let within = |v: f64, size: u32| v.clamp(0.0, f64::from(size));
        let line = [
            [cx - ux * length / 2.0, cy - uy * length / 2.0],
            [cx - uy * bend, cy + ux * bend],
            [cx + ux * length / 2.0, cy + uy * length / 2.0],
        ]
        .map(|[x, y]| [tenth(within(x, self.width)), tenth(within(y, self.height))]);
        let (x0, x1) = (line.iter().map(|p| p[0]))
            .fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(v), b.max(v)));
        let (y0, y1) = (line.iter().map(|p| p[1]))
            .fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(v), b.max(v)));
        for y in self.span(y0 - half_width - 1.0, y1 + half_width + 1.0, self.height) {
            for x in self.span(x0 - half_width - 1.0, x1 + half_width + 1.0, self.width) {
                if distance(f64::from(x) + 0.5, f64::from(y) + 0.5, &line) <= half_width {
                    self.set(x, y, SCRATCH);
                }
            }
        }
        let jitter = radius / 3.0;
        let path = line
            .iter()
            .map(|[x, y]| {
                let x = within(x + draws.between(-jitter, jitter), self.width);
                let y = within(y + draws.between(-jitter, jitter), self.height);
                [tenth(x), tenth(y)]
            })
            .collect();
        Stroke { path, radius }
    }
}

/// The distance from a point to a path of segments.
fn distance(x: f64, y: f64, path: &[[f64; 2]]) -> f64 {
    let mut best = f64::MAX;
    for pair in path.windows(2) {
        let ([ax, ay], [bx, by]) = (pair[0], pair[1]);
        let (dx, dy) = (bx - ax, by - ay);
        let length = dx * dx + dy * dy;
        let t = if length > 0.0 {
            (((x - ax) * dx + (y - ay) * dy) / length).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (px, py) = (ax + t * dx - x, ay + t * dy - y);
        best = best.min((px * px + py * py).sqrt());
    }
    best
}

/// Pixels as a PNG of red, green and blue, as the set's pictures are.
fn png_rgb(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let rgb: Vec<u8> = rgba.chunks(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    let mut png = Vec::new();
    PngEncoder::new_with_quality(&mut png, CompressionType::Default, FilterType::Adaptive)
        .write_image(&rgb, width, height, ExtendedColorType::Rgb8)
        .map_err(|e| format!("cannot write a PNG: {e}"))?;
    Ok(png)
}

/// A truth as a grey PNG: 255 where the defect was drawn.
fn png_truth(width: u32, height: u32, truth: &[bool]) -> Result<Vec<u8>, String> {
    let grey: Vec<u8> = truth.iter().map(|t| if *t { 255 } else { 0 }).collect();
    let mut png = Vec::new();
    PngEncoder::new_with_quality(&mut png, CompressionType::Default, FilterType::Adaptive)
        .write_image(&grey, width, height, ExtendedColorType::L8)
        .map_err(|e| format!("cannot write a PNG: {e}"))?;
    Ok(png)
}

/// A location's canonical file: the shape on the picture as `store import` brings it in.
fn location_file(picture: &[u8], shape: Shape) -> Result<Vec<u8>, String> {
    let image = import::image_of_bytes(picture, Projection::Flat)?;
    let origin = Origin::new(Step::new(DRAW_STEP, "1")).map_err(|e| e.to_string())?;
    let location = Location::new(image.binding(), shape, origin).map_err(|e| e.to_string())?;
    Ok(Artifact::from(location).to_bytes())
}

/// Draws the edit set: `count` of `sources` chosen by `seed`, each with its three defects.
/// A picture that is not the one its job pins, or one that is not opaque, is refused, and
/// nothing is drawn.
pub fn draw(set: &str, sources: &[Source], count: usize, seed: u64) -> Result<Drawn, String> {
    let limits = DecodeLimits::default();
    let mut ids = BTreeSet::new();
    for source in sources {
        let pin = source.sha256.trim_start_matches("sha256:");
        if !Digest::of_bytes(&source.bytes)
            .hex()
            .eq_ignore_ascii_case(pin)
        {
            return Err(format!(
                "picture {}: its file is not the one its job pins (another SHA-256); nothing \
                 was drawn",
                source.id
            ));
        }
        if !whizbang_eval::lab::is_name(&source.id) || !ids.insert(&source.id) {
            return Err(format!("picture {}: no plain name of its own", source.id));
        }
    }
    if count > sources.len() {
        return Err(format!(
            "{count} pictures are wanted and the set has {}",
            sources.len()
        ));
    }
    // The choice: the pictures by id, shuffled by the seed, the first `count` taken.
    let mut order: Vec<&Source> = sources.iter().collect();
    order.sort_by(|a, b| a.id.cmp(&b.id));
    let mut choosing = Draws::new(seed, &["choose"]);
    for i in (1..order.len()).rev() {
        let j = (choosing.next() % (i as u64 + 1)) as usize;
        order.swap(i, j);
    }
    order.truncate(count);
    order.sort_by(|a, b| a.id.cmp(&b.id));
    let mut files = BTreeMap::new();
    let mut entries = Vec::new();
    let mut items = Vec::new();
    for source in order {
        let identified =
            identify(&source.bytes, &limits).map_err(|e| format!("picture {}: {e}", source.id))?;
        let pixels = identified.pixels;
        if pixels.rgba().chunks(4).any(|p| p[3] != 255) {
            return Err(format!("picture {}: it is not opaque", source.id));
        }
        let (width, height) = (pixels.width(), pixels.height());
        let clean = format!("clean/{}.png", source.id);
        let clean_png = png_rgb(width, height, pixels.rgba())?;
        entries.push(SourceEntry {
            id: source.id.clone(),
            path: source.path.clone(),
            sha256: format!("sha256:{}", source.sha256.trim_start_matches("sha256:")),
            clean: clean.clone(),
            clean_sha256: digest_of(&clean_png),
        });
        files.insert(clean, clean_png);
        for defect in Defect::ALL {
            let mut draws = Draws::new(seed, &[&source.id, defect.name()]);
            let mut canvas = Canvas::new(width, height, pixels.rgba());
            let stroke = match defect {
                Defect::Ball => {
                    canvas.ball(&mut draws);
                    None
                }
                Defect::Spot => {
                    canvas.spot(&mut draws);
                    None
                }
                Defect::Scratch => Some(canvas.scratch(&mut draws)),
            };
            let bounds = canvas.bounds().ok_or_else(|| {
                format!("picture {}: its {} drew nothing", source.id, defect.name())
            })?;
            let name = format!("{}/{}", defect.name(), source.id);
            let picture = format!("{name}.png");
            let picture_png = png_rgb(width, height, &canvas.rgba)?;
            let truth = format!("{name}-truth.png");
            let truth_png = png_truth(width, height, &canvas.truth)?;
            let truth_pixels = canvas.truth.iter().filter(|t| **t).count() as u64;
            let (location, shape) = match &stroke {
                Some(stroke) => (
                    format!("{name}-stroke.loc"),
                    Shape::Stroke {
                        path: (stroke.path.iter())
                            .map(|[x, y]| Point::new(*x, *y))
                            .collect(),
                        radius: stroke.radius,
                    },
                ),
                None => {
                    let [left, top, right, bottom] = bounds;
                    let shape = Shape::Box {
                        left,
                        top,
                        right,
                        bottom,
                    };
                    (format!("{name}-box.loc"), shape)
                }
            };
            let location_bytes = location_file(&picture_png, shape)
                .map_err(|why| format!("picture {}: {why}", source.id))?;
            for recipe in defect.recipes() {
                let prompt = *recipe == "remove_prompt";
                items.push(Item {
                    id: format!("{recipe}-{}", source.id),
                    recipe: (*recipe).to_owned(),
                    source: source.id.clone(),
                    picture: picture.clone(),
                    picture_sha256: digest_of(&picture_png),
                    truth: truth.clone(),
                    truth_sha256: digest_of(&truth_png),
                    truth_pixels,
                    location: (!prompt).then(|| location.clone()),
                    location_sha256: (!prompt).then(|| digest_of(&location_bytes)),
                    bounds: (stroke.is_none() && !prompt).then_some(bounds),
                    stroke: stroke.clone(),
                    phrase: prompt.then(|| PHRASE.to_owned()),
                });
            }
            files.insert(picture, picture_png);
            files.insert(truth, truth_png);
            files.insert(location, location_bytes);
        }
    }
    items.sort_by(|a, b| {
        let at = |r: &str| RECIPES.iter().position(|x| *x == r);
        (at(&a.recipe), &a.source).cmp(&(at(&b.recipe), &b.source))
    });
    Ok(Drawn {
        set: EditSet {
            version: 1,
            set: set.to_owned(),
            seed,
            phrase: PHRASE.to_owned(),
            source: entries,
            item: items,
        },
        files,
    })
}

/// The pictures of a set, each read from a folder laid out as the set's repository is
/// (`<folder>/<set id>/<path>`) through no link, once each: the edit set's sources, named
/// by their items (`<item>`; a family's source picture `<family>-source`).
pub fn sources_of(set: &whizbang_eval::sets::Set, folder: &Path) -> Result<Vec<Source>, String> {
    let mut seen = BTreeSet::new();
    let mut sources = Vec::new();
    for picture in set.pictures()? {
        if !seen.insert(picture.path.clone()) {
            continue;
        }
        let parts: Vec<&str> = picture.path.split('/').collect();
        let id = match parts.as_slice() {
            ["families", family, file] if file.starts_with("source.") => format!("{family}-source"),
            ["items", item, _] => (*item).to_owned(),
            _ => picture.path.replace(['/', '.'], "-"),
        };
        let relative = format!("{}/{}", set.id(), picture.path);
        let bytes = whizbang_eval::sets::read_under(folder, &relative, import::MAX_FILE_BYTES)
            .map_err(|why| {
                let base = folder.display().to_string();
                why.replace(&base, ".")
            })?;
        sources.push(Source {
            id,
            path: picture.path,
            sha256: picture.sha256,
            bytes,
        });
    }
    Ok(sources)
}

/// Whether a folder, or the nearest of its parents that is there, is inside a git work
/// tree: where a set's pictures could be committed.
pub fn inside_a_work_tree(folder: &Path) -> bool {
    let absolute = std::path::absolute(folder).unwrap_or_else(|_| folder.to_path_buf());
    let there = absolute.ancestors().find(|a| a.exists());
    let resolved = there.and_then(|a| a.canonicalize().ok());
    resolved.is_some_and(|r| r.ancestors().any(|a| a.join(".git").exists()))
}

/// Writes a draw's files and its manifest under `out`, making the folders it needs.
pub fn write(drawn: &Drawn, out: &Path) -> Result<(), String> {
    let failed = |why: std::io::Error| format!("the edit set could not be written: {why}");
    for (name, bytes) in &drawn.files {
        let file = out.join(name);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(failed)?;
        }
        std::fs::write(&file, bytes).map_err(failed)?;
    }
    std::fs::write(out.join("edits.toml"), drawn.set.text()).map_err(failed)
}

/// What a draw prints: counts, and the forecast of the round's fills.
pub fn drawn_text(drawn: &Drawn) -> String {
    let fills = drawn.set.item.len() as u64;
    let micros = fills * FILL_MICROS;
    format!(
        "pictures: {}\nitems: {}\nfiles: {}\nfills a round makes: {fills}, forecast at {} US \
         dollars a fill: {} US dollars\n",
        drawn.set.source.len(),
        drawn.set.item.len(),
        drawn.files.len() + 1,
        dollars(FILL_MICROS),
        dollars(micros)
    )
}

/// Text as a TOML basic string.
fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Numbers as a TOML array, each as it reads back.
fn floats(values: &[f64]) -> String {
    let shown: Vec<String> = values.iter().map(|v| format!("{v:?}")).collect();
    format!("[{}]", shown.join(", "))
}

fn dollars(micros: u64) -> String {
    format!(
        "{}.{:02}",
        micros / 1_000_000,
        (micros % 1_000_000) / 10_000
    )
}

/// A corpus id of a file of the set: its path with `/` as `-`, without its extension.
fn corpus_id(file: &str) -> String {
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    stem.replace('/', "-")
}

/// The lab's name for a recipe.
pub fn lab_name(recipe: &str) -> String {
    format!("m3-edits-{recipe}")
}

/// The lab's files the manifest makes: `edits.toml`; `corpus.toml`, which pins every file
/// of the set; and for each recipe `<recipe>-protocol.toml` and `<recipe>-matrix.toml`,
/// whose arms are the recipe and the reference arm `clean`.
pub fn lab_files(set: &EditSet) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    files.insert("edits.toml".to_owned(), set.text());
    files.insert("corpus.toml".to_owned(), corpus_text(set));
    for recipe in RECIPES {
        let items: Vec<&Item> = set.item.iter().filter(|i| i.recipe == recipe).collect();
        files.insert(
            format!("{recipe}-protocol.toml"),
            protocol_text(set, recipe, &items),
        );
        files.insert(
            format!("{recipe}-matrix.toml"),
            matrix_text(set, recipe, &items),
        );
    }
    files
}

fn corpus_text(set: &EditSet) -> String {
    let mut pictures: Vec<Picture> = Vec::new();
    let mut seen = BTreeSet::new();
    let from = |source: &str| {
        let entry = set.source.iter().find(|s| s.id == source);
        entry.map_or_else(String::new, |e| {
            format!("CC-BY-4.0: {} of the set {}", e.path, set.set)
        })
    };
    for source in &set.source {
        pictures.push(Picture {
            id: corpus_id(&source.clean),
            file: source.clean.clone(),
            sha256: source.clean_sha256.clone(),
            group: None,
            licence: format!("{}, its pixels as decoded", from(&source.id)),
            projection: None,
        });
        for defect in Defect::ALL {
            let of = (set.item.iter())
                .filter(|i| i.source == source.id && defect.recipes().contains(&i.recipe.as_str()));
            for item in of {
                let drawn = format!(
                    "{}, with {} drawn by `whizbang eval edits draw` from the seed {}",
                    from(&source.id),
                    defect.what(),
                    set.seed
                );
                if seen.insert(item.picture.clone()) {
                    pictures.push(Picture {
                        id: corpus_id(&item.picture),
                        file: item.picture.clone(),
                        sha256: item.picture_sha256.clone(),
                        group: Some("edits".to_owned()),
                        licence: drawn.clone(),
                        projection: None,
                    });
                }
                if let (Some(file), Some(sha256)) = (&item.location, &item.location_sha256)
                    && seen.insert(file.clone())
                {
                    pictures.push(Picture {
                        id: corpus_id(file),
                        file: file.clone(),
                        sha256: sha256.clone(),
                        group: None,
                        licence: format!("The place of the defect on: {drawn}"),
                        projection: None,
                    });
                }
            }
        }
    }
    let mut body = "version = 1\n".to_owned();
    for picture in &pictures {
        body.push_str(&format!(
            "\n[[image]]\nid = {}\nfile = {}\nsha256 = {}\n",
            quote(&picture.id),
            quote(&picture.file),
            quote(&picture.sha256)
        ));
        if let Some(group) = &picture.group {
            body.push_str(&format!("group = {}\n", quote(group)));
        }
        body.push_str(&format!("licence = {}\n", quote(&picture.licence)));
    }
    format!(
        "# The corpus of the labs `m3-edits-<recipe>` (plan m3, task B1, item 1): every file of\n\
         # the synthetic edit set `edits.toml` beside it, by the path under the folder `whizbang\n\
         # eval edits draw` wrote it to. Written by that program; never edited by hand.\n{body}"
    )
}

/// The recipe input that takes an item's location: `stroke` for `repair`.
fn location_input(recipe: &str) -> &'static str {
    if recipe == "repair" {
        "stroke"
    } else {
        "location"
    }
}

fn quoted_list(ids: &[String]) -> String {
    let lines: Vec<String> = ids.iter().map(|id| format!("  \"{id}\",\n")).collect();
    format!("[\n{}]", lines.concat())
}

fn protocol_text(set: &EditSet, recipe: &str, items: &[&Item]) -> String {
    let images: Vec<String> = items.iter().map(|i| corpus_id(&i.picture)).collect();
    format!(
        "# The lab `{lab}` (plan m3, task B1, item 1): the visual quality of\n\
         # the shipped recipe `{recipe}` on the synthetic edit set (`edits.toml` beside\n\
         # this file), each result against the clean picture, the reference arm `clean`. Wins\n\
         # are the recipe's. The outcome is rascal's rule: `not_worse` when fewer than a third\n\
         # of the decided pairs prefer the clean picture, `inconclusive` under ten decided\n\
         # pairs. Frozen before the round: its digest is in every cell, and a change is a new\n\
         # lab. Written by `whizbang eval edits draw`.\n\
         version = 1\nname = \"{lab}\"\nseed = {seed}\narms = [\"{recipe}\", \"clean\"]\n\
         min_decided = 10\n\n[rules]\npreferred = [2, 3]\nnot_worse = [1, 3]\n\n\
         [[group]]\nname = \"edits\"\nimages = {list}\n",
        lab = lab_name(recipe),
        seed = set.seed,
        list = quoted_list(&images),
    )
}

fn matrix_text(set: &EditSet, recipe: &str, items: &[&Item]) -> String {
    let mut text = format!(
        "# The matrix of the lab `{lab}`; the protocol beside it says what is\n\
         # compared. Each picture of the edit set, with its defect, through the shipped recipe\n\
         # `{recipe}`, filled by the config's default fill backend (a mask found by its default\n\
         # segmenter), and the clean picture handed back by the reference arm. The commands\n\
         # that run it are under task B1 of `docs/plans/m3-milestone-3.md`. Written by\n\
         # `whizbang eval edits draw`.\n\
         version = 1\nname = \"{lab}\"\ncorpus = \"corpus.toml\"\nprotocol = \"{recipe}-protocol.toml\"\n\
         seed = {seed}\n",
        lab = lab_name(recipe),
        seed = set.seed,
    );
    for item in items {
        let clean = (set.source.iter())
            .find(|s| s.id == item.source)
            .map(|s| corpus_id(&s.clean))
            .unwrap_or_default();
        let mut inputs = vec![format!("clean = \"{clean}\"")];
        if let Some(location) = &item.location {
            inputs.push(format!(
                "{} = \"{}\"",
                location_input(recipe),
                corpus_id(location)
            ));
        }
        text.push_str(&format!(
            "\n[[image]]\nid = \"{}\"\ninputs = {{ {} }}\n",
            corpus_id(&item.picture),
            inputs.join(", ")
        ));
        if let Some(phrase) = &item.phrase {
            text.push_str(&format!("texts = {{ prompt = \"{phrase}\" }}\n"));
        }
    }
    text.push_str(&format!(
        "\n[[arm]]\nname = \"{recipe}\"\nrecipe = \"{recipe}\"\n\n\
         [[arm]]\nname = \"clean\"\nreference = \"clean\"\n"
    ));
    text
}

// --- scoring ----------------------------------------------------------------------------------

fn sizes(what: &str, pixels: usize, rgba: &[&[u8]], planes: &[usize]) -> Result<(), String> {
    if rgba.iter().any(|p| p.len() != pixels * 4) || planes.iter().any(|n| *n != pixels) {
        return Err(format!(
            "{what}: the pictures and masks are not of one size"
        ));
    }
    Ok(())
}

/// The count of pixels the mask does not weigh (an effective weight of 0) that differ, in
/// any channel, between the picture the recipe was given and its result.
pub fn changed_outside(input: &[u8], output: &[u8], weights: &[f32]) -> Result<u64, String> {
    sizes("preservation", weights.len(), &[input, output], &[])?;
    let changed = (weights.iter().enumerate())
        .filter(|(at, w)| **w == 0.0 && input[at * 4..at * 4 + 4] != output[at * 4..at * 4 + 4])
        .count();
    Ok(changed as u64)
}

/// The IoU of the mask, where its weight is at least a half, with the drawn defect's.
pub fn iou(truth: &[bool], weights: &[f32]) -> Result<f64, String> {
    sizes("IoU", truth.len(), &[], &[weights.len()])?;
    let (mut both, mut either) = (0_u64, 0_u64);
    for (t, w) in truth.iter().zip(weights) {
        let masked = *w >= 0.5;
        both += u64::from(*t && masked);
        either += u64::from(*t || masked);
    }
    Ok(if either == 0 {
        0.0
    } else {
        both as f64 / either as f64
    })
}

/// The share of the drawn defect's pixels inside the mask, where its weight is at least a
/// half.
pub fn coverage(truth: &[bool], weights: &[f32]) -> Result<f64, String> {
    let (both, defect, _) = overlap("coverage", truth, weights)?;
    Ok(if defect == 0 {
        0.0
    } else {
        both as f64 / defect as f64
    })
}

/// The mask's pixels, where its weight is at least a half, as a multiple of the drawn
/// defect's.
pub fn size_ratio(truth: &[bool], weights: &[f32]) -> Result<f64, String> {
    let (_, defect, masked) = overlap("the mask's size", truth, weights)?;
    Ok(if defect == 0 {
        0.0
    } else {
        masked as f64 / defect as f64
    })
}

/// The pixels of the drawn defect inside the mask (where its weight is at least a half),
/// of the defect, and of the mask.
fn overlap(what: &str, truth: &[bool], weights: &[f32]) -> Result<(u64, u64, u64), String> {
    sizes(what, truth.len(), &[], &[weights.len()])?;
    let (mut both, mut defect, mut masked) = (0_u64, 0_u64, 0_u64);
    for (t, w) in truth.iter().zip(weights) {
        let inside = *w >= 0.5;
        both += u64::from(*t && inside);
        defect += u64::from(*t);
        masked += u64::from(inside);
    }
    Ok((both, defect, masked))
}

/// The mean absolute error, per channel of red, green and blue, inside `region`.
fn mean_error(result: &[u8], clean: &[u8], region: &[bool]) -> f64 {
    let (mut sum, mut count) = (0_u64, 0_u64);
    for (at, _) in region.iter().enumerate().filter(|(_, r)| **r) {
        for c in 0..3 {
            sum += u64::from(result[at * 4 + c].abs_diff(clean[at * 4 + c]));
        }
        count += 3;
    }
    if count == 0 {
        0.0
    } else {
        sum as f64 / count as f64
    }
}

/// A plain blur fill of what the mask weighs: each such pixel is the mean of the pixels
/// the mask does not weigh in the smallest square around it, 9 pixels across and doubling,
/// that holds any; then put back through the mask's weights. Pixels it does not weigh are
/// left as they are.
pub fn blur_fill(
    input: &[u8],
    weights: &[f32],
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    let (w, h) = (width as usize, height as usize);
    sizes("the blur fill", w * h, &[input], &[weights.len()])?;
    // Sums over the rectangle from the top left, of each channel and of the count of
    // pixels the mask does not weigh.
    let stride = w + 1;
    let mut sums = vec![[0_u64; 4]; stride * (h + 1)];
    for y in 0..h {
        for x in 0..w {
            let at = y * w + x;
            let mut here = [0_u64; 4];
            if weights[at] == 0.0 {
                for c in 0..3 {
                    here[c] = u64::from(input[at * 4 + c]);
                }
                here[3] = 1;
            }
            for c in 0..4 {
                sums[(y + 1) * stride + x + 1][c] =
                    here[c] + sums[y * stride + x + 1][c] + sums[(y + 1) * stride + x][c]
                        - sums[y * stride + x][c];
            }
        }
    }
    let total = |x0: usize, y0: usize, x1: usize, y1: usize| {
        let mut out = [0_u64; 4];
        for (c, value) in out.iter_mut().enumerate() {
            *value = sums[y1 * stride + x1][c] + sums[y0 * stride + x0][c]
                - sums[y0 * stride + x1][c]
                - sums[y1 * stride + x0][c];
        }
        out
    };
    let mut out = input.to_vec();
    let largest = w.max(h);
    for y in 0..h {
        for x in 0..w {
            let at = y * w + x;
            let weight = f64::from(weights[at]);
            if weight == 0.0 {
                continue;
            }
            let mut reach = 4;
            let found = loop {
                let found = total(
                    x.saturating_sub(reach),
                    y.saturating_sub(reach),
                    (x + reach + 1).min(w),
                    (y + reach + 1).min(h),
                );
                if found[3] > 0 || reach >= largest {
                    break found;
                }
                reach *= 2;
            };
            for c in 0..3 {
                let original = f64::from(input[at * 4 + c]);
                let filled = if found[3] > 0 {
                    found[c] as f64 / found[3] as f64
                } else {
                    original
                };
                let mixed = original * (1.0 - weight) + filled * weight;
                out[at * 4 + c] = mixed.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    Ok(out)
}

/// What one item scored.
#[derive(Debug, Clone, PartialEq)]
pub struct Scored {
    pub changed_outside: u64,
    pub iou: f64,
    pub coverage: f64,
    pub size_ratio: f64,
    pub fill_error: f64,
    pub blur_error: f64,
}

/// One item scored: the picture the recipe was given, the clean picture, the drawn
/// defect's mask, the recipe's result and its mask's effective weights.
pub fn score_item(
    input: &[u8],
    clean: &[u8],
    truth: &[bool],
    output: &[u8],
    weights: &[f32],
    width: u32,
    height: u32,
) -> Result<Scored, String> {
    let pixels = width as usize * height as usize;
    sizes(
        "an item",
        pixels,
        &[input, clean, output],
        &[truth.len(), weights.len()],
    )?;
    let blurred = blur_fill(input, weights, width, height)?;
    Ok(Scored {
        changed_outside: changed_outside(input, output, weights)?,
        iou: iou(truth, weights)?,
        coverage: coverage(truth, weights)?,
        size_ratio: size_ratio(truth, weights)?,
        fill_error: mean_error(output, clean, truth),
        blur_error: mean_error(&blurred, clean, truth),
    })
}

/// What `eval edits score` reads: the folder the draw wrote, the lab's files, the store of
/// the round, and the commit the record names.
pub struct Scoring<'a> {
    pub set: &'a Path,
    pub labs: &'a Path,
    pub root: &'a Path,
    pub commit: &'a str,
}

/// One recipe's aggregates.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecipeScore {
    pub recipe: String,
    /// The SHA-256 of the shipped recipe's text.
    pub recipe_digest: String,
    /// Each constant set a step selects, as `<name> sha256:<hex>` of the shipped text.
    pub constant_sets: Vec<String>,
    pub lab: String,
    pub protocol_digest: String,
    /// The backends and models the runs' requests went to, by capability.
    pub filler: String,
    pub finder: String,
    pub items: u64,
    pub completed: u64,
    pub failed: u64,
    /// Items with no completed or failed attempt: none, or one interrupted.
    pub not_run: u64,
    pub changed_outside_mask: u64,
    pub items_preserved: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mask_iou_mean: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mask_iou_min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage_mean: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage_min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_ratio_mean: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill_error_mean: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blur_error_mean: Option<f64>,
    /// Items whose fill's error is below the blur fill's.
    pub fill_beats_blur: u64,
    /// What the requests of every attempt of the lab's cells, and of the steps' runs under
    /// them, were settled at; and of that, what no answer settled.
    pub settled_micros: u64,
    pub unresolved_micros: u64,
}

/// The record of a scoring: the run's head, and each recipe's aggregates.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Record {
    pub commit: String,
    pub whizbang: String,
    pub set: String,
    pub seed: u64,
    pub phrase: String,
    pub pictures: u64,
    pub items: u64,
    pub settled_micros: u64,
    pub unresolved_micros: u64,
    pub recipe: Vec<RecipeScore>,
}

fn shown(value: Option<f64>) -> String {
    value.map_or_else(|| "none".to_owned(), |v| format!("{v:.4}"))
}

impl Record {
    /// `docs/results/m3-edits.md`: what was measured and how, and the numbers, as blocks of
    /// `name = "value"` lines.
    pub fn page(&self) -> String {
        let mut page = String::from(
            "# The edits' rung 2: task success and preservation\n\n\
             Plan m3, task B1, item 1 (`SPEC.md` section 8, \"The ladder\"). The synthetic edit set\n\
             (`labs/m3-edits/edits.toml`) was drawn by `whizbang eval edits draw` from the pictures\n\
             of the set below, each held to its job's pin: on each picture a shaded red ball with a\n\
             box (`remove`, and `remove_prompt` asked for the phrase below), a thin light scratch\n\
             with a stroke along it (`repair`), and a dark spot with a box (`repair_area`), the\n\
             drawn defect's exact mask kept as truth and the clean picture as the reference. Each\n\
             recipe ran as the lab `m3-edits-<recipe>` (`labs/m3-edits/`), and `whizbang eval edits\n\
             score` read its runs from the store. Only aggregates are kept, never an item.\n\n\
             - **Preservation**: pixels whose weight in the result's mask is 0 and which differ from\n\
             \x20 the picture the recipe was given, summed over the items (0 is required).\n\
             - **Task success**: the IoU of the result's mask, where its weight is at least a half,\n\
             \x20 with the drawn defect's mask (for `remove_prompt`, the whole recipe's shaped mask);\n\
             \x20 and the fill's mean error per channel (0 to 255) inside the drawn defect's mask\n\
             \x20 against the clean picture, beside that of a plain blur fill of the same mask.\n\
             \x20 Beside the IoU, the defect's coverage (the share of the drawn defect inside the\n\
             \x20 mask, where its weight is at least a half) and the mask's size as a multiple of\n\
             \x20 the defect's: each recipe grows and feathers its mask on purpose, so IoU alone\n\
             \x20 understates a mask that covers the whole defect.\n\n\
             ## The run\n\n```toml\n",
        );
        for (name, value) in [
            ("commit", self.commit.clone()),
            ("whizbang", self.whizbang.clone()),
            ("set", self.set.clone()),
            ("seed", self.seed.to_string()),
            ("phrase", self.phrase.clone()),
            ("pictures", self.pictures.to_string()),
            ("items", self.items.to_string()),
            ("settled_micros", self.settled_micros.to_string()),
            ("unresolved_micros", self.unresolved_micros.to_string()),
        ] {
            page.push_str(&format!("{name} = \"{value}\"\n"));
        }
        page.push_str("```\n");
        for recipe in &self.recipe {
            page.push_str(&format!("\n## {}\n\n```toml\n", recipe.recipe));
            let sets = if recipe.constant_sets.is_empty() {
                "none".to_owned()
            } else {
                recipe.constant_sets.join(", ")
            };
            for (name, value) in [
                ("recipe", recipe.recipe.clone()),
                ("recipe_digest", recipe.recipe_digest.clone()),
                ("constant_sets", sets),
                ("lab", recipe.lab.clone()),
                ("protocol_digest", recipe.protocol_digest.clone()),
                ("filler", recipe.filler.clone()),
                ("finder", recipe.finder.clone()),
                ("items", recipe.items.to_string()),
                ("completed", recipe.completed.to_string()),
                ("failed", recipe.failed.to_string()),
                ("not_run", recipe.not_run.to_string()),
                (
                    "changed_outside_mask",
                    recipe.changed_outside_mask.to_string(),
                ),
                ("items_preserved", recipe.items_preserved.to_string()),
                ("mask_iou_mean", shown(recipe.mask_iou_mean)),
                ("mask_iou_min", shown(recipe.mask_iou_min)),
                ("coverage_mean", shown(recipe.coverage_mean)),
                ("coverage_min", shown(recipe.coverage_min)),
                ("size_ratio_mean", shown(recipe.size_ratio_mean)),
                ("fill_error_mean", shown(recipe.fill_error_mean)),
                ("blur_error_mean", shown(recipe.blur_error_mean)),
                ("fill_beats_blur", recipe.fill_beats_blur.to_string()),
                ("settled_micros", recipe.settled_micros.to_string()),
                ("unresolved_micros", recipe.unresolved_micros.to_string()),
            ] {
                page.push_str(&format!("{name} = \"{value}\"\n"));
            }
            page.push_str("```\n");
        }
        page
    }

    /// `docs/results/m3-edits.toml`: the same, for a program.
    pub fn toml(&self) -> String {
        let mut text = format!(
            "commit = {}\nwhizbang = {}\nset = {}\nseed = {}\nphrase = {}\npictures = {}\n\
             items = {}\nsettled_micros = {}\nunresolved_micros = {}\n",
            quote(&self.commit),
            quote(&self.whizbang),
            quote(&self.set),
            self.seed,
            quote(&self.phrase),
            self.pictures,
            self.items,
            self.settled_micros,
            self.unresolved_micros
        );
        for r in &self.recipe {
            let sets: Vec<String> = r.constant_sets.iter().map(|s| quote(s)).collect();
            text.push_str(&format!(
                "\n[[recipe]]\nrecipe = {}\nrecipe_digest = {}\nconstant_sets = [{}]\nlab = {}\n\
                 protocol_digest = {}\nfiller = {}\nfinder = {}\nitems = {}\ncompleted = {}\n\
                 failed = {}\nnot_run = {}\nchanged_outside_mask = {}\nitems_preserved = {}\n\
                 fill_beats_blur = {}\nsettled_micros = {}\nunresolved_micros = {}\n",
                quote(&r.recipe),
                quote(&r.recipe_digest),
                sets.join(", "),
                quote(&r.lab),
                quote(&r.protocol_digest),
                quote(&r.filler),
                quote(&r.finder),
                r.items,
                r.completed,
                r.failed,
                r.not_run,
                r.changed_outside_mask,
                r.items_preserved,
                r.fill_beats_blur,
                r.settled_micros,
                r.unresolved_micros
            ));
            for (name, value) in [
                ("mask_iou_mean", r.mask_iou_mean),
                ("mask_iou_min", r.mask_iou_min),
                ("coverage_mean", r.coverage_mean),
                ("coverage_min", r.coverage_min),
                ("size_ratio_mean", r.size_ratio_mean),
                ("fill_error_mean", r.fill_error_mean),
                ("blur_error_mean", r.blur_error_mean),
            ] {
                if let Some(value) = value {
                    text.push_str(&format!("{name} = {value:?}\n"));
                }
            }
        }
        text
    }

    /// What `eval edits score` prints: counts.
    pub fn text(&self) -> String {
        let mut text = String::new();
        for r in &self.recipe {
            text.push_str(&format!(
                "{}: items {}, completed {}, failed {}, not run {}, changed outside the mask {}\n",
                r.recipe, r.items, r.completed, r.failed, r.not_run, r.changed_outside_mask
            ));
        }
        text
    }
}

/// A file of the set's folder, held to the digest the manifest pins.
fn pinned(set: &Path, file: &str, sha256: &str) -> Result<Vec<u8>, String> {
    let bytes = whizbang_eval::lab::read_relative(set, file, import::MAX_FILE_BYTES)?;
    if digest_of(&bytes) != sha256 {
        return Err(format!(
            "{file}: not the file the edit set pins (another SHA-256)"
        ));
    }
    Ok(bytes)
}

fn text_of(dir: &Path, file: &str) -> Result<String, String> {
    let bytes = whizbang_eval::lab::read_relative(dir, file, TEXT_LIMIT)?;
    String::from_utf8(bytes).map_err(|_| format!("{file} is not UTF-8"))
}

/// What the store holds of one recipe's cell: its last attempt, how it ended, and its
/// outputs by port.
struct Attempt {
    attempt: u32,
    ended: Ended,
    outputs: BTreeMap<String, ArtifactId>,
}

/// What the store holds of one lab: each cell's last attempt by the image it was given,
/// what every attempt and the steps' runs under it were settled at and of that what no
/// answer settled, and the models their requests went to.
#[derive(Default)]
struct Held {
    attempts: BTreeMap<ArtifactId, Attempt>,
    settled_micros: u64,
    unresolved_micros: u64,
    models: BTreeMap<Capability, BTreeSet<String>>,
}

/// What one run of the store says of its spend and its models, and the run that called it.
struct Spent {
    parent: Option<String>,
    settled_micros: u64,
    unresolved_micros: u64,
    models: Vec<(Capability, String)>,
}

/// The most runs walked up from a step's run to the lab's cell that called it.
const NESTING_LIMIT: usize = 64;

/// One recipe's lab, as committed: its scope, and the shipped recipe it runs.
struct Lab {
    recipe: String,
    lab: String,
    protocol_digest: String,
    scope: String,
    recipe_digest: String,
    constant_sets: Vec<String>,
}

fn lab_of(labs: &Path, recipe: &str) -> Result<Lab, String> {
    let registry = crate::wiring::registry()?;
    let protocol_text = text_of(labs, &format!("{recipe}-protocol.toml"))?;
    let (protocol, digest): (Protocol, String) = Protocol::parse(protocol_text.as_bytes())?;
    let matrix = Matrix::parse(&text_of(labs, &format!("{recipe}-matrix.toml"))?)?;
    let corpus = Corpus::parse(&text_of(labs, &matrix.corpus)?)?;
    let load = |name: &str| laboratory::recipe(&registry, name);
    let validated = matrix.validate(&protocol, &digest, &corpus, &registry, &load)?;
    let Some(Checks::Recipe(checked)) =
        (validated.arms.iter()).find(|a| matches!(a, Checks::Recipe(_)))
    else {
        return Err(format!("the lab of {recipe} runs no recipe"));
    };
    if checked.name != recipe {
        return Err(format!("the lab of {recipe} runs {}", checked.name));
    }
    let names: BTreeSet<&str> = (checked.steps.iter())
        .filter_map(|s| s.constants.as_deref())
        .collect();
    let constant_sets = names
        .into_iter()
        .map(|name| {
            let text = laboratory::constants(name).unwrap_or_default();
            format!("{name} {}", digest_of(text.as_bytes()))
        })
        .collect();
    Ok(Lab {
        recipe: recipe.to_owned(),
        lab: protocol.name.clone(),
        scope: protocol::scope(&protocol.name, &digest),
        protocol_digest: digest,
        recipe_digest: format!("sha256:{}", checked.digest.trim_start_matches("sha256:")),
        constant_sets,
    })
}

/// Every lab's cells the store at `root` holds, by the lab's scope.
fn held(root: &Path, labs: &[Lab]) -> Result<BTreeMap<String, Held>, String> {
    let mut found: BTreeMap<String, Held> = BTreeMap::new();
    // Every run's spend and models, and each lab cell's run by its id with its lab's scope:
    // a fill or a find is a step's run nested under the recipe's, and holds what was paid.
    let mut spent: BTreeMap<String, Spent> = BTreeMap::new();
    let mut cells: BTreeMap<String, String> = BTreeMap::new();
    harness::each_run(root, &mut |record, run, ended| {
        let paid = &record.spend().requests;
        spent.insert(
            run.to_owned(),
            Spent {
                parent: record.identity().parent.clone(),
                settled_micros: paid.iter().map(|p| p.settled_micros()).sum(),
                unresolved_micros: paid.iter().map(|p| p.unresolved_micros()).sum(),
                models: (record.models().iter())
                    .map(|m| {
                        let name = format!("{} {} {}", m.backend, m.model, m.version);
                        (m.capability, name)
                    })
                    .collect(),
            },
        );
        let Some((scope, _, attempt)) = harness::named_by_request(record) else {
            return;
        };
        let Some(lab) = labs.iter().find(|l| l.scope == scope) else {
            return;
        };
        if record.asked().action.as_deref() != Some(lab.recipe.as_str()) {
            return;
        }
        let Some(image) = (record.inputs().iter()).find(|i| i.port == "image") else {
            return;
        };
        cells.insert(run.to_owned(), scope.clone());
        let held = found.entry(scope).or_default();
        let outputs = (record.outputs().iter())
            .map(|o| (o.port.clone(), ArtifactId(o.artifact)))
            .collect();
        let image = ArtifactId(image.artifact);
        let later = (held.attempts.get(&image)).is_none_or(|a| a.attempt < attempt);
        if later {
            held.attempts.insert(
                image,
                Attempt {
                    attempt,
                    ended,
                    outputs,
                },
            );
        }
    })?;
    for (run, what) in &spent {
        // The lab cell this run is, or is nested under.
        let mut at = Some(run.as_str());
        let mut scope = None;
        for _ in 0..NESTING_LIMIT {
            let Some(here) = at else { break };
            if let Some(of) = cells.get(here) {
                scope = Some(of);
                break;
            }
            at = spent.get(here).and_then(|s| s.parent.as_deref());
        }
        let Some(held) = scope.and_then(|of| found.get_mut(of)) else {
            continue;
        };
        held.settled_micros += what.settled_micros;
        held.unresolved_micros += what.unresolved_micros;
        for (capability, name) in &what.models {
            (held.models.entry(*capability).or_default()).insert(name.clone());
        }
    }
    Ok(found)
}

/// A recipe's completed result read from the store: its pixels and its mask's weights.
fn result_of(
    reader: &Reader,
    outputs: &BTreeMap<String, ArtifactId>,
) -> Result<(Vec<u8>, Vec<f32>), String> {
    let limits = DecodeLimits::default();
    let missing = |port: &str| format!("a completed run has no output {port}");
    let image = outputs.get("image").ok_or_else(|| missing("image"))?;
    let mask = outputs.get("mask").ok_or_else(|| missing("mask"))?;
    let Artifact::Image(image) =
        artifact::get(reader, ArtifactType::Image, *image, &limits).map_err(|e| e.to_string())?
    else {
        return Err(missing("image"));
    };
    let Artifact::Mask(mask) =
        artifact::get(reader, ArtifactType::Mask, *mask, &limits).map_err(|e| e.to_string())?
    else {
        return Err(missing("mask"));
    };
    let pixels = image.pixels(&limits).map_err(|e| e.to_string())?;
    let space = mask.space();
    let mut weights = Vec::with_capacity(space.width() as usize * space.height() as usize);
    for y in 0..space.height() {
        for x in 0..space.width() {
            weights.push(mask.weight(x, y).unwrap_or(0.0));
        }
    }
    Ok((pixels.rgba().to_vec(), weights))
}

fn mean(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

/// Scores the round of each recipe the store at `root` holds, against the edit set the
/// draw wrote at `set` and the lab's files at `labs`.
pub fn score(scoring: &Scoring<'_>) -> Result<Record, String> {
    let commit = scoring.commit;
    if commit.len() != 40
        || !commit
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err("the commit is the 40 hex digits `git rev-parse HEAD` prints".to_owned());
    }
    let set = EditSet::parse(&text_of(scoring.set, "edits.toml")?)?;
    let labs = (RECIPES.iter())
        .map(|recipe| lab_of(scoring.labs, recipe))
        .collect::<Result<Vec<Lab>, String>>()?;
    let held = held(scoring.root, &labs)?;
    let reader = Reader::open(scoring.root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let limits = DecodeLimits::default();
    let mut cleans = BTreeMap::new();
    for source in &set.source {
        let bytes = pinned(scoring.set, &source.clean, &source.clean_sha256)?;
        let pixels = identify(&bytes, &limits).map_err(|e| e.to_string())?.pixels;
        cleans.insert(source.id.clone(), pixels);
    }
    let empty = Held::default();
    let mut recipes = Vec::new();
    for lab in &labs {
        let held = held.get(&lab.scope).unwrap_or(&empty);
        let models = |capability: Capability| {
            (held.models.get(&capability)).map_or_else(
                || "none".to_owned(),
                |names| names.iter().cloned().collect::<Vec<_>>().join(", "),
            )
        };
        let mut score = RecipeScore {
            recipe: lab.recipe.clone(),
            recipe_digest: lab.recipe_digest.clone(),
            constant_sets: lab.constant_sets.clone(),
            lab: lab.lab.clone(),
            protocol_digest: lab.protocol_digest.clone(),
            filler: models(Capability::Fill),
            finder: models(Capability::Segment),
            items: 0,
            completed: 0,
            failed: 0,
            not_run: 0,
            changed_outside_mask: 0,
            items_preserved: 0,
            mask_iou_mean: None,
            mask_iou_min: None,
            coverage_mean: None,
            coverage_min: None,
            size_ratio_mean: None,
            fill_error_mean: None,
            blur_error_mean: None,
            fill_beats_blur: 0,
            settled_micros: held.settled_micros,
            unresolved_micros: held.unresolved_micros,
        };
        let (mut ious, mut fills, mut blurs) = (Vec::new(), Vec::new(), Vec::new());
        let (mut covered, mut ratios) = (Vec::new(), Vec::new());
        for item in set.item.iter().filter(|i| i.recipe == lab.recipe) {
            score.items += 1;
            let bytes = pinned(scoring.set, &item.picture, &item.picture_sha256)?;
            let image = import::image_of_bytes(&bytes, Projection::Flat)?.id();
            let attempt = held.attempts.get(&image);
            match attempt.map(|a| a.ended) {
                Some(Ended::Completed) => {}
                Some(Ended::Failed) => {
                    score.failed += 1;
                    continue;
                }
                _ => {
                    score.not_run += 1;
                    continue;
                }
            }
            let outputs = attempt.map(|a| &a.outputs).ok_or("no attempt")?;
            let input = identify(&bytes, &limits).map_err(|e| e.to_string())?.pixels;
            let truth_bytes = pinned(scoring.set, &item.truth, &item.truth_sha256)?;
            let truth: Vec<bool> = (identify(&truth_bytes, &limits).map_err(|e| e.to_string())?)
                .pixels
                .rgba()
                .chunks(4)
                .map(|p| p[0] == 255)
                .collect();
            let clean = cleans
                .get(&item.source)
                .ok_or_else(|| format!("{}: no clean picture", item.id))?;
            let (output, weights) = result_of(&reader, outputs)?;
            let scored = score_item(
                input.rgba(),
                clean.rgba(),
                &truth,
                &output,
                &weights,
                input.width(),
                input.height(),
            )?;
            score.completed += 1;
            score.changed_outside_mask += scored.changed_outside;
            score.items_preserved += u64::from(scored.changed_outside == 0);
            score.fill_beats_blur += u64::from(scored.fill_error < scored.blur_error);
            ious.push(scored.iou);
            covered.push(scored.coverage);
            ratios.push(scored.size_ratio);
            fills.push(scored.fill_error);
            blurs.push(scored.blur_error);
        }
        score.mask_iou_mean = mean(&ious);
        score.mask_iou_min = ious.iter().copied().reduce(f64::min);
        score.coverage_mean = mean(&covered);
        score.coverage_min = covered.iter().copied().reduce(f64::min);
        score.size_ratio_mean = mean(&ratios);
        score.fill_error_mean = mean(&fills);
        score.blur_error_mean = mean(&blurs);
        recipes.push(score);
    }
    Ok(Record {
        commit: commit.to_owned(),
        whizbang: env!("CARGO_PKG_VERSION").to_owned(),
        set: set.set.clone(),
        seed: set.seed,
        phrase: set.phrase.clone(),
        pictures: set.source.len() as u64,
        items: set.item.len() as u64,
        settled_micros: recipes.iter().map(|r| r.settled_micros).sum(),
        unresolved_micros: recipes.iter().map(|r| r.unresolved_micros).sum(),
        recipe: recipes,
    })
}

/// Writes the record as `m3-edits.md` and `m3-edits.toml` in `out`.
pub fn write_record(record: &Record, out: &Path) -> Result<(), String> {
    let failed = |why: std::io::Error| format!("the record could not be written: {why}");
    std::fs::create_dir_all(out).map_err(failed)?;
    std::fs::write(out.join("m3-edits.md"), record.page()).map_err(failed)?;
    std::fs::write(out.join("m3-edits.toml"), record.toml()).map_err(failed)
}
