//! Segment (`SPEC.md` section 4; ADR 0006, "Segment"): an image and a batch of requests,
//! each a phrase, a point or a box, go out; for each request, instances with planes,
//! boxes and scores come back. One request to a backend is one image, so whatever a
//! backend computes of the image it computes once for the batch.

use std::sync::Arc;
use std::time::Instant;

use crate::contract::Cancellation;

use super::{Bounds, Declaration, Fault, Picture, Unfit, feature, pixel_count};

/// The most requests one batch holds.
pub const QUERY_LIMIT: usize = 64;
/// The most instances that can be asked for one request.
pub const INSTANCE_LIMIT: u32 = 64;
/// The most bytes a phrase holds.
pub const PHRASE_LIMIT: usize = 512;

/// One request of a batch, as it is read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Asked<'a> {
    /// What to find, in words.
    Phrase(&'a str),
    /// The thing at this place, in pixels of the image.
    Point { x: f64, y: f64 },
    /// The thing in this box, in pixels of the image.
    Box {
        left: f64,
        top: f64,
        right: f64,
        bottom: f64,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Phrase(String),
    Point([f64; 2]),
    Box([f64; 4]),
}

/// One request of a batch: a phrase, a point or a box.
#[derive(Debug, Clone, PartialEq)]
pub struct Query(Kind);

fn numbers(values: &[f64]) -> String {
    let shown: Vec<String> = values.iter().map(f64::to_string).collect();
    shown.join(" ")
}

/// Whether the four numbers are a box with area: finite, not negative, the left under
/// the right and the top under the bottom.
fn is_box(b: &[f64; 4]) -> bool {
    b.iter().all(|v| v.is_finite() && *v >= 0.0) && b[0] < b[2] && b[1] < b[3]
}

impl Query {
    /// A request by a phrase of 1 to [`PHRASE_LIMIT`] bytes.
    pub fn phrase(phrase: &str) -> Result<Query, Unfit> {
        if phrase.is_empty() {
            return Err(Unfit::Empty { what: "the phrase" });
        }
        if phrase.len() > PHRASE_LIMIT {
            return Err(Unfit::TooLarge {
                what: "the phrase, in bytes,",
                size: phrase.len() as u64,
                limit: PHRASE_LIMIT as u64,
            });
        }
        Ok(Query(Kind::Phrase(phrase.to_owned())))
    }

    /// A request by a point: two finite numbers, neither negative.
    pub fn point(x: f64, y: f64) -> Result<Query, Unfit> {
        if !(x.is_finite() && y.is_finite() && x >= 0.0 && y >= 0.0) {
            return Err(Unfit::OutOfRange {
                what: "the point",
                at: None,
                found: numbers(&[x, y]),
                allowed: "in a pixel of the image",
            });
        }
        Ok(Query(Kind::Point([x, y])))
    }

    /// A request by a box with area: four finite numbers, none negative.
    pub fn boxed(left: f64, top: f64, right: f64, bottom: f64) -> Result<Query, Unfit> {
        let bounds = [left, top, right, bottom];
        if !is_box(&bounds) {
            return Err(Unfit::OutOfRange {
                what: "the box",
                at: None,
                found: numbers(&bounds),
                allowed: "inside the image, with area",
            });
        }
        Ok(Query(Kind::Box(bounds)))
    }

    /// The request, to read.
    pub fn asked(&self) -> Asked<'_> {
        match &self.0 {
            Kind::Phrase(phrase) => Asked::Phrase(phrase),
            Kind::Point([x, y]) => Asked::Point { x: *x, y: *y },
            Kind::Box([left, top, right, bottom]) => Asked::Box {
                left: *left,
                top: *top,
                right: *right,
                bottom: *bottom,
            },
        }
    }

    /// The feature a backend must declare to be sent this request.
    pub fn feature(&self) -> &'static str {
        match self.0 {
            Kind::Phrase(_) => feature::PHRASE,
            Kind::Point(_) => feature::POINT,
            Kind::Box(_) => feature::BOX,
        }
    }
}

/// What a segmenter is sent: one image, and a batch of requests about it.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentRequest {
    image: Picture,
    queries: Vec<Query>,
    max_instances: u32,
}

impl SegmentRequest {
    /// A batch of 1 to [`QUERY_LIMIT`] requests about this image, asking at most
    /// `max_instances` instances for each, from 1 to [`INSTANCE_LIMIT`]. A point is in a
    /// pixel of the image: its `x` is under the width and its `y` under the height. A
    /// box is inside the image, and its edges may be the image's edges.
    pub fn new(
        image: Picture,
        queries: Vec<Query>,
        max_instances: u32,
    ) -> Result<SegmentRequest, Unfit> {
        let what = "the batch of requests";
        if queries.is_empty() {
            return Err(Unfit::Empty { what });
        }
        if queries.len() > QUERY_LIMIT {
            return Err(Unfit::TooLarge {
                what,
                size: queries.len() as u64,
                limit: QUERY_LIMIT as u64,
            });
        }
        if !(1..=INSTANCE_LIMIT).contains(&max_instances) {
            return Err(Unfit::OutOfRange {
                what: "the most instances for a request",
                at: None,
                found: max_instances.to_string(),
                allowed: "from 1 to 64",
            });
        }
        let (width, height) = (f64::from(image.width()), f64::from(image.height()));
        for query in &queries {
            let (what, allowed, values): (_, _, &[f64]) = match &query.0 {
                Kind::Phrase(_) => continue,
                Kind::Point(point) => ("the point", "in a pixel of the image", point),
                Kind::Box(bounds) => ("the box", "inside the image, with area", bounds),
            };
            let is_point = matches!(query.0, Kind::Point(_));
            let inside = values.iter().enumerate().all(|(at, value)| {
                let side = if at % 2 == 0 { width } else { height };
                if is_point {
                    *value < side
                } else {
                    *value <= side
                }
            });
            if !inside {
                return Err(Unfit::OutOfRange {
                    what,
                    at: None,
                    found: numbers(values),
                    allowed,
                });
            }
        }
        Ok(SegmentRequest {
            image,
            queries,
            max_instances,
        })
    }

    /// The image: canonical pixels.
    pub fn image(&self) -> &Picture {
        &self.image
    }

    /// The batch, in order.
    pub fn queries(&self) -> &[Query] {
        &self.queries
    }

    /// The most instances wanted for each request.
    pub fn max_instances(&self) -> u32 {
        self.max_instances
    }
}

/// One thing a segmenter found: its plane, the size of the image, one byte a pixel, 255
/// on the thing and 0 off it; its box, in pixels of the image, which holds every pixel
/// that is on; and its score. What `{:?}` prints of one is its size, how many pixels
/// are on, its box and its score, never the plane.
#[derive(Clone, PartialEq)]
pub struct Instance {
    width: u32,
    height: u32,
    plane: Vec<u8>,
    bounds: [f64; 4],
    score: f64,
}

impl Instance {
    /// An instance in an image of this size. Refused: a size of no pixels, or over
    /// [`PIXEL_LIMIT`](super::PIXEL_LIMIT), before a byte is read; a plane of another
    /// count of bytes; a byte that is neither 0 nor 255; a plane with nothing on it; a
    /// box (left, top, right, bottom) that is not inside the image with area; a pixel
    /// that is on outside the box, which holds the whole pixels it touches; and a score
    /// that is not a number from 0 to 1.
    pub fn new(
        width: u32,
        height: u32,
        plane: Vec<u8>,
        bounds: [f64; 4],
        score: f64,
    ) -> Result<Instance, Unfit> {
        let what = "the plane of an instance";
        let expected = pixel_count(what, width, height)?;
        if plane.len() as u64 != expected {
            return Err(Unfit::WrongLength {
                what,
                expected,
                found: plane.len() as u64,
            });
        }
        if let Some(at) = plane.iter().position(|sample| !matches!(sample, 0 | 255)) {
            return Err(Unfit::OutOfRange {
                what,
                at: Some(at as u64),
                found: plane[at].to_string(),
                allowed: "0 or 255",
            });
        }
        if !plane.contains(&255) {
            return Err(Unfit::Empty { what });
        }
        if !(is_box(&bounds) && bounds[2] <= f64::from(width) && bounds[3] <= f64::from(height)) {
            return Err(Unfit::OutOfRange {
                what: "the box of an instance",
                at: None,
                found: numbers(&bounds),
                allowed: "inside the image, with area",
            });
        }
        // The whole pixels the box touches: from the floor of its left and top to the
        // ceiling of its right and bottom.
        let columns = bounds[0].floor() as u32..bounds[2].ceil() as u32;
        let rows = bounds[1].floor() as u32..bounds[3].ceil() as u32;
        let outside = plane.iter().enumerate().any(|(at, sample)| {
            let (x, y) = (at as u32 % width, at as u32 / width);
            *sample == 255 && !(columns.contains(&x) && rows.contains(&y))
        });
        if outside {
            return Err(Unfit::NotAllowed {
                what,
                why: "is on outside its box",
            });
        }
        if !(0.0..=1.0).contains(&score) {
            return Err(Unfit::OutOfRange {
                what: "the score of an instance",
                at: None,
                found: score.to_string(),
                allowed: "from 0 to 1",
            });
        }
        Ok(Instance {
            width,
            height,
            plane,
            bounds,
            score,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// One byte a pixel, row by row from the top left: 255 on the thing, 0 off it.
    pub fn plane(&self) -> &[u8] {
        &self.plane
    }

    /// Left, top, right and bottom, in pixels of the image.
    pub fn bounds(&self) -> [f64; 4] {
        self.bounds
    }

    /// From 0 to 1.
    pub fn score(&self) -> f64 {
        self.score
    }
}

impl std::fmt::Debug for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let on = self.plane.iter().filter(|sample| **sample == 255).count();
        write!(
            f,
            "Instance({} by {}, {on} on, box {:?}, score {})",
            self.width, self.height, self.bounds, self.score
        )
    }
}

/// What a segmenter answers: for each request of the batch, in the batch's order, the
/// instances it found, in the order the backend gives them.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentResponse {
    answers: Vec<Vec<Instance>>,
}

impl SegmentResponse {
    /// An answer. Whether it fits a request is checked by the handle that sent the
    /// request.
    pub fn new(answers: Vec<Vec<Instance>>) -> SegmentResponse {
        SegmentResponse { answers }
    }

    /// The instances of each request, in the batch's order.
    pub fn answers(&self) -> &[Vec<Instance>] {
        &self.answers
    }
}

/// A backend that segments. It answers one batch at a time and returns when it has its
/// answer; it never retries. It is expected to stop early when the run is cancelled, and
/// to give up at the run's deadline ([`Bounds`]).
pub trait Segmenter: Send + Sync {
    /// The model that answers, and the features it supports.
    fn declaration(&self) -> &Declaration;

    /// Answers every request of the batch.
    fn segment(&self, request: &SegmentRequest, bounds: &Bounds) -> Result<SegmentResponse, Fault>;
}

/// The handle through which an action asks a segmenter. It is bound to one run: it
/// holds that run's cancellation signal and deadline, so an action hands it neither.
#[derive(Clone)]
pub struct SegmentHandle {
    backend: Arc<dyn Segmenter>,
    bounds: Bounds,
}

impl SegmentHandle {
    /// A handle on this segmenter for the run with this cancellation signal.
    pub fn new(backend: Arc<dyn Segmenter>, cancellation: Cancellation) -> SegmentHandle {
        SegmentHandle {
            backend,
            bounds: Bounds::new(cancellation),
        }
    }

    /// The same handle, for a run that must be over by this time. A deadline is only
    /// ever narrowed: the executor sets it, and an action that holds the handle, or a
    /// clone of it, may shorten it; a later time than the one already set changes
    /// nothing.
    pub fn until(mut self, deadline: Instant) -> SegmentHandle {
        self.bounds.until(deadline);
        self
    }

    /// The same handle, counting each request it sends in this tally, which the
    /// executor waits on before it ends the run (ADR 0008, "Nested calls").
    pub(crate) fn counted(mut self, tally: std::sync::Arc<super::Tally>) -> SegmentHandle {
        self.bounds.counted(tally);
        self
    }

    /// What the segmenter behind the handle declares.
    pub fn declaration(&self) -> &Declaration {
        self.backend.declaration()
    }

    /// Sends the batch and hands back the answer. Nothing is sent once the run is
    /// cancelled or its deadline has passed, or when the batch holds a kind of request
    /// the segmenter does not declare. An answer is malformed unless it answers each
    /// request once, with no more instances than were asked for, each the size of the
    /// image. The answer is checked after the backend built it: what a backend may
    /// build before that is its own to bound (`docs/carry-over.md`).
    pub fn segment(&self, request: &SegmentRequest) -> Result<SegmentResponse, Fault> {
        self.bounds.before_sending(
            self.declaration(),
            request.queries.iter().map(Query::feature),
        )?;
        let capability = crate::contract::Capability::Segment;
        self.bounds.send(capability, self.declaration(), || {
            self.held(request, self.backend.segment(request, &self.bounds)?)
        })
    }

    /// The backend's answer, held to the request.
    fn held(
        &self,
        request: &SegmentRequest,
        response: SegmentResponse,
    ) -> Result<SegmentResponse, Fault> {
        let what = "the segmenter's answer";
        if response.answers.len() != request.queries.len() {
            return Err(Unfit::NotAllowed {
                what,
                why: "does not answer each request once",
            }
            .into());
        }
        let expected = (request.image.width(), request.image.height());
        for answer in &response.answers {
            if answer.len() as u64 > u64::from(request.max_instances) {
                return Err(Unfit::NotAllowed {
                    what,
                    why: "holds more instances for a request than were asked for",
                }
                .into());
            }
            if let Some(other) = answer
                .iter()
                .find(|instance| (instance.width, instance.height) != expected)
            {
                return Err(Unfit::WrongSize {
                    what: "the plane of an instance",
                    expected,
                    found: (other.width, other.height),
                }
                .into());
            }
        }
        Ok(response)
    }
}

impl std::fmt::Debug for SegmentHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SegmentHandle({:?})", self.declaration())
    }
}
