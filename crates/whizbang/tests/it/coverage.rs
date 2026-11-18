//! The timings of checkpoint 1D (plan 1D, task V1, item 4; the carry-over row on the
//! speed of local models): on the Mac, for SAM 3 and for Tiny, and for each of a
//! phrase, a point and a box, in a worker of its own, the first request (the worker's
//! start, the check of the installation's files and the load among it), the load alone,
//! and a second request; and what the GPU holds once SAM 3's two models are resident.
//! Run by `scripts/test-models.sh coverage`, under `scripts/memory-watch.sh`, which
//! `scripts/model-evidence.sh` runs to write `docs/results/m1d-segment-coverage.md`. The
//! picture is the model tier's fixture, asked what the model tier asks it.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use whizbang::pack::OneWorker;
use whizbang::wiring::{self, Models, Over};
use whizbang_backends::pack::queue::Status;
use whizbang_core::capability::{Query, SegmentRequest};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Call, Capability, Context, Engine, Executor, Failure, Kind,
    PreparedCall, Produced, Requirement, Value, ValueType,
};
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::scratch::Scratch;
use whizbang_testkit::threads;
use whizbang_testkit::worker::{self as kit, Guard};

use crate::model_tier::{LOOSE_BOX, PHRASE, POINT, models_dir, seconds, write_out};
use crate::parity::PICTURES;

/// What one request answered: how long it took, how many instances, and what the
/// backend said beside them.
#[derive(Debug, Clone)]
struct Timed {
    took: Duration,
    instances: usize,
    said: String,
}

type Kept = Arc<Mutex<Vec<Timed>>>;

/// An action that asks the segmenter about the model tier's picture once, for the
/// phrase, the point or the box its argument `kind` names.
fn one(kept: &Kept) -> Arc<dyn Action> {
    struct One(ActionDescriptor, Kept);
    impl Action for One {
        fn descriptor(&self) -> &ActionDescriptor {
            &self.0
        }
        fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
            let kind = call
                .arguments()
                .iter()
                .find_map(|(name, value)| match (name.as_str(), value) {
                    ("kind", Value::Text(kind)) => Some(kind.clone()),
                    _ => None,
                })
                .expect("a kind");
            let [left, top, right, bottom] = LOOSE_BOX;
            let query = match kind.as_str() {
                "phrase" => Query::phrase(PHRASE).unwrap(),
                "point" => Query::point(POINT[0], POINT[1]).unwrap(),
                _ => Query::boxed(left, top, right, bottom).unwrap(),
            };
            let picture = crate::parity::picture_of(&PICTURES[0]);
            let request = SegmentRequest::new(picture, vec![query], 3).unwrap();
            let segment = context.capabilities().segment().expect("a segmenter");
            let begun = Instant::now();
            let response = segment
                .segment(&request)
                .map_err(|fault| Failure::error("segment", fault.sentence()))?;
            self.1.lock().unwrap().push(Timed {
                took: begun.elapsed(),
                instances: response.answers()[0].len(),
                said: response.diagnostics().to_owned(),
            });
            Ok(Produced::nothing())
        }
    }
    let need = Requirement::new(Capability::Segment);
    let draft = ActionDescriptor::draft("one", "1.0.0", Kind::Primitive, "A test action.")
        .argument(Argument::optional("kind", "The query.", ValueType::Text))
        .needs(need);
    Arc::new(One(draft.build().unwrap(), kept.clone()))
}

/// A store, a config of one backend for a model, and the binary's one worker.
struct Timing {
    worker: Guard<Arc<OneWorker>>,
    _dir: Scratch,
    root: PathBuf,
    config: PathBuf,
    kept: Kept,
}

impl Timing {
    fn new(model: &str, kind: &str) -> Timing {
        let dir = Scratch::new(&format!("coverage-{model}-{kind}"));
        let root = dir.path().join("store");
        drop(Store::open(&root, Arc::new(RealFs)).unwrap());
        let config = dir.path().join("whizbang.toml");
        std::fs::write(
            &config,
            format!(
                "[backends.segment]\nkind = \"pack\"\nmodel = \"{model}\"\n\n\
                 [backends.segment.settings]\nmax_lifetime_s = 600\ncall_limit_s = 120\n"
            ),
        )
        .unwrap();
        Timing {
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
        registry.register(one(&self.kept)).unwrap();
        let store = Arc::new(Store::open(&self.root, Arc::new(RealFs)).unwrap());
        wiring::runner(registry, store, &over).expect("the executor builds")
    }
}

fn call(kind: &str) -> Call {
    Call::new("one").argument("kind", Value::text(kind))
}

/// The first request of `kind` in a new worker, and the load inside it, by the
/// supervisor's status; then a second request of the same kind.
fn first_and_second(model: &str, kind: &str) -> (Duration, Duration, Timed, Timed) {
    let timing = Timing::new(model, kind);
    let executor = Arc::new(timing.runner());
    let begun = Instant::now();
    let first = {
        let executor = executor.clone();
        let kind = kind.to_owned();
        threads::spawn(move || {
            let finished = executor.submit(&call(&kind)).unwrap();
            assert_eq!(finished.outcome().name(), "completed");
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
    assert!(timing.worker.wait_for(within, &loading));
    let load_begun = begun.elapsed();
    assert!(
        timing
            .worker
            .wait_for(within, &|status: &Status| status.resident.is_some())
    );
    let loaded = begun.elapsed();
    first.join().unwrap();
    let first_took = begun.elapsed();
    let finished = executor.submit(&call(kind)).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    let kept = std::mem::take(&mut *timing.kept.lock().unwrap());
    assert_eq!(kept.len(), 2);
    let sent: Vec<&str> = timing
        .worker
        .journal()
        .iter()
        .map(|sent| sent.kind)
        .collect();
    assert_eq!(sent, ["load", "segment", "segment"], "{model} {kind}");
    drop(executor);

    (
        loaded - load_begun,
        first_took,
        kept[0].clone(),
        kept[1].clone(),
    )
}

/// The MiB the GPU holds, from what `sam3` says beside an answer on the GPU.
fn gpu_mib(said: &str) -> String {
    said.split("; ")
        .find_map(|part| part.strip_suffix(" MiB on the GPU"))
        .unwrap_or_else(|| panic!("no memory said: {said}"))
        .to_owned()
}

#[test]
#[ignore = "the model tier: just test-models, on a machine with the model installed"]
fn the_first_load_and_a_second_request_of_each_query_on_sam3_and_tiny() {
    let mut lines = Vec::new();
    for model in ["sam3", "tiny"] {
        for kind in ["phrase", "point", "box"] {
            let (load, first, one, two) = first_and_second(model, kind);
            assert!(one.instances > 0 && two.instances > 0, "{model} {kind}");
            lines.push((format!("{model}_{kind}_load_seconds"), seconds(load)));
            lines.push((format!("{model}_{kind}_first_seconds"), seconds(first)));
            lines.push((format!("{model}_{kind}_second_seconds"), seconds(two.took)));
        }
    }
    // SAM 3 with its two models resident: a phrase loads `Sam3Model`, then a point the
    // tracker beside it; what the GPU holds is said beside the point's answer.
    let timing = Timing::new("sam3", "both");
    let executor = timing.runner();
    for kind in ["phrase", "point"] {
        let finished = executor.submit(&call(kind)).unwrap();
        assert_eq!(finished.outcome().name(), "completed");
    }
    drop(executor);
    let kept = std::mem::take(&mut *timing.kept.lock().unwrap());
    lines.push(("sam3_one_model_gpu_mib".to_owned(), gpu_mib(&kept[0].said)));
    lines.push(("sam3_two_models_gpu_mib".to_owned(), gpu_mib(&kept[1].said)));
    write_out("coverage", &lines);
}
