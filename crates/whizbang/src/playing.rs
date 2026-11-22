//! What `whizbang play` offers through the playground host (`whizbang_serve::play`; plan
//! m5, task H1; ADR 0041): each action of the registry, a call of one made from the
//! page's arguments as the stdio server makes one from a tool's; the backends' readiness
//! for `/api/hello`; and the file a running host keeps in its store, `play.json`, by which
//! a second `whizbang play` finds it. Nothing here runs anything: a call goes through the
//! executor's entry point, as every door's does.
//!
//! The fixture host of the playground's Chrome tier (task U1) builds the same
//! [`Playground`] over the test kit's fakes.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json, json};
use whizbang_core::contract::{ActionDescriptor, Call, RequestId, RunId};
use whizbang_core::description::Kit;
use whizbang_core::store::Store;
use whizbang_core::store::fs::{Fs, RealFs};
use whizbang_serve::play::{Play, Refused, Token};
use whizbang_serve::tools;

use crate::backend_status;
use crate::pack::OneWorker;
use crate::wiring::{self, Over};

/// The name of the file a running host keeps in its store's folder.
pub const PLAY_FILE: &str = "play.json";

/// The actions of a registry, offered to the page, over what the executor was built
/// over; and the store's writer the executor holds, through which the page's pictures
/// and drawn locations are kept (plan m5, task W1, item 3), let go when the host stops.
pub struct Playground {
    descriptors: Vec<ActionDescriptor>,
    over: Arc<Over>,
    worker: Option<Arc<OneWorker>>,
    kits: Vec<Kit>,
    /// The actions each task's steps call, by the task's name.
    steps: BTreeMap<String, Vec<String>>,
    store: Mutex<Option<Arc<Store>>>,
    root: Option<PathBuf>,
    /// What was read and is kept, so that a poll or a page of thumbnails does not read the
    /// store again for what it already said.
    kept: Mutex<Remembered>,
    /// How many thumbnails were made, not taken from those kept.
    made: std::sync::atomic::AtomicU64,
    /// The binary's map from a run to its command line (plan m5, task W2, item 5), when
    /// it gave one: the commands' parser lives in the binary.
    commands: Option<Commands>,
}

/// The binary's map from what a run asked to the words of the command that asks it
/// again (`command_of.rs`), or why no command line says it.
pub type Commands =
    Arc<dyn Fn(&crate::play_result::Commanded) -> Result<Vec<String>, String> + Send + Sync>;

/// The most thumbnails kept in memory, and the most bytes of them.
pub const THUMBNAILS_KEPT: usize = 64;
pub const THUMBNAIL_BYTES_KEPT: usize = 16 * 1024 * 1024;

/// The most verdicts of ended runs kept in memory.
pub const VERDICTS_KEPT: usize = 512;

/// What the playground keeps of what it read: the allowance with the generation it was
/// read from; the newest thumbnails by identity and edge, the least recently used let go
/// first; and the verdict of each run that has ended, which never changes.
#[derive(Default)]
struct Remembered {
    allowance: Option<(u64, Option<Json>)>,
    thumbnails: std::collections::VecDeque<((String, u32), Vec<u8>)>,
    verdicts: std::collections::VecDeque<(String, Json)>,
}

/// The actions a recipe's steps call, from its text, in its order, each once.
fn steps_of(text: &str) -> Option<(String, Vec<String>)> {
    let recipe = whizbang_core::recipe::Recipe::read(text).ok()?;
    let mut called: Vec<String> = Vec::new();
    for step in recipe.steps {
        if !called.contains(&step.op) {
            called.push(step.op);
        }
    }
    Some((recipe.name, called))
}

impl Playground {
    /// The actions these descriptors describe, the backends `over` offers, and the
    /// pack's one worker of the process, stopped when the host ends. The kits are the
    /// build's ([`wiring::kits`]).
    pub fn new(descriptors: Vec<ActionDescriptor>, over: Arc<Over>) -> Playground {
        let worker = over.worker().cloned();
        let mut texts: Vec<String> = whizbang_rascal::recipes::SHIPPED
            .iter()
            .map(|text| (*text).to_owned())
            .chain(
                whizbang_squint::recipes::SHIPPED
                    .iter()
                    .map(|(_, text)| (*text).to_owned()),
            )
            .collect();
        texts.extend(over.recipe_files().unwrap_or_default());
        let mut steps: BTreeMap<String, Vec<String>> =
            texts.iter().filter_map(|text| steps_of(text)).collect();
        // The loop calls a removal and a check (plan m4, task L1).
        steps.insert(
            whizbang_rascal::checked::NAME.to_owned(),
            vec!["remove_prompt".to_owned(), "check".to_owned()],
        );
        Playground {
            descriptors,
            over,
            worker,
            kits: wiring::kits(),
            steps,
            store: Mutex::new(None),
            root: None,
            kept: Mutex::new(Remembered::default()),
            made: std::sync::atomic::AtomicU64::new(0),
            commands: None,
        }
    }

    /// The same, saying each run's command line through the binary's map.
    pub fn with_commands(mut self, commands: Commands) -> Playground {
        self.commands = Some(commands);
        self
    }

    fn tasks(&self) -> crate::play_result::Tasks<'_> {
        crate::play_result::Tasks {
            descriptors: &self.descriptors,
            kits: &self.kits,
        }
    }

    /// The same, with the store the executor writes, open to write, at `root`.
    pub fn with_store(mut self, store: Arc<Store>, root: &Path) -> Playground {
        self.store = Mutex::new(Some(store));
        self.root = Some(root.to_path_buf());
        self
    }

    /// The same, listing these kits, in this order, in place of the build's.
    pub fn with_kits(mut self, kits: Vec<Kit>) -> Playground {
        self.kits = kits;
        self
    }

    fn store(&self) -> Result<Arc<Store>, Refused> {
        self.store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .ok_or_else(|| Refused::new(503, "the playground is stopping"))
    }

    fn remembered(&self) -> std::sync::MutexGuard<'_, Remembered> {
        self.kept
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The allowance, folded from the store's books only when it has published since
    /// it was last folded.
    fn allowance(&self) -> Option<Json> {
        let store = self.store().ok()?;
        let newest = store.snapshot().ok()?.number();
        if let Some((number, said)) = &self.remembered().allowance
            && *number == newest
        {
            return said.clone();
        }
        let (number, said) = crate::play_kept::allowance(&store)?;
        self.remembered().allowance = Some((number, said.clone()));
        said
    }

    /// The verdict of a run that has ended, read once it is known: a verdict, or a record
    /// read whole that holds none. A record that could not be read (the store was busy)
    /// is not kept, and is read again at the next poll.
    fn verdict(&self, run: &RunId) -> Json {
        let key = run.as_str().to_owned();
        if let Some((_, said)) = self.remembered().verdicts.iter().find(|(k, _)| *k == key) {
            return said.clone();
        }
        let stated = self
            .root
            .as_deref()
            .and_then(|root| crate::door::verdict(root, run));
        let said = match stated {
            Some(stated) => json!(whizbang_serve::escaped(&stated.word)),
            None if self.has_record(run) => Json::Null,
            None => return Json::Null,
        };
        let mut kept = self.remembered();
        kept.verdicts.push_back((key, said.clone()));
        while kept.verdicts.len() > VERDICTS_KEPT {
            kept.verdicts.pop_front();
        }
        said
    }

    /// Whether the store holds the run's record, read whole.
    fn has_record(&self, run: &RunId) -> bool {
        let Ok(store) = self.store() else {
            return false;
        };
        let Ok(generation) = store.snapshot() else {
            return false;
        };
        whizbang_core::contract::record::stored(&generation, run.as_str(), |entry| {
            store.read(entry)
        })
        .is_some_and(|read| read.is_ok())
    }

    /// How many thumbnails were made, not taken from those kept: for a test.
    #[doc(hidden)]
    pub fn thumbnails_made(&self) -> u64 {
        self.made.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn descriptor(&self, action: &str) -> Option<&ActionDescriptor> {
        self.descriptors.iter().find(|d| d.name() == action)
    }
}

impl Play for Playground {
    fn call(
        &self,
        action: &str,
        arguments: &Map<String, Json>,
    ) -> Result<(Call, Option<String>), String> {
        let Some(descriptor) = self.descriptor(action) else {
            return Err("this build has no action of that name".to_owned());
        };
        if crate::play_form::is_use_case(descriptor.name()) {
            // A check that takes a run's job reads it from the store, by the run's id.
            let job = match arguments.get(crate::play_result::JOB_OF) {
                None | Some(Json::Null) => None,
                Some(Json::String(run)) => {
                    let store = self.store().map_err(|refused| refused.why)?;
                    Some(crate::play_result::job_of(&store, run, descriptor.name())?)
                }
                Some(_) => return Err("job_of names a run by its id".to_owned()),
            };
            return crate::play_form::use_case_call(descriptor.name(), arguments, job.as_ref())
                .map(|call| (call, None));
        }
        tools::call_of(descriptor, arguments)
    }

    fn hello(&self) -> Map<String, Json> {
        let mut said = Map::new();
        let (kits, tools) = crate::play_form::kits_json(&self.kits, &self.descriptors, &self.steps);
        said.insert("kits".to_owned(), kits);
        said.insert("tools".to_owned(), tools);
        // `backends` is always a list; a readiness that cannot be read is said beside it.
        match backend_status::readiness(&self.over) {
            Ok(listed) => {
                let sized: Vec<Json> = listed.into_iter().map(with_size).collect();
                said.insert("backends".to_owned(), Json::Array(sized));
            }
            Err(why) => {
                said.insert("backends".to_owned(), json!([]));
                said.insert(
                    "backends_error".to_owned(),
                    json!(crate::door::printable(&why)),
                );
            }
        }
        if let Some(allowance) = self.allowance() {
            said.insert("allowance".to_owned(), allowance);
        }
        said
    }

    fn choosers(&self, action: &str) -> Vec<(String, Vec<Json>)> {
        let Some(descriptor) = self.descriptor(action) else {
            return Vec::new();
        };
        let listed = backend_status::readiness(&self.over).unwrap_or_default();
        crate::play_form::choosers_of(descriptor)
            .into_iter()
            .map(|(field, capability)| {
                let rows = listed
                    .iter()
                    .filter(|row| row["capability"] == capability.as_str())
                    .cloned()
                    .map(with_size)
                    .collect();
                (field, rows)
            })
            .collect()
    }

    fn picture(&self, bytes: &[u8], name: &str) -> Result<Json, Refused> {
        crate::play_kept::picture(self.store()?.as_ref(), bytes, name)
    }

    fn pictures(&self, after: Option<&str>, limit: usize) -> Result<Json, Refused> {
        crate::play_kept::pictures(self.store()?.as_ref(), after, limit)
    }

    fn artifact(&self, id: &str, edge: Option<u32>) -> Result<(&'static str, Vec<u8>), Refused> {
        let Some(edge) = edge else {
            return crate::play_kept::artifact(self.store()?.as_ref(), id, None);
        };
        let key = (id.to_owned(), edge);
        {
            let mut kept = self.remembered();
            if let Some(at) = kept.thumbnails.iter().position(|(k, _)| *k == key)
                && let Some(found) = kept.thumbnails.remove(at)
            {
                let png = found.1.clone();
                kept.thumbnails.push_back(found);
                return Ok(("image/png", png));
            }
        }
        let made = crate::play_kept::artifact(self.store()?.as_ref(), id, Some(edge))?;
        self.made.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if made.0 == "image/png" {
            let mut kept = self.remembered();
            kept.thumbnails.push_back((key, made.1.clone()));
            let bytes =
                |kept: &Remembered| kept.thumbnails.iter().map(|(_, b)| b.len()).sum::<usize>();
            while kept.thumbnails.len() > THUMBNAILS_KEPT || bytes(&kept) > THUMBNAIL_BYTES_KEPT {
                kept.thumbnails.pop_front();
            }
        }
        Ok(made)
    }

    fn location(&self, drawn: &Json) -> Result<Json, Refused> {
        crate::play_kept::location(self.store()?.as_ref(), drawn)
    }

    fn facts(&self, run: &RunId, ended: bool) -> Map<String, Json> {
        let mut said = Map::new();
        let Ok(store) = self.store() else {
            return said;
        };
        let (spent, reserved) = crate::play_kept::spend(&store, run);
        said.insert("spent".to_owned(), json!(spent));
        said.insert("reserved".to_owned(), json!(reserved));
        let verdict = if ended { self.verdict(run) } else { Json::Null };
        said.insert("verdict".to_owned(), verdict);
        said
    }

    fn journal(&self, run: &RunId) -> Option<(Vec<Json>, u64)> {
        crate::play_kept::journal(self.store().ok()?.as_ref(), run)
    }

    fn record(&self, run: &RunId) -> Result<Json, Refused> {
        let store = self.store()?;
        let root = self
            .root
            .as_deref()
            .ok_or_else(|| Refused::new(503, "the playground is stopping"))?;
        let rows = backend_status::readiness(&self.over).unwrap_or_default();
        let resolve =
            |capability: &str, kind: &str, model: &str| resolved(&rows, capability, kind, model);
        crate::play_result::record_json(&store, root, run, &self.tasks(), &resolve)
    }

    fn command(&self, run: &RunId) -> Result<Json, Refused> {
        let Some(commands) = &self.commands else {
            return Err(Refused::new(
                501,
                "this build says no command line of a run",
            ));
        };
        let store = self.store()?;
        let root = self
            .root
            .clone()
            .ok_or_else(|| Refused::new(503, "the playground is stopping"))?;
        let top = crate::play_result::record_of(&store, run)?;
        let recorded = top.asked().config;
        let records = crate::play_result::tree(&store, top);
        let action = records[0].asked().action.clone().unwrap_or_default();
        let descriptor = self.descriptor(&action);
        let default_of = |capability: &str| self.default_backend(capability);
        let rows = backend_status::readiness(&self.over).unwrap_or_default();
        let resolve =
            |capability: &str, kind: &str, model: &str| resolved(&rows, capability, kind, model);
        let mut commanded =
            crate::play_result::commanded(&records, descriptor, &resolve, &default_of)?;
        let (config, env_file, models_dir) = self.over.files_named();
        commanded.store = root;
        commanded.config = config.map(Path::to_path_buf);
        commanded.env_file = env_file.map(Path::to_path_buf);
        commanded.models_dir = models_dir.map(Path::to_path_buf);
        let shipped = whizbang_rascal::recipes::TITLES
            .iter()
            .any(|(name, _)| *name == action);
        if descriptor.is_some_and(|d| d.kind() == whizbang_core::contract::Kind::Recipe)
            && !shipped
            && !crate::play_form::is_use_case(&action)
        {
            commanded.recipe_file = self.over.recipe_file_named(&action);
        }
        let words = commands(&commanded)
            .map_err(|why| Refused::new(422, &whizbang_serve::escaped(&why)))?;
        // A new line or a tab stays as it is inside a word's quotes, which a shell keeps;
        // any other control character is said by no command line a person can paste
        // safely into a terminal.
        let unsafe_word = |word: &String| {
            word.chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        };
        if words.iter().any(unsafe_word) {
            return Err(Refused::new(
                422,
                "the run's words hold a control character that no command line says safely",
            ));
        }
        let now = self.over.config_digest();
        let changed = match (recorded, now) {
            (Some(recorded), Some(now)) => recorded != now,
            _ => false,
        };
        let mut text = String::new();
        if changed && let (Some(recorded), Some(now)) = (recorded, now) {
            text.push_str(&format!(
                "# the config has changed since this run: it ran under {recorded}, and this \
                 command runs under the config as it is now, {now}\n"
            ));
        }
        text.push_str(&crate::play_result::command_line(&words));
        Ok(json!({
            "command": text,
            "words": words,
            "config_changed": changed,
            "recorded_config": recorded.map(|digest| digest.to_string()),
            "config": now.map(|digest| digest.to_string()),
        }))
    }

    fn default_backend(&self, capability: &str) -> Option<String> {
        let listed = backend_status::readiness(&self.over).ok()?;
        listed
            .iter()
            .find(|row| row["capability"] == capability && row["default"] == true)
            .and_then(|row| row["name"].as_str().map(str::to_owned))
    }

    fn admitted(&self, id: &RequestId) -> Option<RunId> {
        let store = self.store().ok()?;
        let generation = store.snapshot().ok()?;
        let entry = generation.get(&id.admission_name())?.clone();
        let bytes = store.read(&entry).ok()?;
        let admission = whizbang_core::contract::Admission::from_bytes(&bytes).ok()?;
        (admission.request() == Some(id)).then(|| admission.run().clone())
    }

    fn closing(&self) {
        // The store's writer goes with the executor's: the host has let that go.
        self.store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        *self.remembered() = Remembered::default();
        if let Some(worker) = &self.worker {
            worker.kill();
        }
    }
}

/// The config's name of the backend of this capability whose kind and model are these, as
/// a run's record names a request's backend: none when the config names none, or more
/// than one, so.
fn resolved(rows: &[Json], capability: &str, kind: &str, model: &str) -> Option<String> {
    let mut named = rows.iter().filter(|row| {
        row["capability"] == capability && row["kind"] == kind && row["model"] == model
    });
    let one = named.next()?;
    if named.next().is_some() {
        return None;
    }
    one["name"].as_str().map(str::to_owned)
}

/// A backend's row with the size of the catalogue's model it would fetch, for a local
/// model that is not installed: what Get would download (plan m5, task W1, item 4).
fn with_size(mut row: Json) -> Json {
    let size = row["model"].as_str().and_then(|model| {
        let catalogue = wiring::catalogue().ok()?;
        catalogue.entry(model).map(|entry| entry.bytes())
    });
    row["size"] = json!(size);
    row
}

/// What a running host keeps in its store's folder: its port, its process and its
/// token, readable by its owner only (ADR 0041, "Reconnecting").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hosting {
    pub port: u16,
    pub process: u32,
    pub token: String,
}

/// The file of the store at `root`.
pub fn hosting_file(root: &Path) -> PathBuf {
    root.join(PLAY_FILE)
}

/// Writes the file of a running host, for its owner alone, in place of one a host that
/// ended without removing it left.
pub fn write_hosting(root: &Path, hosting: &Hosting) -> io::Result<()> {
    let path = hosting_file(root);
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path)?;
    let text = serde_json::to_vec(hosting).map_err(io::Error::other)?;
    file.write_all(&text)?;
    file.sync_all()
}

/// The file of a host of the store at `root`, when one is there, reads as one, and can
/// be trusted: the store's folder is this user's and nobody else may write it, and the
/// file is a regular file of this user's that nobody else may read or write. Anything
/// else is not read, and a launch goes on as if no host were there.
pub fn read_hosting(root: &Path) -> Option<Hosting> {
    let writers = RealFs.who_may_write(root).ok()?;
    if !writers.owner_is_this_user || writers.group || writers.anyone {
        return None;
    }
    let path = hosting_file(root);
    let found = std::fs::symlink_metadata(&path).ok()?;
    if !found.is_file() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let user = whizbang_core::store::fs::this_user().ok()?;
        if found.uid() != user || found.mode() & 0o077 != 0 {
            return None;
        }
    }
    let mut text = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(4096)
        .read_to_end(&mut text)
        .ok()?;
    serde_json::from_slice(&text).ok()
}

/// The name of the page that sends Chrome on to the address with its token.
pub const OPENER_FILE: &str = "play-open.html";

/// Writes, for its owner alone, the page in the store's folder that sends a browser on
/// to `address`, the address with its token: what Chrome is given in place of the
/// address, so that the token is never on a command line, which other users of the
/// machine can read. The path of the page.
pub fn write_opener(root: &Path, address: &str) -> io::Result<PathBuf> {
    let path = root.join(OPENER_FILE);
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path)?;
    let page = format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\">\n\
         <meta http-equiv=\"refresh\" content=\"0;url={address}\">\n\
         <title>whizbang playground</title></head>\n\
         <body><p><a href=\"{address}\">Open the playground</a></p></body></html>\n"
    );
    file.write_all(page.as_bytes())?;
    file.sync_all()?;
    Ok(path)
}

/// Removes the opening page of the store at `root`; one that is not there is nothing
/// to remove.
pub fn remove_opener(root: &Path) {
    let _ = std::fs::remove_file(root.join(OPENER_FILE));
}

/// The command that opens the page `opener` in Google Chrome: `open -a "Google Chrome"`
/// on the Mac, `google-chrome` elsewhere, in a process group of its own so that a
/// signal meant for whizbang does not reach the browser. Its arguments name the page's
/// path and nothing else of it.
pub fn chrome_command(opener: &Path, mac: bool) -> std::process::Command {
    use std::process::{Command, Stdio};
    let mut command = if mac {
        let mut command = Command::new("open");
        command.args(["-a", "Google Chrome"]).arg(opener);
        command
    } else {
        let mut command = Command::new("google-chrome");
        command.arg(opener);
        command
    };
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

/// Removes the file of the host of the store at `root`; one that is not there is
/// nothing to remove.
pub fn remove_hosting(root: &Path) {
    let _ = std::fs::remove_file(hosting_file(root));
}

/// Whether the host this file names answers, at its port on loopback, to its token:
/// the one `/api/hello` it asks, within a few seconds. A host that answers so is the
/// host of the store whose file named it, since only that host has the token.
pub fn answers(hosting: &Hosting) -> bool {
    let Some(token) = Token::parse(&hosting.token) else {
        return false;
    };
    let address = SocketAddr::from(([127, 0, 0, 1], hosting.port));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_secs(2)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let request = format!(
        "GET /api/hello HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\n\r\n",
        hosting.port,
        token.hex()
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut answer = Vec::new();
    let _ = stream.take(64 * 1024).read_to_end(&mut answer);
    answer.starts_with(b"HTTP/1.1 200 ")
}

/// The address a person opens: the host's page, with the token after `#k=` when it is
/// to be handed over, which the page takes out of the address bar at once.
pub fn address(port: u16, token: Option<&Token>) -> String {
    match token {
        Some(token) => format!("http://127.0.0.1:{port}/#k={}", token.hex()),
        None => format!("http://127.0.0.1:{port}/"),
    }
}
