//! What a lease takes (ADR 0017, "What a lease takes"; plan 1B, task M6b): a hold on
//! the installation a run was admitted with, by its identity, asked of the
//! inventory. The lease's place in the executor is not built (task I7's second
//! half): here the action of an admitted run stands in that place, takes the hold,
//! and reaches a meter only with it. And the checkpoint's exit line for models, in
//! one test.

use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use whizbang_backends::models::{
    Catalogue, Current, HoldRefused, InstallRefused, Installations, ModelsDir, RemoveRefused, State,
};
use whizbang_core::capability::{
    Declaration, Installation, Installed, InstalledRoster, Inventory, Roster,
};
use whizbang_core::contract::{
    Action, ActionDescriptor, Call, Cancellation, Capability, Clock, Context, Engine, Executor,
    Failure, Kind, ModelIdentity, PreparedCall, Produced, Registry, Requirement,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::scratch::Scratch;
use whizbang_testkit::threads;

use super::attaching::{Other, served_each_file_once, stopped_mid_file};
use super::holds::{CHILD, alone, hold, holds_over};
use super::none;
use super::publishing::{
    adapter, entry_at, first, installations, installed, is_whole, reading, record_of, said, second,
};
use super::remove::{Hand, SECOND, marked, removal_on_a_thread, removed, removing};

/// The inventory of this directory, with the second entry as the catalogue's.
fn inventory(root: &Path) -> Arc<Installations> {
    Arc::new(Installations::of(
        ModelsDir::at(Arc::new(RealFs), root),
        Catalogue::new(vec![second()], &none()).unwrap(),
        [(second().adapter().clone(), adapter())],
        none(),
    ))
}

/// What the action does in the lease's place, once for each request it makes: stops
/// at the barrier `admitted`, then takes a hold on the installation its call was
/// prepared under, and only with it reaches the meter; it keeps the hold at the
/// barrier `held` and lets it go. `asks` is how many requests it makes.
struct Leasing {
    descriptor: ActionDescriptor,
    inventory: Arc<Installations>,
    barriers: Barriers,
    asks: usize,
    /// The fake meter in the place after the lease: how many requests reached it.
    metered: Arc<AtomicUsize>,
    /// How each try for the hold ended.
    leases: Arc<Mutex<Vec<Result<String, HoldRefused>>>>,
}

impl Action for Leasing {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, _: &Context) -> Result<Produced, Failure> {
        let model = call.needs()[0].model().unwrap();
        let admitted = Installation::of(model.version().parse::<Digest>().unwrap());
        for _ in 0..self.asks {
            self.barriers.stop_at("admitted");
            let lease = self.inventory.hold(model.model(), admitted);
            let ended = lease.as_ref().map(|hold| hold.installation().to_string());
            self.leases.lock().unwrap().push(ended.map_err(|why| *why));
            if let Ok(hold) = lease {
                self.metered.fetch_add(1, Ordering::SeqCst);
                self.barriers.stop_at("held");
                hold.let_go();
            }
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

type Leases = Arc<Mutex<Vec<Result<String, HoldRefused>>>>;

/// A request for the action `cuts`, admitted and run on a thread by an executor over
/// the inventory of this directory. The action makes `asks` requests.
struct Run {
    barriers: Barriers,
    metered: Arc<AtomicUsize>,
    leases: Leases,
    ended: std::thread::JoinHandle<String>,
    _store: Scratch,
}

impl Run {
    fn admitted(root: &Path, asks: usize) -> Run {
        let store = Scratch::new("leases-store");
        let barriers = Barriers::new();
        let metered = Arc::new(AtomicUsize::new(0));
        let leases = Leases::default();
        let inventory = inventory(root);
        let descriptor =
            ActionDescriptor::draft("cuts", "1.0.0", Kind::Primitive, "A test action.")
                .needs(Requirement::new(Capability::Segment))
                .build()
                .unwrap();
        let action = Leasing {
            descriptor,
            inventory: inventory.clone(),
            barriers: barriers.clone(),
            asks,
            metered: metered.clone(),
            leases: leases.clone(),
        };
        let mut registry = Registry::new();
        registry.register(Arc::new(action)).unwrap();
        let declared = ModelIdentity::new("local", "tiny", "declared");
        let roster = Roster::new()
            .offer(
                Capability::Segment,
                "here",
                Declaration::new(declared, &[]).unwrap(),
            )
            .unwrap()
            .default_for(Capability::Segment, "here")
            .unwrap();
        let backends = InstalledRoster::over(roster, inventory);
        let opened = Store::open(&store.path().join("store"), Arc::new(RealFs)).unwrap();
        let executor = Engine::over(
            registry,
            Arc::new(backends),
            Arc::new(opened),
            Digest::of_bytes(b""),
        )
        .with_clock(Arc::new(Ticking(AtomicU64::new(1000))));
        let ended = threads::spawn(move || {
            let finished = executor.submit(&Call::new("cuts")).unwrap();
            finished.outcome().name().to_owned()
        });
        // The run is admitted: its action stands before the lease.
        barriers.arrived("admitted", 1);
        Run {
            barriers,
            metered,
            leases,
            ended,
            _store: store,
        }
    }

    fn leases(&self) -> Vec<Result<String, HoldRefused>> {
        self.leases.lock().unwrap().clone()
    }

    fn metered(&self) -> usize {
        self.metered.load(Ordering::SeqCst)
    }

    fn ended(self) -> (Vec<Result<String, HoldRefused>>, usize) {
        self.barriers.open("admitted");
        self.barriers.open("held");
        self.ended.join().unwrap();
        let metered = self.metered.load(Ordering::SeqCst);
        (self.leases.lock().unwrap().clone(), metered)
    }
}

// --- between the admission and the lease --------------------------------------------------------

#[test]
fn a_reinstall_or_a_removal_between_the_admission_and_the_lease_refuses_the_hold_and_nothing_is_metered()
 {
    // A reinstall, under another revision: the pointer names another installation.
    // The hold is asked for the identity the admission recorded: refused, and the
    // new installation does not answer in its place.
    let scratch = Scratch::new("leases-reinstall");
    let root = scratch.path();
    let old = installed(root, &second());
    let run = Run::admitted(root, 1);
    let new = installed(root, &entry_at("2026-11.3"));
    assert_ne!(new.identity(), old.identity());
    assert_eq!(
        run.ended(),
        (vec![Err(HoldRefused::NotNamed)], 0),
        "after a reinstall"
    );
    assert!(alone(root, &new).is_some());
    // A removal there: no pointer.
    let scratch = Scratch::new("leases-removal");
    let root = scratch.path();
    installed(root, &second());
    let run = Run::admitted(root, 1);
    removed(root).unwrap();
    assert_eq!(
        run.ended(),
        (vec![Err(HoldRefused::NotNamed)], 0),
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
    assert_eq!(
        run.ended(),
        (vec![Err(HoldRefused::Draining)], 0),
        "while draining"
    );
    kept.let_go();
    barriers.open("try");
    assert!(remover.join().unwrap().is_ok());
    // Nothing happened in between: the hold is given, on the installation the
    // admission recorded, and the request reaches the meter.
    let scratch = Scratch::new("leases-given");
    let root = scratch.path();
    let record = installed(root, &second());
    let run = Run::admitted(root, 1);
    assert_eq!(
        run.ended(),
        (vec![Ok(record.identity().to_string())], 1),
        "with nothing in between"
    );
    assert!(alone(root, &record).is_some());
}

#[test]
fn a_retry_inside_an_admitted_run_whose_installation_was_replaced_is_refused_the_same_way() {
    // The run makes two requests. The first has its hold and reaches the meter. The
    // model is installed again under another revision before the retry: the retry
    // asks for the identity the admission recorded, is refused, and is not metered.
    let scratch = Scratch::new("leases-retry");
    let root = scratch.path();
    let old = installed(root, &second());
    let run = Run::admitted(root, 2);
    run.barriers.release("admitted");
    run.barriers.arrived("held", 1);
    assert_eq!(run.leases(), [Ok(old.identity().to_string())]);
    assert_eq!(run.metered(), 1);
    // While the first request holds it, the installation is superseded and kept.
    let new = installed(root, &entry_at("2026-11.3"));
    is_whole(root, &old);
    run.barriers.release("held");
    run.barriers.arrived("admitted", 2);
    let (leases, metered) = run.ended();
    assert_eq!(
        leases,
        [Ok(old.identity().to_string()), Err(HoldRefused::NotNamed)]
    );
    assert_eq!(metered, 1);
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
    // The request has its hold. A removal starts: it marks the model and waits.
    let barriers = Barriers::new();
    let clock = Arc::new(Hand::new());
    let remover = removal_on_a_thread(root, &barriers, &clock);
    barriers.arrived("try", 1);
    barriers.release("try");
    barriers.arrived("try", 2);
    assert!(marked(root));
    assert_eq!(reading(root), Current::Installed(record.clone()));
    is_whole(root, &record);
    // The request ends and lets its hold go: the removal goes on.
    let (leases, metered) = run.ended();
    assert_eq!((leases.len(), metered), (1, 1));
    barriers.open("try");
    let removed = remover.join().unwrap().unwrap();
    assert_eq!(removed.installation(), Some(record.identity()));
    assert_eq!(reading(root), Current::NotInstalled);
    assert!(installations(root).is_empty());
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
