//! A run's report (plan m5, task W3, items 3 to 5; `SPEC.md` section 7, "Report viewer and
//! export"): one self-contained HTML document made from the run's record and what the
//! store keeps beside it, which the playground shows in an isolated frame and downloads
//! as "Export"; and the run's archive, its record and outputs, for "Export run".
//!
//! **What a report holds.** What was asked; the pictures given and made, and each mask
//! over the picture it is on; the steps, each with its child run and how it ended; a
//! loop's attempts; for a check, its verdict and reasons, its checklist as the judge
//! answered it, its findings with their places drawn on the pictures, and each judge
//! turn's text, read from the stored exchange by [`written::for_display`], with its cost;
//! and the cost of every paid request. A withheld part, a picture the store does not
//! hold and a failed turn are each said as such.
//!
//! **Self-contained and inert**, as the lab's pairs page is (`pairs_page.rs`): every
//! picture is inlined as a `data:` address, at most [`PICTURE_EDGE`] pixels on its long
//! side; the document carries its own policy ([`POLICY`]), which lets it load nothing
//! and run nothing, and it holds no script, no form, no link out and no event handler.
//! Every text from a model, a tool or a person is escaped ([`escape`]), so a reason that
//! holds markup shows as its characters.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

use whizbang_core::artifact::{self, Artifact, ArtifactId, Contents};
use whizbang_core::capability::written::{self, Shown, ShownAnswer, ShownText};
use whizbang_core::contract::RunId;
use whizbang_core::contract::record::{
    self, ArgumentValue, DIAGNOSTICS_PREFIX, Judged, RunRecord, Status,
};
use whizbang_core::identity::{CanonicalPixels, DecodeLimits, Digest, PixelDigest};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Generation, Kind, Reader};

/// The policy a report carries in its own head.
pub const POLICY: &str = "default-src 'none'; img-src data:; style-src 'unsafe-inline'";

/// The longest side of a picture a report inlines, in pixels.
pub const PICTURE_EDGE: u32 = 2048;

/// The most pictures a report inlines; past it a picture is named, not shown.
pub const PICTURES_LIMIT: usize = 48;

/// The most runs under a run that a report reads.
pub const RUNS_LIMIT: usize = 256;

/// The most judge turns a report shows whole.
pub const TURNS_SHOWN: usize = 64;

/// Whether a character changes the direction text is shown in (U+202A to U+202E and
/// U+2066 to U+2069), which could make a reason read other than it is.
fn bidi(c: char) -> bool {
    matches!(u32::from(c), 0x202A..=0x202E | 0x2066..=0x2069)
}

/// Escapes text for HTML: the five characters that could start markup or end an
/// attribute; a new line and a tab kept, and `\r\n` as a new line; every other control
/// character, and each that changes the direction of text, as its code.
pub fn escape(text: &str) -> String {
    let text = text.replace("\r\n", "\n");
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '\n' | '\t' => out.push(c),
            c if c.is_control() || bidi(c) => {
                let _ = write!(out, "\\u{{{:x}}}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out
}

/// Bytes as base64, for a `data:` address.
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for at in 0..4 {
            if at <= chunk.len() {
                out.push(TABLE[((n >> (18 - 6 * at)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Millionths of a US dollar as money, to a tenth of a cent.
#[allow(clippy::cast_precision_loss)]
fn usd(micros: u64) -> String {
    format!("${:.4}", micros as f64 / 1_000_000.0)
}

/// A time in milliseconds since the epoch as a UTC date and time.
fn when(ms: u64) -> String {
    let secs = ms / 1000;
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rest = secs % 86_400;
    // The civil date of a day count, by the usual arithmetic on eras of 400 years.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        rest / 3600,
        (rest % 3600) / 60
    )
}

/// A picture as a PNG at most [`PICTURE_EDGE`] pixels on its long side, from RGBA.
fn png_at_most(width: u32, height: u32, rgba: Vec<u8>) -> Option<Vec<u8>> {
    let image = image::RgbaImage::from_raw(width, height, rgba)?;
    let long = width.max(height).max(1);
    let image = match long > PICTURE_EDGE {
        true => {
            let side = |n: u32| {
                let scaled = u64::from(n) * u64::from(PICTURE_EDGE) / u64::from(long);
                u32::try_from(scaled.max(1)).unwrap_or(1)
            };
            let filter = image::imageops::FilterType::Triangle;
            image::imageops::resize(&image, side(width), side(height), filter)
        }
        false => image,
    };
    whizbang_core::identity::encode::png_of(
        image.as_raw(),
        image.width(),
        image.height(),
        whizbang_core::identity::encode::Channels::Rgba,
    )
    .ok()
}

/// What a report reads a store through: a reader of its own, at one generation.
struct Shelf {
    reader: Reader,
    generation: Generation,
    limits: DecodeLimits,
    /// Each artifact read, and each image's pixels decoded, once.
    artifacts: std::cell::RefCell<BTreeMap<Digest, Option<std::rc::Rc<Artifact>>>>,
    pixels: std::cell::RefCell<BTreeMap<Digest, Option<std::rc::Rc<CanonicalPixels>>>>,
}

impl Shelf {
    fn open(root: &Path) -> Result<Shelf, String> {
        let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
        let generation = reader.snapshot().map_err(|e| e.to_string())?;
        Ok(Shelf {
            reader,
            generation,
            limits: DecodeLimits::default(),
            artifacts: std::cell::RefCell::new(BTreeMap::new()),
            pixels: std::cell::RefCell::new(BTreeMap::new()),
        })
    }

    fn record(&self, run: &str) -> Option<RunRecord> {
        record::stored(&self.generation, run, |entry| {
            self.reader.read_record(entry)
        })?
        .ok()
    }

    /// What the run kept among its diagnostics under `name`, with or without the
    /// prefix of a diagnostic's name (a record names an exchange with it).
    fn diagnostic(&self, run: &str, name: &str) -> Option<Vec<u8>> {
        let held = self.generation.run(run)?;
        let name = match name.starts_with(DIAGNOSTICS_PREFIX) {
            true => name.to_owned(),
            false => format!("{DIAGNOSTICS_PREFIX}{name}"),
        };
        let entry = held.get(&name)?;
        (entry.kind == Kind::Diagnostic)
            .then(|| self.reader.read_record(entry).ok())
            .flatten()
    }

    /// The artifact of this identity, read once.
    fn artifact(&self, id: Digest) -> Option<std::rc::Rc<Artifact>> {
        if let Some(read) = self.artifacts.borrow().get(&id) {
            return read.clone();
        }
        let wanted = ArtifactId(id);
        let read = artifact::type_anywhere(&self.generation, wanted)
            .and_then(|kind| artifact::get_anywhere(&self.reader, kind, wanted, &self.limits).ok())
            .map(std::rc::Rc::new);
        self.artifacts.borrow_mut().insert(id, read.clone());
        read
    }

    /// The canonical pixels of the image of this identity, decoded once.
    fn pixels(&self, id: Digest) -> Option<std::rc::Rc<CanonicalPixels>> {
        if let Some(read) = self.pixels.borrow().get(&id) {
            return read.clone();
        }
        let read = match self.artifact(id).as_deref() {
            Some(Artifact::Image(image)) => image.pixels(&self.limits).ok().map(std::rc::Rc::new),
            _ => None,
        };
        self.pixels.borrow_mut().insert(id, read.clone());
        read
    }

    /// The pixel digest of the image of this identity, from its file's identity.
    fn pixel_digest(&self, id: Digest) -> Option<PixelDigest> {
        match self.artifact(id).as_deref() {
            Some(Artifact::Image(image)) => Some(image.identity().pixels),
            _ => None,
        }
    }
}

/// One run of the report's tree: its id and its record.
struct Ran {
    id: String,
    record: RunRecord,
}

/// The run and the runs under it, the run first, then each in the order its parent's
/// steps name it, at most [`RUNS_LIMIT`].
fn tree(shelf: &Shelf, run: &str, top: RunRecord) -> Vec<Ran> {
    let mut all = vec![Ran {
        id: run.to_owned(),
        record: top,
    }];
    let mut at = 0;
    while at < all.len() && all.len() < RUNS_LIMIT {
        let steps: Vec<String> = all[at]
            .record
            .steps()
            .iter()
            .map(|step| step.run().to_owned())
            .collect();
        for child in steps {
            if all.len() >= RUNS_LIMIT || all.iter().any(|ran| ran.id == child) {
                continue;
            }
            if let Some(record) = shelf.record(&child) {
                all.push(Ran { id: child, record });
            }
        }
        at += 1;
    }
    all
}

/// The pictures a report shows, each by the identity of its artifact.
struct Pictures {
    /// The `data:` address of each picture drawn, by a key of the report's own.
    drawn: BTreeMap<String, Option<String>>,
    count: usize,
}

impl Pictures {
    fn new() -> Pictures {
        Pictures {
            drawn: BTreeMap::new(),
            count: 0,
        }
    }

    /// The `data:` address of the image of identity `id`, made once; none when the
    /// store does not hold it, or the report holds as many pictures as it shows.
    fn image(&mut self, shelf: &Shelf, id: Digest) -> Option<String> {
        let key = format!("image:{}", id.hex());
        if let Some(said) = self.drawn.get(&key) {
            return said.clone();
        }
        let made = match self.count < PICTURES_LIMIT {
            true => (shelf.pixels(id))
                .and_then(|p| png_at_most(p.width(), p.height(), p.rgba().to_vec())),
            false => None,
        };
        let said = made.map(|png| format!("data:image/png;base64,{}", base64(&png)));
        self.count += usize::from(said.is_some());
        self.drawn.insert(key, said.clone());
        said
    }

    /// A mask of identity `id` over the picture it is on: the picture, with the mask's
    /// weight in red over it; none when either is not in the store.
    fn mask_over(&mut self, shelf: &Shelf, id: Digest, on: &[Digest]) -> Option<String> {
        let key = format!("mask:{}", id.hex());
        if let Some(said) = self.drawn.get(&key) {
            return said.clone();
        }
        let mut made = None;
        if self.count < PICTURES_LIMIT
            && let Some(read) = shelf.artifact(id)
            && let Artifact::Mask(mask) = read.as_ref()
        {
            let base = (on.iter())
                .find(|image| shelf.pixel_digest(**image) == Some(mask.image()))
                .and_then(|image| shelf.pixels(*image));
            let space = mask.space();
            let (width, height) = (space.width(), space.height());
            let count = (width as usize) * (height as usize);
            let mut rgba = match &base {
                Some(p) if p.width() == width && p.height() == height => p.rgba().to_vec(),
                _ => [40u8, 40, 40, 255].repeat(count),
            };
            for (at, pixel) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let core = f32::from(*mask.core().get(at).unwrap_or(&0));
                let blend = f32::from(*mask.blend().get(at).unwrap_or(&0));
                let weight = core.max(blend) / 255.0 * 0.6;
                let mix = |from: u8, to: f32| {
                    (f32::from(from) * (1.0 - weight) + to * weight).round() as u8
                };
                pixel[0] = mix(pixel[0], 255.0);
                pixel[1] = mix(pixel[1], 48.0);
                pixel[2] = mix(pixel[2], 48.0);
            }
            made = png_at_most(width, height, rgba);
        }
        let said = made.map(|png| format!("data:image/png;base64,{}", base64(&png)));
        self.count += usize::from(said.is_some());
        self.drawn.insert(key, said.clone());
        said
    }
}

const STYLE: &str = "\
:root{color-scheme:dark;--bg:#0f1114;--panel:#171a1f;--line:#2a2f37;--ink:#f5f7f9;\
--quiet:#a9b1bc;--pass:#4fd18b;--fail:#ff6b6b;--unsure:#f2c14e;--mark:#ff3030}\
*{box-sizing:border-box}body{margin:0;padding:24px;background:var(--bg);color:var(--ink);\
font:14px/1.5 -apple-system,system-ui,sans-serif}h1{font-size:22px;margin:0 0 4px}\
h2{font-size:15px;margin:28px 0 8px;color:var(--quiet);text-transform:uppercase;\
letter-spacing:.04em}.meta,.quiet{color:var(--quiet)}code,.mono{font-family:ui-monospace,\
monospace;font-size:12px}table{border-collapse:collapse;width:100%}td,th{text-align:left;\
padding:6px 8px;border-bottom:1px solid var(--line);vertical-align:top}th{color:var(--quiet);\
font-weight:600}.pictures{display:flex;flex-wrap:wrap;gap:16px}figure{margin:0;\
background:var(--panel);border:1px solid var(--line);border-radius:8px;padding:8px;\
max-width:100%}figure img{display:block;max-width:min(480px,100%);height:auto}\
figcaption{color:var(--quiet);font-size:12px;margin-top:6px}.placed{position:relative;\
display:inline-block}.placed svg{position:absolute;inset:0;width:100%;height:100%}\
.verdict{display:inline-block;padding:2px 10px;border-radius:999px;font-weight:700}\
.verdict--pass{background:var(--pass);color:#06210f}.verdict--fail{background:var(--fail);\
color:#2a0606}.verdict--unsure{background:var(--unsure);color:#2a1f04}\
.turn{background:var(--panel);border:1px solid var(--line);border-radius:8px;padding:12px;\
margin:8px 0}.said{white-space:pre-wrap;word-break:break-word}.note{color:var(--unsure)}\
.missing{display:inline-block;padding:16px;border:1px dashed var(--line);color:var(--quiet)}";

/// Text of an argument's value.
fn value_text(value: &ArgumentValue) -> String {
    match value {
        ArgumentValue::Text(text) => text.clone(),
        ArgumentValue::Number(n) => n.to_string(),
        ArgumentValue::Integer(n) => n.to_string(),
        ArgumentValue::Boolean(b) => b.to_string(),
    }
}

fn status_word(record: &RunRecord) -> &'static str {
    match record.outcome().lost.is_empty() {
        false => "interrupted",
        true => record.outcome().status.name(),
    }
}

/// An image figure: the picture, or a frame saying it is not in the store.
fn figure(
    out: &mut String,
    src: Option<&str>,
    caption: &str,
    places: &[[f64; 4]],
    polys: &[Vec<[f64; 2]>],
) {
    out.push_str("<figure>");
    match src {
        Some(src) => {
            out.push_str("<span class=\"placed\">");
            let _ = write!(out, "<img src=\"{src}\" alt=\"{}\">", escape(caption));
            if !places.is_empty() || !polys.is_empty() {
                out.push_str(
                    "<svg viewBox=\"0 0 1 1\" preserveAspectRatio=\"none\" aria-hidden=\"true\">",
                );
                for [left, top, right, bottom] in places {
                    let _ = write!(
                        out,
                        "<rect x=\"{left}\" y=\"{top}\" width=\"{}\" height=\"{}\" fill=\"none\" \
                         stroke=\"#ff3030\" stroke-width=\"0.006\"/>",
                        right - left,
                        bottom - top
                    );
                }
                for points in polys {
                    let list: Vec<String> =
                        points.iter().map(|[x, y]| format!("{x},{y}")).collect();
                    let _ = write!(
                        out,
                        "<polygon points=\"{}\" fill=\"none\" stroke=\"#ff3030\" \
                         stroke-width=\"0.006\"/>",
                        list.join(" ")
                    );
                }
                out.push_str("</svg>");
            }
            out.push_str("</span>");
        }
        None => out.push_str("<span class=\"missing\">not in the store</span>"),
    }
    let _ = write!(out, "<figcaption>{}</figcaption></figure>", escape(caption));
}

fn text_of(shown: &ShownText) -> String {
    match shown.cut {
        0 => escape(&shown.text),
        cut => format!(
            "{}<span class=\"note\"> … {cut} more bytes not shown</span>",
            escape(&shown.text)
        ),
    }
}

/// The parts of a message or a turn, as the report shows them.
fn parts_html(out: &mut String, parts: &[Shown]) {
    for part in parts {
        match part {
            Shown::Text { text, cut } => {
                let _ = write!(
                    out,
                    "<div class=\"said\">{}</div>",
                    text_of(&ShownText {
                        text: text.clone(),
                        cut: *cut
                    })
                );
            }
            Shown::Picture {
                width,
                height,
                found,
                ..
            } => {
                let said = match found {
                    true => "a picture of the run, in the store, not shown in this report",
                    false => "a picture the store does not hold",
                };
                let _ = write!(
                    out,
                    "<div class=\"quiet\">[{said}, {width} by {height}]</div>"
                );
            }
            Shown::Withheld { at, adapter, bytes } => {
                let _ = write!(
                    out,
                    "<div class=\"note\">[part {} withheld: {} bytes of {}, kept as its digest]</div>",
                    at + 1,
                    bytes,
                    escape(adapter)
                );
            }
            Shown::Opaque { adapter, bytes } => {
                let _ = write!(
                    out,
                    "<div class=\"quiet\">[{bytes} bytes of {}, not shown]</div>",
                    escape(adapter)
                );
            }
            Shown::Call { name, arguments } => {
                let _ = write!(
                    out,
                    "<div class=\"mono\">calls {}: {}</div>",
                    escape(name),
                    text_of(arguments)
                );
            }
            Shown::Answered {
                text,
                error,
                pictures,
            } => {
                let label = if *error {
                    "tool error"
                } else {
                    "tool answered"
                };
                let _ = write!(out, "<div class=\"mono\">{label}: {}</div>", text_of(text));
                let missing = pictures
                    .iter()
                    .filter(|p| matches!(p, Shown::Picture { found: false, .. }))
                    .count();
                if !pictures.is_empty() {
                    let _ = write!(
                        out,
                        "<div class=\"quiet\">[{} pictures, {missing} of them not in the store]</div>",
                        pictures.len()
                    );
                }
            }
        }
    }
}

/// What came back in a stored exchange, as a report shows it: a turn's parts, each text
/// escaped, each withheld part and each picture the store does not hold said as such;
/// a turn that was not kept, with its size; a failed turn, with its error.
pub fn answer_html(shown: &written::Displayed) -> String {
    let mut out = String::new();
    match &shown.answer {
        ShownAnswer::Turn { parts, .. } => parts_html(&mut out, parts),
        ShownAnswer::Withheld { parts, bytes, .. } => {
            let _ = write!(
                out,
                "<p class=\"note\">the turn was not kept: {parts} parts, {bytes} bytes of text</p>"
            );
        }
        ShownAnswer::Failed { kind, sentence } => {
            let _ = write!(
                out,
                "<p class=\"note\">the turn failed: {}: {}</p>",
                escape(kind),
                escape(sentence)
            );
        }
    }
    // What the judge was shown that is not in the store, or was withheld.
    let sent = shown.messages.iter().flat_map(|m| m.parts.iter());
    let mut missing = 0;
    let mut withheld = 0;
    for part in sent {
        match part {
            Shown::Picture { found: false, .. } => missing += 1,
            Shown::Answered { pictures, .. } => {
                missing += (pictures.iter())
                    .filter(|p| matches!(p, Shown::Picture { found: false, .. }))
                    .count();
            }
            Shown::Withheld { .. } => withheld += 1,
            _ => {}
        }
    }
    if missing + withheld > 0 {
        let _ = write!(
            out,
            "<p class=\"quiet\">of what the judge was shown: {missing} pictures not in the \
             store, {withheld} parts withheld</p>"
        );
    }
    out
}

/// The report of the run `run` in the store at `root`; `title` is the task's title as
/// a person reads it, when it has one.
pub fn report(root: &Path, run: &RunId, title: Option<&str>) -> Result<String, String> {
    let shelf = Shelf::open(root)?;
    let top = shelf
        .record(run.as_str())
        .ok_or_else(|| "the store holds no record of that run".to_owned())?;
    let ran = tree(&shelf, run.as_str(), top);
    let record = &ran[0].record;
    let action = record.asked().action.clone().unwrap_or_default();
    let heading = title.unwrap_or(&action);
    let mut pictures = Pictures::new();
    let mut out = String::new();
    let _ = write!(
        out,
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta http-equiv=\"Content-Security-Policy\" content=\"{POLICY}\">\
         <meta name=\"referrer\" content=\"no-referrer\"><meta name=\"task\" content=\"{}\">\
         <title>{} · {}</title><style>{STYLE}</style></head><body>",
        escape(&action),
        escape(heading),
        escape(run.as_str())
    );
    let _ = write!(
        out,
        "<header><h1>{}</h1><p class=\"meta\"><span class=\"mono\">{}</span> · {} {} · {} · {}</p></header>",
        escape(heading),
        escape(run.as_str()),
        escape(&action),
        escape(record.asked().version.as_deref().unwrap_or("")),
        record.time().start_ms.map(when).unwrap_or_default(),
        status_word(record)
    );
    if let Some(error) = &record.outcome().error {
        let _ = write!(
            out,
            "<p class=\"note\">{}: {}</p>",
            escape(&error.kind),
            escape(&error.message)
        );
    }

    // The verdict, from the run or the check under it.
    let judged = ran.iter().find(|r| r.record.verdict().is_some());
    if let Some(judged) = judged
        && let Some((decision, why)) = judged.record.verdict()
    {
        let word = match decision {
            record::Decision::Pass => "pass",
            record::Decision::Fail => "fail",
            record::Decision::Unsure => "unsure",
        };
        let _ = write!(
            out,
            "<h2>Verdict</h2><p><span class=\"verdict verdict--{word}\">{}</span></p>\
             <p class=\"said\">{}</p>",
            word.to_uppercase(),
            escape(why)
        );
        let checked = ran
            .iter()
            .find(|r| r.record.asked().action.as_deref() == Some(whizbang_squint::check::NAME))
            .unwrap_or(judged);
        if let Some(bytes) = shelf.diagnostic(&checked.id, whizbang_squint::use_case::REASONS) {
            let items: Vec<String> = String::from_utf8_lossy(&bytes)
                .lines()
                .filter_map(|line| serde_json::from_str::<String>(line).ok())
                .collect();
            if !items.is_empty() {
                out.push_str("<h2>Reasons</h2><ul>");
                for item in items {
                    let _ = write!(out, "<li class=\"said\">{}</li>", escape(&item));
                }
                out.push_str("</ul>");
            }
        }
        if let Some(bytes) = shelf.diagnostic(&checked.id, whizbang_squint::check::JUDGED)
            && let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes)
            && let Some(answers) = json["answers"].as_array()
        {
            out.push_str(
                "<h2>Checklist</h2><table><tr><th>Item</th><th>Answer</th><th>Confidence</th>\
                 <th>Row</th></tr>",
            );
            for answer in answers {
                let word = |key: &str| escape(answer[key].as_str().unwrap_or(""));
                let item = match answer["id"].as_str() {
                    Some(id) => escape(id),
                    None => {
                        let cited = answer["obligation_ids"]
                            .as_array()
                            .cloned()
                            .unwrap_or_default();
                        let cited: Vec<&str> = cited.iter().filter_map(|c| c.as_str()).collect();
                        format!("finding: {}", escape(&cited.join(", ")))
                    }
                };
                let _ = write!(
                    out,
                    "<tr><td class=\"mono\">{item}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    word("disposition"),
                    word("confidence"),
                    word("row")
                );
            }
            out.push_str("</table>");
        }
    }

    // What was asked.
    let arguments = &record.asked().arguments;
    if !arguments.is_empty() {
        out.push_str("<h2>Asked</h2><table>");
        for argument in arguments {
            let _ = write!(
                out,
                "<tr><th class=\"mono\">{}</th><td class=\"said\">{}</td></tr>",
                escape(&argument.name),
                escape(&value_text(&argument.value))
            );
        }
        out.push_str("</table>");
    }

    // The pictures given and made, and the places of the findings on them.
    let images: Vec<(String, Digest)> = (record.inputs().iter())
        .filter(|input| input.artifact_type == artifact::ArtifactType::Image)
        .map(|input| (format!("given: {}", input.port), input.artifact))
        .chain(
            (record.outputs().iter())
                .filter(|output| output.artifact_type == artifact::ArtifactType::Image)
                .map(|output| (format!("made: {}", output.port), output.artifact)),
        )
        .collect();
    let found = judged.and_then(|r| r.record.findings());
    if !images.is_empty() {
        out.push_str("<h2>Pictures</h2><div class=\"pictures\">");
        for (caption, id) in &images {
            let src = pictures.image(&shelf, *id);
            let here = found.map(|f| {
                f.places
                    .iter()
                    .filter(|p| p.image == *id)
                    .collect::<Vec<_>>()
            });
            let here = here.unwrap_or_default();
            let boxes: Vec<[f64; 4]> = here.iter().filter_map(|p| p.boxed).collect();
            let polys: Vec<Vec<[f64; 2]>> = here.iter().filter_map(|p| p.polygon.clone()).collect();
            let caption = match here.len() {
                0 => caption.clone(),
                1 => format!("{caption} · 1 place the judge found"),
                n => format!("{caption} · {n} places the judge found"),
            };
            figure(&mut out, src.as_deref(), &caption, &boxes, &polys);
        }
        out.push_str("</div>");
    }
    if let Some(found) = found
        && !found.counts.is_empty()
    {
        out.push_str(
            "<h2>Findings</h2><table><tr><th>Obligation</th><th>Answer</th><th>Confidence</th>\
             <th>Count</th></tr>",
        );
        for count in &found.counts {
            let word = |value: serde_json::Value| escape(value.as_str().unwrap_or(""));
            let _ = write!(
                out,
                "<tr><td class=\"mono\">{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                escape(&count.obligation),
                word(serde_json::to_value(count.disposition).unwrap_or_default()),
                word(serde_json::to_value(count.confidence).unwrap_or_default()),
                count.count
            );
        }
        out.push_str("</table>");
    }

    // Masks, over the pictures they are on: the run's, else those of the runs under it.
    let all_images: Vec<Digest> = ran
        .iter()
        .flat_map(|r| {
            let inputs = r
                .record
                .inputs()
                .iter()
                .map(|i| (i.artifact_type, i.artifact));
            let outputs = r
                .record
                .outputs()
                .iter()
                .map(|o| (o.artifact_type, o.artifact));
            inputs.chain(outputs).collect::<Vec<_>>()
        })
        .filter(|(kind, _)| *kind == artifact::ArtifactType::Image)
        .map(|(_, id)| id)
        .collect();
    let masks_of = |r: &Ran| -> Vec<(String, Digest)> {
        (r.record.outputs().iter())
            .filter(|o| o.artifact_type == artifact::ArtifactType::Mask)
            .map(|o| {
                (
                    format!(
                        "{} · {}",
                        r.record.asked().action.as_deref().unwrap_or(""),
                        o.port
                    ),
                    o.artifact,
                )
            })
            .collect()
    };
    let mut masks = masks_of(&ran[0]);
    if masks.is_empty() {
        masks = ran.iter().skip(1).flat_map(masks_of).collect();
    }
    // Each mask once, at most eight.
    let mut seen = std::collections::BTreeSet::new();
    masks.retain(|(_, id)| seen.insert(*id));
    masks.truncate(8);
    let mut all_images = all_images;
    all_images.sort();
    all_images.dedup();
    if !masks.is_empty() {
        out.push_str("<h2>Masks over the picture</h2><div class=\"pictures\">");
        for (caption, id) in &masks {
            let src = pictures.mask_over(&shelf, *id, &all_images);
            figure(&mut out, src.as_deref(), caption, &[], &[]);
        }
        out.push_str("</div>");
    }

    // A loop's attempts.
    if let Some(attempts) = record.attempts() {
        out.push_str(
            "<h2>Attempts</h2><table><tr><th>Attempt</th><th>Checked</th><th>Settled</th></tr>",
        );
        for attempt in &attempts.list {
            let checked = match (&attempt.verdict, &attempt.unchecked) {
                (Some(verdict), _) => escape(
                    serde_json::to_value(verdict)
                        .unwrap_or_default()
                        .as_str()
                        .unwrap_or(""),
                ),
                (None, Some(why)) => format!("not checked: {}", escape(why)),
                _ => String::new(),
            };
            let _ = write!(
                out,
                "<tr><td>{}{}</td><td>{checked}</td><td class=\"mono\">{}</td></tr>",
                attempt.attempt,
                if attempts.kept == Some(attempt.attempt) {
                    " (kept)"
                } else {
                    ""
                },
                usd(attempt.settled_micros)
            );
        }
        let _ = write!(
            out,
            "</table><p class=\"quiet\">{}</p>",
            escape(&attempts.ended)
        );
    }

    // The steps, of every run in the tree.
    let steps: Vec<(&Ran, &record::Step)> = ran
        .iter()
        .flat_map(|r| r.record.steps().iter().map(move |s| (r, s)))
        .collect();
    if !steps.is_empty() {
        out.push_str(
            "<h2>Steps</h2><table><tr><th>Step</th><th>Of</th><th>Ended</th><th>Run</th></tr>",
        );
        for (parent, step) in steps {
            let ended = step.status().map_or("abandoned", Status::name);
            let id = step.recipe().map(|r| r.id.clone()).unwrap_or_default();
            let name = match id.is_empty() {
                true => escape(step.action()),
                false => format!("{} ({})", escape(step.action()), escape(&id)),
            };
            let _ = write!(
                out,
                "<tr><td>{name}</td><td>{}</td><td>{ended}</td><td class=\"mono\">{}</td></tr>",
                escape(parent.record.asked().action.as_deref().unwrap_or("")),
                escape(step.run())
            );
        }
        out.push_str("</table>");
    }

    // Each judge turn, read from what the run stored of it, with its cost.
    let turns: Vec<(&Ran, &Judged)> = ran
        .iter()
        .flat_map(|r| {
            (r.record.judge().iter())
                .filter(|j| matches!(j, Judged::Turn { .. }))
                .map(move |j| (r, j))
        })
        .collect();
    if !turns.is_empty() {
        out.push_str("<h2>The judge's turns</h2>");
        // A picture the judge was shown is the store's when an image of the run's tree has
        // its pixels: resolved against the store, not against what this report inlined.
        let held: std::collections::BTreeSet<PixelDigest> = (all_images.iter())
            .filter_map(|id| shelf.pixel_digest(*id))
            .collect();
        let found_pixels = |digest: PixelDigest| held.contains(&digest);
        for (at, (r, turn)) in turns.iter().enumerate() {
            let Judged::Turn {
                request,
                exchange,
                stop,
                usage,
                ..
            } = turn
            else {
                continue;
            };
            let paid = (r.record.spend().requests.iter())
                .find(|paid| paid.request() == *request)
                .map(|paid| paid.settled_micros());
            let _ = write!(
                out,
                "<div class=\"turn\"><p class=\"meta\">turn {} · request {} · {} · {}{}</p>",
                at + 1,
                request,
                stop.as_ref()
                    .map(|s| escape(
                        serde_json::to_value(s)
                            .unwrap_or_default()
                            .as_str()
                            .unwrap_or("")
                    ))
                    .unwrap_or_else(|| "no answer".to_owned()),
                paid.map_or_else(|| "nothing paid".to_owned(), usd),
                usage
                    .as_ref()
                    .map(|u| format!(" · {} tokens in, {} out", u.input_tokens, u.output_tokens))
                    .unwrap_or_default()
            );
            if at >= TURNS_SHOWN {
                out.push_str("<p class=\"quiet\">not shown: the report shows the first turns whole</p></div>");
                continue;
            }
            let read = exchange
                .as_deref()
                .and_then(|name| shelf.diagnostic(&r.id, name))
                .map(|bytes| written::for_display(&bytes, &found_pixels));
            match read {
                Some(Ok(shown)) => out.push_str(&answer_html(&shown)),
                Some(Err(why)) => {
                    let _ = write!(
                        out,
                        "<p class=\"note\">its exchange could not be read: {}</p>",
                        escape(&why.to_string())
                    );
                }
                None => out.push_str("<p class=\"note\">its exchange is not in the store</p>"),
            }
            out.push_str("</div>");
        }
    }

    // What was paid, for every run in the tree.
    let paid: Vec<(&Ran, &record::Paid)> = ran
        .iter()
        .flat_map(|r| r.record.spend().requests.iter().map(move |p| (r, p)))
        .collect();
    out.push_str("<h2>Cost</h2>");
    let models: Vec<String> = ran
        .iter()
        .flat_map(|r| r.record.models().iter())
        .map(|m| format!("{} · {} · {}", m.capability.name(), m.backend, m.model))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    if !models.is_empty() {
        let _ = write!(
            out,
            "<p class=\"quiet\">models: {}</p>",
            escape(&models.join("; "))
        );
    }
    if paid.is_empty() {
        out.push_str("<p>Nothing was paid: the work was local and free.</p>");
    } else {
        out.push_str(
            "<table><tr><th>Of</th><th>Request</th><th>Reserved</th><th>Settled</th>\
             <th>Unresolved</th></tr>",
        );
        let mut total = 0u64;
        for (r, p) in &paid {
            total = total.saturating_add(p.settled_micros());
            let _ = write!(
                out,
                "<tr><td>{}</td><td>{}</td><td class=\"mono\">{}</td><td class=\"mono\">{}</td>\
                 <td class=\"mono\">{}</td></tr>",
                escape(r.record.asked().action.as_deref().unwrap_or("")),
                p.request(),
                usd(p.reserved_micros()),
                usd(p.settled_micros()),
                usd(p.unresolved_micros())
            );
        }
        let _ = write!(
            out,
            "</table><p>Settled in all: <span class=\"mono\">{}</span></p>",
            usd(total)
        );
    }
    let _ = writeln!(
        out,
        "<p class=\"quiet\">Made by whizbang {} from the run's record. It loads nothing and \
         runs nothing.</p></body></html>",
        env!("CARGO_PKG_VERSION")
    );
    Ok(out)
}

/// The run's archive, a tar file: `record.json`, its record as `runs export` writes it;
/// `report.html`; and under `outputs/` each output as the artifact export writes it,
/// its canonical file, named by its port and type, and each picture beside it as a PNG.
/// No name in it comes from the store but a port's, which keeps the rule for names. It is
/// written by the core's ustar writer, the one a bundle is written by (ADR 0048), every
/// member's time the run's end.
pub fn archive(root: &Path, run: &RunId, title: Option<&str>) -> Result<Vec<u8>, String> {
    let shelf = Shelf::open(root)?;
    let record = shelf
        .record(run.as_str())
        .ok_or_else(|| "the store holds no record of that run".to_owned())?;
    let mtime = record.time().end_ms / 1000;
    let mut out = whizbang_core::bundle::tar::Writer::new(mtime);
    let mut tar_file = |name: &str, bytes: &[u8]| out.add(name, bytes).map_err(|e| e.to_string());
    let mut line = record.to_bytes().trim_ascii_end().to_vec();
    line.push(b'\n');
    tar_file("record.json", &line)?;
    let report = report(root, run, title)?;
    tar_file("report.html", report.as_bytes())?;
    let mut named = std::collections::BTreeSet::new();
    for (at, output) in record.outputs().iter().enumerate() {
        let plain: String = (output.port.chars())
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
            .take(64)
            .collect();
        // A name that is empty once cleaned, or taken already, is the output's place.
        let mut port = match plain.is_empty() {
            true => format!("output-{}", at + 1),
            false => plain,
        };
        while !named.insert(port.clone()) {
            port = format!("{port}-{}", at + 1);
        }
        let Some(held) = shelf.artifact(output.artifact) else {
            continue;
        };
        let kind = output.artifact_type.name();
        tar_file(&format!("outputs/{port}.{kind}"), &held.to_bytes())?;
        let png = match held.as_ref() {
            Artifact::Image(image) => image
                .pixels(&shelf.limits)
                .ok()
                .and_then(|p| whizbang_core::identity::encode::png(&p).ok()),
            Artifact::Mask(mask) => whizbang_core::identity::encode::png_of(
                mask.core(),
                mask.space().width(),
                mask.space().height(),
                whizbang_core::identity::encode::Channels::Grey,
            )
            .ok(),
            _ => None,
        };
        if let Some(png) = png {
            tar_file(&format!("outputs/{port}.png"), &png)?;
        }
    }
    Ok(out.finish())
}
