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
//!   side, never an arm; each pair's left is written before its right. Each is a PNG whose
//!   pixels are not compressed ([`stored_png`]; plan mE, task P0, item 1), RGB when every
//!   picture of the pair is opaque and RGBA otherwise, so its size and every length inside
//!   it follow from its dimensions and that one choice of the pair alone. A compressing encoder gave a
//!   reference arm's picture, made from the source's own pixels, the source's size to the
//!   byte; plan m4's chunk of padding hid that from the size but not from the chunk's own
//!   length.
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
//! files, which name places and sides only; `assignment.json` beside the page, which names
//! every arm; and, when an arm hands the source back (a reference whose picture is the one
//! given, as `--from-runs` without `--reference` draws), a side whose file is byte for byte
//! the source's file beside it, since one encoder writes both from the same pixels. The
//! page shows that as well (hold `s`): there the blinding is the drawn order of the sides
//! alone, by the plan's design. A file's size and its chunks' lengths say nothing beyond
//! the picture's width and height and whether every picture of its pair is opaque.
//!
//! The same page is drawn from runs already in the store (`--from-runs`, plan mE, task M2;
//! [`pairs_from_runs`]): each case's finished run against the picture it was given or
//! against another cases file's run of it, so a round that did not run through the lab,
//! the loop's among them, is picked as a lab round is.
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
use whizbang_eval::lab::protocol::Protocol;

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
pub(crate) fn folder(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_dir() => Ok(()),
        Ok(_) => Err("the folder of --out, or its pictures, is a link or a file".to_owned()),
        Err(_) => std::fs::create_dir_all(path)
            .map_err(|e| format!("cannot make the folder of --out: {}", e.kind())),
    }
}

/// A file written new: what is there (a file or a link) is taken away first, never
/// written through.
pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> Result<u64, String> {
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
    policy_for(TEMPLATE)
}

/// The same policy for another page built this way (the loop's contact sheet,
/// [`crate::loop_sheet`]): its first script and its style, by their digests.
pub fn policy_for(template: &str) -> String {
    let script = inside(template, "<script>", "</script>");
    let style = inside(template, "<style>", "</style>");
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

pub(crate) fn png(pixels: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    use whizbang_core::identity::encode::{Channels, png_of};
    png_of(pixels, width, height, Channels::Rgba)
}

/// The CRC a PNG chunk ends with (ISO 3309, as the format's own text gives it), over the
/// chunk's type and data.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// The largest stored block of deflate: its length is two bytes.
const STORED_BLOCK: usize = 65_535;

/// The size on disk of [`stored_png`]'s file of a picture of these dimensions: the
/// signature, `IHDR`, one `IDAT` of a zlib stream of stored blocks over the rows (each a
/// filter byte and three bytes a pixel when `opaque`, else four), and `IEND`. It follows
/// from the dimensions and the pair's one choice of `opaque` alone.
pub fn stored_png_size(width: u32, height: u32, opaque: bool) -> u64 {
    let channels = if opaque { 3 } else { 4 };
    let raw = u64::from(height) * (1 + channels * u64::from(width));
    let blocks = raw.div_ceil(STORED_BLOCK as u64).max(1);
    let idat = 2 + 5 * blocks + raw + 4;
    8 + (12 + 13) + (12 + idat) + 12
}

/// Whether every pixel of every one of these RGBA pictures is opaque: the one choice of a
/// pair's pictures, whole and of the focus, between RGB and RGBA, so the sides share it.
pub fn opaque(pictures: &[&[u8]]) -> bool {
    (pictures.iter()).all(|rgba| rgba.as_chunks::<4>().0.iter().all(|px| px[3] == u8::MAX))
}

/// A picture of a pair written as a PNG whose pixel data is not compressed (plan mE, task
/// P0, item 1): eight bits a sample, RGB when the pair is `opaque` ([`opaque`]) and RGBA
/// otherwise, no filter, one `IDAT` holding a zlib stream of stored deflate blocks of the
/// largest size and the rest, and no other chunk. So its size and every chunk's length
/// follow from its width and height and the pair's choice ([`stored_png_size`]), and two
/// pictures of one size in a pair are files of one length, whichever arm made them.
pub fn stored_png(pixels: &[u8], width: u32, height: u32, opaque: bool) -> Result<Vec<u8>, String> {
    let row = 4 * width as usize;
    if width == 0 || height == 0 || pixels.len() != row * height as usize {
        return Err("a picture of a pair is not RGBA of its size; no page was finished".into());
    }
    if opaque && !self::opaque(&[pixels]) {
        return Err("a picture of a pair said to be opaque is not; no page was finished".into());
    }
    let size = stored_png_size(width, height, opaque);
    let idat = size - 8 - 25 - 12 - 12;
    if idat > u64::from(i32::MAX as u32) {
        return Err(
            "a picture of a pair is too large for one PNG chunk; no page was finished".into(),
        );
    }
    let chunk = |out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]| {
        out.extend((data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend(kind);
        out.extend(data);
        let crc = crc32(&out[start..]);
        out.extend(crc.to_be_bytes());
    };
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    for line in pixels.chunks_exact(row) {
        raw.push(0);
        if opaque {
            raw.extend(line.as_chunks::<4>().0.iter().flat_map(|px| &px[..3]));
        } else {
            raw.extend(line);
        }
    }
    let (mut a, mut b) = (1_u32, 0_u32);
    for byte in &raw {
        a = (a + u32::from(*byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    let mut zlib = Vec::with_capacity(idat as usize);
    zlib.extend([0x78, 0x01]);
    let blocks: Vec<&[u8]> = raw.chunks(STORED_BLOCK).collect();
    for (n, block) in blocks.iter().enumerate() {
        let len = block.len() as u16;
        zlib.push(u8::from(n + 1 == blocks.len()));
        zlib.extend(len.to_le_bytes());
        zlib.extend((!len).to_le_bytes());
        zlib.extend(*block);
    }
    zlib.extend(((b << 16) | a).to_be_bytes());
    let mut header = Vec::with_capacity(13);
    header.extend(width.to_be_bytes());
    header.extend(height.to_be_bytes());
    // Eight bits a sample, RGB or RGBA, deflate, adaptive filtering, no interlace.
    header.extend([8, if opaque { 2 } else { 6 }, 0, 0, 0]);
    let mut out = Vec::with_capacity(size as usize);
    out.extend(b"\x89PNG\r\n\x1a\n");
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    Ok(out)
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
    let drawn = Drawn {
        protocol,
        protocol_digest: &validated.protocol_digest,
        candidates,
        by_cell: (shown.values())
            .map(|(s, id)| (s.cell.clone(), *id))
            .collect(),
        sources: prepared.pictures.clone(),
        limit: asked.limit,
    };
    write_drawn(&drawn, asked.root, &out)
}

/// Pairs drawn and ready to be written: the shared end of [`pairs`] and
/// [`pairs_from_runs`].
struct Drawn<'a> {
    protocol: &'a Protocol,
    protocol_digest: &'a str,
    /// In the order [`pairs::chosen`] gives.
    candidates: Vec<Candidate>,
    /// The picture each side's cell shows.
    by_cell: BTreeMap<String, ArtifactId>,
    /// The picture each pair's results started from, by the pair's id.
    sources: BTreeMap<String, ArtifactId>,
    limit: Option<u32>,
}

/// Draws the order and sides of the pairs and writes the page, its pictures and the
/// assignment in `out` (the module's description says what).
fn write_drawn(drawn: &Drawn<'_>, root: &Path, out: &Path) -> Result<Written, String> {
    let protocol = drawn.protocol;
    let candidates = &drawn.candidates;
    let of = protocol.images().count();
    let subset = drawn.limit.map(|limit| Subset {
        limit,
        of: of as u32,
    });
    let assignment = pairs::draw(protocol, drawn.protocol_digest, candidates, subset);
    let shares = match drawn.limit {
        Some(limit) => pairs::shares(protocol, limit)?,
        None => Vec::new(),
    };
    folder(out)?;
    folder(&out.join("pictures"))?;
    let text = serde_json::to_string_pretty(&assignment).map_err(|e| e.to_string())? + "\n";
    let mut bytes = write_new(&out.join("assignment.json"), text.as_bytes())?;
    let digest = digest_of(text.as_bytes());
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let by_cell = &drawn.by_cell;
    let mut on_page = Vec::new();
    for (n, pair) in assignment.pair.iter().enumerate() {
        let file = format!("{:03}", n + 1);
        let (left, topology) = picture(&reader, by_cell[pair.left.cell.as_str()])?;
        let (right, _) = picture(&reader, by_cell[pair.right.cell.as_str()])?;
        let source = (drawn.sources.get(&pair.id))
            .ok_or("a pair has no picture its results started from; no page was finished")?;
        let (source, _) = picture(&reader, *source)?;
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
        // Each picture is written uncompressed, so its size follows from its dimensions and
        // the pair's one choice of RGB or RGBA alone ([`stored_png`]), left first.
        let opaque = opaque(&[left.rgba(), right.rgba(), source.rgba()]);
        for (side, pixels) in [("left", &left), ("right", &right), ("source", &source)] {
            let whole = stored_png(pixels.rgba(), pixels.width(), pixels.height(), opaque)?;
            bytes += write_new(&named(side, ""), &whole)?;
            if let Some(f) = focus.filter(|_| (pixels.width(), pixels.height()) == (width, height))
            {
                let crop = stored_png(&f.crop(width, pixels.rgba()), f.width, f.height, opaque)?;
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
        drawn.protocol_digest
    );
    let subset_words = match subset {
        Some(Subset { limit, of }) => format!(
            "This round is a subset: {limit} of its {of} pairs, drawn by group from the \
             protocol's seed. A group with fewer than {} {} is inconclusive, never a win; the \
             decode says so.",
            protocol.min_decided,
            if protocol.version >= 2 {
                "pairs picked"
            } else {
                "decided pairs"
            }
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

/// What `whizbang lab pairs --from-runs` is given (plan mE, task M2, item 4).
pub struct FromRuns<'a> {
    /// The cases file whose runs are arm `a`, the recipe.
    pub cases: &'a Path,
    /// The protocol: its arms, its seed and its groups of case ids.
    pub protocol: &'a Path,
    /// Another cases file whose runs of the same cases are arm `b`; without it, arm `b` is
    /// the picture each of arm `a`'s runs was given.
    pub reference: Option<&'a Path>,
    pub root: &'a Path,
    pub out: &'a Path,
    pub limit: Option<u32>,
    pub force: bool,
}

/// Each case's finished run, by case id: its run's id, the picture it was given and its
/// finished picture. A case of the protocol whose run did not finish is refused.
fn finished_of(
    file: &Path,
    root: &Path,
    chosen: &[(String, String)],
) -> Result<BTreeMap<String, (String, ArtifactId, ArtifactId)>, String> {
    let bytes = crate::loop_score::read_limited(file, "the cases")?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "the cases are not UTF-8")?;
    let cases = whizbang_eval::edit_gate::Cases::parse(text)?;
    let held = crate::edits_gate::case_runs(root, &cases)?;
    let mut out = BTreeMap::new();
    for r in &held.runs {
        if let (Some(run), true, Some(given), Some(image)) = (&r.run, r.finished, r.given, r.image)
        {
            out.insert(r.case.id.clone(), (run.clone(), given, image));
        }
    }
    let unfinished = (chosen.iter())
        .filter(|(id, _)| !out.contains_key(id))
        .count();
    if unfinished > 0 {
        return Err(format!(
            "{unfinished} cases the protocol judges have no finished run in the store; no \
             page was written"
        ));
    }
    Ok(out)
}

/// Writes the blinded pairs of runs already in the store, as [`pairs`] writes a round's:
/// for each case the protocol judges, arm `a` the finished picture of its run (a recipe's,
/// or the loop's kept one), arm `b` the picture that run was given or the finished picture
/// of the reference cases' run of the same case, drawn by the protocol's seed. A side names
/// its run by its id; the picture given, by `source.<case>`. `whizbang lab decode` reads
/// what it writes unchanged.
pub fn pairs_from_runs(asked: &FromRuns<'_>) -> Result<Written, String> {
    let out = checked_out(asked.out, asked.root, asked.force)?;
    let bytes = crate::loop_score::read_limited(asked.protocol, "the protocol")?;
    let (protocol, digest) = Protocol::parse(&bytes)?;
    let chosen = pairs::chosen(&protocol, asked.limit)?;
    let ours = finished_of(asked.cases, asked.root, &chosen)?;
    let reference = match asked.reference {
        Some(file) => Some(finished_of(file, asked.root, &chosen)?),
        None => None,
    };
    let (mut by_cell, mut sources) = (BTreeMap::new(), BTreeMap::new());
    let mut candidates = Vec::new();
    for (case, group) in &chosen {
        let (run, given, image) = &ours[case];
        by_cell.insert(run.clone(), *image);
        sources.insert(case.clone(), *given);
        let b = match &reference {
            Some(reference) => {
                let (other, _, finished) = &reference[case];
                by_cell.insert(other.clone(), *finished);
                shown_as(other, 0)
            }
            None => {
                let source = format!("source.{case}");
                by_cell.insert(source.clone(), *given);
                shown_as(&source, 0)
            }
        };
        candidates.push(Candidate {
            image: case.clone(),
            group: group.clone(),
            a: shown_as(run, 0),
            b,
        });
    }
    let drawn = Drawn {
        protocol: &protocol,
        protocol_digest: &digest,
        candidates,
        by_cell,
        sources,
        limit: asked.limit,
    };
    write_drawn(&drawn, asked.root, &out)
}
