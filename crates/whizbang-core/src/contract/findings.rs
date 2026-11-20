//! Findings (ADR 0035, "Findings"): the third kind of result an action hands back
//! beside its artifacts, with the verdict and measurements.
//!
//! A finding is one answer of a judge's that counts against a pass, as data: which
//! form of answer it is, every obligation it cites, by id, how it was answered (its
//! disposition and its confidence), and the place it cited, when it cited one: the
//! image it is on and a box or a polygon in fractions of that image. **No text of a
//! judge's is in it**: the shape has no field that holds one.
//!
//! The document is of the kind [`FINDINGS`], at most [`FINDINGS_MAX_BYTES`] written as
//! JSON, and its shape is fixed here and by [`FINDINGS_SCHEMA`]. It carries the
//! decision of the whole of what was judged, which is never cut, and says whether the
//! list is complete: a list that was cut short in any way says `complete: false` and
//! counts what was left out, and whoever would act on the findings reads that word
//! first.
//!
//! An action hands the document back as JSON ([`Produced::with_findings`]
//! (super::Produced::with_findings)); the executor reads it strictly, and a document
//! that is not of this shape ends the run as an error. What a caller is handed is the
//! document as it was read ([`Findings`]).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::identity::Digest;

use super::record::Decision;

/// The kind and version of a findings document.
pub const FINDINGS: &str = "findings/1";

/// The JSON Schema of a `findings/1` document.
pub const FINDINGS_SCHEMA: &str = include_str!("../../schemas/findings-1.json");

/// The most bytes a findings document takes, written as JSON: 32 KiB.
pub const FINDINGS_MAX_BYTES: usize = 32 * 1024;

/// The most findings one document lists.
pub const FINDINGS_LIMIT: usize = 32;

/// The most places the findings of one document cite together.
pub const FINDING_PLACES_LIMIT: usize = 8;

/// The most obligations one finding cites.
pub const FINDING_OBLIGATIONS_LIMIT: usize = 16;

/// The most points of a place's polygon; the fewest are three.
pub const FINDING_POLYGON_LIMIT: usize = 64;

/// Which form of answer a finding is: an item of the judge's checklist, which is about
/// one obligation, or a separate observation, which cites one or more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Form {
    Item,
    Observation,
}

/// How an obligation was answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Violation,
    Held,
    AllowedChange,
    Hidden,
    CantTell,
    Observed,
}

/// How sure the answer was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Confident,
    Tentative,
}

/// The place a finding cited: the image it is on, by identity, and where on it: a
/// box, as its left, top, right and bottom, or a polygon, as its points, each an x
/// and a y; all in fractions of the image, from 0 to 1. One of the two, never both.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoundAt {
    pub image: Digest,
    #[serde(
        rename = "box",
        default,
        deserialize_with = "not_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub boxed: Option<[f64; 4]>,
    #[serde(
        default,
        deserialize_with = "not_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub polygon: Option<Vec<[f64; 2]>>,
}

/// A field that must be there, and may be null: left out, it is refused, as the
/// schema refuses it.
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(deserializer)
}

/// A field that may be left out, and is never null where it is there.
fn not_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// One finding. `confidence` is null for a disposition that takes none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub form: Form,
    pub obligations: Vec<String>,
    pub disposition: Disposition,
    #[serde(deserialize_with = "present")]
    pub confidence: Option<Confidence>,
    #[serde(deserialize_with = "present")]
    pub place: Option<FoundAt>,
}

/// What was left out of a document that is not complete: findings, and places of
/// findings that were kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeftOut {
    pub findings: u64,
    pub places: u64,
}

/// A findings document, as it was read. See the module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Findings {
    findings: String,
    decision: Decision,
    complete: bool,
    left_out: LeftOut,
    items: Vec<Finding>,
}

/// How many findings cite an obligation with one disposition and confidence: what a
/// run's record keeps of findings (`run/6`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingCount {
    pub obligation: String,
    pub disposition: Disposition,
    #[serde(deserialize_with = "present")]
    pub confidence: Option<Confidence>,
    pub count: u64,
}

/// Whether `id` is an obligation's id: one to eight upper-case ASCII letters, then one
/// to four digits, as `SF1` and `RQ12` are.
pub fn is_obligation_id(id: &str) -> bool {
    let letters = id.bytes().take_while(u8::is_ascii_uppercase).count();
    let digits = &id.as_bytes()[letters..];
    (1..=8).contains(&letters)
        && (1..=4).contains(&digits.len())
        && digits.iter().all(u8::is_ascii_digit)
}

impl FoundAt {
    /// What is wrong with the place, when something is.
    pub(super) fn fault(&self) -> Option<&'static str> {
        let inside = |n: &f64| n.is_finite() && (0.0..=1.0).contains(n);
        match (&self.boxed, &self.polygon) {
            (Some(sides), None) => {
                let [left, top, right, bottom] = sides;
                (!sides.iter().all(inside) || left >= right || top >= bottom).then_some(
                    "a box is not its left, top, right and bottom in fractions of the image, \
                     the left before the right and the top before the bottom",
                )
            }
            (None, Some(points)) => {
                let counted = (3..=FINDING_POLYGON_LIMIT).contains(&points.len());
                (!counted || !points.iter().flatten().all(inside))
                    .then_some("a polygon is not 3 to 64 points in fractions of the image")
            }
            _ => Some("a place is not one box or one polygon"),
        }
    }
}

impl Findings {
    /// Reads a document an action handed back: strictly, and held to every rule of
    /// the kind. The complaint names what is wrong and quotes nothing of the document.
    pub fn read(document: &serde_json::Value) -> Result<Findings, String> {
        let bytes = serde_json::to_vec(document).map_or(usize::MAX, |bytes| bytes.len());
        if bytes > FINDINGS_MAX_BYTES {
            return Err(format!("it is over {FINDINGS_MAX_BYTES} bytes"));
        }
        let read = Findings::deserialize(document)
            .map_err(|_| "it is not of the shape of findings/1".to_owned())?;
        match read.fault() {
            Some(fault) => Err(fault.to_owned()),
            None => Ok(read),
        }
    }

    /// The document of these findings, in their order, under this decision, with
    /// this left out: complete exactly when nothing was. Held to every rule of the
    /// kind, as a document that is read is.
    pub fn of(
        decision: Decision,
        items: Vec<Finding>,
        left_out: LeftOut,
    ) -> Result<Findings, String> {
        let made = Findings {
            findings: FINDINGS.to_owned(),
            decision,
            complete: left_out == LeftOut::default(),
            left_out,
            items,
        };
        Findings::read(&made.to_json())
    }

    fn fault(&self) -> Option<&'static str> {
        if self.findings != FINDINGS {
            return Some("it is not of the kind findings/1");
        }
        if self.items.len() > FINDINGS_LIMIT {
            return Some("it lists more than 32 findings");
        }
        let places = self.items.iter().filter_map(|item| item.place.as_ref());
        if places.clone().count() > FINDING_PLACES_LIMIT {
            return Some("its findings cite more than 8 places");
        }
        if let Some(fault) = { places }.find_map(FoundAt::fault) {
            return Some(fault);
        }
        for item in &self.items {
            let cited = &item.obligations;
            let distinct = cited
                .iter()
                .enumerate()
                .all(|(at, id)| !cited[..at].contains(id));
            if !(1..=FINDING_OBLIGATIONS_LIMIT).contains(&cited.len())
                || !cited.iter().all(|id| is_obligation_id(id))
                || !distinct
            {
                return Some(
                    "a finding does not cite 1 to 16 obligations, each once, by an id of \
                     upper-case letters and then digits",
                );
            }
            if item.form == Form::Item && cited.len() != 1 {
                return Some("an item of the checklist cites more than one obligation");
            }
            // `observed` takes no confidence; every other disposition takes one.
            if (item.disposition == Disposition::Observed) != item.confidence.is_none() {
                return Some("a finding's confidence is null for observed, and for nothing else");
            }
        }
        let whole = self.left_out == LeftOut::default();
        (self.complete != whole)
            .then_some("it is complete and counts something left out, or is not and counts nothing")
    }

    /// The document as JSON, as it was handed back.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }

    /// The decision of the whole of what was judged, worked out before anything was
    /// cut: never one worked out from the list.
    pub fn decision(&self) -> Decision {
        self.decision
    }

    /// Whether the list holds every finding and every place there was. A caller that
    /// would act on the findings reads this first.
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// What was left out.
    pub fn left_out(&self) -> LeftOut {
        self.left_out
    }

    /// The findings, in the order they were answered.
    pub fn items(&self) -> &[Finding] {
        &self.items
    }

    /// How many findings cite each obligation with each disposition and confidence,
    /// in the order of those three. A finding that cites two obligations counts under
    /// each.
    pub fn counts(&self) -> Vec<FindingCount> {
        let mut counts = BTreeMap::new();
        for item in &self.items {
            for obligation in &item.obligations {
                let key = (obligation.clone(), item.disposition, item.confidence);
                *counts.entry(key).or_insert(0u64) += 1;
            }
        }
        let count = |((obligation, disposition, confidence), count)| FindingCount {
            obligation,
            disposition,
            confidence,
            count,
        };
        counts.into_iter().map(count).collect()
    }

    /// The places the findings cite, in the order of the findings.
    pub fn places(&self) -> Vec<FoundAt> {
        let places = self.items.iter().filter_map(|item| item.place.clone());
        places.collect()
    }
}
