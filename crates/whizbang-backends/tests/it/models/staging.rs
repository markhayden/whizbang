//! Downloading into staging (ADR 0017, "Downloading into staging"): each file of a
//! catalogue's entry is downloaded under the model's lock into one staging folder,
//! stopped at its stated size and checked by its length and its hash. Every download is
//! the test kit's fake, a few kilobytes from memory; the disk is the test kit's shim
//! over a scratch folder.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use whizbang_backends::models::{
    CatalogueFile, Entry, FileRefused, MOVES_LIMIT, ModelsDir, StageRefused, Staged, stage,
};
use whizbang_core::capability::{Fault, FaultKind};
use whizbang_core::contract::Cancellation;
use whizbang_core::egress::{Address, DownloadRequest, Downloaded, Downloader};
use whizbang_core::store::fs::{Fs, RealFs};
use whizbang_testkit::downloads::{Ended, FakeDownloader};
use whizbang_testkit::fs_shim::{Event, Lost, ShimFs, Tail};
use whizbang_testkit::scratch::Scratch;

use super::{MARKER, file_name, hash, none, origin, parts, secrets};

/// 2,500 bytes, three pieces of the fake's, different at every place a piece starts.
fn graph() -> Vec<u8> {
    (0..2500u32).map(|at| (at % 251) as u8).collect()
}

const TERMS: &[u8] = b"terms";

fn address(text: &str) -> Address {
    Address::of(text, &none()).unwrap()
}

fn at(name: &str) -> Address {
    address(&format!("https://weights.example.org/files/{name}"))
}

/// The entry `tiny`: `graph.onnx` of 2,500 bytes and `LICENSE.txt` of 5.
fn entry() -> Entry {
    entry_of(&[("graph.onnx", graph()), ("LICENSE.txt", TERMS.to_vec())])
}

/// An entry of these files, each stated with the size and the hash of this content.
fn entry_of(files: &[(&str, Vec<u8>)]) -> Entry {
    let mut parts = parts("tiny");
    parts.origins.push(origin("https://mirror.example.net"));
    parts.files = files
        .iter()
        .map(|(name, content)| {
            let size = content.len() as u64;
            CatalogueFile::new(file_name(name), size, hash(content), at(name), &none()).unwrap()
        })
        .collect();
    Entry::new(parts, &none()).unwrap()
}

/// A fake that serves both files as the entry states them.
fn serving_both() -> FakeDownloader {
    FakeDownloader::new()
        .serving(&at("graph.onnx"), graph())
        .serving(&at("LICENSE.txt"), TERMS.to_vec())
}

/// Where a staged file is, by the layout of ADR 0017.
fn staged_at(root: &Path, name: &str) -> PathBuf {
    incoming(root).join("files").join(name)
}

fn incoming(root: &Path) -> PathBuf {
    root.join("models/tiny/staging/incoming")
}

fn staging(
    fs: &Arc<ShimFs>,
    root: &Path,
    entry: &Entry,
    fake: &FakeDownloader,
) -> Result<Staged, StageRefused> {
    let dir = ModelsDir::at(fs.clone(), root);
    stage(&dir, entry, fake, &Cancellation::new(), &none())
}

/// What a file holds, read without the shim; nothing when it is not there.
fn on_disk(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

/// Every name under a folder, as text, for a search of what was written.
fn names_under(root: &Path) -> Vec<String> {
    let mut names = Vec::new();
    let mut left = vec![root.to_path_buf()];
    while let Some(dir) = left.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            names.push(
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            );
            if path.is_dir() && !path.is_symlink() {
                left.push(path);
            }
        }
    }
    names.sort();
    names
}

fn file_refused(name: &str, why: FileRefused) -> StageRefused {
    StageRefused::File {
        name: name.to_owned(),
        why,
    }
}

/// A refusal holds no path: nothing of the scratch folder, no separator, one short
/// line of printable ASCII.
fn holds_no_path(refused: &StageRefused, root: &Path) {
    let printed = format!("{refused} {refused:?}");
    assert!(
        printed.bytes().all(|b| b.is_ascii_graphic() || b == b' '),
        "{printed:?}"
    );
    assert!(printed.len() <= 400);
    assert!(!printed.contains(&*root.to_string_lossy()), "{printed}");
    for part in ["staging", "incoming", "models/", "/files"] {
        assert!(!printed.contains(part), "{printed}");
    }
}

// --- two files, staged and verified ------------------------------------------------------------

#[test]
fn two_files_are_staged_and_verified() {
    let scratch = Scratch::new("stage-two");
    let fs = Arc::new(ShimFs::new());
    let fake = serving_both();
    let staged = staging(&fs, scratch.path(), &entry(), &fake).unwrap();
    assert_eq!(staged.model().as_str(), "tiny");
    // What was staged, in the order of the names: each with its size and its hash.
    let files: Vec<_> = staged
        .files()
        .iter()
        .map(|file| (file.name().as_str().to_owned(), file.size(), file.sha256()))
        .collect();
    assert_eq!(
        files,
        [
            ("LICENSE.txt".to_owned(), 5, hash(TERMS)),
            ("graph.onnx".to_owned(), 2500, hash(&graph())),
        ]
    );
    // And on the disk, where the layout says, whole.
    assert_eq!(
        on_disk(&staged_at(scratch.path(), "graph.onnx")).unwrap(),
        graph()
    );
    assert_eq!(
        on_disk(&staged_at(scratch.path(), "LICENSE.txt")).unwrap(),
        TERMS
    );
    assert_eq!(
        names_under(scratch.path()),
        [
            "models",
            "models/tiny",
            "models/tiny/LOCK",
            "models/tiny/staging",
            "models/tiny/staging/incoming",
            "models/tiny/staging/incoming/files",
            "models/tiny/staging/incoming/files/LICENSE.txt",
            "models/tiny/staging/incoming/files/graph.onnx",
        ]
    );
    // Each file was written as a stream, synced and finished, and its folder synced:
    // what is staged is durable.
    let events = fs.events();
    for name in ["graph.onnx", "LICENSE.txt"] {
        let path = staged_at(scratch.path(), name);
        let of_file: Vec<&Event> = events
            .iter()
            .filter(|event| matches!(event, Event::Begin(p) | Event::Append(p) | Event::SyncFile(p) | Event::Finish(p) if *p == path))
            .collect();
        assert_eq!(
            of_file.first(),
            Some(&&Event::Begin(path.clone())),
            "{name}"
        );
        assert_eq!(
            of_file.last(),
            Some(&&Event::Finish(path.clone())),
            "{name}"
        );
        assert_eq!(
            of_file[of_file.len() - 2],
            &Event::SyncFile(path.clone()),
            "{name}"
        );
    }
    let files_dir = incoming(scratch.path()).join("files");
    let last_finish = events
        .iter()
        .rposition(|e| matches!(e, Event::Finish(_)))
        .unwrap();
    assert!(events[last_finish..].contains(&Event::SyncDir(files_dir)));
    // Nothing was written through the whole-file call: a model's file is never held
    // in memory whole.
    assert!(!events.iter().any(|event| matches!(event, Event::Write(_))));
    fs.crash(Lost::Unsynced);
    assert_eq!(
        on_disk(&staged_at(scratch.path(), "graph.onnx")).unwrap(),
        graph()
    );
    assert_eq!(
        on_disk(&staged_at(scratch.path(), "LICENSE.txt")).unwrap(),
        TERMS
    );
}

#[test]
fn a_download_asks_for_one_byte_beyond_the_stated_size_and_is_handed_exactly_the_size() {
    let scratch = Scratch::new("stage-count");
    let fs = Arc::new(ShimFs::new());
    let fake = serving_both();
    staging(&fs, scratch.path(), &entry(), &fake).unwrap();
    let asked: Vec<_> = fake
        .requests()
        .iter()
        .map(|asked| {
            (
                asked.from().clone(),
                asked.most_bytes(),
                asked.handed(),
                asked.ended(),
            )
        })
        .collect();
    // One request for each file, in the order of the names; no credential.
    assert_eq!(
        asked,
        [
            (at("LICENSE.txt"), 6, 5, Ended::Whole),
            (at("graph.onnx"), 2501, 2500, Ended::Whole),
        ]
    );
    assert!(
        fake.requests()
            .iter()
            .all(|asked| asked.headers().is_empty())
    );
    // The same of a downloader that does not keep to what it was asked, serving the
    // file as it is stated: nothing beyond the size comes, and nothing is refused.
    let scratch = Scratch::new("stage-count");
    let fake = FakeDownloader::new()
        .serving_too_many(&at("graph.onnx"), graph())
        .serving_too_many(&at("LICENSE.txt"), TERMS.to_vec());
    staging(&Arc::new(ShimFs::new()), scratch.path(), &entry(), &fake).unwrap();
    assert_eq!(fake.handed(&at("graph.onnx")), 2500);
}

// --- a file that is not what the catalogue states ----------------------------------------------

/// Stages the entry with `graph.onnx` served as these bytes, by a downloader that
/// keeps to what it is asked or one that does not, and gives the refusal, having
/// looked that nothing staged is left.
fn refusal_of_graph(served: Vec<u8>, too_many: bool) -> (StageRefused, FakeDownloader) {
    let scratch = Scratch::new("stage-bad");
    let fs = Arc::new(ShimFs::new());
    let fake = FakeDownloader::new().serving(&at("LICENSE.txt"), TERMS.to_vec());
    let fake = if too_many {
        fake.serving_too_many(&at("graph.onnx"), served)
    } else {
        fake.serving(&at("graph.onnx"), served)
    };
    let refused = staging(&fs, scratch.path(), &entry(), &fake).unwrap_err();
    holds_no_path(&refused, scratch.path());
    // `LICENSE.txt` came first and passed; it is gone with the rest.
    assert_eq!(fake.requests()[0].ended(), Ended::Whole);
    assert_eq!(
        names_under(scratch.path()),
        [
            "models",
            "models/tiny",
            "models/tiny/LOCK",
            "models/tiny/staging"
        ]
    );
    // And gone durably: the removal's name was synced, so a crash that undoes every
    // name that was not brings nothing of the staging folder back.
    fs.crash(Lost::Unsynced);
    assert!(!incoming(scratch.path()).exists());
    (refused, fake)
}

#[test]
fn a_wrong_hash_fails_the_download_and_leaves_no_staged_file_that_passed() {
    // The right length, one byte another: at the start, in the middle, at the end.
    for place in [0, 1250, 2499] {
        let mut served = graph();
        served[place] ^= 1;
        let (refused, _) = refusal_of_graph(served, false);
        assert_eq!(
            refused,
            file_refused("graph.onnx", FileRefused::WrongHash),
            "{place}"
        );
    }
}

#[test]
fn a_short_file_fails_the_download_and_leaves_no_staged_file_that_passed() {
    for length in [0, 1, 1024, 2499] {
        let (refused, _) = refusal_of_graph(graph()[..length].to_vec(), false);
        assert_eq!(
            refused,
            file_refused("graph.onnx", FileRefused::TooShort),
            "{length}"
        );
    }
}

#[test]
fn a_file_one_byte_too_long_fails_the_download_and_leaves_no_staged_file_that_passed() {
    // The stated file with one byte after it: its first 2,500 bytes have the right
    // hash, and a download that asked for the size alone would take it for whole.
    let mut longer = graph();
    longer.push(7);
    let (refused, fake) = refusal_of_graph(longer.clone(), false);
    assert_eq!(refused, file_refused("graph.onnx", FileRefused::TooLong));
    // The byte beyond was handed over, and the sink answered no to it.
    let asked = &fake.requests()[1];
    assert_eq!(
        (asked.most_bytes(), asked.handed(), asked.ended()),
        (2501, 2501, Ended::Stopped)
    );
    // From a downloader that hands over whatever it has, however much: the sink stops
    // it at the piece that passes the size.
    let mut much_longer = graph();
    much_longer.extend(std::iter::repeat_n(7, 5000));
    for served in [longer, much_longer] {
        let (refused, fake) = refusal_of_graph(served, true);
        assert_eq!(refused, file_refused("graph.onnx", FileRefused::TooLong));
        let asked = &fake.requests()[1];
        assert_eq!(asked.ended(), Ended::Stopped);
        assert!(asked.handed() <= 2500 + 1024, "{}", asked.handed());
    }
}

#[test]
fn a_refusal_names_the_file_by_its_catalogue_name_escaped_and_cut_with_no_path() {
    // A name longer than what is shown: the refusal holds its first 64 characters and
    // three dots.
    let long = format!("{}.onnx", "g".repeat(100));
    let entry = entry_of(&[(&long, graph())]);
    let scratch = Scratch::new("stage-name");
    let fake = FakeDownloader::new().serving(&at(&long), b"short".to_vec());
    let refused = staging(&Arc::new(ShimFs::new()), scratch.path(), &entry, &fake).unwrap_err();
    assert_eq!(
        refused,
        file_refused(&format!("{}...", "g".repeat(64)), FileRefused::TooShort)
    );
    holds_no_path(&refused, scratch.path());
    assert!(!format!("{refused}").contains(&long));
    // Every refusal says one sentence of fixed words.
    let said = |refused: StageRefused| refused.to_string();
    assert_eq!(
        said(StageRefused::HoldsSecret),
        "a name holds a secret, and is not shown"
    );
    assert_eq!(
        said(StageRefused::Busy),
        "the model is being installed or removed by another"
    );
    assert_eq!(said(StageRefused::Cancelled), "the download was cancelled");
    assert_eq!(
        said(StageRefused::Disk(std::io::ErrorKind::PermissionDenied)),
        "the models directory could not be written: permission is denied"
    );
    assert_eq!(
        said(StageRefused::Disk(std::io::ErrorKind::StorageFull)),
        "the models directory could not be written: the disk is full"
    );
    assert_eq!(
        said(StageRefused::Disk(std::io::ErrorKind::Other)),
        "the models directory could not be written: the system refused"
    );
    let of_file = [
        (
            FileRefused::TooShort,
            "is shorter than the catalogue states",
        ),
        (FileRefused::TooLong, "is longer than the catalogue states"),
        (
            FileRefused::WrongHash,
            "does not have the hash the catalogue states",
        ),
        (
            FileRefused::Elsewhere,
            "moved to an origin its model does not name",
        ),
        (FileRefused::TooManyMoves, "moved more than 5 times"),
        (
            FileRefused::Download(FaultKind::Unavailable),
            "could not be downloaded: unavailable",
        ),
    ];
    for (why, sentence) in of_file {
        assert_eq!(
            said(file_refused("graph.onnx", why)),
            format!("the file `graph.onnx` {sentence}")
        );
    }
}

// --- a file that moved, a fault, a cancellation ------------------------------------------------

#[test]
fn a_file_that_moved_is_asked_for_only_at_an_origin_the_entry_names() {
    let entry = entry_of(&[("graph.onnx", graph())]);
    // To an origin the entry names: followed, and staged.
    let mirror = address("https://mirror.example.net/g?sig=abc");
    let scratch = Scratch::new("stage-moved");
    let fake = FakeDownloader::new()
        .moved(&at("graph.onnx"), mirror.clone())
        .serving(&mirror, graph());
    staging(&Arc::new(ShimFs::new()), scratch.path(), &entry, &fake).unwrap();
    assert_eq!(
        on_disk(&staged_at(scratch.path(), "graph.onnx")).unwrap(),
        graph()
    );
    assert_eq!((fake.asked(&at("graph.onnx")), fake.asked(&mirror)), (1, 1));
    assert_eq!(fake.requests()[1].most_bytes(), 2501);
    // To an origin it does not name, by host, by scheme and by port: refused, and
    // nothing is asked of it.
    for elsewhere in [
        "https://elsewhere.example.org/g",
        "http://mirror.example.net/g",
        "https://mirror.example.net:8443/g",
    ] {
        let elsewhere = address(elsewhere);
        let scratch = Scratch::new("stage-moved");
        let fake = FakeDownloader::new()
            .moved(&at("graph.onnx"), elsewhere.clone())
            .serving(&elsewhere, graph());
        let refused = staging(&Arc::new(ShimFs::new()), scratch.path(), &entry, &fake).unwrap_err();
        assert_eq!(refused, file_refused("graph.onnx", FileRefused::Elsewhere));
        assert_eq!(fake.asked(&elsewhere), 0);
        assert!(!incoming(scratch.path()).exists());
    }
}

#[test]
fn a_file_that_moves_more_than_five_times_is_refused() {
    assert_eq!(MOVES_LIMIT, 5);
    let entry = entry_of(&[("graph.onnx", graph())]);
    // A chain of this many moves, the last address serving the file.
    let chain = |moves: usize| {
        let mut fake = FakeDownloader::new();
        let mut from = at("graph.onnx");
        for hop in 0..moves {
            let to = address(&format!("https://mirror.example.net/hop/{hop}"));
            fake = fake.moved(&from, to.clone());
            from = to;
        }
        fake.serving(&from, graph())
    };
    let scratch = Scratch::new("stage-hops");
    let fake = chain(MOVES_LIMIT);
    staging(&Arc::new(ShimFs::new()), scratch.path(), &entry, &fake).unwrap();
    assert_eq!(fake.requests().len(), MOVES_LIMIT + 1);
    let scratch = Scratch::new("stage-hops");
    let fake = chain(MOVES_LIMIT + 1);
    let refused = staging(&Arc::new(ShimFs::new()), scratch.path(), &entry, &fake).unwrap_err();
    assert_eq!(
        refused,
        file_refused("graph.onnx", FileRefused::TooManyMoves)
    );
    // The sixth move was answered, and the address it named was not asked.
    assert_eq!(fake.requests().len(), MOVES_LIMIT + 1);
    assert!(!incoming(scratch.path()).exists());
}

#[test]
fn a_download_that_fails_or_is_cancelled_fails_the_staging_and_leaves_nothing_staged() {
    let fault = Fault::new(FaultKind::Unavailable, "the scripted host is down");
    let fakes = [
        FakeDownloader::new().failing(&at("graph.onnx"), fault.clone()),
        FakeDownloader::new().failing_after(&at("graph.onnx"), graph(), 1500, fault),
    ];
    for fake in fakes {
        let fake = fake.serving(&at("LICENSE.txt"), TERMS.to_vec());
        let scratch = Scratch::new("stage-fault");
        let refused =
            staging(&Arc::new(ShimFs::new()), scratch.path(), &entry(), &fake).unwrap_err();
        assert_eq!(
            refused,
            file_refused("graph.onnx", FileRefused::Download(FaultKind::Unavailable))
        );
        holds_no_path(&refused, scratch.path());
        assert!(!format!("{refused}").contains("scripted host"));
        assert!(!incoming(scratch.path()).exists());
    }
    // Cancelled before it starts: nothing is asked of the downloader.
    let scratch = Scratch::new("stage-cancel");
    let fake = serving_both();
    let cancellation = Cancellation::new();
    cancellation.cancel();
    let dir = ModelsDir::at(Arc::new(ShimFs::new()), scratch.path());
    let refused = stage(&dir, &entry(), &fake, &cancellation, &none()).unwrap_err();
    assert_eq!(refused, StageRefused::Cancelled);
    assert!(fake.requests().is_empty());
    assert!(!incoming(scratch.path()).exists());
}

#[test]
fn a_download_that_ends_as_cancelled_ends_the_staging_as_cancelled() {
    let scratch = Scratch::new("stage-cancelled");
    let fake = FakeDownloader::new()
        .serving(&at("LICENSE.txt"), TERMS.to_vec())
        .failing(
            &at("graph.onnx"),
            Fault::new(FaultKind::Cancelled, "the download was cancelled"),
        );
    let refused = staging(&Arc::new(ShimFs::new()), scratch.path(), &entry(), &fake).unwrap_err();
    assert_eq!(refused, StageRefused::Cancelled);
    assert!(!incoming(scratch.path()).exists());
}

/// A downloader that keeps to nothing: it hands over each of its pieces whatever its
/// sink answers, and then answers as it was told.
struct Rude {
    calls: std::sync::Mutex<Vec<(Vec<Vec<u8>>, Downloaded)>>,
}

impl Rude {
    fn playing(calls: Vec<(Vec<Vec<u8>>, Downloaded)>) -> Rude {
        let mut calls = calls;
        calls.reverse();
        Rude {
            calls: std::sync::Mutex::new(calls),
        }
    }
}

impl Downloader for Rude {
    fn download(
        &self,
        _: &DownloadRequest,
        _: &Cancellation,
        sink: &mut dyn FnMut(&[u8]) -> bool,
    ) -> Result<Downloaded, Fault> {
        let (pieces, ended) = self.calls.lock().unwrap().pop().unwrap();
        for piece in pieces {
            let _ = sink(&piece);
        }
        Ok(ended)
    }
}

#[test]
fn a_downloader_that_goes_on_after_its_sink_answered_no_has_nothing_more_kept() {
    // A first piece one byte longer than the file, to which the sink answers no; and
    // then, all the same, the file as it is stated.
    let entry = entry_of(&[("graph.onnx", graph())]);
    let mut longer = graph();
    longer.push(7);
    let rude = Rude::playing(vec![(vec![longer, graph()], Downloaded::Bytes(5001))]);
    let scratch = Scratch::new("stage-rude");
    let fs = Arc::new(ShimFs::new());
    let dir = ModelsDir::at(fs.clone(), scratch.path());
    let refused = stage(&dir, &entry, &rude, &Cancellation::new(), &none()).unwrap_err();
    assert_eq!(refused, file_refused("graph.onnx", FileRefused::TooLong));
    // Nothing was appended: not the piece that was too long, and nothing after it.
    assert!(
        !fs.events()
            .iter()
            .any(|event| matches!(event, Event::Append(_)))
    );
    assert!(!incoming(scratch.path()).exists());
}

#[test]
fn a_downloader_that_hands_over_bytes_and_then_says_the_file_moved_is_refused() {
    // The first part of the file, and then another address, which would give the
    // rest: the two together are the file as it is stated, and are not taken for it.
    let entry = entry_of(&[("graph.onnx", graph())]);
    let mirror = address("https://mirror.example.net/g");
    let rude = Rude::playing(vec![
        (vec![graph()[..1000].to_vec()], Downloaded::Moved(mirror)),
        (vec![graph()[1000..].to_vec()], Downloaded::Bytes(1500)),
    ]);
    let scratch = Scratch::new("stage-rude");
    let dir = ModelsDir::at(Arc::new(ShimFs::new()), scratch.path());
    let refused = stage(&dir, &entry, &rude, &Cancellation::new(), &none()).unwrap_err();
    assert_eq!(
        refused,
        file_refused("graph.onnx", FileRefused::Download(FaultKind::Malformed))
    );
    assert_eq!(
        rude.calls.lock().unwrap().len(),
        1,
        "the address it named was asked"
    );
    assert!(!incoming(scratch.path()).exists());
}

// --- the model's lock --------------------------------------------------------------------------

#[test]
fn staging_is_under_the_models_lock_and_the_next_cleans_what_the_last_left() {
    let scratch = Scratch::new("stage-lock");
    let fs = Arc::new(ShimFs::new());
    let first = staging(&fs, scratch.path(), &entry(), &serving_both()).unwrap();
    // While what was staged is held, a second staging of the model is refused at
    // once, asks nothing of its downloader, and touches nothing.
    let before = names_under(scratch.path());
    let fake = serving_both();
    let refused = staging(&fs, scratch.path(), &entry(), &fake).unwrap_err();
    assert_eq!(refused, StageRefused::Busy);
    assert!(fake.requests().is_empty());
    assert_eq!(names_under(scratch.path()), before);
    assert_eq!(
        on_disk(&staged_at(scratch.path(), "graph.onnx")).unwrap(),
        graph()
    );
    // The lock was the first thing taken.
    assert_eq!(
        fs.events()
            .iter()
            .find(|event| !matches!(event, Event::CreateDir(_) | Event::SyncDir(_))),
        Some(&Event::Lock(scratch.path().join("models/tiny/LOCK")))
    );
    drop(first);
    // Let go, the next stages; what the last left is removed first, so a file of the
    // last staging that the next entry does not list is gone.
    let other = entry_of(&[("LICENSE.txt", TERMS.to_vec())]);
    staging(&fs, scratch.path(), &other, &serving_both()).unwrap();
    assert!(!staged_at(scratch.path(), "graph.onnx").exists());
    assert_eq!(
        on_disk(&staged_at(scratch.path(), "LICENSE.txt")).unwrap(),
        TERMS
    );
}

// --- what the directory holds is not trusted ---------------------------------------------------

#[cfg(unix)]
#[test]
fn a_link_where_staging_belongs_is_removed_and_never_followed() {
    // A link to a folder and a link to a file, on the real file system and on the
    // shim.
    for (to_a_folder, on_the_shim) in [(true, false), (false, false), (true, true), (false, true)] {
        let scratch = Scratch::new("stage-link");
        let outside = Scratch::new("stage-link-outside");
        std::fs::write(outside.path().join("kept"), b"not whizbang's").unwrap();
        std::fs::create_dir_all(scratch.path().join("models/tiny/staging")).unwrap();
        let target = if to_a_folder {
            outside.path().to_path_buf()
        } else {
            outside.path().join("kept")
        };
        std::os::unix::fs::symlink(&target, incoming(scratch.path())).unwrap();
        let fs: Arc<dyn Fs> = if on_the_shim {
            Arc::new(ShimFs::new())
        } else {
            Arc::new(RealFs)
        };
        let dir = ModelsDir::at(fs, scratch.path());
        stage(
            &dir,
            &entry(),
            &serving_both(),
            &Cancellation::new(),
            &none(),
        )
        .unwrap();
        // The link is gone, a folder is in its place, and what it led to is as it was.
        assert!(!incoming(scratch.path()).is_symlink());
        assert_eq!(
            on_disk(&staged_at(scratch.path(), "graph.onnx")).unwrap(),
            graph()
        );
        assert_eq!(
            names_under(outside.path()),
            ["kept"],
            "written or removed through the link"
        );
        assert_eq!(
            on_disk(&outside.path().join("kept")).unwrap(),
            b"not whizbang's"
        );
    }
}

#[test]
fn staging_touches_nothing_outside_the_models_directory() {
    // By the shim's own list of what was done, over a staging that passes, one that
    // is refused and cleaned, and one after it.
    let scratch = Scratch::new("stage-inside");
    let fs = Arc::new(ShimFs::new());
    staging(&fs, scratch.path(), &entry(), &serving_both()).unwrap();
    let short = FakeDownloader::new().serving(&at("LICENSE.txt"), b"term".to_vec());
    staging(&fs, scratch.path(), &entry(), &short).unwrap_err();
    staging(&fs, scratch.path(), &entry(), &serving_both()).unwrap();
    for event in fs.events() {
        let paths = match &event {
            Event::Rename(from, to) => vec![from.clone(), to.clone()],
            Event::CreateDir(p)
            | Event::Write(p)
            | Event::SyncFile(p)
            | Event::SyncDir(p)
            | Event::Remove(p)
            | Event::Lock(p)
            | Event::Begin(p)
            | Event::Append(p)
            | Event::Finish(p)
            | Event::Open(p)
            | Event::Read(p)
            | Event::TryShare(p) => vec![p.clone()],
        };
        for path in paths {
            assert!(path.starts_with(scratch.path()), "{event:?}");
        }
    }
}

// --- a secret never becomes a name on the disk -------------------------------------------------

#[test]
fn a_name_that_holds_a_secret_the_installer_was_told_of_never_becomes_a_name_on_the_disk() {
    // The entry was made by a constructor that was told of no secret; the installer
    // is told of one, and looks again.
    let named = {
        let mut parts = parts(&format!("tiny-{MARKER}"));
        parts.files =
            vec![CatalogueFile::new(file_name("f"), 5, hash(TERMS), at("f"), &none()).unwrap()];
        Entry::new(parts, &none()).unwrap()
    };
    let with_file = entry_of(&[(&format!("{MARKER}.onnx"), TERMS.to_vec())]);
    for entry in [named, with_file] {
        let scratch = Scratch::new("stage-secret");
        let fs = Arc::new(ShimFs::new());
        let fake = FakeDownloader::new()
            .serving(&at("f"), TERMS.to_vec())
            .serving(&at(&format!("{MARKER}.onnx")), TERMS.to_vec());
        let dir = ModelsDir::at(fs.clone(), scratch.path());
        let refused = stage(&dir, &entry, &fake, &Cancellation::new(), &secrets()).unwrap_err();
        assert_eq!(refused, StageRefused::HoldsSecret);
        assert!(!format!("{refused} {refused:?}").contains(&MARKER[..4]));
        // Nothing was made, asked or written: not a folder, not a lock.
        assert_eq!(names_under(scratch.path()), Vec::<String>::new());
        assert!(fs.events().is_empty());
        assert!(fake.requests().is_empty());
        // Told of no secret, the same entry is staged, and the marker is then a name:
        // the refusal above is what kept it off the disk.
        let staged = stage(&dir, &entry, &fake, &Cancellation::new(), &none());
        assert!(staged.is_ok());
        assert!(
            names_under(scratch.path())
                .iter()
                .any(|name| name.contains(MARKER))
        );
    }
}

// --- each step of the file system refused, and a crash at each ---------------------------------

/// The events of one staging of the entry on an empty directory.
fn clean_events() -> usize {
    let scratch = Scratch::new("stage-events");
    let fs = Arc::new(ShimFs::new());
    staging(&fs, scratch.path(), &entry(), &serving_both()).unwrap();
    fs.events().len()
}

#[test]
fn each_step_of_the_file_system_refused_fails_the_staging_without_a_path_and_the_next_stages() {
    let events = clean_events();
    assert!((20..60).contains(&events), "{events}");
    for event in 0..events {
        let scratch = Scratch::new("stage-refuse");
        let fs = Arc::new(ShimFs::new().fail_before(event));
        let refused = staging(&fs, scratch.path(), &entry(), &serving_both()).unwrap_err();
        assert!(
            matches!(refused, StageRefused::Disk(_)),
            "event {event}: {refused:?}"
        );
        holds_no_path(&refused, scratch.path());
        // Whatever was left, the next staging removes it and stages whole.
        staging(&fs, scratch.path(), &entry(), &serving_both())
            .unwrap_or_else(|refused| panic!("event {event}: {refused:?}"));
        assert_eq!(
            on_disk(&staged_at(scratch.path(), "graph.onnx")).unwrap(),
            graph()
        );
        assert_eq!(
            on_disk(&staged_at(scratch.path(), "LICENSE.txt")).unwrap(),
            TERMS
        );
    }
}

#[test]
fn a_crash_at_any_event_of_staging_leaves_what_the_next_staging_removes_and_stages_whole() {
    let events = clean_events();
    let losses = [
        Lost::Unsynced,
        Lost::ContentOnly,
        Lost::Subset(0b01),
        Lost::Subset(0b10),
        Lost::Names(0b0101),
        Lost::Names(0b1010),
    ];
    // What a crash leaves of a streamed file's unsynced bytes, each way the shim can
    // leave them: half, the full length zeroed, all, none.
    let tails = [Tail::Half, Tail::Zeroed, Tail::All, Tail::None];
    for (event, tail) in (0..events).flat_map(|event| tails.map(|tail| (event, tail))) {
        for lost in losses {
            let scratch = Scratch::new("stage-crash");
            let fs = Arc::new(ShimFs::new().with_tail(tail).crash_before(event, lost));
            assert!(
                staging(&fs, scratch.path(), &entry(), &serving_both()).is_err(),
                "event {event}, {lost:?}, {tail:?}"
            );
            assert!(fs.crashed());
            // The process is gone, and its lock with it. The next opens the same
            // directory: it trusts nothing it finds in staging, whatever the crash
            // left of a file, and stages every file again, whole.
            let fs = Arc::new(ShimFs::new());
            let fake = serving_both();
            staging(&fs, scratch.path(), &entry(), &fake)
                .unwrap_or_else(|refused| panic!("event {event}, {lost:?}, {tail:?}: {refused:?}"));
            assert_eq!(
                (
                    fake.handed(&at("graph.onnx")),
                    fake.handed(&at("LICENSE.txt"))
                ),
                (2500, 5)
            );
            assert_eq!(
                on_disk(&staged_at(scratch.path(), "graph.onnx")).unwrap(),
                graph()
            );
            assert_eq!(
                on_disk(&staged_at(scratch.path(), "LICENSE.txt")).unwrap(),
                TERMS
            );
        }
    }
}

#[test]
fn a_models_directory_on_a_file_system_that_streams_nothing_is_refused_and_not_worked_around() {
    /// The store's interface as it was before the streamed calls: every other call
    /// goes to the shim, and the three new ones answer as the interface's own do.
    struct Old(ShimFs);
    impl Fs for Old {
        fn create_dir(&self, path: &Path) -> std::io::Result<()> {
            self.0.create_dir(path)
        }
        fn write_new(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
            self.0.write_new(path, bytes)
        }
        fn sync_dir(&self, path: &Path) -> std::io::Result<()> {
            self.0.sync_dir(path)
        }
        fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
            self.0.rename(from, to)
        }
        fn remove_all(&self, path: &Path) -> std::io::Result<()> {
            self.0.remove_all(path)
        }
        fn read_at_most(&self, path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
            self.0.read_at_most(path, limit)
        }
        fn list(&self, path: &Path) -> std::io::Result<Vec<PathBuf>> {
            self.0.list(path)
        }
        fn exists(&self, path: &Path) -> std::io::Result<bool> {
            self.0.exists(path)
        }
        fn hold_lock_file(
            &self,
            path: &Path,
            create: bool,
        ) -> std::io::Result<Option<Box<dyn whizbang_core::store::fs::LockFile>>> {
            self.0.hold_lock_file(path, create)
        }
        fn share(&self, path: &Path) -> std::io::Result<Box<dyn whizbang_core::store::fs::Held>> {
            self.0.share(path)
        }
        fn try_alone(
            &self,
            path: &Path,
        ) -> std::io::Result<Option<Box<dyn whizbang_core::store::fs::Held>>> {
            self.0.try_alone(path)
        }
    }
    let scratch = Scratch::new("stage-old");
    let dir = ModelsDir::at(Arc::new(Old(ShimFs::new())), scratch.path());
    let refused = stage(
        &dir,
        &entry(),
        &serving_both(),
        &Cancellation::new(),
        &none(),
    )
    .unwrap_err();
    assert_eq!(refused, StageRefused::Disk(std::io::ErrorKind::Unsupported));
    assert!(!staged_at(scratch.path(), "LICENSE.txt").exists());
}
