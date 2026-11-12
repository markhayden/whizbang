//! The file-system shim itself: what a crash keeps and what it drops. The store's tests
//! trust these rules, so each has its own test here.

use std::path::Path;
use std::sync::Arc;

use whizbang_core::store::fs::Fs;
use whizbang_testkit::fs_shim::{Event, Lost, ShimFs};
use whizbang_testkit::scratch::Scratch;

fn on_disk(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

#[test]
fn a_write_is_two_events_and_its_content_is_synced_when_it_returns() {
    let dir = Scratch::new("shim");
    let fs = ShimFs::new();
    let file = dir.path().join("f");
    fs.write_new(&file, b"content").unwrap();
    assert_eq!(
        fs.events(),
        vec![Event::Write(file.clone()), Event::SyncFile(file.clone())]
    );
    assert!(fs.write_new(&file, b"again").is_err(), "the name is taken");
    fs.sync_dir(dir.path()).unwrap();
    fs.crash(Lost::Unsynced);
    assert_eq!(on_disk(&file).unwrap(), b"content");
}

#[test]
fn a_file_in_a_directory_that_was_never_synced_is_gone_after_a_crash() {
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
    assert_eq!(on_disk(&file), None, "the name was never made durable");
}

#[test]
fn a_crash_between_a_files_write_and_its_sync_leaves_no_file_or_half_of_it() {
    for (lost, expected) in [
        (Lost::Unsynced, None),
        (Lost::ContentOnly, Some(b"1234".to_vec())),
        (Lost::Subset(0), Some(b"1234".to_vec())),
        (Lost::Subset(1), None),
    ] {
        let dir = Scratch::new("shim");
        let fs = ShimFs::new().crash_before(1, lost);
        let file = dir.path().join("f");
        assert!(fs.write_new(&file, b"12345678").is_err());
        assert!(fs.crashed());
        assert_eq!(fs.events(), vec![Event::Write(file.clone())]);
        assert_eq!(on_disk(&file), expected, "{lost:?}");
    }
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
    fs.sync_dir(&sub).unwrap();
    fs.crash(Lost::Unsynced);
    assert!(!sub.exists());
}

/// Two synced directories, `a` holding `new` and `b` holding `pointer`.
fn two_directories(dir: &Scratch, fs: &ShimFs) {
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    fs.create_dir(&a).unwrap();
    fs.create_dir(&b).unwrap();
    fs.write_new(&a.join("new"), b"new").unwrap();
    fs.write_new(&b.join("pointer"), b"old").unwrap();
    fs.sync_dir(&a).unwrap();
    fs.sync_dir(&b).unwrap();
    fs.sync_dir(dir.path()).unwrap();
}

#[test]
fn a_rename_is_undone_unless_the_destination_directory_was_synced() {
    for synced in [false, true] {
        let dir = Scratch::new("shim");
        let fs = ShimFs::new();
        two_directories(&dir, &fs);
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));

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
fn a_lost_rename_out_of_a_directory_that_is_durably_gone_leaves_the_file_nowhere() {
    let dir = Scratch::new("shim");
    let fs = ShimFs::new();
    two_directories(&dir, &fs);
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    fs.rename(&a.join("new"), &b.join("moved")).unwrap();
    fs.remove_all(&a).unwrap();
    fs.sync_dir(dir.path()).unwrap();
    // The removal of `a` is durable; the new name in `b` is not.
    fs.crash(Lost::Unsynced);
    assert!(!a.exists());
    assert_eq!(on_disk(&b.join("moved")), None);
    assert_eq!(on_disk(&b.join("pointer")).unwrap(), b"old");
}

#[test]
fn a_removal_is_undone_unless_its_directory_was_synced() {
    for synced in [false, true] {
        let dir = Scratch::new("shim");
        let fs = ShimFs::new();
        let sub = dir.path().join("sub");
        fs.create_dir(&sub).unwrap();
        fs.write_new(&sub.join("f"), b"content").unwrap();
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
fn losing_content_only_keeps_every_name() {
    let dir = Scratch::new("shim");
    let fs = ShimFs::new();
    two_directories(&dir, &fs);
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    fs.rename(&a.join("new"), &b.join("pointer")).unwrap();
    fs.write_new(&a.join("later"), b"later").unwrap();
    fs.crash(Lost::ContentOnly);
    assert_eq!(
        on_disk(&b.join("pointer")).unwrap(),
        b"new",
        "the rename stands"
    );
    assert_eq!(on_disk(&a.join("later")).unwrap(), b"later");
    assert_eq!(fs.crash_choices(), 2);
}

#[test]
fn a_subset_loses_the_names_of_the_chosen_directories_and_keeps_the_rest() {
    // Names not yet durable in two directories: a new file in `a`, and in `b` a rename
    // over `pointer`. In order of their paths, `a` is bit 0 and `b` is bit 1.
    for mask in 0..4u64 {
        let dir = Scratch::new("shim");
        let fs = ShimFs::new();
        two_directories(&dir, &fs);
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        fs.write_new(&a.join("later"), b"later").unwrap();
        fs.rename(&a.join("new"), &b.join("pointer")).unwrap();
        fs.crash(Lost::Subset(mask));
        assert_eq!(fs.crash_choices(), 2);

        let a_lost = mask & 1 == 1;
        let b_lost = mask & 2 == 2;
        assert_eq!(on_disk(&a.join("later")).is_none(), a_lost, "mask {mask}");
        let pointer = on_disk(&b.join("pointer")).unwrap();
        assert_eq!(pointer == b"old", b_lost, "mask {mask}");
        assert_eq!(on_disk(&a.join("new")).is_some(), b_lost, "mask {mask}");
    }
}

#[test]
fn after_a_crash_every_call_fails_and_nothing_more_reaches_the_disk() {
    let dir = Scratch::new("shim");
    let fs = ShimFs::new();
    let file = dir.path().join("f");
    fs.write_new(&file, b"content").unwrap();
    fs.sync_dir(dir.path()).unwrap();
    fs.crash(Lost::Unsynced);
    assert!(fs.crashed());
    assert!(fs.write_new(&dir.path().join("g"), b"late").is_err());
    assert!(fs.create_dir(&dir.path().join("d")).is_err());
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
    let fs = ShimFs::new().fail_before(2);
    let file = dir.path().join("f");
    fs.write_new(&file, b"content").unwrap();
    assert!(fs.sync_dir(dir.path()).is_err());
    assert!(!fs.crashed());
    assert_eq!(fs.read(&file).unwrap(), b"content", "still visible");
    fs.sync_dir(dir.path()).unwrap();
    assert_eq!(fs.events().len(), 3, "the refused call is not an event");

    fs.refuse_before(3);
    assert!(fs.remove_all(&file).is_err());
    fs.remove_all(&file).unwrap();
}

#[test]
fn a_call_can_be_refused_by_what_it_is() {
    let dir = Scratch::new("shim");
    let fs = ShimFs::new();
    let sub = dir.path().join("sub");
    fs.create_dir(&sub).unwrap();
    // The second sync of a directory from now.
    fs.refuse_matching(1, |event| matches!(event, Event::SyncDir(_)));
    fs.write_new(&sub.join("f"), b"content").unwrap();
    fs.sync_dir(&sub).unwrap();
    assert!(fs.sync_dir(dir.path()).is_err());
    assert!(!fs.crashed());
    fs.sync_dir(dir.path()).unwrap();
    assert_eq!(fs.events().len(), 5, "the refused call is not an event");
}

#[test]
fn a_write_refused_at_its_sync_leaves_the_file_behind() {
    let dir = Scratch::new("shim");
    let fs = ShimFs::new().fail_before(1);
    let file = dir.path().join("f");
    assert!(fs.write_new(&file, b"content").is_err());
    assert!(fs.exists(&file).unwrap(), "written, and never synced");
    assert!(
        fs.write_new(&file, b"content").is_err(),
        "the name is taken"
    );
}

#[test]
fn a_rename_between_two_file_systems_is_refused_and_is_not_an_event() {
    let dir = Scratch::new("shim");
    let other = dir.path().join("other");
    let fs = ShimFs::new().cross_device(&other);
    fs.create_dir(&other).unwrap();
    fs.write_new(&other.join("f"), b"content").unwrap();
    let before = fs.events().len();
    let error = fs
        .rename(&other.join("f"), &dir.path().join("f"))
        .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::CrossesDevices);
    assert_eq!(fs.events().len(), before);
    fs.rename(&other.join("f"), &other.join("g")).unwrap();
}

#[test]
fn a_pause_runs_its_hook_before_the_event_and_the_call_then_goes_on() {
    let dir = Scratch::new("shim");
    let fs = Arc::new(ShimFs::new());
    let (first, second) = (dir.path().join("first"), dir.path().join("second"));
    let hook = {
        let (fs, second) = (fs.clone(), second.clone());
        move || fs.write_new(&second, b"from the hook").unwrap()
    };
    fs.pause_before(0, hook);
    fs.write_new(&first, b"paused").unwrap();
    assert_eq!(
        fs.events(),
        vec![
            Event::Write(second.clone()),
            Event::SyncFile(second.clone()),
            Event::Write(first.clone()),
            Event::SyncFile(first.clone()),
        ]
    );

    // A hook that crashes the shim: the paused call fails.
    let fs = Arc::new(ShimFs::new());
    let hook = {
        let fs = fs.clone();
        move || fs.crash(Lost::Unsynced)
    };
    fs.pause_before(0, hook);
    assert!(fs.create_dir(&dir.path().join("d")).is_err());
    assert!(!dir.path().join("d").exists());
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
fn syncing_a_directory_that_is_not_there_fails() {
    let dir = Scratch::new("shim");
    let fs = ShimFs::new();
    assert!(fs.sync_dir(&dir.path().join("missing")).is_err());
}
