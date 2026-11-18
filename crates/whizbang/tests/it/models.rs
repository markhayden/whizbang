//! `whizbang models list`, `get` and `remove` (plan 1C, task N3b; ADR 0017). The
//! binary's own functions (`whizbang::model_commands`), given a test's catalogue, a
//! fake of the download and a clock the test moves; and the real binary, which has
//! the catalogue it ships, whose one model, `sam3` (task N5), needs the Hub's token:
//! every start of it here is refused before anything is asked of any address, so
//! nothing here leaves the machine. Every key is made up at run time.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use whizbang::model_commands::{self, Downloads, PAUSE, Tools, Under};
use whizbang::pack as kind;
use whizbang::wiring::{self, Models};
use whizbang_backends::models::{
    Catalogue, CatalogueFile, Entry, EntryParts, FileName, Installations, Licence, ModelName,
    ModelsDir, Platform, Revision, Title,
};
use whizbang_core::capability::{FaultKind, Installation, Installed, Inventory, Unready};
use whizbang_core::config::DOWNLOAD_LEAST_KIB_PER_SECOND;
use whizbang_core::contract::{Call, Clock, Executor, Secrets};
use whizbang_core::egress::{Address, CheckedDownloader, Origin};
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::downloads::FakeDownloader;
use whizbang_testkit::scratch::Scratch;
use whizbang_testkit::threads;

use crate::pack::{self, Local};

/// The origin every file of the test's catalogue is at.
const ORIGIN: &str = "https://weights.example.org";

/// The adapter the test's entries name, and the revision the build states of it.
const ADAPTER: &str = "fake-adapter";

fn none() -> Secrets {
    Secrets::none()
}

/// A made-up key, long enough to be a secret, assembled so that no scan of this file
/// finds one. It is a model's name as well as anything else: lower-case letters,
/// digits and dashes.
fn made_up_key(tail: &str) -> String {
    ["made", "up", "key", "0123456789", tail].join("-")
}

fn at(file: &str) -> Address {
    Address::of(&format!("{ORIGIN}/files/{file}"), &none()).unwrap()
}

/// The bytes of a file of a model, named for both.
fn bytes_of(model: &str, file: &str, size: usize) -> Vec<u8> {
    let seed = format!("{model}/{file}");
    seed.bytes().cycle().take(size).collect()
}

/// An entry of the test's catalogue: these files, each of this size, at
/// [`ORIGIN`], for these platforms, under [`ADAPTER`].
fn entry(
    name: &str,
    title: &str,
    licence: &str,
    files: &[(&str, usize)],
    on: &[Platform],
) -> Entry {
    let files = files
        .iter()
        .map(|(file, size)| {
            let content = bytes_of(name, file, *size);
            CatalogueFile::new(
                FileName::new(file, &none()).unwrap(),
                content.len() as u64,
                Digest::of_bytes(&content),
                at(&format!("{name}/{file}")),
                &none(),
            )
            .unwrap()
        })
        .collect();
    let parts = EntryParts {
        name: ModelName::new(name, &none()).unwrap(),
        title: Title::new(title, &none()).unwrap(),
        revision: Revision::new("2026-11.1", &none()).unwrap(),
        adapter: ModelName::new(ADAPTER, &none()).unwrap(),
        licence: Licence::new(licence, &none()).unwrap(),
        platforms: on.to_vec(),
        features: Vec::new(),
        origins: vec![Origin::of(ORIGIN, &none()).unwrap()],
        files,
    };
    Entry::new(parts, &none()).unwrap()
}

/// `alpha`: two files, 2,500 bytes and 30, for both platforms.
fn alpha() -> Entry {
    entry(
        "alpha",
        "Alpha, for tests",
        "Apache-2.0",
        &[("graph.onnx", 2500), ("LICENSE.txt", 30)],
        &Platform::ALL,
    )
}

/// `beta`: one file of 1,200,000 bytes, for both platforms.
fn beta() -> Entry {
    entry(
        "beta",
        "Beta, for tests",
        "MIT",
        &[("weights.bin", 1_200_000)],
        &Platform::ALL,
    )
}

/// A fake of the download that serves every file of these entries.
fn serving(entries: &[Entry]) -> FakeDownloader {
    let mut fake = FakeDownloader::new();
    for entry in entries {
        for file in entry.files() {
            let content = bytes_of(
                entry.name().as_str(),
                file.name().as_str(),
                usize::try_from(file.size()).unwrap(),
            );
            fake = fake.serving(file.from(), content);
        }
    }
    fake
}

/// The adapters of the test's build: [`ADAPTER`], at revision 7.
fn adapters() -> Vec<(ModelName, Revision)> {
    vec![(
        ModelName::new(ADAPTER, &none()).unwrap(),
        Revision::new("7", &none()).unwrap(),
    )]
}

/// A clock a test moves: each pause is kept, and moves it by its span; and a test
/// may give it something to do while it pauses.
#[derive(Default)]
struct Hand {
    start: Option<Instant>,
    moved_ms: AtomicU64,
    paused: Mutex<Vec<Duration>>,
    meanwhile: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl Hand {
    fn new() -> Arc<Hand> {
        Arc::new(Hand {
            start: Some(Instant::now()),
            ..Hand::default()
        })
    }

    fn pauses(&self) -> Vec<Duration> {
        self.paused.lock().unwrap().clone()
    }

    fn while_paused(&self, look: impl Fn() + Send + Sync + 'static) {
        *self.meanwhile.lock().unwrap() = Some(Arc::new(look));
    }
}

impl Clock for Hand {
    fn now_ms(&self) -> u64 {
        self.moved_ms.load(Ordering::SeqCst)
    }

    fn now(&self) -> Instant {
        let moved = Duration::from_millis(self.moved_ms.load(Ordering::SeqCst));
        self.start.unwrap() + moved
    }

    fn pause(&self, span: Duration) {
        self.paused.lock().unwrap().push(span);
        let ms = u64::try_from(span.as_millis()).unwrap();
        self.moved_ms.fetch_add(ms, Ordering::SeqCst);
        let look = self.meanwhile.lock().unwrap().clone();
        if let Some(look) = look {
            look();
        }
        std::thread::yield_now();
    }
}

/// What the maker of the download client was asked for: each least rate it was
/// given, and the origins.
#[derive(Default)]
struct Made {
    least: Mutex<Vec<u64>>,
    origins: Mutex<Vec<Vec<String>>>,
}

/// The maker of the download client a test gives: a checked downloader, held to the
/// origins it is handed, around this fake.
fn downloads(fake: &Arc<FakeDownloader>, made: &Arc<Made>) -> Downloads {
    let (fake, made) = (fake.clone(), made.clone());
    Arc::new(move |least: u64, origins: &[Origin], _: &Secrets| {
        made.least.lock().unwrap().push(least);
        let named = origins.iter().map(ToString::to_string).collect();
        made.origins.lock().unwrap().push(named);
        CheckedDownloader::new(fake.clone(), origins)
    })
}

/// A scratch folder, and in it the place of a models directory that nothing has
/// made yet, and a config file's place.
struct Shelf {
    dir: Scratch,
}

impl Shelf {
    fn new(label: &str) -> Shelf {
        Shelf {
            dir: Scratch::new(label),
        }
    }

    fn models_dir(&self) -> PathBuf {
        self.dir.path().join("models")
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// The test's catalogue of these entries, under the test's adapters, with this
    /// shelf's models directory named by the flag.
    fn models(&self, entries: &[Entry]) -> Models {
        Models::of(
            Catalogue::new(entries.to_vec(), &none()).unwrap(),
            adapters(),
        )
        .flag(Some(&self.models_dir()))
    }

    /// What the binary's functions are given: these models, this clock, and a fake
    /// of the download that serves these entries' files.
    fn tools(&self, models: Models, clock: &Arc<Hand>, fake: &Arc<FakeDownloader>) -> Tools {
        Tools::of(models, clock.clone(), downloads(fake, &Arc::default()))
    }
}

/// `list`, with no config.
fn list(tools: &Tools) -> String {
    model_commands::list(Under::default(), tools).expect("the models are listed")
}

/// `get`, with no config, keeping what it says as it goes.
fn get(tools: &Tools, model: &str) -> (Result<String, String>, Vec<String>) {
    let mut said = Vec::new();
    let got = model_commands::get(Under::default(), model, tools, &mut |line| {
        said.push(line.to_owned());
    });
    (got, said)
}

/// The line `list` prints of this model.
fn line_of(listed: &str, model: &str) -> String {
    let found = listed
        .lines()
        .find(|line| line.split('\t').next() == Some(model));
    found
        .unwrap_or_else(|| panic!("no line of {model} in:\n{listed}"))
        .to_owned()
}

/// The state `list` gives this model: the last column of its line.
fn state_of(listed: &str, model: &str) -> String {
    line_of(listed, model)
        .rsplit('\t')
        .next()
        .unwrap()
        .to_owned()
}

/// The identity of the installation `get` reported.
fn identity_in(got: &str, model: &str) -> String {
    let prefix = format!("installed {model}: ");
    let rest = got.strip_prefix(&prefix).unwrap_or_else(|| panic!("{got}"));
    rest.trim_end().to_owned()
}

/// Every name under this folder, as paths below it.
fn everything_under(folder: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, found: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            found.push(path.strip_prefix(root).unwrap().display().to_string());
            if path.is_dir() && !path.is_symlink() {
                walk(root, &path, found);
            }
        }
    }
    let mut found = Vec::new();
    walk(folder, folder, &mut found);
    found.sort();
    found
}

/// The real binary, with a cleared environment and a temporary home, kept until the
/// program has been waited for.
fn binary() -> whizbang_testkit::binary::Binary {
    whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang"))
}

/// The guide shows this text as a command prints it: each line of it is a line of
/// the guide (`docs/guides/models.md`), whole, in the same order.
fn in_the_guide(text: &str) {
    let guide = include_str!("../../../../docs/guides/models.md");
    let lines: Vec<&str> = guide.lines().collect();
    let wanted: Vec<&str> = text.lines().collect();
    let found = lines
        .windows(wanted.len())
        .any(|window| window == wanted.as_slice());
    assert!(found, "the guide does not show:\n{text}");
}

/// The platform this build is not for: the other of the two a catalogue can name.
fn elsewhere() -> Platform {
    match Platform::this() {
        Some(Platform::MacosArm64) => Platform::LinuxX86_64,
        _ => Platform::MacosArm64,
    }
}

// --- list ----------------------------------------------------------------------------------------

#[test]
fn list_names_each_model_of_the_catalogue_with_its_title_licence_size_platforms_and_state() {
    let shelf = Shelf::new("models-list");
    let clock = Hand::new();
    let fake = Arc::new(serving(&[alpha(), beta()]));
    let tools = shelf.tools(shelf.models(&[alpha(), beta()]), &clock, &fake);
    // Before anything is installed: the directory is not there, and listing does
    // not make it. Each model is not installed.
    let listed = list(&tools);
    assert_eq!(
        listed,
        "models: 2\n\
         directory: the one --models-dir names, not made yet\n\
         alpha\tAlpha, for tests\tApache-2.0\t2.5 KB\tlinux-x86_64,macos-arm64\tnot installed\n\
         beta\tBeta, for tests\tMIT\t1.2 MB\tlinux-x86_64,macos-arm64\tnot installed\n"
    );
    assert!(!shelf.models_dir().exists());
    // One installed: its state says so, and the other's does not move.
    let (got, _) = get(&tools, "alpha");
    got.unwrap();
    let listed = list(&tools);
    assert_eq!(
        listed,
        "models: 2\n\
         directory: the one --models-dir names\n\
         alpha\tAlpha, for tests\tApache-2.0\t2.5 KB\tlinux-x86_64,macos-arm64\tinstalled\n\
         beta\tBeta, for tests\tMIT\t1.2 MB\tlinux-x86_64,macos-arm64\tnot installed\n"
    );
    in_the_guide(&listed);
    // The size of each: under a thousand bytes, then thousands, millions and
    // thousands of millions, with one figure after the point.
    for (bytes, shown) in [
        (999, "999 B"),
        (1000, "1.0 KB"),
        (2530, "2.5 KB"),
        (1_200_000, "1.2 MB"),
        (3_440_000_000, "3.4 GB"),
    ] {
        assert_eq!(model_commands::size(bytes), shown);
    }
    // A catalogue that holds no model says so, and opens nothing.
    let empty = shelf.tools(
        Models::of(Catalogue::new(Vec::new(), &none()).unwrap(), adapters())
            .flag(Some(&shelf.path("never"))),
        &clock,
        &fake,
    );
    assert_eq!(
        list(&empty),
        "models: 0: the catalogue holds no model yet\n"
    );
    assert!(!shelf.path("never").exists());
    // What a damaged installation is listed as.
    std::fs::write(
        shelf.models_dir().join("models/alpha/CURRENT"),
        b"not a pointer",
    )
    .unwrap();
    assert_eq!(
        state_of(&list(&tools), "alpha"),
        "damaged: get installs it again"
    );
}

/// What the shipped catalogue's `sam3` is said to be on this platform when nothing is
/// installed: on the Mac, not installed and nothing more, since its adapter ships
/// (task R2a); on a platform its entry does not name, not ready for that.
fn sam3_not_installed() -> String {
    if Platform::this() == Some(Platform::MacosArm64) {
        "not installed".to_owned()
    } else {
        format!("not installed; not ready: {}", Unready::NotForThisPlatform)
    }
}

/// What `get sam3` of the real binary is refused with, under a config that names no
/// token for the Hub, on this platform: the token first on the platform its entry
/// names, and the platform elsewhere.
fn sam3_get_refused() -> &'static str {
    if Platform::this() == Some(Platform::MacosArm64) {
        "a download of the model from https://huggingface.co needs a token, and the config \
         names none for that origin, so the model was not installed and nothing was downloaded."
    } else {
        "the model does not run on this platform (its entry names macos-arm64), so it was not \
         installed and nothing was downloaded"
    }
}

#[test]
fn under_the_catalogue_it_ships_no_models_command_makes_a_models_directory_or_asks_anything_of_any_address()
 {
    // The real binary, with the catalogue it ships, whose one model is `sam3` (task
    // N5): `list` names it, not installed (and ready to be, since task R2a ships its
    // adapter, on the Mac; elsewhere not ready for its platform); `get` is refused before
    // anything is asked of any address, since no config here names the Hub's token
    // (and, on a platform its entry does not name, for that first); `remove` finds
    // it not installed. None of the three makes a models directory, whatever names
    // one; the platform's would be in the function's temporary home, which stays
    // empty. So no test that starts the binary contacts any host.
    let shelf = Shelf::new("models-shipped");
    let config = shelf.path("whizbang.toml");
    let configured = shelf.path("configured");
    std::fs::write(
        &config,
        format!("[models]\ndir = '{}'\n", configured.to_str().unwrap()),
    )
    .unwrap();
    let flagged = shelf.models_dir();
    let flag = flagged.to_str().unwrap();
    let config = config.to_str().unwrap();
    let not_removed = "the model was not removed: the model is not installed";
    let platforms = "the one in this machine's data folder for whizbang";
    let runs: [(&[&str], i32, &str); 9] = [
        (&["models", "list"], 0, platforms),
        (
            &["models", "list", "--models-dir", flag],
            0,
            "the one --models-dir names",
        ),
        (
            &["models", "list", "--config", config],
            0,
            "the one the config's `models.dir` names",
        ),
        (&["models", "get", "sam3"], 1, sam3_get_refused()),
        (
            &["models", "get", "sam3", "--models-dir", flag],
            1,
            sam3_get_refused(),
        ),
        (
            &["models", "get", "sam3", "--config", config],
            1,
            sam3_get_refused(),
        ),
        (&["models", "remove", "sam3"], 1, not_removed),
        (
            &["models", "remove", "sam3", "--models-dir", flag],
            1,
            not_removed,
        ),
        (
            &[
                "models", "remove", "sam3", "--wait", "1", "--config", config,
            ],
            1,
            not_removed,
        ),
    ];
    for (args, code, said) in runs {
        let mut command = binary();
        let home = command.home().to_path_buf();
        let out = command.args(args).output().expect("the binary runs");
        assert_eq!(out.status.code(), Some(code), "{args:?}: {out:?}");
        let (stdout, stderr) = (
            String::from_utf8(out.stdout).unwrap(),
            String::from_utf8(out.stderr).unwrap(),
        );
        if code == 0 {
            // And `tiny` (plan 1D, task T1), which needs no token.
            let listed = format!(
                "models: 2\n\
                 directory: {said}, not made yet\n\
                 sam3\tSAM 3\tSAM License\t3.4 GB\tmacos-arm64\t{0}\n\
                 tiny\tTiny (OWLv2 and SAM 2.1 Hiera-Tiny)\tApache-2.0\t777.4 MB\tmacos-arm64\t{0}\n",
                sam3_not_installed()
            );
            assert_eq!(stdout, listed);
            assert_eq!(stderr, "");
            if said == platforms && Platform::this() == Some(Platform::MacosArm64) {
                in_the_guide(&format!("$ whizbang models list\n{stdout}"));
            }
        } else {
            assert_eq!(stdout, "");
            assert!(stderr.starts_with(&format!("whizbang: {said}")), "{stderr}");
        }
        assert!(everything_under(&home).is_empty(), "{args:?}");
    }
    assert!(!flagged.exists() && !configured.exists());
}

#[test]
fn a_relative_models_dir_is_refused_before_anything_is_made() {
    // From a working folder of the test's own: a relative path named there is
    // refused by every command that takes the flag, and nothing is made in it.
    let shelf = Shelf::new("models-relative");
    let working = shelf.path("working");
    std::fs::create_dir(&working).unwrap();
    let store = shelf.path("store");
    let mask = format!("sha256:{}", "0".repeat(64));
    let runs: [&[&str]; 5] = [
        &["models", "list", "--models-dir", "kept"],
        &["models", "get", "sam3", "--models-dir", "kept"],
        &["models", "remove", "sam3", "--models-dir", "./kept"],
        &["models", "list", "--models-dir", "kept/models"],
        &[
            "rascal",
            "grow",
            "--mask",
            &mask,
            "--px",
            "1",
            "--models-dir",
            "kept",
            "--store",
        ],
    ];
    for args in runs {
        let mut command = binary();
        command.args(args).current_dir(&working);
        if args[0] == "rascal" {
            command.arg(&store);
        }
        let out = command.output().expect("the binary runs");
        assert_eq!(out.status.code(), Some(1), "{args:?}: {out:?}");
        let said = String::from_utf8(out.stderr).unwrap();
        assert_eq!(said, format!("whizbang: {}\n", wiring::RELATIVE_MODELS_DIR));
        assert!(everything_under(&working).is_empty(), "{args:?}");
    }
    // And through the wiring a command's executor is built over, the same, under a
    // catalogue that holds a model and before the config is read.
    let models = shelf.models(&[alpha()]).flag(Some(Path::new("kept")));
    let refused = wiring::Over::nothing()
        .config(Some(&shelf.path("no-such.toml")))
        .installed(models)
        .unwrap_err();
    assert_eq!(refused, wiring::RELATIVE_MODELS_DIR);
    assert!(!store.exists());
}

// --- get -----------------------------------------------------------------------------------------

#[test]
fn get_installs_a_model_and_list_then_says_it_is_installed() {
    let shelf = Shelf::new("models-get");
    let clock = Hand::new();
    let fake = Arc::new(serving(&[alpha(), beta()]));
    let tools = shelf.tools(shelf.models(&[alpha(), beta()]), &clock, &fake);
    assert_eq!(state_of(&list(&tools), "alpha"), "not installed");
    let (got, said) = get(&tools, "alpha");
    let got = got.unwrap();
    // It says which installation it made, and nothing as it went: no other was
    // installing the model, so it waited for nothing and did not pause.
    let identity = identity_in(&got, "alpha");
    assert!(
        identity.starts_with("sha256:") && identity.len() == 71,
        "{got}"
    );
    assert!(said.is_empty(), "{said:?}");
    assert!(clock.pauses().is_empty(), "{:?}", clock.pauses());
    // Each file was asked for once, and only those of the model it was asked for.
    for file in alpha().files() {
        assert_eq!(fake.asked(file.from()), 1);
    }
    assert_eq!(fake.requests().len(), 2);
    // The installation is the one the inventory finds ready.
    let installed = Installations::of(
        ModelsDir::at(Arc::new(RealFs), &shelf.models_dir()),
        Catalogue::new(vec![alpha()], &none()).unwrap(),
        adapters(),
        none(),
    );
    let record = installed.ready("alpha").unwrap();
    assert_eq!(record.identity().digest().to_string(), identity);
    assert_eq!(state_of(&list(&tools), "alpha"), "installed");
    assert_eq!(state_of(&list(&tools), "beta"), "not installed");
    // The directory was made for its owner only.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(shelf.models_dir())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
    }
    // A second `get` of an installed model downloads nothing and says the same.
    let (again, _) = get(&tools, "alpha");
    assert_eq!(again.unwrap(), got);
    assert_eq!(fake.requests().len(), 2);
}

#[test]
fn the_catalogues_sam3_declares_phrase_point_and_box() {
    // Task Q1 of checkpoint 1D: SAM 3 answers a point and a box from the same files, so
    // its entry declares the three features at a new revision of the catalogue, and the
    // files, their sizes and their hashes are those of the revision before.
    let catalogue = wiring::catalogue().unwrap();
    let sam3 = catalogue.entry("sam3").unwrap();
    assert_eq!(
        sam3.features().collect::<Vec<_>>(),
        ["box", "phrase", "point"]
    );
    assert_eq!(sam3.revision().as_str(), "2026-11.2");
    let weights = sam3
        .files()
        .iter()
        .find(|file| file.name().as_str() == "model.safetensors")
        .unwrap();
    assert_eq!(
        weights.sha256().to_string(),
        "sha256:6d06f0a5f84e435071fe6603e61d0b4cc7b40e0d39d487cfd4d67d8cc11cc14a"
    );
    // The adapter the pack ships declares the same three.
    let adapter = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pack/src/whizbang_pack/sam3.py"),
    )
    .unwrap();
    assert!(adapter.contains(r#"features = frozenset(("phrase", "point", "box"))"#));
}

#[test]
fn the_catalogues_tiny_is_pinned_licensed_and_needs_no_token() {
    // Task T1 of checkpoint 1D: OWLv2 and SAM 2.1 Hiera-Tiny, run by `owlv2-sam2`, on
    // the Mac alone (as the lock is), from two public repositories, each at a commit.
    let catalogue = wiring::catalogue().unwrap();
    let tiny = catalogue.entry("tiny").unwrap();
    assert_eq!(tiny.adapter().as_str(), "owlv2-sam2");
    assert_eq!(tiny.licence().as_str(), "Apache-2.0");
    assert_eq!(tiny.platforms(), [Platform::MacosArm64]);
    assert_eq!(
        tiny.features().collect::<Vec<_>>(),
        ["box", "phrase", "point"]
    );
    // Public: no download needs a token, and the origins are those approved for the Hub.
    assert_eq!(tiny.needs_token(), None);
    let origins: Vec<String> = tiny.origins().iter().map(ToString::to_string).collect();
    assert_eq!(
        origins,
        ["https://huggingface.co", "https://us.aws.cdn.hf.co"]
    );
    // Each file under its model's prefix, first asked for at its repository's own path
    // at one commit; the model cards, which state the licence, installed beside them.
    let repositories = [
        (
            "owlv2.",
            "/google/owlv2-base-patch16/resolve/2a1560802f8cf3c408fec9b809d705f56a2f7146/",
        ),
        (
            "sam2.",
            "/facebook/sam2.1-hiera-tiny/resolve/de431c4043854a71d8101e17995dfe596bf101a5/",
        ),
    ];
    let mut names = Vec::new();
    for file in tiny.files() {
        let name = file.name().as_str();
        let (prefix, path) = repositories
            .iter()
            .find(|(prefix, _)| name.starts_with(prefix))
            .unwrap_or_else(|| panic!("{name} is under no model's prefix"));
        let from = file.from();
        assert_eq!(from.origin().to_string(), "https://huggingface.co");
        assert_eq!(from.rest(), format!("{path}{}", &name[prefix.len()..]));
        names.push(name);
    }
    assert_eq!(
        names,
        [
            "owlv2.README.md",
            "owlv2.added_tokens.json",
            "owlv2.config.json",
            "owlv2.merges.txt",
            "owlv2.model.safetensors",
            "owlv2.preprocessor_config.json",
            "owlv2.special_tokens_map.json",
            "owlv2.tokenizer_config.json",
            "owlv2.vocab.json",
            "sam2.README.md",
            "sam2.config.json",
            "sam2.model.safetensors",
            "sam2.preprocessor_config.json",
        ]
    );
    // The weights, by the size and hash the Hub's listing of each commit states.
    let weights = |name: &str| {
        let file = tiny.files().iter().find(|f| f.name().as_str() == name);
        file.map(|f| (f.size(), f.sha256().to_string())).unwrap()
    };
    assert_eq!(
        weights("owlv2.model.safetensors"),
        (
            619_918_824,
            "sha256:c4a566429e6b2c015abacb53087bf673260486c369a84e626e9b09fe57ca316d".to_owned()
        )
    );
    assert_eq!(
        weights("sam2.model.safetensors"),
        (
            155_908_064,
            "sha256:48c14467e5cf9e51870511feb72c89688e82dd74523142c0538b663e193ac2a7".to_owned()
        )
    );
    // The adapter is shipped, at the revision its module states, and the module of
    // the pack declares the same three features.
    let table = wiring::adapters();
    let adapter = table
        .iter()
        .find(|adapter| adapter.name.as_str() == "owlv2-sam2")
        .unwrap();
    assert!(adapter.shipped);
    let module = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pack/src/whizbang_pack/owlv2_sam2.py"),
    )
    .unwrap();
    assert!(module.contains(&format!("REVISION = \"{}\"", adapter.revision.as_str())));
    assert!(module.contains(r#"features = frozenset(("phrase", "point", "box"))"#));
}

/// An entry of this name that holds `alpha`'s files, byte for byte, at addresses of its
/// own, and these more, and needs a token for a download from [`ORIGIN`].
fn holding_alphas_files(name: &str, more: &[(&str, &[u8])]) -> Entry {
    let alpha = alpha();
    let mut files: Vec<CatalogueFile> = alpha
        .files()
        .iter()
        .map(|file| {
            let from = at(&format!("{name}/{}", file.name().as_str()));
            CatalogueFile::new(
                file.name().clone(),
                file.size(),
                file.sha256(),
                from,
                &none(),
            )
            .unwrap()
        })
        .collect();
    for (file, content) in more {
        let from = at(&format!("{name}/{file}"));
        let named = FileName::new(file, &none()).unwrap();
        let size = content.len() as u64;
        let made = CatalogueFile::new(named, size, Digest::of_bytes(content), from, &none());
        files.push(made.unwrap());
    }
    let parts = EntryParts {
        name: ModelName::new(name, &none()).unwrap(),
        title: Title::new("Holds alpha's files", &none()).unwrap(),
        revision: Revision::new("2026-11.2", &none()).unwrap(),
        adapter: ModelName::new(ADAPTER, &none()).unwrap(),
        licence: Licence::new("Apache-2.0", &none()).unwrap(),
        platforms: Platform::ALL.to_vec(),
        features: Vec::new(),
        origins: vec![Origin::of(ORIGIN, &none()).unwrap()],
        files,
    };
    Entry::new(parts, &none())
        .unwrap()
        .needing_a_token_at(Origin::of(ORIGIN, &none()).unwrap())
        .unwrap()
}

#[test]
fn an_install_whose_files_are_all_held_asks_for_no_token() {
    let shelf = Shelf::new("models-held-no-token");
    let clock = Hand::new();
    let fake = Arc::new(serving(&[alpha()]));
    let gamma = holding_alphas_files("gamma", &[]);
    // `delta` holds one file more, which nothing holds.
    let delta = holding_alphas_files("delta", &[("extra.bin", b"new")]);
    let entries = [alpha(), gamma.clone(), delta];
    let tools = shelf.tools(shelf.models(&entries), &clock, &fake);
    // Before anything holds its files, `gamma` needs its token, which the config does
    // not name: refused, and nothing is downloaded.
    let (refused, _) = get(&tools, "gamma");
    assert!(refused.unwrap_err().contains("needs a token"));
    assert!(fake.requests().is_empty());
    get(&tools, "alpha").0.unwrap();
    assert_eq!(fake.requests().len(), 2);
    // Now every file of `gamma` is held by `alpha`'s installation: no token is asked
    // for, nothing is downloaded, and `gamma` is installed of the same files.
    let (got, _) = get(&tools, "gamma");
    let got = got.unwrap();
    assert!(got.starts_with("installed gamma: sha256:"), "{got}");
    assert_eq!(fake.requests().len(), 2);
    let installed = Installations::of(
        ModelsDir::at(Arc::new(RealFs), &shelf.models_dir()),
        Catalogue::new(entries.to_vec(), &none()).unwrap(),
        adapters(),
        none(),
    );
    let record = installed.ready("gamma").unwrap();
    assert_eq!(
        record.identity().digest().to_string(),
        identity_in(&got, "gamma")
    );
    // A file that nothing holds is to be downloaded: the token is asked for, and the
    // install is refused for want of it before anything is asked.
    let (refused, _) = get(&tools, "delta");
    assert!(refused.unwrap_err().contains("needs a token"));
    assert_eq!(fake.requests().len(), 2);
}

#[test]
fn get_makes_the_download_client_with_the_configs_least_rate_or_the_default() {
    // The maker of the download client is given the config's least rate of a
    // download, or the default when the config gives none or there is no config,
    // and the origins the entry names and no other. The client's own tests show
    // a slow download ended by the rate it was made with (task N1).
    let shelf = Shelf::new("models-least");
    let clock = Hand::new();
    let fake = Arc::new(serving(&[alpha(), beta()]));
    let made = Arc::new(Made::default());
    let tools = Tools::of(
        shelf.models(&[alpha(), beta()]),
        clock.clone(),
        downloads(&fake, &made),
    );
    let config = shelf.path("whizbang.toml");
    std::fs::write(&config, "[models]\ndownload_least_kib_per_second = 7\n").unwrap();
    let configured = Under {
        config: Some(&config),
        env_file: None,
    };
    model_commands::get(configured, "alpha", &tools, &mut |_| {}).unwrap();
    std::fs::write(&config, "").unwrap();
    model_commands::get(configured, "beta", &tools, &mut |_| {}).unwrap();
    model_commands::get(Under::default(), "alpha", &tools, &mut |_| {}).unwrap();
    assert_eq!(
        *made.least.lock().unwrap(),
        [
            7,
            DOWNLOAD_LEAST_KIB_PER_SECOND,
            DOWNLOAD_LEAST_KIB_PER_SECOND
        ]
    );
    assert_eq!(DOWNLOAD_LEAST_KIB_PER_SECOND, 256);
    for origins in made.origins.lock().unwrap().iter() {
        assert_eq!(origins, &[ORIGIN.to_owned()]);
    }
}

#[test]
fn a_second_get_attaches_to_the_first_waits_by_the_clock_and_ends_with_the_same_installation() {
    let shelf = Shelf::new("models-attach");
    let models = shelf.models(&[alpha()]);
    // The first `get`'s download stops inside the second file it stages, with the
    // first one staged, until the test lets it go. The entry keeps its files in the
    // order of their names: `LICENSE.txt`, then `graph.onnx`.
    let barriers = Barriers::new();
    let entry = alpha();
    let file = |name: &str| {
        let found = entry
            .files()
            .iter()
            .find(|file| file.name().as_str() == name);
        found.unwrap().from().clone()
    };
    assert_eq!(entry.files()[1].name().as_str(), "graph.onnx");
    let first = Arc::new(
        FakeDownloader::new()
            .serving(&file("LICENSE.txt"), bytes_of("alpha", "LICENSE.txt", 30))
            .stopping_after(
                &file("graph.onnx"),
                bytes_of("alpha", "graph.onnx", 2500),
                1024,
                barriers.clone(),
                "mid",
            ),
    );
    let installer = {
        let tools = shelf.tools(models.clone(), &Hand::new(), &first);
        threads::spawn(move || get(&tools, "alpha").0)
    };
    barriers.arrived("mid", 1);
    // The second, while the first is stopped: it downloads nothing, says what the
    // status record shows, waits by its clock between two looks, and lets the
    // first go once it has said that.
    let clock = Hand::new();
    let second = Arc::new(FakeDownloader::new());
    let tools = shelf.tools(models, &clock, &second);
    let mut said = Vec::new();
    let attached = model_commands::get(Under::default(), "alpha", &tools, &mut |line| {
        if said.is_empty() {
            barriers.release("mid");
        }
        said.push(line.to_owned());
    });
    // Whatever the second answered, the first is not left stopped.
    barriers.open("mid");
    let installed = installer.join().unwrap();
    let installed = installed.unwrap();
    assert_eq!(attached.unwrap(), installed);
    assert_eq!(
        said[0],
        "another request is installing the model: 1 of 2 files staged"
    );
    in_the_guide(&format!("whizbang: {}", said[0]));
    // A line is said when what is shown changes, and not at each look.
    for pair in said.windows(2) {
        assert_ne!(pair[0], pair[1], "{said:?}");
    }
    assert!(second.requests().is_empty());
    for file in alpha().files() {
        assert_eq!(first.asked(file.from()), 1);
    }
    let pauses = clock.pauses();
    assert!(!pauses.is_empty());
    assert!(pauses.iter().all(|span| *span == PAUSE), "{pauses:?}");
    assert_eq!(PAUSE, Duration::from_millis(250));
    assert_eq!(state_of(&list(&tools), "alpha"), "installed");
}

// --- pin -----------------------------------------------------------------------------------------

/// A draft of one entry, `drafted`, at [`ORIGIN`]: these files, each with its bound
/// as its size and a hash of zeros, at the address of the test's catalogue.
fn draft(files: &[(&str, u64)]) -> String {
    let files: Vec<String> = files
        .iter()
        .map(|(name, bound)| {
            format!(
                "{{\"name\":\"{name}\",\"size\":{bound},\"sha256\":\"sha256:{}\",\
                 \"from\":\"{ORIGIN}/files/drafted/{name}\"}}",
                "0".repeat(64)
            )
        })
        .collect();
    format!(
        "{{\"format\":\"catalogue/2\",\"models\":[{{\"name\":\"drafted\",\"title\":\"A draft\",\
         \"revision\":\"1\",\"adapter\":\"{ADAPTER}\",\"licence\":\"MIT\",\
         \"platforms\":[\"macos-arm64\"],\"features\":[],\"origins\":[\"{ORIGIN}\"],\
         \"files\":[{}]}}]}}",
        files.join(",")
    )
}

#[test]
fn pin_prints_the_size_and_hash_of_each_file_it_fetched_through_the_checked_downloader_and_installs_nothing()
 {
    // Plan 1C, task N5: `models pin <draft>` fetches each file of a draft entry once,
    // through the checked downloader held to the draft's origins, and prints its size
    // and SHA-256. A draft's size is the most bytes wanted. Nothing is installed or
    // written: no models directory is made.
    let shelf = Shelf::new("models-pin");
    let clock = Hand::new();
    let (graph, licence, big) = (
        bytes_of("drafted", "graph.onnx", 2500),
        bytes_of("drafted", "LICENSE.txt", 30),
        bytes_of("drafted", "big.bin", 20),
    );
    let storage = Address::of("https://storage.example.net/x?sig=abc", &none()).unwrap();
    let fake = Arc::new(
        FakeDownloader::new()
            .serving(&at("drafted/graph.onnx"), graph.clone())
            .serving(&at("drafted/LICENSE.txt"), licence.clone())
            .serving(&at("drafted/big.bin"), big)
            .moved(&at("drafted/weights.bin"), storage.clone())
            .serving(&storage, b"never".to_vec()),
    );
    let made = Arc::new(Made::default());
    let tools = Tools::of(shelf.models(&[]), clock.clone(), downloads(&fake, &made));
    let path = shelf.path("draft.json");
    let files = [
        ("graph.onnx", 4000),
        ("LICENSE.txt", 30),
        ("big.bin", 10),
        ("weights.bin", 100),
    ];
    std::fs::write(&path, draft(&files)).unwrap();
    let said = model_commands::pin(Under::default(), &path, &tools).unwrap_err();
    assert_eq!(
        said,
        format!(
            "pinned drafted: nothing was installed or written\n\
             LICENSE.txt\t30\t{}\n\
             big.bin\tnot pinned: more than its bound of 10 bytes came\n\
             graph.onnx\t2500\t{}\n\
             weights.bin\tnot pinned: rejected: the origin is not one this download may \
             reach; it was moved to https://storage.example.net, which the draft does not \
             name, and nothing was asked there\n",
            Digest::of_bytes(&licence),
            Digest::of_bytes(&graph),
        )
    );
    // Through a checked downloader the maker made for the draft's origins alone; the
    // address off them was not asked, and nothing of it but its origin is printed.
    assert_eq!(*made.origins.lock().unwrap(), [vec![ORIGIN.to_owned()]]);
    assert_eq!(fake.asked(&storage), 0);
    assert!(!said.contains("sig=abc"));
    // A draft whose files all end within their bounds: the same lines, and done.
    std::fs::write(&path, draft(&files[..2])).unwrap();
    let pinned = model_commands::pin(Under::default(), &path, &tools).unwrap();
    assert!(pinned.ends_with(&format!("graph.onnx\t2500\t{}\n", Digest::of_bytes(&graph))));
    // Nothing was installed or written: no models directory, nothing beside the draft.
    assert!(!shelf.models_dir().exists());
    assert_eq!(everything_under(shelf.dir.path()), ["draft.json"]);
    // What is not a draft of one entry is refused, before anything is asked.
    let asked = fake.requests().len();
    std::fs::write(&path, "{\"format\":\"catalogue/2\",\"models\":[]}").unwrap();
    let refused = model_commands::pin(Under::default(), &path, &tools).unwrap_err();
    assert_eq!(refused, "the draft holds 0 models, and pin takes one");
    let refused = model_commands::pin(Under::default(), shelf.dir.path(), &tools).unwrap_err();
    assert!(refused.starts_with("the draft cannot be read"), "{refused}");
    assert_eq!(fake.requests().len(), asked);
}

// --- the table of adapters (task N5) ------------------------------------------------------------

mod inventory {
    use super::*;

    #[test]
    fn a_model_whose_adapter_the_binarys_table_names_as_not_yet_shipped_is_not_ready_for_want_of_an_adapter_installed_or_not()
     {
        // Plan 1C, task N5: the table fixes the revision of an adapter the pack does
        // not ship yet. `get` installs the model under it; the inventory calls the
        // model not ready, for want of an adapter, before the install and after it;
        // and `list` says it is installed and not ready, for that. Once the table
        // names the adapter as shipped, under the same revision (task R2a), the same
        // installation is ready: nothing is downloaded again.
        let shelf = Shelf::new("models-not-yet-shipped");
        let clock = Hand::new();
        let fake = Arc::new(serving(&[alpha()]));
        let catalogue = Catalogue::new(vec![alpha()], &none()).unwrap();
        let not_yet = Models::of(catalogue.clone(), Vec::new())
            .not_yet_shipped(adapters())
            .flag(Some(&shelf.models_dir()));
        let tools = shelf.tools(not_yet, &clock, &fake);
        let inventory = |shipped: Vec<(ModelName, Revision)>| {
            Installations::of(
                ModelsDir::at(Arc::new(RealFs), &shelf.models_dir()),
                catalogue.clone(),
                shipped,
                none(),
            )
        };
        let no_adapter =
            |installed: Installed| matches!(installed, Installed::NotReady(Unready::NoAdapter));
        assert!(no_adapter(inventory(Vec::new()).look("alpha")));
        assert_eq!(
            state_of(&list(&tools), "alpha"),
            "not installed; not ready: this build has no adapter for it"
        );
        let (got, _) = get(&tools, "alpha");
        let identity = identity_in(&got.unwrap(), "alpha");
        assert!(no_adapter(inventory(Vec::new()).look("alpha")));
        assert_eq!(
            state_of(&list(&tools), "alpha"),
            "installed, not ready: this build has no adapter for it"
        );
        // Shipped under the same revision: ready, as the same installation, and
        // nothing more was asked for.
        let asked = fake.requests().len();
        let record = inventory(adapters()).ready("alpha").unwrap();
        assert_eq!(record.identity().digest().to_string(), identity);
        let shipped = shelf.tools(shelf.models(&[alpha()]), &clock, &fake);
        assert_eq!(state_of(&list(&shipped), "alpha"), "installed");
        let (again, _) = get(&shipped, "alpha");
        assert_eq!(identity_in(&again.unwrap(), "alpha"), identity);
        assert_eq!(fake.requests().len(), asked);
        // The binary's own table names `sam3` at the revision of the adapter that takes
        // points and boxes (task Q1), shipped; and `owlv2-sam2` beside it (task T1).
        let table = wiring::adapters();
        assert_eq!(table.len(), 2);
        assert_eq!(
            (
                table[1].name.as_str(),
                table[1].revision.as_str(),
                table[1].shipped
            ),
            ("owlv2-sam2", "1", true)
        );
        assert_eq!(
            (
                table[0].name.as_str(),
                table[0].revision.as_str(),
                table[0].shipped
            ),
            ("sam3", "2", true)
        );
    }
}

// --- remove --------------------------------------------------------------------------------------

#[test]
fn remove_drains_and_answers_busy_at_its_wait_with_the_model_still_installed() {
    let shelf = Shelf::new("models-remove");
    let clock = Hand::new();
    let fake = Arc::new(serving(&[alpha()]));
    let tools = Arc::new(shelf.tools(shelf.models(&[alpha()]), &clock, &fake));
    // Not installed, and nothing there: refused, and nothing is made.
    let refused = model_commands::remove(Under::default(), "alpha", Duration::ZERO, &tools);
    assert_eq!(
        refused.unwrap_err(),
        "the model was not removed: the model is not installed"
    );
    assert!(!shelf.models_dir().exists());
    let (got, _) = get(&tools, "alpha");
    let identity = identity_in(&got.unwrap(), "alpha");
    // A run holds the installation, as a lease does while a request is out.
    let installed = Installations::of(
        ModelsDir::at(Arc::new(RealFs), &shelf.models_dir()),
        Catalogue::new(vec![alpha()], &none()).unwrap(),
        adapters(),
        none(),
    );
    let digest = Digest::from_hex(identity.strip_prefix("sha256:").unwrap()).unwrap();
    let hold = installed.hold("alpha", Installation::of(digest)).unwrap();
    // While the removal waits for it, `list` says the model is being removed:
    // its remover is alive and its mark is set.
    let seen = Arc::new(Mutex::new(Vec::new()));
    {
        let (seen, tools) = (seen.clone(), tools.clone());
        clock.while_paused(move || {
            seen.lock().unwrap().push(state_of(&list(&tools), "alpha"));
        });
    }
    let refused = model_commands::remove(Under::default(), "alpha", Duration::from_secs(2), &tools);
    let refused = refused.unwrap_err();
    assert_eq!(
        refused,
        "the model was not removed: the model is held by a run, which did not let it go in \
         time; nothing was removed, and the model is as it was"
    );
    in_the_guide(&format!("whizbang: {refused}"));
    // It waited by its clock, a pause at a time, to its wait and no longer.
    let pauses = clock.pauses();
    assert!(pauses.iter().all(|span| *span == PAUSE), "{pauses:?}");
    assert_eq!(pauses.len(), 8, "{pauses:?}");
    let seen = std::mem::take(&mut *seen.lock().unwrap());
    assert!(!seen.is_empty());
    assert!(
        seen.iter().all(|state| state == "being removed"),
        "{seen:?}"
    );
    clock.while_paused(|| {});
    // Still installed, under the same installation, which is still held.
    assert_eq!(state_of(&list(&tools), "alpha"), "installed");
    assert_eq!(
        installed
            .ready("alpha")
            .unwrap()
            .identity()
            .digest()
            .to_string(),
        identity
    );
    // Let go, the model is removed: drained, and gone.
    hold.let_go();
    let removed = model_commands::remove(Under::default(), "alpha", Duration::from_secs(2), &tools);
    assert_eq!(removed.unwrap(), format!("removed alpha: {identity}\n"));
    assert_eq!(state_of(&list(&tools), "alpha"), "not installed");
    // Asked again, it is not installed.
    let again = model_commands::remove(Under::default(), "alpha", Duration::ZERO, &tools);
    assert_eq!(
        again.unwrap_err(),
        "the model was not removed: the model is not installed"
    );
}

// --- what list says, and what get refuses -------------------------------------------------------

#[test]
fn a_model_whose_adapter_this_build_lacks_is_listed_with_that_reason() {
    let shelf = Shelf::new("models-no-adapter");
    let clock = Hand::new();
    let fake = Arc::new(serving(&[alpha(), beta()]));
    // `alpha` is installed by a build that had the adapter.
    let had = shelf.tools(shelf.models(&[alpha(), beta()]), &clock, &fake);
    get(&had, "alpha").0.unwrap();
    // This build has none: the installed model is installed and not ready, the
    // other not installed, and each says why. No install could make either ready,
    // so `get` downloads nothing.
    let lacking = shelf.tools(
        Models::of(
            Catalogue::new(vec![alpha(), beta()], &none()).unwrap(),
            Vec::new(),
        )
        .flag(Some(&shelf.models_dir())),
        &clock,
        &fake,
    );
    let listed = list(&lacking);
    assert_eq!(
        state_of(&listed, "alpha"),
        "installed, not ready: this build has no adapter for it"
    );
    assert_eq!(
        state_of(&listed, "beta"),
        "not installed; not ready: this build has no adapter for it"
    );
    let before = fake.requests().len();
    let (refused, _) = get(&lacking, "beta");
    assert_eq!(
        refused.unwrap_err(),
        "this build has no adapter for the model, so it was not installed and nothing was downloaded"
    );
    assert_eq!(fake.requests().len(), before);
    // And with no directory there, the same is said of each, from the catalogue
    // alone, and nothing is made.
    let nowhere = shelf.path("nowhere");
    let listed = list(
        &shelf.tools(
            Models::of(Catalogue::new(vec![alpha()], &none()).unwrap(), Vec::new())
                .flag(Some(&nowhere)),
            &clock,
            &fake,
        ),
    );
    assert_eq!(
        state_of(&listed, "alpha"),
        "not installed; not ready: this build has no adapter for it"
    );
    assert!(!nowhere.exists());
}

#[test]
fn a_model_this_platform_cannot_run_is_listed_as_that_and_get_refuses_it() {
    let shelf = Shelf::new("models-platform");
    let clock = Hand::new();
    // `gamma` runs only on the platform this build is not for.
    let gamma = entry(
        "gamma",
        "Gamma, for tests",
        "MIT",
        &[("weights.bin", 64)],
        &[elsewhere()],
    );
    let fake = Arc::new(serving(&[alpha(), gamma.clone()]));
    let tools = shelf.tools(shelf.models(&[alpha(), gamma.clone()]), &clock, &fake);
    let listed = list(&tools);
    assert_eq!(
        line_of(&listed, "gamma"),
        format!(
            "gamma\tGamma, for tests\tMIT\t64 B\t{}\tnot installed; not ready: it does not run on \
             this platform",
            elsewhere().name()
        )
    );
    assert_eq!(state_of(&listed, "alpha"), "not installed");
    // `get` refuses it before anything is asked or made: no directory, no request.
    let (refused, said) = get(&tools, "gamma");
    assert_eq!(
        refused.unwrap_err(),
        format!(
            "the model does not run on this platform (its entry names {}), so it was not \
             installed and nothing was downloaded",
            elsewhere().name()
        )
    );
    assert!(said.is_empty());
    assert!(fake.requests().is_empty());
    assert!(!shelf.models_dir().exists());
    // A build for a platform no catalogue names runs none of them, and says so of an
    // installed model too.
    get(&tools, "alpha").0.unwrap();
    let nowhere = shelf.tools(shelf.models(&[alpha(), gamma]).runs_on(None), &clock, &fake);
    assert_eq!(
        state_of(&list(&nowhere), "alpha"),
        "installed, not ready: it does not run on this platform"
    );
    let (refused, _) = get(&nowhere, "alpha");
    assert!(refused.unwrap_err().starts_with(
        "the model does not run on this platform (its entry names linux-x86_64, macos-arm64)"
    ),);
}

#[cfg(unix)]
#[test]
fn a_model_whose_layout_has_a_link_is_listed_as_having_one() {
    let shelf = Shelf::new("models-link");
    let clock = Hand::new();
    let fake = Arc::new(serving(&[alpha(), beta()]));
    let tools = shelf.tools(shelf.models(&[alpha(), beta()]), &clock, &fake);
    get(&tools, "alpha").0.unwrap();
    get(&tools, "beta").0.unwrap();
    // `alpha`'s holds folder is swapped for a link to a folder elsewhere; `beta` is
    // left as it was. The inventory calls the first damaged; `list` says why.
    let holds = shelf.models_dir().join("models/alpha/holds");
    let elsewhere = shelf.path("elsewhere");
    std::fs::rename(&holds, &elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &holds).unwrap();
    let said = "a folder of its layout is a link, which whizbang does not go through; take the \
                link away";
    let listed = list(&tools);
    assert_eq!(state_of(&listed, "alpha"), said);
    assert_eq!(state_of(&listed, "beta"), "installed");
    // The folder that holds every model's folder, the same, for each model.
    std::fs::remove_file(&holds).unwrap();
    std::fs::rename(&elsewhere, &holds).unwrap();
    assert_eq!(state_of(&list(&tools), "alpha"), "installed");
    let models = shelf.models_dir().join("models");
    let before = everything_under(&models);
    let moved = shelf.path("moved");
    std::fs::rename(&models, &moved).unwrap();
    std::os::unix::fs::symlink(&moved, &models).unwrap();
    let listed = list(&tools);
    assert_eq!(state_of(&listed, "alpha"), said);
    assert_eq!(state_of(&listed, "beta"), said);
    // Nothing was made or taken away through the link.
    assert_eq!(everything_under(&moved), before);
}

#[test]
fn a_secret_the_config_names_is_in_nothing_the_commands_print() {
    let shelf = Shelf::new("models-secret");
    let clock = Hand::new();
    let key = made_up_key("models");
    // The config names a variable; the env file holds its value, the key. The
    // test's catalogue, made by a reader told of no secret, has the key as a
    // model's name and in another's title and licence.
    let config = shelf.path("whizbang.toml");
    std::fs::write(
        &config,
        "[backends.fill]\nkind = \"fake\"\nkey_env = \"FAKE_KEY\"\n",
    )
    .unwrap();
    let env = shelf.path("keys.env");
    std::fs::write(&env, format!("FAKE_KEY={key}\n")).unwrap();
    let under = Under {
        config: Some(&config),
        env_file: Some(&env),
    };
    let named = entry(
        &key,
        "Named, for tests",
        "MIT",
        &[("weights.bin", 16)],
        &Platform::ALL,
    );
    let titled = entry(
        "titled",
        &format!("Titled {key}"),
        &format!("Licence {key}"),
        &[("weights.bin", 16)],
        &Platform::ALL,
    );
    let fake = Arc::new(serving(&[named.clone(), titled.clone()]));
    let tools = shelf.tools(shelf.models(&[named, titled]), &clock, &fake);
    let mut printed = Vec::new();
    // `list`: the model whose name is the key is not shown; the other is, with the
    // key taken out of its title and its licence.
    let listed = model_commands::list(under, &tools).unwrap();
    assert!(
        listed.contains("a model whose name holds a secret, not shown"),
        "{listed}"
    );
    assert!(listed.contains("\ntitled\t"), "{listed}");
    printed.push(listed);
    // `get` of each: the one named by the key is not in the catalogue the commands
    // read; the other is installed.
    let mut said = Vec::new();
    let refused = model_commands::get(under, &key, &tools, &mut |line| said.push(line.to_owned()));
    let refused = refused.unwrap_err();
    assert!(
        refused.starts_with("the catalogue holds no model of that name"),
        "{refused}"
    );
    printed.push(refused);
    let got = model_commands::get(under, "titled", &tools, &mut |line| {
        said.push(line.to_owned())
    });
    printed.push(got.unwrap());
    printed.extend(said);
    printed.push(model_commands::list(under, &tools).unwrap());
    // `remove` of each, and the complaint of a model that is not installed.
    printed.push(model_commands::remove(under, &key, Duration::ZERO, &tools).unwrap_err());
    printed.push(model_commands::remove(under, "titled", Duration::ZERO, &tools).unwrap());
    printed.push(model_commands::remove(under, "titled", Duration::ZERO, &tools).unwrap_err());
    // A config that holds the key itself is refused without it.
    std::fs::write(
        &config,
        format!("[backends.fill]\nkind = \"fake\"\nkey_env = \"FAKE_KEY\"\nmodel = \"{key}\"\n"),
    )
    .unwrap();
    printed.push(model_commands::list(under, &tools).unwrap_err());
    for text in &printed {
        assert!(!text.contains(&key), "{text}");
        assert!(!text.contains("0123456789"), "{text}");
    }
    assert_eq!(printed.len(), 8);
}

/// A platform that counts each time it is asked where its models directory is, and
/// answers a folder that is not there.
fn counting_platform(shelf: &Shelf) -> (wiring::Platform, Arc<AtomicU64>) {
    let asked = Arc::new(AtomicU64::new(0));
    let (count, place) = (asked.clone(), shelf.path("platform-models"));
    let platform: wiring::Platform = Arc::new(move || {
        count.fetch_add(1, Ordering::SeqCst);
        Some(place.clone())
    });
    (platform, asked)
}

#[test]
fn list_of_an_empty_catalogue_and_get_or_remove_of_a_model_it_does_not_hold_never_ask_where_the_platforms_directory_is()
 {
    // Review of N3b: where the platform's models directory is, is asked only when a
    // directory is to be looked in. A `list` that finds the catalogue empty, and a
    // `get` or a `remove` of a name the catalogue does not hold, never ask.
    let shelf = Shelf::new("models-unasked");
    let clock = Hand::new();
    let fake = Arc::new(FakeDownloader::new());
    let empty = Catalogue::new(Vec::new(), &none()).unwrap();
    for catalogue in [empty, Catalogue::new(vec![alpha()], &none()).unwrap()] {
        let (platform, asked) = counting_platform(&shelf);
        let models = Models::of(catalogue.clone(), adapters()).platform(platform);
        let tools = Tools::of(models, clock.clone(), downloads(&fake, &Arc::default()));
        if catalogue.entries().is_empty() {
            assert_eq!(
                list(&tools),
                "models: 0: the catalogue holds no model yet\n"
            );
        }
        assert!(get(&tools, "absent").0.is_err());
        let removed = model_commands::remove(Under::default(), "absent", PAUSE, &tools);
        assert!(removed.is_err());
        assert_eq!(asked.load(Ordering::SeqCst), 0);
        // And `list` of a catalogue that holds a model asks, once: the control.
        if !catalogue.entries().is_empty() {
            assert!(list(&tools).contains("not made yet"));
            assert_eq!(asked.load(Ordering::SeqCst), 1);
        }
    }
    assert!(fake.requests().is_empty());
    assert!(!shelf.path("platform-models").exists());
}

/// Each shipped entry whose download needs a token and that runs on this platform:
/// `sam3` on the Mac (task N5). Elsewhere `get` refuses it for its platform before the
/// token is looked for, so the runs below are none there, and the halves of these
/// tests that read the binary's source are what hold them; the macOS runner runs
/// them.
fn shipped_needing_a_token() -> Vec<String> {
    let catalogue = wiring::catalogue().unwrap();
    let names: Vec<String> = catalogue
        .entries()
        .iter()
        .filter(|entry| entry.needs_token().is_some())
        .filter(|entry| Platform::this().is_some_and(|on| entry.runs_on(on)))
        .map(|entry| entry.name().as_str().to_owned())
        .collect();
    if Platform::this() == Some(Platform::MacosArm64) {
        assert_eq!(names, ["sam3"]);
    }
    names
}

/// `get` of a shipped model by the real binary, with the function's temporary home,
/// a config that names the sign-in and a models directory of the test's, and these
/// variables of the test's own, after `plant` was handed the home to put files in;
/// what it says on its standard error.
fn get_with_the_sign_in(
    shelf: &Shelf,
    model: &str,
    variables: &[(&str, &str)],
    plant: impl FnOnce(&Path),
) -> String {
    let config = shelf.path("signed-in.toml");
    std::fs::write(
        &config,
        "[[models.tokens]]\norigin = \"https://huggingface.co\"\nsign_in = \"huggingface\"\n",
    )
    .unwrap();
    let mut command = binary();
    plant(command.home());
    command.args(["models", "get", model, "--config"]);
    command
        .arg(&config)
        .arg("--models-dir")
        .arg(shelf.models_dir());
    for (name, value) in variables {
        command.env(name, value);
    }
    let out = command.output().expect("the binary runs");
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    // Refused before anything was asked: the models directory, which `get` opens
    // before it makes the download client, was not made.
    assert!(!shelf.models_dir().exists());
    String::from_utf8(out.stderr).unwrap()
}

/// The code of the binary's sources, without the lines that are comments alone and
/// without white space.
fn binary_code(file: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(file);
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .flat_map(str::chars)
        .filter(|c| !c.is_whitespace())
        .collect()
}

#[test]
fn the_real_binary_started_with_a_temporary_home_reads_the_sign_in_there_and_no_other() {
    // Where the binary reads the sign-in: `main`'s one function hands the four
    // variables the Hub's documentation names, in its order, to the function that
    // works out the place (`sign_in::token_file`, held in its own test with made-up
    // folders), and reads nothing else for it. The text is in a data file: written
    // here, `just check` would refuse it as a read of the environment in a test.
    let wanted: String = include_str!("sign-in-reads.txt")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    assert!(wanted.starts_with("fnhub_sign_in_"), "{wanted}");
    assert_eq!(binary_code("main.rs").matches(&wanted).count(), 1);
    // The real binary, started with a temporary home: for each shipped model that
    // needs the sign-in's token, the token is looked for there and nowhere else.
    // With nothing there, it is not there; with a file there that others can read,
    // that file is the one refused. Neither asks anything of any address.
    let shelf = Shelf::new("models-sign-in-home");
    for model in shipped_needing_a_token() {
        let said = get_with_the_sign_in(&shelf, &model, &[], |_| {});
        assert!(
            said.contains("the Hub's sign-in has no token on this machine"),
            "{said}"
        );
        // A token file in the temporary home that others can read: refused as that,
        // so it is the file that was looked at. Its mode is the file system's own
        // default for a new file, which lets others read it.
        let said = get_with_the_sign_in(&shelf, &model, &[], |home| {
            let place = home.join(".cache").join("huggingface");
            std::fs::create_dir_all(&place).unwrap();
            let folder = std::fs::Permissions::from_mode(0o700);
            std::fs::set_permissions(&place, folder).unwrap();
            std::fs::write(place.join("token"), made_up_key("home")).unwrap();
            let open = std::fs::Permissions::from_mode(0o644);
            std::fs::set_permissions(place.join("token"), open).unwrap();
        });
        assert!(
            said.contains("can be read or written by others than its owner"),
            "{said}"
        );
        assert!(!said.contains("0123456789"), "{said}");
    }
}

#[test]
fn the_real_binary_started_with_hf_token_set_reads_the_sign_in_and_never_the_variable() {
    // No source of the binary names either variable that holds a token itself; the
    // Hub's tools take them in place of the file, and whizbang does not (plan 1C, D12).
    let sources = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = 0;
    for entry in std::fs::read_dir(&sources).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        let code = binary_code(&name);
        assert!(!code.contains("\"HF_TOKEN\""), "{name}");
        assert!(!code.contains("HUGGING_FACE_HUB_TOKEN"), "{name}");
        files += 1;
    }
    assert!(files > 10);
    // The real binary, with both variables set to a made-up token and no sign-in in
    // its temporary home: for each shipped model that needs the sign-in's token, the
    // sign-in is not there, and the variables are not taken for it.
    let shelf = Shelf::new("models-sign-in-variable");
    let token = made_up_key("variable");
    for model in shipped_needing_a_token() {
        let said = get_with_the_sign_in(
            &shelf,
            &model,
            &[("HF_TOKEN", &token), ("HUGGING_FACE_HUB_TOKEN", &token)],
            |_| {},
        );
        assert!(
            said.contains("the Hub's sign-in has no token on this machine"),
            "{said}"
        );
        assert!(!said.contains(&token), "{said}");
    }
}

// --- the cached check of an installation's files (plan 2A, task R1) ----------------------------

/// The check record of this installation of `fake-ok` in a test's models directory.
fn check_record(local: &Local, identity: &str) -> PathBuf {
    let hex = identity.strip_prefix("sha256:").unwrap();
    let model = local.models_dir().join("models").join(pack::OK);
    model.join("checks").join(hex)
}

/// What is in the model's `checks/`, by name.
fn checks_held(local: &Local) -> Vec<String> {
    let checks = local
        .models_dir()
        .join("models")
        .join(pack::OK)
        .join("checks");
    let mut names: Vec<String> = std::fs::read_dir(checks)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// One call of `finds` through a fresh executor over this folder, and what it came to.
fn one_call(local: &Local) -> Vec<pack::Came> {
    let over = local.over(&pack::one(pack::OK, ""));
    let executor = local.runner(&over);
    let finished = executor.submit(&Call::new("finds")).unwrap();
    let came = local.came();
    assert_eq!(finished.outcome().name() == "completed", came[0].is_ok());
    came
}

/// The installation of `fake-ok` checked once in full by a first process, with its
/// check record written; then a second process, as a fresh one worker is: its own
/// record of what it verified is empty. The identity, and the installed file.
fn checked_by_a_first_process(local: &mut Local) -> (String, PathBuf) {
    let over = local.over(&pack::one(pack::OK, ""));
    let identity = local.identity(&over, pack::OK);
    drop(over);
    let file = local.installed_file(pack::OK, &identity, "weights.bin");
    assert!(!check_record(local, &identity).exists());
    assert_eq!(one_call(local), [Ok(pack::determined())]);
    // The record of the full check: beside the installation, owner-only, and nothing
    // left of the writing.
    let hex = identity.strip_prefix("sha256:").unwrap().to_owned();
    assert_eq!(checks_held(local), [hex]);
    let mode = std::fs::metadata(check_record(local, &identity))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    // The installation's own folder holds its record and its files and nothing else.
    let folder = file.parent().unwrap().parent().unwrap();
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["RECORD", "files"]);
    local.worker = pack::guarded();
    (identity, file)
}

/// Takes every reading of the file away, keeping its size, its time and its inode: a
/// full hash of it can no longer run, and fails.
fn unreadable(file: &Path) {
    let before = std::fs::metadata(file).unwrap();
    std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o000)).unwrap();
    assert!(std::fs::read(file).is_err(), "the file can still be read");
    let after = std::fs::metadata(file).unwrap();
    assert_eq!(
        (before.len(), before.modified().unwrap()),
        (after.len(), after.modified().unwrap())
    );
}

fn readable(file: &Path) {
    std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o400)).unwrap();
}

/// A check record as a check made now would write it: each file's time of change, in
/// `files`, as it is now. Taking a file's reading away changes that time, so a record
/// made before it no longer holds; this is the record made after it.
fn restamped(record: &[u8], files: &Path) -> Vec<u8> {
    use std::os::unix::fs::MetadataExt;
    let text = String::from_utf8(record.to_vec()).unwrap();
    let mut out = String::new();
    for (at, line) in text.lines().enumerate() {
        let mut words: Vec<String> = line.split(' ').map(str::to_owned).collect();
        if at >= 2 {
            let found = std::fs::symlink_metadata(files.join(words.last().unwrap())).unwrap();
            words[3] = found.ctime().to_string();
            words[4] = found.ctime_nsec().to_string();
        }
        out.push_str(&words.join(" "));
        out.push('\n');
    }
    out.into_bytes()
}

#[test]
fn an_unchanged_installation_checked_before_is_loaded_without_a_full_hash() {
    let mut local = Local::new("models-cached");
    let (identity, file) = checked_by_a_first_process(&mut local);
    // The second process cannot read the file, and loads the model all the same: it
    // trusted the record, made after the file's reading was taken away, and no file was
    // read whole.
    unreadable(&file);
    let at = check_record(&local, &identity);
    let record = restamped(&std::fs::read(&at).unwrap(), file.parent().unwrap());
    std::fs::write(&at, record).unwrap();
    assert_eq!(one_call(&local), [Ok(pack::determined())]);
    let loads: Vec<&str> = local.named().iter().map(|sent| sent.0).collect();
    assert_eq!(loads, ["load", "segment"]);
    readable(&file);
}

#[test]
fn a_file_changed_since_the_check_is_hashed_again_and_refused() {
    let mut local = Local::new("models-changed");
    let (identity, file) = checked_by_a_first_process(&mut local);
    let record = std::fs::read(check_record(&local, &identity)).unwrap();
    // One byte changed, the length kept: the file's time of last write moves.
    let mut bytes = std::fs::read(&file).unwrap();
    bytes[0] = b'W';
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::write(&file, &bytes).unwrap();
    // The record no longer holds, so every file is read again, and the damage is found:
    // refused, never sent, and no worker started.
    assert_eq!(
        one_call(&local),
        [Err((FaultKind::Unavailable, kind::DAMAGED.to_owned()))]
    );
    assert!(local.named().is_empty());
    // A check that did not pass writes no record.
    assert_eq!(
        std::fs::read(check_record(&local, &identity)).unwrap(),
        record
    );
}

#[test]
fn a_check_record_that_is_damaged_is_ignored_and_the_full_hash_runs() {
    let mut local = Local::new("models-damaged-record");
    let (identity, file) = checked_by_a_first_process(&mut local);
    let at = check_record(&local, &identity);
    let good = std::fs::read(&at).unwrap();
    let text = String::from_utf8(good.clone()).unwrap();
    assert!(text.starts_with("check/2\n"), "{text}");
    // Three damaged records: cut short, of another hash (the stamps kept), and not a
    // record at all. Each time, a file that cannot be read shows that a full hash was
    // tried: the request is refused as unread. Read again, the full hash passes, the
    // model loads, and the record is written again, whole.
    let line = text.lines().nth(2).unwrap();
    let hash = line.split(' ').nth(6).unwrap();
    let other = format!("sha256:{}", "0".repeat(64));
    let damaged = [
        good[..good.len() - 3].to_vec(),
        text.replace(hash, &other).into_bytes(),
        b"not a check record\n".to_vec(),
    ];
    for (round, bytes) in damaged.iter().enumerate() {
        std::fs::write(&at, bytes).unwrap();
        unreadable(&file);
        assert_eq!(
            one_call(&local),
            [Err((FaultKind::Unavailable, kind::UNVERIFIED.to_owned()))],
            "{round}"
        );
        readable(&file);
        assert_eq!(one_call(&local), [Ok(pack::determined())], "{round}");
        let now = restamped(&good, file.parent().unwrap());
        assert_eq!(std::fs::read(&at).unwrap(), now, "{round}");
        local.worker = pack::guarded();
    }
}

#[test]
fn a_rewrite_that_restores_the_time_of_last_write_is_seen_by_the_time_of_change() {
    let mut local = Local::new("models-restored-time");
    let (identity, file) = checked_by_a_first_process(&mut local);
    let record = std::fs::read(check_record(&local, &identity)).unwrap();
    // One byte changed, the length kept, and the time of last write set back to what
    // the record holds: only the time of change says the file was written.
    let written = std::fs::metadata(&file).unwrap().modified().unwrap();
    let mut bytes = std::fs::read(&file).unwrap();
    bytes[0] = b'W';
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::write(&file, &bytes).unwrap();
    let opened = std::fs::OpenOptions::new().write(true).open(&file).unwrap();
    opened.set_modified(written).unwrap();
    drop(opened);
    assert_eq!(
        std::fs::metadata(&file).unwrap().modified().unwrap(),
        written
    );
    assert_eq!(
        one_call(&local),
        [Err((FaultKind::Unavailable, kind::DAMAGED.to_owned()))]
    );
    assert_eq!(
        std::fs::read(check_record(&local, &identity)).unwrap(),
        record
    );
}

#[test]
fn a_checks_folder_or_a_check_record_that_is_a_link_is_not_trusted_and_nothing_is_written_through_it()
 {
    let mut local = Local::new("models-checks-link");
    let (identity, file) = checked_by_a_first_process(&mut local);
    let at = check_record(&local, &identity);
    let checks = at.parent().unwrap().to_path_buf();
    let files = file.parent().unwrap().to_path_buf();
    let elsewhere = local.models_dir().parent().unwrap().join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    let name = at.file_name().unwrap().to_owned();

    // `checks/` a link to a folder that holds a record that would hold, were it read:
    // it is not, so the full hash runs, and fails on a file that cannot be read.
    unreadable(&file);
    let good = restamped(&std::fs::read(&at).unwrap(), &files);
    std::fs::write(elsewhere.join(&name), &good).unwrap();
    std::fs::remove_file(&at).unwrap();
    std::fs::remove_dir(&checks).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &checks).unwrap();
    assert_eq!(
        one_call(&local),
        [Err((FaultKind::Unavailable, kind::UNVERIFIED.to_owned()))]
    );
    // Readable again, the full hash passes and the model loads; nothing is written
    // through the link.
    readable(&file);
    local.worker = pack::guarded();
    assert_eq!(one_call(&local), [Ok(pack::determined())]);
    assert!(
        std::fs::symlink_metadata(&checks)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let held: Vec<_> = std::fs::read_dir(&elsewhere).unwrap().collect();
    assert_eq!(held.len(), 1);
    assert_eq!(std::fs::read(elsewhere.join(&name)).unwrap(), good);

    // The record itself a link to a file that would hold: not trusted either. The
    // record written after the full hash replaces the link, and what it led to is kept.
    std::fs::remove_file(&checks).unwrap();
    std::fs::create_dir(&checks).unwrap();
    unreadable(&file);
    let target = elsewhere.join("record");
    let led = restamped(&good, &files);
    std::fs::write(&target, &led).unwrap();
    std::os::unix::fs::symlink(&target, &at).unwrap();
    local.worker = pack::guarded();
    assert_eq!(
        one_call(&local),
        [Err((FaultKind::Unavailable, kind::UNVERIFIED.to_owned()))]
    );
    readable(&file);
    local.worker = pack::guarded();
    assert_eq!(one_call(&local), [Ok(pack::determined())]);
    assert!(std::fs::symlink_metadata(&at).unwrap().is_file());
    assert_eq!(std::fs::read(&at).unwrap(), restamped(&good, &files));
    assert_eq!(std::fs::read(&target).unwrap(), led);
}
