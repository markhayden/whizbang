//! Evaluation sets (plan 2C, task E1, items 1 and 2): one set's manifest, families,
//! labels, corrections and seal, read strictly and checked; and the check over a tree of
//! sets. Ported from squint (`crates/squint-eval/src/sets.rs`, at the commit
//! `docs/sources.toml` pins), reshaped: nothing here writes a set or a seal, and nothing
//! fetches. `docs/evalsets.md` is how the sets are laid out.
//!
//! An item is a job and a label. The job is a squint job file that `check` takes as it is
//! (its pictures named relative to the set's folder, each pinned by its SHA-256); the labels
//! live in their own file, so a blind set has none. What only the binary knows, whether a
//! job is one `check` can judge under its use case and what a rule file says, is handed in
//! ([`Judging`]), so this crate depends on no use case.
//!
//! **The seal is checked offline.** Its components are worked out as squint worked them
//! out, with one difference: the pictures' component is made from the SHA-256 each job
//! pins, not from files on disk. The two are the same wherever the pictures are there and
//! match their pins, which is the only way a picture is ever imported
//! (`whizbang eval sets fetch`), so a set is checked without a picture on the machine.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use whizbang_core::identity::Digest;

pub const MANIFEST_FILE: &str = "set.toml";
pub const SEAL_FILE: &str = "seal.toml";
pub const CORRECTIONS_FILE: &str = "corrections.toml";

/// The most bytes of one file of a set that is read.
pub const FILE_LIMIT: u64 = 4 * 1024 * 1024;

/// What the binary knows of judging that a set is checked against: the text of the rule
/// file of a use case, as `check` ships it; and whether a job file is one `check` takes
/// under its use case (its roles and requirements resolve against the rules).
pub struct Judging<'a> {
    pub rules: &'a dyn Fn(&str) -> Option<&'static str>,
    pub job: &'a dyn Fn(&str) -> Result<(), String>,
}

/// `set.toml`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub set: SetMeta,
    #[serde(default)]
    pub item: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetMeta {
    /// `<use-case>/<set>`, which is also the set's path under the root.
    pub id: String,
    pub use_case: String,
    pub split: Split,
    pub licence: String,
    /// Where the pictures are: `github.com/<owner>/<repo>`, served raw at `images_rev`.
    pub images_repo: String,
    #[serde(default)]
    pub images_rev: Option<String>,
    /// The labels file, relative to the set's folder; none for a blind set.
    #[serde(default)]
    pub labels: Option<String>,
    /// The SHA-256 of a blind set's labels file, which is held elsewhere.
    #[serde(default)]
    pub labels_sha256: Option<String>,
    pub families: String,
    pub rules: RulesPin,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RulesPin {
    pub base: String,
    pub version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Split {
    Dev,
    Heldout,
    Blind,
}

impl std::fmt::Display for Split {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Split::Dev => "dev",
            Split::Heldout => "heldout",
            Split::Blind => "blind",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub id: String,
    pub family: String,
    /// The job file, relative to the set's folder.
    pub job: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Families {
    #[serde(default)]
    family: Vec<Family>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Family {
    id: String,
    #[allow(dead_code)]
    description: String,
}

/// `labels.toml`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Labels {
    #[serde(default)]
    pub label: Vec<Label>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Label {
    pub item: String,
    pub expected: Expected,
    #[serde(default)]
    pub failure_types: Vec<String>,
    pub words: String,
    #[serde(default)]
    pub obligation_ids: Vec<String>,
    pub adjudicated_by: String,
    pub adjudicated_on: String,
    #[serde(default)]
    pub regions: Vec<Region>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Expected {
    Pass,
    Fail,
}

/// A labelled region on a picture.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub frame: String,
    pub kind: RegionKind,
    #[serde(default)]
    pub polygon: Option<Vec<[f64; 2]>>,
    #[serde(default)]
    pub bbox: Option<[f64; 4]>,
    pub defect: String,
    #[serde(default)]
    pub obligation_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionKind {
    Polygon,
    Bbox,
    WholeImage,
    Absent,
}

/// `corrections.toml`: the owner's corrections to labels found after the set was scored.
/// The labels file stays as sealed; a correction supersedes the label of its item.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Corrections {
    #[serde(default)]
    correction: Vec<Correction>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Correction {
    item: String,
    /// The label's `expected` as sealed, which the correction replaces.
    was: Expected,
    expected: Expected,
    #[serde(default)]
    failure_types: Vec<String>,
    words: String,
    #[serde(default)]
    obligation_ids: Vec<String>,
    adjudicated_by: String,
    adjudicated_on: String,
    #[allow(dead_code)]
    found_by: String,
}

/// `seal.toml`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Seal {
    seal: SealBody,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct SealBody {
    sha256: String,
    #[allow(dead_code)]
    created: String,
    #[allow(dead_code)]
    attestation: String,
    components: Components,
}

/// One SHA-256 for each component of a seal.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Components {
    pub items: String,
    pub families: String,
    pub jobs: String,
    pub images: String,
    pub rules: String,
    pub labels: String,
}

impl Components {
    fn overall(&self) -> String {
        hex(format!(
            "items:{}\nfamilies:{}\njobs:{}\nimages:{}\nrules:{}\nlabels:{}\n",
            self.items, self.families, self.jobs, self.images, self.rules, self.labels
        )
        .as_bytes())
    }

    fn changed(&self, other: &Components) -> Vec<&'static str> {
        [
            ("items", &self.items, &other.items),
            ("families", &self.families, &other.families),
            ("jobs", &self.jobs, &other.jobs),
            ("images", &self.images, &other.images),
            ("rules", &self.rules, &other.rules),
            ("labels", &self.labels, &other.labels),
        ]
        .into_iter()
        .filter(|(_, a, b)| a != b)
        .map(|(name, _, _)| name)
        .collect()
    }
}

// --- a job, as squint's seal wrote it ---------------------------------------------------------

/// A job file as squint read it, field for field, so that the seal's component of the
/// jobs is made of the same JSON squint made it of. Nothing else reads this: `check` takes
/// the job file itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JobFile {
    job: Job,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Job {
    use_case: String,
    #[serde(default)]
    brief: Option<String>,
    #[serde(default)]
    requirements: Vec<Obligation>,
    #[serde(default)]
    images: Vec<JobImage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Obligation {
    id: String,
    kind: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    matters: Option<bool>,
    #[serde(default)]
    hidden_ok: bool,
    #[serde(default)]
    scope: Option<Vec<String>>,
    #[serde(default)]
    evidence: Option<Vec<String>>,
    #[serde(default)]
    evidence_kinds: Option<Vec<String>>,
    #[serde(default)]
    waives: Vec<String>,
    #[serde(default)]
    condition: Option<String>,
    #[serde(default)]
    disabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JobImage {
    id: String,
    role: String,
    #[serde(default)]
    label: Option<String>,
    source: Source,
    #[serde(default)]
    projection: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
}

/// One picture of a set: the item, the image's id in its job, its path relative to the
/// set's folder, and the SHA-256 its job pins, in lower case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    pub item: String,
    pub image: String,
    pub path: String,
    pub sha256: String,
}

/// One set, read from its folder: its manifest, its families, its labels with any
/// corrections applied, and each item's job file as text.
#[derive(Debug, Clone)]
pub struct Set {
    pub dir: PathBuf,
    pub manifest: Manifest,
    families: Families,
    /// The labels as sealed: none for a blind set.
    sealed_labels: Option<Labels>,
    /// The labels with the corrections applied.
    pub labels: Option<Labels>,
    /// Each item's job file, as text, in the manifest's order.
    pub jobs: Vec<(String, String)>,
    parsed: Vec<Job>,
}

fn said(set: &str, what: impl std::fmt::Display) -> String {
    format!("{set}: {what}")
}

fn hex(bytes: &[u8]) -> String {
    Digest::of_bytes(bytes).hex()
}

/// A file of a set, as text, at most [`FILE_LIMIT`] bytes.
fn read_text(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let shown = path.display();
    let file = std::fs::File::open(path).map_err(|e| format!("cannot read {shown}: {e}"))?;
    let mut text = String::new();
    file.take(FILE_LIMIT + 1)
        .read_to_string(&mut text)
        .map_err(|e| format!("cannot read {shown}: {e}"))?;
    if text.len() as u64 > FILE_LIMIT {
        return Err(format!("{shown} is over {FILE_LIMIT} bytes"));
    }
    Ok(text)
}

fn read_toml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let text = read_text(path)?;
    toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// A path a set names, relative to its folder, refused when it could lead out of it.
fn inside(dir: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    let plain = path
        .components()
        .all(|part| matches!(part, std::path::Component::Normal(_)));
    if relative.is_empty() || !plain {
        return Err(format!(
            "the path {relative:?} is not a plain path inside the set's folder"
        ));
    }
    Ok(dir.join(path))
}

impl Set {
    /// Reads a set from its folder: every file parsed strictly (an unknown key anywhere
    /// is refused, which is how a manifest or a job is kept free of an answer), and the
    /// corrections beside the labels applied.
    pub fn load(dir: &Path) -> Result<Set, String> {
        let manifest: Manifest = read_toml(&dir.join(MANIFEST_FILE))?;
        let id = manifest.set.id.clone();
        let families: Families = read_toml(&inside(dir, &manifest.set.families)?)?;
        let sealed_labels = match &manifest.set.labels {
            Some(relative) => Some(read_toml::<Labels>(&inside(dir, relative)?)?),
            None => None,
        };
        let mut jobs = Vec::new();
        let mut parsed = Vec::new();
        for item in &manifest.item {
            let text = read_text(&inside(dir, &item.job)?)
                .map_err(|e| said(&id, format!("item {}: {e}", item.id)))?;
            let file: JobFile =
                toml::from_str(&text).map_err(|e| said(&id, format!("item {}: {e}", item.id)))?;
            jobs.push((item.id.clone(), text));
            parsed.push(file.job);
        }
        let corrections = dir.join(CORRECTIONS_FILE);
        let labels = match (&sealed_labels, corrections.is_file()) {
            (Some(labels), true) => Some(corrected(&id, labels, &read_toml(&corrections)?)?),
            (None, true) => {
                return Err(said(
                    &id,
                    "a set without a labels file has corrections to no label",
                ));
            }
            (labels, false) => labels.clone(),
        };
        Ok(Set {
            dir: dir.to_path_buf(),
            manifest,
            families,
            sealed_labels,
            labels,
            jobs,
            parsed,
        })
    }

    pub fn id(&self) -> &str {
        &self.manifest.set.id
    }

    /// Every picture of every item, by its pin. A picture with no pin, or one given
    /// inline, is refused: a set's pictures are files fetched by hash.
    pub fn pictures(&self) -> Result<Vec<Picture>, String> {
        let mut out = Vec::new();
        for ((item, _), job) in self.jobs.iter().zip(&self.parsed) {
            for image in &job.images {
                let Some(sha256) = &image.source.sha256 else {
                    return Err(said(
                        self.id(),
                        format!("item {item}: image {} has no sha256 pin", image.id),
                    ));
                };
                let sha256 = sha256.to_ascii_lowercase();
                if Digest::from_hex(&sha256).is_none() {
                    return Err(said(
                        self.id(),
                        format!(
                            "item {item}: image {} has a pin that is no SHA-256",
                            image.id
                        ),
                    ));
                }
                inside(&self.dir, &image.source.path)
                    .map_err(|e| said(self.id(), format!("item {item}: {e}")))?;
                out.push(Picture {
                    item: item.clone(),
                    image: image.id.clone(),
                    path: image.source.path.clone(),
                    sha256,
                });
            }
        }
        Ok(out)
    }

    /// Every rule one set can break by itself: the rules it pins are the ones `check`
    /// ships, at that version and of its use case; its items are unique, of declared
    /// families, of its use case, and are jobs `check` takes; every picture is pinned;
    /// its labels are right for its split; and its seal holds.
    pub fn check(&self, judging: &Judging<'_>) -> Result<Report, String> {
        let id = self.id();
        let meta = &self.manifest.set;
        let rules = rules_text(meta, judging)?;
        let declared: BTreeSet<&str> = self.families.family.iter().map(|f| f.id.as_str()).collect();
        let mut seen = BTreeSet::new();
        for ((item, entry), (_, text)) in
            self.parsed.iter().zip(&self.manifest.item).zip(&self.jobs)
        {
            if !seen.insert(entry.id.as_str()) {
                return Err(said(id, format!("item {} appears twice", entry.id)));
            }
            if !declared.contains(entry.family.as_str()) {
                return Err(said(
                    id,
                    format!(
                        "item {}: family {:?} is not declared",
                        entry.id, entry.family
                    ),
                ));
            }
            if item.use_case != meta.use_case {
                return Err(said(
                    id,
                    format!(
                        "item {}: its job is of another use case than the set",
                        entry.id
                    ),
                ));
            }
            (judging.job)(text).map_err(|e| said(id, format!("item {}: {e}", entry.id)))?;
        }
        let pictures = self.pictures()?;
        match (meta.split, &self.sealed_labels, &meta.labels_sha256) {
            (Split::Blind, Some(_), _) => {
                return Err(said(id, "a blind set carries no labels file"));
            }
            (Split::Blind, None, None) => {
                return Err(said(id, "a blind set must carry labels_sha256"));
            }
            (Split::Blind, None, Some(_)) => {}
            (split, None, _) => {
                return Err(said(id, format!("a {split} set must name its labels file")));
            }
            (_, Some(labels), _) => {
                self.check_labels(labels)?;
                if let Some(corrected) = &self.labels {
                    self.check_labels(corrected)?;
                }
            }
        }
        self.verify_seal(rules)?;
        Ok(Report {
            items: self.jobs.len(),
            pictures: pictures.len(),
        })
    }

    /// Every item labelled once, nothing else labelled, every region of the shape of its
    /// kind.
    pub fn check_labels(&self, labels: &Labels) -> Result<(), String> {
        let id = self.id();
        let items: BTreeSet<&str> = self.jobs.iter().map(|(item, _)| item.as_str()).collect();
        let mut labelled = BTreeSet::new();
        for label in &labels.label {
            if !items.contains(label.item.as_str()) {
                return Err(said(
                    id,
                    format!("a label of {:?}, which is no item", label.item),
                ));
            }
            if !labelled.insert(label.item.as_str()) {
                return Err(said(id, format!("item {} is labelled twice", label.item)));
            }
            for region in &label.regions {
                let shaped = match (region.kind, &region.polygon, &region.bbox) {
                    (RegionKind::Polygon, Some(points), None) => points.len() >= 3,
                    (RegionKind::Bbox, None, Some(_)) => true,
                    (RegionKind::WholeImage | RegionKind::Absent, None, None) => true,
                    _ => false,
                };
                if !shaped {
                    return Err(said(
                        id,
                        format!(
                            "item {}: a region of the wrong shape for its kind",
                            label.item
                        ),
                    ));
                }
            }
        }
        if let Some(missing) = items.iter().find(|item| !labelled.contains(*item)) {
            return Err(said(id, format!("item {missing} has no label")));
        }
        Ok(())
    }

    /// The seal's components as the set stands, the pictures by their pins.
    fn components(&self, rules: &str) -> Result<Components, String> {
        let id = self.id();
        let mut ids: Vec<&str> = self.manifest.item.iter().map(|i| i.id.as_str()).collect();
        ids.sort_unstable();
        let mut assignments: Vec<String> = (self.manifest.item.iter())
            .map(|i| format!("{}={}", i.id, i.family))
            .collect();
        assignments.sort();
        let mut ordered: Vec<(&String, &Job)> = self
            .jobs
            .iter()
            .map(|(item, _)| item)
            .zip(&self.parsed)
            .collect();
        ordered.sort_by(|a, b| a.0.cmp(b.0));
        let mut jobs = String::new();
        for (item, job) in ordered {
            jobs.push_str(item);
            jobs.push('\n');
            jobs.push_str(&serde_json::to_string(job).map_err(|e| said(id, e))?);
            jobs.push('\n');
        }
        let mut lines: Vec<String> = (self.pictures()?.into_iter())
            .map(|picture| format!("{} {}", picture.path, picture.sha256))
            .collect();
        lines.sort();
        lines.dedup();
        let pin = &self.manifest.set.rules;
        let labels = match (&self.manifest.set.labels, &self.manifest.set.labels_sha256) {
            (Some(relative), _) => hex(read_text(&inside(&self.dir, relative)?)?.as_bytes()),
            (None, Some(hash)) => hash.to_ascii_lowercase(),
            (None, None) => return Err(said(id, "no labels file and no labels_sha256")),
        };
        Ok(Components {
            items: hex(ids.join("\n").as_bytes()),
            families: hex(assignments.join("\n").as_bytes()),
            jobs: hex(jobs.as_bytes()),
            images: hex(lines.join("\n").as_bytes()),
            rules: hex(format!("{}@{}\n{rules}", pin.base, pin.version).as_bytes()),
            labels,
        })
    }

    /// The set as it stands gives every component of its seal, and the seal's own digest
    /// is of its components. A seal that does not match is refused, naming what changed.
    fn verify_seal(&self, rules: &str) -> Result<(), String> {
        let id = self.id();
        let sealed: Seal = read_toml(&self.dir.join(SEAL_FILE))?;
        let now = self.components(rules)?;
        let changed = sealed.seal.components.changed(&now);
        if !changed.is_empty() {
            return Err(said(
                id,
                format!(
                    "the seal does not match the set; changed: {}",
                    changed.join(", ")
                ),
            ));
        }
        if sealed.seal.sha256 != now.overall() {
            return Err(said(id, "the seal's sha256 is not of its own components"));
        }
        Ok(())
    }
}

/// The rule file the set pins: shipped, of that version and of the set's use case.
fn rules_text(meta: &SetMeta, judging: &Judging<'_>) -> Result<&'static str, String> {
    #[derive(Deserialize)]
    struct Head {
        rules: HeadRules,
    }
    #[derive(Deserialize)]
    struct HeadRules {
        use_case: String,
        version: u32,
    }
    let pin = &meta.rules;
    let text = (judging.rules)(&pin.base).ok_or_else(|| {
        said(
            &meta.id,
            format!("rules base {:?} is not shipped", pin.base),
        )
    })?;
    // The header alone: a rule file holds more than this reads, and is read whole where
    // it is used.
    let head: toml::Table = toml::from_str(text).map_err(|e| said(&meta.id, e))?;
    let head: Head = head
        .try_into()
        .map_err(|e: toml::de::Error| said(&meta.id, e))?;
    if head.rules.version != pin.version {
        return Err(said(
            &meta.id,
            format!(
                "rules version {} is pinned, and the shipped {} is version {}",
                pin.version, pin.base, head.rules.version
            ),
        ));
    }
    if head.rules.use_case != meta.use_case {
        return Err(said(&meta.id, "the pinned rules are of another use case"));
    }
    Ok(text)
}

/// The labels with each correction applied: one correction an item at most, of an item
/// that is labelled, whose `was` is the label as sealed.
fn corrected(id: &str, labels: &Labels, corrections: &Corrections) -> Result<Labels, String> {
    let mut out = labels.clone();
    let mut seen = BTreeSet::new();
    for correction in &corrections.correction {
        if !seen.insert(correction.item.as_str()) {
            return Err(said(
                id,
                format!("item {} is corrected twice", correction.item),
            ));
        }
        let Some(label) = out.label.iter_mut().find(|l| l.item == correction.item) else {
            return Err(said(
                id,
                format!("a correction of {:?}, which has no label", correction.item),
            ));
        };
        if label.expected != correction.was {
            return Err(said(
                id,
                format!(
                    "the correction of {} says it was {:?}, and the label says {:?}",
                    correction.item, correction.was, label.expected
                ),
            ));
        }
        label.expected = correction.expected;
        label.failure_types = correction.failure_types.clone();
        label.words = correction.words.clone();
        label.obligation_ids = correction.obligation_ids.clone();
        label.adjudicated_by = correction.adjudicated_by.clone();
        label.adjudicated_on = correction.adjudicated_on.clone();
    }
    Ok(out)
}

/// What a set's check found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Report {
    pub items: usize,
    pub pictures: usize,
}

/// What the check of a tree found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TreeReport {
    pub sets: usize,
    pub items: usize,
    pub pictures: usize,
}

/// The folder of every set under a root, `<use-case>/<set>/set.toml`, in order.
pub fn discover(root: &Path) -> Result<Vec<PathBuf>, String> {
    let folders = |dir: &Path| -> Result<Vec<PathBuf>, String> {
        let read =
            std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        let mut dirs: Vec<PathBuf> = read
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.is_dir())
            .collect();
        dirs.sort();
        Ok(dirs)
    };
    let mut found = Vec::new();
    for use_case in folders(root)? {
        for set in folders(&use_case)? {
            if set.join(MANIFEST_FILE).is_file() {
                found.push(set);
            }
        }
    }
    if found.is_empty() {
        return Err(format!(
            "no set under {} (each is <use-case>/<set>/set.toml)",
            root.display()
        ));
    }
    Ok(found)
}

/// `whizbang eval sets check`: every set under the root passes its own check and lives at
/// its id's path, and across sets a family of one use case is in one split only.
pub fn check(root: &Path, judging: &Judging<'_>) -> Result<TreeReport, String> {
    let mut report = TreeReport::default();
    let mut families: BTreeMap<(String, String), (Split, String)> = BTreeMap::new();
    for dir in discover(root)? {
        let set = Set::load(&dir)?;
        let one = set.check(judging)?;
        let meta = &set.manifest.set;
        if root.join(&meta.id) != dir {
            return Err(said(&meta.id, "the set does not live at its id's path"));
        }
        for item in &set.manifest.item {
            let key = (meta.use_case.clone(), item.family.clone());
            match families.get(&key) {
                Some((split, other)) if *split != meta.split => {
                    return Err(said(
                        &meta.id,
                        format!(
                            "family {:?} is in the {} split here and in the {split} split of {other}",
                            item.family, meta.split
                        ),
                    ));
                }
                _ => {
                    families.insert(key, (meta.split, meta.id.clone()));
                }
            }
        }
        report.sets += 1;
        report.items += one.items;
        report.pictures += one.pictures;
    }
    Ok(report)
}

/// The set of this id under the root, read: `<root>/<id>`, and the id is two plain names.
pub fn find(root: &Path, id: &str) -> Result<Set, String> {
    let parts: Vec<&str> = id.split('/').collect();
    let plain = |part: &&str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
            && *part != "."
            && *part != ".."
    };
    if parts.len() != 2 || !parts.iter().all(plain) {
        return Err("a set's id is <use-case>/<set>, two plain names".to_owned());
    }
    let set = Set::load(&root.join(id))?;
    if set.id() != id {
        return Err(said(id, "the set at that path has another id"));
    }
    Ok(set)
}
