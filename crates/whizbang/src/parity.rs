//! When two answers of a segmenter agree (plan 1D, task V1; ADR 0027): the bar a
//! segmenter's answer is held to against the answer a fixture records, here on the Mac's
//! CPU against its GPU, and in milestone 6 a remote worker against the local one
//! (`SPEC.md` section 18, item 3).
//!
//! **Agreement.** Two answers to the same request agree when each query has the same
//! count of instances in both, and, paired in the order each answer gives them, each
//! pair's masks overlap by an IoU of at least [`IOU_TOLERANCE`] and their scores differ
//! by at most [`SCORE_MARGIN`]. An answer one instance short does not agree, however
//! well the rest overlap.
//!
//! **The two numbers are measured, not chosen** (ADR 0027): each parity fixture
//! (`fixtures/parity/segment/`) was asked three times on the GPU and three times on the
//! CPU, of SAM 3 and of Tiny; the tolerance is the worst IoU found, rounded down to 0.01,
//! and the margin the widest difference of scores found, rounded up to 0.01.

/// The least IoU of each pair of masks for two answers to agree: the worst found on the
/// Mac, 0.999945 (Tiny on the CPU), rounded down (ADR 0027).
pub const IOU_TOLERANCE: f64 = 0.99;

/// The most two paired scores may differ by for two answers to agree: the widest found
/// on the Mac, 0.000016 (SAM 3 on the CPU), rounded up (ADR 0027).
pub const SCORE_MARGIN: f64 = 0.01;

/// One instance as agreement reads it: its score, and its plane, one byte a pixel, 0
/// off the thing and anything else on it.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub score: f64,
    pub plane: Vec<u8>,
}

/// How far two answers are apart: the least IoU of a pair, and the widest difference of
/// two paired scores. Two answers with no pair at all are 1 and 0 apart.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Apart {
    pub least_iou: f64,
    pub widest_score: f64,
}

impl Apart {
    /// Whether this is inside the tolerance and the margin.
    pub fn inside(&self) -> bool {
        self.least_iou >= IOU_TOLERANCE && self.widest_score <= SCORE_MARGIN
    }

    /// The worse of the two, field by field.
    pub fn worst(self, other: Apart) -> Apart {
        Apart {
            least_iou: self.least_iou.min(other.least_iou),
            widest_score: self.widest_score.max(other.widest_score),
        }
    }
}

/// The IoU of two planes of the same size: the pixels on in both over the pixels on in
/// either; 1 when neither has a pixel on. Planes of two sizes overlap by 0.
pub fn iou(one: &[u8], other: &[u8]) -> f64 {
    if one.len() != other.len() {
        return 0.0;
    }
    let (mut both, mut either) = (0u64, 0u64);
    for (a, b) in one.iter().zip(other) {
        let (a, b) = (*a != 0, *b != 0);
        both += u64::from(a && b);
        either += u64::from(a || b);
    }
    if either == 0 {
        return 1.0;
    }
    both as f64 / either as f64
}

/// How far two answers are apart, query by query and pair by pair; `None` when a query
/// has another count of instances in one than in the other, or the answers another
/// count of queries.
pub fn apart(one: &[Vec<Found>], other: &[Vec<Found>]) -> Option<Apart> {
    if one.len() != other.len() {
        return None;
    }
    let mut found = Apart {
        least_iou: 1.0,
        widest_score: 0.0,
    };
    for (a, b) in one.iter().zip(other) {
        if a.len() != b.len() {
            return None;
        }
        for (x, y) in a.iter().zip(b) {
            let pair = Apart {
                least_iou: iou(&x.plane, &y.plane),
                widest_score: (x.score - y.score).abs(),
            };
            found = found.worst(pair);
        }
    }
    Some(found)
}

/// Whether two answers agree: the same count of instances for each query, and each pair
/// inside the tolerance and the margin.
pub fn agree(one: &[Vec<Found>], other: &[Vec<Found>]) -> bool {
    apart(one, other).is_some_and(|apart| apart.inside())
}
