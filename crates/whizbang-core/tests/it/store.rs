//! The store's publication and recovery (plan 1A, task A2). Crashes come from the testkit's
//! file-system shim, which drops what was not synced; no test here raises an error and
//! reopens the same files to stand for a crash.
//!
//! The one writer, and what is and is not a store, are in `store/locking.rs` (task A3).

mod locking;
mod prune;
mod runs;

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use whizbang_core::store::fs::{Fs, Held, LockFile, RealFs};
use whizbang_core::store::{Digest, Error, Published, Reader, Store};
use whizbang_testkit::fs_shim::{Event, Lost, ShimFs};
use whizbang_testkit::scratch::Scratch;

const A1: &[u8] = b"the first content of a";
const A2: &[u8] = b"the second content of a, which is longer";
const B: &[u8] = b"the content of b";
const C: &[u8] = b"the content of c";

type Contents = BTreeMap<String, Vec<u8>>;

fn contents(pairs: &[(&str, &[u8])]) -> Contents {
    pairs
        .iter()
        .map(|(name, bytes)| (name.to_string(), bytes.to_vec()))
        .collect()
}

/// Generation 1, the old one.
fn old() -> Contents {
    contents(&[("a", A1), ("b", B)])
}

/// Generation 2, the new one: `a` replaced, `b` kept, `c` added.
fn new() -> Contents {
    contents(&[("a", A2), ("b", B), ("c", C)])
}

fn root(dir: &Scratch) -> PathBuf {
    dir.path().join("store")
}

/// A store that holds the old generation, written by a process that ended cleanly.
fn with_old(dir: &Scratch) -> PathBuf {
    let root = root(dir);
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    let mut publication = store.begin().unwrap();
    publication.put("a", A1).unwrap();
    publication.put("b", B).unwrap();
    assert_eq!(publication.publish().unwrap().generation, 1);
    root
}

/// The process under test: opens the store and publishes the new generation.
fn publish_new(root: &Path, fs: Arc<ShimFs>) -> Result<Published, Error> {
    let store = Store::open(root, fs)?;
    let mut publication = store.begin()?;
    publication.put("a", A2)?;
    publication.put("c", C)?;
    publication.publish()
}

/// What a reader sees: the generation's number, and every name with the bytes read back
/// through its digest. Reading fails unless every entry is whole, so a value returned here
/// is a complete generation.
fn seen(root: &Path, fs: Arc<dyn Fs>) -> (u64, Contents) {
    let reader = Reader::open(root, fs).unwrap();
    let generation = reader.snapshot().unwrap();
    let mut contents = Contents::new();
    for (name, entry) in generation.entries() {
        let bytes = reader.read(entry).unwrap();
        assert_eq!(bytes.len() as u64, entry.bytes, "{name}");
        assert_eq!(Digest::of(&bytes), entry.digest, "{name}");
        contents.insert(name.clone(), bytes);
    }
    (generation.number(), contents)
}

/// After a restart: the next writer opens the store, which recovers it, and what it then
/// holds is returned. Nothing is left in staging.
fn after_restart(root: &Path) -> (u64, Contents) {
    let fs = Arc::new(ShimFs::new());
    let store = Store::open(root, fs.clone()).unwrap();
    assert_eq!(
        std::fs::read_dir(root.join("staging")).unwrap().count(),
        0,
        "leftover staging is removed on open"
    );
    drop(store);
    seen(root, fs)
}

fn position(events: &[Event], what: &str, matches: impl Fn(&Event) -> bool) -> usize {
    events
        .iter()
        .position(matches)
        .unwrap_or_else(|| panic!("no event: {what}"))
}

fn last_position(events: &[Event], what: &str, matches: impl Fn(&Event) -> bool) -> usize {
    events
        .iter()
        .rposition(matches)
        .unwrap_or_else(|| panic!("no event: {what}"))
}

/// The events of one uninterrupted run of `publish_new` on a store holding the old
/// generation.
fn recorded() -> (PathBuf, Vec<Event>) {
    let dir = Scratch::new("store-recorded");
    let root = with_old(&dir);
    let fs = Arc::new(ShimFs::new());
    publish_new(&root, fs.clone()).unwrap();
    (root, fs.events())
}

/// The points of a publication that the plan names, as event numbers.
struct Points {
    /// The write of the second payload file: a crash before it is a crash while payloads
    /// are being written.
    second_payload: usize,
    /// The rename that replaces the pointer.
    pointer: usize,
    /// The sync of the pointer's directory: the last step before success is reported.
    final_sync: usize,
    /// How many events the whole run has.
    count: usize,
}

fn points() -> Points {
    let (root, events) = recorded();
    let in_staging = |path: &Path| path.starts_with(root.join("staging"));
    let payload = |event: &Event| {
        matches!(event, Event::Write(path)
            if in_staging(path) && Digest::from_hex(&path.file_name().unwrap().to_string_lossy()).is_some())
    };
    let first_payload = position(&events, "a payload write", payload);
    let second_payload = first_payload
        + 1
        + position(
            &events[first_payload + 1..],
            "a second payload write",
            payload,
        );
    let pointer = position(
        &events,
        "the pointer rename",
        |event| matches!(event, Event::Rename(_, to) if *to == root.join("CURRENT")),
    );
    let final_sync = last_position(
        &events,
        "the sync of the root",
        |event| matches!(event, Event::SyncDir(path) if *path == root),
    );
    assert!(second_payload < pointer && pointer < final_sync);
    Points {
        second_payload,
        pointer,
        final_sync,
        count: events.len(),
    }
}

/// When a [`Hooked`] file system runs its hook.
enum When {
    /// Just before the first file is shared: the moment between a reader's look at the
    /// pointer and its hold on the generation.
    Sharing,
    /// Just before the first read of this file.
    Reading(PathBuf),
    /// Just before the first listing of this directory.
    Listing(PathBuf),
}

/// The real file system, running a hook once at a chosen call: what another process does
/// between two calls of the one under test, at a point the shim's events do not reach.
struct Hooked {
    when: When,
    hook: std::sync::Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl Hooked {
    fn new(when: When, hook: impl FnOnce() + Send + 'static) -> Hooked {
        Hooked {
            when,
            hook: std::sync::Mutex::new(Some(Box::new(hook))),
        }
    }

    fn run(&self) {
        if let Some(hook) = self.hook.lock().unwrap().take() {
            hook();
        }
    }
}

impl Fs for Hooked {
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
        if matches!(&self.when, When::Reading(file) if file == path) {
            self.run();
        }
        RealFs.read(path)
    }
    fn list(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        if matches!(&self.when, When::Listing(dir) if dir == path) {
            self.run();
        }
        RealFs.list(path)
    }
    fn exists(&self, path: &Path) -> io::Result<bool> {
        RealFs.exists(path)
    }
    fn hold_lock_file(&self, path: &Path, create: bool) -> io::Result<Option<Box<dyn LockFile>>> {
        RealFs.hold_lock_file(path, create)
    }
    fn share(&self, path: &Path) -> io::Result<Box<dyn Held>> {
        if matches!(self.when, When::Sharing) {
            self.run();
        }
        RealFs.share(path)
    }
    fn try_alone(&self, path: &Path) -> io::Result<Option<Box<dyn Held>>> {
        RealFs.try_alone(path)
    }
}

// Content-addressed files, staged on the store's own file system.

#[test]
fn content_is_named_by_its_sha_256() {
    let digest = Digest::of(b"hello");
    assert_eq!(
        digest.to_string(),
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
    assert_eq!(Digest::from_hex(&digest.to_string()), Some(digest));
    assert_eq!(Digest::from_hex("2cf2"), None);
    assert_eq!(Digest::from_hex(&digest.to_string().to_uppercase()), None);
}

#[test]
fn a_payload_is_staged_under_the_store_and_published_under_its_digest() {
    let dir = Scratch::new("store");
    let root = root(&dir);
    let fs = Arc::new(ShimFs::new());
    let store = Store::open(&root, fs.clone()).unwrap();
    let opened = fs.events().len();
    let mut publication = store.begin().unwrap();
    let digest = publication.put("a", A1).unwrap();
    assert_eq!(digest, Digest::of(A1));

    // Staged inside the store's own directory, so the move into place is a rename within
    // one file system; synced as it is written; and not yet where a reader would find it.
    let events = fs.events();
    let staged: Vec<PathBuf> = events[opened..]
        .iter()
        .filter_map(|event| match event {
            Event::Write(path) => Some(path.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(staged.len(), 1);
    assert!(staged[0].starts_with(root.join("staging")), "{staged:?}");
    assert_eq!(
        staged[0].file_name().unwrap().to_string_lossy(),
        digest.to_string()
    );
    assert_eq!(events.last(), Some(&Event::SyncFile(staged[0].clone())));
    assert!(!root.join("objects").join(digest.to_string()).exists());

    publication.publish().unwrap();
    let object = root.join("objects").join(digest.to_string());
    assert_eq!(std::fs::read(&object).unwrap(), A1);
    assert!(
        fs.events()
            .contains(&Event::Rename(staged[0].clone(), object)),
        "the staged file itself was renamed into place"
    );
    assert_eq!(seen(&root, fs), (1, contents(&[("a", A1)])));
}

#[test]
fn the_same_bytes_are_stored_once() {
    let dir = Scratch::new("store");
    let root = root(&dir);
    let fs = Arc::new(ShimFs::new());
    let store = Store::open(&root, fs.clone()).unwrap();
    let mut publication = store.begin().unwrap();
    let first = publication.put("one", B).unwrap();
    let second = publication.put("two", B).unwrap();
    assert_eq!(first, second);
    publication.publish().unwrap();
    assert_eq!(std::fs::read_dir(root.join("objects")).unwrap().count(), 1);

    // A later publication of bytes the store already holds writes no payload.
    let before = fs.events().len();
    let mut publication = store.begin().unwrap();
    publication.put("three", B).unwrap();
    publication.publish().unwrap();
    let name = Digest::of(B).to_string();
    assert!(
        !fs.events()[before..]
            .iter()
            .any(|event| matches!(event, Event::Write(path) if path.ends_with(&name))),
        "the payload was written again"
    );
    assert_eq!(std::fs::read_dir(root.join("objects")).unwrap().count(), 1);
    assert_eq!(
        seen(&root, fs),
        (2, contents(&[("one", B), ("two", B), ("three", B)]))
    );
}

#[test]
fn a_stored_file_that_no_longer_matches_its_entry_is_refused_by_its_digest() {
    // Other bytes of the same length; fewer bytes; no file at all.
    let same_length: Vec<u8> = B.iter().map(|byte| byte ^ 1).collect();
    for (damage, reason) in [
        (Some(&same_length[..]), "has the digest"),
        (Some(&B[..4]), "is 4 bytes here"),
        (None, "is missing"),
    ] {
        let dir = Scratch::new("store");
        let root = with_old(&dir);
        let object = root.join("objects").join(Digest::of(B).to_string());
        std::fs::remove_file(&object).unwrap();
        if let Some(bytes) = damage {
            std::fs::write(&object, bytes).unwrap();
        }
        let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
        let generation = reader.snapshot().unwrap();
        match reader.read(generation.get("b").unwrap()) {
            Err(Error::Corrupt {
                path,
                reason: found,
            }) => {
                assert_eq!(path, object);
                assert!(found.contains(reason), "{found}");
                assert!(found.contains(&Digest::of(B).to_string()), "{found}");
            }
            other => panic!("{reason}: {other:?}"),
        }
        assert_eq!(reader.read(generation.get("a").unwrap()).unwrap(), A1);
    }
}

// A new immutable generation, then one pointer; the order of the syncs.

#[test]
fn files_are_synced_then_their_directory_then_the_pointer_then_its_directory() {
    let (root, events) = recorded();
    let at = |what: &str, wanted: Event| position(&events, what, |event| *event == wanted);
    let objects = root.join("objects");
    let generations = root.join("generations");

    // Each payload: its content synced, then renamed into place.
    let mut last_payload_rename = 0;
    for bytes in [A2, C] {
        let name = Digest::of(bytes).to_string();
        let staged = match &events[position(
            &events,
            "the payload's write",
            |event| matches!(event, Event::Write(path) if path.ends_with(&name)),
        )] {
            Event::Write(path) => path.clone(),
            other => panic!("{other:?}"),
        };
        let synced = at("the payload's sync", Event::SyncFile(staged.clone()));
        let renamed = at(
            "the payload's rename",
            Event::Rename(staged, objects.join(&name)),
        );
        assert!(
            synced < renamed,
            "{name}: renamed into place before it was synced"
        );
        last_payload_rename = last_payload_rename.max(renamed);
    }
    // Then the directory the payloads are now in.
    let objects_synced = at("the sync of objects", Event::SyncDir(objects.clone()));
    assert!(last_payload_rename < objects_synced);

    // Then the generation: synced, renamed to its number, and its directory synced.
    let generation_renamed = position(
        &events,
        "the generation's rename",
        |event| matches!(event, Event::Rename(_, to) if *to == generations.join("2.json")),
    );
    let generation_staged = match &events[generation_renamed] {
        Event::Rename(from, _) => from.clone(),
        other => panic!("{other:?}"),
    };
    let generation_synced = at("the generation's sync", Event::SyncFile(generation_staged));
    let generations_synced = at(
        "the sync of generations",
        Event::SyncDir(generations.clone()),
    );
    assert!(
        objects_synced < generation_synced,
        "payloads are durable before the generation"
    );
    assert!(generation_synced < generation_renamed);
    assert!(generation_renamed < generations_synced);

    // Then the pointer: written beside, synced, renamed over the old one; then its
    // directory, after which nothing but tidying up remains.
    let pointer_renamed = position(
        &events,
        "the pointer's rename",
        |event| matches!(event, Event::Rename(_, to) if *to == root.join("CURRENT")),
    );
    let pointer_staged = match &events[pointer_renamed] {
        Event::Rename(from, _) => from.clone(),
        other => panic!("{other:?}"),
    };
    let pointer_synced = at("the pointer's sync", Event::SyncFile(pointer_staged));
    let root_synced = last_position(
        &events,
        "the sync of the root",
        |event| matches!(event, Event::SyncDir(path) if *path == root),
    );
    assert!(
        generations_synced < pointer_synced,
        "the generation is durable before the pointer"
    );
    assert!(pointer_synced < pointer_renamed);
    assert!(pointer_renamed < root_synced);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Rename(_, to) if *to == root.join("CURRENT")))
            .count(),
        1,
        "one pointer, replaced once"
    );
    for event in &events[root_synced + 1..] {
        assert!(
            matches!(event, Event::Remove(path) if path.starts_with(root.join("staging"))),
            "after the final sync: {event:?}"
        );
    }
}

#[test]
fn a_published_generation_is_never_written_again() {
    let dir = Scratch::new("store");
    let root = with_old(&dir);
    let first = std::fs::read(root.join("generations/1.json")).unwrap();
    assert_eq!(std::fs::read(root.join("CURRENT")).unwrap(), b"1\n");

    publish_new(&root, Arc::new(ShimFs::new())).unwrap();
    assert_eq!(
        std::fs::read(root.join("generations/1.json")).unwrap(),
        first
    );
    assert_eq!(std::fs::read(root.join("CURRENT")).unwrap(), b"2\n");
    assert_eq!(after_restart(&root), (2, new()));
}

#[test]
fn a_store_that_never_published_holds_the_empty_generation() {
    let dir = Scratch::new("store");
    let root = root(&dir);
    let fs = Arc::new(ShimFs::new());
    let store = Store::open(&root, fs.clone()).unwrap();
    assert_eq!(store.snapshot().unwrap().number(), 0);
    assert_eq!(seen(&root, fs), (0, Contents::new()));
}

// Visibility and durability are two guarantees.

#[test]
fn replacing_the_pointer_makes_the_generation_visible_before_it_is_durable() {
    let dir = Scratch::new("store");
    let root = with_old(&dir);
    let points = points();

    // The final sync is refused: the pointer was replaced and its directory never synced.
    let fs = Arc::new(ShimFs::new().fail_before(points.final_sync));
    let result = publish_new(&root, fs.clone());

    // Success is not reported, and the error says which generation is in doubt.
    match result {
        Err(Error::Unacknowledged { generation, .. }) => assert_eq!(generation, 2),
        other => panic!("{other:?}"),
    }
    // Visible: a reader sees the new generation, whole.
    assert_eq!(seen(&root, fs.clone()), (2, new()));
    // Not durable: power is lost, and the old generation is what the disk holds.
    fs.crash(Lost::Unsynced);
    assert_eq!(after_restart(&root), (1, old()));
}

#[test]
fn before_the_pointer_is_replaced_a_reader_sees_the_old_generation() {
    let dir = Scratch::new("store");
    let root = with_old(&dir);
    let fs = Arc::new(ShimFs::new().fail_before(points().pointer));
    let result = publish_new(&root, fs.clone());
    assert!(matches!(result, Err(Error::Io { .. })), "{result:?}");
    assert_eq!(seen(&root, fs), (1, old()));
}

#[test]
fn success_is_reported_only_after_the_final_sync() {
    let points = points();
    for crash_at in 0..=points.count {
        let dir = Scratch::new("store");
        let root = with_old(&dir);
        let fs = Arc::new(ShimFs::new().crash_before(crash_at, Lost::Unsynced));
        let result = publish_new(&root, fs.clone());
        assert_eq!(
            result.is_ok(),
            crash_at > points.final_sync,
            "crash before event {crash_at}: {result:?}"
        );
    }
}

// Recovery: the four points the plan names.

#[test]
fn a_crash_while_payloads_are_written_leaves_the_old_generation() {
    let points = points();
    for lost in [Lost::Unsynced, Lost::ContentOnly] {
        let dir = Scratch::new("store");
        let root = with_old(&dir);
        let fs = Arc::new(ShimFs::new().crash_before(points.second_payload, lost));
        assert!(publish_new(&root, fs.clone()).is_err());
        assert!(fs.crashed());
        assert_eq!(after_restart(&root), (1, old()), "{lost:?}");
    }
}

#[test]
fn a_crash_after_the_payloads_and_before_the_pointer_leaves_the_old_generation() {
    let points = points();
    for lost in [Lost::Unsynced, Lost::ContentOnly] {
        let dir = Scratch::new("store");
        let root = with_old(&dir);
        let fs = Arc::new(ShimFs::new().crash_before(points.pointer, lost));
        assert!(publish_new(&root, fs.clone()).is_err());
        assert!(fs.crashed());
        // Both payloads and the generation file were on their way; nothing names them, and
        // the generation file that was never published is not kept.
        assert_eq!(after_restart(&root), (1, old()), "{lost:?}");
        assert!(!root.join("generations/2.json").exists(), "{lost:?}");
    }
}

#[test]
fn a_crash_after_the_pointer_and_before_its_directory_sync_leaves_one_whole_generation() {
    let points = points();
    // The disk that drops the unsynced rename holds the old generation; the disk that kept
    // it holds the new one. Either is complete, and neither was reported as a success.
    for (lost, expected) in [
        (Lost::Unsynced, (1, old())),
        (Lost::ContentOnly, (2, new())),
    ] {
        let dir = Scratch::new("store");
        let root = with_old(&dir);
        let fs = Arc::new(ShimFs::new().crash_before(points.final_sync, lost));
        assert!(publish_new(&root, fs.clone()).is_err());
        assert!(fs.crashed());
        assert_eq!(after_restart(&root), expected, "{lost:?}");
    }
}

#[test]
fn a_crash_after_the_final_sync_leaves_the_new_generation() {
    let points = points();
    for lost in [Lost::Unsynced, Lost::ContentOnly] {
        // In the tidying up that follows the final sync.
        let dir = Scratch::new("store");
        let root = with_old(&dir);
        let fs = Arc::new(ShimFs::new().crash_before(points.final_sync + 1, lost));
        let _ = publish_new(&root, fs.clone());
        assert!(fs.crashed());
        assert_eq!(after_restart(&root), (2, new()), "{lost:?}");

        // And once success was reported to the caller.
        let dir = Scratch::new("store");
        let root = with_old(&dir);
        let fs = Arc::new(ShimFs::new());
        assert_eq!(publish_new(&root, fs.clone()).unwrap().generation, 2);
        fs.crash(lost);
        assert_eq!(after_restart(&root), (2, new()), "{lost:?}");
    }
}

// Recovery: every point, not only the named ones.

/// Runs a case under every loss a crash can bring: every unsynced name lost, none lost,
/// and each subset of the directories that held unsynced names. The case returns how many
/// such directories its crash found.
fn for_every_loss(case: impl Fn(Lost) -> usize) {
    case(Lost::Unsynced);
    case(Lost::ContentOnly);
    let mut mask = 0;
    loop {
        let choices = case(Lost::Subset(mask));
        assert!(choices < 16, "{choices} directories with unsynced names");
        mask += 1;
        if mask >= 1 << choices {
            break;
        }
    }
}

#[test]
fn a_crash_at_any_event_leaves_one_whole_generation_and_a_store_that_still_publishes() {
    let points = points();
    for crash_at in 0..=points.count {
        for_every_loss(|lost| {
            let case = format!("{lost:?}, crash before event {crash_at}");
            let dir = Scratch::new("store");
            let root = with_old(&dir);
            let fs = Arc::new(ShimFs::new().crash_before(crash_at, lost));
            let result = publish_new(&root, fs.clone());
            // Past the last event there is no crash point: power is lost after the run.
            fs.crash(lost);

            let (number, held) = after_restart(&root);
            // The new generation is durable once the pointer's directory was synced; a disk
            // that keeps unsynced names shows it from the rename on; a disk that keeps
            // some shows one or the other.
            let is_new = match lost {
                Lost::Unsynced => Some(crash_at > points.final_sync),
                Lost::ContentOnly => Some(crash_at > points.pointer),
                Lost::Subset(_) | Lost::Names(_) => None,
            };
            match is_new.unwrap_or(number == 2) {
                true => assert_eq!((number, &held), (2, &new()), "{case}"),
                false => assert_eq!((number, &held), (1, &old()), "{case}"),
            }
            if result.is_ok() {
                assert_eq!(number, 2, "{case}: success was reported");
            }

            // The recovered store takes the next publication, on top of what it holds.
            let next = Arc::new(ShimFs::new());
            let store = Store::open(&root, next.clone()).unwrap();
            let mut publication = store.begin().unwrap();
            publication.put("d", b"after the crash").unwrap();
            assert_eq!(
                publication.publish().unwrap().generation,
                number + 1,
                "{case}"
            );
            let mut expected = held;
            expected.insert("d".to_string(), b"after the crash".to_vec());
            next.crash(lost);
            assert_eq!(after_restart(&root), (number + 1, expected), "{case}");
            fs.crash_choices()
        });
    }
}

#[test]
fn a_crash_at_any_event_of_a_stores_first_publication_leaves_nothing_or_all_of_it() {
    let first = |root: &Path, fs: Arc<ShimFs>| -> Result<Published, Error> {
        let store = Store::open(root, fs)?;
        let mut publication = store.begin()?;
        publication.put("a", A1)?;
        publication.put("b", B)?;
        publication.publish()
    };
    let count = {
        let dir = Scratch::new("store");
        let fs = Arc::new(ShimFs::new());
        first(&root(&dir), fs.clone()).unwrap();
        fs.events().len()
    };
    for crash_at in 0..=count {
        for_every_loss(|lost| {
            let case = format!("{lost:?}, crash before event {crash_at}");
            let dir = Scratch::new("store");
            let root = root(&dir);
            let fs = Arc::new(ShimFs::new().crash_before(crash_at, lost));
            let result = first(&root, fs.clone());
            fs.crash(lost);

            let (number, held) = after_restart(&root);
            match number {
                0 => assert!(held.is_empty(), "{case}"),
                1 => assert_eq!(held, old(), "{case}"),
                other => panic!("{case}: generation {other}"),
            }
            if result.is_ok() {
                assert_eq!(number, 1, "{case}: success was reported");
            }
            fs.crash_choices()
        });
    }
}

#[test]
fn opening_to_write_removes_leftover_staging_and_opening_to_read_removes_nothing() {
    let points = points();
    let dir = Scratch::new("store");
    let root = with_old(&dir);
    // A disk that keeps names: the dead writer's staging directory is still there.
    let fs = Arc::new(ShimFs::new().crash_before(points.second_payload, Lost::ContentOnly));
    assert!(publish_new(&root, fs).is_err());
    let leftover = || std::fs::read_dir(root.join("staging")).unwrap().count();
    assert_eq!(leftover(), 1);

    let fs = Arc::new(ShimFs::new());
    assert_eq!(seen(&root, fs.clone()), (1, old()));
    assert_eq!(fs.events(), vec![], "a reader changes nothing");
    assert_eq!(leftover(), 1);

    Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    assert_eq!(leftover(), 0);
}

#[test]
fn a_publication_that_is_dropped_leaves_nothing_behind() {
    let dir = Scratch::new("store");
    let root = with_old(&dir);
    let fs = Arc::new(ShimFs::new());
    let store = Store::open(&root, fs.clone()).unwrap();
    let mut publication = store.begin().unwrap();
    publication.put("c", C).unwrap();
    drop(publication);
    assert_eq!(std::fs::read_dir(root.join("staging")).unwrap().count(), 0);
    assert_eq!(seen(&root, fs), (1, old()));
}

#[test]
fn a_reader_of_a_store_that_does_not_exist_is_refused_and_creates_nothing() {
    let dir = Scratch::new("store");
    let root = root(&dir);
    assert!(matches!(
        Reader::open(&root, Arc::new(RealFs)),
        Err(Error::Missing { .. })
    ));
    assert!(!root.exists());
}

#[test]
fn a_pointer_or_a_generation_that_cannot_be_read_is_an_error_not_an_empty_store() {
    let entry = |bytes: &[u8]| {
        format!(
            r#"{{"digest": "{}", "bytes": {}, "kind": "output"}}"#,
            Digest::of(bytes),
            bytes.len()
        )
    };
    let generation = |format: u32, number: u64, entries: &str| {
        format!(
            r#"{{"format": {format}, "number": {number}, "entries": {{{entries}}}, "runs": {{}}}}"#
        )
    };
    let a = format!(r#""a": {}"#, entry(A1));
    for (what, file, bytes) in [
        (
            "a pointer that is not a number",
            "CURRENT",
            Some("two\n".to_string()),
        ),
        (
            "a pointer without its newline",
            "CURRENT",
            Some("1".to_string()),
        ),
        (
            "a pointer written another way",
            "CURRENT",
            Some("01\n".to_string()),
        ),
        ("a pointer with a sign", "CURRENT", Some("+1\n".to_string())),
        (
            "a pointer too large for a number",
            "CURRENT",
            Some("18446744073709551616\n".to_string()),
        ),
        (
            "a pointer to a generation with no file",
            "CURRENT",
            Some("7\n".to_string()),
        ),
        (
            "a generation that is not one",
            "generations/1.json",
            Some("{}".to_string()),
        ),
        (
            "another format",
            "generations/1.json",
            Some(generation(1, 1, &a)),
        ),
        (
            "another generation's number",
            "generations/1.json",
            Some(generation(2, 5, &a)),
        ),
        (
            "a name twice",
            "generations/1.json",
            Some(generation(2, 1, &format!("{a}, {a}"))),
        ),
        (
            "a field it does not know",
            "generations/1.json",
            Some(generation(2, 1, &a).replace("{\"format\"", "{\"more\": 1, \"format\"")),
        ),
        (
            "an entry that does not say what kind of reference it is",
            "generations/1.json",
            Some(generation(2, 1, &a).replace(", \"kind\": \"output\"", "")),
        ),
        (
            "a kind it does not know",
            "generations/1.json",
            Some(generation(2, 1, &a).replace("\"output\"", "\"input\"")),
        ),
        (
            "no runs",
            "generations/1.json",
            Some(generation(2, 1, &a).replace(", \"runs\": {}", "")),
        ),
        (
            "a run twice",
            "generations/1.json",
            Some(generation(2, 1, &a).replace(
                "\"runs\": {}",
                "\"runs\": {\"r\": {\"entries\": {}}, \"r\": {\"entries\": {}}}",
            )),
        ),
    ] {
        let dir = Scratch::new("store");
        let root = with_old(&dir);
        std::fs::remove_file(root.join(file)).unwrap();
        if let Some(bytes) = bytes {
            std::fs::write(root.join(file), bytes).unwrap();
        }
        // Damage; or, for the file that says so itself, another format.
        let refused = |result: &Result<(), Error>| match result {
            Err(Error::Corrupt { .. }) => what != "another format",
            Err(Error::OtherFormat { format: 1, .. }) => what == "another format",
            _ => false,
        };
        let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
        let result = reader.snapshot().map(|_| ());
        assert!(refused(&result), "{what}: {result:?}");
        // A writer refuses it too, and tidies nothing on the way. The fault is one a
        // writer at work could seem to cause, so it is confirmed under the lock: the lock
        // is taken, and nothing else happens.
        std::fs::create_dir(root.join("staging/leftover")).unwrap();
        let before = tree(&root);
        let fs = Arc::new(ShimFs::new());
        let result = Store::open(&root, fs.clone()).map(|_| ());
        assert!(refused(&result), "{what}: {result:?}");
        assert_eq!(fs.events(), vec![Event::Lock(root.join("LOCK"))], "{what}");
        assert_eq!(tree(&root), before, "{what}");
    }

    // The same generation file, well formed, is read: the cases above fail for what they
    // change and nothing else.
    let dir = Scratch::new("store");
    let root = with_old(&dir);
    std::fs::write(root.join("generations/1.json"), generation(2, 1, &a)).unwrap();
    assert_eq!(seen(&root, Arc::new(RealFs)), (1, contents(&[("a", A1)])));
}

// The real file system.

#[test]
fn the_real_file_system_publishes_and_reopens() {
    let dir = Scratch::new("store-real");
    let root = dir.path().join("deeper").join("store");
    let store = Store::open(&root, Arc::new(RealFs)).unwrap();
    let mut publication = store.begin().unwrap();
    publication.put("a", A1).unwrap();
    publication.put("b", B).unwrap();
    assert_eq!(publication.publish().unwrap().generation, 1);
    drop(store);

    let store = Store::open(&root, Arc::new(RealFs)).unwrap();
    let mut publication = store.begin().unwrap();
    publication.put("a", A2).unwrap();
    publication.put("c", C).unwrap();
    assert_eq!(publication.publish().unwrap().generation, 2);
    assert_eq!(seen(&root, Arc::new(RealFs)), (2, new()));
    assert_eq!(std::fs::read_dir(root.join("staging")).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn what_the_real_file_system_creates_is_readable_by_its_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let dir = Scratch::new("store-real");
    let root = root(&dir);
    let store = Store::open(&root, Arc::new(RealFs)).unwrap();
    let mut publication = store.begin().unwrap();
    publication.put("a", A1).unwrap();
    // While the publication is open, so that its staging is looked at too.
    let mut stack = vec![root.clone()];
    let mut looked_at = 0;
    while let Some(path) = stack.pop() {
        let meta = std::fs::metadata(&path).unwrap();
        let mode = meta.permissions().mode() & 0o777;
        if meta.is_dir() {
            assert_eq!(mode, 0o700, "{}", path.display());
            stack.extend(
                std::fs::read_dir(&path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path()),
            );
        } else {
            assert_eq!(mode, 0o600, "{}", path.display());
        }
        looked_at += 1;
    }
    assert!(looked_at >= 6, "{looked_at}");
    publication.publish().unwrap();
    for file in ["CURRENT", "generations/1.json"] {
        let mode = std::fs::metadata(root.join(file))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "{file}");
    }
}

// Publications of one process.

#[test]
fn a_publication_builds_on_the_generation_current_when_it_publishes() {
    let dir = Scratch::new("store");
    let root = with_old(&dir);
    let fs = Arc::new(ShimFs::new());
    let store = Store::open(&root, fs.clone()).unwrap();
    // Both begin while generation 1 is current.
    let mut first = store.begin().unwrap();
    let mut second = store.begin().unwrap();
    first.put("a", A2).unwrap();
    second.put("c", C).unwrap();
    assert_eq!(first.publish().unwrap().generation, 2);
    assert_eq!(second.publish().unwrap().generation, 3);
    assert_eq!(
        seen(&root, fs),
        (3, new()),
        "the second kept what the first published"
    );
}

#[test]
fn publications_from_several_threads_each_get_a_generation_and_none_is_lost() {
    const THREADS: usize = 8;
    const EACH: usize = 6;
    let dir = Scratch::new("store");
    let root = root(&dir);
    let fs = Arc::new(ShimFs::new());
    let store = Store::open(&root, fs.clone()).unwrap();
    std::thread::scope(|scope| {
        for thread in 0..THREADS {
            let store = &store;
            scope.spawn(move || {
                for n in 0..EACH {
                    let name = format!("{thread}-{n}");
                    let mut publication = store.begin().unwrap();
                    publication.put(&name, name.as_bytes()).unwrap();
                    publication.publish().unwrap();
                }
            });
        }
    });
    let (number, held) = seen(&root, fs);
    assert_eq!(number, (THREADS * EACH) as u64);
    assert_eq!(held.len(), THREADS * EACH);
    for (name, bytes) in held {
        assert_eq!(name.as_bytes(), bytes);
    }
}

// A publication reported successful is there after any later crash.

#[test]
fn content_the_store_already_holds_is_durable_when_a_publication_that_names_it_succeeds() {
    let at = first_sync_of_objects();
    let dir = Scratch::new("store");
    let root = root(&dir);
    let fs = Arc::new(ShimFs::new().fail_before(at));
    let store = Store::open(&root, fs.clone()).unwrap();
    // The first publication renames its payload into place and is refused the sync of the
    // directory: the content is there, and its name is not durable.
    let mut first = store.begin().unwrap();
    first.put("a", A1).unwrap();
    assert!(matches!(first.publish(), Err(Error::Io { .. })));
    // The second names the same bytes, stages nothing, and succeeds.
    let mut second = store.begin().unwrap();
    second.put("b", A1).unwrap();
    assert_eq!(second.publish().unwrap().generation, 1);
    fs.crash(Lost::Unsynced);
    assert_eq!(after_restart(&root), (1, contents(&[("b", A1)])));
}

/// SplitMix64: a small generator with a fixed sequence for each seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// As `seen`, for a store that may be damaged: what is wrong, in place of a panic.
fn try_seen(root: &Path, fs: Arc<dyn Fs>) -> Result<(u64, Contents), String> {
    let reader = Reader::open(root, fs).map_err(|e| e.to_string())?;
    let generation = reader.snapshot().map_err(|e| e.to_string())?;
    let mut contents = Contents::new();
    for (name, entry) in generation.entries() {
        let bytes = reader.read(entry).map_err(|e| format!("{name}: {e}"))?;
        contents.insert(name.clone(), bytes);
    }
    Ok((generation.number(), contents))
}

fn attempt(store: &Store, puts: &[(String, Vec<u8>)]) -> Result<Published, Error> {
    let mut publication = store.begin()?;
    for (name, bytes) in puts {
        publication.put(name, bytes)?;
    }
    publication.publish()
}

#[test]
fn random_runs_of_publications_refusals_and_a_crash_keep_every_reported_success() {
    const SEEDS: u64 = 300;
    let mut failures = Vec::new();
    for seed in 0..SEEDS {
        let mut rng = Rng(seed);
        let dir = Scratch::new("store-random");
        let root = root(&dir);
        let lost = match rng.below(8) {
            0..4 => Lost::Unsynced,
            4 => Lost::ContentOnly,
            _ => Lost::Subset(rng.next()),
        };
        // Some runs lose power at an event; most, after the last publication.
        let crash_at = match rng.below(4) {
            0 => rng.below(90) as usize,
            _ => usize::MAX,
        };
        let fs = Arc::new(ShimFs::new().crash_before(crash_at, lost));
        // What a reader sees now, and every state the disk may hold after a crash: the
        // last reported success, and whatever became visible, or may have, since.
        let mut visible = Contents::new();
        let mut allowed = vec![Contents::new()];
        if let Ok(store) = Store::open(&root, fs.clone()) {
            // A caller whose publication was refused usually tries the same one again.
            let mut again: Option<Vec<(String, Vec<u8>)>> = None;
            for _ in 0..3 + rng.below(4) {
                // A refusal for this attempt: of the event at some distance, or of a kind
                // of call. A second try is more often left alone.
                // The syncs of directories are refused most: each is the step that
                // makes something durable.
                if rng.below(4) == 0 {
                    fs.refuse_before(fs.events().len() + rng.below(14) as usize);
                } else if again.is_none() && rng.below(3) != 0 {
                    let skip = rng.below(2) as usize;
                    match rng.below(8) {
                        0 => fs.refuse_matching(skip, |e| matches!(e, Event::Rename(..))),
                        1 => fs.refuse_matching(skip, |e| matches!(e, Event::SyncFile(_))),
                        _ => fs.refuse_matching(skip, |e| matches!(e, Event::SyncDir(_))),
                    }
                }
                let puts = match again.take() {
                    Some(puts) if rng.below(4) != 0 => puts,
                    _ => (0..1 + rng.below(2))
                        .map(|_| {
                            let name = ["a", "b", "c", "d"][rng.below(4) as usize];
                            let bytes = format!("content {}", rng.below(6)).into_bytes();
                            (name.to_string(), bytes)
                        })
                        .collect(),
                };
                let mut next = visible.clone();
                next.extend(puts.iter().cloned());
                match attempt(&store, &puts) {
                    Ok(_) => {
                        visible = next;
                        allowed = vec![visible.clone()];
                        // Sometimes the power goes right after a success.
                        if rng.below(2) == 0 {
                            break;
                        }
                    }
                    Err(Error::Unacknowledged { .. }) => {
                        visible = next;
                        allowed.push(visible.clone());
                    }
                    // Cut short by the crash: it may have reached the pointer.
                    Err(_) if fs.crashed() => {
                        allowed.push(next);
                        break;
                    }
                    // Refused before the pointer: it never became visible.
                    Err(_) => again = Some(puts),
                }
            }
        }
        fs.crash(lost);

        let restarted = Arc::new(ShimFs::new());
        let held = Store::open(&root, restarted.clone())
            .map_err(|e| e.to_string())
            .and_then(|_| try_seen(&root, restarted));
        match held {
            Err(error) => failures.push(format!("seed {seed}, {lost:?}: {error}")),
            Ok((_, held)) if !allowed.contains(&held) => failures.push(format!(
                "seed {seed}, {lost:?}: holds {:?}, a state that was never visible",
                held.keys().collect::<Vec<_>>()
            )),
            Ok(_) => {}
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {SEEDS} runs: {:#?}",
        failures.len(),
        &failures[..failures.len().min(3)]
    );
}

/// The number of the event at which a store's first publication, of one payload, syncs
/// `objects/`.
fn first_sync_of_objects() -> usize {
    let dir = Scratch::new("store");
    let root = root(&dir);
    let fs = Arc::new(ShimFs::new());
    let store = Store::open(&root, fs.clone()).unwrap();
    let mut publication = store.begin().unwrap();
    publication.put("a", A1).unwrap();
    publication.publish().unwrap();
    position(
        &fs.events(),
        "the sync of objects",
        |event| matches!(event, Event::SyncDir(path) if *path == root.join("objects")),
    )
}

#[test]
fn content_another_publication_is_still_bringing_in_is_durable_when_this_one_succeeds() {
    let at = first_sync_of_objects();
    let dir = Scratch::new("store");
    let root = root(&dir);
    let fs = Arc::new(ShimFs::new());
    let store = Arc::new(Store::open(&root, fs.clone()).unwrap());
    // The first publication has renamed its payload into place and is about to sync the
    // directory. Before it does, as from another thread, a second publication names the
    // same bytes, succeeds, and power is lost.
    let second = {
        let (store, fs) = (store.clone(), fs.clone());
        move || {
            let mut second = store.begin().unwrap();
            second.put("b", A1).unwrap();
            assert_eq!(second.publish().unwrap().generation, 1);
            fs.crash(Lost::Unsynced);
        }
    };
    fs.pause_before(at, second);
    let mut first = store.begin().unwrap();
    first.put("a", A1).unwrap();
    assert!(first.publish().is_err(), "it was cut short by the crash");
    assert_eq!(after_restart(&root), (1, contents(&[("b", A1)])));
}

// Refusals that are not crashes.

#[test]
fn a_put_whose_write_failed_part_way_can_be_made_again() {
    let dir = Scratch::new("store");
    let root = with_old(&dir);
    let fs = Arc::new(ShimFs::new());
    let store = Store::open(&root, fs.clone()).unwrap();
    let mut publication = store.begin().unwrap();
    // The write happens and its sync is refused: the staged file is there, unsynced.
    fs.refuse_before(fs.events().len() + 1);
    assert!(matches!(publication.put("c", C), Err(Error::Io { .. })));
    assert!(matches!(fs.events().last(), Some(Event::Write(_))));
    publication.put("c", C).unwrap();
    publication.put("a", A2).unwrap();
    assert_eq!(publication.publish().unwrap().generation, 2);
    fs.crash(Lost::Unsynced);
    assert_eq!(after_restart(&root), (2, new()));
}

#[test]
fn a_staging_on_another_file_system_is_refused_with_nothing_visible() {
    let dir = Scratch::new("store");
    let root = with_old(&dir);
    let fs = Arc::new(ShimFs::new().cross_device(&root.join("staging")));
    match publish_new(&root, fs.clone()) {
        Err(Error::OtherFileSystem { staging, .. }) => assert_eq!(staging, root.join("staging")),
        other => panic!("{other:?}"),
    }
    assert_eq!(seen(&root, fs), (1, old()));
    assert_eq!(std::fs::read_dir(root.join("objects")).unwrap().count(), 2);
    assert_eq!(
        std::fs::read_dir(root.join("generations")).unwrap().count(),
        1
    );

    // A new store is refused as it is created, before it has a pointer.
    let dir = Scratch::new("store");
    let root = self::root(&dir);
    let fs = Arc::new(ShimFs::new().cross_device(&root.join("staging")));
    let result = Store::open(&root, fs.clone()).map(|_| ());
    assert!(
        matches!(result, Err(Error::OtherFileSystem { .. })),
        "{result:?}"
    );
    assert!(!root.join("CURRENT").exists());
    assert!(matches!(
        Reader::open(&root, fs),
        Err(Error::NotAStore { .. })
    ));
    // What it left is an unfinished creation, which a writer on one file system completes.
    assert_eq!(after_restart(&root), (0, Contents::new()));
}

#[test]
fn the_last_generation_number_is_refused_and_nothing_overflows() {
    let dir = Scratch::new("store");
    let root = with_old(&dir);
    let last = u64::MAX;
    let text = std::fs::read_to_string(root.join("generations/1.json")).unwrap();
    assert!(text.contains("\"number\": 1,"), "{text}");
    std::fs::write(
        root.join(format!("generations/{last}.json")),
        text.replace("\"number\": 1,", &format!("\"number\": {last},")),
    )
    .unwrap();
    std::fs::write(root.join("CURRENT"), format!("{last}\n")).unwrap();

    let fs = Arc::new(ShimFs::new());
    let store = Store::open(&root, fs.clone()).unwrap();
    assert_eq!(store.snapshot().unwrap().number(), last);
    let mut publication = store.begin().unwrap();
    publication.put("c", C).unwrap();
    let result = publication.publish();
    assert!(matches!(result, Err(Error::Corrupt { .. })), "{result:?}");
    assert_eq!(seen(&root, fs), (last, old()));
}

// A store as it is created.

#[test]
fn a_new_store_has_its_directories_and_its_pointer_durable_before_it_is_used() {
    let dir = Scratch::new("store");
    let root = root(&dir);
    let fs = Arc::new(ShimFs::new());
    Store::open(&root, fs.clone()).unwrap();

    // The directories are synced into the root before the pointer is written.
    let events = fs.events();
    let directories = last_position(&events, "a directory", |event| {
        matches!(event, Event::CreateDir(_))
    });
    let pointer = position(&events, "the pointer's write", |event| {
        matches!(event, Event::Write(_))
    });
    assert_eq!(events[directories + 1], Event::SyncDir(root.clone()));
    assert!(directories + 1 < pointer);
    assert_eq!(events.last(), Some(&Event::SyncDir(root.clone())));

    fs.crash(Lost::Unsynced);
    assert_eq!(std::fs::read(root.join("CURRENT")).unwrap(), b"0\n");
    for name in ["objects", "generations", "staging"] {
        assert!(root.join(name).is_dir(), "{name}");
    }
    let reader = Arc::new(ShimFs::new());
    assert_eq!(seen(&root, reader.clone()), (0, Contents::new()));
    assert_eq!(reader.events(), vec![]);
}

// What is a store, and what is not: a directory with a pointer.

/// Every path under a directory, with the content of each file.
fn tree(dir: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            found.push((path.clone(), None));
            stack.extend(
                std::fs::read_dir(&path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path()),
            );
        } else {
            found.push((path.clone(), Some(std::fs::read(&path).unwrap())));
        }
    }
    found.sort();
    found
}

#[test]
fn a_directory_that_is_not_a_store_is_refused_and_left_exactly_as_it_is() {
    type Build = fn(&Path);
    let cases: [(&str, Build); 7] = [
        ("unrelated files", |root| {
            std::fs::write(root.join("notes.txt"), b"mine").unwrap();
        }),
        ("an unrelated staging folder with files in it", |root| {
            std::fs::create_dir(root.join("staging")).unwrap();
            std::fs::write(root.join("staging/draft.txt"), b"mine").unwrap();
        }),
        ("an objects folder with files and no pointer", |root| {
            std::fs::create_dir(root.join("objects")).unwrap();
            std::fs::write(root.join("objects/thing"), b"mine").unwrap();
        }),
        ("generation files and no pointer", |root| {
            std::fs::create_dir(root.join("generations")).unwrap();
            std::fs::write(root.join("generations/1.json"), b"{}").unwrap();
        }),
        ("the three folders and a folder in staging", |root| {
            for name in ["objects", "generations", "staging", "staging/1-0"] {
                std::fs::create_dir(root.join(name)).unwrap();
            }
        }),
        ("the three folders and one more thing", |root| {
            for name in ["objects", "generations", "staging", "other"] {
                std::fs::create_dir(root.join(name)).unwrap();
            }
        }),
        ("a file where a folder of the store would be", |root| {
            std::fs::write(root.join("objects"), b"mine").unwrap();
        }),
    ];
    for (what, build) in cases {
        let dir = Scratch::new("store");
        let root = root(&dir);
        std::fs::create_dir(&root).unwrap();
        build(&root);
        let before = tree(&root);

        let fs = Arc::new(ShimFs::new());
        let result = Store::open(&root, fs.clone()).map(|_| ());
        match result {
            Err(Error::NotAStore { root: named, .. }) => assert_eq!(named, root, "{what}"),
            other => panic!("{what}: {other:?}"),
        }
        let result = Reader::open(&root, fs.clone()).map(|_| ());
        assert!(
            matches!(result, Err(Error::NotAStore { .. })),
            "{what}: {result:?}"
        );
        assert_eq!(fs.events(), vec![], "{what}");
        assert_eq!(tree(&root), before, "{what}");
    }
}

#[test]
fn a_store_that_lost_its_pointer_is_refused_and_nothing_in_it_is_removed() {
    let dir = Scratch::new("store");
    let root = with_old(&dir);
    std::fs::remove_file(root.join("CURRENT")).unwrap();
    std::fs::create_dir(root.join("staging/leftover")).unwrap();
    let before = tree(&root);
    let fs = Arc::new(ShimFs::new());
    let result = Store::open(&root, fs.clone()).map(|_| ());
    assert!(matches!(result, Err(Error::NotAStore { .. })), "{result:?}");
    let result = Reader::open(&root, fs.clone()).map(|_| ());
    assert!(matches!(result, Err(Error::NotAStore { .. })), "{result:?}");
    // The writer confirmed what it saw under the lock, and did nothing else.
    assert_eq!(fs.events(), vec![Event::Lock(root.join("LOCK"))]);
    assert_eq!(tree(&root), before);
}

#[test]
fn an_empty_directory_and_an_unfinished_creation_become_a_store() {
    // What a creation that did not finish can leave: some of the three folders, empty,
    // and in staging the first pointer, whole or cut short.
    type Case = (&'static str, &'static [&'static str], Option<&'static [u8]>);
    let cases: [Case; 5] = [
        ("an empty directory", &[], None),
        ("one folder", &["objects"], None),
        ("two folders", &["generations", "staging"], None),
        (
            "the three folders",
            &["objects", "generations", "staging"],
            None,
        ),
        (
            "the three folders and a staged pointer cut short",
            &["objects", "generations", "staging"],
            Some(b"0"),
        ),
    ];
    for (what, folders, staged) in cases {
        let dir = Scratch::new("store");
        let root = root(&dir);
        std::fs::create_dir(&root).unwrap();
        for folder in folders {
            std::fs::create_dir(root.join(folder)).unwrap();
        }
        if let Some(bytes) = staged {
            std::fs::write(root.join("staging/CURRENT"), bytes).unwrap();
        }
        // To a reader it is not a store yet, and the reader leaves it alone.
        let before = tree(&root);
        let fs = Arc::new(ShimFs::new());
        let result = Reader::open(&root, fs.clone()).map(|_| ());
        assert!(
            matches!(result, Err(Error::NotAStore { .. })),
            "{what}: {result:?}"
        );
        assert_eq!(tree(&root), before, "{what}");

        assert_eq!(after_restart(&root), (0, Contents::new()), "{what}");
        assert_eq!(
            std::fs::read(root.join("CURRENT")).unwrap(),
            b"0\n",
            "{what}"
        );
    }
}

#[test]
fn a_crash_at_any_event_of_a_stores_creation_leaves_no_store_or_an_empty_one() {
    let count = {
        let dir = Scratch::new("store");
        let fs = Arc::new(ShimFs::new());
        Store::open(&root(&dir), fs.clone()).unwrap();
        fs.events().len()
    };
    for crash_at in 0..=count {
        for_every_loss(|lost| {
            let case = format!("{lost:?}, crash before event {crash_at}");
            let dir = Scratch::new("store");
            let root = root(&dir);
            let fs = Arc::new(ShimFs::new().crash_before(crash_at, lost));
            let result = Store::open(&root, fs.clone()).map(|_| ());
            fs.crash(lost);

            // A reader is told there is no store, or sees the empty one; never damage.
            let reader = Arc::new(ShimFs::new());
            let read = Reader::open(&root, reader.clone()).and_then(|r| r.snapshot());
            match read {
                Err(Error::Missing { .. } | Error::NotAStore { .. }) => {
                    assert!(result.is_err(), "{case}: the open had succeeded")
                }
                Ok(generation) => assert_eq!(generation.number(), 0, "{case}"),
                Err(other) => panic!("{case}: {other:?}"),
            }
            assert_eq!(reader.events(), vec![], "{case}");
            // A writer completes it.
            assert_eq!(after_restart(&root), (0, Contents::new()), "{case}");
            fs.crash_choices()
        });
    }
}
