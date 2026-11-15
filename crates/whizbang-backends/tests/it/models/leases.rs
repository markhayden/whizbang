//! The lease (ADR 0017, "The lease"; plan 1B, task M6b): what stands in the lease's
//! place of an executor over the inventory. Before a request to a local model leaves
//! it takes a hold on the installation the run was admitted with, by its identity,
//! and lets it go when the request is over. Each test asks through an executor a
//! test builds over the real inventory, with a fake meter in the place after the
//! lease. And the checkpoint's exit line for models, in one test.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use whizbang_backends::models::{
    Catalogue, Current, HoldRefused, InstallRefused, Installations, Lease, ModelsDir,
    RemoveRefused, State,
};
use whizbang_core::capability::{
    Bounds, Declaration, Fault, FaultKind, Installation, Installed, InstalledRoster, Inventory,
    Picture, Query, Roster, SegmentRequest, SegmentResponse, Segmenter, Stage,
};
use whizbang_core::contract::RefusalKind;
use whizbang_core::contract::record::RunRecord;
use whizbang_core::contract::{
    Action, ActionDescriptor, Around, Call, Cancellation, Capabilities, Capability, Clock, Context,
    Engine, Executor, Failure, Kind, ModelIdentity, Open, Place, PreparedCall, Produced, Registry,
    Requirement, Secrets, Stop, Wrapped,
};
use whizbang_core::egress::{Origin, Recipient};
use whizbang_core::identity::{CanonicalPixels, Digest};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Reader, Store};
use whizbang_testkit::around::{FakeWrapper, Saw};
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::capabilities::FakeSegmenter;
use whizbang_testkit::scratch::Scratch;
use whizbang_testkit::threads;

use super::attaching::{Other, served_each_file_once, stopped_mid_file};
use super::holds::{CHILD, alone, hold, holds_over};
use super::none;
use super::publishing::{
    adapter, entry_at, first, installations, installed, is_whole, reading, record_of, said, second,
};
use super::remove::{Hand, SECOND, marked, removal_on_a_thread, removed, removing};

/// What a request is refused with when the pointer no longer names the installation
/// its run was admitted with, and when the model is draining.
const REPLACED: &str =
    "the model's installation was replaced or removed after the run was admitted";
const DRAINING: &str = "the model is being removed, and no hold on it is given";

/// The inventory of this directory, with the second entry as the catalogue's.
fn inventory(root: &Path) -> Arc<Installations> {
    Arc::new(Installations::of(
        ModelsDir::at(Arc::new(RealFs), root),
        Catalogue::new(vec![second()], &none()).unwrap(),
        [(second().adapter().clone(), adapter())],
        none(),
    ))
}

/// What the local segmenter declares: the model `tiny`, under a version of its own.
fn declared() -> Declaration {
    let model = ModelIdentity::new("local", "tiny", "declared");
    Declaration::new(model, &["point"]).unwrap()
}

fn a_point() -> SegmentRequest {
    let rgba = (0..24u8).flat_map(|at| [at, at, at, 255]).collect();
    let picture = Picture::new(CanonicalPixels::from_rgba(6, 4, rgba).unwrap()).unwrap();
    SegmentRequest::new(picture, vec![Query::point(1.0, 1.0).unwrap()], 1).unwrap()
}

/// How a request was answered, as the action saw it: nothing for an answer; the
/// fault's kind, its stage and its sentence otherwise.
type Answered = Option<(FaultKind, Stage, String)>;

fn refused(why: &str) -> Answered {
    Some((FaultKind::Rejected, Stage::NotSent, why.to_owned()))
}

/// An action that makes `asks` requests of the segmenter it was handed, each after
/// it stopped at the barrier `admitted`, and keeps how each was answered.
struct Asking {
    descriptor: ActionDescriptor,
    barriers: Barriers,
    asks: usize,
    answers: Arc<Mutex<Vec<Answered>>>,
}

impl Action for Asking {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, _: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let Some(segment) = context.capabilities().segment() else {
            return Err(Failure::error("handle", "the run was handed no segmenter"));
        };
        for _ in 0..self.asks {
            self.barriers.stop_at("admitted");
            let fault = segment.segment(&a_point()).err();
            let answered =
                fault.map(|fault| (fault.kind(), fault.stage(), fault.sentence().to_owned()));
            self.answers.lock().unwrap().push(answered);
        }
        Ok(Produced::nothing())
    }
}

/// A clock that says one millisecond more each time it is asked.
struct Ticking(AtomicU64);

impl Clock for Ticking {
    fn now_ms(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst)
    }
}

/// A request for the action `cuts`, admitted and run on a thread by an executor over
/// the inventory of this directory, with the lease over that inventory in the
/// lease's place and a fake meter in the place after it. The action makes `asks`
/// requests; the segmenter holds each open at the barrier `held`.
struct Run {
    barriers: Barriers,
    meter: FakeWrapper,
    segmenter: Arc<FakeSegmenter>,
    answers: Arc<Mutex<Vec<Answered>>>,
    ended: std::thread::JoinHandle<Result<String, String>>,
    store: Arc<Scratch>,
}

/// The refusals in the record of the one run this store holds.
fn refusals(store: &Path) -> Vec<(RefusalKind, String)> {
    let reader = Reader::open(&store.join("store"), Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    let [run] = generation.runs().values().collect::<Vec<_>>()[..] else {
        panic!("not one run");
    };
    let record = reader.read(&run.entries()["record"]).unwrap();
    let record = RunRecord::from_bytes(&record).unwrap();
    let refusals = record.outcome().refusals.iter();
    refusals
        .map(|refusal| (refusal.kind(), refusal.why().to_owned()))
        .collect()
}

/// An executor over the inventory of this directory whose one action, `cuts`, makes
/// `asks` requests of this segmenter, with this in the meter's place.
fn executor_over(
    root: &Path,
    store: &Scratch,
    asking: (usize, &Barriers, &Arc<Mutex<Vec<Answered>>>),
    segmenter: Arc<dyn Segmenter>,
    meter: Arc<dyn Around>,
) -> Engine {
    let (asks, barriers, answers) = asking;
    let inventory = inventory(root);
    let descriptor = ActionDescriptor::draft("cuts", "1.0.0", Kind::Primitive, "A test action.")
        .needs(Requirement::new(Capability::Segment))
        .build()
        .unwrap();
    let action = Asking {
        descriptor,
        barriers: barriers.clone(),
        asks,
        answers: answers.clone(),
    };
    let mut registry = Registry::new();
    registry.register(Arc::new(action)).unwrap();
    let roster = Roster::new()
        .offer(Capability::Segment, "here", segmenter.declaration().clone())
        .unwrap()
        .default_for(Capability::Segment, "here")
        .unwrap();
    let backends = InstalledRoster::over(roster, inventory.clone());
    let opened = Store::open(&store.path().join("store"), Arc::new(RealFs)).unwrap();
    Engine::over(
        registry,
        Arc::new(backends),
        Arc::new(opened),
        Digest::of_bytes(b""),
    )
    .with_clock(Arc::new(Ticking(AtomicU64::new(1000))))
    .with_capabilities(Capabilities::none().with_segment(segmenter))
    .with_wrapper(Place::Lease, Arc::new(Lease::over(inventory)))
    .with_wrapper(Place::Meter, meter)
}

/// How a request for `cuts` ended: the outcome's name, or that it was not answered.
fn submitted(executor: &Engine) -> Result<String, String> {
    match executor.submit(&Call::new("cuts")) {
        Ok(finished) => Ok(finished.outcome().name().to_owned()),
        Err(error) => Err(format!("{error:?}")),
    }
}

impl Run {
    fn admitted(root: &Path, asks: usize) -> Run {
        let store = Scratch::new("leases-store");
        let barriers = Barriers::new();
        let answers = Arc::default();
        let meter = FakeWrapper::new("meter");
        let segmenter = Arc::new(
            FakeSegmenter::new()
                .declared_as(declared())
                .stopping_at(&barriers, "held"),
        );
        let executor = executor_over(
            root,
            &store,
            (asks, &barriers, &answers),
            segmenter.clone(),
            Arc::new(meter.clone()),
        );
        let ended = threads::spawn(move || submitted(&executor));
        // The run is admitted: its action stands before its first request.
        barriers.arrived("admitted", 1);
        Run {
            barriers,
            meter,
            segmenter,
            answers,
            ended,
            store: Arc::new(store),
        }
    }

    fn answers(&self) -> Vec<Answered> {
        self.answers.lock().unwrap().clone()
    }

    /// How many requests reached the meter's place, and how many the backend.
    fn reached(&self) -> (usize, u64) {
        (self.meter.requests().len(), self.segmenter.requests())
    }

    /// Lets everything go on, and says how each request was answered and how many
    /// reached the meter's place and the backend. The store outlives it for one
    /// who kept a share of its folder.
    fn ended(self) -> (Vec<Answered>, (usize, u64)) {
        self.barriers.open("admitted");
        self.barriers.open("held");
        assert_eq!(self.ended.join().unwrap(), Ok("completed".to_owned()));
        let reached = (self.meter.requests().len(), self.segmenter.requests());
        (self.answers.lock().unwrap().clone(), reached)
    }
}

// --- between the admission and the lease --------------------------------------------------------

#[test]
fn a_reinstall_or_a_removal_between_the_admission_and_the_lease_refuses_the_request_before_dispatch()
 {
    // A reinstall, under another revision: the pointer names another installation.
    // The lease is asked for the identity the admission recorded: the request is
    // refused before it leaves, the meter's place and the backend have seen
    // nothing, and the new installation does not answer in its place.
    let scratch = Scratch::new("leases-reinstall");
    let root = scratch.path();
    let old = installed(root, &second());
    let run = Run::admitted(root, 1);
    let new = installed(root, &entry_at("2026-11.3"));
    assert_ne!(new.identity(), old.identity());
    assert_eq!(
        run.ended(),
        (vec![refused(REPLACED)], (0, 0)),
        "after a reinstall"
    );
    assert!(alone(root, &new).is_some(), "the new one was not held");
    // A removal there: no pointer.
    let scratch = Scratch::new("leases-removal");
    let root = scratch.path();
    installed(root, &second());
    let run = Run::admitted(root, 1);
    removed(root).unwrap();
    assert_eq!(
        run.ended(),
        (vec![refused(REPLACED)], (0, 0)),
        "after a removal"
    );
    // A removal that has begun and waits for another run's hold: draining.
    let scratch = Scratch::new("leases-draining");
    let root = scratch.path();
    let record = installed(root, &second());
    let other = holds_over(root);
    let kept = hold(&other, &record).unwrap();
    let run = Run::admitted(root, 1);
    let barriers = Barriers::new();
    let clock = Arc::new(Hand::new());
    let remover = removal_on_a_thread(root, &barriers, &clock);
    barriers.arrived("try", 1);
    // What the run answers then: its action is told that nothing left, in the
    // words of a model that is being removed; the run itself ends as its action
    // ends it, here completed; and its record holds the refusal, as a request
    // that was not ready.
    let store = run.store.clone();
    assert_eq!(
        run.ended(),
        (vec![refused(DRAINING)], (0, 0)),
        "while draining"
    );
    assert_eq!(
        refusals(store.path()),
        [(RefusalKind::RequestNotReady, DRAINING.to_owned())]
    );
    kept.let_go();
    barriers.open("try");
    assert!(remover.join().unwrap().is_ok());
    // Nothing happened in between: the request goes by the lease, the meter's
    // place and the backend, under the installation the admission recorded; the
    // installation is held while the request is with the backend, and let go
    // when it is over.
    let scratch = Scratch::new("leases-given");
    let root = scratch.path();
    let record = installed(root, &second());
    let run = Run::admitted(root, 1);
    run.barriers.release("admitted");
    run.barriers.arrived("held", 1);
    assert!(alone(root, &record).is_none(), "held while it is asked");
    let [asked] = run.meter.requests().try_into().unwrap();
    assert_eq!(asked.model.2, record.identity().to_string());
    assert_eq!(run.ended(), (vec![None], (1, 1)), "with nothing in between");
    assert!(alone(root, &record).is_some(), "let go when it is over");
}

#[test]
fn a_retry_inside_an_admitted_run_whose_installation_was_replaced_is_refused_the_same_way() {
    // The run makes two requests. The first has its lease and reaches the meter
    // and the backend. The model is installed again under another revision before
    // the retry: the retry is leased for the identity the admission recorded, is
    // refused before dispatch, and reaches neither.
    let scratch = Scratch::new("leases-retry");
    let root = scratch.path();
    let old = installed(root, &second());
    let run = Run::admitted(root, 2);
    run.barriers.release("admitted");
    run.barriers.arrived("held", 1);
    assert_eq!(run.reached(), (1, 1));
    // While the first request holds it, the installation is superseded and kept.
    let new = installed(root, &entry_at("2026-11.3"));
    is_whole(root, &old);
    run.barriers.release("held");
    run.barriers.arrived("admitted", 2);
    assert_eq!(run.answers(), [None]);
    let (answers, reached) = run.ended();
    assert_eq!(answers, [None, refused(REPLACED)]);
    assert_eq!(reached, (1, 1));
    assert_eq!(reading(root), Current::Installed(new));
}

#[test]
fn a_removal_that_starts_while_a_request_holds_the_model_waits_for_it() {
    let scratch = Scratch::new("leases-waits");
    let root = scratch.path();
    let record = installed(root, &second());
    let run = Run::admitted(root, 1);
    run.barriers.release("admitted");
    run.barriers.arrived("held", 1);
    // The request is with the backend, under its lease: the installation is held.
    assert!(alone(root, &record).is_none(), "held while it is asked");
    // A removal starts: it marks the model and waits.
    let barriers = Barriers::new();
    let clock = Arc::new(Hand::new());
    let remover = removal_on_a_thread(root, &barriers, &clock);
    barriers.arrived("try", 1);
    barriers.release("try");
    barriers.arrived("try", 2);
    assert!(marked(root));
    assert_eq!(reading(root), Current::Installed(record.clone()));
    is_whole(root, &record);
    // The request ends and its lease is let go: the removal goes on.
    let (answers, reached) = run.ended();
    assert_eq!((answers, reached), (vec![None], (1, 1)));
    barriers.open("try");
    let removed = remover.join().unwrap().unwrap();
    assert_eq!(removed.installation(), Some(record.identity()));
    assert_eq!(reading(root), Current::NotInstalled);
    assert!(installations(root).is_empty());
}

// --- a lease lets go when it is dropped ---------------------------------------------------------

/// A segmenter that says whether the installation was held when it was asked, and
/// panics.
struct Panicking(Declaration, Box<dyn Fn() -> bool + Send + Sync>, AtomicBool);

impl Segmenter for Panicking {
    fn declaration(&self) -> &Declaration {
        &self.0
    }

    fn segment(&self, _: &SegmentRequest, _: &Bounds) -> Result<SegmentResponse, Fault> {
        self.2.store((self.1)(), Ordering::SeqCst);
        panic!("the backend went wrong")
    }
}

/// What stands in the meter's place and panics before a request leaves, having
/// said whether the installation was held then.
struct PanicsBefore(Box<dyn Fn() -> bool + Send + Sync>, AtomicBool);

impl Around for PanicsBefore {
    fn before(&self, _: &Wrapped<'_>) -> Result<Box<dyn Open>, Stop> {
        self.1.store((self.0)(), Ordering::SeqCst);
        panic!("the meter went wrong")
    }
}

#[test]
fn a_lease_lets_its_hold_go_when_the_backend_panics_and_when_the_place_after_it_panics() {
    // What the lease holds for a request is dropped then, and never told how the
    // request ended: the hold goes with it.
    let held = |root: &Path, record| {
        let (root, record) = (root.to_path_buf(), record);
        Box::new(move || alone(&root, &record).is_none()) as Box<dyn Fn() -> bool + Send + Sync>
    };
    // The backend panics under the lease.
    let scratch = Scratch::new("leases-panic");
    let root = scratch.path();
    let record = installed(root, &second());
    let store = Scratch::new("leases-store");
    let (barriers, answers) = (Barriers::new(), Arc::default());
    barriers.open("admitted");
    let backend = Arc::new(Panicking(
        declared(),
        held(root, record.clone()),
        AtomicBool::new(false),
    ));
    let meter = FakeWrapper::new("meter");
    let executor = executor_over(
        root,
        &store,
        (1, &barriers, &answers),
        backend.clone(),
        Arc::new(meter.clone()),
    );
    let ended = submitted(&executor);
    assert!(ended.is_err(), "the run is abandoned: {ended:?}");
    assert!(backend.2.load(Ordering::SeqCst), "held when it was asked");
    assert!(matches!(
        meter.saw().as_slice(),
        [Saw::Before(_), Saw::Dropped]
    ));
    assert!(alone(root, &record).is_some(), "and let go");
    drop(executor);
    // The meter's place panics before the request leaves.
    let scratch = Scratch::new("leases-panic-before");
    let root = scratch.path();
    let record = installed(root, &second());
    let store = Scratch::new("leases-store");
    let segmenter = Arc::new(FakeSegmenter::new().declared_as(declared()));
    let meter = Arc::new(PanicsBefore(
        held(root, record.clone()),
        AtomicBool::new(false),
    ));
    let executor = executor_over(
        root,
        &store,
        (1, &barriers, &answers),
        segmenter.clone(),
        meter.clone(),
    );
    let _ = submitted(&executor);
    assert!(
        meter.1.load(Ordering::SeqCst),
        "held when the meter was asked"
    );
    assert_eq!(segmenter.requests(), 0);
    assert!(alone(root, &record).is_some(), "and let go");
}

// --- what the lease does not hold ---------------------------------------------------------------

#[test]
fn a_backend_that_names_a_recipient_takes_no_lease_and_a_local_one_with_no_installation_is_refused()
{
    let scratch = Scratch::new("leases-others");
    let root = scratch.path();
    let record = installed(root, &second());
    let ask = |segmenter: Arc<FakeSegmenter>,
               backends: Arc<dyn whizbang_core::contract::Backends>| {
        let store = Scratch::new("leases-store");
        let (barriers, answers): (Barriers, Arc<Mutex<Vec<Answered>>>) = Default::default();
        barriers.open("admitted");
        let descriptor =
            ActionDescriptor::draft("cuts", "1.0.0", Kind::Primitive, "A test action.")
                .needs(Requirement::new(Capability::Segment))
                .build()
                .unwrap();
        let action = Asking {
            descriptor,
            barriers,
            asks: 1,
            answers: answers.clone(),
        };
        let mut registry = Registry::new();
        registry.register(Arc::new(action)).unwrap();
        let opened = Store::open(&store.path().join("store"), Arc::new(RealFs)).unwrap();
        let meter = FakeWrapper::new("meter");
        let executor = Engine::over(registry, backends, Arc::new(opened), Digest::of_bytes(b""))
            .with_clock(Arc::new(Ticking(AtomicU64::new(1000))))
            .with_capabilities(Capabilities::none().with_segment(segmenter.clone()))
            // A gate that lets everything by: the lease is what is asked here.
            .with_wrapper(Place::Gate, Arc::new(FakeWrapper::new("gate")))
            .with_wrapper(Place::Lease, Arc::new(Lease::over(inventory(root))))
            .with_wrapper(Place::Meter, Arc::new(meter.clone()));
        assert_eq!(submitted(&executor), Ok("completed".to_owned()));
        let answered = answers.lock().unwrap().clone();
        (answered, meter.requests().len(), segmenter.requests())
    };
    let roster = |declaration: &Declaration| {
        Roster::new()
            .offer(Capability::Segment, "here", declaration.clone())
            .unwrap()
            .default_for(Capability::Segment, "here")
            .unwrap()
    };
    // A backend that names a recipient answers with no model of this machine: it
    // goes by the lease, whatever is installed, and nothing is held for it.
    let elsewhere =
        Recipient::new(Origin::of("https://api.example.com", &Secrets::none()).unwrap());
    let hosted = Arc::new(FakeSegmenter::new().declared_as(declared().sending_to(elsewhere)));
    let backends = Arc::new(roster(hosted.declaration()));
    assert_eq!(ask(hosted, backends), (vec![None], 1, 1));
    // A local backend whose run was admitted under no installation, by a roster
    // that is over no inventory: the lease has nothing to hold it to, and refuses.
    let local = Arc::new(FakeSegmenter::new().declared_as(declared()));
    let backends = Arc::new(roster(local.declaration()));
    let not_admitted = "the run was not admitted under an installation of the model";
    assert_eq!(ask(local, backends), (vec![refused(not_admitted)], 0, 0));
    assert!(alone(root, &record).is_some());
}

#[test]
fn a_hold_is_asked_by_a_declared_name_that_is_looked_up_in_the_catalogue() {
    let scratch = Scratch::new("leases-names");
    let root = scratch.path();
    let record = installed(root, &second());
    let asked = inventory(root);
    let admitted = Installation::of(record.identity().digest());
    assert_eq!(asked.look("tiny"), Installed::Ready(admitted));
    for name in ["Tiny", "../tiny", "tiny/", "", "other"] {
        let refused = asked.hold(name, admitted).unwrap_err();
        assert_eq!(refused, HoldRefused::NotNamed, "{name:?}");
    }
    // By the identity that was admitted, never by what is current: with the model
    // installed and ready, a hold on another identity is refused as not named,
    // and the installation that is current is not held in its place.
    let another = Installation::of(record_of(&entry_at("2026-11.3")).identity().digest());
    assert_eq!(asked.look("tiny"), Installed::Ready(admitted));
    assert_eq!(
        asked.hold("tiny", another).unwrap_err(),
        HoldRefused::NotNamed
    );
    assert!(
        alone(root, &record).is_some(),
        "the current one is not held"
    );
    // An identity that cannot be read back, because its text holds a secret the
    // inventory was told of, is refused under that name, and holds nothing.
    let hex = record.identity().hex();
    let knowing = Installations::of(
        ModelsDir::at(Arc::new(RealFs), root),
        Catalogue::new(vec![second()], &none()).unwrap(),
        [(second().adapter().clone(), adapter())],
        Secrets::new([hex[8..24].to_owned()]).unwrap(),
    );
    let refused = knowing.hold("tiny", admitted).unwrap_err();
    assert_eq!(refused, HoldRefused::NotAnIdentity);
    assert!(!refused.to_string().contains(&hex[8..24]));
    assert!(alone(root, &record).is_some());
    // Two holds through one inventory are one share of the hold file, and a count.
    let one = asked.hold("tiny", admitted).unwrap();
    let two = asked.hold("tiny", admitted).unwrap();
    assert_eq!(one.installation(), record.identity());
    drop(one);
    assert!(alone(root, &record).is_none());
    drop(two);
    assert!(alone(root, &record).is_some());
}

// --- the checkpoint's exit line -----------------------------------------------------------------

#[test]
fn a_model_installs_is_attached_to_is_cancelled_and_is_drained_from_this_process_and_a_second_one()
{
    // The exit line of checkpoint 1B, for models: "a model installs, attaches,
    // cancels and drains with fake downloads". Each clause through the crate's own
    // interface, with this process on one side and a real second process on the
    // other: the test's own program run once more, three times, one after another.
    let scratch = Scratch::new("leases-exit");
    let root = scratch.path();
    let before = installed(root, &first());
    let attaching = "models::attaching::child_asks_for_the_model_and_says_what_it_sees";

    // Cancels. An install here is stopped inside its second file; the second
    // process asks for the same model, attaches, and is shown how far the install
    // is. The request that started the install cancels it: both end as cancelled,
    // and what was installed before is named and whole.
    let barriers = Barriers::new();
    let cancellation = Cancellation::new();
    let (_, installer) = stopped_mid_file(root, &barriers, &cancellation);
    let mut other = Other::running(attaching, root);
    assert_eq!(other.says("sees"), "Installing 1 2");
    cancellation.cancel();
    barriers.release("mid");
    assert_eq!(installer.join().unwrap(), Err(InstallRefused::Cancelled));
    assert_eq!(other.says("ended"), "Cancelled");
    assert_eq!(other.says("asked"), "0");
    other.ended_well();
    assert_eq!(reading(root), Current::Installed(before.clone()));
    is_whole(root, &before);
    assert_eq!(said(root), Some((State::Cancelled, 1, 2)));

    // Installs, and attaches. The install runs again here; the second process
    // attaches, sees it, downloads nothing, and ends with the same installation.
    let barriers = Barriers::new();
    let (fake, installer) = stopped_mid_file(root, &barriers, &Cancellation::new());
    let mut other = Other::running(attaching, root);
    assert_eq!(other.says("sees"), "Installing 1 2");
    barriers.release("mid");
    let published = installer.join().unwrap().unwrap();
    assert_eq!(published, record_of(&second()));
    assert_eq!(other.says("ended"), published.identity().to_string());
    assert_eq!(other.says("asked"), "0");
    other.ended_well();
    served_each_file_once(&fake);
    assert_eq!(reading(root), Current::Installed(published.clone()));
    is_whole(root, &published);
    assert_eq!(installations(root), [published.identity().hex()]);

    // Drains. The second process holds the installation. A removal here refuses a
    // new hold and answers busy at its deadline, with nothing changed; a second
    // removal goes on when the other process lets its hold go, and the model's
    // files are gone.
    let mut other = Other::running(CHILD, root);
    assert_eq!(other.says("holds"), published.identity().to_string());
    let holds = holds_over(root);
    let clock = Hand::new();
    let busy = removing(Arc::new(RealFs), root, &clock, SECOND, &mut || {
        assert_eq!(hold(&holds, &published).unwrap_err(), HoldRefused::Draining);
        clock.advance(SECOND);
    });
    assert_eq!(busy, Err(RemoveRefused::Held));
    assert_eq!(reading(root), Current::Installed(published.clone()));
    assert!(!marked(root));
    let barriers = Barriers::new();
    let remover = removal_on_a_thread(root, &barriers, &Arc::new(Hand::new()));
    barriers.arrived("try", 1);
    other.tell("go");
    assert_eq!(other.says("let"), "go");
    other.ended_well();
    barriers.open("try");
    let removed = remover.join().unwrap().unwrap();
    assert_eq!(removed.installation(), Some(published.identity()));
    assert_eq!(reading(root), Current::NotInstalled);
    assert!(installations(root).is_empty());
    assert!(!marked(root));
}
