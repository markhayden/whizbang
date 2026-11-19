//! The decode of a round's picks (plan m3, task L2, item 2; rascal's `lab/pairs.rs`, its
//! `decode`, at the commit `docs/sources.toml` pins): each group of the protocol decoded
//! from the picks made on a frozen assignment of blinded pairs. A pair is `decided` when
//! its pick is a side, a win when that side is arm `a`'s, a loss when it is `b`'s; `same`
//! and `neither` count toward neither side. A group is `inconclusive` below the
//! protocol's `min_decided` decided pairs, else `preferred` when wins over decided pairs
//! reach the protocol's fraction (inclusive), else `not_worse` when losses over decided
//! pairs stay below its fraction, else `worse`.
//!
//! The assignment is L3's to write (`whizbang lab pairs`); the picks are the owner's, and
//! what is read of them is said only as counts: a refusal names a pair by its id, which is
//! a corpus id of the committed manifest, and never quotes what the file holds.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::protocol::Protocol;

/// The frozen assignment of a round's pairs: what the decode needs and the page must not
/// show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub assignment_version: u32,
    pub lab: String,
    pub protocol_digest: String,
    /// The two arms compared, `[a, b]`: wins are a's.
    pub arms: [String; 2],
    pub seed: u64,
    /// In page order.
    pub pair: Vec<Pair>,
}

/// One pair: an image, the two cells shown, and the side arm `a` took.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pair {
    /// The pair's id: the image's corpus id.
    pub id: String,
    pub group: String,
    pub left: Shown,
    pub right: Shown,
    /// `left` or `right`: the side arm `a` is on.
    pub a_side: Side,
}

/// A side of the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Left,
    Right,
}

/// A cell's attempt as one side shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shown {
    pub cell: String,
    pub attempt: u32,
}

/// The picks, as the page writes them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Picks {
    pub picks_version: u32,
    /// `sha256:<hex>` of the assignment file the picks were made on.
    pub assignment_digest: String,
    pub pick: Vec<Pick>,
}

/// One pick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pick {
    pub pair: String,
    pub pick: Picked,
}

/// What the owner picked for a pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Picked {
    Left,
    Right,
    Same,
    Neither,
}

/// A group's outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Outcome {
    Preferred,
    NotWorse,
    Worse,
    Inconclusive,
}

impl Outcome {
    pub fn word(self) -> &'static str {
        match self {
            Outcome::Preferred => "preferred",
            Outcome::NotWorse => "not_worse",
            Outcome::Worse => "worse",
            Outcome::Inconclusive => "inconclusive",
        }
    }
}

/// One group's decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub group: String,
    pub pairs: u32,
    pub decided: u32,
    pub wins: u32,
    pub losses: u32,
    pub same: u32,
    pub neither: u32,
    pub outcome: Outcome,
}

/// The decode of a picks file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decode {
    pub arms: [String; 2],
    pub groups: Vec<Group>,
}

/// A JSON file of the lab read as its form, in fixed words and the place when the reader
/// says one: what the file holds is never quoted.
fn read_json<T: serde::de::DeserializeOwned>(what: &str, bytes: &[u8]) -> Result<T, String> {
    serde_json::from_slice(bytes).map_err(|e| {
        let place = match (e.line(), e.column()) {
            (0, _) => String::new(),
            (line, column) => format!(", at line {line}, column {column}"),
        };
        format!("{what} is not of the form this build reads{place}")
    })
}

impl Assignment {
    pub fn parse(bytes: &[u8]) -> Result<Assignment, String> {
        let read: Assignment = read_json("the assignment", bytes)?;
        if read.assignment_version != 1 {
            return Err("the assignment: its version is not 1".to_owned());
        }
        Ok(read)
    }
}

impl Picks {
    pub fn parse(bytes: &[u8]) -> Result<Picks, String> {
        read_json("the picks", bytes)
    }
}

/// The outcome of a group's counts under the protocol's rules.
pub fn outcome(protocol: &Protocol, decided: u32, wins: u32, losses: u32) -> Outcome {
    let [pn, pd] = protocol.rules.preferred;
    let [nn, nd] = protocol.rules.not_worse;
    let (decided, wins, losses) = (u64::from(decided), u64::from(wins), u64::from(losses));
    if decided < u64::from(protocol.min_decided) {
        Outcome::Inconclusive
    } else if wins * u64::from(pd) >= u64::from(pn) * decided {
        Outcome::Preferred
    } else if losses * u64::from(nd) < u64::from(nn) * decided {
        Outcome::NotWorse
    } else {
        Outcome::Worse
    }
}

/// Decodes `picks` made on `assignment`, whose file's digest is `assignment_digest`, under
/// the protocol whose digest is `protocol_digest`. Refused: picks of another version, made
/// on another assignment, or with a pair missing, picked twice or not in the assignment;
/// and an assignment of another lab or protocol, or naming arms the protocol does not
/// compare in its order.
pub fn decode(
    protocol: &Protocol,
    protocol_digest: &str,
    assignment: &Assignment,
    assignment_digest: &str,
    picks: &Picks,
) -> Result<Decode, String> {
    if assignment.lab != protocol.name
        || assignment.protocol_digest != protocol_digest
        || assignment.arms != protocol.arms
    {
        return Err(
            "the assignment is not of this lab, its protocol and its two arms; nothing was \
             decoded"
                .to_owned(),
        );
    }
    if picks.picks_version != 1 {
        return Err("the picks: their version is not 1".to_owned());
    }
    if picks.assignment_digest != assignment_digest {
        return Err(
            "the picks were made on another assignment than this one: stale or foreign picks; \
             nothing was decoded"
                .to_owned(),
        );
    }
    let pairs: BTreeMap<&str, &Pair> = (assignment.pair.iter())
        .map(|p| (p.id.as_str(), p))
        .collect();
    let mut by_pair: BTreeMap<&str, Picked> = BTreeMap::new();
    for (at, pick) in picks.pick.iter().enumerate() {
        let Some((id, _)) = pairs.get_key_value(pick.pair.as_str()) else {
            return Err(format!(
                "pick {} of the list names a pair that is not in the assignment",
                at + 1
            ));
        };
        if by_pair.insert(id, pick.pick).is_some() {
            return Err(format!("pair {id} is picked twice"));
        }
    }
    let missing: BTreeSet<&&str> = pairs
        .keys()
        .filter(|id| !by_pair.contains_key(**id))
        .collect();
    if let Some(id) = missing.first() {
        return Err(format!("pair {id} has no pick"));
    }
    let mut groups = Vec::new();
    // An assignment drawn for one group holds that group's pairs only; the others are not
    // reported.
    for group in
        (protocol.group.iter()).filter(|g| assignment.pair.iter().any(|p| p.group == g.name))
    {
        let mut counts = [0_u32; 4];
        let (mut wins, mut losses) = (0, 0);
        for pair in assignment.pair.iter().filter(|p| p.group == group.name) {
            counts[0] += 1;
            let side = match by_pair[pair.id.as_str()] {
                Picked::Same => {
                    counts[2] += 1;
                    continue;
                }
                Picked::Neither => {
                    counts[3] += 1;
                    continue;
                }
                Picked::Left => Side::Left,
                Picked::Right => Side::Right,
            };
            counts[1] += 1;
            if side == pair.a_side {
                wins += 1;
            } else {
                losses += 1;
            }
        }
        let [pairs, decided, same, neither] = counts;
        groups.push(Group {
            group: group.name.clone(),
            pairs,
            decided,
            wins,
            losses,
            same,
            neither,
            outcome: outcome(protocol, decided, wins, losses),
        });
    }
    Ok(Decode {
        arms: assignment.arms.clone(),
        groups,
    })
}

impl Decode {
    /// The decode as `whizbang lab decode` prints it: counts and an outcome a group.
    pub fn text(&self) -> String {
        let mut out = format!("arms: {} against {}\n", self.arms[0], self.arms[1]);
        for g in &self.groups {
            out.push_str(&format!(
                "group {}: pairs {}, decided {}, wins {}, losses {}, same {}, neither {}: {}\n",
                g.group,
                g.pairs,
                g.decided,
                g.wins,
                g.losses,
                g.same,
                g.neither,
                g.outcome.word()
            ));
        }
        out
    }
}
