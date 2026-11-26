//! The checklist's evidence items (plan mE, task T1, item 4): the regions a tool returns,
//! taken into the checklist as items the judge must answer, each bound to the mandatory
//! instances its kind of evidence bears on. Ported from squint's
//! `crates/squint-core/src/checklist.rs` (`ingest`, `iou`, `OVERLAP_IOU`) at the commit
//! `docs/sources.toml` pins; the static items are `crate::use_case::Checklist::build`'s.
//!
//! A region binds to every mandatory instance whose target is the image the region is on
//! and whose evidence kinds hold the tool's (`use_case::Instance::evidence_kinds`); one that
//! binds to none is dropped and reported, never shown. A result that would take the
//! checklist past its budget of evidence items is refused whole before anything of it is
//! shown. Two evidence items on one image whose boxes overlap past [`OVERLAP_IOU`] name
//! each other.
//!
//! What changed from squint. A region is a box or a polygon in pixels of the image the run
//! handed the tool, by its binding (ADR 0032, "Amended: regions"), which the caller names
//! the image of; squint mapped a region through its frame registry to its root frame first,
//! and compared boxes there. Here the overlap of two items is read on one image in its own
//! pixels, and a polygon by its bounds, as squint's was.

use crate::use_case::{Checklist, Instance, Item, Resolved};

/// The IoU above which two evidence items on one image name each other.
pub const OVERLAP_IOU: f64 = 0.7;

/// A region a tool returned, as `check` hands it here: the image it is on (by its id in the
/// job), its bounds in that image's pixels (a polygon's, for a polygon), what is shown of
/// where it is, its label and its score.
#[derive(Debug, Clone, PartialEq)]
pub struct Located {
    pub image: String,
    pub bounds: [f64; 4],
    pub shown: String,
    pub label: String,
    pub score: f64,
}

/// A region the checklist did not take, by its place in the result, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dropped {
    pub region: usize,
    pub reason: String,
}

/// What taking one result did: the items it added, and the regions it dropped.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ingest {
    pub added: Vec<String>,
    pub dropped: Vec<Dropped>,
}

/// The evidence items taken so far, beside the checklist: each item's id, image and bounds,
/// which later items' overlaps are read against.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Evidence {
    taken: Vec<(String, String, [f64; 4])>,
}

impl Evidence {
    /// How many evidence items the checklist holds.
    pub fn count(&self) -> usize {
        self.taken.len()
    }
}

/// Takes the regions of one tool call into the checklist, in their order, each as the item
/// `chk:ev:<call>:<n>`: the regions of `kind` that bind to a mandatory instance on their
/// image. Refused, with nothing taken, when the items would pass `budget`.
pub fn ingest(
    checklist: &mut Checklist,
    evidence: &mut Evidence,
    call: &str,
    kind: &str,
    regions: &[Located],
    resolved: &Resolved,
    budget: usize,
) -> Result<Ingest, String> {
    let mut out = Ingest::default();
    let mut bound: Vec<(&Located, Vec<String>)> = Vec::new();
    for (n, region) in regions.iter().enumerate() {
        let instances: Vec<String> = (resolved.instances.iter())
            .filter(|i| i.target == region.image && i.evidence_kinds.iter().any(|k| k == kind))
            .map(Instance::id)
            .collect();
        if instances.is_empty() {
            out.dropped.push(Dropped {
                region: n,
                reason: format!(
                    "no mandatory instance on {:?} accepts evidence kind {kind:?}",
                    region.image
                ),
            });
            continue;
        }
        bound.push((region, instances));
    }
    if evidence.count() + bound.len() > budget {
        return Err(format!(
            "evidence items would reach {} of a budget of {budget}; the result of {call} is not presented",
            evidence.count() + bound.len()
        ));
    }
    for (n, (region, instances)) in bound.into_iter().enumerate() {
        let id = format!("chk:ev:{}:{n}", call.trim_start_matches("call:"));
        let overlaps: Vec<String> = (evidence.taken.iter())
            .filter(|(_, image, bounds)| {
                *image == region.image && iou(bounds, &region.bounds) > OVERLAP_IOU
            })
            .map(|(other, _, _)| other.clone())
            .collect();
        for other in &overlaps {
            if let Some(item) = checklist.items.iter_mut().find(|item| &item.id == other) {
                item.overlaps.push(id.clone());
            }
        }
        evidence
            .taken
            .push((id.clone(), region.image.clone(), region.bounds));
        out.added.push(id.clone());
        checklist.items.push(Item {
            id,
            instances,
            label: Some(region.label.clone()),
            location: Some(region.shown.clone()),
            overlaps,
        });
    }
    Ok(out)
}

/// The intersection over union of two boxes, `[left, top, right, bottom]`.
pub fn iou(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let area = |r: &[f64; 4]| (r[2] - r[0]).max(0.0) * (r[3] - r[1]).max(0.0);
    let ix = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0);
    let iy = (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    let inter = ix * iy;
    let union = area(a) + area(b) - inter;
    if union <= 0.0 { 0.0 } else { inter / union }
}
