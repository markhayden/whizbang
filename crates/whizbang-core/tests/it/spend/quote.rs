//! Quotes (plan 1B, task S4): a paid request has an enforceable bound, or it is not
//! sent. At admission, through an executor with the real ledger whose backends are a
//! roster under `spend::PerCall`; and the check one request is held to before it
//! leaves, asked directly.

use std::num::NonZeroU64;
use std::sync::Arc;

use whizbang_core::capability::{Declaration, Price, Roster, TimeLimit};
use whizbang_core::contract::{
    Backends, Call, CallError, Capability, Engine, Estimate, Executor, Made, ModelIdentity,
    NoBackends, Offered, Produced, Refusal, RefusalKind, Registry, Request, RequestId, Secrets,
    Snapshot, Stale, Unavailable,
};
use whizbang_core::egress::{Origin, Recipient};
use whizbang_core::identity::Digest;
use whizbang_core::spend::{NotQuoted, PerCall, StoreLedger, request_bound};
use whizbang_testkit::capabilities::{FakeFiller, Paint, roster};

use super::amount;
use super::support::{World, allow, ran, reserved};

fn times(requests: u64) -> NonZeroU64 {
    NonZeroU64::new(requests).unwrap()
}

/// An executor with `paint`, which needs a filler, the real ledger, and as its
/// backends a roster that offers one filler at this price, under `PerCall` for calls
/// that send a backend this many requests.
fn painting(world: &World, price: Price, requests: u64) -> Engine {
    let filler = FakeFiller::new(Paint::Inverse).priced(price);
    let backends = PerCall::new(Arc::new(roster(None, Some(&filler), None)), times(requests));
    let mut registry = Registry::new();
    registry.register(world.paints("paint")).unwrap();
    let digest = Digest::of_bytes(b"");
    Engine::over(registry, Arc::new(backends), world.store.clone(), digest)
        .with_config(world.source.clone())
        .with_clock(world.clock.clone())
        .with_ledger(Arc::new(StoreLedger))
}

fn asked(id: &str) -> Request {
    Request::new(Call::new("paint")).with_id(RequestId::parse(id).unwrap())
}

fn no_bound() -> CallError {
    CallError::Refused(Refusal::NoBound {
        action: "paint".to_owned(),
    })
}

fn at_most(micros: u32) -> Price {
    Price::at_most(f64::from(micros)).unwrap()
}

fn declared() -> Declaration {
    Declaration::new(ModelIdentity::new("fake", "m", "1"), &[]).unwrap()
}

#[test]
fn a_call_that_needs_a_backend_offering_only_an_estimate_is_refused_at_admission_with_no_run() {
    let world = World::new("quote-estimate-only");
    allow(&world.store, 1_000_000);
    let before = world.held();
    let engine = painting(&world, Price::about(250_000.0).unwrap(), 2);
    assert_eq!(engine.request(&asked("order-1")).unwrap_err(), no_bound());
    assert_eq!(
        engine.submit(&Call::new("paint")).unwrap_err(),
        no_bound(),
        "a started run is admitted the same way"
    );
    assert_eq!(
        world.held(),
        before,
        "no run, no reservation, and the id free"
    );
    assert_eq!(world.ran(), 0);
    // The id is free: at a price that bounds a request, the same request runs.
    ran(painting(&world, at_most(250_000), 2).request(&asked("order-1")));
    assert_eq!(world.ran(), 1);
}

#[test]
fn the_bound_of_a_need_covers_every_request_the_call_sends_its_backend() {
    let bound = |price: Price, requests: u64| {
        let filler = FakeFiller::new(Paint::Inverse).priced(price);
        let roster = Arc::new(roster(None, Some(&filler), None));
        let model = Backends::resolve(roster.as_ref(), Capability::Fill, None, &[]).unwrap();
        let per_call = PerCall::new(roster, times(requests));
        assert_eq!(
            per_call.resolve(Capability::Fill, None, &[]).unwrap(),
            model,
            "resolution is the roster's"
        );
        per_call.bound(Capability::Fill, &model)
    };
    assert_eq!(bound(at_most(250_000), 1), Estimate::of_micros(250_000));
    assert_eq!(bound(at_most(250_000), 3), Estimate::of_micros(750_000));
    assert_eq!(bound(Price::free(), 3), Estimate::of_micros(0));
    // No bound stays none, and a product that reaches the largest amount is none.
    assert!(bound(Price::about(1.0).unwrap(), 3).is_unbounded());
    let huge = Price::at_most(9_000_000_000_000_000_000.0).unwrap();
    assert!(!bound(huge, 2).is_unbounded());
    assert!(bound(huge, 3).is_unbounded());

    // Through an executor: a call that sends its backend three requests reserves
    // three times the price of one.
    let world = World::new("quote-per-call");
    allow(&world.store, 1_000_000);
    ran(painting(&world, at_most(250_000), 3).request(&asked("order-1")));
    assert_eq!(reserved(&world.store)[0].micros(), 750_000);
}

#[test]
fn the_check_refuses_a_request_to_a_backend_with_no_bound_and_one_whose_quote_is_over_what_its_run_has_left()
 {
    let origin = Origin::of("https://api.example.com", &Secrets::none()).unwrap();
    let remote = || declared().sending_to(Recipient::new(origin.clone()));

    // A bound within what the run has left is what the request is held to.
    let bounded = remote().priced(at_most(250_000));
    assert_eq!(
        request_bound(&bounded, Some(250_000)),
        Ok(Some(amount(250_000)))
    );
    // One over it is refused, with both numbers.
    let over = request_bound(&bounded, Some(249_999)).unwrap_err();
    assert_eq!(
        over,
        NotQuoted::OverRun {
            bound_micros: 250_000,
            left_micros: 249_999
        }
    );
    assert_eq!(over.kind(), RefusalKind::RequestOverAllowance);
    assert!(over.to_string().contains("250000") && over.to_string().contains("249999"));
    // A run nothing was reserved for is held to no amount.
    assert_eq!(request_bound(&bounded, None), Ok(Some(amount(250_000))));

    // An estimate only is no bound, whatever the run has left.
    let estimated = remote().priced(Price::about(1.0).unwrap());
    let none = request_bound(&estimated, Some(1_000_000)).unwrap_err();
    assert_eq!(
        none,
        NotQuoted::NoBound {
            estimated_micros: Some(1)
        }
    );
    assert_eq!(none.kind(), RefusalKind::RequestNoBound);
    assert_eq!(request_bound(&estimated, None), Err(none));
    // So is a price by time under a timeout of whizbang's own, and no price at all
    // from a backend that names a recipient.
    let timed = Price::by_time(1_000.0, TimeLimit::own_timeout(30_000).unwrap()).unwrap();
    assert_eq!(
        request_bound(&remote().priced(timed), Some(1_000_000)),
        Err(NotQuoted::NoBound {
            estimated_micros: Some(30_000)
        })
    );
    assert_eq!(
        request_bound(&remote(), Some(1_000_000)),
        Err(NotQuoted::NoBound {
            estimated_micros: None
        })
    );

    // Not paid: a local backend that states no price, and a backend that says it is
    // free, with a run that has nothing left.
    assert_eq!(request_bound(&declared(), Some(0)), Ok(None));
    assert_eq!(
        request_bound(&remote().priced(Price::free()), Some(0)),
        Ok(None)
    );
    // A bound of nothing is a bound: the request is paid, and held to nothing.
    assert_eq!(
        request_bound(&remote().priced(at_most(0)), Some(0)),
        Ok(Some(amount(0)))
    );
}

#[test]
fn a_price_that_changed_since_the_preparation_is_stale_at_admission() {
    let world = World::new("quote-stale");
    allow(&world.store, 1_000_000);
    let then = painting(&world, at_most(250_000), 1);
    let prepared = then.prepare(&Call::new("paint")).unwrap();
    assert_eq!(prepared.estimate(), Estimate::of_micros(250_000));
    let before = world.held();
    let request = asked("order-1").expecting(&prepared);
    for price in [300_000, 200_000] {
        let now = painting(&world, at_most(price), 1);
        assert_eq!(
            now.request(&request).unwrap_err(),
            CallError::Refused(Refusal::Stale {
                action: "paint".to_owned(),
                why: Stale::Estimate,
            })
        );
    }
    // So is the same price for a call that now sends its backend one request more.
    assert!(matches!(
        painting(&world, at_most(250_000), 2).request(&request),
        Err(CallError::Refused(Refusal::Stale {
            why: Stale::Estimate,
            ..
        }))
    ));
    assert_eq!((world.held(), world.ran()), (before, 0));
    // At the price it was prepared at, it is admitted.
    ran(then.request(&request));
    assert_eq!(reserved(&world.store)[0].micros(), 250_000);
}

#[test]
fn a_backend_priced_by_time_is_admitted_at_price_times_its_hosts_limit_and_refused_with_only_a_timeout_of_whizbangs_own()
 {
    let world = World::new("quote-by-time");
    allow(&world.store, 1_000_000);
    // 1,000 millionths a second: thirty seconds are 30,000.
    let own = Price::by_time(1_000.0, TimeLimit::own_timeout(30_000).unwrap()).unwrap();
    let before = world.held();
    assert_eq!(
        painting(&world, own, 1)
            .request(&asked("order-1"))
            .unwrap_err(),
        no_bound()
    );
    assert_eq!((world.held(), world.ran()), (before, 0));

    let host = Price::by_time(1_000.0, TimeLimit::of_host(30_000).unwrap()).unwrap();
    ran(painting(&world, host, 1).request(&asked("order-1")));
    let reservations = reserved(&world.store);
    assert_eq!(reservations.len(), 1);
    assert_eq!(reservations[0].micros(), 30_000);
    assert_eq!(world.ran(), 1);
}

/// What one snapshot offers, for a test: a roster of one filler, and that filler,
/// made, under the name the roster offers it by.
struct Offering {
    roster: Roster,
    filler: Arc<FakeFiller>,
}

impl Backends for Offering {
    fn resolve(
        &self,
        capability: Capability,
        choice: Option<&str>,
        features: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        Backends::resolve(&self.roster, capability, choice, features)
    }

    fn bound(&self, capability: Capability, model: &ModelIdentity) -> Estimate {
        Backends::bound(&self.roster, capability, model)
    }
}

impl Offered for Offering {
    fn default_of(&self, capability: Capability) -> Option<&str> {
        (capability == Capability::Fill).then_some("fake")
    }

    fn made(&self, capability: Capability, name: &str) -> Option<Made> {
        let offered = capability == Capability::Fill && name == "fake";
        offered.then(|| Made::Fill(self.filler.clone()))
    }
}

#[test]
fn under_a_snapshot_that_holds_backends_the_bound_of_a_need_covers_every_request_and_the_run_is_handed_the_snapshots_backend()
 {
    // An executor under a snapshot that holds backends asks those, and not the ones
    // it was built with (ADR 0015, "One snapshot of the config"): so the wrapper that
    // makes a need's bound cover every request stands around what the snapshot
    // offers, and still hands a run the backend the snapshot made.
    let world = World::new("quote-snapshot");
    allow(&world.store, 1_000_000);
    let filler = Arc::new(FakeFiller::new(Paint::Inverse).priced(at_most(250_000)));
    let offering: Arc<dyn Offered> = Arc::new(Offering {
        roster: roster(None, Some(filler.as_ref()), None),
        filler: filler.clone(),
    });
    let snapshot = Snapshot::of(Digest::of_bytes(b"a config")).offering(offering);
    let mut registry = Registry::new();
    let fills = world.action("paint", &[Capability::Fill], |_, context| {
        let fill = context.capabilities().fill().expect("a filler is handed");
        fill.fill(&super::meter::fill_request()).unwrap();
        Ok(Produced::nothing())
    });
    registry.register(fills).unwrap();
    let digest = Digest::of_bytes(b"");
    let engine = Engine::over(registry, Arc::new(NoBackends), world.store.clone(), digest)
        .with_config(world.source.clone())
        .with_clock(world.clock.clone())
        .with_ledger(Arc::new(StoreLedger));
    let estimate = |engine: &Engine| {
        let prepared = engine.prepare(&Call::new("paint")).unwrap();
        prepared.estimate().micros()
    };
    // As the snapshot offers them, the bound of a need is that of one request.
    world.source.swap(snapshot.clone());
    assert_eq!(estimate(&engine), 250_000);
    // Around what it offers, the wrapper makes it cover three; a snapshot that holds
    // no backends has nothing to put a wrapper around, and is as it was.
    let three = |offered: Arc<dyn Offered>| -> Arc<dyn Offered> {
        Arc::new(PerCall::new(offered, times(3)))
    };
    world.source.swap(snapshot.offering_through(three));
    assert_eq!(estimate(&engine), 750_000);
    let bare = Snapshot::of(Digest::of_bytes(b"a config")).offering_through(three);
    assert!(!bare.holds_backends());
    // The run reserves that, and is handed the filler the snapshot made.
    let finished = ran(engine.request(&asked("covered")));
    assert_eq!(finished.outcome().name(), "completed");
    let reserved = reserved(&world.store);
    assert_eq!(reserved.len(), 1);
    assert_eq!(reserved[0].micros(), 750_000);
    assert_eq!(filler.requests(), 1);
    // No bound stays no bound behind the wrapper, however it is held.
    let about = Arc::new(FakeFiller::new(Paint::Inverse).priced(Price::about(40.0).unwrap()));
    let offering: Arc<dyn Offered> = Arc::new(Offering {
        roster: roster(None, Some(about.as_ref()), None),
        filler: about,
    });
    let snapshot = Snapshot::of(Digest::of_bytes(b"another")).offering(offering);
    world.source.swap(snapshot.offering_through(three));
    assert_eq!(engine.request(&asked("unbounded")), Err(no_bound()));
}
