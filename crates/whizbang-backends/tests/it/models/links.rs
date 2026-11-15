//! A folder of the layout that is a link (ADR 0017, "A folder of the layout that is a
//! link"): `models/`, a model's folder, its `installations/`, `staging/` and `holds/`.
//! A removal, an install, a staging and a hold each refuse it in fixed words, and
//! list, delete and make nothing through it. The directory's own folder may be a
//! link. Each link here is made by the test on the real disk, as a user makes one
//! who moves a folder to another disk and leaves a link where it was.

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use whizbang_backends::models::{
    Current, HoldRefused, Holds, InstallRefused, InstallationRecord, ModelsDir, RemoveRefused,
    StageRefused, stage,
};
use whizbang_core::contract::Cancellation;
use whizbang_core::store::fs::Fs;
use whizbang_testkit::fs_shim::{Event, ShimFs};
use whizbang_testkit::scratch::Scratch;

use super::holds::hold;
use super::none;
use super::publishing::{
    first, installations, installed, installing, is_whole, reading, record_of, second, tiny,
};
use super::remove::{Hand, removed, removing};
use super::staging::{incoming, serving_both};

/// A folder of the layout.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Folder {
    Models,
    Model,
    Installations,
    Staging,
    Holds,
}

/// The four of the reviewer's probe, and `models/` itself.
const FOLDERS: [Folder; 5] = [
    Folder::Installations,
    Folder::Staging,
    Folder::Model,
    Folder::Holds,
    Folder::Models,
];

/// Those inside a model's folder: a swap of one of them leaves the model's own
/// files, its lock and its mark among them, where they were.
const INSIDE: [Folder; 3] = [Folder::Installations, Folder::Staging, Folder::Holds];

impl Folder {
    fn under(self, root: &Path) -> PathBuf {
        match self {
            Folder::Models => root.join("models"),
            Folder::Model => tiny(root),
            Folder::Installations => tiny(root).join("installations"),
            Folder::Staging => tiny(root).join("staging"),
            Folder::Holds => tiny(root).join("holds"),
        }
    }
}

/// Everything under a folder, by its name: a folder, a link as where it leads and
/// never followed, a file as its bytes.
fn seen(root: &Path) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut left = vec![root.to_path_buf()];
    while let Some(dir) = left.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.strip_prefix(root).unwrap().to_string_lossy();
            let kind = std::fs::symlink_metadata(&path).unwrap();
            let what = if kind.is_symlink() {
                let leads = std::fs::read_link(&path).unwrap();
                format!("a link to {}", leads.display())
            } else if kind.is_dir() {
                left.push(path.clone());
                "a folder".to_owned()
            } else {
                format!("a file: {:?}", std::fs::read(&path).unwrap())
            };
            found.push((name.into_owned(), what));
        }
    }
    found.sort();
    found
}

/// The model `tiny` installed from the first entry, and beside its installation
/// what an install and a removal delete: a file that is no installation's, and the
/// staging of an install that died.
fn furnished(root: &Path) -> InstallationRecord {
    let record = installed(root, &first());
    std::fs::write(tiny(root).join("installations/notes.txt"), b"x").unwrap();
    std::fs::create_dir_all(incoming(root).join("files")).unwrap();
    std::fs::write(incoming(root).join("files/graph.onnx"), b"part").unwrap();
    record
}

/// Moves a folder of the layout out of the models directory and leaves a link to
/// it at its name. Where it went.
fn moved_out(root: &Path, which: Folder, outside: &Path) -> PathBuf {
    let (from, to) = (which.under(root), outside.join("moved"));
    std::fs::rename(&from, &to).unwrap();
    symlink(&to, &from).unwrap();
    to
}

/// Takes the link away and puts the folder back where it was.
fn moved_back(root: &Path, which: Folder, moved: &Path) {
    std::fs::remove_file(which.under(root)).unwrap();
    std::fs::rename(moved, which.under(root)).unwrap();
}

/// A refusal for a link: one line of fixed words, and no path.
fn fixed_words(refused: &(impl std::fmt::Display + std::fmt::Debug), roots: &[&Path]) {
    let said = refused.to_string();
    assert_eq!(
        said,
        "a folder of the models directory is a link, and nothing is done through it"
    );
    let printed = format!("{said} {refused:?}");
    assert!(!printed.contains('/'), "{printed}");
    for root in roots {
        assert!(!printed.contains(&*root.to_string_lossy()), "{printed}");
    }
}

fn never() {
    panic!("the removal waited");
}

const SECOND: Duration = Duration::from_secs(1);

/// The index of the first event of this kind among the events of a clean install
/// of the second entry over a furnished directory.
fn clean_install_index(what: impl Fn(&Event) -> bool) -> usize {
    let scratch = Scratch::new("links-events");
    furnished(scratch.path());
    let fs = Arc::new(ShimFs::new());
    installing(&fs, scratch.path(), &second(), &serving_both()).unwrap();
    fs.events().iter().position(what).unwrap()
}

/// A shim that moves this folder out, and leaves a link, just before the call that
/// would be its event number `event`: what another process could do at that moment.
/// What the folder outside held just after it was moved is kept to compare with.
type Seen = Arc<Mutex<Vec<(String, String)>>>;

fn swapping(root: &Path, which: Folder, outside: &Path, event: usize) -> (Arc<ShimFs>, Seen) {
    let fs = Arc::new(ShimFs::new());
    let then: Seen = Seen::default();
    let (root, outside, keep) = (root.to_path_buf(), outside.to_path_buf(), then.clone());
    fs.pause_before(event, move || {
        moved_out(&root, which, &outside);
        *keep.lock().unwrap() = seen(&outside);
    });
    (fs, then)
}

// --- a removal -----------------------------------------------------------------------------------

#[test]
fn a_removal_refuses_a_folder_of_the_layout_that_is_a_link_and_deletes_nothing_through_it() {
    for which in FOLDERS {
        let scratch = Scratch::new("links-remove");
        let outside = Scratch::new("links-remove-outside");
        let root = scratch.path();
        let record = furnished(root);
        let moved = moved_out(root, which, outside.path());
        let (inside, out) = (seen(root), seen(outside.path()));
        assert!(out.len() > 1, "{which:?}: nothing is there to delete");
        let fs = Arc::new(ShimFs::new());
        let ended = removing(fs.clone(), root, &Hand::new(), SECOND, &mut never);
        assert_eq!(ended, Err(RemoveRefused::Linked), "{which:?}");
        fixed_words(&RemoveRefused::Linked, &[root, outside.path()]);
        // Nothing was asked of the disk but the look: no lock, no mark, no deleting.
        assert_eq!(fs.events(), [], "{which:?}");
        assert_eq!(seen(outside.path()), out, "{which:?}: changed outside");
        assert_eq!(seen(root), inside, "{which:?}: changed inside");
        // With the folder back in its place, the model is removed.
        moved_back(root, which, &moved);
        let ended = removed(root).unwrap();
        assert_eq!(ended.installation(), Some(record.identity()), "{which:?}");
        assert!(installations(root).is_empty());
        assert!(!incoming(root).exists());
    }
}

#[test]
fn a_folder_swapped_for_a_link_while_a_removal_waits_for_the_lock_is_refused_once_it_has_it() {
    for which in INSIDE {
        let scratch = Scratch::new("links-remove-swap");
        let outside = Scratch::new("links-remove-swap-outside");
        let root = scratch.path();
        let record = furnished(root);
        // The removal has looked, and found no link. Before its first step the
        // folder is moved out, with a link left: it looks again under the model's
        // lock, before it lists anything.
        let (fs, then) = swapping(root, which, outside.path(), 0);
        let ended = removing(fs.clone(), root, &Hand::new(), SECOND, &mut never);
        assert_eq!(ended, Err(RemoveRefused::Linked), "{which:?}");
        assert_eq!(
            seen(outside.path()),
            *then.lock().unwrap(),
            "{which:?}: changed outside"
        );
        // Its mark, which it made, is taken off; nothing else was removed, and
        // the pointer names what it named.
        let mark = tiny(root).join("DRAINING");
        let removals: Vec<Event> = fs
            .events()
            .into_iter()
            .filter(|event| matches!(event, Event::Remove(_)))
            .collect();
        assert_eq!(removals, [Event::Remove(mark.clone())], "{which:?}");
        assert!(!mark.exists());
        if which != Folder::Installations {
            assert_eq!(reading(root), Current::Installed(record));
        }
    }
}

// --- an install, and its cleaning ----------------------------------------------------------------

#[test]
fn an_install_refuses_a_folder_of_the_layout_that_is_a_link_and_cleans_nothing_through_it() {
    for which in FOLDERS {
        let scratch = Scratch::new("links-install");
        let outside = Scratch::new("links-install-outside");
        let root = scratch.path();
        furnished(root);
        let moved = moved_out(root, which, outside.path());
        let (inside, out) = (seen(root), seen(outside.path()));
        let fs = Arc::new(ShimFs::new());
        let fake = serving_both();
        // An install, which would delete the file that is no installation's and
        // the staging that was left; and a staging alone, which would remove the
        // staging.
        let ended = installing(&fs, root, &second(), &fake);
        assert_eq!(ended, Err(InstallRefused::Linked), "{which:?}");
        fixed_words(&InstallRefused::Linked, &[root, outside.path()]);
        let dir = ModelsDir::at(fs.clone(), root);
        let staged = stage(&dir, &second(), &fake, &Cancellation::new(), &none());
        assert_eq!(staged.err(), Some(StageRefused::Linked), "{which:?}");
        fixed_words(&StageRefused::Linked, &[root, outside.path()]);
        // Nothing was asked of the disk but the look, and nothing of the downloader.
        assert_eq!(fs.events(), [], "{which:?}");
        assert!(fake.requests().is_empty(), "{which:?}");
        assert_eq!(seen(outside.path()), out, "{which:?}: changed outside");
        assert_eq!(seen(root), inside, "{which:?}: changed inside");
        // With the folder back in its place, the install cleans and installs.
        moved_back(root, which, &moved);
        let again = installing(&Arc::new(ShimFs::new()), root, &second(), &fake);
        assert_eq!(again, Ok(record_of(&second())), "{which:?}");
        assert_eq!(installations(root), [record_of(&second()).identity().hex()]);
    }
}

#[test]
fn a_folder_swapped_for_a_link_after_an_install_looked_is_refused_before_it_cleans() {
    for which in INSIDE {
        let scratch = Scratch::new("links-install-swap");
        let outside = Scratch::new("links-install-swap-outside");
        let root = scratch.path();
        let record = furnished(root);
        // The install has looked, and found no link. Before its first step the
        // folder is moved out, with a link left: the cleaning looks again before it
        // lists, and the unstaging before it removes.
        let (fs, then) = swapping(root, which, outside.path(), 0);
        let fake = serving_both();
        let ended = installing(&fs, root, &second(), &fake);
        assert_eq!(ended, Err(InstallRefused::Linked), "{which:?}");
        assert!(fake.requests().is_empty(), "{which:?}");
        assert_eq!(
            seen(outside.path()),
            *then.lock().unwrap(),
            "{which:?}: changed outside"
        );
        if which != Folder::Installations {
            assert_eq!(reading(root), Current::Installed(record));
        }
    }
}

#[test]
fn a_staging_folder_swapped_for_a_link_after_a_staging_looked_is_not_removed_through() {
    let scratch = Scratch::new("links-stage-swap");
    let outside = Scratch::new("links-stage-swap-outside");
    let root = scratch.path();
    furnished(root);
    let (fs, then) = swapping(root, Folder::Staging, outside.path(), 0);
    let fake = serving_both();
    let dir = ModelsDir::at(fs.clone(), root);
    let staged = stage(&dir, &second(), &fake, &Cancellation::new(), &none());
    assert_eq!(staged.err(), Some(StageRefused::Linked));
    assert!(fake.requests().is_empty());
    // What the dead install left is where the folder went, as it was.
    assert_eq!(seen(outside.path()), *then.lock().unwrap());
    assert!(
        outside
            .path()
            .join("moved/incoming/files/graph.onnx")
            .exists()
    );
}

#[test]
fn a_folder_swapped_for_a_link_inside_a_download_is_refused_before_anything_is_published() {
    // Before the first file is begun: the cleaning and the staging have looked.
    let begun = clean_install_index(|event| matches!(event, Event::Begin(_)));
    for which in INSIDE {
        let scratch = Scratch::new("links-publish-swap");
        let outside = Scratch::new("links-publish-swap-outside");
        let root = scratch.path();
        let record = furnished(root);
        let (fs, then) = swapping(root, which, outside.path(), begun);
        let ended = installing(&fs, root, &second(), &serving_both());
        assert_eq!(ended, Err(InstallRefused::Linked), "{which:?}");
        let (after, then) = (seen(outside.path()), then.lock().unwrap().clone());
        if which == Folder::Staging {
            // The download went on into the folder where it now is: that is the
            // moment this record does not defend. Nothing that was there is gone,
            // and what was staged is not removed through the link.
            for kept in &then {
                assert!(after.contains(kept), "{kept:?} is gone");
            }
            assert!(outside.path().join("moved/incoming/files").is_dir());
        } else {
            assert_eq!(after, then, "{which:?}: changed outside");
        }
        // Nothing was published: no pointer was written, and no folder renamed in.
        let published = |event: &Event| {
            matches!(event, Event::Rename(_, to)
                if to.ends_with("CURRENT") || to.parent().is_some_and(|p| p.ends_with("installations")))
        };
        assert!(!fs.events().iter().any(published), "{which:?}");
        if which != Folder::Installations {
            assert_eq!(reading(root), Current::Installed(record));
        }
    }
}

// --- a hold --------------------------------------------------------------------------------------

#[test]
fn a_hold_is_refused_through_a_folder_of_the_layout_that_is_a_link_and_makes_nothing() {
    for which in FOLDERS {
        let scratch = Scratch::new("links-hold");
        let outside = Scratch::new("links-hold-outside");
        let root = scratch.path();
        let record = furnished(root);
        let moved = moved_out(root, which, outside.path());
        let (inside, out) = (seen(root), seen(outside.path()));
        let fs = Arc::new(ShimFs::new());
        let holds = Holds::over(ModelsDir::at(fs.clone(), root));
        let refused = hold(&holds, &record).err();
        assert_eq!(refused, Some(HoldRefused::Linked), "{which:?}");
        fixed_words(&HoldRefused::Linked, &[root, outside.path()]);
        // Not the directory's lock, and no share of a hold file out there.
        assert_eq!(fs.events(), [], "{which:?}");
        assert_eq!(holds.count(&super::model("tiny"), record.identity()), 0);
        assert_eq!(seen(outside.path()), out, "{which:?}: changed outside");
        assert_eq!(seen(root), inside, "{which:?}: changed inside");
        moved_back(root, which, &moved);
        let held = hold(&holds, &record).unwrap();
        assert_eq!(held.installation(), record.identity());
    }
}

// --- the directory's own folder ------------------------------------------------------------------

#[test]
fn a_models_directory_whose_own_folder_is_a_link_installs_holds_and_removes() {
    // The whole directory on another disk: the folder it is opened at is a link.
    let real = Scratch::new("links-root-real");
    let scratch = Scratch::new("links-root");
    let root = scratch.path().join("models-directory");
    symlink(real.path(), &root).unwrap();
    let fs = Arc::new(ShimFs::new());
    let record = installing(&fs, &root, &first(), &serving_both()).unwrap();
    is_whole(real.path(), &record);
    assert_eq!(reading(&root), Current::Installed(record.clone()));
    let holds = Holds::over(ModelsDir::at(fs.clone(), &root));
    let held = hold(&holds, &record).unwrap();
    drop(held);
    let next = installing(&fs, &root, &second(), &serving_both()).unwrap();
    assert_eq!(installations(real.path()), [next.identity().hex()]);
    let shim: Arc<dyn Fs> = fs;
    let ended = removing(shim, &root, &Hand::new(), SECOND, &mut never).unwrap();
    assert_eq!(ended.installation(), Some(next.identity()));
    assert!(installations(real.path()).is_empty());
    assert_eq!(reading(&root), Current::NotInstalled);
    assert!(root.is_symlink(), "the link is as it was");
}
