//! What a deletion removes (ADR 0008, "What deleting a run removes"; plan m3, task Q1,
//! item 3): a run holds each name outside runs that it took or committed, an import marks
//! what it brought in, and a deletion removes the names that no remaining run holds and
//! no import marked, decided from the generation alone. And a name kept in one run, which
//! only that run's own calls read.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use whizbang_core::artifact::{self, ArtifactError, ArtifactType, Mask};
use whizbang_core::contract::{
    Action, ActionDescriptor, Call, CallError, Context, Engine, Executor, Failure, InputFault,
    InputPort, NoBackends, OutputPort, PreparedCall, Produced, Refusal, Registry,
};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Digest, Entry, HOLDS_PREFIX, IMPORTED_PREFIX, Kind, Reader, Store};
use whizbang_testkit::fs_shim::{Lost, ShimFs};
use whizbang_testkit::scratch::Scratch;

use super::{for_every_loss, losses, root};
use crate::artifact::support::{limits, on, rect_mask_by};

const ONLY_ONE: &[u8] = b"an image only run one holds";
const SHARED: &[u8] = b"an image runs one and two hold";
const IMPORTED: &[u8] = b"an image that was imported and that run one took";
const OLDER: &[u8] = b"an image a store held before runs held names";
const SPEND: &[u8] = b"a spend record";

fn entry(bytes: &[u8]) -> Entry {
    Entry {
        digest: Digest::of_bytes(bytes),
        bytes: bytes.len() as u64,
        kind: Kind::Output,
    }
}

/// A store whose run `one` holds three images and whose run `two` holds one of them; one
/// image of the three was imported, and one image is older than any hold.
fn with_holds(root: &Path) {
    let store = Store::open(root, Arc::new(ShimFs::new())).unwrap();
    let mut publication = store.begin().unwrap();
    publication
        .create("spend/1", Kind::Diagnostic, SPEND)
        .unwrap();
    publication.put("artifacts/older", OLDER).unwrap();
    publication.put("artifacts/imported", IMPORTED).unwrap();
    publication.mark_imported("artifacts/imported").unwrap();
    for (name, bytes) in [
        ("artifacts/only-one", ONLY_ONE),
        ("artifacts/shared", SHARED),
    ] {
        publication.keep(name, bytes).unwrap();
        publication.hold_in("one", name, entry(bytes)).unwrap();
    }
    publication
        .hold_in("one", "artifacts/imported", entry(IMPORTED))
        .unwrap();
    publication
        .hold_in("two", "artifacts/shared", entry(SHARED))
        .unwrap();
    publication
        .put_in("one", "record", Kind::Diagnostic, b"one")
        .unwrap();
    publication
        .put_in("two", "record", Kind::Diagnostic, b"two")
        .unwrap();
    assert_eq!(publication.publish().unwrap().generation, 1);
}

/// The names outside runs a reader sees, each read through its entry.
fn names(root: &Path) -> BTreeSet<String> {
    let reader = Reader::open(root, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    for entry in generation.references() {
        reader.read(entry).unwrap();
    }
    generation.entries().keys().cloned().collect()
}

/// The digests of every file under `objects/`.
fn objects(root: &Path) -> BTreeSet<String> {
    std::fs::read_dir(root.join("objects"))
        .unwrap()
        .map(|file| file.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| name.to_string()).collect()
}

#[test]
fn a_deletion_removes_what_no_run_and_no_import_holds() {
    let dir = Scratch::new("store-holds");
    let root = root(&dir);
    with_holds(&root);
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    let generation = store.snapshot().unwrap();
    let one = generation.run("one").unwrap();
    let held: Vec<&str> = one
        .entries()
        .keys()
        .filter_map(|name| name.strip_prefix(HOLDS_PREFIX))
        .collect();
    assert_eq!(
        held,
        [
            "artifacts/imported",
            "artifacts/only-one",
            "artifacts/shared"
        ]
    );
    // The mark of an import is a name outside runs, with the content it marks.
    let marked = format!("{IMPORTED_PREFIX}artifacts/imported");
    assert_eq!(generation.get(&marked), Some(&entry(IMPORTED)));
    // A reader that holds the generation from before the deletion keeps it whole.
    let before = Reader::open(&root, Arc::new(RealFs)).unwrap();
    let kept = before.snapshot().unwrap();

    store.delete(&["one"]).unwrap();
    // Run two holds the shared image and the import marked its own; the older image was
    // never held by a run, so no deletion accounts for it; the spend record is made.
    let left = set(&[
        "artifacts/imported",
        "artifacts/older",
        "artifacts/shared",
        "imported/artifacts/imported",
        "spend/1",
    ]);
    assert_eq!(names(&root), left);
    let pruned = store.prune().unwrap();
    assert_eq!(pruned.generations_in_use, [1]);
    for entry in kept.references() {
        assert!(
            before.read(entry).is_ok(),
            "the held generation reads whole"
        );
    }
    drop(before);

    store.delete(&["two"]).unwrap();
    let left = set(&[
        "artifacts/imported",
        "artifacts/older",
        "imported/artifacts/imported",
        "spend/1",
    ]);
    assert_eq!(names(&root), left);
    // Prune removes the content that only the removed names had; what is left is each
    // name's, and nothing else.
    store.prune().unwrap();
    let named: BTreeSet<String> = (store.snapshot().unwrap().references())
        .map(|entry| entry.digest.hex())
        .collect();
    assert_eq!(objects(&root), named);
    for gone in [ONLY_ONE, SHARED] {
        assert!(!objects(&root).contains(&Digest::of_bytes(gone).hex()));
    }
}

#[test]
fn a_run_that_holds_a_name_a_deletion_removed_meanwhile_names_it_again() {
    // A run's admission holds each of its inputs, read from the generation it was
    // admitted against. A deletion published between that reading and the admission
    // removed the name: the admission gives it back, with the content the run read.
    let dir = Scratch::new("store-holds");
    let root = root(&dir);
    with_holds(&root);
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    let mut admission = store.begin().unwrap();
    admission
        .hold_in("three", "artifacts/only-one", entry(ONLY_ONE))
        .unwrap();
    admission
        .put_in("three", "record", Kind::Diagnostic, b"three")
        .unwrap();
    store.delete(&["one"]).unwrap();
    // A prune meanwhile keeps what a publication in progress holds.
    store.prune().unwrap();
    assert!(!names(&root).contains("artifacts/only-one"));
    admission.publish().unwrap();
    assert!(names(&root).contains("artifacts/only-one"));
    let generation = store.snapshot().unwrap();
    let held = format!("{HOLDS_PREFIX}artifacts/only-one");
    assert_eq!(
        generation.run("three").unwrap().get(&held),
        Some(&entry(ONLY_ONE))
    );
    // A name made under the records that outlive every run is never held.
    let mut other = store.begin().unwrap();
    assert!(other.hold_in("three", "spend/1", entry(SPEND)).is_err());
}

#[test]
fn a_name_kept_in_a_run_is_that_runs_alone_and_goes_with_it() {
    let dir = Scratch::new("store-holds");
    let root = root(&dir);
    with_holds(&root);
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    let mut publication = store.begin().unwrap();
    publication
        .keep_in("one", "artifacts/crop", b"a crop a child made")
        .unwrap();
    // A run that is not there is given nothing: its calls are over with it.
    publication
        .keep_in("gone", "artifacts/crop", b"a crop a child made")
        .unwrap();
    publication.publish().unwrap();
    let generation = store.snapshot().unwrap();
    assert!(generation.get("artifacts/crop").is_none());
    assert!(generation.run("gone").is_none());
    let crop = generation
        .run("one")
        .unwrap()
        .get("artifacts/crop")
        .unwrap();
    assert_eq!(store.read_output(crop).unwrap(), b"a crop a child made");
    // As run one's calls read the store, and as any other's does.
    let scoped = generation.clone().scoped_to("one");
    assert_eq!(scoped.get("artifacts/crop"), Some(crop));
    assert!(
        generation
            .clone()
            .scoped_to("two")
            .get("artifacts/crop")
            .is_none()
    );
    store.delete(&["one"]).unwrap();
    assert!(!names(&root).contains("artifacts/crop"));
}

/// The process under test: deletes run `one`, then prunes, as `whizbang store delete`
/// does.
fn delete_and_prune(root: &Path, fs: Arc<ShimFs>) -> Result<(), whizbang_core::store::Error> {
    let store = Store::open(root, fs)?;
    store.delete(&["one"])?;
    store.prune()?;
    Ok(())
}

#[test]
fn a_crash_at_any_event_of_a_deletion_and_its_prune_leaves_it_done_or_undone_and_never_removes_what_is_held()
 {
    let count = {
        let dir = Scratch::new("store-holds");
        let root = root(&dir);
        with_holds(&root);
        let fs = Arc::new(ShimFs::new());
        delete_and_prune(&root, fs.clone()).unwrap();
        fs.events().len()
    };
    let before = set(&[
        "artifacts/imported",
        "artifacts/older",
        "artifacts/only-one",
        "artifacts/shared",
        "imported/artifacts/imported",
        "spend/1",
    ]);
    let after: BTreeSet<String> = (before.iter())
        .filter(|name| *name != "artifacts/only-one")
        .cloned()
        .collect();
    for crash_at in 0..=count {
        for_every_loss(|lost| {
            let case = format!("{lost:?}, crash before event {crash_at}");
            let dir = Scratch::new("store-holds");
            let root = root(&dir);
            with_holds(&root);
            let fs = Arc::new(ShimFs::new().crash_before(crash_at, lost));
            let result = delete_and_prune(&root, fs.clone());
            fs.crash(lost);

            // The store opens; the deletion is there whole or not at all, and every name
            // reads through its entry (`names` reads each).
            let store = Store::open(&root, Arc::new(ShimFs::new()))
                .unwrap_or_else(|e| panic!("{case}: {e}"));
            let number = store.snapshot().unwrap().number();
            match number {
                1 => {
                    assert!(result.is_err(), "{case}: success was reported");
                    assert_eq!(names(&root), before, "{case}");
                }
                2 => assert_eq!(names(&root), after, "{case}"),
                other => panic!("{case}: generation {other}"),
            }
            if matches!(lost, Lost::Unsynced) && crash_at == count {
                assert_eq!(number, 2, "{case}");
            }
            // Finishing it, or doing it again, leaves the names of a finished deletion,
            // and content for each name and nothing else.
            if number == 1 {
                store.delete(&["one"]).unwrap();
            }
            store.prune().unwrap();
            assert_eq!(names(&root), after, "{case}");
            let named: BTreeSet<String> = (store.snapshot().unwrap().references())
                .map(|entry| entry.digest.hex())
                .collect();
            assert_eq!(objects(&root), named, "{case}");
            losses(&fs)
        });
    }
}

type Body = Box<dyn Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync>;

struct Act(ActionDescriptor, Body);

impl Action for Act {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        (self.1)(call, context)
    }
}

fn act(draft: whizbang_core::contract::Draft, body: Body) -> Arc<dyn Action> {
    Arc::new(Act(draft.build().unwrap(), body))
}

fn draft(name: &str) -> whizbang_core::contract::Draft {
    let kind = whizbang_core::contract::Kind::Primitive;
    ActionDescriptor::draft(name, "1.0.0", kind, "A test action.")
}

/// The mask the action `make` hands back, its origin naming it.
fn made() -> Mask {
    rect_mask_by(on("photo"), "make")
}

#[test]
fn another_root_run_cannot_read_a_childs_image_of_this_one() {
    let dir = Scratch::new("store-holds");
    let store = Arc::new(Store::open(&root(&dir), Arc::new(ShimFs::new())).unwrap());
    let id = made().id();
    let read =
        move |context: &Context| artifact::get(context.store(), ArtifactType::Mask, id, &limits());
    let make = act(
        draft("make").output(OutputPort::one("mask", ArtifactType::Mask, "A mask.")),
        Box::new(|_, _| Ok(Produced::nothing().with("mask", made()))),
    );
    let take = act(
        draft("take").input(InputPort::required("mask", ArtifactType::Mask, "A mask.")),
        Box::new(move |_, context| {
            read(context).unwrap();
            Ok(Produced::nothing())
        }),
    );
    // The first run of a tree calls `make`, reads its mask, and hands it to a second
    // call: all of it inside the tree.
    let outer = act(
        draft("outer"),
        Box::new(move |_, context| {
            assert!(matches!(read(context), Err(ArtifactError::Missing { .. })));
            context.executor().submit(&Call::new("make")).unwrap();
            read(context).unwrap();
            let taken = Call::new("take").input("mask", id);
            let finished = context.executor().submit(&taken).unwrap();
            assert_eq!(finished.outcome().name(), "completed");
            Ok(Produced::nothing())
        }),
    );
    let mut registry = Registry::new();
    for action in [make, take, outer] {
        registry.register(action).unwrap();
    }
    let engine = Engine::over(
        registry,
        Arc::new(NoBackends),
        store.clone(),
        Digest::of_bytes(b""),
    );
    let finished = engine.submit(&Call::new("outer")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    let first = finished.run().as_str().to_owned();

    // The mask is a name of the first run alone: none outside runs.
    let name = artifact::name(ArtifactType::Mask, id);
    let generation = store.snapshot().unwrap();
    assert!(generation.get(&name).is_none());
    let kept = generation.run(&first).unwrap().get(&name).unwrap();
    assert_eq!(store.read_output(kept).unwrap(), made().to_bytes());
    assert!(!artifact::holds(&generation, ArtifactType::Mask, id));

    // A run of another tree that names it is refused: the store holds no such input.
    match engine.submit(&Call::new("take").input("mask", id)) {
        Err(CallError::Refused(Refusal::Input { fault, .. })) => {
            assert_eq!(fault, InputFault::Missing);
        }
        other => panic!("{other:?}"),
    }
    // Nor does a reader find it.
    let reader = Reader::open(&root(&dir), Arc::new(RealFs)).unwrap();
    let got = artifact::get(&reader, ArtifactType::Mask, id, &limits());
    assert!(matches!(got, Err(ArtifactError::Missing { .. })));

    // It goes with the first run; the run of `make` keeps its own reference.
    store.delete(&[first.as_str()]).unwrap();
    let generation = store.snapshot().unwrap();
    assert!(
        generation
            .references()
            .any(|entry| entry.digest == kept.digest)
    );
    assert!(
        !generation
            .runs()
            .values()
            .any(|run| run.get(&name).is_some())
    );
}
