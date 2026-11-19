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

pub const PLENTY: u64 = 20_000_000;

pub fn never() -> bool {
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

// --- an arm's details and second look (plan m3, task U1, item 2) ------------------------------

use crate::anthropic::submitting;

/// A matrix over the set of this id, at one run at a time, under the config's default judge
/// of Haiku, with these arms as the file writes them.
pub fn matrix_with(set: &str, arms: &str) -> Matrix {
    Matrix::parse(&format!(
        "[matrix]\nname = \"settings\"\nsets = [\"{set}\"]\narms = [{arms}]\n\
         repetitions = 1\nconcurrency = 1\n\
         [[matrix.judges]]\nbackend = \"default\"\nmodel = \"claude-haiku-5-5\"\n"
    ))
    .unwrap()
}

/// A submission of these items: each id with its disposition and confidence.
pub fn items(answers: &[(&str, &str, &str)]) -> Vec<u8> {
    let items: Vec<serde_json::Value> = answers
        .iter()
        .map(|(id, disposition, confidence)| {
            let location = (*disposition == "violation").then(|| {
                serde_json::json!({"kind": "bbox", "frame": "raw:candidate",
                                   "bbox": [0.1, 0.1, 0.4, 0.4], "polygon": null})
            });
            serde_json::json!({"id": id, "disposition": disposition, "confidence": confidence,
                               "reason": "Looked at.", "allowed_by": null, "evidence": [],
                               "location": location})
        })
        .collect();
    submitting(
        serde_json::json!({"schema_version": 1, "items": items, "findings": [],
                                  "opinion": {"verdict": "pass", "reason": "Recorded only."}}),
    )
}

pub const SF1: &str = "chk:ob:SF1@candidate";
pub const SF2: &str = "chk:ob:SF2@candidate";

/// Each run of `check` in the store: its id and its record, in the order of the runs.
pub fn checks(root: &Path) -> Vec<(String, serde_json::Value)> {
    let listed = crate::wired::whizbang(&["runs", "list"], root);
    let mut found = Vec::new();
    for line in listed.out.lines().skip(1) {
        let run = line.split('\t').next().unwrap().to_owned();
        let record = record_of(root, &run);
        if record["asked"]["action"] == "check" {
            found.push((run, record));
        }
    }
    found
}

/// The text of a run's argument of this name, if it has one.
fn argument(record: &serde_json::Value, name: &str) -> Option<String> {
    let arguments = record["asked"]["arguments"].as_array().unwrap();
    let found = arguments.iter().find(|a| a["name"] == name)?;
    Some(found["value"]["text"].as_str().unwrap().to_owned())
}

fn steps_of(record: &serde_json::Value) -> Vec<String> {
    (record["steps"].as_array().unwrap().iter())
        .map(|step| step["action"].as_str().unwrap().to_owned())
        .collect()
}

/// What a run kept of its judging.
fn judged_of(root: &Path, run: &str) -> serde_json::Value {
    let run = RunId::parse(run).unwrap();
    let kept = whizbang::door::diagnostic(root, &run, whizbang_squint::check::JUDGED).unwrap();
    serde_json::from_slice(&kept).unwrap()
}

#[test]
fn an_arms_details_and_second_look_are_in_its_cells_identity_and_run() {
    let arm = r#"{ arm = "F", details = [{ detail = "change_map", use_cases = ["source-fidelity"] }], second_look = "on" }"#;
    let matrix = matrix_with(SET, &format!("\"F\", {arm}"));
    // The arm's name holds both settings, and reads back as the same arm.
    let named = "F;details=change_map@source-fidelity;second_look=on";
    let settled = &matrix.matrix.arms[1];
    assert_eq!(settled.to_string(), named);
    assert_eq!(
        &named.parse::<whizbang_eval::matrix::Arm>().unwrap(),
        settled
    );
    // Each setting is part of the matrix's identity.
    let without_look = matrix_with(
        SET,
        r#""F", { arm = "F", details = [{ detail = "change_map", use_cases = ["source-fidelity"] }] }"#,
    );
    let without_detail = matrix_with(SET, r#""F", { arm = "F", second_look = "on" }"#);
    assert_ne!(matrix.identity(), without_look.identity());
    assert_ne!(matrix.identity(), without_detail.identity());
    assert_ne!(without_look.identity(), without_detail.identity());
    // And of each cell's, and of its run's arguments: arm F as before, then the arm with
    // its settings, whose tentative item gets its second look.
    let (judged, tiny) = world_with("matrix-settings", &["a"]);
    let answers = vec![
        items(&[(SF1, "held", "confident"), (SF2, "held", "confident")]),
        items(&[(SF1, "held", "confident"), (SF2, "held", "tentative")]),
        items(&[(SF2, "held", "confident")]),
    ];
    let (report, log) = run_matrix(&judged, answers, &|e| {
        ran(e, &judged, &tiny, &matrix, PLENTY, &never)
    });
    let report = report.unwrap();
    assert_eq!(
        (report.completed, report.verdicts.get("pass")),
        (2, Some(&2))
    );
    assert_eq!(log.lock().unwrap().len(), 3);
    let root = &judged.world.root;
    let mut runs = checks(root);
    runs.sort_by_key(|(_, record)| argument(record, "second_look"));
    let (plain, set) = (&runs[0].1, &runs[1].1);
    let cell = |record: &serde_json::Value| -> serde_json::Value {
        serde_json::from_str(&argument(record, "cell").unwrap()).unwrap()
    };
    assert_eq!(cell(plain)["arm"], "F");
    assert_eq!(cell(set)["arm"], named);
    assert_eq!(argument(plain, "second_look").as_deref(), Some("off"));
    assert_eq!(argument(set, "second_look").as_deref(), Some("on"));
    assert_eq!(argument(plain, "detail").as_deref(), Some("align"));
    assert_eq!(argument(set, "detail").as_deref(), Some("change_map"));
    assert_eq!(steps_of(plain), ["align"]);
    assert_eq!(steps_of(set)[..2], ["align", "change_map"]);
}

#[test]
fn an_arm_naming_a_detail_for_a_use_case_that_cannot_take_it_is_refused_before_anything_runs() {
    // Tiles for a use case that relates a pair, a pair's detail for one that has none, a
    // detail that is none, and a setting of the control: each refused as the file is read.
    for (arm, says) in [
        (
            r#"{ arm = "F", details = [{ detail = "tiles", use_cases = ["source-fidelity"] }] }"#,
            "the detail tiles cannot be measured for the use case \"source-fidelity\"",
        ),
        (
            r#"{ arm = "F", details = [{ detail = "align", use_cases = ["reality-checks"] }] }"#,
            "the detail align cannot be measured for the use case \"reality-checks\"",
        ),
        (
            r#"{ arm = "F", details = [{ detail = "outline", use_cases = ["reality-checks"] }] }"#,
            "is no detail measured first",
        ),
        (r#""C;second_look=on""#, "settings of an arm F"),
        (r#""F;second_look=maybe""#, "second_look is on or off"),
    ] {
        let text = format!(
            "[matrix]\nname = \"m\"\nsets = [\"{SET}\"]\narms = [{arm}]\nrepetitions = 1\n\
             [[matrix.judges]]\nbackend = \"default\"\nmodel = \"claude-haiku-5-5\"\n"
        );
        let refused = Matrix::parse(&text).unwrap_err();
        assert!(refused.contains(says), "{arm}: {refused}");
    }
    // What the matrix takes of each use case is what check takes.
    for (detail, _) in whizbang_eval::matrix::DETAILS_TAKEN {
        for use_case in whizbang_squint::use_case::NAMES {
            assert_eq!(
                whizbang_eval::matrix::takes(detail, use_case),
                whizbang_squint::details::takes(detail, use_case),
                "{detail} {use_case}"
            );
        }
    }
    // Tiles for brief-compliance, whose job here has a reference: refused as the matrix is
    // planned, naming the item, before any run.
    let judged = crate::anthropic::Judged::new("matrix-tiles-pair");
    let id = "brief-compliance/tiny-v1";
    let tiny = crate::fetch::tiny_of(
        &judged.world.beside("set"),
        id,
        "brief-compliance",
        &["candidate", "reference"],
        &["a"],
    );
    crate::fetch::fetched_of(&tiny, &judged.world.root, id);
    let matrix = matrix_with(
        id,
        r#"{ arm = "F", details = [{ detail = "tiles", use_cases = ["brief-compliance"] }] }"#,
    );
    let Err(refused) = evaluation::plan(&matrix, &tiny.root, &judged.world.root) else {
        panic!("planned");
    };
    assert!(refused.contains("item a"), "{refused}");
    assert!(
        refused.contains("this brief-compliance job relates a pair"),
        "{refused}"
    );
    assert!(checks(&judged.world.root).is_empty());
}

const REALITY: &str = "reality-checks/tiny-v1";

/// A judged world with a set of reality-checks of these items in its store.
fn reality_world(label: &str, items: &[&str]) -> (crate::anthropic::Judged, Tiny) {
    let judged = crate::anthropic::Judged::new(label);
    let tiny = crate::fetch::tiny_of(
        &judged.world.beside("set"),
        REALITY,
        "reality-checks",
        &["candidate"],
        items,
    );
    crate::fetch::fetched_of(&tiny, &judged.world.root, REALITY);
    (judged, tiny)
}

/// The four items of reality-checks, `RE2` as given and the rest held, confidently.
fn reality(disposition: &str, confidence: &str) -> Vec<u8> {
    let ids: Vec<String> = ["RE1", "RE2", "RE3", "RE4"]
        .iter()
        .map(|rule| format!("chk:ob:{rule}@candidate"))
        .collect();
    let answers: Vec<(&str, &str, &str)> = ids
        .iter()
        .map(|id| match id.as_str() {
            "chk:ob:RE2@candidate" => (id.as_str(), disposition, confidence),
            _ => (id.as_str(), "held", "confident"),
        })
        .collect();
    items(&answers)
}

#[test]
fn an_arm_with_tiles_measures_the_tiles_first_in_each_of_its_runs() {
    let (judged, tiny) = reality_world("matrix-tiles", &["a", "b"]);
    let matrix = matrix_with(
        REALITY,
        r#"{ arm = "F", details = [{ detail = "tiles", use_cases = ["reality-checks"] }] }"#,
    );
    let answers = vec![reality("held", "confident"); 2];
    let (report, log) = run_matrix(&judged, answers, &|e| {
        ran(e, &judged, &tiny, &matrix, PLENTY, &never)
    });
    assert_eq!(report.unwrap().completed, 2);
    let runs = checks(&judged.world.root);
    assert_eq!(runs.len(), 2);
    for (_, record) in &runs {
        assert_eq!(argument(record, "detail").as_deref(), Some("tiles"));
        assert_eq!(steps_of(record), ["view", "view", "view", "view"]);
    }
    // Each request opened with the tiles, before the judge's first turn.
    for (_, request) in log.lock().unwrap().iter() {
        let words = whizbang_squint::details::TILES_INTRO;
        let body = String::from_utf8_lossy(request);
        assert!(
            body.contains(&words[..60]),
            "{}",
            &body[..200.min(body.len())]
        );
    }
}

#[test]
fn an_arm_with_the_second_look_gives_one_on_a_tentative_item() {
    let (judged, tiny) = reality_world("matrix-second-look", &["a", "b"]);
    let matrix = matrix_with(REALITY, r#"{ arm = "F", second_look = "on" }"#);
    // Item a is left tentative and settled; item b is confident at once.
    let answers = vec![
        reality("held", "tentative"),
        items(&[("chk:ob:RE2@candidate", "held", "confident")]),
        reality("held", "confident"),
    ];
    let (report, log) = run_matrix(&judged, answers, &|e| {
        ran(e, &judged, &tiny, &matrix, PLENTY, &never)
    });
    let report = report.unwrap();
    assert_eq!(report.verdicts.get("pass"), Some(&2));
    assert_eq!(log.lock().unwrap().len(), 3);
    let root = &judged.world.root;
    let mut looks: Vec<serde_json::Value> = checks(root)
        .iter()
        .map(|(run, _)| judged_of(root, run)["second_look"].clone())
        .collect();
    looks.sort_by_key(|look| look.is_null());
    assert_eq!(
        looks[0]["items"],
        serde_json::json!(["chk:ob:RE2@candidate"])
    );
    assert_eq!(looks[0]["outcome"], "settled");
    assert_eq!(looks[0]["verdict_before"], "unsure");
    assert_eq!(looks[0]["verdict_after"], "pass");
    assert!(looks[1].is_null());
}
