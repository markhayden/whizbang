//! `whizbang models list`, `get` and `remove` (plan 1C, task N3b; ADR 0017; the guide
//! `docs/guides/models.md`): what the binary's commands do, over the models directory's
//! interface (`whizbang_backends::models`). The binary parses its arguments and calls
//! these; a test calls them with a catalogue, a fake of the download and a clock of its
//! own ([`Tools::of`]).
//!
//! **Which of them works out where the models directory is, and when.** None does
//! unless the catalogue gives it a reason (the catalogue this build ships holds one
//! model, `sam3`, task N5); `pin` never does:
//!
//! - `list` with a catalogue that holds at least one model. It opens the directory
//!   only when something is there: where nothing is, every model is not installed,
//!   and nothing is made.
//! - `get` of a model the catalogue holds, that runs on this platform and whose
//!   adapter is in this build's table, shipped or not yet (task N5). It opens the
//!   directory, and makes it where it is not.
//! - `remove` of a model the catalogue holds. It opens the directory only when
//!   something is there: where nothing is, the model is not installed.
//! - `get` and `remove` of a name this build's catalogue does not hold (plan 1D, task
//!   U1): the directory is worked out, and opened where it is there, for the user's
//!   catalogue (`catalogue.json`) and adapters; nothing is made for the look.
//! - `adapter add` always: it opens the directory, and makes it where it is not.
//!
//! Each asks the flag first, then the config's `[models] dir`, then the platform's,
//! which is worked out only when the other two are silent ([`Models::chosen`]). A
//! relative `--models-dir` is refused by all three before anything else.
//!
//! **What is printed holds no secret the config names:** the values of the variables
//! it names are read from its env file first, and every text a command prints, a
//! complaint included, has them taken out. A model whose name holds one is not looked
//! for: to these commands it is not in the catalogue.
//!
//! **Only `get` reads a download's token, and only of a model that needs one** (plan
//! 1C, task N4; [`get`]): `list`, `remove`, and `get` of a model whose download needs
//! no token or whose token is a variable of the env file never open the Hub's
//! sign-in.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use whizbang_backends::models::{
    ADAPTER_MAX_BYTES, Carrying, Current, Entry, FileRefused, InstallRefused, ModelsDir,
    PinRefused, Pinned, RemoveRefused, Revision, State, Status, add_adapter, all_held, current,
    draining, install_or_attach_carrying, pin_file, remove as remove_model,
    remove_unnamed_adapters, unrunnable, verify_in_full,
};
use whizbang_core::capability::{FaultKind, Unready};
use whizbang_core::config::{
    Credentials, DOWNLOAD_LEAST_KIB_PER_SECOND, Loaded, ModelToken, SIGN_IN_ORIGIN, TokenFrom,
};
use whizbang_core::contract::{Cancellation, Clock, Secrets, SystemClock};
use whizbang_core::egress::{CheckedDownloader, Credential, NotAllowed, Origin};

use crate::sign_in::{self, SignInRefused};
use crate::wiring::{self, Models, Platform, Said, SignIn};

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
    sign_in: Option<SignIn>,
}

impl Tools {
    /// What the binary gives: the models this build ships, with the flag's directory
    /// and what says where the platform's is; the machine's clock; the download
    /// client; and what says where the Hub's sign-in keeps its token.
    pub fn shipped(
        dir: Option<&Path>,
        platform: Platform,
        sign_in: SignIn,
    ) -> Result<Tools, String> {
        let models = Models::shipped()?.flag(dir).platform(platform);
        let tools = Tools::of(
            models,
            Arc::new(SystemClock),
            Arc::new(whizbang_backends::net::downloader),
        );
        Ok(tools.sign_in(sign_in))
    }

    /// These, with no place for a sign-in: what a test gives.
    pub fn of(models: Models, clock: Arc<dyn Clock>, downloads: Downloads) -> Tools {
        Tools {
            models,
            clock,
            downloads,
            sign_in: None,
        }
    }

    /// The same, with what says where the Hub's sign-in keeps its token
    /// (`sign_in::token_file`). **It is not asked here, and not unless it is
    /// needed:** only by `get` of a model whose entry says a download from its origin
    /// needs a token, under a config that names the sign-in for that origin. Without
    /// it, the sign-in is not there.
    pub fn sign_in(mut self, says: SignIn) -> Tools {
        self.sign_in = Some(says);
        self
    }
}

impl std::fmt::Debug for Tools {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tools")
            .field("models", &self.models)
            .field("sign_in", &self.sign_in.is_some())
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
/// least rate of a download, the secrets of the variables it names, and where it says
/// each download's token is, with the tokens of its env file.
struct Read {
    dir: Option<std::path::PathBuf>,
    least: u64,
    secrets: Secrets,
    tokens: Vec<ModelToken>,
    credentials: Credentials,
    /// Whether an env file was named: without one a variable is never named back
    /// (ADR 0011, "When the env file is asked for").
    env_file: bool,
    /// The origins the config approves (`[egress.recipients]`): each recipient's, and
    /// those it may pass data on to. A download of a user's model reaches no other
    /// (plan 1D, task U1).
    approved: Vec<Origin>,
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
            tokens: Vec::new(),
            credentials: Credentials::none(),
            env_file: under.env_file.is_some(),
            approved: Vec::new(),
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
        secrets: loaded.kept_out(),
        tokens: config.model_tokens().to_vec(),
        credentials: loaded.credentials().clone(),
        env_file: under.env_file.is_some(),
        approved: config
            .recipients()
            .flat_map(|(_, approval)| {
                std::iter::once(approval.origin().clone()).chain(approval.upstreams().to_vec())
            })
            .collect(),
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
fn entry<'a>(models: &'a Models, name: &str, secrets: &Secrets) -> Result<&'a Entry, String> {
    match models.catalogue.entry(name) {
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

/// The revision this build's pack ships the entry's adapter under, when it ships it; or,
/// for a user's adapter, the hash of its copy the directory's registry names.
fn adapter_of<'a>(models: &'a Models, entry: &Entry) -> Option<&'a Revision> {
    let adapters = &models.adapters;
    adapters
        .iter()
        .find(|(name, _)| name == entry.adapter())
        .map(|(_, revision)| revision)
}

/// The revision this build's table fixes for the entry's adapter, shipped or not yet:
/// what `get` installs under (plan 1C, task N5). A model whose adapter is in the
/// table but not shipped yet is installed under the revision it will be loaded under,
/// so nothing is downloaded again when the adapter ships.
fn revision_of<'a>(models: &'a Models, entry: &Entry) -> Option<&'a Revision> {
    adapter_of(models, entry).or_else(|| {
        models
            .not_yet_shipped
            .iter()
            .find(|(name, _)| name == entry.adapter())
            .map(|(_, revision)| revision)
    })
}

/// Whether anything is at the directory's place, a link followed. What cannot be
/// looked at is said by the opening.
fn there(root: &Path) -> bool {
    !matches!(std::fs::metadata(root), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
}

/// The models a command about this model is over: this build's, when its catalogue
/// holds the model; otherwise this build's and the user's of the models directory, when
/// there is one to look in (plan 1D, task U1). Nothing is made for the look.
fn with_users_for(tools: &Tools, model: &str, read: &Read) -> Result<Models, String> {
    if tools.models.catalogue.entry(model).is_some() {
        return Ok(tools.models.clone());
    }
    let (said, root) = tools.models.chosen(read.dir.as_deref())?;
    if !root.is_dir() {
        return Ok(tools.models.clone());
    }
    let dir = wiring::open_models_dir(said, &root, tools.clock.clone())?;
    tools.models.clone().with_users(&dir, &read.secrets)
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
        // The user's models and adapters beside this build's (plan 1D, task U1).
        let models = match &dir {
            Some(dir) => tools.models.clone().with_users(dir, &read.secrets)?,
            None => tools.models.clone(),
        };
        let entries = models.catalogue.entries();
        let mut text = format!("models: {}\n", entries.len());
        text.push_str(&format!("directory: {}", named(said)));
        text.push_str(if dir.is_some() {
            "\n"
        } else {
            ", not made yet\n"
        });
        let inventory = dir.as_ref().map(|dir| {
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
                _ => absent(&models, entry),
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
fn absent(models: &Models, entry: &Entry) -> String {
    match unrunnable(entry, models.runs_on, adapter_of(models, entry).is_some()) {
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
/// **A file another installation holds** is linked and not downloaded (plan 1D, task
/// Q1; ADR 0017, "A file shared between installations").
///
/// **A credential** (plan 1C, D12): an entry that says a download from its origin
/// needs a token is installed with one, which every request for that origin carries
/// and no request for another does. Where the token is, the config says for that
/// origin (`[[models.tokens]]`): a variable of the env file, or the Hub's
/// command-line sign-in, which is read here, after the checks above and before the
/// models directory is made or anything is asked, from the place the binary was
/// given ([`Tools::sign_in`]), and by no other command. **It is not read when
/// every file of the entry is held** by an installation in the directory: nothing is
/// to be downloaded (plan 1D, task Q1). It is told to redaction
/// before the request is made, so nothing this prints, the status record and every
/// refusal hold it, and it is dropped when the install ends. One limit: whether every
/// file is held is looked at before the model's lock is taken, so a file removed in
/// between is asked for without the token, its origin refuses it, and `get` says to run again. An origin that refuses the token (401 or
/// 403) ends the install with a sentence that says to ask for access.
pub fn get(
    under: Under<'_>,
    model: &str,
    tools: &Tools,
    say: &mut dyn FnMut(&str),
) -> Result<String, String> {
    get_until(under, model, tools, &Cancellation::new(), say)
}

/// [`get`], until `cancellation` is tripped (plan m5, task M1: the playground's Cancel and
/// its quit). An install this request started ends cancelled, and the installation that
/// was there before it is left as it was; one this request attached to goes on, and
/// this request leaves it (ADR 0017, "Attaching and cancelling").
pub fn get_until(
    under: Under<'_>,
    model: &str,
    tools: &Tools,
    cancellation: &Cancellation,
    say: &mut dyn FnMut(&str),
) -> Result<String, String> {
    run(under, tools, |read| {
        let models = with_users_for(tools, model, read)?;
        let entry = entry(&models, model, &read.secrets)?;
        if unrunnable(entry, tools.models.runs_on, true) == Some(Unready::NotForThisPlatform) {
            return Err(format!(
                "the model does not run on this platform (its entry names {}), so it was not \
                 installed and nothing was downloaded",
                platforms(entry, ", ")
            ));
        }
        let Some(adapter) = revision_of(&models, entry) else {
            return Err(
                "this build has no adapter for the model, so it was not installed and \
                        nothing was downloaded"
                    .to_owned(),
            );
        };
        // A user's model is downloaded only from origins the config approves (plan 1D,
        // task U1): the shipped catalogue's origins are this build's, a user's are not.
        if models.is_users(entry)
            && !entry
                .origins()
                .iter()
                .all(|origin| read.approved.contains(origin))
        {
            return Err(
                "the model's entry in the models directory's catalogue names an origin the \
                 config does not approve as a recipient (`[egress.recipients]`), so it was \
                 not installed and nothing was downloaded"
                    .to_owned(),
            );
        }
        let (said, root) = tools.models.chosen(read.dir.as_deref())?;
        // Every file held by an installation in the directory already: the install
        // links them and downloads nothing, so no token is read (plan 1D, task Q1;
        // ADR 0017, "A file shared between installations"). A directory that is not
        // there holds nothing, and is not made for the look; a look that fails is no
        // answer, and the token is read as before.
        let held = root.is_dir()
            && wiring::open_models_dir(said, &root, tools.clock.clone())
                .is_ok_and(|dir| all_held(&dir, entry, &read.secrets).unwrap_or(false));
        // The token, read now and not before: the model is to be installed, a file
        // is to be downloaded, and nothing has been asked yet. From here on every
        // text has it taken out, beside the config's secrets.
        let credential = if held {
            None
        } else {
            credential_of(entry, read, tools)?
        };
        let secrets = match &credential {
            Some((_, joined)) => joined.clone(),
            None => read.secrets.clone(),
        };
        let carried = credential.as_ref().map(|(credential, _)| credential);
        let installed = (|| -> Result<String, String> {
            let dir = wiring::open_models_dir(said, &root, tools.clock.clone())?;
            let downloader = (tools.downloads)(read.least, entry.origins(), &secrets)
                .map_err(|refused| format!("the download client could not be made: {refused}"))?;
            let mut last = None;
            let clock = tools.clock.clone();
            // A file another installation holds is linked, not downloaded (plan 1D,
            // task Q1).
            let carrying = Carrying {
                downloader: &downloader,
                credential: carried,
                shares: true,
            };
            let installed = install_or_attach_carrying(
                &dir,
                entry,
                adapter,
                &carrying,
                cancellation,
                &secrets,
                &mut |status| {
                    let line = shown(status);
                    if last.as_ref() != Some(&line) {
                        say(&secrets.redact(&line));
                        last = Some(line);
                    }
                    clock.pause(PAUSE);
                },
            )
            .map_err(|refused| {
                // Every file was held when looked at, so no token was read; one was
                // removed before the model's lock was taken, and its download went
                // without one (the review of plan 1D, task Q1).
                let raced = held && entry.needs_token().is_some();
                not_installed(&refused, carried.is_some(), raced)
            })?;
            // The installation this one replaced was deleted: a copy of a user's
            // adapter only it named goes with it (plan 2C, task F1).
            let _ = remove_unnamed_adapters(&dir, &secrets);
            Ok(format!(
                "installed {}: {}\n",
                entry.name().as_str(),
                installed.identity().digest()
            ))
        })();
        // The credential, and with it the token, is dropped here: nothing keeps it
        // after the install ends.
        drop(credential);
        match installed {
            Ok(text) => Ok(secrets.redact(&text)),
            Err(complaint) => Err(secrets.redact(&complaint)),
        }
    })
}

/// The header a download's token is carried in, as a bearer's.
const TOKEN_HEADER: &str = "Authorization";

/// The credential a download of this entry carries, and the secrets that hold its
/// token beside the config's; none when no download of the entry needs a token (plan
/// 1C, D12). The token is where the config says for the entry's origin: a variable
/// of the env file, read with the config; or the Hub's sign-in, read here, from the
/// place the binary was given, and never for another origin. Refused, in fixed words
/// that hold no token and no path and say how to sign in or which key to set: an
/// origin the config names no token for, a variable the env file does not set, and a
/// sign-in that is not there or cannot be read.
fn credential_of(
    entry: &Entry,
    read: &Read,
    tools: &Tools,
) -> Result<Option<(Credential, Secrets)>, String> {
    let Some(origin) = entry.needs_token() else {
        return Ok(None);
    };
    let not_downloaded = "so the model was not installed and nothing was downloaded";
    let token = match read.tokens.iter().find(|token| token.origin() == origin) {
        None => {
            // No default: the config names the sign-in or nobody's token is read, so
            // a `get` never reads the Hub's token of one who did not opt in. The
            // refusal ends with the lines to add, each a line of its own.
            let head = format!(
                "a download of the model from {origin} needs a token, and the config names \
                 none for that origin, {not_downloaded}."
            );
            return Err(if origin.to_string() == SIGN_IN_ORIGIN {
                format!(
                    "{head} To use the sign-in of the Hub's command-line tool (`hf auth \
                     login`), add these three lines to the config; or name a variable of the \
                     env file that holds the token, as `key_env` in place of `sign_in`:\n\
                     [[models.tokens]]\norigin = \"{origin}\"\nsign_in = \"huggingface\""
                )
            } else {
                format!(
                    "{head} Set the token in the env file, and add these three lines to the \
                     config, with the variable's name:\n[[models.tokens]]\norigin = \
                     \"{origin}\"\nkey_env = \"NAME_OF_THE_VARIABLE\""
                )
            });
        }
        Some(token) => match token.from() {
            TokenFrom::Variable(_) => match read.credentials.model_token(origin) {
                Some(value) => value.expose().to_owned(),
                // The download is the one use of the token, so the env file is asked
                // for here; the variable is not named, since a key pasted in its
                // place cannot be told from a name.
                None if !read.env_file => {
                    return Err(format!(
                        "the config names a variable in `[[models.tokens]]` for the token of \
                         {origin}, and no env file was given, {not_downloaded}: name the env \
                         file that sets it with --env-file"
                    ));
                }
                // The variable is not named: a key pasted in its place cannot be told
                // from a name.
                None => {
                    return Err(format!(
                        "the env file does not set the variable the config's \
                         `[[models.tokens]]` `key_env` names for the token of {origin}, \
                         {not_downloaded}: set it in the env file"
                    ));
                }
            },
            TokenFrom::SignIn => {
                let place = tools.sign_in.as_ref().and_then(|says| says());
                let got = match place {
                    Some(place) => sign_in::read(&place),
                    None => Err(SignInRefused::NoPlace),
                };
                match got {
                    Ok(token) => token.expose().to_owned(),
                    Err(refused) => {
                        return Err(format!(
                            "the token of {origin} is the Hub's sign-in's, {not_downloaded}: \
                             {refused}"
                        ));
                    }
                }
            }
        },
    };
    // Told to redaction before anything is asked: every text from here on, the
    // client's faults among them, has the token taken out. A token is at least as
    // long as a secret (`sign_in::read`, and the env file's reader).
    let told = Secrets::new([token.clone()])
        .map_err(|_| format!("the token is shorter than a token, {not_downloaded}"))?;
    let secrets = read.secrets.joined(&told);
    let credential = Credential::new(
        origin.clone(),
        TOKEN_HEADER,
        &format!("Bearer {token}"),
        &secrets,
    )
    .map_err(|_| format!("the token cannot be carried in a request, {not_downloaded}"))?;
    Ok(Some((credential, secrets)))
}

/// What `get` says of an install that was refused. An origin that refused a request
/// that carried a token refused that token: the account it is of has no access to the
/// model, or it is not the right token.
///
/// When every file was held at the look before the lock, no token was read; an origin
/// that then refuses a download for want of one met a held file removed in between,
/// and `get` again reads the token (`raced`).
fn not_installed(refused: &InstallRefused, carried: bool, raced: bool) -> String {
    let credential = matches!(
        refused,
        InstallRefused::File {
            why: FileRefused::Download(FaultKind::Credential),
            ..
        }
    );
    if credential && carried {
        return format!(
            "the model was not installed: {refused}. The origin refused the token: ask for \
             access to the model on its page with the account the token is of (on the Hub, \
             accept the model's terms there), and check that the token is that account's"
        );
    }
    if credential && raced {
        return format!(
            "the model was not installed: {refused}. A file another installation held was \
             removed while this one was made, and its download carried no token, since none \
             was to be downloaded: run `whizbang models get` again, which reads the token"
        );
    }
    if credential {
        return format!(
            "the model was not installed: {refused}. The origin asks for a token, and the \
             catalogue does not say that the model needs one"
        );
    }
    format!("the model was not installed: {refused}")
}

// --- pin -----------------------------------------------------------------------------------------

/// `whizbang models pin <draft>`: a developer's command (plan 1C, task N5). The draft is
/// a catalogue (`catalogue/2`) of one entry whose sizes and hashes are not known yet:
/// each file's `size` is the most bytes wanted of it, and its `sha256` any hash. Each
/// file is fetched once, through the checked downloader held to the draft's origins,
/// with the draft's token as `get` would carry it, and a line of its name, size and
/// SHA-256 is printed. **Nothing is installed or written:** no models directory is
/// opened, and the bytes are hashed as they come and dropped.
///
/// A file moved to an origin the draft does not name is not fetched there, and its
/// line names that origin, and only the origin: so the host a download is moved to
/// is named before an entry lists it (open question 2). A file that does not end
/// within its bound, or whose download failed, says so in its line, with the fault's
/// kind and fixed words. Every line has the config's secrets and the token taken
/// out. What is pinned is what the origin served that once: trust on first use.
pub fn pin(under: Under<'_>, draft: &Path, tools: &Tools) -> Result<String, String> {
    run(under, tools, |read| {
        // A draft is a file: anything else is refused before it is opened, so a
        // pipe in its place cannot hold the command.
        let unreadable = "the draft cannot be read: name a catalogue file of one entry";
        if !std::fs::metadata(draft).is_ok_and(|meta| meta.is_file()) {
            return Err(unreadable.to_owned());
        }
        let file = std::fs::File::open(draft).map_err(|_| unreadable.to_owned())?;
        let catalogue = whizbang_backends::models::Catalogue::read_from(file, &read.secrets)
            .map_err(|refused| format!("the draft cannot be used: {refused}"))?;
        let [entry] = catalogue.entries() else {
            return Err(format!(
                "the draft holds {} models, and pin takes one",
                catalogue.entries().len()
            ));
        };
        let credential = credential_of(entry, read, tools)?;
        let secrets = match &credential {
            Some((_, joined)) => joined.clone(),
            None => read.secrets.clone(),
        };
        let carried = credential.as_ref().map(|(credential, _)| credential);
        let refused = Arc::new(std::sync::Mutex::new(Vec::<Origin>::new()));
        let noted = refused.clone();
        let downloader = (tools.downloads)(read.least, entry.origins(), &secrets)
            .map_err(|refused| format!("the download client could not be made: {refused}"))?
            .noting_refused(Arc::new(move |origin: &Origin| {
                if let Ok(mut noted) = noted.lock() {
                    noted.push(origin.clone());
                }
            }));
        let carrying = Carrying {
            downloader: &downloader,
            credential: carried,
            shares: false,
        };
        let mut text = format!(
            "pinned {}: nothing was installed or written\n",
            entry.name().as_str()
        );
        let mut whole = true;
        for file in entry.files() {
            if let Ok(mut noted) = refused.lock() {
                noted.clear();
            }
            let pinned = pin_file(entry, file, &carrying, &Cancellation::new());
            let name = file.name().as_str();
            let line = match pinned {
                Ok(Pinned { size, sha256 }) => format!("{name}\t{size}\t{sha256}"),
                Err(why) => {
                    whole = false;
                    let said = match why {
                        PinRefused::OverBound => {
                            format!("more than its bound of {} bytes came", file.size())
                        }
                        PinRefused::Elsewhere => {
                            "it moved to an origin the draft does not name".to_owned()
                        }
                        PinRefused::TooManyMoves => "it moved too many times".to_owned(),
                        PinRefused::Cancelled => "it was cancelled".to_owned(),
                        PinRefused::Download(fault) => {
                            format!("{}: {}", fault.kind().name(), fault.sentence())
                        }
                    };
                    let elsewhere = refused
                        .lock()
                        .map(|noted| noted.clone())
                        .unwrap_or_default();
                    match elsewhere.first() {
                        Some(origin) => format!(
                            "{name}\tnot pinned: {said}; it was moved to {origin}, which the \
                             draft does not name, and nothing was asked there"
                        ),
                        None => format!("{name}\tnot pinned: {said}"),
                    }
                }
            };
            text.push_str(&secrets.redact(&line));
            text.push('\n');
        }
        drop(credential);
        if whole { Ok(text) } else { Err(text) }
    })
}

// --- adapter add ---------------------------------------------------------------------------------

/// `whizbang models adapter add <file> --name <name>` (plan 1D, task U1; ADR 0017, "A
/// user's weights and a user's adapter"): copies one Python file the user wrote into the
/// models directory, outside the pack `whizbang setup` refreshes, as a read-only copy
/// named by its hash, and records `user-<name>` and that hash in the directory's
/// registry. **Nothing is fetched, and nothing of the file runs here:** it runs only
/// in the worker, for an installation made under this hash, and only when the copy's
/// bytes still hash to it. A model of the user's catalogue that names the adapter is
/// installed under it by `get`; adding a changed file under the same name is a new
/// hash, and `get` installs the model again under it, while an installation made
/// under the earlier copy keeps it. The file is a regular file of at most
/// [`ADAPTER_MAX_BYTES`]; the directory is made where it is not.
pub fn adapter_add(
    under: Under<'_>,
    file: &Path,
    name: &str,
    tools: &Tools,
) -> Result<String, String> {
    run(under, tools, |read| {
        let unreadable = "the adapter's file cannot be read: name one Python file";
        if !std::fs::metadata(file).is_ok_and(|meta| meta.is_file()) {
            return Err(unreadable.to_owned());
        }
        let mut code = Vec::new();
        std::fs::File::open(file)
            .and_then(|opened| {
                use std::io::Read as _;
                opened.take(ADAPTER_MAX_BYTES + 1).read_to_end(&mut code)
            })
            .map_err(|_| unreadable.to_owned())?;
        let (said, root) = tools.models.chosen(read.dir.as_deref())?;
        let dir = wiring::open_models_dir(said, &root, tools.clock.clone())?;
        let added = add_adapter(&dir, name, &code, &read.secrets)
            .map_err(|refused| format!("the adapter was not added: {refused}"))?;
        Ok(format!(
            "added {}: sha256:{}{}\n",
            added.name.as_str(),
            added.revision.as_str(),
            if added.copied {
                ""
            } else {
                " (the same file was added before)"
            }
        ))
    })
}

// --- verify --------------------------------------------------------------------------------------

/// `whizbang models verify <model>`: the full check of the model's current installation,
/// asked for by hand (ADR 0017, "The cached check"; plan 2C, task F1). Every file is
/// read whole, whatever an earlier check recorded; when all are whole the check record
/// is written, so the next load trusts them; when one is not, nothing is written and
/// the answer says to install the model again.
pub fn verify(under: Under<'_>, model: &str, tools: &Tools) -> Result<String, String> {
    run(under, tools, |read| {
        let secrets = &read.secrets;
        let models = with_users_for(tools, model, read)?;
        let entry = entry(&models, model, secrets)?;
        let name = entry.name().as_str();
        let (said, root) = tools.models.chosen(read.dir.as_deref())?;
        let not_installed = || format!("{name} is not installed, so nothing was checked");
        if !there(&root) {
            return Err(not_installed());
        }
        let dir = wiring::open_models_dir(said, &root, tools.clock.clone())?;
        let record = match current(&dir, entry.name(), secrets) {
            Ok(Current::Installed(record)) => record,
            Ok(Current::NotInstalled) => return Err(not_installed()),
            Ok(Current::Damaged(_)) => {
                return Err(format!(
                    "{name} is damaged: its pointer or the record it names cannot be read; \
                     `get` installs it again"
                ));
            }
            Err(error) => return Err(format!("{name} could not be read: {error}")),
        };
        let identity = record.identity();
        match verify_in_full(&dir, entry.name(), identity, secrets, &|| false) {
            Ok(true) => Ok(format!("whole {name}: {}\n", identity.digest())),
            Ok(false) => Err(format!(
                "{name} is not whole: a file of its installation {} is missing, of another \
                 size or hash, or a link; `get` installs it again",
                identity.digest()
            )),
            Err(error) => Err(format!("{name} could not be checked: {error}")),
        }
    })
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
        let models = with_users_for(tools, model, read)?;
        let entry = entry(&models, model, secrets)?;
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
        // A copy of a user's adapter that only the removed installation named goes
        // with it (plan 2C, task F1); one that cannot be taken now is taken by the next.
        let _ = remove_unnamed_adapters(&dir, secrets);
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

// --- the playground's Models screen (plan m5, task M1) -------------------------------------

/// One model of the catalogue as the playground's Models screen shows it: what `list`
/// prints of it, with its state as a word beside the words, and how far an install of
/// it that is running has got, from its status record (files done of files: an install
/// counts files, not bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub title: String,
    pub licence: String,
    pub bytes: u64,
    pub files: u64,
    /// `installed`, `not_installed`, `damaged`, `out_of_date`, `being_removed`,
    /// `not_for_this_platform`, `no_adapter`, `link` or `unreadable`.
    pub state: &'static str,
    /// The state in words, as `list` prints it.
    pub why: String,
    /// While an install of the model runs, here or in another process: the files it has
    /// staged.
    pub files_done: Option<u64>,
    /// Whether a download of the model needs a token the config does not lead to:
    /// `none` when it needs none or the config names where it is; `not_named` when the
    /// config names none for its origin; `not_set` when the config names a variable of
    /// the env file that the env file does not set. Never the token, nor the variable.
    pub sign_in: &'static str,
}

/// The state of a model as a word, from the words [`state`] and [`absent`] say.
fn state_word(words: &str) -> &'static str {
    let ends = |tail: &str| words.ends_with(tail);
    if words == "installed" {
        "installed"
    } else if words == "being removed" {
        "being_removed"
    } else if words.contains("is a link") {
        "link"
    } else if words.contains("cannot be read") {
        "unreadable"
    } else if ends(&Unready::NotForThisPlatform.to_string()) {
        "not_for_this_platform"
    } else if ends(&Unready::NoAdapter.to_string()) {
        "no_adapter"
    } else if words.contains("out of date") {
        "out_of_date"
    } else if words.starts_with("damaged") {
        "damaged"
    } else {
        "not_installed"
    }
}

/// Whether the config leads to the token a download of this entry needs (see
/// [`Row::sign_in`]). The Hub's sign-in is not read: that it is named is enough here,
/// and `get` says when it is not there.
fn sign_in_of(entry: &Entry, read: &Read) -> &'static str {
    let Some(origin) = entry.needs_token() else {
        return "none";
    };
    match read.tokens.iter().find(|token| token.origin() == origin) {
        None => "not_named",
        Some(token) => match token.from() {
            TokenFrom::SignIn => "none",
            TokenFrom::Variable(_) => match read.credentials.model_token(origin) {
                Some(_) => "none",
                None => "not_set",
            },
        },
    }
}

/// The catalogue's models as the Models screen shows them: [`list`]'s, as rows. Nothing
/// is made: a directory that is not there holds nothing.
pub fn rows(under: Under<'_>, tools: &Tools) -> Result<Vec<Row>, String> {
    tools.models.flag_is_absolute()?;
    let read = read(under)?;
    let secrets = &read.secrets;
    let (said, root) = tools.models.chosen(read.dir.as_deref())?;
    let dir = if there(&root) {
        Some(wiring::open_models_dir(said, &root, tools.clock.clone())?)
    } else {
        None
    };
    let models = match &dir {
        Some(dir) => tools.models.clone().with_users(dir, secrets)?,
        None => tools.models.clone(),
    };
    let inventory = dir.as_ref().map(|dir| {
        wiring::inventory_of(
            dir,
            &models.catalogue,
            &models.adapters,
            models.runs_on,
            secrets,
        )
    });
    let mut rows = Vec::new();
    for entry in models.catalogue.entries() {
        if holds_secret(entry.name().as_str(), secrets) {
            continue;
        }
        let words = match (&dir, &inventory) {
            (Some(dir), Some(inventory)) => state(dir, inventory, entry, secrets),
            _ => absent(&models, entry),
        };
        let files_done = dir.as_ref().and_then(|dir| {
            let status = whizbang_backends::models::status(dir, entry.name(), secrets).ok()??;
            (status.state() == State::Installing).then(|| status.files_done())
        });
        rows.push(Row {
            name: entry.name().as_str().to_owned(),
            title: secrets.redact(entry.title().as_str()),
            licence: secrets.redact(entry.licence().as_str()),
            bytes: entry.bytes(),
            files: entry.files().len() as u64,
            state: state_word(&words),
            why: secrets.redact(&words),
            files_done,
            sign_in: sign_in_of(entry, &read),
        });
    }
    Ok(rows)
}

/// Whether `get` of this model would start an install: the checks it makes before it
/// reads a token or makes anything, and that the config leads to the token its download
/// needs. Why not, in `get`'s words; nothing is read but the config and its env file.
pub fn installable(under: Under<'_>, model: &str, tools: &Tools) -> Result<(), String> {
    run(under, tools, |read| {
        let models = with_users_for(tools, model, read)?;
        let entry = entry(&models, model, &read.secrets)?;
        if unrunnable(entry, tools.models.runs_on, true) == Some(Unready::NotForThisPlatform) {
            return Err(format!(
                "the model does not run on this platform (its entry names {})",
                platforms(entry, ", ")
            ));
        }
        if revision_of(&models, entry).is_none() {
            return Err("this build has no adapter for the model".to_owned());
        }
        if models.is_users(entry)
            && !entry
                .origins()
                .iter()
                .all(|origin| read.approved.contains(origin))
        {
            return Err(
                "the model's entry names an origin the config does not approve as a \
                 recipient (`[egress.recipients]`)"
                    .to_owned(),
            );
        }
        match sign_in_of(entry, read) {
            "not_named" => Err(
                "a download of the model needs a sign-in, and the config names none for its \
                 origin: add one under `[[models.tokens]]` (docs/guides/models.md, \"A model whose files need a token\")"
                    .to_owned(),
            ),
            "not_set" => Err(
                "a download of the model needs a token, and the env file does not set the \
                 variable the config names for it: set it in the env file"
                    .to_owned(),
            ),
            _ => Ok(String::new()),
        }
    })
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_download_after_a_held_file_was_removed_says_to_get_again() {
        // The review of plan 1D, task Q1 (#182): no token was read, every file being
        // held; one was removed before the lock, and its origin refused the download.
        let refused = InstallRefused::File {
            name: "model.safetensors".to_owned(),
            why: FileRefused::Download(FaultKind::Credential),
        };
        let said = not_installed(&refused, false, true);
        assert!(
            said.ends_with("run `whizbang models get` again, which reads the token"),
            "{said}"
        );
        assert!(!said.contains("ask for access"), "{said}");
        let said = not_installed(&refused, false, false);
        assert!(
            said.ends_with("does not say that the model needs one"),
            "{said}"
        );
        let said = not_installed(&refused, true, false);
        assert!(said.contains("ask for access"), "{said}");
    }
}
