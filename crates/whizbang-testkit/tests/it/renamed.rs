//! The shim, when a folder is renamed with names in it that are not yet durable: a new
//! file, a removal, a rename inside it. Each such name follows the folder, so a crash
//! loses it under the folder's new name exactly as it would have under the old, unless
//! the folder it is in was synced after the name was made (ADR 0018, "A name that is not
//! durable follows a rename of its folder").
//!
//! Every sequence here is also run on the real file system, with no crash, and what is
//! left there is what the shim leaves when a crash keeps every name.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use whizbang_core::store::fs::{Fs, RealFs};
use whizbang_testkit::fs_shim::{Event, Lost, ShimFs, Tail};
use whizbang_testkit::scratch::Scratch;

const WHOLE: &[u8] = b"0123456789abcdef";

/// What is under a folder: each name from the folder down, a file with its content and
/// a folder with none.
type Listing = BTreeMap<String, Option<Vec<u8>>>;

/// Names to expect, each with a file's content or none for a folder.
type Names<'a> = &'a [(&'a str, Option<&'a [u8]>)];

fn listing(root: &Path) -> Listing {
    let mut found = Listing::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if path.is_dir() {
                found.insert(name, None);
                dirs.push(path);
            } else {
                found.insert(name, Some(std::fs::read(&path).unwrap()));
            }
        }
    }
    found
}

fn expected(names: Names<'_>) -> Listing {
    names
        .iter()
        .map(|(name, content)| ((*name).to_owned(), content.map(<[u8]>::to_vec)))
        .collect()
}

/// Which of the folders a sequence syncs.
#[derive(Debug, Clone, Copy, Default)]
struct Synced {
    /// The folder `a`, after the file was made in it and before it is renamed.
    before: bool,
    /// The folder under its new name `b`, after the rename.
    after: bool,
    /// The folder that holds `a` and `b`, after the rename.
    parent: bool,
}

/// A file made in the folder `a`, whose own name is durable, and `a` renamed to `b`.
fn renamed(fs: &dyn Fs, root: &Path, synced: Synced) -> io::Result<()> {
    let (a, b) = (root.join("a"), root.join("b"));
    fs.create_dir(&a)?;
    fs.sync_dir(root)?;
    fs.write_new(&a.join("f"), WHOLE)?;
    if synced.before {
        fs.sync_dir(&a)?;
    }
    fs.rename(&a, &b)?;
    if synced.after {
        fs.sync_dir(&b)?;
    }
    if synced.parent {
        fs.sync_dir(root)?;
    }
    Ok(())
}

/// The sequence on the real file system, with no crash; and on the shim, where what is
/// left when a crash keeps every name is the same.
fn kept(steps: &dyn Fn(&dyn Fs, &Path) -> io::Result<()>) -> Listing {
    let real = Scratch::new("shim-renamed-real");
    steps(&RealFs, real.path()).unwrap();
    let on_disk = listing(real.path());
    let dir = Scratch::new("shim-renamed-kept");
    let fs = ShimFs::new().with_tail(Tail::All);
    steps(&fs, dir.path()).unwrap();
    assert_eq!(listing(dir.path()), on_disk, "before any crash");
    fs.crash(Lost::ContentOnly);
    assert_eq!(listing(dir.path()), on_disk, "with every name kept");
    on_disk
}

/// What the shim leaves of the sequence when the crash loses as `lost` says.
fn left(steps: &dyn Fn(&dyn Fs, &Path) -> io::Result<()>, lost: Lost) -> Listing {
    let dir = Scratch::new("shim-renamed");
    let fs = ShimFs::new();
    steps(&fs, dir.path()).unwrap();
    fs.crash(lost);
    listing(dir.path())
}

const FILE_IN_A: Names<'static> = &[("a", None), ("a/f", Some(WHOLE))];
const FILE_IN_B: Names<'static> = &[("b", None), ("b/f", Some(WHOLE))];

#[test]
fn a_file_in_a_renamed_folder_is_lost_under_its_new_name_unless_its_folder_was_synced() {
    // (which folders were synced, what a crash that loses every name not durable leaves)
    let no = Synced::default();
    let cases: [(Synced, Names<'_>); 8] = [
        // Nothing synced: the rename goes back, and the file was never durable.
        (no, &[("a", None)]),
        // Its folder synced before the rename: the file is durable, the rename is not.
        (Synced { before: true, ..no }, FILE_IN_A),
        // Its folder synced after the rename, under its new name: the same.
        (Synced { after: true, ..no }, FILE_IN_A),
        // The parent synced: the rename is durable, and the file is not. The folder
        // is in its place without the file.
        (Synced { parent: true, ..no }, &[("b", None)]),
        (
            Synced {
                before: true,
                parent: true,
                ..no
            },
            FILE_IN_B,
        ),
        (
            Synced {
                after: true,
                parent: true,
                ..no
            },
            FILE_IN_B,
        ),
        (
            Synced {
                before: true,
                after: true,
                ..no
            },
            FILE_IN_A,
        ),
        (
            Synced {
                before: true,
                after: true,
                parent: true,
            },
            FILE_IN_B,
        ),
    ];
    for (synced, unsynced_lost) in cases {
        let steps = move |fs: &dyn Fs, root: &Path| renamed(fs, root, synced);
        assert_eq!(kept(&steps), expected(FILE_IN_B), "{synced:?}");
        assert_eq!(
            left(&steps, Lost::Unsynced),
            expected(unsynced_lost),
            "{synced:?}"
        );
    }
}

#[test]
fn a_file_in_a_renamed_folder_fares_by_its_new_name_under_each_way_of_losing_names() {
    // Neither the file's name nor the rename is durable. The folders with names not
    // durable are the parent (the rename) and `b` (the file), in that order; the names
    // are `b` and `b/f`.
    let steps = |fs: &dyn Fs, root: &Path| renamed(fs, root, Synced::default());
    let cases: [(Lost, Names<'_>); 10] = [
        (Lost::Unsynced, &[("a", None)]),
        (Lost::ContentOnly, FILE_IN_B),
        (Lost::Subset(0b00), FILE_IN_B),
        (Lost::Subset(0b01), FILE_IN_A),
        (Lost::Subset(0b10), &[("b", None)]),
        (Lost::Subset(0b11), &[("a", None)]),
        (Lost::Names(0b00), FILE_IN_B),
        (Lost::Names(0b01), FILE_IN_A),
        (Lost::Names(0b10), &[("b", None)]),
        (Lost::Names(0b11), &[("a", None)]),
    ];
    for (lost, leaves) in cases {
        assert_eq!(left(&steps, lost), expected(leaves), "{lost:?}");
    }
    let dir = Scratch::new("shim-renamed-named");
    let fs = ShimFs::new();
    steps(&fs, dir.path()).unwrap();
    fs.crash(Lost::ContentOnly);
    assert_eq!((fs.crash_choices(), fs.crash_names()), (2, 2));
    assert_eq!(
        fs.crash_named(),
        vec![dir.path().join("b"), dir.path().join("b").join("f")],
        "the file is named as it is now"
    );
}

#[test]
fn a_file_two_folders_down_follows_the_rename_of_the_folder_at_the_top() {
    // (whether the folders below the top were synced, what is left)
    let cases: [(bool, Names<'_>); 2] = [
        // Nothing below the top is durable: the top is in its place, empty.
        (false, &[("b", None)]),
        // The folders are durable and the file is not.
        (true, &[("b", None), ("b/x", None), ("b/x/y", None)]),
    ];
    for (folders, leaves) in cases {
        let steps = move |fs: &dyn Fs, root: &Path| {
            let (a, b) = (root.join("a"), root.join("b"));
            fs.create_dir(&a)?;
            fs.create_dir(&a.join("x"))?;
            fs.create_dir(&a.join("x").join("y"))?;
            if folders {
                fs.sync_dir(&a)?;
                fs.sync_dir(&a.join("x"))?;
            }
            fs.write_new(&a.join("x").join("y").join("f"), WHOLE)?;
            fs.rename(&a, &b)?;
            fs.sync_dir(root)
        };
        let all = [
            ("b", None),
            ("b/x", None),
            ("b/x/y", None),
            ("b/x/y/f", Some(WHOLE)),
        ];
        assert_eq!(kept(&steps), expected(&all), "{folders}");
        assert_eq!(left(&steps, Lost::Unsynced), expected(leaves), "{folders}");
    }
    // The file's own folder synced under its new name, after the rename: it is durable.
    let steps = |fs: &dyn Fs, root: &Path| {
        let (a, b) = (root.join("a"), root.join("b"));
        fs.create_dir(&a)?;
        fs.create_dir(&a.join("x"))?;
        fs.sync_dir(&a)?;
        fs.write_new(&a.join("x").join("f"), WHOLE)?;
        fs.rename(&a, &b)?;
        fs.sync_dir(&b.join("x"))?;
        fs.sync_dir(root)
    };
    assert_eq!(
        left(&steps, Lost::Unsynced),
        expected(&[("b", None), ("b/x", None), ("b/x/f", Some(WHOLE))])
    );
}

#[test]
fn a_file_follows_a_folder_that_is_renamed_twice() {
    // (whether the parent is synced at the end, what is left)
    let cases: [(bool, Names<'_>); 2] = [(true, &[("c", None)]), (false, &[("a", None)])];
    for (parent, leaves) in cases {
        let steps = move |fs: &dyn Fs, root: &Path| {
            let (a, b, c) = (root.join("a"), root.join("b"), root.join("c"));
            fs.create_dir(&a)?;
            fs.sync_dir(root)?;
            fs.write_new(&a.join("f"), WHOLE)?;
            fs.rename(&a, &b)?;
            fs.rename(&b, &c)?;
            if parent {
                fs.sync_dir(root)?;
            }
            Ok(())
        };
        assert_eq!(
            kept(&steps),
            expected(&[("c", None), ("c/f", Some(WHOLE))]),
            "{parent}"
        );
        assert_eq!(left(&steps, Lost::Unsynced), expected(leaves), "{parent}");
    }
    // The first rename durable and the second not: the file is lost under the name
    // between the two.
    let steps = |fs: &dyn Fs, root: &Path| {
        let (a, b, c) = (root.join("a"), root.join("b"), root.join("c"));
        fs.create_dir(&a)?;
        fs.sync_dir(root)?;
        fs.write_new(&a.join("f"), WHOLE)?;
        fs.rename(&a, &b)?;
        fs.sync_dir(root)?;
        fs.rename(&b, &c)
    };
    assert_eq!(left(&steps, Lost::Unsynced), expected(&[("b", None)]));
}

#[test]
fn a_file_follows_a_folder_renamed_over_an_empty_folder() {
    // (whether the parent is synced at the end, what is left)
    let cases: [(bool, Names<'_>); 2] = [
        (true, &[("b", None)]),
        // The rename goes back, and the empty folder it replaced is there again.
        (false, &[("a", None), ("b", None)]),
    ];
    for (parent, leaves) in cases {
        let steps = move |fs: &dyn Fs, root: &Path| {
            let (a, b) = (root.join("a"), root.join("b"));
            fs.create_dir(&a)?;
            fs.create_dir(&b)?;
            fs.sync_dir(root)?;
            fs.write_new(&a.join("f"), WHOLE)?;
            fs.rename(&a, &b)?;
            if parent {
                fs.sync_dir(root)?;
            }
            Ok(())
        };
        assert_eq!(kept(&steps), expected(FILE_IN_B), "{parent}");
        assert_eq!(left(&steps, Lost::Unsynced), expected(leaves), "{parent}");
    }
}

#[test]
fn a_whole_file_and_a_streamed_file_in_one_renamed_folder_each_fare_by_their_own_rule() {
    // A file written whole, and one streamed with six bytes synced and ten not.
    fn steps(fs: &dyn Fs, root: &Path, before: bool, parent: bool) -> io::Result<()> {
        let (a, b) = (root.join("a"), root.join("b"));
        fs.create_dir(&a)?;
        fs.sync_dir(root)?;
        fs.write_new(&a.join("w"), WHOLE)?;
        let mut new = fs.begin_new(&a.join("s"), 64)?;
        new.append(b"synced")?;
        new.sync()?;
        new.append(b"0123456789")?;
        if before {
            fs.sync_dir(&a)?;
        }
        fs.rename(&a, &b)?;
        if parent {
            fs.sync_dir(root)?;
        }
        Ok(())
    }
    assert_eq!(
        kept(&|fs: &dyn Fs, root: &Path| steps(fs, root, false, true)),
        expected(&[
            ("b", None),
            ("b/s", Some(b"synced0123456789")),
            ("b/w", Some(WHOLE)),
        ])
    );
    let tails: [(Tail, &[u8]); 4] = [
        (Tail::Half, b"synced01234"),
        (Tail::Zeroed, b"synced\0\0\0\0\0\0\0\0\0\0"),
        (Tail::All, b"synced0123456789"),
        (Tail::None, b"synced"),
    ];
    for (tail, streamed) in tails {
        // (the folder synced before the rename, the parent after, the loss, what is left)
        let both: Names<'_> = &[("b", None), ("b/s", Some(streamed)), ("b/w", Some(WHOLE))];
        let back: Names<'_> = &[("a", None), ("a/s", Some(streamed)), ("a/w", Some(WHOLE))];
        let cases: [(bool, bool, Lost, Names<'_>); 5] = [
            (true, true, Lost::Unsynced, both),
            (false, true, Lost::Unsynced, &[("b", None)]),
            (true, false, Lost::Unsynced, back),
            (false, false, Lost::Unsynced, &[("a", None)]),
            (false, false, Lost::ContentOnly, both),
        ];
        for (before, parent, lost, leaves) in cases {
            let dir = Scratch::new("shim-renamed-both");
            let fs = ShimFs::new().with_tail(tail);
            steps(&fs, dir.path(), before, parent).unwrap();
            fs.crash(lost);
            assert_eq!(
                listing(dir.path()),
                expected(leaves),
                "{tail:?}, {before}, {parent}, {lost:?}"
            );
        }
    }
}

#[test]
fn a_file_whose_sync_was_refused_is_still_cut_after_its_folder_is_renamed() {
    let dir = Scratch::new("shim-renamed-unsynced");
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    let fs = ShimFs::new();
    fs.create_dir(&a).unwrap();
    fs.create_dir(&a.join("x")).unwrap();
    fs.refuse_matching(0, |event| matches!(event, Event::SyncFile(_)));
    assert!(fs.write_new(&a.join("x").join("f"), WHOLE).is_err());
    fs.rename(&a, &b).unwrap();
    fs.crash(Lost::ContentOnly);
    assert_eq!(
        listing(dir.path()),
        expected(&[("b", None), ("b/x", None), ("b/x/f", Some(b"01234567"))])
    );
}

#[test]
fn a_rename_and_a_removal_inside_a_folder_follow_the_folders_rename() {
    // A file renamed inside the folder, the folder then renamed: the inner rename goes
    // back under the folder's new name.
    let steps = |fs: &dyn Fs, root: &Path| {
        let (a, b) = (root.join("a"), root.join("b"));
        fs.create_dir(&a)?;
        fs.sync_dir(root)?;
        fs.write_new(&a.join("p"), WHOLE)?;
        fs.sync_dir(&a)?;
        fs.rename(&a.join("p"), &a.join("q"))?;
        fs.rename(&a, &b)?;
        fs.sync_dir(root)
    };
    assert_eq!(kept(&steps), expected(&[("b", None), ("b/q", Some(WHOLE))]));
    assert_eq!(
        left(&steps, Lost::Unsynced),
        expected(&[("b", None), ("b/p", Some(WHOLE))])
    );

    // A file removed inside the folder, and a folder with a file in it: each comes back
    // under the folder's new name.
    let steps = |fs: &dyn Fs, root: &Path| {
        let (a, b) = (root.join("a"), root.join("b"));
        fs.create_dir(&a)?;
        fs.sync_dir(root)?;
        fs.write_new(&a.join("f"), WHOLE)?;
        fs.create_dir(&a.join("x"))?;
        fs.write_new(&a.join("x").join("g"), b"inside")?;
        fs.sync_dir(&a)?;
        fs.sync_dir(&a.join("x"))?;
        fs.remove_all(&a.join("f"))?;
        fs.remove_all(&a.join("x"))?;
        fs.rename(&a, &b)?;
        fs.sync_dir(root)
    };
    assert_eq!(kept(&steps), expected(&[("b", None)]));
    assert_eq!(
        left(&steps, Lost::Unsynced),
        expected(&[
            ("b", None),
            ("b/f", Some(WHOLE)),
            ("b/x", None),
            ("b/x/g", Some(b"inside")),
        ])
    );
}

#[test]
fn a_renamed_file_keeps_the_fate_of_its_own_names_as_before() {
    // The store's way: a file written under one name and renamed to another in the
    // same folder. Nothing of this changed: with the folder synced it is there under
    // its new name, and without it is nowhere.
    for (synced, leaves) in [(true, &[("final", Some(WHOLE))][..]), (false, &[][..])] {
        let steps = move |fs: &dyn Fs, root: &Path| {
            fs.write_new(&root.join("tmp"), WHOLE)?;
            fs.rename(&root.join("tmp"), &root.join("final"))?;
            if synced {
                fs.sync_dir(root)?;
            }
            Ok(())
        };
        assert_eq!(kept(&steps), expected(&[("final", Some(WHOLE))]));
        assert_eq!(left(&steps, Lost::Unsynced), expected(leaves), "{synced}");
    }
}

// --- a file whose sync was refused, removed or replaced, and brought back ---------------------------

#[test]
fn a_file_whose_sync_was_refused_comes_back_cut_to_half_when_a_crash_undoes_its_removal() {
    // Removed by itself, with its folder, and replaced by a rename over it.
    for how in ["removed", "removed with its folder", "replaced"] {
        let dir = Scratch::new("shim-unsynced-restored");
        let folder = dir.path().join("d");
        let (file, whole) = (folder.join("f"), dir.path().join("whole"));
        let fs = ShimFs::new();
        fs.create_dir(&folder).unwrap();
        fs.refuse_matching(0, |event| matches!(event, Event::SyncFile(_)));
        assert!(fs.write_new(&file, WHOLE).is_err());
        fs.sync_dir(dir.path()).unwrap();
        fs.sync_dir(&folder).unwrap();
        match how {
            "removed" => fs.remove_all(&file).unwrap(),
            "removed with its folder" => fs.remove_all(&folder).unwrap(),
            _ => {
                fs.write_new(&whole, b"another, whole").unwrap();
                fs.rename(&whole, &file).unwrap();
            }
        }
        fs.crash(Lost::Unsynced);
        assert_eq!(std::fs::read(&file).unwrap(), b"01234567", "{how}");
    }

    // And a whole file renamed over one whose sync was refused is whole when the
    // rename is kept: it is not cut for the file it replaced.
    let dir = Scratch::new("shim-unsynced-restored");
    let (file, whole) = (dir.path().join("f"), dir.path().join("whole"));
    let fs = ShimFs::new();
    fs.refuse_matching(0, |event| matches!(event, Event::SyncFile(_)));
    assert!(fs.write_new(&file, WHOLE).is_err());
    fs.write_new(&whole, b"another, whole").unwrap();
    fs.rename(&whole, &file).unwrap();
    fs.crash(Lost::ContentOnly);
    assert_eq!(std::fs::read(&file).unwrap(), b"another, whole");
}
