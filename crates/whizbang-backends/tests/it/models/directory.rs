//! A models directory is its owner's alone, and its lock is asked for until a time
//! (ADR 0017, "A models directory is its owner's alone"; "Three kinds of lock").
//!
//! Every folder here is a test's own, under its scratch folder, and a mode is changed
//! only there. Nothing is run as another user: a folder that is another's is the
//! answer of a file system that says so. No test sleeps: the clock is one a test
//! moves, and what it is asked to wait is counted.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use whizbang_backends::models::{
    CatalogueFile, Current, Entry, HoldRefused, Holds, InstallRefused, InstallationRecord,
    ModelsDir, OpenRefused, current, install,
};
use whizbang_core::contract::{Cancellation, Clock};
use whizbang_core::store::fs::{
    Fs, Held, LockFile, NewFile, OpenFile, RealFs, UserUnknown, Writers,
};
use whizbang_testkit::fs_shim::{Event, ShimFs};
use whizbang_testkit::scratch::Scratch;
use whizbang_testkit::threads;

use super::publishing::{adapter, index_of, installed, record_of, second};
use super::staging::{TERMS, at, graph, names_under, serving_both};
use super::{file_name, hash, model, none, parts};

/// How long the directory's lock is asked for, and the most tries it is asked in:
/// what the record states, written again here.
const WAIT: Duration = Duration::from_secs(5);
const TRIES: usize = 20_000;

/// What is run once, at a stated pause.
type Hook = Box<dyn FnOnce() + Send>;

/// A clock a test moves. It never sleeps: a pause is counted, and moves the clock by
/// what was asked when the clock is one that moves. At one stated pause it runs what
/// the test gave it, once.
struct Paced {
    start: Instant,
    moves: bool,
    waited: Mutex<Duration>,
    pauses: AtomicUsize,
    at: Mutex<Option<(usize, Hook)>>,
}

impl Paced {
    fn new(moves: bool) -> Arc<Paced> {
        Arc::new(Paced {
            start: Instant::now(),
            moves,
            waited: Mutex::new(Duration::ZERO),
            pauses: AtomicUsize::new(0),
            at: Mutex::new(None),
        })
    }

    /// A clock that every pause moves by what was asked.
    fn moving() -> Arc<Paced> {
        Paced::new(true)
    }

    /// A clock that stands still whatever it is asked to wait.
    fn standing() -> Arc<Paced> {
        Paced::new(false)
    }

    /// Runs `hook` inside the pause of this number, counted from one.
    fn at_pause(&self, number: usize, hook: impl FnOnce() + Send + 'static) {
        *self.at.lock().unwrap() = Some((number, Box::new(hook)));
    }

    fn pauses(&self) -> usize {
        self.pauses.load(Ordering::SeqCst)
    }

    /// Everything it was asked to wait, added up.
    fn waited(&self) -> Duration {
        *self.waited.lock().unwrap()
    }
}

impl Clock for Paced {
    fn now_ms(&self) -> u64 {
        0
    }

    fn now(&self) -> Instant {
        if self.moves {
            self.start + self.waited()
        } else {
            self.start
        }
    }

    fn pause(&self, span: Duration) {
        let number = self.pauses.fetch_add(1, Ordering::SeqCst) + 1;
        *self.waited.lock().unwrap() += span;
        let hook = {
            let mut at = self.at.lock().unwrap();
            match &*at {
                Some((when, _)) if *when == number => at.take(),
                _ => None,
            }
        };
        if let Some((_, hook)) = hook {
            hook();
        }
    }
}

/// The real file system, with every call that was made of it kept by its name; and,
/// for a test of a folder that is another's, with that answer in place of the disk's.
struct Asked {
    calls: Mutex<Vec<&'static str>>,
    anothers: bool,
    /// From this asking of whose a folder is on, counted from 0, the answer is that
    /// the user this process runs as could not be learned.
    unknown_from: Option<usize>,
}

impl Asked {
    fn new(anothers: bool) -> Arc<Asked> {
        Arc::new(Asked {
            calls: Mutex::new(Vec::new()),
            anothers,
            unknown_from: None,
        })
    }

    /// The real file system, but that it cannot learn who its user is, from this
    /// asking on.
    fn unknown_from(asking: usize) -> Arc<Asked> {
        Arc::new(Asked {
            calls: Mutex::new(Vec::new()),
            anothers: false,
            unknown_from: Some(asking),
        })
    }

    fn note(&self, call: &'static str) {
        self.calls.lock().unwrap().push(call);
    }

    fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().unwrap().clone()
    }
}

impl Fs for Asked {
    fn create_dir(&self, path: &Path) -> std::io::Result<()> {
        self.note("create_dir");
        RealFs.create_dir(path)
    }
    fn write_new(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        self.note("write_new");
        RealFs.write_new(path, bytes)
    }
    fn sync_dir(&self, path: &Path) -> std::io::Result<()> {
        self.note("sync_dir");
        RealFs.sync_dir(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        self.note("rename");
        RealFs.rename(from, to)
    }
    fn remove_all(&self, path: &Path) -> std::io::Result<()> {
        self.note("remove_all");
        RealFs.remove_all(path)
    }
    fn read_at_most(&self, path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
        self.note("read_at_most");
        RealFs.read_at_most(path, limit)
    }
    fn list(&self, path: &Path) -> std::io::Result<Vec<PathBuf>> {
        self.note("list");
        RealFs.list(path)
    }
    fn exists(&self, path: &Path) -> std::io::Result<bool> {
        self.note("exists");
        RealFs.exists(path)
    }
    fn hold_lock_file(
        &self,
        path: &Path,
        create: bool,
    ) -> std::io::Result<Option<Box<dyn LockFile>>> {
        self.note("hold_lock_file");
        RealFs.hold_lock_file(path, create)
    }
    fn share(&self, path: &Path) -> std::io::Result<Box<dyn Held>> {
        self.note("share");
        RealFs.share(path)
    }
    fn try_alone(&self, path: &Path) -> std::io::Result<Option<Box<dyn Held>>> {
        self.note("try_alone");
        RealFs.try_alone(path)
    }
    fn begin_new(&self, path: &Path, most: u64) -> std::io::Result<Box<dyn NewFile>> {
        self.note("begin_new");
        RealFs.begin_new(path, most)
    }
    fn open_file(&self, path: &Path) -> std::io::Result<Box<dyn OpenFile>> {
        self.note("open_file");
        RealFs.open_file(path)
    }
    fn try_share(&self, path: &Path) -> std::io::Result<Option<Box<dyn Held>>> {
        self.note("try_share");
        RealFs.try_share(path)
    }
    fn is_link(&self, path: &Path) -> std::io::Result<bool> {
        self.note("is_link");
        RealFs.is_link(path)
    }
    fn who_may_write(&self, path: &Path) -> std::io::Result<Writers> {
        let asked = self.calls();
        let asking = asked
            .iter()
            .filter(|call| **call == "who_may_write")
            .count();
        self.note("who_may_write");
        if self.unknown_from.is_some_and(|from| asking >= from) {
            return Err(UserUnknown::error());
        }
        let mut writers = RealFs.who_may_write(path)?;
        if self.anothers {
            writers.owner_is_this_user = false;
        }
        Ok(writers)
    }
}

/// The permission bits of what has this name, the sticky bit among them.
fn mode_of(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

fn set_mode(path: &Path, mode: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// An entry of two files under this model's name, served by `serving_both`.
fn entry_named(name: &str) -> Entry {
    let mut parts = parts(name);
    parts.files = [("graph.onnx", graph()), ("LICENSE.txt", TERMS.to_vec())]
        .iter()
        .map(|(file, content)| {
            let size = content.len() as u64;
            CatalogueFile::new(file_name(file), size, hash(content), at(file), &none()).unwrap()
        })
        .collect();
    Entry::new(parts, &none()).unwrap()
}

fn installing_in(dir: &ModelsDir, entry: &Entry) -> Result<InstallationRecord, InstallRefused> {
    install(
        dir,
        entry,
        &adapter(),
        &serving_both(),
        &Cancellation::new(),
        &none(),
    )
}

/// What is printed of a refusal, both ways: one line of printable ASCII, short, with
/// nothing of a path in it.
fn printed(refused: OpenRefused, root: &Path) -> String {
    let printed = format!("{refused} {refused:?}");
    assert!(
        printed.bytes().all(|b| b.is_ascii_graphic() || b == b' '),
        "{printed:?}"
    );
    assert!(printed.len() <= 400, "{} characters", printed.len());
    assert!(!printed.contains('/'), "{printed:?}");
    for part in root.iter().filter(|part| part.len() > 1) {
        let part = part.to_string_lossy();
        assert!(!printed.contains(&*part), "{printed:?} holds {part:?}");
    }
    printed
}

// --- whose a models directory is ---------------------------------------------------------------

#[test]
fn a_new_models_directory_is_readable_and_writable_by_its_owner_only() {
    let scratch = Scratch::new("directory-new");
    let root = scratch.path().join("kept");
    assert!(!root.exists());
    // Opened where nothing is, it is made, for its owner only, and its name is made
    // durable in the folder that holds it.
    let fs = Arc::new(ShimFs::new());
    let dir = ModelsDir::open(fs.clone(), &root, Paced::standing()).unwrap();
    assert_eq!(mode_of(&root), 0o700);
    assert_eq!(
        fs.events(),
        [
            Event::CreateDir(root.clone()),
            Event::SyncDir(scratch.path().to_path_buf()),
        ]
    );
    // And so is everything an install makes in it: no folder and no file that its
    // group or anyone else can read, write or go into.
    let record = installing_in(&dir, &entry_named("tiny")).unwrap();
    let names = names_under(&root);
    assert!(names.len() > 8, "{names:?}");
    for name in &names {
        let mode = mode_of(&root.join(name));
        assert_eq!(mode & 0o077, 0, "{name}: {mode:o}");
    }
    // Opened again, by its owner, it is taken as it is: nothing is made, and the
    // model is as it was installed.
    let fs = Arc::new(ShimFs::new());
    let again = ModelsDir::open(fs.clone(), &root, Paced::standing()).unwrap();
    assert!(fs.events().is_empty(), "{:?}", fs.events());
    assert_eq!(
        current(&again, &model("tiny"), &none()).unwrap(),
        Current::Installed(record)
    );
    assert_eq!(names_under(&root), names);
    // A folder its owner made that others may read, and none of them write, is its
    // owner's alone too.
    for mode in [0o755, 0o750, 0o711, 0o500] {
        let made = scratch.path().join(format!("made-{mode:o}"));
        std::fs::create_dir(&made).unwrap();
        set_mode(&made, mode);
        let opened = ModelsDir::open(Arc::new(RealFs), &made, Paced::standing());
        assert!(opened.is_ok(), "{mode:o}: {opened:?}");
        assert_eq!(mode_of(&made), mode, "and its mode is not changed");
        set_mode(&made, 0o700);
    }
}

#[test]
fn a_models_directory_that_its_group_or_anyone_can_write_is_refused_and_nothing_in_it_is_read() {
    let scratch = Scratch::new("directory-others");
    // A models directory with a model installed in it, made as whizbang makes one.
    let root = scratch.path().join("kept");
    ModelsDir::open(Arc::new(RealFs), &root, Paced::standing()).unwrap();
    installed(&root, &second());
    let names = names_under(&root);

    // Its group may write it; anyone may; both; and anyone may, with the mark that
    // keeps one user from deleting another's names, which keeps nobody from adding
    // one.
    for mode in [0o720, 0o770, 0o702, 0o707, 0o722, 0o777, 0o1777] {
        set_mode(&root, mode);
        let fs = Asked::new(false);
        let refused = ModelsDir::open(fs.clone(), &root, Paced::standing()).unwrap_err();
        assert_eq!(refused, OpenRefused::OthersMayWrite, "{mode:o}");
        // Nothing in it was read, listed, locked or made: the one thing asked was
        // whose the folder is.
        assert_eq!(fs.calls(), ["who_may_write"], "{mode:o}");
        assert_eq!(mode_of(&root), mode, "its mode is not changed for its user");
    }
    // The same on the test kit's disk, where it is no event.
    let fs = Arc::new(ShimFs::new());
    let refused = ModelsDir::open(fs.clone(), &root, Paced::standing()).unwrap_err();
    assert_eq!(refused, OpenRefused::OthersMayWrite);
    assert!(fs.events().is_empty(), "{:?}", fs.events());
    set_mode(&root, 0o700);
    assert_eq!(names_under(&root), names, "and nothing in it was changed");

    // A folder that is another user's, though nobody but its owner may write it:
    // its owner is one who can write it, and is not this user.
    let fs = Asked::new(true);
    let refused = ModelsDir::open(fs.clone(), &root, Paced::standing()).unwrap_err();
    assert_eq!(refused, OpenRefused::NotOwn);
    assert_eq!(fs.calls(), ["who_may_write"]);
    // Another's that others may write as well is refused as another's: taking the
    // others' leave away would not make it this user's.
    set_mode(&root, 0o777);
    let refused = ModelsDir::open(Asked::new(true), &root, Paced::standing()).unwrap_err();
    assert_eq!(refused, OpenRefused::NotOwn);
    set_mode(&root, 0o700);
    assert_eq!(names_under(&root), names);

    // Each refusal says how to mend it, in fixed words, with nothing of a path.
    let said = printed(OpenRefused::OthersMayWrite, &root);
    assert!(said.contains("chmod go-w"), "{said:?}");
    let said = printed(OpenRefused::NotOwn, &root);
    assert!(said.contains("chown"), "{said:?}");
    assert!(said.contains("a folder of your own"), "{said:?}");
    printed(
        OpenRefused::Disk(std::io::ErrorKind::PermissionDenied),
        &root,
    );

    // What has the name and is no folder, and a name under a folder that is not
    // there, are the disk's refusal: nothing is made of either.
    let file = scratch.path().join("file");
    std::fs::write(&file, b"content").unwrap();
    let refused = ModelsDir::open(Arc::new(RealFs), &file, Paced::standing()).unwrap_err();
    assert_eq!(
        refused,
        OpenRefused::Disk(std::io::ErrorKind::NotADirectory)
    );
    let deep = scratch.path().join("missing/kept");
    let refused = ModelsDir::open(Arc::new(RealFs), &deep, Paced::standing()).unwrap_err();
    assert_eq!(refused, OpenRefused::Disk(std::io::ErrorKind::NotFound));
    assert!(!scratch.path().join("missing").exists());

    // Mended, the same folder opens, and the model in it is the one installed.
    let dir = ModelsDir::open(Arc::new(RealFs), &root, Paced::standing()).unwrap();
    assert_eq!(
        current(&dir, &model("tiny"), &none()).unwrap(),
        Current::Installed(record_of(&second()))
    );
}

#[test]
fn a_failure_to_learn_who_the_user_is_has_its_own_refusal_and_is_never_read_as_a_directory_that_is_absent()
 {
    let scratch = Scratch::new("directory-user");
    // A directory that is there, with a model in it: it is not used, and the one
    // thing asked was whose it is.
    let root = scratch.path().join("kept");
    ModelsDir::open(Arc::new(RealFs), &root, Paced::standing()).unwrap();
    installed(&root, &second());
    let names = names_under(&root);
    let fs = Asked::unknown_from(0);
    let refused = ModelsDir::open(fs.clone(), &root, Paced::standing()).unwrap_err();
    assert_eq!(refused, OpenRefused::UserUnknown);
    assert_eq!(fs.calls(), ["who_may_write"]);
    assert_eq!(names_under(&root), names);
    // A directory that is not there: the failure is not "nothing has the name", so
    // no folder is made to be refused afterwards.
    let absent = scratch.path().join("absent");
    let fs = Asked::unknown_from(0);
    let refused = ModelsDir::open(fs.clone(), &absent, Paced::standing()).unwrap_err();
    assert_eq!(refused, OpenRefused::UserUnknown);
    assert_eq!(fs.calls(), ["who_may_write"]);
    assert!(!absent.exists());
    // Learned once and then not: the folder that was made is refused the same way,
    // and not as a failure of the disk.
    let fs = Asked::unknown_from(1);
    let refused = ModelsDir::open(fs.clone(), &absent, Paced::standing()).unwrap_err();
    assert_eq!(refused, OpenRefused::UserUnknown);
    assert_eq!(fs.calls().last(), Some(&"who_may_write"));
    // It has a sentence of its own, with nothing of a path, which is not the
    // sentence of a disk that refused.
    let said = printed(OpenRefused::UserUnknown, &root);
    assert!(said.contains("which user whizbang runs as"), "{said:?}");
    assert_ne!(
        OpenRefused::UserUnknown.to_string(),
        OpenRefused::Disk(std::io::ErrorKind::Other).to_string()
    );
    // A failure of the disk is still that, and "not found" of the folder that is to
    // hold the directory is not a user that could not be learned.
    let orphan = scratch.path().join("no-such-folder").join("models");
    assert_eq!(
        ModelsDir::open(Arc::new(RealFs), &orphan, Paced::standing()).unwrap_err(),
        OpenRefused::Disk(std::io::ErrorKind::NotFound)
    );
}

/// Whether this source text names the opening that asks nothing, `ModelsDir::at`, or
/// gives `ModelsDir` another name under which it could. Lines that are comments
/// are not read; everything else is, with its white space taken out, so that
/// nothing is hidden by where a line breaks. What it looks for:
///
/// - the path itself, `ModelsDir::at` and `<ModelsDir>::at`, called or not;
/// - `ModelsDir as`, which gives the type another name where it is imported;
/// - a `type` whose right side ends in `ModelsDir`;
/// - `Self::at` in a file that holds an `impl ModelsDir`.
///
/// A tripwire, not a proof: a macro that builds the name from pieces is not seen.
fn names_the_unasked_opening(source: &str, defines_it: bool) -> bool {
    let code: String = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .flat_map(str::chars)
        .filter(|c| !c.is_whitespace())
        .collect();
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let path = ["ModelsDir::at", "ModelsDir>::at", "Self::at"]
        .into_iter()
        .filter(|path| !path.starts_with("Self") || code.contains("implModelsDir"))
        .any(|path| {
            code.match_indices(path)
                .any(|(at, _)| !code[at + path.len()..].starts_with(word))
        });
    // `as` and then a name: `&ModelsDir as &dyn …` is no alias, and is not code here.
    let renamed = code.contains("ModelsDiras");
    let aliased = code.split(';').any(|piece| {
        piece
            .rfind("type")
            .is_some_and(|at| piece[at..].contains('=') && piece.ends_with("ModelsDir"))
    });
    // The one file that defines it holds its one definition; no other file that
    // holds an `impl ModelsDir` defines a function of that name.
    let defined = match code.contains("implModelsDir") {
        true => code.matches("fnat(").count(),
        false => 0,
    };
    path || renamed || aliased || defined != usize::from(defines_it)
}

#[test]
fn the_binary_names_a_models_directory_only_through_the_opening_that_asks_whose_it_is() {
    // `ModelsDir::at` asks nothing of the folder. The lines that accept a look and
    // then an act (`docs/carry-over.md`) rest on no code of the product using it:
    // not the binary, and not a crate the binary is built from, under any name.
    // First the detector, against each way round it and against what it must let by.
    for dodge in [
        "let dir = ModelsDir::at(fs, root);",
        "let dir = ModelsDir :: at (fs, root);",
        "let dir = ModelsDir::\n    at(fs, root);",
        "let make = ModelsDir::at;",
        "let dir = <ModelsDir>::at(fs, root);",
        "use whizbang_backends::models::ModelsDir as Dir;",
        "use whizbang_backends::models::{Catalogue, ModelsDir\n    as Dir};",
        "type Dir = ModelsDir;",
        "pub(crate) type Dir = whizbang_backends::models::ModelsDir;",
        "let x = \"https://h\"; let dir = ModelsDir::at(fs, root);",
        "impl ModelsDir { pub fn sly(fs: F, root: &Path) -> ModelsDir { Self::at(fs, root) } }",
        "impl ModelsDir { pub fn at(fs: F) -> ModelsDir { todo() } }",
        "impl ModelsDir { pub(crate) fn at(fs: F) -> ModelsDir { todo() } }",
    ] {
        assert!(names_the_unasked_opening(dodge, false), "{dodge}");
    }
    for plain in [
        "let dir = ModelsDir::open(fs, root, clock)?;",
        "    /// A test's is `ModelsDir::at`.\n    // ModelsDir::at(fs, root)\nfn f(dir: &ModelsDir) {}",
        "return Err(Refused::at(\"files\", Rule::NoFiles));",
        "use whizbang_backends::models::{Installations, ModelsDir};",
        "type Read<T> = Result<T, ConfigRefused>; fn f(dir: ModelsDir) {}",
        "impl Other { fn f() -> Other { Self::at(1) } }",
        "impl Other { pub fn at(place: usize) -> Other { Other(place) } }",
    ] {
        assert!(!names_the_unasked_opening(plain, false), "{plain}");
    }
    let defining = "impl ModelsDir { pub fn at(fs: F) -> ModelsDir { ModelsDir { fs } } }";
    assert!(!names_the_unasked_opening(defining, true));

    // Then every source file of every product crate. The test kit is no product.
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let products = [
        "whizbang",
        "whizbang-backends",
        "whizbang-core",
        "whizbang-eval",
        "whizbang-rascal",
        "whizbang-serve",
        "whizbang-squint",
    ];
    let listed: Vec<String> = std::fs::read_dir(&crates)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name != "whizbang-testkit" && !name.starts_with('.'))
        .collect();
    assert_eq!(
        listed.len(),
        products.len(),
        "a crate is not looked at: {listed:?}"
    );
    let mut left: Vec<_> = products
        .iter()
        .map(|name| crates.join(name).join("src"))
        .collect();
    let mut read = 0;
    while let Some(folder) = left.pop() {
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                left.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                let defines_it = path.ends_with("whizbang-backends/src/models/directory.rs");
                assert!(
                    !names_the_unasked_opening(&text, defines_it),
                    "{}",
                    path.display()
                );
                read += 1;
            }
        }
    }
    assert!(read > 100, "{read} source files were read");
}

// --- the directory's lock, asked for until a time ----------------------------------------------

#[test]
fn two_publications_at_once_both_end_and_neither_is_refused_as_busy() {
    let scratch = Scratch::new("directory-two");
    let root = scratch.path().join("kept");
    let (one, two) = (entry_named("one"), entry_named("two"));

    // The event of an install that is made under the directory's lock: the making
    // of the identity's hold file.
    let makes_hold_file = |event: &Event| matches!(event, Event::Write(p) if p.parent().is_some_and(|p| p.ends_with("holds")));
    let events = {
        let fs = Arc::new(ShimFs::new());
        let dir = ModelsDir::open(fs.clone(), &scratch.path().join("dry"), Paced::standing());
        installing_in(&dir.unwrap(), &one).unwrap();
        fs.events()
    };
    let under_the_lock = index_of(&events, makes_hold_file);

    // The first publication stops there, with the directory's lock, says so, and
    // goes on when it is told to.
    let (holding, held) = mpsc::channel::<()>();
    let (go_on, told) = mpsc::channel::<()>();
    let first_fs = Arc::new(ShimFs::new());
    first_fs.pause_before(under_the_lock, move || {
        holding.send(()).unwrap();
        told.recv().unwrap();
    });
    let first_dir = ModelsDir::open(first_fs.clone(), &root, Paced::standing()).unwrap();
    let first_entry = one.clone();
    let first = threads::spawn(move || installing_in(&first_dir, &first_entry));
    held.recv().unwrap();
    assert!(
        RealFs
            .hold_lock_file(&root.join("LOCK"), false)
            .unwrap()
            .is_none(),
        "the first has the directory's lock"
    );

    // The second, of another model, comes to the lock while the first has it. It is
    // not refused: it asks again by its clock. Inside its first pause the first is
    // told to go on, and has ended, and let the lock go, before the pause returns.
    let clock = Paced::moving();
    let first = Mutex::new(Some(first));
    let ended = Arc::new(Mutex::new(None));
    let end = ended.clone();
    clock.at_pause(1, move || {
        go_on.send(()).unwrap();
        let first = first.lock().unwrap().take().unwrap();
        *end.lock().unwrap() = Some(first.join().unwrap());
    });
    let second_dir = ModelsDir::open(Arc::new(ShimFs::new()), &root, clock.clone()).unwrap();
    let second = installing_in(&second_dir, &two);

    // Both ended, and neither as busy. The second waited once, and less than the
    // time it would have asked for.
    assert_eq!(second.as_ref().err(), None);
    let first = ended.lock().unwrap().take();
    assert_eq!(first.as_ref().and_then(|ended| ended.as_ref().err()), None);
    assert_eq!(clock.pauses(), 1);
    assert!(clock.waited() < WAIT, "{:?}", clock.waited());
    for (name, record) in [("one", first.unwrap().unwrap()), ("two", second.unwrap())] {
        assert_eq!(
            current(&second_dir, &model(name), &none()).unwrap(),
            Current::Installed(record)
        );
    }
}

#[test]
fn a_directory_whose_lock_is_held_past_five_seconds_of_the_clock_answers_busy_and_nothing_waits_longer()
 {
    let scratch = Scratch::new("directory-held");
    let root = scratch.path().join("kept");
    ModelsDir::open(Arc::new(RealFs), &root, Paced::standing()).unwrap();
    let record = installed(&root, &second());
    // Another has the directory's lock, and does not let it go.
    let other = RealFs.hold_lock_file(&root.join("LOCK"), false).unwrap();
    assert!(other.is_some());

    // A hold asks for the lock once. Every pause it asks of its clock moves the
    // clock by what was asked; at five seconds it is answered busy.
    let clock = Paced::moving();
    let dir = ModelsDir::open(Arc::new(RealFs), &root, clock.clone()).unwrap();
    let holds = Holds::over(dir.clone());
    let refused = holds.hold(&model("tiny"), record.identity(), &none());
    assert_eq!(refused.unwrap_err(), HoldRefused::DirectoryBusy);
    // It waited five seconds of the clock and no longer: it asked for no pause
    // once the time had come, and it was the time that ended it, far under the
    // ceiling of tries.
    assert_eq!(clock.waited(), WAIT);
    assert!(clock.pauses() > 0);
    assert!(clock.pauses() < TRIES / 2, "{} pauses", clock.pauses());
    let pace = WAIT / u32::try_from(clock.pauses()).unwrap();
    assert!(pace >= Duration::from_micros(500), "{pace:?} between tries");

    // A publication is answered the same, at the same time, and has changed no
    // pointer.
    let clock = Paced::moving();
    let dir = ModelsDir::open(Arc::new(ShimFs::new()), &root, clock.clone()).unwrap();
    let refused = installing_in(&dir, &entry_named("tiny")).unwrap_err();
    assert_eq!(refused, InstallRefused::DirectoryBusy);
    assert_eq!(clock.waited(), WAIT);
    drop(other);
    assert_eq!(
        current(&dir, &model("tiny"), &none()).unwrap(),
        Current::Installed(record.clone())
    );

    // Let go, the lock is given at the first try: no pause is asked of the clock.
    let clock = Paced::moving();
    let dir = ModelsDir::open(Arc::new(RealFs), &root, clock.clone()).unwrap();
    let holds = Holds::over(dir);
    drop(
        holds
            .hold(&model("tiny"), record.identity(), &none())
            .unwrap(),
    );
    assert_eq!(clock.pauses(), 0);
}

#[test]
fn under_a_clock_that_stands_still_a_held_lock_answers_busy_at_the_ceiling_of_tries() {
    let scratch = Scratch::new("directory-ceiling");
    let root = scratch.path().join("kept");
    ModelsDir::open(Arc::new(RealFs), &root, Paced::standing()).unwrap();
    let record = installed(&root, &second());
    let other = Arc::new(Mutex::new(
        RealFs.hold_lock_file(&root.join("LOCK"), false).unwrap(),
    ));
    assert!(other.lock().unwrap().is_some());

    // The clock stands still, so no time ever comes. One pause past the ceiling the
    // other would let the lock go, and the hold would be given: tries that had no
    // end would be seen so, and not by a test that never ends.
    let clock = Paced::standing();
    let let_go = other.clone();
    clock.at_pause(TRIES, move || drop(let_go.lock().unwrap().take()));
    let dir = ModelsDir::open(Arc::new(RealFs), &root, clock.clone()).unwrap();
    let holds = Holds::over(dir);
    let refused = holds.hold(&model("tiny"), record.identity(), &none());
    assert_eq!(refused.unwrap_err(), HoldRefused::DirectoryBusy);
    // Twenty thousand tries, a pause between each two, and none after the last.
    assert_eq!(clock.pauses(), TRIES - 1);
    assert!(other.lock().unwrap().is_some(), "the other still has it");
    assert_eq!(clock.now(), clock.start, "and the clock stood still");
}
