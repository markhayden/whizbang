//! The supervisor's queue (task P4): one call in the worker at a time, in the order the
//! requests came; a bounded queue; a request for another model that waits and then
//! unloads the first; a cancel, a deadline and each kind's limit, on the injected clock;
//! and a restart that ends the current call and every queued request, sends nothing
//! again, and is not made in a loop. Task P5b adds, from the review of P4: a cancel and
//! then an earlier deadline, what a fault that ended the worker says comes next, and the
//! guard that ends a call in the worker without waiting for it.
//!
//! Every test here starts the pack through the test kit's guard
//! (`super::worker::queued`), and is of the group `pack-workers`. The clock is the
//! machine's, moved on by the test ([`Stepped`]), or one that stands still until the test
//! moves it ([`Still`]): a wait of the supervisor's ends when the test moves the clock
//! past it, never by a sleep of the test's. What a test waits for is a state of the
//! supervisor ([`Supervisor::wait_for`]), which it is told of.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use whizbang_backends::pack::queue::{QUEUED_MOST, Request, Status, Supervisor};
use whizbang_backends::pack::worker::{
    CALL_LIMIT, CANCEL_LIMIT, Called, Failed, LOAD_LIMIT, Limits, RESTART_PAUSE,
    STARTS_FAILED_MOST, UNLOAD_LIMIT,
};
use whizbang_backends::pack::{Ended, Launch};
use whizbang_core::capability::{FaultKind, Query, Stage};
use whizbang_core::contract::{Cancellation, Clock, SystemClock};
use whizbang_testkit::threads;
use whizbang_testkit::worker as kit;

use super::conformance::{installation, model, segment};
use super::worker::{queued, queued_on_a_card, queued_over};

/// How long a test waits, on the clock, for a state of the supervisor.
const WITHIN: Duration = Duration::from_secs(20);

/// The machine's clock, moved on by the test: [`Stepped::step`] moves it at once; a pause
/// waits as the machine's does.
#[derive(Default)]
struct Stepped {
    moved_ms: AtomicU64,
}

impl Stepped {
    fn moved(&self) -> Duration {
        Duration::from_millis(self.moved_ms.load(Ordering::SeqCst))
    }

    fn step(&self, by: Duration) {
        self.moved_ms
            .fetch_add(by.as_millis() as u64, Ordering::SeqCst);
    }
}

impl Clock for Stepped {
    fn now_ms(&self) -> u64 {
        SystemClock.now_ms() + self.moved_ms.load(Ordering::SeqCst)
    }

    fn now(&self) -> Instant {
        Instant::now() + self.moved()
    }

    fn pause(&self, span: Duration) {
        SystemClock.pause(span);
    }
}

/// A clock that stands still until the test moves it ([`Still::step`]); its pause waits
/// as the machine's does, so that the supervisor's own waits are not rushed through.
/// What is held to a time of it happens when the test says, and not before.
pub(super) struct Still {
    start: Instant,
    moved: Mutex<Duration>,
}

impl Still {
    pub(super) fn new() -> Still {
        Still {
            start: Instant::now(),
            moved: Mutex::new(Duration::ZERO),
        }
    }

    pub(super) fn step(&self, by: Duration) {
        *self.moved.lock().unwrap() += by;
    }
}

impl Clock for Still {
    fn now_ms(&self) -> u64 {
        SystemClock.now_ms()
    }

    fn now(&self) -> Instant {
        self.start + *self.moved.lock().unwrap()
    }

    fn pause(&self, span: Duration) {
        SystemClock.pause(span);
    }
}

type Answer = Result<Called, Box<Failed>>;

/// A request for this fake model: its load, with no settings, and a segment of one phrase.
fn request(name: &str) -> Request {
    let (_, ask) = segment(name, vec![Query::phrase("a red thing").unwrap()], 1);
    Request {
        model: model(name),
        installation: installation(),
        settings: Default::default(),
        ask,
        removals: 0,
    }
}

/// Limits of a signal the test holds, and no deadline.
fn cancelled_by(signal: &Cancellation) -> Limits {
    Limits {
        cancellation: signal.clone(),
        deadline: None,
        ..Limits::none()
    }
}

/// A request asked on a thread of the test's own.
fn asked(supervisor: &Arc<Supervisor>, request: Request, limits: Limits) -> JoinHandle<Answer> {
    let supervisor = supervisor.clone();
    threads::spawn(move || supervisor.call(request, &limits))
}

/// Waits, within [`WITHIN`] of the clock, for the supervisor to be in a state.
fn seen(supervisor: &Supervisor, state: impl Fn(&Status) -> bool) {
    assert!(
        supervisor.wait_for(WITHIN, &state),
        "{:?}",
        supervisor.status()
    );
}

/// Whether a call of this kind is in the worker and has said this many events.
fn calling(status: &Status, kind: &str, events: u32) -> bool {
    status
        .calling
        .as_ref()
        .is_some_and(|calling| calling.kind == kind && calling.events >= events)
}

/// What the supervisor wrote to a worker, in order: the request's number, the kind and
/// the model.
fn sent(supervisor: &Supervisor) -> Vec<(u64, &'static str, String)> {
    supervisor
        .journal()
        .into_iter()
        .map(|sent| (sent.number, sent.kind, sent.model.as_str().to_owned()))
        .collect()
}

/// The process ids of the workers the supervisor wrote to, in order, each once.
fn workers_written_to(supervisor: &Supervisor) -> Vec<u32> {
    let mut workers: Vec<u32> = Vec::new();
    for sent in supervisor.journal() {
        if workers.last() != Some(&sent.worker) {
            workers.push(sent.worker);
        }
    }
    workers
}

fn failed(answer: Answer) -> Box<Failed> {
    match answer {
        Ok(called) => panic!("answered: {called:?}"),
        Err(failed) => failed,
    }
}

#[test]
fn the_seventeenth_waiting_request_is_refused_as_unavailable_and_not_held() {
    let clock = Arc::new(Stepped::default());
    let supervisor = queued(None, clock, |_| {});
    // One call holds the worker until it is cancelled.
    let held = Cancellation::new();
    let first = asked(&supervisor, request("fake-wait"), cancelled_by(&held));
    seen(&supervisor, |status| calling(status, "segment", 1));

    // Sixteen wait behind it, each seen in the queue before the next is asked.
    let leave = Cancellation::new();
    let mut waiting = Vec::new();
    for count in 1..=QUEUED_MOST {
        waiting.push(asked(&supervisor, request("fake-ok"), cancelled_by(&leave)));
        seen(&supervisor, |status| status.waiting.len() == count);
    }

    // The seventeenth is refused at once, on the caller's thread, and is not held.
    let refused = failed(supervisor.call(request("fake-ok"), &Limits::none()));
    assert_eq!(refused.fault.kind(), FaultKind::Unavailable, "{refused:?}");
    assert_eq!(refused.fault.stage(), Stage::NotSent);
    assert_eq!(
        refused.fault.sentence(),
        "the queue of the pack's worker is full, 16 requests wait; this one was not held"
    );
    let status = supervisor.status();
    assert_eq!(status.waiting.len(), QUEUED_MOST);
    let numbers: Vec<u64> = status
        .waiting
        .iter()
        .map(|waiting| waiting.number)
        .collect();
    assert_eq!(numbers, (2..=17).collect::<Vec<u64>>());

    // Those that wait leave when they are cancelled, never sent; then the first.
    leave.cancel();
    for waiter in waiting {
        let left = failed(waiter.join().unwrap());
        assert_eq!(left.fault.kind(), FaultKind::Cancelled, "{left:?}");
        assert_eq!(left.fault.stage(), Stage::NotSent);
    }
    held.cancel();
    let first = failed(first.join().unwrap());
    assert_eq!(first.fault.kind(), FaultKind::Cancelled);
    assert_eq!(
        sent(&supervisor),
        [
            (1, "load", "fake-wait".to_owned()),
            (1, "segment", "fake-wait".to_owned())
        ]
    );
}

#[test]
fn requests_run_one_at_a_time_in_the_order_they_came() {
    let clock = Arc::new(Stepped::default());
    let supervisor = queued(None, clock, |_| {});
    let held = Cancellation::new();
    let first = asked(&supervisor, request("fake-wait"), cancelled_by(&held));
    seen(&supervisor, |status| calling(status, "segment", 1));
    // Three more, each in the queue before the next is asked.
    let mut after = Vec::new();
    for count in 1..=3 {
        after.push(asked(&supervisor, request("fake-ok"), Limits::none()));
        seen(&supervisor, |status| status.waiting.len() == count);
    }
    held.cancel();
    assert!(first.join().unwrap().is_err());
    for answer in after {
        let called = answer.join().unwrap();
        assert!(called.is_ok(), "{called:?}");
    }

    // One worker, one call at a time: each was written only once the one before it had
    // ended (a worker asked a second call while one runs answers `unavailable`, and none
    // did), in the order the requests came.
    let ok = || "fake-ok".to_owned();
    assert_eq!(
        sent(&supervisor),
        [
            (1, "load", "fake-wait".to_owned()),
            (1, "segment", "fake-wait".to_owned()),
            (2, "unload", "fake-wait".to_owned()),
            (2, "load", ok()),
            (2, "segment", ok()),
            (3, "segment", ok()),
            (4, "segment", ok()),
        ]
    );
    assert_eq!(workers_written_to(&supervisor).len(), 1);
    let status = supervisor.status();
    assert!(
        status.waiting.is_empty() && status.calling.is_none(),
        "{status:?}"
    );
}

#[test]
fn a_request_for_another_model_waits_for_the_current_call_and_the_first_model_is_then_unloaded() {
    let clock = Arc::new(Stepped::default());
    let supervisor = queued(None, clock, |_| {});
    let held = Cancellation::new();
    let first = asked(&supervisor, request("fake-wait"), cancelled_by(&held));
    seen(&supervisor, |status| calling(status, "segment", 1));
    let other = asked(&supervisor, request("fake-ok"), Limits::none());
    seen(&supervisor, |status| status.waiting.len() == 1);

    // It waits: the first model is resident and in its call, and nothing was unloaded.
    let status = supervisor.status();
    assert_eq!(
        status.resident.as_ref().map(|name| name.as_str()),
        Some("fake-wait")
    );
    assert_eq!(sent(&supervisor).len(), 2);

    // Once the current call ends, the first model is unloaded, and the other loaded.
    held.cancel();
    assert!(first.join().unwrap().is_err());
    assert!(other.join().unwrap().is_ok());
    assert_eq!(
        sent(&supervisor)[2..],
        [
            (2, "unload", "fake-wait".to_owned()),
            (2, "load", "fake-ok".to_owned()),
            (2, "segment", "fake-ok".to_owned()),
        ]
    );
    let resident = supervisor.status().resident;
    assert_eq!(resident.as_ref().map(|name| name.as_str()), Some("fake-ok"));
}

#[test]
fn two_requests_for_one_model_cause_one_load() {
    let clock = Arc::new(Stepped::default());
    let supervisor = queued(None, clock, |_| {});
    assert!(supervisor.call(request("fake-ok"), &Limits::none()).is_ok());
    assert!(supervisor.call(request("fake-ok"), &Limits::none()).is_ok());
    let ok = || "fake-ok".to_owned();
    assert_eq!(
        sent(&supervisor),
        [
            (1, "load", ok()),
            (1, "segment", ok()),
            (2, "segment", ok())
        ]
    );

    // Its settings are part of what is loaded: other settings unload it and load it
    // again.
    let mut changed = request("fake-ok");
    changed.settings.insert(
        "threshold".to_owned(),
        whizbang_backends::pack::proto::Scalar::Whole(3),
    );
    assert!(supervisor.call(changed, &Limits::none()).is_ok());
    assert_eq!(
        sent(&supervisor)[3..],
        [(3, "unload", ok()), (3, "load", ok()), (3, "segment", ok())]
    );
}

#[test]
fn a_cancelled_call_that_stops_ends_as_cancelled_and_the_worker_lives_on() {
    let clock = Arc::new(Stepped::default());
    let supervisor = queued(None, clock, |_| {});
    let signal = Cancellation::new();
    let call = asked(&supervisor, request("fake-wait"), cancelled_by(&signal));
    seen(&supervisor, |status| calling(status, "segment", 1));
    let worker = supervisor.status().worker;
    assert!(worker.is_some());

    signal.cancel();
    let stopped = failed(call.join().unwrap());
    // The worker was sent `cancel`, stopped the call, and answered it so.
    assert_eq!(stopped.fault.kind(), FaultKind::Cancelled, "{stopped:?}");
    assert_eq!(stopped.fault.stage(), Stage::Answered);
    assert_eq!(stopped.said.as_deref(), Some("the call was cancelled"));
    assert_eq!(stopped.ended, None);

    // It lives on, and answers the next request.
    assert_eq!(supervisor.status().worker, worker);
    assert!(supervisor.call(request("fake-ok"), &Limits::none()).is_ok());
    assert_eq!(workers_written_to(&supervisor), [worker.unwrap()]);
    assert_eq!(kit::pack_children(), [worker.unwrap()]);
}

#[test]
fn a_cancelled_call_that_has_not_stopped_at_its_deadline_has_its_worker_restarted() {
    let clock = Arc::new(Stepped::default());
    let supervisor = queued(None, clock.clone(), |_| {});
    let signal = Cancellation::new();
    let call = asked(&supervisor, request("fake-deaf"), cancelled_by(&signal));
    seen(&supervisor, |status| calling(status, "segment", 1));
    let first = supervisor.status().worker.unwrap();

    // The cancel is sent, and the worker ignores it: five seconds after it, on the
    // clock, the worker is killed and waited for.
    signal.cancel();
    seen(&supervisor, |status| {
        status
            .calling
            .as_ref()
            .is_some_and(|calling| calling.cancel_sent)
    });
    clock.step(CANCEL_LIMIT);
    let killed = failed(call.join().unwrap());
    assert_eq!(killed.fault.kind(), FaultKind::Cancelled, "{killed:?}");
    assert_eq!(killed.fault.stage(), Stage::Unresolved);
    assert_eq!(killed.ended, Some(Ended::Killed));
    assert_eq!(
        killed.fault.sentence(),
        "the pack's worker did not stop the cancelled request within 5 seconds of its cancel and was killed; the pack's worker is restarted for the next request"
    );
    assert!(!kit::runs_the_pack(first));
    assert_eq!(supervisor.status().worker, None);

    // The next request starts another.
    assert!(supervisor.call(request("fake-ok"), &Limits::none()).is_ok());
    let second = supervisor.status().worker.unwrap();
    assert_ne!(second, first);
    assert_eq!(workers_written_to(&supervisor), [first, second]);
    assert_eq!(kit::pack_children(), [second]);
}

#[test]
fn a_call_past_its_deadline_ends_as_timed_out_and_its_worker_is_restarted() {
    let clock = Arc::new(Stepped::default());
    let supervisor = queued(None, clock.clone(), |_| {});
    let limits = Limits {
        cancellation: Cancellation::new(),
        deadline: Some(clock.now() + Duration::from_secs(10)),
        ..Limits::none()
    };
    let call = asked(&supervisor, request("fake-deaf"), limits);
    seen(&supervisor, |status| calling(status, "segment", 1));
    let first = supervisor.status().worker.unwrap();

    // The deadline passes on the clock: the call ends there, at the worker's boundary,
    // and the worker is killed.
    clock.step(Duration::from_secs(10));
    let late = failed(call.join().unwrap());
    assert_eq!(late.fault.kind(), FaultKind::TimedOut, "{late:?}");
    assert_eq!(late.fault.stage(), Stage::Unresolved);
    assert_eq!(late.ended, Some(Ended::Killed));
    assert_eq!(
        late.fault.sentence(),
        "the request's deadline passed before the pack's worker ended it, and the worker was killed; the pack's worker is restarted for the next request"
    );
    assert!(!kit::runs_the_pack(first));

    // A request whose deadline has passed before its turn is not sent at all.
    let limits = Limits {
        cancellation: Cancellation::new(),
        deadline: Some(clock.now()),
        ..Limits::none()
    };
    let unsent = failed(supervisor.call(request("fake-ok"), &limits));
    assert_eq!(unsent.fault.kind(), FaultKind::TimedOut);
    assert_eq!(unsent.fault.stage(), Stage::NotSent);
    assert_eq!(
        kit::pack_children(),
        Vec::<u32>::new(),
        "nothing was started for it"
    );

    // The next request starts another worker.
    assert!(supervisor.call(request("fake-ok"), &Limits::none()).is_ok());
    let second = supervisor.status().worker.unwrap();
    assert_ne!(second, first);
    assert_eq!(kit::pack_children(), [second]);
}

#[test]
fn a_call_with_no_deadline_is_cut_off_at_the_kinds_limit_and_a_cancelled_one_five_seconds_after_its_cancel()
 {
    let clock = Arc::new(Stepped::default());
    let supervisor = queued(None, clock.clone(), |_| {});

    // No deadline: the kind's limit, 120 seconds of the clock, ends it.
    let call = asked(&supervisor, request("fake-deaf"), Limits::none());
    seen(&supervisor, |status| calling(status, "segment", 1));
    let begun = clock.moved();
    clock.step(CALL_LIMIT);
    let cut = failed(call.join().unwrap());
    assert_eq!(cut.fault.kind(), FaultKind::TimedOut, "{cut:?}");
    assert_eq!(cut.fault.stage(), Stage::Unresolved);
    assert_eq!(cut.ended, Some(Ended::Killed));
    assert!(
        cut.fault.sentence().starts_with(
            "the pack's worker did not end the request within 120 seconds and was killed"
        ),
        "{}",
        cut.fault.sentence()
    );
    assert_eq!(clock.moved() - begun, CALL_LIMIT);

    // No deadline, and cancelled: five seconds after its cancel.
    let signal = Cancellation::new();
    let call = asked(&supervisor, request("fake-deaf"), cancelled_by(&signal));
    seen(&supervisor, |status| calling(status, "segment", 1));
    signal.cancel();
    seen(&supervisor, |status| {
        status
            .calling
            .as_ref()
            .is_some_and(|calling| calling.cancel_sent)
    });
    clock.step(CANCEL_LIMIT);
    let cut = failed(call.join().unwrap());
    assert_eq!(cut.fault.kind(), FaultKind::Cancelled, "{cut:?}");
    assert_eq!(cut.ended, Some(Ended::Killed));
    assert_eq!(kit::pack_children(), Vec::<u32>::new());
}

#[test]
fn a_load_or_an_unload_that_does_not_end_within_its_limit_has_its_worker_killed() {
    // A load that never ends: 120 seconds of the clock.
    let clock = Arc::new(Stepped::default());
    let supervisor = queued(Some("slow-load"), clock.clone(), |_| {});
    let call = asked(&supervisor, request("fake-ok"), Limits::none());
    seen(&supervisor, |status| calling(status, "load", 1));
    let worker = supervisor.status().worker.unwrap();
    clock.step(LOAD_LIMIT);
    let slow = failed(call.join().unwrap());
    assert_eq!(slow.fault.kind(), FaultKind::TimedOut, "{slow:?}");
    assert_eq!(slow.ended, Some(Ended::Killed));
    assert!(
        slow.fault
            .sentence()
            .starts_with("the pack's worker did not end the request within 120 seconds"),
        "{}",
        slow.fault.sentence()
    );
    assert!(!kit::runs_the_pack(worker));
    assert_eq!(sent(&supervisor), [(1, "load", "fake-ok".to_owned())]);
    drop(supervisor);

    // An unload that never ends, when another model is asked for: 30 seconds.
    let clock = Arc::new(Stepped::default());
    let supervisor = queued(Some("slow-unload"), clock.clone(), |_| {});
    assert!(supervisor.call(request("fake-ok"), &Limits::none()).is_ok());
    let call = asked(&supervisor, request("fake-wait"), Limits::none());
    seen(&supervisor, |status| calling(status, "unload", 1));
    let worker = supervisor.status().worker.unwrap();
    clock.step(UNLOAD_LIMIT);
    let slow = failed(call.join().unwrap());
    assert_eq!(slow.fault.kind(), FaultKind::TimedOut, "{slow:?}");
    assert_eq!(slow.ended, Some(Ended::Killed));
    assert!(
        slow.fault
            .sentence()
            .starts_with("the pack's worker did not end the request within 30 seconds"),
        "{}",
        slow.fault.sentence()
    );
    assert!(!kit::runs_the_pack(worker));
    // The other model was never loaded.
    assert_eq!(
        sent(&supervisor).last(),
        Some(&(2, "unload", "fake-ok".to_owned()))
    );
    assert_eq!(supervisor.status().resident, None);
}

#[test]
fn a_worker_that_floods_events_has_its_call_ended_as_malformed_and_is_restarted() {
    let clock = Arc::new(Stepped::default());
    let supervisor = queued(Some("flood"), clock, |_| {});
    // It sends 300 events before each call, more than a request may send.
    let flooded = failed(supervisor.call(request("fake-ok"), &Limits::none()));
    assert_eq!(flooded.fault.kind(), FaultKind::Malformed, "{flooded:?}");
    assert_eq!(flooded.fault.stage(), Stage::Unresolved);
    assert!(flooded.ended.is_some());
    assert!(
        flooded
            .fault
            .sentence()
            .ends_with("; the pack's worker is restarted for the next request"),
        "{}",
        flooded.fault.sentence()
    );
    assert_eq!(supervisor.status().worker, None);

    // The next request starts another, which floods again and is ended again.
    let again = failed(supervisor.call(request("fake-ok"), &Limits::none()));
    assert_eq!(again.fault.kind(), FaultKind::Malformed);
    let workers = workers_written_to(&supervisor);
    assert_eq!(workers.len(), 2, "{workers:?}");
    assert_ne!(workers[0], workers[1]);
    assert_eq!(kit::pack_children(), Vec::<u32>::new());
}

#[test]
fn a_restart_ends_the_current_call_and_every_queued_request_each_with_a_fault_that_says_the_worker_was_restarted()
 {
    let clock = Arc::new(Stepped::default());
    let supervisor = queued(None, clock.clone(), |_| {});
    let signal = Cancellation::new();
    let current = asked(&supervisor, request("fake-deaf"), cancelled_by(&signal));
    seen(&supervisor, |status| calling(status, "segment", 1));
    let mut queued_ones = Vec::new();
    for count in 1..=2 {
        queued_ones.push(asked(&supervisor, request("fake-ok"), Limits::none()));
        seen(&supervisor, |status| status.waiting.len() == count);
    }

    // The current call does not stop at its cancel: its worker is killed, which is a
    // restart.
    signal.cancel();
    seen(&supervisor, |status| {
        status
            .calling
            .as_ref()
            .is_some_and(|calling| calling.cancel_sent)
    });
    clock.step(CANCEL_LIMIT);
    let current = failed(current.join().unwrap());
    assert_eq!(current.fault.kind(), FaultKind::Cancelled);
    assert!(
        current.fault.sentence().contains("restarted"),
        "{}",
        current.fault.sentence()
    );
    for waiting in queued_ones {
        let ended = failed(waiting.join().unwrap());
        assert_eq!(ended.fault.kind(), FaultKind::Unavailable, "{ended:?}");
        assert_eq!(ended.fault.stage(), Stage::NotSent);
        assert_eq!(
            ended.fault.sentence(),
            "the pack's worker was restarted before this request's turn came; it was not sent"
        );
        assert_eq!(ended.ended, None);
    }
    // Neither queued request reached a worker, and none is left waiting.
    assert!(sent(&supervisor).iter().all(|(number, ..)| *number == 1));
    let status = supervisor.status();
    assert!(
        status.waiting.is_empty() && status.calling.is_none(),
        "{status:?}"
    );
    assert_eq!(status.worker, None);
    assert_eq!(kit::pack_children(), Vec::<u32>::new());
}

#[test]
fn a_worker_that_exits_inside_a_call_fails_what_was_in_flight_and_the_next_request_starts_another()
{
    let clock = Arc::new(Stepped::default());
    let supervisor = queued(None, clock, |_| {});
    let exited = failed(supervisor.call(request("fake-exit"), &Limits::none()));
    assert_eq!(exited.fault.kind(), FaultKind::Unavailable, "{exited:?}");
    assert_eq!(exited.fault.stage(), Stage::Unresolved);
    assert_eq!(exited.ended, Some(Ended::Exited(4)));
    assert_eq!(
        exited.fault.sentence(),
        "the pack's worker ended inside the call: it ended inside a call (exit code 4); the pack's worker is restarted for the next request"
    );
    let first = workers_written_to(&supervisor);
    assert_eq!(supervisor.status().worker, None);
    assert_eq!(supervisor.status().resident, None);

    // The next request starts another worker, loads its model there, and is answered.
    assert!(supervisor.call(request("fake-ok"), &Limits::none()).is_ok());
    let workers = workers_written_to(&supervisor);
    assert_eq!(workers.len(), 2);
    assert_eq!(workers[0], first[0]);
    assert_eq!(kit::pack_children(), [workers[1]]);
    assert_eq!(
        sent(&supervisor)[2..],
        [
            (2, "load", "fake-ok".to_owned()),
            (2, "segment", "fake-ok".to_owned())
        ]
    );
}

#[test]
fn nothing_that_was_dispatched_is_sent_again_after_a_restart() {
    let clock = Arc::new(Stepped::default());
    let supervisor = queued(None, clock.clone(), |_| {});
    // One request dispatched to a worker that will not stop, and one queued behind it.
    let signal = Cancellation::new();
    let dispatched = asked(&supervisor, request("fake-deaf"), cancelled_by(&signal));
    seen(&supervisor, |status| calling(status, "segment", 1));
    let waiting = asked(&supervisor, request("fake-ok"), Limits::none());
    seen(&supervisor, |status| status.waiting.len() == 1);
    signal.cancel();
    seen(&supervisor, |status| {
        status
            .calling
            .as_ref()
            .is_some_and(|calling| calling.cancel_sent)
    });
    clock.step(CANCEL_LIMIT);
    assert!(dispatched.join().unwrap().is_err());
    assert!(waiting.join().unwrap().is_err());

    // The next request starts a worker, which is sent that request and nothing else.
    assert!(supervisor.call(request("fake-ok"), &Limits::none()).is_ok());
    let deaf = || "fake-deaf".to_owned();
    let ok = || "fake-ok".to_owned();
    assert_eq!(
        sent(&supervisor),
        [
            (1, "load", deaf()),
            (1, "segment", deaf()),
            (3, "load", ok()),
            (3, "segment", ok()),
        ]
    );
    let workers = workers_written_to(&supervisor);
    assert_eq!(workers.len(), 2);
    let on_second: Vec<u64> = supervisor
        .journal()
        .iter()
        .filter(|sent| sent.worker == workers[1])
        .map(|sent| sent.number)
        .collect();
    assert_eq!(on_second, [3, 3]);
}

#[test]
fn a_queued_request_that_is_cancelled_leaves_the_queue_and_is_never_sent() {
    let clock = Arc::new(Stepped::default());
    let (supervisor, root) = queued_over(None, clock, |_| {});
    let held = Cancellation::new();
    let first = asked(&supervisor, request("fake-wait"), cancelled_by(&held));
    seen(&supervisor, |status| calling(status, "segment", 1));

    // Queued, with the count of removals its hold was given, which it keeps (D5): the
    // model's count, which seven removals raised (task P5b reads it).
    let folder = root.join("models/fake-ok");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("REMOVALS"),
        br#"{"format":"removals/1","count":7}"#,
    )
    .unwrap();
    let leave = Cancellation::new();
    let mut counted = request("fake-ok");
    counted.removals = 7;
    let second = asked(&supervisor, counted, cancelled_by(&leave));
    seen(&supervisor, |status| status.waiting.len() == 1);
    let status = supervisor.status();
    assert_eq!(status.waiting[0].number, 2);
    assert_eq!(status.waiting[0].removals, 7);
    assert_eq!(status.waiting[0].model.as_str(), "fake-ok");

    // Cancelled, it leaves the queue while the first call still holds the worker.
    leave.cancel();
    let left = failed(second.join().unwrap());
    assert_eq!(left.fault.kind(), FaultKind::Cancelled, "{left:?}");
    assert_eq!(left.fault.stage(), Stage::NotSent);
    assert_eq!(
        left.fault.sentence(),
        "the request was cancelled while it waited in the queue of the pack's worker; it was not sent"
    );
    let status = supervisor.status();
    assert!(status.waiting.is_empty(), "{status:?}");
    assert!(calling(&status, "segment", 1), "{status:?}");

    held.cancel();
    assert!(first.join().unwrap().is_err());
    assert!(sent(&supervisor).iter().all(|(number, ..)| *number == 1));
}

#[test]
fn a_worker_that_ends_before_it_answers_anything_is_not_started_again_in_a_loop() {
    // A command line the worker refuses: each worker ends at once, before its hello.
    let clock = Arc::new(Stepped::default());
    let supervisor = queued(None, clock.clone(), |launch: &mut Launch| {
        launch.models_dir = "relative/models".into();
    });
    for _ in 0..STARTS_FAILED_MOST {
        let ended = failed(supervisor.call(request("fake-ok"), &Limits::none()));
        assert_eq!(ended.ended, Some(Ended::Exited(2)), "{ended:?}");
        assert_eq!(ended.fault.stage(), Stage::NotSent);
    }

    // Three in a row: the next request is refused at once, and no worker is started.
    let refused = failed(supervisor.call(request("fake-ok"), &Limits::none()));
    assert_eq!(refused.fault.kind(), FaultKind::Unavailable);
    assert_eq!(refused.fault.stage(), Stage::NotSent);
    assert_eq!(refused.ended, None, "no worker was started for it");
    assert_eq!(
        refused.fault.sentence(),
        "the pack's worker ended before it answered anything 3 times in a row; no worker is started until 60 seconds after the last"
    );
    assert_eq!(kit::children(), Vec::new());

    // Once the pause has passed on the clock, one more is started; it ends as the others
    // did, and the pause begins again.
    clock.step(RESTART_PAUSE);
    let ended = failed(supervisor.call(request("fake-ok"), &Limits::none()));
    assert_eq!(ended.ended, Some(Ended::Exited(2)), "{ended:?}");
    let refused = failed(supervisor.call(request("fake-ok"), &Limits::none()));
    assert_eq!(refused.ended, None);
    assert_eq!(refused.fault.kind(), FaultKind::Unavailable);
}

// --- follow-ups of the review of task P4, built with task P5b ---------------------------------

#[test]
fn a_cancelled_call_whose_deadline_comes_before_five_seconds_after_its_cancel_is_killed_at_its_deadline_as_cancelled()
 {
    let clock = Arc::new(Still::new());
    let supervisor = queued(None, clock.clone(), |_| {});
    let signal = Cancellation::new();
    let limits = Limits {
        cancellation: signal.clone(),
        deadline: Some(clock.now() + Duration::from_secs(2)),
        ..Limits::none()
    };
    let call = asked(&supervisor, request("fake-deaf"), limits);
    seen(&supervisor, |status| calling(status, "segment", 1));
    let first = supervisor.status().worker.unwrap();
    // Cancelled; the worker ignores it. Its deadline, two seconds on, comes before five
    // seconds after its cancel: the call ends there, cancelled, and the worker is killed.
    signal.cancel();
    seen(&supervisor, |status| {
        status
            .calling
            .as_ref()
            .is_some_and(|calling| calling.cancel_sent)
    });
    clock.step(Duration::from_secs(2));
    let killed = failed(call.join().unwrap());
    assert_eq!(killed.fault.kind(), FaultKind::Cancelled, "{killed:?}");
    assert_eq!(killed.fault.stage(), Stage::Unresolved);
    assert_eq!(killed.ended, Some(Ended::Killed));
    assert_eq!(
        killed.fault.sentence(),
        "the pack's worker had not stopped the cancelled request by the end of its wait and was killed; the pack's worker is restarted for the next request"
    );
    assert!(!kit::runs_the_pack(first));
}

#[test]
fn a_fault_that_ended_the_worker_at_its_start_says_what_the_next_request_finds() {
    // Each worker ends at once, before its hello: no worker ran, so none is restarted.
    // The next request starts another, until the third in a row; then none is started
    // until the pause has passed.
    let clock = Arc::new(Stepped::default());
    let supervisor = queued(None, clock, |launch: &mut Launch| {
        launch.models_dir = "relative/models".into();
    });
    for count in 1..=STARTS_FAILED_MOST {
        let ended = failed(supervisor.call(request("fake-ok"), &Limits::none()));
        assert_eq!(ended.ended, Some(Ended::Exited(2)), "{ended:?}");
        let said = ended.fault.sentence();
        assert!(
            said.starts_with("the pack's worker ended before its hello: "),
            "{said}"
        );
        let next = if count < STARTS_FAILED_MOST {
            "; it said no hello, and the next request starts another worker"
        } else {
            "; 3 workers in a row ended before they answered anything, and no worker is started until 60 seconds after the last"
        };
        assert!(said.ends_with(next), "{said}");
        assert!(!said.contains("restarted"), "{said}");
    }
}

#[test]
fn the_guard_ends_the_call_in_its_worker_at_once_and_every_waiting_request() {
    let clock = Arc::new(Stepped::default());
    let supervisor = queued(None, clock, |_| {});
    // A call that ignores its cancel holds the worker, and nothing will end it; one more
    // request waits.
    let call = asked(&supervisor, request("fake-deaf"), Limits::none());
    seen(&supervisor, |status| calling(status, "segment", 1));
    let worker = supervisor.status().worker.unwrap();
    let waiting = asked(&supervisor, request("fake-ok"), Limits::none());
    seen(&supervisor, |status| status.waiting.len() == 1);

    // The guard is dropped, as a test that fails drops it: on a thread of its own, so
    // that this one sees whether the drop waits for the call.
    let (done, dropped) = mpsc::channel();
    let dropping = threads::spawn(move || {
        drop(supervisor);
        let _ = done.send(());
    });
    assert!(
        dropped.recv_timeout(Duration::from_secs(10)).is_ok(),
        "the guard waited for the call in its worker"
    );
    dropping.join().unwrap();
    let killed = failed(call.join().unwrap());
    assert_eq!(killed.fault.kind(), FaultKind::Unavailable, "{killed:?}");
    assert_eq!(killed.ended, Some(Ended::Killed), "{killed:?}");
    assert_eq!(
        killed.fault.sentence(),
        "the pack's worker was killed when its supervisor was ended"
    );
    let left = failed(waiting.join().unwrap());
    assert_eq!(left.fault.kind(), FaultKind::Unavailable, "{left:?}");
    assert_eq!(left.fault.stage(), Stage::NotSent);
    assert_eq!(
        left.fault.sentence(),
        "the supervisor of the pack's worker was ended before this request's turn came; it was not sent"
    );
    assert!(!kit::runs_the_pack(worker));
}

/// The card a `load` named goes with each call of the model it made resident, a call that
/// finds the model resident and makes no load among them (plan m7, task B1, item 6; ADR
/// 0048). The worker is the pack's fakes with a model on a card (`fake_card.py`, beside
/// the fake host's side), whose own code names the card as on a GPU host; a model on no
/// card names none.
#[test]
fn the_card_a_load_named_goes_with_each_call_of_the_model_it_made_resident() {
    let clock = Arc::new(Stepped::default());
    let (supervisor, _place) = queued_on_a_card(clock);
    let named = Some("NVIDIA Fake A10G");
    let first = supervisor
        .call(request("fake-cuda"), &Limits::none())
        .unwrap();
    assert_eq!(
        (first.device.as_str(), first.card.as_deref()),
        ("cuda", named)
    );
    let second = supervisor
        .call(request("fake-cuda"), &Limits::none())
        .unwrap();
    assert_eq!(second.card.as_deref(), named);
    let cuda = || "fake-cuda".to_owned();
    assert_eq!(
        sent(&supervisor),
        [
            (1, "load", cuda()),
            (1, "segment", cuda()),
            (2, "segment", cuda())
        ]
    );
    // Another model, on no card: its load names none, and its calls carry none.
    let other = supervisor
        .call(request("fake-ok"), &Limits::none())
        .unwrap();
    assert_eq!(other.card, None);
}
