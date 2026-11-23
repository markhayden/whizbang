//! A store's files, for tests that look at them or change them by hand (ADR 0047).
//! Each helper fails the test that calls it when the store is not as it says.
#![allow(clippy::expect_used)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use whizbang_core::store::fs::{Fs, Held, LockFile, RealFs};
use whizbang_core::store::{Reader, WHOLE_FORMAT};

/// Writes the current generation of the store at `root` again as the format before
/// buckets, the whole store in its one file, and returns that file's path: so a test can
/// change a name of it by hand, as other hands might, and the store reads what it says.
/// The buckets it named stay where they are, named by nothing.
pub fn whole(root: &Path) -> PathBuf {
    let reader = Reader::open(root, Arc::new(RealFs)).expect("the store opens to read");
    let generation = reader.snapshot().expect("the store has a generation");
    let file = root
        .join("generations")
        .join(format!("{}.json", generation.number()));
    let text = serde_json::json!({
        "format": WHOLE_FORMAT,
        "number": generation.number(),
        "entries": generation.entries(),
        "runs": generation.runs(),
    });
    let mut bytes = serde_json::to_vec_pretty(&text).expect("a generation is JSON");
    bytes.push(b'\n');
    drop(reader);
    std::fs::remove_file(&file).expect("the generation's file is removed");
    std::fs::write(&file, bytes).expect("the generation's file is written");
    file
}

/// The digests of the packs of buckets the current generation of the store at `root`
/// names: files beside its generation files, `<digest>.pack`.
pub fn buckets(root: &Path) -> BTreeSet<String> {
    let reader = Reader::open(root, Arc::new(RealFs)).expect("the store opens to read");
    let generation = reader.snapshot().expect("the store has a generation");
    generation
        .bucket_digests()
        .map(|digest| digest.hex())
        .collect()
}

/// Generation `number` of the store at `root`, read from its file and the buckets it
/// names, as one JSON document in the shape of the format before buckets: its number,
/// every name outside runs (`entries`) and every run (`runs`). For a test that looks at
/// an older generation whose file is still there; a file of that format is read as it is.
pub fn generation_at(root: &Path, number: u64) -> serde_json::Value {
    let file = root.join("generations").join(format!("{number}.json"));
    let text = std::fs::read(&file).expect("the generation's file is there");
    let mut found: serde_json::Value = serde_json::from_slice(&text).expect("a generation is JSON");
    if found["format"] == serde_json::json!(WHOLE_FORMAT) {
        return found;
    }
    let mut runs = serde_json::Map::new();
    let empty = "0".repeat(64);
    let all = (found["buckets"].as_str())
        .expect("a generation names its buckets")
        .to_owned();
    let digests: Vec<(usize, String)> = (all.as_bytes().chunks(64))
        .map(|digest| String::from_utf8_lossy(digest).into_owned())
        .enumerate()
        .filter(|(_, digest)| *digest != empty)
        .collect();
    for (at, digest) in digests {
        let pack = root.join("generations").join(format!("{digest}.pack"));
        let bytes = std::fs::read(pack).expect("a pack is there");
        let pack: serde_json::Value = serde_json::from_slice(&bytes).expect("a pack is JSON");
        let bucket = &pack["buckets"][at.to_string()];
        let entries = found["entries"]
            .as_object_mut()
            .expect("a generation has entries");
        for (name, entry) in bucket["entries"].as_object().into_iter().flatten() {
            entries.insert(name.clone(), entry.clone());
        }
        for (id, run) in bucket["runs"].as_object().into_iter().flatten() {
            runs.insert(id.clone(), run.clone());
        }
    }
    found["runs"] = serde_json::Value::Object(runs);
    found
}

/// How many of these events come after the last sync of the store's folder at `root`,
/// the step that makes a publication durable: the steps a publication takes after it,
/// the removal of older generations' files and of the buckets it superseded and the
/// clearing of its staging (ADR 0036, ADR 0047), which can fail without failing it.
pub fn after_durable(events: &[crate::fs_shim::Event], root: &Path) -> usize {
    let durable = crate::fs_shim::Event::SyncDir(root.to_path_buf());
    let last = events
        .iter()
        .rposition(|event| *event == durable)
        .expect("a publication syncs the store's folder");
    events.len() - last - 1
}

/// The real file system, with two things changed for a test of where a store may live
/// (plan m6, task S1, item 4): every lock it is asked for is granted, as on a file system
/// whose locks do not hold, when `grants_every_lock`; and the type of file system it
/// says a path is on, or that its table of mounts cannot be read, when `mounted` says.
pub struct Placed {
    /// Every lock is granted, to every opening.
    pub grants_every_lock: bool,
    /// What [`Fs::file_system_type`] answers: a type, or `Err` with why the table could
    /// not be read; `None` asks the real one.
    pub mounted: Option<Result<String, String>>,
}

/// A lock that holds nothing: what [`Placed`] grants.
struct NoLock;

impl LockFile for NoLock {
    fn record(&self, _: &[u8]) -> std::io::Result<()> {
        Ok(())
    }
}

impl Fs for Placed {
    fn create_dir(&self, path: &Path) -> std::io::Result<()> {
        RealFs.create_dir(path)
    }
    fn write_new(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        RealFs.write_new(path, bytes)
    }
    fn sync_dir(&self, path: &Path) -> std::io::Result<()> {
        RealFs.sync_dir(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        RealFs.rename(from, to)
    }
    fn remove_all(&self, path: &Path) -> std::io::Result<()> {
        RealFs.remove_all(path)
    }
    fn read_at_most(&self, path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
        RealFs.read_at_most(path, limit)
    }
    fn list(&self, path: &Path) -> std::io::Result<Vec<PathBuf>> {
        RealFs.list(path)
    }
    fn exists(&self, path: &Path) -> std::io::Result<bool> {
        RealFs.exists(path)
    }
    fn hold_lock_file(
        &self,
        path: &Path,
        create: bool,
    ) -> std::io::Result<Option<Box<dyn LockFile>>> {
        if !self.grants_every_lock {
            return RealFs.hold_lock_file(path, create);
        }
        if create && !path.exists() {
            std::fs::write(path, b"")?;
        }
        Ok(Some(Box::new(NoLock)))
    }
    fn share(&self, path: &Path) -> std::io::Result<Box<dyn Held>> {
        RealFs.share(path)
    }
    fn try_alone(&self, path: &Path) -> std::io::Result<Option<Box<dyn Held>>> {
        RealFs.try_alone(path)
    }
    fn file_system_type(&self, path: &Path) -> std::io::Result<Option<String>> {
        match &self.mounted {
            None => RealFs.file_system_type(path),
            Some(Ok(kind)) => Ok(Some(kind.clone())),
            Some(Err(why)) => Err(std::io::Error::other(why.clone())),
        }
    }
}

/// The real file system with every sync left out: for a test of what a store keeps over
/// many publications, which is not a test of what a crash leaves.
pub struct Unsynced;

impl Fs for Unsynced {
    fn create_dir(&self, path: &Path) -> std::io::Result<()> {
        RealFs.create_dir(path)
    }
    fn write_new(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(bytes)
    }
    fn sync_dir(&self, _: &Path) -> std::io::Result<()> {
        Ok(())
    }
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        RealFs.rename(from, to)
    }
    fn remove_all(&self, path: &Path) -> std::io::Result<()> {
        RealFs.remove_all(path)
    }
    fn read_at_most(&self, path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
        RealFs.read_at_most(path, limit)
    }
    fn list(&self, path: &Path) -> std::io::Result<Vec<PathBuf>> {
        RealFs.list(path)
    }
    fn exists(&self, path: &Path) -> std::io::Result<bool> {
        RealFs.exists(path)
    }
    fn hold_lock_file(
        &self,
        path: &Path,
        create: bool,
    ) -> std::io::Result<Option<Box<dyn LockFile>>> {
        RealFs.hold_lock_file(path, create)
    }
    fn share(&self, path: &Path) -> std::io::Result<Box<dyn Held>> {
        RealFs.share(path)
    }
    fn try_alone(&self, path: &Path) -> std::io::Result<Option<Box<dyn Held>>> {
        RealFs.try_alone(path)
    }
}
