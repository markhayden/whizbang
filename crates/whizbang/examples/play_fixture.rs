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
//! contacts; and a judge whose key is named and not set, `keyless-judge`. The store's
//! allowance is set to ten dollars. Nothing a case does reaches a model or leaves the
//! machine.
//!
//! **The script.** A request that carries `x-fixture-script: <words>` sets the kit's
//! script before it is answered (`PlayKit::set`: `hold`, `release`, `pass`, `fail`,
//! `reset`); the real host refuses that header.
//!
//! Options: `--store <folder>` (made if missing, its last folder alone), `--port <n>` (0,
//! the default, takes a free port), `--stdin-quit` (quit when the standard input ends, so
//! a test that dies leaves no host behind), `--third-kit` (a kit of fake actions,
//! `tinker`, beside the build's: `SPEC.md` section 1's rule that a kit needs no change
//! to a door).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Map, Value as Json};
use whizbang::pack::OneWorker;
use whizbang::playing::{Playground, address};
use whizbang::wiring::{self, Make, Over};
use whizbang_core::artifact::ArtifactType;
use whizbang_core::contract::{
    Action, ActionDescriptor, Call, Context, Failure, InputPort, Kind, PreparedCall, Produced,
    RequestId, RunId,
};
use whizbang_core::description::Kit;
use whizbang_core::spend::Amount;
use whizbang_serve::play::{Host, Play, Refused, Settings, Token, threads};
use whizbang_testkit::playkit::{KIND, PlayKit};

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
         url = \"https://fill.example.org/v1\"\n\n\
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

/// The binary's door, with the kit's script.
struct Fixture {
    door: Playground,
    kit: Arc<PlayKit>,
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
    fn default_backend(&self, capability: &str) -> Option<String> {
        self.door.default_backend(capability)
    }
    fn admitted(&self, id: &RequestId) -> bool {
        self.door.admitted(id)
    }
    fn script(&self, words: &str) -> Result<(), String> {
        self.kit.set(words)
    }
    fn closing(&self) {
        self.door.closing();
    }
}

/// The config's file: beside the store, named after it.
fn config_path(store: &Path) -> PathBuf {
    let mut name = store.file_name().unwrap_or_default().to_os_string();
    name.push(".fixture.toml");
    store.with_file_name(name)
}

fn main() {
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
    let shipped = wiring::Models::shipped()
        .unwrap_or_else(|why| fail(&why))
        .runs_on(Some(whizbang_backends::models::Platform::MacosArm64));
    let over = Over::nothing()
        .config(Some(&config))
        .kinds(kinds)
        .installed(shipped.flag(Some(&models)))
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
    let door = Playground::new(descriptors, Arc::new(over))
        .with_store(writer, &store)
        .with_kits(kits);
    let play = Arc::new(Fixture { door, kit });
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
    let _ = std::fs::remove_dir_all(&models);
}
