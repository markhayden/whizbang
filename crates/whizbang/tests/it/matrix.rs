//! `matrix::…`: a matrix's cells run as runs of `check` through the binary's executor,
//! under a judge served on loopback (plan 2C, task E1, item 4). Sets are made in the
//! test, of three or four items.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use whizbang::evaluation::{self, Planned, Report, Running};
use whizbang_core::contract::{
    Answer, Call, CallError, Cancel, Executor, PreparedCall, Request, RequestId, RunId, Watched,
};
use whizbang_eval::matrix::Matrix;
use whizbang_eval::sets::Expected;

use crate::anthropic::{CHECK_CLASSES, Judged, Log, record_of, serving, submission};
use crate::fetch::{SET, Tiny, fetched, tiny};

/// A matrix over the test's set under the config's default judge, of Haiku.
pub fn matrix_of(arm: &str, repetitions: u32, concurrency: u32) -> Matrix {
    Matrix::parse(&format!(
        "[matrix]\nname = \"tiny\"\nsets = [\"{SET}\"]\narms = [\"{arm}\"]\n\
         repetitions = {repetitions}\nconcurrency = {concurrency}\n\
         [[matrix.judges]]\nbackend = \"default\"\nmodel = \"claude-haiku-5-5\"\n"
    ))
    .unwrap()
}

/// What the runner was given beside the executor.
type Planning = (Vec<Planned>, BTreeMap<(String, String), Expected>);

/// A judged world with a set of these items whose pictures are in its store.
pub fn world_with(label: &str, items: &[&str]) -> (Judged, Tiny) {
    let judged = Judged::new(label);
    let tiny = tiny(&judged.world.beside("set"), items);
    fetched(&tiny, &judged.world.root);
    (judged, tiny)
}

/// Runs `wrap` over the binary's executor under a judge on loopback that gives these
/// answers: what it gave back, and what the server saw.
pub fn run_matrix(
    judged: &Judged,
    answers: Vec<Vec<u8>>,
    wrap: &dyn Fn(&dyn Executor) -> Result<Report, String>,
) -> (Result<Report, String>, Log) {
    let (listener, log) = serving(&judged.pki, answers);
    let origin = format!("https://127.0.0.1:{}", listener.port());
    let over = judged.over_with(
        &judged.address(listener.port()),
        Some(&origin),
        &CHECK_CLASSES,
        "",
    );
    let executor = judged.runner(&over);
    let report = wrap(&executor);
    drop(executor);
    listener.finish();
    (report, log)
}

/// The runner over the matrix's plan.
pub fn ran(
    executor: &dyn Executor,
    judged: &Judged,
    tiny: &Tiny,
    matrix: &Matrix,
    left_micros: u64,
    stop: &dyn Fn() -> bool,
) -> Result<Report, String> {
    let (planned, labels): Planning =
        evaluation::plan(matrix, &tiny.root, &judged.world.root).unwrap();
    let running = Running {
        executor,
        root: &judged.world.root,
        left_micros,
        stop,
    };
    evaluation::run(matrix, &planned, &labels, &running)
}

const PLENTY: u64 = 20_000_000;

fn never() -> bool {
    false
}

/// The cell each run of `check` in the store says it is, by run.
fn cells(root: &Path) -> Vec<serde_json::Value> {
    let listed = crate::wired::whizbang(&["runs", "list"], root);
    let mut found = Vec::new();
    for line in listed.out.lines().skip(1) {
        let run = line.split('\t').next().unwrap();
        let record = record_of(root, run);
        if record["asked"]["action"] != "check" {
            continue;
        }
        let cell = record["asked"]["arguments"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["name"] == "cell")
            .map(|a| a["value"]["text"].as_str().unwrap().to_owned())
            .unwrap();
        found.push(serde_json::from_str(&cell).unwrap());
    }
    found
}

#[test]
fn each_cell_runs_as_a_run_of_check_with_its_cell_in_the_record() {
    let (judged, tiny) = world_with("matrix-cells", &["a", "b", "c"]);
    let matrix = matrix_of("C", 1, 2);
    let answers = vec![submission("held", "Kept."); 3];
    let (report, log) = run_matrix(&judged, answers, &|e| {
        ran(e, &judged, &tiny, &matrix, PLENTY, &never)
    });
    let report = report.unwrap();
    assert_eq!((report.cells, report.started, report.completed), (3, 3, 3));
    assert_eq!(report.verdicts.get("pass"), Some(&3));
    assert_eq!((report.labelled, report.agreed), (3, 3));
    assert_eq!(log.lock().unwrap().len(), 3);
    let mut found = cells(&judged.world.root);
    found.sort_by_key(|cell| cell["item"].as_str().unwrap().to_owned());
    let identity: String = matrix.identity().chars().take(16).collect();
    for (cell, item) in found.iter().zip(["a", "b", "c"]) {
        assert_eq!(
            cell,
            &serde_json::json!({
                "matrix": "tiny", "identity": identity, "set": SET, "item": item,
                "backend": "default", "model": "claude-haiku-5-5", "arm": "C",
                "repetition": 1, "attempt": 1
            })
        );
    }
    assert_eq!(found.len(), 3);
}

/// An answer of 400 from the judge, which is not sent again: the run ends in an error.
fn refused_answer() -> Vec<u8> {
    let body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"no"}}"#;
    format!(
        "HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

#[test]
fn an_interrupted_matrix_resumes_and_a_failed_attempt_is_kept() {
    let (judged, tiny) = world_with("matrix-resume", &["a", "b", "c"]);
    let matrix = matrix_of("C", 1, 1);
    // The first time, a passes, b's judge refuses it, and the matrix is stopped before c.
    let asked = Mutex::new(0);
    let stop = || {
        let mut asked = asked.lock().unwrap();
        *asked += 1;
        *asked > 2
    };
    let answers = vec![submission("held", "Kept."), refused_answer()];
    let (report, _) = run_matrix(&judged, answers, &|e| {
        ran(e, &judged, &tiny, &matrix, PLENTY, &stop)
    });
    let report = report.unwrap();
    assert_eq!(
        (
            report.started,
            report.completed,
            report.failed,
            report.not_run
        ),
        (2, 1, 1, 1)
    );
    // Run again, it resumes: only c runs. b's failed attempt is kept and reported, and is
    // not run again, so no later success replaces it.
    let answers = vec![submission("held", "Kept.")];
    let (report, log) = run_matrix(&judged, answers, &|e| {
        ran(e, &judged, &tiny, &matrix, PLENTY, &never)
    });
    let report = report.unwrap();
    assert_eq!(
        (
            report.started,
            report.completed,
            report.failed,
            report.not_run
        ),
        (1, 2, 1, 0)
    );
    assert_eq!(log.lock().unwrap().len(), 1);
    let identity: String = matrix.identity().chars().take(16).collect();
    let attempts = evaluation::attempts(&judged.world.root, &identity).unwrap();
    let ended: Vec<(String, Vec<evaluation::Ended>)> = attempts
        .iter()
        .map(|(cell, list)| (cell.item.clone(), list.iter().map(|(_, _, e)| *e).collect()))
        .collect();
    use evaluation::Ended::{Completed, Failed};
    assert_eq!(
        ended,
        [
            ("a".to_owned(), vec![Completed]),
            ("b".to_owned(), vec![Failed]),
            ("c".to_owned(), vec![Completed])
        ]
    );
}

#[test]
fn a_matrix_whose_forecast_passes_the_allowance_left_is_refused_before_its_first_run() {
    let (judged, tiny) = world_with("matrix-forecast", &["a", "b", "c"]);
    let matrix = matrix_of("F", 2, 2);
    // Six cells of Haiku under F: 6 x 3,293 x 2 x 2 = 79,032 millionths of a dollar.
    let (planned, _) = evaluation::plan(&matrix, &tiny.root, &judged.world.root).unwrap();
    let cells: Vec<_> = planned.iter().map(|p| p.cell.clone()).collect();
    assert_eq!(whizbang_eval::matrix::forecast(&cells), Ok(79_032));
    let executor = judged.runner(&judged.over("https://127.0.0.1:9/v1/messages", None));
    let refused = ran(&executor, &judged, &tiny, &matrix, 79_031, &never).unwrap_err();
    assert_eq!(
        refused,
        "the matrix's forecast for its 6 runs still to run is 79032 millionths of a dollar, \
         more than the 79031 the store's allowance has left; nothing was run"
    );
    drop(executor);
    let listed = crate::wired::whizbang(&["runs", "list"], &judged.world.root);
    assert_eq!(listed.out, "runs: 0\n");
}

/// An executor that, before each start, counts the reservations the store's books hold
/// open, and keeps the most it saw.
struct Counting<'a> {
    inner: &'a dyn Executor,
    root: &'a Path,
    most: Mutex<usize>,
}

impl Counting<'_> {
    fn open(&self) -> usize {
        use whizbang_core::store::{Reader, fs::RealFs};
        let reader = Reader::open(self.root, std::sync::Arc::new(RealFs)).unwrap();
        let generation = reader.snapshot().unwrap();
        let books = whizbang_core::spend::fold::fold(&generation, |e| reader.read(e));
        (books.reservations().values())
            .filter(|one| one.state() == whizbang_core::spend::fold::State::Open)
            .count()
    }
}

impl Executor for Counting<'_> {
    fn prepare(&self, call: &Call) -> Result<PreparedCall, CallError> {
        self.inner.prepare(call)
    }
    fn submit(&self, call: &Call) -> Result<whizbang_core::contract::Finished, CallError> {
        self.inner.submit(call)
    }
    fn request(&self, request: &Request) -> Result<Answer, CallError> {
        self.inner.request(request)
    }
    fn start(&self, request: &Request) -> Result<Answer, CallError> {
        let started = self.inner.start(request);
        // The run just started holds its reservation from its admission.
        let open = self.open();
        let mut most = self.most.lock().unwrap();
        *most = (*most).max(open);
        started
    }
    fn status(&self, run: &RunId) -> Result<Watched, CallError> {
        self.inner.status(run)
    }
    fn wait(&self, run: &RunId) -> Result<Watched, CallError> {
        self.inner.wait(run)
    }
    fn cancel(&self, run: &RunId) -> Result<Cancel, CallError> {
        self.inner.cancel(run)
    }
    fn request_id(&self, text: &str) -> Result<RequestId, CallError> {
        self.inner.request_id(text)
    }
}

#[test]
fn at_most_the_concurrency_of_runs_hold_a_reservation_at_once() {
    let (judged, tiny) = world_with("matrix-concurrency", &["a", "b", "c", "d"]);
    let matrix = matrix_of("C", 1, 2);
    let answers = vec![submission("held", "Kept."); 4];
    let most = Mutex::new(0);
    let (report, _) = run_matrix(&judged, answers, &|e| {
        let counting = Counting {
            inner: e,
            root: &judged.world.root,
            most: Mutex::new(0),
        };
        let report = ran(&counting, &judged, &tiny, &matrix, PLENTY, &never);
        *most.lock().unwrap() = *counting.most.lock().unwrap();
        report
    });
    assert_eq!(report.unwrap().completed, 4);
    let most = *most.lock().unwrap();
    assert!(
        (1..=2).contains(&most),
        "{most} reservations were open at once"
    );
}
