//! Parity fixtures and the mask-agreement tolerance (plan 1D, task V1; ADR 0027).
//!
//! **The fixtures** (`fixtures/parity/segment/`): for SAM 3 and for Tiny, each of three
//! pictures ([`PICTURES`]: two drawn by a program, one of them the model tier's, and
//! rascal's `chair-blob.png`, ported) asked a phrase, a point and a box in one request,
//! and the instances the model answered on the Mac's GPU: for each, its score, its box,
//! its predicted IoU and its mask as a PNG plane; with the model's revision in the
//! catalogue and the adapter's in the binary's table. The fast tests hold them to the
//! tree; the model tier (`just test-models`) asks each again, three times on the GPU
//! and three on the CPU (the setting `device`), and holds every answer to them within
//! the tolerance.
//!
//! **Writing them.** The model tier's test writes what the GPU answered first, in the
//! fixtures' form, to `model-tier/out/parity/`, and what it measured to
//! `model-tier/out/parity.txt`; copying that folder over `fixtures/parity/segment/` and
//! committing it is what makes a fixture. It writes them before it holds the answers to
//! the committed ones, so a model or an adapter that changes what it answers can be
//! recorded again.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use whizbang::pack::OneWorker;
use whizbang::parity::{self, Apart, Found, IOU_TOLERANCE, SCORE_MARGIN};
use whizbang::wiring::{self, Models, Over};
use whizbang_core::capability::{Picture, Query, SegmentRequest};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Call, Capability, Context, Engine, Executor, Failure, Kind,
    PreparedCall, Produced, Requirement, Value, ValueType,
};
use whizbang_core::identity::{DecodeLimits, Digest, identify};
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::scratch::Scratch;
use whizbang_testkit::worker::{self as kit, Guard};

use crate::model_tier::{models_dir, shared, write_out};
use crate::wired::whizbang;

/// A picture of the fixtures: its name, its file in the repository, and the phrase, the
/// point and the box it is asked.
pub(crate) struct Fixture {
    pub(crate) name: &'static str,
    pub(crate) file: &'static str,
    pub(crate) phrase: &'static str,
    pub(crate) point: [f64; 2],
    pub(crate) boxed: [f64; 4],
}

/// The three pictures. `circle` is the model tier's (a red disc and a blue square, 640
/// by 480); `shapes`, drawn by a program for these fixtures (480 by 320: a green
/// triangle and a purple ring on sand); `chair`, rascal's `fixtures/images/chair-blob.png`
/// at the commit `docs/sources.toml` pins, which rascal drew by a program
/// (`images::chair_blob`: a chair and its shadow on a floor of boards, 480 by 320).
pub(crate) const PICTURES: [Fixture; 3] = [
    Fixture {
        name: "circle",
        file: "docs/results/m1c-local-pack/circle.png",
        phrase: "circle",
        point: [420.0, 200.0],
        boxed: [300.0, 80.0, 540.0, 320.0],
    },
    Fixture {
        name: "shapes",
        file: "fixtures/parity/segment/pictures/shapes.png",
        phrase: "triangle",
        point: [130.0, 200.0],
        boxed: [270.0, 80.0, 430.0, 240.0],
    },
    Fixture {
        name: "chair",
        file: "fixtures/parity/segment/pictures/chair.png",
        phrase: "chair",
        point: [190.0, 162.0],
        boxed: [125.0, 60.0, 260.0, 255.0],
    },
];

/// The models, by their names in the catalogue.
const MODELS: [&str; 2] = ["sam3", "tiny"];
/// The most instances each query is asked for: the tracker's three candidates.
const MOST: u32 = 3;
/// How many times each fixture is asked on each device.
const TIMES: usize = 3;
/// The queries' names, in the order of a request.
const QUERIES: [&str; 3] = ["phrase", "point", "box"];

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixtures() -> PathBuf {
    repository().join("fixtures/parity/segment")
}

pub(crate) fn picture_of(fixture: &Fixture) -> Picture {
    let bytes = std::fs::read(repository().join(fixture.file)).unwrap();
    let found = identify(&bytes, &DecodeLimits::default()).unwrap();
    Picture::new(found.pixels).unwrap()
}

fn digest_of(path: &Path) -> String {
    Digest::of_bytes(&std::fs::read(path).unwrap()).to_string()
}

/// One instance as an answer gave it.
#[derive(Debug, Clone)]
struct Given {
    score: f64,
    bounds: [f64; 4],
    iou: Option<f64>,
    plane: Vec<u8>,
}

/// An answer, query by query, of a picture of this size.
#[derive(Debug, Clone)]
struct Answer {
    width: u32,
    height: u32,
    queries: Vec<Vec<Given>>,
}

impl Answer {
    fn found(&self) -> Vec<Vec<Found>> {
        self.queries
            .iter()
            .map(|query| {
                query
                    .iter()
                    .map(|given| Found {
                        score: given.score,
                        plane: given.plane.clone(),
                    })
                    .collect()
            })
            .collect()
    }
}

/// The committed fixture of a model and a picture: its fields, and its answer with each
/// plane read from its PNG.
fn committed(model: &str, fixture: &Fixture) -> (serde_json::Value, Answer) {
    let folder = fixtures().join(model);
    let path = folder.join(format!("{}.json", fixture.name));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("no fixture for {model} and {}", fixture.name));
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    let (width, height) = (
        json["width"].as_u64().unwrap() as u32,
        json["height"].as_u64().unwrap() as u32,
    );
    let queries = json["answers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|query| {
            query
                .as_array()
                .unwrap()
                .iter()
                .map(|given| {
                    let file = folder.join(given["plane"].as_str().unwrap());
                    let plane = image::open(&file).unwrap().to_luma8();
                    assert_eq!(plane.dimensions(), (width, height), "{}", file.display());
                    let bounds: Vec<f64> = given["box"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|edge| edge.as_f64().unwrap())
                        .collect();
                    Given {
                        score: given["score"].as_f64().unwrap(),
                        bounds: bounds.try_into().unwrap(),
                        iou: given["iou"].as_f64(),
                        plane: plane.into_raw(),
                    }
                })
                .collect()
        })
        .collect();
    (
        json,
        Answer {
            width,
            height,
            queries,
        },
    )
}

/// Writes an answer in the fixtures' form, under `folder`.
fn write_fixture(folder: &Path, model: &str, fixture: &Fixture, answer: &Answer) {
    std::fs::create_dir_all(folder).unwrap();
    let catalogue = wiring::catalogue().unwrap();
    let entry = catalogue.entry(model).unwrap();
    let adapters = wiring::adapters();
    let adapter = adapters
        .iter()
        .find(|adapter| adapter.name.as_str() == entry.adapter().as_str())
        .unwrap();
    let mut answers = Vec::new();
    for (query, given) in QUERIES.iter().zip(&answer.queries) {
        let mut listed = Vec::new();
        for (at, one) in given.iter().enumerate() {
            let file = format!("{}-{query}-{}.png", fixture.name, at + 1);
            let plane =
                image::GrayImage::from_raw(answer.width, answer.height, one.plane.clone()).unwrap();
            plane.save(folder.join(&file)).unwrap();
            listed.push(serde_json::json!({
                "score": one.score,
                "box": one.bounds,
                "iou": one.iou,
                "plane": file,
            }));
        }
        answers.push(serde_json::Value::Array(listed));
    }
    let json = serde_json::json!({
        "model": model,
        "model_revision": entry.revision().as_str(),
        "adapter": adapter.name.as_str(),
        "adapter_revision": adapter.revision.as_str(),
        "picture": fixture.name,
        "picture_file": fixture.file,
        "picture_digest": digest_of(&repository().join(fixture.file)),
        "width": answer.width,
        "height": answer.height,
        "device": "mps",
        "phrase": fixture.phrase,
        "point": fixture.point,
        "box": fixture.boxed,
        "max_instances": MOST,
        "answers": answers,
    });
    let text = serde_json::to_string_pretty(&json).unwrap() + "\n";
    std::fs::write(folder.join(format!("{}.json", fixture.name)), text).unwrap();
}

// --- the fast tests ----------------------------------------------------------------------------

/// A plane of `size` pixels with the pixels from `from` up to `to` on.
fn plane(size: usize, from: usize, to: usize) -> Vec<u8> {
    (0..size)
        .map(|at| if (from..to).contains(&at) { 255 } else { 0 })
        .collect()
}

fn found(score: f64, plane: Vec<u8>) -> Found {
    Found { score, plane }
}

#[test]
fn agreement_is_the_count_and_each_pairs_iou_and_score() {
    // The IoU of two planes: on in both over on in either; 1 when neither is on.
    assert_eq!(parity::iou(&plane(10, 0, 4), &plane(10, 2, 6)), 2.0 / 6.0);
    assert_eq!(parity::iou(&plane(10, 0, 0), &plane(10, 0, 0)), 1.0);
    assert_eq!(parity::iou(&plane(10, 0, 4), &plane(12, 0, 4)), 0.0);
    // The measured numbers are a tolerance and a margin a pair can be inside.
    assert!((0.9..=1.0).contains(&IOU_TOLERANCE), "{IOU_TOLERANCE}");
    assert!((0.0..0.1).contains(&SCORE_MARGIN), "{SCORE_MARGIN}");
    // A plane of 10,000 pixels, the first 1,000 on; another with the first `m` on
    // overlaps it by 1000 / m. The widest `m` inside the tolerance agrees, one more
    // pixel does not.
    let size = 10_000;
    let base = plane(size, 0, 1000);
    let widest = (1000.0 / IOU_TOLERANCE).floor() as usize;
    let at = |m: usize, score: f64| {
        let one = vec![vec![found(0.9, base.clone())]];
        let other = vec![vec![found(score, plane(size, 0, m))]];
        parity::agree(&one, &other)
    };
    assert!(at(widest, 0.9));
    assert!(!at(widest + 1, 0.9));
    // A score within the margin agrees; one past it does not, however well the masks
    // overlap.
    assert!(at(1000, 0.9 - SCORE_MARGIN + 1e-9));
    assert!(!at(1000, 0.9 - SCORE_MARGIN - 0.001));
    // Pairs are taken in order, query by query: two instances in the other order do
    // not agree, and what is apart is the worst pair.
    let a = found(0.9, plane(size, 0, 1000));
    let b = found(0.8, plane(size, 5000, 6000));
    let one = vec![vec![a.clone(), b.clone()], vec![]];
    assert!(parity::agree(&one, &one));
    assert_eq!(
        parity::apart(&one, &one),
        Some(Apart {
            least_iou: 1.0,
            widest_score: 0.0
        })
    );
    let swapped = vec![vec![b.clone(), a.clone()], vec![]];
    assert!(!parity::agree(&one, &swapped));
    assert_eq!(parity::apart(&one, &swapped).unwrap().least_iou, 0.0);
    // The queries are held apart: the same instances under another query do not agree.
    let moved = vec![vec![a.clone()], vec![b.clone()]];
    assert!(!parity::agree(&one, &moved));
}

#[test]
fn an_answer_one_instance_short_does_not_agree() {
    let a = found(0.9, plane(100, 0, 10));
    let b = found(0.8, plane(100, 50, 60));
    let whole = vec![vec![a.clone(), b.clone()]];
    let short = vec![vec![a.clone()]];
    // The instance it has is the same, and still it does not agree, either way round.
    assert_eq!(parity::apart(&whole, &short), None);
    assert!(!parity::agree(&whole, &short));
    assert!(!parity::agree(&short, &whole));
    // Nor does an answer with a query missing, or with nothing where one was found.
    assert!(!parity::agree(&whole, &[]));
    assert!(!parity::agree(&whole, &[vec![]]));
}

#[test]
fn the_committed_fixtures_name_the_revisions_the_catalogue_holds() {
    let catalogue = wiring::catalogue().unwrap();
    let adapters = wiring::adapters();
    for model in MODELS {
        let entry = catalogue.entry(model).unwrap();
        let adapter = adapters
            .iter()
            .find(|adapter| adapter.name.as_str() == entry.adapter().as_str())
            .unwrap();
        for fixture in &PICTURES {
            let (json, answer) = committed(model, fixture);
            // Of the catalogue's model and the binary's adapter, as the tree has them.
            assert_eq!(json["model"], model);
            assert_eq!(json["model_revision"], entry.revision().as_str(), "{model}");
            assert_eq!(json["adapter"], adapter.name.as_str());
            assert_eq!(
                json["adapter_revision"],
                adapter.revision.as_str(),
                "{model}"
            );
            // Of the picture the tree has, asked what the table here asks, on the GPU.
            let file = repository().join(fixture.file);
            assert_eq!(json["picture_digest"], digest_of(&file), "{}", fixture.name);
            assert_eq!(json["phrase"], fixture.phrase);
            assert_eq!(json["device"], "mps");
            let picture = picture_of(fixture);
            assert_eq!(
                (answer.width, answer.height),
                (picture.width(), picture.height())
            );
            // Each query answered within its count, each mask a plane of the picture's
            // size with something on, inside its box.
            assert_eq!(answer.queries.len(), QUERIES.len());
            for query in &answer.queries {
                assert!(query.len() <= MOST as usize);
                for given in query {
                    assert!((0.0..=1.0).contains(&given.score));
                    assert!(given.plane.contains(&255));
                    let [left, top, right, bottom] = given.bounds;
                    for (at, sample) in given.plane.iter().enumerate() {
                        let (x, y) = (
                            (at as u32 % answer.width) as f64,
                            (at as u32 / answer.width) as f64,
                        );
                        if *sample != 0 {
                            assert!(x + 1.0 > left && x < right && y + 1.0 > top && y < bottom);
                        }
                    }
                }
            }
            // Every answer agrees with itself.
            assert!(parity::agree(&answer.found(), &answer.found()));
        }
        // And the folder holds nothing else: each fixture's file and its planes.
        let mut held: Vec<String> = std::fs::read_dir(fixtures().join(model))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        held.sort();
        let mut named = Vec::new();
        for fixture in &PICTURES {
            let (json, _) = committed(model, fixture);
            named.push(format!("{}.json", fixture.name));
            for query in json["answers"].as_array().unwrap() {
                for given in query.as_array().unwrap() {
                    named.push(given["plane"].as_str().unwrap().to_owned());
                }
            }
        }
        named.sort();
        assert_eq!(held, named, "{model}");
    }
    // The pictures drawn for the fixtures hold no metadata.
    for fixture in &PICTURES[1..] {
        let bytes = std::fs::read(repository().join(fixture.file)).unwrap();
        let mut kinds = Vec::new();
        let mut at = 8;
        while at < bytes.len() {
            let length = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
            kinds.push(String::from_utf8(bytes[at + 4..at + 8].to_vec()).unwrap());
            at += 12 + length;
        }
        kinds.dedup();
        assert_eq!(kinds, ["IHDR", "IDAT", "IEND"], "{}", fixture.name);
    }
}

// --- the model tier ----------------------------------------------------------------------------

/// What the action was answered, by the picture asked: the answer, kept in order.
type Kept = Arc<Mutex<Vec<Answer>>>;

/// An action that asks the segmenter, in one request, the phrase, the point and the box
/// of the picture its argument `picture` names, and keeps what it answered.
fn asks(kept: &Kept) -> Arc<dyn Action> {
    struct Asks(ActionDescriptor, Kept);
    impl Action for Asks {
        fn descriptor(&self) -> &ActionDescriptor {
            &self.0
        }
        fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
            let named = call
                .arguments()
                .iter()
                .find(|(name, _)| name == "picture")
                .and_then(|(_, value)| match value {
                    Value::Text(text) => Some(text.clone()),
                    _ => None,
                })
                .expect("a picture");
            let fixture = PICTURES
                .iter()
                .find(|fixture| fixture.name == named)
                .unwrap();
            let segment = context.capabilities().segment().expect("a segmenter");
            let [x, y] = fixture.point;
            let [left, top, right, bottom] = fixture.boxed;
            let queries = vec![
                Query::phrase(fixture.phrase).unwrap(),
                Query::point(x, y).unwrap(),
                Query::boxed(left, top, right, bottom).unwrap(),
            ];
            let picture = picture_of(fixture);
            let (width, height) = (picture.width(), picture.height());
            let request = SegmentRequest::new(picture, queries, MOST).unwrap();
            let response = segment
                .segment(&request)
                .map_err(|fault| Failure::error("segment", fault.sentence()))?;
            let queries = response
                .answers()
                .iter()
                .enumerate()
                .map(|(query, found)| {
                    found
                        .iter()
                        .enumerate()
                        .map(|(at, instance)| Given {
                            score: instance.score(),
                            bounds: instance.bounds(),
                            iou: response.predicted_iou(query, at),
                            plane: instance.plane().to_vec(),
                        })
                        .collect()
                })
                .collect();
            self.1.lock().unwrap().push(Answer {
                width,
                height,
                queries,
            });
            Ok(Produced::nothing())
        }
    }
    let need = Requirement::new(Capability::Segment)
        .feature("phrase")
        .feature("point")
        .feature("box");
    let draft = ActionDescriptor::draft("asks", "1.0.0", Kind::Primitive, "A test action.")
        .argument(Argument::optional(
            "picture",
            "The fixture.",
            ValueType::Text,
        ))
        .needs(need);
    Arc::new(Asks(draft.build().unwrap(), kept.clone()))
}

/// A store, a config of one backend of the kind `pack` for a model on a device, and the
/// binary's one worker, in the test kit's guard.
struct Asking {
    /// First, so that it is dropped first.
    worker: Guard<Arc<OneWorker>>,
    _dir: Scratch,
    root: PathBuf,
    config: PathBuf,
    kept: Kept,
}

impl Asking {
    fn new(model: &str, device: &str) -> Asking {
        let dir = Scratch::new(&format!("parity-{model}-{device}"));
        let root = dir.path().join("store");
        drop(Store::open(&root, Arc::new(RealFs)).unwrap());
        let config = dir.path().join("whizbang.toml");
        std::fs::write(
            &config,
            format!(
                "[backends.segment]\nkind = \"pack\"\nmodel = \"{model}\"\n\n\
                 [backends.segment.settings]\nmax_lifetime_s = 900\ncall_limit_s = 120\n\
                 device = \"{device}\"\n"
            ),
        )
        .unwrap();
        Asking {
            worker: kit::installed(OneWorker::installed, |worker| {
                worker.kill();
            }),
            _dir: dir,
            root,
            config,
            kept: Arc::default(),
        }
    }

    fn runner(&self) -> Engine {
        let over = Over::nothing()
            .config(Some(&self.config))
            .kinds(wiring::kinds(&self.worker))
            .pack(self.worker.clone())
            .installed(Models::shipped().unwrap().flag(Some(&models_dir())))
            .expect("the models directory is opened");
        let mut registry = wiring::registry().unwrap();
        registry.register(asks(&self.kept)).unwrap();
        let store = Arc::new(Store::open(&self.root, Arc::new(RealFs)).unwrap());
        wiring::runner(registry, store, &over).expect("the executor builds")
    }

    /// The device the record of a run names for its one request.
    fn device(&self, run: &str) -> String {
        let json = whizbang(&["runs", "show", run, "--json"], &self.root);
        assert_eq!(json.code, Some(0), "{}", json.err);
        let record: serde_json::Value = serde_json::from_str(&json.out).unwrap();
        record["models"][0]["device"].as_str().unwrap().to_owned()
    }
}

#[test]
#[ignore = "the model tier: just test-models, on a machine with the model installed"]
fn sam3_and_tiny_answer_each_fixture_inside_the_tolerance_on_mps_and_on_cpu() {
    let out = shared().join("out").join("parity");
    let _ = std::fs::remove_dir_all(&out);
    let mut lines = Vec::new();
    let mut outside = Vec::new();
    for model in MODELS {
        let mut answered = Vec::new();
        for (device, named) in [("auto", "mps"), ("cpu", "cpu")] {
            let asking = Asking::new(model, device);
            let executor = asking.runner();
            let mut runs = Vec::new();
            for _ in 0..TIMES {
                for fixture in &PICTURES {
                    let call = Call::new("asks").argument("picture", Value::text(fixture.name));
                    let finished = executor.submit(&call).unwrap();
                    assert_eq!(finished.outcome().name(), "completed", "{model} {device}");
                    runs.push(finished.run().as_str().to_owned());
                }
            }
            drop(executor);
            // Each answer's record names the device it was asked on.
            for run in &runs {
                assert_eq!(asking.device(run), named, "{model} {device} {run}");
            }
            let kept = std::mem::take(&mut *asking.kept.lock().unwrap());
            assert_eq!(kept.len(), TIMES * PICTURES.len());
            answered.push((named, kept));
        }
        // What the GPU answered first is written in the fixtures' form.
        let (_, gpu) = &answered[0];
        for (fixture, answer) in PICTURES.iter().zip(gpu) {
            write_fixture(&out.join(model), model, fixture, answer);
        }
        // Each answer, held to the committed fixture of its picture.
        for (named, kept) in &answered {
            let mut worst = Apart {
                least_iou: 1.0,
                widest_score: 0.0,
            };
            let mut counts = 0;
            for (at, answer) in kept.iter().enumerate() {
                let fixture = &PICTURES[at % PICTURES.len()];
                let file = fixtures()
                    .join(model)
                    .join(format!("{}.json", fixture.name));
                if !file.is_file() {
                    outside.push(format!("{model} {}: no fixture is committed", fixture.name));
                    continue;
                }
                let (_, expected) = committed(model, fixture);
                match parity::apart(&expected.found(), &answer.found()) {
                    Some(apart) => worst = worst.worst(apart),
                    None => counts += 1,
                }
                if !parity::agree(&expected.found(), &answer.found()) {
                    outside.push(format!(
                        "{model} {named} {} round {}",
                        fixture.name,
                        at / PICTURES.len() + 1
                    ));
                }
            }
            lines.push((
                format!("{model}_{named}_least_iou"),
                format!("{:.6}", worst.least_iou),
            ));
            lines.push((
                format!("{model}_{named}_widest_score"),
                format!("{:.6}", worst.widest_score),
            ));
            lines.push((format!("{model}_{named}_counts_differ"), counts.to_string()));
        }
    }
    write_out("parity", &lines);
    assert!(
        outside.is_empty(),
        "outside the tolerance: {outside:?}; measured: {lines:?}"
    );
}
