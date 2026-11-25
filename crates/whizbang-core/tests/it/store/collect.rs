//! The store's collection of what no run uses (plan m6, task S1, item 7): an import that
//! no run took, older than an age, is let go with its mark and the names that came in
//! with it, in one publication; one a run took stays.

use std::sync::Arc;

use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Entry, Store};
use whizbang_testkit::scratch::Scratch;

const HOUR_MS: u64 = 3_600_000;

/// The time now, as the store stamps a mark.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Imports `name` with `content`, and the name `with` beside it, as the playground and
/// the engine API import a picture.
fn import(store: &Store, name: &str, content: &[u8], with: &str) -> Entry {
    let mut publication = store.begin().unwrap();
    publication.put(name, content).unwrap();
    publication.put(with, b"what came in with it").unwrap();
    publication
        .mark_imported_with(name, &[with.to_owned()])
        .unwrap();
    publication.publish().unwrap();
    store.snapshot().unwrap().get(name).unwrap().clone()
}

#[test]
fn an_import_a_run_took_stays_and_one_none_took_past_its_age_goes() {
    let dir = Scratch::new("store-collect");
    let store = Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap();
    let taken = import(
        &store,
        "artifacts/image/taken",
        b"taken",
        "play/pictures/taken",
    );
    import(
        &store,
        "artifacts/image/left",
        b"left",
        "play/pictures/left",
    );
    import(
        &store,
        "artifacts/location/drawn",
        b"drawn",
        "play/locations/drawn",
    );
    // A run took the first: its admission holds it.
    let mut publication = store.begin().unwrap();
    publication.create_run("run-1").unwrap();
    publication
        .hold_in("run-1", "artifacts/image/taken", taken)
        .unwrap();
    publication.publish().unwrap();
    // A name a run committed and no import marked is not an import, and is not looked at.
    let mut publication = store.begin().unwrap();
    publication.put("notes/a", b"a note").unwrap();
    publication.publish().unwrap();

    // Not yet of age: nothing goes, and no generation is published for nothing (plan m6,
    // task D1: a server collects every hour).
    let before = store.snapshot().unwrap().number();
    let collected = store
        .collect_unused_imports(24 * HOUR_MS, now_ms())
        .unwrap();
    assert_eq!(collected.names, 0);
    assert_eq!(collected.generation, before);
    assert_eq!(store.snapshot().unwrap().number(), before);
    let names = |store: &Store| -> Vec<String> {
        store
            .snapshot()
            .unwrap()
            .entries()
            .keys()
            .cloned()
            .collect()
    };
    assert_eq!(names(&store).len(), 10, "{:?}", names(&store));

    // A day on: the two no run took go, each with its mark and what came in with it.
    let collected = store
        .collect_unused_imports(24 * HOUR_MS, now_ms() + 25 * HOUR_MS)
        .unwrap();
    assert_eq!(collected.names, 6);
    assert_eq!(
        names(&store),
        [
            "artifacts/image/taken",
            "imported/artifacts/image/taken",
            "notes/a",
            "play/pictures/taken",
        ]
    );
    // What it let go goes from the disk at the next prune; what a run holds is read.
    let generation = store.snapshot().unwrap();
    store.prune().unwrap();
    store
        .read(generation.get("artifacts/image/taken").unwrap())
        .unwrap();
    // A mark with no time, as one written before marks had one, is never let go.
    let mut publication = store.begin().unwrap();
    publication.put("artifacts/image/old", b"old").unwrap();
    publication
        .put("imported/artifacts/image/old", b"old")
        .unwrap();
    publication.publish().unwrap();
    store
        .collect_unused_imports(0, now_ms() + 1_000 * HOUR_MS)
        .unwrap();
    assert!(
        store
            .snapshot()
            .unwrap()
            .get("artifacts/image/old")
            .is_some()
    );
}

#[test]
fn an_import_taken_by_a_run_started_during_the_collection_stays() {
    let dir = Scratch::new("store-collect");
    let store = Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap();
    let picture = import(&store, "artifacts/image/p", b"picture", "play/pictures/p");
    // A run is admitted with the picture: its admission read it, and holds it when it
    // is published. The collection is published first, while the admission is on its
    // way: it leaves the picture whole, with its mark and the name beside it.
    let mut admission = store.begin().unwrap();
    admission.create_run("run-1").unwrap();
    admission
        .hold_in("run-1", "artifacts/image/p", picture.clone())
        .unwrap();
    let collected = store.collect_unused_imports(0, now_ms() + HOUR_MS).unwrap();
    assert_eq!(collected.names, 0);
    admission.publish().unwrap();
    let generation = store.snapshot().unwrap();
    let held = generation.get("artifacts/image/p").unwrap();
    assert_eq!(held, &picture);
    assert_eq!(store.read(held).unwrap(), b"picture");
    for kept in ["imported/artifacts/image/p", "play/pictures/p"] {
        assert!(generation.get(kept).is_some(), "{kept}");
    }
    assert!(
        generation
            .run("run-1")
            .unwrap()
            .get("holds/artifacts/image/p")
            .is_some()
    );
    // An admission that read the picture before the collection and holds it only after
    // the collection was published gives the name back, with its hold, and reads it.
    let late = import(
        &store,
        "artifacts/image/late",
        b"late",
        "play/pictures/late",
    );
    let collected = store.collect_unused_imports(0, now_ms() + HOUR_MS).unwrap();
    assert_eq!(collected.names, 3);
    let mut admission = store.begin().unwrap();
    admission.create_run("run-3").unwrap();
    admission
        .hold_in("run-3", "artifacts/image/late", late)
        .unwrap();
    admission.publish().unwrap();
    let generation = store.snapshot().unwrap();
    let held = generation.get("artifacts/image/late").unwrap();
    assert_eq!(store.read(held).unwrap(), b"late");
    // And a later collection leaves it: a run holds it.
    let collected = store
        .collect_unused_imports(0, now_ms() + 2 * HOUR_MS)
        .unwrap();
    assert_eq!(collected.names, 0);
    // And the other order: a run admitted first holds it, and the collection leaves it.
    let other = import(&store, "artifacts/image/q", b"other", "play/pictures/q");
    let mut publication = store.begin().unwrap();
    publication.create_run("run-2").unwrap();
    publication
        .hold_in("run-2", "artifacts/image/q", other)
        .unwrap();
    publication.publish().unwrap();
    let collected = store
        .collect_unused_imports(0, now_ms() + 3 * HOUR_MS)
        .unwrap();
    assert_eq!(collected.names, 0);
    assert!(store.snapshot().unwrap().get("artifacts/image/q").is_some());
}

#[test]
fn an_import_made_again_is_younger_and_keeps_every_name_that_came_in_with_it() {
    let dir = Scratch::new("store-collect");
    let store = Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap();
    import(
        &store,
        "artifacts/image/p",
        b"picture",
        "imports/picture/first",
    );
    let first = store
        .snapshot()
        .unwrap()
        .imported("artifacts/image/p")
        .unwrap()
        .clone();
    let mut publication = store.begin().unwrap();
    publication
        .put("imports/picture/second", b"the same picture again")
        .unwrap();
    publication
        .mark_imported_with("artifacts/image/p", &["imports/picture/second".to_owned()])
        .unwrap();
    publication.publish().unwrap();
    let again = store
        .snapshot()
        .unwrap()
        .imported("artifacts/image/p")
        .unwrap()
        .clone();
    assert!(again.at_ms >= first.at_ms);
    assert_eq!(
        again.with,
        ["imports/picture/first", "imports/picture/second"]
    );
    // Its age is from the second import: a collection of what is older than an hour
    // at the first import's time and a half hour on leaves it.
    let collected = store
        .collect_unused_imports(HOUR_MS, again.at_ms + HOUR_MS / 2)
        .unwrap();
    assert_eq!(collected.names, 0);
    // And past it, every name that came in with either import goes with it.
    let collected = store
        .collect_unused_imports(HOUR_MS, again.at_ms + 2 * HOUR_MS)
        .unwrap();
    assert_eq!(collected.names, 4);
    let generation = store.snapshot().unwrap();
    for gone in [
        "imports/picture/first",
        "imports/picture/second",
        "artifacts/image/p",
    ] {
        assert!(generation.get(gone).is_none(), "{gone}");
    }
}

// --- What the store keeps for replay (plan m7, task B1, item 7; ADR 0048) ----------------------

use whizbang_core::store::Digest;
use whizbang_core::store::{BUNDLES_PREFIX, Error, Kind, Reader};

/// Whether the store's folder holds the content of these bytes.
fn on_disk(store: &Store, bytes: &[u8]) -> bool {
    let hex = Digest::of_bytes(bytes).hex();
    store.root().join("objects").join(hex).exists()
}

/// The entry of a bundle named `name` holding these files, each under its file digest, and
/// a manifest, in one publication, as an import of a bundle names them.
fn bundle(store: &Store, name: &str, files: &[&[u8]]) -> String {
    let entry = format!("{BUNDLES_PREFIX}{name}");
    let mut publication = store.begin().unwrap();
    for file in files {
        let digest = Digest::of_bytes(file).hex();
        publication
            .put(&format!("{entry}/files/{digest}"), file)
            .unwrap();
    }
    publication
        .put(
            &format!("{entry}/manifest"),
            format!("the manifest of {name}").as_bytes(),
        )
        .unwrap();
    publication.publish().unwrap();
    entry
}

/// Lets go of a bundle's entry, in a publication of its own.
fn let_go(store: &Store, entry: &str) {
    let mut publication = store.begin().unwrap();
    publication.let_go_of_entry(entry).unwrap();
    publication.publish().unwrap();
}

/// The names under an entry the current generation holds.
fn named_under(store: &Store, entry: &str) -> usize {
    let generation = store.snapshot().unwrap();
    let names = generation.entries().keys();
    names
        .filter(|name| name.starts_with(&format!("{entry}/")))
        .count()
}

/// The engine API's hourly collection (ADR 0048) is the store's collection and then its
/// prune: an upload no run took goes with its bytes, and one a run took stays whole.
#[test]
fn the_hourly_collection_ends_in_a_prune_and_an_uploads_bytes_go_with_its_name() {
    let dir = Scratch::new("store-collect-prune");
    let store = Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap();
    let taken = import(&store, "artifacts/image/taken", b"taken", "uploads/taken");
    import(&store, "artifacts/image/left", b"left", "uploads/left");
    let mut publication = store.begin().unwrap();
    publication.create_run("run-1").unwrap();
    publication
        .hold_in("run-1", "artifacts/image/taken", taken)
        .unwrap();
    publication.publish().unwrap();
    assert!(on_disk(&store, b"left") && on_disk(&store, b"taken"));
    // Not of age: nothing goes, and nothing is removed from the disk.
    let (collected, _) = store.collect_and_prune(24 * HOUR_MS, now_ms()).unwrap();
    assert_eq!(collected.names, 0);
    assert!(on_disk(&store, b"left"));
    // A day on: the upload no run took goes, its name and its bytes, at once.
    let (collected, pruned) = store
        .collect_and_prune(24 * HOUR_MS, now_ms() + 25 * HOUR_MS)
        .unwrap();
    assert_eq!(collected.names, 3);
    assert!(pruned.objects_removed >= 1, "{pruned:?}");
    assert!(!on_disk(&store, b"left"));
    let generation = store.snapshot().unwrap();
    assert!(generation.get("artifacts/image/left").is_none());
    // The one a run took is named and on disk, and reads.
    store
        .read(generation.get("artifacts/image/taken").unwrap())
        .unwrap();
    // A second collection finds nothing, and removes nothing.
    let (collected, pruned) = store
        .collect_and_prune(24 * HOUR_MS, now_ms() + 26 * HOUR_MS)
        .unwrap();
    assert_eq!((collected.names, pruned.objects_removed), (0, 0));
}

/// A run admitted against a generation holds it until the executor is done with the run
/// (`contract/engine.rs`, `Store::hold`), as a reader of another process holds the
/// generation of its snapshot: the collection's prune leaves every file either reads,
/// and they go at the first collection after both have let go.
#[test]
fn a_run_going_keeps_every_file_a_held_reader_reads() {
    let dir = Scratch::new("store-collect-held");
    let root = dir.path().join("store");
    let store = Store::open(&root, Arc::new(RealFs)).unwrap();
    import(
        &store,
        "artifacts/image/read",
        b"read by a run",
        "uploads/read",
    );
    import(
        &store,
        "artifacts/image/seen",
        b"seen by a reader",
        "uploads/seen",
    );
    // The run's hold, as the executor takes it at admission, and a reader's snapshot.
    let holding = store.hold().unwrap();
    let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
    let seen = reader.snapshot().unwrap();
    let (collected, _) = store.collect_and_prune(0, now_ms() + HOUR_MS).unwrap();
    assert_eq!(collected.names, 6);
    // The current generation names neither, and both read from what holds them.
    let current = store.snapshot().unwrap();
    assert!(current.get("artifacts/image/read").is_none());
    let held = holding.generation();
    assert_eq!(
        store
            .read(held.get("artifacts/image/read").unwrap())
            .unwrap(),
        b"read by a run"
    );
    assert_eq!(
        reader
            .read(seen.get("artifacts/image/seen").unwrap())
            .unwrap(),
        b"seen by a reader"
    );
    // Once both have let go, the next collection's prune removes them.
    drop(holding);
    drop(seen);
    drop(reader);
    store.collect_and_prune(0, now_ms() + 2 * HOUR_MS).unwrap();
    assert!(!on_disk(&store, b"read by a run"));
    assert!(!on_disk(&store, b"seen by a reader"));
}

/// A bundle's entry names what it holds: `store delete` of the runs that held the same
/// artifact, the collection of the import that brought it, and a prune leave it.
#[test]
fn a_bundles_entry_keeps_its_artifacts_from_delete_prune_and_the_collection() {
    let dir = Scratch::new("store-bundle-kept");
    let store = Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap();
    let file: &[u8] = b"the canonical file of an artifact";
    // A run held the artifact through an import, which a bundle holds too.
    let imported = import(&store, "artifacts/image/a", file, "uploads/a");
    let mut publication = store.begin().unwrap();
    publication.create_run("run-1").unwrap();
    publication
        .hold_in("run-1", "artifacts/image/a", imported)
        .unwrap();
    publication
        .put_in("run-1", "outputs/image/0", Kind::Output, file)
        .unwrap();
    publication.publish().unwrap();
    let entry = bundle(&store, "b1", &[file]);
    let named = format!("{entry}/files/{}", Digest::of_bytes(file).hex());
    store.delete(&["run-1"]).unwrap();
    store
        .collect_and_prune(0, now_ms() + 1_000 * HOUR_MS)
        .unwrap();
    let generation = store.snapshot().unwrap();
    assert!(generation.run("run-1").is_none());
    assert!(generation.get("artifacts/image/a").is_none());
    assert_eq!(store.read(generation.get(&named).unwrap()).unwrap(), file);
    assert_eq!(named_under(&store, &entry), 2);
    assert!(on_disk(&store, file));
}

/// A bundle let go of goes whole, in one publication; what it alone named goes at the
/// next prune, and what a run names stays. An entry the store does not hold, and a name
/// that is no entry's, are refused, and nothing is published.
#[test]
fn a_deleted_bundles_artifacts_go_at_the_next_prune_unless_a_run_names_them() {
    let dir = Scratch::new("store-bundle-gone");
    let store = Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap();
    let (alone, shared): (&[u8], &[u8]) = (b"named by the bundle alone", b"named by a run too");
    let entry = bundle(&store, "b1", &[alone, shared]);
    let mut publication = store.begin().unwrap();
    publication.create_run("run-1").unwrap();
    publication
        .put_in("run-1", "outputs/image/0", Kind::Output, shared)
        .unwrap();
    publication.publish().unwrap();
    // A neighbour whose name starts as this entry's does is another entry, and stays.
    let neighbour = bundle(&store, "b10", &[b"the neighbour's"]);
    let before = store.snapshot().unwrap().number();
    let refused = |entry: &str| {
        let mut publication = store.begin().unwrap();
        let asked = publication.let_go_of_entry(entry);
        asked.and_then(|()| publication.publish().map(|_| ()))
    };
    for wrong in ["bundles/", "bundles/b1/files", "runs/run-1", "bundles/none"] {
        assert!(
            matches!(refused(wrong), Err(Error::NoSuchEntry { .. })),
            "{wrong}"
        );
    }
    assert_eq!(store.snapshot().unwrap().number(), before);
    let_go(&store, &entry);
    assert_eq!(named_under(&store, &entry), 0);
    assert_eq!(named_under(&store, &neighbour), 2);
    store.prune().unwrap();
    assert!(!on_disk(&store, alone));
    assert!(on_disk(&store, shared));
    assert!(on_disk(&store, b"the neighbour's"));
    let generation = store.snapshot().unwrap();
    let output = generation
        .run("run-1")
        .unwrap()
        .entries()
        .get("outputs/image/0");
    assert_eq!(store.read(output.unwrap()).unwrap(), shared);
}

/// Two bundles that hold one file each name it: it stays while either is held.
#[test]
fn an_artifact_two_bundles_share_stays_until_both_are_deleted() {
    let dir = Scratch::new("store-bundle-shared");
    let store = Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap();
    let file: &[u8] = b"a file two bundles hold";
    let first = bundle(&store, "first", &[file]);
    let second = bundle(&store, "second", &[file]);
    let_go(&store, &first);
    store.prune().unwrap();
    assert!(on_disk(&store, file));
    let named = format!("{second}/files/{}", Digest::of_bytes(file).hex());
    let generation = store.snapshot().unwrap();
    assert_eq!(store.read(generation.get(&named).unwrap()).unwrap(), file);
    let_go(&store, &second);
    store.prune().unwrap();
    assert!(!on_disk(&store, file));
}

/// An ordinary import names its artifact under its own name, with its mark, whether it
/// came before the bundle or after: the bundle's going leaves it, and its own collection
/// is its own.
#[test]
fn an_ordinary_import_before_or_after_a_bundle_keeps_its_artifact_when_the_bundle_goes() {
    let dir = Scratch::new("store-bundle-import");
    let store = Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap();
    let (before, after): (&[u8], &[u8]) = (b"imported before", b"imported after");
    import(&store, "artifacts/image/before", before, "uploads/before");
    let entry = bundle(&store, "b1", &[before, after]);
    import(&store, "artifacts/image/after", after, "uploads/after");
    let_go(&store, &entry);
    store.prune().unwrap();
    let generation = store.snapshot().unwrap();
    for (name, bytes) in [
        ("artifacts/image/before", before),
        ("artifacts/image/after", after),
    ] {
        assert_eq!(store.read(generation.get(name).unwrap()).unwrap(), bytes);
        assert!(generation.imported(name).is_some(), "{name} keeps its mark");
    }
    // And the imports go by their own collection, with their bytes.
    store
        .collect_and_prune(0, now_ms() + 1_000 * HOUR_MS)
        .unwrap();
    assert!(!on_disk(&store, before) && !on_disk(&store, after));
}
