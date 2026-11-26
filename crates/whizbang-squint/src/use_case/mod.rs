//! The five use cases of `SPEC.md` section 7 as one table (plan 2C, task A1, item 1):
//! each one's name, the roles it requires, its rules with their kinds, and from those its
//! checklist, its prompt and its submission. Ported from squint at the commit
//! `docs/sources.toml` pins: the rule files (`crates/squint-core/rules/*.toml`, byte for
//! byte), the obligations' merge, validation and resolution (`obligations.rs`), the
//! rendering of the prompt and the checklist (`render.rs`, `checklist.rs`, the template
//! `prompts/v1.md`, byte for byte), the aggregation table (`aggregate.rs`) and the rules
//! a submission is held to (`submission.rs`).
//!
//! What is cut, and why: a rule file's profile, `extends` and `[screen]`, and a consumer's
//! own rule files, which no base file, arm, set or user of whizbang has asked for (dropped by
//! plan mE, task T1, item 4, with a line in `docs/carry-over.md`); a file that holds one is
//! refused when read. `[hints]` is read ([`Hints`]), and the evidence items a tool's regions
//! add to the checklist are `crate::checklist`'s.
//!
//! **The judge disposes; the code decides.** The verdict is worked out from the judge's
//! dispositions by squint's table ([`row`]), for every kind, `must_not` included, and
//! never from the judge's own opinion.

mod render;
mod submission;
mod v2;
mod v3;

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub use render::{Checklist, Item, NO_BRIEF, clean, opening, system, system_with};
pub use submission::{
    Class, Decided, REASONS, Rejection, cited_calls_held, decided_of, submit_definition,
    submit_tool,
};
pub use v2::{
    Given, KEY_LIMIT, PROBLEMS_SHOWN, Recovery, Submitted, decided_of_v2, item_keys, submit_tool_v2,
};
pub use v3::{ReadBack, decided_of_v3, read_back, submit_tool_v3};

/// The values of the setting `submission`, the form the judge answers in (ADR 0042), and its
/// default: v1, squint's; v2, keyed by checklist id; v3, each finding inside its item.
pub const FORMATS: [&str; 3] = ["v1", "v2", "v3"];
pub const DEFAULT_FORMAT: &str = "v1";

use crate::job::{Job, Role};

/// The tool the judge answers with.
pub const SUBMIT: &str = "submit_verdict";

/// The last words of the opening.
pub const INSTRUCTION: &str =
    "Inspect the images, then call submit_verdict with one disposition per checklist item.";

/// The words a repair is asked with, after a submission that is not a verdict.
pub const REPAIR: &str = "Repair: call submit_verdict once, alone, with exactly one \
                          disposition per checklist item. This is the only repair.";

/// The use cases, each with its rule file, squint's, byte for byte.
pub const TABLE: [(&str, &str); 5] = [
    (
        "source-fidelity",
        include_str!("../../rules/source-fidelity.toml"),
    ),
    (
        "reference-consistency",
        include_str!("../../rules/reference-consistency.toml"),
    ),
    (
        "reality-checks",
        include_str!("../../rules/reality-checks.toml"),
    ),
    (
        "brief-compliance",
        include_str!("../../rules/brief-compliance.toml"),
    ),
    (
        "product-fidelity",
        include_str!("../../rules/product-fidelity.toml"),
    ),
];

/// The use cases, by name, in the order of the table.
pub const NAMES: [&str; 5] = [TABLE[0].0, TABLE[1].0, TABLE[2].0, TABLE[3].0, TABLE[4].0];

/// What an obligation does in aggregation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    MustHold,
    MustNot,
    Allowed,
    Observe,
}

impl Kind {
    pub fn is_mandatory(self) -> bool {
        matches!(self, Kind::MustHold | Kind::MustNot)
    }
}

/// One obligation, as a rule file or a job's requirements write it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Obligation {
    pub id: String,
    pub kind: Kind,
    #[serde(default)]
    pub text: String,
    /// Controls uncertainty only. Defaults to true for mandatory kinds.
    #[serde(default)]
    pub matters: Option<bool>,
    /// Mandatory kinds only: `hidden` is recorded instead of unsure.
    #[serde(default)]
    pub hidden_ok: bool,
    /// Image roles or ids whose correctness this judges; omitted means `candidate`.
    #[serde(default)]
    pub scope: Option<Vec<String>>,
    /// Image roles or ids consulted to judge it; omitted means every source and
    /// reference.
    #[serde(default)]
    pub evidence: Option<Vec<String>>,
    /// Tool evidence kinds whose regions bear on this: an evidence item from a tool's
    /// regions of one of these kinds binds to it (`crate::checklist::ingest`).
    #[serde(default)]
    pub evidence_kinds: Option<Vec<String>>,
    /// Exemptions only: the mandatory obligation ids this may waive.
    #[serde(default)]
    pub waives: Vec<String>,
    /// Exemptions only: the condition under which the exemption applies.
    #[serde(default)]
    pub condition: Option<String>,
    /// Drops an obligation of this id from the merge.
    #[serde(default)]
    pub disabled: bool,
}

impl Obligation {
    /// The effective `matters`: as given, else true for mandatory kinds.
    pub fn matters(&self) -> bool {
        self.matters.unwrap_or(self.kind.is_mandatory())
    }
}

/// The `[rules]` header of a rule file.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Header {
    pub use_case: String,
    pub version: u32,
    #[serde(default)]
    pub requires_roles: Vec<Role>,
    #[serde(default)]
    pub default_evidence_kinds: Vec<String>,
}

/// A rule file.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub rules: Header,
    #[serde(default, rename = "rule")]
    pub obligations: Vec<Obligation>,
    #[serde(default)]
    pub hints: Hints,
}

/// A rule file's `[hints]` (squint's, plan mE, task T1, item 4): what the rules tell a tool
/// of what matters, handed to a tool file as its arguments ([`Hints::arguments`]). Squint
/// held a free table for each tool; whizbang reads the two its tools take, and refuses any
/// other.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hints {
    /// `regions`: things whose change matters more, and less.
    #[serde(default)]
    pub regions: Option<RegionHints>,
    /// `outline`: the protected things whose shape it compares.
    #[serde(default)]
    pub outline: Option<OutlineHints>,
}

/// `[hints.regions]`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionHints {
    #[serde(default)]
    pub preserve: Vec<String>,
    #[serde(default)]
    pub ignore: Vec<String>,
}

/// `[hints.outline]`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutlineHints {
    #[serde(default)]
    pub classes: Vec<String>,
}

impl Hints {
    /// The arguments the hints give the tool of this name, each a list of phrases
    /// separated by semicolons: `regions` takes `preserve` and `ignore`, `outline` its
    /// `classes` as `phrases`. None for another tool, or for a hint with no phrase. A
    /// phrase is cleaned to one line, and a semicolon in it becomes a comma.
    pub fn arguments(&self, tool: &str) -> Vec<(String, String)> {
        let listed = |phrases: &[String]| {
            (phrases.iter())
                .map(|p| render::clean(p).replace(';', ","))
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join("; ")
        };
        let given: Vec<(&str, String)> = match (tool, &self.regions, &self.outline) {
            ("regions", Some(hints), _) => vec![
                ("preserve", listed(&hints.preserve)),
                ("ignore", listed(&hints.ignore)),
            ],
            ("outline", _, Some(hints)) => vec![("phrases", listed(&hints.classes))],
            _ => Vec::new(),
        };
        (given.into_iter())
            .filter(|(_, value)| !value.is_empty())
            .map(|(name, value)| (name.to_owned(), value))
            .collect()
    }
}

/// The rules of the use case of this name, read from its file; none for a name that is
/// not in the table.
pub fn rules(name: &str) -> Option<Rules> {
    let (_, text) = TABLE.iter().find(|(named, _)| *named == name)?;
    toml::from_str(text).ok()
}

/// One obligation resolved to one target image: `SF2@candidate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instance {
    pub obligation_id: String,
    pub target: String,
    pub kind: Kind,
    pub matters: bool,
    pub hidden_ok: bool,
    pub text: String,
    /// Image ids consulted to judge this instance.
    pub evidence: Vec<String>,
    /// The kinds of a tool's regions that bear on it: its obligation's `evidence_kinds`,
    /// else its rule file's `default_evidence_kinds` (squint's resolution), which an
    /// evidence item binds by (`crate::checklist::ingest`).
    pub evidence_kinds: Vec<String>,
    /// `rules` or `job`.
    pub provenance: &'static str,
}

impl Instance {
    /// The instance id, `<obligation>@<target>`.
    pub fn id(&self) -> String {
        format!("{}@{}", self.obligation_id, self.target)
    }
}

/// An exemption resolved to the targets it may waive on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exemption {
    pub id: String,
    pub text: String,
    pub waives: Vec<String>,
    pub condition: Option<String>,
    pub targets: Vec<String>,
    pub provenance: &'static str,
}

impl Exemption {
    /// True when this exemption may waive `obligation` on `target`.
    pub fn applies_to(&self, obligation: &str, target: &str) -> bool {
        self.waives.iter().any(|w| w == obligation) && self.targets.iter().any(|t| t == target)
    }
}

/// An observational obligation resolved to its targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    pub id: String,
    pub text: String,
    pub targets: Vec<String>,
    pub provenance: &'static str,
}

/// Everything resolution makes of one job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub use_case: String,
    /// The version of the use case's rule file.
    pub version: u32,
    pub instances: Vec<Instance>,
    pub exemptions: Vec<Exemption>,
    pub observations: Vec<Observation>,
}

impl Resolved {
    pub fn instance(&self, id: &str) -> Option<&Instance> {
        self.instances.iter().find(|i| i.id() == id)
    }
}

/// Rule ids are two capital letters and a number of at most four digits (`SF1`);
/// requirement ids are `RQ` and such a number. Four digits is what a finding may cite
/// (`findings/1`), so an obligation that passes here is never left out of the findings
/// for its id.
fn check_id(id: &str, job: bool) -> Result<(), String> {
    let at = id.find(|c: char| c.is_ascii_digit()).unwrap_or(id.len());
    let (letters, digits) = id.split_at(at);
    let ok = letters.len() == 2
        && letters.chars().all(|c| c.is_ascii_uppercase())
        && (1..=4).contains(&digits.len())
        && digits.chars().all(|c| c.is_ascii_digit());
    if !ok {
        return Err(format!(
            "id {id:?} must be two capital letters and a number of at most four digits (SF1, RQ2)"
        ));
    }
    if job && letters != "RQ" {
        return Err(format!("job requirement id {id:?} must start with RQ"));
    }
    Ok(())
}

/// The use case's rules, then the job's requirements over them: a requirement of an id
/// already there replaces it, one marked `disabled` drops it.
fn merge(rules: &Rules, job: &Job) -> Result<Vec<(Obligation, &'static str)>, String> {
    let mut merged: Vec<(Obligation, &'static str)> = Vec::new();
    let layers = [(&rules.obligations, "rules"), (&job.requirements, "job")];
    for (obligations, provenance) in layers {
        for ob in obligations {
            check_id(&ob.id, provenance == "job")?;
            if ob.disabled {
                merged.retain(|(m, _)| m.id != ob.id);
                continue;
            }
            match merged.iter_mut().find(|(m, _)| m.id == ob.id) {
                Some(existing) => *existing = (ob.clone(), provenance),
                None => merged.push((ob.clone(), provenance)),
            }
        }
    }
    Ok(merged)
}

fn check_image(entry: &str, job: &Job, id: &str, field: &str) -> Result<(), String> {
    let role = matches!(entry, "candidate" | "source" | "reference");
    if role || job.images.iter().any(|i| i.id == entry) {
        Ok(())
    } else {
        Err(format!(
            "{id} {field} entry {entry:?} is neither a role nor an image id in the job"
        ))
    }
}

/// Squint's checks of a merge: ids unique, every text given, an exemption's waives and
/// condition, `hidden_ok` on a mandatory kind only, scope and evidence naming roles or
/// images of the job, and an image of each role the use case requires.
fn validate(merged: &[(Obligation, &str)], rules: &Rules, job: &Job) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for (o, _) in merged {
        if !seen.insert(o.id.as_str()) {
            return Err(format!("duplicate obligation id {:?} after merge", o.id));
        }
    }
    for (o, _) in merged {
        if o.text.trim().is_empty() {
            return Err(format!("{} has no text", o.id));
        }
        if o.kind == Kind::Allowed {
            for w in &o.waives {
                match merged.iter().find(|(m, _)| &m.id == w) {
                    None => return Err(format!("{} waives unknown obligation {w:?}", o.id)),
                    Some((m, _)) if !m.kind.is_mandatory() => {
                        return Err(format!("{} waives {w}, which is not mandatory", o.id));
                    }
                    _ => {}
                }
            }
            let condition = o.condition.as_deref().map_or("", str::trim);
            if !o.waives.is_empty() && condition.is_empty() {
                return Err(format!(
                    "exemption {} waives {:?} but has no condition",
                    o.id, o.waives
                ));
            }
        } else if !o.waives.is_empty() {
            return Err(format!("{} is not an exemption and cannot waive", o.id));
        } else if o.condition.is_some() {
            return Err(format!(
                "{} is not an exemption and takes no condition",
                o.id
            ));
        }
        if o.hidden_ok && !o.kind.is_mandatory() {
            return Err(format!("hidden_ok on {} is only for mandatory kinds", o.id));
        }
        if let Some(scope) = &o.scope {
            if scope.is_empty() {
                return Err(format!("{} has an explicit empty scope", o.id));
            }
            for entry in scope {
                check_image(entry, job, &o.id, "scope")?;
            }
        }
        for entry in o.evidence.iter().flatten() {
            check_image(entry, job, &o.id, "evidence")?;
        }
    }
    if job.use_case != rules.rules.use_case {
        return Err(format!(
            "the job's use case {:?} does not match the rules' {:?}",
            job.use_case, rules.rules.use_case
        ));
    }
    for role in &rules.rules.requires_roles {
        if !job.images.iter().any(|i| i.role == *role) {
            return Err(format!(
                "{:?} requires an image with role {:?}, the job has none",
                rules.rules.use_case,
                role.word()
            ));
        }
    }
    Ok(())
}

/// A scope or an evidence list as image ids: a role is every image of that role, an id
/// is itself; in the job's order, each once.
fn expand(entries: &[String], job: &Job) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for image in &job.images {
        let hit = entries
            .iter()
            .any(|e| *e == image.id || e == image.role.word());
        if hit && !out.contains(&image.id) {
            out.push(image.id.clone());
        }
    }
    out
}

/// The job's use case's rules and its requirements, merged, checked and resolved into
/// instances, exemptions and observations; or why the job cannot be judged under them.
/// A use case that is not in the table, an image of a role the use case does not take,
/// a mandatory obligation with no target, a job with no mandatory instance, and a
/// reference no mandatory obligation consults are each refused.
pub fn resolve(job: &Job) -> Result<Resolved, String> {
    let rules = rules(&job.use_case)
        .ok_or_else(|| format!("{:?} is not a use case of the table", job.use_case))?;
    let takes = |role: Role| rules.rules.requires_roles.contains(&role) || role == Role::Candidate;
    if let Some(image) = job.images.iter().find(|i| !takes(i.role)) {
        return Err(format!(
            "{} takes no image with role {:?}, and the job's {:?} has it",
            job.use_case,
            image.role.word(),
            image.id
        ));
    }
    let merged = merge(&rules, job)?;
    validate(&merged, &rules, job)?;
    let default_scope = vec!["candidate".to_owned()];
    let default_evidence = vec!["source".to_owned(), "reference".to_owned()];
    let mut resolved = Resolved {
        use_case: rules.rules.use_case.clone(),
        version: rules.rules.version,
        instances: Vec::new(),
        exemptions: Vec::new(),
        observations: Vec::new(),
    };
    for (o, provenance) in merged {
        let scope = o.scope.as_ref().unwrap_or(&default_scope);
        let targets = expand(scope, job);
        match o.kind {
            Kind::MustHold | Kind::MustNot => {
                if targets.is_empty() {
                    return Err(format!(
                        "{} resolves to no target image in this job (scope {scope:?})",
                        o.id
                    ));
                }
                let evidence = expand(o.evidence.as_ref().unwrap_or(&default_evidence), job);
                let evidence_kinds = (o.evidence_kinds.clone())
                    .unwrap_or_else(|| rules.rules.default_evidence_kinds.clone());
                for target in targets {
                    resolved.instances.push(Instance {
                        obligation_id: o.id.clone(),
                        target,
                        kind: o.kind,
                        matters: o.matters(),
                        hidden_ok: o.hidden_ok,
                        text: o.text.clone(),
                        evidence: evidence.clone(),
                        evidence_kinds: evidence_kinds.clone(),
                        provenance,
                    });
                }
            }
            Kind::Allowed => resolved.exemptions.push(Exemption {
                id: o.id,
                text: o.text,
                waives: o.waives,
                condition: o.condition,
                targets,
                provenance,
            }),
            Kind::Observe => resolved.observations.push(Observation {
                id: o.id,
                text: o.text,
                targets,
                provenance,
            }),
        }
    }
    if resolved.instances.is_empty() {
        return Err("this check has no mandatory instance".to_owned());
    }
    for image in job.images.iter().filter(|i| i.role == Role::Reference) {
        if !resolved
            .instances
            .iter()
            .any(|i| i.evidence.contains(&image.id))
        {
            return Err(format!(
                "reference image {:?} is consulted by no mandatory obligation",
                image.id
            ));
        }
    }
    Ok(resolved)
}

/// What a judge said of an item or a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Violation,
    Held,
    AllowedChange,
    Hidden,
    CantTell,
    /// Findings only: an observation under an `observe` obligation.
    Observed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Confident,
    Tentative,
}

/// One row of the aggregation. Ordered so that the greatest is the strictest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Row {
    Recorded,
    Unsure,
    Fail,
}

/// Squint's table: the row one disposition makes of one mandatory instance, whatever its
/// kind. A violation of a `must_not` is the forbidden thing seen, as a violation of a
/// `must_hold` is the required thing broken.
pub fn row(instance: &Instance, d: Disposition, c: Confidence) -> Row {
    use Confidence::{Confident, Tentative};
    use Disposition::{AllowedChange, CantTell, Held, Hidden, Violation};
    match (d, c) {
        (Violation, Confident) => Row::Fail,
        (Violation, Tentative) => Row::Unsure,
        (CantTell, _) if instance.matters => Row::Unsure,
        (Hidden, _) if instance.matters && !instance.hidden_ok => Row::Unsure,
        (Held | AllowedChange, Tentative) if instance.matters => Row::Unsure,
        _ => Row::Recorded,
    }
}
