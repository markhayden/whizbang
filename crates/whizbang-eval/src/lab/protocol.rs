//! The protocol, `protocol.toml` (rascal's `lab/protocol.rs`): the lab's name, the seed the
//! pairs are drawn from, the two arms compared, `min_decided` and the rules, and the groups
//! of corpus pictures judged together. It is committed before a round runs; the digest of
//! its bytes is in every cell's identity and in the scope of every run of the round, and a
//! protocol changed after a run is refused: a change is a new lab, with a new name.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use whizbang_core::identity::Digest;

use super::{digest_of, is_name};

/// `protocol.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Protocol {
    /// 1, rascal's rules; or 2 (plan mE, task P0, item 3), where arm `a` is the recipe and
    /// `b` the reference, a `neither` is a loss of the recipe, and `not_worse` is read over
    /// every pair picked ([`super::decode::outcome`]).
    pub version: u32,
    /// The lab's name; a protocol changed after a run is a new name.
    pub name: String,
    /// The seed the pairs' order and sides are drawn from; the matrix repeats it.
    pub seed: u64,
    /// The two arms compared, by the matrix's names: `[a, b]`; wins are a's. The matrix
    /// runs these two arms and no other.
    pub arms: [String; 2],
    /// Fewer decided pairs than this in a group is `inconclusive`; under version 2, fewer
    /// pairs picked.
    #[serde(default = "ten")]
    pub min_decided: u32,
    #[serde(default)]
    pub rules: Rules,
    #[serde(default)]
    pub group: Vec<Group>,
}

/// A group of corpus pictures judged together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub name: String,
    /// Corpus ids.
    pub images: Vec<String>,
}

/// The outcome rules, as fractions of the decided pairs (`[numerator, denominator]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    /// `preferred` when wins over decided pairs is at least this.
    pub preferred: [u32; 2],
    /// `not_worse` when losses over decided pairs is below this.
    pub not_worse: [u32; 2],
}

impl Default for Rules {
    fn default() -> Self {
        Rules {
            preferred: [2, 3],
            not_worse: [1, 3],
        }
    }
}

fn ten() -> u32 {
    10
}

impl Protocol {
    /// A protocol's bytes, read strictly, with their digest: version 1 or 2, a plain name, two
    /// distinct arms, `min_decided` of at least 1, fractions between 0 and 1, groups with
    /// unique names, none empty, and a picture in one group at most.
    pub fn parse(bytes: &[u8]) -> Result<(Protocol, String), String> {
        let text = std::str::from_utf8(bytes).map_err(|_| "the protocol is not UTF-8")?;
        let protocol: Protocol =
            toml::from_str(text).map_err(|e| super::unreadable("the protocol", text, &e))?;
        let refuse = |why: String| Err(format!("the protocol: {why}"));
        if !matches!(protocol.version, 1 | 2) {
            return refuse(format!("version {} is not 1 or 2", protocol.version));
        }
        if !is_name(&protocol.name) {
            return refuse("its name is no plain name".to_owned());
        }
        let [a, b] = &protocol.arms;
        if a == b || !is_name(a) || !is_name(b) {
            return refuse("arms are two distinct plain names".to_owned());
        }
        if protocol.min_decided == 0 {
            return refuse("min_decided is at least 1".to_owned());
        }
        for (name, [n, d]) in [
            ("preferred", protocol.rules.preferred),
            ("not_worse", protocol.rules.not_worse),
        ] {
            if d == 0 || n > d {
                return refuse(format!(
                    "rules.{name} = [{n}, {d}] is not a fraction between 0 and 1"
                ));
            }
        }
        if protocol.group.is_empty() {
            return refuse("at least one group is required".to_owned());
        }
        let mut names = BTreeSet::new();
        let mut images = BTreeSet::new();
        for group in &protocol.group {
            if !is_name(&group.name) || !names.insert(group.name.as_str()) {
                return refuse("each group has a plain name of its own".to_owned());
            }
            if group.images.is_empty() {
                return refuse(format!("group {} has no images", group.name));
            }
            for id in &group.images {
                if !images.insert(id.as_str()) {
                    return refuse(format!(
                        "an image is in more than one group (group {})",
                        group.name
                    ));
                }
            }
        }
        Ok((protocol, digest_of(bytes)))
    }

    /// Every corpus id the groups name, with its group.
    pub fn images(&self) -> impl Iterator<Item = (&str, &str)> {
        (self.group.iter()).flat_map(|g| {
            g.images
                .iter()
                .map(move |id| (id.as_str(), g.name.as_str()))
        })
    }
}

/// The scope of a lab's runs (`whizbang::harness::request_id`): `lab-`, the first 12 hex
/// digits of the SHA-256 of the lab's name, `-`, the first 16 of the protocol's digest.
pub fn scope(name: &str, protocol_digest: &str) -> String {
    let named: String = Digest::of_bytes(name.as_bytes())
        .hex()
        .chars()
        .take(12)
        .collect();
    let digest = protocol_digest
        .strip_prefix("sha256:")
        .unwrap_or(protocol_digest);
    let frozen: String = digest.chars().take(16).collect();
    format!("lab-{named}-{frozen}")
}

/// The protocol frozen: refused when any scope of the runs a store holds is of this lab
/// under another protocol, whose digest was the protocol's when it ran.
pub fn frozen<'a>(
    scopes: impl IntoIterator<Item = &'a str>,
    name: &str,
    protocol_digest: &str,
) -> Result<(), String> {
    let ours = scope(name, protocol_digest);
    let lab = &ours[..ours.len() - 16];
    for seen in scopes {
        if seen.starts_with(lab) && seen != ours {
            return Err(format!(
                "the lab {name} ran under another protocol (its runs' scope is {seen}, this \
                 protocol's is {ours}): a protocol changed after a run is a new lab, with a \
                 new name; nothing was run"
            ));
        }
    }
    Ok(())
}
