//! Files shared between installations (ADR 0017, "A file shared between installations";
//! plan 1D, task Q1): an install that shares links a file another installation holds and
//! downloads nothing for it, and two installs that stage one file at once download it
//! once. Every download is the test kit's fake; the disk is the real one under a
//! scratch folder, since a link is a link of the real disk.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use whizbang_backends::models::{
    Carrying, CatalogueFile, Current, Entry, InstallRefused, InstallationRecord, ModelsDir,
    all_held, current, install_carrying, verify,
};
use whizbang_core::contract::Cancellation;
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
    let waiter = {
        let (fake, root) = (second.clone(), root.clone());
        threads::spawn(move || sharing(&root, &entry_of("other", "2026-11.1"), &fake))
    };
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
    let waiter = {
        let (fake, root) = (second.clone(), root.clone());
        threads::spawn(move || sharing(&root, &entry_of("other", "2026-11.1"), &fake))
    };
    cancellation.cancel();
    barriers.release("mid");
    assert_eq!(installer.join().unwrap(), Err(InstallRefused::Cancelled));
    let other = waiter.join().unwrap().unwrap();
    assert_eq!(second.requests().len(), 2);
    assert!(whole(&root, &other));
}
