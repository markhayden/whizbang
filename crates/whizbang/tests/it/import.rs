//! `whizbang store import`: a canonical artifact file becomes an input in a store.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use whizbang_core::artifact::{
    self, Artifact, ArtifactType, BandProfile, Line, Location, Mask, Origin, Plan, Series, Shape,
    Source, Step, UseCase, Windows,
};
use whizbang_core::identity::{Binding, DecodeLimits, Digest, PixelDigest, Space};
use whizbang_core::store::Reader;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::scratch::Scratch;

/// Runs `whizbang` and gives its exit code, what it printed and what it complained of.
fn whizbang(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_whizbang"))
        .args(args)
        .output()
        .expect("the binary runs");
    (
        out.status.code().expect("the binary exits"),
        String::from_utf8(out.stdout).expect("output is UTF-8"),
        String::from_utf8(out.stderr).expect("errors are UTF-8"),
    )
}

/// Imports into a store that must be there.
fn import(file: &Path, store: &Path) -> (i32, String, String) {
    whizbang(&[
        "store",
        "import",
        file.to_str().expect("a path of UTF-8"),
        "--store",
        store.to_str().expect("a path of UTF-8"),
    ])
}

/// Imports, making the store if there is none.
fn import_creating(file: &Path, store: &Path) -> (i32, String, String) {
    whizbang(&[
        "store",
        "import",
        file.to_str().expect("a path of UTF-8"),
        "--create",
        "--store",
        store.to_str().expect("a path of UTF-8"),
    ])
}

fn created(store: &Path) -> String {
    format!("created a store at {}\n", store.display())
}

fn made_by(action: &str) -> Origin {
    Origin::new(Step::new(action, "1")).expect("a step with an action")
}

fn photo() -> Binding {
    Binding::root(
        PixelDigest(Digest::of_bytes(b"photo")),
        Space::flat(8, 6).expect("a space"),
    )
}

/// A mask of an 8 by 6 image with a band, a feather and a tone ramp.
fn mask() -> Mask {
    let line = Location::new(
        photo(),
        Shape::Line {
            line: Line::Column(4.0),
            width: 3.0,
        },
        made_by("line"),
    )
    .expect("a line");
    let mut core = vec![0; 48];
    for row in 0..6 {
        core[row * 8 + 4] = 255;
    }
    let mut mask = Mask::from_core(
        photo(),
        UseCase::Seam,
        core,
        Source {
            locations: vec![line.id()],
            instances: Vec::new(),
        },
        made_by("fixture"),
    )
    .expect("a mask");
    let band = BandProfile::new(
        photo(),
        line.id(),
        4,
        1.0,
        vec![1.5; 6],
        Windows {
            gap: 1,
            step_columns: 3,
            texture_columns: 8,
        },
        "fan-v1",
        made_by("fan"),
    )
    .expect("a band");
    let sigma = Series::new(
        photo(),
        "sigma_px",
        "px",
        1,
        vec![0.5; 6],
        made_by("feather"),
    )
    .expect("a series");
    mask.set_profile(band, sigma).expect("a profile");
    let step = Series::new(
        photo(),
        "tone_ramp.step",
        "levels",
        3,
        vec![2.0; 18],
        made_by("ramp"),
    )
    .expect("a series");
    mask.push_tone_ramp(step, 0.5).expect("a tone ramp");
    mask
}

fn read_back(store: &Path, kind: ArtifactType, id: artifact::ArtifactId) -> Artifact {
    let reader = Reader::open(store, Arc::new(RealFs)).expect("a store");
    artifact::get(&reader, kind, id, &DecodeLimits::default()).expect("the artifact")
}

#[test]
fn a_canonical_mask_file_is_imported_and_reads_back_as_the_same_mask() {
    let dir = Scratch::new("import");
    let (file, store) = (dir.path().join("seam.mask"), dir.path().join("store"));
    let mask = mask();
    std::fs::write(&file, mask.to_bytes()).expect("the fixture is written");

    let (code, printed, complaint) = import_creating(&file, &store);
    assert_eq!((code, complaint.as_str()), (0, ""), "{printed}");
    assert_eq!(
        printed,
        format!("{}imported mask {}\n", created(&store), mask.id())
    );
    let read = read_back(&store, ArtifactType::Mask, mask.id());
    assert_eq!(read, Artifact::from(mask.clone()));
    assert_eq!(read.origin(), mask.origin());

    // A second import of the same artifact publishes nothing.
    let generation = |store: &Path| {
        Reader::open(store, Arc::new(RealFs))
            .expect("a store")
            .snapshot()
            .expect("a generation")
            .number()
    };
    assert_eq!(generation(&store), 1);
    let (code, printed, _) = import(&file, &store);
    assert_eq!(code, 0);
    assert_eq!(printed, format!("already there: mask {}\n", mask.id()));
    assert_eq!(generation(&store), 1);

    // The same mask imported into a second store has the same identity there, and the
    // two stores hold the same bytes.
    let second = dir.path().join("second");
    let (code, printed, _) = import_creating(&file, &second);
    assert_eq!(code, 0);
    assert_eq!(
        printed,
        format!("{}imported mask {}\n", created(&second), mask.id())
    );
    assert_eq!(read_back(&second, ArtifactType::Mask, mask.id()), read);
}

#[test]
fn an_artifact_of_any_type_is_imported_under_its_own_type() {
    let dir = Scratch::new("import");
    let store = dir.path().join("store");
    let panorama = Binding::root(
        PixelDigest(Digest::of_bytes(b"panorama")),
        Space::panorama(400, 200).expect("a space"),
    );
    let artifacts: Vec<Artifact> = vec![
        Location::new(photo(), Shape::WholeImage, made_by("find"))
            .expect("a location")
            .into(),
        Series::new(
            photo(),
            "sigma_px",
            "px",
            1,
            vec![0.5; 6],
            made_by("feather"),
        )
        .expect("a series")
        .into(),
        Plan::new(panorama, 40, made_by("cut"))
            .expect("a plan")
            .into(),
    ];
    for (n, artifact) in artifacts.iter().enumerate() {
        let file = dir.path().join(format!("{n}.artifact"));
        std::fs::write(&file, artifact.to_bytes()).expect("the fixture is written");
        // The first makes the store, and says so; with the store there, `--create`
        // changes nothing.
        let (code, printed, complaint) = import_creating(&file, &store);
        assert_eq!(code, 0, "{complaint}");
        let (kind, id) = (artifact.artifact_type(), artifact.id());
        let made = if n == 0 {
            created(&store)
        } else {
            String::new()
        };
        assert_eq!(printed, format!("{made}imported {kind} {id}\n"));
        assert_eq!(read_back(&store, kind, id), *artifact);
    }
}

#[test]
fn a_file_that_is_no_canonical_artifact_is_refused_and_nothing_is_created() {
    let dir = Scratch::new("import");
    let store = dir.path().join("store");
    let mask = mask().to_bytes();
    let mut cut = mask.clone();
    cut.truncate(mask.len() - 1);
    let mut longer = mask.clone();
    longer.push(0);
    for (what, bytes, says) in [
        (
            "a PNG",
            b"\x89PNG\r\n\x1a\n and the rest".to_vec(),
            "a PNG image",
        ),
        (
            "a text",
            b"core.png".to_vec(),
            "not a canonical artifact file",
        ),
        ("a mask cut short", cut, "not a canonical artifact file"),
        ("a mask and a byte more", longer, "follow the artifact"),
    ] {
        let file = dir.path().join("given");
        std::fs::write(&file, bytes).expect("the fixture is written");
        let (code, printed, complaint) = import(&file, &store);
        assert_eq!((code, printed.as_str()), (1, ""), "{what}: {complaint}");
        assert!(complaint.contains(says), "{what}: {complaint}");
        assert!(complaint.contains("given"), "{what}: {complaint}");
        assert!(!store.exists(), "{what}: a store was created");
    }
    // A file that is not there, and a command that does not say which store.
    let (code, _, complaint) = import(&dir.path().join("absent"), &store);
    assert_eq!(code, 1);
    assert!(complaint.contains("cannot read"), "{complaint}");
    assert!(!store.exists());
    let (code, _, complaint) = whizbang(&["store", "import", "anything"]);
    assert_eq!(code, 2);
    assert!(complaint.contains("--store"), "{complaint}");
}

#[test]
fn import_makes_a_store_only_when_told_to_and_says_when_it_has() {
    let dir = Scratch::new("import");
    let file = dir.path().join("seam.mask");
    let mask = mask();
    std::fs::write(&file, mask.to_bytes()).expect("the fixture is written");

    // Nothing there, and an empty directory: refused, like every store command, with
    // nothing made; the complaint says how to have a store made.
    let missing = dir.path().join("missing");
    let (code, printed, complaint) = import(&file, &missing);
    assert_eq!((code, printed.as_str()), (1, ""), "{complaint}");
    assert!(complaint.contains("there is no store at"), "{complaint}");
    assert!(complaint.contains("--create"), "{complaint}");
    assert!(!missing.exists());
    let empty = dir.path().join("empty");
    std::fs::create_dir(&empty).expect("a directory");
    let (code, printed, complaint) = import(&file, &empty);
    assert_eq!((code, printed.as_str()), (1, ""), "{complaint}");
    assert!(complaint.contains("is not a store"), "{complaint}");
    assert!(complaint.contains("--create"), "{complaint}");
    assert_eq!(std::fs::read_dir(&empty).expect("a directory").count(), 0);

    // Told to, it makes one in either place, and says so.
    for store in [&missing, &empty] {
        let (code, printed, complaint) = import_creating(&file, store);
        assert_eq!(code, 0, "{complaint}");
        assert_eq!(
            printed,
            format!("{}imported mask {}\n", created(store), mask.id())
        );
        // From then on it is a store, with or without `--create`.
        let (code, printed, _) = import(&file, store);
        assert_eq!(code, 0);
        assert_eq!(printed, format!("already there: mask {}\n", mask.id()));
    }

    // A directory that holds something else is no place for a store, told to or not.
    let other = dir.path().join("other");
    std::fs::create_dir(&other).expect("a directory");
    std::fs::write(other.join("notes.txt"), b"mine").expect("a file");
    for run in [import, import_creating] {
        let (code, _, complaint) = run(&file, &other);
        assert_eq!(code, 1);
        assert!(complaint.contains("notes.txt"), "{complaint}");
        assert!(!complaint.contains("--create"), "{complaint}");
        assert_eq!(std::fs::read_dir(&other).expect("a directory").count(), 1);
    }
}

#[test]
fn import_is_a_writer_and_is_refused_while_another_process_holds_the_store() {
    let dir = Scratch::new("import");
    let (file, root) = (dir.path().join("seam.mask"), dir.path().join("store"));
    std::fs::write(&file, mask().to_bytes()).expect("the fixture is written");
    let holder = whizbang_core::store::Store::open(&root, Arc::new(RealFs)).expect("a store");
    for run in [import, import_creating] {
        let (code, printed, complaint) = run(&file, &root);
        assert_eq!((code, printed.as_str()), (1, ""), "{complaint}");
        assert!(
            complaint.contains(&format!("process {} (", std::process::id())),
            "{complaint}"
        );
        assert!(complaint.contains("one process at a time"), "{complaint}");
    }
    drop(holder);
    let (code, printed, _) = import(&file, &root);
    assert_eq!(code, 0, "{printed}");
}
