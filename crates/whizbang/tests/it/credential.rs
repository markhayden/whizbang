//! A download's credential (plan 1C, task N4, D12; ADR 0011, "A download's token"; ADR
//! 0012, "A download's token"). The binary's own `get` (`whizbang::model_commands`),
//! given a test's catalogue, a fake of the download and the place of a sign-in in a
//! temporary folder; and the installer itself, for a request moved to another origin.
//! Every token here is made up at run time, and is put in a temporary folder. Nothing
//! here leaves the machine: the Hub's origin is named, and only the fake is asked.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use whizbang::model_commands::{self, Downloads, Tools, Under};
use whizbang::sign_in::{self, HOW_TO_SIGN_IN};
use whizbang::wiring::{Models, SignIn};
use whizbang_backends::models::{
    Carrying, Catalogue, CatalogueFile, Entry, EntryParts, FileName, Licence, ModelName, ModelsDir,
    Platform, Revision, Title, install_carrying,
};
use whizbang_core::capability::{Fault, FaultKind};
use whizbang_core::contract::{Cancellation, Secrets, SystemClock};
use whizbang_core::egress::{Address, CheckedDownloader, Credential, Origin};
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::downloads::FakeDownloader;
use whizbang_testkit::scratch::Scratch;

/// The Hub's origin: named, never asked. Only the fake answers here.
const HUB: &str = "https://huggingface.co";

/// Another origin, where a file of the Hub may be moved to.
const CDN: &str = "https://cdn.example.org";

/// An origin of the test's own that needs a token kept in the env file.
const WEIGHTS: &str = "https://weights.example.org";

const ADAPTER: &str = "fake-adapter";

/// The config that names the sign-in for the Hub's origin.
const SIGNED_IN: &str =
    "[[models.tokens]]\norigin = \"https://huggingface.co\"\nsign_in = \"huggingface\"\n";

fn none() -> Secrets {
    Secrets::none()
}

/// A made-up token, put together at run time so that no scan of this file finds one,
/// and of no provider's shape.
fn made_up_token(tail: &str) -> String {
    ["made", "up", "token", "0123456789", tail].join("-")
}

fn origin(text: &str) -> Origin {
    Origin::of(text, &none()).unwrap()
}

fn address(origin: &str, path: &str) -> Address {
    Address::of(&format!("{origin}/{path}"), &none()).unwrap()
}

fn bytes_of(model: &str, file: &str) -> Vec<u8> {
    format!("{model}/{file}").bytes().cycle().take(64).collect()
}

/// An entry of these files, each `(origin, name)` and asked for at
/// `<origin>/<model>/<name>`, whose origins are those of its files and `more`; and a
/// download from `needs`, when given, needs a token.
fn entry(name: &str, files: &[(&str, &str)], more: &[&str], needs: Option<&str>) -> Entry {
    let mut origins: Vec<Origin> = Vec::new();
    for at in files.iter().map(|(at, _)| *at).chain(more.iter().copied()) {
        if !origins.contains(&origin(at)) {
            origins.push(origin(at));
        }
    }
    let files = files
        .iter()
        .map(|(at, file)| {
            let content = bytes_of(name, file);
            CatalogueFile::new(
                FileName::new(file, &none()).unwrap(),
                content.len() as u64,
                Digest::of_bytes(&content),
                address(at, &format!("{name}/{file}")),
                &none(),
            )
            .unwrap()
        })
        .collect();
    let parts = EntryParts {
        name: ModelName::new(name, &none()).unwrap(),
        title: Title::new("For tests", &none()).unwrap(),
        revision: Revision::new("1", &none()).unwrap(),
        adapter: ModelName::new(ADAPTER, &none()).unwrap(),
        licence: Licence::new("MIT", &none()).unwrap(),
        platforms: Platform::ALL.to_vec(),
        features: Vec::new(),
        origins,
        files,
    };
    let entry = Entry::new(parts, &none()).unwrap();
    match needs {
        Some(at) => entry.needing_a_token_at(origin(at)).unwrap(),
        None => entry,
    }
}

/// `gated`: one file at the Hub, one at the other origin; a download from the Hub
/// needs a token.
fn gated() -> Entry {
    entry(
        "gated",
        &[(HUB, "model.safetensors"), (CDN, "config.json")],
        &[],
        Some(HUB),
    )
}

/// A fake that serves every file of these entries at the address its entry names.
fn serving(entries: &[Entry]) -> FakeDownloader {
    let mut fake = FakeDownloader::new();
    for entry in entries {
        for file in entry.files() {
            let content = bytes_of(entry.name().as_str(), file.name().as_str());
            fake = fake.serving(file.from(), content);
        }
    }
    fake
}

/// The secrets the maker of the download client was handed, each time.
type Told = Arc<Mutex<Vec<Secrets>>>;

/// The maker of the download client: a checked downloader around the fake, keeping
/// the secrets it was handed.
fn downloads(fake: &Arc<FakeDownloader>, told: &Told) -> Downloads {
    let (fake, told) = (fake.clone(), told.clone());
    Arc::new(move |_least: u64, origins: &[Origin], secrets: &Secrets| {
        told.lock().unwrap().push(secrets.clone());
        CheckedDownloader::new(fake.clone(), origins)
    })
}

/// A test's folder: a models directory not made yet, a home folder, a config and an
/// env file.
struct Bench {
    dir: Scratch,
    /// How often the place of the sign-in was asked for.
    asked: Arc<AtomicU64>,
}

impl Bench {
    fn new(label: &str) -> Bench {
        let bench = Bench {
            dir: Scratch::new(label),
            asked: Arc::default(),
        };
        std::fs::create_dir(bench.home()).unwrap();
        bench
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn models_dir(&self) -> PathBuf {
        self.path("models")
    }

    fn home(&self) -> PathBuf {
        self.path("home")
    }

    /// The place the Hub's tool keeps its token under the test's home folder, as
    /// `sign_in::token_file` gives it.
    fn token_file(&self) -> PathBuf {
        self.home().join(".cache").join("huggingface").join("token")
    }

    /// Writes a token file at this place, readable and writable by its owner alone.
    fn sign_in_at(&self, place: &Path, token: &str) {
        std::fs::create_dir_all(place.parent().unwrap()).unwrap();
        std::fs::write(place, token).unwrap();
        std::fs::set_permissions(place, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    fn sign_in(&self, token: &str) {
        self.sign_in_at(&self.token_file(), token);
    }

    /// What says where the sign-in is: the test's home folder, through the one
    /// function that works out the place; each ask counted.
    fn says(&self) -> SignIn {
        self.says_from(None)
    }

    /// The same, with the Hub's own folder (`HF_HOME`) at this place.
    fn says_from(&self, hf_home: Option<PathBuf>) -> SignIn {
        let (home, asked) = (self.home(), self.asked.clone());
        Arc::new(move || {
            asked.fetch_add(1, Ordering::SeqCst);
            sign_in::token_file(
                None,
                hf_home.as_deref().map(Path::as_os_str),
                None,
                Some(home.as_os_str()),
            )
        })
    }

    fn asked(&self) -> u64 {
        self.asked.load(Ordering::SeqCst)
    }

    fn tools(&self, entries: &[Entry], fake: &Arc<FakeDownloader>, told: &Told) -> Tools {
        self.tools_saying(entries, fake, told, self.says())
    }

    fn tools_saying(
        &self,
        entries: &[Entry],
        fake: &Arc<FakeDownloader>,
        told: &Told,
        says: SignIn,
    ) -> Tools {
        let catalogue = Catalogue::new(entries.to_vec(), &none()).unwrap();
        let adapters = vec![(
            ModelName::new(ADAPTER, &none()).unwrap(),
            Revision::new("7", &none()).unwrap(),
        )];
        let models = Models::of(catalogue, adapters).flag(Some(&self.models_dir()));
        Tools::of(models, Arc::new(SystemClock), downloads(fake, told)).sign_in(says)
    }

    /// Writes the config, and the env file when it is given text.
    fn config(&self, config: &str, env: Option<&str>) -> (PathBuf, Option<PathBuf>) {
        let path = self.path("whizbang.toml");
        std::fs::write(&path, config).unwrap();
        let env = env.map(|text| {
            let env = self.path("keys.env");
            std::fs::write(&env, text).unwrap();
            env
        });
        (path, env)
    }
}

fn under<'a>(config: &'a (PathBuf, Option<PathBuf>)) -> Under<'a> {
    Under {
        config: Some(&config.0),
        env_file: config.1.as_deref(),
    }
}

fn get(under: Under<'_>, model: &str, tools: &Tools) -> (Result<String, String>, Vec<String>) {
    let mut said = Vec::new();
    let got = model_commands::get(under, model, tools, &mut |line| said.push(line.to_owned()));
    (got, said)
}

/// The headers each request carried, by the origin it was asked at, in order.
fn carried(fake: &FakeDownloader) -> Vec<(String, Vec<(String, String)>)> {
    fake.requests()
        .iter()
        .map(|asked| (asked.from().origin().to_string(), asked.headers()))
        .collect()
}

fn bearer(token: &str) -> Vec<(String, String)> {
    vec![("Authorization".to_owned(), format!("Bearer {token}"))]
}

/// Every file under this folder, with its bytes.
fn files_under(folder: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let mut folders = vec![folder.to_path_buf()];
    while let Some(dir) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else {
                let bytes = std::fs::read(&path).unwrap();
                found.push((path, bytes));
            }
        }
    }
    found
}

fn holds(bytes: &[u8], text: &str) -> bool {
    bytes
        .windows(text.len())
        .any(|window| window == text.as_bytes())
}

#[test]
fn a_download_from_an_origin_the_config_names_a_token_for_carries_it_and_one_from_another_origin_does_not()
 {
    // The token of the test's own origin is a key of the env file. `weighted` needs
    // it for its file there, and has one file at another origin; `free` has a file at
    // the same origin and needs no token.
    let bench = Bench::new("credential-carries");
    let token = made_up_token("weights");
    let config = bench.config(
        "[[models.tokens]]\norigin = \"https://weights.example.org\"\nkey_env = \"WEIGHTS_TOKEN\"\n",
        Some(&format!("WEIGHTS_TOKEN={token}\n")),
    );
    let weighted = entry(
        "weighted",
        &[(WEIGHTS, "weights.bin"), (CDN, "config.json")],
        &[],
        Some(WEIGHTS),
    );
    let free = entry("free", &[(WEIGHTS, "weights.bin")], &[], None);
    let fake = Arc::new(serving(&[weighted.clone(), free.clone()]));
    let told = Told::default();
    let tools = bench.tools(&[weighted, free], &fake, &told);
    let (got, _) = get(under(&config), "weighted", &tools);
    assert!(got.unwrap().starts_with("installed weighted: "));
    assert_eq!(
        carried(&fake),
        [
            (CDN.to_owned(), Vec::new()),
            (WEIGHTS.to_owned(), bearer(&token)),
        ]
    );
    // A model that needs no token is asked for with none, at the same origin.
    let (got, _) = get(under(&config), "free", &tools);
    assert!(got.unwrap().starts_with("installed free: "));
    assert_eq!(carried(&fake)[2], (WEIGHTS.to_owned(), Vec::new()));
    assert_eq!(fake.requests().len(), 3);
    // The token of the env file is no sign-in's: the sign-in was not looked for.
    assert_eq!(bench.asked(), 0);
}

#[test]
fn a_download_moved_to_another_origin_is_asked_there_with_no_credential() {
    let bench = Bench::new("credential-moved");
    let token = made_up_token("moved");
    bench.sign_in(&token);
    let config = bench.config(SIGNED_IN, None);
    // One file, at the Hub, which answers that it moved to the other origin, and
    // from there back to the Hub.
    let model = entry("moving", &[(HUB, "model.bin")], &[CDN], Some(HUB));
    let first = model.files()[0].from().clone();
    let away = address(CDN, "stored/model.bin");
    let back = address(HUB, "again/model.bin");
    let bytes = bytes_of("moving", "model.bin");
    let fake = || {
        FakeDownloader::new()
            .moved(&first, away.clone())
            .moved(&away, back.clone())
            .serving(&back, bytes.clone())
    };
    // Through `get`, behind the checked downloader, which follows the moves.
    let followed = Arc::new(fake());
    let told = Told::default();
    let tools = bench.tools(std::slice::from_ref(&model), &followed, &told);
    let (got, _) = get(under(&config), "moving", &tools);
    assert!(got.unwrap().starts_with("installed moving: "));
    let expected = [
        (HUB.to_owned(), bearer(&token)),
        (CDN.to_owned(), Vec::new()),
        (HUB.to_owned(), bearer(&token)),
    ];
    assert_eq!(carried(&followed), expected);
    // And the installer itself, handed a downloader that follows nothing: it asks
    // each address an answer moves the file to, and attaches the credential only at
    // the credential's own origin.
    let bare = FakeDownloader::new();
    let bare = Arc::new(
        bare.moved(&first, away.clone())
            .moved(&away, back.clone())
            .serving(&back, bytes.clone()),
    );
    let root = bench.path("bare-models");
    let dir = ModelsDir::open(Arc::new(RealFs), &root, Arc::new(SystemClock)).unwrap();
    let credential = Credential::new(
        origin(HUB),
        "Authorization",
        &format!("Bearer {token}"),
        &none(),
    )
    .unwrap();
    let carrying = Carrying {
        downloader: &*bare,
        credential: Some(&credential),
    };
    let secrets = Secrets::new([token.clone()]).unwrap();
    let revision = Revision::new("7", &none()).unwrap();
    install_carrying(
        &dir,
        &model,
        &revision,
        &carrying,
        &Cancellation::new(),
        &secrets,
    )
    .unwrap();
    assert_eq!(carried(&bare), expected);
}

#[test]
fn an_entry_whose_origin_needs_a_token_the_config_does_not_name_is_refused_before_anything_is_asked_and_says_how_to_sign_in_or_which_key_to_set()
 {
    let bench = Bench::new("credential-unnamed");
    bench.sign_in(&made_up_token("unnamed"));
    let model = gated();
    let weighted = entry("weighted", &[(WEIGHTS, "weights.bin")], &[], Some(WEIGHTS));
    let fake = Arc::new(serving(&[model.clone(), weighted.clone()]));
    let told = Told::default();
    let tools = bench.tools(&[model, weighted], &fake, &told);
    let sign_in = "sign in with the Hub's command-line tool (`hf auth login`) and name the \
                   sign-in in the config, as `[[models.tokens]]` with `origin = \
                   \"https://huggingface.co\"` and `sign_in = \"huggingface\"`";
    let key = "set the token in the env file and name its variable in the config, as \
               `[[models.tokens]]` with `origin = \"https://huggingface.co\"` and `key_env`";
    // No config; and a config that names a token for another origin only.
    let elsewhere = bench.config(
        "[[models.tokens]]\norigin = \"https://weights.example.org\"\nsign_in = \"huggingface\"\n",
        None,
    );
    assert!(model_commands::list(under(&elsewhere), &tools).is_err());
    let elsewhere = bench.config(
        "[[models.tokens]]\norigin = \"https://weights.example.org\"\nkey_env = \"WEIGHTS_TOKEN\"\n",
        Some("WEIGHTS_TOKEN=made-up-value-of-another-origin\n"),
    );
    for (under, label) in [
        (Under::default(), "no config"),
        (under(&elsewhere), "another"),
    ] {
        let (got, said) = get(under, "gated", &tools);
        let refused = got.unwrap_err();
        assert!(
            refused.starts_with(
                "a download of the model from https://huggingface.co needs a token, and the \
                 config names none for that origin, so the model was not installed and \
                 nothing was downloaded: "
            ),
            "{label}: {refused}"
        );
        assert!(refused.contains(sign_in), "{label}: {refused}");
        assert!(refused.contains(key), "{label}: {refused}");
        assert!(said.is_empty());
        // The guide shows it as the binary prints it, a line of its own.
        let guide = include_str!("../../../../docs/guides/models.md");
        let line = format!("whizbang: {refused}");
        assert!(guide.lines().any(|shown| shown == line), "{line}");
    }
    // An origin that is not the Hub's is told of the key alone: the sign-in is the
    // Hub's.
    let (got, _) = get(Under::default(), "weighted", &tools);
    let refused = got.unwrap_err();
    assert!(refused.contains("`key_env`"), "{refused}");
    assert!(!refused.contains("sign_in"), "{refused}");
    // A variable the env file does not set is named, with what to do.
    let unset = bench.config(
        "[[models.tokens]]\norigin = \"https://huggingface.co\"\nkey_env = \"HUB_TOKEN\"\n",
        Some("OTHER=made-up-value-not-a-token\n"),
    );
    let (got, _) = get(under(&unset), "gated", &tools);
    assert_eq!(
        got.unwrap_err(),
        "the config names the variable `HUB_TOKEN` for the token of https://huggingface.co, \
         and the env file does not set it, so the model was not installed and nothing was \
         downloaded: set it in the env file"
    );
    // Nothing was asked, no models directory was made, and the sign-in, which the
    // config does not name, was not looked for.
    assert!(fake.requests().is_empty());
    assert!(!bench.models_dir().exists());
    assert_eq!(bench.asked(), 0);
    assert!(told.lock().unwrap().is_empty());
}

#[test]
fn the_token_is_in_no_status_record_no_refusal_and_nothing_get_prints() {
    let bench = Bench::new("credential-nowhere");
    let token = made_up_token("nowhere");
    bench.sign_in(&token);
    let config = bench.config(SIGNED_IN, None);
    // `gated` is installed; `broken` is refused by a fault whose words hold the
    // token, as a server's words may; `short` is refused as too short.
    let model = gated();
    let broken = entry("broken", &[(HUB, "model.bin")], &[], Some(HUB));
    let short = entry("short", &[(HUB, "model.bin")], &[], Some(HUB));
    let fake = serving(std::slice::from_ref(&model))
        .failing(
            broken.files()[0].from(),
            Fault::new(FaultKind::Unavailable, &format!("busy for {token}")),
        )
        .serving(short.files()[0].from(), b"short".to_vec());
    let fake = Arc::new(fake);
    let told = Told::default();
    let tools = bench.tools(&[model, broken, short], &fake, &told);
    let mut printed = Vec::new();
    for name in ["gated", "broken", "short", "gated"] {
        let (got, said) = get(under(&config), name, &tools);
        printed.push(got.unwrap_or_else(|refused| refused));
        printed.extend(said);
    }
    assert!(printed[0].starts_with("installed gated: "), "{printed:?}");
    assert!(
        printed[1].contains("could not be downloaded: unavailable"),
        "{printed:?}"
    );
    assert!(
        printed[2].contains("is shorter than the catalogue states"),
        "{printed:?}"
    );
    printed.push(model_commands::list(under(&config), &tools).unwrap());
    for text in &printed {
        assert!(!text.contains(&token), "{text}");
        assert!(!text.contains("0123456789"), "{text}");
    }
    // Every file of the models directory, each model's status record among them.
    let written = files_under(&bench.models_dir());
    let statuses = written
        .iter()
        .filter(|(path, _)| path.file_name().unwrap() == "STATUS")
        .count();
    assert_eq!(statuses, 3);
    for (path, bytes) in &written {
        assert!(!holds(bytes, &token), "{}", path.display());
        assert!(!holds(bytes, "0123456789"), "{}", path.display());
    }
    // It was carried: the requests to the Hub had it.
    let requests = carried(&fake);
    assert!(
        requests
            .iter()
            .filter(|(at, _)| at == HUB)
            .all(|(_, headers)| *headers == bearer(&token)),
        "{requests:?}"
    );
}

#[test]
fn an_origin_that_answers_that_access_is_refused_ends_the_install_with_a_sentence_that_says_to_ask_for_access()
 {
    let bench = Bench::new("credential-access");
    bench.sign_in(&made_up_token("access"));
    let config = bench.config(SIGNED_IN, None);
    let model = gated();
    // The Hub answers 403, as the client says a refused credential. The files are in
    // the order of their names: the second is the Hub's.
    assert_eq!(model.files()[1].from().origin(), &origin(HUB));
    let fake = serving(std::slice::from_ref(&model)).failing(
        model.files()[1].from(),
        Fault::new(FaultKind::Credential, "the server refused the request"),
    );
    let fake = Arc::new(fake);
    let told = Told::default();
    let tools = bench.tools(&[model], &fake, &told);
    let (got, _) = get(under(&config), "gated", &tools);
    let refused = got.unwrap_err();
    assert_eq!(
        refused,
        "the model was not installed: the file `model.safetensors` could not be downloaded: \
         credential. The origin refused the token: ask for access to the model on its page \
         with the account the token is of (on the Hub, accept the model's terms there), and \
         check that the token is that account's"
    );
    let listed = model_commands::list(under(&config), &tools).unwrap();
    assert!(listed.ends_with("\tnot installed\n"), "{listed}");
}

#[test]
fn a_token_of_the_sign_in_is_read_from_the_place_the_wiring_was_given_and_sent_to_its_origin_only()
{
    let bench = Bench::new("credential-place");
    let given = made_up_token("given");
    let other = made_up_token("other");
    // A token under the home folder, and another in a folder of the Hub's own: which
    // is read is which the wiring was given.
    bench.sign_in(&given);
    let hf_home = bench.path("hf-home");
    bench.sign_in_at(&hf_home.join("token"), &other);
    let config = bench.config(SIGNED_IN, None);
    let model = gated();
    for (says, token, wrong) in [
        (bench.says(), &given, &other),
        (bench.says_from(Some(hf_home.clone())), &other, &given),
    ] {
        let fake = Arc::new(serving(std::slice::from_ref(&model)));
        let told = Told::default();
        let tools = bench.tools_saying(std::slice::from_ref(&model), &fake, &told, says);
        let asked = bench.asked();
        let (got, _) = get(under(&config), "gated", &tools);
        got.unwrap();
        // Asked once, and the token is carried to its origin, the Hub, and to no
        // other: the file at the other origin was asked for with nothing.
        assert_eq!(bench.asked(), asked + 1);
        assert_eq!(
            carried(&fake),
            [
                (CDN.to_owned(), Vec::new()),
                (HUB.to_owned(), bearer(token)),
            ]
        );
        assert!(fake.requests().iter().all(|asked| {
            !asked
                .headers()
                .iter()
                .any(|(_, value)| value.contains(wrong))
        }));
        model_commands::remove(under(&config), "gated", std::time::Duration::ZERO, &tools).unwrap();
    }
    // The place itself, as the binary's `main` works it out from what it is handed
    // (`sign_in::token_file`; every variable, with made-up folders, in its own test).
    assert_eq!(
        sign_in::token_file(None, None, None, Some(bench.home().as_os_str())),
        Some(bench.token_file())
    );
}

#[test]
fn the_sign_ins_store_is_not_opened_by_a_command_that_downloads_nothing_from_its_origin() {
    let bench = Bench::new("credential-unopened");
    bench.sign_in(&made_up_token("unopened"));
    let model = gated();
    // A model that needs no token, one whose adapter this build lacks, one for
    // another platform, and one whose Hub token is a key of the env file.
    let free = entry("free", &[(HUB, "free.bin")], &[], None);
    let parts_of = |name: &str, adapter: &str, on: Vec<Platform>| {
        let file = entry(name, &[(HUB, "model.bin")], &[], None).files()[0].clone();
        EntryParts {
            name: ModelName::new(name, &none()).unwrap(),
            title: Title::new("For tests", &none()).unwrap(),
            revision: Revision::new("1", &none()).unwrap(),
            adapter: ModelName::new(adapter, &none()).unwrap(),
            licence: Licence::new("MIT", &none()).unwrap(),
            platforms: on,
            features: Vec::new(),
            origins: vec![origin(HUB)],
            files: vec![file],
        }
    };
    let lacking = Entry::new(
        parts_of("lacking", "another-adapter", Platform::ALL.to_vec()),
        &none(),
    )
    .unwrap()
    .needing_a_token_at(origin(HUB))
    .unwrap();
    let this = Platform::this();
    let others: Vec<Platform> = Platform::ALL
        .into_iter()
        .filter(|p| Some(*p) != this)
        .collect();
    let foreign = Entry::new(parts_of("foreign", ADAPTER, others), &none())
        .unwrap()
        .needing_a_token_at(origin(HUB))
        .unwrap();
    let entries = [model.clone(), free.clone(), lacking, foreign];
    let fake = Arc::new(serving(&entries));
    let told = Told::default();
    let tools = bench.tools(&entries, &fake, &told);
    let signed_in = bench.config(SIGNED_IN, None);
    let signed = under(&signed_in);
    let wait = std::time::Duration::ZERO;
    assert!(model_commands::list(signed, &tools).is_ok());
    assert!(get(signed, "free", &tools).0.is_ok());
    assert!(get(signed, "lacking", &tools).0.is_err());
    assert!(get(signed, "foreign", &tools).0.is_err());
    assert!(get(signed, "absent", &tools).0.is_err());
    assert!(model_commands::list(signed, &tools).is_ok());
    assert!(model_commands::remove(signed, "gated", wait, &tools).is_err());
    assert!(model_commands::remove(signed, "free", wait, &tools).is_ok());
    let keyed = bench.config(
        "[[models.tokens]]\norigin = \"https://huggingface.co\"\nkey_env = \"HUB_TOKEN\"\n",
        Some(&format!("HUB_TOKEN={}\n", made_up_token("keyed"))),
    );
    assert!(get(under(&keyed), "gated", &tools).0.is_ok());
    assert!(model_commands::remove(under(&keyed), "gated", wait, &tools).is_ok());
    assert_eq!(bench.asked(), 0);
    // And the one command that does: `get` of the model, under the sign-in.
    let signed_in = bench.config(SIGNED_IN, None);
    assert!(get(under(&signed_in), "gated", &tools).0.is_ok());
    assert_eq!(bench.asked(), 1);
}

#[test]
fn a_sign_in_that_is_not_there_or_cannot_be_read_ends_the_install_before_anything_is_asked_and_its_refusal_holds_no_path()
 {
    let bench = Bench::new("credential-unread");
    let config = bench.config(SIGNED_IN, None);
    let model = gated();
    let place = bench.token_file();
    let token = made_up_token("unread");
    let refusal = |refused: &str| {
        format!(
            "the token of https://huggingface.co is the Hub's sign-in's, so the model was not \
             installed and nothing was downloaded: {refused}"
        )
    };
    // Each case writes what is at the place, or nothing; and says the sign-in.
    type Plant = Box<dyn Fn(&Path)>;
    let file = |bytes: Vec<u8>, mode: u32| -> Plant {
        Box::new(move |place: &Path| {
            std::fs::create_dir_all(place.parent().unwrap()).unwrap();
            std::fs::write(place, &bytes).unwrap();
            std::fs::set_permissions(place, std::fs::Permissions::from_mode(mode)).unwrap();
        })
    };
    let cases: Vec<(&str, Plant, &str)> = vec![
        (
            "nothing",
            Box::new(|_: &Path| {}),
            "the Hub's sign-in has no token on this machine",
        ),
        (
            "a folder",
            Box::new(|place: &Path| std::fs::create_dir_all(place).unwrap()),
            "the Hub's sign-in's token is not a regular file",
        ),
        (
            "readable by others",
            file(token.clone().into_bytes(), 0o644),
            "the Hub's sign-in's token file can be read or written by others than its owner, \
             so it is not used; make it its owner's alone (mode 600), or sign in again, which \
             writes it so",
        ),
        (
            "writable by the group",
            file(token.clone().into_bytes(), 0o620),
            "the Hub's sign-in's token file can be read or written by others than its owner",
        ),
        (
            "empty",
            file(Vec::new(), 0o600),
            "the Hub's sign-in's token file is empty",
        ),
        (
            "not a token",
            file(format!("{token}\0{token}").into_bytes(), 0o600),
            "the Hub's sign-in's token file holds something that is not a token",
        ),
        (
            "too long",
            file(token.repeat(20).into_bytes(), 0o600),
            "the Hub's sign-in's token file is longer than a token",
        ),
        (
            "too short",
            file(b"short\n".to_vec(), 0o600),
            "the Hub's sign-in's token file is shorter than a token",
        ),
    ];
    let fake = Arc::new(serving(std::slice::from_ref(&model)));
    let told = Told::default();
    for (label, plant, said) in cases {
        let _ = std::fs::remove_dir_all(bench.home());
        std::fs::create_dir(bench.home()).unwrap();
        plant(&place);
        let tools = bench.tools(std::slice::from_ref(&model), &fake, &told);
        let (got, lines) = get(under(&config), "gated", &tools);
        let refused = got.unwrap_err();
        assert!(refused.starts_with(&refusal(said)), "{label}: {refused}");
        assert!(refused.ends_with(HOW_TO_SIGN_IN), "{label}: {refused}");
        for path in [bench.dir.path(), place.as_path()] {
            assert!(
                !refused.contains(path.to_str().unwrap()),
                "{label}: {refused}"
            );
        }
        assert!(!refused.contains("huggingface/token"), "{label}: {refused}");
        assert!(!refused.contains("0123456789"), "{label}: {refused}");
        assert!(lines.is_empty());
    }
    // And with no place at all: the wiring gives none.
    let tools = bench.tools_saying(
        std::slice::from_ref(&model),
        &fake,
        &told,
        Arc::new(|| None),
    );
    let refused = get(under(&config), "gated", &tools).0.unwrap_err();
    assert!(
        refused.contains("where the Hub's sign-in keeps its token is not known"),
        "{refused}"
    );
    // Nothing was asked, nothing was made, and no client was made.
    assert!(fake.requests().is_empty());
    assert!(!bench.models_dir().exists());
    assert!(told.lock().unwrap().is_empty());
}

#[test]
fn a_token_of_the_sign_in_is_redacted_from_a_fault_made_after_it_was_read() {
    // Once read, the token is a secret to everything that comes after: the maker of
    // the download client is told it, as the client's own faults are made with it;
    // the installer is told it; and a fault whose words hold it, as a server's words
    // may, is in nothing that is printed or written.
    let bench = Bench::new("credential-redacted");
    let token = made_up_token("redacted");
    bench.sign_in(&format!("{token}\n"));
    let config = bench.config(SIGNED_IN, None);
    let model = gated();
    let fake = serving(std::slice::from_ref(&model)).failing(
        model.files()[1].from(),
        Fault::new(FaultKind::Malformed, &format!("answered {token} back")),
    );
    let fake = Arc::new(fake);
    let told = Told::default();
    let tools = bench.tools(std::slice::from_ref(&model), &fake, &told);
    let (got, said) = get(under(&config), "gated", &tools);
    let refused = got.unwrap_err();
    assert!(
        refused.contains("could not be downloaded: malformed"),
        "{refused}"
    );
    assert!(!refused.contains(&token) && said.is_empty(), "{refused}");
    let told = told.lock().unwrap().clone();
    assert_eq!(told.len(), 1);
    let redacted = told[0].redact(&format!("a fault that says {token}"));
    assert!(!redacted.contains(&token), "{redacted}");
    // The line break the file ended with is not part of the token: what was sent is
    // the token alone.
    assert!(
        carried(&fake)
            .iter()
            .any(|(at, headers)| at == HUB && *headers == bearer(&token))
    );
    for (path, bytes) in files_under(&bench.models_dir()) {
        assert!(!holds(&bytes, &token), "{}", path.display());
    }
}
