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

/// `eval matrix` runs on the shared harness (plan m3, task L1, item 0) as it ran before it:
/// an interrupted matrix resumes, and a failed attempt is kept and not run again.
#[test]
fn eval_matrix_still_resumes_and_keeps_a_failed_attempt_through_the_shared_runner() {
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
    runs_of(root, "check")
}

/// Each run of this action in the store: its id and its record, in the order of the runs.
pub fn runs_of(root: &Path, action: &str) -> Vec<(String, serde_json::Value)> {
    let listed = crate::wired::whizbang(&["runs", "list"], root);
    let mut found = Vec::new();
    for line in listed.out.lines().skip(1) {
        let run = line.split('\t').next().unwrap().to_owned();
        let record = record_of(root, &run);
        if record["asked"]["action"] == action {
            found.push((run, record));
        }
    }
    found
}

/// The text of a run's argument of this name, if it has one.
pub fn argument(record: &serde_json::Value, name: &str) -> Option<String> {
    let arguments = record["asked"]["arguments"].as_array().unwrap();
    let found = arguments.iter().find(|a| a["name"] == name)?;
    Some(found["value"]["text"].as_str().unwrap().to_owned())
}

pub fn steps_of(record: &serde_json::Value) -> Vec<String> {
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
    // A second look may be given per use case, `on` or `strict`, named or written as a table.
    let per = "F;second_look=on@reference-consistency|strict@reality-checks";
    let arm: whizbang_eval::matrix::Arm = per.parse().unwrap();
    assert_eq!(arm.to_string(), per);
    assert_eq!(arm.second_look_for("reference-consistency"), "on");
    assert_eq!(arm.second_look_for("reality-checks"), "strict");
    assert_eq!(arm.second_look_for("source-fidelity"), "off");
    let table = matrix_with(
        SET,
        r#"{ arm = "F", second_look = [{ look = "on", use_cases = ["reference-consistency"] }, { look = "strict", use_cases = ["reality-checks"] }] }"#,
    );
    assert_eq!(table.matrix.arms[0], arm);
    assert_eq!(settled.second_look_for("product-fidelity"), "on");
    for refused in [
        "F;second_look=on@reality-checks|strict@reality-checks",
        "F;second_look=maybe@reality-checks",
        "F;second_look=on|strict@reality-checks",
        "F;second_look=on@nowhere",
    ] {
        assert!(
            refused.parse::<whizbang_eval::matrix::Arm>().is_err(),
            "{refused}"
        );
    }
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
        (
            r#"{ arm = "C", details = [{ detail = "align", use_cases = ["source-fidelity"] }] }"#,
            "details are a setting of an arm F",
        ),
        (
            r#""F;second_look=maybe""#,
            "second_look is on, strict or off",
        ),
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

#[test]
fn the_second_look_is_an_arm_of_c() {
    // Plan mE, task M1, item 2: arm C may take the second look, and stays the control
    // otherwise: nothing measured first, `view` and `align` offered, `align` answered with
    // its pictures alone.
    let matrix = matrix_with(SET, r#""C", { arm = "C", second_look = "on" }"#);
    let looked = &matrix.matrix.arms[1];
    assert_eq!(looked.to_string(), "C;second_look=on");
    assert_eq!(
        "C;second_look=on".parse::<whizbang_eval::matrix::Arm>(),
        Ok(looked.clone())
    );
    assert_eq!(
        (looked.offered(), looked.details(), looked.projection()),
        ("view,align".to_owned(), "none", "visual_only")
    );
    // Forecast at three times arm C, as a second look under F is.
    let cell = |arm: &str| whizbang_eval::matrix::Cell {
        set: SET.to_owned(),
        item: "a".to_owned(),
        backend: "default".to_owned(),
        model: "claude-haiku-5-5".to_owned(),
        arm: arm.to_owned(),
        repetition: 1,
    };
    assert_eq!(
        whizbang_eval::matrix::forecast(&[cell("C;second_look=on")]),
        Ok(3 * whizbang_eval::matrix::forecast(&[cell("C")]).unwrap())
    );
    // Run: arm C as before, then arm C with its second look, whose tentative item gets one
    // more turn.
    let (judged, tiny) = world_with("matrix-c-look", &["a"]);
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
    let mut runs = checks(&judged.world.root);
    runs.sort_by_key(|(_, record)| argument(record, "second_look"));
    let (plain, look) = (&runs[0].1, &runs[1].1);
    for (record, second_look) in [(plain, "off"), (look, "on")] {
        assert_eq!(
            argument(record, "second_look").as_deref(),
            Some(second_look)
        );
        assert_eq!(argument(record, "details").as_deref(), Some("none"));
        assert_eq!(
            argument(record, "projection").as_deref(),
            Some("visual_only")
        );
        assert_eq!(argument(record, "tools").as_deref(), Some("view,align"));
    }
    let arm_of = |record: &serde_json::Value| -> serde_json::Value {
        serde_json::from_str::<serde_json::Value>(&argument(record, "cell").unwrap()).unwrap()
            ["arm"]
            .clone()
    };
    assert_eq!(arm_of(look), "C;second_look=on");
    let kept = judged_of(&judged.world.root, &runs[1].0);
    assert_eq!(kept["second_look"]["items"], serde_json::json!([SF2]));
    assert_eq!(kept["second_look"]["verdict_before"], "unsure");
    assert!(judged_of(&judged.world.root, &runs[0].0)["second_look"].is_null());
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

/// The bound of one check by a model, in millionths of a dollar: sixteen requests at the
/// default ceilings, at the model's row of prices in this build.
fn bound_of_a_check(model: &str) -> u64 {
    use whizbang_backends::service::spend::{DEFAULT_CEILINGS, bound_micros};
    let prices = (whizbang_backends::anthropic::spend::prices_of(model))
        .or_else(|| whizbang_backends::google::spend::prices_of(model))
        .or_else(|| whizbang_backends::openai::spend::prices_of(model))
        .unwrap_or_else(|| panic!("{model} has no price"));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let request = bound_micros(DEFAULT_CEILINGS, prices).round() as u64;
    request * whizbang_squint::check::TURNS_LIMIT as u64
}

#[test]
fn the_judge_round_is_220_and_715_cells_forecast_within_its_store_and_capped_by_its_dearest_check()
{
    // Plan e1, task J1, items 1 and 2, and "Prices and caps": the two committed matrices of
    // the judge round, their cells over the sets as committed, the forecasts `eval matrix`
    // sets against what the evaluation store has left after the format round (58.20
    // dollars), and the cap of one call: the largest bound of one check among the round's
    // judges, plus a cent. Nothing is run or sent.
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |name: &str| -> Matrix {
        let text = std::fs::read_to_string(repo.join(format!("matrices/{name}.toml"))).unwrap();
        Matrix::parse(&text).unwrap()
    };
    let (dev, blind) = (read("e-judges-dev"), read("e-judges-blind"));
    assert_eq!(dev.matrix.name, "e-judges-dev");
    assert_eq!(blind.matrix.name, "e-judges-blind");
    // The settled arm, named, and arm C: nothing measured first, view and align offered,
    // no second look; one repetition, two at once.
    for m in [&dev, &blind] {
        let arms: Vec<String> = m.matrix.arms.iter().map(ToString::to_string).collect();
        assert_eq!(arms, ["C;submission=v1"]);
        let arm = &m.matrix.arms[0];
        assert_eq!(arm.details(), "none");
        assert_eq!(arm.offered(), "view,align");
        assert!(!arm.looks());
        assert_eq!(m.matrix.repetitions, 1);
        assert_eq!(m.matrix.concurrency, 2);
    }
    let judges_of = |m: &Matrix| -> Vec<(String, String)> {
        (m.matrix.judges.iter())
            .map(|j| (j.backend.clone(), j.model.clone()))
            .collect()
    };
    let pair = |b: &str, m: &str| (b.to_owned(), m.to_owned());
    assert_eq!(
        judges_of(&dev),
        [
            pair("gemini-4-argon", "gemini-4-argon"),
            pair("gpt-6-1-sol", "gpt-6.1-sol"),
        ]
    );
    let five = [
        pair("claude-haiku-5-5", "claude-haiku-5-5"),
        pair("gemini-3-8-flash", "gemini-3.8-flash"),
        pair("gemini-4-argon", "gemini-4-argon"),
        pair("gpt-6-1-terra", "gpt-6.1-terra"),
        pair("gpt-6-1-sol", "gpt-6.1-sol"),
    ];
    assert_eq!(judges_of(&blind), five);
    // Each judge is a judge of the evaluation config, of the model it names.
    let config = std::fs::read(repo.join("labs/e-loop/config.toml")).unwrap();
    let config =
        whizbang_core::config::Config::parse(&config, &whizbang_core::contract::Secrets::none())
            .unwrap();
    let offered: Vec<(&str, Option<&str>)> = (config
        .backends(whizbang_core::contract::Capability::Judge))
    .map(|(name, backend)| (name, backend.model()))
    .collect();
    for (backend, model) in &five {
        assert!(
            offered.contains(&(backend.as_str(), Some(model.as_str()))),
            "{backend}"
        );
    }
    // The dev matrix names every dev set the repo holds, each with its own labels: the ten
    // the format round ran, 110 items. The blind matrix names blind-v3 alone, whose labels
    // are not published: 143 items, 24 families. 253 items in all.
    let items_of = |m: &Matrix, split: &str| -> Vec<(String, Vec<String>)> {
        (m.matrix.sets.iter())
            .map(|id| {
                let set = whizbang_eval::sets::find(&repo.join("evalsets"), id).unwrap();
                assert_eq!(set.manifest.set.split.to_string(), split, "{id}");
                assert_eq!(set.manifest.set.labels.is_some(), split == "dev", "{id}");
                (
                    id.clone(),
                    set.jobs.iter().map(|(i, _)| i.clone()).collect(),
                )
            })
            .collect()
    };
    let mut every_dev = Vec::new();
    for dir in whizbang_eval::sets::discover(&repo.join("evalsets")).unwrap() {
        let set = whizbang_eval::sets::Set::load(&dir).unwrap();
        if set.manifest.set.split.to_string() == "dev" {
            every_dev.push(set.manifest.set.id.clone());
        }
    }
    let mut named = dev.matrix.sets.clone();
    named.sort();
    every_dev.sort();
    assert_eq!(named, every_dev);
    assert_eq!(blind.matrix.sets, ["source-fidelity/blind-v3"]);
    let (dev_items, blind_items) = (items_of(&dev, "dev"), items_of(&blind, "blind"));
    let count = |items: &[(String, Vec<String>)]| items.iter().map(|(_, i)| i.len()).sum::<usize>();
    assert_eq!((count(&dev_items), count(&blind_items)), (110, 143));
    let v3 = whizbang_eval::sets::find(&repo.join("evalsets"), "source-fidelity/blind-v3").unwrap();
    let families: std::collections::BTreeSet<String> =
        v3.families_by_item().into_values().collect();
    assert_eq!(families.len(), 24);
    // The cells, as the binary counts them.
    let (dev_cells, blind_cells) = (dev.cells(&dev_items), blind.cells(&blind_items));
    assert_eq!(dev_cells.len(), 110 * 2);
    assert_eq!(blind_cells.len(), 143 * 5);
    assert_eq!((dev_cells.len(), blind_cells.len()), (220, 715));
    // The forecasts: each cell at its model's settled cost of a decision under arm C, the
    // whole doubled. The dev matrix runs first, against the 58.20 dollars left; the blind
    // one is set against what is left after it, at worst its whole forecast less.
    let forecast = |cells: &[whizbang_eval::matrix::Cell]| -> u64 {
        whizbang_eval::matrix::forecast(cells).unwrap()
    };
    let left: u64 = 58_200_000;
    assert_eq!(forecast(&dev_cells), 2 * 110 * (29_732 + 33_635));
    assert_eq!(forecast(&dev_cells), 13_940_740);
    assert_eq!(
        forecast(&blind_cells),
        2 * 143 * (3_293 + 22_752 + 29_732 + 17_233 + 33_635)
    );
    assert_eq!(forecast(&blind_cells), 30_500_470);
    assert!(forecast(&dev_cells) <= left);
    assert!(forecast(&blind_cells) <= left - forecast(&dev_cells));
    // The cap: the largest bound of one check among the round's judges, plus a cent; the
    // dearest rows, Argon's and Sol's, are equal.
    let bounds: Vec<u64> = five.iter().map(|(_, m)| bound_of_a_check(m)).collect();
    assert_eq!(bounds, [112_768, 725_760, 1_935_360, 967_680, 1_935_360]);
    let cap = bounds.iter().max().unwrap() + 10_000;
    assert_eq!(cap, 1_945_360);
}

#[test]
fn arm_c_is_check_as_it_ships() {
    // One item, judged once as a cell of arm C and once as `whizbang squint check` calls
    // it with no option but the judge (`door::check_job` with the measuring of nothing
    // stated): the judge is sent the same request, byte for byte (plan e1, task S1, item 8).
    let (judged, tiny) = world_with("matrix-arm-c", &["a"]);
    let matrix = matrix_of("C", 1, 1);
    let (report, by_matrix) = run_matrix(&judged, vec![submission("held", "Kept.")], &|e| {
        ran(e, &judged, &tiny, &matrix, PLENTY, &never)
    });
    assert_eq!(report.unwrap().completed, 1);
    let set = whizbang_eval::sets::find(&tiny.root, SET).unwrap();
    let job = whizbang_squint::job::Job::from_toml(&set.jobs[0].1).unwrap();
    let held = whizbang::import::pictures_by_file(&judged.world.root).unwrap();
    let images: Vec<_> = (job.images.iter())
        .map(|image| {
            let pin = image.source.as_ref().unwrap().sha256.as_deref().unwrap();
            held[&whizbang_core::identity::Digest::from_hex(pin).unwrap()]
        })
        .collect();
    let (listener, by_door) = serving(&judged.pki, vec![submission("held", "Kept.")]);
    let origin = format!("https://127.0.0.1:{}", listener.port());
    let over = judged.over_with(
        &judged.address(listener.port()),
        Some(&origin),
        &CHECK_CLASSES,
        "",
    );
    let executor = judged.runner(&over);
    let measuring = whizbang::door::Measuring::stated(None, None).unwrap();
    let root = judged.world.root.clone();
    let read = |run: &_| whizbang::door::verdict(&root, run);
    let printed =
        whizbang::door::check_job(&executor, &job, &images, None, None, &measuring, &read);
    drop(executor);
    listener.finish();
    assert!(printed.unwrap().contains("verdict: pass"));
    let (by_matrix, by_door) = (by_matrix.lock().unwrap(), by_door.lock().unwrap());
    assert_eq!((by_matrix.len(), by_door.len()), (1, 1));
    assert!(
        by_matrix[0].1 == by_door[0].1,
        "the judge was sent another request:\n{}\n{}",
        String::from_utf8_lossy(&by_matrix[0].1),
        String::from_utf8_lossy(&by_door[0].1)
    );
}

#[test]
fn the_submission_round_is_1320_cells_forecast_within_its_store_and_capped_by_its_dearest_check() {
    // Plan e1, task F2, item 1, and "Prices and caps": the committed matrix of the format
    // round, its cells over the sets as committed, the forecast `eval matrix` sets against
    // the evaluation store's 70 dollars, and the cap of one call the round needs: the
    // largest bound of one check among its judges, plus a cent. Nothing is run or sent.
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text = std::fs::read_to_string(repo.join("matrices/e-submission.toml")).unwrap();
    let matrix = Matrix::parse(&text).unwrap();
    assert_eq!(matrix.matrix.name, "e-submission");
    let arms: Vec<String> = matrix.matrix.arms.iter().map(ToString::to_string).collect();
    assert_eq!(arms, ["C;submission=v1", "C;submission=v2"]);
    for arm in &matrix.matrix.arms {
        // Arm C: nothing measured first, view and align offered, no second look.
        assert_eq!(arm.details(), "none");
        assert_eq!(arm.offered(), "view,align");
        assert!(!arm.looks());
    }
    let judges: Vec<(&str, &str)> = (matrix.matrix.judges.iter())
        .map(|j| (j.backend.as_str(), j.model.as_str()))
        .collect();
    assert_eq!(
        judges,
        [
            ("claude-haiku-5-5", "claude-haiku-5-5"),
            ("gemini-3-8-flash", "gemini-3.8-flash"),
            ("gpt-6-1-terra", "gpt-6.1-terra"),
        ]
    );
    // Each judge is a judge of the evaluation config, of the model it names.
    let config = std::fs::read(repo.join("labs/e-loop/config.toml")).unwrap();
    let config =
        whizbang_core::config::Config::parse(&config, &whizbang_core::contract::Secrets::none())
            .unwrap();
    let offered: Vec<(&str, Option<&str>)> = (config
        .backends(whizbang_core::contract::Capability::Judge))
    .map(|(name, backend)| (name, backend.model()))
    .collect();
    for (backend, model) in &judges {
        assert!(offered.contains(&(*backend, Some(*model))), "{backend}");
    }
    // The ten dev sets, 110 items, each with its own labels.
    let mut items = Vec::new();
    for id in &matrix.matrix.sets {
        let set = whizbang_eval::sets::find(&repo.join("evalsets"), id).unwrap();
        assert_eq!(set.manifest.set.split.to_string(), "dev", "{id}");
        assert!(set.manifest.set.labels.is_some(), "{id}");
        items.push((
            id.clone(),
            set.jobs.iter().map(|(i, _)| i.clone()).collect(),
        ));
    }
    let cells = matrix.cells(&items);
    assert_eq!(items.iter().map(|(_, i)| i.len()).sum::<usize>(), 110);
    assert_eq!(cells.len(), 1_320);
    // The forecast: each cell at its model's settled cost of a decision under arm C, the
    // whole doubled; under the 69.97 dollars the store has left.
    let forecast = whizbang_eval::matrix::forecast(&cells).unwrap();
    assert_eq!(forecast, 2 * 2 * 2 * 110 * (3_293 + 22_752 + 17_233));
    assert_eq!(forecast, 38_084_640);
    assert!(forecast <= 69_970_000);
    // The cap: the bound of one check is sixteen requests at the judge's ceilings, under
    // either format and with the second look or without (the turns are the budget's);
    // the dearest of the three is Terra's.
    use whizbang_backends::service::spend::{DEFAULT_CEILINGS, bound_micros};
    let check = |model: &str| -> u64 {
        let prices = (whizbang_backends::anthropic::spend::prices_of(model))
            .or_else(|| whizbang_backends::google::spend::prices_of(model))
            .or_else(|| whizbang_backends::openai::spend::prices_of(model))
            .unwrap_or_else(|| panic!("{model} has no price"));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let request = bound_micros(DEFAULT_CEILINGS, prices).round() as u64;
        request * whizbang_squint::check::TURNS_LIMIT as u64
    };
    let bounds: Vec<u64> = judges.iter().map(|(_, model)| check(model)).collect();
    assert_eq!(bounds, [112_768, 725_760, 967_680]);
    let cap = bounds.iter().max().unwrap() + 10_000;
    assert_eq!(cap, 977_680);
}
