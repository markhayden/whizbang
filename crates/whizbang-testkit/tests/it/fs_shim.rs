//! The file-system shim itself: what a crash keeps and what it drops. The store's tests
//! trust these rules, so each has its own test here.

use std::path::Path;

use whizbang_core::store::fs::Fs;
use whizbang_testkit::fs_shim::{Event, Lost, ShimFs};
use whizbang_testkit::scratch::Scratch;

fn on_disk(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

#[test]
fn a_file_that_was_written_and_never_synced_is_gone_after_a_crash() {
    let dir = Scratch::new("shim");
    let fs = ShimFs::new();
    let file = dir.path().join("f");
    fs.write_new(&file, b"content").unwrap();
    assert_eq!(
        fs.read(&file).unwrap(),
        b"content",
        "visible before the crash"
    );
    fs.crash(Lost::Unsynced);
    assert_eq!(on_disk(&file), None);
}

#[test]
fn a_synced_file_in_a_directory_that_was_never_synced_is_gone_after_a_crash() {
    let dir = Scratch::new("shim");
    let fs = ShimFs::new();
    let file = dir.path().join("f");
    fs.write_new(&file, b"content").unwrap();
    fs.sync_file(&file).unwrap();
    fs.crash(Lost::Unsynced);
    assert_eq!(on_disk(&file), None, "the name was never made durable");
}

#[test]
fn a_file_whose_name_is_durable_and_whose_content_is_not_is_torn() {
    let dir = Scratch::new("shim");
    let fs = ShimFs::new();
    let file = dir.path().join("f");
    fs.write_new(&file, b"12345678").unwrap();
    fs.sync_dir(dir.path()).unwrap();
    fs.crash(Lost::Unsynced);
    assert_eq!(
        on_disk(&file).unwrap(),
        b"1234",
        "half of it reached the disk"
    );
}

#[test]
fn a_synced_file_in_a_synced_directory_survives_a_crash() {
    let dir = Scratch::new("shim");
    let fs = ShimFs::new();
    let file = dir.path().join("f");
    fs.write_new(&file, b"content").unwrap();
    fs.sync_file(&file).unwrap();
    fs.sync_dir(dir.path()).unwrap();
    fs.crash(Lost::Unsynced);
    assert_eq!(on_disk(&file).unwrap(), b"content");
}

#[test]
fn syncing_another_directory_does_not_make_a_name_durable() {
    let dir = Scratch::new("shim");
    let fs = ShimFs::new();
    let sub = dir.path().join("sub");
    fs.create_dir(&sub).unwrap();
    fs.sync_dir(dir.path()).unwrap();
    let file = sub.join("f");
    fs.write_new(&file, b"content").unwrap();
    fs.sync_file(&file).unwrap();
    fs.sync_dir(dir.path()).unwrap();
    fs.crash(Lost::Unsynced);
    assert!(sub.is_dir(), "the directory's own name was synced");
    assert_eq!(on_disk(&file), None, "the file's directory was not");
}

#[test]
fn a_directory_whose_name_was_never_synced_is_gone_with_everything_in_it() {
    let dir = Scratch::new("shim");
    let fs = ShimFs::new();
    let sub = dir.path().join("sub");
    fs.create_dir(&sub).unwrap();
    let file = sub.join("f");
    fs.write_new(&file, b"content").unwrap();
    fs.sync_file(&file).unwrap();
    fs.sync_dir(&sub).unwrap();
    fs.crash(Lost::Unsynced);
    assert!(!sub.exists());
}

#[test]
fn a_rename_is_undone_unless_the_destination_directory_was_synced() {
    for synced in [false, true] {
        let dir = Scratch::new("shim");
        let fs = ShimFs::new();
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        fs.create_dir(&a).unwrap();
        fs.create_dir(&b).unwrap();
        fs.write_new(&a.join("new"), b"new").unwrap();
        fs.sync_file(&a.join("new")).unwrap();
        fs.write_new(&b.join("pointer"), b"old").unwrap();
        fs.sync_file(&b.join("pointer")).unwrap();
        fs.sync_dir(&a).unwrap();
        fs.sync_dir(&b).unwrap();
        fs.sync_dir(dir.path()).unwrap();

        fs.rename(&a.join("new"), &b.join("pointer")).unwrap();
        assert_eq!(
            fs.read(&b.join("pointer")).unwrap(),
            b"new",
            "visible at once"
        );
        if synced {
            fs.sync_dir(&b).unwrap();
        }
        fs.crash(Lost::Unsynced);
        if synced {
            assert_eq!(on_disk(&b.join("pointer")).unwrap(), b"new");
            assert_eq!(on_disk(&a.join("new")), None);
        } else {
            assert_eq!(
                on_disk(&b.join("pointer")).unwrap(),
                b"old",
                "the old file is back"
            );
            assert_eq!(
                on_disk(&a.join("new")).unwrap(),
                b"new",
                "under its old name"
            );
        }
    }
}

#[test]
fn a_renamed_file_keeps_its_unsynced_content_under_its_new_name() {
    let dir = Scratch::new("shim");
    let fs = ShimFs::new();
    let (from, to) = (dir.path().join("from"), dir.path().join("to"));
    fs.write_new(&from, b"12345678").unwrap();
    fs.rename(&from, &to).unwrap();
    fs.sync_dir(dir.path()).unwrap();
    fs.crash(Lost::Unsynced);
    assert_eq!(
        on_disk(&to).unwrap(),
        b"1234",
        "renamed into place before it was synced"
    );
}

#[test]
fn a_removal_is_undone_unless_its_directory_was_synced() {
    for synced in [false, true] {
        let dir = Scratch::new("shim");
        let fs = ShimFs::new();
        let sub = dir.path().join("sub");
        fs.create_dir(&sub).unwrap();
        fs.write_new(&sub.join("f"), b"content").unwrap();
        fs.sync_file(&sub.join("f")).unwrap();
        fs.sync_dir(&sub).unwrap();
        fs.sync_dir(dir.path()).unwrap();

        fs.remove_all(&sub).unwrap();
        assert!(!fs.exists(&sub).unwrap());
        if synced {
            fs.sync_dir(dir.path()).unwrap();
        }
        fs.crash(Lost::Unsynced);
        if synced {
            assert!(!sub.exists());
        } else {
            assert_eq!(
                on_disk(&sub.join("f")).unwrap(),
                b"content",
                "the tree is back"
            );
        }
    }
}

#[test]
fn losing_content_only_keeps_every_name_and_tears_what_was_not_synced() {
    let dir = Scratch::new("shim");
    let fs = ShimFs::new();
    let (synced, unsynced, moved) = (
        dir.path().join("synced"),
        dir.path().join("unsynced"),
        dir.path().join("moved"),
    );
    fs.write_new(&synced, b"12345678").unwrap();
    fs.sync_file(&synced).unwrap();
    fs.write_new(&unsynced, b"12345678").unwrap();
    fs.rename(&unsynced, &moved).unwrap();
    fs.crash(Lost::ContentOnly);
    assert_eq!(on_disk(&synced).unwrap(), b"12345678");
    assert_eq!(on_disk(&unsynced), None, "the rename stands");
    assert_eq!(on_disk(&moved).unwrap(), b"1234");
}

#[test]
fn after_a_crash_every_call_fails_and_nothing_more_reaches_the_disk() {
    let dir = Scratch::new("shim");
    let fs = ShimFs::new();
    let file = dir.path().join("f");
    fs.write_new(&file, b"content").unwrap();
    fs.sync_file(&file).unwrap();
    fs.sync_dir(dir.path()).unwrap();
    fs.crash(Lost::Unsynced);
    assert!(fs.crashed());
    assert!(fs.write_new(&dir.path().join("g"), b"late").is_err());
    assert!(fs.create_dir(&dir.path().join("d")).is_err());
    assert!(fs.sync_file(&file).is_err());
    assert!(fs.sync_dir(dir.path()).is_err());
    assert!(fs.rename(&file, &dir.path().join("h")).is_err());
    assert!(fs.remove_all(&file).is_err());
    assert!(fs.read(&file).is_err());
    assert!(fs.list(dir.path()).is_err());
    assert!(fs.exists(&file).is_err());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    assert_eq!(on_disk(&file).unwrap(), b"content");
}

#[test]
fn a_crash_point_is_the_number_of_the_event_it_comes_before() {
    let dir = Scratch::new("shim");
    let fs = ShimFs::new().crash_before(2, Lost::Unsynced);
    let file = dir.path().join("f");
    fs.write_new(&file, b"content").unwrap();
    fs.sync_file(&file).unwrap();
    assert!(!fs.crashed());
    assert!(
        fs.sync_dir(dir.path()).is_err(),
        "the third event never happens"
    );
    assert!(fs.crashed());
    assert_eq!(on_disk(&file), None);
    assert_eq!(
        fs.events(),
        vec![Event::Write(file.clone()), Event::SyncFile(file.clone())]
    );
}

#[test]
fn an_injected_failure_refuses_one_call_and_drops_nothing() {
    let dir = Scratch::new("shim");
    let fs = ShimFs::new().fail_before(1);
    let file = dir.path().join("f");
    fs.write_new(&file, b"content").unwrap();
    assert!(fs.sync_file(&file).is_err());
    assert!(!fs.crashed());
    assert_eq!(fs.read(&file).unwrap(), b"content", "still visible");
    fs.sync_file(&file).unwrap();
    assert_eq!(fs.events().len(), 2, "the refused call is not an event");
}

#[test]
fn events_are_recorded_in_order_and_reads_are_not_events() {
    let dir = Scratch::new("shim");
    let fs = ShimFs::new();
    let sub = dir.path().join("sub");
    let (f, g) = (sub.join("f"), sub.join("g"));
    fs.create_dir(&sub).unwrap();
    fs.write_new(&f, b"content").unwrap();
    fs.read(&f).unwrap();
    fs.list(&sub).unwrap();
    fs.exists(&f).unwrap();
    fs.sync_file(&f).unwrap();
    fs.rename(&f, &g).unwrap();
    fs.sync_dir(&sub).unwrap();
    fs.remove_all(&g).unwrap();
    assert_eq!(
        fs.events(),
        vec![
            Event::CreateDir(sub.clone()),
            Event::Write(f.clone()),
            Event::SyncFile(f.clone()),
            Event::Rename(f.clone(), g.clone()),
            Event::SyncDir(sub.clone()),
            Event::Remove(g.clone()),
        ]
    );
}

#[test]
fn syncing_a_file_that_is_not_there_fails() {
    let dir = Scratch::new("shim");
    let fs = ShimFs::new();
    assert!(fs.sync_file(&dir.path().join("missing")).is_err());
    assert!(fs.sync_dir(&dir.path().join("missing")).is_err());
}
