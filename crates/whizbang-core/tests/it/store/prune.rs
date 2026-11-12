//! Prune (plan 1A, task A3): what nothing names goes, what a reader or a publication in
//! progress still needs stays, and a crash at any point leaves the store whole.

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use whizbang_core::store::fs::{Fs, Held, RealFs};
use whizbang_core::store::{Digest, Generation, Kind, Pruned, Reader, Store};
use whizbang_testkit::fs_shim::{Event, Lost, ShimFs};
use whizbang_testkit::scratch::Scratch;

use super::runs::{EXCHANGE, LEDGER, MASK, ONLY_ONE, object, runs, with_runs};
use super::{A2, B, C, points, publish_new, root, with_old};

/// The names in a directory of the store.
fn names(root: &Path, dir: &str) -> BTreeSet<String> {
    std::fs::read_dir(root.join(dir))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

fn digests(contents: &[&[u8]]) -> BTreeSet<String> {
    contents
        .iter()
        .map(|bytes| Digest::of(bytes).to_string())
        .collect()
}

/// Reads every reference of a generation, in runs and outside them, and returns how many.
fn read_all(
    read: impl Fn(&whizbang_core::store::Entry) -> Vec<u8>,
    generation: &Generation,
) -> usize {
    let mut count = 0;
    for entry in generation.references() {
        let bytes = read(entry);
        assert_eq!(Digest::of(&bytes), entry.digest);
        count += 1;
    }
    count
}

/// The store of `with_runs` after runs `one` and `two` were deleted, each by a
/// publication of its own: generation 3, with two older generation files and two files
/// of content that nothing names any more.
fn with_deleted_runs(root: &Path) {
    with_runs(root);
    let store = Store::open(root, Arc::new(ShimFs::new())).unwrap();
    store.delete(&["one"]).unwrap();
    assert_eq!(store.delete(&["two"]).unwrap().generation, 3);
}

/// What a finished prune of `with_deleted_runs` leaves.
fn assert_pruned(root: &Path, case: &str) {
    assert_eq!(
        names(root, "generations"),
        ["3.json".to_string()].into(),
        "{case}"
    );
    assert_eq!(
        names(root, "objects"),
        digests(&[LEDGER, EXCHANGE, B]),
        "{case}"
    );
}

#[test]
fn a_prune_removes_older_generations_and_content_that_nothing_names() {
    let dir = Scratch::new("store-prune");
    let root = with_old(&dir);
    // A publication that died after its payloads were in place and before its pointer:
    // complete content that no generation names.
    let points = points();
    let dead = Arc::new(ShimFs::new().crash_before(points.pointer, Lost::ContentOnly));
    assert!(publish_new(&root, dead).is_err());
    assert_eq!(
        names(&root, "objects").len(),
        4,
        "a1, b, and the dead a2 and c"
    );

    let fs = Arc::new(ShimFs::new());
    let store = Store::open(&root, fs.clone()).unwrap();
    let mut publication = store.begin().unwrap();
    publication.put("a", B).unwrap();
    assert_eq!(publication.publish().unwrap().generation, 2);
    let before = fs.events().len();
    let pruned = store.prune().unwrap();
    assert_eq!(
        pruned,
        Pruned {
            generations_removed: vec![1],
            generations_in_use: vec![],
            objects_removed: 3,
        }
    );
    assert_eq!(names(&root, "generations"), ["2.json".to_string()].into());
    assert_eq!(names(&root, "objects"), digests(&[B]));

    // Names are removed, then their directory is synced: generations, then content.
    let events = &fs.events()[before..];
    let (generations, objects) = (root.join("generations"), root.join("objects"));
    assert_eq!(events.len(), 6, "{events:?}");
    assert_eq!(events[0], Event::Remove(generations.join("1.json")));
    assert_eq!(events[1], Event::SyncDir(generations));
    assert!(
        events[2..5]
            .iter()
            .all(|event| matches!(event, Event::Remove(path) if path.starts_with(&objects))),
        "{events:?}"
    );
    assert_eq!(events[5], Event::SyncDir(objects));

    // Nothing more to do, and nothing done; the store reads and publishes as before.
    let before = fs.events().len();
    assert_eq!(store.prune().unwrap(), Pruned::default());
    assert_eq!(fs.events().len(), before);
    let generation = store.snapshot().unwrap();
    assert_eq!(read_all(|entry| store.read(entry).unwrap(), &generation), 2);
    let mut publication = store.begin().unwrap();
    publication.put("c", C).unwrap();
    assert_eq!(publication.publish().unwrap().generation, 3);
}

#[test]
fn deleting_one_of_two_runs_that_share_an_artifact_keeps_it_and_removes_what_only_the_one_had() {
    let dir = Scratch::new("store-prune");
    let root = root(&dir);
    with_runs(&root);
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    store.delete(&["one"]).unwrap();
    let pruned = store.prune().unwrap();
    assert_eq!(pruned.objects_removed, 1);
    assert!(!object(&root, ONLY_ONE).exists(), "only run one had it");
    assert!(object(&root, MASK).exists(), "run two shares it");

    let generation = store.snapshot().unwrap();
    let mask = generation.run("two").unwrap().get("mask").unwrap();
    assert_eq!(store.read_output(mask).unwrap(), MASK);
    assert_eq!(read_all(|entry| store.read(entry).unwrap(), &generation), 5);
}

#[test]
fn a_failed_runs_diagnostics_are_read_after_reopening_and_after_other_runs_are_pruned() {
    let dir = Scratch::new("store-prune");
    let root = root(&dir);
    with_runs(&root);
    let diagnostics = |root: &Path| {
        let reader = Reader::open(root, Arc::new(RealFs)).unwrap();
        let generation = reader.snapshot().unwrap();
        let failed = generation.run("failed").unwrap();
        assert!(
            failed
                .entries()
                .values()
                .all(|entry| entry.kind == Kind::Diagnostic)
        );
        let exchange = reader.read(failed.get("exchange").unwrap()).unwrap();
        let journal = reader.read(failed.get("journal").unwrap()).unwrap();
        (exchange, journal)
    };
    // After reopening.
    assert_eq!(diagnostics(&root), (EXCHANGE.to_vec(), B.to_vec()));

    // After the other runs are deleted and pruned: one of them held the exchange's very
    // bytes as an output. That reference is gone; the diagnostic one keeps the bytes.
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    store.delete(&["one", "two"]).unwrap();
    let pruned = store.prune().unwrap();
    assert_eq!(pruned.objects_removed, 2, "the mask and run one's render");
    drop(store);
    assert_eq!(runs(&root), ["failed"]);
    assert_eq!(diagnostics(&root), (EXCHANGE.to_vec(), B.to_vec()));

    // And the record outside runs is not a prune's to remove, with every run gone.
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    store.delete(&["failed"]).unwrap();
    store.prune().unwrap();
    let generation = store.snapshot().unwrap();
    assert_eq!(
        store.read(generation.get("spend/1").unwrap()).unwrap(),
        LEDGER
    );
    assert_eq!(names(&root, "objects"), digests(&[LEDGER]));
}

#[test]
fn a_reader_goes_on_reading_its_generation_through_a_prune_at_every_step_of_it() {
    // With the reader holding generation 1, the prune has two events: the removal of
    // generation 2's file, and the sync of its directory. The reader's whole read happens
    // before each of them in turn, and after the prune.
    for at in 0..2 {
        let dir = Scratch::new("store-prune");
        let root = root(&dir);
        with_runs(&root);
        // The reader opens generation 1, which names everything.
        let reader = Arc::new(Reader::open(&root, Arc::new(ShimFs::new())).unwrap());
        let generation = reader.snapshot().unwrap();
        assert_eq!(generation.number(), 1);

        let fs = Arc::new(ShimFs::new());
        let store = Store::open(&root, fs.clone()).unwrap();
        store.delete(&["one"]).unwrap();
        store.delete(&["two"]).unwrap();
        let read = Arc::new(Mutex::new(0));
        let hook = {
            let (reader, generation, read) = (reader.clone(), generation.clone(), read.clone());
            move || {
                *read.lock().unwrap() = read_all(|entry| reader.read(entry).unwrap(), &generation);
            }
        };
        fs.pause_before(fs.events().len() + at, hook);
        let pruned = store.prune().unwrap();
        assert_eq!(
            pruned,
            Pruned {
                generations_removed: vec![2],
                generations_in_use: vec![1],
                objects_removed: 0,
            },
            "at {at}"
        );
        // After the prune as well, for as long as the reader keeps its generation.
        assert_eq!(
            read_all(|entry| reader.read(entry).unwrap(), &generation),
            7
        );
        assert_eq!(*read.lock().unwrap(), 7, "at {at}: read during the prune");

        // The reader moves on to the current generation, and the next prune takes what
        // was left for it. A reader that only now opens was never in its way.
        assert_eq!(reader.snapshot().unwrap().number(), 3);
        let pruned = store.prune().unwrap();
        assert_eq!(
            pruned,
            Pruned {
                generations_removed: vec![1],
                generations_in_use: vec![],
                objects_removed: 2,
            },
            "at {at}"
        );
        assert_pruned(&root, &format!("at {at}"));
        let current = reader.snapshot().unwrap();
        assert_eq!(read_all(|entry| reader.read(entry).unwrap(), &current), 3);
    }
}

#[test]
fn a_reader_that_is_dropped_holds_nothing() {
    let dir = Scratch::new("store-prune");
    let root = root(&dir);
    with_runs(&root);
    let reader = Reader::open(&root, Arc::new(ShimFs::new())).unwrap();
    reader.snapshot().unwrap();
    // Another reader, whose process dies with the generation open.
    let dead = Arc::new(ShimFs::new());
    let dying = Reader::open(&root, dead.clone()).unwrap();
    dying.snapshot().unwrap();

    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    store.delete(&["one", "two"]).unwrap();
    assert_eq!(store.prune().unwrap().generations_in_use, vec![1]);
    drop(reader);
    assert_eq!(store.prune().unwrap().generations_in_use, vec![1]);
    dead.crash(Lost::Unsynced);
    let pruned = store.prune().unwrap();
    assert_eq!(pruned.generations_removed, vec![1]);
    assert_eq!(pruned.objects_removed, 2);
}

/// The real file system, running a hook once, just before the first file is shared: the
/// moment between a reader's look at the pointer and its hold on the generation.
struct BeforeSharing {
    hook: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl Fs for BeforeSharing {
    fn create_dir(&self, path: &Path) -> io::Result<()> {
        RealFs.create_dir(path)
    }
    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        RealFs.write_new(path, bytes)
    }
    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        RealFs.sync_dir(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        RealFs.rename(from, to)
    }
    fn remove_all(&self, path: &Path) -> io::Result<()> {
        RealFs.remove_all(path)
    }
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        RealFs.read(path)
    }
    fn list(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        RealFs.list(path)
    }
    fn exists(&self, path: &Path) -> io::Result<bool> {
        RealFs.exists(path)
    }
    fn hold_lock_file(&self, path: &Path, holder: &[u8]) -> io::Result<Option<Box<dyn Held>>> {
        RealFs.hold_lock_file(path, holder)
    }
    fn share(&self, path: &Path) -> io::Result<Box<dyn Held>> {
        if let Some(hook) = self.hook.lock().unwrap().take() {
            hook();
        }
        RealFs.share(path)
    }
    fn try_alone(&self, path: &Path) -> io::Result<Option<Box<dyn Held>>> {
        RealFs.try_alone(path)
    }
}

#[test]
fn a_reader_whose_generation_is_pruned_before_it_holds_it_reads_the_current_one() {
    let dir = Scratch::new("store-prune");
    let root = root(&dir);
    with_runs(&root);
    // The reader has read the pointer, which says 1. Before it holds generation 1, the
    // writer deletes two runs and prunes: generation 1 and what only it named are gone.
    let hook = {
        let root = root.clone();
        move || {
            let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
            store.delete(&["one", "two"]).unwrap();
            let pruned = store.prune().unwrap();
            assert_eq!(pruned.generations_removed, vec![1]);
            assert_eq!(pruned.objects_removed, 2);
        }
    };
    let fs = Arc::new(BeforeSharing {
        hook: Mutex::new(Some(Box::new(hook))),
    });
    let reader = Reader::open(&root, fs).unwrap();
    let generation = reader.snapshot().unwrap();
    assert_eq!(generation.number(), 2, "not the one that went");
    assert_eq!(
        read_all(|entry| reader.read(entry).unwrap(), &generation),
        3
    );
}

#[test]
fn a_prune_keeps_what_a_publication_in_progress_has_named() {
    let dir = Scratch::new("store-prune");
    let root = with_old(&dir);
    // Content the store holds and nothing names: a dead publication's `a2` and `c`.
    let points = points();
    let dead = Arc::new(ShimFs::new().crash_before(points.pointer, Lost::ContentOnly));
    assert!(publish_new(&root, dead).is_err());

    let fs = Arc::new(ShimFs::new());
    let store = Arc::new(Store::open(&root, fs.clone()).unwrap());
    let mut publication = store.begin().unwrap();
    // Found in the store, so not staged; and new, so staged.
    publication.put("found", A2).unwrap();
    publication
        .put("new", b"staged by this publication")
        .unwrap();
    let pruned = store.prune().unwrap();
    assert_eq!(pruned.objects_removed, 1, "the dead c, which nobody names");
    assert!(object(&root, A2).exists());

    // And between its payloads reaching their place and its generation: the prune runs
    // just before this publication syncs the directory of its payloads.
    let ran = Arc::new(Mutex::new(None));
    let hook = {
        let (store, ran) = (store.clone(), ran.clone());
        move || *ran.lock().unwrap() = Some(store.prune().unwrap())
    };
    let sync_of_objects = fs.events().len() + 1; // after the rename of the one staged payload
    fs.pause_before(sync_of_objects, hook);
    assert_eq!(publication.publish().unwrap().generation, 2);
    assert_eq!(ran.lock().unwrap().take(), Some(Pruned::default()));
    assert_eq!(
        fs.events()[sync_of_objects],
        Event::SyncDir(root.join("objects"))
    );

    let generation = store.snapshot().unwrap();
    assert_eq!(read_all(|entry| store.read(entry).unwrap(), &generation), 4);
    // A publication that is dropped names nothing any more.
    let mut publication = store.begin().unwrap();
    publication.put("dropped", b"never published").unwrap();
    drop(publication);
    let mut publication = store.begin().unwrap();
    publication.put("c", C).unwrap();
    publication.publish().unwrap();
    assert_eq!(store.prune().unwrap().generations_removed, vec![1, 2]);
    assert_eq!(names(&root, "objects").len(), 5);
}

#[test]
fn a_crash_at_any_event_of_a_prune_leaves_the_store_whole_and_a_second_prune_finishes() {
    let prune = |root: &Path, fs: Arc<ShimFs>| Store::open(root, fs)?.prune();
    let count = {
        let dir = Scratch::new("store-prune");
        let root = root(&dir);
        with_deleted_runs(&root);
        let fs = Arc::new(ShimFs::new());
        let pruned = prune(&root, fs.clone()).unwrap();
        assert_eq!(pruned.generations_removed, vec![1, 2]);
        assert_eq!(pruned.objects_removed, 2);
        assert_pruned(&root, "uninterrupted");
        fs.events().len()
    };

    let case = |crash_at: usize, lost: Lost| -> (usize, usize) {
        let case = format!("{lost:?}, crash before event {crash_at}");
        let dir = Scratch::new("store-prune");
        let root = root(&dir);
        with_deleted_runs(&root);
        let fs = Arc::new(ShimFs::new().crash_before(crash_at, lost));
        let result = prune(&root, fs.clone());
        fs.crash(lost);

        // The current generation is whole, whatever the crash kept of the removals.
        let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
        let generation = reader.snapshot().unwrap();
        assert_eq!(generation.number(), 3, "{case}");
        assert_eq!(runs(&root), ["failed"], "{case}");
        assert_eq!(
            read_all(
                |entry| reader.read(entry).unwrap_or_else(|e| panic!("{case}: {e}")),
                &generation
            ),
            3,
            "{case}"
        );
        drop(reader);
        if result.is_ok() && lost == Lost::Unsynced {
            assert_pruned(&root, &case);
        }

        // The next writer finishes the work, and the store still publishes.
        let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
        store.prune().unwrap_or_else(|e| panic!("{case}: {e}"));
        assert_pruned(&root, &case);
        let mut publication = store.begin().unwrap();
        publication
            .put_in("next", "out", Kind::Output, MASK)
            .unwrap();
        assert_eq!(publication.publish().unwrap().generation, 4, "{case}");
        let generation = store.snapshot().unwrap();
        assert_eq!(read_all(|entry| store.read(entry).unwrap(), &generation), 4);
        (fs.crash_choices(), fs.crash_names())
    };

    let mut most_names = 0;
    for crash_at in 0..=count {
        case(crash_at, Lost::Unsynced);
        case(crash_at, Lost::ContentOnly);
        // Each subset of the directories, then each subset of the names: some removals
        // from one directory kept, others from the same directory lost.
        let (directories, names) = case(crash_at, Lost::Subset(0));
        assert!(directories < 4 && names < 8, "{directories}, {names}");
        for mask in 1..1u64 << directories {
            case(crash_at, Lost::Subset(mask));
        }
        for mask in 0..1u64 << names {
            case(crash_at, Lost::Names(mask));
        }
        most_names = most_names.max(names);
    }
    assert!(
        most_names >= 2,
        "no crash had two removals in one directory not yet durable"
    );
}

#[cfg(unix)]
#[test]
fn every_path_the_store_creates_is_for_its_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    // Everything under the root, the root included: directories 0700, files 0600.
    let owner_only = |root: &Path, at_least: usize, when: &str| {
        let mut stack = vec![root.to_path_buf()];
        let mut seen = BTreeSet::new();
        while let Some(path) = stack.pop() {
            let meta = std::fs::metadata(&path).unwrap();
            let mode = meta.permissions().mode() & 0o777;
            if meta.is_dir() {
                assert_eq!(mode, 0o700, "{when}: {}", path.display());
                stack.extend(
                    std::fs::read_dir(&path)
                        .unwrap()
                        .map(|entry| entry.unwrap().path()),
                );
            } else {
                assert_eq!(mode, 0o600, "{when}: {}", path.display());
            }
            seen.insert(path.strip_prefix(root).unwrap().to_path_buf());
        }
        assert!(seen.len() >= at_least, "{when}: {seen:?}");
        seen
    };

    let dir = Scratch::new("store-modes");
    // The directories above the root that the store has to make are its own too.
    let root = dir.path().join("made").join("store");
    let store = Store::open(&root, Arc::new(RealFs)).unwrap();
    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&dir.path().join("made")), 0o700);
    let seen = owner_only(&root, 6, "a new store");
    assert!(seen.contains(Path::new("LOCK")), "the lock file: {seen:?}");
    assert!(seen.contains(Path::new("CURRENT")));

    // Staging, while a publication is open: its directory and a payload.
    let mut publication = store.begin().unwrap();
    publication.put("spend/1", LEDGER).unwrap();
    publication
        .put_in("one", "mask", Kind::Output, MASK)
        .unwrap();
    publication
        .put_in("failed", "journal", Kind::Diagnostic, B)
        .unwrap();
    owner_only(&root, 10, "staged");
    publication.publish().unwrap();
    let seen = owner_only(&root, 10, "published");
    assert!(seen.contains(Path::new("generations/1.json")));

    // What a deletion and a prune write: a generation, a pointer.
    store.delete(&["one"]).unwrap();
    store.prune().unwrap();
    let seen = owner_only(&root, 8, "pruned");
    assert!(seen.contains(Path::new("generations/2.json")));
    assert!(!seen.contains(Path::new("generations/1.json")));
}
