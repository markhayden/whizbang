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
        out.push_str(&format!(
            "result: {}\n",
            if self.passed() { "pass" } else { "fail" }
        ));
        out
    }
}

pub(crate) fn read(path: &Path) -> Result<Vec<u8>, String> {
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
pub(crate) struct Package<'a> {
    root: &'a Path,
    pub(crate) digest: String,
    pub(crate) listed: BTreeMap<String, String>,
}

impl<'a> Package<'a> {
    pub(crate) fn open(root: &'a Path) -> Result<Package<'a>, String> {
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

    pub(crate) fn json(&self, path: &str) -> Result<Json, String> {
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
struct Recorded<'a>(&'a [Json]);

impl Judge for Recorded<'_> {
    fn answer(&mut self, turn: usize, _request: &Json) -> Option<Json> {
        let recorded = self.0.get(turn.checked_sub(1)?)?;
        recorded.get("response").filter(|r| !r.is_null()).cloned()
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
    let mut judge = Recorded(&turns);
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
            Ok(differences) if differences.is_empty() => tally.same += 1,
            Ok(differences) => {
                tally.differed += 1;
                for difference in differences {
                    *tally.places.entry(general(&difference.path)).or_default() += 1;
                }
            }
            Err(refusal) => refuse(&mut tally, refusal.to_string()),
        }
    }
    let written = receipt(&package.digest, asked.commit, tally.passed());
    std::fs::write(asked.receipt, written)
        .map_err(|_| "the receipt cannot be written".to_owned())?;
    Ok(tally)
}
