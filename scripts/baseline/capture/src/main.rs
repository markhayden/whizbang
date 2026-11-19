//! Captures three decisions from frozen squint, offline, in the layout of the baseline package
//! that `scripts/baseline/convert.py` reads (plan 2C, task E2, item 3).
//!
//!     baseline-capture <package folder>
//!
//! Each decision is squint's own `run::check`, with the tools of arm C (`view`, and `align`
//! under `visual_only`), on synthetic pictures made here, judged by a provider that answers
//! scripted turns and prepares every picture as squint's adapter prepared it for
//! `claude-haiku-5-5` (`crates/squint-providers/src/prepare.rs`, with the model's limits from
//! the shipped catalogue). No model is called and nothing leaves the machine. Nothing in what
//! it writes is anyone's but this program's.
//!
//! What it writes under the package folder: the set (`evalsets/source-fidelity/synthetic-v1/`,
//! its jobs and pictures), each decision under `replay/synthetic/<item>/` with the `result.json`
//! squint's runner wrote beside it, and `assembly/generated/preparation.json`. `run.sh` adds
//! the package digest and converts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use image::{DynamicImage, Rgb, RgbImage};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use squint_core::config::Config;
use squint_core::image::{ImagePlan, Prepared};
use squint_core::job::Job;
use squint_core::ledger::Pricing;
use squint_core::provider::{Provider, SendFuture, TurnRequest, TurnResponse};
use squint_core::run;
use squint_core::tool::ToolRegistry;
use squint_providers::catalog::{Catalog, Images};
use squint_providers::prepare::{PREP_VERSION, Preparer};
use squint_testkit::ScriptedProvider;

const MODEL: &str = "claude-haiku-5-5";
/// The set of the three ordinary decisions, and of the one whose crop is scaled down.
const SET: &str = "source-fidelity/synthetic-v1";
const SCALED_SET: &str = "source-fidelity/scaled-v1";

const CONFIG: &str = r#"
[provider]
profile = "anthropic"
model = "claude-haiku-5-5"

[tools]
enabled = ["view", "align"]

[privacy]
cloud_calls = true
account = "anthropic:synthetic"

[output]
dir = "./squint-out"
"#;

/// A scripted judge that prepares pictures as squint's adapter did for the model.
struct Capture {
    script: ScriptedProvider,
    images: Images,
}

impl Provider for Capture {
    fn id(&self) -> &str {
        "anthropic-messages"
    }

    fn send<'a>(&'a self, req: TurnRequest<'a>) -> SendFuture<'a> {
        self.script.send(req)
    }

    fn pricing(&self) -> Option<&dyn Pricing> {
        self.script.pricing()
    }

    fn prep_version(&self) -> &str {
        PREP_VERSION
    }

    fn prepare_image(
        &self,
        id: &str,
        pixels: &DynamicImage,
        plan: ImagePlan,
    ) -> squint_core::Result<Prepared> {
        Preparer::new(&self.images, None, plan).prepare(id, pixels)
    }
}

/// One value of a fixed sequence, from a position: the pictures are the same on every run.
fn noise(x: u32, y: u32, salt: u32) -> u8 {
    let mut v = x
        .wrapping_mul(374_761_393)
        .wrapping_add(y.wrapping_mul(668_265_263))
        .wrapping_add(salt.wrapping_mul(2_246_822_519));
    v = (v ^ (v >> 13)).wrapping_mul(1_274_126_177);
    (v ^ (v >> 16)) as u8
}

/// A scene of 96 by 72 pixels: blocks of texture, a sky, a door and a bicycle.
fn scene(shift: (i64, i64), door: Rgb<u8>) -> RgbImage {
    let (w, h) = (96u32, 72u32);
    RgbImage::from_fn(w, h, |x, y| {
        let (sx, sy) = (x as i64 - shift.0, y as i64 - shift.1);
        let (bx, by) = (sx.rem_euclid(400) as u32 / 4, sy.rem_euclid(400) as u32 / 4);
        let n = noise(bx, by, 7);
        if (30..46).contains(&sx) && (34..62).contains(&sy) {
            return door;
        }
        if (60..80).contains(&sx) && (44..52).contains(&sy) {
            return Rgb([30, 60, 200]);
        }
        if sy < 20 {
            return Rgb([120 + n / 8, 170 + n / 8, 230]);
        }
        Rgb([90 + n / 2, 70 + n / 3, 50 + n / 4])
    })
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("a folder");
    std::fs::write(path, bytes).expect("written");
}

fn png(img: &RgbImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .expect("encoded");
    bytes
}

/// Writes one item of the set: its candidate, its family's source and its job.
fn item(
    set: &Path,
    set_id: &str,
    name: &str,
    candidate: &RgbImage,
    source_path: &str,
    source: &[u8],
) -> PathBuf {
    let candidate = png(candidate);
    let candidate_path = format!("items/{name}/candidate.png");
    write(&set.join(&candidate_path), &candidate);
    let job = format!(
        r#"# Item of {set_id}; made by scripts/baseline/capture.
[job]
use_case = "source-fidelity"
brief = "Restyle the scene; keep the structure and every named subject."

[[job.requirements]]
id = "RQ1"
kind = "must_hold"
scope = ["candidate"]
evidence = ["source"]
text = "The front door keeps its identity, shape, position and extent relative to the source."

[[job.images]]
id = "candidate"
role = "candidate"
source = {{ path = "{candidate_path}", sha256 = "{}" }}

[[job.images]]
id = "source"
role = "source"
source = {{ path = "{source_path}", sha256 = "{}" }}
"#,
        sha256(&candidate),
        sha256(source),
    );
    let path = set.join(format!("items/{name}/job.toml"));
    write(&path, job.as_bytes());
    path
}

fn tool_use(id: &str, name: &str, arguments: Value) -> Value {
    json!({"type": "tool_use", "id": id, "name": name, "arguments": arguments})
}

/// A turn that calls tools, as an adapter that keeps its blocks in order reports one.
fn calls(text: &str, uses: &[Value]) -> Value {
    let mut content = vec![json!({"type": "text", "text": text})];
    content.extend(uses.iter().cloned());
    let tool_calls: Vec<Value> = uses
        .iter()
        .map(|u| json!({"id": u["id"], "name": u["name"], "arguments": u["arguments"]}))
        .collect();
    json!({
        "text": [text],
        "tool_calls": tool_calls,
        "content": content,
        "stop_reason": "tool_use",
        "usage": {"input_tokens": 1200, "output_tokens": 80},
    })
}

/// The submission: one answer for each item of the checklist.
fn submit(id: &str, checklist: &[String], answer: &dyn Fn(&str) -> Value, verdict: &str) -> Value {
    let items: Vec<Value> = checklist.iter().map(|item| answer(item)).collect();
    let arguments = json!({
        "schema_version": 1,
        "items": items,
        "findings": [],
        "opinion": {"verdict": verdict, "reason": "As the items say."},
    });
    calls("Submitting.", &[tool_use(id, "submit_verdict", arguments)])
}

fn held(item: &str) -> Value {
    json!({"id": item, "disposition": "held", "confidence": "confident",
           "reason": "It matches the source.", "allowed_by": null, "location": null,
           "evidence": []})
}

/// A strip of 1,100 by 40 pixels: a crop of all of it is longer than 1,024 and is scaled
/// down, the case transcript format 2 was made for.
fn strip(salt: u32) -> RgbImage {
    RgbImage::from_fn(1100, 40, |x, y| {
        let n = noise(x / 3, y / 3, salt);
        Rgb([n, n / 2 + 40, 200 - n / 2])
    })
}

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).expect("the package folder"));
    // `scaled`: the one decision whose crop is scaled down; otherwise the three ordinary ones.
    let scaled = std::env::args().nth(2).as_deref() == Some("scaled");
    let set_id = if scaled { SCALED_SET } else { SET };
    let catalog = Catalog::shipped().expect("the catalogue");
    let images = catalog.model(MODEL).expect("the model").images.clone();
    let config = Config::parse(CONFIG, &[], &|_| None).expect("the config");
    let set = out.join("evalsets").join(set_id);

    let items = if scaled {
        let source_png = png(&strip(3));
        let source_path = "families/strip/source.png";
        write(&set.join(source_path), &source_png);
        vec![("wide", item(&set, set_id, "wide", &strip(4), source_path, &source_png))]
    } else {
        let source = scene((0, 0), Rgb([20, 90, 40]));
        let source_png = png(&source);
        let source_path = "families/door/source.png";
        write(&set.join(source_path), &source_png);
        let kept = scene((2, 1), Rgb([20, 90, 40]));
        let mut moved = scene((2, 1), Rgb([20, 90, 40]));
        for y in 30..58 {
            for x in 10..26 {
                moved.put_pixel(x, y, Rgb([20, 90, 40]));
            }
        }
        vec![
            ("kept", item(&set, set_id, "kept", &kept, source_path, &source_png)),
            ("moved", item(&set, set_id, "moved", &moved, source_path, &source_png)),
            ("cut-short", item(&set, set_id, "cut-short", &kept, source_path, &source_png)),
        ]
    };

    for (name, job_path) in items {
        let job = Job::load(&job_path).expect("the job");
        let tools = ToolRegistry::new(
            squint_tools::builtin::all(),
            &["view".into(), "align".into()],
            &[],
        )
        .expect("the tools")
        .with_projections(&BTreeMap::from([("align".into(), "visual_only".into())]))
        .expect("the projection");
        let setup = run::setup(&config, &job, &set, &tools, None).expect("the setup");
        let checklist: Vec<String> = setup
            .prepared
            .checklist
            .items
            .iter()
            .map(|i| i.id.clone())
            .collect();
        let script: Vec<Value> = match name {
            "wide" => vec![
                calls(
                    "The whole strip.",
                    &[tool_use("toolu_w1", "view", json!({"frame": "raw:candidate", "bbox": [0.0, 0.0, 1.0, 1.0]}))],
                ),
                submit("toolu_w2", &checklist, &held, "pass"),
            ],
            "kept" => vec![
                calls(
                    "I will look closer and register the pair.",
                    &[
                        tool_use(
                            "toolu_k1",
                            "view",
                            json!({"frame": "raw:candidate", "also": "raw:source",
                                   "bbox": [0.25, 0.25, 0.75, 0.75]}),
                        ),
                        tool_use("toolu_k2", "align", json!({"image": "candidate", "against": "source"})),
                    ],
                ),
                submit("toolu_k3", &checklist, &held, "pass"),
            ],
            "moved" => vec![
                calls(
                    "Registering first.",
                    &[tool_use("toolu_m1", "align", json!({"image": "raw:candidate", "against": "raw:source"}))],
                ),
                calls(
                    "Now the door in the aligned frame.",
                    &[tool_use(
                        "toolu_m2",
                        "view",
                        json!({"frame": "aligned:candidate:t1", "bbox": [0.1, 0.3, 0.6, 0.9]}),
                    )],
                ),
                submit(
                    "toolu_m3",
                    &checklist,
                    &|item: &str| {
                        if item.contains("RQ1") {
                            json!({"id": item, "disposition": "violation", "confidence": "confident",
                                   "reason": "The door is painted over.", "allowed_by": null,
                                   "location": {"kind": "bbox", "frame": "raw:candidate",
                                                "bbox": [0.3, 0.45, 0.5, 0.85], "polygon": null,
                                                "yaw": null, "pitch": null, "fov": null},
                                   "evidence": ["call:1", "call:2"]})
                        } else {
                            held(item)
                        }
                    },
                    "fail",
                ),
            ],
            _ => vec![
                json!({
                    "text": ["Let me look at"],
                    "tool_calls": [{"id": "toolu_c0", "name": "view", "arguments": {"frame": "raw:source"}}],
                    "content": [{"type": "text", "text": "Let me look at"},
                                {"type": "tool_use", "id": "toolu_c0", "name": "view",
                                 "arguments": {"frame": "raw:source"}}],
                    "stop_reason": "max_tokens",
                    "usage": {"input_tokens": 1200, "output_tokens": 16000},
                }),
                calls(
                    "The top left of the source.",
                    &[tool_use("toolu_c1", "view", json!({"frame": "source", "bbox": [0.0, 0.0, 0.5, 0.5]}))],
                ),
                calls(
                    "Closer still.",
                    &[tool_use("toolu_c2", "view", json!({"frame": "crop:call:1:0", "bbox": [0.25, 0.25, 0.75, 0.75]}))],
                ),
                submit(
                    "toolu_c3",
                    &checklist,
                    &|item: &str| {
                        if item.contains("RQ1") {
                            json!({"id": item, "disposition": "cant_tell", "confidence": "tentative",
                                   "reason": "The crop does not show the door.", "allowed_by": null,
                                   "location": null, "evidence": ["call:2"]})
                        } else {
                            held(item)
                        }
                    },
                    "unsure",
                ),
            ],
        };
        let responses: Vec<TurnResponse> = script
            .into_iter()
            .map(|v| serde_json::from_value(v).expect("a response"))
            .collect();
        let provider = Capture {
            script: ScriptedProvider::new(responses),
            images: images.clone(),
        };
        let repetition = out.join("replay/synthetic").join(name);
        std::fs::create_dir_all(&repetition).expect("a folder");
        let outcome = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(run::check(
                &config,
                &job,
                &set,
                &run::Services::new(&provider, &tools),
                &repetition,
                "capture",
            ))
            .expect("the decision");
        let named = outcome.dir.file_name().expect("a name").to_string_lossy().to_string();
        let result = json!({
            "set": set_id,
            "item": name,
            "cell": format!("synthetic/{MODEL}/C"),
            "repetition": 1,
            "dir": named,
            "verdict": outcome.decision.verdict,
            "exit_code": outcome.exit_code,
        });
        write(
            &repetition.join("result.json"),
            serde_json::to_string_pretty(&result).expect("json").as_bytes(),
        );
        println!("capture: {name}: exit {}", outcome.exit_code);
    }

    // The preparation's settings for the model, with the keys of the 2026-11 package's
    // generated file (docs/baseline/transcript.md). `resize` and `jpeg_quality` are that
    // package's values, as `whizbang eval baseline` holds them; `metadata`, `plan` and
    // `png_when_alpha` are this program's words, which the replay copies and never reads.
    let long_edge = images.native_long_edge_px;
    let settings = json!({"settings": {MODEL: {
        "config_max_long_edge": null,
        "decode_max_bytes": config.images.decode_bounds().max_bytes,
        "decode_max_pixels": config.images.decode_bounds().max_pixels,
        "formats_accepted": images.formats,
        "jpeg_quality": squint_providers::prepare::JPEG_QUALITY,
        "long_edge_px": long_edge,
        "metadata": "stripped",
        "model": MODEL,
        "native_long_edge_px": images.native_long_edge_px,
        "plan": "standard",
        "png_when_alpha": "png",
        "prep_version": PREP_VERSION,
        "resize": "Lanczos3, never upscaled",
    }}});
    write(
        &out.join("assembly/generated/preparation.json"),
        serde_json::to_string_pretty(&settings).expect("json").as_bytes(),
    );
}
