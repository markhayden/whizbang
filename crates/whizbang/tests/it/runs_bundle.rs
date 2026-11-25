//! `runs_bundle::…`: `whizbang runs bundle` and `whizbang bundle import|show|list|delete`,
//! the real binary on real stores (ADR 0048; plan m7, task B1, items 4, 5 and 8): a
//! bundle written from one store goes into another, and is all that store needs of the
//! runs once the first store and the file are gone; it is the store's evidence, never its
//! runs; and the committed fixture, `fixtures/bundle/remove-then-check.tar`, reads.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use whizbang_core::artifact::{ArtifactType, Location, Origin, Shape, Step};
use whizbang_core::bundle;
use whizbang_core::contract::{
    Action, ActionDescriptor, Call, Context, Executor, Failure, OutputPort, PreparedCall, Produced,
    Value,
};
use whizbang_core::identity::{Binding, Digest, PixelDigest, Space};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Kind, Reader, Store};
use whizbang_rascal::checked::{CHECK, REMOVE};
use whizbang_testkit::scratch::Scratch;

use crate::bundle::{Members, chain};
use crate::runner;

struct Ran {
    code: Option<i32>,
    out: String,
    err: String,
}

fn whizbang(args: &[&str], paths: &[&Path]) -> Ran {
    let mut command = crate::wired::binary();
    command.args(args);
    for path in paths {
        command.arg(path);
    }
    let out = command.output().expect("the binary runs");
    Ran {
        code: out.status.code(),
        out: String::from_utf8(out.stdout).expect("output is UTF-8"),
        err: String::from_utf8(out.stderr).expect("output is UTF-8"),
    }
}

/// Runs the binary and holds it to succeed: what it printed.
fn ok(args: &[&str], paths: &[&Path]) -> String {
    let ran = whizbang(args, paths);
    assert_eq!(ran.code, Some(0), "{args:?}: {}", ran.err);
    ran.out
}

/// The committed fixture.
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/bundle/remove-then-check.tar")
}

/// Every name the store holds under a bundle's entry, with what it holds.
fn entry_names(root: &Path, digest: &Digest) -> Vec<(String, Kind, Vec<u8>)> {
    let reader = Reader::open(root, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    let entry = format!("{}/", bundle::entry(digest));
    (generation.entries().iter())
        .filter(|(name, _)| name.starts_with(&entry))
        .map(|(name, held)| (name.clone(), held.kind, reader.read(held).unwrap()))
        .collect()
}

/// Holds a store to hold every member of a bundle under its entry, byte for byte, the
/// files as outputs and the runs' members as diagnostics, and nothing else there.
fn holds_whole(root: &Path, bytes: &[u8]) {
    let members = Members::of(bytes);
    let digest = Digest::of_bytes(bytes);
    let entry = bundle::entry(&digest);
    let held = entry_names(root, &digest);
    assert_eq!(held.len(), members.members.len() + 1);
    for (name, kind, content) in held {
        let member = name.strip_prefix(&format!("{entry}/")).unwrap();
        if member == "manifest" {
            assert_eq!(kind, Kind::Output);
            let manifest: serde_json::Value = serde_json::from_slice(&content).unwrap();
            assert_eq!(manifest, members.manifest);
            continue;
        }
        let (member, expected) = match member.strip_prefix("files/") {
            Some(hex) => (format!("files/sha256-{hex}"), Kind::Output),
            None => (member.to_owned(), Kind::Diagnostic),
        };
        assert_eq!(kind, expected, "{member}");
        assert_eq!(content, members.members[&member], "{member}");
    }
}

#[test]
fn a_bundle_imports_into_an_empty_store_and_survives_its_archive_and_first_store_gone_a_restart_and_a_prune()
 {
    let chain = chain("runs-bundle-import");
    let dir = Scratch::new("runs-bundle-elsewhere");
    let file = dir.path().join("chain.tar");
    let said = ok(
        &["runs", "bundle", &chain.removal, &chain.check, "--to"],
        &[&file, Path::new("--store"), &chain.world.root],
    );
    assert!(said.starts_with("bundled 2 steps ("), "{said}");
    let bytes = std::fs::read(&file).unwrap();
    let digest = Digest::of_bytes(&bytes);
    assert!(said.contains(&digest.to_string()), "{said}");
    // Not inside the store, and not through a link.
    let inside = whizbang(
        &["runs", "bundle", &chain.removal, "--to"],
        &[
            &chain.world.root.join("x.tar"),
            Path::new("--store"),
            &chain.world.root,
        ],
    );
    assert_eq!(inside.code, Some(1));
    assert!(inside.err.contains("is inside the store"), "{}", inside.err);
    std::os::unix::fs::symlink(&file, dir.path().join("link.tar")).unwrap();
    let linked = whizbang(
        &["runs", "bundle", &chain.removal, "--overwrite", "--to"],
        &[
            &dir.path().join("link.tar"),
            Path::new("--store"),
            &chain.world.root,
        ],
    );
    assert_eq!(linked.code, Some(1));
    assert!(linked.err.contains("is a link"), "{}", linked.err);

    // Into a store that is not there yet.
    let other = dir.path().join("other");
    let missing = whizbang(
        &["bundle", "import"],
        &[&file, Path::new("--store"), &other],
    );
    assert_eq!(missing.code, Some(1));
    assert!(missing.err.contains("pass --create"), "{}", missing.err);
    let said = ok(
        &["bundle", "import"],
        &[&file, Path::new("--store"), &other, Path::new("--create")],
    );
    assert!(said.starts_with("created a store at "), "{said}");
    assert!(
        said.contains(&format!("imported bundle {digest} (2 steps)")),
        "{said}"
    );

    // The first store and the archive go; each command after is a process of its own.
    let first = chain.world.root.clone();
    std::fs::remove_dir_all(&first).unwrap();
    std::fs::remove_file(&file).unwrap();
    drop(chain);
    let pruned = ok(&["store", "prune", "--store"], &[&other]);
    assert!(pruned.contains("content files removed: 0"), "{pruned}");
    holds_whole(&other, &bytes);
    let listed = ok(&["bundle", "list", "--store"], &[&other]);
    assert!(listed.starts_with("bundles: 1\n"), "{listed}");
    assert!(
        listed.contains(&format!("{digest}\tsteps 2\t"))
            && listed.contains(&format!("{REMOVE} > {CHECK}")),
        "{listed}"
    );
    // What the store holds of it is the bundle: it reads back whole from the store's
    // names, and checks as the file did.
    let reader = Reader::open(&other, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    let manifest = bundle::manifest_of(&reader, &generation, &digest)
        .unwrap()
        .unwrap();
    assert_eq!(
        manifest,
        bundle::check(Cursor::new(&bytes)).unwrap().manifest
    );
    // A reader holds the generation it read, and a prune leaves it: this one is done.
    drop(reader);

    // Deleted, and pruned: its names go, and so does what only it named.
    let deleted = ok(&["bundle", "delete", &digest.hex(), "--store"], &[&other]);
    assert!(
        deleted.starts_with(&format!("deleted bundle {digest}\n")),
        "{deleted}"
    );
    assert!(entry_names(&other, &digest).is_empty());
    let objects = std::fs::read_dir(other.join("objects")).unwrap().count();
    assert_eq!(objects, 0, "{deleted}");
    let again = whizbang(
        &["bundle", "delete", &digest.to_string(), "--store"],
        &[&other],
    );
    assert_eq!(again.code, Some(1));
    assert!(again.err.contains("holds no bundle"), "{}", again.err);
}

#[test]
fn a_second_import_changes_nothing_and_its_runs_are_not_the_stores_own() {
    let chain = chain("runs-bundle-twice");
    let dir = Scratch::new("runs-bundle-twice-elsewhere");
    let file = dir.path().join("chain.tar");
    let root = chain.world.root.clone();
    ok(
        &["runs", "bundle", &chain.removal, &chain.check, "--to"],
        &[&file, Path::new("--store"), &root],
    );
    let digest = Digest::of_bytes(&std::fs::read(&file).unwrap());
    let other = dir.path().join("other");
    ok(
        &["bundle", "import"],
        &[&file, Path::new("--store"), &other, Path::new("--create")],
    );
    let generation = |root: &Path| {
        let reader = Reader::open(root, Arc::new(RealFs)).unwrap();
        reader.snapshot().unwrap().number()
    };
    let before = generation(&other);
    let said = ok(
        &["bundle", "import"],
        &[&file, Path::new("--store"), &other],
    );
    assert_eq!(
        said,
        format!("already there: bundle {digest} (2 steps); nothing changed\n")
    );
    assert_eq!(generation(&other), before);
    // No run of the store: listed, counted or booked.
    let runs = ok(&["runs", "list", "--store"], &[&other]);
    assert!(runs.starts_with("runs: 0\n"), "{runs}");
    let stats = ok(&["runs", "stats", "--store"], &[&other]);
    assert!(stats.starts_with("runs: 0\n"), "{stats}");
    let store = ok(&["store", "list", "--store"], &[&other]);
    assert!(store.contains("\nruns: 0\n"), "{store}");
    // Into the store the runs came from: its own runs, its figures and its books are
    // what they were.
    let (listed, counted, booked) = (
        ok(&["runs", "list", "--store"], &[&root]),
        ok(&["runs", "stats", "--store"], &[&root]),
        ok(&["spend", "status", "--store"], &[&root]),
    );
    let said = ok(&["bundle", "import"], &[&file, Path::new("--store"), &root]);
    assert!(
        said.starts_with(&format!("imported bundle {digest}")),
        "{said}"
    );
    assert_eq!(ok(&["runs", "list", "--store"], &[&root]), listed);
    assert_eq!(ok(&["runs", "stats", "--store"], &[&root]), counted);
    assert_eq!(ok(&["spend", "status", "--store"], &[&root]), booked);
    // Deleting a run of the store leaves the bundle's copy of it.
    ok(&["store", "delete", &chain.removal, "--store"], &[&root]);
    holds_whole(&root, &std::fs::read(&file).unwrap());
    // A refused bundle changes nothing, and says so.
    let broken = dir.path().join("broken.tar");
    let mut members = Members::of(&std::fs::read(&file).unwrap());
    let name = members
        .members
        .keys()
        .find(|n| n.starts_with("files/"))
        .unwrap()
        .clone();
    members.members.get_mut(&name).unwrap()[0] ^= 1;
    std::fs::write(&broken, members.bytes()).unwrap();
    let before = generation(&other);
    let refused = whizbang(
        &["bundle", "import"],
        &[&broken, Path::new("--store"), &other],
    );
    assert_eq!(refused.code, Some(1));
    assert!(
        refused.err.contains("nothing was imported and nothing ran"),
        "{}",
        refused.err
    );
    assert_eq!(generation(&other), before);
    let shown = whizbang(&["bundle", "show"], &[&broken]);
    assert_eq!(shown.code, Some(1));
}

/// An action that returns one location, whose origin says what it was asked: the same
/// location, so the same identity, from two runs asked two things, in two files.
struct Places(ActionDescriptor);

fn the_place(said: &str) -> Location {
    let photo = Binding::root(
        PixelDigest(Digest::of_bytes(b"a photo")),
        Space::flat(8, 6).unwrap(),
    );
    let origin = Origin::new(Step::new("places", "1.0.0").argument("said", said)).unwrap();
    Location::new(photo, Shape::WholeImage, origin).unwrap()
}

impl Action for Places {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, call: &PreparedCall, _: &Context) -> Result<Produced, Failure> {
        let said = call
            .arguments()
            .iter()
            .find_map(|(name, value)| match value {
                Value::Text(text) if name == "said" => Some(text.clone()),
                _ => None,
            });
        Ok(Produced::nothing().with("where", the_place(&said.unwrap_or_default())))
    }
}

/// Two runs of [`Places`], asked two things, in a store at `root`.
fn two_places(root: &Path) -> (String, String) {
    let store = Arc::new(Store::open(root, Arc::new(RealFs)).unwrap());
    let mut registry = whizbang::wiring::registry().unwrap();
    let kind = whizbang_core::contract::Kind::Primitive;
    let places = ActionDescriptor::draft("places", "1.0.0", kind, "A test action.")
        .argument(whizbang_core::contract::Argument::required(
            "said",
            "What to say.",
            whizbang_core::contract::ValueType::Text,
        ))
        .output(OutputPort::one(
            "where",
            ArtifactType::Location,
            "Where it is.",
        ));
    registry
        .register(Arc::new(Places(places.build().unwrap())))
        .unwrap();
    let executor = runner(registry, store);
    let run = |said: &str| {
        let call = Call::new("places").argument("said", Value::text(said));
        let finished = executor.submit(&call).unwrap();
        assert_eq!(finished.outcome().outputs().len(), 1);
        finished.run().as_str().to_owned()
    };
    (run("here"), run("there"))
}

#[test]
fn two_files_of_one_identity_with_other_origins_survive_import_a_restart_and_a_prune_byte_for_byte()
{
    let dir = Scratch::new("runs-bundle-origins");
    let root = dir.path().join("store");
    let (here, there) = two_places(&root);
    let (one, two) = (the_place("here"), the_place("there"));
    assert_eq!(one.id(), two.id());
    assert_ne!(one.to_bytes(), two.to_bytes());
    let file = dir.path().join("places.tar");
    ok(
        &["runs", "bundle", &here, &there, "--to"],
        &[&file, Path::new("--store"), &root],
    );
    let bytes = std::fs::read(&file).unwrap();
    let manifest = bundle::check(Cursor::new(&bytes)).unwrap().manifest;
    // Two file members of one artifact, each the file of its own run.
    assert_eq!(manifest.artifacts.len(), 2);
    assert!(manifest.artifacts.iter().all(|f| f.artifact == one.id().0));
    let files: Vec<Digest> = manifest.artifacts.iter().map(|f| f.file).collect();
    for (run, location) in [(&here, &one), (&there, &two)] {
        let file = manifest
            .artifacts
            .iter()
            .find(|f| f.of[0].run == *run)
            .unwrap();
        assert_eq!(file.file, Digest::of_bytes(&location.to_bytes()));
    }
    // Neither action ships: both are carried as evidence.
    assert!(manifest.steps.iter().all(|step| !step.replayable));

    // Into a store that holds the location already, by an ordinary import of one file.
    let other = dir.path().join("other");
    let loose = dir.path().join("one.location");
    std::fs::write(&loose, one.to_bytes()).unwrap();
    ok(
        &["store", "import"],
        &[&loose, Path::new("--store"), &other, Path::new("--create")],
    );
    ok(
        &["bundle", "import"],
        &[&file, Path::new("--store"), &other],
    );
    std::fs::remove_dir_all(&root).unwrap();
    ok(&["store", "prune", "--store"], &[&other]);
    let digest = Digest::of_bytes(&bytes);
    // Each file under its own digest, as an output, byte for byte the file its run made.
    let held = entry_names(&other, &digest);
    for location in [&one, &two] {
        let file = Digest::of_bytes(&location.to_bytes());
        assert!(files.contains(&file));
        let name = format!("{}/files/{}", bundle::entry(&digest), file.hex());
        let (_, kind, content) = held.iter().find(|(n, _, _)| *n == name).unwrap();
        assert_eq!(*kind, Kind::Output);
        assert_eq!(*content, location.to_bytes());
    }
    holds_whole(&other, &bytes);
    // The ordinary import kept its own name, with its own file, and keeps it once the
    // bundle goes.
    ok(
        &["bundle", "delete", &digest.to_string(), "--store"],
        &[&other],
    );
    let reader = Reader::open(&other, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    let name = whizbang_core::artifact::name(ArtifactType::Location, one.id());
    let entry = generation.get(&name).unwrap();
    assert_eq!(reader.read(entry).unwrap(), one.to_bytes());
}

#[test]
fn the_committed_fixture_reads() {
    let bytes = std::fs::read(fixture()).unwrap();
    let checked = bundle::check(Cursor::new(&bytes)).unwrap();
    assert_eq!(checked.digest, Digest::of_bytes(&bytes));
    let manifest = &checked.manifest;
    assert_eq!(manifest.bundle, 1);
    assert_eq!(manifest.steps.len(), 2);
    assert_eq!(manifest.steps[0].action.as_deref(), Some(REMOVE));
    assert_eq!(manifest.steps[1].action.as_deref(), Some(CHECK));
    // The check took the removal's picture, and the picture it came from.
    let link = manifest.steps[1].inputs[0].from.as_ref().unwrap();
    assert_eq!((link.step, link.port.as_str(), link.place), (0, "image", 0));
    assert_eq!(
        manifest.steps[1].inputs[1].artifact,
        manifest.steps[0].inputs[0].artifact
    );
    // Its fill kept its painted answer, and the judge's turns travel.
    let members = Members::of(&bytes);
    let fill = members.run_of(0, "fill");
    assert!(
        members
            .members
            .contains_key(&format!("runs/{fill}/diagnostics/fill-answer.png"))
    );
    assert!(
        members
            .members
            .keys()
            .any(|name| name.contains("/diagnostics/exchange-"))
    );
    assert!(!(manifest.withheld.iter()).any(|w| matches!(w, bundle::Withheld::FillAnswer { .. })));
    // It is what the writer of this build writes: the manifest first and the members in
    // order, every header fixed, so it is the same bytes written again from its members.
    let mut writer = bundle::tar::Writer::new(0);
    writer.add(bundle::MANIFEST, &manifest.to_bytes()).unwrap();
    for (name, content) in &members.members {
        writer.add(name, content).unwrap();
    }
    assert_eq!(writer.finish(), bytes);
    // It imports, and `bundle show` reads it.
    let dir = Scratch::new("runs-bundle-fixture");
    let other = dir.path().join("store");
    ok(
        &["bundle", "import"],
        &[
            &fixture(),
            Path::new("--store"),
            &other,
            Path::new("--create"),
        ],
    );
    holds_whole(&other, &bytes);
    let shown = ok(&["bundle", "show"], &[&fixture()]);
    assert!(
        shown.starts_with(&format!("bundle: {}\n", checked.digest)),
        "{shown}"
    );
    assert!(shown.contains("step 1: run "), "{shown}");
}

/// Writes the fixture from a chain this build runs, to the scratch folder of this test
/// and not over the committed file: how `fixtures/bundle/remove-then-check.tar` was made
/// (it was copied from such a folder, once). A run's id and its times are the clock's, so
/// a fixture written again is another file, and reads the same way.
#[test]
fn a_fixture_is_written_as_the_committed_one_was() {
    let chain = chain("runs-bundle-fixture-made");
    let made = crate::bundle::made(&chain);
    let file = chain
        .world
        .root
        .parent()
        .unwrap()
        .join("remove-then-check.tar");
    std::fs::write(&file, &made.bytes).unwrap();
    let committed = bundle::check(Cursor::new(std::fs::read(fixture()).unwrap()))
        .unwrap()
        .manifest;
    // The same steps, the same shape of families, the same links.
    assert_eq!(made.manifest.steps.len(), committed.steps.len());
    for (new, old) in made.manifest.steps.iter().zip(&committed.steps) {
        assert_eq!(new.action, old.action);
        assert_eq!(new.runs.len(), old.runs.len());
        assert_eq!(new.inputs.len(), old.inputs.len());
        assert_eq!(
            new.inputs.iter().map(|i| &i.from).collect::<Vec<_>>(),
            old.inputs.iter().map(|i| &i.from).collect::<Vec<_>>()
        );
    }
    assert_eq!(made.manifest.artifacts.len(), committed.artifacts.len());
    assert_eq!(made.manifest.withheld, committed.withheld);
}
