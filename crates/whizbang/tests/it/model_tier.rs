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
    let executor = tier.runner(&over);
    // The fixture, brought into the store as an import brings a picture.
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
    let started = Instant::now();
    let asked = Call::new("remove_prompt")
        .argument("prompt", whizbang_core::contract::Value::text(PHRASE))
        .input("image", id);
    let finished = executor.submit(&asked).unwrap();
    let took = started.elapsed();
    let outputs = finished.outcome().outputs().to_vec();
    assert_eq!(outputs.len(), 1, "{:?}", finished.outcome());
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
