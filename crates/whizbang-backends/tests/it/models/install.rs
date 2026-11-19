//! Files shared between installations (ADR 0017, "A file shared between installations";
//! plan 1D, task Q1): an install that shares links a file another installation holds and
//! downloads nothing for it, and two installs that stage one file at once download it
//! once. Every download is the test kit's fake; the disk is the real one under a
//! scratch folder, since a link is a link of the real disk.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use whizbang_backends::models::{
    Carrying, CatalogueFile, Current, Entry, InstallRefused, InstallationRecord, ModelsDir,
    all_held, current, install_carrying, verify,
};
use whizbang_core::contract::{Cancellation, Clock};
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::downloads::FakeDownloader;
use whizbang_testkit::scratch::Scratch;
use whizbang_testkit::threads;

use super::publishing::adapter;
use super::staging::{TERMS, at, graph, serving_both};
use super::{file_name, hash, model, none, parts, revision};

/// The entry of this model at this revision of the catalogue, of the two files every
/// entry here has: the same bytes, so that each installation of them can share them.
pub(super) fn entry_of(name: &str, catalogue: &str) -> Entry {
    let mut parts = parts(name);
    parts.revision = revision(catalogue);
    parts.files = [("graph.onnx", graph()), ("LICENSE.txt", TERMS.to_vec())]
        .iter()
        .map(|(file, content)| {
            let size = content.len() as u64;
            CatalogueFile::new(file_name(file), size, hash(content), at(file), &none()).unwrap()
        })
        .collect();
    Entry::new(parts, &none()).unwrap()
}

/// A clock that notes when it is asked to let the pace of a shared file's lock pass:
/// the moment an install is seen waiting for another's lock.
struct Waits(Arc<AtomicBool>);

impl Clock for Waits {
    fn now_ms(&self) -> u64 {
        0
    }

    fn pause(&self, span: Duration) {
        if span == Duration::from_millis(10) {
            self.0.store(true, Ordering::SeqCst);
        }
        std::thread::yield_now();
    }
}

/// Installs, sharing, on the real disk, in a directory opened with a clock that says
/// when the install waits for the lock of a shared file.
fn sharing_seen(
    root: &Path,
    entry: &Entry,
    fake: &FakeDownloader,
    waiting: &Arc<AtomicBool>,
) -> Result<InstallationRecord, InstallRefused> {
    let dir = ModelsDir::open(Arc::new(RealFs), root, Arc::new(Waits(waiting.clone()))).unwrap();
    let carrying = Carrying {
        downloader: fake,
        credential: None,
        shares: true,
    };
    install_carrying(
        &dir,
        entry,
        &adapter(),
        &carrying,
        &Cancellation::new(),
        &none(),
    )
}

/// Waits, up to ten seconds, until the flag is set.
fn seen(flag: &AtomicBool) {
    let until = Instant::now() + Duration::from_secs(10);
    while !flag.load(Ordering::SeqCst) {
        assert!(
            Instant::now() < until,
            "the second install was not seen waiting"
        );
        std::thread::yield_now();
    }
}

/// Installs, sharing, on the real disk.
pub(super) fn sharing(
    root: &Path,
    entry: &Entry,
    fake: &FakeDownloader,
) -> Result<InstallationRecord, InstallRefused> {
    let dir = ModelsDir::at(Arc::new(RealFs), root);
    let carrying = Carrying {
        downloader: fake,
        credential: None,
        shares: true,
    };
    install_carrying(
        &dir,
        entry,
        &adapter(),
        &carrying,
        &Cancellation::new(),
        &none(),
    )
}

/// Where a file of an installation is.
pub(super) fn file_of(root: &Path, record: &InstallationRecord, name: &str) -> PathBuf {
    root.join("models")
        .join(record.model().as_str())
        .join("installations")
        .join(record.identity().hex())
        .join("files")
        .join(name)
}

fn inode(path: &Path) -> u64 {
    std::fs::metadata(path).unwrap().ino()
}

fn whole(root: &Path, record: &InstallationRecord) -> bool {
    let dir = ModelsDir::at(Arc::new(RealFs), root);
    verify(&dir, record.model(), record.identity(), &none(), &|| false).unwrap()
}

#[test]
fn a_file_already_held_by_another_installation_is_linked_and_not_downloaded() {
    let scratch = Scratch::new("shared-linked");
    let root = scratch.path();
    let fake = serving_both();
    let tiny = sharing(root, &entry_of("tiny", "2026-11.1"), &fake).unwrap();
    assert_eq!(fake.requests().len(), 2);
    // Another model of the same files: nothing is asked of any address, and each file
    // is one file under two names, whole under both.
    let nothing = FakeDownloader::new();
    let dir = ModelsDir::at(Arc::new(RealFs), root);
    assert!(all_held(&dir, &entry_of("other", "2026-11.1"), &none()).unwrap());
    let other = sharing(root, &entry_of("other", "2026-11.1"), &nothing).unwrap();
    assert!(nothing.requests().is_empty());
    for name in ["graph.onnx", "LICENSE.txt"] {
        assert_eq!(
            inode(&file_of(root, &tiny, name)),
            inode(&file_of(root, &other, name))
        );
    }
    assert!(whole(root, &tiny) && whole(root, &other));
    assert_eq!(
        current(&dir, &model("other"), &none()).unwrap(),
        Current::Installed(other.clone())
    );
    // A new revision of the same model links from the installation it replaces, whose
    // folder the install then deletes: the file lives on under the new name.
    let next = sharing(root, &entry_of("tiny", "2026-11.2"), &nothing).unwrap();
    assert!(nothing.requests().is_empty());
    assert!(!file_of(root, &tiny, "graph.onnx").exists());
    assert!(whole(root, &next));
    // A held copy that is not whole is not linked: the file is downloaded. The copies
    // are one file, so the damage is made where nothing shares it: a third model alone.
    let third = sharing(root, &entry_of("third", "2026-11.1"), &serving_both());
    let third = third.unwrap();
    for record in [&next, &other] {
        let held = file_of(root, record, "graph.onnx");
        std::fs::remove_file(&held).unwrap();
        let mut damaged = graph();
        damaged[7] ^= 1;
        std::fs::write(&held, damaged).unwrap();
    }
    std::fs::remove_file(file_of(root, &third, "graph.onnx")).unwrap();
    std::fs::write(file_of(root, &third, "graph.onnx"), b"short").unwrap();
    let fake = serving_both();
    let fourth = sharing(root, &entry_of("fourth", "2026-11.1"), &fake).unwrap();
    assert_eq!(fake.asked(&at("graph.onnx")), 1);
    assert_eq!(fake.asked(&at("LICENSE.txt")), 0);
    assert!(whole(root, &fourth));
    // Without sharing, nothing is linked: every file is downloaded.
    let fake = serving_both();
    let dir = ModelsDir::at(Arc::new(RealFs), root);
    let carrying = Carrying {
        downloader: &fake,
        credential: None,
        shares: false,
    };
    let entry = entry_of("fifth", "2026-11.1");
    install_carrying(
        &dir,
        &entry,
        &adapter(),
        &carrying,
        &Cancellation::new(),
        &none(),
    )
    .unwrap();
    assert_eq!(fake.requests().len(), 2);
}

#[test]
fn a_held_file_planted_as_a_link_is_never_linked_and_the_file_is_downloaded() {
    // The review of plan 1D, task Q1 (#182): a held file that is a link, to a file of
    // the same bytes outside the directory, is passed over, as a folder that is a link
    // is: nothing is followed, and the new installation's file is its own.
    let scratch = Scratch::new("shared-planted");
    let root = scratch.path();
    let tiny = sharing(root, &entry_of("tiny", "2026-11.1"), &serving_both()).unwrap();
    let outside = scratch.path().join("outside.onnx");
    std::fs::write(&outside, graph()).unwrap();
    let held = file_of(root, &tiny, "graph.onnx");
    std::fs::remove_file(&held).unwrap();
    std::os::unix::fs::symlink(&outside, &held).unwrap();
    let dir = ModelsDir::at(Arc::new(RealFs), root);
    assert!(!all_held(&dir, &entry_of("other", "2026-11.1"), &none()).unwrap());
    let fake = serving_both();
    let other = sharing(root, &entry_of("other", "2026-11.1"), &fake).unwrap();
    assert_eq!(fake.asked(&at("graph.onnx")), 1);
    assert_eq!(fake.asked(&at("LICENSE.txt")), 0);
    let made = file_of(root, &other, "graph.onnx");
    assert!(
        !std::fs::symlink_metadata(&made)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_ne!(inode(&made), inode(&outside));
    assert!(whole(root, &other));
}

#[test]
fn two_installs_sharing_a_file_at_once_download_it_once() {
    let scratch = Scratch::new("shared-once");
    let root = scratch.path().to_path_buf();
    let barriers = Barriers::new();
    // The first is stopped inside its first file, with the locks of both its files.
    let first = Arc::new(
        FakeDownloader::new()
            .serving(&at("LICENSE.txt"), TERMS.to_vec())
            .stopping_after(&at("graph.onnx"), graph(), 1024, barriers.clone(), "mid"),
    );
    let installer = {
        let (fake, root) = (first.clone(), root.clone());
        threads::spawn(move || sharing(&root, &entry_of("tiny", "2026-11.1"), &fake))
    };
    barriers.arrived("mid", 1);
    // The second, of another model with the same files, starts now: it waits for the
    // first's locks, and downloads nothing.
    let second = Arc::new(serving_both());
    let waiting = Arc::new(AtomicBool::new(false));
    let waiter = {
        let (fake, root, waiting) = (second.clone(), root.clone(), waiting.clone());
        threads::spawn(move || {
            sharing_seen(&root, &entry_of("other", "2026-11.1"), &fake, &waiting)
        })
    };
    // Let go only once the second is seen waiting for the first's locks.
    seen(&waiting);
    assert!(second.requests().is_empty());
    barriers.release("mid");
    let tiny = installer.join().unwrap().unwrap();
    let other = waiter.join().unwrap().unwrap();
    assert!(
        second.requests().is_empty(),
        "{:?}",
        second.requests().len()
    );
    assert_eq!(first.asked(&at("graph.onnx")), 1);
    assert_eq!(first.asked(&at("LICENSE.txt")), 1);
    assert_eq!(
        inode(&file_of(&root, &tiny, "graph.onnx")),
        inode(&file_of(&root, &other, "graph.onnx"))
    );
    // The lock files are where the layout says, one for each file, by its hash.
    let mut locks: Vec<String> = std::fs::read_dir(root.join("shared"))
        .unwrap()
        .map(|found| found.unwrap().file_name().into_string().unwrap())
        .collect();
    locks.sort();
    let mut hashes = vec![hash(&graph()).hex(), hash(TERMS).hex()];
    hashes.sort();
    assert_eq!(locks, hashes);

    // When the first is refused, the second downloads for itself once it has the
    // locks: a cancel of the first while the second waits.
    let scratch = Scratch::new("shared-refused");
    let root = scratch.path().to_path_buf();
    let barriers = Barriers::new();
    let cancellation = Cancellation::new();
    let first = Arc::new(
        FakeDownloader::new()
            .serving(&at("LICENSE.txt"), TERMS.to_vec())
            .stopping_after(&at("graph.onnx"), graph(), 1024, barriers.clone(), "mid"),
    );
    let installer = {
        let (fake, root, cancellation) = (first.clone(), root.clone(), cancellation.clone());
        threads::spawn(move || {
            let dir = ModelsDir::at(Arc::new(RealFs), &root);
            let carrying = Carrying {
                downloader: &*fake,
                credential: None,
                shares: true,
            };
            let entry = entry_of("tiny", "2026-11.1");
            install_carrying(&dir, &entry, &adapter(), &carrying, &cancellation, &none())
        })
    };
    barriers.arrived("mid", 1);
    let second = Arc::new(serving_both());
    let waiting = Arc::new(AtomicBool::new(false));
    let waiter = {
        let (fake, root, waiting) = (second.clone(), root.clone(), waiting.clone());
        threads::spawn(move || {
            sharing_seen(&root, &entry_of("other", "2026-11.1"), &fake, &waiting)
        })
    };
    seen(&waiting);
    cancellation.cancel();
    barriers.release("mid");
    assert_eq!(installer.join().unwrap(), Err(InstallRefused::Cancelled));
    let other = waiter.join().unwrap().unwrap();
    assert_eq!(second.requests().len(), 2);
    assert!(whole(&root, &other));
}
