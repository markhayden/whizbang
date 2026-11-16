//! The file-system interface's streamed files and its try at a share (ADR 0018), held on
//! the real file system and on the test kit's, by the same tests: the rules are the
//! interface's, so both keep them. What only the test kit's does, a refusal of one step
//! and a crash, is in its own tests (`whizbang-testkit`, `streams`).
//!
//! Every file here is a few bytes. The bound on a piece is met by a buffer in memory that
//! is refused before anything is written, and the bound on a file by a small stated most.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use whizbang_core::store::fs::{
    Fs, Held, LockFile, PIECE_MAX_BYTES, RealFs, Streamed, UserUnknown,
};
use whizbang_testkit::fs_shim::ShimFs;
use whizbang_testkit::scratch::Scratch;

/// The real file system, and the test kit's over it.
fn both() -> Vec<(&'static str, Arc<dyn Fs>)> {
    vec![
        ("the real file system", Arc::new(RealFs)),
        ("the shim", Arc::new(ShimFs::new())),
    ]
}

/// A try at a share, made on a thread of its own and waited for ten seconds at most: a
/// try that waited for the holder fails here, and does not hang the test. The thread ends
/// when the try answers, which a waiting one does once the test lets the holder go.
fn tried(fs: &Arc<dyn Fs>, path: &Path) -> io::Result<Option<Box<dyn Held>>> {
    let (answer, answered) = std::sync::mpsc::channel();
    let (fs, path) = (fs.clone(), path.to_path_buf());
    let thread = whizbang_testkit::threads::spawn(move || {
        let _ = answer.send(fs.try_share(&path));
    });
    let got = answered
        .recv_timeout(Duration::from_secs(10))
        .expect("a try at a share waited: it answers at once, given or not");
    thread.join().unwrap();
    got
}

/// No error of the interface names where a file is.
fn path_free(error: &io::Error, dir: &Scratch) {
    let said = error.to_string();
    let place = dir.path().to_string_lossy().into_owned();
    assert!(!said.contains(&place), "an error names a path: {said}");
    assert!(!said.contains("whizbang-"), "an error names a path: {said}");
}

fn written(fs: &dyn Fs, path: &Path, pieces: &[&[u8]]) -> u64 {
    let mut new = fs.begin_new(path, 8192).unwrap();
    for piece in pieces {
        new.append(piece).unwrap();
    }
    new.sync().unwrap();
    new.finish().unwrap()
}

/// The file's pieces, read through a buffer of `size` bytes, each with its length.
fn pieces(fs: &dyn Fs, path: &Path, size: usize) -> Vec<Vec<u8>> {
    let mut open = fs.open_file(path).unwrap();
    let mut read = Vec::new();
    // A file of these tests has a few pieces; sixty-four is more than any.
    for _ in 0..64 {
        let mut buffer = vec![0xAA; size];
        let filled = open.read(&mut buffer).unwrap();
        assert!(filled <= size);
        if filled == 0 {
            return read;
        }
        buffer.truncate(filled);
        read.push(buffer);
    }
    panic!("a file of a few bytes did not end in sixty-four pieces");
}

// --- writing ----------------------------------------------------------------------------------

#[test]
fn a_file_written_in_pieces_holds_them_in_order_and_finish_says_its_length() {
    for (which, fs) in both() {
        let dir = Scratch::new("fs-pieces");
        let file = dir.path().join("f");
        let mut new = fs.begin_new(&file, 1024).unwrap();
        assert_eq!(
            std::fs::read(&file).unwrap(),
            b"",
            "{which}: it is there, empty"
        );
        new.append(b"one, ").unwrap();
        new.append(b"").unwrap();
        new.append(b"two, ").unwrap();
        new.sync().unwrap();
        new.append(b"three").unwrap();
        new.sync().unwrap();
        assert_eq!(new.finish().unwrap(), 15, "{which}");
        assert_eq!(std::fs::read(&file).unwrap(), b"one, two, three", "{which}");
    }
}

#[test]
fn a_file_with_nothing_in_it_is_whole_without_a_sync() {
    for (which, fs) in both() {
        let dir = Scratch::new("fs-empty");
        let file = dir.path().join("f");
        assert_eq!(
            fs.begin_new(&file, 0).unwrap().finish().unwrap(),
            0,
            "{which}"
        );
        assert_eq!(std::fs::read(&file).unwrap(), b"", "{which}");
    }
}

#[cfg(unix)]
#[test]
fn a_streamed_file_is_readable_by_its_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    for (which, fs) in both() {
        let dir = Scratch::new("fs-mode");
        let file = dir.path().join("f");
        written(&*fs, &file, &[b"content"]);
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "{which}");
    }
}

#[test]
fn a_name_that_is_taken_is_refused_and_what_has_it_is_left_alone() {
    for (which, fs) in both() {
        let dir = Scratch::new("fs-taken");
        let file = dir.path().join("f");
        std::fs::write(&file, b"what was there").unwrap();
        let error = fs.begin_new(&file, 1024).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists, "{which}");
        path_free(&error, &dir);
        assert_eq!(std::fs::read(&file).unwrap(), b"what was there", "{which}");

        let folder = dir.path().join("d");
        std::fs::create_dir(&folder).unwrap();
        let error = fs.begin_new(&folder, 1024).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists, "{which}");
        assert!(folder.is_dir(), "{which}");

        let nowhere = dir.path().join("missing").join("f");
        let error = fs.begin_new(&nowhere, 1024).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::NotFound, "{which}");
        path_free(&error, &dir);
    }
}

#[cfg(unix)]
#[test]
fn a_streamed_file_follows_no_link_at_its_name() {
    for (which, fs) in both() {
        let dir = Scratch::new("fs-link-new");
        let target = dir.path().join("target");
        std::fs::write(&target, b"the target").unwrap();
        let to_a_file = dir.path().join("to-a-file");
        std::os::unix::fs::symlink(&target, &to_a_file).unwrap();
        let absent = dir.path().join("absent");
        let to_nothing = dir.path().join("to-nothing");
        std::os::unix::fs::symlink(&absent, &to_nothing).unwrap();

        for link in [&to_a_file, &to_nothing] {
            let error = fs.begin_new(link, 1024).err().unwrap();
            assert_eq!(error.kind(), io::ErrorKind::AlreadyExists, "{which}");
            assert!(
                std::fs::symlink_metadata(link).unwrap().is_symlink(),
                "{which}: the link is as it was"
            );
        }
        assert_eq!(std::fs::read(&target).unwrap(), b"the target", "{which}");
        assert!(
            !absent.exists(),
            "{which}: nothing was made through the link"
        );
    }
}

#[test]
fn a_file_grows_to_the_most_its_opening_stated_and_no_further() {
    for (which, fs) in both() {
        let dir = Scratch::new("fs-most");

        // Up to the most, to the byte.
        let exact = dir.path().join("exact");
        let mut new = fs.begin_new(&exact, 10).unwrap();
        new.append(b"012345").unwrap();
        new.append(b"6789").unwrap();
        new.sync().unwrap();
        assert_eq!(new.finish().unwrap(), 10, "{which}");

        // One byte past it, at the end and across it: refused before any of the piece
        // is written.
        for (name, last) in [("at", &b"6789x"[..]), ("across", &b"67890123456789"[..])] {
            let file = dir.path().join(name);
            let mut new = fs.begin_new(&file, 10).unwrap();
            new.append(b"012345").unwrap();
            let error = new.append(last).err().unwrap();
            assert_eq!(error.kind(), io::ErrorKind::FileTooLarge, "{which}");
            path_free(&error, &dir);
            assert_eq!(std::fs::read(&file).unwrap(), b"012345", "{which}");
        }

        // A file that may hold nothing takes nothing.
        let none = dir.path().join("none");
        let mut new = fs.begin_new(&none, 0).unwrap();
        new.append(b"").unwrap();
        assert_eq!(
            new.append(b"x").err().unwrap().kind(),
            io::ErrorKind::FileTooLarge,
            "{which}"
        );
        assert_eq!(std::fs::read(&none).unwrap(), b"", "{which}");
    }
}

#[test]
fn a_piece_over_the_bound_is_refused_before_anything_is_written() {
    // In memory only: the piece is refused, so no file ever holds it.
    let piece = vec![7u8; PIECE_MAX_BYTES + 1];
    for (which, fs) in both() {
        let dir = Scratch::new("fs-piece");
        let file = dir.path().join("f");
        let mut new = fs.begin_new(&file, u64::MAX).unwrap();
        new.append(b"before").unwrap();
        let error = new.append(&piece).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{which}");
        path_free(&error, &dir);
        assert_eq!(std::fs::read(&file).unwrap(), b"before", "{which}");
    }
}

#[test]
fn finish_answers_only_for_a_file_whose_every_byte_was_synced() {
    for (which, fs) in both() {
        let dir = Scratch::new("fs-unsynced");

        let never = dir.path().join("never");
        let mut new = fs.begin_new(&never, 1024).unwrap();
        new.append(b"content").unwrap();
        let error = new.finish().err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{which}");
        path_free(&error, &dir);

        let since = dir.path().join("since");
        let mut new = fs.begin_new(&since, 1024).unwrap();
        new.append(b"content").unwrap();
        new.sync().unwrap();
        new.append(b"!").unwrap();
        assert!(new.finish().is_err(), "{which}: a byte since the last sync");

        let synced = dir.path().join("synced");
        let mut new = fs.begin_new(&synced, 1024).unwrap();
        new.append(b"content").unwrap();
        new.sync().unwrap();
        new.sync().unwrap();
        assert_eq!(new.finish().unwrap(), 7, "{which}");
    }
}

#[test]
fn a_handle_that_answered_with_an_error_answers_nothing_more() {
    let long = vec![7u8; PIECE_MAX_BYTES + 1];
    for (which, fs) in both() {
        let dir = Scratch::new("fs-broken");
        // Broken by a file too large, and by a piece too long.
        for (name, breaker) in [("large", &b"0123456789abc"[..]), ("long", &long[..])] {
            let file = dir.path().join(name);
            let mut new = fs.begin_new(&file, 12).unwrap();
            new.append(b"01234567").unwrap();
            new.sync().unwrap();
            assert!(new.append(breaker).is_err(), "{which}");
            // It was whole, and synced, a moment ago; it says so no more.
            let error = new.append(b"8").err().unwrap();
            path_free(&error, &dir);
            assert!(new.append(b"").is_err(), "{which}");
            assert!(new.sync().is_err(), "{which}");
            assert!(new.finish().is_err(), "{which}");
            assert_eq!(std::fs::read(&file).unwrap(), b"01234567", "{which}");
        }
    }
}

// --- the count, without a disk ----------------------------------------------------------------

#[test]
fn the_count_admits_a_piece_up_to_the_bound_and_a_file_up_to_its_most() {
    let mut count = Streamed::new(u64::MAX);
    count.admit(PIECE_MAX_BYTES).unwrap();
    count.appended(PIECE_MAX_BYTES);
    assert_eq!(count.written(), PIECE_MAX_BYTES as u64);
    assert_eq!(count.durable(), 0);
    assert_eq!(
        count.admit(PIECE_MAX_BYTES + 1).err().unwrap().kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(count.usable().is_err(), "a refusal breaks it");
    assert!(count.admit(1).is_err());

    // A file one byte short of the most takes one byte and not two; and a count that
    // would pass the largest number is a file too large, not a wrapped count.
    let most = 3 * PIECE_MAX_BYTES as u64;
    let mut count = Streamed::new(most);
    for _ in 0..2 {
        count.admit(PIECE_MAX_BYTES).unwrap();
        count.appended(PIECE_MAX_BYTES);
    }
    count.admit(PIECE_MAX_BYTES - 1).unwrap();
    count.appended(PIECE_MAX_BYTES - 1);
    assert_eq!(count.written(), most - 1);
    count.admit(1).unwrap();
    assert_eq!(
        count.admit(2).err().unwrap().kind(),
        io::ErrorKind::FileTooLarge
    );
    assert!(count.usable().is_err());
}

#[test]
fn the_count_is_whole_when_every_byte_is_durable_and_never_once_broken() {
    let mut count = Streamed::new(100);
    assert_eq!(count.whole().unwrap(), 0);
    count.admit(40).unwrap();
    count.appended(40);
    count.synced();
    assert_eq!((count.written(), count.durable()), (40, 40));
    assert_eq!(count.whole().unwrap(), 40);
    count.admit(2).unwrap();
    count.appended(2);
    assert_eq!((count.written(), count.durable()), (42, 40));
    assert_eq!(
        count.whole().err().unwrap().kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(count.usable().is_err(), "asked too early, it is broken");

    let mut count = Streamed::new(100);
    count.usable().unwrap();
    count.fail();
    assert!(count.usable().is_err());
    assert!(count.admit(0).is_err());
    assert!(count.whole().is_err());
}

// --- reading ----------------------------------------------------------------------------------

#[test]
fn a_file_is_read_in_pieces_each_full_but_the_last() {
    for (which, fs) in both() {
        let dir = Scratch::new("fs-read");
        let file = dir.path().join("f");
        std::fs::write(&file, b"0123456789").unwrap();
        assert_eq!(
            pieces(&*fs, &file, 4),
            vec![b"0123".to_vec(), b"4567".to_vec(), b"89".to_vec()],
            "{which}"
        );
        assert_eq!(
            pieces(&*fs, &file, 5),
            vec![b"01234".to_vec(), b"56789".to_vec()],
            "{which}: a file that ends with a piece ends with an empty one after it"
        );
        assert_eq!(
            pieces(&*fs, &file, 64),
            vec![b"0123456789".to_vec()],
            "{which}"
        );
        let empty = dir.path().join("empty");
        std::fs::write(&empty, b"").unwrap();
        assert_eq!(pieces(&*fs, &empty, 4), Vec::<Vec<u8>>::new(), "{which}");

        // At the end it goes on saying so, and what the buffer held is left.
        let mut open = fs.open_file(&file).unwrap();
        let mut buffer = [0xAA; 16];
        assert_eq!(open.read(&mut buffer).unwrap(), 10, "{which}");
        assert_eq!(&buffer[10..], [0xAA; 6], "{which}");
        assert_eq!(open.read(&mut buffer).unwrap(), 0, "{which}");
        assert_eq!(open.read(&mut buffer).unwrap(), 0, "{which}");
    }
}

#[test]
fn what_was_streamed_is_read_back_as_it_was_written() {
    for (which, fs) in both() {
        let dir = Scratch::new("fs-round");
        let file = dir.path().join("f");
        // A few KiB, in pieces of unlike lengths.
        let content: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        let parts: Vec<&[u8]> = vec![&content[..1], &content[1..1500], &content[1500..]];
        assert_eq!(written(&*fs, &file, &parts), 5000, "{which}");
        assert_eq!(pieces(&*fs, &file, 1024).concat(), content, "{which}");
    }
}

#[test]
fn a_buffer_that_holds_nothing_or_more_than_a_piece_is_refused() {
    // In memory only: nothing this long is read or written.
    let mut long = vec![0u8; PIECE_MAX_BYTES + 1];
    for (which, fs) in both() {
        let dir = Scratch::new("fs-buffer");
        let file = dir.path().join("f");
        std::fs::write(&file, b"0123456789").unwrap();

        let mut open = fs.open_file(&file).unwrap();
        let error = open.read(&mut []).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{which}");
        path_free(&error, &dir);
        // Broken: an empty answer would have read as the end of the file.
        assert!(open.read(&mut [0u8; 4]).is_err(), "{which}");

        let mut open = fs.open_file(&file).unwrap();
        let error = open.read(&mut long).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{which}");
        assert!(open.read(&mut [0u8; 4]).is_err(), "{which}");

        // A buffer of a whole piece is taken, and filled as far as the file goes.
        let mut open = fs.open_file(&file).unwrap();
        assert_eq!(
            open.read(&mut long[..PIECE_MAX_BYTES]).unwrap(),
            10,
            "{which}"
        );
        assert_eq!(&long[..10], b"0123456789", "{which}");
    }
}

#[test]
fn only_a_regular_file_is_opened() {
    for (which, fs) in both() {
        let dir = Scratch::new("fs-regular");
        let error = fs.open_file(&dir.path().join("missing")).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::NotFound, "{which}");
        path_free(&error, &dir);

        let folder = dir.path().join("d");
        std::fs::create_dir(&folder).unwrap();
        let error = fs.open_file(&folder).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData, "{which}");
        path_free(&error, &dir);
    }
}

#[cfg(unix)]
#[test]
fn a_link_is_not_followed_to_what_it_names() {
    use std::os::unix::fs::symlink;
    for (which, fs) in both() {
        let dir = Scratch::new("fs-link-read");
        let target = dir.path().join("target");
        std::fs::write(&target, b"the target").unwrap();
        let folder = dir.path().join("d");
        std::fs::create_dir(&folder).unwrap();
        let (to_a_file, to_a_folder, to_nothing) = (
            dir.path().join("to-a-file"),
            dir.path().join("to-a-folder"),
            dir.path().join("to-nothing"),
        );
        symlink(&target, &to_a_file).unwrap();
        symlink(&folder, &to_a_folder).unwrap();
        symlink(dir.path().join("absent"), &to_nothing).unwrap();
        for link in [&to_a_file, &to_a_folder, &to_nothing] {
            let error = fs.open_file(link).err().unwrap();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData, "{which}");
            path_free(&error, &dir);
        }
        // A second name for the same file is no link to follow: it is the file.
        let second = dir.path().join("second");
        std::fs::hard_link(&target, &second).unwrap();
        assert_eq!(
            pieces(&*fs, &second, 64),
            vec![b"the target".to_vec()],
            "{which}"
        );
    }
}

// --- removing a link ----------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn removing_a_link_removes_the_link_and_follows_it_nowhere() {
    use std::os::unix::fs::symlink;
    for (which, fs) in both() {
        let dir = Scratch::new("fs-link-removed");
        let target = dir.path().join("target");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("kept"), b"kept").unwrap();
        let file = dir.path().join("file");
        std::fs::write(&file, b"a file").unwrap();
        let untouched = || {
            assert_eq!(
                std::fs::read(target.join("kept")).unwrap(),
                b"kept",
                "{which}"
            );
            assert_eq!(std::fs::read(&file).unwrap(), b"a file", "{which}");
        };

        // A link to a folder, to a file and to nothing, each removed by itself.
        for leads_to in [target.clone(), file.clone(), dir.path().join("absent")] {
            let link = dir.path().join("link");
            symlink(&leads_to, &link).unwrap();
            fs.remove_all(&link).unwrap();
            assert!(std::fs::symlink_metadata(&link).is_err(), "{which}");
            untouched();
        }

        // Links inside a folder that is removed.
        let folder = dir.path().join("folder");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("own"), b"its own").unwrap();
        symlink(&target, folder.join("to-a-folder")).unwrap();
        symlink(&file, folder.join("to-a-file")).unwrap();
        symlink(dir.path().join("absent"), folder.join("to-nothing")).unwrap();
        fs.remove_all(&folder).unwrap();
        assert!(!folder.exists(), "{which}");
        untouched();
    }
}

// --- the try at a share -----------------------------------------------------------------------

#[test]
fn a_try_at_a_share_is_given_beside_shares_and_refused_while_one_has_the_file_alone() {
    for (which, fs) in both() {
        let dir = Scratch::new("fs-try");
        let file = dir.path().join("f");
        let error = fs.try_share(&file).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::NotFound, "{which}");
        path_free(&error, &dir);
        assert!(!file.exists(), "{which}: a try makes nothing");
        std::fs::write(&file, b"content").unwrap();

        // Free, and beside a share that waited and one that did not.
        let first = tried(&fs, &file).unwrap().expect("a free file");
        let waited = fs.share(&file).unwrap();
        let second = tried(&fs, &file).unwrap().expect("beside two shares");

        // Each of them keeps the file from being taken alone, until the last is gone.
        assert!(fs.try_alone(&file).unwrap().is_none(), "{which}");
        drop(waited);
        drop(first);
        assert!(
            fs.try_alone(&file).unwrap().is_none(),
            "{which}: one try's share is left"
        );
        drop(second);

        // Held alone: refused, at once, for as long as it is; then given.
        let alone = fs.try_alone(&file).unwrap().expect("nobody shares it now");
        assert!(tried(&fs, &file).unwrap().is_none(), "{which}");
        assert!(tried(&fs, &file).unwrap().is_none(), "{which}: and again");
        drop(alone);
        let given = tried(&fs, &file).unwrap();
        assert!(given.is_some(), "{which}: let go, it is free");
        drop(given);

        // A lock file is held alone too.
        let lock = fs.hold_lock_file(&file, false).unwrap().expect("free");
        assert!(tried(&fs, &file).unwrap().is_none(), "{which}");
        drop(lock);
        assert!(tried(&fs, &file).unwrap().is_some(), "{which}");

        assert_eq!(
            std::fs::read(&file).unwrap(),
            b"content",
            "{which}: no try wrote anything"
        );
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            1,
            "{which}: or made anything"
        );
    }
}

// --- a file system that was not given the new calls ---------------------------------------------

/// The store's own calls and nothing more, as a file system written before these were.
struct Bare;

impl Fs for Bare {
    fn create_dir(&self, path: &Path) -> io::Result<()> {
        RealFs.create_dir(path)
    }
    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        RealFs.write_new(path, bytes)
    }
    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        RealFs.sync_dir(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        RealFs.rename(from, to)
    }
    fn remove_all(&self, path: &Path) -> io::Result<()> {
        RealFs.remove_all(path)
    }
    fn read_at_most(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>> {
        RealFs.read_at_most(path, limit)
    }
    fn list(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        RealFs.list(path)
    }
    fn exists(&self, path: &Path) -> io::Result<bool> {
        RealFs.exists(path)
    }
    fn hold_lock_file(&self, path: &Path, create: bool) -> io::Result<Option<Box<dyn LockFile>>> {
        RealFs.hold_lock_file(path, create)
    }
    fn share(&self, path: &Path) -> io::Result<Box<dyn Held>> {
        RealFs.share(path)
    }
    fn try_alone(&self, path: &Path) -> io::Result<Option<Box<dyn Held>>> {
        RealFs.try_alone(path)
    }
}

#[test]
fn a_file_system_without_the_new_calls_says_so_and_touches_nothing() {
    let dir = Scratch::new("fs-bare");
    let file = dir.path().join("f");
    std::fs::write(&file, b"content").unwrap();
    let new = dir.path().join("new");

    let error = Bare.begin_new(&new, 1024).err().unwrap();
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    path_free(&error, &dir);
    assert!(!new.exists());
    let error = Bare.open_file(&file).err().unwrap();
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    // Not "held alone by another", which nobody said: an error.
    let error = Bare.try_share(&file).err().unwrap();
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    // Not "no link", which nobody said either.
    let error = Bare.is_link(&file).err().unwrap();
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    path_free(&error, &dir);
    // Not "its owner's alone": whose a folder is was not said.
    let error = Bare.who_may_write(dir.path()).err().unwrap();
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    path_free(&error, &dir);
    assert!(Bare.try_alone(&file).unwrap().is_some(), "and nothing held");
}

// --- whose a folder is, and who else may write it -------------------------------------------------

#[cfg(unix)]
#[test]
fn who_may_write_a_folder_is_read_from_its_owner_and_its_mode_and_a_link_to_it_is_followed() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = Scratch::new("fs-writers");
    let folder = dir.path().join("folder");
    // A folder this file system made is its user's, and nobody else's to write.
    RealFs.create_dir(&folder).unwrap();
    let alone = RealFs.who_may_write(&folder).unwrap();
    assert!(alone.owner_is_this_user && !alone.group && !alone.anyone);
    // The two bits are read each by itself; the bits that let others read, and the
    // owner's own, are not asked.
    let link = dir.path().join("link");
    symlink(&folder, &link).unwrap();
    for (mode, group, anyone) in [
        (0o700, false, false),
        (0o755, false, false),
        (0o500, false, false),
        (0o720, true, false),
        (0o702, false, true),
        (0o722, true, true),
        (0o1777, true, true),
    ] {
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(mode)).unwrap();
        for name in [&folder, &link] {
            let found = RealFs.who_may_write(name).unwrap();
            assert!(found.owner_is_this_user, "{mode:o}");
            assert_eq!((found.group, found.anyone), (group, anyone), "{mode:o}");
        }
    }
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
    // It changes nothing, and leaves nothing beside the folder.
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
    // A file is no folder, and a name nothing has is not there: each an error, with
    // no path.
    let file = dir.path().join("file");
    std::fs::write(&file, b"content").unwrap();
    let error = RealFs.who_may_write(&file).err().unwrap();
    assert_eq!(error.kind(), io::ErrorKind::NotADirectory);
    path_free(&error, &dir);
    let error = RealFs
        .who_may_write(&dir.path().join("missing"))
        .err()
        .unwrap();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
}

#[cfg(unix)]
#[test]
fn the_real_file_system_says_that_a_folder_of_another_user_is_not_this_users() {
    use std::os::unix::fs::MetadataExt;
    // Who the process runs as is read from a socket pair it has just made, and is
    // the owner of a folder and of a file it makes: on both runners, or this fails.
    let dir = Scratch::new("fs-anothers");
    let folder = dir.path().join("folder");
    RealFs.create_dir(&folder).unwrap();
    let file = dir.path().join("file");
    std::fs::write(&file, b"content").unwrap();
    let mine = std::fs::metadata(&file).unwrap().uid();
    assert_eq!(std::fs::metadata(&folder).unwrap().uid(), mine);
    assert!(RealFs.who_may_write(&folder).unwrap().owner_is_this_user);
    // The root of the file system is a real folder of another user: the one who
    // administers the machine. No test runs anything as another user. Where the
    // tests are themselves run as that user there is no folder of another's to
    // ask about, and only then is nothing asked.
    let root = Path::new("/");
    if std::fs::metadata(root).unwrap().uid() == mine {
        return;
    }
    let found = RealFs.who_may_write(root).unwrap();
    assert!(!found.owner_is_this_user, "the root folder is another's");
    // And asked again, the same: the answer about the user is kept, not the folder's.
    assert!(!RealFs.who_may_write(root).unwrap().owner_is_this_user);
    assert!(RealFs.who_may_write(&folder).unwrap().owner_is_this_user);
}

#[test]
fn a_failure_to_learn_the_user_is_an_error_of_its_own_kind_and_never_a_name_that_is_not_there() {
    let error = UserUnknown::error();
    assert!(UserUnknown::is(&error));
    // `open` reads "not found" as a directory that is still to be made: this is
    // never that, whatever the system said when it refused.
    assert_eq!(error.kind(), io::ErrorKind::Other);
    assert_eq!(
        error.to_string(),
        "the user this process runs as could not be learned"
    );
    for other in [
        io::Error::from(io::ErrorKind::NotFound),
        io::Error::other("another failure"),
    ] {
        assert!(!UserUnknown::is(&other));
    }
}

// --- a look at a name for a link -----------------------------------------------------------------

#[cfg(unix)]
#[test]
fn a_link_is_seen_at_its_own_name_whatever_it_leads_to_and_nothing_else_is_one() {
    use std::os::unix::fs::symlink;
    let dir = Scratch::new("fs-link");
    let (folder, file) = (dir.path().join("folder"), dir.path().join("file"));
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(&file, b"content").unwrap();
    std::fs::write(folder.join("inside"), b"content").unwrap();
    let to_a_folder = dir.path().join("to-a-folder");
    let to_a_file = dir.path().join("to-a-file");
    let to_nothing = dir.path().join("to-nothing");
    symlink(&folder, &to_a_folder).unwrap();
    symlink(&file, &to_a_file).unwrap();
    symlink(dir.path().join("missing"), &to_nothing).unwrap();
    let before = std::fs::read_dir(dir.path()).unwrap().count();
    for link in [&to_a_folder, &to_a_file, &to_nothing] {
        assert!(RealFs.is_link(link).unwrap(), "{}", link.display());
    }
    // A folder and a file are none; neither is a name nothing has, nor one under a
    // name that is no folder. A link on the way to a name is followed: what is
    // asked of is the name itself.
    for plain in [
        folder.clone(),
        file.clone(),
        dir.path().join("missing"),
        dir.path().join("missing/deeper"),
        file.join("under-a-file"),
        to_a_folder.join("inside"),
    ] {
        assert!(!RealFs.is_link(&plain).unwrap(), "{}", plain.display());
    }
    // The look made nothing and removed nothing.
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), before);
    assert_eq!(std::fs::read_link(&to_a_folder).unwrap(), folder);
}
