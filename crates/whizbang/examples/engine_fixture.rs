//! The engine fixture (plan m6, task H1, item 8): the engine API as it ships
//! (`whizbang_serve::engine::Host`, with the binary's wiring and the door the binary hands
//! it, `whizbang::engining::Served`), over the test kit's fakes, in one process. It is an
//! example target, so the binary never holds it, nor any fake. The tests of what the
//! engine does (`crates/whizbang/tests/it/serve_http.rs`) start it in their own process
//! through [`start`]; run as a program, it serves until its input ends.
//!
//! **Its backends** are the test kit's `playkit` (`whizbang_testkit::playkit`), under a
//! config it writes beside the store: a segmenter that costs nothing, `local-seg`, and a
//! filler, `cloud-fill`, at an address of the reserved `.example` domain, which nothing
//! contacts, its key a made-up one in an env file written beside the store. The store's
//! allowance is set to ten dollars. Nothing a test does reaches a model or leaves the
//! machine. **User story 1** (plan m6, task D1, item 4) starts it with backends of the
//! test's own instead ([`start_with`]): a config, an env file, the kinds that serve it (the
//! kind `remote` at the fake host, and fal's filler at a server on loopback) and the root
//! their certificates are of.
//!
//! It prints one line first, the address it listens on, `http://127.0.0.1:<port>`; its
//! token is [`TOKEN`]. Options: `--store <folder>` (made if missing, its last folder
//! alone), `--port <n>` (0, the default, takes a free port).

use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use whizbang::engining::Served;
use whizbang::pack::OneWorker;
use whizbang::serving::Actions;
use whizbang::wiring::{self, Over};
use whizbang_backends::net::testing::Pki;
use whizbang_core::config::Kinds;
use whizbang_core::contract::Opening;
use whizbang_core::spend::Amount;
use whizbang_core::store::fs::{Fs, RealFs};
use whizbang_serve::engine::{Host, Secret, Settings};
use whizbang_testkit::playkit::{KIND, PlayKit};

/// The fixture's token: made up, and long enough.
pub const TOKEN: &str = "made-up-fixture-engine-token-0123456789";

/// The variable the filler's made-up key is under.
pub const FILL_KEY: &str = "PLAYKIT_FILL_KEY";

/// The config of the fixture's backends; with a models directory, a local segmenter of
/// the kind `pack` too, `sam3`, not installed there, so that the directory is opened.
pub fn config_text(models: Option<&Path>) -> String {
    let local = match models {
        None => String::new(),
        Some(dir) => format!(
            "[models]\ndir = {:?}\n\n[backends.segment.sam3]\nkind = \"pack\"\nmodel = \"sam3\"\n\n",
            dir.to_string_lossy()
        ),
    };
    local
        + &format!(
            "[backends.segment]\ndefault = \"local-seg\"\n\n\
         [backends.segment.local-seg]\nkind = \"{KIND}\"\nmodel = \"local-seg\"\n\
         url = \"https://segment.example.org/v1\"\n\n\
         [backends.fill]\ndefault = \"cloud-fill\"\n\n\
         [backends.fill.cloud-fill]\nkind = \"{KIND}\"\nmodel = \"fill\"\n\
         url = \"https://fill.example.org/v1\"\nkey_env = \"{FILL_KEY}\"\n\n\
         [egress.recipients.segment]\norigin = \"https://segment.example.org\"\n\
         classes = [\"prompts\", \"source_images\"]\n\n\
         [egress.recipients.fill]\norigin = \"https://fill.example.org\"\n\
         classes = [\"source_images\", \"derived_images\"]\n"
        )
}

/// The made-up key of `cloud-fill`, assembled so that no scan of this file finds one.
pub fn made_up_key() -> String {
    ["made", "up", "engine", "fixture", "key", "0123456789"].join("-")
}

/// A file beside the store, named after it.
fn beside(store: &Path, suffix: &str) -> PathBuf {
    let mut name = store.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    store.with_file_name(name)
}

/// The config's file, beside the store.
pub fn config_path(store: &Path) -> PathBuf {
    beside(store, ".engine.toml")
}

/// The env file, beside the store.
pub fn env_path(store: &Path) -> PathBuf {
    beside(store, ".engine.env")
}

/// A running fixture: the host, the kit whose script steers its backends, and the one
/// worker of the pack its wiring was handed, which no request has started while its
/// `status` is none.
pub struct Fixture {
    pub host: Host,
    pub kit: Arc<PlayKit>,
    pub worker: Arc<OneWorker>,
}

/// Backends of a test's own, in place of the kit's: the config's text, the env file's,
/// the kinds that serve them, and the root their servers' certificates are of.
pub struct Backends {
    pub config: String,
    pub env: String,
    pub kinds: Kinds,
    pub trusting: Arc<Pki>,
}

/// Writes the config, with a models directory when one is given, and the env file with
/// the filler's key when `keyed`, beside the store.
pub fn write_files(store: &Path, keyed: bool, models: Option<&Path>) -> Result<(), String> {
    let config = config_path(store);
    let env = env_path(store);
    std::fs::write(&config, config_text(models)).map_err(|error| error.to_string())?;
    let key = match keyed {
        true => format!("{FILL_KEY}={}\n", made_up_key()),
        false => String::new(),
    };
    std::fs::write(&env, key).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&env, std::fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Starts the engine over the store at `store`, under the config and env file beside it
/// (written by [`write_files`]), with these settings.
pub fn start(store: &Path, settings: Settings) -> Result<Fixture, String> {
    start_on(store, Arc::new(RealFs), settings)
}

/// The same, with the store on this file system: the store is opened, and refused, before
/// anything listens, as `whizbang serve --http` opens it (`crates/whizbang/src/serve.rs`).
pub fn start_on(store: &Path, fs: Arc<dyn Fs>, settings: Settings) -> Result<Fixture, String> {
    let kit = PlayKit::new();
    let worker = OneWorker::installed();
    let mut kinds = wiring::kinds(&worker);
    kinds
        .register(kit.kind())
        .map_err(|error| error.to_string())?;
    let over = Over::nothing()
        .config(Some(&config_path(store)))
        .env_file(Some(&env_path(store)))
        .kinds(kinds);
    serve(store, fs, settings, over, kit, worker)
}

/// Starts the engine over the store at `store` with these backends, their config and env
/// file written beside it: as `whizbang serve --http` serves, with the binary's one worker
/// handed to the wiring, and nothing of the kit's.
pub fn start_with(store: &Path, settings: Settings, backends: Backends) -> Result<Fixture, String> {
    let (config, env) = (config_path(store), env_path(store));
    std::fs::write(&config, &backends.config).map_err(|error| error.to_string())?;
    std::fs::write(&env, &backends.env).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&env, std::fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
    }
    let worker = OneWorker::installed();
    let over = Over::nothing()
        .config(Some(&config))
        .env_file(Some(&env))
        .kinds(backends.kinds)
        .pack(worker.clone())
        .trusting(backends.trusting);
    serve(
        store,
        Arc::new(RealFs),
        settings,
        over,
        PlayKit::new(),
        worker,
    )
}

fn serve(
    store: &Path,
    fs: Arc<dyn Fs>,
    settings: Settings,
    over: Over,
    kit: Arc<PlayKit>,
    worker: Arc<OneWorker>,
) -> Result<Fixture, String> {
    let over = over.installed(wiring::Models::shipped()?)?;
    let opening = Opening::open_in_existing_parent(store, fs, wiring::recovery())
        .map_err(|error| wiring::not_opened(&error))?;
    let writer = opening.writer().map_err(|why| why.to_string())?;
    // Ten dollars to spend, in a new store; a store opened again keeps its books.
    let held = writer.snapshot().map_err(|error| error.to_string())?;
    if held.runs().is_empty()
        && let Some(ten) = Amount::new(10_000_000)
    {
        let _ = whizbang_core::spend::reset(&writer, ten, 1);
    }
    let registry = wiring::registry_over(&over, false)?;
    let descriptors = registry.descriptors().cloned().collect();
    let executor = wiring::runner(registry, writer.clone(), &over)?;
    let actions = Actions::new(descriptors, store.to_path_buf(), over.worker().cloned());
    let door = Served::new(actions, writer);
    let token = Secret::new(TOKEN)?;
    let host = Host::start(Arc::new(executor), Arc::new(door), settings, token)
        .map_err(|error| format!("cannot listen: {error}"))?;
    Ok(Fixture { host, kit, worker })
}

fn main() {
    let mut store = std::env::temp_dir().join("whizbang-engine-fixture");
    let mut port: u16 = 0;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--store" => store = args.next().map(PathBuf::from).unwrap_or(store),
            "--port" => port = args.next().and_then(|p| p.parse().ok()).unwrap_or(0),
            other => {
                eprintln!("engine_fixture: unknown argument {other}");
                std::process::exit(2);
            }
        }
    }
    let started = write_files(&store, true, None).and_then(|()| {
        start(
            &store,
            Settings::new(SocketAddr::from(([127, 0, 0, 1], port))),
        )
    });
    let fixture = match started {
        Ok(fixture) => fixture,
        Err(why) => {
            eprintln!("engine_fixture: {why}");
            std::process::exit(2);
        }
    };
    let mut out = std::io::stdout();
    let _ = writeln!(out, "http://{}", fixture.host.address());
    let _ = out.flush();
    let _ = std::io::copy(&mut std::io::stdin().lock(), &mut std::io::sink());
    let stopper = fixture.host.stopper();
    stopper.quit();
    fixture.host.wait();
    let _ = std::fs::remove_file(config_path(&store));
    let _ = std::fs::remove_file(env_path(&store));
}
