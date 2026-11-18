//! The behavioural half of the conformance suite (plan 1C, D3; task P3b): a table of
//! cases, each produced by a fake model or a fake fault of the real pack, run over a
//! transport. The transport here is the local one, the supervisor and its worker
//! process; milestone 6 runs the same cases over the HTTP codec (ADR 0020, "The HTTP
//! codec, on paper").
//!
//! The table names every case, what produces it and the test that shows it: those of
//! tasks P3a and P3b here and in the supervisor's tests, and those of task P4
//! (cancellation, deadlines, a flood of events, a slow load or unload, a worker that ends
//! inside a call) in the queue's (`super::queue`). Every test here that starts a worker starts it through the test
//! kit's guard (`super::worker::supervised`), and is of the group `pack-workers`.

use std::collections::BTreeSet;
use std::io::{Cursor, Read};
use std::path::Path;

use whizbang_backends::models::{InstallationId, ModelName};
use whizbang_backends::pack::proto::{
    self, Answer, Ask, Awaited, Found, Message, Open, Raster, Reason,
};
use whizbang_backends::pack::worker::{Called, Failed};
use whizbang_backends::pack::{Ended, Worker};
use whizbang_core::capability::{FaultKind, Picture, Query, SegmentRequest, Stage};
use whizbang_core::contract::Secrets;
use whizbang_core::identity::{CanonicalPixels, Digest};
use whizbang_testkit::worker as kit;

use super::worker::{supervised, system};

pub(super) const WIDTH: u32 = 8;
pub(super) const HEIGHT: u32 = 6;

// --- the transport ------------------------------------------------------------------------------

/// What the cases are run over: one request of `pack/1` carried to a worker, and its end.
pub(super) trait Transport {
    /// One request, and its result or its failure.
    fn call(&mut self, ask: Ask) -> Result<Called, Box<Failed>>;
    /// Which worker answers now, if one runs: another after a restart.
    fn answering(&self) -> Option<u32>;
}

/// The local transport: the supervisor, its worker process, and the frames on its
/// standard streams.
impl Transport for Worker {
    fn call(&mut self, ask: Ask) -> Result<Called, Box<Failed>> {
        Worker::call(self, ask)
    }

    fn answering(&self) -> Option<u32> {
        self.pid()
    }
}

// --- what the cases are made of -----------------------------------------------------------------

/// 8 by 6: red 255 in the left half and 0 in the right, so the phrase of `fake-ok` finds
/// the left half; the other samples vary.
pub(super) fn rgba() -> Vec<u8> {
    let mut rgba = Vec::new();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let red = if x < WIDTH / 2 { 255 } else { 0 };
            rgba.extend([red, (x * 30) as u8, (y * 40) as u8, 255]);
        }
    }
    rgba
}

pub(super) fn picture() -> Picture {
    Picture::new(CanonicalPixels::from_rgba(WIDTH, HEIGHT, rgba()).unwrap()).unwrap()
}

pub(super) fn model(name: &str) -> ModelName {
    ModelName::new(name, &Secrets::none()).unwrap()
}

pub(super) fn installation() -> InstallationId {
    let text = Digest::of_bytes(b"an installation of a fake").to_string();
    InstallationId::from_text(&text, &Secrets::none()).unwrap()
}

/// A `load` of this fake model, with no settings.
pub(super) fn load(name: &str) -> Ask {
    Ask::Load {
        model: model(name),
        installation: installation(),
        settings: Default::default(),
    }
}

/// The request of the core's, and the `segment` that carries it to this fake model.
pub(super) fn segment(name: &str, queries: Vec<Query>, most: u32) -> (SegmentRequest, Ask) {
    let request = SegmentRequest::new(picture(), queries, most).unwrap();
    let ask = Ask::segment(&request, model(name), installation());
    (request, ask)
}

/// A phrase, a point and a box.
pub(super) fn three_queries() -> Vec<Query> {
    vec![
        Query::phrase("a red thing").unwrap(),
        Query::point(5.5, 2.25).unwrap(),
        Query::boxed(1.0, 1.0, 3.0, 4.0).unwrap(),
    ]
}

/// A plane of the picture's size with the pixels `on` says on.
fn plane_where(on: impl Fn(u32, u32) -> bool) -> Vec<u8> {
    let mut samples = Vec::new();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            samples.push(if on(x, y) { 255 } else { 0 });
        }
    }
    samples
}

/// Loads `name` and asks it to segment, over this transport: the answer to the segment.
fn loaded_then(
    transport: &mut dyn Transport,
    name: &str,
    queries: Vec<Query>,
    most: u32,
) -> (SegmentRequest, Result<Called, Box<Failed>>) {
    let loaded = transport.call(load(name));
    assert!(loaded.is_ok(), "{loaded:?}");
    let (request, ask) = segment(name, queries, most);
    (request, transport.call(ask))
}

// --- the table ------------------------------------------------------------------------------------

/// What produces a case: a fake model, a fake fault of the worker's own, or both.
#[derive(Debug, Clone, Copy)]
enum By {
    Model(&'static str),
    Fault(&'static str),
}

/// One behavioural case: what it shows, what produces it, the task that shows it, and
/// the test that does.
#[derive(Debug)]
struct Case {
    shows: &'static str,
    by: &'static [By],
    task: &'static str,
    test: &'static str,
}

const TABLE: &[Case] = &[
    Case {
        shows: "a result names the device of the model that answered it, not the hello's",
        by: &[By::Model("fake-mps")],
        task: "P7a",
        test: "conformance::a_result_names_the_device_of_the_model_that_answered_it_and_the_hello_the_workers_own",
    },
    Case {
        shows: "a segment request is answered with the masks the request determines",
        by: &[By::Model("fake-ok")],
        task: "P3b",
        test: "conformance::the_real_pack_with_a_fake_model_answers_a_segment_request_whose_masks_are_those_the_request_determines",
    },
    Case {
        shows: "a result over its request's bound is refused before its planes are read, as too_large",
        by: &[By::Model("fake-big")],
        task: "P3b",
        test: "conformance::a_result_over_the_bound_its_request_gives_is_refused_before_it_is_built_and_the_call_ends_as_too_large",
    },
    Case {
        shows: "a payload whose length disagrees with its entry ends the call as malformed, and the next call starts another worker",
        by: &[By::Model("fake-bad")],
        task: "P3b",
        test: "conformance::a_malformed_payload_ends_the_call_as_malformed_and_the_worker_is_restarted",
    },
    Case {
        shows: "a worker that ends inside a call ends it as unavailable and unresolved",
        by: &[By::Model("fake-exit")],
        task: "P3b",
        test: "worker::a_request_that_could_not_be_written_is_not_sent_and_one_written_and_unanswered_is_unresolved",
    },
    Case {
        shows: "a fault made from a worker's failure quotes nothing of its error output, which is handed on whole",
        by: &[By::Fault("error-output"), By::Model("fake-exit")],
        task: "P3b",
        test: "worker::a_fault_made_from_a_workers_failure_holds_none_of_the_marker_the_host_and_the_token_its_error_output_held",
    },
    Case {
        shows: "a request from the worker is answered unsupported before a byte of its payload is read",
        by: &[By::Fault("asks")],
        task: "P3b",
        test: "worker::a_request_from_the_worker_is_refused_before_its_payload_is_read_and_answered_as_unsupported",
    },
    Case {
        shows: "a refusal whizbang sends is a text the worker's reader takes",
        by: &[By::Fault("other-version")],
        task: "P3b",
        test: "worker::a_text_whizbang_sends_is_cleaned_as_a_text_of_the_protocol_and_the_worker_takes_it",
    },
    Case {
        shows: "a request the worker does not read ends at the call's limit as timed_out",
        by: &[By::Fault("deaf-after-hello")],
        task: "P3b",
        test: "worker::a_request_the_worker_does_not_read_ends_at_the_calls_limit_as_timed_out_and_leaves_nothing_behind",
    },
    Case {
        shows: "a word whizbang writes behind a full input waits no longer than its limit",
        by: &[By::Fault("garbage-after-hello")],
        task: "P3b",
        test: "worker::a_word_to_a_worker_whose_input_is_full_waits_no_longer_than_its_limit",
    },
    Case {
        shows: "a worker of another version is refused at its hello",
        by: &[By::Fault("other-version")],
        task: "P3a",
        test: "worker::a_worker_of_another_protocol_version_is_refused_at_the_hello_and_is_not_left_running",
    },
    Case {
        shows: "a worker that says no hello is killed at its limit",
        by: &[By::Fault("no-hello")],
        task: "P3a",
        test: "worker::a_worker_that_says_no_hello_within_its_limit_is_killed_and_the_request_ends_as_unavailable",
    },
    Case {
        shows: "a worker that does not end at the end of its input is killed at the deadline",
        by: &[By::Fault("deaf-to-end-of-input")],
        task: "P3a",
        test: "worker::a_worker_that_does_not_end_at_the_end_of_its_input_is_killed_at_the_deadline_and_waited_for",
    },
    Case {
        shows: "a process the worker started keeps neither stop nor drop waiting",
        by: &[By::Fault("grandchild")],
        task: "P3a",
        test: "worker::a_process_the_worker_started_that_holds_its_streams_open_keeps_neither_stop_nor_drop_waiting",
    },
    Case {
        shows: "a cancelled call that stops ends as cancelled",
        by: &[By::Model("fake-wait")],
        task: "P4",
        test: "queue::a_cancelled_call_that_stops_ends_as_cancelled_and_the_worker_lives_on",
    },
    Case {
        shows: "a cancelled call that does not stop has its worker restarted",
        by: &[By::Model("fake-deaf")],
        task: "P4",
        test: "queue::a_cancelled_call_that_has_not_stopped_at_its_deadline_has_its_worker_restarted",
    },
    Case {
        shows: "a worker that floods events has its call ended as malformed",
        by: &[By::Fault("flood")],
        task: "P4",
        test: "queue::a_worker_that_floods_events_has_its_call_ended_as_malformed_and_is_restarted",
    },
    Case {
        shows: "a load or an unload past its limit has its worker killed",
        by: &[By::Fault("slow-load"), By::Fault("slow-unload")],
        task: "P4",
        test: "queue::a_load_or_an_unload_that_does_not_end_within_its_limit_has_its_worker_killed",
    },
    Case {
        shows: "a worker that exits inside a call fails what was in flight, and the next request starts another",
        by: &[By::Model("fake-exit")],
        task: "P4",
        test: "queue::a_worker_that_exits_inside_a_call_fails_what_was_in_flight_and_the_next_request_starts_another",
    },
];

/// The names a dictionary of the pack's source binds to its classes, from its line
/// `<name> = {` to its closing `}`: the fake models (`MODELS`) or the fake faults
/// (`FAULTS`).
fn python_names(file: &str, dictionary: &str) -> BTreeSet<String> {
    let path = Path::new(&kit::source()).join("whizbang_pack").join(file);
    let text = std::fs::read_to_string(&path).unwrap();
    let start = format!("{dictionary} = {{");
    let mut lines = text.lines().skip_while(|line| *line != start);
    assert!(lines.next().is_some(), "{file} has no {dictionary}");
    let mut names = BTreeSet::new();
    for line in lines.take(64) {
        if line == "}" {
            return names;
        }
        let name = line.trim().split(':').next().unwrap().trim_matches('"');
        names.insert(name.to_owned());
    }
    panic!("{dictionary} of {file} does not end within 64 lines");
}

/// The text of every test file of the pack's tests here, and the plan of 1C.
fn texts() -> (String, String) {
    let here = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/pack"));
    let mut tests = String::new();
    for name in ["conformance.rs", "worker.rs", "proto.rs", "queue.rs"] {
        tests.push_str(&std::fs::read_to_string(here.join(name)).unwrap());
    }
    let plan = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/plans/m1c-local-pack.md"
    ));
    (tests, std::fs::read_to_string(plan).unwrap())
}

#[test]
fn every_case_of_the_table_names_the_fake_model_or_the_fake_fault_that_produces_it() {
    let models = python_names("fake_models.py", "MODELS");
    let faults = python_names("fakes.py", "FAULTS");
    // D3's six, and `fake-mps` of task P7a.
    assert_eq!(models.len(), 7, "{models:?}");
    let (tests, plan) = texts();
    let mut named_models = BTreeSet::new();
    let mut named_faults = BTreeSet::new();
    let mut shown = BTreeSet::new();
    for case in TABLE {
        // Each case is one thing shown, once.
        assert!(shown.insert(case.shows), "{case:?} twice");
        // Something of the pack produces it, by a name the pack has.
        assert!(!case.by.is_empty(), "{case:?}");
        for by in case.by {
            match by {
                By::Model(name) => {
                    assert!(models.contains(*name), "{name} is no fake model");
                    named_models.insert(*name);
                }
                By::Fault(name) => {
                    assert!(faults.contains(*name), "{name} is no fake fault");
                    named_faults.insert(*name);
                }
            }
        }
        // The plan names its test; a test of a task that is built is here.
        assert!(
            plan.contains(case.test),
            "the plan does not name {}",
            case.test
        );
        let (module, name) = case.test.split_once("::").unwrap();
        assert!(
            ["conformance", "worker", "queue"].contains(&module),
            "{case:?}"
        );
        // Every task of the table is built: each case's test is here, and those of P4 are
        // in the queue's tests.
        assert!(
            ["P3a", "P3b", "P4", "P7a"].contains(&case.task),
            "a case of the task {}",
            case.task
        );
        if case.task == "P4" {
            assert_eq!(module, "queue", "{case:?}");
        }
        assert!(
            tests.contains(&format!("fn {name}()")),
            "{} is not a test here",
            case.test
        );
    }
    // And every fake of the pack produces a case of the table.
    let models: BTreeSet<&str> = models.iter().map(String::as_str).collect();
    let faults: BTreeSet<&str> = faults.iter().map(String::as_str).collect();
    assert_eq!(named_models, models, "a fake model is in no case");
    assert_eq!(named_faults, faults, "a fake fault is in no case");
}

// --- the cases this task shows ----------------------------------------------------------------

#[test]
fn a_result_names_the_device_of_the_model_that_answered_it_and_the_hello_the_workers_own() {
    // Task P7a: the hello names the device the worker's work runs on while no model is
    // resident, `cpu`; a result names the device of the model that answered it, which
    // `fake-mps` says is `mps`, as a model its runtime placed on the Apple GPU would.
    let mut worker = supervised(None, system());
    worker.ready().unwrap();
    assert_eq!(worker.said().unwrap().device.as_deref(), Some("cpu"));
    let transport: &mut dyn Transport = &mut *worker;
    let (_, answered) = loaded_then(transport, "fake-mps", three_queries(), 2);
    assert_eq!(answered.unwrap().device, "mps");
}

#[test]
fn the_real_pack_with_a_fake_model_answers_a_segment_request_whose_masks_are_those_the_request_determines()
 {
    let mut worker = supervised(None, system());
    let transport: &mut dyn Transport = &mut *worker;
    let (request, answered) = loaded_then(transport, "fake-ok", three_queries(), 2);
    let called = answered.unwrap();
    assert_eq!(called.device, "cpu");
    assert!(called.threads >= 1, "{called:?}");
    let Answer::Segment(found) = &called.answer else {
        panic!("{:?}", called.answer);
    };

    // What the request determines, query by query (`fake_models.determined`): the phrase,
    // every pixel whose red is 128 or more; the point, its own pixel; the box, every pixel
    // whose centre is inside it. One instance each, and in the fixed order: best score
    // first.
    let phrase = plane_where(|x, _| x < WIDTH / 2);
    let point = plane_where(|x, y| (x, y) == (5, 2));
    let boxed = plane_where(|x, y| (1..3).contains(&x) && (1..4).contains(&y));
    // Each plane is the picture's size, under the binding its request names.
    let instance = |query, score, iou, bounds, samples| Found {
        query,
        score,
        iou,
        bounds,
        plane: Raster {
            binding: request.binding(),
            width: WIDTH,
            height: HEIGHT,
            samples,
        },
    };
    let expected = vec![
        instance(2, 0.9, Some(0.75), [1.0, 1.0, 3.0, 4.0], boxed),
        instance(1, 0.8, Some(0.5), [5.0, 2.0, 6.0, 3.0], point),
        instance(0, 0.7, None, [0.0, 0.0, 4.0, 6.0], phrase),
    ];
    assert_eq!(found, &expected);

    // The core takes each as an instance, query by query.
    let instances = proto::answers(found.clone(), request.queries().len()).unwrap();
    assert_eq!(
        instances.iter().map(Vec::len).collect::<Vec<_>>(),
        [1, 1, 1]
    );
    assert_eq!(instances[0][0].score(), 0.7);
    // The worker said nothing beside its answer.
    assert_eq!(called.diagnostics, "");
    assert!(called.events.is_empty());
}

/// Input that holds these bytes, and fails a read of anything past the first `fence` of
/// them: what a reader that reads too far is told.
struct Fenced {
    bytes: Cursor<Vec<u8>>,
    fence: u64,
}

impl Read for Fenced {
    fn read(&mut self, into: &mut [u8]) -> std::io::Result<usize> {
        let left = self.fence.saturating_sub(self.bytes.position()) as usize;
        if left == 0 {
            return Err(std::io::Error::other("read past the fence"));
        }
        let most = into.len().min(left);
        self.bytes.read(&mut into[..most])
    }
}

#[test]
fn a_result_over_the_bound_its_request_gives_is_refused_before_it_is_built_and_the_call_ends_as_too_large()
 {
    // Over the transport: `fake-big` answers one instance more than its request allows
    // for a query. The call ends as too_large, and the worker with it.
    let mut worker = supervised(None, system());
    let phrase = vec![Query::phrase("a red thing").unwrap()];
    let (_, answered) = loaded_then(&mut *worker, "fake-big", phrase.clone(), 1);
    let failed = answered.unwrap_err();
    assert_eq!(failed.fault.kind(), FaultKind::TooLarge, "{failed:?}");
    assert_eq!(failed.fault.stage(), Stage::Unresolved);
    assert!(failed.ended.is_some(), "the worker was ended");
    assert_eq!(worker.pid(), None);

    // Before it is built: the same frame, read under the same request, is refused by its
    // header, and not one byte of a plane is read. Each plane is 48 bytes, and a read
    // past the header and the first payload's length fails.
    let (request, ask) = segment("fake-big", phrase, 1);
    let found: Vec<Found> = (0..2)
        .map(|_| Found {
            query: 0,
            score: 0.5,
            iou: None,
            bounds: [0.0, 0.0, f64::from(WIDTH), f64::from(HEIGHT)],
            plane: Raster {
                binding: request.binding(),
                width: WIDTH,
                height: HEIGHT,
                samples: vec![255; (WIDTH * HEIGHT) as usize],
            },
        })
        .collect();
    let result = Message::Result {
        id: "r2".to_owned(),
        device: "cpu".to_owned(),
        threads: 3,
        diagnostics: String::new(),
        answer: Answer::Segment(found),
    };
    let mut bytes = Vec::new();
    proto::write(&result, &mut bytes).unwrap();
    let header = 4 + u64::from(u32::from_be_bytes(bytes[..4].try_into().unwrap()));
    let mut open = Open::new();
    open.expect("r2", Awaited::of(&ask)).unwrap();
    let mut input = Fenced {
        bytes: Cursor::new(bytes),
        fence: header,
    };
    let refused = proto::hear(&mut input, &mut open).unwrap_err();
    assert_eq!(refused.reason(), Reason::OverBound, "{refused:?}");
    assert!(refused.counts_instances());
    assert_eq!(
        input.bytes.position(),
        header,
        "nothing past the header was read"
    );
}

#[test]
fn a_malformed_payload_ends_the_call_as_malformed_and_the_worker_is_restarted() {
    let mut worker = supervised(None, system());
    let transport: &mut dyn Transport = &mut *worker;
    let phrase = vec![Query::phrase("a red thing").unwrap()];
    assert!(transport.call(load("fake-bad")).is_ok());
    let first = transport.answering();
    assert!(first.is_some());
    let (_, ask) = segment("fake-bad", phrase.clone(), 1);
    let failed = transport.call(ask).unwrap_err();
    assert_eq!(transport.answering(), None, "the worker was ended");
    assert_eq!(failed.fault.kind(), FaultKind::Malformed, "{failed:?}");
    assert_eq!(failed.fault.stage(), Stage::Unresolved);
    // It ended by itself, and was not killed: at the error that named the conversation,
    // or at the end of its input (exit code 0); or, when the reader of its output had
    // already stopped at the refused frame and let the pipe go while the worker still
    // wrote the rest of it, at that write (exit code 2), which the Linux runner showed
    // (task P5b).
    assert!(
        matches!(failed.ended, Some(Ended::Exited(0) | Ended::Exited(2))),
        "{failed:?}"
    );

    // The next call starts another worker, which answers.
    let (_, answered) = loaded_then(transport, "fake-ok", phrase, 1);
    assert!(answered.is_ok(), "{answered:?}");
    assert!(transport.answering().is_some());
    assert_ne!(transport.answering(), first);
    assert_eq!(kit::pack_children().len(), 1);
}
