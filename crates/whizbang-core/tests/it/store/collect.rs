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

    // Not yet of age: nothing goes, and the publication is made all the same.
    let collected = store
        .collect_unused_imports(24 * HOUR_MS, now_ms())
        .unwrap();
    assert_eq!(collected.names, 0);
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
