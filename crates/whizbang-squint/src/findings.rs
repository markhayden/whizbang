//! The findings `check` hands its caller (plan m4, task K2; ADR 0035, "Findings"): one
//! for every answer of the judge's submission that counts against a pass, in both forms
//! the judge answers in, as the kind of result `findings/1`
//! ([`whizbang_core::contract::Findings`]).
//!
//! **Made from what the run keeps, and from nothing else.** The one function here reads
//! the document a run of `check` keeps of what it decided ([`crate::check::JUDGED`]). A
//! run that was made reads the document it has just written, and a run answered from
//! the decision cache reads the document of the run that answered it, so the two hand
//! back the same findings, by the same code.
//!
//! **What is a finding.** Every answer whose row of the aggregation is not `recorded`,
//! that is, every answer that makes the verdict `fail` or `unsure`
//! ([`crate::use_case::row`]): an item of the checklist, and a separate observation. A
//! finding keeps every obligation its answer cites, its disposition and its confidence,
//! as the submission was read. An item of the checklist is about one obligation, but
//! for the item of a reference image, which is about each obligation that consults the
//! reference: that item is one finding of the form `item` for each of them, in their
//! order, since an item of `findings/1` cites one obligation and none may be dropped.
//!
//! **The decision is of the whole submission.** It is the strictest row of every answer
//! the document holds, worked out before anything is cut, and it is never cut: nothing
//! after `check` works a verdict out from a list that may be short.
//!
//! **A finding is data from a model, and a short list says it is short.** At most
//! [`FINDINGS_LIMIT`] findings are handed back, in the order of the submission, and
//! among them at most [`FINDING_PLACES_LIMIT`] places. A place is a box or a polygon
//! on an image the run was given, each number held inside the picture; a location that
//! is the whole image, or an absence, is no place. Whatever cannot be handed back is
//! left out and counted, and the document then says `complete: false`: a finding past
//! the limit, or one whose obligations `findings/1` does not take (more than
//! [`FINDING_OBLIGATIONS_LIMIT`], none at all, or an id that is not letters and then
//! digits); a
//! place past the limit, on an image the run was not given, or that is no box or
//! polygon of the kind once its numbers are held (a box or a polygon that encloses no
//! area, a polygon of more than [`FINDING_POLYGON_LIMIT`] points); and, last, whatever is cut from the end
//! to fit [`FINDINGS_MAX_BYTES`], places before findings. No text of the judge's
//! travels: the kind has no field for one.

use serde_json::Value as Json;
use whizbang_core::contract::record::Decision;
use whizbang_core::contract::{
    Confidence, Disposition, FINDING_OBLIGATIONS_LIMIT, FINDING_PLACES_LIMIT,
    FINDING_POLYGON_LIMIT, FINDINGS_LIMIT, FINDINGS_MAX_BYTES, Finding, Findings, Form, FoundAt,
    LeftOut, is_obligation_id,
};
use whizbang_core::identity::Digest;

/// What the envelope of a document takes beside its findings, at most: its kind, its
/// decision, whether it is complete and its two counts.
const ENVELOPE_BYTES: usize = 256;

/// What a kept location comes to.
enum Cited {
    /// The answer cited no box and no polygon.
    Nothing,
    /// It cited one that cannot be handed back.
    LeftOut,
    At(FoundAt),
}

fn inside(number: &Json) -> Option<f64> {
    let number = number.as_f64().filter(|number| number.is_finite())?;
    Some(number.clamp(0.0, 1.0))
}

/// Whether a polygon encloses any area: points on one line, or all held to one edge or
/// corner of the picture, enclose none, as a box whose sides meet does.
fn encloses(points: &[[f64; 2]]) -> bool {
    let after = points.iter().cycle().skip(1);
    let doubled: f64 = (points.iter().zip(after))
        .map(|([x, y], [next_x, next_y])| x * next_y - next_x * y)
        .sum();
    doubled.abs() > f64::EPSILON
}

/// The place a kept location cites, on the images the run was given.
fn cited(location: Option<&Json>, images: &[(String, Digest)]) -> Cited {
    let Some(location) = location.filter(|location| !location.is_null()) else {
        return Cited::Nothing;
    };
    let kind = location.get("kind").and_then(Json::as_str);
    if !matches!(kind, Some("bbox" | "polygon")) {
        return Cited::Nothing;
    }
    let frame = location.get("frame").and_then(Json::as_str);
    let named = frame.and_then(|frame| frame.strip_prefix("raw:"));
    let image = named.and_then(|named| images.iter().find(|(id, _)| id == named));
    let Some((_, image)) = image else {
        return Cited::LeftOut;
    };
    let place = if kind == Some("bbox") {
        let sides = location.get("bbox").and_then(Json::as_array);
        let sides: Option<Vec<f64>> = sides.map(|sides| sides.iter().filter_map(inside).collect());
        let boxed = match sides.as_deref() {
            Some([left, top, right, bottom]) if left < right && top < bottom => {
                Some([*left, *top, *right, *bottom])
            }
            _ => None,
        };
        boxed.map(|boxed| FoundAt {
            image: *image,
            boxed: Some(boxed),
            polygon: None,
        })
    } else {
        let points = location.get("polygon").and_then(Json::as_array);
        let point = |point: &Json| match point.as_array().map(Vec::as_slice) {
            Some([x, y]) => Some([inside(x)?, inside(y)?]),
            _ => None,
        };
        let read: Option<Vec<[f64; 2]>> =
            points.and_then(|points| points.iter().map(point).collect());
        read.filter(|points| (3..=FINDING_POLYGON_LIMIT).contains(&points.len()))
            .filter(|points| encloses(points))
            .map(|points| FoundAt {
                image: *image,
                boxed: None,
                polygon: Some(points),
            })
    };
    place.map_or(Cited::LeftOut, Cited::At)
}

/// A word of the kept document as the kind's own.
fn word<T: serde::de::DeserializeOwned>(answer: &Json, key: &str) -> Result<T, String> {
    let read = answer.get(key).cloned().map(serde_json::from_value);
    match read {
        Some(Ok(word)) => Ok(word),
        _ => Err(format!("an answer's {key} is not a word of the kind")),
    }
}

/// A list of obligations' ids, each once, in the order given.
fn ids(list: &Json) -> Result<Vec<String>, String> {
    let list = list
        .as_array()
        .ok_or("an answer's obligations are not a list")?;
    let mut ids: Vec<String> = Vec::new();
    for id in list {
        let id = id.as_str().ok_or("an obligation's id is not text")?;
        if !ids.iter().any(|seen| seen == id) {
            ids.push(id.to_owned());
        }
    }
    Ok(ids)
}

/// The findings of what a run of `check` decided, from the document it keeps
/// ([`crate::check::JUDGED`]); or why the document gives none, in words that quote
/// nothing of it. See the module.
pub fn of_judged(judged: &[u8]) -> Result<Findings, String> {
    let judged: Json =
        serde_json::from_slice(judged).map_err(|_| "what the run decided is not JSON")?;
    let answers = judged.get("answers").and_then(Json::as_array);
    let answers = answers.ok_or("what the run decided holds no list of answers")?;
    let images: Vec<(String, Digest)> = (judged.get("images").and_then(Json::as_array))
        .ok_or("what the run decided holds no list of images")?
        .iter()
        .filter_map(|image| {
            let id = image.get("id")?.as_str()?.to_owned();
            let artifact = image.get("artifact")?.as_str()?.parse().ok()?;
            Some((id, artifact))
        })
        .collect();
    let mut decision = Decision::Pass;
    let mut items: Vec<Finding> = Vec::new();
    let mut left_out = LeftOut::default();
    let mut places = 0;
    for answer in answers {
        // Every answer enters the decision, before any is cut.
        match answer.get("row").and_then(Json::as_str) {
            Some("recorded") => continue,
            Some("unsure") if decision == Decision::Pass => decision = Decision::Unsure,
            Some("unsure") => {}
            Some("fail") => decision = Decision::Fail,
            _ => return Err("an answer's row is not a row of the aggregation".to_owned()),
        }
        let disposition: Disposition = word(answer, "disposition")?;
        let confidence: Confidence = word(answer, "confidence")?;
        // `observed` takes no confidence in the kind.
        let confidence = (disposition != Disposition::Observed).then_some(confidence);
        // An observation is one finding of everything it cites; an item is one of each
        // obligation it is about.
        let (form, cites) = match (answer.get("obligation_ids"), answer.get("obligations")) {
            (Some(cited), _) => (Form::Observation, vec![ids(cited)?]),
            (None, Some(about)) => {
                let each = ids(about)?.into_iter().map(|id| vec![id]);
                (Form::Item, each.collect())
            }
            (None, None) => return Err("an answer cites no obligation".to_owned()),
        };
        // An answer that counts and is about no obligation cannot be handed back.
        if cites.is_empty() {
            left_out.findings += 1;
        }
        let place = cited(answer.get("location"), &images);
        for obligations in cites {
            let taken = (1..=FINDING_OBLIGATIONS_LIMIT).contains(&obligations.len())
                && obligations.iter().all(|id| is_obligation_id(id));
            if items.len() >= FINDINGS_LIMIT || !taken {
                left_out.findings += 1;
                continue;
            }
            let place = match &place {
                Cited::Nothing => None,
                Cited::At(place) if places < FINDING_PLACES_LIMIT => {
                    places += 1;
                    Some(place.clone())
                }
                Cited::At(_) | Cited::LeftOut => {
                    left_out.places += 1;
                    None
                }
            };
            items.push(Finding {
                form,
                obligations,
                disposition,
                confidence,
                place,
            });
        }
    }
    // What is over the size is cut from the end: places first, then findings.
    let written = |items: &[Finding]| serde_json::to_vec(items).map_or(usize::MAX, |b| b.len());
    while written(&items) + ENVELOPE_BYTES > FINDINGS_MAX_BYTES {
        if let Some(last) = items.iter_mut().rev().find(|item| item.place.is_some()) {
            last.place = None;
            left_out.places += 1;
        } else if items.pop().is_some() {
            left_out.findings += 1;
        } else {
            break;
        }
    }
    Findings::of(decision, items, left_out)
}
