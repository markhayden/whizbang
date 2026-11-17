//! A models directory (ADR 0017, "The directory"): one folder, given by whoever opens
//! it, reached only through the store's file-system interface, so that a test can
//! stand between the installer and the disk.
//!
//! Every name under it is made here, from a [`ModelName`], a [`super::FileName`] or
//! the names this layout gives, and from nothing else. No error carries a path out.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use whizbang_core::contract::Clock;
use whizbang_core::store::fs::{Fs, LockFile, UserUnknown};

use super::ModelName;

/// How long the directory's lock is asked for, by the clock a directory was opened
/// with, before it is said to be busy.
const LOCK_WAIT: Duration = Duration::from_secs(5);

/// What the clock is asked to let pass between two tries for the directory's lock.
const LOCK_PACE: Duration = Duration::from_millis(1);

/// The most times the directory's lock is tried, whatever the clock says: a clock
/// that stands still, as a test's does, never comes to the time, and the tries must
/// end all the same. Under a clock that keeps time the time comes first: five
/// seconds hold 5,000 tries at this pace.
const LOCK_TRIES: usize = 20_000;

/// What reading a small file of the directory found.
pub(super) enum Read {
    /// The file's bytes: all of them, or the most asked for and one more.
    Bytes(Vec<u8>),
    /// Nothing has the name, or a folder on the way to it is not there.
    Missing,
    /// What has the name is not a regular file under its own name: a link, a folder.
    NotAFile,
}

/// What is carried, inside an error of the file system, by a refusal of a folder of
/// the layout that is a link ([`ModelsDir::unlinked`]). It holds no path. Each kind
/// of refusal here is made from an error of the file system, and looks for this
/// first ([`is_linked`]), so that every way out of the installer, a hold and a
/// removal says the same fixed words of it.
#[derive(Debug)]
struct Linked;

/// The words every refusal for a link says.
pub(super) const LINKED: &str =
    "a folder of the models directory is a link, and nothing is done through it";

impl std::fmt::Display for Linked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(LINKED)
    }
}

impl std::error::Error for Linked {}

/// Whether this error of the file system is the refusal of a folder of the layout
/// that is a link.
pub(super) fn is_linked(error: &io::Error) -> bool {
    error.get_ref().is_some_and(|inner| inner.is::<Linked>())
}

/// One models directory. What `{:?}` prints holds no path.
#[derive(Clone)]
pub struct ModelsDir {
    fs: Arc<dyn Fs>,
    root: PathBuf,
    clock: Option<Arc<dyn Clock>>,
}

/// Why a models directory was not opened. Fixed words that say how to mend it, and
/// no path: nothing of the folder's name is quoted, as in no refusal here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenRefused {
    /// The folder is this user's, and its mode lets its group, or anyone, write it.
    /// Nothing in it was read.
    OthersMayWrite,
    /// The folder is another user's, whatever its mode: its owner can write it, and
    /// is not the user this process runs as. Nothing in it was read.
    NotOwn,
    /// The user this process runs as could not be learned, so whose the folder is
    /// cannot be told. Nothing in it was read, and nothing was made.
    UserUnknown,
    /// The file system refused, or cannot say whose a folder is. The kind of failure,
    /// and no path.
    Disk(io::ErrorKind),
}

impl std::fmt::Display for OpenRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            OpenRefused::OthersMayWrite => {
                "the models directory can be written by its group or by anyone, and is not used; take that away (chmod go-w on the folder) and ask again"
            }
            OpenRefused::NotOwn => {
                "the models directory belongs to another user, and is not used; name a folder of your own, or make this one yours (chown) and ask again"
            }
            OpenRefused::UserUnknown => {
                "which user whizbang runs as could not be learned, so whose the models directory is cannot be told, and it is not used; nothing in it was read and nothing was made; ask again"
            }
            OpenRefused::Disk(_) => "the models directory could not be made or looked at",
        })
    }
}

impl std::error::Error for OpenRefused {}

impl From<io::Error> for OpenRefused {
    fn from(error: io::Error) -> OpenRefused {
        if UserUnknown::is(&error) {
            return OpenRefused::UserUnknown;
        }
        OpenRefused::Disk(error.kind())
    }
}

impl std::fmt::Debug for ModelsDir {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ModelsDir")
    }
}

impl ModelsDir {
    /// The models directory at this folder, which is there, over this file system.
    /// Nothing is read or made until something is asked of it.
    ///
    /// **Nothing is asked of the folder either:** not whose it is, nor who may write
    /// it. This is for a folder its caller made and answers for, which is a test's;
    /// no source file of a product crate names this, under any name: the binary
    /// reaches a models directory only through [`ModelsDir::open`]
    /// (`directory::the_binary_names_a_models_directory_only_through_the_opening_that_asks_whose_it_is`).
    /// And it is given no clock: its lock is tried up to the ceiling of tries,
    /// giving way to other threads between two, as before there was a time.
    pub fn at(fs: Arc<dyn Fs>, root: &Path) -> ModelsDir {
        ModelsDir {
            fs,
            root: root.to_path_buf(),
            clock: None,
        }
    }

    /// Opens the models directory at this folder, for its owner alone (ADR 0017, "A
    /// models directory is its owner's alone"), with the clock its lock is asked for
    /// by ([`ModelsDir::lock`]).
    ///
    /// - **Where nothing has the name, the folder is made,** readable and writable by
    ///   its owner only, as every folder under it is, and its name made durable. The
    ///   folder that is to hold it must be there.
    /// - **A folder that is there is asked whose it is before anything else**
    ///   ([`Fs::who_may_write`]), and nothing in it is read, listed, locked or made
    ///   when it is refused: one that is another user's ([`OpenRefused::NotOwn`]),
    ///   and one of this user's whose mode lets its group or anyone write it
    ///   ([`OpenRefused::OthersMayWrite`]). Its mode is not changed for its user:
    ///   the refusal says how to mend it. A folder that others may read and none of
    ///   them write is taken as it is.
    /// - **A link at the name is followed,** as everywhere: the folder asked about
    ///   is the one the name leads to.
    ///
    /// - **Where the user this process runs as cannot be learned,** the directory is
    ///   not opened ([`OpenRefused::UserUnknown`]): that failure is never read as a
    ///   folder that is not there, so nothing is made to be refused afterwards.
    ///
    /// **A look, when it is opened.** A mode or an owner changed afterwards is not
    /// seen, and the folders above this one are not looked at.
    pub fn open(
        fs: Arc<dyn Fs>,
        root: &Path,
        clock: Arc<dyn Clock>,
    ) -> Result<ModelsDir, OpenRefused> {
        let dir = ModelsDir {
            fs,
            root: root.to_path_buf(),
            clock: Some(clock),
        };
        let writers = match dir.fs.who_may_write(root) {
            Ok(writers) => writers,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                dir.ensure_dir(root)?;
                // What was just made is asked as what was found is: a folder that
                // came to the name first is another's until it says otherwise.
                dir.fs.who_may_write(root)?
            }
            Err(error) => return Err(error.into()),
        };
        if !writers.owner_is_this_user {
            return Err(OpenRefused::NotOwn);
        }
        if writers.group || writers.anyone {
            return Err(OpenRefused::OthersMayWrite);
        }
        Ok(dir)
    }

    pub(super) fn fs(&self) -> &dyn Fs {
        &*self.fs
    }

    /// `models/`: the folder that holds a folder for each model and nothing else.
    pub(super) fn models(&self) -> PathBuf {
        self.root.join("models")
    }

    /// `models/<model>/`: one model's folder.
    pub(super) fn model(&self, model: &ModelName) -> PathBuf {
        self.models().join(model.as_str())
    }

    /// `models/<model>/staging/`: where an install of the model keeps its work.
    pub(super) fn staging(&self, model: &ModelName) -> PathBuf {
        self.model(model).join("staging")
    }

    /// `models/<model>/staging/incoming/`: the one staging folder of the model.
    pub(super) fn incoming(&self, model: &ModelName) -> PathBuf {
        self.staging(model).join("incoming")
    }

    /// Whether no folder of the model's layout is a link: `models/` under the
    /// directory, the model's folder under it, and the model's `installations/`,
    /// `staging/` and `holds/`. Each name is looked at itself, without following it
    /// ([`Fs::is_link`]); a name that nothing has is no link. With a link at one of
    /// them the answer is an error that [`is_linked`] knows, and whoever asked does
    /// nothing through it: lists nothing, deletes nothing, makes nothing.
    ///
    /// The directory's own folder is not looked at: it may be a link, which is how
    /// the whole directory is put on another disk.
    ///
    /// **A look, not a guard.** A folder that is swapped for a link after this
    /// answered is followed by the call that comes next (ADR 0017, "A folder of the
    /// layout that is a link").
    pub(super) fn unlinked(&self, model: &ModelName) -> io::Result<()> {
        let folder = self.model(model);
        // The model's folder before what is in it: a name under a link is a name
        // of what the link leads to.
        let layout = [
            self.models(),
            folder.clone(),
            folder.join("installations"),
            folder.join("staging"),
            folder.join("holds"),
        ];
        for path in &layout {
            if self.fs.is_link(path)? {
                return Err(io::Error::other(Linked));
            }
        }
        Ok(())
    }

    /// Removes the model's staging folder and syncs the removal, for one who holds
    /// the model's lock. What cannot be removed now is removed by the next to stage.
    /// Nothing is removed through a folder of the layout that is a link.
    pub(super) fn unstage(&self, model: &ModelName) {
        if self.unlinked(model).is_err() {
            return;
        }
        let _ = self.remove(&self.incoming(model));
        let _ = self.fs.sync_dir(&self.staging(model));
    }

    /// The directory's lock (`LOCK`), taken alone for a moment: around every change
    /// of a pointer and the making of a hold file, and around the reading of the
    /// mark that a publication decides from. `None` when another did not let it go.
    ///
    /// **It is asked for until a time** (ADR 0017, "Three kinds of lock"). The store's
    /// interface has no lock taken alone that waits, and one that waits with no end
    /// is not wanted: a lease, and a removal's wait, must each end. So the lock is
    /// tried; when it is not given, the clock the directory was opened with is asked
    /// to let a millisecond pass ([`Clock::pause`]), and it is tried again, until
    /// five seconds of that clock have passed since the first try. Then the answer
    /// is busy, and nothing is asked of the clock after it. The machine's clock
    /// sleeps through a pause; a test's does not.
    ///
    /// **And at most 20,000 times,** whatever the clock says: under a clock that
    /// stands still the time never comes, and the count ends the tries. A directory
    /// that was given no clock ([`ModelsDir::at`]) has the count alone, and gives
    /// way to other threads between two tries.
    ///
    /// Whoever has the lock waits for nothing while it has it, so it is let go after
    /// a few calls of the file system. The first try comes before anything is asked
    /// of the clock: a lock that is free costs no look at the time.
    pub(super) fn lock(&self) -> io::Result<Option<Box<dyn LockFile>>> {
        let path = self.root.join("LOCK");
        // Set at the first try that was refused. A time too far for the clock to
        // count is no time: the ceiling ends the tries.
        let mut until = None;
        for tried in 1..=LOCK_TRIES {
            if let Some(lock) = self.fs.hold_lock_file(&path, true)? {
                return Ok(Some(lock));
            }
            if tried == LOCK_TRIES {
                break;
            }
            let Some(clock) = &self.clock else {
                std::thread::yield_now();
                continue;
            };
            let now = clock.now();
            let until = *until.get_or_insert_with(|| now.checked_add(LOCK_WAIT));
            if until.is_some_and(|until| now >= until) {
                break;
            }
            clock.pause(LOCK_PACE);
        }
        Ok(None)
    }

    /// Reads a small file whizbang wrote: at most `limit` bytes and one more, so
    /// that one that is too long is seen to be. Only a regular file under its own
    /// name is read: a link is not followed.
    pub(super) fn read(&self, path: &Path, limit: u64) -> io::Result<Read> {
        let mut file = match self.fs.open_file(path) {
            Ok(file) => file,
            Err(error) => {
                return match error.kind() {
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory => Ok(Read::Missing),
                    io::ErrorKind::InvalidData => Ok(Read::NotAFile),
                    _ => Err(error),
                };
            }
        };
        let mut bytes = vec![
            0;
            usize::try_from(limit)
                .unwrap_or(usize::MAX)
                .saturating_add(1)
        ];
        let filled = file.read(&mut bytes)?;
        bytes.truncate(filled);
        Ok(Read::Bytes(bytes))
    }

    /// Makes durable the names that lead to a model's folder: `models/` in the
    /// directory, and the model's folder in `models/`. A folder that was found made
    /// was made by one who may have been refused the sync of its name.
    pub(super) fn sync_names(&self) -> io::Result<()> {
        self.fs.sync_dir(&self.root)?;
        self.fs.sync_dir(&self.models())
    }

    /// Makes a folder that may be there already, and makes its name durable when it
    /// was made. It is made and not looked for first: of two who make it at once, one
    /// makes it and the other finds it made.
    pub(super) fn ensure_dir(&self, path: &Path) -> io::Result<()> {
        match self.fs.create_dir(path) {
            Ok(()) => path
                .parent()
                .map_or(Ok(()), |parent| self.fs.sync_dir(parent)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// Removes a file or a folder with all in it, that may not be there. A link is
    /// removed and not followed.
    pub(super) fn remove(&self, path: &Path) -> io::Result<()> {
        match self.fs.remove_all(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}
