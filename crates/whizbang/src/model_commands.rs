//! `whizbang models list`, `get` and `remove` (plan 1C, task N3b; ADR 0017; the guide
//! `docs/guides/models.md`): what the binary's commands do, over the models directory's
//! interface (`whizbang_backends::models`). The binary parses its arguments and calls
//! these; a test calls them with a catalogue, a fake of the download and a clock of its
//! own ([`Tools::of`]).
//!
//! **Which of them works out where the models directory is, and when.** None does
//! unless the catalogue gives it a reason, and the catalogue this build ships holds no
//! model until its first entry (task N5):
//!
//! - `list` with a catalogue that holds at least one model. It opens the directory
//!   only when something is there: where nothing is, every model is not installed,
//!   and nothing is made.
//! - `get` of a model the catalogue holds, that runs on this platform and whose
//!   adapter this build has. It opens the directory, and makes it where it is not.
//! - `remove` of a model the catalogue holds. It opens the directory only when
//!   something is there: where nothing is, the model is not installed.
//!
//! Each asks the flag first, then the config's `[models] dir`, then the platform's,
//! which is worked out only when the other two are silent ([`Models::chosen`]). A
//! relative `--models-dir` is refused by all three before anything else.
//!
//! **What is printed holds no secret the config names:** the values of the variables
//! it names are read from its env file first, and every text a command prints, a
//! complaint included, has them taken out. A model whose name holds one is not looked
//! for: to these commands it is not in the catalogue.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use whizbang_backends::models::{
    Current, Entry, InstallRefused, ModelsDir, RemoveRefused, Revision, State, Status, current,
    draining, install_or_attach, remove as remove_model, unrunnable,
};
use whizbang_core::capability::Unready;
use whizbang_core::config::{DOWNLOAD_LEAST_KIB_PER_SECOND, Loaded};
use whizbang_core::contract::{Cancellation, Clock, Secrets, SystemClock};
use whizbang_core::egress::{CheckedDownloader, NotAllowed, Origin};

use crate::wiring::{self, Models, Platform, Said};

/// How long a command waits between two looks: one who attaches to an install that
/// is running, and a removal that waits for a lock or a hold. The clock's pause.
pub const PAUSE: Duration = Duration::from_millis(250);

/// The longest `--wait` a removal takes, in seconds: a day.
pub const WAIT_LIMIT_SECONDS: u64 = 86_400;

/// What makes the download client: given the least rate of a download in kibibytes
/// a second, the origins of the entry and the secrets the config names, a client held
/// to those origins. The binary gives the client's one public way,
/// `whizbang_backends::net::downloader`; a test gives a fake behind a checked
/// downloader.
pub type Downloads =
    Arc<dyn Fn(u64, &[Origin], &Secrets) -> Result<CheckedDownloader, NotAllowed> + Send + Sync>;

/// What a command is given beside what it was asked: the models (the catalogue, the
/// adapters, the platform, and what says where the directory is), the clock it waits
/// by, and what makes the download client.
#[derive(Clone)]
pub struct Tools {
    models: Models,
    clock: Arc<dyn Clock>,
    downloads: Downloads,
}

impl Tools {
    /// What the binary gives: the models this build ships, with the flag's directory
    /// and what says where the platform's is; the machine's clock; and the download
    /// client.
    pub fn shipped(dir: Option<&Path>, platform: Platform) -> Result<Tools, String> {
        let models = Models::shipped()?.flag(dir).platform(platform);
        Ok(Tools::of(
            models,
            Arc::new(SystemClock),
            Arc::new(whizbang_backends::net::downloader),
        ))
    }

    /// These: what a test gives.
    pub fn of(models: Models, clock: Arc<dyn Clock>, downloads: Downloads) -> Tools {
        Tools {
            models,
            clock,
            downloads,
        }
    }
}

impl std::fmt::Debug for Tools {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tools")
            .field("models", &self.models)
            .finish()
    }
}

/// The config a command runs under, and its env file (ADR 0011, "Where the file is").
#[derive(Debug, Clone, Copy, Default)]
pub struct Under<'a> {
    pub config: Option<&'a Path>,
    pub env_file: Option<&'a Path>,
}

/// What a command takes from the config: where it says the models directory is, the
/// least rate of a download, and the secrets of the variables it names.
struct Read {
    dir: Option<std::path::PathBuf>,
    least: u64,
    secrets: Secrets,
}

/// Reads the config and its env file, as an executor's source reads them; with no
/// config, nothing is said and no secret is known. A config that cannot be used is
/// said in the readers' own words, which hold no path.
fn read(under: Under<'_>) -> Result<Read, String> {
    let Some(config) = under.config else {
        return Ok(Read {
            dir: None,
            least: DOWNLOAD_LEAST_KIB_PER_SECOND,
            secrets: Secrets::none(),
        });
    };
    let loaded = Loaded::of_files(config, under.env_file)
        .map_err(|refused| format!("the config cannot be used: {refused}"))?;
    let config = loaded.config();
    Ok(Read {
        dir: config.models_dir().map(Path::to_owned),
        least: config
            .download_least_kib_per_second()
            .unwrap_or(DOWNLOAD_LEAST_KIB_PER_SECOND),
        secrets: loaded.secrets().clone(),
    })
}

/// Runs a command: the flag is looked at first, then the config is read; and what
/// comes back, either way, has the config's secrets taken out.
fn run(
    under: Under<'_>,
    tools: &Tools,
    command: impl FnOnce(&Read) -> Result<String, String>,
) -> Result<String, String> {
    tools.models.flag_is_absolute()?;
    let read = read(under)?;
    match command(&read) {
        Ok(text) => Ok(read.secrets.redact(&text)),
        Err(complaint) => Err(read.secrets.redact(&complaint)),
    }
}

/// The entry of the model of this name, compared exactly: none when the catalogue
/// holds no model of it, or when its name holds a secret.
fn entry<'a>(tools: &'a Tools, name: &str, secrets: &Secrets) -> Result<&'a Entry, String> {
    match tools.models.catalogue.entry(name) {
        Some(entry) if !holds_secret(entry.name().as_str(), secrets) => Ok(entry),
        _ => Err(
            "the catalogue holds no model of that name; `whizbang models list` names \
                  those it holds"
                .to_owned(),
        ),
    }
}

/// Whether this text holds one of these secrets.
fn holds_secret(text: &str, secrets: &Secrets) -> bool {
    secrets.redact(text) != text
}

/// The revision this build's adapter of the entry states, when the build has it.
fn adapter_of<'a>(tools: &'a Tools, entry: &Entry) -> Option<&'a Revision> {
    let adapters = &tools.models.adapters;
    adapters
        .iter()
        .find(|(name, _)| name == entry.adapter())
        .map(|(_, revision)| revision)
}

/// Whether anything is at the directory's place, a link followed. What cannot be
/// looked at is said by the opening.
fn there(root: &Path) -> bool {
    !matches!(std::fs::metadata(root), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
}

/// What is printed of where the directory is: which of the three named it, and
/// never its path.
fn named(said: Said) -> &'static str {
    match said {
        Said::Flag => "the one --models-dir names",
        Said::Config => "the one the config's `models.dir` names",
        Said::Platform => "the one in this machine's data folder for whizbang",
    }
}

/// A count of bytes as a person reads it: under a thousand as bytes, then in
/// thousands, millions or thousands of millions with one figure after the point.
pub fn size(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let float = bytes as f64;
    match bytes {
        0..1_000 => format!("{bytes} B"),
        1_000..1_000_000 => format!("{:.1} KB", float / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1} MB", float / 1e6),
        _ => format!("{:.1} GB", float / 1e9),
    }
}

/// The names of the platforms an entry runs on, joined by `,`, or `sep`.
fn platforms(entry: &Entry, sep: &str) -> String {
    let names: Vec<&str> = entry.platforms().iter().map(|p| p.name()).collect();
    names.join(sep)
}

// --- list ----------------------------------------------------------------------------------------

/// `whizbang models list`: for each model of the catalogue, a line of its name, title,
/// licence, size, platforms and state, apart by tabs, after a line of how many there
/// are and one of which directory was looked in.
pub fn list(under: Under<'_>, tools: &Tools) -> Result<String, String> {
    run(under, tools, |read| {
        let entries = tools.models.catalogue.entries();
        if entries.is_empty() {
            return Ok("models: 0: the catalogue holds no model yet\n".to_owned());
        }
        let (said, root) = tools.models.chosen(read.dir.as_deref())?;
        let dir = if there(&root) {
            Some(wiring::open_models_dir(said, &root, tools.clock.clone())?)
        } else {
            None
        };
        let mut text = format!("models: {}\n", entries.len());
        text.push_str(&format!("directory: {}", named(said)));
        text.push_str(if dir.is_some() {
            "\n"
        } else {
            ", not made yet\n"
        });
        let inventory = dir.as_ref().map(|dir| {
            let models = &tools.models;
            wiring::inventory_of(
                dir,
                &models.catalogue,
                &models.adapters,
                models.runs_on,
                &read.secrets,
            )
        });
        for entry in entries {
            if holds_secret(entry.name().as_str(), &read.secrets) {
                text.push_str("a model whose name holds a secret, not shown\n");
                continue;
            }
            let state = match (&dir, &inventory) {
                (Some(dir), Some(inventory)) => state(dir, inventory, entry, &read.secrets),
                _ => absent(tools, entry),
            };
            text.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{state}\n",
                entry.name().as_str(),
                entry.title().as_str(),
                entry.licence().as_str(),
                size(entry.bytes()),
                platforms(entry, ","),
            ));
        }
        Ok(text)
    })
}

/// The state of a model when there is no directory to look in: not installed, and
/// why no install by this build would make it ready, when none would.
fn absent(tools: &Tools, entry: &Entry) -> String {
    let models = &tools.models;
    match unrunnable(entry, models.runs_on, adapter_of(tools, entry).is_some()) {
        Some(why) => format!("not installed; not ready: {why}"),
        None => "not installed".to_owned(),
    }
}

/// The state of a model in a directory that was opened: being removed, a link in its
/// layout, or what the inventory says, with whether it is installed where the
/// inventory's reason is one no install would mend.
fn state(
    dir: &ModelsDir,
    inventory: &whizbang_backends::models::Installations,
    entry: &Entry,
    secrets: &Secrets,
) -> String {
    let name = entry.name();
    // A mark is a removal's only while its remover is alive (`models::draining`).
    match draining(dir, name, secrets) {
        Ok(true) => return "being removed".to_owned(),
        Ok(false) => {}
        Err(_) => return "its mark of a removal cannot be read".to_owned(),
    }
    match dir.has_link(name) {
        Ok(false) => {}
        Ok(true) => {
            return "a folder of its layout is a link, which whizbang does not go through; \
                    take the link away"
                .to_owned();
        }
        Err(_) => return "damaged: its folder cannot be looked at".to_owned(),
    }
    match inventory.ready(name.as_str()) {
        Ok(_) => "installed".to_owned(),
        Err(Unready::NotInstalled) => "not installed".to_owned(),
        Err(Unready::Damaged) => "damaged: get installs it again".to_owned(),
        Err(Unready::OutOfDate) => {
            "installed, not ready: its installation is out of date; get installs the current one"
                .to_owned()
        }
        Err(why @ (Unready::NoAdapter | Unready::NotForThisPlatform)) => {
            match current(dir, name, secrets) {
                Ok(Current::Installed(_)) => format!("installed, not ready: {why}"),
                Ok(Current::NotInstalled) => format!("not installed; not ready: {why}"),
                Ok(Current::Damaged(_)) | Err(_) => format!("damaged; not ready: {why}"),
            }
        }
        // The catalogue holds it: not reached, but for a name that holds a secret,
        // which is not listed.
        Err(Unready::NotInCatalogue) => "not in the catalogue".to_owned(),
    }
}

// --- get -----------------------------------------------------------------------------------------

/// What one who attaches says of what the status record shows.
fn shown(status: Option<Status>) -> String {
    match status {
        Some(status) if status.state() == State::Installing => format!(
            "another request is installing the model: {} of {} files staged",
            status.files_done(),
            status.files()
        ),
        _ => "another request holds the model, and is not installing it; waiting".to_owned(),
    }
}

/// `whizbang models get <model>`: installs the model, or attaches to the install of it
/// that is running and ends as that ended (ADR 0017, "Attaching and cancelling"). What
/// is answered is the installation's identity. While it waits for another's install,
/// it says what the status record shows each time that changes, through `say`, and
/// waits [`PAUSE`] by its clock between two looks.
///
/// Refused before anything is asked or made: a model the catalogue does not hold, one
/// whose entry does not name this platform, and one whose adapter this build does not
/// have, since no install by this build would make it ready.
///
/// A credential: none is attached to a request. An entry that says its origin needs a
/// token is task N4's, which reads it here, before the downloader is made; until then
/// the shipped catalogue holds no such entry.
pub fn get(
    under: Under<'_>,
    model: &str,
    tools: &Tools,
    say: &mut dyn FnMut(&str),
) -> Result<String, String> {
    run(under, tools, |read| {
        let secrets = &read.secrets;
        let entry = entry(tools, model, secrets)?;
        if unrunnable(entry, tools.models.runs_on, true) == Some(Unready::NotForThisPlatform) {
            return Err(format!(
                "the model does not run on this platform (its entry names {}), so it was not \
                 installed and nothing was downloaded",
                platforms(entry, ", ")
            ));
        }
        let Some(adapter) = adapter_of(tools, entry) else {
            return Err(
                "this build has no adapter for the model, so it was not installed and \
                        nothing was downloaded"
                    .to_owned(),
            );
        };
        let (said, root) = tools.models.chosen(read.dir.as_deref())?;
        let dir = wiring::open_models_dir(said, &root, tools.clock.clone())?;
        let downloader = (tools.downloads)(read.least, entry.origins(), secrets)
            .map_err(|refused| format!("the download client could not be made: {refused}"))?;
        let mut last = None;
        let clock = tools.clock.clone();
        let installed = install_or_attach(
            &dir,
            entry,
            adapter,
            &downloader,
            &Cancellation::new(),
            secrets,
            &mut |status| {
                let line = shown(status);
                if last.as_ref() != Some(&line) {
                    say(&secrets.redact(&line));
                    last = Some(line);
                }
                clock.pause(PAUSE);
            },
        )
        .map_err(|refused| not_installed(&refused))?;
        Ok(format!(
            "installed {}: {}\n",
            entry.name().as_str(),
            installed.identity().digest()
        ))
    })
}

/// What `get` says of an install that was refused.
fn not_installed(refused: &InstallRefused) -> String {
    format!("the model was not installed: {refused}")
}

// --- remove --------------------------------------------------------------------------------------

/// `whizbang models remove <model> --wait <seconds>`: removes the model (ADR 0017,
/// "Removal"). It marks the model as draining, so that no new hold is given, and waits
/// for the holds of runs that use it, [`PAUSE`] by its clock at a time, until `wait`
/// has passed; then it clears the pointer and deletes the files. Past its wait it
/// answers busy, and the model is installed as it was.
pub fn remove(
    under: Under<'_>,
    model: &str,
    wait: Duration,
    tools: &Tools,
) -> Result<String, String> {
    run(under, tools, |read| {
        let secrets = &read.secrets;
        let entry = entry(tools, model, secrets)?;
        let (said, root) = tools.models.chosen(read.dir.as_deref())?;
        if !there(&root) {
            return Err(not_removed(RemoveRefused::NotInstalled));
        }
        let dir = wiring::open_models_dir(said, &root, tools.clock.clone())?;
        let clock = tools.clock.clone();
        let removed = remove_model(&dir, entry.name(), &*clock, wait, secrets, &mut || {
            clock.pause(PAUSE);
        })
        .map_err(not_removed)?;
        Ok(match removed.installation() {
            Some(id) => format!("removed {}: {}\n", entry.name().as_str(), id.digest()),
            None => format!("removed {}\n", entry.name().as_str()),
        })
    })
}

/// What `remove` says of a removal that was refused; of one that changed nothing for
/// another or a hold, that the model is as it was.
fn not_removed(refused: RemoveRefused) -> String {
    match refused {
        RemoveRefused::Busy | RemoveRefused::Held | RemoveRefused::DirectoryBusy => format!(
            "the model was not removed: {refused}; nothing was removed, and the model is as it \
             was"
        ),
        other => format!("the model was not removed: {other}"),
    }
}
