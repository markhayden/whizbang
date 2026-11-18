//! Matrices (plan 2C, task E1, item 4): which sets are run under which judges and arms,
//! how many times, and how many runs at once. Ported from squint
//! (`crates/squint-eval/src/matrix.rs` and `arms.rs`, at the commit `docs/sources.toml`
//! pins), reshaped for whizbang: a judge is a backend of the config, named with the model
//! it must be; an arm is what `check` measures first and the tools it offers; and the
//! spend of a matrix is the store's allowance, with a forecast set against it before the
//! first run, not a cap of the matrix's own.
//!
//! A cell is one item of one set under one judge and one arm, once: its repetition is
//! part of it. Each cell is one run of `check`, and an attempt of a cell that was
//! interrupted is followed by another attempt of the same cell.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use whizbang_core::identity::Digest;

/// The most runs a matrix may have at once.
pub const CONCURRENCY_LIMIT: u32 = 8;
/// The most repetitions of a cell.
pub const REPETITIONS_LIMIT: u32 = 10;
/// The most tools an arm names beyond its own.
pub const ARM_TOOLS_LIMIT: usize = 8;

/// A matrix file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Matrix {
    pub matrix: Spec,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub name: String,
    /// Set ids under the evaluation sets' root, `<use-case>/<set>`.
    pub sets: Vec<String>,
    pub judges: Vec<Judge>,
    pub arms: Vec<Arm>,
    pub repetitions: u32,
    /// How many runs are admitted at once, each reserving its own bound.
    #[serde(default = "two")]
    pub concurrency: u32,
}

fn two() -> u32 {
    2
}

/// A judge: the config's backend of that name (`default` for the one that answers a
/// call that names none), which must be of this model.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Judge {
    pub backend: String,
    pub model: String,
}

/// An arm: `C`, nothing measured first and the judge offered `view` and `align`, the
/// latter answered with its pictures alone (squint's control); or `F`, the details of each
/// pair measured first and opening the conversation. Either may name more tools after a
/// `+` each, which the judge is offered too: `C+outline`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arm {
    pub first: bool,
    pub tools: Vec<String>,
}

impl Arm {
    /// The tools the judge is offered, separated by commas.
    pub fn offered(&self) -> String {
        let base: &[&str] = if self.first {
            &["view", "align", "resize"]
        } else {
            &["view", "align"]
        };
        let mut all: Vec<String> = base.iter().map(|tool| (*tool).to_owned()).collect();
        for tool in &self.tools {
            if !all.contains(tool) {
                all.push(tool.clone());
            }
        }
        all.join(",")
    }

    /// `check`'s argument `details`: `first` or `none`.
    pub fn details(&self) -> &'static str {
        if self.first { "first" } else { "none" }
    }

    /// `check`'s argument `projection`: `visual_only` for the control, `full` otherwise.
    pub fn projection(&self) -> &'static str {
        if self.first { "full" } else { "visual_only" }
    }
}

impl std::fmt::Display for Arm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.first { "F" } else { "C" })?;
        for tool in &self.tools {
            write!(f, "+{tool}")?;
        }
        Ok(())
    }
}

impl std::str::FromStr for Arm {
    type Err = String;

    fn from_str(text: &str) -> Result<Arm, String> {
        let mut parts = text.split('+');
        let first = match parts.next() {
            Some("C") => false,
            Some("F") => true,
            _ => return Err(format!("arm {text:?} is not C or F, with +<tool> after it")),
        };
        let tools: Vec<String> = parts.map(str::to_owned).collect();
        let plain = |tool: &String| {
            tool.starts_with(|c: char| c.is_ascii_lowercase())
                && tool.len() <= 64
                && tool
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        };
        if tools.len() > ARM_TOOLS_LIMIT || !tools.iter().all(plain) {
            return Err(format!("arm {text:?} names a tool that is no tool's name"));
        }
        Ok(Arm { first, tools })
    }
}

impl Serialize for Arm {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Arm {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Arm, D::Error> {
        let text = String::deserialize(d)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// One cell: an item of a set under a judge and an arm, at a repetition from 1.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cell {
    pub set: String,
    pub item: String,
    pub backend: String,
    pub model: String,
    pub arm: String,
    pub repetition: u32,
}

impl Matrix {
    /// A matrix file, read strictly and checked.
    pub fn parse(text: &str) -> Result<Matrix, String> {
        let matrix: Matrix = toml::from_str(text).map_err(|e| format!("the matrix: {e}"))?;
        matrix.validate()?;
        Ok(matrix)
    }

    fn validate(&self) -> Result<(), String> {
        let m = &self.matrix;
        let plain = !m.name.is_empty()
            && m.name.len() <= 64
            && (m.name.chars())
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
        if !plain {
            return Err(
                "the matrix's name is 1 to 64 letters, digits, `-`, `_` and `.`".to_owned(),
            );
        }
        if m.sets.is_empty() || m.judges.is_empty() || m.arms.is_empty() {
            return Err("a matrix names at least one set, one judge and one arm".to_owned());
        }
        if !(1..=REPETITIONS_LIMIT).contains(&m.repetitions) {
            return Err(format!("repetitions is 1 to {REPETITIONS_LIMIT}"));
        }
        if !(1..=CONCURRENCY_LIMIT).contains(&m.concurrency) {
            return Err(format!("concurrency is 1 to {CONCURRENCY_LIMIT}"));
        }
        let distinct = |count: usize, set: usize| count == set;
        let sets: BTreeSet<&String> = m.sets.iter().collect();
        let judges: BTreeSet<&Judge> = m.judges.iter().collect();
        let arms: BTreeSet<String> = m.arms.iter().map(Arm::to_string).collect();
        if !distinct(m.sets.len(), sets.len())
            || !distinct(m.judges.len(), judges.len())
            || !distinct(m.arms.len(), arms.len())
        {
            return Err("a matrix names each set, judge and arm once".to_owned());
        }
        Ok(())
    }

    /// The matrix's identity: the SHA-256 of its content in one canonical form, whatever
    /// the file's layout. Its first 16 hex digits name it in each run's cell.
    pub fn identity(&self) -> String {
        let canonical = serde_json::to_string(self).unwrap_or_default();
        Digest::of_bytes(canonical.as_bytes()).hex()
    }

    /// Every cell over the items of each set, given in the matrix's order of sets: sets,
    /// then items, then judges, then arms, then repetitions.
    pub fn cells(&self, items: &[(String, Vec<String>)]) -> Vec<Cell> {
        let m = &self.matrix;
        let mut out = Vec::new();
        for (set, its) in items {
            for item in its {
                for judge in &m.judges {
                    for arm in &m.arms {
                        for repetition in 1..=m.repetitions {
                            out.push(Cell {
                                set: set.clone(),
                                item: item.clone(),
                                backend: judge.backend.clone(),
                                model: judge.model.clone(),
                                arm: arm.to_string(),
                                repetition,
                            });
                        }
                    }
                }
            }
        }
        out
    }
}

/// What one decision of a model under arm C settled at in the study, in millionths of a
/// US dollar: each model's cell of arm C in `docs/baseline/manifest.toml`, its spend over
/// its decisions, rounded up. A test holds this to the manifest.
pub const SETTLED_PER_DECISION: [(&str, u64); 7] = [
    ("claude-haiku-5-5", 3_293),
    ("claude-opus-5-5", 118_637),
    ("claude-sonnet-5-5", 50_029),
    ("gemini-3.8-flash", 22_752),
    ("gemini-4-argon", 29_732),
    ("gpt-6.1-sol", 33_635),
    ("gpt-6.1-terra", 17_233),
];

/// What a matrix is forecast to cost, in millionths of a US dollar: each cell at its
/// model's settled cost per decision ([`SETTLED_PER_DECISION`]), twice that under arm `F`,
/// whose opening details are counted at twice arm C, and the whole doubled. A model with
/// no settled cost is refused: nothing can be forecast of it.
pub fn forecast(cells: &[Cell]) -> Result<u64, String> {
    let mut total: u64 = 0;
    for cell in cells {
        let Some((_, rate)) = SETTLED_PER_DECISION.iter().find(|(m, _)| *m == cell.model) else {
            return Err(format!(
                "no settled cost of a decision is known for the model {:?}, so the matrix \
                 cannot be forecast",
                cell.model
            ));
        };
        let arm = if cell.arm.starts_with('F') { 2 } else { 1 };
        total = total.saturating_add(rate.saturating_mul(arm));
    }
    Ok(total.saturating_mul(2))
}
