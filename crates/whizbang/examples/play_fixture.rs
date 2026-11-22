//! The fixture host of the playground's Chrome cases (plan m5, task U1, item 5; task W1):
//! the playground's host as it ships (`whizbang_serve::play::Host`, task H1), over the
//! binary's registry and wiring and the door the binary hands it
//! (`whizbang::playing::Playground`), started by Playwright in a new store. It is an
//! example target, so the binary never holds it, nor any fake.
//!
//! It prints one line first, the page's address with the launch token, as `whizbang play
//! --no-open --stdin-quit --port 0` does, so the test that started it can open the page:
//!
//! ```text
//! http://127.0.0.1:<port>/#k=<64 hex>
//! ```
//!
//! **Its backends** are the test kit's `playkit` (`whizbang_testkit::playkit`), under a
//! config it writes beside the store: a segmenter that costs nothing, `local-seg`; the
//! catalogue's `sam3` through the kind `pack`, not installed in an empty models directory
//! of its own; a filler, `cloud-fill`, and a judge,
//! `cloud-judge`, each at an address of the reserved `.example` domain, which nothing
//! contacts (the filler's key a made-up one, in an env file the fixture writes beside the
//! store); and a judge whose key is named and not set, `keyless-judge`. The store's
//! allowance is set to ten dollars. Nothing a case does reaches a model or leaves the
//! machine.
//!
//! **The script.** A request that carries `x-fixture-script: <words>` sets the kit's
//! script before it is answered (`PlayKit::set`: `hold`, `hold-pictures`, `release`,
//! `pass`, `fail`, `reset`); the real host refuses that header.
//!
//! **Its models** (plan m5, task M1): the shipped catalogue and, beside it, the test kit's
//! `pocket` (`whizbang_testkit::playmodels`), whose three small files a fake download
//! serves from memory, at an address of the `.example` domain. The script holds an
//! install after its first file (`hold-install`, `release-install`), and holds the
//! installed `pocket` as a run does (`use-model`, `let-model-go`), so a removal waits and
//! answers busy. A removal waits one second.
//!
//! Options: `--store <folder>` (made if missing, its last folder alone), `--port <n>` (0,
//! the default, takes a free port), `--stdin-quit` (quit when the standard input ends, so
//! a test that dies leaves no host behind), `--third-kit` (a kit of fake actions,
//! `tinker`, beside the build's: `SPEC.md` section 1's rule that a kit needs no change
//! to a door).

/// The binary's own command line, parse and dispatch (`main.rs` and its commands), for the
/// mode `cli` (plan m5, task W2, item 5). Its `main` is not called here.
#[path = "../src/main.rs"]
#[allow(dead_code)]
mod binary;
// The commands name one another from the crate's root, as they do in the binary.
#[allow(unused_imports)]
use binary::{COMMIT, eval, export, rascal, runs, spend, stats, store};

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Map, Value as Json};
use whizbang::model_commands::{Downloads, Tools};
use whizbang::pack::OneWorker;
use whizbang::play_models::PlayModels;
use whizbang::playing::{Playground, address};
use whizbang::wiring::{self, Make, Over};
use whizbang_backends::models::{
    Catalogue, CatalogueFile, Current, Entry, EntryParts, FileName, Hold, Holds, Licence,
    ModelName, ModelsDir, Platform as ModelPlatform, Revision, Title, current,
};
use whizbang_core::artifact::ArtifactType;
use whizbang_core::contract::{
    Action, ActionDescriptor, Call, Context, Failure, InputPort, Kind, PreparedCall, Produced,
    RequestId, RunId,
};
use whizbang_core::contract::{Secrets, SystemClock};
use whizbang_core::description::Kit;
use whizbang_core::egress::{Address, CheckedDownloader, Origin};
use whizbang_core::identity::Digest;
use whizbang_core::spend::Amount;
use whizbang_core::store::fs::RealFs;
use whizbang_serve::play::{Host, Play, Refused, Settings, Token, threads};
use whizbang_testkit::downloads::FakeDownloader;
use whizbang_testkit::playkit::{KIND, PlayKit};
use whizbang_testkit::playmodels::{self, GatedDownloader, MODEL};

fn fail(message: &str) -> ! {
    eprintln!("play_fixture: {message}");
    std::process::exit(2)
}

/// The config of the fixture's backends.
fn config_text() -> String {
    format!(
        "[backends.segment]\ndefault = \"local-seg\"\n\n\
         [backends.segment.local-seg]\nkind = \"{KIND}\"\nmodel = \"local-seg\"\n\
         url = \"https://segment.example.org/v1\"\n\n\
         [backends.segment.sam3]\nkind = \"pack\"\nmodel = \"sam3\"\n\n\
         [backends.fill]\ndefault = \"cloud-fill\"\n\n\
         [backends.fill.cloud-fill]\nkind = \"{KIND}\"\nmodel = \"fill\"\n\
         url = \"https://fill.example.org/v1\"\nkey_env = \"PLAYKIT_FILL_KEY\"\n\n\
         [backends.judge]\ndefault = \"cloud-judge\"\n\n\
         [backends.judge.cloud-judge]\nkind = \"{KIND}\"\nmodel = \"judge\"\n\
         url = \"https://judge.example.org/v1\"\n\n\
         [backends.judge.keyless-judge]\nkind = \"{KIND}\"\nmodel = \"keyless\"\n\
         url = \"https://keyless.example.org/v1\"\nkey_env = \"PLAYKIT_KEYLESS_KEY\"\n\n\
         [egress.recipients.segment]\norigin = \"https://segment.example.org\"\n\
         classes = [\"prompts\", \"source_images\"]\n\n\
         [egress.recipients.fill]\norigin = \"https://fill.example.org\"\n\
         classes = [\"source_images\", \"derived_images\"]\n\n\
         [egress.recipients.judge]\norigin = \"https://judge.example.org\"\n\
         classes = [\"prompts\", \"source_images\", \"derived_images\", \"evidence\"]\n\n\
         [egress.recipients.keyless]\norigin = \"https://keyless.example.org\"\n\
         classes = [\"prompts\", \"source_images\", \"derived_images\", \"evidence\"]\n"
    )
}

/// The third kit's one task: it takes a picture and hands back nothing.
struct Echo(ActionDescriptor);

impl Action for Echo {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, _: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        context.events().progress("echoed");
        Ok(Produced::nothing())
    }
}

fn echo() -> Arc<dyn Action> {
    let descriptor = ActionDescriptor::draft(
        "echo_picture",
        "1.0.0",
        Kind::Recipe,
        "Looks at a picture and changes nothing: a task of a kit of fakes.",
    )
    .kit("tinker")
    .title("Echo a picture")
    .input(InputPort::required(
        "image",
        ArtifactType::Image,
        "The picture to look at.",
    ))
    .build()
    .unwrap_or_else(|error| fail(&error.to_string()));
    Arc::new(Echo(descriptor))
}

/// The binary's door, with the kit's script and the models' fakes.
struct Fixture {
    door: Playground,
    kit: Arc<PlayKit>,
    downloads: Arc<GatedDownloader>,
    models: PathBuf,
    held: std::sync::Mutex<Option<Hold>>,
}

impl Fixture {
    /// A hold on the installed `pocket`, as a run that uses it takes one.
    fn use_model(&self) -> Result<(), String> {
        let none = Secrets::none();
        let dir = ModelsDir::open(Arc::new(RealFs), &self.models, Arc::new(SystemClock))
            .map_err(|refused| refused.to_string())?;
        let name = ModelName::new(MODEL, &none).map_err(|refused| refused.to_string())?;
        let Ok(Current::Installed(record)) = current(&dir, &name, &none) else {
            return Err("pocket is not installed".to_owned());
        };
        let hold = Holds::over(dir)
            .hold(&name, record.identity(), &none)
            .map_err(|refused| refused.to_string())?;
        *self
            .held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(hold);
        Ok(())
    }
}

/// The catalogue the fixture's models are of: the shipped one, and `pocket`.
fn catalogue() -> Catalogue {
    let none = Secrets::none();
    let shipped = wiring::catalogue().unwrap_or_else(|why| fail(&why));
    let files = playmodels::files()
        .into_iter()
        .map(|(name, bytes)| {
            let at = Address::of(&playmodels::address_of(name), &none)
                .unwrap_or_else(|why| fail(&why.to_string()));
            CatalogueFile::new(
                FileName::new(name, &none).unwrap_or_else(|why| fail(&why.to_string())),
                bytes.len() as u64,
                Digest::of_bytes(&bytes),
                at,
                &none,
            )
            .unwrap_or_else(|why| fail(&why.to_string()))
        })
        .collect();
    let named =
        |text: &str| ModelName::new(text, &none).unwrap_or_else(|why| fail(&why.to_string()));
    let parts = EntryParts {
        name: named(MODEL),
        title: Title::new("Pocket (a fixture's model)", &none)
            .unwrap_or_else(|why| fail(&why.to_string())),
        revision: Revision::new("2026-11.1", &none).unwrap_or_else(|why| fail(&why.to_string())),
        adapter: named("owlv2-sam2"),
        licence: Licence::new("Apache-2.0", &none).unwrap_or_else(|why| fail(&why.to_string())),
        platforms: ModelPlatform::ALL.to_vec(),
        features: Vec::new(),
        origins: vec![
            Origin::of(playmodels::ORIGIN, &none).unwrap_or_else(|why| fail(&why.to_string())),
        ],
        files,
    };
    let pocket = Entry::new(parts, &none).unwrap_or_else(|why| fail(&why.to_string()));
    let mut entries = shipped.entries().to_vec();
    entries.push(pocket);
    Catalogue::new(entries, &none).unwrap_or_else(|why| fail(&why.to_string()))
}

/// The fake download of `pocket`'s files, held when the script says so.
fn served() -> Arc<GatedDownloader> {
    let none = Secrets::none();
    let mut fake = FakeDownloader::new();
    for (name, bytes) in playmodels::files() {
        let at = Address::of(&playmodels::address_of(name), &none)
            .unwrap_or_else(|why| fail(&why.to_string()));
        fake = fake.serving(&at, bytes);
    }
    Arc::new(GatedDownloader::new(fake))
}

impl Play for Fixture {
    fn call(
        &self,
        action: &str,
        arguments: &Map<String, Json>,
    ) -> Result<(Call, Option<String>), String> {
        self.door.call(action, arguments)
    }
    fn hello(&self) -> Map<String, Json> {
        self.door.hello()
    }
    fn choosers(&self, action: &str) -> Vec<(String, Vec<Json>)> {
        self.door.choosers(action)
    }
    fn picture(&self, bytes: &[u8], name: &str) -> Result<Json, Refused> {
        self.door.picture(bytes, name)
    }
    fn pictures(&self, after: Option<&str>, limit: usize) -> Result<Json, Refused> {
        self.door.pictures(after, limit)
    }
    fn artifact(&self, id: &str, edge: Option<u32>) -> Result<(&'static str, Vec<u8>), Refused> {
        // Held while the script says `hold-pictures`: the page's connections stay open.
        self.kit.picture_asked();
        self.door.artifact(id, edge)
    }
    fn location(&self, drawn: &Json) -> Result<Json, Refused> {
        self.door.location(drawn)
    }
    fn facts(&self, run: &RunId, ended: bool) -> Map<String, Json> {
        self.door.facts(run, ended)
    }
    fn journal(&self, run: &RunId) -> Option<(Vec<Json>, u64)> {
        self.door.journal(run)
    }
    fn record(&self, run: &RunId) -> Result<Json, Refused> {
        self.door.record(run)
    }
    fn command(&self, run: &RunId) -> Result<Json, Refused> {
        self.door.command(run)
    }
    fn default_backend(&self, capability: &str) -> Option<String> {
        self.door.default_backend(capability)
    }
    fn admitted(&self, id: &RequestId) -> Option<RunId> {
        self.door.admitted(id)
    }
    fn script(&self, words: &str) -> Result<(), String> {
        match words {
            "hold-install" => {
                self.downloads.hold();
                Ok(())
            }
            "release-install" => {
                self.downloads.release();
                Ok(())
            }
            "use-model" => self.use_model(),
            "let-model-go" => {
                self.held
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take();
                Ok(())
            }
            _ => self.kit.set(words),
        }
    }
    fn closing(&self) {
        self.downloads.release();
        self.door.closing();
    }
    fn models(&self) -> Option<&dyn whizbang_serve::play::models::Models> {
        self.door.models()
    }
}

/// The made-up key of `cloud-fill`, which the fixture's env file sets: a Chrome case looks
/// for it in everything the page is answered and shows (plan m5, task M1). Assembled so
/// that no scan of this file finds one.
fn made_up_key() -> String {
    ["made", "up", "fixture", "key", "0123456789"].join("-")
}

/// The env file's place: beside the store, named after it.
fn env_path(store: &Path) -> PathBuf {
    let mut name = store.file_name().unwrap_or_default().to_os_string();
    name.push(".fixture.env");
    store.with_file_name(name)
}

/// The config's file: beside the store, named after it.
fn config_path(store: &Path) -> PathBuf {
    let mut name = store.file_name().unwrap_or_default().to_os_string();
    name.push(".fixture.toml");
    store.with_file_name(name)
}

/// The mode `cli`: the binary's own parse and dispatch of the words after `cli`, as
/// `whizbang` would run them, over the fixture's fake backends (plan m5, task W2, item 5).
/// The config and the models directory are the host's, written again beside the store
/// the words name, so a command the host copied runs under the same config.
fn cli(words: Vec<String>) -> std::process::ExitCode {
    let store = words
        .iter()
        .position(|word| word == "--store")
        .and_then(|at| words.get(at + 1))
        .map(PathBuf::from);
    let Some(store) = store else {
        fail("cli: the command names no --store");
    };
    let config = config_path(&store);
    std::fs::write(&config, config_text()).unwrap_or_else(|error| fail(&error.to_string()));
    let models = config.with_extension("models");
    let _ = std::fs::create_dir(&models);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&models, std::fs::Permissions::from_mode(0o700));
    }
    let kit = PlayKit::new();
    let _ = rascal::MORE_KINDS.set(vec![kit.kind()]);
    let code = binary::run(std::iter::once("whizbang".to_owned()).chain(words));
    let _ = std::fs::remove_file(&config);
    let _ = std::fs::remove_dir_all(&models);
    code
}

fn main() -> std::process::ExitCode {
    let mut given = std::env::args().skip(1).peekable();
    if given.peek().map(String::as_str) == Some("cli") {
        return cli(given.skip(1).collect());
    }
    host();
    std::process::ExitCode::SUCCESS
}

fn host() {
    let mut store = std::env::temp_dir().join("whizbang-play-fixture");
    let mut port: u16 = 0;
    let mut stdin_quit = false;
    let mut third_kit = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--store" => store = args.next().map(PathBuf::from).unwrap_or(store),
            "--port" => port = args.next().and_then(|p| p.parse().ok()).unwrap_or(0),
            "--stdin-quit" => stdin_quit = true,
            "--third-kit" => third_kit = true,
            other => fail(&format!("unknown argument {other}")),
        }
    }
    let config = config_path(&store);
    let env = env_path(&store);
    std::fs::write(&env, format!("PLAYKIT_FILL_KEY={}\n", made_up_key()))
        .unwrap_or_else(|error| fail(&error.to_string()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&env, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::write(&config, config_text()).unwrap_or_else(|error| fail(&error.to_string()));
    let kit = PlayKit::new();
    let mut kinds = wiring::kinds(&OneWorker::installed());
    kinds
        .register(kit.kind())
        .unwrap_or_else(|error| fail(&error.to_string()));
    // A models directory of its own, empty: the catalogue's `sam3` is not installed.
    let models = config.with_extension("models");
    let _ = std::fs::create_dir(&models);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&models, std::fs::Permissions::from_mode(0o700));
    }
    // The catalogue's models as on the Mac, whose platform `sam3` names, on every runner:
    // so it is shown not installed, with its size, and never not for this platform.
    // Nothing is installed or run.
    let adapters = wiring::adapters()
        .into_iter()
        .map(|adapter| (adapter.name, adapter.revision))
        .collect();
    let shipped = wiring::Models::of(catalogue(), adapters)
        .runs_on(Some(whizbang_backends::models::Platform::MacosArm64));
    let over = Over::nothing()
        .config(Some(&config))
        .env_file(Some(&env))
        .kinds(kinds)
        .installed(shipped.clone().flag(Some(&models)))
        .unwrap_or_else(|why| fail(&why));
    let opening = wiring::opening(&store, Make::TheRoot, wiring::recovery())
        .unwrap_or_else(|error| fail(&wiring::not_opened(&error)));
    let writer = opening
        .writer()
        .unwrap_or_else(|why| fail(&why.to_string()));
    // Ten dollars to spend, in a new store; a store opened again keeps its books.
    if let Some(ten) = Amount::new(10_000_000) {
        let _ = whizbang_core::spend::reset(&writer, ten, 1);
    }
    let mut registry = wiring::registry_over(&over, false).unwrap_or_else(|why| fail(&why));
    let mut kits = wiring::kits();
    if third_kit {
        registry
            .register(echo())
            .unwrap_or_else(|error| fail(&error.to_string()));
        kits.push(Kit::new("tinker", "Tinker", "try things").unwrap_or_else(|why| fail(&why)));
    }
    let descriptors = registry.descriptors().cloned().collect();
    let executor = wiring::runner(registry, writer.clone(), &over).unwrap_or_else(|why| fail(&why));
    let downloads = served();
    let serving = downloads.clone();
    let made: Downloads = Arc::new(move |_least: u64, origins: &[Origin], _: &Secrets| {
        CheckedDownloader::new(serving.clone(), origins)
    });
    let tools = Tools::of(shipped.flag(Some(&models)), Arc::new(SystemClock), made);
    let models_door = PlayModels::new(tools, Some(&config), Some(&env))
        .waiting(std::time::Duration::from_secs(1));
    let door = Playground::new(descriptors, Arc::new(over))
        .with_store(writer, &store)
        .with_kits(kits)
        .with_models(models_door)
        .with_commands(Arc::new(binary::command_of::words));
    let play = Arc::new(Fixture {
        door,
        kit,
        downloads,
        models: models.clone(),
        held: std::sync::Mutex::new(None),
    });
    let token = Token::new().unwrap_or_else(|error| fail(&error.to_string()));
    let mut settings = Settings::new(store);
    settings.port = port;
    let host = Host::start(Arc::new(executor), play, settings, token.clone())
        .unwrap_or_else(|error| fail(&format!("cannot listen: {error}")));
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", address(host.address().port(), Some(&token)));
    let _ = out.flush();
    if stdin_quit {
        let stopper = host.stopper();
        threads::spawn(
            "input",
            Box::new(move || {
                let _ = std::io::copy(&mut std::io::stdin().lock(), &mut std::io::sink());
                stopper.quit();
            }),
        )
        .unwrap_or_else(|error| fail(&error.to_string()));
    }
    host.wait();
    let _ = std::fs::remove_file(&config);
    let _ = std::fs::remove_file(&env);
    let _ = std::fs::remove_dir_all(&models);
}
