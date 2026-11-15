//! What the tests of the egress strand share: made-up recipients, a config read from a
//! text, an executor over a store of its own, and what one run came to. Nothing here
//! sends anything: the sender behind every executor is the test kit's fake.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use whizbang_core::capability::{Fault, FillRequest, Filler, Picture, Support};
use whizbang_core::config::Config;
use whizbang_core::contract::record::{Handed, RunRecord, Status, Went};
use whizbang_core::contract::{
    Action, ActionDescriptor, Answer, Backends, Call, Capabilities, Capability, Clock, Context,
    Engine, Executor, Failure, Kind, PreparedCall, Produced, RefusalKind, Registry, Request,
    Requirement, RunId, Secrets,
};
use whizbang_core::egress::{
    Account, Address, Classes, DataClass, DispatchPolicy, Origin, Piece, Recipient, Reply, Ways,
};
use whizbang_core::identity::{CanonicalPixels, Digest};
use whizbang_core::store::{Kind as Stored, Reader, Store};
use whizbang_testkit::capabilities::roster;
use whizbang_testkit::fs_shim::ShimFs;
use whizbang_testkit::scratch::Scratch;

pub(super) const API: &str = "https://api.example.com";
pub(super) const UPSTREAM: &str = "https://uploads.example.net";
pub(super) const ELSEWHERE: &str = "https://elsewhere.example.org";

/// The account every test's recipient at [`API`] is under, and what a record shows of
/// it.
pub(super) const ACCOUNT: &str = "acct_4f8a1c9e77";
pub(super) const ACCOUNT_CUT: &str = "acct...";

/// A made-up key: the shape of one, and the key of nothing. It is put together at run
/// time, so that no file of the repository holds one whole.
pub(super) fn made_up_key(tail: &str) -> String {
    format!("sk{}abcdefghij0123456789ABCDEFGHIJ{tail}", "-")
}

pub(super) fn origin(address: &str) -> Origin {
    Origin::of(address, &Secrets::none()).unwrap()
}

pub(super) fn address(text: &str) -> Address {
    Address::of(text, &Secrets::none()).unwrap()
}

pub(super) fn account(text: &str) -> Account {
    Account::new(text, &Secrets::none()).unwrap()
}

/// The recipient at the API's origin, under the account, which may forward to the
/// upstream.
pub(super) fn recipient() -> Recipient {
    Recipient::new(origin(API))
        .with_account(account(ACCOUNT))
        .forwarding_to(vec![origin(UPSTREAM)])
        .unwrap()
}

/// The `egress` table that approves [`recipient`] for these classes, as a config
/// writes it.
pub(super) fn approving(classes: &[&str]) -> String {
    let classes: Vec<String> = classes.iter().map(|class| format!("{class:?}")).collect();
    format!(
        "[egress.recipients.words]\norigin = {API:?}\naccount = {ACCOUNT:?}\nupstreams = \
         [{UPSTREAM:?}]\nclasses = [{}]\n",
        classes.join(", ")
    )
}

/// The config a text is read as.
pub(super) fn config(text: &str) -> Config {
    match Config::parse(text.as_bytes(), &Secrets::none()) {
        Ok(config) => config,
        Err(refused) => panic!("refused: {refused}\n{text}"),
    }
}

/// The policy a config's `egress` table makes.
pub(super) fn policy_of(config: &Config) -> DispatchPolicy {
    DispatchPolicy::from_approvals(config.recipients().map(|(_, approval)| approval)).unwrap()
}

/// The policy the `egress` table of this text makes.
pub(super) fn policy(text: &str) -> DispatchPolicy {
    policy_of(&config(text))
}

pub(super) fn prompts() -> Classes {
    Classes::of(&[DataClass::Prompts])
}

pub(super) fn a_prompt() -> Vec<Piece> {
    vec![Piece::of(DataClass::Prompts, b"a red door".to_vec())]
}

pub(super) fn ok() -> Result<Reply, Fault> {
    Ok(Reply::answered(200, b"{}".to_vec()).unwrap())
}

pub(super) fn moved(to: &str) -> Result<Reply, Fault> {
    Ok(Reply::moved(address(to)))
}

/// A clock that says one millisecond more each time it is asked.
struct Ticking(AtomicU64);

impl Clock for Ticking {
    fn now_ms(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst)
    }
}

type Body = Box<dyn Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync>;

struct Scripted(ActionDescriptor, Body);

impl Action for Scripted {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        (self.1)(call, context)
    }
}

/// An action of this name with these needs, whose run is whatever the test says.
pub(super) fn action(
    name: &str,
    needs: Vec<Requirement>,
    body: impl Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync + 'static,
) -> Arc<dyn Action> {
    let mut draft = ActionDescriptor::draft(name, "1.0.0", Kind::Primitive, "A test action.");
    for need in needs {
        draft = draft.needs(need);
    }
    Arc::new(Scripted(draft.build().unwrap(), Box::new(body)))
}

/// An action with this descriptor, whose run is whatever the test says.
pub(super) fn scripted(
    descriptor: ActionDescriptor,
    body: impl Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync + 'static,
) -> Arc<dyn Action> {
    Arc::new(Scripted(descriptor, Box::new(body)))
}

/// A store of its own, in a folder that goes with the test.
pub(super) struct World {
    dir: Scratch,
    fs: Arc<ShimFs>,
}

impl World {
    pub(super) fn new() -> World {
        World {
            dir: Scratch::new("egress"),
            fs: Arc::new(ShimFs::new()),
        }
    }

    pub(super) fn root(&self) -> PathBuf {
        self.dir.path().join("store")
    }

    pub(super) fn store(&self) -> Arc<Store> {
        Arc::new(Store::open(&self.root(), self.fs.clone()).unwrap())
    }
}

/// An executor over the world's store, with these backends on offer and held.
pub(super) fn engine(
    world: &World,
    backends: impl Backends + 'static,
    held: Capabilities,
    actions: Vec<Arc<dyn Action>>,
) -> Engine {
    let mut registry = Registry::new();
    for action in actions {
        registry.register(action).unwrap();
    }
    Engine::over(
        registry,
        Arc::new(backends),
        world.store(),
        Digest::of_bytes(b""),
    )
    .with_clock(Arc::new(Ticking(AtomicU64::new(1000))))
    .with_capabilities(held)
}

/// The run's record, read from the store, and its bytes.
pub(super) fn stored(world: &World, run: &RunId) -> (RunRecord, Vec<u8>) {
    let reader = Reader::open(&world.root(), world.fs.clone()).unwrap();
    let generation = reader.snapshot().unwrap();
    let entries = generation.run(run.as_str()).unwrap().entries();
    let entry = &entries["record"];
    assert_eq!(entry.kind, Stored::Diagnostic);
    let bytes = reader.read(entry).unwrap();
    (RunRecord::from_bytes(&bytes).unwrap(), bytes)
}

pub(super) fn fill_request() -> FillRequest {
    let rgba = (0..24u8).flat_map(|at| [at, at, at, 255]).collect();
    let picture = Picture::new(CanonicalPixels::from_rgba(6, 4, rgba).unwrap()).unwrap();
    let mut plane = vec![0; 24];
    plane[..6].fill(255);
    FillRequest::new(picture, Support::from_plane(6, 4, plane).unwrap(), None).unwrap()
}

/// Every byte of every file under a folder, one file after another.
pub(super) fn every_byte_under(folder: &Path, into: &mut Vec<u8>) {
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

pub(super) fn holds(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

/// One entry of where a run's data went: the origin, the cut account, the classes
/// and the outcome.
pub(super) type Entry = (String, Option<String>, Classes, Handed);

pub(super) fn entry(origin: &str, account: Option<&str>, classes: Classes, how: Handed) -> Entry {
    (origin.to_owned(), account.map(str::to_owned), classes, how)
}

/// What one run that asked its filler once came to.
pub(super) struct Ran {
    pub(super) record: RunRecord,
    /// The record's bytes.
    pub(super) bytes: Vec<u8>,
    /// The fault the action was answered with, when it was.
    pub(super) fault: Option<Fault>,
    /// Every byte of every file under the store, after the run.
    pub(super) store: Vec<u8>,
}

impl Ran {
    /// Where the run's data went.
    pub(super) fn went(&self) -> Vec<Entry> {
        let entry = |went: &Went| {
            (
                went.origin().to_owned(),
                went.account().map(str::to_owned),
                went.classes(),
                went.outcome(),
            )
        };
        self.record.egress().iter().map(entry).collect()
    }

    /// The run's refusals of a request: each kind and sentence.
    pub(super) fn refused(&self) -> Vec<(RefusalKind, String)> {
        let refusals = &self.record.outcome().refusals;
        refusals
            .iter()
            .map(|refused| {
                assert_eq!(refused.action(), None);
                (refused.kind(), refused.why().to_owned())
            })
            .collect()
    }
}

/// Runs one action that asks this filler once, catches whatever it is answered and
/// completes, on an executor with these ways out and these secrets.
pub(super) fn run_one(filler: Arc<dyn Filler>, ways: Ways, secrets: Secrets) -> Ran {
    run_in(&World::new(), filler, ways, secrets)
}

/// The same, over this world's store.
pub(super) fn run_in(world: &World, filler: Arc<dyn Filler>, ways: Ways, secrets: Secrets) -> Ran {
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
    let executor = engine(
        world,
        roster(None, Some(filler.as_ref()), None),
        Capabilities::none().with_fill(filler.clone()),
        vec![asks],
    )
    .with_secrets(secrets)
    .with_ways(ways);
    let Ok(Answer::Ran(finished)) = executor.request(&Request::new(Call::new("asks"))) else {
        panic!("the request did not run");
    };
    let (record, bytes) = stored(world, finished.run());
    assert_eq!(record.outcome().status, Status::Completed);
    let fault = answered.lock().unwrap().take();
    let mut store = Vec::new();
    every_byte_under(&world.root(), &mut store);
    Ran {
        record,
        bytes,
        fault,
        store,
    }
}
