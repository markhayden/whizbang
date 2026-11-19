//! The replay of a recorded decision (plan 2C, task E2, items 4, 5 and 7): squint's judge
//! loop (`crates/squint-core/src/run.rs`, `drive`, at the commit `docs/sources.toml` pins),
//! run on whizbang's own parts, with a judge that answers the recorded turns. What it writes
//! is a transcript's body in the format of `docs/baseline/transcript.md`: every request the
//! judge was sent, what it answered, what each of its calls came to, the images it was shown
//! and how the decision ended. `whizbang eval baseline` compares it with the recorded one.
//!
//! **What is whizbang's, and what is squint's.** The words the judge reads are whizbang's
//! ports of squint's: the system prompt and the checklist ([`crate::use_case`]), the Tools
//! section and the presentation of a result ([`crate::details`]), the submission and the
//! verdict worked out from it. The pixels are whizbang's: `view` cuts with [`view::shown`],
//! `align` measures with [`align::measure`] and warps with [`align::warped`], and every
//! picture the judge is shown is prepared by the core ([`Piece::prepared`]). The loop's shape
//! is squint's: the opening, the batches and their budget, the checklist after each batch,
//! the final turn, the repair and the texts of each refusal.
//!
//! **`view` as squint's judge knew it** (item 4). The judge names a frame, as squint's did
//! (`raw:candidate`, `crop:call:1:0`, `aligned:candidate:t1`, or an image's id for its raw
//! frame), and is answered with squint's presentation: the crop's place on its parent as a
//! box of fractions, its size, and squint's summary. The action `view` and `check` keep
//! whizbang's form, which names an image by its identity; `check` taking squint's form is on
//! the carry-over list.
//!
//! What this does not reproduce, each a difference the comparison reports by place: a call
//! whose arguments break the tool's schema (squint's schema checker worded the refusal), a
//! submission that does not validate (whizbang's port words its reasons its own way), a
//! panorama, and a picture with transparency (squint encoded it as its own PNG).

use std::collections::BTreeMap;

use image::RgbImage;
use serde_json::{Value as Json, json};
use whizbang_core::capability::Picture;
use whizbang_core::contract::record::Decision;
use whizbang_core::egress::{DataClass, Piece};
use whizbang_core::identity::{CanonicalPixels, Digest};

use crate::details::{self, Projection};
use crate::job::{Job, Role};
use crate::use_case::{self, Checklist, Resolved, SUBMIT};
use crate::{align, view};

/// A crop smaller than this on either side is refused, as squint's `view` refused it.
const MIN_CROP: u32 = 8;

/// The refusal squint's `view` gave for angles on a flat frame.
const FLAT_FRAME: &str = "this frame is flat: send bbox [left, top, right, bottom] as fractions \
                          of it; yaw, pitch and fov are for equirectangular panoramas";

/// The budget a decision ran under: squint's `[budget]` and `[provider] max_output_tokens`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub tool_turns: u32,
    pub tool_calls: u32,
    pub repairs: u32,
    pub repair_output_tokens: u32,
    pub max_output_tokens: u32,
}

impl Default for Budget {
    /// Squint's defaults (`crates/squint-core/src/config.rs`).
    fn default() -> Budget {
        Budget {
            tool_turns: 4,
            tool_calls: 8,
            repairs: 1,
            repair_output_tokens: 32_000,
            max_output_tokens: 16_000,
        }
    }
}

/// One picture of the job, read by the caller: its pixels, and the two digests squint
/// recorded of its file.
#[derive(Debug, Clone)]
pub struct Shown {
    pub pixels: CanonicalPixels,
    /// The SHA-256 of the file, lowercase hex.
    pub sha256: String,
    /// Squint's digest of the decoded samples (`contract::pixel_sha256`), lowercase hex.
    pub pixel_sha256: String,
}

/// The judge of a replay: it answers each request with the response recorded for that
/// turn, numbered from 1, or `None` when the record holds no more.
pub trait Judge {
    fn answer(&mut self, turn: usize, request: &Json) -> Option<Json>;
}

/// What a replay came to: the parts of a transcript the decision makes.
#[derive(Debug, Clone, PartialEq)]
pub struct Replayed {
    pub evidence_tools: Vec<String>,
    pub submission_tool: String,
    pub images: Vec<Json>,
    pub turns: Vec<Json>,
    pub repairs: Vec<Json>,
    pub terminal: Json,
}

/// How a run ended early: squint's error class and its message, prefixed as squint's
/// `Display` prefixed it.
struct Ended {
    class: &'static str,
    message: String,
}

fn ended(class: &'static str, prefix: &str, message: &str) -> Ended {
    Ended {
        class,
        message: format!("{prefix}: {message}"),
    }
}

/// A frame the judge may name: its pixels and the image at its root.
struct Frame {
    pixels: CanonicalPixels,
    root: String,
}

/// What a tool returned, before it is presented.
struct Envelope {
    status: &'static str,
    summary: String,
    measurements: Json,
    regions: Json,
    images: Vec<Returned>,
}

struct Returned {
    frame: String,
    parent: String,
    mapping: Json,
    pixels: CanonicalPixels,
    root: String,
}

fn invalid_args(summary: String) -> Envelope {
    Envelope {
        status: "invalid_args",
        summary,
        measurements: json!([]),
        regions: json!([]),
        images: Vec::new(),
    }
}

/// `view` as squint's judge was told of it (`crates/squint-tools/manifests/view.toml`).
pub fn view_definition() -> Json {
    json!({
        "name": view::NAME,
        "description": "Shows a region closer: a crop of a flat frame by box, or a square \
            perspective view of a panorama by yaw, pitch and field of view; optionally the same \
            region in a second frame.",
        "input_schema": {
            "type": "object",
            "additionalProperties": false,
            "required": ["frame"],
            "properties": {
                "frame": {"type": "string", "description": "The frame to look into: a frame id \
                    (raw:candidate, or a crop a tool returned), or an image id (candidate) for its \
                    raw frame."},
                "also": {"type": "string", "description": "A second frame of the same projection \
                    to show the same region in, for comparison: a frame id (raw:source), or an \
                    image id (source) for its raw frame."},
                "bbox": {"type": "array", "items": {"type": "number", "minimum": 0, "maximum": 1},
                    "minItems": 4, "maxItems": 4, "description": "[left, top, right, bottom] as \
                    fractions of the frame, for a crop. Not with yaw, pitch and fov."},
                "yaw": {"type": "number", "minimum": -180, "exclusiveMaximum": 180,
                    "description": "Panoramas only, together with pitch and fov: degrees from the \
                    centre column, increasing to the right. A flat frame takes bbox instead."},
                "pitch": {"type": "number", "minimum": -90, "maximum": 90, "description":
                    "Panoramas only, with yaw and fov: degrees above the horizon."},
                "fov": {"type": "number", "minimum": 10, "maximum": 120, "description":
                    "Panoramas only, with yaw and pitch: horizontal field of view in degrees of \
                    the square perspective view."},
            },
        },
    })
}

/// `align` as squint's judge was told of it.
fn align_definition() -> Result<Json, String> {
    let tool = details::align_tool()?;
    Ok(json!({
        "name": tool.name(),
        "description": tool.description(),
        "input_schema": tool.schema(),
    }))
}

/// The run's frames, by name, and the images of the job.
struct Frames {
    frames: BTreeMap<String, Frame>,
    images: Vec<String>,
}

impl Frames {
    /// A frame named in a call: by its name, or by an image's id for its raw frame.
    fn named(&self, given: &str) -> Option<String> {
        if self.frames.contains_key(given) {
            return Some(given.to_owned());
        }
        let raw = format!("raw:{given}");
        (self.images.iter().any(|id| id == given) && self.frames.contains_key(&raw)).then_some(raw)
    }
}

/// One crop as squint's `view` cut it: the box snapped outward to whole pixels, refused
/// under [`MIN_CROP`], scaled down past [`view::MAX_SIDE`] by `view`'s one method.
fn crop(frame: &Frame, b: &[f64; 4]) -> Result<(CanonicalPixels, Json, String), String> {
    let (w, h) = (frame.pixels.width(), frame.pixels.height());
    let cut = view::snapped(b, w, h);
    let (x0, y0, x1, y1) = (cut[0], cut[1], cut[2], cut[3]);
    if x1 - x0 < MIN_CROP || y1 - y0 < MIN_CROP {
        return Err(format!(
            "bbox {b:?} covers only {}×{} pixels; at least {MIN_CROP}×{MIN_CROP} are needed",
            x1 - x0,
            y1 - y0
        ));
    }
    let pixels = view::shown(&frame.pixels, cut).map_err(|why| why.to_string())?;
    let at = |v: u32, of: u32| f64::from(v) / f64::from(of);
    let mapping = json!({"bbox": [at(x0, w), at(y0, h), at(x1, w), at(y1, h)]});
    let text = format!(
        "Crop [{:.3}, {:.3}, {:.3}, {:.3}]",
        at(x0, w),
        at(y0, h),
        at(x1, w),
        at(y1, h)
    );
    Ok((pixels, mapping, text))
}

/// The box a call of `view` asks for, or squint's words for why it asks for none. Every
/// frame here is flat.
fn requested(args: &Json) -> Result<[f64; 4], String> {
    let number = |key: &str| -> Result<Option<f64>, String> {
        match args.get(key) {
            None | Some(Json::Null) => Ok(None),
            Some(v) => v
                .as_f64()
                .filter(|f| f.is_finite())
                .map(Some)
                .ok_or_else(|| format!("{key} must be a number")),
        }
    };
    let bbox = match args.get("bbox") {
        None | Some(Json::Null) => None,
        Some(Json::Array(a)) if a.len() == 4 => {
            let mut b = [0.0; 4];
            for (slot, v) in b.iter_mut().zip(a) {
                *slot = v
                    .as_f64()
                    .filter(|f| f.is_finite())
                    .ok_or_else(|| "bbox entries must be numbers".to_owned())?;
            }
            Some(b)
        }
        Some(_) => return Err("bbox must be [left, top, right, bottom]".to_owned()),
    };
    let angles = [number("yaw")?, number("pitch")?, number("fov")?]
        .iter()
        .filter(|v| v.is_some())
        .count();
    match (bbox, angles) {
        (Some(_), n) if n > 0 => Err("give either bbox or yaw, pitch and fov, not both".to_owned()),
        (Some(b), _) => {
            if b.iter().any(|v| !(0.0..=1.0).contains(v)) || b[0] >= b[2] || b[1] >= b[3] {
                return Err(format!(
                    "bbox {b:?} must be fractions with left < right and top < bottom"
                ));
            }
            Ok(b)
        }
        (None, 0) => {
            Err("give bbox for a crop, or yaw, pitch and fov for a perspective view".to_owned())
        }
        (None, _) => Err(FLAT_FRAME.to_owned()),
    }
}

/// A call of `view`, answered as squint's `view` answered it: each crop is the frame
/// `crop:<call>:<n>`, cut from the frame named, and the summary says what was cut of which.
fn view_call(frames: &Frames, call_id: &str, args: &Json) -> Envelope {
    let Some(frame_id) = args.get("frame").and_then(Json::as_str) else {
        return invalid_args("frame must be a frame id".to_owned());
    };
    let Some(frame) = frames.frames.get(frame_id) else {
        return invalid_args(format!("frame: unknown frame id {frame_id:?}"));
    };
    let also = match args.get("also") {
        None | Some(Json::Null) => None,
        Some(Json::String(id)) if id == frame_id => {
            return invalid_args("also must name a different frame from frame".to_owned());
        }
        Some(Json::String(id)) => match frames.frames.get(id) {
            Some(other) => Some((id.as_str(), other)),
            None => return invalid_args(format!("also: unknown frame id {id:?}")),
        },
        Some(_) => return invalid_args("also must be a frame id".to_owned()),
    };
    let b = match requested(args) {
        Ok(b) => b,
        Err(why) => return invalid_args(why),
    };
    let mut images = Vec::new();
    let mut summary = String::new();
    for (n, (id, frame)) in std::iter::once((frame_id, frame)).chain(also).enumerate() {
        let (pixels, mapping, text) = match crop(frame, &b) {
            Ok(cut) => cut,
            Err(why) => return invalid_args(why),
        };
        if n == 0 {
            summary = format!("{text} of {id}");
        } else {
            summary.push_str(&format!(", and the same of {id}"));
        }
        images.push(Returned {
            frame: format!("crop:{call_id}:{n}"),
            parent: id.to_owned(),
            mapping,
            pixels,
            root: frame.root.clone(),
        });
    }
    let sizes: Vec<String> = images
        .iter()
        .map(|i| format!("{} {}×{}", i.frame, i.pixels.width(), i.pixels.height()))
        .collect();
    summary.push_str(&format!(" ({}).", sizes.join(", ")));
    Envelope {
        status: "ok",
        summary,
        measurements: json!([]),
        regions: json!([]),
        images,
    }
}

fn rgb(pixels: &CanonicalPixels) -> Result<RgbImage, String> {
    let samples: Vec<u8> = pixels
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    RgbImage::from_raw(pixels.width(), pixels.height(), samples)
        .ok_or_else(|| "the pixels do not make an image".to_owned())
}

fn opaque(rgb: &RgbImage) -> Result<CanonicalPixels, String> {
    let rgba: Vec<u8> = rgb
        .pixels()
        .flat_map(|p| [p.0[0], p.0[1], p.0[2], 255])
        .collect();
    CanonicalPixels::from_rgba(rgb.width(), rgb.height(), rgba).map_err(|why| why.to_string())
}

/// A call of `align`, answered as squint's `align` answered it, then under the projection.
/// `Err` is a failure of the tool, which ends the run.
fn align_call(frames: &Frames, args: &Json, projection: Projection) -> Result<Envelope, Ended> {
    let tool = |why: String| ended("tool", "tool", &why);
    let id = |key: &str| args.get(key).and_then(Json::as_str);
    let (Some(image), Some(against)) = (id("image"), id("against")) else {
        return Ok(invalid_args(
            "image and against must be image ids".to_owned(),
        ));
    };
    if image == against {
        return Ok(invalid_args(
            "image and against must be two different images".to_owned(),
        ));
    }
    let raw = |id: &str| frames.frames.get(&format!("raw:{id}"));
    let (Some(img), Some(tgt)) = (raw(image), raw(against)) else {
        let unknown: Vec<String> = [image, against]
            .iter()
            .filter(|i| raw(i).is_none())
            .map(|i| format!("{i:?}"))
            .collect();
        return Ok(invalid_args(format!(
            "unknown image id: {}",
            unknown.join(", ")
        )));
    };
    let (img, tgt) = (
        rgb(&img.pixels).map_err(tool)?,
        rgb(&tgt.pixels).map_err(tool)?,
    );
    let (found, h) = align::measure(&img, &tgt, &align::Options::default())
        .map_err(|why| tool(format!("align: {why}")))?;
    let mut images = Vec::new();
    let mut regions = json!([]);
    let status = match (found.status, h, &found.transform) {
        (align::Status::Registered, Some(h), Some(transform)) => {
            let warped = align::warped(&img, &tgt, &h).map_err(|why| tool(why.to_string()))?;
            images.push(Returned {
                frame: align::aligned_frame(image),
                parent: format!("raw:{against}"),
                mapping: json!({"homography": transform.params.to_vec()}),
                pixels: opaque(&warped).map_err(tool)?,
                root: image.to_owned(),
            });
            "ok"
        }
        (align::Status::NotRegistered, ..) => {
            regions = json!([{
                "label": format!("not registered onto {against}"),
                "score": (1.0 - found.quality.ncc.max(0.0)).clamp(0.0, 1.0),
                "location": {"frame": format!("raw:{image}"), "kind": "whole_image",
                             "bbox": null, "polygon": null, "yaw": null, "pitch": null,
                             "fov": null},
            }]);
            "not_applicable"
        }
        _ => "not_applicable",
    };
    if projection == Projection::VisualOnly {
        let computed = status == "ok" && !images.is_empty();
        return Ok(Envelope {
            status: if computed { "ok" } else { "not_applicable" },
            summary: if computed {
                details::VISUAL_ONLY_COMPUTED
            } else {
                details::VISUAL_ONLY_UNAVAILABLE
            }
            .to_owned(),
            measurements: json!([]),
            regions: json!([]),
            images,
        });
    }
    let measurements: Vec<Json> = found
        .measurements
        .iter()
        .map(|m| json!({"name": m.name, "value": m.value, "unit": m.unit, "frame": null}))
        .collect();
    Ok(Envelope {
        status,
        summary: align::summary(&found, image, against),
        measurements: json!(measurements),
        regions,
        images,
    })
}

/// What squint's loop made of a response.
enum Class {
    Submission,
    Batch,
    Mixed,
    Excluded(&'static str),
}

fn classify(response: &Json) -> Class {
    match response.get("stop_reason").and_then(Json::as_str) {
        Some("max_tokens") => return Class::Excluded("truncated"),
        Some("refusal") => return Class::Excluded("refusal"),
        _ => {}
    }
    let calls = tool_calls(response);
    let submits = calls
        .iter()
        .filter(|c| c.get("name").and_then(Json::as_str) == Some(SUBMIT))
        .count();
    match (calls.len(), submits) {
        (0, _) => Class::Excluded("no_call"),
        (1, 1) => Class::Submission,
        (_, 0) => Class::Batch,
        _ => Class::Mixed,
    }
}

fn tool_calls(response: &Json) -> Vec<Json> {
    response
        .get("tool_calls")
        .and_then(Json::as_array)
        .cloned()
        .unwrap_or_default()
}

/// The stop reason as squint's `Debug` wrote it into a message.
fn stop_debug(response: &Json) -> String {
    match response.get("stop_reason") {
        Some(Json::String(word)) => match word.as_str() {
            "end_turn" => "EndTurn".to_owned(),
            "tool_use" => "ToolUse".to_owned(),
            "max_tokens" => "MaxTokens".to_owned(),
            "refusal" => "Refusal".to_owned(),
            other => format!("Other({other:?})"),
        },
        Some(other) => match other.get("other").and_then(Json::as_str) {
            Some(word) => format!("Other({word:?})"),
            None => other.to_string(),
        },
        None => "EndTurn".to_owned(),
    }
}

fn describe(reason: &str) -> &'static str {
    match reason {
        "truncated" => "the response was truncated",
        "refusal" => "the model refused",
        _ => "the model answered without calling a tool",
    }
}

/// The assistant message as the judge must see it again: its content in order when the
/// response gave it, otherwise its texts and then its calls.
fn assistant(response: &Json) -> Json {
    let content = response
        .get("content")
        .and_then(Json::as_array)
        .filter(|c| !c.is_empty())
        .cloned()
        .unwrap_or_else(|| {
            let mut content: Vec<Json> = response
                .get("text")
                .and_then(Json::as_array)
                .map(|texts| {
                    texts
                        .iter()
                        .map(|t| json!({"type": "text", "text": t}))
                        .collect()
                })
                .unwrap_or_default();
            for c in tool_calls(response) {
                content.push(json!({"type": "tool_use", "id": c["id"], "name": c["name"],
                                    "arguments": c["arguments"]}));
            }
            content
        });
    json!({"role": "assistant", "content": content})
}

fn text(text: &str) -> Json {
    json!({"type": "text", "text": text})
}

fn result(call: &Json, content: &str, is_error: bool, images: Vec<String>) -> Json {
    json!({"type": "tool_result", "tool_use_id": call["id"], "content": content,
           "is_error": is_error, "images": images})
}

/// The repair message after a rejected response: an error result for each call, then the
/// repair's words, and the final turn's again when inside it.
fn rejection(calls: &[Json], reason: &str, last: Option<&str>) -> Json {
    let mut content: Vec<Json> = calls
        .iter()
        .map(|c| result(c, reason, true, Vec::new()))
        .collect();
    content.push(text(&format!(
        "Repair: call {SUBMIT} once, alone, with exactly one disposition per checklist item. \
         This is the only repair.{}",
        last.map(|i| format!("\n\n{i}")).unwrap_or_default()
    )));
    json!({"role": "user", "content": content})
}

fn discarded(last: Option<&str>) -> Json {
    json!({"role": "user", "content": [text(&format!(
        "Your previous output was discarded: it contained no tool call. Inspect the images and \
         call a tool, or call {SUBMIT} with exactly one disposition per checklist item.{}",
        last.map(|i| format!("\n\n{i}")).unwrap_or_default()
    ))]})
}

/// One run of the loop.
struct Run<'a> {
    job: &'a Job,
    resolved: Resolved,
    checklist: Checklist,
    budget: Budget,
    long_edge: u32,
    projection: Projection,
    judge: &'a mut dyn Judge,
    system: String,
    tools: Vec<Json>,
    frames: Frames,
    images: Vec<Json>,
    turns: Vec<Json>,
    repairs: Vec<Json>,
    repairs_used: u32,
    next_call: u32,
}

impl Run<'_> {
    /// The prepared bytes' digest and size of a picture the judge is shown.
    fn prepared(&self, pixels: &CanonicalPixels) -> Result<Json, Ended> {
        let picture = Picture::new(pixels.clone())
            .map_err(|why| ended("image", "image", &why.to_string()))?;
        let piece = Piece::prepared(DataClass::DerivedImages, &picture, self.long_edge)
            .map_err(|why| ended("image", "image", &format!("cannot prepare: {why:?}")))?;
        Ok(json!({"sha256": Digest::of_bytes(piece.bytes()).hex(),
                  "bytes": piece.bytes().len()}))
    }

    fn checklist_text(&self) -> String {
        self.checklist.render(&self.resolved)
    }

    /// Sends one request and records the turn.
    fn send(&mut self, history: &[Json], allowance: u32) -> Result<Json, Ended> {
        let number = self.turns.len() + 1;
        let request = json!({
            "system": self.system,
            "history": history,
            "tools": self.tools,
            "max_output_tokens": allowance,
        });
        let answer = self.judge.answer(number, &request);
        let Some(response) = answer else {
            self.turns
                .push(json!({"turn": number, "request": request, "response": null,
                                   "excluded": null, "calls": [], "meta": {}}));
            return Err(ended("provider", "provider", "the recorded turns ran out"));
        };
        let kept = json!({
            "content": response.get("content").cloned().unwrap_or(json!([])),
            "text": response.get("text").cloned().unwrap_or(json!([])),
            "tool_calls": response.get("tool_calls").cloned().unwrap_or(json!([])),
            "stop_reason": response.get("stop_reason").cloned().unwrap_or(json!("end_turn")),
        });
        self.turns
            .push(json!({"turn": number, "request": request, "response": kept,
                               "excluded": null, "calls": [], "meta": {}}));
        Ok(kept)
    }

    /// Records what a call of the last turn came to.
    fn settled(&mut self, call: &Json, disposition: &str, result: Json) {
        if let Some(turn) = self.turns.last_mut()
            && let Some(calls) = turn.get_mut("calls").and_then(Json::as_array_mut)
        {
            calls.push(json!({"id": call["id"], "name": call["name"],
                              "arguments": call["arguments"], "disposition": disposition,
                              "result": result, "meta": {}}));
        }
    }

    fn excluded(&mut self, reason: &str, response: &Json) {
        if let Some(turn) = self.turns.last_mut() {
            turn["excluded"] = json!({"reason": reason});
        }
        for call in tool_calls(response) {
            self.settled(&call, "excluded", Json::Null);
        }
    }

    fn rejected_all(&mut self, calls: &[Json], reason: &str) {
        for call in calls {
            self.settled(
                call,
                "rejected",
                json!({"content": reason, "is_error": true, "images": []}),
            );
        }
    }

    fn take_repair(&mut self, form: &str, reason: &str) -> bool {
        if self.repairs_used >= self.budget.repairs {
            return false;
        }
        self.repairs_used += 1;
        self.repairs
            .push(json!({"after_turn": self.turns.len(), "form": form, "reason": reason}));
        true
    }

    /// Answers one call of a batch: refused before the tool when it names no tool or an
    /// image or frame the run does not have, otherwise run and presented.
    fn execute(&mut self, call: &Json) -> Result<Json, Ended> {
        let call_id = format!("call:{}", self.next_call);
        self.next_call += 1;
        let name = call.get("name").and_then(Json::as_str).unwrap_or_default();
        let mut arguments = call.get("arguments").cloned().unwrap_or(Json::Null);
        let refuse = |run: &mut Self, why: String| {
            let said = format!("invalid arguments: {why}");
            run.settled(
                call,
                "rejected",
                json!({"content": said, "is_error": true, "images": []}),
            );
            Ok(result(call, &said, true, Vec::new()))
        };
        let (image_args, frame_args): (&[&str], &[&str]) = match name {
            "view" => (&[], &["frame", "also"]),
            "align" => (&["image", "against"], &[]),
            _ => return refuse(self, format!("unknown tool {name:?}")),
        };
        if !arguments.is_object() {
            return refuse(self, "the arguments are not an object".to_owned());
        }
        // An image named by its raw frame is passed on as its id; a frame named by an image's
        // id is passed on as that image's raw frame.
        for arg in image_args {
            let Some(given) = arguments
                .get(*arg)
                .and_then(Json::as_str)
                .map(str::to_owned)
            else {
                continue;
            };
            let id = given.strip_prefix("raw:").unwrap_or(&given).to_owned();
            if !self.frames.images.contains(&id) {
                return refuse(self, format!("{arg}: unknown image id {given:?}"));
            }
            arguments[*arg] = json!(id);
        }
        for arg in frame_args {
            let Some(given) = arguments
                .get(*arg)
                .and_then(Json::as_str)
                .map(str::to_owned)
            else {
                continue;
            };
            let Some(frame) = self.frames.named(&given) else {
                return refuse(self, format!("{arg}: unknown frame id {given:?}"));
            };
            arguments[*arg] = json!(frame);
        }
        let envelope = match name {
            "view" => view_call(&self.frames, &call_id, &arguments),
            _ => align_call(&self.frames, &arguments, self.projection)?,
        };
        if envelope.status == "invalid_args" {
            return refuse(self, envelope.summary);
        }
        let mut shown = Vec::new();
        let mut listed = Vec::new();
        for returned in envelope.images {
            let prepared = self.prepared(&returned.pixels)?;
            self.images.push(json!({
                "id": returned.frame, "frame": returned.frame, "origin": call["id"],
                "width": returned.pixels.width(), "height": returned.pixels.height(),
                "parent": returned.parent, "root": returned.root, "mapping": returned.mapping,
                "prepared": prepared,
            }));
            listed.push(json!({"frame": returned.frame, "parent": returned.parent,
                               "mapping": returned.mapping}));
            shown.push(returned.frame.clone());
            self.frames.frames.insert(
                returned.frame,
                Frame {
                    pixels: returned.pixels,
                    root: returned.root,
                },
            );
        }
        let said = details::presentation(
            &call_id,
            envelope.status,
            &envelope.summary,
            envelope.measurements,
            envelope.regions,
            json!(listed),
        );
        self.settled(
            call,
            "executed",
            json!({"content": said, "is_error": false, "images": shown}),
        );
        Ok(result(call, &said, false, shown))
    }

    /// The verdict a submission states, or squint's error for one that states none.
    fn finish(&mut self, call: &Json) -> Result<String, String> {
        let arguments = call.get("arguments").cloned().unwrap_or(Json::Null);
        let decided = use_case::decided_of(&arguments, self.job, &self.resolved, &self.checklist)?;
        self.settled(call, "final", Json::Null);
        Ok(match decided.verdict.decision() {
            Decision::Pass => "pass",
            Decision::Fail => "fail",
            Decision::Unsure => "unsure",
        }
        .to_owned())
    }

    /// Squint's `drive`, from the first request to the verdict or the error.
    fn drive(&mut self, opening: Json) -> Result<String, Ended> {
        let mut history = vec![opening];
        let mut calls_used: u32 = 0;
        let mut turns_used: u32 = 0;
        let mut allowance = self.budget.max_output_tokens;
        while turns_used < self.budget.tool_turns {
            let response = self.send(&history, allowance)?;
            allowance = self.budget.max_output_tokens;
            let calls = tool_calls(&response);
            match classify(&response) {
                Class::Submission => {
                    history.push(assistant(&response));
                    match self.finish(&calls[0]) {
                        Ok(verdict) => return Ok(verdict),
                        Err(message) => {
                            if !self.take_repair("rejected", "invalid_submission") {
                                self.settled(&calls[0], "final", Json::Null);
                                return Err(ended("submission_invalid", "submission", &message));
                            }
                            let said = format!("Submission rejected: {message}");
                            self.rejected_all(&calls, &said);
                            history.push(rejection(&calls, &said, None));
                        }
                    }
                }
                Class::Mixed => {
                    history.push(assistant(&response));
                    if !self.take_repair("rejected", "mixed_batch") {
                        return Err(ended(
                            "model_output",
                            "model_output",
                            "submit_verdict must be the only call in its batch (mixed batch), \
                             again after the repair",
                        ));
                    }
                    let said =
                        format!("Not executed: {SUBMIT} must be the only call in its batch.");
                    self.rejected_all(&calls, &said);
                    history.push(rejection(&calls, &said, None));
                }
                Class::Excluded(reason) => {
                    self.excluded(reason, &response);
                    if !self.take_repair("reissue", reason) {
                        return Err(ended(
                            "model_output",
                            "model_output",
                            &format!(
                                "{} (stop reason {}), again after the repair",
                                describe(reason),
                                stop_debug(&response)
                            ),
                        ));
                    }
                    if reason == "truncated" {
                        allowance = self.budget.repair_output_tokens;
                    } else {
                        history.push(discarded(None));
                    }
                }
                Class::Batch => {
                    history.push(assistant(&response));
                    turns_used += 1;
                    let remaining = self.budget.tool_calls.saturating_sub(calls_used);
                    if calls.len() as u64 > u64::from(remaining) {
                        let said = format!(
                            "Not executed: this batch of {} calls exceeds the remaining call \
                             budget of {remaining}. Submit your verdict now.",
                            calls.len()
                        );
                        let results: Vec<Json> = calls
                            .iter()
                            .map(|c| result(c, &said, true, Vec::new()))
                            .collect();
                        self.rejected_all(&calls, &said);
                        history.push(json!({"role": "user", "content": results}));
                        break;
                    }
                    let mut results = Vec::new();
                    for call in &calls {
                        results.push(self.execute(call)?);
                    }
                    calls_used += u32::try_from(calls.len()).unwrap_or(u32::MAX);
                    results.push(text(&self.checklist_text()));
                    history.push(json!({"role": "user", "content": results}));
                    if calls_used >= self.budget.tool_calls {
                        break;
                    }
                }
            }
        }
        // The final turn: only the submission is executed.
        let instruction = format!(
            "Tool budget exhausted. Only {SUBMIT} will be executed now; any other call is \
             answered with an error. Submit your verdict with exactly one disposition per \
             checklist item.\n\n{}",
            self.checklist_text()
        );
        history.push(json!({"role": "system", "content": [text(&instruction)]}));
        loop {
            let response = self.send(&history, allowance)?;
            allowance = self.budget.max_output_tokens;
            let calls = tool_calls(&response);
            match classify(&response) {
                Class::Submission => {
                    history.push(assistant(&response));
                    match self.finish(&calls[0]) {
                        Ok(verdict) => return Ok(verdict),
                        Err(message) => {
                            if !self.take_repair("rejected", "invalid_submission") {
                                self.settled(&calls[0], "final", Json::Null);
                                return Err(ended("submission_invalid", "submission", &message));
                            }
                            let said = format!("Submission rejected: {message}");
                            self.rejected_all(&calls, &said);
                            history.push(rejection(&calls, &said, Some(&instruction)));
                        }
                    }
                }
                Class::Batch | Class::Mixed => {
                    history.push(assistant(&response));
                    let said =
                        format!("Not available: only {SUBMIT} is executed on the final turn.");
                    if !self.take_repair("rejected", "stray_call") {
                        let names: Vec<&str> = calls
                            .iter()
                            .map(|c| c.get("name").and_then(Json::as_str).unwrap_or_default())
                            .collect();
                        return Err(ended(
                            "no_submission",
                            "no_submission",
                            &format!(
                                "the final turn called {names:?} instead of submitting, again \
                                 after the repair"
                            ),
                        ));
                    }
                    self.rejected_all(&calls, &said);
                    history.push(rejection(&calls, &said, Some(&instruction)));
                }
                Class::Excluded(reason) => {
                    self.excluded(reason, &response);
                    if !self.take_repair("reissue", reason) {
                        return Err(ended(
                            "model_output",
                            "model_output",
                            &format!(
                                "the final turn ended with {} (stop reason {}), again after the \
                                 repair",
                                describe(reason),
                                stop_debug(&response)
                            ),
                        ));
                    }
                    if reason == "truncated" {
                        allowance = self.budget.repair_output_tokens;
                    } else {
                        history.push(discarded(Some(&instruction)));
                    }
                }
            }
        }
    }
}

/// Replays one decision of arm C: the job and its pictures, in the job's order; the budget
/// it ran under; the long edge each picture is fitted to; how a call of `align` is answered;
/// and the judge that answers the recorded turns. `Err` is a job this cannot replay.
pub fn replay(
    job: &Job,
    pictures: &[Shown],
    budget: &Budget,
    long_edge: u32,
    projection: Projection,
    judge: &mut dyn Judge,
) -> Result<Replayed, String> {
    if pictures.len() != job.images.len() {
        return Err("the job names one picture for each of its images".to_owned());
    }
    if job
        .images
        .iter()
        .any(|i| i.projection.as_deref().is_some_and(|p| p != "flat"))
    {
        return Err("a panorama is not replayed".to_owned());
    }
    let resolved = use_case::resolve(job)?;
    let checklist = Checklist::build(&resolved, job);
    let names = [view::NAME, align::NAME];
    let system = use_case::system_with(&resolved, job, &details::tools_section(&names));
    let submit = use_case::submit_definition().ok_or("the submission's schema is not read")?;
    let tools = vec![view_definition(), align_definition()?, submit];
    let mut run = Run {
        job,
        resolved,
        checklist,
        budget: *budget,
        long_edge,
        projection,
        judge,
        system,
        tools,
        frames: Frames {
            frames: BTreeMap::new(),
            images: job.images.iter().map(|i| i.id.clone()).collect(),
        },
        images: Vec::new(),
        turns: Vec::new(),
        repairs: Vec::new(),
        repairs_used: 0,
        next_call: 1,
    };
    let mut opening = Vec::new();
    for (image, shown) in job.images.iter().zip(pictures) {
        let (width, height) = (shown.pixels.width(), shown.pixels.height());
        let prepared = run.prepared(&shown.pixels).map_err(|e| e.message)?;
        run.images.push(json!({
            "id": image.id, "frame": format!("raw:{}", image.id), "origin": "job",
            "role": image.role.word(), "width": width, "height": height,
            "source": {"sha256": shown.sha256, "pixel_sha256": shown.pixel_sha256},
            "prepared": prepared,
        }));
        run.frames.frames.insert(
            format!("raw:{}", image.id),
            Frame {
                pixels: shown.pixels.clone(),
                root: image.id.clone(),
            },
        );
        let role = match image.role {
            Role::Candidate => "Candidate",
            Role::Source => "Source",
            Role::Reference => "Reference",
        };
        let label = image
            .label
            .as_deref()
            .map(|l| format!(", {}", use_case::clean(l)))
            .unwrap_or_default();
        opening.push(text(&format!(
            "Image `{}` (role {role}{label}, frame raw:{}, {width}x{height}, Flat).",
            image.id, image.id
        )));
        opening.push(json!({"type": "image", "image_id": image.id}));
    }
    opening.push(text(&run.checklist_text()));
    opening.push(text(&format!(
        "Inspect the images, use tools where a closer look would change your answer (at most \
         {} calls over {} turns), then call submit_verdict with one disposition per checklist \
         item.",
        budget.tool_calls, budget.tool_turns
    )));
    let terminal = match run.drive(json!({"role": "user", "content": opening})) {
        Ok(verdict) => json!({"kind": "verdict", "verdict": verdict}),
        Err(e) => json!({"kind": "error", "class": e.class, "message": e.message}),
    };
    Ok(Replayed {
        evidence_tools: names.iter().map(|n| (*n).to_owned()).collect(),
        submission_tool: SUBMIT.to_owned(),
        images: run.images,
        turns: run.turns,
        repairs: run.repairs,
        terminal,
    })
}
