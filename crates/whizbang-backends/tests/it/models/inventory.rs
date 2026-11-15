//! Readiness from what is installed (ADR 0017, "The inventory"; plan 1B, task M6):
//! the inventory over a models directory, asked through an executor a test builds
//! over it. Every model is installed by the installer, from the test kit's fake of
//! a download, and the directory is damaged by hand.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use whizbang_backends::models::{
    Catalogue, Entry, InstallationRecord, Installations, ModelsDir, Revision,
};
use whizbang_core::capability::{
    Declaration, Installation, Installed, InstalledRoster, Inventory, Roster, Unready,
};
use whizbang_core::contract::{
    Action, ActionDescriptor, Answer, Call, CallError, Capability, Clock, Context, Engine,
    Executor, Failure, Kind, ModelIdentity, PreparedCall, Produced, Readiness, Refusal, Registry,
    Request, RequestId, Requirement, Stale,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Reader, Store};
use whizbang_testkit::fs_shim::{Event, ShimFs};
use whizbang_testkit::scratch::Scratch;

use super::publishing::{
    adapter, entry_at, first, flip, folder_of, hold_file_of, installed, installing, is_whole,
    record_of, second, tiny,
};
use super::remove::removed;
use super::staging::{serving_both, touches_only};
use super::{MARKER, model, none, parts, revision, secrets};

/// The inventory of this directory: the catalogue holds `tiny` as this entry, and
/// the build has the adapter the entry names, at this revision.
fn inventory_of(root: &Path, entry: &Entry, adapter_revision: &Revision) -> Installations {
    Installations::of(
        ModelsDir::at(Arc::new(RealFs), root),
        Catalogue::new(vec![entry.clone()], &none()).unwrap(),
        [(entry.adapter().clone(), adapter_revision.clone())],
        none(),
    )
}

/// The inventory with the second entry as the catalogue's, under the adapter's
/// revision every install here states.
fn inventory(root: &Path) -> Installations {
    inventory_of(root, &second(), &adapter())
}

fn ready(record: &InstallationRecord) -> Installed {
    Installed::Ready(Installation::of(record.identity().digest()))
}

/// An action that keeps, each time it runs, the version of the model its call was
/// prepared under: for a local model over an inventory, the identity of the
/// installation the run was admitted with.
struct Scripted(ActionDescriptor, Arc<Mutex<Vec<String>>>);

impl Action for Scripted {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, call: &PreparedCall, _: &Context) -> Result<Produced, Failure> {
        let under = call.needs()[0]
            .model()
            .map(|model| model.version().to_owned());
        self.1.lock().unwrap().push(under.unwrap_or_default());
        Ok(Produced::nothing())
    }
}

/// A clock that says one millisecond more each time it is asked.
struct Ticking(AtomicU64);

impl Clock for Ticking {
    fn now_ms(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst)
    }
}

/// An executor a test builds over an inventory: a store of its own, a roster that
/// offers one local segmenter under the name `here`, whose model is `configured`,
/// and one action, `cuts`, that needs a segmenter. `ran` is what each run of the
/// action was handed as the version of its model.
struct World {
    store: Scratch,
    executor: Engine,
    ran: Arc<Mutex<Vec<String>>>,
}

impl World {
    fn over(inventory: Installations, configured: &str) -> World {
        World::reopened(Scratch::new("inventory-store"), inventory, configured)
    }

    /// The same over a store that is there already.
    fn reopened(store: Scratch, inventory: Installations, configured: &str) -> World {
        let ran = Arc::new(Mutex::new(Vec::new()));
        let descriptor =
            ActionDescriptor::draft("cuts", "1.0.0", Kind::Primitive, "A test action.")
                .needs(Requirement::new(Capability::Segment))
                .build()
                .unwrap();
        let mut registry = Registry::new();
        registry
            .register(Arc::new(Scripted(descriptor, ran.clone())))
            .unwrap();
        let declared = ModelIdentity::new("local", configured, "declared");
        let roster = Roster::new()
            .offer(
                Capability::Segment,
                "here",
                Declaration::new(declared, &[]).unwrap(),
            )
            .unwrap()
            .default_for(Capability::Segment, "here")
            .unwrap();
        let backends = InstalledRoster::over(roster, Arc::new(inventory));
        let opened = Store::open(&store.path().join("store"), Arc::new(RealFs)).unwrap();
        let executor = Engine::over(
            registry,
            Arc::new(backends),
            Arc::new(opened),
            Digest::of_bytes(b""),
        )
        .with_clock(Arc::new(Ticking(AtomicU64::new(1000))));
        World {
            store,
            executor,
            ran,
        }
    }

    fn call() -> Call {
        Call::new("cuts")
    }

    /// Why the call is prepared as not ready, and that it is refused at admission
    /// with the same words and makes no run.
    #[track_caller]
    fn why_not(&self) -> String {
        let prepared = self.executor.prepare(&World::call()).unwrap();
        let Readiness::NotReady(missing) = prepared.readiness() else {
            panic!("the call is ready: {prepared:?}");
        };
        assert_eq!(missing.len(), 1);
        let why = missing[0].why().to_owned();
        let Err(CallError::Refused(Refusal::NotReady { missing, .. })) =
            self.executor.submit(&World::call())
        else {
            panic!("admitted, though its model is not ready");
        };
        assert_eq!(missing[0].why(), why);
        assert!(self.ran().is_empty());
        why
    }

    /// The version of the model the call is prepared under, when it is ready.
    #[track_caller]
    fn prepared_under(&self) -> (PreparedCall, String) {
        let prepared = self.executor.prepare(&World::call()).unwrap();
        assert_eq!(prepared.readiness(), &Readiness::Ready, "{prepared:?}");
        let version = prepared.needs()[0].model().unwrap().version().to_owned();
        (prepared, version)
    }

    /// Under which version of its model each run of the action ran, in order.
    fn ran(&self) -> Vec<String> {
        self.ran.lock().unwrap().clone()
    }

    /// How many runs the store holds.
    fn runs(&self) -> usize {
        let reader = Reader::open(&self.store.path().join("store"), Arc::new(RealFs)).unwrap();
        reader.snapshot().unwrap().runs().len()
    }
}

const NOT_READY: &str = "the segment backend here is local, and its model is not ready: ";

// --- the five answers ---------------------------------------------------------------------------

#[test]
fn a_configured_model_that_the_catalogue_does_not_hold_is_prepared_as_not_ready_saying_so() {
    let scratch = Scratch::new("inventory-catalogue");
    let root = scratch.path();
    // `tiny` is in the catalogue and installed. The config names another model.
    installed(root, &second());
    let world = World::over(inventory(root), "tiny-two");
    assert_eq!(
        world.why_not(),
        format!("{NOT_READY}it is not in the catalogue")
    );
    // The name is compared exactly: no case is folded, nothing is cut or joined.
    let asked = inventory(root);
    for name in [
        "Tiny",
        "TINY",
        "tiny ",
        " tiny",
        "tiny/",
        "tin",
        "tiny.",
        "",
        "models/tiny",
    ] {
        assert_eq!(
            asked.look(name),
            Installed::NotReady(Unready::NotInCatalogue),
            "{name:?}"
        );
    }
    assert_eq!(asked.look("tiny"), ready(&record_of(&second())));
}

#[test]
fn a_declared_name_is_looked_up_in_the_catalogue_and_never_used_as_a_path() {
    // Names a config can hold and no catalogue can: with a separator, with dots
    // that lead up and back down to the model's own folder, absolute, with a
    // secret. None is in the catalogue, and nothing on the disk is opened for any.
    let scratch = Scratch::new("inventory-path");
    let root = scratch.path();
    installed(root, &second());
    let fs = Arc::new(ShimFs::new());
    let asked = Installations::of(
        ModelsDir::at(fs.clone(), root),
        Catalogue::new(vec![second()], &none()).unwrap(),
        [(second().adapter().clone(), adapter())],
        secrets(),
    );
    let absolute = tiny(root).display().to_string();
    for name in [
        "../tiny",
        "tiny/../tiny",
        "./tiny",
        "x/../../models/tiny",
        "tiny/installations",
        absolute.as_str(),
        "..",
        ".",
        MARKER,
    ] {
        assert_eq!(
            asked.look(name),
            Installed::NotReady(Unready::NotInCatalogue),
            "{name:?}"
        );
        assert!(asked.ready(name).is_err());
    }
    assert!(fs.events().is_empty(), "{:?}", fs.events());
    // The one name the catalogue holds: its pointer and its record are read, and
    // nothing else is opened, locked or changed.
    assert_eq!(asked.look("tiny"), ready(&record_of(&second())));
    let events = fs.events();
    let record = folder_of(root, &record_of(&second())).join("RECORD");
    assert_eq!(
        events,
        [
            Event::Open(tiny(root).join("CURRENT")),
            Event::Read(tiny(root).join("CURRENT")),
            Event::Open(record.clone()),
            Event::Read(record),
        ]
    );
    touches_only(&events, root);
    assert_eq!(format!("{asked:?}"), "Installations(1 models)"); // A catalogue made by a reader that was told of other secrets than the
    // inventory: a model's name that holds a secret the inventory was told of is
    // not looked for on the disk, under that name or any other.
    let named = Entry::new(parts(MARKER), &none()).unwrap();
    let fs = Arc::new(ShimFs::new());
    let asked = Installations::of(
        ModelsDir::at(fs.clone(), root),
        Catalogue::new(vec![named.clone()], &none()).unwrap(),
        [(named.adapter().clone(), adapter())],
        secrets(),
    );
    assert_eq!(
        asked.look(MARKER),
        Installed::NotReady(Unready::NotInCatalogue)
    );
    assert!(fs.events().is_empty(), "{:?}", fs.events());
}

#[test]
fn a_model_in_the_catalogue_is_ready_when_installed_and_not_installed_otherwise() {
    let scratch = Scratch::new("inventory-installed");
    let root = scratch.path();
    let world = World::over(inventory(root), "tiny");
    // Never installed: no folder of the model, and none is made by asking.
    assert_eq!(world.why_not(), format!("{NOT_READY}it is not installed"));
    assert!(std::fs::read_dir(root).unwrap().next().is_none());
    // Installed: ready, under the identity of the installation, as its version.
    let record = installed(root, &second());
    let (_, version) = world.prepared_under();
    assert_eq!(version, record.identity().to_string());
    assert_eq!(inventory(root).ready("tiny"), Ok(record.clone()));
    // Removed: not installed again.
    removed(root).unwrap();
    assert_eq!(world.why_not(), format!("{NOT_READY}it is not installed"));
    // A removal that has cleared the pointer and not yet taken its mark off: not
    // installed. And a mark alone, over an installation that is named, does not
    // make it unready: no hold is given on it meanwhile, which is the lease's.
    std::fs::write(tiny(root).join("DRAINING"), b"").unwrap();
    assert_eq!(world.why_not(), format!("{NOT_READY}it is not installed"));
    std::fs::remove_file(tiny(root).join("DRAINING")).unwrap();
    installed(root, &second());
    std::fs::write(tiny(root).join("DRAINING"), b"").unwrap();
    assert_eq!(inventory(root).look("tiny"), ready(&record));
}

#[test]
fn an_installation_whose_recorded_catalogue_or_adapter_revision_is_not_the_current_one_is_prepared_as_out_of_date()
 {
    let scratch = Scratch::new("inventory-revisions");
    let root = scratch.path();
    let record = installed(root, &second());
    let said = format!("{NOT_READY}its installation is out of date");
    // The catalogue's entry is at another revision than the one installed.
    let world = World::over(
        inventory_of(root, &entry_at("2026-11.3"), &adapter()),
        "tiny",
    );
    assert_eq!(world.why_not(), said);
    // The adapter states another revision than the one the installation was made
    // under.
    let world = World::over(inventory_of(root, &second(), &revision("8")), "tiny");
    assert_eq!(world.why_not(), said);
    // Two revisions are the same or they are not: an "older" catalogue is out of
    // date as a "newer" one is.
    let world = World::over(inventory_of(root, &first(), &adapter()), "tiny");
    assert_eq!(world.why_not(), said);
    // The build has no adapter of the name the entry gives: no revision is current.
    let no_adapter = Installations::of(
        ModelsDir::at(Arc::new(RealFs), root),
        Catalogue::new(vec![second()], &none()).unwrap(),
        [(model("another-adapter"), adapter())],
        none(),
    );
    assert_eq!(
        no_adapter.look("tiny"),
        Installed::NotReady(Unready::OutOfDate)
    );
    // The entry names another adapter at the same revisions, or other files: what
    // would be installed now is not what is installed.
    let mut other = parts("tiny");
    other.revision = revision("2026-11.2");
    other.adapter = model("another-adapter");
    other.files = second().files().to_vec();
    let other = Entry::new(other, &none()).unwrap();
    let world = World::over(inventory_of(root, &other, &adapter()), "tiny");
    assert_eq!(world.why_not(), said);
    // Both current: ready. Installing the current entry makes it ready again.
    let world = World::over(
        inventory_of(root, &entry_at("2026-11.3"), &adapter()),
        "tiny",
    );
    assert_eq!(world.why_not(), said);
    let new = installed(root, &entry_at("2026-11.3"));
    let (_, version) = world.prepared_under();
    assert_eq!(version, new.identity().to_string());
    assert_ne!(new.identity(), record.identity());
}

#[test]
fn an_installation_record_that_is_damaged_or_a_pointer_that_names_nothing_is_prepared_as_damaged_and_a_new_install_repairs_it()
 {
    type Damage = fn(&Path, &InstallationRecord);
    let damages: [(&str, Damage); 9] = [
        ("the record is cut", |root, record| {
            let at = folder_of(root, record).join("RECORD");
            let bytes = std::fs::read(&at).unwrap();
            std::fs::write(at, &bytes[..bytes.len() / 2]).unwrap();
        }),
        ("the record has one byte another", |root, record| {
            flip(&folder_of(root, record).join("RECORD"), 40);
        }),
        ("the record is gone", |root, record| {
            std::fs::remove_file(folder_of(root, record).join("RECORD")).unwrap();
        }),
        ("the record is another installation's", |root, record| {
            let other = record_of(&first());
            std::fs::write(folder_of(root, record).join("RECORD"), other.to_bytes()).unwrap();
        }),
        ("the record is a folder", |root, record| {
            let at = folder_of(root, record).join("RECORD");
            std::fs::remove_file(&at).unwrap();
            std::fs::create_dir(at).unwrap();
        }),
        (
            "the pointer names a folder that is not there",
            |root, record| {
                std::fs::remove_dir_all(folder_of(root, record)).unwrap();
            },
        ),
        (
            "the pointer names an installation that never was",
            |root, _| {
                let never = record_of(&entry_at("2026-11.9")).identity();
                std::fs::write(tiny(root).join("CURRENT"), format!("{never}\n")).unwrap();
            },
        ),
        ("the pointer is not a pointer", |root, _| {
            std::fs::write(tiny(root).join("CURRENT"), b"damaged").unwrap();
        }),
        ("the pointer is empty", |root, _| {
            std::fs::write(tiny(root).join("CURRENT"), b"").unwrap();
        }),
    ];
    for (what, damage) in damages {
        let scratch = Scratch::new("inventory-damaged");
        let root = scratch.path();
        let record = installed(root, &second());
        let world = World::over(inventory(root), "tiny");
        world.prepared_under();
        damage(root, &record);
        assert_eq!(
            world.why_not(),
            format!("{NOT_READY}its installation is damaged"),
            "{what}"
        );
        assert_eq!(
            inventory(root).ready("tiny"),
            Err(Unready::Damaged),
            "{what}"
        );
        // A new install repairs it: ready again, under the same identity, whole.
        let fs = Arc::new(ShimFs::new());
        let repaired = installing(&fs, root, &second(), &serving_both());
        assert_eq!(repaired, Ok(record.clone()), "{what}");
        is_whole(root, &record);
        assert!(hold_file_of(root, &record).exists());
        let (_, version) = world.prepared_under();
        assert_eq!(version, record.identity().to_string(), "{what}");
    }
}

#[test]
fn a_pointer_or_a_record_that_cannot_be_read_is_prepared_as_damaged_never_ready_and_never_not_installed()
 {
    // The disk refuses one call, once: the opening of the pointer, a read of it,
    // the opening of the record, a read of it.
    type Refused = fn(&Event) -> bool;
    let refusals: [(&str, Refused); 4] = [
        (
            "the pointer cannot be opened",
            |event| matches!(event, Event::Open(p) if p.ends_with("CURRENT")),
        ),
        (
            "the pointer cannot be read",
            |event| matches!(event, Event::Read(p) if p.ends_with("CURRENT")),
        ),
        (
            "the record cannot be opened",
            |event| matches!(event, Event::Open(p) if p.ends_with("RECORD")),
        ),
        (
            "the record cannot be read",
            |event| matches!(event, Event::Read(p) if p.ends_with("RECORD")),
        ),
    ];
    for (what, refusal) in refusals {
        let scratch = Scratch::new("inventory-unreadable");
        let root = scratch.path();
        let record = installed(root, &second());
        let fs = Arc::new(ShimFs::new());
        let over = || {
            Installations::of(
                ModelsDir::at(fs.clone(), root),
                Catalogue::new(vec![second()], &none()).unwrap(),
                [(second().adapter().clone(), adapter())],
                none(),
            )
        };
        let world = World::over(over(), "tiny");
        world.prepared_under();
        // Asked through the executor: prepared as not ready, as damaged.
        fs.refuse_matching(0, refusal);
        let prepared = world.executor.prepare(&World::call()).unwrap();
        let Readiness::NotReady(missing) = prepared.readiness() else {
            panic!("{what}: the call is ready: {prepared:?}");
        };
        assert_eq!(
            missing[0].why(),
            format!("{NOT_READY}its installation is damaged"),
            "{what}"
        );
        // Asked of the inventory: damaged, and neither of the other two.
        fs.refuse_matching(0, refusal);
        assert_eq!(over().ready("tiny"), Err(Unready::Damaged), "{what}");
        // The disk answers again: ready, under the same identity. Nothing was
        // changed by the asking.
        assert_eq!(over().look("tiny"), ready(&record), "{what}");
        is_whole(root, &record);
    }
}

#[cfg(unix)]
#[test]
fn a_pointer_or_a_record_the_system_refuses_permission_to_read_is_damaged_and_never_not_installed()
{
    use std::os::unix::fs::PermissionsExt;
    for file in ["CURRENT", "RECORD"] {
        let scratch = Scratch::new("inventory-permission");
        let root = scratch.path();
        let record = installed(root, &second());
        let at = match file {
            "CURRENT" => tiny(root).join("CURRENT"),
            _ => folder_of(root, &record).join("RECORD"),
        };
        std::fs::set_permissions(&at, std::fs::Permissions::from_mode(0o000)).unwrap();
        // One who may read every file whatever its mode (root) is refused nothing,
        // and has nothing to show here.
        if std::fs::read(&at).is_err() {
            assert_eq!(
                inventory(root).look("tiny"),
                Installed::NotReady(Unready::Damaged),
                "{file}"
            );
        }
        std::fs::set_permissions(&at, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(inventory(root).look("tiny"), ready(&record), "{file}");
    }
}

#[cfg(unix)]
#[test]
fn a_model_one_of_whose_layout_folders_is_a_link_is_damaged_and_never_ready() {
    // What every install, hold and removal refuses is not called ready (ADR 0017,
    // "A folder of the layout that is a link"): the folder is moved out, a link
    // is left at its name, and the pointer and the record read through it as before.
    for folder in [
        "models",
        "models/tiny",
        "models/tiny/installations",
        "models/tiny/staging",
        "models/tiny/holds",
    ] {
        let scratch = Scratch::new("inventory-link");
        let outside = Scratch::new("inventory-link-outside");
        let root = scratch.path();
        let record = installed(root, &second());
        let world = World::over(inventory(root), "tiny");
        world.prepared_under();
        let (at, moved) = (root.join(folder), outside.path().join("moved"));
        std::fs::rename(&at, &moved).unwrap();
        std::os::unix::fs::symlink(&moved, &at).unwrap();
        assert_eq!(
            world.why_not(),
            format!("{NOT_READY}its installation is damaged"),
            "{folder}"
        );
        assert_eq!(
            inventory(root).ready("tiny"),
            Err(Unready::Damaged),
            "{folder}"
        );
        // With the folder back in its place the model is ready, as it was.
        std::fs::remove_file(&at).unwrap();
        std::fs::rename(&moved, &at).unwrap();
        let (_, version) = world.prepared_under();
        assert_eq!(version, record.identity().to_string(), "{folder}");
    }
}

#[test]
fn a_record_that_holds_a_secret_the_inventory_was_told_of_is_damage_and_what_is_said_holds_nothing_of_the_disk()
 {
    let scratch = Scratch::new("inventory-secret");
    let root = scratch.path();
    let record = installed(root, &second());
    // The inventory is told a secret that the record on the disk spells.
    let told = whizbang_core::contract::Secrets::new(["owlv2-sam2".to_owned()]).unwrap();
    let asked = Installations::of(
        ModelsDir::at(Arc::new(RealFs), root),
        Catalogue::new(vec![second()], &none()).unwrap(),
        [(second().adapter().clone(), adapter())],
        told,
    );
    assert_eq!(asked.look("tiny"), Installed::NotReady(Unready::Damaged));
    // What an executor says of a model that is not ready names the capability and
    // the backend as the roster offers it, and then fixed words: nothing of the
    // model, of its identity or of where it is.
    std::fs::write(tiny(root).join("CURRENT"), b"damaged").unwrap();
    let world = World::over(inventory(root), "tiny");
    let said = world.why_not();
    for part in [
        "tiny",
        &*root.to_string_lossy(),
        &record.identity().hex(),
        "CURRENT",
    ] {
        assert!(!said.contains(part), "{said}");
    }
}

#[test]
fn known_an_installation_whose_file_was_damaged_after_it_was_published_is_ready_until_it_is_installed_again()
 {
    // A limit, pinned (ADR 0017, "The inventory"). The inventory is asked at each
    // preparation and reads the pointer and the record, never a file: a file is
    // held to its size and its hash when it is published, and when the same
    // identity is installed again.
    let scratch = Scratch::new("inventory-files");
    let root = scratch.path();
    let record = installed(root, &second());
    flip(&folder_of(root, &record).join("files/graph.onnx"), 77);
    assert_eq!(inventory(root).look("tiny"), ready(&record));
    std::fs::remove_file(folder_of(root, &record).join("files/LICENSE.txt")).unwrap();
    assert_eq!(inventory(root).look("tiny"), ready(&record));
    // Installed again, it is found damaged and repaired.
    assert_eq!(installed(root, &second()), record);
    is_whole(root, &record);
    assert_eq!(inventory(root).look("tiny"), ready(&record));
}

// --- a reinstall under a prepared call ----------------------------------------------------------

#[test]
fn after_a_reinstall_the_old_preparation_is_stale_a_new_request_runs_on_the_new_installation_and_a_repeat_returns_the_first_run()
 {
    let scratch = Scratch::new("inventory-reinstall");
    let root = scratch.path();
    // The catalogue's entry is the second, with one adapter revision at first.
    let old = installed(root, &second());
    let world = World::over(inventory(root), "tiny");
    let (prepared, version) = world.prepared_under();
    assert_eq!(version, old.identity().to_string());
    // The first request, under an id, with its preparation: it runs, on the
    // installation it was prepared under.
    let id = RequestId::parse("first-request").unwrap();
    let first_request = || {
        Request::new(World::call())
            .with_id(id.clone())
            .expecting(&prepared)
    };
    let Ok(Answer::Ran(finished)) = world.executor.request(&first_request()) else {
        panic!("the first request did not run");
    };
    let first_run = finished.run().clone();
    assert_eq!(world.ran(), [old.identity().to_string()]);
    assert_eq!(world.runs(), 1);
    // The model is installed again, damaged in between: the same entry, repaired,
    // is the same identity, and the old preparation is still good.
    flip(&folder_of(root, &old).join("RECORD"), 40);
    assert_eq!(installed(root, &second()), old);
    assert_eq!(
        world
            .executor
            .prepare(&World::call())
            .unwrap()
            .fingerprint(),
        prepared.fingerprint()
    );
    // Reinstalled under another revision of the adapter: another installation. The
    // inventory of an executor that knows the new revision answers under it.
    let dir = ModelsDir::at(Arc::new(RealFs), root);
    let new = whizbang_backends::models::install(
        &dir,
        &second(),
        &revision("8"),
        &serving_both(),
        &whizbang_core::contract::Cancellation::new(),
        &none(),
    )
    .unwrap();
    assert_ne!(new.identity(), old.identity());
    drop(world.executor);
    // One executor writes a store at a time: the next is opened over the same
    // store, with the inventory of the build that has the new adapter.
    let store = world.store;
    let after = World::reopened(store, inventory_of(root, &second(), &revision("8")), "tiny");
    // A new request with the old preparation: stale, for the model, and no run.
    let stale = after
        .executor
        .request(&Request::new(World::call()).expecting(&prepared))
        .unwrap_err();
    assert_eq!(
        stale,
        CallError::Refused(Refusal::Stale {
            action: "cuts".to_owned(),
            why: Stale::Model {
                capability: Capability::Segment
            },
        })
    );
    assert!(after.ran().is_empty());
    assert_eq!(after.runs(), 1);
    // One under a new id, with no preparation: it runs, under the new installation.
    // Its action is handed that identity as its model's version, and no backend:
    // the backend of a run for a local model is the lease's (task I7, its second
    // half), and until then no request reaches a local model.
    let next = RequestId::parse("second-request").unwrap();
    let Ok(Answer::Ran(finished)) = after
        .executor
        .request(&Request::new(World::call()).with_id(next))
    else {
        panic!("the request under a new id did not run");
    };
    assert_ne!(finished.run(), &first_run);
    assert_eq!(after.ran(), [new.identity().to_string()]);
    assert_eq!(after.runs(), 2);
    // A repeat of the first request's id, as it was first sent: the first run, and
    // nothing runs again. It is not stale and it is not run on the new installation.
    let Ok(Answer::Repeat(earlier)) = after.executor.request(&first_request()) else {
        panic!("the repeat of the first request was not answered as a repeat");
    };
    assert_eq!(earlier.run(), &first_run);
    assert_eq!(after.ran(), [new.identity().to_string()]);
    assert_eq!(after.runs(), 2);
}
