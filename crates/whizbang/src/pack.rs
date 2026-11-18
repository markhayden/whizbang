//! The kind `pack` (plan 1C, task P6a; ADR 0011, "Kinds", "The kind `pack`"): a local
//! segmenter, answered by the pack's worker through the one supervisor of a process
//! (decision D4).
//!
//! **One worker in a process.** [`OneWorker`] is made once by whoever builds the
//! executor, given to the kinds of the build ([`crate::wiring::kinds`]) and to the
//! wiring ([`crate::wiring::Over::pack`]), which tells it the models directory it opened
//! and the catalogue it is over. Every backend the kind makes, under every snapshot of
//! the config, holds the same one. Its supervisor is made at the first request for a
//! local model, and the supervisor starts the worker then (D7): never at start-up, and
//! never for a run that needs none. The worker is stopped when the last holder of the
//! one worker lets it go.
//!
//! **Where the worker is.** The binary's one worker ([`OneWorker::installed`]) starts
//! the worker itself, the pack's entry point [`Launch::worker`] names, from the
//! installation `whizbang setup` made in the models directory ([`installed_launch`]):
//! its Python and the pack's source, never a path compiled into the binary (ADR 0021).
//! Where either is missing a request is refused, `unavailable`, never sent, saying to
//! run `whizbang setup`. The worker is told the models directory the wiring opened, the
//! same one the supervisor reads the counts of removals in. A test hands the wiring a
//! worker of its own (`OneWorker::starting`); the binary never does, and cannot: that
//! seam, and `Launch::testing` beneath it, are compiled only with the feature `fakes`,
//! which only the tests' builds turn on.
//!
//! **A backend's table.** The capability `segment` alone; a model, which is a model's
//! name; no url, no provider and no credential, since nothing is sent anywhere; and
//! these settings, each refused in fixed words where the config is read when it breaks
//! its rule, and any other setting refused:
//!
//! | Setting | Is | Rule |
//! |---|---|---|
//! | `threshold`, `pad`, `min_cover` | A model's setting, sent in its `load` | A text of a number from 0 to 1, as `"0.2"` |
//! | `outline` | A model's setting, sent in its `load` | A text |
//! | `call_limit_s` | The kind's limit for a call | A whole number of seconds from 1 to 120; 120 when not given |
//! | `max_lifetime_s` | The lifetime of a worker a request starts | A whole number of seconds from 1 to 31,536,000; 86,400 when not given |
//!
//! A text of a model's setting is held to the protocol's rule for one (ADR 0020): no
//! `/`, no `\`, printable ASCII, so no `load` is refused for it when it is written.
//!
//! **What it declares:** the model, of the backend `pack`, at the version `local`,
//! with the features the catalogue's entry for it states; no recipient and no price,
//! so a call to it may cost nothing and is admitted without the ledger (decision D16).
//! A roster over the inventory answers it under its installation's identity.
//!
//! **One request** goes to the queue with its run's cancellation and deadline, the
//! kind's limit for a call and the lifetime of a worker it starts (task P6a). It names
//! the installation the run was handed its backend under, the identity its admission
//! recorded, which its lease holds ([`Bounds::installation`]), and never the one the
//! model's pointer names now; and it is queued with the count of removals that lease's
//! hold read ([`Bounds::removals`]), never one read later (task P6b; decision D5). A
//! request handed neither is refused, never sent ([`NOT_LEASED`]).
//!
//! **Its answer says where it was answered** (task P7a; ADR 0025): on the device the
//! worker's result names, and never from a cache, since the kind has none. The run's
//! record keeps both.
//!
//! **An installation's files are verified before it is loaded** (decision D14): the
//! first request for an installation in a process reads each of its files whole and
//! holds it to the size and the hash it was installed with. A damaged one is not
//! loaded: the request is refused, `unavailable` and never sent, saying the model is
//! damaged ([`DAMAGED`]), and it is read again at the next request, so that an install
//! that repairs it is seen. One found whole is not read again in the process: a file
//! damaged after that is loaded as it is, when the worker loads the model again.
//!
//! **A later process trusts the last full check while the files are unchanged** (plan
//! 2A, task R1): a full check that passes leaves a record beside the installation of
//! each file's size, time of last write, inode and hash, and the first request of a
//! later process that finds every file with the same three loads it without reading
//! the files again; any change, or a record missing or damaged, reads them all
//! ([`whizbang_backends::models::verify_checked`]).

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use whizbang_backends::models::{
    Catalogue, InstallationId, ModelName, ModelsDir, RemoveRefused, Removed, remove_and_tell,
    verify_checked,
};
use whizbang_backends::pack::proto::{self, Answer, Ask, Scalar};
use whizbang_backends::pack::queue::{Request, Sent, Status, Supervisor};
use whizbang_backends::pack::worker::CALL_LIMIT;
use whizbang_backends::pack::{Called, Ended, Launch, Limits, Said, Worker};
use whizbang_core::capability::{
    Bounds, Declaration, Device, Fault, FaultKind, Instance, SegmentRequest, SegmentResponse,
    Segmenter, Served,
};
use whizbang_core::config::{Backend, Kind, KindRefused, Setting, Table};
use whizbang_core::contract::{Capability, Clock, Made, ModelIdentity, Secrets, SystemClock};

use crate::pack_setup;

/// The name a config gives the kind: `kind = "pack"`.
pub const KIND: &str = "pack";

/// The version a backend of the kind declares its model at. A roster over the
/// inventory answers the model under its installation's identity in its place.
pub const VERSION: &str = "local";

/// The lifetime of a worker, when a backend's table gives none: a day.
pub const LIFETIME_DEFAULT_S: u32 = 86_400;

/// The longest lifetime a backend may give a worker: a year, the most the worker
/// takes (`pack/src/whizbang_pack/worker.py`, `LIFETIME_MOST`).
pub const LIFETIME_MOST_S: u32 = 31_536_000;

/// The settings of a model the kind knows, each sent in the model's `load`.
pub const MODEL_SETTINGS: [&str; 4] = ["min_cover", "outline", "pad", "threshold"];

/// How long a removal waits between two looks at a lock or a hold.
const PAUSE: Duration = Duration::from_millis(250);

/// What the kind says of a table of another capability.
pub const NOT_SEGMENT: &str = "the kind pack answers segment and no other capability";
/// What it says of a table that names no model.
pub const NO_MODEL: &str = "a backend of the kind pack names the model it runs";
/// What it says of a model that is not a model's name.
pub const BAD_MODEL: &str = "the model a backend of the kind pack names is not a model's name: lower-case letters, digits, _ and -";
/// What it says of a model the catalogue the wiring is over does not hold. The kind
/// declares what the catalogue's entry states, and with no entry there is nothing to
/// declare: said here, before a call could be told only that a feature is missing.
pub const NOT_IN_CATALOGUE: &str =
    "the model a backend of the kind pack names is not in the catalogue";
/// What it says of a table with a url, a provider or a credential.
pub const NOT_LOCAL: &str =
    "a backend of the kind pack is local: it takes no url, no provider and no credential";
/// What it says of a setting it does not know.
pub const UNKNOWN_SETTING: &str = "a backend of the kind pack takes the settings call_limit_s, max_lifetime_s, min_cover, outline, pad and threshold, and was given another";
/// What it says of `max_lifetime_s` outside its rule.
pub const BAD_LIFETIME: &str = "max_lifetime_s is a whole number of seconds from 1 to 31536000";
/// What it says of `call_limit_s` outside its rule.
pub const BAD_CALL_LIMIT: &str = "call_limit_s is a whole number of seconds from 1 to 120";
/// What it says of `threshold`, `pad` or `min_cover` outside its rule.
pub const BAD_FRACTION: &str =
    "threshold, pad and min_cover are each a text of a number from 0 to 1, such as 0.2 in quotes";
/// What it says of `outline` that is not a text.
pub const BAD_OUTLINE: &str = "outline is a text";
/// What it says of a model's setting whose text the protocol cannot carry.
pub const BAD_TEXT: &str = "a text of a setting of the kind pack is 1 to 1024 printable ASCII characters with no space at either end, and no / or \\";
/// What a request is told when a file of the installation it names is not the size or
/// the hash it was installed with (decision D14): nothing was loaded.
pub const DAMAGED: &str = "the model's installation is damaged: a file of it is not as it was installed; nothing was loaded and the request was not sent; install the model again with whizbang models get";
/// What a request is told when the installation's files could not be read to verify.
pub const UNVERIFIED: &str = "the files of the model's installation could not be read to verify them; nothing was loaded and the request was not sent";
/// What a request is told whose run was cancelled while its installation's files
/// were being verified.
pub const VERIFY_CANCELLED: &str = "the run was cancelled while the model's files were being verified; nothing was loaded and the request was not sent";
/// What a request is told whose run's deadline passed while its installation's files
/// were being verified.
pub const VERIFY_TIMED_OUT: &str = "the run's deadline passed while the model's files were being verified; nothing was loaded and the request was not sent";
/// What a request is told that was not handed under an installation its lease held.
pub const NOT_LEASED: &str =
    "the request was not handed an installation of the model held by a lease; it was not sent";
/// What a request is told when the one worker was told no models directory.
const NO_DIRECTORY: &str =
    "no models directory was opened for the pack's worker; the request was not sent";
/// What it says of a model and features no declaration takes.
const NOT_DECLARED: &str =
    "a backend of the kind pack cannot declare that model with those features";

/// What makes the worker the supervisor runs, given the models directory the wiring
/// opened and the secrets of the run whose request makes the supervisor; or why it
/// is not made, as a request's fault.
pub type Start = Arc<dyn Fn(&Path, &Secrets) -> Result<Worker, Fault> + Send + Sync>;

/// What the binary starts, for the models directory at `root`: the worker itself,
/// from the Python and the pack's source of the installation `whizbang setup` made
/// there (`pack_setup::at`), with this lifetime, and nothing more on its command line.
pub fn installed_launch(root: &Path, lifetime_s: u32) -> Launch {
    let place = pack_setup::at(root);
    Launch::worker(place.python, place.source, root.to_path_buf(), lifetime_s)
}

/// Where the one worker is: the models directory the wiring opened, by its folder and
/// as it was opened, and the catalogue the wiring is over.
#[derive(Clone)]
struct Place {
    root: PathBuf,
    dir: ModelsDir,
    catalogue: Catalogue,
}

#[derive(Default)]
struct Held {
    place: Option<Place>,
    supervisor: Option<Arc<Supervisor>>,
    /// The installations whose files were found whole in this process (decision D14):
    /// each is verified once. Kept by the model and the identity both, though the
    /// identity alone would do (the model's name is part of what it is the digest
    /// of, and `verify` holds the record to both): a key that says so needs no proof.
    verified: BTreeSet<(ModelName, InstallationId)>,
}

/// The one worker of a process (decision D4): what makes it, where it is, and its
/// supervisor once the first request made it. What `{:?}` prints is whether it is
/// placed and whether its supervisor was made.
pub struct OneWorker {
    start: Start,
    held: Mutex<Held>,
    /// Told when the supervisor is made.
    made: Condvar,
}

impl std::fmt::Debug for OneWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let held = self.held();
        f.debug_struct("OneWorker")
            .field("placed", &held.place.is_some())
            .field("supervisor", &held.supervisor.is_some())
            .finish()
    }
}

impl OneWorker {
    /// The binary's one worker: the worker itself, from the installation `whizbang
    /// setup` made in the models directory ([`installed_launch`]), on the machine's
    /// clock. A request is refused, `unavailable` and never sent, saying to run
    /// `whizbang setup`, where the pack's source is not there; the supervisor refuses
    /// it the same way where its Python is not.
    pub fn installed() -> Arc<OneWorker> {
        let start: Start = Arc::new(|root: &Path, secrets: &Secrets| {
            let launch = installed_launch(root, LIFETIME_DEFAULT_S);
            if !launch.source.is_dir() {
                let words = "the pack's source is not installed in the models directory; run whizbang setup";
                return Err(Fault::new(FaultKind::Unavailable, words).not_sent());
            }
            let clock: Arc<dyn Clock> = Arc::new(SystemClock);
            Ok(Worker::new(launch, clock, secrets.clone()))
        });
        Arc::new(OneWorker {
            start,
            held: Mutex::default(),
            made: Condvar::new(),
        })
    }

    /// A one worker that `start` makes: a test's, which starts the pack's fakes
    /// through the test kit's guard. It is built only with the feature `fakes`,
    /// which only the tests' builds turn on (this crate's development dependency on
    /// itself), so the binary `just smoke` and a release build make cannot name it.
    #[cfg(feature = "fakes")]
    pub fn starting(start: Start) -> Arc<OneWorker> {
        Arc::new(OneWorker {
            start,
            held: Mutex::default(),
            made: Condvar::new(),
        })
    }

    fn held(&self) -> MutexGuard<'_, Held> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Tells it the models directory the wiring opened, at this folder, and the
    /// catalogue the wiring is over. Told again, the last is kept; but once its
    /// supervisor was made over one directory, another is refused: a process has one
    /// worker, told one models directory.
    pub(crate) fn place(
        &self,
        root: &Path,
        dir: ModelsDir,
        catalogue: Catalogue,
    ) -> Result<(), String> {
        let mut held = self.held();
        let other = held.place.as_ref().is_some_and(|place| place.root != root);
        if other && held.supervisor.is_some() {
            return Err(
                "the pack's worker of this process runs over another models directory; one process has one worker"
                    .to_owned(),
            );
        }
        held.place = Some(Place {
            root: root.to_path_buf(),
            dir,
            catalogue,
        });
        Ok(())
    }

    /// The features the catalogue's entry for this model states: none when the
    /// worker was told no models directory, where no local backend is ready
    /// whatever it declares; and nothing at all when the catalogue does not hold
    /// the model.
    fn features(&self, model: &str) -> Option<Vec<String>> {
        let held = self.held();
        let Some(place) = held.place.as_ref() else {
            return Some(Vec::new());
        };
        let entry = place.catalogue.entry(model)?;
        Some(entry.features().map(str::to_owned).collect())
    }

    /// The supervisor, made now if no request made it before.
    fn supervisor(&self, secrets: &Secrets) -> Result<Arc<Supervisor>, Fault> {
        let mut held = self.held();
        let Some(place) = held.place.clone() else {
            return Err(Fault::new(FaultKind::Unavailable, NO_DIRECTORY).not_sent());
        };
        if let Some(supervisor) = &held.supervisor {
            return Ok(supervisor.clone());
        }
        let worker = (self.start)(&place.root, secrets)?;
        let supervisor = Arc::new(Supervisor::new(worker, place.dir));
        held.supervisor = Some(supervisor.clone());
        self.made.notify_all();
        Ok(supervisor)
    }

    /// Verifies the files of this installation of this model, the first time it is
    /// asked in this process, before anything loads it (decision D14): each is read
    /// whole and held to the size and the hash it was installed with, unless the
    /// record of a full check that passed in this or an earlier process says each is
    /// unchanged since ([`whizbang_backends::models::verify_checked`]). One found
    /// whole is not read again in this process; one found damaged is read again the
    /// next time, so that an
    /// install that repairs it is seen. The installation is the one the request's
    /// lease holds, so nothing deletes it while it is read. The reading ends, never
    /// sent, at the run's cancel (`cancelled`) or its deadline (`timed_out`), looked
    /// at before each piece of a file, so a cancelled run does not wait for gigabytes
    /// to be hashed.
    fn verified(
        &self,
        model: &ModelName,
        installation: InstallationId,
        secrets: &Secrets,
        bounds: &Bounds,
    ) -> Result<(), Fault> {
        let key = (model.clone(), installation);
        let dir = {
            let held = self.held();
            if held.verified.contains(&key) {
                return Ok(());
            }
            let Some(place) = held.place.as_ref() else {
                return Err(Fault::new(FaultKind::Unavailable, NO_DIRECTORY).not_sent());
            };
            place.dir.clone()
        };
        let past = || {
            bounds
                .deadline()
                .is_some_and(|deadline| Instant::now() >= deadline)
        };
        let stop = || bounds.is_cancelled() || past();
        // Read without the lock: hashing a model's files takes time, and another
        // request may verify the same installation meanwhile, to the same end.
        match verify_checked(&dir, model, installation, secrets, &stop) {
            Ok(true) => {
                self.held().verified.insert(key);
                Ok(())
            }
            Ok(false) => Err(Fault::new(FaultKind::Unavailable, DAMAGED).not_sent()),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
                let fault = match bounds.is_cancelled() {
                    true => Fault::new(FaultKind::Cancelled, VERIFY_CANCELLED),
                    false => Fault::new(FaultKind::TimedOut, VERIFY_TIMED_OUT),
                };
                Err(fault.not_sent())
            }
            Err(_) => Err(Fault::new(FaultKind::Unavailable, UNVERIFIED).not_sent()),
        }
    }

    /// Starts a worker as a request would, hears its hello, and stops it, waiting for
    /// it to end (decision D7): what `whizbang backends status --probe` asks (task
    /// P7b). It is not the supervisor's: no request is sent to it, and it is gone
    /// before this returns. Refused, with nothing started: while the worker was told
    /// no models directory, and once a request has made the supervisor, since one
    /// process has one worker (decision D4). Otherwise, what the worker said, or the
    /// fault that says why it did not say hello.
    pub fn hello(&self, secrets: &Secrets) -> Result<Said, Fault> {
        let root = {
            let held = self.held();
            if held.supervisor.is_some() {
                let words = "the pack's worker of this process is running already; one process has one worker";
                return Err(Fault::new(FaultKind::Unavailable, words).not_sent());
            }
            let Some(place) = held.place.as_ref() else {
                let words =
                    "no models directory was opened for the pack's worker; it was not started";
                return Err(Fault::new(FaultKind::Unavailable, words).not_sent());
            };
            place.root.clone()
        };
        let mut worker = (self.start)(&root, secrets)?;
        let ready = worker.ready();
        let said = worker.said().cloned();
        worker.stop();
        ready?;
        said.ok_or_else(|| {
            let words = "the pack's worker was started and said no hello";
            Fault::new(FaultKind::Unavailable, words).not_sent()
        })
    }

    /// The supervisor, if a request made it.
    fn made(&self) -> Option<Arc<Supervisor>> {
        self.held().supervisor.clone()
    }

    /// What the supervisor is doing now; nothing while no request has made it.
    pub fn status(&self) -> Option<Status> {
        self.made().map(|supervisor| supervisor.status())
    }

    /// The requests written to a worker, the last the supervisor keeps; none while no
    /// request has made it.
    pub fn journal(&self) -> Vec<Sent> {
        self.made()
            .map(|supervisor| supervisor.journal())
            .unwrap_or_default()
    }

    /// Waits until a request has made the supervisor and its status is as `until`
    /// says, or `within` has passed: on the machine's clock until the supervisor is
    /// made, then on its own ([`Supervisor::wait_for`]). False when it passed first.
    pub fn wait_for(&self, within: Duration, until: &dyn Fn(&Status) -> bool) -> bool {
        let begun = Instant::now();
        let mut held = self.held();
        let supervisor = loop {
            if let Some(supervisor) = &held.supervisor {
                break supervisor.clone();
            }
            let left = within.saturating_sub(begun.elapsed());
            if left.is_zero() {
                return false;
            }
            held = self
                .made
                .wait_timeout(held, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        };
        drop(held);
        supervisor.wait_for(within.saturating_sub(begun.elapsed()), until)
    }

    /// Ends the supervisor, without waiting for the call in the worker
    /// ([`Supervisor::kill`]): what the test kit's guard calls. Says how the worker
    /// ended; nothing when none was running or no request made the supervisor.
    pub fn kill(&self) -> Option<Ended> {
        self.made().and_then(|supervisor| supervisor.kill())
    }

    /// Removes this model from the models directory the worker is told of, as
    /// `whizbang_backends::models::remove` does, telling the supervisor each time the
    /// mark is set ([`remove_and_tell`], [`Supervisor::removing`]): the requests queued
    /// for the model end, and it is unloaded when it is resident and idle. **This is
    /// how a process that holds the worker removes a model** (decision D5). A removal
    /// in another process, as `whizbang models remove`, which makes no worker, is read
    /// by the supervisor in the model's count.
    pub fn remove(
        &self,
        model: &ModelName,
        patience: Duration,
        secrets: &Secrets,
    ) -> Result<Removed, RemoveRefused> {
        let (place, supervisor) = {
            let held = self.held();
            (held.place.clone(), held.supervisor.clone())
        };
        let Some(place) = place else {
            return Err(RemoveRefused::NotInstalled);
        };
        let clock = SystemClock;
        let mut pause = || clock.pause(PAUSE);
        let mut marked = || {
            if let Some(supervisor) = &supervisor {
                supervisor.removing(model);
            }
        };
        remove_and_tell(
            &place.dir,
            model,
            &clock,
            patience,
            secrets,
            &mut pause,
            &mut marked,
        )
    }
}

/// What a backend's settings make: the model's, for its `load`, and the kind's own.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Taken {
    model: BTreeMap<String, Scalar>,
    call_limit: Duration,
    lifetime_s: u32,
}

impl Taken {
    fn of<'a>(
        settings: impl Iterator<Item = (&'a str, &'a Setting)>,
    ) -> Result<Taken, KindRefused> {
        let refused = |why: &'static str| KindRefused::Table(why);
        let mut taken = Taken {
            model: BTreeMap::new(),
            call_limit: CALL_LIMIT,
            lifetime_s: LIFETIME_DEFAULT_S,
        };
        for (key, setting) in settings {
            match (key, setting) {
                ("max_lifetime_s", Setting::Integer(seconds)) => {
                    taken.lifetime_s = u32::try_from(*seconds)
                        .ok()
                        .filter(|seconds| (1..=LIFETIME_MOST_S).contains(seconds))
                        .ok_or(refused(BAD_LIFETIME))?;
                }
                ("max_lifetime_s", _) => return Err(refused(BAD_LIFETIME)),
                ("call_limit_s", Setting::Integer(seconds)) => {
                    let seconds = u64::try_from(*seconds)
                        .ok()
                        .filter(|seconds| (1..=CALL_LIMIT.as_secs()).contains(seconds))
                        .ok_or(refused(BAD_CALL_LIMIT))?;
                    taken.call_limit = Duration::from_secs(seconds);
                }
                ("call_limit_s", _) => return Err(refused(BAD_CALL_LIMIT)),
                ("threshold" | "pad" | "min_cover", Setting::Text(text)) => {
                    model_text(key, text)?;
                    if !fraction(text) {
                        return Err(refused(BAD_FRACTION));
                    }
                    taken
                        .model
                        .insert(key.to_owned(), Scalar::Text(text.clone()));
                }
                ("threshold" | "pad" | "min_cover", _) => return Err(refused(BAD_FRACTION)),
                ("outline", Setting::Text(text)) => {
                    model_text(key, text)?;
                    taken
                        .model
                        .insert(key.to_owned(), Scalar::Text(text.clone()));
                }
                ("outline", _) => return Err(refused(BAD_OUTLINE)),
                _ => return Err(refused(UNKNOWN_SETTING)),
            }
        }
        Ok(taken)
    }
}

/// A model's setting whose text a `load` can carry (ADR 0020), or the kind's refusal.
fn model_text(key: &str, text: &str) -> Result<(), KindRefused> {
    proto::check_setting(key, &Scalar::Text(text.to_owned()))
        .map_err(|_| KindRefused::Table(BAD_TEXT))
}

/// Whether a text is a number from 0 to 1 written as digits with at most one point:
/// `0.2`, `1`, `.5`; not `1e-1`, `nan` or `-0`.
fn fraction(text: &str) -> bool {
    let digits = text.bytes().filter(u8::is_ascii_digit).count();
    let points = text.bytes().filter(|byte| *byte == b'.').count();
    digits >= 1
        && points <= 1
        && digits + points == text.len()
        && text
            .parse::<f64>()
            .is_ok_and(|number| (0.0..=1.0).contains(&number))
}

/// Whether a backend's table names a way off the machine: a url, a provider or a
/// credential. The kind refuses such a table ([`NOT_LOCAL`]).
fn names_a_way_out(backend: &Backend) -> bool {
    backend.address().is_some()
        || backend.provider().is_some()
        || backend.credentials().next().is_some()
}

/// Whether a backend's table, under this capability, is a local backend's: it names
/// no url, no provider and no credential, and, of the kind `pack`, it is under
/// `segment`, the one capability the kind answers. The one rule the kind and the
/// wiring share: a config opens a models directory only when one of its tables is
/// such a one (`wiring::Over::installed`), and every table of the kind `pack` that is
/// not such a one the kind refuses ([`NOT_LOCAL`], [`NOT_SEGMENT`]) before it would
/// ask anything of a model. A table of another kind (a test's own kind of a local
/// backend) is held to the first half alone.
pub fn is_local(capability: Capability, backend: &Backend) -> bool {
    let answered = backend.kind() != KIND || capability == Capability::Segment;
    answered && !names_a_way_out(backend)
}

/// The kind `pack`, over the one worker of a process.
pub struct PackKind {
    worker: Arc<OneWorker>,
}

impl PackKind {
    pub fn over(worker: Arc<OneWorker>) -> PackKind {
        PackKind { worker }
    }
}

impl Kind for PackKind {
    fn name(&self) -> &str {
        KIND
    }

    fn make(&self, table: &Table<'_>) -> Result<Made, KindRefused> {
        if table.capability() != Capability::Segment {
            return Err(KindRefused::Table(NOT_SEGMENT));
        }
        let backend = table.backend();
        if names_a_way_out(backend) {
            return Err(KindRefused::Table(NOT_LOCAL));
        }
        let model = backend.model().ok_or(KindRefused::Table(NO_MODEL))?;
        let model =
            ModelName::new(model, table.secrets()).map_err(|_| KindRefused::Table(BAD_MODEL))?;
        let taken = Taken::of(backend.settings())?;
        let features = self
            .worker
            .features(model.as_str())
            .ok_or(KindRefused::Table(NOT_IN_CATALOGUE))?;
        let features: Vec<&str> = features.iter().map(String::as_str).collect();
        let identity = ModelIdentity::new(KIND, model.as_str(), VERSION);
        let declaration =
            Declaration::new(identity, &features).map_err(|_| KindRefused::Table(NOT_DECLARED))?;
        Ok(Made::Segment(Arc::new(Local {
            declaration,
            model,
            taken,
            secrets: table.secrets().clone(),
            worker: self.worker.clone(),
        })))
    }
}

/// A backend of the kind: one model, its settings, and the one worker.
struct Local {
    declaration: Declaration,
    model: ModelName,
    taken: Taken,
    /// The executor's secrets, as the kind was handed them: the supervisor's error
    /// output is redacted with them, and the models directory is read with them.
    secrets: Secrets,
    worker: Arc<OneWorker>,
}

impl Segmenter for Local {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    /// One request, answered from the installation the run was handed its backend
    /// under, which its lease holds, and queued with the count of removals that
    /// lease's hold read (task P6b): never the installation the model's pointer names
    /// now, and never a count read later. Its files are verified before anything
    /// loads it.
    fn segment(&self, request: &SegmentRequest, bounds: &Bounds) -> Result<SegmentResponse, Fault> {
        let not_leased = || Fault::new(FaultKind::Rejected, NOT_LEASED).not_sent();
        let (Some(handed), Some(removals)) = (bounds.installation(), bounds.removals()) else {
            return Err(not_leased());
        };
        let installation = InstallationId::from_text(&handed.to_string(), &self.secrets)
            .map_err(|_| not_leased())?;
        self.worker
            .verified(&self.model, installation, &self.secrets, bounds)?;
        let supervisor = self.worker.supervisor(&self.secrets)?;
        let ask = Ask::segment(request, self.model.clone(), installation);
        let queued = Request {
            model: self.model.clone(),
            installation,
            settings: self.taken.model.clone(),
            ask,
            removals,
        };
        let limits = Limits {
            cancellation: bounds.cancellation().clone(),
            deadline: bounds.deadline(),
            call_limit: Some(self.taken.call_limit),
            lifetime_s: Some(self.taken.lifetime_s),
        };
        match supervisor.call(queued, &limits) {
            Ok(called) => answered(request, called),
            Err(failed) => Err(failed.fault),
        }
    }
}

/// The core's response to a request, from what the worker answered: each instance
/// under the query it answers, in the order the wire held them, answered on the
/// device the worker's result names, and never from a cache: the kind has none, and
/// says so (plan 1C, task P7a; ADR 0025). An answer the core cannot take is
/// malformed, as an answer.
fn answered(request: &SegmentRequest, called: Called) -> Result<SegmentResponse, Fault> {
    let malformed = || {
        let words = "the pack's worker answered with an instance the request cannot hold";
        Fault::new(FaultKind::Malformed, words).answered(None)
    };
    let Answer::Segment(found) = called.answer else {
        return Err(malformed());
    };
    let mut answers: Vec<Vec<Instance>> = vec![Vec::new(); request.queries().len()];
    for one in found {
        let place = usize::try_from(one.query).map_err(|_| malformed())?;
        let plane = one.plane;
        let instance = Instance::new(
            plane.binding,
            plane.width,
            plane.height,
            plane.samples,
            one.bounds,
            one.score,
        )
        .map_err(|_| malformed())?;
        answers.get_mut(place).ok_or_else(malformed)?.push(instance);
    }
    // The codec held the result's device to the protocol's rule, which is a device's:
    // one that is not is refused here all the same, as an answer the core cannot take.
    let device = Device::named(&called.device).map_err(|_| malformed())?;
    let served = Served::unknown().on(device).cached(false);
    Ok(SegmentResponse::new(answers).served_as(served))
}

#[cfg(test)]
mod tests {
    use super::{BAD_CALL_LIMIT, BAD_LIFETIME, LIFETIME_DEFAULT_S, Taken, fraction};
    use whizbang_core::config::{KindRefused, Setting};

    #[test]
    fn a_fraction_is_digits_and_one_point_from_0_to_1() {
        for taken in ["0", "1", "0.2", ".5", "1.0", "0.000001"] {
            assert!(fraction(taken), "{taken}");
        }
        for refused in [
            "", ".", "1.5", "-0", "+0.2", "1e-1", "nan", "inf", "0.2.1", " 0.2",
        ] {
            assert!(!fraction(refused), "{refused}");
        }
    }

    #[test]
    fn with_no_setting_the_lifetime_is_a_day_and_the_call_limit_the_supervisors() {
        let taken = Taken::of(std::iter::empty()).unwrap();
        assert_eq!(taken.lifetime_s, LIFETIME_DEFAULT_S);
        assert_eq!(taken.call_limit.as_secs(), 120);
        assert!(taken.model.is_empty());
        let one = |key: &'static str, value: i64| {
            let setting = Setting::Integer(value);
            Taken::of(std::iter::once((key, &setting)))
                .map(|taken| (taken.lifetime_s, taken.call_limit.as_secs()))
        };
        assert_eq!(one("max_lifetime_s", 1), Ok((1, 120)));
        assert_eq!(one("max_lifetime_s", 31_536_000), Ok((31_536_000, 120)));
        assert_eq!(
            one("max_lifetime_s", 31_536_001),
            Err(KindRefused::Table(BAD_LIFETIME))
        );
        assert_eq!(one("call_limit_s", 1), Ok((LIFETIME_DEFAULT_S, 1)));
        assert_eq!(one("call_limit_s", 120), Ok((LIFETIME_DEFAULT_S, 120)));
        assert_eq!(
            one("call_limit_s", 121),
            Err(KindRefused::Table(BAD_CALL_LIMIT))
        );
    }
}
