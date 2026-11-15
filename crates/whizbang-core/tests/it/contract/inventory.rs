//! An inventory (ADR 0015; plan 1B, task I7, as far as it is built): what a roster
//! over an inventory prepares of a local model. Nothing here reads a disk: the fake
//! inventory is told what is installed.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use whizbang_core::capability::{
    Declaration, Installation, Installed, InstalledRoster, Inventory, NoInventory, Price, Roster,
    Unready,
};
use whizbang_core::contract::{
    Answer, Backends, Call, CallError, Capabilities, Capability, Engine, Estimate, Executor,
    ModelIdentity, Produced, Readiness, Refusal, Request, Requirement, Secrets, Stale, Value,
};
use whizbang_core::egress::{Origin, Recipient};
use whizbang_core::identity::Digest;
use whizbang_core::store::Reader;
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

// --- what is left as it was --------------------------------------------------------------------

#[test]
fn known_a_run_admitted_for_a_local_model_over_an_inventory_is_handed_no_backend_for_it() {
    // A pin, not a wish. An executor hands a run the backend that declares the model
    // its call resolved, by its backend, its model and its version (ADR 0009). Over an
    // inventory the call resolved the installation's identity as the version, which
    // no backend declares, and nothing yet hands a run the installation it was
    // admitted with: that is the lease's half of task I7. So the call is ready, its
    // run is admitted, and its action finds no handle: no request reaches a local
    // model that no lease holds.
    let world = World::new();
    let filler = Arc::new(FakeFiller::new(Paint::Inverse));
    let roster = whizbang_testkit::capabilities::roster(None, Some(filler.as_ref()), None);
    let inventory = Arc::new(FakeInventory::new());
    let installation = inventory.install("fake-filler", b"a filler, whole");
    let handed = Arc::new(AtomicUsize::new(0));
    let paints = action("paints", vec![Requirement::new(Capability::Fill)], {
        let handed = handed.clone();
        move |_, context| {
            if let Some(fill) = context.capabilities().fill() {
                handed.fetch_add(1, Ordering::SeqCst);
                fill.fill(&fill_request()).unwrap();
            }
            Ok(Produced::nothing())
        }
    });
    let held = Capabilities::none().with_fill(filler.clone());
    let actions = || vec![paints.clone()];
    let call = Call::new("paints");

    // The roster alone: the action is handed the filler, and asks it.
    let alone = engine(&world, roster.clone(), held.clone(), actions());
    let finished = alone.submit(&call).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!((handed.load(Ordering::SeqCst), filler.requests()), (1, 1));
    let (record, _) = stored(&world, finished.run());
    assert_eq!(record.models()[0].version, "1");
    // One executor writes a store at a time.
    drop(alone);

    // Over the inventory: ready, admitted, and handed nothing.
    let over = InstalledRoster::over(roster, inventory);
    let executor = engine(&world, over, held, actions());
    let prepared = executor.prepare(&call).unwrap();
    assert_eq!(prepared.readiness(), &Readiness::Ready);
    assert_eq!(
        prepared.needs()[0].model().unwrap().version(),
        installation.to_string()
    );
    let finished = executor.submit(&call).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!((handed.load(Ordering::SeqCst), filler.requests()), (1, 1));
    let (record, _) = stored(&world, finished.run());
    assert_eq!(record.models().len(), 0);
}
