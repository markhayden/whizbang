//! What a writer leaves of older generations (ADR 0036): as it publishes, it removes the
//! file of every older generation that nobody holds, by prune's own rule, so a store keeps
//! the current generation's file and those readers and holds still share, and no trail.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Generation, Kind, Pruned, Reader, Store};
use whizbang_testkit::fs_shim::{Event, ShimFs};
use whizbang_testkit::scratch::Scratch;

use super::runs::{MASK, with_runs};
use super::{Hooked, When, for_every_loss, losses, root};

/// The numbers of the generation files on disk.
fn generation_files(root: &Path) -> BTreeSet<u64> {
    std::fs::read_dir(root.join("generations"))
        .unwrap()
        .map(|entry| {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            name.strip_suffix(".json").unwrap().parse().unwrap()
        })
        .collect()
}

/// Reads every reference of a generation through `read`, and returns how many.
fn read_all(read: impl Fn(&whizbang_core::store::Entry) -> Vec<u8>, at: &Generation) -> usize {
    at.references().map(read).count()
}

#[test]
fn a_writer_that_publishes_200_times_leaves_the_current_file_and_each_held_one_until_let_go() {
    let dir = Scratch::new("store-growth");
    let root = root(&dir);
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    let (mut reader, mut hold) = (None, None);
    let mut held_by_reader: Option<Generation> = None;
    for number in 1..=200u64 {
        let mut publication = store.begin().unwrap();
        publication
            .put_in(
                &format!("run-{number}"),
                "out",
                Kind::Output,
                &number.to_be_bytes(),
            )
            .unwrap();
        assert_eq!(publication.publish().unwrap().generation, number);
        match number {
            // Another process's reader holds 50; the writer itself holds 100.
            50 => {
                let opened = Reader::open(&root, Arc::new(RealFs)).unwrap();
                held_by_reader = Some(opened.snapshot().unwrap());
                reader = Some(opened);
            }
            100 => hold = Some(store.hold().unwrap()),
            // Each still reads all of its generation, a hundred publications on.
            150 => {
                let opened = reader.take().unwrap();
                let held = held_by_reader.take().unwrap();
                assert_eq!(read_all(|entry| opened.read(entry).unwrap(), &held), 50);
            }
            180 => {
                let holding = hold.take().unwrap();
                let held = holding.generation();
                assert_eq!(read_all(|entry| store.read(entry).unwrap(), held), 100);
            }
            _ => {}
        }
        // At most the current file and the held ones, after every publication. One let
        // go goes with the next publication.
        let mut expected = BTreeSet::from([number]);
        if (50..=150).contains(&number) {
            expected.insert(50);
        }
        if (100..=180).contains(&number) {
            expected.insert(100);
        }
        assert_eq!(generation_files(&root), expected, "after {number}");
    }
    // Prune finds no generation left to remove, and is idempotent; content is its alone.
    let pruned = store.prune().unwrap();
    assert_eq!(pruned.generations_removed, Vec::<u64>::new());
    assert_eq!(pruned.generations_in_use, Vec::<u64>::new());
    assert_eq!(
        pruned.objects_removed, 0,
        "every run's content is still named"
    );
    assert_eq!(store.prune().unwrap(), Pruned::default());
    let current = store.snapshot().unwrap();
    assert_eq!(read_all(|entry| store.read(entry).unwrap(), &current), 200);
}

/// Generation 3 of `with_runs` after two deletions, with the files of 1, 2 and 3 on disk:
/// readers held them while the writer published. The reader of 2 is returned and still
/// holds it; the others are gone.
fn three_files_and_a_reader_of_2(root: &Path) -> (Reader, Generation) {
    with_runs(root);
    let first = Reader::open(root, Arc::new(RealFs)).unwrap();
    first.snapshot().unwrap();
    let store = Store::open(root, Arc::new(ShimFs::new())).unwrap();
    store.delete(&["one"]).unwrap();
    let second = Reader::open(root, Arc::new(RealFs)).unwrap();
    let held = second.snapshot().unwrap();
    store.delete(&["two"]).unwrap();
    let third = Reader::open(root, Arc::new(RealFs)).unwrap();
    third.snapshot().unwrap();
    drop((store, first, third));
    assert_eq!(generation_files(root), BTreeSet::from([1, 2, 3]));
    (second, held)
}

/// The process under test: opens the store and publishes generation 4.
fn publish_4(root: &Path, fs: Arc<ShimFs>) -> Result<u64, whizbang_core::store::Error> {
    let store = Store::open(root, fs)?;
    let mut publication = store.begin()?;
    publication.put_in("next", "out", Kind::Output, MASK)?;
    Ok(publication.publish()?.generation)
}

#[test]
fn a_crash_at_any_event_of_the_removal_after_a_publication_leaves_the_store_whole() {
    // The events of the removal: those after the publication's final sync, the sync of
    // the root. The sweep starts at that sync and runs past the last event.
    let (from, count) = {
        let dir = Scratch::new("store-growth");
        let root = root(&dir);
        let (reader, _) = three_files_and_a_reader_of_2(&root);
        let fs = Arc::new(ShimFs::new());
        assert_eq!(publish_4(&root, fs.clone()).unwrap(), 4);
        assert_eq!(generation_files(&root), BTreeSet::from([2, 4]));
        drop(reader);
        let events = fs.events();
        let synced = Event::SyncDir(root.clone());
        let last_sync = events.iter().rposition(|event| *event == synced).unwrap();
        let removed: Vec<&Event> = events[last_sync..]
            .iter()
            .filter(|event| matches!(event, Event::Remove(path) if path.starts_with(root.join("generations"))))
            .collect();
        assert_eq!(removed.len(), 2, "1 and 3, not the held 2: {events:#?}");
        (last_sync, events.len())
    };
    for crash_at in from..=count {
        for_every_loss(|lost| {
            let case = format!("{lost:?}, crash before event {crash_at}");
            let dir = Scratch::new("store-growth");
            let root = root(&dir);
            let (reader, held) = three_files_and_a_reader_of_2(&root);
            let fs = Arc::new(ShimFs::new().crash_before(crash_at, lost));
            let result = publish_4(&root, fs.clone());
            fs.crash(lost);

            // A whole current generation, and the pointer names a file.
            let fresh = Reader::open(&root, Arc::new(RealFs)).unwrap();
            let current = fresh.snapshot().unwrap_or_else(|e| panic!("{case}: {e}"));
            assert!([3, 4].contains(&current.number()), "{case}");
            if result.is_ok() {
                assert_eq!(current.number(), 4, "{case}: success was reported");
            }
            let read = |entry: &_| fresh.read(entry).unwrap_or_else(|e| panic!("{case}: {e}"));
            read_all(read, &current);
            drop(fresh);
            // The held generation is there, whole, whatever the crash kept.
            let files = generation_files(&root);
            assert!(
                files.contains(&2) && files.contains(&current.number()),
                "{case}"
            );
            assert!(files.is_subset(&BTreeSet::from([1, 2, 3, 4])), "{case}");
            assert_eq!(read_all(|entry| reader.read(entry).unwrap(), &held), 5);

            // The next writer publishes, and removes what is left but the held 2; once the
            // reader goes, the next publication removes 2; prune then finds no
            // generation to remove, twice.
            let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
            let next = store
                .delete(&["next"])
                .or_else(|_| store.delete(&["failed"]));
            let next = next.unwrap_or_else(|e| panic!("{case}: {e}")).generation;
            assert_eq!(generation_files(&root), BTreeSet::from([2, next]), "{case}");
            drop(reader);
            let last = store.delete(&["failed"]).map(|done| done.generation);
            let last = last.unwrap_or_else(|_| {
                let mut publication = store.begin().unwrap();
                publication.put("notes/2", b"after").unwrap();
                publication.publish().unwrap().generation
            });
            assert_eq!(generation_files(&root), BTreeSet::from([last]), "{case}");
            assert_eq!(
                store.prune().unwrap().generations_removed,
                Vec::<u64>::new()
            );
            assert_eq!(store.prune().unwrap(), Pruned::default(), "{case}");
            losses(&fs)
        });
    }
}

#[test]
fn a_writers_snapshot_whose_file_its_own_publication_removed_reads_the_current_one() {
    // The writer's snapshot holds nothing. It has looked at the pointer, which says 1;
    // before it reads generation 1, another thread of the process publishes 2, which
    // removes 1.json. The snapshot reads the pointer again: no damage is reported.
    let dir = Scratch::new("store-growth");
    let root = root(&dir);
    let slot: Arc<Mutex<Option<Arc<Store>>>> = Arc::new(Mutex::new(None));
    let hook = {
        let (slot, root) = (slot.clone(), root.clone());
        move || {
            let store = slot.lock().unwrap().clone().unwrap();
            let mut publication = store.begin().unwrap();
            publication.put("c", b"published meanwhile").unwrap();
            assert_eq!(publication.publish().unwrap().generation, 2);
            assert!(!root.join("generations/1.json").exists());
        }
    };
    let fs = Arc::new(Hooked::new(
        When::Reading(root.join("generations/1.json")),
        hook,
    ));
    // A new store reads no generation file until it has published one.
    let store = Arc::new(Store::open(&root, fs).unwrap());
    let mut publication = store.begin().unwrap();
    publication.put("a", b"first").unwrap();
    assert_eq!(publication.publish().unwrap().generation, 1);
    *slot.lock().unwrap() = Some(store.clone());
    let generation = store.snapshot().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(generation.number(), 2);
    assert_eq!(read_all(|entry| store.read(entry).unwrap(), &generation), 2);
    slot.lock().unwrap().take();
}
