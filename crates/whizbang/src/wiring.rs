//! Wiring: the registry and the executor of this build, each made in one place.
//!
//! The binary calls [`registry`] and describes what it holds. A test calls the same
//! function and then registers its fakes, so what a test runs against is the shipping
//! registry and nothing the binary would not also have, plus what the test added. Fakes
//! live in `whizbang-testkit`, which this crate depends on for its tests only.

use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use whizbang_core::artifact::Contents;
use whizbang_core::config::{Config, Credentials, Files, Kinds, Offers};
use whizbang_core::contract::{
    Action, ConfigSource, Engine, Interrupted, NoBackends, Opening, Place, Recovery, Registry,
    Secrets, Snapshot, SourceRefused, SystemClock,
};
use whizbang_core::description::Description;
use whizbang_core::egress::DispatchPolicy;
use whizbang_core::spend::{CallCap, Meter, PerCall, PreparationAge, StoreLedger};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{self, Store};

/// Every action this build ships, registered in the order a door lists them: the
/// recipe first, then the primitives. Tools add theirs here as they are ported. What
/// comes back otherwise is why an action could not be registered, in a sentence.
///
/// `check` and `view` are squint's (ADR 0009). They are described, and no command runs
/// them yet: `whizbang squint check` is milestone 2's. `check` needs a judge, and this
/// build has no kind of backend ([`kinds`]), so on this build it is prepared as not
/// ready; `view` needs none, and is what `check` calls.
pub fn registry() -> Result<Registry, String> {
    let said = |error: &dyn std::fmt::Display| error.to_string();
    let mut registry = Registry::new();
    let toolbox = whizbang_squint::check::Toolbox::standard();
    let check = whizbang_squint::check::Check::new(toolbox).map_err(|e| said(&e))?;
    let grow = whizbang_rascal::grow::Grow::new();
    let view = whizbang_squint::view::View::new().map_err(|e| said(&e))?;
    let actions: [Arc<dyn Action>; 3] = [Arc::new(check), Arc::new(grow), Arc::new(view)];
    for action in actions {
        registry.register(action).map_err(|e| said(&e))?;
    }
    Ok(registry)
}

/// How many requests one call may send one backend, which is how many times the
/// bound of one request an admission reserves for each backend the call needs
/// (`spend::PerCall`). It is the most any action of this build sends one backend:
/// the turns of `check`'s judge loop, `whizbang_squint::check::TURNS_LIMIT`, which a
/// test holds this number to. `grow` and `view` send none.
///
/// Nothing ties the number to an action: one that sent a backend more requests than
/// this would have reserved for too few. The meter then refuses the request that
/// its run has nothing left for, unsent, so the cost of a number that is too small
/// is a run that fails, never a request that was not counted. And a call reserves
/// this many bounds whether or not it sends that many: what it did not spend is
/// released when its run ends.
pub const REQUESTS_PER_CALL: u64 = 16;

/// The kinds of backend this build has (ADR 0011, "Kinds"): none. No network client
/// and no local model is built at this checkpoint, so a backend a config names is of
/// a kind this build does not have, and is not ready. A test registers a fake.
pub fn kinds() -> Kinds {
    Kinds::new()
}

/// What this build's executor is built over, beside its actions and its store: the
/// config file and its env file, as the command line names them, the kinds of
/// backend, and the secrets it is told of from the start. What `{:?}` prints is
/// whether each file is named, how many kinds there are and how many secrets: no
/// path and no secret.
#[derive(Clone, Default)]
pub struct Over {
    config: Option<PathBuf>,
    env: Option<PathBuf>,
    kinds: Kinds,
    secrets: Secrets,
}

impl Over {
    /// No config, no env file, no kind and no secret: what a command that names no
    /// config gives. An executor over it is under the empty config (ADR 0011, "Where
    /// the file is").
    pub fn nothing() -> Over {
        Over::default()
    }

    /// The same, under this config file (`--config`), or under none.
    pub fn config(mut self, config: Option<&Path>) -> Over {
        self.config = config.map(Path::to_owned);
        self
    }

    /// The same, with the values of the variables a config names read from this env
    /// file (`--env-file`), or from none.
    pub fn env_file(mut self, env: Option<&Path>) -> Over {
        self.env = env.map(Path::to_owned);
        self
    }

    /// The same, with these kinds of backend. The binary gives [`kinds`]; a test
    /// gives that with its fakes registered.
    pub fn kinds(mut self, kinds: Kinds) -> Over {
        self.kinds = kinds;
        self
    }

    /// The same, told of these secrets from the start. **They are given once, here,
    /// and to both:** to the executor, which takes them out of everything it writes
    /// (`Engine::with_secrets`), and to the source of its config, which refuses a
    /// config that holds one (`Files::knowing`). Nothing else gives either its
    /// secrets, so the two never know two sets (ADR 0011, "The source over both
    /// files").
    pub fn knowing(mut self, secrets: Secrets) -> Over {
        self.secrets = secrets;
        self
    }
}

impl std::fmt::Debug for Over {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Over")
            .field("config", &self.config.is_some())
            .field("env_file", &self.env.is_some())
            .field("kinds", &self.kinds)
            .field("secrets", &self.secrets)
            .finish()
    }
}

impl Over {
    /// The source over the two files, when a config is named: the kinds of the
    /// build, and the secrets it was told of.
    fn files(&self) -> Option<Files> {
        let config = self.config.as_deref()?;
        let files = Files::new(config, self.env.as_deref(), self.kinds.clone());
        Some(files.knowing(self.secrets.clone()))
    }

    /// Where an executor built over this reads its config (ADR 0015, "One snapshot
    /// of the config"). With a config named it is the source over the config file
    /// and its env file, which reads both again each time it is asked. With none it
    /// is one snapshot of the empty config: its digest, no backend, no cap, the
    /// policy that approves nothing, and no setting. **Either way a snapshot holds
    /// backends,** those a config offers or none at all, so the executor asks and
    /// hands over the snapshot's and never any it was built with: a call with no
    /// backend to hand is refused as not ready, before admission.
    ///
    /// **The backends of every snapshot stand behind `spend::PerCall`,** so the
    /// bound a call is prepared with, held to a cap by and admitted with covers
    /// [`REQUESTS_PER_CALL`] requests to each backend it needs.
    fn source(&self) -> Result<Arc<dyn ConfigSource>, String> {
        let requests = NonZeroU64::new(REQUESTS_PER_CALL)
            .ok_or("a call sends a backend one request at least")?;
        if let Some(files) = self.files() {
            return Ok(Arc::new(Reserving { files, requests }));
        }
        let empty = Config::empty();
        let offers = Offers::of(&empty, &Credentials::none(), &self.kinds)
            .map_err(|error| error.to_string())?;
        let snapshot = Snapshot::of(empty.digest())
            .offering(Arc::new(offers))
            .sending_under(DispatchPolicy::none())
            .recording(Arc::new(empty));
        Ok(Arc::new(per_call(snapshot, requests)))
    }
}

/// This snapshot, with its backends behind `spend::PerCall` for this many requests.
fn per_call(snapshot: Snapshot, requests: NonZeroU64) -> Snapshot {
    snapshot.offering_through(|offered| Arc::new(PerCall::new(offered, requests)))
}

/// The source over the config's files, each of whose snapshots holds its backends
/// behind `spend::PerCall`.
struct Reserving {
    files: Files,
    requests: NonZeroU64,
}

impl ConfigSource for Reserving {
    fn snapshot(&self) -> Result<Snapshot, SourceRefused> {
        Ok(per_call(self.files.snapshot()?, self.requests))
    }
}

/// Reads the config a command named, as its executor's source will read it, and
/// answers nothing or why it cannot be used: the readers' own refusal, which names
/// the key of the config, escaped and cut, or the line of the env file, and never a
/// path (ADR 0011, "What a refusal says"). A call is told less, in fixed words; a
/// command says this before it opens its store. With no config named there is
/// nothing to read.
pub fn loaded(over: &Over) -> Result<(), String> {
    match over.files().map(|files| files.load()) {
        Some(Err(refused)) => Err(format!("the config cannot be used: {refused}")),
        Some(Ok(_)) | None => Ok(()),
    }
}

/// What every executor of this build is given, whichever way it holds its store:
/// the source of its config, the secrets, and the parts of an admission.
///
/// - **The source** is [`Over::source`], and the executor is built with no backend
///   of its own: it goes by the snapshot the source answers, and by nothing else.
/// - **The secrets** are those the source was told of ([`Over::knowing`]).
/// - **The parts of an admission,** in the order they are asked (ADR 0015, "A part
///   in the admission"): the age of the preparation a request came with
///   (`spend::PreparationAge`), then the cap of one call (`spend::CallCap`). A
///   preparation that is too old is refused as stale before its estimate, which is
///   as old, is held to a cap. Neither writes a record into an admission.
fn wired(engine: Engine, over: &Over) -> Result<Engine, String> {
    Ok(engine
        .with_config(over.source()?)
        .with_secrets(over.secrets.clone())
        .with_part(Arc::new(PreparationAge))
        .with_part(Arc::new(CallCap)))
}

/// The executor of this build over a registry and a read of a store: it prepares calls
/// and runs none. The binary gives it [`registry`]; a test gives it that registry with
/// its fakes added. It is wired as [`runner`] is.
pub fn executor(
    registry: Registry,
    store: Arc<dyn Contents + Send + Sync>,
    over: &Over,
) -> Result<Engine, String> {
    // The digest an executor is built with is the one it has until it is given a
    // source, which is on the next line: it is never asked.
    let digest = Config::empty().digest();
    wired(
        Engine::new(registry, Arc::new(NoBackends), store, digest),
        over,
    )
}

/// The executor of this build that runs actions: over a registry and a store opened to
/// write, which it reads and writes its runs to. `whizbang rascal grow` runs its action
/// on it, and the tests run theirs, with their fakes registered. What it is given is
/// under [`wired`], and with it the money (`SPEC.md` section 6; ADR 0016):
///
/// - **the real ledger** (`spend::StoreLedger`), which reserves what a call may cost
///   at its admission against the allowance the owner's reset set, and closes the
///   reservation when the run ends;
/// - **the meter** (`spend::Meter`), in the meter's place between each handle and
///   its backend, over the same store: it debits each paid request from what its
///   run reserved before the request leaves, settles it when it ends, and notes it
///   in the run's record.
///
/// **The three are given together, here and nowhere else:** the ledger, the bound of
/// a call ([`REQUESTS_PER_CALL`], in the source) and the meter. With the meter alone a
/// bounded request of a run that reserved nothing passes unmetered; with the ledger
/// alone nothing a run sends is charged to what it reserved.
///
/// What comes back otherwise is why the executor could not be built.
pub fn runner(registry: Registry, store: Arc<Store>, over: &Over) -> Result<Engine, String> {
    let digest = Config::empty().digest();
    let meter = Meter::new(store.clone(), Arc::new(SystemClock));
    let engine = Engine::over(registry, Arc::new(NoBackends), store, digest);
    Ok(wired(engine, over)?
        .with_ledger(Arc::new(StoreLedger))
        .with_wrapper(Place::Meter, Arc::new(meter)))
}

/// What recovers a store before this build is handed its writer (ADR 0015, "One way
/// to a writer, with recovery before it"): the parts, in the order they are asked.
/// One, so far: the part that ends every run a dead process left, as interrupted
/// (ADR 0015, "The interrupted record, and what readers say of it"). The one that
/// would have closed what such a run reserved was the spend strand's, which keeps none
/// (ADR 0016, "The strand cut down"): the reservation of a run a dead process left
/// stays open and counted in full, and the owner's reset clears it.
pub fn recovery() -> Recovery {
    Recovery::none().with_part(Arc::new(Interrupted))
}

/// Whether a command that opens a store to write makes one where there is none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Make {
    /// The store must be there: nothing is made, and nothing is completed.
    Nothing,
    /// The store is made where there is none, its last directory alone: a missing
    /// parent is refused.
    TheRoot,
}

/// Opens a store to write: **the one place this binary does** (a test holds its
/// sources to that). The writer lock is taken here; the store is recovered, by
/// `recovery`, before the opening hands anyone its writer. A command gives
/// [`recovery`]; a test gives that with its fakes.
pub fn opening(root: &Path, make: Make, recovery: Recovery) -> Result<Opening, store::Error> {
    match make {
        Make::Nothing => Opening::open_existing(root, Arc::new(RealFs), recovery),
        Make::TheRoot => Opening::open_in_existing_parent(root, Arc::new(RealFs), recovery),
    }
}

/// What a command says when a store could not be opened to write: the store's own
/// sentence, which names the store by the path the caller gave and is escaped where
/// it is printed. Of a store that is held it says one thing more, because the store
/// says "another process" where the holder has not written its name yet, and the
/// holder can be this process: a second opening of one store in one process is
/// refused as any second writer is.
pub fn not_opened(error: &store::Error) -> String {
    match error {
        store::Error::Held { .. } => format!(
            "{error} (the writer that holds it is another opening of the store, in \
             another process or in this one)"
        ),
        other => other.to_string(),
    }
}

/// The static description of a build of this crate's version that holds this registry.
pub fn description(registry: &Registry) -> Description {
    Description::new(env!("CARGO_PKG_VERSION"), registry)
}

/// The description as `whizbang describe` prints it for a person: the build, then each
/// action's name and purpose.
pub fn describe_text(description: &Description) -> String {
    let mut text = format!("{} {}\n", description.name(), description.version());
    if description.actions().is_empty() {
        text.push_str("no actions yet\n");
    }
    for action in description.actions() {
        text.push_str(&format!("  {}  {}\n", action.name(), action.purpose()));
    }
    text
}
