//! The model tier (plan 1C, task R2b; decision D15): SAM 3, installed, answers a phrase
//! on the Mac's GPU through the binary's wiring: the kind `pack`, the queue and the
//! supervisor of the one worker, the worker the installation `whizbang setup` made, and
//! the adapter `sam3`. Never in CI and never in the fast tier: each test is ignored
//! there, and `just test-models` runs them on a machine that has the model
//! (`scripts/test-models.sh`). Plan 1D, task T1, adds Tiny (`tiny`, the adapter
//! `owlv2-sam2`), asked a phrase, a point and a box the same way.
//!
//! **Where the model is.** A test beside the real binary's start reads nothing of its
//! environment (`scripts/check-binary-start.sh`), so `scripts/test-models.sh` writes the
//! models directory the local environment names into one file in cargo's folder for
//! tests' scratch (`model-tier/models-dir`), and each test reads it from there. Each
//! test writes what it measured, `name=value` lines and no path, beside it
//! (`model-tier/out/`), which `scripts/model-evidence.sh` reads.
//!
//! **The worker** is the binary's own (`OneWorker::installed`), held by the test kit's
//! guard (`kit::installed`), which kills it and waits for it on every path and fails the
//! test if one outlives it. The config sets `max_lifetime_s`, so a worker nobody waits
//! for ends by itself. The CPU fallback of the GPU's runtime is off: the supervisor's
//! environment for the worker never sets `PYTORCH_ENABLE_MPS_FALLBACK`
//! (`worker::the_workers_environment_holds_the_allow_list_and_no_key`).
//!
//! **What the binary has no command for.** No action of 1C asks for the segment
//! capability (the plan, R2b: "no action is added in 1C"), so the request is made by a
//! test action through the binary's wiring, as the tests of the kind do (`pack.rs`). The
//! real binary, started through the test kit's launcher, reads the run's record back.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use whizbang::pack::OneWorker;
use whizbang::wiring::{self, Models, Over};
use whizbang_backends::pack::queue::Status;
use whizbang_core::capability::{Picture, Query, SegmentRequest};
use whizbang_core::contract::{
    Action, ActionDescriptor, Call, Capability, Context, Engine, Executor, Failure, Kind,
    PreparedCall, Produced, Requirement,
};
use whizbang_core::identity::{DecodeLimits, Digest, identify};
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::scratch::Scratch;
use whizbang_testkit::threads;
use whizbang_testkit::worker::{self as kit, Guard};

use crate::wired::whizbang;

/// The fixture picture: 640 by 480, a grey ground, a red disc of radius 90 centred at
/// (420, 200) and a blue square from (80, 280) to (240, 440). Drawn by a program, so it
/// is nobody's; written with no metadata (`evidence::the_fixture_picture_holds_no_metadata`).
pub(crate) const FIXTURE: &[u8] =
    include_bytes!("../../../../docs/results/m1c-local-pack/circle.png");
/// The phrase that names the disc.
pub(crate) const PHRASE: &str = "circle";
/// A loose box drawn by hand around the disc (330 to 510 across, 110 to 290 down):
/// left, top, right, bottom.
pub(crate) const LOOSE_BOX: [f64; 4] = [300.0, 80.0, 540.0, 320.0];
/// The most instances asked for.
const MOST: u32 = 4;
/// The model.
const SAM3: &str = "sam3";
/// The second model (plan 1D, task T1).
const TINY: &str = "tiny";

/// One answer: for each instance, its score, its box, the digest of its plane and how
/// many pixels are on; and how long the request took.
#[derive(Debug, Clone)]
struct Answered {
    instances: Vec<(f64, [f64; 4], String, usize)>,
    took: Duration,
}

/// The folder `scripts/test-models.sh` and these tests share, in cargo's folder for the
/// scratch of tests.
pub(crate) fn shared() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join("model-tier")
}

/// The models directory `scripts/test-models.sh` was given: an absolute path, one line.
pub(crate) fn models_dir() -> PathBuf {
    let named = std::fs::read_to_string(shared().join("models-dir"))
        .expect("run the model tier by just test-models, which names the models directory");
    let dir = PathBuf::from(named.trim_end_matches('\n'));
    assert!(dir.is_absolute() && dir.is_dir(), "not a models directory");
    dir
}

/// Writes what a test measured, `name=value` lines, for the evidence script.
pub(crate) fn write_out(test: &str, lines: &[(String, String)]) {
    let out = shared().join("out");
    std::fs::create_dir_all(&out).unwrap();
    let text: String = lines
        .iter()
        .map(|(name, value)| format!("{name}={value}\n"))
        .collect();
    std::fs::write(out.join(format!("{test}.txt")), text).unwrap();
}

fn picture() -> Picture {
    let found = identify(FIXTURE, &DecodeLimits::default()).unwrap();
    Picture::new(found.pixels).unwrap()
}

/// An action that asks the segmenter once for [`PHRASE`] about the fixture, and keeps
/// what it answered.
fn finds(came: &Arc<Mutex<Vec<Answered>>>) -> Arc<dyn Action> {
    struct Finds(ActionDescriptor, Arc<Mutex<Vec<Answered>>>);
    impl Action for Finds {
        fn descriptor(&self) -> &ActionDescriptor {
            &self.0
        }
        fn run(&self, _: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
            let segment = context.capabilities().segment().expect("a segmenter");
            let query = Query::phrase(PHRASE).unwrap();
            let request = SegmentRequest::new(picture(), vec![query], MOST).unwrap();
            let begun = Instant::now();
            let response = segment
                .segment(&request)
                .map_err(|fault| Failure::error("segment", fault.sentence()))?;
            let took = begun.elapsed();
            let instances = response.answers()[0]
                .iter()
                .map(|found| {
                    let plane = found.plane();
                    let on = plane.iter().filter(|sample| **sample != 0).count();
                    let digest = Digest::of_bytes(plane).to_string();
                    (found.score(), found.bounds(), digest, on)
                })
                .collect();
            self.1.lock().unwrap().push(Answered { instances, took });
            Ok(Produced::nothing())
        }
    }
    let need = Requirement::new(Capability::Segment).feature("phrase");
    let draft =
        ActionDescriptor::draft("finds", "1.0.0", Kind::Primitive, "A test action.").needs(need);
    Arc::new(Finds(draft.build().unwrap(), came.clone()))
}

/// The point asked of the disc: its centre (plan 1D, task Q1).
pub(crate) const POINT: [f64; 2] = [420.0, 200.0];

/// What one request of a point and a box about the fixture answered: for each query, its
/// instances as [`Answered`] holds them, and the time of the whole request.
#[derive(Debug, Clone, Default)]
struct Places {
    point: Vec<(f64, [f64; 4], String, usize)>,
    boxed: Vec<(f64, [f64; 4], String, usize)>,
    took: Duration,
}

/// An action that asks the segmenter once, in one batch, for [`POINT`] and for
/// [`LOOSE_BOX`] about the fixture, and keeps what it answered.
fn places(came: &Arc<Mutex<Vec<Places>>>) -> Arc<dyn Action> {
    struct Asks(ActionDescriptor, Arc<Mutex<Vec<Places>>>);
    impl Action for Asks {
        fn descriptor(&self) -> &ActionDescriptor {
            &self.0
        }
        fn run(&self, _: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
            let segment = context.capabilities().segment().expect("a segmenter");
            let [left, top, right, bottom] = LOOSE_BOX;
            let queries = vec![
                Query::point(POINT[0], POINT[1]).unwrap(),
                Query::boxed(left, top, right, bottom).unwrap(),
            ];
            let request = SegmentRequest::new(picture(), queries, MOST).unwrap();
            let begun = Instant::now();
            let response = segment
                .segment(&request)
                .map_err(|fault| Failure::error("segment", fault.sentence()))?;
            let took = begun.elapsed();
            let of = |at: usize| {
                response.answers()[at]
                    .iter()
                    .map(|found| {
                        let plane = found.plane();
                        let on = plane.iter().filter(|sample| **sample != 0).count();
                        let digest = Digest::of_bytes(plane).to_string();
                        (found.score(), found.bounds(), digest, on)
                    })
                    .collect()
            };
            self.1.lock().unwrap().push(Places {
                point: of(0),
                boxed: of(1),
                took,
            });
            Ok(Produced::nothing())
        }
    }
    let need = Requirement::new(Capability::Segment)
        .feature("point")
        .feature("box");
    let draft =
        ActionDescriptor::draft("places", "1.0.0", Kind::Primitive, "A test action.").needs(need);
    Arc::new(Asks(draft.build().unwrap(), came.clone()))
}

/// A store, a config of one backend of the kind `pack` for a model, `sam3` unless
/// another is named, and the binary's one worker, in the test kit's guard.
struct Tier {
    /// First, so that it is dropped first.
    worker: Guard<Arc<OneWorker>>,
    _dir: Scratch,
    root: PathBuf,
    config: PathBuf,
    came: Arc<Mutex<Vec<Answered>>>,
    placed: Arc<Mutex<Vec<Places>>>,
}

impl Tier {
    fn new(label: &str) -> Tier {
        Tier::of(label, SAM3)
    }

    fn of(label: &str, model: &str) -> Tier {
        Tier::with(label, model, "")
    }

    /// The same, the config ending with `more` (per-subject comparison's `[tools]`).
    fn with(label: &str, model: &str, more: &str) -> Tier {
        let dir = Scratch::new(label);
        let root = dir.path().join("store");
        drop(Store::open(&root, Arc::new(RealFs)).unwrap());
        let config = dir.path().join("whizbang.toml");
        std::fs::write(
            &config,
            format!(
                "[backends.segment]\nkind = \"pack\"\nmodel = \"{model}\"\n\n\
                 [backends.segment.settings]\nmax_lifetime_s = 600\ncall_limit_s = 120\n{more}"
            ),
        )
        .unwrap();
        Tier {
            worker: kit::installed(OneWorker::installed, |worker| {
                worker.kill();
            }),
            _dir: dir,
            root,
            config,
            came: Arc::default(),
            placed: Arc::default(),
        }
    }

    /// What the binary's wiring builds, as `Under::over` builds it: the config, the
    /// kinds over the one worker, and the catalogue it ships in the models directory.
    fn over(&self) -> Over {
        Over::nothing()
            .config(Some(&self.config))
            .kinds(wiring::kinds(&self.worker))
            .pack(self.worker.clone())
            .installed(Models::shipped().unwrap().flag(Some(&models_dir())))
            .expect("the models directory is opened")
    }

    fn runner(&self, over: &Over) -> Engine {
        let mut registry = wiring::registry().unwrap();
        registry.register(finds(&self.came)).unwrap();
        registry.register(places(&self.placed)).unwrap();
        let store = Arc::new(Store::open(&self.root, Arc::new(RealFs)).unwrap());
        wiring::runner(registry, store, over).expect("the executor builds")
    }

    /// One run of `finds`: its id, once it completed.
    fn run(&self, executor: &Engine) -> String {
        let finished = executor.submit(&Call::new("finds")).unwrap();
        assert_eq!(finished.outcome().name(), "completed");
        finished.run().as_str().to_owned()
    }

    fn came(&self) -> Vec<Answered> {
        std::mem::take(&mut *self.came.lock().unwrap())
    }

    /// The record of a run, as the real binary prints it.
    fn record(&self, run: &str) -> serde_json::Value {
        let json = whizbang(&["runs", "show", run, "--json"], &self.root);
        assert_eq!(json.code, Some(0), "{}", json.err);
        serde_json::from_str(&json.out).unwrap()
    }

    /// What a mask of the store weighs at each pixel: its core where set, else its
    /// blend, as a fill reads it.
    fn mask_weights(&self, id: &str) -> Vec<f32> {
        let reader = whizbang_core::store::Reader::open(&self.root, Arc::new(RealFs)).unwrap();
        let read = whizbang_core::artifact::get_anywhere(
            &reader,
            whizbang_core::artifact::ArtifactType::Mask,
            whizbang_core::artifact::ArtifactId(id.parse::<Digest>().unwrap()),
            &DecodeLimits::default(),
        );
        let Ok(whizbang_core::artifact::Artifact::Mask(mask)) = read else {
            panic!("no mask {id}");
        };
        let width = mask.binding().source_space().width();
        (0..mask.core().len() as u32)
            .map(|at| mask.weight(at % width, at / width).unwrap())
            .collect()
    }

    /// The kinds of the requests written to the worker, in order.
    fn sent(&self) -> Vec<&'static str> {
        self.worker.journal().iter().map(|sent| sent.kind).collect()
    }
}

/// Whether an instance's box is inside the loose box.
fn inside(bounds: [f64; 4]) -> bool {
    bounds[0] >= LOOSE_BOX[0]
        && bounds[1] >= LOOSE_BOX[1]
        && bounds[2] <= LOOSE_BOX[2]
        && bounds[3] <= LOOSE_BOX[3]
}

/// The fields of an answer, under a name's prefix.
fn fields_of(prefix: &str, answered: &Answered) -> Vec<(String, String)> {
    let mut lines = vec![
        (format!("{prefix}_seconds"), seconds(answered.took)),
        (
            format!("{prefix}_instances"),
            answered.instances.len().to_string(),
        ),
    ];
    for (at, (score, bounds, digest, on)) in answered.instances.iter().enumerate() {
        let n = at + 1;
        lines.push((format!("{prefix}_{n}_score"), format!("{score:.3}")));
        let edges: Vec<String> = bounds.iter().map(|edge| format!("{edge:.1}")).collect();
        lines.push((format!("{prefix}_{n}_box"), edges.join(" ")));
        lines.push((format!("{prefix}_{n}_mask"), digest.clone()));
        lines.push((format!("{prefix}_{n}_pixels_on"), on.to_string()));
    }
    lines
}

pub(crate) fn seconds(took: Duration) -> String {
    format!("{:.2}", took.as_secs_f64())
}

/// The device and the installation the record of a run names for its one request.
fn device_and_installation(record: &serde_json::Value) -> (String, String) {
    device_and_installation_of(record, SAM3)
}

/// The same, of a run that asked this model.
fn device_and_installation_of(record: &serde_json::Value, of: &str) -> (String, String) {
    assert_eq!(record["record"], "run/4");
    let models = record["models"].as_array().unwrap();
    assert_eq!(models.len(), 1, "{models:?}");
    let model = &models[0];
    assert_eq!(
        (&model["capability"], &model["backend"], &model["model"]),
        (
            &serde_json::json!("segment"),
            &serde_json::json!("pack"),
            &serde_json::json!(of)
        )
    );
    assert_eq!(model["ended"], "answered");
    assert_eq!(model["cached"], false);
    let device = model["device"].as_str().unwrap().to_owned();
    let installation = model["version"].as_str().unwrap().to_owned();
    (device, installation)
}

#[test]
#[ignore = "the model tier: just test-models, on a machine with the model installed"]
fn sam3_answers_a_phrase_on_the_gpu_with_an_instance_inside_the_fixtures_box_and_the_record_names_the_device()
 {
    let tier = Tier::new("model-tier-phrase");
    let over = tier.over();
    let ready = over.installations().unwrap().ready(SAM3);
    let identity = ready
        .expect("sam3 is installed and ready")
        .identity()
        .digest()
        .to_string();
    let executor = tier.runner(&over);
    let run = tier.run(&executor);
    let came = tier.came();
    assert_eq!(came.len(), 1);
    let answered = &came[0];
    // At least one instance, and one whose box is inside the box drawn by hand.
    assert!(
        answered
            .instances
            .iter()
            .any(|(_, bounds, _, on)| inside(*bounds) && *on > 0),
        "no instance inside the loose box: {answered:?}"
    );
    assert_eq!(tier.sent(), ["load", "segment"]);
    drop(executor);
    // The record names the device the worker's result named: the Apple GPU.
    let (device, installation) = device_and_installation(&tier.record(&run));
    assert_eq!(device, "mps");
    assert_eq!(installation, identity);
    let mut lines = vec![
        ("run".to_owned(), run),
        ("device".to_owned(), device),
        ("installation".to_owned(), installation),
    ];
    lines.extend(fields_of("request", answered));
    write_out("phrase", &lines);
}

#[test]
#[ignore = "the model tier: just test-models, on a machine with the model installed"]
fn a_second_request_for_the_same_model_loads_nothing() {
    let tier = Tier::new("model-tier-second");
    let over = tier.over();
    // Whether a full check of the installation's files passed before, in an earlier
    // process, and left its record (plan 2A, task R1): then the first load below
    // reads no file whole.
    let ready = over.installations().unwrap().ready(SAM3);
    let identity = ready.expect("sam3 is installed and ready").identity();
    let record = models_dir()
        .join("models")
        .join(SAM3)
        .join("checks")
        .join(identity.digest().hex());
    let checked_before = if record.is_file() { "yes" } else { "no" };
    let executor = Arc::new(tier.runner(&over));
    // The first run, on a thread of the test kit's, while this one sees when the load
    // is written and when the model is resident, by the supervisor's status.
    let begun = Instant::now();
    let first = {
        let executor = executor.clone();
        threads::spawn(move || {
            let finished = executor.submit(&Call::new("finds")).unwrap();
            assert_eq!(finished.outcome().name(), "completed");
            finished.run().as_str().to_owned()
        })
    };
    let loading = |status: &Status| {
        status.resident.is_some()
            || status
                .calling
                .as_ref()
                .is_some_and(|calling| calling.kind == "load")
    };
    let within = Duration::from_secs(180);
    assert!(tier.worker.wait_for(within, &loading));
    let load_begun = begun.elapsed();
    assert!(
        tier.worker
            .wait_for(within, &|status: &Status| status.resident.is_some())
    );
    let loaded = begun.elapsed();
    let first = first.join().unwrap();
    let first_took = begun.elapsed();
    // The second, in a run of its own, on the model already resident.
    let second = tier.run(&executor);
    let came = tier.came();
    assert_eq!(came.len(), 2);
    // One load, two requests, one worker.
    assert_eq!(tier.sent(), ["load", "segment", "segment"]);
    let workers: Vec<u32> = tier
        .worker
        .journal()
        .iter()
        .map(|sent| sent.worker)
        .collect();
    assert!(workers.windows(2).all(|pair| pair[0] == pair[1]));
    // Both answered on the GPU, the same answer.
    drop(executor);
    let mut lines = Vec::new();
    for (name, run) in [("first_run", &first), ("second_run", &second)] {
        let (device, _) = device_and_installation(&tier.record(run));
        assert_eq!(device, "mps", "{name}");
        lines.push((name.to_owned(), run.clone()));
    }
    let digests = |answered: &Answered| -> Vec<String> {
        answered
            .instances
            .iter()
            .map(|(_, _, digest, _)| digest.clone())
            .collect()
    };
    assert_eq!(digests(&came[0]), digests(&came[1]));
    lines.push(("device".to_owned(), "mps".to_owned()));
    lines.push(("loads".to_owned(), "1".to_owned()));
    lines.push(("until_load_written_seconds".to_owned(), seconds(load_begun)));
    lines.push(("load_seconds".to_owned(), seconds(loaded - load_begun)));
    lines.push(("first_load_seconds".to_owned(), seconds(loaded)));
    lines.push(("check_record_before".to_owned(), checked_before.to_owned()));
    lines.push((
        "first_request_seconds".to_owned(),
        seconds(first_took - loaded),
    ));
    lines.push(("first_run_seconds".to_owned(), seconds(first_took)));
    lines.push(("second_request_seconds".to_owned(), seconds(came[1].took)));
    lines.extend(fields_of("first", &came[0]));
    lines.extend(fields_of("second", &came[1]));
    write_out("second", &lines);
}

#[test]
#[ignore = "the model tier: just test-models, on a machine with the model installed"]
fn sam3_answers_a_point_and_a_box_on_the_mac() {
    // Plan 1D, task Q1: a point at the disc's centre and the loose box around it, asked
    // of SAM 3's tracker in one request, through the binary's wiring, on the GPU.
    let tier = Tier::new("model-tier-places");
    let over = tier.over();
    let ready = over.installations().unwrap().ready(SAM3);
    let identity = ready
        .expect("sam3 is installed and ready")
        .identity()
        .digest()
        .to_string();
    let executor = tier.runner(&over);
    let finished = executor.submit(&Call::new("places")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    let run = finished.run().as_str().to_owned();
    let placed = std::mem::take(&mut *tier.placed.lock().unwrap());
    assert_eq!(placed.len(), 1);
    let answered = &placed[0];
    // Each answers its candidates, at most three, best predicted IoU first; the best of
    // each is inside the loose box and holds the point.
    for (name, found) in [("point", &answered.point), ("box", &answered.boxed)] {
        assert!(!found.is_empty() && found.len() <= 3, "{name}: {found:?}");
        assert!(
            found.windows(2).all(|pair| pair[0].0 >= pair[1].0),
            "{name}: {found:?}"
        );
        let (_, bounds, _, on) = &found[0];
        assert!(inside(*bounds) && *on > 0, "{name}: {found:?}");
        assert!(
            bounds[0] <= POINT[0]
                && POINT[0] < bounds[2]
                && bounds[1] <= POINT[1]
                && POINT[1] < bounds[3],
            "{name}: {found:?}"
        );
    }
    assert_eq!(tier.sent(), ["load", "segment"]);
    drop(executor);
    let (device, installation) = device_and_installation(&tier.record(&run));
    assert_eq!(device, "mps");
    assert_eq!(installation, identity);
    let mut lines = vec![
        ("run".to_owned(), run),
        ("device".to_owned(), device),
        ("request_seconds".to_owned(), seconds(answered.took)),
    ];
    for (prefix, found) in [("point", &answered.point), ("box", &answered.boxed)] {
        let one = Answered {
            instances: found.clone(),
            took: answered.took,
        };
        lines.extend(fields_of(prefix, &one));
    }
    write_out("places", &lines);
}

#[test]
#[ignore = "the model tier: just test-models, on a machine with the model installed"]
fn tiny_answers_a_phrase_a_point_and_a_box_on_the_mac() {
    // Plan 1D, task T1: the phrase, then the point and the loose box in one request,
    // asked of Tiny (OWLv2 and SAM 2.1 Hiera-Tiny, the adapter `owlv2-sam2`) through the
    // binary's wiring, on the GPU, with one load for the two requests.
    let tier = Tier::of("model-tier-tiny", TINY);
    let over = tier.over();
    let ready = over.installations().unwrap().ready(TINY);
    let identity = ready
        .expect("tiny is installed and ready")
        .identity()
        .digest()
        .to_string();
    let executor = tier.runner(&over);
    let phrase = tier.run(&executor);
    let finished = executor.submit(&Call::new("places")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    let places = finished.run().as_str().to_owned();
    let came = tier.came();
    assert_eq!(came.len(), 1);
    // The phrase finds the disc: an instance inside the loose box.
    assert!(
        came[0]
            .instances
            .iter()
            .any(|(_, bounds, _, on)| inside(*bounds) && *on > 0),
        "no instance inside the loose box: {came:?}"
    );
    let placed = std::mem::take(&mut *tier.placed.lock().unwrap());
    assert_eq!(placed.len(), 1);
    let answered = &placed[0];
    // The point and the box each answer SAM 2.1's candidates, at most three, best
    // predicted IoU first; the best of each is inside the loose box and holds the point.
    for (name, found) in [("point", &answered.point), ("box", &answered.boxed)] {
        assert!(!found.is_empty() && found.len() <= 3, "{name}: {found:?}");
        assert!(
            found.windows(2).all(|pair| pair[0].0 >= pair[1].0),
            "{name}: {found:?}"
        );
        let (_, bounds, _, on) = &found[0];
        assert!(inside(*bounds) && *on > 0, "{name}: {found:?}");
        assert!(
            bounds[0] <= POINT[0]
                && POINT[0] < bounds[2]
                && bounds[1] <= POINT[1]
                && POINT[1] < bounds[3],
            "{name}: {found:?}"
        );
    }
    assert_eq!(tier.sent(), ["load", "segment", "segment"]);
    drop(executor);
    let mut lines = vec![
        ("phrase_run".to_owned(), phrase.clone()),
        ("places_run".to_owned(), places.clone()),
    ];
    for run in [&phrase, &places] {
        let (device, installation) = device_and_installation_of(&tier.record(run), TINY);
        assert_eq!(device, "mps");
        assert_eq!(installation, identity);
    }
    lines.push(("device".to_owned(), "mps".to_owned()));
    lines.push(("installation".to_owned(), identity));
    lines.extend(fields_of("phrase", &came[0]));
    lines.push(("places_seconds".to_owned(), seconds(answered.took)));
    for (prefix, found) in [("point", &answered.point), ("box", &answered.boxed)] {
        let one = Answered {
            instances: found.clone(),
            took: answered.took,
        };
        lines.extend(fields_of(prefix, &one));
    }
    write_out("tiny", &lines);
}

/// One row of the table of readings in per-subject comparison's record: the example, the
/// subject, and its six readings in the record's order.
struct Reading {
    example: String,
    subject: String,
    one_minus_iou: f64,
    present: (bool, bool),
    areas: (f64, f64),
    shift: f64,
}

/// The rows between the record's marks `read: begin` and `read: end`.
fn readings_of(record: &str) -> Vec<Reading> {
    let (_, rest) = record
        .split_once("<!-- read: begin -->")
        .expect("the record marks its readings");
    let (table, _) = rest.split_once("<!-- read: end -->").unwrap();
    let yes = |cell: &str| match cell {
        "yes" => true,
        "no" => false,
        other => panic!("{other} is not yes or no"),
    };
    table
        .lines()
        .filter(|line| line.starts_with("| ") && !line.starts_with("| Example"))
        .map(|line| {
            let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
            assert_eq!(cells.len(), 8, "{line}");
            let number = |at: usize| cells[at].parse::<f64>().unwrap();
            Reading {
                example: cells[0].to_owned(),
                subject: cells[1].to_owned(),
                one_minus_iou: number(2),
                present: (yes(cells[3]), yes(cells[4])),
                areas: (number(5), number(6)),
                shift: number(7),
            }
        })
        .collect()
}

#[test]
#[ignore = "the model tier: just test-models, on a machine with the model installed"]
fn per_subject_comparison_answers_the_records_examples_on_the_mac() {
    // Plan 2C, task X1, item 7, rung 1 (its record made by task F1): the tool file the pack
    // ships, run by the tool worker of the pack `whizbang setup` installed, asks SAM 3 on
    // the Mac's GPU for each subject; each example of its record reads as the record says.
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let folder = repository.join("docs/reference/tools");
    let record = std::fs::read_to_string(folder.join("per_subject.md")).unwrap();
    let want = readings_of(&record);
    assert_eq!(want.len(), 4);
    let tool = repository.join("pack/tools/per_subject.py");
    let tier = Tier::with(
        "model-tier-per-subject",
        SAM3,
        &format!("\n[tools]\npaths = [\"{}\"]\n", tool.display()),
    );
    let root = tier.root.clone();
    let picture = |name: &str| {
        let file = folder.join("per_subject").join(format!("{name}.png"));
        let said =
            whizbang::import::import_picture(&file, &root, false, wiring::recovery()).unwrap();
        let id = said
            .split_whitespace()
            .find(|word| word.starts_with("sha256:"))
            .unwrap()
            .trim_end_matches(['.', ',']);
        whizbang_core::artifact::ArtifactId(id.parse().unwrap())
    };
    let source = picture("source");
    let candidates = [("removed", picture("removed")), ("toned", picture("toned"))];
    let over = tier.over();
    let files = over.tool_files().unwrap();
    let declared = whizbang::trying::declared_in("per_subject", files.as_deref()).unwrap();
    assert!(declared.file);
    let registry = wiring::registry_over(&over, false).unwrap();
    let store = Arc::new(Store::open(&root, Arc::new(RealFs)).unwrap());
    let executor = wiring::runner(registry, store, &over).expect("the executor builds");
    // The tool worker is the one worker's, held by the test kit's guard with it.
    let arguments = [
        (
            "phrases".to_owned(),
            whizbang_core::contract::Value::text("chair"),
        ),
        (
            "points".to_owned(),
            whizbang_core::contract::Value::text("158 110"),
        ),
    ];
    let mut lines = Vec::new();
    for (example, candidate) in candidates {
        let started = Instant::now();
        let tried = whizbang::trying::read_pair_with(
            &executor, &root, &declared, source, candidate, &arguments,
        )
        .unwrap();
        let took = started.elapsed();
        let run = tried.tool.clone().expect("the pair registered");
        let shown = whizbang(
            &[
                "runs",
                "show",
                run.as_str(),
                "--diagnostic",
                "diagnostics/tool_file.json",
            ],
            &root,
        );
        assert_eq!(shown.code, Some(0), "{}", shown.err);
        let read: serde_json::Value = serde_json::from_str(&shown.out).unwrap();
        let readings = read["readings"].as_array().unwrap();
        let mine: Vec<&Reading> = want.iter().filter(|r| r.example == example).collect();
        assert_eq!(readings.len(), mine.len(), "{example}: {readings:?}");
        for (got, want) in readings.iter().zip(mine) {
            assert_eq!(got["subject"], want.subject.as_str(), "{example}");
            let value = |name: &str| got["values"][name].as_f64().unwrap();
            // The model's readings on one Mac, within what the record's rounding and the
            // GPU's own variation allow (ADR 0027 measured masks within an IoU of 0.99).
            let near = |name: &str, recorded: f64, within: f64| {
                let read = value(name);
                assert!(
                    (read - recorded).abs() <= within,
                    "{example}, {}: {name} read {read}, the record says {recorded}",
                    want.subject
                );
            };
            near("one_minus_iou", want.one_minus_iou, 0.02);
            near("area_source", want.areas.0, 0.005);
            near("area_candidate", want.areas.1, 0.005);
            near("shift", want.shift, 0.01);
            assert_eq!(
                (
                    value("present_source") == 1.0,
                    value("present_candidate") == 1.0
                ),
                want.present,
                "{example}, {}",
                want.subject
            );
        }
        let primary = tried.primary(&declared).unwrap();
        lines.push((format!("{example}_value"), primary.to_string()));
        lines.push((format!("{example}_seconds"), seconds(took)));
    }
    drop(executor);
    write_out("per_subject", &lines);
}

#[test]
#[ignore = "the model tier: just test-models, on a machine with the model installed"]
fn remove_prompt_makes_a_mask_with_sam3_on_the_mac() {
    use whizbang_core::artifact::{self, Artifact, ArtifactType, Image, ImageRole, Origin, Step};
    use whizbang_core::identity::{Projection, Topology};
    let tier = Tier::new("model-tier-remove-prompt");
    let over = tier.over();
    // The fixture, brought into the store as an import brings a picture, before the
    // executor holds the store's writer.
    let origin = Origin::new(Step::new("fixture", "1")).unwrap();
    let limits = DecodeLimits::default();
    let image = Image::new(
        FIXTURE.to_vec(),
        Projection::Flat,
        Topology::Bounded,
        ImageRole::Picture,
        origin,
        &limits,
    )
    .unwrap();
    let store = Store::open(&tier.root, Arc::new(RealFs)).unwrap();
    let mut publication = store.begin().unwrap();
    let id = artifact::put(&mut publication, &Artifact::from(image)).unwrap();
    publication.publish().unwrap();
    drop(store);
    let executor = tier.runner(&over);
    let started = Instant::now();
    // The mask alone: the recipe ends in a fill since plan m3's task F1, and this
    // tier's config names no filler, so the run stops after `feather`, which asks none.
    let asked = Call::new("remove_prompt")
        .argument("prompt", whizbang_core::contract::Value::text(PHRASE))
        .argument("until", whizbang_core::contract::Value::text("feather"))
        .input("image", id);
    let finished = executor.submit(&asked).unwrap();
    let took = started.elapsed();
    let outputs = finished.outcome().outputs().to_vec();
    assert_eq!(outputs.len(), 1, "{:?}", finished.outcome());
    assert_eq!(outputs[0].port(), "until_feather_mask");
    let run = finished.run().as_str().to_owned();
    drop(executor);
    // The mask holds the disc: its core reaches inside the box drawn by hand.
    let store = Store::open(&tier.root, Arc::new(RealFs)).unwrap();
    let read = artifact::get(&store, ArtifactType::Mask, outputs[0].id(), &limits);
    let mask = read.unwrap().into_mask().unwrap();
    let width = mask.space().width() as usize;
    let on = mask.core().iter().filter(|s| **s != 0).count();
    let centre = (LOOSE_BOX[1] + LOOSE_BOX[3]) as usize / 2 * width
        + (LOOSE_BOX[0] + LOOSE_BOX[2]) as usize / 2;
    assert!(on > 0 && mask.core()[centre] != 0, "{on} pixels on");
    // The segment step's record names the GPU.
    let recipe = tier.record(&run);
    let segment = recipe["steps"][0]["run"].as_str().unwrap().to_owned();
    let (device, _) = device_and_installation(&tier.record(&segment));
    assert_eq!(device, "mps");
    write_out(
        "remove_prompt",
        &[
            ("run".to_owned(), run),
            ("core_pixels".to_owned(), on.to_string()),
            ("seconds".to_owned(), seconds(took)),
        ],
    );
}

#[test]
#[ignore = "the model tier: just test-models, on a machine with the model installed"]
fn remove_checked_makes_its_mask_with_sam3_under_a_scripted_judge() {
    use crate::checked::{Setup, backend, kit_of, passes};
    use whizbang_core::artifact::{self, Artifact, ArtifactType, Image, ImageRole, Origin, Step};
    use whizbang_core::contract::record::Decision;
    use whizbang_core::identity::{Projection, Topology};
    use whizbang_core::spend::Amount;
    // Plan m4, task L1: the loop whose find is SAM 3's, on the Mac's GPU, through the
    // binary's wiring; its filler and its judge are the test's own, and the judge
    // passes the first attempt.
    let more = format!("\n{}{}", backend("fill"), backend("judge"));
    let tier = Tier::with("model-tier-remove-checked", SAM3, &more);
    let (kit, kind) = kit_of(&Setup::answering(vec![passes()]));
    let mut kinds = wiring::kinds(&tier.worker);
    kinds.register(kind).unwrap();
    let over = Over::nothing()
        .config(Some(&tier.config))
        .kinds(kinds)
        .pack(tier.worker.clone())
        .installed(Models::shipped().unwrap().flag(Some(&models_dir())))
        .expect("the models directory is opened");
    let origin = Origin::new(Step::new("fixture", "1")).unwrap();
    let limits = DecodeLimits::default();
    let image = Image::new(
        FIXTURE.to_vec(),
        Projection::Flat,
        Topology::Bounded,
        ImageRole::Picture,
        origin,
        &limits,
    )
    .unwrap();
    let store = Store::open(&tier.root, Arc::new(RealFs)).unwrap();
    let mut publication = store.begin().unwrap();
    let id = artifact::put(&mut publication, &Artifact::from(image)).unwrap();
    publication.publish().unwrap();
    whizbang_core::spend::reset(&store, Amount::new(1_000_000).unwrap(), 1).unwrap();
    drop(store);
    let executor = tier.runner(&over);
    let started = Instant::now();
    let asked = Call::new("remove_checked")
        .argument("prompt", whizbang_core::contract::Value::text(PHRASE))
        .input("image", id);
    let finished = executor.submit(&asked).unwrap();
    let took = started.elapsed();
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    assert_eq!(finished.verdict().unwrap().decision(), Decision::Pass);
    let outputs = finished.outcome().outputs().to_vec();
    let run = finished.run().as_str().to_owned();
    drop(executor);
    // One request of the filler and one of the judge; the segmenter was the real one.
    assert_eq!(kit.requests(), (0, 1, 1));
    // The kept mask holds the disc: it weighs on the centre of the box drawn by hand.
    let mask = outputs
        .iter()
        .find(|output| output.port() == "mask")
        .unwrap();
    let store = Store::open(&tier.root, Arc::new(RealFs)).unwrap();
    let read = artifact::get(&store, ArtifactType::Mask, mask.id(), &limits);
    let mask = read.unwrap().into_mask().unwrap();
    let (x, y) = (
        (LOOSE_BOX[0] + LOOSE_BOX[2]) / 2.0,
        (LOOSE_BOX[1] + LOOSE_BOX[3]) / 2.0,
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let weight = mask.weight(x as u32, y as u32).unwrap();
    assert!(weight > 0.0, "the mask weighs nothing at the disc's centre");
    // The find's step `segment` names the GPU in its record.
    let looped = tier.record(&run);
    assert_eq!(looped["attempts"]["kept"], 1);
    let find = looped["steps"][0]["run"].as_str().unwrap().to_owned();
    let segment = tier.record(&find)["steps"][0]["run"]
        .as_str()
        .unwrap()
        .to_owned();
    let (device, _) = device_and_installation(&tier.record(&segment));
    assert_eq!(device, "mps");
    // What it took, for whoever reads the test's output.
    eprintln!(
        "remove_checked: run {run}, one attempt, {} s",
        seconds(took)
    );
}

#[test]
#[ignore = "the model tier: just test-models, on a machine with the model installed"]
fn a_located_retry_finds_the_phrase_inside_a_crop_with_sam3() {
    use crate::checked::{Setup, backend, kit_of, passes};
    use crate::located::remains_at;
    use whizbang_core::artifact::{self, Artifact, Image, ImageRole, Origin, Step};
    use whizbang_core::identity::{Projection, Topology};
    use whizbang_core::spend::Amount;
    // Plan m4, task L2: a located retry whose crop is asked of SAM 3, on the Mac's GPU,
    // through the binary's wiring. The filler and the judge are the test's own: the
    // judge fails the first attempt and names the loose box about the disc, as
    // fractions of the picture of 640 by 480, and passes whatever comes next.
    let place = [
        LOOSE_BOX[0] / 640.0,
        LOOSE_BOX[1] / 480.0,
        LOOSE_BOX[2] / 640.0,
        LOOSE_BOX[3] / 480.0,
    ];
    let more = format!("\n{}{}", backend("fill"), backend("judge"));
    let tier = Tier::with("model-tier-located", SAM3, &more);
    let (kit, kind) = kit_of(&Setup::answering(vec![remains_at(place), passes()]));
    let mut kinds = wiring::kinds(&tier.worker);
    kinds.register(kind).unwrap();
    let over = Over::nothing()
        .config(Some(&tier.config))
        .kinds(kinds)
        .pack(tier.worker.clone())
        .installed(Models::shipped().unwrap().flag(Some(&models_dir())))
        .expect("the models directory is opened");
    let origin = Origin::new(Step::new("fixture", "1")).unwrap();
    let image = Image::new(
        FIXTURE.to_vec(),
        Projection::Flat,
        Topology::Bounded,
        ImageRole::Picture,
        origin,
        &DecodeLimits::default(),
    )
    .unwrap();
    let store = Store::open(&tier.root, Arc::new(RealFs)).unwrap();
    let mut publication = store.begin().unwrap();
    let id = artifact::put(&mut publication, &Artifact::from(image)).unwrap();
    publication.publish().unwrap();
    whizbang_core::spend::reset(&store, Amount::new(1_000_000).unwrap(), 1).unwrap();
    drop(store);
    let executor = tier.runner(&over);
    let started = Instant::now();
    let asked = Call::new("remove_checked")
        .argument("prompt", whizbang_core::contract::Value::text(PHRASE))
        .input("image", id);
    let finished = executor.submit(&asked).unwrap();
    let took = started.elapsed();
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    let run = finished.run().as_str().to_owned();
    drop(executor);
    // The retry cut one crop, and SAM 3 found the phrase inside it: the crop gave a
    // mask, which was carried onto the picture.
    let looped = tier.record(&run);
    let second = &looped["attempts"]["list"][1];
    assert_eq!(second["changed"]["change"], "located", "{second}");
    let crops = second["changed"]["crops"].as_array().unwrap();
    assert_eq!(crops.len(), 1);
    assert!(
        crops[0]["mask"].is_string(),
        "nothing was found in the crop"
    );
    // The crop is the loose box, a twentieth wider each way: 264 pixels each way, cut
    // from the picture pixel for pixel, and asked of the model on the GPU.
    let runs = second["runs"].as_array().unwrap();
    let asked_of = |at: usize| tier.record(runs[at].as_str().unwrap());
    let (view, segment, carry) = (asked_of(0), asked_of(1), asked_of(2));
    assert_eq!(view["asked"]["action"], "view");
    assert_eq!(segment["asked"]["action"], "segment");
    assert_eq!(carry["asked"]["action"], "carry");
    let (device, _) = device_and_installation(&segment);
    assert_eq!(device, "mps");
    // The disc was in the first attempt's mask already. When what the model finds in
    // the crop adds no pixel to it, the retry's mask is the one before and is not
    // filled a second time; when it adds any, the retry is filled and checked, and the
    // judge's second answer passes it.
    let (_, fills, checks) = kit.requests();
    match second["verdict"].as_str() {
        Some(verdict) => {
            assert_eq!((verdict, fills, checks), ("pass", 2, 2));
            assert_eq!(looped["attempts"]["kept"], 2);
        }
        None => {
            assert_eq!(
                second["unchecked"],
                "its shaped mask is the one before it, and nothing was filled"
            );
            assert_eq!((fills, checks), (1, 1));
            assert_eq!(looped["attempts"]["kept"], 1);
        }
    }
    eprintln!(
        "remove_checked, located: run {run}, attempt 2 {}, {} s",
        second["verdict"].as_str().unwrap_or("not filled"),
        seconds(took)
    );
}

/// Where the large discs of [`second_disc_scene`] are: nine, one more than the eight
/// instances the first find keeps (`remove_prompt`, its step `segment`).
const LARGE_DISCS: [(i64, i64); 9] = [
    (100, 90),
    (320, 90),
    (540, 90),
    (100, 240),
    (320, 240),
    (540, 240),
    (430, 165),
    (320, 390),
    (540, 390),
];

/// The scene of the located retry's fill (plan mE, task L2, item 5): 640 by 480, white,
/// nine black discs of radius 30 ([`LARGE_DISCS`]), and a small pale one, radius 10 at
/// (120, 390), light grey on the white. The first find keeps at most eight instances of
/// the phrase, and the nine large discs, each far more plainly a circle, fill them: so
/// the pale disc is missed by the first find because of that bound, not because of what
/// the model sees, while a crop about it holds it alone. Drawn by this program, so
/// nothing is committed beside it.
fn second_disc_scene() -> Vec<u8> {
    use image::ImageEncoder;
    let rgba: Vec<u8> = (0..640u32 * 480)
        .flat_map(|at| {
            let (x, y) = (i64::from(at % 640), i64::from(at / 640));
            let disc = |cx: i64, cy: i64, r: i64| (x - cx).pow(2) + (y - cy).pow(2) <= r * r;
            if LARGE_DISCS.iter().any(|(cx, cy)| disc(*cx, *cy, 30)) {
                [0, 0, 0, 255]
            } else if disc(120, 390, 10) {
                [200, 200, 200, 255]
            } else {
                [255, 255, 255, 255]
            }
        })
        .collect();
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&rgba, 640, 480, image::ExtendedColorType::Rgba8)
        .unwrap();
    png
}

#[test]
#[ignore = "the model tier: just test-models, on a machine with the model installed"]
fn a_located_retrys_carried_mask_reaches_a_fill_and_a_check_with_sam3() {
    use crate::checked::{Setup, backend, kit_of, passes};
    use crate::located::remains_at;
    use whizbang_core::artifact::{self, Artifact, Image, ImageRole, Origin, Step};
    use whizbang_core::identity::{Projection, Topology};
    use whizbang_core::spend::Amount;
    // Plan mE, task L2, item 5: on a picture where the first find's eight instances are
    // taken by nine large discs, the judge fails the first attempt and names a loose box
    // about the small pale disc beside them; SAM 3 finds it inside the crop, where it is
    // alone, the mask is carried, joined, filled and checked, and the judge's second
    // answer passes it.
    let place = [80.0 / 640.0, 350.0 / 480.0, 160.0 / 640.0, 430.0 / 480.0];
    let more = format!("\n{}{}", backend("fill"), backend("judge"));
    let tier = Tier::with("model-tier-located-fill", SAM3, &more);
    let (kit, kind) = kit_of(&Setup::answering(vec![remains_at(place), passes()]));
    let mut kinds = wiring::kinds(&tier.worker);
    kinds.register(kind).unwrap();
    let over = Over::nothing()
        .config(Some(&tier.config))
        .kinds(kinds)
        .pack(tier.worker.clone())
        .installed(Models::shipped().unwrap().flag(Some(&models_dir())))
        .expect("the models directory is opened");
    let origin = Origin::new(Step::new("fixture", "1")).unwrap();
    let image = Image::new(
        second_disc_scene(),
        Projection::Flat,
        Topology::Bounded,
        ImageRole::Picture,
        origin,
        &DecodeLimits::default(),
    )
    .unwrap();
    let store = Store::open(&tier.root, Arc::new(RealFs)).unwrap();
    let mut publication = store.begin().unwrap();
    let id = artifact::put(&mut publication, &Artifact::from(image)).unwrap();
    publication.publish().unwrap();
    whizbang_core::spend::reset(&store, Amount::new(1_000_000).unwrap(), 1).unwrap();
    drop(store);
    let executor = tier.runner(&over);
    let asked = Call::new("remove_checked")
        .argument("prompt", whizbang_core::contract::Value::text(PHRASE))
        .input("image", id);
    let finished = executor.submit(&asked).unwrap();
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    let run = finished.run().as_str().to_owned();
    drop(executor);
    let looped = tier.record(&run);
    let second = &looped["attempts"]["list"][1];
    assert_eq!(second["changed"]["change"], "located", "{second}");
    let crops = second["changed"]["crops"].as_array().unwrap();
    assert!(
        crops[0]["mask"].is_string(),
        "SAM 3 found nothing in the crop about the second disc"
    );
    // The carried mask added pixels: the retry was filled and checked, and passed.
    assert_eq!(second["verdict"], "pass", "{second}");
    let (_, fills, checks) = kit.requests();
    assert_eq!((fills, checks), (2, 2));
    assert_eq!(looped["attempts"]["kept"], 2);
    // The kept mask covers the second disc's centre; the first attempt's did not.
    let mask_of = |attempt: usize| -> Vec<f32> {
        let runs = looped["attempts"]["list"][attempt]["runs"]
            .as_array()
            .unwrap();
        let shape = runs
            .iter()
            .map(|run| tier.record(run.as_str().unwrap()))
            .find(|record| {
                record["asked"]["action"] == "remove_prompt"
                    && record["outputs"].as_array().is_some_and(|outputs| {
                        outputs.iter().any(|o| o["port"] == "until_feather_mask")
                    })
            })
            .unwrap();
        let output = (shape["outputs"].as_array().unwrap().iter())
            .find(|o| o["port"] == "until_feather_mask")
            .unwrap();
        tier.mask_weights(output["artifact"].as_str().unwrap())
    };
    let at = 390 * 640 + 120;
    assert!(
        mask_of(0)[at] < 0.5,
        "the first find did not miss the pale disc, though nine larger ones could fill its eight"
    );
    assert!(mask_of(1)[at] >= 0.5);
}

/// The scene of `segment`'s record (`docs/reference/tools/segment.md`, plan m3, task D1):
/// 192 by 128, a grey ground, two red discs (radius 24 at (56, 64), radius 14 at
/// (150, 32)) and a blue square from (104, 72) to (152, 112), drawn by a program. The
/// same drawing is in `whizbang-rascal`'s tests of the record (`tool_records::segment`),
/// which hold it to the digest this test writes.
fn segment_scene() -> whizbang_core::identity::CanonicalPixels {
    let rgba = (0..192u32 * 128)
        .flat_map(|at| {
            let (x, y) = (i64::from(at % 192), i64::from(at / 192));
            let disc = |cx: i64, cy: i64, r: i64| (x - cx).pow(2) + (y - cy).pow(2) <= r * r;
            if disc(56, 64, 24) || disc(150, 32, 14) {
                [200, 40, 40, 255]
            } else if (104..152).contains(&x) && (72..112).contains(&y) {
                [40, 70, 190, 255]
            } else {
                [128, 128, 128, 255]
            }
        })
        .collect();
    whizbang_core::identity::CanonicalPixels::from_rgba(192, 128, rgba).unwrap()
}

#[test]
#[ignore = "the model tier: just test-models, on a machine with the model installed"]
fn segment_answers_the_records_examples_on_the_mac() {
    use whizbang_core::artifact::{self, Artifact, ArtifactType, Image, ImageRole, Origin, Step};
    use whizbang_core::contract::Value;
    use whizbang_core::identity::{Projection, Topology, encode};
    let tier = Tier::new("model-tier-segment-record");
    let over = tier.over();
    let scene = segment_scene();
    let limits = DecodeLimits::default();
    let image = Image::new(
        encode::png(&scene).unwrap(),
        Projection::Flat,
        Topology::Bounded,
        ImageRole::Picture,
        Origin::new(Step::new("fixture", "1")).unwrap(),
        &limits,
    )
    .unwrap();
    let store = Store::open(&tier.root, Arc::new(RealFs)).unwrap();
    let mut publication = store.begin().unwrap();
    let id = artifact::put(&mut publication, &Artifact::from(image)).unwrap();
    publication.publish().unwrap();
    drop(store);
    let executor = tier.runner(&over);
    // Each example: its name, the kind of query, the query and the most instances.
    let examples = [
        ("phrase", "phrase", "red circle", 4),
        ("point", "point", "128 92", 1),
    ];
    let mut out = format!("scene {}\n", scene.digest().0);
    let mut runs = Vec::new();
    for (name, kind, query, most) in examples {
        let started = Instant::now();
        let asked = Call::new("segment")
            .argument("kind", Value::text(kind))
            .argument("query", Value::text(query))
            .argument("max_instances", Value::Integer(most))
            .input("image", id);
        let finished = executor.submit(&asked).unwrap();
        let took = started.elapsed();
        assert_eq!(
            finished.outcome().name(),
            "completed",
            "{:?}",
            finished.outcome()
        );
        let masks: Vec<_> = finished
            .outcome()
            .outputs()
            .iter()
            .map(|o| o.id())
            .collect();
        runs.push((name, finished.run().as_str().to_owned(), masks, took));
    }
    drop(executor);
    let store = Store::open(&tier.root, Arc::new(RealFs)).unwrap();
    for (name, run, masks, took) in runs {
        let said = whizbang(
            &[
                "runs",
                "show",
                &run,
                "--diagnostic",
                "diagnostics/segment.txt",
            ],
            &tier.root,
        );
        assert_eq!(said.code, Some(0), "{}", said.err);
        let (device, installation) = device_and_installation(&tier.record(&run));
        assert_eq!(device, "mps");
        out.push_str(&format!(
            "example {name} {} {installation} {device} {}\n",
            masks.len(),
            seconds(took)
        ));
        let lines: Vec<&str> = said
            .out
            .lines()
            .filter(|l| l.starts_with("mask "))
            .collect();
        assert_eq!(lines.len(), masks.len());
        for (line, id) in lines.iter().zip(&masks) {
            let read = artifact::get(&store, ArtifactType::Mask, *id, &limits);
            let mask = read.unwrap().into_mask().unwrap();
            // The core as runs, from a run of 0s: each run's length, apart by commas.
            let (mut runs, mut last, mut length) = (Vec::new(), 0u8, 0usize);
            for sample in mask.core() {
                let on = u8::from(*sample != 0);
                if on != last {
                    runs.push(length.to_string());
                    (last, length) = (on, 0);
                }
                length += 1;
            }
            runs.push(length.to_string());
            // "mask <id> score <s> box <l t r b> [iou <i>]": the score and the box.
            let words: Vec<&str> = line.split(' ').collect();
            out.push_str(&format!(
                "mask {} {} {} {} {} {}\n",
                words[3],
                words[5],
                words[6],
                words[7],
                words[8],
                runs.join(",")
            ));
        }
    }
    let path = shared().join("out");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("segment-record.txt"), out).unwrap();
}

/// The fixed-class segmenter (plan mE, task T2).
const EOMT: &str = "eomt-ade20k";

/// The 150 classes of ADE20K, as the pack's adapter lists them (`CLASSES` of
/// `pack/src/whizbang_pack/eomt_ade20k.py`).
fn ade20k_classes() -> Vec<String> {
    let module = include_str!("../../../../pack/src/whizbang_pack/eomt_ade20k.py");
    let start = module.find("CLASSES = (").expect("the list") + "CLASSES = (".len();
    let body = &module[start..start + module[start..].find("\n)").expect("its end")];
    let classes: Vec<String> = (body.split('"').skip(1).step_by(2))
        .map(str::to_owned)
        .collect();
    assert_eq!(classes.len(), 150);
    classes
}

#[test]
#[ignore = "the model tier: just test-models, on a machine with the model installed"]
fn eomt_labels_every_pixel_of_the_fixture_on_the_mac() {
    // Plan mE, task T2, item 3: one labels request of every class on the fixture, through
    // the binary's wiring and the action `labels`, on the GPU. Every pixel has exactly one
    // class, and each class's confidence is from 0 to 1. A second call answers the same
    // masks: it is answered from the labels the adapter kept of the picture, not by the
    // model again, so it shows the action's and the kept answer's agreement, not the
    // model's repeatability.
    use whizbang_core::artifact::{self, Artifact, Image, ImageRole, Origin, Step};
    use whizbang_core::contract::{Outcome, Value};
    use whizbang_core::identity::{Projection, Topology};
    let tier = Tier::of("model-tier-eomt", EOMT);
    let over = tier.over();
    let ready = over.installations().unwrap().ready(EOMT);
    let identity = (ready
        .expect("eomt-ade20k is installed and ready")
        .identity())
    .digest()
    .to_string();
    let limits = DecodeLimits::default();
    let origin = Origin::new(Step::new("fixture", "1")).unwrap();
    let image = Image::new(
        FIXTURE.to_vec(),
        Projection::Flat,
        Topology::Bounded,
        ImageRole::Picture,
        origin,
        &limits,
    )
    .unwrap();
    let store = Store::open(&tier.root, Arc::new(RealFs)).unwrap();
    let mut publication = store.begin().unwrap();
    let id = artifact::put(&mut publication, &Artifact::from(image)).unwrap();
    publication.publish().unwrap();
    // The store is held by one handle at a time: the fixture's is let go before the
    // executor opens it, and the binary reads the record once the executor is dropped.
    drop(store);
    let executor = tier.runner(&over);
    let classes = ade20k_classes();
    let call = Call::new("labels")
        .input("image", id)
        .argument("classes", Value::text(&classes.join("\n")));
    let started = Instant::now();
    let first = executor.submit(&call).unwrap();
    let took = started.elapsed();
    let Outcome::Completed { outputs } = first.outcome() else {
        panic!("{:?}", first.outcome());
    };
    let listed = first.measurements().unwrap()["classes"]
        .as_array()
        .unwrap()
        .clone();
    assert!(
        !listed.is_empty() && listed.len() == outputs.len(),
        "{listed:?}"
    );
    let mut pixels = 0u64;
    for class in &listed {
        assert!(
            classes
                .iter()
                .any(|name| name == class["name"].as_str().unwrap())
        );
        let score = class["score"].as_f64().unwrap();
        assert!((0.0..=1.0).contains(&score) && score > 0.0, "{class}");
        pixels += class["pixels"].as_u64().unwrap();
    }
    // Every pixel, once: the action refuses a pixel labelled twice, and these add up.
    assert_eq!(pixels, 640 * 480);
    let second = executor.submit(&call).unwrap();
    assert_eq!(second.outcome(), first.outcome());
    drop(executor);
    let record = tier.record(first.run().as_str());
    for model in record["models"].as_array().unwrap() {
        assert_eq!(
            (model["model"].as_str(), model["device"].as_str()),
            (Some(EOMT), Some("mps"))
        );
        assert_eq!(model["version"].as_str(), Some(identity.as_str()));
    }
    let mut lines = vec![
        ("run".to_owned(), first.run().as_str().to_owned()),
        ("device".to_owned(), "mps".to_owned()),
        ("installation".to_owned(), identity),
        ("seconds".to_owned(), seconds(took)),
        ("classes".to_owned(), listed.len().to_string()),
    ];
    for (at, class) in listed.iter().enumerate() {
        let name = class["name"].as_str().unwrap().replace(' ', "_");
        lines.push((
            format!("class_{}", at + 1),
            format!("{name} {} {}", class["pixels"], class["score"]),
        ));
    }
    write_out("eomt", &lines);
}
