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

/// Runs `whizbang` and waits for its exit, for so long and no longer: a run that has not
/// ended by then is killed and the test fails. For what must not hang.
fn whizbang_within(args: &[&str], seconds: u64) -> (i32, String, String) {
    let child = Command::new(env!("CARGO_BIN_EXE_whizbang"))
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("the binary runs");
    let id = child.id();
    let (ended, has_ended) = std::sync::mpsc::channel();
    whizbang_testkit::threads::spawn(move || {
        let _ = ended.send(child.wait_with_output());
    });
    let Ok(out) = has_ended.recv_timeout(std::time::Duration::from_secs(seconds)) else {
        let _ = Command::new("kill").args(["-9", &id.to_string()]).status();
        panic!("whizbang {args:?} had not ended after {seconds} seconds");
    };
    let out = out.expect("the binary ran");
    (
        out.status.code().expect("the binary exits"),
        String::from_utf8(out.stdout).expect("output is UTF-8"),
        String::from_utf8(out.stderr).expect("errors are UTF-8"),
    )
}

/// The most bytes a file to import may have: what the command's help says.
const LIMIT: u64 = 65 * 1024 * 1024;

/// A file of this many zero bytes that takes no room: nothing is written to it.
fn sparse(path: &Path, bytes: u64) {
    let file = std::fs::File::create(path).expect("a file");
    file.set_len(bytes).expect("a length");
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

#[test]
fn import_reads_a_regular_file_or_a_link_to_one_and_refuses_anything_else_unread() {
    let dir = Scratch::new("import");
    let store = dir.path().join("store");
    let path = |name: &str| dir.path().join(name);
    let refused = |what: &str, file: &Path, says: &str| {
        // Bounded: what is refused is refused without being opened, so a path that
        // would never finish reading, or never even open, comes back at once.
        let args = [
            "store",
            "import",
            file.to_str().expect("a path of UTF-8"),
            "--create",
            "--store",
            store.to_str().expect("a path of UTF-8"),
        ];
        let (code, printed, complaint) = whizbang_within(&args, 20);
        assert_eq!((code, printed.as_str()), (1, ""), "{what}: {complaint}");
        assert!(complaint.contains(says), "{what}: {complaint}");
        assert!(complaint.contains("a regular file"), "{what}: {complaint}");
        assert!(!store.exists(), "{what}: a store was created");
    };

    std::fs::create_dir(path("folder")).expect("a directory");
    refused("a directory", &path("folder"), "is a directory");

    #[cfg(unix)]
    {
        // A link is followed: to a directory it is a directory.
        std::os::unix::fs::symlink(path("folder"), path("to-folder")).expect("a link");
        refused(
            "a link to a directory",
            &path("to-folder"),
            "is a directory",
        );

        // A FIFO nobody writes to: opening it to read would wait for ever.
        let fifo = path("fifo");
        let made = Command::new("mkfifo").arg(&fifo).status();
        if made.is_ok_and(|status| status.success()) {
            refused("a FIFO", &fifo, "is a FIFO");
            std::os::unix::fs::symlink(&fifo, path("to-fifo")).expect("a link");
            refused("a link to a FIFO", &path("to-fifo"), "is a FIFO");
        } else {
            eprintln!("skip: no FIFO could be made here (mkfifo), so none was imported");
        }

        // A socket, which cannot be opened as a file at all.
        match std::os::unix::net::UnixListener::bind(path("socket")) {
            Ok(_listener) => refused("a socket", &path("socket"), "is a socket"),
            Err(error) => eprintln!("skip: no socket could be made here ({error})"),
        }

        // A device that never ends, and is open to everyone.
        let zero = Path::new("/dev/zero");
        if zero.exists() {
            refused("a device", zero, "is a device");
        } else {
            eprintln!("skip: there is no /dev/zero here, so no device was imported");
        }

        // A link to an artifact file is the file.
        let mask = mask();
        std::fs::write(path("seam.mask"), mask.to_bytes()).expect("the fixture is written");
        std::os::unix::fs::symlink(path("seam.mask"), path("to-mask")).expect("a link");
        let (code, printed, complaint) = import_creating(&path("to-mask"), &store);
        assert_eq!(code, 0, "{complaint}");
        assert_eq!(
            printed,
            format!("{}imported mask {}\n", created(&store), mask.id())
        );
    }
}

#[test]
fn import_refuses_a_file_over_its_limit_by_its_size_and_reads_one_at_the_limit() {
    let dir = Scratch::new("import");
    let store = dir.path().join("store");

    // One byte over: refused by its size. The file is sparse, and so is never read:
    // nothing here takes the room it claims.
    let over = dir.path().join("over");
    sparse(&over, LIMIT + 1);
    let (code, printed, complaint) = import_creating(&over, &store);
    assert_eq!((code, printed.as_str()), (1, ""), "{complaint}");
    assert!(
        complaint.contains(&format!("is {} bytes", LIMIT + 1)),
        "{complaint}"
    );
    assert!(complaint.contains("65 MiB"), "{complaint}");
    assert!(complaint.contains("over"), "{complaint}");
    assert!(!store.exists());

    // Exactly at the limit: read whole, which costs the limit and no more, and refused
    // for what it holds.
    let at = dir.path().join("at");
    sparse(&at, LIMIT);
    let (code, printed, complaint) = import_creating(&at, &store);
    assert_eq!((code, printed.as_str()), (1, ""), "{complaint}");
    assert!(
        complaint.contains("not a canonical artifact file"),
        "{complaint}"
    );
    assert!(!store.exists());
}

#[test]
fn the_help_of_import_says_its_limit_and_what_create_makes() {
    let (code, printed, _) = whizbang(&["store", "import", "--help"]);
    assert_eq!(code, 0);
    // One line of words, however the help was wrapped.
    let words = printed.split_whitespace().collect::<Vec<_>>().join(" ");
    for says in [
        "at most 65 MiB",
        "a regular file, or a link to one",
        "Only the last part of the path is made",
        "an unfinished creation",
        "is completed",
    ] {
        assert!(words.contains(says), "{says:?} is not in: {words}");
    }
}

#[test]
fn create_makes_the_last_part_of_the_path_only_and_completes_an_unfinished_creation() {
    let dir = Scratch::new("import");
    let file = dir.path().join("seam.mask");
    let mask = mask();
    std::fs::write(&file, mask.to_bytes()).expect("the fixture is written");

    // A mistyped part in the middle of the path: refused, as `mkdir` without `-p`
    // refuses, and nothing is made.
    let mistyped = dir.path().join("stroes");
    let store = mistyped.join("mine");
    let (code, printed, complaint) = import_creating(&file, &store);
    assert_eq!((code, printed.as_str()), (1, ""), "{complaint}");
    assert!(
        complaint.contains(&format!("there is no directory {}", mistyped.display())),
        "{complaint}"
    );
    assert!(!mistyped.exists());

    // What a creation that did not finish leaves: the lock file and empty folders. It
    // is completed, and that is a store created.
    let unfinished = dir.path().join("unfinished");
    std::fs::create_dir_all(unfinished.join("staging")).expect("a directory");
    std::fs::create_dir(unfinished.join("objects")).expect("a directory");
    std::fs::write(unfinished.join("LOCK"), b"").expect("a lock file");
    let (code, _, complaint) = import(&file, &unfinished);
    assert_eq!(code, 1);
    assert!(complaint.contains("--create"), "{complaint}");
    let (code, printed, complaint) = import_creating(&file, &unfinished);
    assert_eq!(code, 0, "{complaint}");
    assert_eq!(
        printed,
        format!("{}imported mask {}\n", created(&unfinished), mask.id())
    );
    assert_eq!(
        read_back(&unfinished, ArtifactType::Mask, mask.id()),
        Artifact::from(mask)
    );
}

#[test]
fn create_makes_a_store_named_by_one_part_alone_in_the_current_directory() {
    // `--store mine`: the path has no parent written in it, and the directory that
    // holds it is the one the command runs in.
    let dir = Scratch::new("import");
    let mask = mask();
    std::fs::write(dir.path().join("seam.mask"), mask.to_bytes()).expect("the fixture");
    let out = Command::new(env!("CARGO_BIN_EXE_whizbang"))
        .args([
            "store",
            "import",
            "seam.mask",
            "--create",
            "--store",
            "mine",
        ])
        .current_dir(dir.path())
        .output()
        .expect("the binary runs");
    let complaint = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{complaint}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("created a store at mine\nimported mask {}\n", mask.id())
    );
    assert_eq!(
        read_back(&dir.path().join("mine"), ArtifactType::Mask, mask.id()),
        Artifact::from(mask)
    );
}

#[test]
fn a_diagnostic_under_the_artifacts_name_is_not_the_artifact_and_import_brings_it_in() {
    let dir = Scratch::new("import");
    let (file, store) = (dir.path().join("seam.mask"), dir.path().join("store"));
    let mask = mask();
    std::fs::write(&file, mask.to_bytes()).expect("the fixture is written");
    let (code, _, complaint) = import_creating(&file, &store);
    assert_eq!(code, 0, "{complaint}");

    // By hand: the reference under the mask's name is made a diagnostic. No reader is
    // given the mask from it, so it is not "already there".
    let generation = store.join("generations/1.json");
    let text = std::fs::read_to_string(&generation).expect("a generation");
    assert_eq!(text.matches("\"kind\": \"output\"").count(), 1);
    std::fs::write(
        &generation,
        text.replace("\"kind\": \"output\"", "\"kind\": \"diagnostic\""),
    )
    .expect("a generation");
    let (code, printed, complaint) = import(&file, &store);
    assert_eq!(code, 0, "{complaint}");
    assert_eq!(printed, format!("imported mask {}\n", mask.id()));
    assert_eq!(
        read_back(&store, ArtifactType::Mask, mask.id()),
        Artifact::from(mask)
    );
}
