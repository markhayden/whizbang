//! A throwaway directory for one test, in the system temp folder, removed when dropped.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A directory that exists for as long as this value does.
#[derive(Debug)]
pub struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    /// Creates an empty directory whose name holds `label`, the process id and a counter,
    /// so that tests running side by side never share one.
    ///
    /// # Panics
    /// When the directory cannot be created: a test cannot go on without it.
    #[allow(clippy::panic)]
    pub fn new(label: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!(
            "whizbang-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        if let Err(error) = std::fs::create_dir_all(&dir) {
            panic!("cannot create {}: {error}", dir.display());
        }
        Scratch { dir }
    }

    /// The directory.
    pub fn path(&self) -> &Path {
        &self.dir
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
