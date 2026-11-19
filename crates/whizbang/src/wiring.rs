//! Wiring: the registry and the executor of this build, each made in one place.
//!
//! The binary calls [`registry`] and describes what it holds. A test calls the same
//! function and then registers its fakes, so what a test runs against is the shipping
//! registry and nothing the binary would not also have, plus what the test added. Fakes
//! live in `whizbang-testkit`, which this crate depends on for its tests only.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use whizbang_backends::models::{
    Catalogue, Entry, Installations, Lease, ModelName, ModelsDir, OpenRefused,
    Platform as ModelPlatform, Revision, is_users, registered, users_catalogue,
};
use whizbang_core::artifact::Contents;
use whizbang_core::capability::{Inventory, NoInventory};
use whizbang_core::config::{Config, Credentials, Files, Kinds, Loaded, Offers};
use whizbang_core::contract::{
    Action, Capability, ConfigSource, Engine, Interrupted, NoBackends, Opening, Place, Recovery,
    Registry, Secrets, Snapshot, SystemClock,
};
use whizbang_core::description::Description;
use whizbang_core::egress::{DispatchPolicy, Ways};
use whizbang_core::spend::{CallCap, Meter, PreparationAge, StoreLedger};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{self, Store};

use crate::pack::{OneWorker, PackKind};
use crate::tool_file::Files as ToolFiles;
use whizbang_backends::service::ServiceKind;

/// Every action this build ships, registered in the order a door lists them: the
/// recipes first, `check` and then those of the recipe files (`whizbang_rascal::recipes`),
/// then the primitives. Tools add theirs here as they are ported, one line each. What
/// comes back otherwise is why an action could not be registered, in a sentence.
///
/// `check`, `align`, `change_map`, `edge_similarity`, `resize` and `view` are squint's
/// (ADR 0009); `whizbang squint check` runs `check`. `check` needs a judge, which a config gives as a backend of the kind
/// `service` ([`kinds`]); the five tools need none, and are what `check` calls.
pub fn registry() -> Result<Registry, String> {
    registry_with(false)
}

/// The same actions, with the decision cache of `check` on when `cached` says
/// (`[cache]`, [`Over::decisions_cached`]; plan 2C, task M1). `whizbang squint check` and
/// `whizbang serve` build theirs so; evaluation builds [`registry`], which never reads
/// the cache.
pub fn registry_with(cached: bool) -> Result<Registry, String> {
    registry_of(cached, None, &[])
}

/// The same, with the tool files the config lists, when it lists any (`[tools] paths`;
/// plan 2C, task X1; ADR 0032): each an action of its own, and each on offer to `check`,
/// run only when a run names it. Each file is read, and its declaration held to its
/// rules, here: a file that breaks one is refused, and nothing is built. And with the
/// recipe files it lists (`[recipes] paths`; plan m3, task R1, item 6; ADR 0033): each
/// read and checked as a shipped recipe is, and refused, with nothing built, when it is
/// not one this build runs.
pub fn registry_over(over: &Over, cached: bool) -> Result<Registry, String> {
    registry_of(cached, over.tool_files()?, &over.recipe_files()?)
}

fn registry_of(
    cached: bool,
    files: Option<Arc<ToolFiles>>,
    recipes: &[String],
) -> Result<Registry, String> {
    let said = |error: &dyn std::fmt::Display| error.to_string();
    // Everything but the recipes, in the order a door lists them: what a recipe is
    // checked against.
    let mut primitives = Registry::new();
    let toolbox = match &files {
        Some(files) => whizbang_squint::check::Toolbox::with_files(files.clone()),
        None => whizbang_squint::check::Toolbox::standard(),
    };
    let check = whizbang_squint::check::Check::new(toolbox).map_err(|e| said(&e))?;
    let check = if cached { check.caching() } else { check };
    let view = whizbang_squint::view::View::new().map_err(|e| said(&e))?;
    let align = whizbang_squint::align::Align::new().map_err(|e| said(&e))?;
    let resize = whizbang_squint::resize::Resize::new().map_err(|e| said(&e))?;
    let change_map = whizbang_squint::compare::Compare::change_map().map_err(|e| said(&e))?;
    let edges = whizbang_squint::compare::Compare::edge_similarity().map_err(|e| said(&e))?;
    let check: Arc<dyn Action> = Arc::new(check);
    // What each use case's recipe measures first (plan m3, task Q1).
    let details = whizbang_squint::measured::DetailsStep::new().map_err(|e| said(&e))?;
    let squint: [Arc<dyn Action>; 4] = [
        Arc::new(align),
        Arc::new(change_map),
        Arc::new(edges),
        Arc::new(details),
    ];
    for action in squint {
        primitives.register(action).map_err(|e| said(&e))?;
    }
    whizbang_rascal::close::register(&mut primitives).map_err(|e| said(&e))?;
    whizbang_rascal::feather::register(&mut primitives).map_err(|e| said(&e))?;
    whizbang_rascal::grow::register(&mut primitives).map_err(|e| said(&e))?;
    whizbang_rascal::plug::register(&mut primitives).map_err(|e| said(&e))?;
    whizbang_rascal::render::register(&mut primitives).map_err(|e| said(&e))?;
    primitives
        .register(Arc::new(resize))
        .map_err(|e| said(&e))?;
    whizbang_rascal::segment::register(&mut primitives).map_err(|e| said(&e))?;
    whizbang_rascal::select::register(&mut primitives).map_err(|e| said(&e))?;
    whizbang_rascal::subtract::register(&mut primitives).map_err(|e| said(&e))?;
    primitives.register(Arc::new(view)).map_err(|e| said(&e))?;
    // Each registered by its own module (plan m3, "Shared files").
    whizbang_rascal::paste::register(&mut primitives).map_err(|e| said(&e))?;
    whizbang_rascal::fill::register(&mut primitives).map_err(|e| said(&e))?;
    whizbang_rascal::cut::register(&mut primitives).map_err(|e| said(&e))?;
    whizbang_rascal::put::register(&mut primitives).map_err(|e| said(&e))?;
    whizbang_rascal::window::register(&mut primitives).map_err(|e| said(&e))?;
    whizbang_rascal::unwindow::register(&mut primitives).map_err(|e| said(&e))?;
    whizbang_rascal::apply::register(&mut primitives).map_err(|e| said(&e))?;
    if let Some(files) = &files {
        for action in files.actions()? {
            primitives.register(action).map_err(|e| said(&e))?;
        }
    }
    // A recipe is checked against the build's other actions, `check` among them.
    let mut against = Registry::new();
    against.register(check.clone()).map_err(|e| said(&e))?;
    for descriptor in primitives.descriptors() {
        if let Some(action) = primitives.get(descriptor.name()) {
            against.register(action.clone()).map_err(|e| said(&e))?;
        }
    }
    let shipped = whizbang_rascal::recipes::SHIPPED
        .iter()
        .map(|text| (*text, false))
        // Squint's use cases as recipes (plan m3, task Q1).
        .chain(
            whizbang_squint::recipes::SHIPPED
                .iter()
                .map(|(_, text)| (*text, false)),
        );
    let users = recipes.iter().map(|text| (text.as_str(), true));
    let mut registry = Registry::new();
    registry.register(check).map_err(|e| said(&e))?;
    for (text, users) in shipped.chain(users) {
        let action = whizbang_rascal::recipes::action_of(text, &against).map_err(|e| {
            if users {
                format!("the config cannot be used: a recipe file is not one this build runs: {e}")
            } else {
                said(&e)
            }
        })?;
        registry.register(Arc::new(action)).map_err(|e| said(&e))?;
    }
    for descriptor in primitives.descriptors() {
        if let Some(action) = primitives.get(descriptor.name()) {
            registry.register(action.clone()).map_err(|e| said(&e))?;
        }
    }
    Ok(registry)
}

/// The kinds of backend this build has (ADR 0011, "Kinds"): `pack`, a local segmenter
/// over the one worker of the process (plan 1C, task P6a; [`crate::pack`]), which is
/// this; and `service`, a judge whose provider is `anthropic` (plan 2A, task S1),
/// `google` or `openai` (plan m2c, task P1; `whizbang_backends::service`). Whoever builds the executor makes the one worker
/// once, and gives the same to the wiring ([`Over::pack`]), which tells it the models
/// directory it opened. A backend of any other kind is of a kind this build does not
/// have, and is not ready. A test registers a fake beside them.
pub fn kinds(worker: &Arc<OneWorker>) -> Kinds {
    let mut kinds = Kinds::new();
    // Two kinds, under names of the rule, in kinds that hold none: each is taken.
    let _ = kinds.register(Arc::new(PackKind::over(worker.clone())));
    let _ = kinds.register(Arc::new(ServiceKind::new()));
    kinds
}

/// The catalogue this build ships (ADR 0017, "The catalogue", "The shipped catalogue";
/// plan 1C, task N5): the models it can install, read from `catalogue.json` beside
/// this file, which is compiled into the binary. A catalogue is strict JSON, so the
/// entry's comment is here.
///
/// One model, `sam3`: the files of `facebook/sam3` on the Hub at the commit
/// `3c879f39826c281e95690f02c7821c4de09afae7`, `model.safetensors` and the small text
/// files beside it, never `sam3.pt`, which is a pickle. **How the sizes and hashes
/// were fixed:** each size is the one the Hub's public listing of that commit states
/// (`https://huggingface.co/api/models/facebook/sam3/tree/<commit>`, read once with no
/// token); each SHA-256 is what `whizbang models pin` worked out from the bytes it
/// fetched once through the checked downloader, with the token. That is trust on
/// first use: what the Hub served that once. The listing states a large file's
/// SHA-256 too, but masks it for this gated repository to a reader without a token.
/// The origins are the Hub and `us.aws.cdn.hf.co`, where the Hub moves the large
/// file's download, as `pin` found and the owner approved.
///
/// A second model, `tiny` (plan 1D, task T1): OWLv2 finds boxes for a phrase and SAM 2.1
/// Hiera-Tiny draws each mask, run by the adapter `owlv2-sam2`. Its files are those of
/// two public repositories of the Hub, each at a commit: `google/owlv2-base-patch16` at
/// `2a1560802f8cf3c408fec9b809d705f56a2f7146` and `facebook/sam2.1-hiera-tiny` at
/// `de431c4043854a71d8101e17995dfe596bf101a5`; each `model.safetensors` and the small
/// files its runtime reads, never a `.bin` or a `.pt`, which are pickles. Both
/// repositories have a `config.json` and a `model.safetensors`, and a catalogue's file
/// names are unique and hold no folder, so each file is installed under its model's
/// prefix (`owlv2.config.json`, `sam2.model.safetensors`), its `from` still the
/// repository's own path. Neither repository holds a licence file: each model card
/// (`README.md`), installed as `owlv2.README.md` and `sam2.README.md`, states the
/// licence, Apache-2.0. Both are public, so no download needs a token. **How the sizes
/// and hashes were fixed** (2026-11-18): each size is the one the Hub's public listing
/// of the commit states, read once with no token; each small file's SHA-256 is what
/// `whizbang models pin` worked out from the bytes it fetched once; each
/// `model.safetensors`'s is the one the listing states for it (not masked, the
/// repositories being public), which an install checks against the bytes it
/// downloads. The large files' downloads are moved to `us.aws.cdn.hf.co`, the host the
/// owner approved for the Hub's storage.
pub fn catalogue() -> Result<Catalogue, String> {
    Catalogue::parse(SHIPPED_CATALOGUE.as_bytes(), &Secrets::none())
        .map_err(|refused| format!("the catalogue this build ships cannot be read: {refused}"))
}

/// The text of the shipped catalogue ([`catalogue`]).
pub const SHIPPED_CATALOGUE: &str = include_str!("catalogue.json");

/// One adapter of this build's table: its name, the revision it states of itself,
/// which is part of an installation's identity (ADR 0017), and whether this build's
/// pack ships it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adapter {
    pub name: ModelName,
    pub revision: Revision,
    /// Whether the pack of this build has the adapter. One that is not shipped yet
    /// has its revision fixed all the same, so that `get` installs its model under
    /// the identity it will be loaded under, and nothing is downloaded again when
    /// the adapter ships; until then the inventory calls the model not ready, for
    /// want of an adapter.
    pub shipped: bool,
}

/// The table of adapters of this build (plan 1C, tasks N5 and R2a; plan 1D, task Q1):
/// `sam3` at revision `2`, shipped. N5 fixed revision `1` before the pack had the
/// adapter, so that `get` installed the model under the identity it is loaded under;
/// R2a shipped the adapter (`pack/src/whizbang_pack/sam3.py`) under that revision. Q1
/// moved it to `2`, the adapter that takes points and boxes: a new installation of the
/// same files, which `get` links from the one of revision `1` and does not download.
/// A model whose adapter is not shipped is not ready, and says so (ADR 0017, "The
/// inventory").
///
/// `owlv2-sam2` at revision `1`, shipped (plan 1D, task T1): the adapter of `tiny`
/// (`pack/src/whizbang_pack/owlv2_sam2.py`, its module named with `_` where the
/// adapter's name has `-`).
pub fn adapters() -> Vec<Adapter> {
    let none = Secrets::none();
    let named = |name: &str, revision: &str, shipped: bool| {
        Some(Adapter {
            name: ModelName::new(name, &none).ok()?,
            revision: Revision::new(revision, &none).ok()?,
            shipped,
        })
    };
    [named("sam3", "2", true), named("owlv2-sam2", "1", true)]
        .into_iter()
        .flatten()
        .collect()
}

/// The models directory in the platform's data folder for whizbang (ADR 0017, "Where
/// the directory is"), from what the environment says, which whoever calls this
/// read: on macOS `Library/Application Support/whizbang/models` under the home
/// folder; elsewhere `whizbang/models` under the data folder `XDG_DATA_HOME` names,
/// or under `.local/share` of the home folder when it names none. A folder the
/// environment names is taken only when it is an absolute path; with none there is
/// no such directory, and nothing is guessed.
///
/// It reads nothing itself: not the environment, and not the disk. The binary's
/// `main` gives it this process's environment, and nothing else does
/// (`wired::only_main_reads_where_the_platforms_data_folder_is`), so no test is
/// pointed at the data folder of whoever runs it.
pub fn platform_models_dir(
    macos: bool,
    home: Option<&std::ffi::OsStr>,
    xdg_data_home: Option<&std::ffi::OsStr>,
) -> Option<PathBuf> {
    fn absolute(folder: Option<&std::ffi::OsStr>) -> Option<&Path> {
        folder.map(Path::new).filter(|folder| folder.is_absolute())
    }
    let data = if macos {
        absolute(home)?.join("Library").join("Application Support")
    } else {
        match absolute(xdg_data_home) {
            Some(data) => data.to_owned(),
            None => absolute(home)?.join(".local").join("share"),
        }
    };
    Some(data.join("whizbang").join("models"))
}

/// Which of the three said where the models directory is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Said {
    /// The command's flag, `--models-dir`.
    Flag,
    /// The config's `[models] dir`.
    Config,
    /// Neither: the platform's data folder.
    Platform,
}

/// Where the models directory is (ADR 0017, "Where the directory is"): what the
/// command's flag says, then what the config says, then the platform's data
/// folder; and nothing when none of the three says.
pub fn models_dir<'a>(
    flag: Option<&'a Path>,
    config: Option<&'a Path>,
    platform: Option<&'a Path>,
) -> Option<(Said, &'a Path)> {
    let flag = flag.map(|dir| (Said::Flag, dir));
    let config = config.map(|dir| (Said::Config, dir));
    flag.or(config)
        .or(platform.map(|dir| (Said::Platform, dir)))
}

/// What says where the platform's models directory is, when it is asked: asked at
/// most once, and only when neither a flag nor a config named a directory that is
/// about to be opened.
pub type Platform = Arc<dyn Fn() -> Option<PathBuf> + Send + Sync>;

/// What says where the Hub's command-line sign-in keeps its token
/// (`sign_in::token_file`), when it is asked: asked at most once, and only when a
/// download from the sign-in's origin is about to be asked and the config names the
/// sign-in for it (plan 1C, D12). The binary's `main` gives the one function that
/// names the real place; a test gives a temporary folder.
pub type SignIn = Arc<dyn Fn() -> Option<PathBuf> + Send + Sync>;

/// What a command says of a relative `--models-dir`, before anything is read or made:
/// a relative path is another folder from each place a command is run in, as the
/// config's `[models] dir` is refused for (ADR 0011, "What the file holds").
pub const RELATIVE_MODELS_DIR: &str = "--models-dir must be an absolute path: a relative one would be another folder from each place a command is run in; nothing was read or made";

/// The models a command's executor is built over: a catalogue, the adapters of the
/// build, the platform its models are to run on, and what the command line and the
/// machine say of where the models directory is. What `{:?}` prints is counts and
/// whether each place is named: no path and no name.
#[derive(Clone)]
pub struct Models {
    pub(crate) catalogue: Catalogue,
    /// The adapters this build's pack ships: what the inventory is told of.
    pub(crate) adapters: Vec<(ModelName, Revision)>,
    /// The adapters whose revision is fixed and which the pack does not ship yet:
    /// `get` installs under them, and the inventory is not told of them.
    pub(crate) not_yet_shipped: Vec<(ModelName, Revision)>,
    pub(crate) runs_on: Option<ModelPlatform>,
    pub(crate) flag: Option<PathBuf>,
    platform: Option<Platform>,
    /// The models of the user's catalogue in the models directory, once it was read
    /// ([`Models::with_users`]); none before.
    users: Vec<ModelName>,
}

impl Models {
    /// The models of this build: the catalogue it ships ([`catalogue`]) under the
    /// table of adapters ([`adapters`]), those it ships and those not yet. What the
    /// binary gives.
    pub fn shipped() -> Result<Models, String> {
        let (shipped, not_yet): (Vec<Adapter>, Vec<Adapter>) =
            adapters().into_iter().partition(|adapter| adapter.shipped);
        let pairs = |table: Vec<Adapter>| -> Vec<(ModelName, Revision)> {
            table
                .into_iter()
                .map(|adapter| (adapter.name, adapter.revision))
                .collect()
        };
        Ok(Models::of(catalogue()?, pairs(shipped)).not_yet_shipped(pairs(not_yet)))
    }

    /// The models of this catalogue under these adapters, each shipped, to run on
    /// the platform this build is for (`Platform::this` of the models directory's
    /// types): what a test gives.
    pub fn of(catalogue: Catalogue, adapters: Vec<(ModelName, Revision)>) -> Models {
        Models {
            catalogue,
            adapters,
            not_yet_shipped: Vec::new(),
            runs_on: ModelPlatform::this(),
            flag: None,
            platform: None,
            users: Vec::new(),
        }
    }

    /// The same, with these adapters too, whose revisions are fixed and which the
    /// pack does not ship yet: `get` installs a model of one under its revision, and
    /// the inventory calls the model not ready, for want of an adapter.
    pub fn not_yet_shipped(mut self, adapters: Vec<(ModelName, Revision)>) -> Models {
        self.not_yet_shipped = adapters;
        self
    }

    /// The same, for models that are to run on this platform, or on none: what a
    /// test gives to show a model its platform cannot run.
    pub fn runs_on(mut self, platform: Option<ModelPlatform>) -> Models {
        self.runs_on = platform;
        self
    }

    /// The same, with the directory the command's flag names (`--models-dir`), or
    /// none.
    pub fn flag(mut self, dir: Option<&Path>) -> Models {
        self.flag = dir.map(Path::to_owned);
        self
    }

    /// The same, with what says where the models directory of the platform's data
    /// folder is ([`platform_models_dir`]). **It is not asked here, and not unless
    /// it is needed:** only when a directory is about to be opened and neither the
    /// flag nor the config named one ([`Over::installed`]). A command that opens no
    /// models directory never learns where the platform's is.
    pub fn platform(mut self, says: Platform) -> Models {
        self.platform = Some(says);
        self
    }
}

impl Models {
    /// The same, with what the user added to this models directory (plan 1D, task U1;
    /// ADR 0017, "A user's weights and a user's adapter"): the adapters of its registry,
    /// each at the hash of its copy, beside this build's; and the models of its
    /// catalogue (`catalogue.json`), read by the reader of the shipped one, beside this
    /// build's. **A model of this build's catalogue is never taken the place of:** a
    /// user's catalogue that names one is refused whole. A user's adapter is named
    /// `user-` and a name, which no adapter of this build has; a table of this build's
    /// that held one would be refused too. What cannot be read is refused, in the
    /// readers' own words, which hold no path.
    pub(crate) fn with_users(
        mut self,
        dir: &ModelsDir,
        secrets: &Secrets,
    ) -> Result<Models, String> {
        if self
            .adapters
            .iter()
            .chain(&self.not_yet_shipped)
            .any(|(name, _)| is_users(name))
        {
            return Err("this build's table of adapters names an adapter as a user's".to_owned());
        }
        let added = registered(dir, secrets).map_err(|refused| {
            format!("the users' adapters of the models directory cannot be used: {refused}")
        })?;
        let users = users_catalogue(dir, secrets).map_err(|refused| {
            format!("the models directory's catalogue (catalogue.json) cannot be used: {refused}")
        })?;
        self.adapters.extend(added);
        let Some(users) = users else {
            return Ok(self);
        };
        let mut entries = self.catalogue.entries().to_vec();
        for entry in users.entries() {
            if self.catalogue.entry(entry.name().as_str()).is_some() {
                return Err(
                    "the models directory's catalogue (catalogue.json) names a model of the \
                     catalogue this build ships, whose place no user's model takes; give the \
                     user's model another name"
                        .to_owned(),
                );
            }
            self.users.push(entry.name().clone());
            entries.push(entry.clone());
        }
        self.catalogue = Catalogue::new(entries, secrets).map_err(|refused| {
            format!(
                "the models directory's catalogue (catalogue.json) cannot be used beside this \
                 build's: {refused}"
            )
        })?;
        Ok(self)
    }

    /// Whether this entry is of the user's catalogue in the models directory.
    pub(crate) fn is_users(&self, entry: &Entry) -> bool {
        self.users.contains(entry.name())
    }
}

impl std::fmt::Debug for Models {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Models")
            .field("models", &self.catalogue.entries().len())
            .field("adapters", &self.adapters.len())
            .field("not_yet_shipped", &self.not_yet_shipped.len())
            .field("users", &self.users.len())
            .field("runs_on", &self.runs_on.map(ModelPlatform::name))
            .field("flag", &self.flag.is_some())
            .field("platform", &self.platform.is_some())
            .finish()
    }
}

impl Models {
    /// A relative `--models-dir` is refused, whatever else is asked, before anything
    /// is read or made ([`RELATIVE_MODELS_DIR`]).
    pub(crate) fn flag_is_absolute(&self) -> Result<(), String> {
        match &self.flag {
            Some(dir) if !dir.is_absolute() => Err(RELATIVE_MODELS_DIR.to_owned()),
            _ => Ok(()),
        }
    }

    /// Where the models directory is, and which of the three said so: the flag,
    /// then `config` (the config's `[models] dir`), then the platform's, which is
    /// asked for only when the other two are silent. With none of the three, why
    /// there is none, and how to name one.
    pub(crate) fn chosen(&self, config: Option<&Path>) -> Result<(Said, PathBuf), String> {
        let named = models_dir(self.flag.as_deref(), config, None);
        let platform = match (named, &self.platform) {
            (None, Some(says)) => says(),
            _ => None,
        };
        match named.or(models_dir(None, None, platform.as_deref())) {
            Some((said, root)) => Ok((said, root.to_owned())),
            None => Err(
                "no models directory is named, and this machine's data folder is not known: \
                 name one with --models-dir, or as `dir` in the config's table `models`"
                    .to_owned(),
            ),
        }
    }
}

/// Opens the models directory a command chose ([`Models::chosen`]), by the opening
/// that asks whose it is (`ModelsDir::open`) and by no other way, with this clock for
/// its lock. That opening makes the directory's own folder where nothing has the
/// name; the folders above the platform's are made here first, each for its owner
/// only, since nobody made a data folder for whizbang before its first use. The
/// folders above one a person named are not made: a path written wrong is said, not
/// made. What comes back otherwise is why the directory is not used, in fixed words
/// that say how to mend it and hold no path.
pub(crate) fn open_models_dir(
    said: Said,
    root: &Path,
    clock: Arc<dyn whizbang_core::contract::Clock>,
) -> Result<ModelsDir, String> {
    if said == Said::Platform
        && let Some(above) = root.parent()
    {
        made_for_its_owner(above).map_err(|_| {
            "the data folder that is to hold the models directory could not be made; \
             name a models directory with --models-dir, or as `dir` in the config's \
             table `models`"
                .to_owned()
        })?;
    }
    ModelsDir::open(Arc::new(RealFs), root, clock).map_err(|refused| not_used(refused, said))
}

/// A models directory that was opened, at its folder, and the inventory over it.
#[derive(Clone)]
struct Opened {
    root: PathBuf,
    dir: ModelsDir,
    catalogue: Catalogue,
    adapters: Vec<(ModelName, Revision)>,
    runs_on: Option<ModelPlatform>,
    installations: Arc<Installations>,
}

impl Opened {
    /// The inventory of this directory, told these secrets.
    fn knowing(
        root: PathBuf,
        dir: ModelsDir,
        catalogue: Catalogue,
        adapters: Vec<(ModelName, Revision)>,
        runs_on: Option<ModelPlatform>,
        secrets: &Secrets,
    ) -> Opened {
        let installations = inventory_of(&dir, &catalogue, &adapters, runs_on, secrets);
        Opened {
            root,
            dir,
            catalogue,
            adapters,
            runs_on,
            installations: Arc::new(installations),
        }
    }

    /// Tells the one worker of the process where it is: this directory, and the
    /// catalogue the inventory is over.
    fn placing(&self, worker: &OneWorker) -> Result<(), String> {
        worker.place(&self.root, self.dir.clone(), self.catalogue.clone())
    }
}

/// Whether a config has a local backend, by the kind `pack`'s own rule
/// ([`crate::pack::is_local`]: no url, no provider and no credential, and for the
/// kind `pack`, under `segment`), so that the wiring and the kind never disagree. It
/// is read from the tables alone, whichever of the kinds and the directory was given
/// first. A table the kind would refuse (a provider and no url, or a `pack` table
/// under `fill`) opens no directory, as it would ask nothing of a model. (Until the
/// review of task P6a any table with no url counted.)
fn has_local_backend(config: &Config) -> bool {
    [Capability::Segment, Capability::Fill, Capability::Judge]
        .into_iter()
        .any(|capability| {
            config
                .backends(capability)
                .any(|(_, backend)| crate::pack::is_local(capability, backend))
        })
}

/// The inventory of this directory, for the models of this catalogue under these
/// adapters, to run on this platform, told these secrets: the one way this crate
/// makes one.
pub(crate) fn inventory_of(
    dir: &ModelsDir,
    catalogue: &Catalogue,
    adapters: &[(ModelName, Revision)],
    runs_on: Option<ModelPlatform>,
    secrets: &Secrets,
) -> Installations {
    Installations::of(
        dir.clone(),
        catalogue.clone(),
        adapters.to_vec(),
        secrets.clone(),
    )
    .on(runs_on)
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
    installed: Option<Opened>,
    /// The one worker of the process, which the kind `pack` of [`Over::kinds`] is
    /// over, when one was given ([`Over::pack`]).
    pack: Option<Arc<OneWorker>>,
    /// Why that worker could not be told the models directory, when it was given
    /// after the directory was opened: the executor is not built.
    unplaced: Option<String>,
    /// The test's root the sender trusts in place of the compiled roots, when a test
    /// gave one ([`Over::trusting`]): compiled only under the feature `fakes`.
    #[cfg(feature = "fakes")]
    trusting: Option<Arc<whizbang_backends::net::testing::Pki>>,
    /// The text of a recipe file a command was given to run (`whizbang rascal run`),
    /// beside those the config lists.
    recipe: Option<String>,
}

impl Over {
    /// The same, with this recipe's text beside the recipe files the config lists: a
    /// recipe a command was given to run, checked and registered as theirs are.
    pub fn recipe(mut self, text: String) -> Over {
        self.recipe = Some(text);
        self
    }

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

    /// The same, whose runner's sender trusts the root of this test's `pki` and no
    /// other, in place of the roots compiled in: for a test whose server is on
    /// loopback. Compiled only under the feature `fakes`, which the binary that ships
    /// is built without.
    #[cfg(feature = "fakes")]
    pub fn trusting(mut self, pki: Arc<whizbang_backends::net::testing::Pki>) -> Over {
        self.trusting = Some(pki);
        self
    }

    /// The same, with these kinds of backend. The binary gives [`kinds`]; a test
    /// gives that with its fakes registered.
    pub fn kinds(mut self, kinds: Kinds) -> Over {
        self.kinds = kinds;
        self
    }

    /// The same, with the one worker of the process: the one the kinds given
    /// ([`Over::kinds`], [`kinds`]) are over. The wiring tells it the models directory
    /// it opens ([`Over::installed`]) and the catalogue it is over, whichever of the
    /// two is given first; where none is opened, no local backend is ready, and the
    /// worker is never asked. A worker whose supervisor runs over another models
    /// directory already is not told this one: the executor is then not built, and
    /// says so.
    pub fn pack(mut self, worker: Arc<OneWorker>) -> Over {
        if let Some(opened) = &self.installed
            && let Err(refused) = opened.placing(&worker)
        {
            self.unplaced = Some(refused);
        }
        self.pack = Some(worker);
        self
    }

    /// The same, told of these secrets from the start. **They are given once, here,
    /// and to both:** to the executor, which takes them out of everything it writes
    /// (`Engine::with_secrets`), and to the source of its config, which refuses a
    /// config that holds one (`Files::knowing`). Nothing else gives either its
    /// secrets, so the two never know two sets (ADR 0011, "The source over both
    /// files").
    ///
    /// **And to the inventory,** when there is one ([`Over::installed`]): it is made
    /// again here, over the same directory, told these. So the inventory and the
    /// executor are told one set whichever of the two is given first
    /// (`wired::a_secret_the_executor_is_told_of_is_refused_as_a_models_name_by_the_binarys_inventory`).
    pub fn knowing(mut self, secrets: Secrets) -> Over {
        self.secrets = secrets;
        if let Some(opened) = self.installed.take() {
            let Opened {
                root,
                dir,
                catalogue,
                adapters,
                runs_on,
                ..
            } = opened;
            self.installed = Some(Opened::knowing(
                root,
                dir,
                catalogue,
                adapters,
                runs_on,
                &self.secrets,
            ));
        }
        self
    }

    /// The same, with what is installed in the models directory of these models
    /// (ADR 0017, "The inventory", "Where the directory is"): the inventory a local
    /// backend's model is looked up in, and the holds a lease is taken from. It is
    /// given after the config ([`Over::config`]), which may say where the directory
    /// is.
    ///
    /// - **The directory is the flag's, then the config's `[models] dir`, then the
    ///   platform's** ([`models_dir`]). The config file is read here, once and by
    ///   itself, for that one key: a directory named in it afterwards is not seen
    ///   by an executor that is already built.
    /// - **It is opened by the opening that asks whose it is** (`ModelsDir::open`),
    ///   and by no other way. That opening makes the directory's own folder and no
    ///   folder above it; here, **the folders above the platform's are made**, for
    ///   their owner only, since nobody made a data folder for whizbang before its
    ///   first use. The folders above one a person named are not made: a path
    ///   written wrong is said, not made.
    /// - **The inventory is told the secrets this was told** ([`Over::knowing`]), and
    ///   no others: nobody else gives the inventory of an executor its secrets.
    /// - **The platform's is not even worked out unless it is the one:** what says
    ///   where it is ([`Models::platform`]) is asked only when a directory is about
    ///   to be opened and neither the flag nor the config named one.
    /// - **Where no model could be asked for, no directory is opened or made:** with
    ///   a catalogue that holds no model; and under a config that has no local
    ///   backend, by the kind `pack`'s own rule for one ([`crate::pack::is_local`];
    ///   plan 1C, task P6a, which gave the build its first kind, and the review of
    ///   it), or no config at all, whose executor has no
    ///   backend. Such an executor is over the inventory that knows no model. Which
    ///   backends are local is read from the config's tables, not from the kinds
    ///   given ([`Over::kinds`]), so the two may be given in either order.
    ///
    /// - **A relative `--models-dir` is refused first,** whatever else is asked and
    ///   before anything is read or made ([`RELATIVE_MODELS_DIR`]).
    ///
    /// - **What the user added to the directory is read with it** ([`Models::with_users`];
    ///   plan 1D, task U1): the models of its `catalogue.json` and the adapters of its
    ///   registry, beside this build's, which none of them takes the place of.
    ///
    /// - **The one worker of the process is told the directory** ([`Over::pack`]),
    ///   and the catalogue: the folder its worker is started over, and the
    ///   directory its supervisor reads the counts of removals in, are this one.
    ///
    /// What comes back otherwise is why the directory is not used, in fixed words
    /// that say how to mend it and hold no path.
    pub fn installed(mut self, models: Models) -> Result<Over, String> {
        models.flag_is_absolute()?;
        if models.catalogue.entries().is_empty() {
            return Ok(self);
        }
        // The config file alone, for its one key and for whether a backend of it is
        // local: whether its env file can be used is said where the two are read
        // together ([`loaded`]), and again at each ask of the source.
        let Some(path) = &self.config else {
            return Ok(self);
        };
        let config = Config::read(path, &self.secrets)
            .map_err(|refused| format!("the config cannot be used: {refused}"))?;
        if !has_local_backend(&config) {
            return Ok(self);
        }
        // The platform's is asked for last, and only when the other two are silent.
        let (said, root) = models.chosen(config.models_dir())?;
        let dir = open_models_dir(said, &root, Arc::new(SystemClock))?;
        // What the user added to the directory, beside this build's (plan 1D, task U1).
        let models = models.with_users(&dir, &self.secrets)?;
        let opened = Opened::knowing(
            root,
            dir,
            models.catalogue,
            models.adapters,
            models.runs_on,
            &self.secrets,
        );
        if let Some(worker) = &self.pack {
            opened.placing(worker)?;
        }
        self.installed = Some(opened);
        Ok(self)
    }

    /// The tool files the config lists, each read and its declaration held to its
    /// rules (plan 2C, task X1; ADR 0032), run by the tool worker of the one worker of
    /// the process; none without a config, or with one that lists none.
    pub fn tool_files(&self) -> Result<Option<Arc<ToolFiles>>, String> {
        let Some(path) = &self.config else {
            return Ok(None);
        };
        let config = Config::read(path, &self.secrets)
            .map_err(|refused| format!("the config cannot be used: {refused}"))?;
        if config.tool_paths().next().is_none() {
            return Ok(None);
        }
        let worker = self.pack.clone().unwrap_or_else(OneWorker::installed);
        let files = ToolFiles::read(config.tool_paths(), worker)
            .map_err(|refused| format!("the config cannot be used: {refused}"))?;
        Ok(Some(Arc::new(files)))
    }

    /// The text of each recipe file the config lists (`[recipes] paths`; ADR 0033), in
    /// its order; none without a config, or with one that lists none. Each is read once:
    /// a regular file under its own name, not a link, that its group and others cannot
    /// write, of at most [`whizbang_core::recipe::FILE_MAX_BYTES`] bytes of UTF-8. The
    /// sentence of a refusal names the file by its name alone.
    pub fn recipe_files(&self) -> Result<Vec<String>, String> {
        let mut texts = Vec::new();
        if let Some(path) = &self.config {
            let config = Config::read(path, &self.secrets)
                .map_err(|refused| format!("the config cannot be used: {refused}"))?;
            for path in config.recipe_paths() {
                let text =
                    recipe_file(path).map_err(|why| format!("the config cannot be used: {why}"))?;
                texts.push(text);
            }
        }
        texts.extend(self.recipe.clone());
        Ok(texts)
    }

    /// The one worker of the process, when one was given ([`Over::pack`]): what a
    /// door that outlives a call stops at its end.
    pub fn worker(&self) -> Option<&Arc<OneWorker>> {
        self.pack.as_ref()
    }

    /// Whether the config turns the decision cache on (`[cache]`): read here, once and
    /// by itself, as [`Over::installed`] reads it. No config is no cache.
    pub fn decisions_cached(&self) -> Result<bool, String> {
        let Some(path) = &self.config else {
            return Ok(false);
        };
        let config = Config::read(path, &self.secrets)
            .map_err(|refused| format!("the config cannot be used: {refused}"))?;
        Ok(config.decisions_cached())
    }

    /// What is installed, as this was given it: the inventory of the models
    /// directory that was opened, or none.
    pub fn installations(&self) -> Option<&Arc<Installations>> {
        self.installed.as_ref().map(|opened| &opened.installations)
    }
}

/// Reads one recipe file, as [`Over::recipe_files`] says: what a config lists, and what
/// `whizbang rascal run` is given. The file that was looked at is the file that is read:
/// after it is opened, its device and inode are compared with the look's, and a file
/// replaced between the two is refused, as a set's file is (`whizbang_eval::sets`). Never
/// more than the limit and one byte is read.
pub fn recipe_file(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let shown = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let refused = |why: &str| format!("the recipe file {}: {why}", shown.escape_debug());
    let meta = std::fs::symlink_metadata(path).map_err(|_| refused("cannot be read"))?;
    if !meta.file_type().is_file() {
        return Err(refused("is a link or not a file, and is not read"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o022 != 0 {
            return Err(refused(
                "can be written by its group or others, and is not run",
            ));
        }
    }
    let limit = whizbang_core::recipe::FILE_MAX_BYTES;
    if meta.len() > limit as u64 {
        return Err(refused(&format!("is over {limit} bytes")));
    }
    let file = std::fs::File::open(path).map_err(|_| refused("cannot be read"))?;
    let opened = file.metadata().map_err(|_| refused("cannot be read"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if (opened.dev(), opened.ino()) != (meta.dev(), meta.ino()) {
            return Err(refused("was replaced while it was opened, and is not read"));
        }
    }
    if !opened.file_type().is_file() {
        return Err(refused("is a link or not a file, and is not read"));
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| refused("cannot be read"))?;
    if bytes.len() > limit {
        return Err(refused(&format!("is over {limit} bytes")));
    }
    String::from_utf8(bytes).map_err(|_| refused("is not UTF-8"))
}

/// Makes this folder and every folder above it that is not there, each for its
/// owner only. A folder that is there is left as it is.
fn made_for_its_owner(folder: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(folder)
}

/// What a command says of a models directory that was not opened: the opening's own
/// sentence, which says how to mend it and holds no path, and which of the three
/// named the directory.
fn not_used(refused: OpenRefused, said: Said) -> String {
    let named = match said {
        Said::Flag => "the one --models-dir names",
        Said::Config => "the one the config's `models.dir` names",
        Said::Platform => "the one in this machine's data folder for whizbang",
    };
    format!("{refused} (the models directory is {named})")
}

impl std::fmt::Debug for Over {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Over")
            .field("config", &self.config.is_some())
            .field("env_file", &self.env.is_some())
            .field("kinds", &self.kinds)
            .field("secrets", &self.secrets)
            .field("models", &self.installed.is_some())
            .field("pack", &self.pack.is_some())
            .finish()
    }
}

impl Over {
    /// The source over the two files, when a config is named: the kinds of the
    /// build, and the secrets it was told of.
    fn files(&self) -> Option<Files> {
        let config = self.config.as_deref()?;
        let files = Files::new(config, self.env.as_deref(), self.kinds.clone());
        Some(files.knowing(self.secrets.clone()).over(self.inventory()))
    }

    /// What is installed, as readiness asks it (ADR 0015, "An inventory"): the
    /// models directory this was given ([`Over::installed`]), or the inventory that
    /// knows no model. **What a config offers always stands over one of the two,**
    /// so a local backend is prepared under the identity of an installation or is
    /// not ready: under no models directory no local backend is ready, and none is
    /// ever answered as its kind declared it, with nothing known of what is
    /// installed and no lease to hold it.
    fn inventory(&self) -> Arc<dyn Inventory> {
        match self.installations() {
            Some(installed) => installed.clone(),
            None => Arc::new(NoInventory),
        }
    }

    /// What `whizbang backends status` reads (plan 1C, task P7b): the config and its
    /// env file, read once as a call's source reads them, what they offer with the
    /// kinds of the build over what is installed, and the one worker of the process
    /// when one was given. Nothing is made but what the kinds make of their tables,
    /// and nothing is started or sent. With no config named it is none: the empty
    /// config offers no backend. Otherwise, why the config cannot be used, in the
    /// readers' own words ([`loaded`]).
    pub(crate) fn configured(&self) -> Result<Option<Configured>, String> {
        let Some(files) = self.files() else {
            return Ok(None);
        };
        let loaded = files
            .load()
            .map_err(|refused| format!("the config cannot be used: {refused}"))?;
        let inventory = self.inventory();
        let offers = Offers::of(loaded.config(), loaded.credentials(), &self.kinds)
            .map_err(|refused| format!("the config cannot be used: {refused}"))?
            .over(inventory.clone());
        let secrets = self.secrets.joined(&loaded.kept_out());
        Ok(Some(Configured {
            loaded,
            offers,
            inventory,
            worker: self.pack.clone(),
            secrets,
        }))
    }
}

/// What a config offers, as `whizbang backends status` reads it ([`Over::configured`]).
pub(crate) struct Configured {
    /// The config and its credentials, as they were read.
    pub(crate) loaded: Loaded,
    /// What it offers with the kinds of the build, over what is installed.
    pub(crate) offers: Offers,
    /// What is installed: the models directory that was opened, or the inventory that
    /// knows no model.
    pub(crate) inventory: Arc<dyn Inventory>,
    /// The one worker of the process, when one was given ([`Over::pack`]).
    pub(crate) worker: Option<Arc<OneWorker>>,
    /// Every secret the command knows: those it was told of and the env file's.
    pub(crate) secrets: Secrets,
}

impl Over {
    /// Where an executor built over this reads its config (ADR 0015, "One snapshot
    /// of the config"). With a config named it is the source over the config file
    /// and its env file, which reads both again each time it is asked. With none it
    /// is one snapshot of the empty config: its digest, no backend, no cap, the
    /// policy that approves nothing, and no setting. **Either way a snapshot holds
    /// backends,** those a config offers or none at all, so the executor asks and
    /// hands over the snapshot's and never any it was built with: a call with no
    /// backend to hand is refused as not ready, before admission.
    fn source(&self) -> Result<Arc<dyn ConfigSource>, String> {
        if let Some(files) = self.files() {
            return Ok(Arc::new(files));
        }
        let empty = Config::empty();
        let offers = Offers::of(&empty, &Credentials::none(), &self.kinds)
            .map_err(|error| error.to_string())?
            .over(self.inventory());
        let snapshot = Snapshot::of(empty.digest())
            .offering(Arc::new(offers))
            .sending_under(DispatchPolicy::none())
            .recording(Arc::new(empty));
        Ok(Arc::new(snapshot))
    }
}

/// The backends a command's run will use, as the command chose them before anything
/// is built (ADR 0011, "When the env file is asked for").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Uses<'a> {
    /// None: the operations of rascal, which ask no backend.
    Nothing,
    /// The one backend of this capability the command named, or with none named the
    /// capability's default; where the config gives no default, every backend of it.
    One(Capability, Option<&'a str>),
    /// Every backend of the config: `whizbang backends status`.
    Every,
}

/// Whether a command that will use these backends may go on without an env file:
/// asked before any backend is built and before anything but the config is read,
/// and only when a config is named and no env file is (ADR 0011, "When the env file
/// is asked for"). When one of the backends names a credential, the refusal names
/// the capability, the backend and the setting (`key_env` or `token_env`), and never
/// the setting's value: a key of capitals and digits pasted there cannot be told from
/// a variable's name. A backend the command names that the config does not give is
/// refused first. A config that cannot be read is not refused here: [`loaded`] says
/// why, in the readers' own words.
pub fn env_file_asked(over: &Over, uses: Uses<'_>) -> Result<(), String> {
    let (Some(path), None) = (&over.config, &over.env) else {
        return Ok(());
    };
    let Ok(config) = Config::read(path, &over.secrets) else {
        return Ok(());
    };
    // A backend the command names that the config does not give is refused at once,
    // without what was typed: nothing would answer the call.
    if let Uses::One(of, Some(named)) = uses
        && !config.backends(of).any(|(name, _)| name == named)
    {
        return Err(format!(
            "the config gives no {} backend of the name the command line gives; nothing \
             was read but the config, and nothing ran",
            of.name()
        ));
    }
    let capabilities = [Capability::Segment, Capability::Fill, Capability::Judge];
    let chosen: Vec<(Capability, &str, &whizbang_core::config::Backend)> = capabilities
        .into_iter()
        .flat_map(|capability| {
            let config = &config;
            config
                .backends(capability)
                .map(move |(name, backend)| (capability, name, backend))
        })
        .filter(|(capability, name, _)| match uses {
            Uses::Nothing => false,
            Uses::Every => true,
            Uses::One(of, named) => {
                *capability == of
                    && match named.or(config.default_of(of)) {
                        Some(chosen) => *name == chosen,
                        None => true,
                    }
            }
        })
        .collect();
    for (capability, name, backend) in chosen {
        if let Some((key, _)) = backend.credentials().next() {
            return Err(format!(
                "the {} backend `{name}` names its credential with `{key}`, and no env file \
                 was given: name the env file that sets it with --env-file; nothing was read \
                 but the config, and nothing ran",
                capability.name()
            ));
        }
    }
    Ok(())
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
    if let Some(refused) = &over.unplaced {
        return Err(refused.clone());
    }
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
/// **The two are given together, here and nowhere else.** With the meter alone a
/// bounded request of a run that reserved nothing passes unmetered; with the ledger
/// alone nothing a run sends is charged to what it reserved. What a call reserves is
/// the bound of one request to each backend it needs, times the requests its action
/// says the need is for (`Requirement::requests`; ADR 0010): one, unless the action
/// says more, as `check` does for the turns of its judge.
///
/// And the ways out (plan 2A, task S1): the network client as the sender behind every
/// checked send, with the check behind a send over it, keeping out the root of this
/// store (`net::ways`; ADR 0019, "For later tasks"). The ways approve no recipient:
/// a run sends under the policy of its config's snapshot, and under no config,
/// under the policy that approves nothing.
///
/// And the lease (`models::Lease`; ADR 0017, "The lease"), in the lease's place,
/// over the models directory this was given ([`Over::installed`]): before a request
/// to a local model leaves it takes a hold on the installation the run was admitted
/// with, and lets it go when the request is over.
///
/// What comes back otherwise is why the executor could not be built.
pub fn runner(registry: Registry, store: Arc<Store>, over: &Over) -> Result<Engine, String> {
    let digest = Config::empty().digest();
    let meter = Meter::new(store.clone(), Arc::new(SystemClock));
    let ways = over
        .ways(store.root())
        .map_err(|refused| format!("the ways out of the store cannot be made: {refused}"))?;
    let engine = Engine::over(registry, Arc::new(NoBackends), store, digest);
    let engine = wired(engine, over)?
        .with_ways(ways)
        .with_ledger(Arc::new(StoreLedger))
        .with_wrapper(Place::Meter, Arc::new(meter));
    // The lease stands in its place wherever there is a models directory to take
    // a hold in. Without one no local backend is ready ([`Over::inventory`]), so
    // no request ever comes to the place.
    Ok(match over.installations() {
        Some(installed) => {
            let lease = Lease::over(installed.clone());
            engine.with_wrapper(Place::Lease, Arc::new(lease))
        }
        None => engine,
    })
}

impl Over {
    /// The ways out of a runner over the store at this root: the network client's
    /// (`net::ways`), or, in a test that gave one, the same trusting its root.
    fn ways(&self, root: &Path) -> Result<Ways, whizbang_core::egress::NotAllowed> {
        #[cfg(feature = "fakes")]
        if let Some(pki) = &self.trusting {
            return whizbang_backends::net::testing::ways_trusting(pki, root, &self.secrets);
        }
        whizbang_backends::net::ways(root, &self.secrets)
    }
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
    let marks = whizbang_squint::marks::all();
    let marks = marks.iter().map(|(tool, mark)| (*tool, mark.word()));
    Description::new(env!("CARGO_PKG_VERSION"), registry).with_marks(marks)
}

/// The description as `whizbang describe` prints it for a person: the build, then each
/// action's name and purpose.
pub fn describe_text(description: &Description) -> String {
    let mut text = format!("{} {}\n", description.name(), description.version());
    if description.actions().is_empty() {
        text.push_str("no actions yet\n");
    }
    for action in description.actions() {
        let mark = description
            .marks()
            .iter()
            .find(|mark| mark.tool() == action.name());
        match mark {
            Some(mark) => text.push_str(&format!(
                "  {}  [{}]  {}\n",
                action.name(),
                mark.mark(),
                action.purpose()
            )),
            None => text.push_str(&format!("  {}  {}\n", action.name(), action.purpose())),
        }
        // When to use it, where it says: a recipe does.
        if let Some(when) = action.use_when() {
            text.push_str(&format!("      use: {when}\n"));
        }
    }
    text
}
