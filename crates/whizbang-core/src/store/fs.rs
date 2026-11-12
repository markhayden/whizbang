//! The file system the store talks to, as a trait, so that a test can stand between the store
//! and the disk, record the order of writes, syncs and renames, and drop what was never
//! synced, as a power loss would. [`RealFs`] is the real one.
//!
//! Durability has two halves. A file's content is durable after [`Fs::sync_file`]. A name in
//! a directory (a new file, a new directory, the new name of a rename, a removal) is durable
//! after [`Fs::sync_dir`] on that directory. Neither implies the other.
//!
//! The store reads through the same trait, so a store that goes on working after its file
//! system has crashed is caught too.

use std::io;
use std::path::{Path, PathBuf};

/// What the store needs from a file system.
pub trait Fs: Send + Sync {
    /// Creates one directory. Its parent exists; the directory does not.
    fn create_dir(&self, path: &Path) -> io::Result<()>;
    /// Creates a file with this content. Fails when the name is taken: the store never
    /// writes over a file, it replaces one by a rename.
    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;
    /// Makes a file's content durable.
    fn sync_file(&self, path: &Path) -> io::Result<()>;
    /// Makes a directory's entries durable.
    fn sync_dir(&self, path: &Path) -> io::Result<()>;
    /// Renames, replacing a file already at `to`. Atomic within one file system.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Removes a file, or a directory with everything in it.
    fn remove_all(&self, path: &Path) -> io::Result<()>;
    /// The whole content of a file.
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    /// The entries of a directory, as full paths, in order.
    fn list(&self, path: &Path) -> io::Result<Vec<PathBuf>>;
    /// Whether something has this name.
    fn exists(&self, path: &Path) -> io::Result<bool>;
}

/// The real file system. What it creates is readable by its owner only.
///
/// On Linux a sync is `fsync`. On macOS the standard library's `sync_all` asks for
/// `F_FULLFSYNC`, which has the drive empty its cache too.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealFs;

impl Fs for RealFs {
    fn create_dir(&self, path: &Path) -> io::Result<()> {
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(path)
    }

    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(path)?.write_all(bytes)
    }

    fn sync_file(&self, path: &Path) -> io::Result<()> {
        std::fs::File::open(path)?.sync_all()
    }

    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        std::fs::File::open(path)?.sync_all()
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }

    fn remove_all(&self, path: &Path) -> io::Result<()> {
        if std::fs::symlink_metadata(path)?.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        }
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn list(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(path)? {
            entries.push(entry?.path());
        }
        entries.sort();
        Ok(entries)
    }

    fn exists(&self, path: &Path) -> io::Result<bool> {
        path.try_exists()
    }
}
