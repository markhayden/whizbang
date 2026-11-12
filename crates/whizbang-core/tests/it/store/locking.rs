//! One writer (plan 1A, task A3): the lock a writer holds for its life, what an open that
//! is refused leaves alone, and what a directory must hold to be, or become, a store.
//!
//! Here a second writer is a second file system in the same process, which the lock tells
//! apart exactly as it tells two processes apart. The same cases with real processes are
//! in the tests of the `whizbang` binary.

use std::path::Path;
use std::sync::Arc;

use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Error, Reader, Store};
use whizbang_testkit::fs_shim::{Event, Lost, ShimFs};
use whizbang_testkit::scratch::Scratch;

use super::{
    A2, C, Contents, Hooked, When, after_restart, new, old, points, publish_new, root, seen, tree,
    with_old,
};

fn held(result: Result<Store, Error>) -> (std::path::PathBuf, Option<String>) {
    match result.map(|_| ()) {
        Err(Error::Held { root, holder }) => (root, holder),
        other => panic!("not refused as held: {other:?}"),
    }
}

#[test]
fn a_second_writer_is_refused_with_the_holder_named_while_a_reader_reads() {
    let dir = Scratch::new("store-lock");
    let root = with_old(&dir);
    let first = Store::open(&root, Arc::new(ShimFs::new())).unwrap();

    let second = Arc::new(ShimFs::new());
    let (named, holder) = held(Store::open(&root, second.clone()));
    assert_eq!(named, root);
    let holder = holder.expect("the holder wrote who it is");
    assert!(
        holder.starts_with(&format!("process {} (", std::process::id())),
        "{holder}"
    );
    let message = Store::open(&root, second.clone())
        .map(|_| ())
        .unwrap_err()
        .to_string();
    assert!(message.contains(&holder), "{message}");
    assert!(message.contains("one process at a time"), "{message}");
    assert_eq!(second.events(), vec![], "a refused writer changes nothing");

    // A reader is not refused, and takes nothing a writer needs.
    assert_eq!(seen(&root, Arc::new(ShimFs::new())), (1, old()));
    let mut publication = first.begin().unwrap();
    publication.put("c", C).unwrap();
    assert_eq!(publication.publish().unwrap().generation, 2);
}

#[test]
fn the_lock_ends_with_the_writer_however_it_ends() {
    let dir = Scratch::new("store-lock");
    let root = with_old(&dir);

    // Dropped.
    let first = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    held(Store::open(&root, Arc::new(ShimFs::new())));
    drop(first);
    let second = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    drop(second);

    // Dead: its process lost power with the store open.
    let fs = Arc::new(ShimFs::new());
    let dead = Store::open(&root, fs.clone()).unwrap();
    held(Store::open(&root, Arc::new(ShimFs::new())));
    fs.crash(Lost::Unsynced);
    Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    drop(dead);
}

#[test]
fn the_real_file_system_refuses_a_second_writer_and_names_the_first() {
    let dir = Scratch::new("store-lock-real");
    let root = root(&dir);
    let first = Store::open(&root, Arc::new(RealFs)).unwrap();
    let (_, holder) = held(Store::open(&root, Arc::new(RealFs)));
    assert!(
        holder
            .unwrap()
            .starts_with(&format!("process {} (", std::process::id()))
    );
    Reader::open(&root, Arc::new(RealFs)).unwrap();
    drop(first);
    Store::open(&root, Arc::new(RealFs)).unwrap();
}

#[test]
fn a_writer_paused_after_staging_keeps_its_staging_through_a_reader_and_a_refused_writer() {
    let dir = Scratch::new("store-lock");
    let root = with_old(&dir);
    // A dead writer's staging, too: the refused writer must not take that either.
    let points = points();
    let dead = Arc::new(ShimFs::new().crash_before(points.second_payload, Lost::ContentOnly));
    assert!(publish_new(&root, dead).is_err());

    // The first writer opens (which removes the dead writer's staging), stages its files
    // and stops there.
    let first = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    let mut publication = first.begin().unwrap();
    publication.put("a", A2).unwrap();
    publication.put("c", C).unwrap();
    let staged = tree(&root.join("staging"));
    assert_eq!(staged.len(), 4, "staging, one directory, two payloads");
    let before = tree(&root);

    let reader = Arc::new(ShimFs::new());
    assert_eq!(seen(&root, reader.clone()), (1, old()));
    assert_eq!(reader.events(), vec![]);
    let second = Arc::new(ShimFs::new());
    held(Store::open(&root, second.clone()));
    assert_eq!(second.events(), vec![]);
    assert_eq!(tree(&root), before, "nothing was touched");

    assert_eq!(publication.publish().unwrap().generation, 2);
    drop(first);
    assert_eq!(after_restart(&root), (2, new()));
}

#[test]
fn nothing_is_removed_before_the_lock_is_held() {
    // An open that has work of both kinds: leftover staging, and a generation file above
    // the pointer.
    let dir = Scratch::new("store-lock");
    let root = with_old(&dir);
    std::fs::create_dir(root.join("staging/leftover")).unwrap();
    std::fs::write(root.join("generations/2.json"), b"{}").unwrap();
    let fs = Arc::new(ShimFs::new());
    Store::open(&root, fs.clone()).unwrap();
    let events = fs.events();
    assert_eq!(events[0], Event::Lock(root.join("LOCK")), "{events:?}");
    assert!(events.contains(&Event::Remove(root.join("staging/leftover"))));
    assert!(events.contains(&Event::Remove(root.join("generations/2.json"))));

    // A new store: only the root and the directories above it exist before the lock.
    let root = dir.path().join("deep/down/store");
    let fs = Arc::new(ShimFs::new());
    Store::open(&root, fs.clone()).unwrap();
    let events = fs.events();
    let locked = events
        .iter()
        .position(|event| *event == Event::Lock(root.join("LOCK")))
        .unwrap();
    for event in &events[..locked] {
        match event {
            Event::CreateDir(path) => assert!(root.starts_with(path), "{event:?}"),
            Event::SyncDir(path) => assert!(root.starts_with(path) && *path != root, "{event:?}"),
            other => panic!("before the lock: {other:?}"),
        }
    }
}

#[test]
fn a_refused_writer_of_a_store_that_is_being_created_changes_nothing() {
    // The first writer is stopped at each event of creating a store; a second arrives.
    let count = {
        let dir = Scratch::new("store-lock");
        let fs = Arc::new(ShimFs::new());
        Store::open(&root(&dir), fs.clone()).unwrap();
        fs.events().len()
    };
    let mut refused = 0;
    for stop_at in 0..count {
        let dir = Scratch::new("store-lock");
        let root = root(&dir);
        let first = Arc::new(ShimFs::new());
        let outcome = Arc::new(std::sync::Mutex::new(None));
        let hook = {
            let (root, outcome) = (root.clone(), outcome.clone());
            move || {
                let before = root.exists().then(|| tree(&root));
                let second = Arc::new(ShimFs::new());
                let result = Store::open(&root, second.clone()).map(|_| ());
                if matches!(result, Err(Error::Held { .. })) {
                    assert_eq!(second.events(), vec![]);
                    assert_eq!(root.exists().then(|| tree(&root)), before);
                }
                *outcome.lock().unwrap() = Some(result);
            }
        };
        first.pause_before(stop_at, hook);
        let result = Store::open(&root, first.clone()).map(|_| ());
        let second = outcome.lock().unwrap().take().expect("the hook ran");
        match second {
            // The second came before the first held the lock, and made the store itself.
            Ok(()) => assert!(result.is_ok(), "stop {stop_at}: {result:?}"),
            Err(Error::Held { .. }) => {
                refused += 1;
                result.unwrap_or_else(|e| panic!("stop {stop_at}: the first failed: {e}"));
            }
            Err(other) => panic!("stop {stop_at}: {other:?}"),
        }
        assert_eq!(after_restart(&root), (0, Contents::new()), "stop {stop_at}");
    }
    assert!(refused >= 5, "{refused} of {count} stops met the lock");
}

#[test]
fn a_writer_that_looks_while_another_finishes_creating_the_store_is_not_misled() {
    // The first writer has staged the first pointer. The second lists staging and sees
    // it; before the second reads it, the first moves it into place and is done. The
    // file the second meant to read is gone, which is no damage and nothing foreign.
    for first_still_there in [true, false] {
        let dir = Scratch::new("store-lock");
        let root = root(&dir);
        for folder in ["objects", "generations", "staging"] {
            std::fs::create_dir_all(root.join(folder)).unwrap();
        }
        std::fs::write(root.join("staging/CURRENT"), b"0\n").unwrap();
        let first = Arc::new(std::sync::Mutex::new(None));
        let hook = {
            let (root, first) = (root.clone(), first.clone());
            move || {
                let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
                assert!(!root.join("staging/CURRENT").exists());
                *first.lock().unwrap() = first_still_there.then_some(store);
            }
        };
        let fs = Arc::new(Hooked::new(
            When::Reading(root.join("staging/CURRENT")),
            hook,
        ));
        let result = Store::open(&root, fs);
        if first_still_there {
            held(result);
        } else {
            assert_eq!(result.unwrap().snapshot().unwrap().number(), 0);
        }
    }
}

// The look a writer takes before it holds the lock is of a store that another writer may
// be changing. Whatever it seems to show, the answer is the truth: held, or what the store
// is once nobody else writes it.

/// Whether the other writer is still there when the one under test reaches the lock.
const STAYS_OR_GOES: [bool; 2] = [true, false];

#[test]
fn a_look_that_meets_a_publication_and_a_prune_reports_no_damage() {
    for holder_stays in STAYS_OR_GOES {
        let dir = Scratch::new("store-look");
        let root = with_old(&dir);
        let holder = Arc::new(std::sync::Mutex::new(Some(
            Store::open(&root, Arc::new(ShimFs::new())).unwrap(),
        )));
        // The second writer has read the pointer, which says 1. Before it reads
        // generation 1, the holder publishes generation 2 and prunes: 1.json is gone.
        let hook = {
            let (root, holder) = (root.clone(), holder.clone());
            move || {
                let mut holder = holder.lock().unwrap();
                let store = holder.as_ref().unwrap();
                let mut publication = store.begin().unwrap();
                publication.put("a", A2).unwrap();
                publication.put("c", C).unwrap();
                publication.publish().unwrap();
                assert_eq!(store.prune().unwrap().generations_removed, vec![1]);
                assert!(!root.join("generations/1.json").exists());
                if !holder_stays {
                    holder.take();
                }
            }
        };
        let fs = Arc::new(Hooked::new(
            When::Reading(root.join("generations/1.json")),
            hook,
        ));
        let result = Store::open(&root, fs);
        if holder_stays {
            let (_, name) = held(result);
            assert!(name.unwrap().starts_with("process "));
        } else {
            let store = result.unwrap_or_else(|e| panic!("a false refusal: {e}"));
            assert_eq!(store.snapshot().unwrap().number(), 2);
            drop(store);
        }
        holder.lock().unwrap().take();
        assert_eq!(seen(&root, Arc::new(RealFs)), (2, new()));
    }
}

#[test]
fn a_look_that_meets_the_end_of_a_stores_creation_does_not_say_it_is_not_a_store() {
    for holder_stays in STAYS_OR_GOES {
        let dir = Scratch::new("store-look");
        let root = root(&dir);
        std::fs::create_dir(&root).unwrap();
        // The second writer has seen that there is no pointer. Before it lists the
        // directory, the first makes the store: the listing shows CURRENT.
        let holder = Arc::new(std::sync::Mutex::new(None));
        let hook = {
            let (root, holder) = (root.clone(), holder.clone());
            move || {
                let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
                *holder.lock().unwrap() = holder_stays.then_some(store);
            }
        };
        let fs = Arc::new(Hooked::new(When::Listing(root.clone()), hook));
        let result = Store::open(&root, fs);
        if holder_stays {
            held(result);
        } else {
            let store = result.unwrap_or_else(|e| panic!("a false refusal: {e}"));
            assert_eq!(store.snapshot().unwrap().number(), 0);
        }
    }
}

#[test]
fn recovery_goes_by_what_is_read_under_the_lock_not_by_the_look_before_it() {
    let dir = Scratch::new("store-look");
    let root = with_old(&dir);
    // The second writer has looked: generation 1. Just before it takes the lock, another
    // writer opens, publishes generation 2 and is gone. A recovery that went by the look
    // would take 2.json for a leftover above the pointer and remove it.
    let second = Arc::new(ShimFs::new());
    let hook = {
        let root = root.clone();
        move || {
            assert_eq!(
                publish_new(&root, Arc::new(ShimFs::new()))
                    .unwrap()
                    .generation,
                2
            )
        }
    };
    second.pause_before(0, hook);
    let store = Store::open(&root, second.clone()).unwrap();
    assert_eq!(second.events()[0], Event::Lock(root.join("LOCK")));
    assert!(root.join("generations/2.json").exists());
    assert_eq!(store.snapshot().unwrap().number(), 2);
    drop(store);
    assert_eq!(seen(&root, Arc::new(RealFs)), (2, new()));
}

#[test]
fn a_writer_writes_its_name_once_the_store_is_sound_and_takes_it_out_when_it_ends() {
    let dir = Scratch::new("store-name");
    let root = with_old(&dir);
    let name = || std::fs::read_to_string(root.join("LOCK")).unwrap();
    assert_eq!(name(), "", "the writer that made the store has ended");

    let fs = Arc::new(ShimFs::new());
    let store = Store::open(&root, fs.clone()).unwrap();
    assert_eq!(
        name(),
        format!(
            "process {} ({})\n",
            std::process::id(),
            Path::new(&std::env::args().next().unwrap())
                .file_name()
                .unwrap()
                .to_string_lossy()
        )
    );
    // A writer that dies leaves its name; the next replaces it; a refused one, whether
    // for the lock or for damage, writes nothing.
    fs.crash(Lost::ContentOnly);
    drop(store);
    assert!(name().starts_with("process "));
    std::fs::write(root.join("LOCK"), b"process 1 (long dead)\n").unwrap();
    std::fs::write(root.join("CURRENT"), b"damaged").unwrap();
    let result = Store::open(&root, Arc::new(ShimFs::new())).map(|_| ());
    assert!(matches!(result, Err(Error::Corrupt { .. })), "{result:?}");
    assert_eq!(name(), "process 1 (long dead)\n");
    std::fs::write(root.join("CURRENT"), b"1\n").unwrap();

    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    let mine = name();
    assert!(mine.starts_with(&format!("process {} (", std::process::id())));
    held(Store::open(&root, Arc::new(ShimFs::new())));
    assert_eq!(name(), mine);
    drop(store);
    assert_eq!(name(), "");

    // With no name to read, a refused writer is told of another process.
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    std::fs::write(root.join("LOCK"), b"").unwrap();
    let error = Store::open(&root, Arc::new(ShimFs::new()))
        .map(|_| ())
        .unwrap_err();
    assert!(
        matches!(&error, Error::Held { holder: None, .. }),
        "{error:?}"
    );
    assert!(
        error.to_string().contains("written by another process"),
        "{error}"
    );
    drop(store);
}

#[test]
fn opening_a_store_that_must_exist_makes_nothing_and_completes_nothing() {
    let dir = Scratch::new("store-existing");
    // Nothing there.
    let missing = dir.path().join("missing");
    let result = Store::open_existing(&missing, Arc::new(ShimFs::new())).map(|_| ());
    assert!(matches!(result, Err(Error::Missing { .. })), "{result:?}");
    assert!(!missing.exists());

    // An empty directory, and an unfinished creation: `open` would make a store of each.
    for folders in [&[][..], &["objects", "staging"]] {
        let root = dir.path().join(format!("unfinished-{}", folders.len()));
        std::fs::create_dir(&root).unwrap();
        for folder in folders {
            std::fs::create_dir(root.join(folder)).unwrap();
        }
        let before = tree(&root);
        let fs = Arc::new(ShimFs::new());
        let result = Store::open_existing(&root, fs.clone()).map(|_| ());
        assert!(
            matches!(result, Err(Error::NotAStore { found: None, .. })),
            "{result:?}"
        );
        assert_eq!(fs.events(), vec![]);
        assert_eq!(tree(&root), before);
    }

    // A directory that holds something else: what was found is named, as by `open`.
    let other = dir.path().join("other");
    std::fs::create_dir(&other).unwrap();
    std::fs::write(other.join(".DS_Store"), b"").unwrap();
    match Store::open_existing(&other, Arc::new(ShimFs::new())).map(|_| ()) {
        Err(Error::NotAStore { found, .. }) => assert_eq!(found, Some(other.join(".DS_Store"))),
        other => panic!("{other:?}"),
    }

    // A store: opened, recovered and held, as by `open`.
    let root = with_old(&dir);
    std::fs::create_dir(root.join("staging/leftover")).unwrap();
    let store = Store::open_existing(&root, Arc::new(ShimFs::new())).unwrap();
    assert!(!root.join("staging/leftover").exists());
    held(Store::open_existing(&root, Arc::new(ShimFs::new())));
    drop(store);
}

// What is a store, and what is not, said so that the cause can be found.

fn not_a_store(root: &Path) -> (Option<std::path::PathBuf>, String) {
    let before = tree(root);
    let fs = Arc::new(ShimFs::new());
    let result = Store::open(root, fs.clone()).map(|_| ());
    assert_eq!(fs.events(), vec![]);
    assert_eq!(tree(root), before);
    match result {
        Err(error @ Error::NotAStore { .. }) => {
            let message = error.to_string();
            let Error::NotAStore { found, .. } = error else {
                unreachable!()
            };
            (found, message)
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_directory_that_is_not_a_store_is_refused_naming_what_was_found_in_it() {
    type Build = fn(&Path);
    let cases: [(&str, Build); 8] = [
        (".DS_Store", |root| {
            std::fs::write(root.join(".DS_Store"), b"").unwrap();
        }),
        ("staging/draft.txt", |root| {
            std::fs::create_dir(root.join("staging")).unwrap();
            std::fs::write(root.join("staging/draft.txt"), b"mine").unwrap();
        }),
        ("objects/thing", |root| {
            std::fs::create_dir(root.join("objects")).unwrap();
            std::fs::write(root.join("objects/thing"), b"mine").unwrap();
        }),
        ("generations/1.json", |root| {
            std::fs::create_dir(root.join("generations")).unwrap();
            std::fs::write(root.join("generations/1.json"), b"{}").unwrap();
        }),
        ("objects", |root| {
            std::fs::write(root.join("objects"), b"a file, not a directory").unwrap();
        }),
        ("LOCK", |root| {
            std::fs::create_dir(root.join("LOCK")).unwrap();
        }),
        // The first pointer on its way is a file, and holds 0 and a newline or the start
        // of that.
        ("staging/CURRENT", |root| {
            std::fs::create_dir_all(root.join("staging/CURRENT")).unwrap();
        }),
        ("staging/CURRENT", |root| {
            std::fs::create_dir(root.join("staging")).unwrap();
            std::fs::write(root.join("staging/CURRENT"), b"1\n").unwrap();
        }),
    ];
    for (stray, build) in cases {
        let dir = Scratch::new("store-what");
        let root = root(&dir);
        std::fs::create_dir(&root).unwrap();
        build(&root);
        let (found, message) = not_a_store(&root);
        assert_eq!(found, Some(root.join(stray)), "{stray}");
        assert!(
            message.contains(&root.join(stray).display().to_string()),
            "{message}"
        );
    }
}

#[test]
fn a_staged_first_pointer_is_accepted_whole_or_cut_short_and_nothing_else_is() {
    for (bytes, accepted) in [
        (&b"0\n"[..], true),
        (b"0", true),
        (b"", true),
        (b"0\n0", false),
        (b"7\n", false),
        (b"\n", false),
        (b"not a pointer", false),
    ] {
        let dir = Scratch::new("store-what");
        let root = root(&dir);
        std::fs::create_dir_all(root.join("staging")).unwrap();
        std::fs::write(root.join("staging/CURRENT"), bytes).unwrap();
        if accepted {
            assert_eq!(after_restart(&root), (0, Contents::new()), "{bytes:?}");
        } else {
            let (found, _) = not_a_store(&root);
            assert_eq!(found, Some(root.join("staging/CURRENT")), "{bytes:?}");
        }
    }
}

#[test]
fn a_lock_file_alone_is_what_an_unfinished_creation_leaves() {
    let dir = Scratch::new("store-what");
    let root = root(&dir);
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("LOCK"), b"process 1 (gone)\n").unwrap();
    assert_eq!(after_restart(&root), (0, Contents::new()));
}

#[test]
fn a_reader_says_only_that_there_is_no_pointer() {
    let dir = Scratch::new("store-what");
    let root = root(&dir);
    std::fs::create_dir(&root).unwrap();
    let error = Reader::open(&root, Arc::new(RealFs))
        .map(|_| ())
        .unwrap_err();
    assert!(
        matches!(&error, Error::NotAStore { found: None, .. }),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        format!("{} is not a store: it has no CURRENT", root.display())
    );
}

#[cfg(unix)]
#[test]
fn a_directory_that_cannot_be_listed_is_an_error_of_the_file_system_not_a_refusal() {
    use std::os::unix::fs::PermissionsExt;

    let dir = Scratch::new("store-what");
    let root = root(&dir);
    std::fs::create_dir_all(root.join("objects")).unwrap();
    let closed = std::fs::Permissions::from_mode(0o000);
    std::fs::set_permissions(root.join("objects"), closed).unwrap();
    // Whoever may read everything (root) is not refused the listing: nothing to test.
    if std::fs::read_dir(root.join("objects")).is_err() {
        let result = Store::open(&root, Arc::new(ShimFs::new())).map(|_| ());
        match result {
            Err(Error::Io { doing, path, .. }) => {
                assert_eq!((doing, path), ("list", root.join("objects")));
            }
            other => panic!("{other:?}"),
        }
    }
    let open = std::fs::Permissions::from_mode(0o700);
    std::fs::set_permissions(root.join("objects"), open).unwrap();
}
