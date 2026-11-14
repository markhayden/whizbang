//! The shim's streamed files and its try at a share (ADR 0018): the events they are, a
//! refusal of each step, and what a crash keeps of a file that was written piece by
//! piece. The rules that the real file system keeps too are held on both in the core's
//! tests (`whizbang-core`, `fs`).
//!
//! What a crash leaves of a streamed file, which these tests pin: every byte a sync made
//! durable, then the first half of the bytes appended since (rounded down), and nothing
//! else; under the name the file has at the crash, if that name survives.

use std::io;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use whizbang_core::store::fs::{Fs, Held, LockFile};
use whizbang_testkit::fs_shim::{Event, Lost, ShimFs, Tail};
use whizbang_testkit::scratch::Scratch;

fn on_disk(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

/// Every way a crash can lose a name of one directory, with whether the one name
/// written there survives it.
const LOSSES: [(Lost, bool); 6] = [
    (Lost::Unsynced, false),
    (Lost::ContentOnly, true),
    (Lost::Subset(0), true),
    (Lost::Subset(1), false),
    (Lost::Names(0), true),
    (Lost::Names(1), false),
];

const FIRST: &[u8] = b"0123456789";
const SECOND: &[u8] = b"abcdef";
const WHOLE: &[u8] = b"0123456789abcdef";

/// A file streamed as an installer would: two pieces, each synced, finished, and its
/// name made durable. Seven events.
fn stream(fs: &ShimFs, dir: &Path) -> io::Result<u64> {
    let file = dir.join("f");
    let mut new = fs.begin_new(&file, 64)?;
    new.append(FIRST)?;
    new.sync()?;
    new.append(SECOND)?;
    new.sync()?;
    let length = new.finish()?;
    fs.sync_dir(dir)?;
    Ok(length)
}

/// The events of [`stream`], in order.
fn stream_events(dir: &Path) -> Vec<Event> {
    let file = dir.join("f");
    vec![
        Event::Begin(file.clone()),
        Event::Append(file.clone()),
        Event::SyncFile(file.clone()),
        Event::Append(file.clone()),
        Event::SyncFile(file.clone()),
        Event::Finish(file.clone()),
        Event::SyncDir(dir.to_path_buf()),
    ]
}

/// A try at a share, made on a thread of its own and waited for ten seconds at most: a
/// try that waited for the holder fails here, and does not hang the test. The thread ends
/// when the try answers, which a waiting one does once the test lets the holder go.
fn tried(fs: &Arc<ShimFs>, path: &Path) -> io::Result<Option<Box<dyn Held>>> {
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

/// What a new file system finds at `path` after a crash, read piece by piece.
fn read_back(path: &Path) -> Vec<u8> {
    let fs = ShimFs::new();
    let mut open = fs.open_file(path).unwrap();
    let mut read = Vec::new();
    // A file of these tests is a few bytes; sixty-four pieces of four are more than any.
    for _ in 0..64 {
        let mut buffer = [0u8; 4];
        let filled = open.read(&mut buffer).unwrap();
        if filled == 0 {
            return read;
        }
        read.extend_from_slice(&buffer[..filled]);
    }
    panic!("a file of a few bytes did not end in sixty-four pieces");
}

// --- events -------------------------------------------------------------------------------------

#[test]
fn each_step_of_a_streamed_file_is_an_event_and_so_is_each_piece_read() {
    let dir = Scratch::new("shim-stream");
    let fs = ShimFs::new();
    assert_eq!(stream(&fs, dir.path()).unwrap(), 16);
    assert_eq!(fs.events(), stream_events(dir.path()));
    assert_eq!(on_disk(&dir.path().join("f")).unwrap(), WHOLE);

    let file = dir.path().join("f");
    let fs = ShimFs::new();
    let mut open = fs.open_file(&file).unwrap();
    let mut buffer = [0u8; 10];
    assert_eq!(open.read(&mut buffer).unwrap(), 10);
    assert_eq!(open.read(&mut buffer).unwrap(), 6);
    assert_eq!(open.read(&mut buffer).unwrap(), 0);
    assert_eq!(
        fs.events(),
        vec![
            Event::Open(file.clone()),
            Event::Read(file.clone()),
            Event::Read(file.clone()),
            Event::Read(file.clone()),
        ]
    );
    // What the interface refuses itself is no event: nothing was asked of the disk.
    assert!(fs.open_file(&dir.path().join("missing")).is_err());
    assert!(open.read(&mut []).is_err());
    assert!(fs.begin_new(&file, 64).is_err(), "the name is taken");
    assert_eq!(fs.events().len(), 4);
}

// --- a refusal of each step -----------------------------------------------------------------------

#[test]
fn a_refusal_of_any_step_of_a_streamed_file_fails_it_there_and_drops_nothing() {
    // What is on the disk when the step numbered so is refused: what the steps before
    // it wrote, whole, since a refusal is no crash.
    let after: [Option<&[u8]>; 7] = [
        None,
        Some(b""),
        Some(FIRST),
        Some(FIRST),
        Some(WHOLE),
        Some(WHOLE),
        Some(WHOLE),
    ];
    for (step, expected) in after.iter().enumerate() {
        let dir = Scratch::new("shim-stream-refused");
        let fs = ShimFs::new().fail_before(step);
        assert!(stream(&fs, dir.path()).is_err(), "step {step}");
        assert!(!fs.crashed());
        assert_eq!(
            fs.events(),
            stream_events(dir.path())[..step],
            "step {step}: the refused call is not recorded, and nothing follows it"
        );
        assert_eq!(
            on_disk(&dir.path().join("f")).as_deref(),
            *expected,
            "step {step}"
        );
        // The refusal was used up: the same steps pass on the same file system.
        let again = Scratch::new("shim-stream-refused-again");
        assert_eq!(stream(&fs, again.path()).unwrap(), 16, "step {step}");
    }
}

#[test]
fn a_handle_whose_step_was_refused_answers_nothing_more() {
    let dir = Scratch::new("shim-stream-broken");
    let file = dir.path().join("f");

    // An append.
    let fs = ShimFs::new();
    let mut new = fs.begin_new(&file, 64).unwrap();
    new.append(FIRST).unwrap();
    new.sync().unwrap();
    fs.refuse_matching(0, |event| matches!(event, Event::Append(_)));
    assert!(new.append(SECOND).is_err());
    let before = fs.events();
    assert!(new.append(SECOND).is_err());
    assert!(new.sync().is_err());
    assert!(new.finish().is_err(), "it was whole before the refusal");
    assert_eq!(fs.events(), before, "and none of them reached the disk");
    assert_eq!(on_disk(&file).unwrap(), FIRST);

    // A sync: what it did not make durable is never called whole.
    let file = dir.path().join("g");
    let mut new = fs.begin_new(&file, 64).unwrap();
    new.append(FIRST).unwrap();
    fs.refuse_matching(0, |event| matches!(event, Event::SyncFile(_)));
    assert!(new.sync().is_err());
    let before = fs.events();
    assert!(new.sync().is_err(), "not even by a second sync");
    assert!(new.finish().is_err());
    assert_eq!(fs.events(), before);
    fs.crash(Lost::ContentOnly);
    assert_eq!(on_disk(&file).unwrap(), b"01234", "it was never synced");
}

#[test]
fn a_refused_finish_says_nothing_of_a_file_that_is_all_there() {
    let dir = Scratch::new("shim-stream-finish");
    let file = dir.path().join("f");
    let fs = ShimFs::new();
    let mut new = fs.begin_new(&file, 64).unwrap();
    new.append(FIRST).unwrap();
    new.sync().unwrap();
    fs.refuse_matching(0, |event| matches!(event, Event::Finish(_)));
    assert!(new.finish().is_err());
    assert_eq!(fs.events().len(), 3);
    fs.crash(Lost::ContentOnly);
    assert_eq!(on_disk(&file).unwrap(), FIRST);
}

#[test]
fn opening_and_each_read_can_be_refused_and_a_refused_read_ends_the_reading() {
    let dir = Scratch::new("shim-read-refused");
    let file = dir.path().join("f");
    std::fs::write(&file, WHOLE).unwrap();
    let fs = ShimFs::new();

    fs.refuse_matching(0, |event| matches!(event, Event::Open(_)));
    assert!(fs.open_file(&file).is_err());
    assert_eq!(fs.events(), vec![]);

    // The second piece of three.
    let mut open = fs.open_file(&file).unwrap();
    fs.refuse_matching(1, |event| matches!(event, Event::Read(_)));
    let mut buffer = [0u8; 6];
    assert_eq!(open.read(&mut buffer).unwrap(), 6);
    assert!(open.read(&mut buffer).is_err());
    let before = fs.events();
    assert_eq!(
        before,
        vec![Event::Open(file.clone()), Event::Read(file.clone())]
    );
    assert!(
        open.read(&mut buffer).is_err(),
        "what follows a lost piece is not handed over as the next"
    );
    assert_eq!(fs.events(), before);
}

// --- a crash --------------------------------------------------------------------------------------

/// In the nextest group of the crash sweeps, by its name.
#[test]
fn a_crash_at_any_event_of_a_streamed_file_keeps_what_was_synced_and_half_of_the_rest() {
    // What the file holds when the crash comes in place of the event numbered so, if
    // its name survives: all that was synced, and the first half of what was appended
    // since.
    let kept: [Option<&[u8]>; 7] = [
        None,
        Some(b""),
        Some(b"01234"),
        Some(FIRST),
        Some(b"0123456789abc"),
        Some(WHOLE),
        Some(WHOLE),
    ];
    for (event, kept) in kept.iter().enumerate() {
        for (lost, survives) in LOSSES {
            let dir = Scratch::new("shim-stream-crash");
            let fs = ShimFs::new().crash_before(event, lost);
            assert!(stream(&fs, dir.path()).is_err(), "event {event}, {lost:?}");
            assert!(fs.crashed());
            assert_eq!(fs.events(), stream_events(dir.path())[..event]);
            let expected = if survives { *kept } else { None };
            assert_eq!(
                on_disk(&dir.path().join("f")).as_deref(),
                expected,
                "event {event}, {lost:?}"
            );
            assert_eq!(
                fs.crash_names(),
                usize::from(event > 0),
                "the one name that was not durable"
            );
        }
    }
    // With all seven done, the name is durable too: whole, whatever is lost.
    for (lost, _) in LOSSES {
        let dir = Scratch::new("shim-stream-crash");
        let fs = ShimFs::new();
        assert_eq!(stream(&fs, dir.path()).unwrap(), 16);
        fs.crash(lost);
        assert_eq!(on_disk(&dir.path().join("f")).unwrap(), WHOLE, "{lost:?}");
        assert_eq!(fs.crash_names(), 0);
    }
}

#[test]
fn a_file_streamed_and_synced_is_whole_after_a_crash_finished_or_not() {
    for finished in [false, true] {
        for (lost, survives) in LOSSES {
            let dir = Scratch::new("shim-stream-synced");
            let file = dir.path().join("f");
            let fs = ShimFs::new();
            let mut new = fs.begin_new(&file, 64).unwrap();
            new.append(FIRST).unwrap();
            new.append(SECOND).unwrap();
            new.sync().unwrap();
            if finished {
                assert_eq!(new.finish().unwrap(), 16);
            } else {
                drop(new);
            }
            fs.crash(lost);
            // The content is durable; the name is another matter, as for any file.
            let expected = survives.then_some(WHOLE);
            assert_eq!(on_disk(&file).as_deref(), expected, "{lost:?}");
        }
    }
}

#[test]
fn a_file_streamed_and_not_synced_is_gone_or_cut_after_a_crash() {
    // (what was appended and never synced, what is left of it)
    let cases: [(&[&[u8]], &[u8]); 5] = [
        (&[], b""),
        (&[b"x"], b""),
        (&[b"xy"], b"x"),
        (&[b"0123", b"456"], b"012"),
        (&[b"0123", b"", b"4567"], b"0123"),
    ];
    for (appended, left) in cases {
        for (lost, survives) in LOSSES {
            let dir = Scratch::new("shim-stream-unsynced");
            let file = dir.path().join("f");
            let fs = ShimFs::new();
            let mut new = fs.begin_new(&file, 64).unwrap();
            for piece in appended {
                new.append(piece).unwrap();
            }
            fs.crash(lost);
            let expected = survives.then_some(left);
            assert_eq!(
                on_disk(&file).as_deref(),
                expected,
                "{appended:?}, {lost:?}"
            );
        }
    }
}

#[test]
fn after_a_crash_a_reader_is_given_what_was_synced_and_at_most_half_of_the_rest() {
    // (synced, appended since, what a reader is given)
    let cases: [(&[u8], &[u8], &[u8]); 4] = [
        (b"synced", b"", b"synced"),
        (b"synced", b"x", b"synced"),
        (b"synced", b"0123456", b"synced012"),
        (b"", b"0123456", b"012"),
    ];
    for (synced, since, given) in cases {
        let dir = Scratch::new("shim-stream-reader");
        let file = dir.path().join("f");
        let fs = ShimFs::new();
        let mut new = fs.begin_new(&file, 64).unwrap();
        new.append(synced).unwrap();
        new.sync().unwrap();
        new.append(since).unwrap();
        // Before the crash a reader of the same file system is given all of it.
        let mut all = synced.to_vec();
        all.extend_from_slice(since);
        assert_eq!(fs.read_at_most(&file, 64).unwrap(), all);
        fs.crash(Lost::ContentOnly);

        let read = read_back(&file);
        assert_eq!(read, given);
        // Never short of what was synced, never bytes that were not appended, and
        // never all of what was not synced.
        assert!(read.starts_with(synced));
        assert!(all.starts_with(&read));
        assert!(since.is_empty() || read.len() < all.len());
    }
}

#[test]
fn after_a_crash_no_handle_reaches_the_disk() {
    let dir = Scratch::new("shim-stream-after");
    let (file, other) = (dir.path().join("f"), dir.path().join("g"));
    std::fs::write(&other, WHOLE).unwrap();
    let fs = ShimFs::new();
    let mut new = fs.begin_new(&file, 64).unwrap();
    new.append(FIRST).unwrap();
    new.sync().unwrap();
    let mut open = fs.open_file(&other).unwrap();
    let share = fs.try_share(&other).unwrap().unwrap();
    fs.sync_dir(dir.path()).unwrap();
    let events = fs.events();
    fs.crash(Lost::Unsynced);

    assert!(new.append(SECOND).is_err());
    assert!(new.sync().is_err());
    assert!(new.finish().is_err());
    assert!(open.read(&mut [0u8; 4]).is_err());
    assert!(fs.begin_new(&dir.path().join("h"), 64).is_err());
    assert!(fs.open_file(&other).is_err());
    assert!(fs.try_share(&other).is_err());
    assert_eq!(fs.events(), events);
    assert_eq!(on_disk(&file).unwrap(), FIRST, "nothing was added");
    assert!(!dir.path().join("h").exists());
    // The share went with the process, though its value is still here.
    assert!(ShimFs::new().try_alone(&other).unwrap().is_some());
    drop(share);
}

// --- a streamed file under another name, and a name used again --------------------------------------

#[test]
fn bytes_not_synced_are_cut_under_the_name_the_file_has_at_the_crash() {
    // The file renamed.
    let dir = Scratch::new("shim-stream-renamed");
    let (file, renamed) = (dir.path().join("f"), dir.path().join("g"));
    let fs = ShimFs::new();
    let mut new = fs.begin_new(&file, 64).unwrap();
    new.append(FIRST).unwrap();
    new.sync().unwrap();
    new.append(SECOND).unwrap();
    fs.rename(&file, &renamed).unwrap();
    // The handle writes on, to the same file, and its events name it as it is now.
    new.append(b"gh").unwrap();
    assert_eq!(fs.events().last(), Some(&Event::Append(renamed.clone())));
    fs.crash(Lost::ContentOnly);
    assert_eq!(on_disk(&renamed).unwrap(), b"0123456789abcd");
    assert_eq!(on_disk(&file), None);

    // The folder it is in renamed, as a staged folder is.
    let dir = Scratch::new("shim-stream-folder");
    let (staging, place) = (dir.path().join("staging"), dir.path().join("place"));
    let fs = ShimFs::new();
    fs.create_dir(&staging).unwrap();
    let mut new = fs.begin_new(&staging.join("f"), 64).unwrap();
    new.append(FIRST).unwrap();
    new.sync().unwrap();
    new.append(SECOND).unwrap();
    drop(new);
    fs.rename(&staging, &place).unwrap();
    fs.crash(Lost::ContentOnly);
    assert_eq!(on_disk(&place.join("f")).unwrap(), b"0123456789abc");
}

#[test]
fn a_name_used_again_is_not_cut_for_the_file_that_had_it_before() {
    // Removed, and written again whole.
    let dir = Scratch::new("shim-stream-again");
    let file = dir.path().join("f");
    let fs = ShimFs::new();
    let mut new = fs.begin_new(&file, 64).unwrap();
    new.append(b"01").unwrap();
    drop(new);
    fs.remove_all(&file).unwrap();
    fs.write_new(&file, WHOLE).unwrap();
    fs.crash(Lost::ContentOnly);
    assert_eq!(on_disk(&file).unwrap(), WHOLE);

    // Its folder removed, and made again.
    let dir = Scratch::new("shim-stream-again");
    let folder = dir.path().join("d");
    let file = folder.join("f");
    let fs = ShimFs::new();
    fs.create_dir(&folder).unwrap();
    let mut new = fs.begin_new(&file, 64).unwrap();
    new.append(b"01").unwrap();
    drop(new);
    fs.remove_all(&folder).unwrap();
    fs.create_dir(&folder).unwrap();
    fs.write_new(&file, WHOLE).unwrap();
    fs.crash(Lost::ContentOnly);
    assert_eq!(on_disk(&file).unwrap(), WHOLE);

    // Replaced by a rename of a file that is whole.
    let dir = Scratch::new("shim-stream-again");
    let (file, whole) = (dir.path().join("f"), dir.path().join("whole"));
    let fs = ShimFs::new();
    let mut new = fs.begin_new(&file, 64).unwrap();
    new.append(b"01").unwrap();
    drop(new);
    fs.write_new(&whole, WHOLE).unwrap();
    fs.rename(&whole, &file).unwrap();
    fs.crash(Lost::ContentOnly);
    assert_eq!(on_disk(&file).unwrap(), WHOLE);

    // Streamed again under the same name, and synced this time.
    let dir = Scratch::new("shim-stream-again");
    let file = dir.path().join("f");
    let fs = ShimFs::new();
    let mut new = fs.begin_new(&file, 64).unwrap();
    new.append(b"01").unwrap();
    drop(new);
    fs.remove_all(&file).unwrap();
    let mut new = fs.begin_new(&file, 64).unwrap();
    new.append(WHOLE).unwrap();
    new.sync().unwrap();
    assert_eq!(new.finish().unwrap(), 16);
    fs.crash(Lost::ContentOnly);
    assert_eq!(on_disk(&file).unwrap(), WHOLE);
}

// --- a streamed file that was removed or replaced, and comes back at the crash ---------------------

#[test]
fn a_streamed_file_that_a_crash_brings_back_is_cut_as_any_other() {
    // Removed, and the removal lost: what comes back is what a crash leaves of the
    // file, and not all that was ever appended to it.
    let dir = Scratch::new("shim-stream-restored");
    let file = dir.path().join("f");
    let fs = ShimFs::new();
    let mut new = fs.begin_new(&file, 64).unwrap();
    new.append(b"synced").unwrap();
    new.sync().unwrap();
    new.append(FIRST).unwrap();
    fs.sync_dir(dir.path()).unwrap();
    fs.remove_all(&file).unwrap();
    assert_eq!(on_disk(&file), None);
    fs.crash(Lost::Unsynced);
    assert_eq!(on_disk(&file).unwrap(), b"synced01234");

    // Removed with the folder it is in, as a failed download's staging is.
    let dir = Scratch::new("shim-stream-restored");
    let folder = dir.path().join("staging");
    let file = folder.join("f");
    let fs = ShimFs::new();
    fs.create_dir(&folder).unwrap();
    let mut new = fs.begin_new(&file, 64).unwrap();
    new.append(FIRST).unwrap();
    fs.sync_dir(dir.path()).unwrap();
    fs.sync_dir(&folder).unwrap();
    fs.remove_all(&folder).unwrap();
    fs.crash(Lost::Unsynced);
    assert_eq!(on_disk(&file).unwrap(), b"01234");

    // Replaced by a rename of a whole file over it, and the rename lost.
    let dir = Scratch::new("shim-stream-restored");
    let (file, whole) = (dir.path().join("f"), dir.path().join("whole"));
    let fs = ShimFs::new();
    let mut new = fs.begin_new(&file, 64).unwrap();
    new.append(FIRST).unwrap();
    fs.sync_dir(dir.path()).unwrap();
    fs.write_new(&whole, WHOLE).unwrap();
    fs.rename(&whole, &file).unwrap();
    assert_eq!(on_disk(&file).unwrap(), WHOLE);
    fs.crash(Lost::Unsynced);
    assert_eq!(on_disk(&file).unwrap(), b"01234");

    // Each file of a folder comes back by its own count: two streamed files that
    // differ in what was synced, and a file written whole beside them.
    let dir = Scratch::new("shim-stream-restored");
    let folder = dir.path().join("staging");
    let (one, two, plain) = (folder.join("a"), folder.join("b"), folder.join("c"));
    let fs = ShimFs::new();
    fs.create_dir(&folder).unwrap();
    let mut new = fs.begin_new(&one, 64).unwrap();
    new.append(FIRST).unwrap();
    let mut new = fs.begin_new(&two, 64).unwrap();
    new.append(b"synced").unwrap();
    new.sync().unwrap();
    new.append(b"0123").unwrap();
    fs.write_new(&plain, WHOLE).unwrap();
    fs.sync_dir(dir.path()).unwrap();
    fs.sync_dir(&folder).unwrap();
    fs.remove_all(&folder).unwrap();
    fs.crash(Lost::Unsynced);
    assert_eq!(on_disk(&one).unwrap(), b"01234");
    assert_eq!(on_disk(&two).unwrap(), b"synced01");
    assert_eq!(on_disk(&plain).unwrap(), WHOLE);

    // And one whose every byte was synced comes back whole.
    let dir = Scratch::new("shim-stream-restored");
    let file = dir.path().join("f");
    let fs = ShimFs::new();
    let mut new = fs.begin_new(&file, 64).unwrap();
    new.append(FIRST).unwrap();
    new.sync().unwrap();
    assert_eq!(new.finish().unwrap(), 10);
    fs.sync_dir(dir.path()).unwrap();
    fs.remove_all(&file).unwrap();
    fs.crash(Lost::Unsynced);
    assert_eq!(on_disk(&file).unwrap(), FIRST);
}

#[test]
fn a_streamed_file_renamed_onto_itself_is_still_cut_at_a_crash() {
    let dir = Scratch::new("shim-stream-self");
    let file = dir.path().join("f");
    let fs = ShimFs::new();
    let mut new = fs.begin_new(&file, 64).unwrap();
    new.append(b"synced").unwrap();
    new.sync().unwrap();
    new.append(FIRST).unwrap();
    fs.rename(&file, &file).unwrap();
    fs.crash(Lost::ContentOnly);
    assert_eq!(on_disk(&file).unwrap(), b"synced01234");
}

// --- a link that was removed, at a crash -------------------------------------------------------------

#[cfg(unix)]
#[test]
fn a_removed_link_that_a_crash_brings_back_is_the_link_again_and_its_target_is_never_touched() {
    use std::os::unix::fs::symlink;
    for (lost, back) in [(Lost::Unsynced, true), (Lost::ContentOnly, false)] {
        let dir = Scratch::new("shim-link");
        let target = dir.path().join("target");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("kept"), b"kept").unwrap();
        let file = dir.path().join("file");
        std::fs::write(&file, b"a file").unwrap();
        let folder = dir.path().join("folder");
        std::fs::create_dir(&folder).unwrap();
        let alone = dir.path().join("link");
        let (inside, to_a_file) = (folder.join("to-a-folder"), folder.join("to-a-file"));
        symlink(&target, &alone).unwrap();
        symlink(&target, &inside).unwrap();
        symlink(&file, &to_a_file).unwrap();

        let fs = ShimFs::new();
        fs.remove_all(&alone).unwrap();
        fs.remove_all(&folder).unwrap();
        assert_eq!(
            fs.events(),
            vec![Event::Remove(alone.clone()), Event::Remove(folder.clone())]
        );
        fs.crash(lost);
        for link in [&alone, &inside, &to_a_file] {
            let found = std::fs::symlink_metadata(link);
            assert_eq!(found.is_ok(), back, "{lost:?}");
            if back {
                assert!(found.unwrap().is_symlink(), "{lost:?}: a link, not a copy");
            }
        }
        if back {
            assert_eq!(std::fs::read_link(&alone).unwrap(), target);
            assert_eq!(std::fs::read_link(&inside).unwrap(), target);
            assert_eq!(std::fs::read_link(&to_a_file).unwrap(), file);
        }
        assert_eq!(std::fs::read(target.join("kept")).unwrap(), b"kept");
        assert_eq!(std::fs::read_dir(&target).unwrap().count(), 1);
        assert_eq!(std::fs::read(&file).unwrap(), b"a file");
    }
}

// --- what a crash leaves of the bytes that were not synced, by a test's choice ---------------------

/// Each choice, with what it leaves of a file of `synced` and then `0123456789`.
const TAILS: [(Tail, &[u8]); 4] = [
    (Tail::Half, b"synced01234"),
    (Tail::Zeroed, b"synced\0\0\0\0\0\0\0\0\0\0"),
    (Tail::All, b"synced0123456789"),
    (Tail::None, b"synced"),
];

/// A file of six synced bytes and ten that are not, its name durable.
fn with_a_tail(fs: &ShimFs, file: &Path) {
    let mut new = fs.begin_new(file, 64).unwrap();
    new.append(b"synced").unwrap();
    new.sync().unwrap();
    new.append(FIRST).unwrap();
    fs.sync_dir(file.parent().unwrap()).unwrap();
}

#[test]
fn each_choice_of_tail_leaves_exactly_what_it_says_of_a_file_that_stays() {
    assert_eq!(Tail::default(), Tail::Half);
    for (tail, left) in TAILS {
        for (lost, _) in LOSSES {
            let dir = Scratch::new("shim-tail-stays");
            let file = dir.path().join("f");
            let fs = ShimFs::new().with_tail(tail);
            with_a_tail(&fs, &file);
            fs.crash(lost);
            assert_eq!(on_disk(&file).unwrap(), left, "{tail:?}, {lost:?}");
            assert_eq!(read_back(&file), left, "{tail:?}");
        }
    }
    // The choice can be made on a shim in use, and a file with nothing unsynced is
    // whole under every one.
    for (tail, _) in TAILS {
        let dir = Scratch::new("shim-tail-stays");
        let file = dir.path().join("f");
        let fs = ShimFs::new();
        let mut new = fs.begin_new(&file, 64).unwrap();
        new.append(FIRST).unwrap();
        new.sync().unwrap();
        fs.set_tail(tail);
        fs.crash(Lost::ContentOnly);
        assert_eq!(on_disk(&file).unwrap(), FIRST, "{tail:?}");
    }
    // A file never synced: nothing of it is durable, and each choice says what is left.
    for (tail, left) in [
        (Tail::Half, &b"01234"[..]),
        (Tail::Zeroed, &b"\0\0\0\0\0\0\0\0\0\0"[..]),
        (Tail::All, FIRST),
        (Tail::None, &b""[..]),
    ] {
        let dir = Scratch::new("shim-tail-stays");
        let file = dir.path().join("f");
        let fs = ShimFs::new().with_tail(tail);
        let mut new = fs.begin_new(&file, 64).unwrap();
        new.append(FIRST).unwrap();
        fs.crash(Lost::ContentOnly);
        assert_eq!(on_disk(&file).unwrap(), left, "{tail:?}");
    }
}

#[test]
fn each_choice_of_tail_leaves_exactly_what_it_says_of_a_file_that_was_renamed() {
    for (tail, left) in TAILS {
        // The rename kept, and the rename lost: the same file under one name or the
        // other.
        for (lost, kept) in [(Lost::ContentOnly, true), (Lost::Unsynced, false)] {
            let dir = Scratch::new("shim-tail-renamed");
            let (file, renamed) = (dir.path().join("f"), dir.path().join("g"));
            let fs = ShimFs::new().with_tail(tail);
            with_a_tail(&fs, &file);
            fs.rename(&file, &renamed).unwrap();
            fs.crash(lost);
            let (there, gone) = if kept {
                (&renamed, &file)
            } else {
                (&file, &renamed)
            };
            assert_eq!(on_disk(there).unwrap(), left, "{tail:?}, {lost:?}");
            assert_eq!(on_disk(gone), None, "{tail:?}, {lost:?}");
        }
    }
}

#[test]
fn each_choice_of_tail_leaves_exactly_what_it_says_of_a_file_that_was_removed_and_comes_back() {
    for (tail, left) in TAILS {
        // Removed by itself.
        let dir = Scratch::new("shim-tail-removed");
        let file = dir.path().join("f");
        let fs = ShimFs::new().with_tail(tail);
        with_a_tail(&fs, &file);
        fs.remove_all(&file).unwrap();
        fs.crash(Lost::Unsynced);
        assert_eq!(on_disk(&file).unwrap(), left, "{tail:?}");

        // Removed with its folder.
        let dir = Scratch::new("shim-tail-removed");
        let folder = dir.path().join("staging");
        let file = folder.join("f");
        let fs = ShimFs::new().with_tail(tail);
        fs.create_dir(&folder).unwrap();
        fs.sync_dir(dir.path()).unwrap();
        with_a_tail(&fs, &file);
        fs.remove_all(&folder).unwrap();
        fs.crash(Lost::Unsynced);
        assert_eq!(on_disk(&file).unwrap(), left, "{tail:?}");

        // Replaced by a rename over it.
        let dir = Scratch::new("shim-tail-removed");
        let (file, whole) = (dir.path().join("f"), dir.path().join("whole"));
        let fs = ShimFs::new().with_tail(tail);
        with_a_tail(&fs, &file);
        fs.write_new(&whole, WHOLE).unwrap();
        fs.rename(&whole, &file).unwrap();
        fs.crash(Lost::Unsynced);
        assert_eq!(on_disk(&file).unwrap(), left, "{tail:?}");

        // And a removal that the crash keeps leaves nothing, whatever the choice.
        let dir = Scratch::new("shim-tail-removed");
        let file = dir.path().join("f");
        let fs = ShimFs::new().with_tail(tail);
        with_a_tail(&fs, &file);
        fs.remove_all(&file).unwrap();
        fs.crash(Lost::ContentOnly);
        assert_eq!(on_disk(&file), None, "{tail:?}");
    }
}

// --- the try at a share ---------------------------------------------------------------------------

/// What holds the file when the share is tried.
#[derive(Debug, Clone, Copy)]
enum Holder {
    Nobody,
    Share,
    TryShare,
    Alone,
    LockFile,
}

/// A lock of either kind, kept until dropped.
enum Kept {
    Nothing,
    Lock(#[allow(dead_code)] Box<dyn Held>),
    LockFile(#[allow(dead_code)] Box<dyn LockFile>),
}

fn hold(fs: &ShimFs, file: &Path, holder: Holder) -> Kept {
    match holder {
        Holder::Nobody => Kept::Nothing,
        Holder::Share => Kept::Lock(fs.share(file).unwrap()),
        Holder::TryShare => Kept::Lock(fs.try_share(file).unwrap().unwrap()),
        Holder::Alone => Kept::Lock(fs.try_alone(file).unwrap().unwrap()),
        Holder::LockFile => Kept::LockFile(fs.hold_lock_file(file, false).unwrap().unwrap()),
    }
}

#[test]
fn a_try_at_a_share_is_given_or_refused_at_once_whoever_holds_the_file() {
    for holder in [
        Holder::Nobody,
        Holder::Share,
        Holder::TryShare,
        Holder::Alone,
        Holder::LockFile,
    ] {
        // Held by the process that tries, and by another.
        for own in [true, false] {
            let dir = Scratch::new("shim-try");
            let file = dir.path().join("f");
            std::fs::write(&file, b"content").unwrap();
            let (trier, other) = (Arc::new(ShimFs::new()), Arc::new(ShimFs::new()));
            let by = if own { &trier } else { &other };
            let kept = hold(by, &file, holder);
            let events = trier.events().len();

            let given = matches!(holder, Holder::Nobody | Holder::Share | Holder::TryShare);
            let got = tried(&trier, &file).unwrap();
            assert_eq!(got.is_some(), given, "{holder:?}, own: {own}");
            assert_eq!(
                trier.events().len(),
                events + usize::from(given),
                "{holder:?}, own: {own}: a share that was given is an event, and a \
                 refused try is none"
            );
            if given {
                assert_eq!(trier.events().last(), Some(&Event::TryShare(file.clone())));
            }

            // With the holder gone, a refused try is given; and what was given keeps
            // the file from being taken alone, by its own process and by the other.
            drop(kept);
            let got = match got {
                Some(got) => got,
                None => tried(&trier, &file)
                    .unwrap()
                    .expect("the holder let go, and the file is free"),
            };
            assert!(trier.try_alone(&file).unwrap().is_none(), "{holder:?}");
            assert!(other.try_alone(&file).unwrap().is_none(), "{holder:?}");
            assert!(other.hold_lock_file(&file, false).unwrap().is_none());
            drop(got);
            assert!(other.try_alone(&file).unwrap().is_some(), "{holder:?}");
            assert_eq!(on_disk(&file).unwrap(), b"content");
        }
    }
}

#[test]
fn a_crash_ends_what_a_try_was_refused_for_and_the_share_a_try_was_given() {
    let dir = Scratch::new("shim-try-crash");
    let file = dir.path().join("f");
    std::fs::write(&file, b"content").unwrap();
    let (trier, other) = (Arc::new(ShimFs::new()), Arc::new(ShimFs::new()));

    let alone = other.try_alone(&file).unwrap().unwrap();
    assert!(tried(&trier, &file).unwrap().is_none());
    other.crash(Lost::Unsynced);
    let share = tried(&trier, &file).unwrap();
    assert!(share.is_some(), "the one who had it alone is gone");
    drop(alone);

    let third = ShimFs::new();
    assert!(third.try_alone(&file).unwrap().is_none());
    trier.crash(Lost::Unsynced);
    assert!(
        third.try_alone(&file).unwrap().is_some(),
        "and so is the share"
    );
    assert!(tried(&trier, &file).is_err(), "it has crashed");
    drop(share);
}

#[test]
fn a_try_at_a_share_can_be_refused_as_a_step_and_then_holds_nothing() {
    let dir = Scratch::new("shim-try-refused");
    let file = dir.path().join("f");
    std::fs::write(&file, b"content").unwrap();
    let fs = ShimFs::new();
    fs.refuse_matching(0, |event| matches!(event, Event::TryShare(_)));
    assert!(fs.try_share(&file).is_err());
    assert_eq!(fs.events(), vec![]);
    assert!(fs.try_alone(&file).unwrap().is_some(), "no share was kept");
    assert!(fs.try_share(&file).unwrap().is_some(), "refused once");

    // And a crash in its place: nothing is given.
    let fs = ShimFs::new().crash_before(0, Lost::Unsynced);
    assert!(fs.try_share(&file).is_err());
    assert!(fs.crashed());
    assert!(ShimFs::new().try_alone(&file).unwrap().is_some());
}
