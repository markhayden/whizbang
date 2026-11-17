//! `whizbang models list`, `get` and `remove` (plan 1C, task N3b; ADR 0017). The
//! binary's own functions (`whizbang::model_commands`), given a test's catalogue, a
//! fake of the download and a clock the test moves; and the real binary, which has
//! the catalogue it ships, empty until its first entry (task N5), so that nothing
//! of it can reach a models directory but by a test's catalogue. Nothing here
//! leaves the machine. Every key is made up at run time.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use whizbang::model_commands::{self, Downloads, PAUSE, Tools, Under};
use whizbang::wiring::{self, Models};
use whizbang_backends::models::{
    Catalogue, CatalogueFile, Entry, EntryParts, FileName, Installations, Licence, ModelName,
    ModelsDir, Platform, Revision, Title,
};
use whizbang_core::capability::Installation;
use whizbang_core::config::DOWNLOAD_LEAST_KIB_PER_SECOND;
use whizbang_core::contract::{Clock, Secrets};
use whizbang_core::egress::{Address, CheckedDownloader, Origin};
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::downloads::FakeDownloader;
use whizbang_testkit::scratch::Scratch;
use whizbang_testkit::threads;

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

#[test]
fn under_the_catalogue_it_ships_no_models_command_resolves_or_makes_a_models_directory() {
    // The real binary, with the catalogue it ships, which holds no model: `list`
    // says so, and `get` and `remove` refuse a model it does not hold. None of the
    // three opens, makes or asks where a models directory is, whatever names one;
    // the platform's would be in the function's temporary home, which stays empty.
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
    let runs: [(&[&str], i32, &str); 9] = [
        (&["models", "list"], 0, ""),
        (&["models", "list", "--models-dir", flag], 0, ""),
        (&["models", "list", "--config", config], 0, ""),
        (
            &["models", "get", "sam3"],
            1,
            "the catalogue holds no model of that name",
        ),
        (
            &["models", "get", "sam3", "--models-dir", flag],
            1,
            "the catalogue holds no model of that name",
        ),
        (
            &["models", "get", "sam3", "--config", config],
            1,
            "the catalogue holds no model of that name",
        ),
        (
            &["models", "remove", "sam3"],
            1,
            "the catalogue holds no model of that name",
        ),
        (
            &["models", "remove", "sam3", "--models-dir", flag],
            1,
            "the catalogue holds no model of that name",
        ),
        (
            &[
                "models", "remove", "sam3", "--wait", "1", "--config", config,
            ],
            1,
            "the catalogue holds no model of that name",
        ),
    ];
    for (args, code, complaint) in runs {
        let mut command = binary();
        let home = command.home().to_path_buf();
        let out = command.args(args).output().expect("the binary runs");
        assert_eq!(out.status.code(), Some(code), "{args:?}: {out:?}");
        let (stdout, stderr) = (
            String::from_utf8(out.stdout).unwrap(),
            String::from_utf8(out.stderr).unwrap(),
        );
        if code == 0 {
            assert_eq!(stdout, "models: 0: the catalogue holds no model yet\n");
            assert_eq!(stderr, "");
            in_the_guide(&format!("$ whizbang models list\n{stdout}"));
        } else {
            assert_eq!(stdout, "");
            assert!(
                stderr.starts_with(&format!("whizbang: {complaint}")),
                "{stderr}"
            );
            assert!(!stderr.contains("sam3"), "{stderr}");
            in_the_guide(&stderr);
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

/// Each shipped entry whose download needs a token: none until the catalogue's first
/// entry (task N5).
fn shipped_needing_a_token() -> Vec<String> {
    let catalogue = wiring::catalogue().unwrap();
    let names: Vec<String> = catalogue
        .entries()
        .iter()
        .filter(|entry| entry.needs_token().is_some())
        .map(|entry| entry.name().as_str().to_owned())
        .collect();
    // Until N5 the catalogue holds no model at all, so the runs below are none and
    // the halves of these tests that read the binary's source are what hold them.
    if catalogue.entries().is_empty() {
        assert!(names.is_empty());
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
