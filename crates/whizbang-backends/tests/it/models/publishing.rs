//! Publishing an installation (ADR 0017, "Publishing", "The pointer", "The status
//! record"): staged files are read back from the disk, become an installation under
//! its identity, and the pointer is replaced under the directory's lock. Every
//! download is the test kit's fake; the disk is the test kit's shim over a scratch
//! folder, and one shim stands for one process.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use whizbang_backends::models::{
    CatalogueFile, Current, Damage, Entry, FileRefused, InstallRefused, InstallationRecord,
    ModelsDir, Revision, State, current, install, stage, status, verify,
};
use whizbang_core::capability::{Fault, FaultKind};
use whizbang_core::contract::{Cancellation, Secrets};
use whizbang_core::store::fs::{Fs, Held, PIECE_MAX_BYTES, RealFs};
use whizbang_testkit::downloads::FakeDownloader;
use whizbang_testkit::fs_shim::{Event, Lost, ShimFs, Tail};
use whizbang_testkit::scratch::Scratch;

use super::staging::{
    TERMS, at, graph, incoming, names_under, on_disk, serving_both, touches_only,
};
use super::{MARKER, file_name, hash, live_remover, model, none, parts, revision, secrets};

/// The entry `tiny` at this revision of the catalogue: the same two files under
/// each, so two revisions are two installations of the same bytes.
pub(super) fn entry_at(catalogue: &str) -> Entry {
    entry_of(
        catalogue,
        &[("graph.onnx", graph()), ("LICENSE.txt", TERMS.to_vec())],
    )
}

/// The entry `tiny` at this revision, of these files.
fn entry_of(catalogue: &str, files: &[(&str, Vec<u8>)]) -> Entry {
    let mut parts = parts("tiny");
    parts.revision = revision(catalogue);
    parts.files = files
        .iter()
        .map(|(name, content)| {
            let size = content.len() as u64;
            CatalogueFile::new(file_name(name), size, hash(content), at(name), &none()).unwrap()
        })
        .collect();
    Entry::new(parts, &none()).unwrap()
}

pub(super) fn first() -> Entry {
    entry_at("2026-11.1")
}

pub(super) fn second() -> Entry {
    entry_at("2026-11.2")
}

/// The adapter's revision. No adapter is in the build: the test states one.
pub(super) fn adapter() -> Revision {
    revision("7")
}

pub(super) fn record_of(entry: &Entry) -> InstallationRecord {
    InstallationRecord::of(entry, adapter(), &none()).unwrap()
}

pub(super) fn installing(
    fs: &Arc<ShimFs>,
    root: &Path,
    entry: &Entry,
    fake: &FakeDownloader,
) -> Result<InstallationRecord, InstallRefused> {
    let dir = ModelsDir::at(fs.clone(), root);
    install(&dir, entry, &adapter(), fake, &Cancellation::new(), &none())
}

/// Installs on a shim of its own: a process that came, installed and went.
pub(super) fn installed(root: &Path, entry: &Entry) -> InstallationRecord {
    installing(&Arc::new(ShimFs::new()), root, entry, &serving_both()).unwrap()
}

/// What the pointer says, read by one who holds no lock, on the real file system.
pub(super) fn reading(root: &Path) -> Current {
    reading_with(root, &none())
}

fn reading_with(root: &Path, secrets: &Secrets) -> Current {
    current(
        &ModelsDir::at(Arc::new(RealFs), root),
        &model("tiny"),
        secrets,
    )
    .unwrap()
}

pub(super) fn said(root: &Path) -> Option<(State, u64, u64)> {
    status(
        &ModelsDir::at(Arc::new(RealFs), root),
        &model("tiny"),
        &none(),
    )
    .unwrap()
    .map(|status| (status.state(), status.files_done(), status.files()))
}

pub(super) fn tiny(root: &Path) -> PathBuf {
    root.join("models/tiny")
}

pub(super) fn folder_of(root: &Path, record: &InstallationRecord) -> PathBuf {
    tiny(root)
        .join("installations")
        .join(record.identity().hex())
}

pub(super) fn hold_file_of(root: &Path, record: &InstallationRecord) -> PathBuf {
    tiny(root).join("holds").join(record.identity().hex())
}

/// A hold on this installation, as task M5 will take one: a share of its hold file,
/// from another process.
fn held(root: &Path, record: &InstallationRecord) -> Box<dyn Held> {
    RealFs
        .try_share(&hold_file_of(root, record))
        .unwrap()
        .unwrap()
}

/// The installation is on the disk whole: its record in the one form, its two files,
/// and nothing else.
pub(super) fn is_whole(root: &Path, record: &InstallationRecord) {
    let folder = folder_of(root, record);
    assert_eq!(on_disk(&folder.join("RECORD")).unwrap(), record.to_bytes());
    assert_eq!(on_disk(&folder.join("files/graph.onnx")).unwrap(), graph());
    assert_eq!(on_disk(&folder.join("files/LICENSE.txt")).unwrap(), TERMS);
    assert_eq!(
        names_under(&folder),
        ["RECORD", "files", "files/LICENSE.txt", "files/graph.onnx"]
    );
}

/// The installations on the disk, by the names of their folders.
pub(super) fn installations(root: &Path) -> Vec<String> {
    names_under(&tiny(root).join("installations"))
        .into_iter()
        .filter(|name| !name.contains('/'))
        .collect()
}

/// Whether an event is the opening, or a read, of a staged file.
fn reads_staged(event: &Event) -> bool {
    matches!(event, Event::Open(p) | Event::Read(p)
        if p.components().any(|part| part.as_os_str() == "incoming"))
}

pub(super) fn index_of(events: &[Event], what: impl Fn(&Event) -> bool) -> usize {
    events.iter().position(what).unwrap()
}

/// The events of installing `entry` after these were installed, each by a process of
/// its own.
pub(super) fn events_after(before: &[&Entry], entry: &Entry) -> Vec<Event> {
    let scratch = Scratch::new("publish-events");
    for earlier in before {
        installed(scratch.path(), earlier);
    }
    let fs = Arc::new(ShimFs::new());
    installing(&fs, scratch.path(), entry, &serving_both()).unwrap();
    fs.events()
}

fn holds_no_path(refused: &InstallRefused, root: &Path) {
    let printed = format!("{refused} {refused:?}");
    assert!(
        printed.bytes().all(|b| b.is_ascii_graphic() || b == b' '),
        "{printed:?}"
    );
    assert!(printed.len() <= 400);
    assert!(!printed.contains(&*root.to_string_lossy()), "{printed}");
    for part in ["staging", "incoming", "models/", "installations", "CURRENT"] {
        assert!(!printed.contains(part), "{printed}");
    }
}

/// Changes one byte of a file and keeps its length.
pub(super) fn flip(path: &Path, place: usize) {
    let mut bytes = std::fs::read(path).unwrap();
    bytes[place] ^= 1;
    std::fs::write(path, bytes).unwrap();
}

// --- an install ends with the pointer naming a verified installation ---------------------------

#[test]
fn an_install_of_two_files_ends_with_the_pointer_naming_a_verified_installation() {
    let scratch = Scratch::new("publish-two");
    let root = scratch.path();
    let fs = Arc::new(ShimFs::new());
    let record = installing(&fs, root, &first(), &serving_both()).unwrap();
    assert_eq!(record, record_of(&first()));
    assert_eq!(record.adapter_revision().as_str(), "7");
    let hex = record.identity().hex();
    // The pointer is the identity's text and one line feed, 72 bytes.
    assert_eq!(
        on_disk(&tiny(root).join("CURRENT")).unwrap(),
        format!("{}\n", record.identity()).into_bytes()
    );
    assert_eq!(format!("{}\n", record.identity()).len(), 72);
    assert_eq!(reading(root), Current::Installed(record.clone()));
    is_whole(root, &record);
    // The layout, and nothing else: the staging folder is gone into the
    // installation, and the hold file is there, empty.
    assert_eq!(
        names_under(root),
        [
            "LOCK".to_owned(),
            "models".to_owned(),
            "models/tiny".to_owned(),
            "models/tiny/CURRENT".to_owned(),
            "models/tiny/LOCK".to_owned(),
            "models/tiny/STATUS".to_owned(),
            "models/tiny/holds".to_owned(),
            format!("models/tiny/holds/{hex}"),
            "models/tiny/installations".to_owned(),
            format!("models/tiny/installations/{hex}"),
            format!("models/tiny/installations/{hex}/RECORD"),
            format!("models/tiny/installations/{hex}/files"),
            format!("models/tiny/installations/{hex}/files/LICENSE.txt"),
            format!("models/tiny/installations/{hex}/files/graph.onnx"),
            "models/tiny/staging".to_owned(),
        ]
    );
    assert_eq!(on_disk(&hold_file_of(root, &record)).unwrap(), b"");
    assert_eq!(said(root), Some((State::Installed, 2, 2)));
    touches_only(&fs.events(), root);
    // Each staged file was read back from the disk before anything was published.
    let events = fs.events();
    let renamed = index_of(
        &events,
        |event| matches!(event, Event::Rename(_, to) if *to == folder_of(root, &record)),
    );
    for name in ["graph.onnx", "LICENSE.txt"] {
        let staged = incoming(root).join("files").join(name);
        let read = index_of(&events, |event| *event == Event::Read(staged.clone()));
        assert!(read < renamed, "{name}");
    }
    // What was published is durable: a crash that undoes every name that was not
    // synced leaves the pointer and the installation.
    fs.crash(Lost::Unsynced);
    assert_eq!(reading(root), Current::Installed(record.clone()));
    is_whole(root, &record);
    assert!(hold_file_of(root, &record).exists());
    // And the model's lock was let go: the next installs.
    assert_eq!(installed(root, &first()), record);
}

#[test]
fn a_failed_install_leaves_the_previous_installation_named_and_readable_and_its_status_says_how_it_ended()
 {
    let fault = Fault::new(FaultKind::Unavailable, "the scripted host is down");
    let mut wrong = graph();
    wrong[7] ^= 1;
    let licence = |fake: FakeDownloader| fake.serving(&at("LICENSE.txt"), TERMS.to_vec());
    let cases: [(FakeDownloader, InstallRefused, u64); 4] = [
        (
            licence(FakeDownloader::new().serving(&at("graph.onnx"), wrong)),
            InstallRefused::File {
                name: "graph.onnx".to_owned(),
                why: FileRefused::WrongHash,
            },
            1,
        ),
        (
            licence(FakeDownloader::new().serving(&at("graph.onnx"), graph()[..100].to_vec())),
            InstallRefused::File {
                name: "graph.onnx".to_owned(),
                why: FileRefused::TooShort,
            },
            1,
        ),
        (
            licence(FakeDownloader::new().failing_after(&at("graph.onnx"), graph(), 1024, fault)),
            InstallRefused::File {
                name: "graph.onnx".to_owned(),
                why: FileRefused::Download(FaultKind::Unavailable),
            },
            1,
        ),
        (
            FakeDownloader::new()
                .serving(&at("graph.onnx"), graph())
                .serving(&at("LICENSE.txt"), b"terms and more".to_vec()),
            InstallRefused::File {
                name: "LICENSE.txt".to_owned(),
                why: FileRefused::TooLong,
            },
            0,
        ),
    ];
    for (fake, refusal, done) in cases {
        let scratch = Scratch::new("publish-failed");
        let root = scratch.path();
        let before = installed(root, &first());
        let refused = installing(&Arc::new(ShimFs::new()), root, &second(), &fake).unwrap_err();
        assert_eq!(refused, refusal);
        holds_no_path(&refused, root);
        assert_eq!(reading(root), Current::Installed(before.clone()));
        is_whole(root, &before);
        assert_eq!(installations(root), [before.identity().hex()]);
        assert_eq!(said(root), Some((State::Failed, done, 2)), "{refusal:?}");
        assert!(!incoming(root).exists());
    }
    // Cancelled before it starts: the status is not touched, since nothing is.
    let scratch = Scratch::new("publish-cancelled");
    let root = scratch.path();
    let before = installed(root, &first());
    let cancellation = Cancellation::new();
    cancellation.cancel();
    let dir = ModelsDir::at(Arc::new(ShimFs::new()), root);
    let refused = install(
        &dir,
        &second(),
        &adapter(),
        &serving_both(),
        &cancellation,
        &none(),
    )
    .unwrap_err();
    assert_eq!(refused, InstallRefused::Cancelled);
    assert_eq!(reading(root), Current::Installed(before));
    assert_eq!(said(root), Some((State::Installed, 2, 2)));
    // Cancelled inside a download: the status says so.
    let fake = FakeDownloader::new()
        .serving(&at("LICENSE.txt"), TERMS.to_vec())
        .failing(
            &at("graph.onnx"),
            Fault::new(FaultKind::Cancelled, "the download was cancelled"),
        );
    let refused = installing(&Arc::new(ShimFs::new()), root, &second(), &fake).unwrap_err();
    assert_eq!(refused, InstallRefused::Cancelled);
    assert_eq!(said(root), Some((State::Cancelled, 1, 2)));
}

#[test]
fn every_refusal_of_an_install_says_one_sentence_of_fixed_words() {
    let sentences = [
        (
            InstallRefused::HoldsSecret,
            "a name or the installation's record holds a secret, and is not shown",
        ),
        (
            InstallRefused::Busy,
            "the model is being installed or removed by another",
        ),
        (InstallRefused::Cancelled, "the install was cancelled"),
        (
            InstallRefused::File {
                name: "graph.onnx".to_owned(),
                why: FileRefused::TooShort,
            },
            "the file `graph.onnx` is shorter than the catalogue states",
        ),
        (
            InstallRefused::NotWhole {
                name: "graph.onnx".to_owned(),
            },
            "the file `graph.onnx` was not on the disk as it was downloaded",
        ),
        (
            InstallRefused::Held,
            "a damaged installation of the same identity is held by a run",
        ),
        (
            InstallRefused::Draining,
            "the model is being removed, and nothing of it is published",
        ),
        (
            InstallRefused::DirectoryBusy,
            "the models directory is locked by another, which did not let it go",
        ),
        (
            InstallRefused::Disk(std::io::ErrorKind::StorageFull),
            "the models directory could not be written: the disk is full",
        ),
        (
            InstallRefused::Disk(std::io::ErrorKind::PermissionDenied),
            "the models directory could not be written: permission is denied",
        ),
        (
            InstallRefused::Disk(std::io::ErrorKind::CrossesDevices),
            "the models directory could not be written: the system refused",
        ),
        (
            InstallRefused::Failed,
            "the install of the model that was running did not install it",
        ),
        (
            InstallRefused::Left,
            "the request stopped waiting for the install of the model, which goes on",
        ),
    ];
    for (refused, sentence) in sentences {
        assert_eq!(refused.to_string(), sentence);
    }
}

// --- the status, as the install goes -----------------------------------------------------------

#[test]
fn the_status_says_installing_and_how_many_files_are_staged_while_the_install_goes() {
    let events = events_after(&[], &first());
    // Before the first piece of each file is kept, and before the first staged file
    // is read back: `LICENSE.txt` comes first, by the order of the names.
    let begun = |name: &'static str| move |event: &Event| matches!(event, Event::Begin(p) if p.ends_with(name));
    let moments = [
        (index_of(&events, begun("LICENSE.txt")), 0),
        (index_of(&events, begun("graph.onnx")), 1),
        (index_of(&events, reads_staged), 2),
    ];
    for (moment, done) in moments {
        let scratch = Scratch::new("publish-status");
        let root = scratch.path().to_path_buf();
        let fs = Arc::new(ShimFs::new());
        let seen = Arc::new(Mutex::new(None));
        let (see, at_root) = (seen.clone(), root.clone());
        fs.pause_before(moment, move || {
            *see.lock().unwrap() = said(&at_root);
        });
        installing(&fs, &root, &first(), &serving_both()).unwrap();
        assert_eq!(
            *seen.lock().unwrap(),
            Some((State::Installing, done, 2)),
            "event {moment}"
        );
        assert_eq!(said(&root), Some((State::Installed, 2, 2)));
    }
    // No model's folder, or one with no status: none. A status that is a link, or
    // too long, is refused and not followed.
    let scratch = Scratch::new("publish-status-none");
    let root = scratch.path();
    assert_eq!(said(root), None);
    std::fs::create_dir_all(tiny(root)).unwrap();
    assert_eq!(said(root), None);
    let dir = ModelsDir::at(Arc::new(RealFs), root);
    let refused = |bytes: &[u8]| {
        let _ = std::fs::remove_file(tiny(root).join("STATUS"));
        std::fs::write(tiny(root).join("STATUS"), bytes).unwrap();
        status(&dir, &model("tiny"), &none())
            .unwrap_err()
            .to_string()
    };
    assert_eq!(
        refused(&[b' '; 257]),
        "the status record is longer than 256 bytes"
    );
    assert_eq!(
        refused(b"installing"),
        "the status record is not JSON at line 1"
    );
    #[cfg(unix)]
    {
        let outside = Scratch::new("publish-status-outside");
        let whole = br#"{"format":"status/1","state":"failed","files_done":1,"files":2}"#;
        std::fs::write(outside.path().join("STATUS"), whole).unwrap();
        std::fs::remove_file(tiny(root).join("STATUS")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("STATUS"), tiny(root).join("STATUS"))
            .unwrap();
        assert_eq!(
            status(&dir, &model("tiny"), &none())
                .unwrap_err()
                .to_string(),
            "the status record cannot be read: the system could not read it"
        );
    }
}

// --- a staged file is read back from the disk before it is trusted -----------------------------

#[test]
fn a_staged_file_that_is_not_on_the_disk_as_it_was_downloaded_is_not_published() {
    let events = events_after(&[&first()], &second());
    let reading_back = index_of(&events, reads_staged);
    type Damage = fn(&Path);
    let damages: [(&str, Damage); 6] = [
        // The right length, one byte another: no crash leaves this of a finished
        // file, and a verifier that trusted the length would publish it.
        ("graph.onnx", |file| flip(file, 1500)),
        ("LICENSE.txt", |file| flip(file, 4)),
        ("graph.onnx", |file| {
            let mut bytes = std::fs::read(file).unwrap();
            bytes.push(0);
            std::fs::write(file, bytes).unwrap();
        }),
        ("graph.onnx", |file| {
            std::fs::write(file, &graph()[..2499]).unwrap();
        }),
        ("LICENSE.txt", |file| std::fs::remove_file(file).unwrap()),
        // A link to a file that is the file: refused, not followed.
        ("graph.onnx", |file| {
            let kept = file.with_file_name("kept-elsewhere");
            std::fs::rename(file, &kept).unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink(&kept, file).unwrap();
        }),
    ];
    for (at, (name, damage)) in damages.into_iter().enumerate() {
        let scratch = Scratch::new("publish-damaged");
        let root = scratch.path();
        let before = installed(root, &first());
        let fs = Arc::new(ShimFs::new());
        let staged = incoming(root).join("files").join(name);
        fs.pause_before(reading_back, move || damage(&staged));
        let refused = installing(&fs, root, &second(), &serving_both()).unwrap_err();
        assert_eq!(
            refused,
            InstallRefused::NotWhole {
                name: name.to_owned()
            },
            "damage {at}"
        );
        // Nothing was published, and the staging is gone.
        assert_eq!(reading(root), Current::Installed(before.clone()));
        assert_eq!(installations(root), [before.identity().hex()]);
        assert!(!hold_file_of(root, &record_of(&second())).exists());
        assert!(!incoming(root).exists());
        assert_eq!(said(root), Some((State::Failed, 2, 2)));
    }
}

#[test]
fn a_staged_file_is_read_back_whole_however_long_and_named_as_it_is_shown_when_it_is_not() {
    // One file, a byte longer than a piece of the streamed read, under a name
    // longer than what is shown. About a megabyte, in a scratch folder.
    let long = format!("{}.onnx", "g".repeat(100));
    let whole: Vec<u8> = (0..=PIECE_MAX_BYTES).map(|at| (at % 251) as u8).collect();
    let entry = entry_of("2026-11.1", &[(&long, whole.clone())]);
    let fake = FakeDownloader::new().serving(&at(&long), whole.clone());
    let scratch = Scratch::new("publish-long");
    let root = scratch.path();
    let fs = Arc::new(ShimFs::new());
    let record = installing(&fs, root, &entry, &fake).unwrap();
    let events = fs.events();
    // Read back in two pieces: the first full, the second its last byte.
    let reads = events.iter().filter(|event| reads_staged(event)).count();
    assert_eq!(reads, 3, "the opening and two reads");
    assert!(on_disk(&folder_of(root, &record).join("files").join(&long)).unwrap() == whole);
    // The last byte another, at the right length: found, in the second piece.
    let scratch = Scratch::new("publish-long");
    let root = scratch.path();
    let fs = Arc::new(ShimFs::new());
    let staged = incoming(root).join("files").join(&long);
    fs.pause_before(index_of(&events, reads_staged), move || {
        flip(&staged, PIECE_MAX_BYTES)
    });
    let refused = installing(&fs, root, &entry, &fake).unwrap_err();
    assert_eq!(
        refused,
        InstallRefused::NotWhole {
            name: format!("{}...", "g".repeat(64))
        }
    );
    assert_eq!(reading(root), Current::NotInstalled);
}

// --- the pointer and the record, read back -----------------------------------------------------

#[test]
fn a_pointer_or_a_record_that_is_not_as_it_is_written_is_damage_and_never_not_installed() {
    let scratch = Scratch::new("publish-read");
    let root = scratch.path();
    assert_eq!(reading(root), Current::NotInstalled);
    let record = installed(root, &first());
    let pointer = tiny(root).join("CURRENT");
    let text = record.identity().to_string();
    let hex = record.identity().hex();
    let put = |bytes: &[u8]| {
        let _ = std::fs::remove_file(&pointer);
        std::fs::write(&pointer, bytes).unwrap();
    };
    let pointers: Vec<Vec<u8>> = vec![
        text.clone().into_bytes(),
        format!("{text}\r\n").into_bytes(),
        format!("{text}\n\n").into_bytes(),
        format!(" {text}\n").into_bytes(),
        format!("{text} \n").into_bytes()[1..].to_vec(),
        format!("{}\n", text.to_uppercase()).into_bytes(),
        format!("sha256:{}\n", hex.to_uppercase()).into_bytes(),
        format!("{hex}\n").into_bytes(),
        format!("{hex}sha256:\n").into_bytes(),
        vec![b'\n'; 72],
        vec![0xff; 72],
        // 72 bytes whose last is not a line feed.
        format!("{text} ").into_bytes(),
        format!("{text}\r").into_bytes(),
        format!("{text}\0").into_bytes(),
        Vec::new(),
    ];
    for bytes in pointers {
        put(&bytes);
        assert_eq!(
            reading(root),
            Current::Damaged(Damage::Pointer),
            "{:?}",
            String::from_utf8_lossy(&bytes)
        );
    }
    // A pointer whose text holds a secret the reader was told of is damage too.
    put(format!("{text}\n").as_bytes());
    assert_eq!(reading(root), Current::Installed(record.clone()));
    let told = Secrets::new([hex[..16].to_owned()]).unwrap();
    assert_eq!(reading_with(root, &told), Current::Damaged(Damage::Pointer));
    // A folder where the pointer belongs, and a link to a pointer that is whole.
    std::fs::remove_file(&pointer).unwrap();
    std::fs::create_dir(&pointer).unwrap();
    assert_eq!(reading(root), Current::Damaged(Damage::Pointer));
    std::fs::remove_dir(&pointer).unwrap();
    #[cfg(unix)]
    {
        let outside = Scratch::new("publish-read-outside");
        std::fs::write(outside.path().join("CURRENT"), format!("{text}\n")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("CURRENT"), &pointer).unwrap();
        assert_eq!(reading(root), Current::Damaged(Damage::Pointer));
        std::fs::remove_file(&pointer).unwrap();
    }
    // No pointer: not installed, whatever else the model's folder holds.
    assert_eq!(reading(root), Current::NotInstalled);
    put(format!("{text}\n").as_bytes());

    // The record. Each is put where the pointer names, and is damage.
    let at_record = folder_of(root, &record).join("RECORD");
    let whole = record.to_bytes();
    let pretty = String::from_utf8(whole.clone())
        .unwrap()
        .replace("\",\"", "\", \"");
    let mut long = whole.clone();
    long.resize(40_000, b' ');
    let another = record_of(&second()).to_bytes();
    let elsewhere = InstallationRecord::new(
        model("small"),
        revision("2026-11.1"),
        model("owlv2-sam2"),
        adapter(),
        record.files().to_vec(),
        &none(),
    )
    .unwrap();
    let records: [(&[u8], Damage); 6] = [
        (b"", Damage::Record),
        (b"{}", Damage::Record),
        (pretty.as_bytes(), Damage::Record),
        (&long, Damage::Record),
        (&another, Damage::Identity),
        (&elsewhere.to_bytes(), Damage::Identity),
    ];
    for (bytes, damage) in records {
        std::fs::write(&at_record, bytes).unwrap();
        assert_eq!(
            reading(root),
            Current::Damaged(damage),
            "{}",
            bytes.len().min(99)
        );
    }
    std::fs::write(&at_record, &whole).unwrap();
    assert_eq!(reading(root), Current::Installed(record.clone()));
    // A record that holds a secret the reader was told of.
    let told = Secrets::new(["owlv2-sam2".to_owned()]).unwrap();
    assert_eq!(reading_with(root, &told), Current::Damaged(Damage::Record));
    // No record, a folder where it belongs, no installation's folder.
    std::fs::remove_file(&at_record).unwrap();
    assert_eq!(reading(root), Current::Damaged(Damage::Record));
    std::fs::create_dir(&at_record).unwrap();
    assert_eq!(reading(root), Current::Damaged(Damage::Record));
    std::fs::remove_dir_all(folder_of(root, &record)).unwrap();
    assert_eq!(reading(root), Current::Damaged(Damage::Record));
    std::fs::write(folder_of(root, &record), b"a file where the folder belongs").unwrap();
    assert_eq!(reading(root), Current::Damaged(Damage::Record));
    // A new install of the model repairs it.
    assert_eq!(installed(root, &first()), record);
    assert_eq!(reading(root), Current::Installed(record.clone()));
    is_whole(root, &record);
}

#[test]
fn a_reader_with_no_lock_that_finds_the_record_gone_reads_the_pointer_again() {
    // What the reader's own process does between its read of the pointer and its
    // opening of the record, standing for another that installs or removes then.
    type Meanwhile = fn(&Path, &InstallationRecord, &InstallationRecord);
    let gone: Meanwhile = |root, old, _| std::fs::remove_dir_all(folder_of(root, old)).unwrap();
    let replaced: Meanwhile = |root, old, new| {
        std::fs::remove_dir_all(folder_of(root, old)).unwrap();
        std::fs::write(tiny(root).join("CURRENT"), format!("{}\n", new.identity())).unwrap();
    };
    let removed: Meanwhile = |root, old, _| {
        std::fs::remove_dir_all(folder_of(root, old)).unwrap();
        std::fs::remove_file(tiny(root).join("CURRENT")).unwrap();
    };
    for (which, meanwhile) in [gone, replaced, removed].into_iter().enumerate() {
        let scratch = Scratch::new("publish-reader");
        let root = scratch.path().to_path_buf();
        // Two installations on the disk, the first kept by a hold; the pointer is
        // put back to the first by hand.
        let old = installed(&root, &first());
        let hold = held(&root, &old);
        let new = installed(&root, &second());
        std::fs::write(tiny(&root).join("CURRENT"), format!("{}\n", old.identity())).unwrap();
        drop(hold);
        let fs = Arc::new(ShimFs::new());
        let (at_root, the_old, the_new) = (root.clone(), old.clone(), new.clone());
        // Event 0 opens the pointer, event 1 reads it, event 2 opens the record.
        fs.pause_before(2, move || meanwhile(&at_root, &the_old, &the_new));
        let dir = ModelsDir::at(fs.clone(), &root);
        let answered = current(&dir, &model("tiny"), &none()).unwrap();
        let expected = match which {
            0 => Current::Damaged(Damage::Record),
            1 => Current::Installed(new),
            _ => Current::NotInstalled,
        };
        assert_eq!(answered, expected, "case {which}");
        // The pointer was opened and read, and then, the record being gone, opened
        // again: there is none to open in the last case.
        let pointer = tiny(&root).join("CURRENT");
        let events = fs.events();
        assert_eq!(
            events[..2],
            [Event::Open(pointer.clone()), Event::Read(pointer.clone())],
            "case {which}"
        );
        assert_eq!(
            events.get(2),
            (which < 2).then_some(&Event::Open(pointer)),
            "case {which}"
        );
    }
}

// --- superseded installations, and the same identity again -------------------------------------

#[test]
fn a_superseded_installation_is_deleted_when_no_hold_is_on_it_and_left_while_one_is() {
    let scratch = Scratch::new("publish-superseded");
    let root = scratch.path();
    let old = installed(root, &first());
    // Nobody holds it: the install that replaces it deletes it, and its hold file
    // stays.
    let new = installed(root, &second());
    assert_eq!(reading(root), Current::Installed(new.clone()));
    assert_eq!(installations(root), [new.identity().hex()]);
    assert!(hold_file_of(root, &old).exists());
    assert!(hold_file_of(root, &new).exists());
    // Held: the install that replaces it leaves it, whole, and does not wait.
    let hold = held(root, &new);
    let third = installed(root, &entry_at("2026-11.3"));
    assert_eq!(reading(root), Current::Installed(third.clone()));
    is_whole(root, &new);
    is_whole(root, &third);
    // Still held at the next install, it is left again; let go, the next install
    // deletes it at its start, before it stages.
    installed(root, &entry_at("2026-11.3"));
    is_whole(root, &new);
    drop(hold);
    let fs = Arc::new(ShimFs::new());
    installing(&fs, root, &entry_at("2026-11.3"), &serving_both()).unwrap();
    assert_eq!(installations(root), [third.identity().hex()]);
    let events = fs.events();
    let deleted = index_of(&events, |event| {
        *event == Event::Remove(folder_of(root, &new))
    });
    let pointing = index_of(&events, |event| *event == Event::Lock(root.join("LOCK")));
    assert!(deleted < pointing);
    assert_eq!(
        events[deleted + 1],
        Event::SyncDir(tiny(root).join("installations"))
    );
    // And not before the pointer it read was durable: the model's folder is synced
    // before anything is deleted.
    assert!(events[..deleted].contains(&Event::SyncDir(tiny(root))));
}

#[test]
fn what_the_pointer_does_not_name_is_deleted_and_a_folder_with_no_hold_file_gets_none() {
    let scratch = Scratch::new("publish-unpointed");
    let root = scratch.path();
    let record = installed(root, &first());
    // A folder that was renamed in and never pointed to, as a crash after step 1
    // leaves one; a file, and a link, where no installation's folder is.
    let never = record_of(&second());
    std::fs::create_dir_all(folder_of(root, &never).join("files")).unwrap();
    std::fs::write(folder_of(root, &never).join("RECORD"), never.to_bytes()).unwrap();
    let outside = Scratch::new("publish-unpointed-outside");
    std::fs::write(outside.path().join("kept"), b"not whizbang's").unwrap();
    std::fs::write(tiny(root).join("installations/notes.txt"), b"x").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), tiny(root).join("installations/elsewhere")).unwrap();
    let fs = Arc::new(ShimFs::new());
    assert_eq!(
        installing(&fs, root, &first(), &serving_both()).unwrap(),
        record
    );
    assert_eq!(installations(root), [record.identity().hex()]);
    assert!(!hold_file_of(root, &never).exists());
    assert_eq!(
        on_disk(&outside.path().join("kept")).unwrap(),
        b"not whizbang's"
    );
    touches_only(&fs.events(), root);
    // With no pointer at all, the same: a folder that no pointer names is deleted,
    // but for one of the identity being installed.
    std::fs::remove_file(tiny(root).join("CURRENT")).unwrap();
    std::fs::create_dir_all(folder_of(root, &never).join("files")).unwrap();
    installed(root, &first());
    assert_eq!(installations(root), [record.identity().hex()]);
    // With a pointer that cannot be read, which installation is in use is not
    // known: nothing is deleted before a new pointer is set.
    let held_one = installed(root, &second());
    std::fs::write(tiny(root).join("CURRENT"), b"damaged").unwrap();
    let fs = Arc::new(ShimFs::new());
    installing(&fs, root, &first(), &serving_both()).unwrap();
    let events = fs.events();
    let pointed = index_of(
        &events,
        |event| matches!(event, Event::Rename(_, to) if to.ends_with("CURRENT")),
    );
    let deleted = index_of(&events, |event| {
        *event == Event::Remove(folder_of(root, &held_one))
    });
    assert!(pointed < deleted);
    assert_eq!(installations(root), [record.identity().hex()]);
}

#[test]
fn an_identity_that_is_on_the_disk_already_is_pointed_to_when_whole_and_replaced_when_damaged() {
    let scratch = Scratch::new("publish-again");
    let root = scratch.path();
    let record = installed(root, &first());
    let folder = folder_of(root, &record);
    let onto = |events: &[Event]| {
        events
            .iter()
            .any(|event| matches!(event, Event::Rename(_, to) if *to == folder))
    };
    // In use and whole: nothing is downloaded and nothing is renamed over it. The
    // folder that names it is synced, and the pointer is written and synced again:
    // the install that made it may have been refused either sync.
    // A staging folder that a dead install left is removed all the same.
    std::fs::create_dir_all(incoming(root).join("files")).unwrap();
    let fs = Arc::new(ShimFs::new());
    let fake = serving_both();
    assert_eq!(installing(&fs, root, &first(), &fake).unwrap(), record);
    assert!(fake.requests().is_empty());
    assert!(!onto(&fs.events()));
    let events = fs.events();
    let locked = index_of(&events, |event| *event == Event::Lock(root.join("LOCK")));
    assert_eq!(
        events[locked - 1],
        Event::SyncDir(tiny(root).join("installations"))
    );
    assert_eq!(events[locked + 5], Event::SyncDir(tiny(root)));
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Remove(p) if p.ends_with("CURRENT")))
    );
    assert!(!incoming(root).exists());
    assert_eq!(said(root), Some((State::Installed, 2, 2)));
    // Superseded earlier and still held, whole: pointed to again, as it is.
    let hold = held(root, &record);
    let other = installed(root, &second());
    let fs = Arc::new(ShimFs::new());
    let fake = serving_both();
    assert_eq!(installing(&fs, root, &first(), &fake).unwrap(), record);
    assert!(fake.requests().is_empty());
    assert!(!onto(&fs.events()));
    assert_eq!(reading(root), Current::Installed(record.clone()));
    assert_eq!(installations(root), [record.identity().hex()]);
    assert!(!incoming(root).exists());
    // Superseded, still held, and damaged at its right length: busy, and the
    // pointer names what it named.
    installed(root, &second());
    flip(&folder.join("files/graph.onnx"), 9);
    let refused =
        installing(&Arc::new(ShimFs::new()), root, &first(), &serving_both()).unwrap_err();
    assert_eq!(refused, InstallRefused::Held);
    assert_eq!(reading(root), Current::Installed(other.clone()));
    assert_eq!(said(root), Some((State::Failed, 2, 2)));
    assert!(!incoming(root).exists());
    drop(hold);
    // Let go: the next install deletes the damaged folder at its start, and
    // installs.
    assert_eq!(installed(root, &first()), record);
    is_whole(root, &record);
}

#[test]
fn a_repair_clears_the_pointer_deletes_the_damaged_installation_and_then_publishes() {
    // Each way the installation in use can be damaged.
    type Damage = fn(&Path);
    let damages: [Damage; 6] = [
        |folder| flip(&folder.join("files/graph.onnx"), 2000),
        |folder| std::fs::remove_file(folder.join("files/LICENSE.txt")).unwrap(),
        |folder| std::fs::write(folder.join("files/extra"), b"x").unwrap(),
        |folder| std::fs::write(folder.join("extra"), b"x").unwrap(),
        |folder| flip(&folder.join("RECORD"), 3),
        |folder| {
            std::fs::remove_dir_all(folder.join("files")).unwrap();
            std::fs::write(folder.join("files"), b"x").unwrap();
        },
    ];
    for (at, damage) in damages.into_iter().enumerate() {
        let scratch = Scratch::new("publish-repair");
        let root = scratch.path();
        let record = installed(root, &first());
        let folder = folder_of(root, &record);
        damage(&folder);
        // Held: the repair answers busy, and the pointer is put back.
        let hold = held(root, &record);
        let fs = Arc::new(ShimFs::new());
        let refused = installing(&fs, root, &first(), &serving_both()).unwrap_err();
        assert_eq!(refused, InstallRefused::Held, "damage {at}");
        assert_eq!(
            on_disk(&tiny(root).join("CURRENT")).unwrap(),
            format!("{}\n", record.identity()).into_bytes()
        );
        assert!(folder.exists());
        assert!(!incoming(root).exists());
        drop(hold);
        // Free: the pointer is cleared first, then the folder is deleted, then the
        // staged folder is renamed in, then the pointer is set.
        let fs = Arc::new(ShimFs::new());
        assert_eq!(
            installing(&fs, root, &first(), &serving_both()).unwrap(),
            record,
            "damage {at}"
        );
        let events = fs.events();
        let pointer = tiny(root).join("CURRENT");
        let order = [
            index_of(&events, |event| *event == Event::Remove(pointer.clone())),
            index_of(&events, |event| *event == Event::Remove(folder.clone())),
            index_of(
                &events,
                |event| matches!(event, Event::Rename(_, to) if *to == folder),
            ),
            index_of(
                &events,
                |event| matches!(event, Event::Rename(_, to) if *to == pointer),
            ),
        ];
        assert!(order.is_sorted(), "{order:?}");
        // The cleared pointer and the deletion were each made durable before the
        // next step.
        assert_eq!(events[order[0] + 1], Event::SyncDir(tiny(root)));
        assert_eq!(
            events[order[1] + 1],
            Event::SyncDir(tiny(root).join("installations"))
        );
        assert_eq!(reading(root), Current::Installed(record.clone()));
        is_whole(root, &record);
    }
}

// --- the directory's lock, the mark, the model's lock ------------------------------------------

#[test]
fn under_the_directorys_lock_a_publication_makes_the_hold_file_and_replaces_the_pointer_and_no_more()
 {
    let scratch = Scratch::new("publish-steps");
    let root = scratch.path();
    let fs = Arc::new(ShimFs::new());
    let record = installing(&fs, root, &first(), &serving_both()).unwrap();
    let model = tiny(root);
    let hold = hold_file_of(root, &record);
    let new = model.join("CURRENT.new");
    let events = fs.events();
    let locked = index_of(&events, |event| *event == Event::Lock(root.join("LOCK")));
    let steps = [
        Event::Write(hold.clone()),
        Event::SyncFile(hold),
        Event::SyncDir(model.join("holds")),
        Event::Write(new.clone()),
        Event::SyncFile(new.clone()),
        Event::Rename(new.clone(), model.join("CURRENT")),
        Event::SyncDir(model.clone()),
    ];
    assert_eq!(events[locked + 1..locked + 8], steps);
    // The installation was in its place, durably, before the lock was taken.
    assert_eq!(
        events[locked - 5..locked],
        [
            Event::Write(incoming(root).join("RECORD")),
            Event::SyncFile(incoming(root).join("RECORD")),
            Event::SyncDir(incoming(root)),
            Event::Rename(incoming(root), folder_of(root, &record)),
            Event::SyncDir(model.join("installations")),
        ]
    );
    // The same identity published again: its hold file is there and is not made
    // again, and the folder that names it is synced all the same, before the
    // pointer names the installation.
    let hold = held(root, &record);
    installed(root, &second());
    drop(hold);
    let fs = Arc::new(ShimFs::new());
    installing(&fs, root, &first(), &serving_both()).unwrap();
    let events = fs.events();
    let locked = index_of(&events, |event| *event == Event::Lock(root.join("LOCK")));
    assert_eq!(events[locked + 1..locked + 6], steps[2..]);

    // Who has the directory's lock, asked by another process at five moments of an
    // install: inside a download, while a staged file is read back, at the making
    // of the hold file, at the sync that makes the pointer durable, and after it.
    let events = events_after(&[&first()], &second());
    let synced = index_of(
        &events,
        |event| matches!(event, Event::Rename(_, to) if to.ends_with("CURRENT")),
    ) + 1;
    let moments = [
        (
            index_of(&events, |event| matches!(event, Event::Append(_))),
            false,
        ),
        (index_of(&events, reads_staged) + 1, false),
        (
            index_of(
                &events,
                |event| matches!(event, Event::Write(p) if p.parent().is_some_and(|p| p.ends_with("holds"))),
            ),
            true,
        ),
        (synced, true),
        (synced + 1, false),
    ];
    for (moment, locked) in moments {
        let scratch = Scratch::new("publish-locked");
        let root = scratch.path().to_path_buf();
        installed(&root, &first());
        let fs = Arc::new(ShimFs::new());
        let seen = Arc::new(Mutex::new(None));
        let (see, at_root) = (seen.clone(), root.clone());
        fs.pause_before(moment, move || {
            let free = RealFs
                .hold_lock_file(&at_root.join("LOCK"), false)
                .unwrap()
                .is_some();
            *see.lock().unwrap() = Some(!free);
        });
        installing(&fs, &root, &second(), &serving_both()).unwrap();
        assert_eq!(*seen.lock().unwrap(), Some(locked), "event {moment}");
    }
}

#[test]
fn a_directory_whose_lock_is_not_let_go_refuses_the_publication_and_names_what_it_named() {
    // Over another installation, and as a first install.
    for over in [true, false] {
        let scratch = Scratch::new("publish-directory-busy");
        let root = scratch.path();
        let was = if over {
            Current::Installed(installed(root, &first()))
        } else {
            Current::NotInstalled
        };
        let other = RealFs
            .hold_lock_file(&root.join("LOCK"), true)
            .unwrap()
            .unwrap();
        let refused =
            installing(&Arc::new(ShimFs::new()), root, &second(), &serving_both()).unwrap_err();
        assert_eq!(refused, InstallRefused::DirectoryBusy);
        assert_eq!(reading(root), was);
        // What was downloaded, read back and renamed in is not thrown away: it is
        // left, whole and not named, with no hold file, for the next install of
        // that identity.
        let next = record_of(&second());
        is_whole(root, &next);
        assert_eq!(installations(root).len(), if over { 2 } else { 1 });
        assert!(!hold_file_of(root, &next).exists());
        assert_eq!(said(root), Some((State::Failed, 2, 2)));
        drop(other);
        // The next install of it downloads nothing, and points to it.
        let fake = serving_both();
        let fs = Arc::new(ShimFs::new());
        assert_eq!(installing(&fs, root, &second(), &fake).unwrap(), next);
        assert!(fake.requests().is_empty());
        assert_eq!(reading(root), Current::Installed(next.clone()));
        assert_eq!(installations(root), [next.identity().hex()]);
        assert_eq!(said(root), Some((State::Installed, 2, 2)));
    }
}

/// The event at which an install first takes the directory's lock.
fn first_lock_of_the_directory(events: &[Event]) -> usize {
    index_of(
        events,
        |event| matches!(event, Event::Lock(p) if !p.ends_with("tiny/LOCK")),
    )
}

#[test]
fn a_mark_of_draining_stops_the_publication_before_a_hold_file_or_a_pointer_is_made() {
    let scratch = Scratch::new("publish-draining");
    let root = scratch.path();
    let before = installed(root, &first());
    // The mark is set when every file is staged and read back, just before the
    // directory's lock is taken to point: a download would have stopped for one
    // set earlier (task M5d), and one that is there when the install starts is
    // taken off as a dead remover's. It is the mark of a remover that is alive:
    // the remover's lock is held throughout. It is read as there or not there.
    let remover = live_remover(root);
    let mark = tiny(root).join("DRAINING");
    let pointing = first_lock_of_the_directory(&events_after(&[&first()], &second()));
    let fs = Arc::new(ShimFs::new());
    let set = mark.clone();
    fs.pause_before(pointing, move || std::fs::write(&set, b"").unwrap());
    let refused = installing(&fs, root, &second(), &serving_both()).unwrap_err();
    assert_eq!(refused, InstallRefused::Draining);
    assert_eq!(reading(root), Current::Installed(before.clone()));
    // What was renamed in is left, whole and not named, with no hold file.
    assert_eq!(installations(root).len(), 2);
    is_whole(root, &record_of(&second()));
    assert!(!hold_file_of(root, &record_of(&second())).exists());
    assert!(
        !fs.events()
            .iter()
            .any(|event| matches!(event, Event::Write(p) if p.ends_with("CURRENT.new")))
    );
    assert_eq!(said(root), Some((State::Failed, 2, 2)));
    assert!(!incoming(root).exists());
    // A repair is a publication too: with the mark set before it clears the
    // pointer, the damaged installation in use is left as it is, pointer and
    // folder, for the removal to find.
    let damaged = |root: &Path| {
        let before = installed(root, &first());
        flip(&folder_of(root, &before).join("files/graph.onnx"), 0);
        before
    };
    let clearing = {
        let scratch = Scratch::new("publish-draining-events");
        damaged(scratch.path());
        let fs = Arc::new(ShimFs::new());
        installing(&fs, scratch.path(), &first(), &serving_both()).unwrap();
        first_lock_of_the_directory(&fs.events())
    };
    drop(remover);
    let scratch = Scratch::new("publish-draining-repair");
    let root = scratch.path();
    let before = damaged(root);
    let remover = live_remover(root);
    let folder = folder_of(root, &before);
    let mark = tiny(root).join("DRAINING");
    let fs = Arc::new(ShimFs::new());
    let set = mark.clone();
    fs.pause_before(clearing, move || std::fs::write(&set, b"").unwrap());
    let refused = installing(&fs, root, &first(), &serving_both()).unwrap_err();
    assert_eq!(refused, InstallRefused::Draining);
    assert_eq!(reading(root), Current::Installed(before.clone()));
    assert!(folder.join("files/graph.onnx").exists());
    std::fs::remove_file(&mark).unwrap();
    // The mark set after a repair cleared the pointer, with the installation held:
    // the pointer is put back, mark or no mark, so that no reader finds the model
    // with neither. A held repair takes the directory's lock twice, to clear the
    // pointer and to put it back; the mark is set here before the second.
    let hold = held(root, &before);
    let held_repair = |fs: &Arc<ShimFs>| {
        let refused = installing(fs, root, &first(), &serving_both()).unwrap_err();
        assert_eq!(refused, InstallRefused::Held);
        assert_eq!(
            on_disk(&tiny(root).join("CURRENT")).unwrap(),
            format!("{}\n", before.identity()).into_bytes()
        );
    };
    let fs = Arc::new(ShimFs::new());
    held_repair(&fs);
    let locks: Vec<usize> = (0..fs.events().len())
        .filter(|at| fs.events()[*at] == Event::Lock(root.join("LOCK")))
        .collect();
    assert_eq!(locks.len(), 2);
    let fs = Arc::new(ShimFs::new());
    let mark = tiny(root).join("DRAINING");
    fs.pause_before(locks[1], move || std::fs::write(&mark, b"").unwrap());
    held_repair(&fs);
    assert!(tiny(root).join("DRAINING").exists());
    drop(hold);
    drop(remover);
    std::fs::remove_file(tiny(root).join("DRAINING")).unwrap();
    installed(root, &second());
}

#[test]
fn a_held_repair_that_could_not_put_the_pointer_back_leaves_the_model_not_installed_and_the_next_is_refused_as_held()
 {
    // A held repair takes the directory's lock twice: to clear the pointer, and to
    // put it back. The second is not given: refused by the file system, or held by
    // another until the tries run out.
    for busy in [false, true] {
        let scratch = Scratch::new("publish-held-repair");
        let root = scratch.path();
        let record = installed(root, &first());
        let folder = folder_of(root, &record);
        flip(&folder.join("files/graph.onnx"), 9);
        let hold = held(root, &record);
        let locks = {
            let fs = Arc::new(ShimFs::new());
            let refused = installing(&fs, root, &first(), &serving_both()).unwrap_err();
            assert_eq!(refused, InstallRefused::Held);
            let events = fs.events();
            let locks: Vec<usize> = (0..events.len())
                .filter(|at| events[*at] == Event::Lock(root.join("LOCK")))
                .collect();
            assert_eq!(locks.len(), 2);
            locks
        };
        let fs = Arc::new(ShimFs::new());
        let other = Arc::new(Mutex::new(None));
        let refused = if busy {
            let (keep, lock) = (other.clone(), root.join("LOCK"));
            fs.pause_before(locks[1], move || {
                *keep.lock().unwrap() = RealFs.hold_lock_file(&lock, true).unwrap();
            });
            let refused = installing(&fs, root, &first(), &serving_both()).unwrap_err();
            assert!(other.lock().unwrap().take().is_some());
            assert_eq!(refused, InstallRefused::DirectoryBusy);
            refused
        } else {
            fs.refuse_before(locks[1]);
            installing(&fs, root, &first(), &serving_both()).unwrap_err()
        };
        assert!(
            matches!(
                refused,
                InstallRefused::Disk(_) | InstallRefused::DirectoryBusy
            ),
            "{refused:?}"
        );
        // No pointer names the damaged folder, which the run still holds.
        assert_eq!(reading(root), Current::NotInstalled, "busy {busy}");
        assert!(folder.exists() && hold_file_of(root, &record).exists());
        assert_eq!(said(root).map(|said| said.0), Some(State::Failed));
        assert!(!incoming(root).exists());
        // The next install of that identity is refused as held, and changes
        // nothing, until the run lets the folder go.
        let refused =
            installing(&Arc::new(ShimFs::new()), root, &first(), &serving_both()).unwrap_err();
        assert_eq!(refused, InstallRefused::Held, "busy {busy}");
        assert_eq!(reading(root), Current::NotInstalled, "busy {busy}");
        assert!(folder.exists());
        drop(hold);
        assert_eq!(installed(root, &first()), record, "busy {busy}");
        assert_eq!(reading(root), Current::Installed(record.clone()));
        is_whole(root, &record);
        assert_eq!(installations(root), [record.identity().hex()]);
    }
}

#[test]
fn one_install_of_a_model_runs_at_a_time_and_the_other_is_busy_and_touches_nothing() {
    let scratch = Scratch::new("publish-busy");
    let root = scratch.path();
    let before = installed(root, &first());
    // Another has the model's lock, with files staged.
    let other = Arc::new(ShimFs::new());
    let staged = stage(
        &ModelsDir::at(other, root),
        &second(),
        &serving_both(),
        &Cancellation::new(),
        &none(),
    )
    .unwrap();
    let names = names_under(root);
    let fake = serving_both();
    let fs = Arc::new(ShimFs::new());
    let refused = installing(&fs, root, &second(), &fake).unwrap_err();
    assert_eq!(refused, InstallRefused::Busy);
    assert!(fake.requests().is_empty());
    assert!(fs.events().is_empty());
    assert_eq!(names_under(root), names);
    assert_eq!(said(root), Some((State::Installed, 2, 2)));
    drop(staged);
    // What the other left in staging is removed by the install, which stages again.
    let after = installed(root, &second());
    assert_eq!(reading(root), Current::Installed(after));
    assert_eq!(installations(root).len(), 1);
    assert_ne!(installations(root), [before.identity().hex()]);
}

// --- a cancel, a secret, another file system ---------------------------------------------------

#[test]
fn a_cancel_before_the_staged_folder_is_renamed_ends_the_install_and_one_after_it_does_not() {
    let events = events_after(&[&first()], &second());
    let renamed = index_of(
        &events,
        |event| matches!(event, Event::Rename(_, to) if to.parent().is_some_and(|p| p.ends_with("installations"))),
    );
    // After the downloader handed over the last file, while the staged files are
    // read back: the install ends as cancelled, and nothing is published.
    let reading_back = index_of(&events, reads_staged) + 1;
    // From the rename on, the publication runs to its end.
    for (moment, cancelled) in [(reading_back, true), (renamed, false), (renamed + 1, false)] {
        let scratch = Scratch::new("publish-cancel");
        let root = scratch.path();
        let before = installed(root, &first());
        let fs = Arc::new(ShimFs::new());
        let cancellation = Cancellation::new();
        let cancel = cancellation.clone();
        fs.pause_before(moment, move || cancel.cancel());
        let dir = ModelsDir::at(fs.clone(), root);
        let ended = install(
            &dir,
            &second(),
            &adapter(),
            &serving_both(),
            &cancellation,
            &none(),
        );
        if cancelled {
            assert_eq!(ended.unwrap_err(), InstallRefused::Cancelled);
            assert_eq!(reading(root), Current::Installed(before.clone()));
            assert_eq!(installations(root), [before.identity().hex()]);
            assert_eq!(said(root), Some((State::Cancelled, 2, 2)));
            assert!(!incoming(root).exists());
        } else {
            let record = ended.unwrap();
            assert_eq!(reading(root), Current::Installed(record), "event {moment}");
            assert_eq!(said(root), Some((State::Installed, 2, 2)));
        }
    }
}

#[test]
fn a_record_that_would_spell_a_secret_is_not_installed_and_nothing_written_holds_one() {
    // The adapter's revision is the caller's word. Made by a constructor that was
    // told of no secret, it holds one the installer is told of: nothing is made,
    // asked or written.
    let scratch = Scratch::new("publish-secret");
    let root = scratch.path();
    let fs = Arc::new(ShimFs::new());
    let dir = ModelsDir::at(fs.clone(), root);
    let fake = serving_both();
    let refused = install(
        &dir,
        &first(),
        &revision(MARKER),
        &fake,
        &Cancellation::new(),
        &secrets(),
    )
    .unwrap_err();
    assert_eq!(refused, InstallRefused::HoldsSecret);
    assert!(!format!("{refused} {refused:?}").contains(&MARKER[..4]));
    assert_eq!(names_under(root), Vec::<String>::new());
    assert!(fs.events().is_empty());
    assert!(fake.requests().is_empty());
    // A model's name that holds one is refused the same way.
    let named = {
        let mut parts = parts(&format!("tiny-{MARKER}"));
        parts.files = first().files().to_vec();
        Entry::new(parts, &none()).unwrap()
    };
    let refused = install(
        &dir,
        &named,
        &adapter(),
        &fake,
        &Cancellation::new(),
        &secrets(),
    )
    .unwrap_err();
    assert_eq!(refused, InstallRefused::HoldsSecret);
    assert!(fs.events().is_empty());
    // An install told of the secret, of an entry that holds none, and one that
    // fails after it: no name and no byte under the directory holds the marker.
    install(
        &dir,
        &first(),
        &adapter(),
        &fake,
        &Cancellation::new(),
        &secrets(),
    )
    .unwrap();
    let short = FakeDownloader::new().serving(&at("LICENSE.txt"), TERMS.to_vec());
    install(
        &dir,
        &second(),
        &adapter(),
        &short,
        &Cancellation::new(),
        &secrets(),
    )
    .unwrap_err();
    for name in names_under(root) {
        assert!(!name.contains(MARKER), "{name}");
        if let Some(bytes) = on_disk(&root.join(&name)) {
            assert!(
                !bytes.windows(MARKER.len()).any(|w| w == MARKER.as_bytes()),
                "{name}"
            );
        }
    }
}

#[test]
fn a_staging_folder_on_another_file_system_is_a_refusal_and_not_damage() {
    // Another file system mounted at `staging/`: the rename of the staged folder
    // into `installations/` then crosses two. (A link at `staging/` is refused
    // before anything is staged: `links::`.)
    let scratch = Scratch::new("publish-devices");
    let root = scratch.path();
    let before = installed(root, &first());
    let fs = Arc::new(ShimFs::new().cross_device(&tiny(root).join("staging")));
    let refused = installing(&fs, root, &second(), &serving_both()).unwrap_err();
    assert_eq!(
        refused,
        InstallRefused::Disk(std::io::ErrorKind::CrossesDevices)
    );
    holds_no_path(&refused, root);
    assert_eq!(reading(root), Current::Installed(before.clone()));
    assert_eq!(installations(root), [before.identity().hex()]);
    assert!(!incoming(root).exists());
    assert_eq!(said(root), Some((State::Failed, 2, 2)));
}

// --- each step of the file system refused ------------------------------------------------------

/// Installs the second entry with each step of the file system refused in turn, over
/// a directory where `previous` was installed, or nothing was.
fn each_step_refused(previous: Option<Entry>) {
    let before: Vec<&Entry> = previous.iter().collect();
    let events = events_after(&before, &second()).len();
    assert!((40..140).contains(&events), "{events}");
    let next = record_of(&second());
    let mut ended_installed = 0;
    for event in 0..events {
        let scratch = Scratch::new("publish-refuse");
        let root = scratch.path();
        let was = match &previous {
            Some(entry) => Current::Installed(installed(root, entry)),
            None => Current::NotInstalled,
        };
        let fs = Arc::new(ShimFs::new().fail_before(event));
        let ended = installing(&fs, root, &second(), &serving_both());
        let pointed = fs
            .events()
            .iter()
            .any(|event| matches!(event, Event::Rename(_, to) if to.ends_with("CURRENT")));
        let now = reading(root);
        match &ended {
            // A step refused after the pointer was durable: the install is done.
            Ok(record) => {
                assert!(pointed, "event {event}");
                assert_eq!(now, Current::Installed(record.clone()), "event {event}");
                ended_installed += 1;
            }
            Err(refused) => {
                assert!(
                    matches!(refused, InstallRefused::Disk(_)),
                    "event {event}: {refused:?}"
                );
                holds_no_path(refused, root);
                // Refused before the pointer was replaced, the model is as it was.
                // The one step after that is the sync that makes it durable: the
                // install is refused, and the pointer names the new installation.
                let expected = if pointed {
                    Current::Installed(next.clone())
                } else {
                    was.clone()
                };
                assert_eq!(now, expected, "event {event}");
            }
        }
        // What the pointer names is whole, whichever it is.
        if let Current::Installed(record) = &now {
            is_whole(root, record);
        }
        // The next install removes what was left, and ends with the one
        // installation, whole.
        assert_eq!(installed(root, &second()), next, "event {event}");
        assert_eq!(reading(root), Current::Installed(next.clone()));
        is_whole(root, &next);
        assert_eq!(
            installations(root),
            [next.identity().hex()],
            "event {event}"
        );
        assert!(!incoming(root).exists());
        assert_eq!(said(root), Some((State::Installed, 2, 2)));
    }
    // The steps after the pointer is durable are the last status and the deleting
    // of what was superseded: a few, and none of the steps before.
    assert!((1..12).contains(&ended_installed), "{ended_installed}");
}

#[test]
fn each_step_of_the_file_system_refused_leaves_a_first_install_not_installed_or_whole() {
    each_step_refused(None);
}

#[test]
fn each_step_of_the_file_system_refused_leaves_the_previous_installation_or_the_new_one_whole() {
    each_step_refused(Some(first()));
}

#[test]
fn a_hold_file_that_was_made_and_never_synced_is_synced_by_the_next_to_publish_its_identity() {
    // The first install is refused the sync of `holds/`, with its hold file made.
    // The second finds the file there. Its pointer is durable only after the hold
    // file's name is: a crash that loses every name that was not synced leaves both.
    let events = events_after(&[], &first());
    let scratch = Scratch::new("publish-hold-synced");
    let root = scratch.path();
    let holds = tiny(root).join("holds");
    let at = index_of(
        &events,
        |event| matches!(event, Event::SyncDir(p) if p.ends_with("holds")),
    );
    let fs = Arc::new(ShimFs::new().fail_before(at));
    installing(&fs, root, &first(), &serving_both()).unwrap_err();
    let record = record_of(&first());
    assert!(hold_file_of(root, &record).exists());
    fs.crash(Lost::ContentOnly);
    let fs = Arc::new(ShimFs::new());
    assert_eq!(
        installing(&fs, root, &first(), &serving_both()).unwrap(),
        record
    );
    assert!(fs.events().contains(&Event::SyncDir(holds)));
    fs.crash(Lost::Unsynced);
    assert_eq!(reading(root), Current::Installed(record.clone()));
    assert!(hold_file_of(root, &record).exists());
}

// --- nothing on the word of a name that is not durable ----------------------------------------

/// The ways a crash loses names, for the tests of a refused sync and then a crash.
const LOSSES: [Lost; 4] = [
    Lost::Unsynced,
    Lost::ContentOnly,
    Lost::Names(0b0101),
    Lost::Names(0b1010),
];

/// Installs the second entry with the sync that makes its pointer durable refused,
/// over a directory prepared by `over`; says what the pointer was, and gives the
/// disk it was done on. With `twice`, the next sync of the model's folder is
/// refused too: the one that comes before anything is deleted.
fn pointer_sync_refused(
    root: &Path,
    over: bool,
    twice: bool,
    tail: Tail,
) -> (Current, Arc<ShimFs>) {
    let before: Vec<Entry> = over.then(first).into_iter().collect();
    let clean = events_after(&before.iter().collect::<Vec<_>>(), &second());
    let synced = index_of(
        &clean,
        |event| matches!(event, Event::Rename(_, to) if to.ends_with("CURRENT")),
    ) + 1;
    let was = match before.first() {
        Some(entry) => Current::Installed(installed(root, entry)),
        None => Current::NotInstalled,
    };
    let fs = Arc::new(ShimFs::new().with_tail(tail).fail_before(synced));
    if twice {
        let (again, folder) = (fs.clone(), tiny(root));
        fs.pause_before(synced, move || {
            again.refuse_matching(0, move |event| *event == Event::SyncDir(folder.clone()));
        });
    }
    let refused = installing(&fs, root, &second(), &serving_both()).unwrap_err();
    assert!(matches!(refused, InstallRefused::Disk(_)), "{refused:?}");
    // The rename has happened: the pointer names the new installation, not durably.
    assert_eq!(reading(root), Current::Installed(record_of(&second())));
    (was, fs)
}

const TAILS: [Tail; 4] = [Tail::Half, Tail::Zeroed, Tail::All, Tail::None];

#[test]
fn the_pointers_sync_refused_and_then_a_crash_leaves_the_old_installation_or_the_new_one_whole() {
    let next = Current::Installed(record_of(&second()));
    for (over, twice, tail) in [true, false]
        .into_iter()
        .flat_map(|over| [true, false].map(|twice| (over, twice)))
        .flat_map(|(over, twice)| TAILS.map(|tail| (over, twice, tail)))
    {
        for lost in LOSSES {
            let at = format!("over {over}, twice {twice}, {tail:?}, {lost:?}");
            let scratch = Scratch::new("publish-sync-crash");
            let root = scratch.path();
            let (was, fs) = pointer_sync_refused(root, over, twice, tail);
            // While the pointer is not durable, the installation it named before is
            // not deleted.
            if twice && over {
                assert_eq!(installations(root).len(), 2, "{at}");
            }
            fs.crash(lost);
            let now = reading(root);
            assert!(now == was || now == next, "{at}: {now:?}");
            if let Current::Installed(record) = &now {
                is_whole(root, record);
                assert!(hold_file_of(root, record).exists(), "{at}");
            }
            // The next install ends with the one installation, whole.
            assert_eq!(installed(root, &second()), record_of(&second()), "{at}");
            assert_eq!(reading(root), next, "{at}");
            assert_eq!(installations(root), [record_of(&second()).identity().hex()]);
        }
    }
}

#[test]
fn asking_again_after_the_pointers_sync_was_refused_makes_the_pointer_durable_before_it_answers() {
    let next = record_of(&second());
    for (over, twice, tail) in [true, false]
        .into_iter()
        .flat_map(|over| [true, false].map(|twice| (over, twice)))
        .flat_map(|(over, twice)| TAILS.map(|tail| (over, twice, tail)))
    {
        let at = format!("over {over}, twice {twice}, {tail:?}");
        let scratch = Scratch::new("publish-sync-retry");
        let root = scratch.path();
        let (_, fs) = pointer_sync_refused(root, over, twice, tail);
        // The same process asks again. The installation is on the disk, whole and
        // named: nothing is downloaded, and the answer is given only after the
        // pointer was synced.
        let fake = serving_both();
        assert_eq!(
            installing(&fs, root, &second(), &fake).unwrap(),
            next,
            "{at}"
        );
        assert!(fake.requests().is_empty(), "{at}");
        fs.crash(Lost::Unsynced);
        assert_eq!(reading(root), Current::Installed(next.clone()), "{at}");
        is_whole(root, &next);
        assert!(hold_file_of(root, &next).exists(), "{at}");
        assert_eq!(installations(root), [next.identity().hex()], "{at}");
    }
}

#[test]
fn an_install_answers_only_when_the_folders_that_name_the_model_are_durable() {
    // A first install is refused the sync that makes `models/` durable, or the one
    // that makes the model's folder durable; both folders are there. Asked again,
    // the install finds them made. It answers only after their names were synced: a
    // crash that loses every name that was not leaves the model installed.
    let clean = events_after(&[], &second());
    let syncs: Vec<usize> = (0..clean.len())
        .filter(|at| matches!(clean[*at], Event::SyncDir(_)))
        .take(2)
        .collect();
    assert_eq!(syncs, [1, 3]);
    let next = record_of(&second());
    for refused in syncs {
        let scratch = Scratch::new("publish-names");
        let root = scratch.path();
        let fs = Arc::new(ShimFs::new().fail_before(refused));
        installing(&fs, root, &second(), &serving_both()).unwrap_err();
        assert_eq!(
            installing(&fs, root, &second(), &serving_both()).unwrap(),
            next
        );
        fs.crash(Lost::Unsynced);
        assert_eq!(reading(root), Current::Installed(next.clone()), "{refused}");
        is_whole(root, &next);
    }
}

// --- an installation's files verified before it is loaded (plan 1C, task P6b) -----------------

#[test]
fn an_installation_is_verified_by_its_identity_whatever_the_pointer_names_and_a_changed_file_is_not_whole()
 {
    let scratch = Scratch::new("publish-verify");
    let root = scratch.path();
    let dir = ModelsDir::at(Arc::new(RealFs), root);
    let old = installed(root, &first());
    let id = old.identity();
    assert!(verify(&dir, &model("tiny"), id, &none()).unwrap());
    // A byte of a file changed, its length kept: not whole. Put back: whole again.
    let file = folder_of(root, &old).join("files").join("graph.onnx");
    flip(&file, 7);
    assert!(!verify(&dir, &model("tiny"), id, &none()).unwrap());
    flip(&file, 7);
    assert!(verify(&dir, &model("tiny"), id, &none()).unwrap());
    // A file more in the installation's folder: not whole.
    let more = folder_of(root, &old).join("files").join("more.bin");
    std::fs::write(&more, b"more").unwrap();
    assert!(!verify(&dir, &model("tiny"), id, &none()).unwrap());
    std::fs::remove_file(&more).unwrap();
    // A superseded installation a hold keeps on the disk is read by its identity, as
    // it is, though the pointer names another; one never installed has no record.
    let held = held(root, &old);
    let new = installed(root, &second());
    assert_eq!(reading(root), Current::Installed(new.clone()));
    assert!(verify(&dir, &model("tiny"), id, &none()).unwrap());
    assert!(verify(&dir, &model("tiny"), new.identity(), &none()).unwrap());
    drop(held);
    let never = record_of(&entry_at("2026-11.9")).identity();
    assert!(!verify(&dir, &model("tiny"), never, &none()).unwrap());
    // The identity of one model's installation names nothing under another model.
    assert!(!verify(&dir, &model("other"), new.identity(), &none()).unwrap());
}

#[test]
#[ignore = "a measurement, not a check: a quarter of a gigabyte written and read"]
fn measure_what_verifying_an_installation_costs() {
    // Decision D14 of plan 1C asks for the cost of verifying an installation's files
    // before it is loaded. One file of 256 MiB, installed and then verified three
    // times; run with `--run-ignored only --no-capture`, in the profile to measure.
    use std::time::Instant;
    let size: usize = 256 << 20;
    let content: Vec<u8> = (0..size).map(|at| (at % 251) as u8).collect();
    let entry = entry_of("2026-11.1", &[("graph.onnx", content.clone())]);
    let fake = FakeDownloader::new().serving(&at("graph.onnx"), content);
    let scratch = Scratch::new("publish-measure");
    let root = scratch.path();
    let record = installing(&Arc::new(ShimFs::new()), root, &entry, &fake).unwrap();
    let dir = ModelsDir::at(Arc::new(RealFs), root);
    for round in 0..3 {
        let begun = Instant::now();
        assert!(verify(&dir, &model("tiny"), record.identity(), &none()).unwrap());
        let took = begun.elapsed();
        let rate = 256.0 / took.as_secs_f64();
        println!("round {round}: 256 MiB verified in {took:?}, {rate:.0} MiB a second");
    }
}
