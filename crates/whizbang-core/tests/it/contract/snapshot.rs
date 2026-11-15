//! One snapshot of the config (ADR 0015; plan 1B, task I6): what the executor asks of
//! its config's source, what a run is admitted under, and what it goes on under
//! whatever the source answers afterwards. No file is read by the executor: a test
//! makes each snapshot, from a config and an env file it writes as text, or by hand.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use whizbang_core::capability::{Declaration, Fault, FaultKind, Stage};
use whizbang_core::config::{Config, EnvFile, Kind, KindRefused, Kinds, Offers, Table};
use whizbang_core::contract::record::{Handed, Status};
use whizbang_core::contract::{
    AdmissionPart, Answer, Asking, Backends, Call, CallError, Capabilities, Capability,
    ConfigSource, Engine, Estimate, Executor, Failure, Made, ModelIdentity, NoBackends, NotJudged,
    Offered, PartDeclined, PartStep, Produced, Readiness, Refusal, RefusalKind, Request,
    Requirement, SECRET_ORIGIN_APPROVED, SECRETS_KEPT_LIMIT, SOURCE_WHY_LIMIT, SOURCE_WHY_WITHHELD,
    Secrets, Settings, Snapshot, SourceRefused, Stale, Unavailable, Value,
};
use whizbang_core::egress::{
    Account, Address, Classes, Credential, DataClass, DispatchPolicy, Origin, Piece, Recipient,
    Reply, Ways,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::Reader;
use whizbang_testkit::capabilities::{FakeFiller, FakeJudge, Paint, roster};
use whizbang_testkit::kinds::{FakeKind, MadeFake};
use whizbang_testkit::sending::{FakeCheck, FakeSender};
use whizbang_testkit::source::FakeSource;

use super::asked::{action, engine, fill_request, stored};
use super::commit::World;
use super::redact::made_up_key;

const API: &str = "https://api.example.com";

fn digest(of: &str) -> Digest {
    Digest::of_bytes(of.as_bytes())
}

/// An executor with these actions under this source, with no backend and no way out
/// of its own.
fn under(
    world: &World,
    source: &Arc<FakeSource>,
    actions: Vec<Arc<dyn whizbang_core::contract::Action>>,
) -> Engine {
    engine(world, NoBackends, Capabilities::none(), actions).with_config(source.clone())
}

fn nothing(name: &str) -> Arc<dyn whizbang_core::contract::Action> {
    action(name, Vec::new(), |_, _| Ok(Produced::nothing()))
}

/// How many runs the store holds.
fn runs(world: &World) -> usize {
    let reader = Reader::open(&world.root(), world.fs.clone()).unwrap();
    reader.snapshot().unwrap().runs().len()
}

/// Every byte of every file under a folder, one file after another.
fn every_byte_under(folder: &std::path::Path, into: &mut Vec<u8>) {
    let mut entries: Vec<_> = std::fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            every_byte_under(&path, into);
        } else {
            into.extend(std::fs::read(&path).unwrap());
            into.push(b'\n');
        }
    }
}

fn holds(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|w| w == needle.as_bytes())
}

/// What a config and an env file, given as text, offer with these kinds: a snapshot
/// with the config's digest, its backends, its secrets and its settings.
fn snapshot_of(config: &str, env: &str, kinds: &Kinds) -> Snapshot {
    let env = EnvFile::parse(env.as_bytes()).unwrap();
    let config = Config::parse(config.as_bytes(), &env.secrets()).unwrap();
    let credentials = env.credentials(&config).unwrap();
    let offers = Offers::of(&config, &credentials, kinds).unwrap();
    Snapshot::of(config.digest())
        .offering(Arc::new(offers))
        .with_secrets(credentials.secrets().clone())
        .recording(Arc::new(config))
}

fn fake_kinds() -> (Arc<FakeKind>, Kinds) {
    let kind = Arc::new(FakeKind::new("fake"));
    let mut kinds = Kinds::new();
    kinds.register(kind.clone()).unwrap();
    (kind, kinds)
}

/// What a part was shown at one asking: the action, the snapshot's digest and cap,
/// the digest the call was prepared under, and whether the snapshot holds a policy.
type Sight = (String, Digest, Option<Estimate>, Digest, bool);

/// A part that keeps what it was shown of the snapshot, for each action.
#[derive(Default)]
struct Sees(Mutex<Vec<Sight>>);

impl AdmissionPart for Sees {
    fn name(&self) -> &'static str {
        "sees"
    }

    fn look(&self, asking: &Asking<'_>, _: &mut PartStep<'_, '_>) -> Result<(), PartDeclined> {
        let snapshot = asking.snapshot();
        self.0.lock().unwrap().push((
            asking.call().action().to_owned(),
            snapshot.digest(),
            snapshot.cap(),
            asking.prepared().config(),
            snapshot.policy().is_some(),
        ));
        Ok(())
    }
}

// --- the source ------------------------------------------------------------------------------

#[test]
fn a_digest_alone_is_a_source_of_a_snapshot_that_holds_nothing_else() {
    let snapshot = digest("a").snapshot().unwrap();
    assert_eq!(snapshot.digest(), digest("a"));
    assert!(!snapshot.holds_backends());
    assert_eq!((snapshot.cap(), snapshot.policy()), (None, None));
    // And a snapshot is a source of itself.
    let capped = snapshot.capped_at(Estimate::of_micros(7));
    let again = capped.snapshot().unwrap();
    assert_eq!(
        (again.digest(), again.cap()),
        (digest("a"), Some(Estimate::of_micros(7)))
    );
}

#[test]
fn a_source_swapped_between_prepare_and_admission_is_a_stale_config() {
    let world = World::new();
    let source = Arc::new(FakeSource::of(Snapshot::of(digest("one"))));
    let executor = under(&world, &source, vec![nothing("does")]);
    let call = Call::new("does");
    let prepared = executor.prepare(&call).unwrap();
    assert_eq!(prepared.config(), digest("one"));
    source.swap(Snapshot::of(digest("two")));
    let refused = executor
        .request(&Request::new(call.clone()).expecting(&prepared))
        .unwrap_err();
    assert_eq!(
        refused,
        CallError::Refused(Refusal::Stale {
            action: "does".to_owned(),
            why: Stale::Config,
        })
    );
    assert_eq!(runs(&world), 0);
    // Prepared again under the config as it is now, it is admitted.
    let prepared = executor.prepare(&call).unwrap();
    assert_eq!(prepared.config(), digest("two"));
    let answer = executor.request(&Request::new(call).expecting(&prepared));
    assert!(matches!(answer, Ok(Answer::Ran(_))));
    assert_eq!(runs(&world), 1);
}

#[test]
fn a_source_that_refuses_fails_the_call_with_no_run_and_the_call_after_it_is_put_right_is_admitted()
{
    let world = World::new();
    let source = Arc::new(FakeSource::of(Snapshot::of(digest("one"))));
    let executor = under(&world, &source, vec![nothing("does")]);
    let why = "the config is not one this build reads";
    source.refuse(why);
    let failed = CallError::Config {
        why: why.to_owned(),
    };
    let id = executor.request_id("request-1").unwrap();
    let request = Request::new(Call::new("does")).with_id(id);
    assert_eq!(executor.prepare(&Call::new("does")), Err(failed.clone()));
    assert_eq!(executor.submit(&Call::new("does")), Err(failed.clone()));
    assert_eq!(executor.request(&request), Err(failed.clone()));
    assert_eq!(executor.start(&request), Err(failed.clone()));
    // It is no refusal of the call: the call was not looked at.
    assert_eq!(failed.refusal(), None);
    assert_eq!(
        failed.to_string(),
        "the config was not used: the config is not one this build reads"
    );
    assert_eq!(runs(&world), 0);
    // Put right, the same request is admitted under its id, which was not taken.
    source.swap(Snapshot::of(digest("two")));
    let Ok(Answer::Ran(finished)) = executor.request(&request) else {
        panic!("the request did not run");
    };
    let (record, _) = stored(&world, finished.run());
    assert_eq!(record.outcome().status, Status::Completed);
    assert_eq!(runs(&world), 1);
}

#[test]
fn a_sources_refusal_is_a_failure_and_is_returned_without_a_second_look_for_the_id() {
    // Another request is admitted under the id while the source is asked, and the
    // source then refuses. A refusal of a call would be followed by a second look
    // for the id, which would now answer a conflict; a source that refused judged
    // no call, and what it said is what comes back.
    let world = World::new();
    let source = Arc::new(FakeSource::of(Snapshot::of(digest("one"))));
    let executor = under(&world, &source, vec![nothing("does"), nothing("other")]);
    let id = executor.request_id("request-1").unwrap();
    source.when_asked({
        let (source, executor, id) = (source.clone(), executor.clone(), id.clone());
        move || {
            let other = Request::new(Call::new("other")).with_id(id);
            assert!(matches!(executor.request(&other), Ok(Answer::Ran(_))));
            source.refuse("the config is not one this build reads");
        }
    });
    // The first look finds the id free; the admission asks the source.
    let failed = executor
        .request(&Request::new(Call::new("does")).with_id(id))
        .unwrap_err();
    assert_eq!(
        failed,
        CallError::Config {
            why: "the config is not one this build reads".to_owned()
        }
    );
    assert_eq!(runs(&world), 1);
}

#[test]
fn a_sources_reason_that_holds_a_secret_is_replaced_and_a_long_one_is_cut() {
    let world = World::new();
    let key = made_up_key("S");
    let source = Arc::new(FakeSource::of(Snapshot::of(digest("one"))));
    let executor = under(&world, &source, vec![nothing("does")])
        .with_secrets(Secrets::new([key.clone()]).unwrap())
        .with_config(source.clone());
    // The type takes fixed words, and a source can leak a made text all the same.
    let leaked: &'static str = Box::leak(format!("the key {key} was refused").into_boxed_str());
    source.refuse(leaked);
    let failed = executor.submit(&Call::new("does")).unwrap_err();
    assert_eq!(
        failed,
        CallError::Config {
            why: SOURCE_WHY_WITHHELD.to_owned()
        }
    );
    let long: &'static str = Box::leak("w".repeat(SOURCE_WHY_LIMIT + 40).into_boxed_str());
    source.refuse(long);
    let failed = executor.submit(&Call::new("does")).unwrap_err();
    assert_eq!(
        failed,
        CallError::Config {
            why: "w".repeat(256)
        }
    );
    source.refuse("  ");
    let failed = executor.submit(&Call::new("does")).unwrap_err();
    assert_eq!(
        failed,
        CallError::Config {
            why: "no reason was given".to_owned()
        }
    );
    assert_eq!(runs(&world), 0);
}

#[test]
fn a_source_that_panics_leaves_the_store_as_it_was_and_the_next_request_is_served() {
    let world = World::new();
    let source = Arc::new(FakeSource::of(Snapshot::of(digest("one"))));
    let executor = under(&world, &source, vec![nothing("does")]);
    source.panic();
    let id = executor.request_id("request-1").unwrap();
    let request = Request::new(Call::new("does")).with_id(id);
    let unwound = catch_unwind(AssertUnwindSafe(|| executor.request(&request)));
    assert!(unwound.is_err());
    let unwound = catch_unwind(AssertUnwindSafe(|| executor.prepare(&Call::new("does"))));
    assert!(unwound.is_err());
    assert_eq!(runs(&world), 0);
    // Nothing was half-admitted and the store is not locked: the same request runs.
    source.swap(Snapshot::of(digest("one")));
    let Ok(Answer::Ran(finished)) = executor.request(&request) else {
        panic!("the request did not run");
    };
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!(runs(&world), 1);
}

#[test]
fn a_snapshot_is_taken_once_for_an_admission_and_every_part_is_shown_it() {
    // The source answers another snapshot each time it is asked. Two parts of one
    // admission are shown the same one, which is the one the call was prepared under.
    let world = World::new();
    let source = Arc::new(FakeSource::of(Snapshot::of(digest("first"))));
    let (first, second) = (Arc::new(Sees::default()), Arc::new(Sees::default()));
    let executor = under(&world, &source, vec![nothing("does")])
        .with_part(first.clone())
        .with_part(second.clone());
    source.when_asked({
        let source = source.clone();
        move || {
            source.swap(Snapshot::of(digest("second")).capped_at(Estimate::of_micros(9)));
        }
    });
    executor.submit(&Call::new("does")).unwrap();
    assert_eq!(source.asked(), 1);
    let cap = Some(Estimate::of_micros(9));
    let seen = vec![(
        "does".to_owned(),
        digest("second"),
        cap,
        digest("second"),
        false,
    )];
    assert_eq!(*first.0.lock().unwrap(), seen);
    assert_eq!(*second.0.lock().unwrap(), seen);
}

// --- a run goes on under the snapshot it was admitted with ------------------------------------

/// A kind whose backends are fillers that send a prompt to their table's address,
/// and name that origin as their recipient. It keeps each filler it made.
#[derive(Default)]
struct Posts {
    made: Mutex<Vec<Arc<FakeFiller>>>,
    /// The account its backends' recipient is under, when a test gave one.
    account: Option<Account>,
}

impl Posts {
    /// The recipient a backend at this origin names.
    fn recipient(&self, origin: &Origin) -> Recipient {
        let recipient = Recipient::new(origin.clone());
        match &self.account {
            Some(account) => recipient.with_account(account.clone()),
            None => recipient,
        }
    }
}

impl Kind for Posts {
    fn name(&self) -> &str {
        "posts"
    }

    fn make(&self, table: &Table<'_>) -> Result<Made, KindRefused> {
        let Some(address) = table.backend().address() else {
            return Err(KindRefused::Table("a posting backend needs a url"));
        };
        let model = table.backend().model().unwrap_or("m");
        let declaration = Declaration::new(ModelIdentity::new("posts", model, "1"), &[])
            .map_err(|_| KindRefused::Table("a posting backend cannot declare that model"))?
            .sending_to(self.recipient(address.origin()));
        let prompt = vec![Piece::of(DataClass::Prompts, b"a red door".to_vec())];
        let filler = FakeFiller::new(Paint::Inverse)
            .declared_as(declaration)
            .posting(address.clone(), prompt);
        let filler = Arc::new(filler);
        self.made.lock().unwrap().push(filler.clone());
        Ok(Made::Fill(filler))
    }
}

fn there() -> Recipient {
    Recipient::new(Origin::of(API, &Secrets::none()).unwrap())
}

fn ok() -> Result<Reply, Fault> {
    Ok(Reply::answered(200, b"{}".to_vec()).unwrap())
}

#[test]
fn a_run_goes_on_under_the_snapshot_it_was_admitted_with_though_the_source_was_swapped_while_it_ran()
 {
    let world = World::new();
    let posts = Arc::new(Posts::default());
    let mut kinds = Kinds::new();
    kinds.register(posts.clone()).unwrap();
    let key = made_up_key("A");
    let prompts = Classes::of(&[DataClass::Prompts]);
    // The first config: a filler, a cap of 2, a policy that approves the recipient
    // for prompts, and its credential.
    let config = |model: &str| {
        format!("[backends.fill]\nkind = \"posts\"\nmodel = \"{model}\"\nurl = \"{API}/v1\"\n")
    };
    let origin = Origin::of(API, &Secrets::none()).unwrap();
    let credential = Credential::new(origin, "x-key", &key, &Secrets::none()).unwrap();
    let first = snapshot_of(&config("one"), "", &kinds)
        .capped_at(Estimate::of_micros(2_000_000))
        .sending_under(DispatchPolicy::none().approving(there(), prompts).unwrap())
        .with_credential(&there(), credential)
        .unwrap()
        .with_secrets(Secrets::new([key.clone()]).unwrap());
    // The second: another model, a cap of 1, and a policy that approves nothing.
    let second = snapshot_of(&config("two"), "", &kinds)
        .capped_at(Estimate::of_micros(1_000_000))
        .sending_under(DispatchPolicy::none());
    let (one, two) = (first.digest(), second.digest());
    assert_ne!(one, two);
    let source = Arc::new(FakeSource::of(first));
    let sender = Arc::new(FakeSender::scripted(vec![ok(), ok()]));
    let sees = Arc::new(Sees::default());
    let swapped = Arc::new(Mutex::new(Some(second)));
    let asks = {
        let (source, swapped) = (source.clone(), swapped.clone());
        let needs = vec![Requirement::new(Capability::Fill)];
        action("asks", needs, move |_, context| {
            // The config changes while the run executes.
            if let Some(second) = swapped.lock().unwrap().take() {
                source.swap(second);
            }
            let fill = context.capabilities().fill().unwrap();
            let sent = fill.fill(&fill_request());
            context.executor().submit(&Call::new("inner")).unwrap();
            match sent {
                Ok(_) => Ok(Produced::nothing()),
                Err(fault) => Err(Failure::error("refused", fault.sentence())),
            }
        })
    };
    let ways = Ways::through(sender.clone()).checked_by(Arc::new(FakeCheck::approving()));
    let executor = under(&world, &source, vec![asks, nothing("inner")])
        .with_ways(ways)
        .with_part(sees.clone());

    let finished = executor.submit(&Call::new("asks")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    // The backend of the first snapshot answered, and no other.
    let made = posts.made.lock().unwrap().clone();
    assert_eq!(made.len(), 2);
    assert_eq!((made[0].requests(), made[1].requests()), (1, 0));
    // The send was held to the first snapshot's policy, and carried its credential.
    let hops = sender.hops();
    assert_eq!(hops.len(), 1);
    assert_eq!(hops[0].headers(), [("x-key".to_owned(), key.clone())]);
    // The call from inside the action was admitted under its caller's snapshot,
    // its cap among it, and the source was asked once: for the run nothing called.
    let cap = Some(Estimate::of_micros(2_000_000));
    assert_eq!(
        *sees.0.lock().unwrap(),
        [
            ("asks".to_owned(), one, cap, one, true),
            ("inner".to_owned(), one, cap, one, true),
        ]
    );
    assert_eq!(source.asked(), 1);
    let (record, _) = stored(&world, finished.run());
    assert_eq!(record.asked().config, Some(one));

    // The next run is admitted under the second: its backend, its cap, its policy.
    let finished = executor.submit(&Call::new("asks")).unwrap();
    assert_eq!(finished.outcome().name(), "error");
    assert_eq!((made[0].requests(), made[1].requests()), (1, 1));
    assert_eq!(sender.requests(), 1, "the second policy approves nothing");
    let seen = sees.0.lock().unwrap();
    let cap = Some(Estimate::of_micros(1_000_000));
    assert_eq!(seen[2], ("asks".to_owned(), two, cap, two, true));
    assert_eq!(seen[3], ("inner".to_owned(), two, cap, two, true));
    assert_eq!(source.asked(), 2);
}

#[test]
fn a_call_from_inside_an_action_is_prepared_under_its_callers_snapshot_too() {
    let world = World::new();
    let source = Arc::new(FakeSource::of(Snapshot::of(digest("one"))));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let outer = {
        let (source, seen) = (source.clone(), seen.clone());
        action("outer", Vec::new(), move |_, context| {
            source.refuse("the config is gone");
            let prepared = context.executor().prepare(&Call::new("inner")).unwrap();
            seen.lock().unwrap().push(prepared.config());
            context.executor().submit(&Call::new("inner")).unwrap();
            Ok(Produced::nothing())
        })
    };
    let executor = under(&world, &source, vec![outer, nothing("inner")]);
    let finished = executor.submit(&Call::new("outer")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!(*seen.lock().unwrap(), [digest("one")]);
    assert_eq!((source.asked(), runs(&world)), (1, 2));
}

#[test]
fn a_run_and_the_run_it_calls_are_refused_what_their_snapshot_refuses_though_the_source_now_approves_it()
 {
    let world = World::new();
    let posts = Arc::new(Posts::default());
    let mut kinds = Kinds::new();
    kinds.register(posts.clone()).unwrap();
    let config = |model: &str| {
        format!("[backends.fill]\nkind = \"posts\"\nmodel = \"{model}\"\nurl = \"{API}/v1\"\n")
    };
    // The first config approves nobody; the second approves the recipient.
    let prompts = Classes::of(&[DataClass::Prompts]);
    let first = snapshot_of(&config("one"), "", &kinds).sending_under(DispatchPolicy::none());
    let second = snapshot_of(&config("two"), "", &kinds)
        .sending_under(DispatchPolicy::none().approving(there(), prompts).unwrap());
    let source = Arc::new(FakeSource::of(first));
    let sender = Arc::new(FakeSender::scripted(vec![ok(), ok()]));
    let faults = Arc::new(Mutex::new(Vec::new()));
    let swapped = Arc::new(Mutex::new(Some(second)));
    let needs = || vec![Requirement::new(Capability::Fill)];
    let asks = {
        let (source, swapped, faults) = (source.clone(), swapped.clone(), faults.clone());
        action("asks", needs(), move |_, context| {
            // The config changes while the run executes: it now approves the send.
            if let Some(second) = swapped.lock().unwrap().take() {
                source.swap(second);
            }
            let fill = context.capabilities().fill().unwrap();
            let fault = fill.fill(&fill_request()).err();
            faults.lock().unwrap().push(("asks", fault));
            context.executor().submit(&Call::new("inner")).unwrap();
            Ok(Produced::nothing())
        })
    };
    let inner = {
        let faults = faults.clone();
        action("inner", needs(), move |_, context| {
            let fill = context.capabilities().fill().unwrap();
            let fault = fill.fill(&fill_request()).err();
            faults.lock().unwrap().push(("inner", fault));
            Ok(Produced::nothing())
        })
    };
    let ways = Ways::through(sender.clone()).checked_by(Arc::new(FakeCheck::approving()));
    let executor = under(&world, &source, vec![asks, inner]).with_ways(ways);

    let finished = executor.submit(&Call::new("asks")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    // Neither the run nor the run it called was let through: both were admitted
    // under the first snapshot, whose policy approves nobody, and both asked its
    // backend.
    assert_eq!(sender.requests(), 0, "nothing was handed over");
    let made = posts.made.lock().unwrap().clone();
    assert_eq!(made.len(), 2);
    assert_eq!((made[0].requests(), made[1].requests()), (2, 0));
    let refused: Vec<_> = faults.lock().unwrap().drain(..).collect();
    assert_eq!(refused.len(), 2);
    for (who, fault) in &refused {
        let fault = fault
            .as_ref()
            .unwrap_or_else(|| panic!("{who} was answered"));
        assert_eq!(
            (fault.kind(), fault.stage()),
            (FaultKind::Rejected, Stage::NotSent)
        );
    }
    let (record, _) = stored(&world, finished.run());
    assert_eq!(record.egress().len(), 1);
    assert_eq!(record.egress()[0].outcome(), Handed::Refused);
    assert_eq!(source.asked(), 1);

    // The next run and the run it calls are admitted under the second, and send.
    let finished = executor.submit(&Call::new("asks")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!(sender.requests(), 2);
    assert_eq!((made[0].requests(), made[1].requests()), (2, 2));
    assert!(
        faults
            .lock()
            .unwrap()
            .iter()
            .all(|(_, fault)| fault.is_none())
    );
    let (record, _) = stored(&world, finished.run());
    assert_eq!(record.egress()[0].outcome(), Handed::Sent);
}

/// A kind whose `make` panics while it is told to.
struct Breaks(AtomicBool);

impl Kind for Breaks {
    fn name(&self) -> &str {
        "breaks"
    }

    fn make(&self, _: &Table<'_>) -> Result<Made, KindRefused> {
        assert!(!self.0.load(Ordering::SeqCst), "the kind went wrong");
        Ok(Made::Fill(Arc::new(FakeFiller::new(Paint::Inverse))))
    }
}

/// A source that makes a snapshot of its config each time it is asked, its backends
/// among it, as a source over a file does.
struct Reads(&'static str, Kinds);

impl ConfigSource for Reads {
    fn snapshot(&self) -> Result<Snapshot, SourceRefused> {
        Ok(snapshot_of(self.0, "", &self.1))
    }
}

#[test]
fn a_kind_that_panics_while_a_snapshot_is_made_leaves_nothing_behind_and_the_next_request_is_served()
 {
    let world = World::new();
    let breaks = Arc::new(Breaks(AtomicBool::new(true)));
    let mut kinds = Kinds::new();
    kinds.register(breaks.clone()).unwrap();
    let source = Arc::new(Reads("[backends.fill]\nkind = \"breaks\"\n", kinds));
    let executor =
        engine(&world, NoBackends, Capabilities::none(), vec![chooses()]).with_config(source);
    let id = executor.request_id("request-1").unwrap();
    let request = Request::new(Call::new("asks")).with_id(id);
    let unwound = catch_unwind(AssertUnwindSafe(|| executor.request(&request)));
    assert!(unwound.is_err());
    let unwound = catch_unwind(AssertUnwindSafe(|| executor.start(&request)));
    assert!(unwound.is_err());
    let unwound = catch_unwind(AssertUnwindSafe(|| executor.prepare(&Call::new("asks"))));
    assert!(unwound.is_err());
    assert_eq!(runs(&world), 0);
    // Nothing was half-admitted and the store is not locked: the same request runs
    // under its id.
    breaks.0.store(false, Ordering::SeqCst);
    let Ok(Answer::Ran(finished)) = executor.request(&request) else {
        panic!("the request did not run");
    };
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!(runs(&world), 1);
}

// --- the secrets -----------------------------------------------------------------------------

#[test]
fn a_secret_of_an_earlier_snapshot_is_still_redacted_from_a_later_runs_record() {
    let world = World::new();
    let (base, old, new) = (made_up_key("B"), made_up_key("O"), made_up_key("N"));
    let with = |key: &str, of: &str| {
        Snapshot::of(digest(of)).with_secrets(Secrets::new([key.to_owned()]).unwrap())
    };
    let source = Arc::new(FakeSource::of(with(&old, "old")));
    let tells = {
        let keys = [base.clone(), old.clone(), new.clone()];
        action("tells", Vec::new(), move |call, context| {
            // The first run knows of two keys; the second is told all three.
            let known = if call.arguments().is_empty() { 2 } else { 3 };
            let said = keys[..known].join(" and ");
            context.events().progress(&format!("using {said}"));
            let written = context.diagnostics().write("keys.txt", said.as_bytes());
            written.map_err(|why| Failure::error("write", &why))?;
            Err(Failure::error("told", &format!("the keys are {said}")))
        })
    };
    let executor = under(&world, &source, vec![tells])
        .with_secrets(Secrets::new([base.clone()]).unwrap())
        .with_config(source.clone());
    let first = executor.submit(&Call::new("tells")).unwrap();
    assert_eq!(first.outcome().name(), "error");
    // The key is rotated: the next snapshot holds the new one alone.
    source.swap(with(&new, "new"));
    let call = Call::new("tells").argument("note", Value::text("again"));
    let second = executor.submit(&call).unwrap();
    assert_eq!(second.outcome().name(), "error");
    let whizbang_core::contract::Outcome::Error { message, .. } = second.outcome() else {
        panic!("not an error");
    };
    assert_eq!(
        message,
        "the keys are [redacted] and [redacted] and [redacted]"
    );
    let mut store = Vec::new();
    every_byte_under(&world.root(), &mut store);
    for key in [&base, &old, &new] {
        assert!(!holds(&store, key), "a key is in the store");
        assert!(
            !holds(&store, &key[..14]),
            "the start of a key is in the store"
        );
    }
    assert!(holds(&store, "the keys are [redacted] and [redacted]"));
    // And a request id that holds the old key is still refused, and not shown.
    let refused = executor.request_id(&format!("id-{old}")).unwrap_err();
    assert_eq!(refused, CallError::Refused(Refusal::SecretInRequestId));
}

#[test]
fn a_request_id_that_holds_a_key_its_admissions_snapshot_brings_is_refused_and_written_nowhere() {
    // The id is made before the executor has taken any snapshot, so nothing knew the
    // key when the id was looked at first.
    let world = World::new();
    let key = made_up_key("I");
    let snapshot = Snapshot::of(digest("one")).with_secrets(Secrets::new([key.clone()]).unwrap());
    let source = Arc::new(FakeSource::of(snapshot));
    let executor = under(&world, &source, vec![nothing("does")]);
    let id = executor.request_id(&key).unwrap();
    let refused = executor
        .request(&Request::new(Call::new("does")).with_id(id))
        .unwrap_err();
    assert_eq!(refused, CallError::Refused(Refusal::SecretInRequestId));
    assert_eq!(runs(&world), 0);
    let mut store = Vec::new();
    every_byte_under(&world.root(), &mut store);
    assert!(!holds(&store, &key));
}

#[test]
fn a_request_id_taken_while_its_admissions_snapshot_was_read_is_not_shown_when_the_snapshot_brings_its_key()
 {
    // Another request is admitted under the id while the source is asked for this
    // admission's snapshot, and that snapshot brings the key the id holds. The look
    // after a refusal would answer a conflict, which names the id: an id that holds
    // a secret is refused without that look, as it is before the first one.
    let world = World::new();
    let key = made_up_key("R");
    let source = Arc::new(FakeSource::of(Snapshot::of(digest("one"))));
    let executor = under(&world, &source, vec![nothing("does"), nothing("other")]);
    let id = executor.request_id(&key).unwrap();
    source.when_asked({
        let (source, executor, id, key) =
            (source.clone(), executor.clone(), id.clone(), key.clone());
        move || {
            let other = Request::new(Call::new("other")).with_id(id);
            assert!(matches!(executor.request(&other), Ok(Answer::Ran(_))));
            let brings = Secrets::new([key]).unwrap();
            source.swap(Snapshot::of(digest("one")).with_secrets(brings));
        }
    });
    let refused = executor
        .request(&Request::new(Call::new("does")).with_id(id))
        .unwrap_err();
    assert_eq!(refused, CallError::Refused(Refusal::SecretInRequestId));
    assert!(!format!("{refused:?} {refused}").contains(&key[..12]));
    assert_eq!(runs(&world), 1);
}

#[test]
fn an_origin_that_holds_a_secret_of_an_earlier_snapshot_is_not_sent_to_and_is_in_no_file() {
    // The executor's own policy approves a recipient at a host, its check lets every
    // address through, and it holds a filler that sends there. The host is a secret
    // of the first snapshot alone. A run admitted under a later snapshot, which
    // brings no secret, has a place for facts made with every secret the executor
    // knew at its admission: the destination cannot be listed, so nothing is sent.
    let world = World::new();
    let host = format!("keyhost-{}.example.com", "0123456789abcdef");
    let at = Origin::of(&format!("https://{host}"), &Secrets::none()).unwrap();
    let to = Address::of(&format!("https://{host}/v1"), &Secrets::none()).unwrap();
    let prompt = vec![Piece::of(DataClass::Prompts, b"a red door".to_vec())];
    let filler = FakeFiller::new(Paint::Inverse).sending_to(Recipient::new(at.clone()));
    let filler = Arc::new(filler.posting(to, prompt));
    let early = Snapshot::of(digest("early")).with_secrets(Secrets::new([host.clone()]).unwrap());
    let source = Arc::new(FakeSource::of(early));
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let answered = Arc::new(Mutex::new(None));
    let asks = {
        let answered = answered.clone();
        let needs = vec![Requirement::new(Capability::Fill)];
        action("asks", needs, move |_, context| {
            let fill = context.capabilities().fill().unwrap();
            *answered.lock().unwrap() = fill.fill(&fill_request()).err();
            Ok(Produced::nothing())
        })
    };
    let policy = DispatchPolicy::none()
        .approving(Recipient::new(at), Classes::of(&[DataClass::Prompts]))
        .unwrap();
    let ways = Ways::through(sender.clone())
        .checked_by(Arc::new(FakeCheck::approving()))
        .under(policy);
    let executor = engine(
        &world,
        roster(None, Some(filler.as_ref()), None),
        Capabilities::none().with_fill(filler.clone()),
        vec![asks, nothing("does")],
    )
    .with_config(source.clone())
    .with_ways(ways);
    executor.submit(&Call::new("does")).unwrap();
    source.swap(Snapshot::of(digest("later")));
    let finished = executor.submit(&Call::new("asks")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!((filler.requests(), sender.requests()), (1, 0));
    let fault: Fault = answered.lock().unwrap().take().unwrap();
    assert_eq!(
        (fault.kind(), fault.stage()),
        (FaultKind::Rejected, Stage::NotSent)
    );
    let (record, _) = stored(&world, finished.run());
    assert_eq!(record.asked().config, Some(digest("later")));
    assert_eq!(record.egress().len(), 0);
    let refusals = &record.outcome().refusals;
    assert_eq!(refusals.len(), 1);
    assert_eq!(refusals[0].kind(), RefusalKind::RequestNotApproved);
    let mut store = Vec::new();
    every_byte_under(&world.root(), &mut store);
    assert!(!holds(&store, "keyhost"));
}

#[test]
fn a_snapshot_that_approves_a_recipient_whose_origin_is_a_secret_is_not_taken() {
    // Every send to such a recipient would be refused where it is made. It is said
    // once, where the snapshot is taken, in fixed words that show no origin: the
    // secret may be the snapshot's own or one of a snapshot before.
    let world = World::new();
    let host = format!("keyhost-{}.example.com", "0123456789abcdef");
    let at = Origin::of(&format!("https://{host}"), &Secrets::none()).unwrap();
    let knows = Secrets::new([host.clone()]).unwrap();
    let approving = |origin: &Origin, probe: bool| {
        let recipient = Recipient::new(origin.clone());
        let policy = match probe {
            true => DispatchPolicy::none().approving_probes_of(recipient),
            false => {
                DispatchPolicy::none().approving(recipient, Classes::of(&[DataClass::Prompts]))
            }
        };
        Snapshot::of(digest("approves")).sending_under(policy.unwrap())
    };
    let source = Arc::new(FakeSource::of(Snapshot::of(digest("one"))));
    let executor = under(&world, &source, vec![nothing("does")]);
    let failed = CallError::Config {
        why: SECRET_ORIGIN_APPROVED.to_owned(),
    };
    assert!(!SECRET_ORIGIN_APPROVED.contains("keyhost"));
    // Before the executor knows the secret, the same snapshot is taken.
    source.swap(approving(&at, false));
    executor.submit(&Call::new("does")).unwrap();
    // Its own secret; then a secret of an earlier snapshot, for a probe alone.
    source.swap(approving(&at, false).with_secrets(knows));
    assert_eq!(executor.submit(&Call::new("does")), Err(failed.clone()));
    assert_eq!(executor.prepare(&Call::new("does")), Err(failed.clone()));
    source.swap(approving(&at, true));
    assert_eq!(executor.submit(&Call::new("does")), Err(failed.clone()));
    assert_eq!(runs(&world), 1);
    // A recipient elsewhere, and a secret origin as an upstream alone, are taken.
    let elsewhere = Origin::of(API, &Secrets::none()).unwrap();
    source.swap(approving(&elsewhere, false));
    executor.submit(&Call::new("does")).unwrap();
    let forwards = Recipient::new(elsewhere).forwarding_to(vec![at]).unwrap();
    let policy = DispatchPolicy::none().approving(forwards, Classes::of(&[DataClass::Prompts]));
    source.swap(Snapshot::of(digest("forwards")).sending_under(policy.unwrap()));
    executor.submit(&Call::new("does")).unwrap();
    assert_eq!(runs(&world), 3);
}

#[test]
fn the_keys_of_two_snapshots_are_both_taken_out_of_everything_a_later_run_writes() {
    // The record, the journal, a diagnostic, a refusal and where the data went: a run
    // admitted under the second snapshot writes none of the first one's key, nor of
    // its own.
    let world = World::new();
    let (old, new) = (made_up_key("O"), made_up_key("N"));
    // The recipient's account holds the first snapshot's key.
    let account = Account::new(&format!("acct-{old}"), &Secrets::none()).unwrap();
    let posts = Arc::new(Posts {
        account: Some(account),
        ..Posts::default()
    });
    let mut kinds = Kinds::new();
    kinds.register(posts.clone()).unwrap();
    let with = |key: &str| Secrets::new([key.to_owned()]).unwrap();
    let source = Arc::new(FakeSource::of(
        Snapshot::of(digest("old")).with_secrets(with(&old)),
    ));
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let tells = {
        let (old, new) = (old.clone(), new.clone());
        let needs = vec![Requirement::new(Capability::Fill)];
        action("tells", needs, move |_, context| {
            let said = format!("{old} and {new}");
            context.events().progress(&format!("using {said}"));
            let written = context.diagnostics().write("keys.txt", said.as_bytes());
            written.map_err(|why| Failure::error("write", &why))?;
            // Two calls that are refused, one in words that hold the first key and
            // one in words that hold the second.
            for key in [&old, &new] {
                let gone = Call::new(&format!("gone-{key}"));
                assert!(context.executor().submit(&gone).is_err());
            }
            let fill = context.capabilities().fill().unwrap();
            let sent = fill.fill(&fill_request());
            sent.map_err(|fault| Failure::error("fault", fault.sentence()))?;
            Err(Failure::error("told", &format!("the keys are {said}")))
        })
    };
    let ways = Ways::through(sender.clone()).checked_by(Arc::new(FakeCheck::approving()));
    let executor = under(&world, &source, vec![tells, nothing("does")]).with_ways(ways);
    executor.submit(&Call::new("does")).unwrap();
    // The key is rotated: the next snapshot brings the new one alone.
    let config = format!("[backends.fill]\nkind = \"posts\"\nurl = \"{API}/v1\"\n");
    let recipient = posts.recipient(&Origin::of(API, &Secrets::none()).unwrap());
    let policy = DispatchPolicy::none()
        .approving(recipient, Classes::of(&[DataClass::Prompts]))
        .unwrap();
    let later = snapshot_of(&config, "", &kinds)
        .with_secrets(with(&new))
        .sending_under(policy);
    source.swap(later);
    let finished = executor.submit(&Call::new("tells")).unwrap();
    let whizbang_core::contract::Outcome::Error { message, .. } = finished.outcome() else {
        panic!("not an error");
    };
    assert_eq!(message, "the keys are [redacted] and [redacted]");
    assert_eq!(sender.requests(), 1);
    let (record, _) = stored(&world, finished.run());
    // Where the data went: an account that holds a secret is not written at all,
    // not even cut, and this one holds the key of the snapshot before.
    assert_eq!(record.egress().len(), 1);
    assert_eq!(record.egress()[0].outcome(), Handed::Sent);
    assert_eq!(record.egress()[0].account(), None);
    // The refusals of the calls it made: a refusal that holds a secret is not
    // quoted, whichever snapshot the secret came with.
    let refusals = &record.outcome().refusals;
    assert_eq!(refusals.len(), 2);
    for refused in refusals {
        assert_eq!(
            refused.why(),
            "the call held something that must not be written, so its refusal is not quoted"
        );
    }
    let mut store = Vec::new();
    every_byte_under(&world.root(), &mut store);
    for key in [&old, &new] {
        assert!(!holds(&store, key), "a key is in the store");
        assert!(
            !holds(&store, &key[..14]),
            "the start of a key is in the store"
        );
    }
    assert!(holds(&store, "using [redacted] and [redacted]"));
}

#[test]
fn a_snapshot_that_would_bring_more_secrets_than_an_executor_keeps_is_not_taken() {
    let world = World::new();
    let some = |from: usize, to: usize| {
        let values = (from..to).map(|at| format!("secret-value-{at:06}"));
        Secrets::new(values).unwrap()
    };
    assert_eq!(SECRETS_KEPT_LIMIT, 4096);
    let source = Arc::new(FakeSource::of(
        Snapshot::of(digest("one")).with_secrets(some(0, 4000)),
    ));
    let executor = under(&world, &source, vec![nothing("does")]);
    executor.submit(&Call::new("does")).unwrap();
    // 97 more would be 4,097: the call fails, nothing is admitted, none is let go.
    source.swap(Snapshot::of(digest("two")).with_secrets(some(4000, 4097)));
    let failed = executor.submit(&Call::new("does")).unwrap_err();
    assert!(matches!(&failed, CallError::Config { why } if why.contains("4096")));
    assert_eq!(runs(&world), 1);
    // 96 more are taken, and the earlier ones are still known.
    source.swap(Snapshot::of(digest("two")).with_secrets(some(4000, 4096)));
    executor.submit(&Call::new("does")).unwrap();
    for held in ["secret-value-000000", "secret-value-004095"] {
        let refused = executor.request_id(held).unwrap_err();
        assert_eq!(refused, CallError::Refused(Refusal::SecretInRequestId));
    }
    assert!(executor.request_id("secret-value-004096").is_ok());
}

// --- the backends ----------------------------------------------------------------------------

const TWO_OF_ONE_MODEL: &str = r#"
[backends.fill]
default = "one"

[backends.fill.one]
kind = "fake"
model = "same"

[backends.fill.two]
kind = "fake"
model = "same"

[backends.fill.gone]
kind = "no-such-kind"
"#;

/// `asks`: needs a filler, which its argument `judge` may choose, and asks it once.
fn chooses() -> Arc<dyn whizbang_core::contract::Action> {
    let needs = vec![Requirement::new(Capability::Fill).chosen_by("judge")];
    action("asks", needs, |_, context| {
        let fill = context.capabilities().fill().unwrap();
        fill.fill(&fill_request()).unwrap();
        Ok(Produced::nothing())
    })
}

fn requests(kind: &FakeKind) -> Vec<(String, u64)> {
    let made = kind.made();
    made.iter()
        .map(|(_, name, fake)| (name.clone(), fake.requests()))
        .collect()
}

#[test]
fn two_backends_of_one_capability_that_declare_one_model_are_told_apart_by_name() {
    let world = World::new();
    let (kind, kinds) = fake_kinds();
    let source = Arc::new(FakeSource::of(snapshot_of(TWO_OF_ONE_MODEL, "", &kinds)));
    let executor = under(&world, &source, vec![chooses()]);
    // The call that names none is handed the default, and one that names the
    // second is handed the second, though both declare one model.
    executor.submit(&Call::new("asks")).unwrap();
    assert_eq!(
        requests(&kind),
        [("one".to_owned(), 1), ("two".to_owned(), 0)]
    );
    let chosen = Call::new("asks").argument("judge", Value::text("two"));
    let finished = executor.submit(&chosen).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!(
        requests(&kind),
        [("one".to_owned(), 1), ("two".to_owned(), 1)]
    );
}

#[test]
fn a_backend_that_is_not_ready_is_refused_with_its_reason_and_never_replaced_by_another() {
    let world = World::new();
    let (kind, kinds) = fake_kinds();
    let source = Arc::new(FakeSource::of(snapshot_of(TWO_OF_ONE_MODEL, "", &kinds)));
    let executor = under(&world, &source, vec![chooses()]);
    let chosen = Call::new("asks").argument("judge", Value::text("gone"));
    let Readiness::NotReady(missing) = executor.prepare(&chosen).unwrap().readiness().clone()
    else {
        panic!("the call is ready");
    };
    assert_eq!(
        missing[0].why(),
        "the fill backend the call chose is not ready: its kind is not one this build has"
    );
    let refused = executor.submit(&chosen).unwrap_err();
    assert!(matches!(
        refused,
        CallError::Refused(Refusal::NotReady { .. })
    ));
    assert_eq!(runs(&world), 0);
    assert!(requests(&kind).iter().all(|(_, requests)| *requests == 0));
}

#[test]
fn a_backend_made_for_a_snapshot_answers_only_the_runs_admitted_under_it() {
    let world = World::new();
    let (kind, kinds) = fake_kinds();
    let config = "[backends.fill]\nkind = \"fake\"\n";
    let source = Arc::new(FakeSource::of(snapshot_of(config, "", &kinds)));
    let executor = under(&world, &source, vec![chooses()]);
    executor.submit(&Call::new("asks")).unwrap();
    executor.submit(&Call::new("asks")).unwrap();
    // The same config, read again: another snapshot, and another backend.
    source.swap(snapshot_of(config, "", &kinds));
    executor.submit(&Call::new("asks")).unwrap();
    let made = kind.made();
    assert_eq!(made.len(), 2);
    let (MadeFake::Fill(first), MadeFake::Fill(second)) = (&made[0].2, &made[1].2) else {
        panic!("not fillers");
    };
    assert!(!Arc::ptr_eq(first, second));
    assert_eq!((first.requests(), second.requests()), (2, 1));
}

/// Backends that resolve a model for every capability and hold no backend.
struct Hollow;

impl Backends for Hollow {
    fn resolve(
        &self,
        _: Capability,
        _: Option<&str>,
        _: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        Ok(ModelIdentity::new("hollow", "m", "1"))
    }
}

impl Offered for Hollow {
    fn default_of(&self, _: Capability) -> Option<&str> {
        Some("default")
    }

    fn made(&self, _: Capability, _: &str) -> Option<Made> {
        None
    }
}

/// The same, holding a backend that declares another model than the one resolved.
struct Other(Arc<FakeFiller>);

impl Backends for Other {
    fn resolve(
        &self,
        capability: Capability,
        choice: Option<&str>,
        features: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        Hollow.resolve(capability, choice, features)
    }
}

impl Offered for Other {
    fn default_of(&self, _: Capability) -> Option<&str> {
        Some("default")
    }

    fn made(&self, _: Capability, _: &str) -> Option<Made> {
        Some(Made::Fill(self.0.clone()))
    }
}

/// The same, holding a backend that declares the model resolved and answers another
/// capability.
struct Crossing(Arc<FakeJudge>);

impl Backends for Crossing {
    fn resolve(
        &self,
        capability: Capability,
        choice: Option<&str>,
        features: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        Hollow.resolve(capability, choice, features)
    }
}

impl Offered for Crossing {
    fn default_of(&self, _: Capability) -> Option<&str> {
        Some("default")
    }

    fn made(&self, _: Capability, _: &str) -> Option<Made> {
        Some(Made::Judge(self.0.clone()))
    }
}

#[test]
fn a_call_cannot_resolve_a_model_its_snapshot_holds_no_backend_for() {
    let world = World::new();
    let filler = Arc::new(FakeFiller::new(Paint::Inverse));
    let source = Arc::new(FakeSource::of(
        Snapshot::of(digest("hollow")).offering(Arc::new(Hollow)),
    ));
    // The executor was built with a filler of its own: a snapshot that holds
    // backends is not helped out with it.
    let held = Capabilities::none().with_fill(filler.clone());
    let executor = engine(&world, NoBackends, held, vec![chooses()]).with_config(source.clone());
    let hollow = Declaration::new(ModelIdentity::new("hollow", "m", "1"), &[]).unwrap();
    let judge = Arc::new(FakeJudge::scripted(Vec::new()).declared_as(hollow));
    for offered in [
        Arc::new(Hollow) as Arc<dyn Offered>,
        Arc::new(Other(filler.clone())),
        Arc::new(Crossing(judge.clone())),
    ] {
        source.swap(Snapshot::of(digest("hollow")).offering(offered));
        // It prepares as ready: the roster resolved a model.
        let prepared = executor.prepare(&Call::new("asks")).unwrap();
        assert_eq!(prepared.readiness(), &Readiness::Ready);
        let refused = executor.submit(&Call::new("asks")).unwrap_err();
        let CallError::Refused(Refusal::NotReady { action, missing }) = refused else {
            panic!("not refused as not ready: {refused:?}");
        };
        assert_eq!(action, "asks");
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].capability(), Capability::Fill);
        assert_eq!(
            missing[0].why(),
            "the config resolved a model for it and holds no backend that declares that \
             model under the name the call resolved"
        );
    }
    assert_eq!((runs(&world), filler.requests()), (0, 0));
    assert_eq!(judge.requests(), 0);
}

// --- the settings ----------------------------------------------------------------------------

#[test]
fn the_settings_of_a_runs_snapshot_are_in_its_record() {
    let world = World::new();
    let (_, kinds) = fake_kinds();
    let config = "[backends.fill]\nkind = \"fake\"\nmodel = \"lama\"\n\n\
                  [backends.fill.settings]\nversion = \"3\"\n\n\
                  [backends.judge]\nkind = \"fake\"\n\n[spend]\ncall_cap_usd = 2\n";
    let source = Arc::new(FakeSource::of(snapshot_of(config, "", &kinds)));
    let executor = under(&world, &source, vec![chooses()]);
    let finished = executor.submit(&Call::new("asks")).unwrap();
    let (record, _) = stored(&world, finished.run());
    let settings = &record.asked().settings;
    let names: Vec<&str> = settings.iter().map(|setting| setting.name()).collect();
    // The backend the call resolved, and not the judge, which it did not need.
    assert_eq!(
        names,
        [
            "backends.fill.default.kind",
            "backends.fill.default.model",
            "backends.fill.default.settings.version",
            "spend.call_cap_micros",
        ]
    );
}

#[test]
fn settings_a_record_would_not_take_fail_the_admission_with_no_run() {
    let world = World::new();
    let (_, kinds) = fake_kinds();
    // A backend's name of 64 characters and a setting's key of 64 make a name longer
    // than a record takes: the config's reader takes both.
    let (name, key) = ("n".repeat(64), "k".repeat(64));
    let config = format!(
        "[backends.fill]\ndefault = \"{name}\"\n\n[backends.fill.{name}]\nkind = \"fake\"\n\n\
         [backends.fill.{name}.settings]\n{key} = \"x\"\n"
    );
    // The fake kind does not take the setting, so the config is read and no backend
    // is made of it: the backends of the snapshot are those of another config.
    let env = EnvFile::parse(b"").unwrap();
    let read = Config::parse(config.as_bytes(), &env.secrets()).unwrap();
    let ready = "[backends.fill]\nkind = \"fake\"\n";
    let offers = snapshot_of(ready, "", &kinds);
    let source = Arc::new(FakeSource::of(
        offers.clone().recording(Arc::new(Named(read))),
    ));
    let executor = under(&world, &source, vec![chooses()]);
    let id = executor.request_id("request-1").unwrap();
    let request = Request::new(Call::new("asks")).with_id(id);
    let failed = executor.request(&request).unwrap_err();
    let CallError::Config { why } = &failed else {
        panic!("not a failure of the config: {failed:?}");
    };
    assert!(
        why.contains("has a name longer than a record takes"),
        "{why}"
    );
    assert!(!why.contains(&key), "{why}");
    assert_eq!(runs(&world), 0);
    // A setting given twice, one more than a record lists, a name that holds a
    // secret and a number that holds one are failures too, and none is shown.
    let secret = made_up_key("T");
    let pair = |name: &str| (name.to_owned(), Value::Integer(1));
    for (pairs, says, hides) in [
        (
            vec![pair("a.b"), pair("a.b")],
            "the setting \"a.b\" is given twice",
            "",
        ),
        (
            (0..97).map(|at| pair(&format!("s.{at}"))).collect(),
            "more than the 96 settings",
            "",
        ),
        (
            vec![pair(&format!("a.{secret}"))],
            "holds a secret, and is not shown",
            secret.as_str(),
        ),
        (vec![pair("a b")], "is not one a record takes", ""),
        (
            vec![("a.n".to_owned(), Value::Integer(1234567890123))],
            "is not one a record takes",
            "1234567890123",
        ),
    ] {
        let secrets = Secrets::new([secret.clone(), "1234567890123".to_owned()]).unwrap();
        let given = offers.clone().recording(Arc::new(Given(pairs)));
        source.swap(given.with_secrets(secrets));
        let failed = executor.request(&request).unwrap_err();
        let CallError::Config { why } = &failed else {
            panic!("not a failure of the config: {failed:?}");
        };
        assert!(why.contains(says), "{why}");
        assert!(hides.is_empty() || !why.contains(hides), "{why}");
        assert_eq!(runs(&world), 0);
    }
    // Settings that panic leave nothing behind either.
    source.swap(offers.clone().recording(Arc::new(Panics)));
    let unwound = catch_unwind(AssertUnwindSafe(|| executor.request(&request)));
    assert!(unwound.is_err());
    assert_eq!(runs(&world), 0);
    // Put right, the same request runs under its id.
    source.swap(offers);
    assert!(matches!(executor.request(&request), Ok(Answer::Ran(_))));
    assert_eq!(runs(&world), 1);
}

/// The settings of a config, asked for the backend named with 64 characters whatever
/// the call resolved.
struct Named(Config);

impl Settings for Named {
    fn of(
        &self,
        _: &[(Capability, &str)],
        secrets: &Secrets,
    ) -> Result<Vec<(String, Value)>, NotJudged> {
        let name = "n".repeat(64);
        Settings::of(&self.0, &[(Capability::Fill, name.as_str())], secrets)
    }
}

/// Settings that are these pairs.
struct Given(Vec<(String, Value)>);

impl Settings for Given {
    fn of(&self, _: &[(Capability, &str)], _: &Secrets) -> Result<Vec<(String, Value)>, NotJudged> {
        Ok(self.0.clone())
    }
}

struct Panics;

impl Settings for Panics {
    fn of(&self, _: &[(Capability, &str)], _: &Secrets) -> Result<Vec<(String, Value)>, NotJudged> {
        panic!("the settings went wrong");
    }
}

// --- what a snapshot shows -------------------------------------------------------------------

#[test]
fn a_snapshot_prints_no_credential_no_secret_and_no_setting() {
    let key = made_up_key("P");
    let origin = Origin::of(API, &Secrets::none()).unwrap();
    let credential = Credential::new(origin, "x-key", &key, &Secrets::none()).unwrap();
    let snapshot = Snapshot::of(digest("one"))
        .capped_at(Estimate::of_micros(5))
        .with_credential(&there(), credential)
        .unwrap()
        .with_secrets(Secrets::new([key.clone()]).unwrap())
        .recording(Arc::new(Given(vec![(key.clone(), Value::text(&key))])));
    let printed = format!("{snapshot:?}");
    assert!(
        !printed.contains(&key) && !printed.contains(&key[..10]),
        "{printed}"
    );
    assert!(printed.contains("Secrets(1)"), "{printed}");
    // A snapshot that was given a credential and no policy sends under the policy
    // that approves nothing, and not under the executor's.
    assert_eq!(snapshot.policy(), Some(&DispatchPolicy::none()));
    let address = Address::of(&format!("{API}/v1"), &Secrets::none()).unwrap();
    assert_eq!(address.origin(), there().origin());
}
