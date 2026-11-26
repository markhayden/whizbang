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
//!
//! **Only the cells a matrix names** (plan mE, task M1, item 1). A judge may list the arms
//! it runs, each with its own repetitions where they are not the judge's; a set's table
//! (`[matrix.set."<id>"]`) may list the arms and the judges it runs. A cell outside any of
//! these lists is not a cell: it is not forecast, run, resumed or counted. A matrix without
//! them names every cell, and its identity is the one it had before the lists.
//!
//! **Gates frozen on it** (plan mE, task M1, item 3): `[gates]` names each gate file frozen
//! on the matrix, beside it, by the SHA-256 of its bytes. The table is in the matrix's
//! identity, which every run holds, so a gate named there was frozen before the first cell.

use std::collections::{BTreeMap, BTreeSet};

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
    /// Every hypothesis the round's correction counts, where the matrix states it (plan mE,
    /// task M1, item 6): its gates' comparisons and the judge's rule's beside them. Left
    /// out of the identity when not given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hypotheses: Option<usize>,
    pub matrix: Spec,
    /// Each gate file frozen on the matrix, by its path beside the matrix, with the SHA-256
    /// of its bytes. Left out of the identity when empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub gates: BTreeMap<String, String>,
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
    /// The arms and judges a set runs, by its id, where it does not run every one. Left out
    /// of the identity when empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub set: BTreeMap<String, SetRuns>,
}

/// What one set runs: the arms, by name, and the judges, by backend; every one of either
/// when it lists none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetRuns {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arms: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub judges: Option<Vec<String>>,
}

/// An arm a judge runs, by its name, with its own repetitions where they are not the
/// judge's. Written as the name alone, or as `{ arm = "<name>", repetitions = <n> }`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct JudgeArm {
    pub arm: String,
    pub repetitions: Option<u32>,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum JudgeArmWritten {
    Name(String),
    Table(JudgeArmTable),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JudgeArmTable {
    arm: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    repetitions: Option<u32>,
}

impl Serialize for JudgeArm {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.repetitions {
            None => JudgeArmWritten::Name(self.arm.clone()),
            Some(repetitions) => JudgeArmWritten::Table(JudgeArmTable {
                arm: self.arm.clone(),
                repetitions: Some(repetitions),
            }),
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for JudgeArm {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<JudgeArm, D::Error> {
        Ok(match JudgeArmWritten::deserialize(d)? {
            JudgeArmWritten::Name(arm) => JudgeArm {
                arm,
                repetitions: None,
            },
            // A table without repetitions is the arm's name alone, and the same identity.
            JudgeArmWritten::Table(table) => JudgeArm {
                arm: table.arm,
                repetitions: table.repetitions,
            },
        })
    }
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
    /// This judge's repetitions, where they are not the matrix's (plan m3, task G1: a
    /// gate's judges are repeated as often as each needs). Left out of the identity when
    /// not given, so a matrix that gives none keeps its identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repetitions: Option<u32>,
    /// The arms this judge runs, where it does not run every arm of the matrix (plan mE,
    /// task M1, item 1). Left out of the identity when not given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arms: Option<Vec<JudgeArm>>,
}

impl Judge {
    /// How many times each cell of this judge is run, where its arms give none of their own.
    pub fn repeated(&self, matrix: &Spec) -> u32 {
        self.repetitions.unwrap_or(matrix.repetitions)
    }

    /// How many times this judge runs each cell of the arm of this name: none when it does
    /// not run the arm.
    pub fn repeated_under(&self, matrix: &Spec, arm: &str) -> Option<u32> {
        match &self.arms {
            None => Some(self.repeated(matrix)),
            Some(arms) => (arms.iter())
                .find(|a| a.arm == arm)
                .map(|a| a.repetitions.unwrap_or_else(|| self.repeated(matrix))),
        }
    }
}

/// An arm: `C`, nothing measured first and the judge offered `view` and `align`, the
/// latter answered with its pictures alone (squint's control); or `F`, the details of each
/// pair measured first and opening the conversation. Either may name more tools after a
/// `+` each, which the judge is offered too: `C+outline`.
///
/// Beside its tools, an arm `F` has two settings (plan m3, task U1, item 2): `details`, the
/// details measured first, each for the use cases it is named for, where `align` alone is
/// measured otherwise; and `second_look`, `on` or `strict` for the use cases it is named for
/// (every use case when it names none), `off` for the rest. Arm `C` may take the second look
/// too (plan mE, task M1, item 2): it is one more turn after a submission that left an item
/// tentative, not a detail measured first, so the control stays the control. Both are part of the arm's name,
/// so of each cell's identity and of the matrix's: `F;details=tiles@reality-checks;second_look=on`,
/// `F;second_look=on@reference-consistency|strict@reality-checks`.
/// A matrix file may write an arm as that name, or as a table:
/// `{ arm = "F", details = [{ detail = "tiles", use_cases = ["reality-checks"] }], second_look = "on" }`,
/// or with `second_look = [{ look = "strict", use_cases = ["reality-checks"] }]`.
///
/// Either arm may name the form the judge answers in, `submission`, `v1`, `v2` or `v3` (plan
/// e1, task F1): `C;submission=v2`, or `submission = "v2"` in a table. It is in the arm's
/// name only when given, so every matrix written before it keeps its identity, and an arm
/// that names none runs the default of the binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arm {
    pub first: bool,
    pub tools: Vec<String>,
    /// The method a tool file the arm offers is run with, by the tool's name (plan mE, task
    /// M1, item 12): `C+regions@eomt`. A tool that names none runs its default method.
    pub methods: BTreeMap<String, String>,
    pub details: Vec<Detail>,
    pub second_look: Vec<Look>,
    pub submission: Option<String>,
}

/// The values an arm's `submission` takes. The binary's tests hold this to
/// `whizbang_squint::check::SUBMISSIONS`.
pub const SUBMISSIONS: [&str; 3] = ["v1", "v2", "v3"];

/// A second look an arm gives, and the use cases it gives it for: every one when none is
/// named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Look {
    pub look: String,
    #[serde(default)]
    pub use_cases: Vec<String>,
}

/// The use cases of the table, by name.
pub const USE_CASES: [&str; 5] = [
    "source-fidelity",
    "reference-consistency",
    "reality-checks",
    "brief-compliance",
    "product-fidelity",
];

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

    /// `check`'s argument `second_look` under a use case: `on`, `strict` or `off`.
    pub fn second_look_for(&self, use_case: &str) -> &str {
        (self.second_look.iter())
            .find(|l| l.use_cases.is_empty() || l.use_cases.iter().any(|u| u == use_case))
            .map_or("off", |l| l.look.as_str())
    }

    /// Whether the arm gives a second look under any use case.
    pub fn looks(&self) -> bool {
        !self.second_look.is_empty()
    }

    /// `check`'s argument `projection`: `visual_only` for the control, `full` otherwise.
    pub fn projection(&self) -> &'static str {
        if self.first { "full" } else { "visual_only" }
    }

    fn validate(&self) -> Result<(), String> {
        if !self.first && !self.details.is_empty() {
            return Err(format!(
                "arm {self}: details are a setting of an arm F; C is the control"
            ));
        }
        if let Some(submission) = &self.submission
            && !SUBMISSIONS.contains(&submission.as_str())
        {
            return Err(format!("arm {self}: submission is v1, v2 or v3"));
        }
        if self.details.len() > ARM_DETAILS_LIMIT {
            return Err(format!("arm {self}: at most {ARM_DETAILS_LIMIT} details"));
        }
        let mut named = BTreeSet::new();
        let alone = self.second_look.len() == 1 && self.second_look[0].use_cases.is_empty();
        for look in &self.second_look {
            if !matches!(look.look.as_str(), "on" | "strict") {
                return Err(format!("arm {self}: second_look is on, strict or off"));
            }
            if look.use_cases.is_empty() && !alone {
                return Err(format!(
                    "arm {self}: a second look for every use case is the arm's only one"
                ));
            }
            for use_case in &look.use_cases {
                if !USE_CASES.contains(&use_case.as_str()) || !named.insert(use_case.as_str()) {
                    return Err(format!(
                        "arm {self}: each use case of a second look is one of the table's, once"
                    ));
                }
            }
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
            match self.methods.get(tool) {
                Some(method) => write!(f, "+{tool}@{method}")?,
                None => write!(f, "+{tool}")?,
            }
        }
        if !self.details.is_empty() {
            let named: Vec<String> = (self.details.iter())
                .map(|d| format!("{}@{}", d.detail, d.use_cases.join(",")))
                .collect();
            write!(f, ";details={}", named.join("|"))?;
        }
        if !self.second_look.is_empty() {
            let named: Vec<String> = (self.second_look.iter())
                .map(|l| match l.use_cases.as_slice() {
                    [] => l.look.clone(),
                    cases => format!("{}@{}", l.look, cases.join(",")),
                })
                .collect();
            write!(f, ";second_look={}", named.join("|"))?;
        }
        if let Some(submission) = &self.submission {
            write!(f, ";submission={submission}")?;
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
        let mut tools: Vec<String> = Vec::new();
        let mut methods = BTreeMap::new();
        for part in parts {
            let (tool, method) = match part.split_once('@') {
                Some((tool, method)) => (tool, Some(method)),
                None => (part, None),
            };
            if !plain(tool) || method.is_some_and(|m| !plain(m)) || tools.iter().any(|t| t == tool)
            {
                return Err(format!("arm {text:?} names a tool that is no tool's name"));
            }
            if let Some(method) = method {
                methods.insert(tool.to_owned(), method.to_owned());
            }
            tools.push(tool.to_owned());
        }
        if tools.len() > ARM_TOOLS_LIMIT {
            return Err(format!("arm {text:?} names a tool that is no tool's name"));
        }
        let mut arm = Arm {
            first,
            tools,
            methods,
            details: Vec::new(),
            second_look: Vec::new(),
            submission: None,
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
                "second_look" if value == "off" => {}
                "second_look" => {
                    for one in value.split('|') {
                        let (look, cases) = one.split_once('@').unwrap_or((one, ""));
                        let use_cases: Vec<String> = (cases.split(','))
                            .filter(|c| !c.is_empty())
                            .map(str::to_owned)
                            .collect();
                        if !plain(look) || !use_cases.iter().all(|c| plain(c)) {
                            return Err(format!(
                                "arm {text:?}: second_look is on, strict or off, with \
                                 @<use case>,... after it"
                            ));
                        }
                        arm.second_look.push(Look {
                            look: look.to_owned(),
                            use_cases,
                        });
                    }
                }
                "submission" if plain(value) => arm.submission = Some(value.to_owned()),
                "submission" => {
                    return Err(format!("arm {text:?}: submission is v1, v2 or v3"));
                }
                _ => {
                    return Err(format!(
                        "arm {text:?}: its settings are details, second_look and submission"
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
    /// More tools the arm offers, each with the method it is run with.
    #[serde(default)]
    tools: Vec<ToolMethod>,
    #[serde(default)]
    details: Vec<Detail>,
    #[serde(default)]
    second_look: Option<Looks>,
    #[serde(default)]
    submission: Option<String>,
}

/// A tool file an arm offers, with the method it is run with.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolMethod {
    tool: String,
    method: String,
}

/// A table's second look: `on`, `strict` or `off` for every use case, or a list of looks.
#[derive(Deserialize)]
#[serde(untagged)]
enum Looks {
    Word(String),
    List(Vec<Look>),
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
                for one in table.tools {
                    if arm.tools.contains(&one.tool) {
                        return Err(serde::de::Error::custom(format!(
                            "arm {}: the tool {} is named twice",
                            table.arm, one.tool
                        )));
                    }
                    arm.tools.push(one.tool.clone());
                    arm.methods.insert(one.tool, one.method);
                }
                arm.details = table.details;
                arm.second_look = match table.second_look {
                    None => Vec::new(),
                    Some(Looks::Word(word)) if word == "off" => Vec::new(),
                    Some(Looks::Word(word)) => vec![Look {
                        look: word,
                        use_cases: Vec::new(),
                    }],
                    Some(Looks::List(looks)) => looks,
                };
                arm.submission = table.submission;
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

    /// A matrix built as a value, held to the checks a file is held to.
    pub fn checked(self) -> Result<Matrix, String> {
        self.validate()?;
        Ok(self)
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
        let repeated = std::iter::once(m.repetitions)
            .chain(m.judges.iter().filter_map(|judge| judge.repetitions));
        if !repeated
            .into_iter()
            .all(|r| (1..=REPETITIONS_LIMIT).contains(&r))
        {
            return Err(format!("repetitions is 1 to {REPETITIONS_LIMIT}"));
        }
        if !(1..=CONCURRENCY_LIMIT).contains(&m.concurrency) {
            return Err(format!("concurrency is 1 to {CONCURRENCY_LIMIT}"));
        }
        let distinct = |count: usize, set: usize| count == set;
        let sets: BTreeSet<&String> = m.sets.iter().collect();
        let judges: BTreeSet<(&String, &String)> =
            m.judges.iter().map(|j| (&j.backend, &j.model)).collect();
        let arms: BTreeSet<String> = m.arms.iter().map(Arm::to_string).collect();
        if !distinct(m.sets.len(), sets.len())
            || !distinct(m.judges.len(), judges.len())
            || !distinct(m.arms.len(), arms.len())
        {
            return Err("a matrix names each set, judge and arm once".to_owned());
        }
        self.validate_lists(&arms)?;
        if self.hypotheses == Some(0) {
            return Err("a matrix's hypotheses are at least one".to_owned());
        }
        for (file, digest) in &self.gates {
            let plain = |part: &str| {
                !part.is_empty()
                    && part != "."
                    && part != ".."
                    && (part.chars())
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            };
            let hex = |c: char| c.is_ascii_digit() || ('a'..='f').contains(&c);
            if !file.split('/').all(plain) || digest.len() != 64 || !digest.chars().all(hex) {
                return Err(
                    "[gates] names each gate file by a plain path beside the matrix, with the \
                     SHA-256 of its bytes in 64 lowercase hex digits"
                        .to_owned(),
                );
            }
        }
        Ok(())
    }

    /// The judges' lists of arms and the sets' tables: each names what the matrix has, once,
    /// and every one of them leaves at least one cell.
    fn validate_lists(&self, arms: &BTreeSet<String>) -> Result<(), String> {
        let m = &self.matrix;
        let canonical = |name: &str| -> Result<String, String> {
            let arm: Arm = name.parse()?;
            let arm = arm.to_string();
            if arms.contains(&arm) {
                Ok(arm)
            } else {
                Err(format!("{arm} is not an arm of the matrix"))
            }
        };
        for judge in &m.judges {
            let Some(listed) = &judge.arms else { continue };
            let mut seen = BTreeSet::new();
            if listed.is_empty() {
                return Err(format!(
                    "the judge {}: a list of arms names at least one",
                    judge.backend
                ));
            }
            for one in listed {
                let arm = canonical(&one.arm)
                    .map_err(|why| format!("the judge {}: {why}", judge.backend))?;
                if arm != one.arm || !seen.insert(arm) {
                    return Err(format!(
                        "the judge {}: each arm it runs is named once, as the matrix names it",
                        judge.backend
                    ));
                }
                if let Some(r) = one.repetitions
                    && !(1..=REPETITIONS_LIMIT).contains(&r)
                {
                    return Err(format!("repetitions is 1 to {REPETITIONS_LIMIT}"));
                }
            }
        }
        for (id, runs) in &m.set {
            if !m.sets.contains(id) {
                return Err(format!(
                    "[matrix.set] names {id:?}, which is not a set of the matrix"
                ));
            }
            if let Some(listed) = &runs.arms {
                let mut seen = BTreeSet::new();
                for name in listed {
                    let arm = canonical(name).map_err(|why| format!("the set {id}: {why}"))?;
                    if arm != *name || !seen.insert(arm) {
                        return Err(format!(
                            "the set {id}: each arm it runs is named once, as the matrix names it"
                        ));
                    }
                }
            }
            if let Some(listed) = &runs.judges {
                let mut seen = BTreeSet::new();
                for name in listed {
                    let named = m.judges.iter().filter(|j| j.backend == *name).count();
                    if named != 1 || !seen.insert(name) {
                        return Err(format!(
                            "the set {id}: each judge it runs is named once, by the backend of \
                             one judge of the matrix"
                        ));
                    }
                }
            }
            if runs.arms.is_none() && runs.judges.is_none() {
                return Err(format!("the set {id}: a set's table lists arms or judges"));
            }
            if runs.arms.as_ref().is_some_and(Vec::is_empty)
                || runs.judges.as_ref().is_some_and(Vec::is_empty)
            {
                return Err(format!("the set {id}: a list names at least one"));
            }
        }
        // Every set, judge and arm the matrix names has a cell.
        let named = |set: Option<&String>, judge: Option<&Judge>, arm: Option<&String>| {
            (m.sets.iter().filter(|s| set.is_none_or(|one| one == *s))).any(|s| {
                (m.judges
                    .iter()
                    .filter(|j| judge.is_none_or(|one| one == *j)))
                .any(|j| {
                    (arms.iter().filter(|a| arm.is_none_or(|one| one == *a)))
                        .any(|a| self.runs(s, j, a).is_some())
                })
            })
        };
        let idle = (m
            .sets
            .iter()
            .find(|s| !named(Some(s), None, None))
            .map(|s| format!("the set {s}")))
        .or_else(|| {
            (m.judges.iter().find(|j| !named(None, Some(j), None)))
                .map(|j| format!("the judge {}", j.backend))
        })
        .or_else(|| {
            (arms.iter().find(|a| !named(None, None, Some(a)))).map(|a| format!("the arm {a}"))
        });
        if let Some(idle) = idle {
            return Err(format!(
                "{idle} has no cell: the lists leave it nothing to run"
            ));
        }
        Ok(())
    }

    /// How many times the matrix runs each item of this set under this judge and arm: none
    /// when the cell is not one it names.
    pub fn runs(&self, set: &str, judge: &Judge, arm: &str) -> Option<u32> {
        let m = &self.matrix;
        if let Some(runs) = m.set.get(set) {
            if runs
                .arms
                .as_ref()
                .is_some_and(|arms| !arms.iter().any(|a| a == arm))
            {
                return None;
            }
            if (runs.judges.as_ref()).is_some_and(|judges| !judges.contains(&judge.backend)) {
                return None;
            }
        }
        judge.repeated_under(m, arm)
    }

    /// Whether the matrix names lists of what it runs: one that names none runs every cell.
    pub fn lists(&self) -> bool {
        !self.matrix.set.is_empty() || self.matrix.judges.iter().any(|j| j.arms.is_some())
    }

    /// The matrix's identity: the SHA-256 of its content in one canonical form, whatever
    /// the file's layout. Its first 16 hex digits name it in each run's cell.
    pub fn identity(&self) -> String {
        let canonical = serde_json::to_string(self).unwrap_or_default();
        Digest::of_bytes(canonical.as_bytes()).hex()
    }

    /// Every cell the matrix names over the items of each set, given in the matrix's order
    /// of sets: sets, then items, then judges, then arms, then repetitions. A cell outside a
    /// judge's or a set's list is none.
    pub fn cells(&self, items: &[(String, Vec<String>)]) -> Vec<Cell> {
        let m = &self.matrix;
        let mut out = Vec::new();
        for (set, its) in items {
            for item in its {
                for judge in &m.judges {
                    for arm in &m.arms {
                        let name = arm.to_string();
                        let Some(times) = self.runs(set, judge, &name) else {
                            continue;
                        };
                        for repetition in 1..=times {
                            out.push(Cell {
                                set: set.clone(),
                                item: item.clone(),
                                backend: judge.backend.clone(),
                                model: judge.model.clone(),
                                arm: name.clone(),
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

/// The resamples of every interval of a round whose correction counts `hypotheses` (plan mE,
/// task M1, item 6): enough that each tail of a 95% interval split `hypotheses` ways holds
/// at least 20 draws, rounded up to a thousand, and never fewer than 20,000.
pub fn resamples_for(hypotheses: usize) -> usize {
    let wanted = (20 * 2 * hypotheses.max(1)) as f64 / 0.05;
    let thousands = (wanted / 1_000.0).ceil() as usize * 1_000;
    thousands.max(20_000)
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
            Ok(arm) if arm.looks() => 3,
            Ok(arm) if arm.first => 2,
            Ok(_) => 1,
            Err(_) if cell.arm.starts_with('F') => 2,
            Err(_) => 1,
        };
        total = total.saturating_add(rate.saturating_mul(arm));
    }
    Ok(total.saturating_mul(2))
}
