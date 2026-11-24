//! `whizbang store import`: a canonical artifact file becomes an input in a store.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use image::ImageEncoder;
use whizbang_core::artifact::{
    self, Artifact, ArtifactType, BandProfile, ImageRole, Line, Location, Mask, Origin, Plan,
    Series, Shape, Source, Step, UseCase, Windows,
};
use whizbang_core::identity::{
    Binding, DecodeLimits, Digest, PixelDigest, Projection, Space, Topology, identify,
};
use whizbang_core::store::Reader;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::scratch::Scratch;

/// Runs `whizbang` and gives its exit code, what it printed and what it complained of.
fn whizbang(args: &[&str]) -> (i32, String, String) {
    let out = whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang"))
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
    // The command is kept until the run has ended or been killed: its temporary home
    // goes with it.
    let mut command = whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang"));
    let child = command
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

    // A second import of the same artifact within the hour publishes nothing: its mark
    // is that young (plan m6, task S1: an upload's age is from its last import, made
    // again once its mark is an hour old).
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
    let out = whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang"))
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
    // The whole store in its one file, as the format before buckets wrote it, so that a
    // name of it can be changed by hand (ADR 0047).
    let generation = whizbang_testkit::stored::whole(&store);
    let text = std::fs::read_to_string(&generation).expect("a generation");
    // The mask's name, and the mark the import left on it, which holds the same file.
    assert_eq!(text.matches("\"kind\": \"output\"").count(), 2);
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

// Pictures: `store import --picture`.

/// Imports a picture, making the store if there is none.
fn import_picture(file: &Path, store: &Path) -> (i32, String, String) {
    whizbang(&[
        "store",
        "import",
        "--picture",
        file.to_str().expect("a path of UTF-8"),
        "--create",
        "--store",
        store.to_str().expect("a path of UTF-8"),
    ])
}

/// The checksum of a PNG chunk: CRC-32 over its name and data.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A PNG chunk, whole: its length, its name, its data and its checksum.
fn chunk(name: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut covered = name.to_vec();
    covered.extend_from_slice(data);
    let mut whole = u32::try_from(data.len())
        .expect("a short chunk")
        .to_be_bytes()
        .to_vec();
    whole.extend_from_slice(&covered);
    whole.extend_from_slice(&crc32(&covered).to_be_bytes());
    whole
}

/// The names of a PNG's chunks, in order.
fn chunk_names(png: &[u8]) -> Vec<String> {
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"), "a PNG");
    let (mut at, mut names) = (8, Vec::new());
    while at < png.len() {
        let length = u32::from_be_bytes(png[at..at + 4].try_into().expect("four bytes"));
        names.push(String::from_utf8_lossy(&png[at + 4..at + 8]).into_owned());
        at += 12 + length as usize;
    }
    names
}

/// A picture of 6 by 4 pixels, no two alike, as red, green, blue and alpha.
fn rgba() -> Vec<u8> {
    (0..24u8)
        .flat_map(|n| [n * 10, 255 - n * 10, n * 3, 255])
        .collect()
}

/// The picture as a PNG.
fn a_png() -> Vec<u8> {
    let mut file = Vec::new();
    image::codecs::png::PngEncoder::new(&mut file)
        .write_image(&rgba(), 6, 4, image::ExtendedColorType::Rgba8)
        .expect("a PNG");
    file
}

/// The picture as a JPEG, without its alpha.
fn a_jpeg() -> Vec<u8> {
    let rgb: Vec<u8> = rgba()
        .chunks(4)
        .flat_map(|pixel| pixel[..3].to_vec())
        .collect();
    let mut file = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, 90)
        .write_image(&rgb, 6, 4, image::ExtendedColorType::Rgb8)
        .expect("a JPEG");
    file
}

/// What a camera or an editor writes beside the pixels, which must not be kept.
const SECRET: &[u8] = b"taken at 51.5007N 0.1246W by serial 0042";

#[test]
fn a_png_and_a_jpeg_are_committed_as_pictures_and_their_identities_printed() {
    let dir = Scratch::new("import-picture");
    let store = dir.path().join("store");
    let mut seen = Vec::new();
    for (n, (name, bytes)) in [("photo.png", a_png()), ("photo.jpg", a_jpeg())]
        .into_iter()
        .enumerate()
    {
        let file = dir.path().join(name);
        std::fs::write(&file, &bytes).expect("the fixture is written");
        let given = identify(&bytes, &DecodeLimits::default()).expect("the fixture decodes");

        let (code, printed, complaint) = import_picture(&file, &store);
        assert_eq!((code, complaint.as_str()), (0, ""), "{printed}");
        let made = if n == 0 {
            created(&store)
        } else {
            String::new()
        };
        let id = printed
            .strip_prefix(&format!("{made}imported image "))
            .and_then(|rest| rest.strip_suffix('\n'))
            .unwrap_or_else(|| panic!("an identity is printed: {printed:?}"));
        let id = artifact::ArtifactId(id.parse().expect("an artifact's identity"));

        let Artifact::Image(image) = read_back(&store, ArtifactType::Image, id) else {
            panic!("an image");
        };
        assert_eq!(image.id(), id);
        assert_eq!(image.role(), ImageRole::Picture);
        // The same picture: what the file shows, pixel for pixel, in the file's size.
        assert_eq!(image.identity().pixels, given.identity.pixels);
        assert_eq!((image.space().width(), image.space().height()), (6, 4));
        seen.push(id);

        // A second import of the same picture publishes nothing.
        let (code, printed, _) = import_picture(&file, &store);
        assert_eq!(code, 0);
        assert_eq!(printed, format!("already there: image {id}\n"));
    }
    // A PNG and a JPEG show different pixels, so they are two pictures.
    assert_ne!(seen[0], seen[1]);
}

#[test]
fn a_picture_over_the_decode_limits_is_refused_before_it_is_committed() {
    let dir = Scratch::new("import-picture");
    let (file, store) = (dir.path().join("huge.png"), dir.path().join("store"));
    // A PNG that says it is 10,000 by 10,000 pixels: twice the core's limit of 50,000,000.
    // Its data is a few bytes; what is refused is what its header says.
    let mut header = Vec::new();
    header.extend_from_slice(&10_000u32.to_be_bytes());
    header.extend_from_slice(&10_000u32.to_be_bytes());
    header.extend_from_slice(&[8, 0, 0, 0, 0]);
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend(chunk(b"IHDR", &header));
    png.extend(chunk(
        b"IDAT",
        &[0x78, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01],
    ));
    png.extend(chunk(b"IEND", &[]));
    std::fs::write(&file, &png).expect("the fixture is written");

    let (code, printed, complaint) = import_picture(&file, &store);
    assert_ne!(code, 0);
    assert_eq!(printed, "");
    assert!(complaint.contains("limit"), "{complaint}");
    // Refused before the store was touched: even `--create` made nothing.
    assert!(!store.exists(), "a store was made for a refused picture");

    // A picture file is not taken for an artifact file, nor an artifact file for a
    // picture.
    let photo = dir.path().join("photo.png");
    std::fs::write(&photo, a_png()).expect("the fixture is written");
    let (code, _, complaint) = import(&photo, &store);
    assert_ne!(code, 0);
    assert!(complaint.contains("photo.png"), "{complaint}");
    let artifact = dir.path().join("seam.mask");
    std::fs::write(&artifact, mask().to_bytes()).expect("the fixture is written");
    let (code, _, complaint) = import_picture(&artifact, &store);
    assert_ne!(code, 0);
    assert!(!complaint.is_empty());
    assert!(!store.exists());
}

#[test]
fn the_committed_picture_holds_no_metadata_of_the_file() {
    let dir = Scratch::new("import-picture");
    let store = dir.path().join("store");

    // A PNG with a text chunk and a private chunk after its header.
    let plain = a_png();
    let mut png = plain[..33].to_vec();
    png.extend(chunk(b"tEXt", &[b"Comment\0".as_slice(), SECRET].concat()));
    png.extend(chunk(b"prVt", SECRET));
    png.extend_from_slice(&plain[33..]);
    // A JPEG with a comment segment after its start.
    let plain = a_jpeg();
    let mut jpeg = plain[..2].to_vec();
    jpeg.extend_from_slice(&[0xff, 0xfe]);
    jpeg.extend_from_slice(
        &u16::try_from(SECRET.len() + 2)
            .expect("short")
            .to_be_bytes(),
    );
    jpeg.extend_from_slice(SECRET);
    jpeg.extend_from_slice(&plain[2..]);

    for (name, bytes) in [("noted.png", png), ("noted.jpg", jpeg)] {
        let file = dir.path().join(name);
        std::fs::write(&file, &bytes).expect("the fixture is written");
        let (code, printed, complaint) = import_picture(&file, &store);
        assert_eq!(code, 0, "{complaint}");
        let id = printed
            .lines()
            .last()
            .and_then(|line| line.strip_prefix("imported image "))
            .unwrap_or_else(|| panic!("an identity is printed: {printed:?}"));
        let id = artifact::ArtifactId(id.parse().expect("an artifact's identity"));
        let Artifact::Image(image) = read_back(&store, ArtifactType::Image, id) else {
            panic!("an image");
        };

        // The picture held is a PNG of the pixels alone: a header, the data, the end.
        let names = chunk_names(image.file());
        assert_eq!(names.first().map(String::as_str), Some("IHDR"), "{names:?}");
        assert_eq!(names.last().map(String::as_str), Some("IEND"), "{names:?}");
        assert!(
            names[1..names.len() - 1].iter().all(|name| name == "IDAT"),
            "{name}: {names:?}"
        );
        // Nothing of what the file said beside its pixels is anywhere in the artifact,
        // nor its name.
        let whole = image.to_bytes();
        for kept in [SECRET, name.as_bytes()] {
            assert!(
                !whole.windows(kept.len()).any(|at| at == kept),
                "{name}: the artifact holds {:?}",
                String::from_utf8_lossy(kept)
            );
        }
        // The file's bytes are not the picture's: the file was not kept.
        assert_ne!(
            image.identity().file,
            identify(&bytes, &DecodeLimits::default())
                .expect("decodes")
                .identity
                .file
        );
    }
}

#[test]
fn a_file_over_the_limit_its_contents_allow_is_refused() {
    // A mask drawn on a picture holds no more pixels than its image: a file whose header
    // claims more is refused by that header, before its pixels are decoded (here its
    // pixels could not be: they are a few bytes of nothing). A canonical file is held to
    // the 65 MiB the largest image's file allows, by its size, unread
    // (`import_refuses_a_file_over_its_limit_by_its_size_and_reads_one_at_the_limit`).
    let dir = Scratch::new("import-mask-limit");
    let store = dir.path().join("store");
    let file = dir.path().join("base.png");
    image::RgbImage::from_pixel(8, 6, image::Rgb([90, 90, 90]))
        .save(&file)
        .unwrap();
    let (code, out, err) = import_picture(&file, &store);
    assert_eq!(code, 0, "{err}");
    let image = out
        .lines()
        .last()
        .unwrap()
        .strip_prefix("imported image ")
        .unwrap()
        .to_owned();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&100u32.to_be_bytes());
    ihdr.extend_from_slice(&100u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 0, 0, 0, 0]);
    let mut claims = b"\x89PNG\r\n\x1a\n".to_vec();
    claims.extend(chunk(b"IHDR", &ihdr));
    claims.extend(chunk(b"IDAT", &[0; 4]));
    claims.extend(chunk(b"IEND", &[]));
    let big = dir.path().join("claims.png");
    std::fs::write(&big, claims).unwrap();
    let before = std::fs::read_dir(store.join("objects"))
        .map(|d| d.count())
        .ok();
    let (code, _, err) = whizbang(&[
        "store",
        "import",
        "--mask",
        big.to_str().unwrap(),
        "--onto",
        &image,
        "--store",
        store.to_str().unwrap(),
    ]);
    assert_eq!(code, 1, "{err}");
    assert!(
        err.contains("100x100 is 10000 pixels, over the limit of 48"),
        "{err}"
    );
    let after = std::fs::read_dir(store.join("objects"))
        .map(|d| d.count())
        .ok();
    assert_eq!(before, after);
}

// A caller's picture (plan m3, task O5).

#[test]
fn a_picture_is_read_through_no_link_and_nothing_but_a_regular_file_is_opened() {
    let dir = Scratch::new("import-picture-links");
    let store = dir.path().join("store");
    let path = |name: &str| dir.path().join(name);
    std::fs::write(path("photo.png"), a_png()).expect("the fixture is written");
    let refused = |what: &str, file: &Path, says: &str| {
        let args = [
            "store",
            "import",
            "--picture",
            file.to_str().expect("a path of UTF-8"),
            "--create",
            "--store",
            store.to_str().expect("a path of UTF-8"),
        ];
        let (code, printed, complaint) = whizbang_within(&args, 20);
        assert_eq!((code, printed.as_str()), (1, ""), "{what}: {complaint}");
        assert!(complaint.contains(says), "{what}: {complaint}");
        assert!(!store.exists(), "{what}: a store was created");
    };
    // A link, to a picture or to anything else, is not followed.
    std::os::unix::fs::symlink(path("photo.png"), path("to-photo")).expect("a link");
    refused("a link to a picture", &path("to-photo"), "is a link");
    std::fs::create_dir(path("folder")).expect("a directory");
    refused("a directory", &path("folder"), "is a directory");
    let fifo = path("fifo");
    if Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .is_ok_and(|s| s.success())
    {
        refused("a FIFO", &fifo, "is a FIFO");
        std::os::unix::fs::symlink(&fifo, path("to-fifo")).expect("a link");
        refused("a link to a FIFO", &path("to-fifo"), "is a link");
    } else {
        eprintln!("skip: no FIFO could be made here (mkfifo)");
    }
    if Path::new("/dev/zero").exists() {
        refused("a device", Path::new("/dev/zero"), "is a device");
    }
    // The file itself is read, through the folders the caller named.
    let (code, printed, complaint) = import_picture(&path("photo.png"), &store);
    assert_eq!(code, 0, "{complaint}");
    assert!(printed.contains("imported image sha256:"), "{printed}");
}

#[test]
fn a_picture_is_a_panorama_when_its_caller_or_its_file_says_so_and_flat_otherwise() {
    let dir = Scratch::new("import-picture-projection");
    let store = dir.path().join("store");
    // A picture of 8 by 4: two to one.
    let rgba: Vec<u8> = (0..32u8)
        .flat_map(|n| [n * 7, 255 - n * 5, n, 255])
        .collect();
    let mut wide = Vec::new();
    image::codecs::png::PngEncoder::new(&mut wide)
        .write_image(&rgba, 8, 4, image::ExtendedColorType::Rgba8)
        .expect("a PNG");
    let file = dir.path().join("wide.png");
    std::fs::write(&file, &wide).expect("the fixture is written");
    let id = |printed: &str| printed.split_whitespace().last().unwrap_or("").to_owned();
    let space = |id: &str| {
        let reader = Reader::open(&store, Arc::new(RealFs)).expect("a reader");
        let id = artifact::ArtifactId(id.parse().expect("an identity"));
        match artifact::get(&reader, ArtifactType::Image, id, &DecodeLimits::default()) {
            Ok(Artifact::Image(image)) => image.space(),
            other => panic!("{other:?}"),
        }
    };
    // Said nothing: flat, with a word that it is two to one.
    let (code, printed, complaint) = import_picture(&file, &store);
    assert_eq!(code, 0, "{complaint}");
    assert!(
        printed.contains("is two to one and says no projection"),
        "{printed}"
    );
    assert_eq!(space(&id(&printed)).projection(), Projection::Flat);
    // Said by the caller: a complete panorama, which wraps.
    let (code, printed, complaint) = whizbang(&[
        "store",
        "import",
        "--picture",
        file.to_str().expect("a path of UTF-8"),
        "--projection",
        "equirectangular",
        "--store",
        store.to_str().expect("a path of UTF-8"),
    ]);
    assert_eq!(code, 0, "{complaint}");
    let made = space(&id(&printed));
    assert_eq!(
        (made.projection(), made.topology()),
        (Projection::Equirectangular, Topology::Wrap)
    );
    // Said by the file's XMP, in a text chunk: the same, and the chunk is not kept.
    let xmp = b"XML:com.adobe.xmp\0\0\0\0\0<x:xmpmeta GPano:ProjectionType=\"equirectangular\"/>";
    let mut tagged = wide[..33].to_vec();
    tagged.extend_from_slice(&chunk(b"iTXt", xmp));
    tagged.extend_from_slice(&wide[33..]);
    let file = dir.path().join("tagged.png");
    std::fs::write(&file, &tagged).expect("the fixture is written");
    let (code, printed, complaint) = import_picture(&file, &store);
    assert_eq!(code, 0, "{complaint}");
    assert!(!printed.contains("two to one"), "{printed}");
    assert!(whizbang::import::says_equirectangular(&tagged));
    assert!(!whizbang::import::says_equirectangular(&wide));
}

#[test]
fn one_file_imported_flat_and_as_a_panorama_is_two_pictures_named_apart() {
    // The review of task O5: a second import of a file under another projection makes
    // another image, and its name of where it came from is its own; the flat one is
    // still what `pictures_by_file` finds. A refusal of a picture that is not a regular
    // file does not speak of links, which are refused.
    let dir = Scratch::new("import-picture-twice");
    let store = dir.path().join("store");
    let file = dir.path().join("photo.png");
    std::fs::write(&file, a_png()).expect("the fixture is written");
    let as_given = |projection: &str| {
        let (code, printed, complaint) = whizbang(&[
            "store",
            "import",
            "--create",
            "--picture",
            file.to_str().expect("UTF-8"),
            "--projection",
            projection,
            "--store",
            store.to_str().expect("UTF-8"),
        ]);
        assert_eq!(code, 0, "{complaint}");
        printed.split_whitespace().last().unwrap_or("").to_owned()
    };
    let flat = as_given("flat");
    let panorama = as_given("equirectangular");
    assert_ne!(flat, panorama);
    let reader = Reader::open(&store, Arc::new(RealFs)).expect("a reader");
    let generation = reader.snapshot().expect("a generation");
    let hex = Digest::of_bytes(&a_png()).hex();
    for (prefix, id) in [
        ("imports/picture/", &flat),
        ("imports/panorama/", &panorama),
    ] {
        let entry = generation.get(&format!("{prefix}{hex}")).expect("named");
        assert_eq!(reader.read(entry).expect("read"), id.as_bytes(), "{prefix}");
    }
    let found = whizbang::import::pictures_by_file(&store).expect("the pictures");
    assert_eq!(
        found.values().map(ToString::to_string).collect::<Vec<_>>(),
        [flat]
    );
    let (code, _, complaint) = import_picture(dir.path(), &store);
    assert_eq!(code, 1);
    assert!(
        complaint
            .contains("is a directory: a picture is read from a regular file, through no link"),
        "{complaint}"
    );
}
