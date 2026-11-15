//! An inventory (ADR 0015; plan 1B, task I7): what a roster over an inventory
//! prepares of a local model, the backend a run admitted for one is handed, and what
//! the lease's place is told. Nothing here reads a disk: the fake
//! inventory is told what is installed.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use whizbang_core::capability::{
    Bounds, Declaration, Fault, FaultKind, FillRequest, Filler, Installation, Installed,
    InstalledRoster, Inventory, NoInventory, Painted, Place, Price, Roster, Stage, Unready,
};
use whizbang_core::contract::{
    Answer, Around, Backends, Call, CallError, Capabilities, Capability, Engine, Estimate,
    Executor, ModelIdentity, Open, Produced, Readiness, Refusal, RefusalKind, Request, Requirement,
    Secrets, Stale, Stop, Unavailable, Value, Wrapped,
};
use whizbang_core::egress::{Origin, Recipient};
use whizbang_core::identity::Digest;
use whizbang_core::store::Reader;
use whizbang_testkit::around::{FakeWrapper, Saw};
use whizbang_testkit::capabilities::{FakeFiller, Paint};
use whizbang_testkit::inventory::FakeInventory;

use super::asked::{action, engine, fill_request, stored};
use super::commit::World;

/// The model of the default judge, of the second, and of the one that is not local.
const TINY: &str = "tiny-judge";
const SMALL: &str = "small-judge";
const HOSTED: &str = "hosted-judge";

/// The four reasons, each with its words.
const REASONS: [(Unready, &str); 4] = [
    (Unready::NotInstalled, "it is not installed"),
    (Unready::OutOfDate, "its installation is out of date"),
    (Unready::Damaged, "its installation is damaged"),
    (Unready::NotInCatalogue, "it is not in the catalogue"),
];

fn local(model: &str, features: &[&str]) -> Declaration {
    Declaration::new(ModelIdentity::new("fake", model, "declared"), features).unwrap()
}

fn elsewhere() -> Recipient {
    Recipient::new(Origin::of("https://api.example.com", &Secrets::none()).unwrap())
}

/// Three judges: `here`, the default, and `second`, both local; and `away`, which
/// names a recipient.
fn roster() -> Roster {
    Roster::new()
        .offer(Capability::Judge, "here", local(TINY, &[]))
        .unwrap()
        .offer(Capability::Judge, "second", local(SMALL, &[]))
        .unwrap()
        .offer(
            Capability::Judge,
            "away",
            local(HOSTED, &[]).sending_to(elsewhere()),
        )
        .unwrap()
        .default_for(Capability::Judge, "here")
        .unwrap()
}

/// An executor over this roster and this inventory, with two actions that need a
/// judge the call may choose: `asks`, and `looks`, which needs the feature `image`.
/// The count is of how often either ran.
fn over(world: &World, inventory: Arc<dyn Inventory>) -> (Engine, Arc<AtomicUsize>) {
    let ran = Arc::new(AtomicUsize::new(0));
    let body = |ran: &Arc<AtomicUsize>| {
        let ran = ran.clone();
        move |_: &_, _: &_| {
            ran.fetch_add(1, Ordering::SeqCst);
            Ok(Produced::nothing())
        }
    };
    let judge = || Requirement::new(Capability::Judge).chosen_by("judge");
    let actions = vec![
        action("asks", vec![judge()], body(&ran)),
        action("looks", vec![judge().feature("image")], body(&ran)),
    ];
    let backends = InstalledRoster::over(roster(), inventory);
    let executor = engine(world, backends, Capabilities::none(), actions);
    (executor, ran)
}

fn choosing(action: &str, judge: &str) -> Call {
    Call::new(action).argument("judge", Value::text(judge))
}

/// How many runs the store holds.
fn runs(world: &World) -> usize {
    let reader = Reader::open(&world.root(), world.fs.clone()).unwrap();
    reader.snapshot().unwrap().runs().len()
}

/// Why the one capability the call needs is not answered.
#[track_caller]
fn why_not(executor: &Engine, call: &Call) -> String {
    let prepared = executor.prepare(call).unwrap();
    let Readiness::NotReady(missing) = prepared.readiness() else {
        panic!("the call is ready: {prepared:?}");
    };
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].capability(), Capability::Judge);
    assert!(prepared.needs()[0].model().is_none());
    missing[0].why().to_owned()
}

// --- not ready, with the reason ------------------------------------------------------------

#[test]
fn a_roster_over_the_fake_inventory_prepares_a_model_that_is_not_ready_as_not_ready_with_the_reason()
 {
    let world = World::new();
    let inventory = Arc::new(FakeInventory::new());
    let (executor, ran) = over(&world, inventory.clone());
    // The second judge is installed throughout: it never answers in place of the
    // default.
    inventory.install(SMALL, b"small, whole");
    for (why, words) in REASONS {
        inventory.fail(TINY, why);
        let said = why_not(&executor, &Call::new("asks"));
        assert_eq!(
            said,
            format!("the judge backend here is local, and its model is not ready: {words}")
        );
        // The same of the backend when the call names it.
        assert_eq!(why_not(&executor, &choosing("asks", "here")), said);
        // And the call is refused at admission, with that reason, and makes no run.
        let Err(CallError::Refused(Refusal::NotReady { action, missing })) =
            executor.submit(&Call::new("asks"))
        else {
            panic!("admitted, though its model is not ready: {why:?}");
        };
        assert_eq!(action, "asks");
        assert_eq!(missing[0].why(), said);
        // What is said names the backend as the roster offers it, and nothing of
        // the model.
        assert!(!said.contains(TINY), "{said}");
    }
    assert_eq!((runs(&world), ran.load(Ordering::SeqCst)), (0, 0));
    // Only the model of the backend that was picked was asked about.
    assert!(inventory.asked().iter().all(|model| model == TINY));
    assert!(!inventory.asked().is_empty());
}

#[test]
fn a_model_the_inventory_was_told_nothing_of_is_not_in_the_catalogue_and_is_never_ready() {
    let world = World::new();
    let inventory = Arc::new(FakeInventory::new());
    let (executor, _) = over(&world, inventory.clone());
    let unknown = "the judge backend here is local, and its model is not ready: it is not in \
                   the catalogue";
    assert_eq!(why_not(&executor, &Call::new("asks")), unknown);
    // Installed, it is ready; forgotten, it is as it was.
    inventory.install(TINY, b"tiny, whole");
    let prepared = executor.prepare(&Call::new("asks")).unwrap();
    assert_eq!(prepared.readiness(), &Readiness::Ready);
    inventory.forget(TINY);
    assert_eq!(why_not(&executor, &Call::new("asks")), unknown);
}

#[test]
fn under_the_default_inventory_no_local_model_is_ready_and_a_backend_that_is_not_local_is_as_it_was()
 {
    let world = World::new();
    let (executor, ran) = over(&world, Arc::new(NoInventory));
    for call in [Call::new("asks"), choosing("asks", "second")] {
        let said = why_not(&executor, &call);
        assert!(
            said.ends_with("is local, and its model is not ready: it is not in the catalogue"),
            "{said}"
        );
    }
    assert_eq!(
        NoInventory.look(TINY),
        Installed::NotReady(Unready::NotInCatalogue)
    );
    // The one that names a recipient is ready, as its roster alone prepares it.
    let away = choosing("asks", "away");
    let prepared = executor.prepare(&away).unwrap();
    assert_eq!(prepared.readiness(), &Readiness::Ready);
    let alone = Backends::resolve(&roster(), Capability::Judge, Some("away"), &[]).unwrap();
    assert_eq!(prepared.needs()[0].model(), Some(&alone));
    assert_eq!(
        executor.submit(&away).unwrap().outcome().name(),
        "completed"
    );
    assert_eq!(ran.load(Ordering::SeqCst), 1);
}

// --- ready, under the identity of the installation ---------------------------------------------

#[test]
fn a_local_model_that_is_ready_is_prepared_under_the_identity_of_its_installation() {
    let world = World::new();
    let inventory = Arc::new(FakeInventory::new());
    let (executor, ran) = over(&world, inventory.clone());
    let installation = inventory.install(TINY, b"tiny, whole");
    assert_eq!(
        installation,
        Installation::of(Digest::of_bytes(b"tiny, whole"))
    );
    let prepared = executor.prepare(&Call::new("asks")).unwrap();
    assert_eq!(prepared.readiness(), &Readiness::Ready);
    // The backend and the model are as declared; the version is the installation's
    // identity, and not the version the backend declared.
    let identity = Digest::of_bytes(b"tiny, whole").to_string();
    assert_eq!(
        prepared.needs()[0].model(),
        Some(&ModelIdentity::new("fake", TINY, &identity))
    );
    assert_eq!(inventory.asked(), [TINY]);
    // The call is admitted and runs.
    let finished = executor.submit(&Call::new("asks")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!((runs(&world), ran.load(Ordering::SeqCst)), (1, 1));
}

#[test]
fn an_identity_is_a_digest_as_text_and_is_a_version_a_declaration_takes() {
    let digest = Digest::of_bytes(b"an installation");
    let installation = Installation::of(digest);
    assert_eq!(installation.identity(), digest);
    let text = installation.to_string();
    assert_eq!(text, digest.to_string());
    assert_eq!(text.len(), 71);
    assert!(text.starts_with("sha256:"));
    Declaration::new(ModelIdentity::new("fake", TINY, &text), &[]).unwrap();
    // Each reason is fixed words.
    for (why, words) in REASONS {
        assert_eq!(why.to_string(), words);
    }
}

#[test]
fn an_installation_replaced_or_removed_under_a_prepared_call_makes_the_preparation_stale() {
    let world = World::new();
    let inventory = Arc::new(FakeInventory::new());
    let (executor, ran) = over(&world, inventory.clone());
    inventory.install(TINY, b"the first");
    let call = Call::new("asks");
    let first = executor.prepare(&call).unwrap();
    let stale = CallError::Refused(Refusal::Stale {
        action: "asks".to_owned(),
        why: Stale::Model {
            capability: Capability::Judge,
        },
    });
    let asked_with = |prepared| executor.request(&Request::new(call.clone()).expecting(prepared));

    // Installed again, from other bytes: another installation.
    inventory.install(TINY, b"the second");
    let second = executor.prepare(&call).unwrap();
    assert_ne!(first.fingerprint(), second.fingerprint());
    assert_eq!(asked_with(&first).unwrap_err(), stale);
    // Removed, or damaged: stale as well, said before the call is said not to be
    // ready.
    for why in [Unready::NotInstalled, Unready::Damaged] {
        inventory.fail(TINY, why);
        assert_eq!(asked_with(&second).unwrap_err(), stale);
    }
    assert_eq!((runs(&world), ran.load(Ordering::SeqCst)), (0, 0));
    // The first installation back, the first preparation is of what is there, and is
    // admitted.
    inventory.install(TINY, b"the first");
    assert_eq!(
        executor.prepare(&call).unwrap().fingerprint(),
        first.fingerprint()
    );
    assert!(matches!(asked_with(&first), Ok(Answer::Ran(_))));
    assert_eq!((runs(&world), ran.load(Ordering::SeqCst)), (1, 1));
}

// --- who is asked, and of what ---------------------------------------------------------------

#[test]
fn a_backend_that_names_a_recipient_is_not_asked_of_the_inventory() {
    let world = World::new();
    let inventory = Arc::new(FakeInventory::new());
    let (executor, _) = over(&world, inventory.clone());
    // The inventory says this model is damaged: nobody asks it.
    inventory.fail(HOSTED, Unready::Damaged);
    let prepared = executor.prepare(&choosing("asks", "away")).unwrap();
    assert_eq!(prepared.readiness(), &Readiness::Ready);
    assert_eq!(
        prepared.needs()[0].model(),
        Some(&ModelIdentity::new("fake", HOSTED, "declared"))
    );
    assert_eq!(inventory.asked(), [""; 0]);
}

#[test]
fn the_inventory_is_asked_about_the_model_of_the_backend_the_call_resolved_and_no_other() {
    let world = World::new();
    let inventory = Arc::new(FakeInventory::new());
    let (executor, _) = over(&world, inventory.clone());
    inventory.install(TINY, b"tiny, whole");
    let small = inventory.install(SMALL, b"small, whole");
    let prepared = executor.prepare(&choosing("asks", "second")).unwrap();
    assert_eq!(
        prepared.needs()[0].model().unwrap().version(),
        small.to_string()
    );
    assert_eq!(inventory.asked(), [SMALL]);
    executor.prepare(&Call::new("asks")).unwrap();
    assert_eq!(inventory.asked(), [SMALL, TINY]);
}

#[test]
fn what_the_roster_refuses_is_said_as_the_roster_says_it_and_the_inventory_is_not_asked() {
    let world = World::new();
    let inventory = Arc::new(FakeInventory::new());
    let (executor, _) = over(&world, inventory.clone());
    inventory.install(TINY, b"tiny, whole");
    assert_eq!(
        why_not(&executor, &choosing("asks", "gone")),
        "no judge backend is named \"gone\""
    );
    assert_eq!(
        why_not(&executor, &Call::new("looks")),
        "the judge backend here does not declare the feature \"image\""
    );
    assert_eq!(inventory.asked(), [""; 0]);
}

// --- what a request may cost -------------------------------------------------------------------

#[test]
fn a_local_backend_that_states_a_price_is_bound_by_it_under_its_installations_identity() {
    let price = |micros: f64| Price::at_most(micros).unwrap();
    let roster = Roster::new()
        .offer(
            Capability::Fill,
            "here",
            local("tiny-fill", &[]).priced(price(300.0)),
        )
        .unwrap()
        // The same backend and model once more, declared under another version.
        .offer(
            Capability::Fill,
            "again",
            Declaration::new(ModelIdentity::new("fake", "tiny-fill", "older"), &[])
                .unwrap()
                .priced(price(200.0)),
        )
        .unwrap()
        .offer(
            Capability::Fill,
            "other",
            local("other-fill", &[]).priced(price(900.0)),
        )
        .unwrap()
        .offer(
            Capability::Fill,
            "away",
            local("hosted-fill", &[])
                .sending_to(elsewhere())
                .priced(price(500.0)),
        )
        .unwrap()
        .offer(
            Capability::Segment,
            "here",
            local("tiny-fill", &[]).priced(price(700.0)),
        )
        .unwrap();
    let inventory = Arc::new(FakeInventory::new());
    inventory.install("tiny-fill", b"tiny, whole");
    let over = InstalledRoster::over(roster.clone(), inventory.clone());
    assert_eq!(over.roster(), &roster);

    // The local model, as it is prepared: under its installation's identity.
    let installed = over.resolve(Capability::Fill, Some("here"), &[]).unwrap();
    assert_ne!(installed.version(), "declared");
    assert_eq!(
        over.bound(Capability::Fill, &installed),
        Estimate::of_micros(300)
    );
    // Two local backends declare that backend and that model: the larger of the two.
    // The roster alone knows no backend that declares that version.
    assert_eq!(
        roster.bound(Capability::Fill, &installed),
        Estimate::default()
    );
    // The bound is that of its capability and of its model, and of no other.
    assert_eq!(
        over.bound(Capability::Segment, &installed),
        Estimate::of_micros(700)
    );
    assert_eq!(
        over.bound(Capability::Judge, &installed),
        Estimate::default()
    );
    let another = ModelIdentity::new("another", "tiny-fill", installed.version());
    assert_eq!(over.bound(Capability::Fill, &another), Estimate::default());
    let unknown = ModelIdentity::new("fake", "no-fill", installed.version());
    assert_eq!(over.bound(Capability::Fill, &unknown), Estimate::default());

    // A backend that names a recipient is bound as its roster binds it: by the
    // model it declares, version and all.
    let hosted = over.resolve(Capability::Fill, Some("away"), &[]).unwrap();
    assert_eq!(hosted.version(), "declared");
    assert_eq!(
        over.bound(Capability::Fill, &hosted),
        Estimate::of_micros(500)
    );
    let moved = ModelIdentity::new("fake", "hosted-fill", installed.version());
    assert_eq!(over.bound(Capability::Fill, &moved), Estimate::default());
    // Asking for a bound asks the inventory nothing.
    assert_eq!(inventory.asked(), ["tiny-fill"]);
}

// --- a run is handed its backend, and the lease's place ----------------------------------------

/// What each request to the filler was answered with, as the action saw it: nothing
/// for an answer, the fault's kind and stage otherwise.
type Answers = Arc<Mutex<Vec<Option<(FaultKind, Stage)>>>>;

/// An executor over a roster that offers this filler as `here`, and over this
/// inventory, with these wrappers in the lease's place and the meter's. Its one
/// action, `paints`, runs `first` and then asks the filler it was handed once, when
/// it was handed one.
fn leased(
    world: &World,
    filler: Arc<dyn Filler>,
    inventory: &Arc<FakeInventory>,
    (lease, meter): (&FakeWrapper, &FakeWrapper),
    first: impl Fn() + Send + Sync + 'static,
) -> (Engine, Answers) {
    let answers = Answers::default();
    let paints = action("paints", vec![Requirement::new(Capability::Fill)], {
        let answers = answers.clone();
        move |_, context| {
            first();
            if let Some(fill) = context.capabilities().fill() {
                let answered = fill.fill(&fill_request());
                let fault = answered.err().map(|fault| (fault.kind(), fault.stage()));
                answers.lock().unwrap().push(fault);
            }
            Ok(Produced::nothing())
        }
    });
    let roster = Roster::new()
        .offer(Capability::Fill, "here", filler.declaration().clone())
        .unwrap()
        .default_for(Capability::Fill, "here")
        .unwrap();
    let over = InstalledRoster::over(roster, inventory.clone());
    let held = Capabilities::none().with_fill(filler);
    let executor = engine(world, over, held, vec![paints])
        .with_wrapper(Place::Lease, Arc::new(lease.clone()))
        .with_wrapper(Place::Meter, Arc::new(meter.clone()));
    (executor, answers)
}

/// A wrapper for the lease's place and one for the meter's, with one journal.
fn places() -> (FakeWrapper, FakeWrapper) {
    let lease = FakeWrapper::new("lease");
    let meter = lease.beside("meter");
    (lease, meter)
}

/// The version a wrapper was told the backend declares, for each request it saw.
fn versions(wrapper: &FakeWrapper) -> Vec<String> {
    let requests = wrapper.requests();
    requests.into_iter().map(|asked| asked.model.2).collect()
}

#[test]
fn a_run_admitted_for_a_local_model_over_an_inventory_is_handed_its_backend_under_the_identity_the_admission_recorded()
 {
    // The pin of the task's first half, turned round. The call resolved the
    // installation's identity as its model's version, which no backend declares:
    // the run is handed the local backend of that model, declaring that identity.
    let world = World::new();
    let filler = Arc::new(FakeFiller::new(Paint::Inverse));
    assert_eq!(filler.declaration().model().version(), "1");
    let inventory = Arc::new(FakeInventory::new());
    let installation = inventory.install("fake-filler", b"a filler, whole");
    let (lease, meter) = places();
    let (executor, answers) = leased(&world, filler.clone(), &inventory, (&lease, &meter), || {});
    let finished = executor.submit(&Call::new("paints")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    // Handed, asked and answered.
    assert_eq!(*answers.lock().unwrap(), [None]);
    assert_eq!(filler.requests(), 1);
    // The lease's place and the meter's are each told the backend under the
    // installation's identity, in the order of ADR 0015.
    let names: Vec<&str> = lease.journal().iter().map(|(name, _)| *name).collect();
    assert_eq!(names, ["lease", "meter", "meter", "lease"]);
    assert_eq!(versions(&lease), [installation.to_string()]);
    assert_eq!(versions(&meter), [installation.to_string()]);
    let [asked] = lease.requests().try_into().unwrap();
    assert_eq!(
        (asked.model.0.as_str(), asked.model.1.as_str()),
        ("fake", "fake-filler")
    );
    // And the run's record names the model under it.
    let (record, _) = stored(&world, finished.run());
    let [model] = record.models() else {
        panic!("{:?}", record.models());
    };
    assert_eq!(model.version, installation.to_string());
}

#[test]
fn a_request_whose_lease_is_refused_reaches_neither_the_meters_place_nor_the_backend() {
    let world = World::new();
    let filler = Arc::new(FakeFiller::new(Paint::Inverse));
    let inventory = Arc::new(FakeInventory::new());
    inventory.install("fake-filler", b"a filler, whole");
    let (lease, meter) = places();
    lease.refuse(RefusalKind::RequestNotReady, "the model is being removed");
    let (executor, answers) = leased(&world, filler.clone(), &inventory, (&lease, &meter), || {});
    let finished = executor.submit(&Call::new("paints")).unwrap();
    // The action is answered a fault that says nothing left; the meter's place
    // and the backend saw nothing; and the refusal is noted in the run's record.
    assert_eq!(
        *answers.lock().unwrap(),
        [Some((FaultKind::Rejected, Stage::NotSent))]
    );
    assert_eq!(meter.saw(), []);
    assert_eq!(filler.requests(), 0);
    assert!(matches!(lease.saw().as_slice(), [Saw::Before(_)]));
    let (record, _) = stored(&world, finished.run());
    let refusals = &record.outcome().refusals;
    assert_eq!(refusals.len(), 1);
    assert_eq!(
        (refusals[0].kind(), refusals[0].why()),
        (RefusalKind::RequestNotReady, "the model is being removed")
    );
}

/// A filler that panics when it is asked.
struct Panicking(Declaration);

impl Filler for Panicking {
    fn declaration(&self) -> &Declaration {
        &self.0
    }

    fn fill(&self, _: &FillRequest, _: &Bounds) -> Result<Painted, Fault> {
        panic!("the backend went wrong")
    }
}

#[test]
fn a_lease_is_let_go_when_its_request_ends_by_an_answer_a_fault_and_a_panic() {
    // What the lease's place holds for a request is told how the request ended,
    // by an answer and by a fault, and is dropped when the backend panics: the
    // fake counts each.
    let declared = FakeFiller::new(Paint::Inverse).declaration().clone();
    let fillers: [(&str, Arc<dyn Filler>); 3] = [
        ("an answer", Arc::new(FakeFiller::new(Paint::Inverse))),
        (
            "a fault",
            Arc::new(FakeFiller::new(Paint::Inverse).failing_at(Stage::Unresolved)),
        ),
        ("a panic", Arc::new(Panicking(declared))),
    ];
    for (how, filler) in fillers {
        let world = World::new();
        let inventory = Arc::new(FakeInventory::new());
        inventory.install("fake-filler", b"a filler, whole");
        let (lease, meter) = places();
        let (executor, _) = leased(&world, filler, &inventory, (&lease, &meter), || {});
        let ended = executor.submit(&Call::new("paints"));
        let saw = lease.saw();
        assert_eq!(saw.len(), 2, "{how}: {saw:?}");
        assert!(matches!(saw[0], Saw::Before(_)), "{how}");
        match how {
            "an answer" => assert_eq!(saw[1], Saw::Answered(None)),
            "a fault" => assert!(matches!(saw[1], Saw::Faulted(_, Stage::Unresolved))),
            // Dropped, and never told: the run is abandoned (ADR 0015).
            _ => {
                assert_eq!(saw[1], Saw::Dropped);
                assert!(
                    matches!(ended, Err(CallError::Abandoned { .. })),
                    "{ended:?}"
                );
            }
        }
    }
}

/// What stands in a place and panics before a request leaves.
struct PanicsBefore;

impl Around for PanicsBefore {
    fn before(&self, _: &Wrapped<'_>) -> Result<Box<dyn Open>, Stop> {
        panic!("the meter went wrong")
    }
}

#[test]
fn a_lease_is_let_go_when_the_place_after_it_panics_before_the_request_leaves() {
    // The lease's place let the request by and holds something for it; the
    // meter's place then panics. What the lease's place held is dropped, never
    // told how the request ended, and the backend is not asked.
    let world = World::new();
    let filler = Arc::new(FakeFiller::new(Paint::Inverse));
    let inventory = Arc::new(FakeInventory::new());
    inventory.install("fake-filler", b"a filler, whole");
    let (lease, meter) = places();
    let (executor, _) = leased(&world, filler.clone(), &inventory, (&lease, &meter), || {});
    let executor = executor.with_wrapper(Place::Meter, Arc::new(PanicsBefore));
    let _ = executor.submit(&Call::new("paints"));
    let saw = lease.saw();
    assert!(
        matches!(saw.as_slice(), [Saw::Before(_), Saw::Dropped]),
        "{saw:?}"
    );
    assert_eq!(filler.requests(), 0);
}

#[test]
fn the_lease_is_asked_for_the_identity_the_admission_recorded_not_the_one_current_at_the_request() {
    // Between the admission and the request the model is installed again, as
    // another installation. The lease's place is told the one the run was
    // admitted with; the next run is admitted with the new one.
    let world = World::new();
    let filler = Arc::new(FakeFiller::new(Paint::Inverse));
    let inventory = Arc::new(FakeInventory::new());
    let admitted = inventory.install("fake-filler", b"a filler, whole");
    let (lease, meter) = places();
    let again = inventory.clone();
    let (executor, _) = leased(&world, filler, &inventory, (&lease, &meter), move || {
        again.install("fake-filler", b"another installation");
    });
    executor.submit(&Call::new("paints")).unwrap();
    assert_eq!(versions(&lease), [admitted.to_string()]);
    let Installed::Ready(current) = inventory.look("fake-filler") else {
        panic!("not ready");
    };
    assert_ne!(current, admitted);
    executor.submit(&Call::new("paints")).unwrap();
    assert_eq!(
        versions(&lease),
        [admitted.to_string(), current.to_string()]
    );
}

/// Backends that resolve every need as one model, whatever is asked.
struct Always(ModelIdentity);

impl Backends for Always {
    fn resolve(
        &self,
        _: Capability,
        _: Option<&str>,
        _: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        Ok(self.0.clone())
    }
}

#[test]
fn a_backend_that_names_a_recipient_is_never_handed_to_a_run_resolved_under_an_installation() {
    // A backend that names a recipient declares the very three texts a local
    // model was resolved under: its backend, its model, and the identity of the
    // installation as its version.
    let installation = Installation::of(Digest::of_bytes(b"a filler, whole"));
    let resolved = ModelIdentity::new("fake", "fake-filler", &installation.to_string());
    let away = || {
        let declared = Declaration::new(resolved.clone(), &[]).unwrap();
        Arc::new(FakeFiller::new(Paint::Inverse).declared_as(declared.sending_to(elsewhere())))
    };
    let handed = Arc::new(AtomicUsize::new(0));
    let paints = || {
        let handed = handed.clone();
        action(
            "paints",
            vec![Requirement::new(Capability::Fill)],
            move |_, context| {
                if let Some(fill) = context.capabilities().fill() {
                    handed.fetch_add(1, Ordering::SeqCst);
                    let _ = fill.fill(&fill_request());
                }
                Ok(Produced::nothing())
            },
        )
    };
    // Alone among the executor's backends: the run is handed nothing.
    let world = World::new();
    let remote = away();
    let held = Capabilities::none().with_fill(remote.clone());
    let executor = engine(&world, Always(resolved.clone()), held, vec![paints()]);
    let finished = executor.submit(&Call::new("paints")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!((handed.load(Ordering::SeqCst), remote.requests()), (0, 0));
    drop(executor);
    // Before the local backend of that model: the run is handed the local one.
    let world = World::new();
    let (remote, local) = (away(), Arc::new(FakeFiller::new(Paint::Inverse)));
    let held = Capabilities::none()
        .with_fill(remote.clone())
        .with_fill(local.clone());
    let lease = FakeWrapper::new("lease");
    let executor = engine(&world, Always(resolved.clone()), held, vec![paints()])
        .with_wrapper(Place::Lease, Arc::new(lease.clone()));
    executor.submit(&Call::new("paints")).unwrap();
    assert_eq!((remote.requests(), local.requests()), (0, 1));
    assert_eq!(versions(&lease), [installation.to_string()]);
    // A version that is no installation's identity is matched as it always was,
    // by all three: the backend that names a recipient is handed, and a local one
    // that declares another version is not.
    let plain = ModelIdentity::new("fake", "fake-filler", "2");
    let declared = Declaration::new(plain.clone(), &[]).unwrap();
    let hosted =
        Arc::new(FakeFiller::new(Paint::Inverse).declared_as(declared.sending_to(elsewhere())));
    let local = Arc::new(FakeFiller::new(Paint::Inverse));
    let world = World::new();
    let held = Capabilities::none()
        .with_fill(local.clone())
        .with_fill(hosted.clone());
    let before = handed.load(Ordering::SeqCst);
    let executor = engine(&world, Always(plain), held, vec![paints()]);
    executor.submit(&Call::new("paints")).unwrap();
    assert_eq!(handed.load(Ordering::SeqCst), before + 1);
    assert_eq!(local.requests(), 0);
}

#[test]
fn a_run_resolved_under_an_installation_is_handed_only_the_local_backend_of_that_backend_and_model()
{
    // The match by identity is by the backend and the model, both: a local
    // backend of another model, and one of another backend with a model of the
    // same name, are not handed, alone or before the one that is meant.
    let installation = Installation::of(Digest::of_bytes(b"a filler, whole"));
    let resolved = ModelIdentity::new("fake", "fake-filler", &installation.to_string());
    let local = |backend: &str, model: &str| {
        let declared = Declaration::new(ModelIdentity::new(backend, model, "1"), &[]).unwrap();
        Arc::new(FakeFiller::new(Paint::Inverse).declared_as(declared))
    };
    let paints = || {
        action(
            "paints",
            vec![Requirement::new(Capability::Fill)],
            |_, context| {
                if let Some(fill) = context.capabilities().fill() {
                    let _ = fill.fill(&fill_request());
                }
                Ok(Produced::nothing())
            },
        )
    };
    let (other_model, other_backend) =
        (local("fake", "other-filler"), local("other", "fake-filler"));
    let others = || {
        Capabilities::none()
            .with_fill(other_model.clone())
            .with_fill(other_backend.clone())
    };
    let world = World::new();
    let executor = engine(&world, Always(resolved.clone()), others(), vec![paints()]);
    executor.submit(&Call::new("paints")).unwrap();
    assert_eq!((other_model.requests(), other_backend.requests()), (0, 0));
    drop(executor);
    let world = World::new();
    let meant = local("fake", "fake-filler");
    let held = others().with_fill(meant.clone());
    let executor = engine(&world, Always(resolved), held, vec![paints()]);
    executor.submit(&Call::new("paints")).unwrap();
    assert_eq!(
        (
            other_model.requests(),
            other_backend.requests(),
            meant.requests()
        ),
        (0, 0, 1)
    );
}

#[test]
fn a_roster_over_an_inventory_does_not_resolve_a_backend_that_names_a_recipient_and_declares_an_identity()
 {
    // Such a backend would be admitted and handed nothing: it is not ready, and
    // the reason says why.
    let installation = Installation::of(Digest::of_bytes(b"a filler, whole"));
    let declared = ModelIdentity::new("fake", "hosted-fill", &installation.to_string());
    let declaration = Declaration::new(declared, &[]).unwrap();
    let roster = Roster::new()
        .offer(
            Capability::Fill,
            "away",
            declaration.sending_to(elsewhere()),
        )
        .unwrap()
        .default_for(Capability::Fill, "away")
        .unwrap();
    let inventory = Arc::new(FakeInventory::new());
    let over = InstalledRoster::over(roster, inventory.clone());
    let refused = over.resolve(Capability::Fill, None, &[]).unwrap_err();
    assert_eq!(
        refused.why(),
        "the fill backend away names a recipient, and declares the identity of an \
         installation as its version"
    );
    assert!(inventory.asked().is_empty());
    assert_eq!(
        Installation::from_version(&installation.to_string()),
        Some(installation)
    );
    for other in ["1", "sha256:abc", "SHA256:00", ""] {
        assert_eq!(Installation::from_version(other), None, "{other:?}");
    }
}
