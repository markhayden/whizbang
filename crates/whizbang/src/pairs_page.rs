//! `whizbang lab pairs` (plan m3, task L3, item 3; rascal's `lab/pairs.rs` and
//! `pages/pairs.html` at the commit `docs/sources.toml` pins): a round's blinded pairs, as a
//! static page for one person to pick on, with the assignment frozen beside it by digest.
//!
//! For each image the protocol judges (or a subset of them, `--limit`, drawn by group from
//! the seed: `whizbang_eval::lab::pairs`), the two arms' cells: a recipe's by its last
//! attempt, which must have completed; a reference's by the picture it hands back. The
//! order and the sides are drawn from the protocol's seed. Written in the folder the owner
//! names (`--out`), and nowhere else:
//!
//! - `assignment.json`: the lab, the protocol's digest, the arms, the seed, the subset when
//!   there is one, and each pair's image, group, cells and the side arm `a` took. It names
//!   the arms, so the picker does not open it; the page never reads it.
//! - `pictures/<nnn>-left.png`, `-right.png` and `-source.png`, the pair's two results and
//!   the picture they started from, re-encoded from their pixels (so no chunk of a file's
//!   own travels), and the same three with `-focus` when the results changed anything: the
//!   region both changed, with context, at full size, through the edge of a picture that
//!   wraps (`whizbang_eval::lab::focus`). Names say the pair's place on the page and the
//!   side, never an arm; each pair's left is written before its right.
//! - `index.html`: the page. A Content-Security-Policy in its head lets nothing load but
//!   pictures from the page's own place or a local file (`img-src 'self' file:`, since a
//!   browser may not count a file's own folder as `'self'`), no script run but its own and
//!   no style but its own, each by digest; it has no address of any other place. Every string it is given (the lab, the pair ids, the notes)
//!   is escaped for where it goes: HTML text, or JSON inside a script element of data whose
//!   `<`, `>` and `&` are written as escapes. Its script sets text, never markup. The picks
//!   leave it only as `picks.json`, a file the owner saves through the browser's download;
//!   it keeps nothing in the browser's storage. A saved picks file can be opened again to
//!   resume.
//!
//! What an inspecting picker could still find: the pair ids (corpus ids) and the pictures'
//! files, which name places and sides only; the size of each file, which can differ between
//! two arms' results; and `assignment.json` beside the page, which names every arm.
//!
//! The folder: by default `pairs/<a>-vs-<b>` under the corpus's folder, which the owner
//! names and which is outside any repository. Refused inside the store, and inside a git
//! work tree unless forced (`--force`); a link where the folder, `pictures` or a file is
//! is refused or replaced, never followed.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use whizbang_core::artifact::{self as artifacts, Artifact, ArtifactId, ArtifactType};
use whizbang_core::identity::{CanonicalPixels, DecodeLimits, Topology};
use whizbang_core::store::Reader;
use whizbang_core::store::fs::RealFs;
use whizbang_eval::lab::cell::Work;
use whizbang_eval::lab::decode::{Shown, Subset};
use whizbang_eval::lab::digest_of;
use whizbang_eval::lab::focus::{Changes, Focus};
use whizbang_eval::lab::pairs::{self, Candidate, Share};

use crate::harness::{self, Ended};
use crate::laboratory::{LabCell, Prepared};

const TEMPLATE: &str = include_str!("../pages/pairs.html");

/// What `whizbang lab pairs` is given beside the round.
pub struct Asked<'a> {
    /// The store the round ran in.
    pub root: &'a Path,
    /// The folder the page is written in.
    pub out: &'a Path,
    /// A subset of this many pairs, drawn by group.
    pub limit: Option<u32>,
    /// Written inside a git work tree.
    pub force: bool,
}

/// What was written: counts, the assignment's digest, and each group's share of a subset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub pairs: usize,
    pub of: usize,
    pub digest: String,
    pub shares: Vec<Share>,
    pub min_decided: u32,
    /// The bytes of the page and its pictures.
    pub bytes: u64,
}

impl Written {
    /// What the command prints: counts and the digest, no path and no corpus id.
    pub fn text(&self) -> String {
        let mut out = format!(
            "pairs: {} of the round's {}\nassignment: {}\n",
            self.pairs, self.of, self.digest
        );
        for share in &self.shares {
            out.push_str(&format!(
                "group {}: {} of {} pairs drawn{}\n",
                share.group,
                share.drawn,
                share.of,
                if share.short(self.min_decided) {
                    format!(
                        ", fewer than min_decided {}: inconclusive whatever is picked",
                        self.min_decided
                    )
                } else {
                    String::new()
                }
            ));
        }
        out.push_str(&format!(
            "written: index.html, assignment.json and pictures/, {} bytes, in the folder of \
             --out\n",
            self.bytes
        ));
        out
    }
}

/// The folder the page goes in, as it will be: refused inside the store, and inside a git
/// work tree unless forced. Its nearest folder that is there is resolved through links.
pub fn checked_out(out: &Path, store: &Path, force: bool) -> Result<PathBuf, String> {
    let mut there = out.to_path_buf();
    let mut rest = Vec::new();
    while std::fs::symlink_metadata(&there).is_err() {
        let Some(name) = there.file_name() else { break };
        rest.push(name.to_owned());
        there = match there.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
            _ => PathBuf::from("."),
        };
    }
    let mut resolved = (there.canonicalize())
        .map_err(|e| format!("the folder of --out cannot be resolved: {}", e.kind()))?;
    for name in rest.iter().rev() {
        resolved.push(name);
    }
    if let Ok(store) = store.canonicalize()
        && (resolved.starts_with(&store) || store.starts_with(&resolved))
    {
        return Err(
            "the pairs page is never written in the store's folder; name another \
                    with --out"
                .to_owned(),
        );
    }
    let tree = resolved.ancestors().any(|a| a.join(".git").exists());
    if tree && !force {
        return Err(
            "the folder of --out is inside a git work tree, where the owner's \
                    pictures could be committed; name a folder outside it, or give --force"
                .to_owned(),
        );
    }
    Ok(resolved)
}

/// A folder made if it is not there; refused when a link or not a folder is.
fn folder(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_dir() => Ok(()),
        Ok(_) => Err("the folder of --out, or its pictures, is a link or a file".to_owned()),
        Err(_) => std::fs::create_dir_all(path)
            .map_err(|e| format!("cannot make the folder of --out: {}", e.kind())),
    }
}

/// A file written new: what is there (a file or a link) is taken away first, never
/// written through.
fn write_new(path: &Path, bytes: &[u8]) -> Result<u64, String> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    let name = name.unwrap_or_default();
    if let Ok(m) = std::fs::symlink_metadata(path) {
        if m.file_type().is_dir() {
            return Err(format!("{name} in the folder of --out is a folder"));
        }
        std::fs::remove_file(path).map_err(|e| format!("cannot replace {name}: {}", e.kind()))?;
    }
    let mut file = (std::fs::OpenOptions::new().write(true).create_new(true))
        .open(path)
        .map_err(|e| format!("cannot write {name}: {}", e.kind()))?;
    file.write_all(bytes)
        .map_err(|e| format!("cannot write {name}: {}", e.kind()))?;
    Ok(bytes.len() as u64)
}

/// Text escaped for HTML: `&`, `<`, `>`, `"` and `'`.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// JSON for a script element of data: `<`, `>`, `&` and the two line separators written
/// as escapes, so no text in it can end the element.
pub fn json_in_html(value: &serde_json::Value) -> String {
    let text = value.to_string();
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c => out.push(c),
        }
    }
    out
}

/// The standard base64 of bytes, as a policy's digest of a script is written.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The text between an element's opening tag and its closing one, the first of `open`.
fn inside<'a>(page: &'a str, open: &str, close: &str) -> &'a str {
    let start = page.find(open).map_or(0, |i| i + open.len());
    let end = page[start..].find(close).map_or(start, |i| start + i);
    &page[start..end]
}

/// The page's policy: nothing but its own pictures, its own style and its own script, the
/// last two by their digests.
pub fn policy() -> String {
    let script = inside(TEMPLATE, "<script>", "</script>");
    let style = inside(TEMPLATE, "<style>", "</style>");
    let digest = |text: &str| {
        let hex = whizbang_core::identity::Digest::of_bytes(text.as_bytes()).hex();
        let raw: Vec<u8> = (0..hex.len() / 2)
            .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap_or(0))
            .collect();
        format!("'sha256-{}'", base64(&raw))
    };
    format!(
        "default-src 'none'; img-src 'self' file:; script-src {}; style-src {}; base-uri 'none'; \
         form-action 'none'",
        digest(script),
        digest(style)
    )
}

/// One pair as the page shows it.
pub struct OnPage {
    pub id: String,
    /// `001`, the start of its pictures' names.
    pub file: String,
    pub focus: bool,
    pub note: String,
}

/// The page, from the lab's name, what it says of the round, the subset's words (empty for
/// a whole round), the assignment's digest and the pairs.
pub fn page(lab: &str, summary: &str, subset: &str, digest: &str, pairs: &[OnPage]) -> String {
    let data = serde_json::json!({
        "digest": digest,
        "pairs": pairs.iter().map(|p| serde_json::json!({
            "id": p.id, "file": p.file, "focus": p.focus, "note": p.note,
        })).collect::<Vec<_>>(),
    });
    let subset = if subset.is_empty() {
        String::new()
    } else {
        format!("<p class=\"subset\">{}</p>", escape(subset))
    };
    TEMPLATE
        .replace("{{CSP}}", &escape(&policy()))
        .replace("{{LAB}}", &escape(lab))
        .replace("{{SUMMARY}}", &escape(summary))
        .replace("{{SUBSET}}", &subset)
        .replace("{{DATA}}", &json_in_html(&data))
}

/// What the page says of a pair's focus.
fn note(focus: Option<&Focus>, width: u32, height: u32) -> String {
    match focus {
        None => format!(
            "Neither result differs from its source here, or the sizes differ: the whole \
             pictures ({width} by {height})."
        ),
        Some(f) if f.width == width && f.height == height => {
            format!("The changes reach across the whole picture ({width} by {height}).")
        }
        Some(f) => format!(
            "The focus: {} by {} of {width} by {height}, at column {}, row {}{}, holding \
             what both sides changed, at full size.",
            f.width,
            f.height,
            f.x,
            f.y,
            if f.x + f.width > width {
                " (through the wrap edge)"
            } else {
                ""
            }
        ),
    }
}

/// A picture of the store, as pixels, with the topology of its space.
fn picture(reader: &Reader, id: ArtifactId) -> Result<(CanonicalPixels, Topology), String> {
    let limits = DecodeLimits::default();
    match artifacts::get(reader, ArtifactType::Image, id, &limits) {
        Ok(Artifact::Image(image)) => Ok((
            image.pixels(&limits).map_err(|e| e.to_string())?,
            image.space().topology(),
        )),
        _ => Err(
            "a picture of the round is not an image the store holds; run the round \
                  again"
                .to_owned(),
        ),
    }
}

fn shown_as(cell: &str, attempt: u32) -> Shown {
    Shown {
        cell: cell.to_owned(),
        attempt,
    }
}

fn png(pixels: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    use whizbang_core::identity::encode::{Channels, png_of};
    png_of(pixels, width, height, Channels::Rgba)
}

/// Writes the page of a prepared round's pairs (the module's description says what).
pub fn pairs(prepared: &Prepared, asked: &Asked<'_>) -> Result<Written, String> {
    let validated = &prepared.validated;
    let protocol = &validated.protocol;
    let out = checked_out(asked.out, asked.root, asked.force)?;
    let labs: Vec<LabCell<'_>> = (prepared.cells.iter())
        .map(|cell| LabCell {
            cell,
            scope: &prepared.scope,
            pictures: &prepared.pictures,
        })
        .collect();
    let work: Vec<&dyn harness::Work> = labs.iter().map(|c| c as &dyn harness::Work).collect();
    let recognise = harness::by_request(&prepared.scope, &work);
    let attempts = harness::attempts(asked.root, &recognise)?;
    let mut results: BTreeMap<String, ArtifactId> = BTreeMap::new();
    harness::each_run(asked.root, &mut |record, id, _| {
        let image = record.outputs().iter().find(|o| o.port == "image");
        if let Some(output) = image {
            results.insert(id.to_owned(), ArtifactId(output.artifact));
        }
    })?;
    // Each side: the cell and attempt the assignment names, and the picture it shows.
    let mut shown: BTreeMap<(String, String), (Shown, ArtifactId)> = BTreeMap::new();
    let (mut failed, mut open) = (0, 0);
    let chosen = pairs::chosen(protocol, asked.limit)?;
    for (image, _) in &chosen {
        for arm in &protocol.arms {
            let cell = (prepared.cells.iter())
                .find(|c| &c.arm == arm && &c.image == image)
                .ok_or("the matrix has no cell for an image the protocol judges")?;
            let no_image = || {
                format!(
                    "the arm {arm} hands back no image, and a pair compares two pictures; no \
                     page was written"
                )
            };
            let side = match &cell.work {
                Work::Reference { picture } => {
                    let id = prepared
                        .pictures
                        .get(picture)
                        .copied()
                        .ok_or_else(no_image)?;
                    Some((shown_as(&cell.identity, 0), id))
                }
                Work::Recipe { .. } => match attempts.get(&cell.identity).and_then(|l| l.last()) {
                    Some((attempt, run, Ended::Completed)) => {
                        let id = results.get(run).copied().ok_or_else(no_image)?;
                        Some((shown_as(&cell.identity, *attempt), id))
                    }
                    Some((_, _, Ended::Failed)) => {
                        failed += 1;
                        None
                    }
                    _ => {
                        open += 1;
                        None
                    }
                },
            };
            if let Some(side) = side {
                shown.insert((arm.clone(), image.clone()), side);
            }
        }
    }
    if failed + open > 0 {
        return Err(format!(
            "{} cells of the pairs have no completed attempt ({failed} failed, {open} not run \
             or interrupted); run the round again{}; no page was written",
            failed + open,
            if failed > 0 {
                " with --retry-failed"
            } else {
                ""
            }
        ));
    }
    let [a, b] = &protocol.arms;
    let candidates: Vec<Candidate> = (chosen.iter())
        .map(|(image, group)| Candidate {
            image: image.clone(),
            group: group.clone(),
            a: shown[&(a.clone(), image.clone())].0.clone(),
            b: shown[&(b.clone(), image.clone())].0.clone(),
        })
        .collect();
    let of = protocol.images().count();
    let subset = asked.limit.map(|limit| Subset {
        limit,
        of: of as u32,
    });
    let assignment = pairs::draw(protocol, &validated.protocol_digest, &candidates, subset);
    let shares = match asked.limit {
        Some(limit) => pairs::shares(protocol, limit)?,
        None => Vec::new(),
    };
    folder(&out)?;
    folder(&out.join("pictures"))?;
    let text = serde_json::to_string_pretty(&assignment).map_err(|e| e.to_string())? + "\n";
    let mut bytes = write_new(&out.join("assignment.json"), text.as_bytes())?;
    let digest = digest_of(text.as_bytes());
    let reader = Reader::open(asked.root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let by_cell: BTreeMap<&str, ArtifactId> = (shown.values())
        .map(|(s, id)| (s.cell.as_str(), *id))
        .collect();
    let mut on_page = Vec::new();
    for (n, pair) in assignment.pair.iter().enumerate() {
        let file = format!("{:03}", n + 1);
        let (left, topology) = picture(&reader, by_cell[pair.left.cell.as_str()])?;
        let (right, _) = picture(&reader, by_cell[pair.right.cell.as_str()])?;
        let (source, _) = picture(&reader, prepared.pictures[&pair.id])?;
        let (width, height) = (left.width(), left.height());
        if (right.width(), right.height()) != (width, height) {
            return Err(format!(
                "pair {}: its two sides are not pictures of one size; no page was finished",
                pair.id
            ));
        }
        let focus = if (source.width(), source.height()) == (width, height) {
            let mut changes = Changes::between(width, height, source.rgba(), left.rgba())?;
            changes.join(&Changes::between(
                width,
                height,
                source.rgba(),
                right.rgba(),
            )?)?;
            changes.focus(topology)
        } else {
            None
        };
        let named = |side: &str, kind: &str| {
            out.join("pictures")
                .join(format!("{file}-{side}{kind}.png"))
        };
        for (side, pixels) in [("left", &left), ("right", &right), ("source", &source)] {
            let whole = png(pixels.rgba(), pixels.width(), pixels.height())?;
            bytes += write_new(&named(side, ""), &whole)?;
            if let Some(f) = focus.filter(|_| (pixels.width(), pixels.height()) == (width, height))
            {
                let crop = png(&f.crop(width, pixels.rgba()), f.width, f.height)?;
                bytes += write_new(&named(side, "-focus"), &crop)?;
            }
        }
        on_page.push(OnPage {
            id: pair.id.clone(),
            file,
            focus: focus.is_some(),
            note: note(focus.as_ref(), width, height),
        });
    }
    let summary = format!(
        "{} pairs; protocol {}.",
        assignment.pair.len(),
        validated.protocol_digest
    );
    let subset_words = match subset {
        Some(Subset { limit, of }) => format!(
            "This round is a subset: {limit} of its {of} pairs, drawn by group from the \
             protocol's seed. A group with fewer than {} decided pairs is inconclusive, never \
             a win; the decode says so.",
            protocol.min_decided
        ),
        None => String::new(),
    };
    let html = page(&protocol.name, &summary, &subset_words, &digest, &on_page);
    bytes += write_new(&out.join("index.html"), html.as_bytes())?;
    Ok(Written {
        pairs: assignment.pair.len(),
        of,
        digest,
        shares,
        min_decided: protocol.min_decided,
        bytes,
    })
}
