//! Blinded pairs (plan m3, task L3, item 3; rascal's `lab/pairs.rs`, its `Draw` and
//! `Assignment::draw`, at the commit `docs/sources.toml` pins): for two arms, one pair for
//! each image the protocol judges, the page's order and the side arm `a` takes drawn from
//! the protocol's seed, so one lab always draws one page. The draw is rascal's: splitmix64
//! from the seed, the order by Fisher–Yates, then one bit a pair, in page order.
//!
//! **A lighter round** (the owner's wish of 2026-11-18): `limit` pairs of the round, drawn
//! by the protocol's groups, each group given its share of the limit by the largest
//! remainder (in the protocol's order on a tie), and its images chosen by a draw from the
//! seed of its own stream ([`SUBSET_STREAM`]); the chosen pairs are then ordered and sided
//! as a whole round's are. The protocol's rules are unchanged: a group with fewer decided
//! pairs than `min_decided` is `inconclusive`, never a win, so a subset too small for a
//! group says so before anything is picked ([`Share::short`]).
//!
//! What was drawn is checked again before a decode ([`check_drawn`]): an assignment whose
//! order, sides, images or groups are not what the seed draws is refused.

use std::collections::BTreeSet;

use super::decode::{Assignment, Pair, Shown, Side, Subset};
use super::protocol::Protocol;

/// A deterministic draw from a seed: splitmix64, as rascal's.
pub struct Draw(u64);

impl Draw {
    pub fn new(seed: u64) -> Draw {
        Draw(seed)
    }

    /// The next 64 bits.
    pub fn bits(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// An index below `n` (`n > 0`).
    pub fn below(&mut self, n: usize) -> usize {
        (self.bits() % n as u64) as usize
    }

    /// Fisher–Yates over `items`.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }
}

/// What the seed is mixed with for a subset's choice of images, so that the order and
/// sides of the pairs are drawn as a whole round's are: the ASCII of `subset`.
pub const SUBSET_STREAM: u64 = 0x7375_6273_6574;

/// One image of a round with the cell of each arm shown for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub image: String,
    pub group: String,
    pub a: Shown,
    pub b: Shown,
}

/// A group's share of a round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Share {
    pub group: String,
    /// Its pairs drawn.
    pub drawn: u32,
    /// Its pairs in the whole round.
    pub of: u32,
}

impl Share {
    /// Whether the group's drawn pairs are fewer than `min_decided`: it is then
    /// `inconclusive` whatever is picked.
    pub fn short(&self, min_decided: u32) -> bool {
        self.drawn < min_decided
    }
}

/// The pairs of a whole round: every image the protocol judges.
fn whole(protocol: &Protocol) -> u32 {
    protocol.images().count() as u32
}

/// Each group's share of `limit` pairs: its pairs times the limit over the round's, rounded
/// down, then one more to the largest remainders, the protocol's order breaking a tie.
/// Refused: a limit of 0, or one that is not below the round's pairs.
pub fn shares(protocol: &Protocol, limit: u32) -> Result<Vec<Share>, String> {
    let total = whole(protocol);
    if limit == 0 || limit >= total {
        return Err(format!(
            "--limit takes 1 to {} pairs, fewer than the round's {total}; leave it out for \
             the whole round",
            total.saturating_sub(1)
        ));
    }
    let mut shares: Vec<(Share, u64)> = (protocol.group.iter())
        .map(|g| {
            let of = g.images.len() as u32;
            let exact = u64::from(limit) * u64::from(of);
            let share = Share {
                group: g.name.clone(),
                drawn: (exact / u64::from(total)) as u32,
                of,
            };
            (share, exact % u64::from(total))
        })
        .collect();
    let given: u32 = shares.iter().map(|(s, _)| s.drawn).sum();
    let mut by_remainder: Vec<usize> = (0..shares.len()).collect();
    by_remainder.sort_by(|x, y| shares[*y].1.cmp(&shares[*x].1).then(x.cmp(y)));
    for at in by_remainder.into_iter().take((limit - given) as usize) {
        shares[at].0.drawn += 1;
    }
    Ok(shares.into_iter().map(|(s, _)| s).collect())
}

/// The images of a round, with their groups, in the protocol's order: every one, or with
/// a limit each group's share, chosen by a draw of the subset's stream.
pub fn chosen(protocol: &Protocol, limit: Option<u32>) -> Result<Vec<(String, String)>, String> {
    let Some(limit) = limit else {
        return Ok((protocol.images())
            .map(|(id, g)| (id.to_owned(), g.to_owned()))
            .collect());
    };
    let shares = shares(protocol, limit)?;
    let mut draw = Draw::new(protocol.seed ^ SUBSET_STREAM);
    let mut kept = BTreeSet::new();
    for (group, share) in protocol.group.iter().zip(&shares) {
        let mut images: Vec<&str> = group.images.iter().map(String::as_str).collect();
        draw.shuffle(&mut images);
        kept.extend(images.into_iter().take(share.drawn as usize));
    }
    Ok((protocol.images())
        .filter(|(id, _)| kept.contains(id))
        .map(|(id, g)| (id.to_owned(), g.to_owned()))
        .collect())
}

/// The page's order of `n` pairs, and for each whether arm `a` is on the left.
fn order_and_sides(seed: u64, n: usize) -> Vec<(usize, bool)> {
    let mut draw = Draw::new(seed);
    let mut order: Vec<usize> = (0..n).collect();
    draw.shuffle(&mut order);
    order
        .into_iter()
        .map(|at| (at, draw.bits() & 1 == 0))
        .collect()
}

/// The assignment of `candidates` (those [`chosen`] gives, in its order) under the
/// protocol: the order shuffled and each pair's sides drawn from the protocol's seed.
pub fn draw(
    protocol: &Protocol,
    protocol_digest: &str,
    candidates: &[Candidate],
    subset: Option<Subset>,
) -> Assignment {
    let pair = order_and_sides(protocol.seed, candidates.len())
        .into_iter()
        .map(|(at, a_left)| {
            let c = &candidates[at];
            let (left, right, a_side) = if a_left {
                (c.a.clone(), c.b.clone(), Side::Left)
            } else {
                (c.b.clone(), c.a.clone(), Side::Right)
            };
            Pair {
                id: c.image.clone(),
                group: c.group.clone(),
                left,
                right,
                a_side,
            }
        })
        .collect();
    Assignment {
        assignment_version: 1,
        lab: protocol.name.clone(),
        protocol_digest: protocol_digest.to_owned(),
        arms: protocol.arms.clone(),
        seed: protocol.seed,
        subset,
        pair,
    }
}

/// Refused when the assignment's images, groups, order or sides are not what the
/// protocol's seed draws, for the whole round or for the subset it says it is.
pub fn check_drawn(protocol: &Protocol, assignment: &Assignment) -> Result<(), String> {
    let refuse = || {
        Err(
            "the assignment is not the one the protocol's seed draws (its images, order or \
             sides were changed); nothing was decoded"
                .to_owned(),
        )
    };
    if assignment.seed != protocol.seed {
        return refuse();
    }
    let limit = match assignment.subset {
        Some(Subset { limit, of }) if of == whole(protocol) => Some(limit),
        Some(_) => return refuse(),
        None => None,
    };
    let chosen = chosen(protocol, limit)?;
    let drawn = order_and_sides(protocol.seed, chosen.len());
    if drawn.len() != assignment.pair.len() {
        return refuse();
    }
    for ((at, a_left), pair) in drawn.into_iter().zip(&assignment.pair) {
        let (id, group) = &chosen[at];
        let side = if a_left { Side::Left } else { Side::Right };
        if pair.id != *id || pair.group != *group || pair.a_side != side {
            return refuse();
        }
    }
    Ok(())
}
