//! Quotes (plan 1B, task S4): a paid request has an enforceable bound, or it is not
//! sent. At admission, through an executor with the real ledger whose backends are a
//! roster, with an action whose need states how many requests it is for; and the
//! check one request is held to before it leaves, asked directly.

use std::sync::Arc;

use whizbang_core::capability::{Declaration, Price, Roster, TimeLimit};
use whizbang_core::contract::{
    Backends, Call, CallError, Capability, Engine, Estimate, Executor, Made, ModelIdentity,
    NoBackends, Offered, Produced, Refusal, RefusalKind, Registry, Request, RequestId, Requirement,
    Secrets, Snapshot, Stale, Unavailable,
};
use whizbang_core::egress::{Origin, Recipient};
use whizbang_core::identity::Digest;
use whizbang_core::spend::{NotQuoted, StoreLedger, request_bound};
use whizbang_testkit::capabilities::{FakeFiller, Paint, roster};

use super::amount;
use super::support::{World, allow, ran, reserved, stating};

/// `paint`: an action that does nothing, whose one need is a filler, for this many
/// requests.
fn paints(world: &World, requests: u64) -> Arc<dyn whizbang_core::contract::Action> {
    let need = Requirement::new(Capability::Fill).requests(requests);
    stating(&world.runs, "paint", vec![need], |_, _| {
        Ok(Produced::nothing())
    })
}

/// An executor with `paint`, which needs a filler for this many requests, the real
/// ledger, and as its backends a roster that offers one filler at this price.
fn painting(world: &World, price: Price, requests: u64) -> Engine {
    let filler = FakeFiller::new(Paint::Inverse).priced(price);
    let backends = roster(None, Some(&filler), None);
    let mut registry = Registry::new();
    registry.register(paints(world, requests)).unwrap();
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
fn the_bound_of_a_need_covers_every_request_the_need_states_and_one_where_it_states_none() {
    // What a call is prepared with: the bound of one request, which is what the
    // roster answers, times the requests the action's need is for.
    let world = World::new("quote-per-need");
    let estimate = |price: Price, requests: u64| {
        let prepared = painting(&world, price, requests).prepare(&Call::new("paint"));
        prepared.unwrap().estimate()
    };
    assert_eq!(estimate(at_most(250_000), 1), Estimate::of_micros(250_000));
    assert_eq!(estimate(at_most(250_000), 3), Estimate::of_micros(750_000));
    assert_eq!(estimate(Price::free(), 3), Estimate::of_micros(0));
    // No bound stays none, and a product that reaches the largest amount is none.
    assert!(estimate(Price::about(1.0).unwrap(), 3).is_unbounded());
    let huge = Price::at_most(9_000_000_000_000_000_000.0).unwrap();
    assert!(!estimate(huge, 2).is_unbounded());
    assert!(estimate(huge, 3).is_unbounded());
    // A need that says nothing of it is for one request, and says so.
    let need = Requirement::new(Capability::Fill);
    assert_eq!(need.sends(), 1);
    assert_eq!(need.requests(3).sends(), 3);

    // Through an executor: a call whose need is for three requests reserves three
    // times the price of one, and one whose need is for one reserves one, under an
    // allowance that covers no more.
    allow(&world.store, 1_000_000);
    ran(painting(&world, at_most(250_000), 3).request(&asked("order-1")));
    assert_eq!(reserved(&world.store)[0].micros(), 750_000);
    allow(&world.store, 250_000);
    ran(painting(&world, at_most(250_000), 1).request(&asked("order-2")));
    assert_eq!(reserved(&world.store)[1].micros(), 250_000);
}

#[test]
fn the_estimate_of_a_call_is_the_sum_of_its_needs_each_times_its_own_requests() {
    // Two needs of one call, each for its own number of requests: a filler at a
    // quarter for two, and a segmenter at a tenth for three.
    let world = World::new("quote-needs");
    let filler = FakeFiller::new(Paint::Inverse).priced(at_most(250_000));
    let segmenter = whizbang_testkit::capabilities::FakeSegmenter::new().priced(at_most(100_000));
    let needs = vec![
        Requirement::new(Capability::Fill).requests(2),
        Requirement::new(Capability::Segment).requests(3),
    ];
    let both = stating(&world.runs, "both", needs, |_, _| Ok(Produced::nothing()));
    let mut registry = Registry::new();
    registry.register(both).unwrap();
    let backends = roster(Some(&segmenter), Some(&filler), None);
    let digest = Digest::of_bytes(b"");
    let engine = Engine::over(registry, Arc::new(backends), world.store.clone(), digest);
    let prepared = engine.prepare(&Call::new("both")).unwrap();
    assert_eq!(prepared.estimate(), Estimate::of_micros(800_000));
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
    // it was built with (ADR 0015, "One snapshot of the config"): the bound of a need
    // is made of what the snapshot's backends answer, and the run is handed the
    // backend the snapshot made.
    let world = World::new("quote-snapshot");
    allow(&world.store, 1_000_000);
    let filler = Arc::new(FakeFiller::new(Paint::Inverse).priced(at_most(250_000)));
    let offering: Arc<dyn Offered> = Arc::new(Offering {
        roster: roster(None, Some(filler.as_ref()), None),
        filler: filler.clone(),
    });
    let snapshot = Snapshot::of(Digest::of_bytes(b"a config")).offering(offering);
    let mut registry = Registry::new();
    let need = Requirement::new(Capability::Fill).requests(3);
    let fills = stating(&world.runs, "paint", vec![need], |_, context| {
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
    // The bound of the need is that of one request, as the snapshot's backends answer
    // it, times the three requests the need is for.
    world.source.swap(snapshot.clone());
    assert_eq!(estimate(&engine), 750_000);
    // The run reserves that, and is handed the filler the snapshot made.
    let finished = ran(engine.request(&asked("covered")));
    assert_eq!(finished.outcome().name(), "completed");
    let reserved = reserved(&world.store);
    assert_eq!(reserved.len(), 1);
    assert_eq!(reserved[0].micros(), 750_000);
    assert_eq!(filler.requests(), 1);
    // No bound stays no bound, however many requests the need is for.
    let about = Arc::new(FakeFiller::new(Paint::Inverse).priced(Price::about(40.0).unwrap()));
    let offering: Arc<dyn Offered> = Arc::new(Offering {
        roster: roster(None, Some(about.as_ref()), None),
        filler: about,
    });
    let snapshot = Snapshot::of(Digest::of_bytes(b"another")).offering(offering);
    world.source.swap(snapshot);
    assert_eq!(engine.request(&asked("unbounded")), Err(no_bound()));
}
