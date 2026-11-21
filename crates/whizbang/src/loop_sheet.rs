//! `whizbang eval loop sheet` (plan e1, task L1, item 6): the contact sheet of the loop's
//! round on segment-01, one static page for a person to mark, blind to what the loop and
//! the judge said.
//!
//! For every completed fill of every run of the manifest's request ids under the judge
//! named ([`crate::loop_score::runs_of`]): the picture the loop was given, read from the
//! folder of the pictures by the pin its entry holds, and the filled picture, from the
//! store, each re-encoded from its pixels. The order is drawn from a seed drawn from the
//! operating system's entropy for each sheet ([`fresh_seed`]) and written only in
//! `assignment.json`, so no one can work the order out again without it. On the page
//! each picture is shown with the phrase of what was to be removed and two marks a person
//! may set, "the named thing is still visible" and "something else was damaged", and a
//! third, "looked at", which either of the two sets. Nothing else: no verdict, no
//! coverage, no attempt number, no run and no judge. A picture goes by a key
//! ([`crate::loop_score::key`]) that names neither its run nor its attempt.
//!
//! Written in the folder `--out` names, and nowhere else: refused inside a git work tree,
//! since the fills are new pictures that are never committed, and refused when the folder
//! is there and not empty, so no sheet is written over another:
//!
//! - `index.html`: the page, built as the pairs page is ([`crate::pairs_page`]): a policy
//!   that lets nothing load but its own pictures, and no script or style but its own, by
//!   digest; every string escaped for where it goes; its script sets text, never markup.
//!   The marks leave it only as `marks.json`, a file the person saves through the
//!   browser, holding the sheet's id ([`loop_score::sheet_id`]) and the pictures looked at
//!   by key; one saved can be opened again to resume. `eval loop score --labels` reads it
//!   with the assignment.
//! - `pictures/<nnn>-source.png` and `<nnn>-edited.png`, named by their place on the page.
//! - `assignment.json`: the seed, and each picture's place, key, item, run and attempt.
//!   The page never reads it, and the person does not open it.
//!
//! What an inspecting person could still find: `assignment.json` beside the page; and
//! that a picture whose phrase and original appear more than once was a run that made
//! more than one fill.

use std::path::Path;
use std::sync::Arc;

use whizbang_core::artifact::{self, Artifact, ArtifactId, ArtifactType};
use whizbang_core::identity::{CanonicalPixels, DecodeLimits, identify};
use whizbang_core::store::Reader;
use whizbang_core::store::fs::RealFs;
use whizbang_eval::lab::digest_of;

use crate::loop_score::{self, Manifest};
use crate::pairs_page::{escape, folder, json_in_html, png, policy_for, write_new};

const TEMPLATE: &str = include_str!("../pages/loop_sheet.html");

/// A seed for one sheet, from the operating system's entropy (`/dev/urandom`).
pub fn fresh_seed() -> Result<u64, String> {
    use std::io::Read;
    let mut bytes = [0u8; 8];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|e| format!("no seed could be drawn for the sheet: {}", e.kind()))?;
    Ok(u64::from_le_bytes(bytes))
}

/// What `eval loop sheet` is given.
pub struct Asked<'a> {
    pub manifest: &'a Path,
    /// The folder of the pictures, laid out as their repository is.
    pub set: &'a Path,
    pub root: &'a Path,
    pub judge: &'a str,
    pub out: &'a Path,
}

/// One picture as the page shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnSheet {
    /// `001`, the start of its two files' names.
    pub file: String,
    pub key: String,
    pub phrase: String,
}

/// The page's policy: as the pairs page's, of this page's own script and style.
pub fn policy() -> String {
    policy_for(TEMPLATE)
}

/// The page, from its summary, the sheet's id and its pictures in their order.
pub fn page(summary: &str, sheet: &str, pictures: &[OnSheet]) -> String {
    let data = serde_json::json!({
        "sheet": sheet,
        "pictures": pictures.iter().map(|p| serde_json::json!({
            "file": p.file, "key": p.key, "phrase": p.phrase,
        })).collect::<Vec<_>>(),
    });
    TEMPLATE
        .replace("{{CSP}}", &escape(&policy()))
        .replace("{{SUMMARY}}", &escape(summary))
        .replace("{{DATA}}", &json_in_html(&data))
}

/// A number from a state, as SplitMix64 draws it.
fn next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The places `0..n` in an order drawn from the seed (Fisher and Yates).
pub fn order(n: usize, seed: u64) -> Vec<usize> {
    let mut state = seed;
    let mut out: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        let j = (next(&mut state) % (i as u64 + 1)) as usize;
        out.swap(i, j);
    }
    out
}

/// What was written: how many pictures, and the bytes of the page and its files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub pictures: usize,
    pub bytes: u64,
}

impl Written {
    /// What the command prints: counts only, no path.
    pub fn text(&self) -> String {
        format!(
            "pictures: {}\nwritten: index.html, assignment.json and pictures/, {} bytes, in the \
             folder of --out\n",
            self.pictures, self.bytes
        )
    }
}

fn pixels_of(reader: &Reader, id: ArtifactId) -> Result<CanonicalPixels, String> {
    let limits = DecodeLimits::default();
    match artifact::get_anywhere(reader, ArtifactType::Image, id, &limits) {
        Ok(Artifact::Image(image)) => image.pixels(&limits).map_err(|e| e.to_string()),
        _ => Err("a fill of the loop is not an image the store holds".to_owned()),
    }
}

/// Writes the contact sheet (the module's description says what). Refused, with nothing
/// written, when `--out` is inside a git work tree.
pub fn sheet(asked: &Asked<'_>) -> Result<Written, String> {
    if crate::edits::inside_a_work_tree(asked.out) {
        return Err(
            "the contact sheet's folder is inside a git work tree; its pictures are never \
             committed, so name a folder outside it; nothing was written"
                .to_owned(),
        );
    }
    let occupied = std::fs::read_dir(asked.out).is_ok_and(|mut e| e.next().is_some());
    if occupied || asked.out.is_file() {
        return Err(
            "the contact sheet's folder is there and not empty; name a new or empty folder; \
             nothing was written"
                .to_owned(),
        );
    }
    let (manifest, _): (Manifest, String) = Manifest::read(asked.manifest)?;
    let runs = loop_score::runs_of(asked.root, &manifest, asked.judge)?;
    let mut fills = Vec::new();
    for run in &runs {
        for attempt in &run.attempts {
            if let Some(image) = attempt.filled {
                fills.push((run, attempt.number, image));
            }
        }
    }
    if fills.is_empty() {
        return Err("the store holds no completed fill of these runs; nothing was written".into());
    }
    let reader = Reader::open(asked.root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let seed = fresh_seed()?;
    folder(asked.out)?;
    folder(&asked.out.join("pictures"))?;
    let mut bytes = 0;
    let mut on_sheet = Vec::new();
    let mut assigned = Vec::new();
    for (place, at) in order(fills.len(), seed).into_iter().enumerate() {
        let (run, attempt, image) = fills[at];
        let entry = (manifest.item.iter())
            .find(|e| e.id == run.item)
            .ok_or("a run is of no entry")?;
        let file = format!("{:03}", place + 1);
        let source = manifest.pinned(asked.set, &entry.image, &entry.image_sha256)?;
        let source = identify(&source, &DecodeLimits::default())
            .map_err(|e| e.to_string())?
            .pixels;
        let edited = pixels_of(&reader, image)?;
        let pictures = asked.out.join("pictures");
        for (side, pixels) in [("source", &source), ("edited", &edited)] {
            let encoded = png(pixels.rgba(), pixels.width(), pixels.height())?;
            bytes += write_new(&pictures.join(format!("{file}-{side}.png")), &encoded)?;
        }
        let key = loop_score::key(seed, &run.run, attempt);
        assigned.push(loop_score::Assigned {
            picture: file.clone(),
            key: key.clone(),
            item: run.item.clone(),
            run: run.run.clone(),
            attempt,
        });
        on_sheet.push(OnSheet {
            file,
            key,
            phrase: entry.phrase.clone(),
        });
    }
    let assignment = loop_score::Assignment {
        seed,
        pictures: assigned,
    };
    let text = serde_json::to_string_pretty(&assignment).map_err(|e| e.to_string())? + "\n";
    bytes += write_new(&asked.out.join("assignment.json"), text.as_bytes())?;
    let summary = format!(
        "{} pictures, in an order drawn from a seed; sheet {}.",
        on_sheet.len(),
        &digest_of(text.as_bytes())[7..23]
    );
    let html = page(&summary, &loop_score::sheet_id(seed), &on_sheet);
    bytes += write_new(&asked.out.join("index.html"), html.as_bytes())?;
    Ok(Written {
        pictures: on_sheet.len(),
        bytes,
    })
}
