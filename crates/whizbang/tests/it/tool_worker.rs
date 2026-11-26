//! `tool_worker::…`: the tool worker (plan 2C, task X1, items 2 and 3; ADR 0032). Through
//! the executor, a tool's ask is a nested call of its run, and a cancel of the run ends
//! the nested call; through the supervisor itself, one ask is open at a time, and a
//! tool's fault or a cancellation ends the worker's group, a process the tool started
//! with it.

use std::sync::atomic::Ordering;
use std::time::Duration;

use serde_json::{Map, json};
use whizbang_backends::pack::ToolWorker;
use whizbang_backends::pack::tool::{Asked, Mask, ToolFault, ToolImage, ToolLimits, ToolRun};
use whizbang_core::contract::{Answer, Call, Cancellation, Executor, Outcome, Request, Value};
use whizbang_core::identity::Digest;
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::worker as kit;

use crate::anthropic::record_of;
use crate::tool_file::{BOUNDS, COVERS, Tools, ran, tool_start, tool_text};

/// A declaration that asks for at most `most` segments.
pub(crate) fn asking(text: &str, most: u64) -> String {
    text.replace(
        "# time_limit_s = 20\n",
        &format!("# time_limit_s = 20\n# segments_most = {most}\n"),
    )
}

/// The nested calls a run's record lists: each its action and how it ended.
fn steps(root: &std::path::Path, run: &str) -> Vec<(String, String)> {
    let record = record_of(root, run);
    record["steps"]
        .as_array()
        .unwrap_or_else(|| panic!("{record}"))
        .iter()
        .map(|step| {
            (
                step["action"].as_str().unwrap_or("").to_owned(),
                step["status"].as_str().unwrap_or("").to_owned(),
            )
        })
        .collect()
}

#[test]
fn a_capability_asked_by_a_tool_is_a_nested_call_under_the_runs_allowance() {
    let text = asking(&tool_text("covers", BOUNDS, COVERS), 1);
    let tools = Tools::new("tool-worker-nested", &[("covers.py", text)]);
    let executor = tools.runner(&tools.over(None));
    let call = Call::new("covers")
        .input("source", tools.image_id())
        .input("candidate", tools.image_id())
        .argument("phrase", Value::text("circle"));
    // The tool declares one segment: its admission reserves for it, and the ask is
    // answered, a call of the run's own.
    let (run, outcome) = ran(&executor, &call);
    assert!(matches!(outcome, Outcome::Completed { .. }), "{outcome:?}");
    assert_eq!(
        steps(&tools.root, run.as_str()),
        [("segment".to_owned(), "completed".to_owned())]
    );
    let report = whizbang::door::diagnostic(&tools.root, &run, whizbang::tool_file::REPORT);
    let report: serde_json::Value = serde_json::from_slice(&report.unwrap()).unwrap();
    // The fake's box for the disc, 180 by 180 pixels of the fixture, and no key of the
    // parent's in the worker's environment.
    let covered = report["readings"][0]["values"]["covered"].as_f64().unwrap();
    assert!(covered > 0.0 && covered < 1.0, "{report}");
    assert_eq!(report["readings"][0]["subject"], json!("keys:"));
    assert_eq!(report["measurements"][0]["value"], json!(covered));
    let record = record_of(&tools.root, run.as_str());
    let settings = record["asked"]["settings"]
        .as_array()
        .unwrap_or_else(|| panic!("{record}"));
    let digest = settings
        .iter()
        .find(|s| s["name"] == json!("tool_file_digest"))
        .map(|s| s["value"].clone());
    assert!(digest.is_some(), "{record}");
    assert!(
        record
            .to_string()
            .contains(report["digest"].as_str().unwrap()),
        "{record}"
    );
}

#[test]
fn cancelling_a_tools_run_cancels_its_nested_call() {
    let text = asking(&tool_text("covers", BOUNDS, COVERS), 1);
    let tools = Tools::new("tool-worker-cancel", &[("covers.py", text)]);
    let barriers = Barriers::new();
    let executor = tools.runner(&tools.over(Some(barriers.clone())));
    let call = Call::new("covers")
        .input("source", tools.image_id())
        .input("candidate", tools.image_id());
    let Ok(Answer::Started(run)) = executor.start(&Request::new(call)) else {
        panic!("the run was not started");
    };
    // The tool's ask is in the segmenter, held there; the tool's run is cancelled.
    barriers.arrived("segment", 1);
    executor.cancel(&run).unwrap();
    barriers.open("segment");
    executor.wait(&run).unwrap();
    // The nested call was cancelled with it, and the worker's group was ended.
    assert_eq!(
        steps(&tools.root, run.as_str()),
        [("segment".to_owned(), "cancelled".to_owned())]
    );
    let record = record_of(&tools.root, run.as_str());
    assert_eq!(record["outcome"]["status"], json!("cancelled"), "{record}");
    assert_eq!(tools.worker.tool_pid(), None);
}

/// A supervisor of the tool worker, as the binary's would be, from the test kit's Python.
fn supervisor() -> ToolWorker {
    tool_start()(None, &whizbang_core::contract::Secrets::none()).unwrap()
}

/// A run of this code, with one picture of 2 by 2 pixels named `source`.
fn run_of(code: &str) -> ToolRun {
    ToolRun {
        tool: "probe".to_owned(),
        digest: Digest::of_bytes(code.as_bytes()).hex(),
        code: code.as_bytes().to_vec(),
        arguments: Map::new(),
        images: vec![ToolImage {
            name: "source".to_owned(),
            width: 2,
            height: 2,
            rgba: vec![200; 16],
        }],
    }
}

fn limits(cancellation: &Cancellation) -> ToolLimits<'_> {
    ToolLimits {
        cancellation,
        deadline: None,
        limit: Duration::from_secs(20),
    }
}

/// A mask of the 2 by 2 picture, its first row on.
fn row() -> Vec<Mask> {
    vec![Mask {
        score: Some(0.5),
        bounds: [0.0, 0.0, 2.0, 1.0],
        width: 2,
        height: 2,
        plane: vec![255, 255, 0, 0],
    }]
}

/// The process the error output names after `child `.
fn child_of(worker: &ToolWorker) -> u32 {
    let lines = worker.error_output();
    let line = lines.iter().find_map(|line| line.strip_prefix("child "));
    line.unwrap_or_else(|| panic!("{lines:?}"))
        .trim()
        .parse()
        .unwrap()
}

/// Whether the process with this id still runs, as other than one that ended.
fn lives(pid: u32) -> bool {
    kit::processes()
        .iter()
        .any(|listed| listed.pid == pid && !listed.state.starts_with('Z'))
}

const TWO_ASKS: &str = r#"import threading
from whizbang_pack.tool_worker import CapabilityRefused
def run(context):
    said = []
    def ask():
        try:
            context.segment('source', phrase='red')
            said.append('answered')
        except CapabilityRefused as refused:
            said.append('refused: ' + refused.sentence)
    other = threading.Thread(target=ask, daemon=True)
    other.start()
    ask()
    other.join(10)
    return {'said': sorted(said)}
"#;

#[test]
fn a_second_capability_request_while_one_is_open_is_refused() {
    let mut worker = supervisor();
    let cancellation = Cancellation::new();
    let heard = worker.asks_heard();
    // The first ask is answered only once the second is there: the second came while the
    // first was open, and is refused.
    let mut answered = 0;
    let mut answer = |_: &Asked| {
        answered += 1;
        for _ in 0..2_000 {
            if heard.load(Ordering::SeqCst) >= 2 {
                return Ok(row().into());
            }
            std::thread::park_timeout(Duration::from_millis(5));
        }
        Err("the second ask never came".to_owned())
    };
    let said = worker.run(run_of(TWO_ASKS), &limits(&cancellation), &mut answer);
    assert_eq!(answered, 1);
    let refused =
        "refused: one capability request is open at a time; this one came while another was";
    assert_eq!(said, Ok(json!({"said": ["answered", refused]})));
    // The worker lives on for the next run.
    assert!(worker.pid().is_some());
    worker.stop();
}

const FAULTS: &str = r#"import subprocess, sys
def run(context):
    child = subprocess.Popen(['/bin/sleep', '30'])
    sys.stderr.write('child %d\n' % child.pid)
    sys.stderr.flush()
    raise RuntimeError('the tool gives up')
"#;

#[test]
fn a_tools_fault_ends_the_group_and_a_child_holding_the_pipes_with_it() {
    let mut worker = supervisor();
    let cancellation = Cancellation::new();
    let ended = worker.run(run_of(FAULTS), &limits(&cancellation), &mut |_| {
        Ok(Vec::<Mask>::new().into())
    });
    assert_eq!(
        ended,
        Err(ToolFault::Failed(
            "the tool failed with RuntimeError: the tool gives up".to_owned()
        ))
    );
    // The worker was ended, its group with it: the child that held its input and its
    // error output is gone, and every reader of the pipes ended.
    assert_eq!(worker.pid(), None);
    let child = child_of(&worker);
    assert!(
        !lives(child),
        "the tool's child {child} outlived the worker"
    );
    assert!(kit::readers_ended(Duration::from_secs(5)));
    assert!(kit::pack_children().is_empty());
}

const DEAF: &str = r#"import subprocess, sys, time
from whizbang_pack.tool_worker import CapabilityRefused
def run(context):
    child = subprocess.Popen(['/bin/sleep', '30'])
    sys.stderr.write('child %d\n' % child.pid)
    sys.stderr.flush()
    try:
        context.segment('source', phrase='red')
    except CapabilityRefused:
        pass
    time.sleep(30)
    return {}
"#;

#[test]
fn a_cancellation_ends_the_group_and_waits_for_it() {
    let mut worker = supervisor();
    let cancellation = Cancellation::new();
    // The run is cancelled while its tool asks; the tool then waits, deaf to it.
    let mut answer = |_: &Asked| {
        cancellation.cancel();
        Err("cancelled".to_owned())
    };
    let ended = worker.run(run_of(DEAF), &limits(&cancellation), &mut answer);
    assert_eq!(ended, Err(ToolFault::Cancelled));
    // Ended at once, not at the tool's time: its group with it, and waited for.
    assert_eq!(worker.pid(), None);
    assert!(worker.ended().is_some());
    let child = child_of(&worker);
    assert!(
        !lives(child),
        "the tool's child {child} outlived the worker"
    );
    assert!(kit::readers_ended(Duration::from_secs(5)));
    assert!(kit::pack_children().is_empty());
}

const HOLDS_THE_LOCK: &str = r#"import subprocess, sys
def run(context):
    child = subprocess.Popen(['/bin/sleep', '30'])
    sys.stderr.write('child %d\n' % child.pid)
    sys.stderr.flush()
    sum(range(4 * 10**9))
    return {}
"#;

#[test]
fn a_worker_that_cannot_end_its_group_has_it_ended_by_whizbang() {
    // The tool starts a process that holds the worker's pipes, then holds Python's lock in
    // C code, so the worker's reader cannot act on the end of its input. The run is given
    // up at its time; past the wait for the worker, whizbang kills the group it made for
    // it (the owner's approval of 2026-11-18, ADR 0032), and the child goes with it.
    let mut worker = supervisor();
    let cancellation = Cancellation::new();
    let limits = ToolLimits {
        cancellation: &cancellation,
        deadline: None,
        limit: Duration::from_secs(2),
    };
    let ended = worker.run(run_of(HOLDS_THE_LOCK), &limits, &mut |_| {
        Ok(Vec::<Mask>::new().into())
    });
    assert_eq!(ended, Err(ToolFault::TimedOut));
    assert_eq!(worker.pid(), None);
    let child = child_of(&worker);
    assert!(
        !lives(child),
        "the tool's child {child} outlived the worker"
    );
    assert!(kit::readers_ended(Duration::from_secs(5)));
    assert!(kit::pack_children().is_empty());
}

#[test]
fn a_tool_file_of_another_digest_is_run_by_a_worker_of_its_own() {
    let mut worker = supervisor();
    let cancellation = Cancellation::new();
    let one = "def run(context):\n    return {}\n";
    let other = "def run(context):\n    return {'readings': []}\n";
    let mut nothing = |_: &Asked| Ok(Vec::<Mask>::new().into());
    worker
        .run(run_of(one), &limits(&cancellation), &mut nothing)
        .unwrap();
    let first = worker.pid().unwrap();
    // The same file again: the same worker.
    worker
        .run(run_of(one), &limits(&cancellation), &mut nothing)
        .unwrap();
    assert_eq!(worker.pid(), Some(first));
    // Another file, or an edit: the worker is ended, and another started.
    worker
        .run(run_of(other), &limits(&cancellation), &mut nothing)
        .unwrap();
    let second = worker.pid().unwrap();
    assert_ne!(second, first);
    assert!(!lives(first));
    worker.stop();
}
