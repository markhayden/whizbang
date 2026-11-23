//! A generation: everything the store holds at one moment, by name. Immutable once
//! published; the pointer names the current one.
//!
//! In memory a generation is the store's few fixed names and 256 buckets, each shared
//! with the store's cache of the buckets it read last and with every copy of the
//! generation: a publication copies only the buckets it changes (ADR 0047). Every name
//! is in one place, the one [`super::buckets`] gives it, and is looked up there.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock};

use serde::de::{DeserializeOwned, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use super::buckets::{self, BUCKETS, Pack};
use super::digest::Digest;

/// What the names a run keeps for the runs under it start with, among its names, and
/// what [`Generation::scoped_to`] adds outside runs: the names of artifacts.
pub const SCOPED_PREFIX: &str = "artifacts/";

/// The version of the generation file's shape, and of what the store holds: 1 had no
/// kinds and no runs; 2 added them; 3, with the same shape, is a store whose images the
/// core's encoder wrote at its fast setting (2026-11-19, plan m3, task O5). A store of
/// format 2 holds the files of the slower setting, which name the same pixels by other
/// identities, so it is refused rather than written beside them; a store is not
/// migrated (ADR 0002, "The encoder's setting"). 4, with the same shape, is a store in
/// which every name outside runs that holds an artifact was written with a hold of the
/// run that committed it or the mark of the import that brought it in (plan m3, task Q1,
/// item 3; ADR 0008, "What deleting a run removes"): a store of format 3 has names that
/// neither holds, which a deletion could not tell from names nothing needs, so it is
/// refused whole rather than read with a rule it was not written for. 5 is the
/// generation in buckets (plan m6, task S1; ADR 0047): the generation file holds the
/// fixed names and the digests of 256 buckets, each a file of the store's content.
/// A generation file of format 4 is still read, and the first publication onto it
/// writes format 5; a build before 5 refuses a file of format 5 by its version.
pub const GENERATION_FORMAT: u32 = 5;

/// The format before buckets, which this build still reads (ADR 0047).
pub const WHOLE_FORMAT: u32 = 4;

/// What a reference to stored content is. The kind belongs to the reference, not to the
/// content: two references to the same bytes can differ, and each is treated as it says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A committed output: what an action produced, and what another may take as input.
    Output,
    /// Diagnostic evidence: kept and readable, and never usable as an output.
    Diagnostic,
}

/// One name's content: a reference to a stored file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// The stored file.
    #[serde(with = "super::digest::bare_hex")]
    pub digest: Digest,
    /// Its length. Reading checks it, so a file cut short is told from the right one
    /// before its digest is worked out, and a reader knows the size without reading.
    pub bytes: u64,
    /// What this reference is.
    pub kind: Kind,
}

/// What one run references, by names of its own. Deleting the run removes these
/// references and no others.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Run {
    #[serde(deserialize_with = "unique")]
    pub(crate) entries: BTreeMap<String, Entry>,
}

impl Run {
    /// Every name of the run, in order, with its content.
    pub fn entries(&self) -> &BTreeMap<String, Entry> {
        &self.entries
    }

    /// The content of one name.
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.entries.get(name)
    }
}

/// One bucket: the names outside runs, and the runs, whose keys fall in it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Bucket {
    pub(crate) entries: BTreeMap<String, Entry>,
    pub(crate) runs: BTreeMap<String, Run>,
    /// What an import's mark says of it, by the name it marks: in the bucket of the
    /// mark, `imported/<name>`.
    pub(crate) imports: BTreeMap<String, Imported>,
}

impl Bucket {
    fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.runs.is_empty() && self.imports.is_empty()
    }
}

/// When an import was made, and the names outside runs that came in with it (plan m6,
/// task S1, item 7): what the store's collection of unused imports reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Imported {
    /// When the import was made, in milliseconds since the Unix epoch.
    pub at_ms: u64,
    /// The names outside runs that came in with it, and go with it.
    pub with: Vec<String>,
}

/// One generation. Number 0 is the empty generation of a store that never published; it has
/// no file.
#[derive(Debug, Clone)]
pub struct Generation {
    pub(crate) number: u64,
    /// The names that belong to no namespace that grows: those with no `/`.
    fixed: BTreeMap<String, Entry>,
    /// The 256 buckets, in order.
    buckets: Vec<Arc<Bucket>>,
    /// The pack each bucket was read from or written in: `None` for an empty one, and
    /// for one this value changed since.
    digests: Vec<Option<Digest>>,
    /// How many buckets each pack it names was written with.
    sizes: BTreeMap<Digest, usize>,
    /// The packs it named when it was read: what a publication of it supersedes.
    read_from: BTreeSet<Digest>,
    /// The buckets this value changed since it was read: what a publication writes.
    changed: BTreeSet<u8>,
    /// Whether this was read from a file of the format before buckets: every bucket
    /// that holds anything is written by the first publication onto it.
    whole: bool,
    /// Every name outside runs, and every run, gathered once when asked for.
    all_entries: OnceLock<BTreeMap<String, Entry>>,
    all_runs: OnceLock<BTreeMap<String, Run>>,
}

impl PartialEq for Generation {
    fn eq(&self, other: &Generation) -> bool {
        self.number == other.number
            && self.fixed == other.fixed
            && self.buckets.len() == other.buckets.len()
            && (self.buckets.iter())
                .zip(&other.buckets)
                .all(|(one, two)| Arc::ptr_eq(one, two) || one == two)
    }
}

impl Eq for Generation {}

impl Generation {
    pub(crate) fn empty() -> Generation {
        Generation {
            number: 0,
            fixed: BTreeMap::new(),
            buckets: (0..BUCKETS).map(|_| Arc::new(Bucket::default())).collect(),
            digests: vec![None; BUCKETS],
            sizes: BTreeMap::new(),
            read_from: BTreeSet::new(),
            changed: BTreeSet::new(),
            whole: false,
            all_entries: OnceLock::new(),
            all_runs: OnceLock::new(),
        }
    }

    /// A generation read from a file of the format before buckets: its names sorted
    /// into buckets, every one to be written by the next publication.
    pub(crate) fn from_whole(
        number: u64,
        entries: BTreeMap<String, Entry>,
        runs: BTreeMap<String, Run>,
    ) -> Generation {
        let mut sorted: Vec<Bucket> = vec![Bucket::default(); BUCKETS];
        let mut fixed = BTreeMap::new();
        for (name, entry) in entries {
            match buckets::of_name(&name) {
                Some(at) => drop(sorted[at as usize].entries.insert(name, entry)),
                None => drop(fixed.insert(name, entry)),
            }
        }
        for (id, run) in runs {
            sorted[buckets::of_run(&id) as usize].runs.insert(id, run);
        }
        Generation {
            number,
            fixed,
            buckets: sorted.into_iter().map(Arc::new).collect(),
            digests: vec![None; BUCKETS],
            sizes: BTreeMap::new(),
            read_from: BTreeSet::new(),
            changed: BTreeSet::new(),
            whole: true,
            all_entries: OnceLock::new(),
            all_runs: OnceLock::new(),
        }
    }

    /// A generation read from a file of format 5: its fixed names, its buckets with the
    /// pack each was read from, and how many buckets each of those packs holds.
    pub(crate) fn from_buckets(
        number: u64,
        fixed: BTreeMap<String, Entry>,
        buckets: Vec<Arc<Bucket>>,
        digests: Vec<Option<Digest>>,
        sizes: BTreeMap<Digest, usize>,
    ) -> Generation {
        Generation {
            number,
            fixed,
            buckets,
            read_from: digests.iter().flatten().copied().collect(),
            digests,
            sizes,
            changed: BTreeSet::new(),
            whole: false,
            all_entries: OnceLock::new(),
            all_runs: OnceLock::new(),
        }
    }

    /// The generation's number: 0 for the empty one, then 1, 2 and so on.
    pub fn number(&self) -> u64 {
        self.number
    }

    /// Every name that belongs to no run, in order, with its content. No command removes
    /// one of these: deleting a run removes that run's names only.
    pub fn entries(&self) -> &BTreeMap<String, Entry> {
        self.all_entries.get_or_init(|| {
            let mut all = self.fixed.clone();
            for bucket in &self.buckets {
                all.extend(bucket.entries.iter().map(|(n, e)| (n.clone(), e.clone())));
            }
            all
        })
    }

    /// The content of one name that belongs to no run.
    pub fn get(&self, name: &str) -> Option<&Entry> {
        match buckets::of_name(name) {
            Some(at) => self.buckets[at as usize].entries.get(name),
            None => self.fixed.get(name),
        }
    }

    /// Every run, in order of its id.
    pub fn runs(&self) -> &BTreeMap<String, Run> {
        self.all_runs.get_or_init(|| {
            let mut all = BTreeMap::new();
            for bucket in &self.buckets {
                all.extend(bucket.runs.iter().map(|(id, r)| (id.clone(), r.clone())));
            }
            all
        })
    }

    /// One run.
    pub fn run(&self, id: &str) -> Option<&Run> {
        self.buckets[buckets::of_run(id) as usize].runs.get(id)
    }

    /// This generation as the runs under the first run `run` read it: each name the run
    /// was given by [`super::Publication::keep_in`] that is an output, and no name outside
    /// runs has, is added outside runs. A name of the run under [`super::HOLDS_PREFIX`],
    /// or one that is a diagnostic, is not. The runs are as they were.
    pub fn scoped_to(mut self, run: &str) -> Generation {
        let Some(own) = self.run(run) else {
            return self;
        };
        let added: Vec<(String, Entry)> = own
            .entries
            .iter()
            .filter(|(name, entry)| {
                entry.kind == Kind::Output
                    && name.starts_with(SCOPED_PREFIX)
                    && self.get(name).is_none()
            })
            .map(|(name, entry)| (name.clone(), entry.clone()))
            .collect();
        for (name, entry) in added {
            self.insert_entry(name, entry);
        }
        self
    }

    /// What the mark of an import of `name` says of the import, when the import was
    /// marked by this build: a mark written before (format 4) says nothing.
    pub fn imported(&self, name: &str) -> Option<&Imported> {
        let at = buckets::of_name(&format!("{}{name}", super::IMPORTED_PREFIX))?;
        self.buckets[at as usize].imports.get(name)
    }

    /// Every reference the generation holds, outside runs and in them.
    pub fn references(&self) -> impl Iterator<Item = &Entry> {
        let buckets = self.buckets.iter().flat_map(|bucket| {
            (bucket.entries.values())
                .chain(bucket.runs.values().flat_map(|run| run.entries.values()))
        });
        self.fixed.values().chain(buckets)
    }

    /// The digests of the packs that hold this generation's buckets, as it was read or
    /// written: the files under `objects/` it names beside its references (ADR 0047).
    /// None for one read from a file of the format before buckets, until a publication
    /// writes them.
    pub fn bucket_digests(&self) -> impl Iterator<Item = Digest> + '_ {
        self.digests
            .iter()
            .flatten()
            .copied()
            .collect::<BTreeSet<Digest>>()
            .into_iter()
    }

    /// What this generation's names take, written: its generation file, and each bucket
    /// that holds anything as a pack of its own. What a limit a test lowered
    /// ([`super::Store::limit_generations_to`]) is held to, so that a test can bring a
    /// store to it by its names as before buckets (ADR 0047). Worked out whole each time.
    pub fn names_bytes(&self) -> u64 {
        let root = buckets::root_text(self.number, &self.fixed, &self.digests).len() as u64;
        let buckets = (0..=u8::MAX)
            .zip(&self.buckets)
            .filter(|(_, bucket)| !bucket.is_empty())
            .map(|(at, bucket)| buckets::pack_text(std::iter::once((at, &**bucket))).len() as u64);
        root + buckets.sum::<u64>()
    }

    /// Its packs, each with the buckets of it this generation names and how many it was
    /// written with: what a writer keeps in memory after it published this generation.
    pub(crate) fn kept_packs(&self) -> BTreeMap<Digest, Packed> {
        let mut kept: BTreeMap<Digest, Packed> = BTreeMap::new();
        for (at, (digest, bucket)) in self.digests.iter().zip(&self.buckets).enumerate() {
            let Some(digest) = digest else { continue };
            let total = self.sizes.get(digest).copied().unwrap_or_default();
            let packed = kept.entry(*digest).or_insert_with(|| Packed {
                buckets: Arc::new(Pack::new()),
                total,
            });
            Arc::make_mut(&mut packed.buckets).insert(at as u8, bucket.clone());
        }
        kept
    }

    // What a publication changes, each in the one place the name or the run is.

    fn changing(&mut self, at: u8) -> &mut Bucket {
        self.changed.insert(at);
        self.digests[at as usize] = None;
        self.all_entries = OnceLock::new();
        self.all_runs = OnceLock::new();
        Arc::make_mut(&mut self.buckets[at as usize])
    }

    fn changing_fixed(&mut self) -> &mut BTreeMap<String, Entry> {
        self.all_entries = OnceLock::new();
        &mut self.fixed
    }

    /// Gives a name outside runs this content.
    pub(crate) fn insert_entry(&mut self, name: String, entry: Entry) {
        match buckets::of_name(&name) {
            Some(at) => drop(self.changing(at).entries.insert(name, entry)),
            None => drop(self.changing_fixed().insert(name, entry)),
        }
    }

    /// Says what the mark of an import of `name` says of it.
    pub(crate) fn set_imported(&mut self, name: &str, imported: Imported) {
        let mark = format!("{}{name}", super::IMPORTED_PREFIX);
        if let Some(at) = buckets::of_name(&mark) {
            self.changing(at).imports.insert(name.to_owned(), imported);
        }
    }

    /// Takes what a mark said of an import of `name` out.
    pub(crate) fn remove_imported(&mut self, name: &str) {
        if self.imported(name).is_none() {
            return;
        }
        let mark = format!("{}{name}", super::IMPORTED_PREFIX);
        if let Some(at) = buckets::of_name(&mark) {
            self.changing(at).imports.remove(name);
        }
    }

    /// Takes a name outside runs out.
    pub(crate) fn remove_entry(&mut self, name: &str) {
        if self.get(name).is_none() {
            return;
        }
        match buckets::of_name(name) {
            Some(at) => drop(self.changing(at).entries.remove(name)),
            None => drop(self.changing_fixed().remove(name)),
        }
    }

    /// Takes a run out, with all its names.
    pub(crate) fn remove_run(&mut self, id: &str) -> Option<Run> {
        self.run(id)?;
        self.changing(buckets::of_run(id)).runs.remove(id)
    }

    /// The run, made empty when it is not there.
    pub(crate) fn run_mut(&mut self, id: &str) -> &mut Run {
        self.changing(buckets::of_run(id))
            .runs
            .entry(id.to_owned())
            .or_default()
    }

    /// The run, when it is there, to change.
    pub(crate) fn existing_run_mut(&mut self, id: &str) -> Option<&mut Run> {
        self.run(id)?;
        self.changing(buckets::of_run(id)).runs.get_mut(id)
    }

    /// Whether any run holds this name of its own.
    pub(crate) fn any_run_has(&self, name: &str) -> bool {
        (self.buckets.iter()).any(|bucket| bucket.runs.values().any(|run| run.get(name).is_some()))
    }

    /// The files a publication writes for this generation: the generation file's text,
    /// and the pack that holds each bucket it changed (every bucket that holds anything,
    /// for one read from the format before buckets), with its digest; no pack when no
    /// bucket it writes holds anything. A bucket it did not change is carried into the
    /// pack too when fewer than half of the buckets its own pack was written with are
    /// still named, so that no pack is kept for a few of its buckets (ADR 0047). The
    /// digests are this value's from here on. A file over `limit` bytes is refused
    /// before anything is written, naming its size.
    pub(crate) fn encode(&mut self, limit: u64) -> Result<Encoded, (u64, u64)> {
        // A limit a test lowered holds the generation's names whole.
        if limit < super::GENERATION_MAX_BYTES {
            let whole = self.names_bytes();
            if whole > limit {
                return Err((whole, limit));
            }
        }
        let mut writes: BTreeSet<u8> = match self.whole {
            true => (0..=u8::MAX).collect(),
            false => self.changed.clone(),
        };
        let mut still: BTreeMap<Digest, Vec<u8>> = BTreeMap::new();
        for at in 0..=u8::MAX {
            if let (false, Some(digest)) = (writes.contains(&at), self.digests[at as usize]) {
                still.entry(digest).or_default().push(at);
            }
        }
        for (digest, named) in still {
            let total = self.sizes.get(&digest).copied().unwrap_or_default();
            if named.len() * 2 < total {
                writes.extend(named);
            }
        }
        let filled: Vec<u8> = (writes.iter().copied())
            .filter(|at| !self.buckets[*at as usize].is_empty())
            .collect();
        // The buckets in order, a pack at a time: a pack takes buckets while it is
        // under [`PACK_BYTES`], so a publication that changes many big buckets writes
        // several packs, and one of a few writes one.
        let mut packs = Vec::new();
        let mut group: Vec<(u8, Vec<u8>)> = Vec::new();
        let mut size = 0u64;
        for at in &filled {
            let one = buckets::pack_text(std::iter::once((*at, &*self.buckets[*at as usize])));
            if !group.is_empty() && size + one.len() as u64 > PACK_BYTES {
                packs.push(std::mem::take(&mut group));
                size = 0;
            }
            size += one.len() as u64;
            group.push((*at, one));
        }
        if !group.is_empty() {
            packs.push(group);
        }
        let mut written = Vec::new();
        for group in packs {
            let at: Vec<u8> = group.iter().map(|(at, _)| *at).collect();
            let text = buckets::pack_text(at.iter().map(|at| (*at, &*self.buckets[*at as usize])));
            if text.len() as u64 > limit {
                return Err((text.len() as u64, limit));
            }
            let digest = Digest::of_bytes(&text);
            for one in &at {
                self.digests[*one as usize] = Some(digest);
            }
            self.sizes.insert(digest, at.len());
            written.push((text, digest));
        }
        for at in &writes {
            if self.buckets[*at as usize].is_empty() {
                self.digests[*at as usize] = None;
            }
        }
        let named: BTreeSet<Digest> = self.digests.iter().flatten().copied().collect();
        self.sizes.retain(|digest, _| named.contains(digest));
        let root = buckets::root_text(self.number, &self.fixed, &self.digests);
        if root.len() as u64 > limit {
            return Err((root.len() as u64, limit));
        }
        let superseded = self.read_from.difference(&named).copied().collect();
        self.read_from = named;
        self.changed.clear();
        self.whole = false;
        Ok(Encoded {
            root,
            packs: written,
            superseded,
        })
    }
}

/// What a pack holds at most, as a publication fills it, unless one bucket alone is more:
/// a publication of few buckets writes one pack, and one of many big buckets, as the
/// first onto a large store of the format before buckets, writes several.
pub const PACK_BYTES: u64 = 16 << 20;

/// A pack as a writer or a reader keeps it in memory: the buckets of it that a
/// generation it read or wrote named, and how many buckets it was written with.
#[derive(Debug, Clone)]
pub(crate) struct Packed {
    pub(crate) buckets: Arc<Pack>,
    pub(crate) total: usize,
}

/// What a publication writes for a generation ([`Generation::encode`]).
pub(crate) struct Encoded {
    /// The generation file.
    pub(crate) root: Vec<u8>,
    /// The packs, each its text and its digest: none when it changed no bucket that
    /// holds anything, and one unless they are many and large.
    pub(crate) packs: Vec<(Vec<u8>, Digest)>,
    /// The packs the generation it was built on named, and it does not.
    pub(crate) superseded: Vec<Digest>,
}

/// A map of a generation file. A key that appears twice is refused: a map would keep the
/// last and hide the first.
pub(crate) fn unique<'de, D: Deserializer<'de>, V: DeserializeOwned>(
    deserializer: D,
) -> Result<BTreeMap<String, V>, D::Error> {
    struct Unique<V>(std::marker::PhantomData<V>);

    impl<'de, V: DeserializeOwned> Visitor<'de> for Unique<V> {
        type Value = BTreeMap<String, V>;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a map from names to what they hold")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut found = BTreeMap::new();
            while let Some((name, value)) = map.next_entry::<String, V>()? {
                if found.contains_key(&name) {
                    return Err(serde::de::Error::custom(format!(
                        "the name {name:?} appears twice"
                    )));
                }
                found.insert(name, value);
            }
            Ok(found)
        }
    }

    deserializer.deserialize_map(Unique(std::marker::PhantomData))
}
