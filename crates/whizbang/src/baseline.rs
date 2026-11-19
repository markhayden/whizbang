//! `whizbang eval baseline` (plan 2C, task E2, item 7): the baseline gate. Each recorded
//! decision of a baseline package is replayed ([`whizbang_squint::replay`]): squint's loop
//! on whizbang's parts, with a judge that answers the recorded turns and each picture
//! prepared for the decision's model. What the replay did is written as a transcript
//! (`docs/baseline/transcript.md`) and compared with the recorded one by
//! [`whizbang_eval::baseline::compare`]. The receipt holds the four fields of
//! `docs/baseline/README.md` and nothing else; what is printed is counts, and the places
//! where replays differed, as paths of the format with every position a `*`, never a value
//! or an item.
//!
//! What a replay reads, all of it checked against what was recorded: the package's listing
//! (`PACKAGE.sha256`), which must hash to its digest (`PACKAGE.digest`), and every file read
//! from the package must be the bytes the listing names; each recorded transcript, which must
//! name that package; the decision's header (`decision.json`: its budget and output limit);
//! the model's preparation (`assembly/generated/preparation.json`); the job, from the set's
//! folder; and each of the job's pictures, by the SHA-256 the job pins: beside the job, in a
//! folder of pictures laid out as the sets' repository is, or anywhere in the package.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::{DynamicImage, ImageReader};
use serde_json::{Value as Json, json};
use whizbang_core::identity::{DecodeLimits, Digest, identify};
use whizbang_eval::baseline::{COMPARISON_VERSION, VOLATILE, compare};
use whizbang_squint::details::Projection;
use whizbang_squint::job::Job;
use whizbang_squint::replay::{self, Budget, Judge, Shown};

/// The files of a transcripts folder that are not transcripts.
const NOT_TRANSCRIPTS: [&str; 3] = [
    "TRANSCRIPTS.sha256",
    "TRANSCRIPTS.digest",
    "audit-findings.jsonl",
];

/// How the 2026-11 package's `assembly/generated/preparation.json` names squint's resize, for
/// each of its seven models: Lanczos3, never larger, which is how the core prepares
/// (`whizbang_core::egress`, `Piece::prepared`). Any other spelling is refused: it would name
/// a preparation nobody checked against whizbang's.
pub const RESIZE: &str = "Lanczos3, never upscaled";

/// The largest file read from a package or a transcripts folder.
const FILE_LIMIT: u64 = 256 * 1024 * 1024;

/// What the gate is asked to do.
pub struct Asked<'a> {
    pub package: &'a Path,
    pub transcripts: &'a Path,
    pub receipt: &'a Path,
    /// The folder of the evaluation sets the jobs are read from.
    pub sets: &'a Path,
    /// A folder of the sets' pictures, laid out as their repository is
    /// (`<folder>/<set id>/<path>`).
    pub pictures: Option<&'a Path>,
    /// The whizbang commit that is replayed: forty lowercase hexadecimal digits.
    pub commit: &'a str,
}

/// What the gate came to.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Tally {
    /// The package's fixtures: the old results its replay folder holds.
    pub fixtures: usize,
    /// The version of the format every transcript holds, and every replay is written in:
    /// 2 compares a scaled tool image by its size and mapping (`docs/baseline/transcript.md`).
    pub version: Option<u64>,
    pub transcripts: usize,
    pub same: usize,
    pub differed: usize,
    /// Decisions that could not be replayed or compared: a picture or a file missing, a
    /// job this cannot replay, a transcript the comparison refuses.
    pub refused: usize,
    /// The places replays differed at, generalised, with how many differences each.
    pub places: BTreeMap<String, usize>,
    /// Why decisions were refused, in fixed words, with how many each.
    pub reasons: BTreeMap<String, usize>,
    /// What a difference was, in words fixed here and values from closed lists, with how
    /// many each: the breakdown that makes a failing gate diagnosable without showing a
    /// transcript ([`Breakdown`]).
    pub breakdown: Breakdown,
}

/// The differences of the replays broken down, every label made of fixed words and values
/// from closed lists: a tool's name, a disposition, an ending, a model and a use case, or
/// `other`. Nothing the judge wrote and nothing of an item is ever a label.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Breakdown {
    /// Decisions that differed only where squint scaled a cut down, and those that differed
    /// anywhere else.
    pub only_scaled: usize,
    pub elsewhere: usize,
    /// Per model and per use case: same, then differed.
    pub models: BTreeMap<String, (usize, usize)>,
    pub use_cases: BTreeMap<String, (usize, usize)>,
    /// Of decisions whose ending differs: the recorded ending, then the replayed one.
    pub endings: BTreeMap<String, usize>,
    /// Of calls whose disposition differs, by tool: recorded, then replayed.
    pub dispositions: BTreeMap<String, usize>,
    /// Of calls whose result's text differs, by tool and what the recorded call came to.
    pub results: BTreeMap<String, usize>,
    /// The place of each differing decision's first difference.
    pub first: BTreeMap<String, usize>,
}

/// A value from a closed list, or `other`.
fn closed<'a>(value: &'a Json, list: &[&'a str]) -> &'a str {
    value
        .as_str()
        .and_then(|v| list.iter().find(|l| **l == v).copied())
        .unwrap_or("other")
}

const TOOLS: [&str; 3] = ["view", "align", "submit_verdict"];
const DISPOSITIONS: [&str; 4] = ["executed", "rejected", "excluded", "final"];
/// Squint's error classes (`crates/squint-core/src/lib.rs`, `Error::class`).
const CLASSES: [&str; 15] = [
    "resolve",
    "provider_rejected",
    "provider",
    "tool",
    "budget",
    "cap",
    "stale_estimate",
    "input",
    "cancelled",
    "image_limit",
    "credential",
    "submission_invalid",
    "no_submission",
    "model_output",
    "io",
];

/// A decision's ending as a label: its kind, then its verdict or its class.
fn ending(terminal: &Json) -> String {
    let kind = closed(&terminal["kind"], &["verdict", "error", "cancelled"]);
    let what = match kind {
        "verdict" => closed(&terminal["verdict"], &["pass", "fail", "unsure"]),
        "other" => "other",
        _ => closed(&terminal["class"], &CLASSES),
    };
    format!("{kind} {what}")
}

/// A model's or a use case's name as a label: its own characters when they are plain.
fn plain(value: &Json) -> String {
    match value.as_str() {
        Some(v)
            if !v.is_empty()
                && v.len() <= 64
                && v.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.:".contains(c)) =>
        {
            v.to_owned()
        }
        _ => "other".to_owned(),
    }
}

/// What kind of refusal squint's harness answered a call with, from the fixed opening words
/// its loop writes before anything else (`crates/squint-core/src/run.rs`): never the words
/// that follow them.
fn refusal_kind(content: &str) -> &'static str {
    let schema = [
        "missing required argument ",
        "unknown argument ",
        "arguments must be an object",
    ];
    if let Some(rest) = content.strip_prefix("invalid arguments: ") {
        if schema.iter().any(|s| rest.starts_with(s)) {
            return "schema";
        }
        let named = [
            "frame: unknown frame id ",
            "also: unknown frame id ",
            "image: unknown image id ",
            "against: unknown image id ",
            "unknown tool ",
        ];
        if named.iter().any(|s| rest.starts_with(s)) {
            return "unknown frame";
        }
        return "tool";
    }
    if content.starts_with("Not executed: this batch of ") {
        return "budget";
    }
    if content.starts_with("Submission rejected: ") {
        return "submission";
    }
    "other"
}

/// The calls of a transcript by turn and position.
fn calls(t: &Json) -> BTreeMap<(usize, usize), Json> {
    let mut out = BTreeMap::new();
    for (at, turn) in t["turns"].as_array().into_iter().flatten().enumerate() {
        for (n, call) in turn["calls"].as_array().into_iter().flatten().enumerate() {
            out.insert((at, n), call.clone());
        }
    }
    out
}

impl Breakdown {
    fn same(&mut self, recorded: &Json) {
        self.models
            .entry(plain(&recorded["subject"]["model"]))
            .or_default()
            .0 += 1;
        self.use_cases
            .entry(plain(&recorded["subject"]["use_case"]))
            .or_default()
            .0 += 1;
    }

    fn differed(
        &mut self,
        recorded_text: &str,
        recorded: &Json,
        replayed: &Json,
        paths: &[String],
    ) {
        self.models
            .entry(plain(&recorded["subject"]["model"]))
            .or_default()
            .1 += 1;
        self.use_cases
            .entry(plain(&recorded["subject"]["use_case"]))
            .or_default()
            .1 += 1;
        let scaled = whizbang_eval::baseline::scaled_places(recorded_text);
        let at_scaled = |path: &String| {
            scaled
                .iter()
                .any(|s| path == s || path.starts_with(&format!("{s}/")))
        };
        if paths.iter().all(at_scaled) {
            self.only_scaled += 1;
        } else {
            self.elsewhere += 1;
        }
        if let Some(first) = paths.first() {
            *self.first.entry(general(first)).or_default() += 1;
        }
        if recorded["terminal"] != replayed["terminal"] {
            let label = format!(
                "{} -> {}",
                ending(&recorded["terminal"]),
                ending(&replayed["terminal"])
            );
            *self.endings.entry(label).or_default() += 1;
        }
        let theirs = calls(replayed);
        for (at, call) in calls(recorded) {
            let tool = closed(&call["name"], &TOOLS);
            let was = closed(&call["disposition"], &DISPOSITIONS);
            let other = theirs.get(&at);
            let now = other.map_or("missing", |c| closed(&c["disposition"], &DISPOSITIONS));
            if other.map(|c| &c["disposition"]) != Some(&call["disposition"]) {
                *self
                    .dispositions
                    .entry(format!("{tool} {was} -> {now}"))
                    .or_default() += 1;
            }
            let content = call["result"]["content"].as_str();
            if content.is_some() && other.map(|c| c["result"]["content"].as_str()) != Some(content)
            {
                let how = match was {
                    "rejected" => {
                        format!("rejected, {}", refusal_kind(content.unwrap_or_default()))
                    }
                    other => other.to_owned(),
                };
                *self.results.entry(format!("{tool} {how}")).or_default() += 1;
            }
        }
    }

    fn text(&self) -> String {
        let mut out = String::new();
        if self.only_scaled + self.elsewhere > 0 {
            out.push_str(&format!(
                "differed only at scaled cuts: {}\ndiffered elsewhere: {}\n",
                self.only_scaled, self.elsewhere
            ));
        }
        for (model, (same, differed)) in &self.models {
            out.push_str(&format!(
                "model {model}: same {same}, differed {differed}\n"
            ));
        }
        for (use_case, (same, differed)) in &self.use_cases {
            out.push_str(&format!(
                "use case {use_case}: same {same}, differed {differed}\n"
            ));
        }
        let tables = [
            ("ending", &self.endings),
            ("disposition", &self.dispositions),
            ("result text of", &self.results),
            ("first difference at", &self.first),
        ];
        for (label, table) in tables {
            for (key, count) in table {
                out.push_str(&format!("{label} {key}: {count}\n"));
            }
        }
        out
    }
}

impl Tally {
    pub fn passed(&self) -> bool {
        self.transcripts > 0
            && self.transcripts == self.fixtures
            && self.same == self.transcripts
            && self.differed == 0
            && self.refused == 0
    }

    /// Counts only.
    pub fn text(&self) -> String {
        let version = self.version.map_or("none".to_owned(), |v| v.to_string());
        let mut out = format!(
            "fixtures: {}\ntranscripts: {}\ntranscript version: {version}\nsame: {}\n\
             differed: {}\nrefused: {}\n",
            self.fixtures, self.transcripts, self.same, self.differed, self.refused
        );
        for (reason, count) in &self.reasons {
            out.push_str(&format!("refused, {reason}: {count}\n"));
        }
        for (place, count) in &self.places {
            out.push_str(&format!("differences at {place}: {count}\n"));
        }
        out.push_str(&self.breakdown.text());
        out.push_str(&format!(
            "result: {}\n",
            if self.passed() { "pass" } else { "fail" }
        ));
        out
    }
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|_| "a file cannot be read".to_owned())?;
    let mut bytes = Vec::new();
    file.take(FILE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "a file cannot be read".to_owned())?;
    if bytes.len() as u64 > FILE_LIMIT {
        return Err("a file is too large".to_owned());
    }
    Ok(bytes)
}

/// The package: its digest and its listing, path to SHA-256.
struct Package<'a> {
    root: &'a Path,
    digest: String,
    listed: BTreeMap<String, String>,
}

impl<'a> Package<'a> {
    fn open(root: &'a Path) -> Result<Package<'a>, String> {
        let digest = String::from_utf8(read(&root.join("PACKAGE.digest"))?)
            .map_err(|_| "PACKAGE.digest is not text".to_owned())?;
        let digest = digest.trim_end_matches('\n');
        let listing = read(&root.join("PACKAGE.sha256"))?;
        if Digest::of_bytes(&listing).hex() != digest {
            return Err("the package's listing does not hash to its digest".to_owned());
        }
        let text = String::from_utf8(listing).map_err(|_| "the listing is not text".to_owned())?;
        let mut listed = BTreeMap::new();
        for line in text.lines() {
            let Some((sha, path)) = line.split_once("  ") else {
                return Err("the package's listing has a line that is not a file".to_owned());
            };
            listed.insert(path.to_owned(), sha.to_owned());
        }
        Ok(Package {
            root,
            digest: format!("sha256:{digest}"),
            listed,
        })
    }

    /// A file of the package, which must be the bytes the listing names.
    fn file(&self, path: &str) -> Result<Vec<u8>, String> {
        let Some(sha) = self.listed.get(path) else {
            return Err("a file the replay needs is not in the package".to_owned());
        };
        let bytes = read(&self.root.join(path))?;
        if &Digest::of_bytes(&bytes).hex() != sha {
            return Err("a file of the package is not the bytes its listing names".to_owned());
        }
        Ok(bytes)
    }

    fn json(&self, path: &str) -> Result<Json, String> {
        serde_json::from_slice(&self.file(path)?)
            .map_err(|_| "a file of the package is not JSON".to_owned())
    }

    /// Any file of the package whose SHA-256 is this one.
    fn by_sha(&self, sha: &str) -> Option<Vec<u8>> {
        let (path, _) = self.listed.iter().find(|(_, s)| *s == sha)?;
        self.file(path).ok()
    }

    /// The old results: one fixture each.
    fn fixtures(&self) -> usize {
        self.listed
            .keys()
            .filter(|p| p.starts_with("replay/") && p.ends_with("/result.json"))
            .count()
    }
}

/// Squint's digest of a picture's samples as decoded, before its orientation is applied
/// (`crates/squint-core/src/contract.rs`, `pixel_sha256`).
fn pixel_sha256(bytes: &[u8]) -> Result<String, String> {
    let decoder = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| "a picture cannot be read".to_owned())?
        .into_decoder()
        .map_err(|_| "a picture cannot be read".to_owned())?;
    let decoded = DynamicImage::from_decoder(decoder)
        .map_err(|_| "a picture cannot be decoded".to_owned())?;
    let canonical = match decoded {
        DynamicImage::ImageLuma8(_) | DynamicImage::ImageRgb8(_) | DynamicImage::ImageRgba8(_) => {
            decoded
        }
        other => DynamicImage::ImageRgb8(other.to_rgb8()),
    };
    let mut hashed = b"pixels\0".to_vec();
    hashed.extend(canonical.width().to_le_bytes());
    hashed.extend(canonical.height().to_le_bytes());
    hashed.extend(u32::from(canonical.color().channel_count()).to_le_bytes());
    hashed.extend(canonical.as_bytes());
    Ok(Digest::of_bytes(&hashed).hex())
}

/// The judge of a replay: the recorded responses, turn by turn.
struct Recorded<'a> {
    turns: &'a [Json],
    /// The recorded ending's message, if it has one.
    ending: &'a Json,
}

impl Judge for Recorded<'_> {
    fn answer(&mut self, turn: usize, _request: &Json) -> Option<Json> {
        let recorded = self.turns.get(turn.checked_sub(1)?)?;
        recorded.get("response").filter(|r| !r.is_null()).cloned()
    }

    fn said(&self) -> Vec<String> {
        let mut said: Vec<String> = self
            .turns
            .iter()
            .flat_map(|turn| turn["calls"].as_array().cloned().unwrap_or_default())
            .filter_map(|call| call["result"]["content"].as_str().map(str::to_owned))
            .collect();
        said.extend(self.ending.as_str().map(str::to_owned));
        said
    }
}

/// A place of a transcript as it is counted: each position in a list a `*`, and nothing
/// below a place whose keys are not the format's (a call's arguments, a tool's schema, an
/// adapter's block, a mapping, the preparation's settings).
fn general(path: &str) -> String {
    let mut out = String::new();
    for segment in path.split('/').filter(|s| !s.is_empty()) {
        let shown = if segment.chars().all(|c| c.is_ascii_digit()) {
            "*"
        } else {
            segment
        };
        out.push('/');
        out.push_str(shown);
        if ["arguments", "input_schema", "block", "mapping", "settings"].contains(&segment) {
            break;
        }
    }
    if out.is_empty() { "/".to_owned() } else { out }
}

/// One recorded decision replayed: the replayed transcript's text, or why there is none.
fn replayed(asked: &Asked<'_>, package: &Package<'_>, recorded: &Json) -> Result<String, String> {
    let named = recorded["meta"]["package_digest"].as_str();
    if named != Some(package.digest.as_str()) {
        return Err("a transcript names another package".to_owned());
    }
    let subject = &recorded["subject"];
    let text = |key: &str| {
        subject[key]
            .as_str()
            .ok_or_else(|| format!("the subject has no {key}"))
    };
    if text("arm")? != "C" {
        return Err("only arm C is replayed".to_owned());
    }
    let (set, item, model) = (text("set")?, text("item")?, text("model")?);
    // The decision's header: the budget it ran under.
    let decision = recorded["meta"]["decision"]
        .as_str()
        .ok_or("a transcript names no decision")?;
    let header = package.json(&format!("{decision}/decision.json"))?;
    let defaults = Budget::default();
    let number = |value: &Json, default: u32| {
        value
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(default)
    };
    let budget_of = &header["config"]["budget"];
    let budget = Budget {
        tool_turns: number(&budget_of["tool_turns"], defaults.tool_turns),
        tool_calls: number(&budget_of["tool_calls"], defaults.tool_calls),
        repairs: number(&budget_of["repairs"], defaults.repairs),
        repair_output_tokens: number(
            &budget_of["repair_output_tokens"],
            defaults.repair_output_tokens,
        ),
        max_output_tokens: number(
            &header["config"]["provider"]["max_output_tokens"],
            defaults.max_output_tokens,
        ),
    };
    // The model's preparation, as the package derived it.
    let preparation = package.json("assembly/generated/preparation.json")?;
    let short = model.rsplit(':').next().unwrap_or(model);
    let settings = preparation["settings"][short].clone();
    let long_edge = settings["long_edge_px"]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .ok_or("the model's preparation names no long edge")?;
    if settings["jpeg_quality"].as_u64() != Some(u64::from(whizbang_core::egress::JPEG_QUALITY)) {
        return Err("the model's preparation is not JPEG at whizbang's quality".to_owned());
    }
    if settings["resize"].as_str() != Some(RESIZE) {
        return Err("the model's preparation does not resize as whizbang does".to_owned());
    }
    let version = settings["prep_version"]
        .as_str()
        .ok_or("the model's preparation names no version")?;
    // The job, and its pictures by the hashes it pins.
    let set_dir = asked.sets.join(set);
    let job_text = read(&set_dir.join("items").join(item).join("job.toml"))?;
    let job = Job::from_toml(&String::from_utf8_lossy(&job_text))
        .map_err(|_| "a job cannot be read".to_owned())?;
    let mut pictures = Vec::new();
    for image in &job.images {
        let source = image
            .source
            .as_ref()
            .ok_or("a job's picture is not a file")?;
        let (Some(path), Some(sha)) = (source.path.as_deref(), source.sha256.as_deref()) else {
            return Err("a job's picture is not pinned".to_owned());
        };
        let candidates: Vec<PathBuf> = std::iter::once(set_dir.join(path))
            .chain(asked.pictures.map(|p| p.join(set).join(path)))
            .collect();
        let bytes = candidates
            .iter()
            .filter_map(|p| read(p).ok())
            .find(|b| Digest::of_bytes(b).hex() == sha)
            .or_else(|| package.by_sha(sha))
            .ok_or("a picture of a job was not found")?;
        let identified = identify(&bytes, &DecodeLimits::default())
            .map_err(|_| "a picture cannot be decoded".to_owned())?;
        pictures.push(Shown {
            pixels: identified.pixels,
            sha256: sha.to_owned(),
            pixel_sha256: pixel_sha256(&bytes)?,
        });
    }
    let turns = recorded["turns"].as_array().cloned().unwrap_or_default();
    let mut judge = Recorded {
        turns: &turns,
        ending: &recorded["terminal"]["message"],
    };
    let done = replay::replay(
        &job,
        &pictures,
        &budget,
        long_edge,
        Projection::VisualOnly,
        &mut judge,
    )
    .map_err(|_| "a job this cannot replay".to_owned())?;
    let transcript = json!({
        "transcript_version": recorded["transcript_version"],
        "origin": "replayed",
        "complete": true,
        "incomplete": [],
        "subject": subject,
        "preparation": {"version": version, "settings": settings},
        "evidence_tools": done.evidence_tools,
        "submission_tool": done.submission_tool,
        "images": done.images,
        "turns": done.turns,
        "repairs": done.repairs,
        "terminal": done.terminal,
        "volatile": VOLATILE,
        "files": [],
        "meta": {"harness": "whizbang", "harness_version": env!("CARGO_PKG_VERSION")},
    });
    Ok(transcript.to_string())
}

/// The recorded transcripts under a folder, by path, sorted.
fn transcripts(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut found = Vec::new();
    let mut folders = vec![root.to_path_buf()];
    // Folders are walked one at a time; a link is not followed.
    while let Some(folder) = folders.pop() {
        let entries = std::fs::read_dir(&folder)
            .map_err(|_| "the transcripts folder cannot be read".to_owned())?;
        for entry in entries {
            let entry = entry.map_err(|_| "the transcripts folder cannot be read".to_owned())?;
            let kind = entry
                .file_type()
                .map_err(|_| "the transcripts folder cannot be read".to_owned())?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if kind.is_dir() {
                folders.push(path);
            } else if kind.is_file()
                && name.ends_with(".json")
                && !NOT_TRANSCRIPTS.contains(&name.as_str())
            {
                found.push(path);
            }
        }
    }
    found.sort();
    Ok(found)
}

/// The receipt of `docs/baseline/README.md`: exactly its four fields.
pub fn receipt(package_digest: &str, commit: &str, passed: bool) -> String {
    format!(
        "package_digest = \"{package_digest}\"\ncomparison_version = \"{COMPARISON_VERSION}\"\n\
         whizbang_commit = \"{commit}\"\nresult = \"{}\"\n",
        if passed { "pass" } else { "fail" }
    )
}

/// Runs the gate: replays and compares every recorded decision, writes the receipt, and
/// says what it came to in counts.
pub fn run(asked: &Asked<'_>) -> Result<Tally, String> {
    let hex = |c: char| c.is_ascii_digit() || ('a'..='f').contains(&c);
    if asked.commit.len() != 40 || !asked.commit.chars().all(hex) {
        return Err(
            "--commit is the full commit replayed: forty lowercase hexadecimal digits".to_owned(),
        );
    }
    let package = Package::open(asked.package)?;
    let mut tally = Tally {
        fixtures: package.fixtures(),
        ..Tally::default()
    };
    let found = transcripts(asked.transcripts)?;
    // One version of the format for the whole folder: a pair of two versions is never
    // compared, so a folder of two is refused before anything is replayed.
    let mut versions = std::collections::BTreeSet::new();
    for path in &found {
        let read_version = read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Json>(&bytes).ok())
            .map(|t| t["transcript_version"].clone());
        if let Some(version) = read_version {
            versions.insert(version.to_string());
        }
    }
    if versions.len() > 1 {
        return Err(
            "the transcripts are not all of one version of the format, and a pair of two \
             versions is never compared"
                .to_owned(),
        );
    }
    tally.version = versions.first().and_then(|v| v.parse().ok());
    for path in found {
        tally.transcripts += 1;
        let refuse = |tally: &mut Tally, why: String| {
            tally.refused += 1;
            *tally.reasons.entry(why).or_default() += 1;
        };
        let recorded_text = match read(&path).map(String::from_utf8) {
            Ok(Ok(text)) => text,
            _ => {
                refuse(&mut tally, "a transcript cannot be read".to_owned());
                continue;
            }
        };
        let Ok(recorded) = serde_json::from_str::<Json>(&recorded_text) else {
            refuse(&mut tally, "a transcript is not JSON".to_owned());
            continue;
        };
        let replayed = match replayed(asked, &package, &recorded) {
            Ok(text) => text,
            Err(why) => {
                refuse(&mut tally, why);
                continue;
            }
        };
        match compare(&recorded_text, &replayed) {
            Ok(differences) if differences.is_empty() => {
                tally.same += 1;
                tally.breakdown.same(&recorded);
            }
            Ok(differences) => {
                tally.differed += 1;
                let paths: Vec<String> = differences.into_iter().map(|d| d.path).collect();
                for path in &paths {
                    *tally.places.entry(general(path)).or_default() += 1;
                }
                let replayed: Json = serde_json::from_str(&replayed).unwrap_or(Json::Null);
                (tally.breakdown).differed(&recorded_text, &recorded, &replayed, &paths);
            }
            Err(refusal) => refuse(&mut tally, refusal.to_string()),
        }
    }
    let written = receipt(&package.digest, asked.commit, tally.passed());
    std::fs::write(asked.receipt, written)
        .map_err(|_| "the receipt cannot be written".to_owned())?;
    Ok(tally)
}
