//! A models directory (ADR 0017, "The directory"): one folder, given by whoever opens
//! it, reached only through the store's file-system interface, so that a test can
//! stand between the installer and the disk.
//!
//! Every name under it is made here, from a [`ModelName`], a [`super::FileName`] or
//! the names this layout gives, and from nothing else. No error carries a path out.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use whizbang_core::store::fs::Fs;

use super::ModelName;

/// One models directory. What `{:?}` prints holds no path.
#[derive(Clone)]
pub struct ModelsDir {
    fs: Arc<dyn Fs>,
    root: PathBuf,
}

impl std::fmt::Debug for ModelsDir {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ModelsDir")
    }
}

impl ModelsDir {
    /// The models directory at this folder, which is there, over this file system.
    /// Nothing is read or made until something is asked of it.
    pub fn at(fs: Arc<dyn Fs>, root: &Path) -> ModelsDir {
        ModelsDir {
            fs,
            root: root.to_path_buf(),
        }
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
