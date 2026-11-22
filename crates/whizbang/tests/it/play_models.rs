//! `play_models::…`: the playground's Models screen (plan m5, task M1): the catalogue's
//! models, an install from the page with its progress, Cancel and Remove; each backend the
//! config offers with whether its key is set; and the allowance with the owner's reset.
//! The host runs in process over the binary's wiring and door, with the test kit's
//! `pocket` model, whose files a fake download serves from memory: nothing is fetched,
//! and no real model is ever asked for. Requests are bytes on a loopback socket.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value as Json, json};
use whizbang::model_commands::{Downloads, Tools};
use whizbang::play_models::PlayModels;
use whizbang::playing::Playground;
use whizbang::wiring::{self, Models, Over};
use whizbang_backends::models::{
    Catalogue, CatalogueFile, Current, Entry, EntryParts, FileName, Holds, Licence, ModelName,
    ModelsDir, Platform, Revision, Title, current,
};
use whizbang_core::contract::{
    Action, ActionDescriptor, Context, Failure, Kind, PreparedCall, Produced, Secrets, SystemClock,
};
use whizbang_core::egress::{Address, CheckedDownloader, Origin};
use whizbang_core::identity::Digest;
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_serve::play::{Host, Settings, Token};
use whizbang_testkit::downloads::FakeDownloader;
use whizbang_testkit::playmodels::{self, GatedDownloader, MODEL};
use whizbang_testkit::scratch::Scratch;

fn none() -> Secrets {
    Secrets::none()
}

/// A made-up value, long enough to be a secret, assembled so that no scan of this file
/// finds one.
fn made_up(tail: &str) -> String {
    ["made", "up", "value", "0123456789", tail].join("-")
}

/// The first revision of `pocket`.
const FIRST: &str = "2026-11.1";

/// The adapter every entry here names, at the revision the test's build has.
const ADAPTER: &str = "owlv2-sam2";

/// Two files a later revision of `pocket` adds, which no earlier installation holds, so
/// that its install downloads two files and the gate holds the second.
fn added() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("extra-a.bin", b"a later revision's first file".to_vec()),
        ("extra-b.bin", b"a later revision's second file".to_vec()),
    ]
}

/// `pocket` at this revision, its files as the test kit serves them (and, past the first
/// revision, [`added`]); and `locked`, whose one file needs a token from its origin.
fn catalogue(revision: &str) -> Catalogue {
    let file = |name: &str, bytes: &[u8]| {
        CatalogueFile::new(
            FileName::new(name, &none()).unwrap(),
            bytes.len() as u64,
            Digest::of_bytes(bytes),
            Address::of(&playmodels::address_of(name), &none()).unwrap(),
            &none(),
        )
        .unwrap()
    };
    let entry = |name: &str, files: Vec<CatalogueFile>| {
        Entry::new(
            EntryParts {
                name: ModelName::new(name, &none()).unwrap(),
                title: Title::new(&format!("{name}, for tests"), &none()).unwrap(),
                revision: Revision::new(revision, &none()).unwrap(),
                adapter: ModelName::new(ADAPTER, &none()).unwrap(),
                licence: Licence::new("Apache-2.0", &none()).unwrap(),
                platforms: Platform::ALL.to_vec(),
                features: Vec::new(),
                origins: vec![Origin::of(playmodels::ORIGIN, &none()).unwrap()],
                files,
            },
            &none(),
        )
        .unwrap()
    };
    let mut files = playmodels::files();
    if revision != FIRST {
        files.extend(added());
    }
    let pocket = entry(
        MODEL,
        files
            .iter()
            .map(|(name, bytes)| file(name, bytes))
            .collect(),
    );
    let locked = entry("locked", vec![file("LICENSE", &playmodels::files()[0].1)])
        .needing_a_token_at(Origin::of(playmodels::ORIGIN, &none()).unwrap())
        .unwrap();
    Catalogue::new(vec![pocket, locked], &none()).unwrap()
}

/// The fake download of `pocket`'s files, held when the test says so.
fn served() -> Arc<GatedDownloader> {
    let mut fake = FakeDownloader::new();
    for (name, bytes) in playmodels::files().into_iter().chain(added()) {
        fake = fake.serving(
            &Address::of(&playmodels::address_of(name), &none()).unwrap(),
            bytes,
        );
    }
    Arc::new(GatedDownloader::new(fake))
}

/// The config: a remote backend whose key the env file sets, one whose key it does not,
/// one that names no key, and a token for the models' origin held in the env file.
fn config_text() -> String {
    let kind = whizbang_testkit::playkit::KIND;
    format!(
        "[backends.fill]\nkind = \"{kind}\"\nmodel = \"fill\"\n\
         url = \"https://fill.example.org/v1\"\nkey_env = \"PLAYKIT_FILL_KEY\"\n\n\
         [backends.judge]\ndefault = \"cloud-judge\"\n\n\
         [backends.judge.cloud-judge]\nkind = \"{kind}\"\nmodel = \"judge\"\n\
         url = \"https://judge.example.org/v1\"\n\n\
         [backends.judge.keyless-judge]\nkind = \"{kind}\"\nmodel = \"keyless\"\n\
         url = \"https://judge.example.org/v2\"\nkey_env = \"PLAYKIT_UNSET_KEY\"\n\n\
         [egress.recipients.fill]\norigin = \"https://fill.example.org\"\n\
         classes = [\"source_images\", \"derived_images\"]\n\n\
         [egress.recipients.judge]\norigin = \"https://judge.example.org\"\n\
         classes = [\"prompts\", \"source_images\", \"derived_images\", \"evidence\"]\n\n\
         [spend]\ncall_cap_usd = 1.5\n\n\
         [[models.tokens]]\norigin = \"{}\"\nkey_env = \"POCKET_TOKEN\"\n",
        playmodels::ORIGIN
    )
}

/// A test action that waits until it is stopped.
struct Waits(ActionDescriptor);

impl Action for Waits {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, _: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        for _ in 0..6000 {
            if context.cancellation().is_cancelled() || context.deadline_passed() {
                return Err(Failure::error("stopped", "the run was told to stop"));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(Produced::nothing())
    }
}

/// The host over the binary's door and the models' fakes, in a store of its own.
struct Hosted {
    scratch: Scratch,
    downloads: Arc<GatedDownloader>,
    host: Option<Host>,
    address: SocketAddr,
    token: Token,
    secrets: Vec<String>,
}

impl Hosted {
    /// A host over a new store in a new scratch folder, with `pocket` at `revision`.
    fn new(label: &str, revision: &str) -> Hosted {
        Hosted::over(Scratch::new(label), revision, true)
    }

    fn over(scratch: Scratch, revision: &str, new: bool) -> Hosted {
        let root = scratch.path().join("store");
        let config = scratch.path().join("whizbang.toml");
        let env = scratch.path().join("keys.env");
        let models_dir = scratch.path().join("models");
        let secrets = vec![made_up("fill"), made_up("pocket")];
        if new {
            std::fs::write(&config, config_text()).unwrap();
            std::fs::write(
                &env,
                format!(
                    "PLAYKIT_FILL_KEY={}\nPOCKET_TOKEN={}\n",
                    secrets[0], secrets[1]
                ),
            )
            .unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let own = std::fs::Permissions::from_mode(0o600);
                std::fs::set_permissions(&env, own).unwrap();
            }
        }
        let kit = whizbang_testkit::playkit::PlayKit::new();
        let mut kinds = wiring::kinds(&whizbang::pack::OneWorker::installed());
        kinds.register(kit.kind()).unwrap();
        let adapters = vec![(
            ModelName::new(ADAPTER, &none()).unwrap(),
            Revision::new("1", &none()).unwrap(),
        )];
        let models = Models::of(catalogue(revision), adapters).flag(Some(&models_dir));
        let over = Over::nothing()
            .config(Some(&config))
            .env_file(Some(&env))
            .kinds(kinds)
            .installed(models.clone())
            .unwrap();
        let store = Arc::new(Store::open(&root, Arc::new(crate::checked::Unsynced)).unwrap());
        if new {
            let ten = whizbang_core::spend::Amount::new(10_000_000).unwrap();
            whizbang_core::spend::reset(&store, ten, 1).unwrap();
        }
        let mut registry = wiring::registry_over(&over, false).unwrap();
        let waits = ActionDescriptor::draft("waits", "1.0.0", Kind::Primitive, "A test action.");
        registry
            .register(Arc::new(Waits(waits.build().unwrap())))
            .unwrap();
        let descriptors = registry.descriptors().cloned().collect();
        let runner = wiring::runner(registry, store.clone(), &over).unwrap();
        let downloads = served();
        let serving = downloads.clone();
        let made: Downloads = Arc::new(move |_: u64, origins: &[Origin], _: &Secrets| {
            CheckedDownloader::new(serving.clone(), origins)
        });
        let tools = Tools::of(models, Arc::new(SystemClock), made);
        let door = PlayModels::new(tools, Some(&config), Some(&env)).waiting(Duration::ZERO);
        let play = Playground::new(descriptors, Arc::new(over))
            .with_store(store, &root)
            .with_models(door);
        let mut settings = Settings::new(root);
        settings.port = 0;
        settings.grace = Duration::from_secs(5);
        let token = Token::new().unwrap();
        let host = Host::start(Arc::new(runner), Arc::new(play), settings, token.clone()).unwrap();
        Hosted {
            address: host.address(),
            scratch,
            downloads,
            host: Some(host),
            token,
            secrets,
        }
    }

    /// Quits, and starts a host again over the same folder with `pocket` at `revision`.
    fn restarted(mut self, revision: &str) -> Hosted {
        let host = self.host.take().unwrap();
        host.stopper().quit();
        host.wait();
        let scratch = std::mem::replace(&mut self.scratch, Scratch::new("play-models-spent"));
        Hosted::over(scratch, revision, false)
    }

    fn models_dir(&self) -> PathBuf {
        self.scratch.path().join("models")
    }

    fn api(&self, method: &str, path: &str, body: Option<&Json>) -> Answer {
        let port = self.address.port();
        let mut head = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {}\r\n",
            self.token.hex()
        );
        if method != "GET" {
            head.push_str(&format!("Origin: http://127.0.0.1:{port}\r\n"));
        }
        let body = body.map(Json::to_string);
        if let Some(body) = &body {
            head.push_str(&format!(
                "Content-Type: application/json\r\nContent-Length: {}\r\n",
                body.len()
            ));
        }
        head.push_str("\r\n");
        head.push_str(body.as_deref().unwrap_or_default());
        send(self.address, head.as_bytes())
    }

    /// `pocket`'s row of `GET /api/models`.
    fn pocket(&self) -> Json {
        let listed = self.api("GET", "/api/models", None);
        assert_eq!(listed.status, 200, "{}", listed.text());
        listed.json()["models"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == MODEL)
            .cloned()
            .unwrap()
    }

    /// Polls `pocket`'s row until `done` holds of it, 20 seconds at most.
    fn pocket_until(&self, done: impl Fn(&Json) -> bool) -> Json {
        let limit = Instant::now() + Duration::from_secs(20);
        loop {
            let row = self.pocket();
            if done(&row) {
                return row;
            }
            assert!(Instant::now() < limit, "pocket never got there: {row}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Waits until a download is held at the gate.
    fn until_held(&self) {
        let limit = Instant::now() + Duration::from_secs(20);
        while !self.downloads.waiting() {
            assert!(Instant::now() < limit, "the install never reached the gate");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// The installation the models directory's pointer names for `pocket`.
    fn installed(&self) -> Option<String> {
        let dir =
            ModelsDir::open(Arc::new(RealFs), &self.models_dir(), Arc::new(SystemClock)).ok()?;
        let name = ModelName::new(MODEL, &none()).unwrap();
        match current(&dir, &name, &none()) {
            Ok(Current::Installed(record)) => Some(record.identity().hex()),
            _ => None,
        }
    }
}

impl Drop for Hosted {
    fn drop(&mut self) {
        self.downloads.release();
        if let Some(host) = self.host.take() {
            let stopper = host.stopper();
            stopper.quit();
            stopper.quit();
            host.wait();
        }
    }
}

struct Answer {
    status: u16,
    body: Vec<u8>,
}

impl Answer {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    fn json(&self) -> Json {
        serde_json::from_slice(&self.body).unwrap_or(Json::Null)
    }
}

fn send(address: SocketAddr, bytes: &[u8]) -> Answer {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    stream.write_all(bytes).unwrap();
    let mut answer = Vec::new();
    let _ = stream.read_to_end(&mut answer);
    let end = answer
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap_or_else(|| panic!("no answer: {:?}", String::from_utf8_lossy(&answer)));
    let status = String::from_utf8_lossy(&answer[..end])
        .split(' ')
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    Answer {
        status,
        body: answer[end + 4..].to_vec(),
    }
}

fn texts_hold_none_of(answers: &[Answer], secrets: &[&str]) {
    for answer in answers {
        let text = answer.text();
        for secret in secrets {
            assert!(!text.contains(secret), "an answer holds a secret: {text}");
        }
        assert!(!text.contains("0123456789"), "{text}");
        // Neither the variables' names, which a key pasted in place of a name would be.
        assert!(!text.contains("PLAYKIT_FILL_KEY"), "{text}");
        assert!(!text.contains("POCKET_TOKEN"), "{text}");
    }
}

#[test]
fn models_lists_only_the_catalogues_models_and_the_configs_backends() {
    let hosted = Hosted::new("play-models-list", FIRST);
    let listed = hosted.api("GET", "/api/models", None);
    assert_eq!(listed.status, 200, "{}", listed.text());
    let rows = listed.json();
    let names: Vec<&str> = rows["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["locked", MODEL]);
    let pocket = &rows["models"][1];
    assert_eq!(pocket["state"], "not_installed");
    assert_eq!(pocket["files"], 3);
    assert_eq!(pocket["bytes"], 600 + 900 + 4096);
    assert_eq!(pocket["sign_in"], "none");
    // `locked` needs a token from its origin, which the config leads to through the env
    // file: Get is offered, and the token is not answered.
    assert_eq!(rows["models"][0]["sign_in"], "none");
    let backends = hosted.api("GET", "/api/backends", None);
    assert_eq!(backends.status, 200, "{}", backends.text());
    let access: Vec<(String, String)> = backends.json()["backends"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["name"].as_str().unwrap().to_owned(),
                row["access"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let access: Vec<(&str, &str)> = access
        .iter()
        .map(|(n, a)| (n.as_str(), a.as_str()))
        .collect();
    assert_eq!(
        access,
        [
            ("default", "key_set"),
            ("cloud-judge", "no_key_needed"),
            ("keyless-judge", "no_key"),
        ]
    );
    // A model the catalogue does not hold is not installed, and nothing is asked.
    let other = hosted.api("POST", "/api/models/depth/get", None);
    assert_eq!(other.status, 404, "{}", other.text());
    assert!(hosted.api("POST", "/api/models/..%2f/get", None).status == 404);
}

#[test]
fn a_second_get_attaches_to_the_running_install() {
    let hosted = Hosted::new("play-models-attach", FIRST);
    hosted.downloads.hold();
    let first = hosted.api("POST", "/api/models/pocket/get", None);
    assert_eq!(first.status, 202, "{}", first.text());
    assert_eq!(first.json()["attached"], false);
    hosted.until_held();
    // Its progress: files done of its files, from its status record.
    let row = hosted.pocket_until(|row| row["files_done"] == 1);
    assert_eq!(row["install"], "installing");
    assert_eq!(row["files"], 3);
    let second = hosted.api("POST", "/api/models/pocket/get", None);
    assert_eq!(second.status, 200, "{}", second.text());
    assert_eq!(second.json()["attached"], true);
    // One install at a time: another model waits for this one.
    let other = hosted.api("POST", "/api/models/locked/get", None);
    assert_eq!(other.status, 409, "{}", other.text());
    // A removal of the model being installed is refused.
    let removed = hosted.api("DELETE", "/api/models/pocket", None);
    assert_eq!(removed.status, 409, "{}", removed.text());
    hosted.downloads.release();
    let row = hosted.pocket_until(|row| row["last"]["ended"] == "installed");
    assert_eq!(row["state"], "installed");
    assert!(hosted.installed().is_some());
}

#[test]
fn a_quit_during_an_install_cancels_it_and_leaves_the_earlier_install() {
    // The earlier install: `pocket` at its first revision, installed whole.
    let hosted = Hosted::new("play-models-quit", FIRST);
    assert_eq!(
        hosted.api("POST", "/api/models/pocket/get", None).status,
        202
    );
    hosted.pocket_until(|row| row["last"]["ended"] == "installed");
    let earlier = hosted.installed().unwrap();
    // The catalogue moves on; the page's Get installs the new revision, and is held
    // after its first file. Cancel answers at once, the install ends cancelled.
    let hosted = hosted.restarted("2026-11.2");
    assert_eq!(hosted.pocket()["state"], "out_of_date");
    hosted.downloads.hold();
    assert_eq!(
        hosted.api("POST", "/api/models/pocket/get", None).status,
        202
    );
    hosted.until_held();
    let cancel = hosted.api("POST", "/api/models/pocket/cancel", None);
    assert_eq!(cancel.status, 202, "{}", cancel.text());
    let row = hosted.pocket_until(|row| row["last"]["ended"] == "cancelled");
    assert_eq!(row["state"], "out_of_date");
    assert_eq!(hosted.installed().as_deref(), Some(earlier.as_str()));
    // Again, and this time a quit cancels it: the host waits for the install and ends,
    // and the earlier install is in place.
    assert_eq!(
        hosted.api("POST", "/api/models/pocket/get", None).status,
        202
    );
    hosted.until_held();
    let mut hosted = hosted;
    let host = hosted.host.take().unwrap();
    let started = Instant::now();
    host.stopper().quit();
    let stopped = host.wait();
    assert_eq!(stopped.left, 0);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(hosted.installed().as_deref(), Some(earlier.as_str()));
}

#[test]
fn status_cancel_and_quit_answer_with_four_runs_and_an_install_going() {
    let mut hosted = Hosted::new("play-models-busy", FIRST);
    hosted.downloads.hold();
    assert_eq!(
        hosted.api("POST", "/api/models/pocket/get", None).status,
        202
    );
    hosted.until_held();
    let mut runs = Vec::new();
    for at in 0..4 {
        let call = json!({"action": "waits", "arguments": {}});
        let prepared = hosted.api("POST", "/api/prepare", Some(&call));
        assert_eq!(prepared.status, 200, "{}", prepared.text());
        let mut start = call;
        start["handle"] = prepared.json()["handle"].clone();
        start["request_id"] = json!(format!("models-busy-{at}"));
        let started = hosted.api("POST", "/api/runs", Some(&start));
        assert_eq!(started.status, 201, "{}", started.text());
        runs.push(started.json()["run"].as_str().unwrap().to_owned());
    }
    // A poll, the models' list, a Stop: each answered at once.
    let polled = hosted.api("GET", &format!("/api/runs/{}?after=0", runs[0]), None);
    assert_eq!(polled.status, 200, "{}", polled.text());
    assert_eq!(hosted.pocket()["install"], "installing");
    let stop = hosted.api("POST", &format!("/api/runs/{}/cancel", runs[0]), None);
    assert_eq!(stop.status, 202, "{}", stop.text());
    let quit = hosted.api("POST", "/api/quit", None);
    assert_eq!(quit.status, 202, "{}", quit.text());
    let stopped = hosted.host.take().unwrap().wait();
    assert_eq!(stopped.left, 0);
    assert!(hosted.installed().is_none());
}

#[test]
fn a_removal_of_a_model_in_use_answers_busy_and_the_model_stays() {
    let hosted = Hosted::new("play-models-remove", FIRST);
    assert_eq!(
        hosted.api("POST", "/api/models/pocket/get", None).status,
        202
    );
    hosted.pocket_until(|row| row["last"]["ended"] == "installed");
    let installed = hosted.installed().unwrap();
    // A run holds it, as a run of a local backend does.
    let dir = ModelsDir::open(
        Arc::new(RealFs),
        &hosted.models_dir(),
        Arc::new(SystemClock),
    )
    .unwrap();
    let name = ModelName::new(MODEL, &none()).unwrap();
    let Ok(Current::Installed(record)) = current(&dir, &name, &none()) else {
        panic!("pocket is not installed");
    };
    let hold = Holds::over(dir)
        .hold(&name, record.identity(), &none())
        .unwrap();
    let busy = hosted.api("DELETE", "/api/models/pocket", None);
    assert_eq!(busy.status, 409, "{}", busy.text());
    assert!(busy.text().contains("not removed"), "{}", busy.text());
    assert_eq!(hosted.installed().as_deref(), Some(installed.as_str()));
    drop(hold);
    let removed = hosted.api("DELETE", "/api/models/pocket", None);
    assert_eq!(removed.status, 200, "{}", removed.text());
    assert_eq!(hosted.pocket()["state"], "not_installed");
}

#[test]
fn a_reset_from_the_page_needs_the_amount_and_the_word_and_closes_the_books() {
    let hosted = Hosted::new("play-models-reset", FIRST);
    let before = hosted.api("GET", "/api/spend", None);
    assert_eq!(before.status, 200, "{}", before.text());
    let before = before.json();
    assert_eq!(before["epoch"], 1);
    assert_eq!(before["allowance"], 10.0);
    assert_eq!(before["call_cap"], 1.5);
    assert_eq!(before["config"], "whizbang.toml");
    assert_eq!(before["since_ms"], 1);
    // Without the word, with another word, without the amount, with an amount that is
    // no amount, with a field the route does not take: refused, and nothing changes.
    for body in [
        json!({"allowance": "12.50"}),
        json!({"allowance": "12.50", "confirm": "Reset"}),
        json!({"confirm": "reset"}),
        json!({"allowance": "12,50", "confirm": "reset"}),
        json!({"allowance": "-1", "confirm": "reset"}),
        json!({"allowance": "12.50", "confirm": "reset", "epoch": 1}),
    ] {
        let refused = hosted.api("POST", "/api/spend/reset", Some(&body));
        assert_eq!(refused.status, 400, "{body}: {}", refused.text());
    }
    assert_eq!(hosted.api("GET", "/api/spend", None).json()["epoch"], 1);
    // The amount and the word: the books close, and a new epoch opens at the amount.
    let body = json!({"allowance": "12.50", "confirm": "reset"});
    let reset = hosted.api("POST", "/api/spend/reset", Some(&body));
    assert_eq!(reset.status, 200, "{}", reset.text());
    assert_eq!(reset.json()["epoch"], 2);
    let after = hosted.api("GET", "/api/spend", None).json();
    assert_eq!(after["epoch"], 2);
    assert_eq!(after["allowance"], 12.5);
    assert_eq!(after["left"], 12.5);
    assert_eq!(after["charged"]["usd"], 0.0);
    assert!(after["since_ms"].as_u64().unwrap() > 1, "{after}");
    // And hello says the same allowance.
    let hello = hosted.api("GET", "/api/hello", None).json();
    assert_eq!(hello["allowance"]["allowance"], 12.5);
}

#[test]
fn no_response_of_any_route_holds_a_key_a_sign_in_or_the_token() {
    let hosted = Hosted::new("play-models-secrets", FIRST);
    let mut answers = Vec::new();
    for path in ["/api/hello", "/api/models", "/api/backends", "/api/spend"] {
        answers.push(hosted.api("GET", path, None));
    }
    // `locked` needs the token the env file holds: its install carries it, and nothing
    // the page is answered holds it, its install's end and its failure among them.
    answers.push(hosted.api("POST", "/api/models/locked/get", None));
    let row = {
        let limit = Instant::now() + Duration::from_secs(20);
        loop {
            let listed = hosted.api("GET", "/api/models", None);
            let row = listed.json()["models"][0].clone();
            if row["last"].is_object() {
                answers.push(listed);
                break row;
            }
            assert!(Instant::now() < limit, "locked's install never ended");
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    assert_eq!(row["name"], "locked");
    answers.push(hosted.api("POST", "/api/models/locked/cancel", None));
    answers.push(hosted.api("DELETE", "/api/models/locked", None));
    answers.push(hosted.api("POST", "/api/models/depth/get", None));
    let body = json!({"allowance": "nope", "confirm": "reset"});
    answers.push(hosted.api("POST", "/api/spend/reset", Some(&body)));
    let prepare = json!({"action": "waits", "arguments": {}});
    answers.push(hosted.api("POST", "/api/prepare", Some(&prepare)));
    answers.push(hosted.api("GET", "/api/runs?open=1", None));
    answers.push(hosted.api("GET", "/api/pictures", None));
    for answer in &answers {
        assert_ne!(answer.status, 0);
    }
    let token = hosted.token.hex();
    let secrets: Vec<&str> = hosted
        .secrets
        .iter()
        .map(String::as_str)
        .chain([token.as_str()])
        .collect();
    texts_hold_none_of(&answers, &secrets);
}
