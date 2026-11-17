//! Removal against the queue (task P5b; the plan of 1C, decisions D5 and D6): a removal
//! in this process tells the supervisor, which ends every request queued for the model,
//! never sent, and unloads the model when it is resident and idle; a removal in another
//! process is seen by the model's count of removals, which the supervisor reads for each
//! queued request every 250 ms of its clock and when it takes one off the queue, and a
//! request whose count has moved from the one its hold was given is ended, whether or
//! not the mark is still there. The lease refuses new requests (1B), and the removal
//! waits for the current call, whose hold it waits for.
//!
//! Every test here starts the pack through the test kit's guard
//! (`super::worker::queued_over`), and is of the group `pack-workers`. The fake models
//! are installed in the fixture's models directory, where the supervisor reads the
//! counts; a hold stands for a request's lease, taken before the request enters the
//! queue and let go when its call is over. One `Holds`, and one `ModelsDir`, stand for
//! one process. The clock stands still until the test moves it (`super::queue::Still`),
//! so what the supervisor reads by the clock is read when the test says; its pause waits
//! as the machine's does. What paces a removal is a barrier, or its own clock.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use whizbang_backends::models::{
    Current, Entry, Hold, HoldRefused, Holds, InstallationRecord, ModelName, ModelsDir,
    RemoveRefused, Removed, current, draining, install, removals, remove, remove_and_tell,
};
use whizbang_backends::pack::proto::Ask;
use whizbang_backends::pack::queue::{READ_REMOVALS_EVERY, Request, Status, Supervisor};
use whizbang_backends::pack::worker::{Called, Failed, Limits};
use whizbang_core::capability::{FaultKind, Query, SegmentRequest, Stage};
use whizbang_core::contract::{Cancellation, Secrets};
use whizbang_core::egress::Address;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::downloads::FakeDownloader;
use whizbang_testkit::threads;

use super::conformance::picture;
use super::queue::Still;
use super::worker::queued_over;
use crate::models::attaching::{Other, root_from_parent, say, told};
use crate::models::{file, parts, revision};

/// How long a test waits for a state of the supervisor. The clock stands still here, so
/// what ends a wait that sees nothing is its count of looks, a few times this long at
/// most: well inside the limit on one test.
const WITHIN: Duration = Duration::from_secs(5);

/// What the sentence of a request ended by a removal says.
const REMOVED_HERE: &str = "the model is being removed; the request was ended while it waited in the queue of the pack's worker, and was not sent";
const REMOVED_ELSEWHERE: &str = "a removal of the model began after the request's hold was given; the request was ended while it waited in the queue of the pack's worker, and was not sent";

type Answer = Result<Called, Box<Failed>>;

fn none() -> Secrets {
    Secrets::none()
}

fn model(name: &str) -> ModelName {
    ModelName::new(name, &none()).unwrap()
}

/// The entry of this fake model: one small file.
fn entry(name: &str) -> Entry {
    let mut parts = parts(name);
    parts.files = vec![file("weights.bin", 3, b"abc")];
    Entry::new(parts, &none()).unwrap()
}

/// Installs this fake model in the models directory, as a process of its own.
fn installed(root: &Path, name: &str) -> InstallationRecord {
    let at = Address::of("https://weights.example.org/files/weights.bin", &none()).unwrap();
    let fake = FakeDownloader::new().serving(&at, b"abc".to_vec());
    let dir = ModelsDir::at(Arc::new(RealFs), root);
    install(
        &dir,
        &entry(name),
        &revision("7"),
        &fake,
        &Cancellation::new(),
        &none(),
    )
    .unwrap()
}

/// The holds of a process of its own over the models directory.
fn holds_over(root: &Path) -> Holds {
    Holds::over(ModelsDir::at(Arc::new(RealFs), root))
}

fn hold(holds: &Holds, record: &InstallationRecord) -> Result<Hold, HoldRefused> {
    holds.hold(record.model(), record.identity(), &none())
}

/// A request for the fake model this hold is on, with the count its hold was given: its
/// load, with no settings, and a segment of one phrase.
fn request(record: &InstallationRecord, held: &Hold) -> Request {
    let asked = SegmentRequest::new(picture(), vec![Query::phrase("a red thing").unwrap()], 1);
    let ask = Ask::segment(&asked.unwrap(), record.model().clone(), record.identity());
    Request {
        model: record.model().clone(),
        installation: record.identity(),
        settings: Default::default(),
        ask,
        removals: held.removals(),
    }
}

/// A request asked on a thread of the test's own, under this hold, which is let go when
/// its call is over, as a lease's is.
fn leased(
    supervisor: &Arc<Supervisor>,
    record: &InstallationRecord,
    held: Hold,
    limits: Limits,
) -> JoinHandle<Answer> {
    let (supervisor, request) = (supervisor.clone(), request(record, &held));
    threads::spawn(move || {
        let answer = supervisor.call(request, &limits);
        drop(held);
        answer
    })
}

fn cancelled_by(signal: &Cancellation) -> Limits {
    Limits {
        cancellation: signal.clone(),
        deadline: None,
    }
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

/// The numbers of the requests that wait, front first.
fn waiting(supervisor: &Supervisor) -> Vec<u64> {
    let status = supervisor.status();
    status
        .waiting
        .iter()
        .map(|waiting| waiting.number)
        .collect()
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

fn failed(answer: Answer) -> Box<Failed> {
    match answer {
        Ok(called) => panic!("answered: {called:?}"),
        Err(failed) => failed,
    }
}

/// A request ended, never sent, as a removal ends one: refused, in these words.
fn ended_by_removal(answer: Answer, words: &str) {
    let ended = failed(answer);
    assert_eq!(ended.fault.kind(), FaultKind::Rejected, "{ended:?}");
    assert_eq!(ended.fault.stage(), Stage::NotSent);
    assert_eq!(ended.fault.sentence(), words);
    assert_eq!(ended.ended, None);
}

/// Whether the model is draining, asked by one who holds no lock.
fn is_draining(root: &Path, name: &str) -> bool {
    let dir = ModelsDir::at(Arc::new(RealFs), root);
    draining(&dir, &model(name), &none()).unwrap()
}

fn removals_of(root: &Path, name: &str) -> u64 {
    let dir = ModelsDir::at(Arc::new(RealFs), root);
    removals(&dir, &model(name), &none()).unwrap()
}

fn is_installed(root: &Path, name: &str) -> bool {
    let dir = ModelsDir::at(Arc::new(RealFs), root);
    matches!(
        current(&dir, &model(name), &none()).unwrap(),
        Current::Installed(_)
    )
}

/// A removal of this model by a process of its own, on a thread, that tells this
/// supervisor each time it sets its mark: a removal in the supervisor's process. Its
/// tries stop at the barrier `try`.
fn removal_here(
    root: &Path,
    name: &str,
    supervisor: &Arc<Supervisor>,
    barriers: &Barriers,
) -> JoinHandle<Result<Removed, RemoveRefused>> {
    let (root, name) = (root.to_path_buf(), model(name));
    let (supervisor, stop) = (supervisor.clone(), barriers.clone());
    threads::spawn(move || {
        let dir = ModelsDir::at(Arc::new(RealFs), &root);
        let clock = Still::new();
        let patience = Duration::from_secs(60);
        let mut pause = || stop.stop_at("try");
        let mut marked = || supervisor.removing(&name);
        remove_and_tell(
            &dir,
            &name,
            &clock,
            patience,
            &none(),
            &mut pause,
            &mut marked,
        )
    })
}

/// A removal of this model in a process of its own, on a thread, that tells nobody: a
/// removal in another process. Its tries stop at the barrier `try`.
fn removal_elsewhere(
    root: &Path,
    name: &str,
    barriers: &Barriers,
) -> JoinHandle<Result<Removed, RemoveRefused>> {
    let (root, name, stop) = (root.to_path_buf(), model(name), barriers.clone());
    threads::spawn(move || {
        let dir = ModelsDir::at(Arc::new(RealFs), &root);
        let clock = Still::new();
        let patience = Duration::from_secs(60);
        remove(&dir, &name, &clock, patience, &none(), &mut || {
            stop.stop_at("try");
        })
    })
}

/// A call to `fake-wait` that holds the worker until it is cancelled, asked under a hold
/// of its own; seen in the worker before this returns.
fn holding_the_worker(
    supervisor: &Arc<Supervisor>,
    holds: &Holds,
    root: &Path,
) -> (JoinHandle<Answer>, Cancellation, InstallationRecord) {
    let record = installed(root, "fake-wait");
    let signal = Cancellation::new();
    let held = hold(holds, &record).unwrap();
    let call = leased(supervisor, &record, held, cancelled_by(&signal));
    seen(supervisor, |status| calling(status, "segment", 1));
    (call, signal, record)
}

#[test]
fn a_removal_refuses_new_requests_cancels_the_queued_ones_and_waits_for_the_current_call() {
    let clock = Arc::new(Still::new());
    let (supervisor, root) = queued_over(None, clock, |_| {});
    let holds = holds_over(&root);
    // The current call is of the model that is removed, and holds it.
    let (current, signal, wait) = holding_the_worker(&supervisor, &holds, &root);
    // Two requests for the same model wait behind it, and one for another model.
    let mut same = Vec::new();
    for count in 1..=2 {
        let held = hold(&holds, &wait).unwrap();
        same.push(leased(&supervisor, &wait, held, Limits::none()));
        seen(&supervisor, |status| status.waiting.len() == count);
    }
    let ok = installed(&root, "fake-ok");
    let other = leased(&supervisor, &ok, hold(&holds, &ok).unwrap(), Limits::none());
    seen(&supervisor, |status| status.waiting.len() == 3);

    // The removal, in this process: it tells the supervisor once it has set its mark.
    let barriers = Barriers::new();
    let remover = removal_here(&root, "fake-wait", &supervisor, &barriers);
    // The two queued for the model end, never sent, while the current call still holds
    // the worker, and their holds are let go.
    seen(&supervisor, |status| status.waiting.len() == 1);
    for waiter in same {
        ended_by_removal(waiter.join().unwrap(), REMOVED_HERE);
    }
    assert_eq!(waiting(&supervisor), [4]);
    assert!(calling(&supervisor.status(), "segment", 1));
    // New requests are refused, by the lease: its hold is refused in this process and
    // in another.
    assert_eq!(hold(&holds, &wait).unwrap_err(), HoldRefused::Draining);
    let elsewhere = holds_over(&root);
    assert_eq!(hold(&elsewhere, &wait).unwrap_err(), HoldRefused::Draining);
    // The removal waits for the current call's hold, try after try.
    barriers.arrived("try", 1);
    barriers.release("try");
    barriers.arrived("try", 2);
    assert!(is_installed(&root, "fake-wait"));
    assert!(!remover.is_finished());

    // The current call ends, and lets its hold go: the removal goes on, and the request
    // for the other model is answered.
    signal.cancel();
    let stopped = failed(current.join().unwrap());
    assert_eq!(stopped.fault.kind(), FaultKind::Cancelled, "{stopped:?}");
    barriers.open("try");
    let removed = remover.join().unwrap().unwrap();
    assert_eq!(removed.installation(), Some(wait.identity()));
    assert!(!is_installed(&root, "fake-wait"));
    assert!(other.join().unwrap().is_ok());
    // Neither queued request for the removed model reached the worker.
    assert!(
        sent(&supervisor)
            .iter()
            .all(|(number, ..)| *number == 1 || *number == 4),
        "{:?}",
        sent(&supervisor)
    );
}

#[test]
fn a_removal_from_a_second_process_cancels_the_queued_requests_within_the_interval_while_another_call_holds_the_worker()
 {
    let clock = Arc::new(Still::new());
    let (supervisor, root) = queued_over(None, clock.clone(), |_| {});
    let holds = holds_over(&root);
    let (current, signal, _) = holding_the_worker(&supervisor, &holds, &root);
    let ok = installed(&root, "fake-ok");
    let queued = leased(&supervisor, &ok, hold(&holds, &ok).unwrap(), Limits::none());
    seen(&supervisor, |status| status.waiting.len() == 1);

    // Another process removes the model: it sets its mark, raises the count, and waits
    // for the queued request's hold. It tells this one nothing.
    let barriers = Barriers::new();
    let remover = removal_elsewhere(&root, "fake-ok", &barriers);
    barriers.arrived("try", 1);
    assert!(is_draining(&root, "fake-ok"));
    assert_eq!(removals_of(&root, "fake-ok"), 1);
    // Until the clock reaches the next look, the request waits.
    let left = supervisor.wait_for(Duration::from_millis(100), &|status: &Status| {
        status.waiting.is_empty()
    });
    assert!(!left, "{:?}", supervisor.status());

    // Within the interval of the clock, it is ended, never sent, while the other call
    // still holds the worker.
    clock.step(READ_REMOVALS_EVERY);
    seen(&supervisor, |status| status.waiting.is_empty());
    ended_by_removal(queued.join().unwrap(), REMOVED_ELSEWHERE);
    assert!(calling(&supervisor.status(), "segment", 1));
    assert!(supervisor.status().waiting.is_empty());
    // Its hold let go, the removal goes on.
    barriers.open("try");
    assert_eq!(
        remover.join().unwrap().unwrap().installation(),
        Some(ok.identity())
    );
    signal.cancel();
    assert!(current.join().unwrap().is_err());
    assert!(sent(&supervisor).iter().all(|(number, ..)| *number == 1));
}

#[test]
fn a_removal_that_began_and_timed_out_between_two_looks_still_cancels_the_requests_queued_before_it()
 {
    let clock = Arc::new(Still::new());
    let (supervisor, root) = queued_over(None, clock.clone(), |_| {});
    let holds = holds_over(&root);
    let (current, signal, _) = holding_the_worker(&supervisor, &holds, &root);
    let ok = installed(&root, "fake-ok");
    let queued = leased(&supervisor, &ok, hold(&holds, &ok).unwrap(), Limits::none());
    seen(&supervisor, |status| status.waiting.len() == 1);

    // Another process removes the model and gives up: the queued request's hold is kept
    // past its patience, of its own clock. It took its mark off when it ended.
    let removal_clock = Still::new();
    let dir = ModelsDir::at(Arc::new(RealFs), &root);
    let busy = remove(
        &dir,
        &model("fake-ok"),
        &removal_clock,
        Duration::from_secs(1),
        &none(),
        &mut || removal_clock.step(Duration::from_secs(1)),
    );
    assert_eq!(busy.unwrap_err(), RemoveRefused::Held);
    assert!(!is_draining(&root, "fake-ok"));
    assert!(!root.join("models/fake-ok/DRAINING").exists());
    assert_eq!(removals_of(&root, "fake-ok"), 1);
    assert_eq!(waiting(&supervisor), [2]);

    // The clock has not moved, so no look by the clock came. The next look is when the
    // request is taken off the queue, once the current call is over: it sees the count
    // moved, though no mark is there, and the request is ended, never sent.
    signal.cancel();
    assert!(current.join().unwrap().is_err());
    seen(&supervisor, |status| status.waiting.is_empty());
    ended_by_removal(queued.join().unwrap(), REMOVED_ELSEWHERE);
    assert!(sent(&supervisor).iter().all(|(number, ..)| *number == 1));
    assert!(supervisor.status().calling.is_none());
}

#[test]
fn a_removal_whose_wait_times_out_answers_busy_and_the_model_takes_requests_again() {
    let clock = Arc::new(Still::new());
    let (supervisor, root) = queued_over(None, clock.clone(), |_| {});
    let holds = holds_over(&root);
    let (current, signal, _) = holding_the_worker(&supervisor, &holds, &root);
    let ok = installed(&root, "fake-ok");
    // A run holds the model, and does not let it go; one request for it waits.
    let run = hold(&holds, &ok).unwrap();
    let queued = leased(&supervisor, &ok, hold(&holds, &ok).unwrap(), Limits::none());
    seen(&supervisor, |status| status.waiting.len() == 1);

    // The removal, in this process, ends the queued request, waits for the run's hold
    // past its patience, of its own clock, and answers busy.
    let removal_clock = Still::new();
    let dir = ModelsDir::at(Arc::new(RealFs), &root);
    let name = model("fake-ok");
    let busy = remove_and_tell(
        &dir,
        &name,
        &removal_clock,
        Duration::from_secs(1),
        &none(),
        &mut || removal_clock.step(Duration::from_secs(1)),
        &mut || supervisor.removing(&name),
    );
    assert_eq!(busy.unwrap_err(), RemoveRefused::Held);
    seen(&supervisor, |status| status.waiting.is_empty());
    ended_by_removal(queued.join().unwrap(), REMOVED_HERE);
    assert!(!is_draining(&root, "fake-ok"));
    assert_eq!(removals_of(&root, "fake-ok"), 1);

    // The model takes requests again: a hold is given, with the count as it is now, and
    // a request under it waits through looks of the clock and is answered.
    let again = hold(&holds, &ok).unwrap();
    assert_eq!(again.removals(), 1);
    let asked = leased(&supervisor, &ok, again, Limits::none());
    seen(&supervisor, |status| status.waiting.len() == 1);
    for _ in 0..3 {
        clock.step(READ_REMOVALS_EVERY);
        let left = supervisor.wait_for(Duration::from_millis(50), &|status: &Status| {
            status.waiting.is_empty()
        });
        assert!(!left, "{:?}", supervisor.status());
    }
    signal.cancel();
    assert!(current.join().unwrap().is_err());
    let answered = asked.join().unwrap();
    assert!(answered.is_ok(), "{answered:?}");
    assert_eq!(
        sent(&supervisor)[2..],
        [
            (3, "unload", "fake-wait".to_owned()),
            (3, "load", "fake-ok".to_owned()),
            (3, "segment", "fake-ok".to_owned()),
        ]
    );
    drop(run);
}

#[test]
fn a_removal_in_this_process_unloads_the_model_that_was_resident_and_idle() {
    let clock = Arc::new(Still::new());
    let (supervisor, root) = queued_over(None, clock, |_| {});
    let holds = holds_over(&root);
    let ok = installed(&root, "fake-ok");
    let held = hold(&holds, &ok).unwrap();
    assert!(
        supervisor
            .call(request(&ok, &held), &Limits::none())
            .is_ok()
    );
    drop(held);
    let worker = supervisor.status().worker;
    assert!(worker.is_some());
    assert_eq!(
        supervisor.status().resident.as_ref().map(ModelName::as_str),
        Some("fake-ok")
    );

    // The removal tells the supervisor: the model is unloaded from the idle worker, which
    // lives on, before the removal ends.
    let dir = ModelsDir::at(Arc::new(RealFs), &root);
    let name = model("fake-ok");
    let removed = remove_and_tell(
        &dir,
        &name,
        &Still::new(),
        Duration::from_secs(60),
        &none(),
        &mut || panic!("the removal waited"),
        &mut || supervisor.removing(&name),
    );
    assert_eq!(removed.unwrap().installation(), Some(ok.identity()));
    assert_eq!(
        sent(&supervisor),
        [
            (1, "load", "fake-ok".to_owned()),
            (1, "segment", "fake-ok".to_owned()),
            (0, "unload", "fake-ok".to_owned()),
        ]
    );
    let status = supervisor.status();
    assert_eq!(status.resident, None);
    assert_eq!(status.worker, worker);
    assert!(status.calling.is_none() && status.waiting.is_empty());
}

/// Run only in a process of its own: removes the model `fake-ok` in the directory it is
/// told of, and stops when it has set its mark, until a line arrives on its input.
#[test]
#[ignore = "run in a process of its own by the test of a remover that is killed"]
fn child_removes_fake_ok_and_stops_when_it_has_marked() {
    let root: PathBuf = root_from_parent();
    let dir = ModelsDir::at(Arc::new(RealFs), &root);
    let ended = remove_and_tell(
        &dir,
        &model("fake-ok"),
        &Still::new(),
        Duration::from_secs(60),
        &none(),
        &mut || {},
        &mut || {
            say("marked yes");
            told();
        },
    );
    say(&format!("ended {}", ended.is_ok()));
}

#[test]
fn after_a_remover_was_killed_with_its_mark_set_a_request_for_the_model_is_answered_by_the_worker()
{
    let clock = Arc::new(Still::new());
    let (supervisor, root) = queued_over(None, clock.clone(), |_| {});
    let holds = holds_over(&root);
    let ok = installed(&root, "fake-ok");
    // A real process removes the model, and is stopped once its mark is set: while it
    // lives no hold is given.
    let mut other = Other::running(
        "pack::removal::child_removes_fake_ok_and_stops_when_it_has_marked",
        &root,
    );
    assert_eq!(other.says("marked"), "yes");
    assert!(is_draining(&root, "fake-ok"));
    assert_eq!(hold(&holds, &ok).unwrap_err(), HoldRefused::Draining);
    // Killed with its mark set: the mark stays, and stops nobody.
    other.killed();
    assert!(root.join("models/fake-ok/DRAINING").exists());
    assert!(!is_draining(&root, "fake-ok"));
    assert_eq!(removals_of(&root, "fake-ok"), 1);

    // A request for the model, under a hold given now, waits behind another call
    // through a look of the clock, and is answered by the worker.
    let (current, signal, _) = holding_the_worker(&supervisor, &holds, &root);
    let held = hold(&holds, &ok).unwrap();
    assert_eq!(held.removals(), 1);
    let asked = leased(&supervisor, &ok, held, Limits::none());
    seen(&supervisor, |status| status.waiting.len() == 1);
    clock.step(READ_REMOVALS_EVERY);
    let left = supervisor.wait_for(Duration::from_millis(50), &|status: &Status| {
        status.waiting.is_empty()
    });
    assert!(!left, "{:?}", supervisor.status());
    signal.cancel();
    assert!(current.join().unwrap().is_err());
    let answered = asked.join().unwrap();
    assert!(answered.is_ok(), "{answered:?}");
    assert_eq!(
        sent(&supervisor).last(),
        Some(&(2, "segment", "fake-ok".to_owned()))
    );
    assert!(root.join("models/fake-ok/DRAINING").exists());
}
