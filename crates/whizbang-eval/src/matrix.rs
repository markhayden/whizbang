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
///
/// Beside its tools, an arm `F` has two settings (plan m3, task U1, item 2): `details`, the
/// details measured first, each for the use cases it is named for, where `align` alone is
/// measured otherwise; and `second_look`, `on` or `off`. Both are part of the arm's name, so
/// of each cell's identity and of the matrix's: `F;details=tiles@reality-checks;second_look=on`.
/// A matrix file may write an arm as that name, or as a table:
/// `{ arm = "F", details = [{ detail = "tiles", use_cases = ["reality-checks"] }], second_look = "on" }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arm {
    pub first: bool,
    pub tools: Vec<String>,
    pub details: Vec<Detail>,
    pub second_look: bool,
}

/// A detail an arm measures first, and the use cases it is measured for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Detail {
    pub detail: String,
    pub use_cases: Vec<String>,
}

/// The details an arm may name: `align` and the two that compare a pair in one grid, each
/// for a use case that relates a pair; and `tiles`, for a use case with one image. The
/// binary's tests hold this to `whizbang_squint::details::takes`.
pub const DETAILS_TAKEN: [(&str, &[&str]); 4] = [
    (
        "align",
        &[
            "source-fidelity",
            "reference-consistency",
            "product-fidelity",
            "brief-compliance",
        ],
    ),
    (
        "change_map",
        &[
            "source-fidelity",
            "reference-consistency",
            "product-fidelity",
            "brief-compliance",
        ],
    ),
    (
        "edge_similarity",
        &[
            "source-fidelity",
            "reference-consistency",
            "product-fidelity",
            "brief-compliance",
        ],
    ),
    ("tiles", &["reality-checks", "brief-compliance"]),
];

/// Whether a detail can be measured first under a use case.
pub fn takes(detail: &str, use_case: &str) -> bool {
    DETAILS_TAKEN
        .iter()
        .any(|(named, cases)| *named == detail && cases.contains(&use_case))
}

/// The most details an arm names.
pub const ARM_DETAILS_LIMIT: usize = 8;

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

    /// `check`'s argument `detail` under a use case: the details the arm names for it,
    /// separated by commas; none when it names none, and `check` then measures `align`.
    pub fn detail_for(&self, use_case: &str) -> Option<String> {
        let named: Vec<&str> = (self.details.iter())
            .filter(|d| d.use_cases.iter().any(|u| u == use_case))
            .map(|d| d.detail.as_str())
            .collect();
        (!named.is_empty()).then(|| named.join(","))
    }

    /// `check`'s argument `second_look`: `on` or `off`.
    pub fn second_look(&self) -> &'static str {
        if self.second_look { "on" } else { "off" }
    }

    /// `check`'s argument `projection`: `visual_only` for the control, `full` otherwise.
    pub fn projection(&self) -> &'static str {
        if self.first { "full" } else { "visual_only" }
    }

    fn validate(&self) -> Result<(), String> {
        if !self.first && (!self.details.is_empty() || self.second_look) {
            return Err(format!(
                "arm {self}: details and the second look are settings of an arm F; C is the \
                 control"
            ));
        }
        if self.details.len() > ARM_DETAILS_LIMIT {
            return Err(format!("arm {self}: at most {ARM_DETAILS_LIMIT} details"));
        }
        let mut seen = BTreeSet::new();
        for detail in &self.details {
            if !DETAILS_TAKEN
                .iter()
                .any(|(named, _)| *named == detail.detail)
            {
                return Err(format!(
                    "arm {self}: {:?} is no detail measured first: align, change_map, \
                     edge_similarity or tiles",
                    detail.detail
                ));
            }
            if detail.use_cases.is_empty() || !seen.insert(detail.detail.as_str()) {
                return Err(format!(
                    "arm {self}: each detail is named once, for at least one use case"
                ));
            }
            for use_case in &detail.use_cases {
                if !takes(&detail.detail, use_case) {
                    let shown: String = use_case.chars().take(64).collect();
                    return Err(format!(
                        "arm {self}: the detail {} cannot be measured for the use case \
                         {shown:?}",
                        detail.detail
                    ));
                }
            }
        }
        Ok(())
    }
}

impl std::fmt::Display for Arm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.first { "F" } else { "C" })?;
        for tool in &self.tools {
            write!(f, "+{tool}")?;
        }
        if !self.details.is_empty() {
            let named: Vec<String> = (self.details.iter())
                .map(|d| format!("{}@{}", d.detail, d.use_cases.join(",")))
                .collect();
            write!(f, ";details={}", named.join("|"))?;
        }
        if self.second_look {
            f.write_str(";second_look=on")?;
        }
        Ok(())
    }
}

/// A name of a tool, a detail or a use case: a lower-case letter, then lower-case
/// letters, digits, `_` and `-`, at most 64.
fn plain(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_lowercase())
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

impl std::str::FromStr for Arm {
    type Err = String;

    fn from_str(text: &str) -> Result<Arm, String> {
        let mut settings = text.split(';');
        let base = settings.next().unwrap_or_default();
        let mut parts = base.split('+');
        let first = match parts.next() {
            Some("C") => false,
            Some("F") => true,
            _ => return Err(format!("arm {text:?} is not C or F, with +<tool> after it")),
        };
        let tools: Vec<String> = parts.map(str::to_owned).collect();
        if tools.len() > ARM_TOOLS_LIMIT || !tools.iter().all(|tool| plain(tool)) {
            return Err(format!("arm {text:?} names a tool that is no tool's name"));
        }
        let mut arm = Arm {
            first,
            tools,
            details: Vec::new(),
            second_look: false,
        };
        let mut said = BTreeSet::new();
        for setting in settings {
            let (name, value) = setting.split_once('=').unwrap_or((setting, ""));
            if !said.insert(name) {
                return Err(format!("arm {text:?} gives {name} twice"));
            }
            match name {
                "details" => {
                    for one in value.split('|') {
                        let (detail, cases) = one.split_once('@').unwrap_or((one, ""));
                        let use_cases: Vec<String> = (cases.split(','))
                            .filter(|c| !c.is_empty())
                            .map(str::to_owned)
                            .collect();
                        if !plain(detail) || !use_cases.iter().all(|c| plain(c)) {
                            return Err(format!(
                                "arm {text:?}: details is <detail>@<use case>,..., separated by |"
                            ));
                        }
                        arm.details.push(Detail {
                            detail: detail.to_owned(),
                            use_cases,
                        });
                    }
                }
                "second_look" => match value {
                    "on" => arm.second_look = true,
                    "off" => {}
                    _ => return Err(format!("arm {text:?}: second_look is on or off")),
                },
                _ => {
                    return Err(format!(
                        "arm {text:?}: its settings are details and second_look"
                    ));
                }
            }
        }
        arm.validate()?;
        Ok(arm)
    }
}

impl Serialize for Arm {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

/// An arm as a matrix file writes it: its name, or a table of its base and its settings.
#[derive(Deserialize)]
#[serde(untagged)]
enum Written {
    Name(String),
    Table(Table),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    arm: String,
    #[serde(default)]
    details: Vec<Detail>,
    #[serde(default)]
    second_look: Option<String>,
}

impl<'de> Deserialize<'de> for Arm {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Arm, D::Error> {
        let read = |text: &str| text.parse::<Arm>().map_err(serde::de::Error::custom);
        match Written::deserialize(d)? {
            Written::Name(text) => read(&text),
            Written::Table(table) => {
                if table.arm.contains(';') {
                    return Err(serde::de::Error::custom(
                        "a table's arm is C or F with its tools; its settings are its keys",
                    ));
                }
                let mut arm = read(&table.arm)?;
                arm.details = table.details;
                arm.second_look = match table.second_look.as_deref() {
                    None | Some("off") => false,
                    Some("on") => true,
                    Some(_) => {
                        return Err(serde::de::Error::custom("second_look is on or off"));
                    }
                };
                arm.validate().map_err(serde::de::Error::custom)?;
                // What the cell will be named must read back as this arm.
                read(&arm.to_string())
            }
        }
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
/// whose opening details are counted at twice arm C, three times under an arm with the
/// second look, and the whole doubled. A model with
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
        // Arm F's details are counted at twice arm C, and a second look at three times
        // (plan m3, "Spend").
        let arm = match cell.arm.parse::<Arm>() {
            Ok(arm) if arm.second_look => 3,
            Ok(arm) if arm.first => 2,
            Ok(_) => 1,
            Err(_) if cell.arm.starts_with('F') => 2,
            Err(_) => 1,
        };
        total = total.saturating_add(rate.saturating_mul(arm));
    }
    Ok(total.saturating_mul(2))
}
