//! What the playground keeps in its store beside runs, and reads back for the page (plan
//! m5, task W1, item 3): a picture the page sent, imported as `whizbang store import
//! --picture` imports one, through the host's own writer, with the name it came with in
//! the same publication; a shape drawn on a picture, committed as a location bound to it;
//! an artifact as the page shows it; and what the ledger says a run spent.
//!
//! A picture is kept as the import keeps every picture: its canonical pixels as a PNG,
//! with nothing of the file's metadata. The name a person gave it is the one thing kept
//! beside it, under `play/pictures/<the image's digest>`, at most 200 characters, kept as
//! given and escaped when it is shown. The same picture sent again under another name
//! keeps the newest name and moves to the front of the list: one picture, one entry. A
//! picture that fails to decode, or to publish, leaves nothing behind.

use std::io::Cursor;

use image::{ExtendedColorType, ImageEncoder, ImageReader};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use whizbang_core::artifact::{self, Artifact, ArtifactId, Location, Origin, Shape, Step};
use whizbang_core::contract::RunId;
use whizbang_core::identity::{DecodeLimits, Digest, IdentityError, Point, Projection};
use whizbang_core::spend::fold::Folded;
use whizbang_core::spend::reading::{Names, Reading};
use whizbang_core::store::Store;
use whizbang_serve::escaped;
use whizbang_serve::play::Refused;

use crate::import;

/// What the name of each picture the playground kept starts with; the image's digest's
/// hex follows.
pub const PICTURES: &str = "play/pictures/";

/// The longest name a picture is kept with, in characters.
pub const NAME_LIMIT: usize = 200;

/// The step a drawn location's origin names.
pub const DRAW_STEP: &str = "play-draw";

/// The most points of a drawn polygon (ADR 0002's).
pub const POLYGON_LIMIT: usize = 64;

/// The most points of a drawn stroke or line.
pub const PATH_LIMIT: usize = 512;

/// What is kept beside a picture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Named {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub at_ms: u64,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

/// Keeps a picture the page sent: decoded within the core's limits, refused whole when it
/// is over them (413, naming the limit) or is no picture (400), and otherwise committed
/// with its name beside it in one publication.
pub fn picture(store: &Store, bytes: &[u8], name: &str) -> Result<Json, Refused> {
    if name.chars().count() > NAME_LIMIT {
        return Err(Refused::new(
            400,
            "a picture's name is at most 200 characters",
        ));
    }
    let limits = DecodeLimits::default();
    // Decoded once, within the core's limits: what judges the bytes is what is kept.
    let identified =
        whizbang_core::identity::identify(bytes, &limits).map_err(|error| match error {
            IdentityError::Limit(why) => Refused::new(
                413,
                &format!("the picture is over the core's decode limits: {why}"),
            ),
            _ => Refused::new(400, "the body is not a picture this build reads"),
        })?;
    let image = import::image_of_identified(&identified, &limits, Projection::Flat)
        .map_err(|_| Refused::new(400, "the body is not a picture this build reads"))?;
    let space = image.space();
    let id = image.id();
    let named = Named {
        id: id.to_string(),
        name: name.to_owned(),
        width: space.width(),
        height: space.height(),
        at_ms: now_ms(),
    };
    let text = serde_json::to_vec(&named).map_err(|_| Refused::new(500, "cannot keep the name"))?;
    let beside = vec![(format!("{PICTURES}{}", id.0.hex()), text)];
    let file = Digest::of_bytes(bytes);
    import::commit_to(
        store,
        &Artifact::Image(image),
        Some((import::IMPORTED, file)),
        &beside,
    )
    .map_err(|_| Refused::new(500, "the store could not keep the picture"))?;
    Ok(json!({"id": named.id, "name": escaped(&named.name),
              "width": named.width, "height": named.height}))
}

/// The pictures the playground kept, newest first: at most `limit`, after the picture
/// `after` names when it names one.
pub fn pictures(store: &Store, after: Option<&str>, limit: usize) -> Result<Json, Refused> {
    let generation = store
        .snapshot()
        .map_err(|_| Refused::new(500, "the store could not answer"))?;
    let mut kept: Vec<Named> = generation
        .entries()
        .range(PICTURES.to_owned()..)
        .take_while(|(name, _)| name.starts_with(PICTURES))
        .filter_map(|(_, entry)| {
            let bytes = store.read(entry).ok()?;
            serde_json::from_slice::<Named>(&bytes).ok()
        })
        .collect();
    kept.sort_by(|a, b| b.at_ms.cmp(&a.at_ms).then_with(|| a.id.cmp(&b.id)));
    let start = match after {
        Some(after) => match kept.iter().position(|one| one.id == after) {
            Some(at) => at + 1,
            None => {
                return Err(Refused::new(
                    400,
                    "after names no picture the playground kept",
                ));
            }
        },
        None => 0,
    };
    let listed: Vec<Json> = kept
        .iter()
        .skip(start)
        .take(limit)
        .map(|one| {
            json!({"id": one.id, "name": escaped(&one.name), "width": one.width,
                   "height": one.height, "at_ms": one.at_ms})
        })
        .collect();
    Ok(json!({ "pictures": listed }))
}

/// The picture the playground kept under this identity, as the list shows it, or none.
pub fn kept(store: &Store, id: &str) -> Result<Option<Json>, Refused> {
    let Ok(digest) = id.parse::<Digest>() else {
        return Ok(None);
    };
    let generation = store
        .snapshot()
        .map_err(|_| Refused::new(500, "the store could not answer"))?;
    let Some(entry) = generation.get(&format!("{PICTURES}{}", digest.hex())) else {
        return Ok(None);
    };
    let one =
        (store.read(entry).ok()).and_then(|bytes| serde_json::from_slice::<Named>(&bytes).ok());
    Ok(one.map(|one| {
        json!({"id": one.id, "name": escaped(&one.name), "width": one.width,
               "height": one.height, "at_ms": one.at_ms})
    }))
}

fn not_found() -> Refused {
    Refused::new(404, "the store holds no artifact of that identity")
}

/// Reads the artifact of this identity, of whichever type it is: its type found once
/// (`artifact::type_anywhere`), then it is read.
fn read_any(store: &Store, id: &str) -> Result<Artifact, Refused> {
    let id = id
        .parse::<Digest>()
        .map(ArtifactId)
        .map_err(|_| not_found())?;
    let limits = DecodeLimits::default();
    let generation = store
        .snapshot()
        .map_err(|_| Refused::new(500, "the store could not answer"))?;
    let kind = artifact::type_anywhere(&generation, id).ok_or_else(not_found)?;
    artifact::get_anywhere(store, kind, id, &limits).map_err(|_| not_found())
}

fn png_of(
    width: u32,
    height: u32,
    bytes: &[u8],
    colour: ExtendedColorType,
) -> Result<Vec<u8>, Refused> {
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(bytes, width, height, colour)
        .map_err(|_| Refused::new(500, "cannot draw the artifact"))?;
    Ok(png)
}

/// A PNG at most `edge` pixels on its long side, of a PNG.
fn thumbnail(png: &[u8], edge: u32) -> Result<Vec<u8>, Refused> {
    let decoded = ImageReader::with_format(Cursor::new(png), image::ImageFormat::Png)
        .decode()
        .map_err(|_| Refused::new(500, "cannot draw the artifact"))?;
    let (width, height) = (decoded.width(), decoded.height());
    if width.max(height) <= edge {
        return Ok(png.to_vec());
    }
    let small = decoded.thumbnail(edge, edge).to_rgba8();
    png_of(
        small.width(),
        small.height(),
        small.as_raw(),
        ExtendedColorType::Rgba8,
    )
}

/// An artifact as the page shows it: a picture as its PNG, a mask as a grey picture of its
/// core, a location as JSON; a picture or a mask at most `edge` pixels on its long side
/// when a thumbnail is asked for. Nothing is stored.
pub fn artifact(
    store: &Store,
    id: &str,
    edge: Option<u32>,
) -> Result<(&'static str, Vec<u8>), Refused> {
    let png = match read_any(store, id)? {
        Artifact::Image(image) => image.file().to_vec(),
        Artifact::Mask(mask) => {
            let space = mask.space();
            png_of(
                space.width(),
                space.height(),
                mask.core(),
                ExtendedColorType::L8,
            )?
        }
        Artifact::Location(location) => {
            return Ok((
                "application/json",
                location_json(&location).to_string().into_bytes(),
            ));
        }
        _ => return Err(Refused::new(415, "this artifact has no picture to show")),
    };
    match edge {
        Some(edge) => Ok(("image/png", thumbnail(&png, edge)?)),
        None => Ok(("image/png", png)),
    }
}

fn points_json(points: &[Point]) -> Json {
    Json::Array(points.iter().map(|p| json!([p.x, p.y])).collect())
}

/// A location as the page draws it.
pub fn location_json(location: &Location) -> Json {
    match location.shape() {
        Shape::Box {
            left,
            top,
            right,
            bottom,
        } => json!({"kind": "box", "left": left, "top": top, "right": right, "bottom": bottom}),
        Shape::Polygon { points } => json!({"kind": "polygon", "points": points_json(points)}),
        Shape::Stroke { path, radius } => {
            json!({"kind": "stroke", "points": points_json(path), "radius": radius})
        }
        Shape::Line {
            line: whizbang_core::artifact::Line::Path(path),
            width,
        } => json!({"kind": "line", "points": points_json(path), "width": width}),
        _ => json!({"kind": "other"}),
    }
}

/// A shape drawn on a picture, as the page sends it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Drawn {
    picture: String,
    shape: Drawing,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Drawing {
    Box {
        left: f64,
        top: f64,
        right: f64,
        bottom: f64,
    },
    Polygon {
        points: Vec<[f64; 2]>,
    },
    Stroke {
        points: Vec<[f64; 2]>,
        radius: f64,
    },
    Line {
        points: Vec<[f64; 2]>,
        width: f64,
    },
}

fn path(points: &[[f64; 2]], most: usize) -> Result<Vec<Point>, Refused> {
    if points.len() > most {
        return Err(Refused::new(
            400,
            &format!("a drawn shape has at most {most} points"),
        ));
    }
    Ok(points.iter().map(|[x, y]| Point { x: *x, y: *y }).collect())
}

/// Commits a shape drawn on a picture of the store as a location bound to that picture
/// (ADR 0002), marked as imported as an import is: the location's identity.
pub fn location(store: &Store, drawn: &Json) -> Result<Json, Refused> {
    let drawn: Drawn = serde_json::from_value(drawn.clone()).map_err(|_| {
        Refused::new(
            400,
            "a drawn location is a picture and a shape: a box, a polygon, a stroke or a line",
        )
    })?;
    let Artifact::Image(image) = read_any(store, &drawn.picture)? else {
        return Err(Refused::new(400, "a location is drawn on a picture"));
    };
    let shape = match drawn.shape {
        Drawing::Box {
            left,
            top,
            right,
            bottom,
        } => Shape::Box {
            left,
            top,
            right,
            bottom,
        },
        Drawing::Polygon { points } => Shape::Polygon {
            points: path(&points, POLYGON_LIMIT)?,
        },
        Drawing::Stroke { points, radius } => Shape::Stroke {
            path: path(&points, PATH_LIMIT)?,
            radius,
        },
        Drawing::Line { points, width } => Shape::Line {
            line: whizbang_core::artifact::Line::Path(path(&points, PATH_LIMIT)?),
            width,
        },
    };
    let origin = Origin::new(Step::new(DRAW_STEP, "1"))
        .map_err(|_| Refused::new(500, "cannot make the location"))?;
    let location = Location::new(image.binding(), shape, origin).map_err(|error| {
        Refused::new(
            400,
            &format!(
                "not a location on that picture: {}",
                escaped(&error.to_string())
            ),
        )
    })?;
    let id = location.id();
    import::commit_to(store, &Artifact::Location(location), None, &[])
        .map_err(|_| Refused::new(500, "the store could not keep the location"))?;
    Ok(json!({ "id": id.to_string() }))
}

/// Millionths of a US dollar as dollars.
#[allow(clippy::cast_precision_loss)]
fn dollars(micros: u128) -> f64 {
    micros as f64 / 1_000_000.0
}

/// The allowance of the newest epoch, what is counted against it since that reset (as
/// `GET /api/spend` counts it), split into what was spent and what runs still going hold
/// beyond it, whether the count is exact or only at least that, and what is left of it, in dollars, none before the first reset; with the number of the generation
/// it was read from. It folds the store's
/// books, so [`Playground`](crate::playing::Playground) keeps the answer for that
/// generation and folds again only when the store has published since.
pub fn allowance(store: &Store) -> Option<(u64, Option<Json>)> {
    let generation = store.snapshot().ok()?;
    let reading = Reading::of(&generation, |entry| store.read(entry));
    let balance = Folded::of(reading).balance();
    let (counted, exactly) = match balance.counted {
        whizbang_core::spend::reading::Counted::Exactly(sum) => (sum.micros(), true),
        whizbang_core::spend::reading::Counted::AtLeast(sum) => (sum.micros(), false),
    };
    // What is counted is what was spent and what runs still going hold beyond it: said
    // apart, so a run going does not read as spent.
    let held = balance.held.micros().min(counted);
    let said = (balance.epoch > 0).then(|| {
        json!({"allowance": dollars(u128::from(balance.allowed.micros())),
               "spent": dollars(counted - held),
               "held": dollars(held),
               "exactly": exactly,
               "left": dollars(u128::from(balance.left.micros()))})
    });
    Some((generation.number(), said))
}

/// What the ledger has settled for a run so far, and what its reservation holds, in
/// dollars: read from the names of that one reservation, as the meter reads them, with
/// no fold of the store's books.
pub fn spend(store: &Store, run: &RunId) -> (f64, f64) {
    let Ok(generation) = store.snapshot() else {
        return (0.0, 0.0);
    };
    match Names::of_run(run, &generation, |entry| store.read(entry)) {
        Some(names) => (
            dollars(names.charged().micros()),
            dollars(u128::from(names.amount().micros())),
        ),
        None => (0.0, 0.0),
    }
}

/// The journal a run that has ended keeps in the store: each line as JSON, in order, and
/// how many events the run counted and did not keep.
pub fn journal(store: &Store, run: &RunId) -> Option<(Vec<Json>, u64)> {
    let generation = store.snapshot().ok()?;
    let entry = generation
        .run(run.as_str())?
        .get(whizbang_core::contract::record::JOURNAL_NAME)?
        .clone();
    let bytes = store.read(&entry).ok()?;
    let journal = whizbang_core::contract::Journal::read(&bytes).ok()?;
    let lines = journal
        .lines()
        .iter()
        .filter_map(|line| serde_json::to_value(line).ok())
        .collect();
    let not_kept = journal.not_kept();
    Some((lines, not_kept.messages().saturating_add(not_kept.events())))
}
