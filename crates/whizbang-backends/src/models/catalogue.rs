//! The catalogue (ADR 0017, "The catalogue"): the models that can be installed. For
//! each: its files with their sizes and hashes, the address each is first asked for at
//! and the origins a download of them may reach, its licence, the features its adapter
//! gives with it, and a revision.
//!
//! A catalogue is a file a person writes, in JSON, and is read strictly: any way JSON
//! allows of saying the one thing, and nothing else. What whizbang writes of one is
//! written one way.

use std::collections::BTreeSet;

use serde::Serialize;
use whizbang_core::contract::{Secrets, text};
use whizbang_core::egress::{Address, NotAllowed, Origin};
use whizbang_core::identity::Digest;

use super::json::{self, Json, Object};
use super::name::{files_named, holds_secret};
use super::{FILE_MAX_BYTES, FileName, Licence, ModelName, Refused, Revision, Rule, What};

/// The format of a catalogue this reader reads.
pub const CATALOGUE_FORMAT: &str = "catalogue/1";

/// The most bytes of a catalogue.
pub const CATALOGUE_MAX_BYTES: u64 = 1 << 20;

/// The most models a catalogue holds.
pub const MODELS_LIMIT: usize = 256;

/// The most files a model has.
pub const FILES_LIMIT: usize = 64;

/// The most features an entry states.
pub const FEATURES_LIMIT: usize = 16;

/// The most origins a download of one model's files may reach.
pub const ORIGINS_LIMIT: usize = 8;

/// A file's size, held to its rule: one byte at least, [`FILE_MAX_BYTES`] at most.
pub(super) fn sized(size: u64) -> Result<u64, Refused> {
    if size == 0 || size > FILE_MAX_BYTES {
        return Err(Refused::rule(Rule::Size));
    }
    Ok(size)
}

/// A hash as it is written: `sha256:` and 64 lower-case hex digits.
pub(super) fn hash_of(text: &str) -> Result<Digest, Refused> {
    text.strip_prefix("sha256:")
        .and_then(Digest::from_hex)
        .ok_or(Refused::rule(Rule::Hash))
}

/// An address as it is written again: its origin, and the rest as it was given.
fn written(address: &Address) -> String {
    format!("{}{}", address.origin(), address.rest())
}

/// One file of a model: its name, its size, its hash, and the address it is first
/// asked for at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogueFile {
    name: FileName,
    size: u64,
    sha256: Digest,
    from: Address,
}

impl CatalogueFile {
    /// A file of this name, size and hash, downloaded from this address. The size is
    /// 1 to [`FILE_MAX_BYTES`].
    ///
    /// The address is written again without a port that is its scheme's own, which
    /// brings its host and its path together: an address that would spell a secret so
    /// is refused, as one that holds a secret is where it is made.
    pub fn new(
        name: FileName,
        size: u64,
        sha256: Digest,
        from: Address,
        secrets: &Secrets,
    ) -> Result<CatalogueFile, Refused> {
        if holds_secret(&written(&from), secrets) {
            return Err(Refused::HoldsSecret);
        }
        Ok(CatalogueFile {
            name,
            size: sized(size)?,
            sha256,
            from,
        })
    }

    pub fn name(&self) -> &FileName {
        &self.name
    }

    /// The size in bytes, which a download is stopped at.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// The SHA-256 of the file's bytes.
    pub fn sha256(&self) -> Digest {
        self.sha256
    }

    /// The address the file is first asked for at.
    pub fn from(&self) -> &Address {
        &self.from
    }
}

/// What an entry is made of. Each part was made by its own constructor; [`Entry::new`]
/// holds them to the rules that are about more than one of them.
#[derive(Debug, Clone)]
pub struct EntryParts {
    pub name: ModelName,
    /// The revision of this entry: whoever changes anything of the entry changes it.
    pub revision: Revision,
    /// The adapter that runs the model.
    pub adapter: ModelName,
    pub licence: Licence,
    /// The features the adapter gives with this model, each a feature's name.
    pub features: Vec<String>,
    /// The origins a download of the files may reach.
    pub origins: Vec<Origin>,
    pub files: Vec<CatalogueFile>,
}

/// One model of a catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    name: ModelName,
    revision: Revision,
    adapter: ModelName,
    licence: Licence,
    features: BTreeSet<String>,
    origins: Vec<Origin>,
    files: Vec<CatalogueFile>,
}

impl Entry {
    /// An entry of these parts.
    ///
    /// - At most [`FEATURES_LIMIT`] features, each a feature's name as a backend
    ///   declares one (ADR 0006) and each stated once; a feature that holds a secret
    ///   is refused whole.
    /// - At most [`ORIGINS_LIMIT`] origins, each named once.
    /// - 1 to [`FILES_LIMIT`] files, no two of one name, where names that differ only
    ///   in the case of their letters are one; and each file's address is at one of
    ///   the origins.
    ///
    /// Features, origins and files are kept in the order of their texts, whatever
    /// order they were given in. A refusal's place counts them as they were given.
    pub fn new(parts: EntryParts, secrets: &Secrets) -> Result<Entry, Refused> {
        let EntryParts {
            name,
            revision,
            adapter,
            licence,
            features,
            mut origins,
            mut files,
        } = parts;
        if features.len() > FEATURES_LIMIT {
            return Err(Refused::at(
                "features",
                Rule::Count {
                    most: FEATURES_LIMIT,
                },
            ));
        }
        let mut stated = BTreeSet::new();
        for (at, feature) in features.into_iter().enumerate() {
            if holds_secret(&feature, secrets) {
                return Err(Refused::HoldsSecret);
            }
            if text::action_name(&feature).is_err() {
                return Err(Refused::at(&format!("features[{at}]"), Rule::Feature));
            }
            if !stated.insert(feature) {
                return Err(Refused::at("features", Rule::NamedTwice));
            }
        }
        if origins.len() > ORIGINS_LIMIT {
            return Err(Refused::at(
                "origins",
                Rule::Count {
                    most: ORIGINS_LIMIT,
                },
            ));
        }
        for (at, origin) in origins.iter().enumerate() {
            if origins[..at].contains(origin) {
                return Err(Refused::at("origins", Rule::NamedTwice));
            }
        }
        files_named(files.iter().map(CatalogueFile::name), FILES_LIMIT)?;
        for (at, file) in files.iter().enumerate() {
            if !origins.contains(file.from.origin()) {
                return Err(Refused::at(&format!("files[{at}].from"), Rule::Elsewhere));
            }
        }
        origins.sort_by_key(Origin::to_string);
        files.sort_by(|one, other| one.name.cmp(&other.name));
        Ok(Entry {
            name,
            revision,
            adapter,
            licence,
            features: stated,
            origins,
            files,
        })
    }

    pub fn name(&self) -> &ModelName {
        &self.name
    }

    pub fn revision(&self) -> &Revision {
        &self.revision
    }

    pub fn adapter(&self) -> &ModelName {
        &self.adapter
    }

    pub fn licence(&self) -> &Licence {
        &self.licence
    }

    /// The features, in the order of their names.
    pub fn features(&self) -> impl Iterator<Item = &str> {
        self.features.iter().map(String::as_str)
    }

    /// Whether the feature is stated.
    pub fn has(&self, feature: &str) -> bool {
        self.features.contains(feature)
    }

    /// The origins a download of the files may reach, in the order of their texts.
    pub fn origins(&self) -> &[Origin] {
        &self.origins
    }

    /// Whether a download of this model's files may reach the origin: what each
    /// address a download is moved to is checked with.
    pub fn may_download_from(&self, origin: &Origin) -> bool {
        self.origins.contains(origin)
    }

    /// The files, in the order of their names.
    pub fn files(&self) -> &[CatalogueFile] {
        &self.files
    }

    /// The bytes of all the files together: what installing the model downloads.
    pub fn bytes(&self) -> u64 {
        self.files.iter().map(CatalogueFile::size).sum()
    }
}

/// The models that can be installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalogue {
    entries: Vec<Entry>,
}

#[derive(Serialize)]
struct WrittenCatalogue<'a> {
    format: &'a str,
    models: Vec<WrittenEntry<'a>>,
}

#[derive(Serialize)]
struct WrittenEntry<'a> {
    name: &'a str,
    revision: &'a str,
    adapter: &'a str,
    licence: &'a str,
    features: Vec<&'a str>,
    origins: Vec<String>,
    files: Vec<WrittenFile<'a>>,
}

#[derive(Serialize)]
struct WrittenFile<'a> {
    name: &'a str,
    size: u64,
    sha256: String,
    from: String,
}

impl Catalogue {
    /// A catalogue of these entries: at most [`MODELS_LIMIT`], no two of one name,
    /// kept in the order of their names.
    ///
    /// A catalogue that could not be read back is not made: one whose written form
    /// ([`Catalogue::to_bytes`]) is over [`CATALOGUE_MAX_BYTES`], or holds a secret.
    /// The written form puts side by side texts that a person may have written apart,
    /// so it is looked at whole.
    pub fn new(mut entries: Vec<Entry>, secrets: &Secrets) -> Result<Catalogue, Refused> {
        if entries.len() > MODELS_LIMIT {
            return Err(Refused::at("models", Rule::Count { most: MODELS_LIMIT }));
        }
        entries.sort_by(|one, other| one.name.cmp(&other.name));
        if entries.windows(2).any(|pair| pair[0].name == pair[1].name) {
            return Err(Refused::at("models", Rule::NamedTwice));
        }
        let catalogue = Catalogue { entries };
        let bytes = catalogue.to_bytes();
        if bytes.len() as u64 > CATALOGUE_MAX_BYTES {
            return Err(Refused::TooLarge {
                what: What::Catalogue,
                limit: CATALOGUE_MAX_BYTES,
            });
        }
        if secrets.redact_bytes(&bytes) != bytes {
            return Err(Refused::HoldsSecret);
        }
        Ok(catalogue)
    }

    /// Reads a catalogue from what this gives: no more than [`CATALOGUE_MAX_BYTES`]
    /// and one byte is asked of it, however much it has, and more than the bound is
    /// refused. A failure to read is refused by its kind, with no path.
    pub fn read_from(from: impl std::io::Read, secrets: &Secrets) -> Result<Catalogue, Refused> {
        let bytes = json::read(What::Catalogue, from, CATALOGUE_MAX_BYTES)?;
        Catalogue::parse(&bytes, secrets)
    }

    /// Reads these bytes as a catalogue. `secrets` are the values that must never be
    /// written: bytes that hold one anywhere are refused, and nothing of them is
    /// shown.
    ///
    /// The length is looked at first. Then: the bytes are JSON; no object gives a
    /// field twice; every field is one this format has, is there, and is of its kind;
    /// every list is within its bound, counted before its items are looked at; every
    /// value keeps its rule.
    pub fn parse(bytes: &[u8], secrets: &Secrets) -> Result<Catalogue, Refused> {
        let json = json::parse(What::Catalogue, bytes, CATALOGUE_MAX_BYTES, secrets)?;
        let mut top = Object::of(json, "")?;
        if top.text("format")? != CATALOGUE_FORMAT {
            return Err(Refused::at("format", Rule::Format));
        }
        let mut entries = Vec::new();
        for (at, model) in top.list("models", MODELS_LIMIT)?.into_iter().enumerate() {
            entries.push(entry(model, &format!("models[{at}]"), secrets)?);
        }
        top.done()?;
        Catalogue::new(entries, secrets)
    }

    /// The catalogue as whizbang writes it: compact JSON, the fields in the order of
    /// this format, the models, the features, the origins and the files each in the
    /// order of their texts. The same catalogue gives the same bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let models = self
            .entries
            .iter()
            .map(|entry| WrittenEntry {
                name: entry.name.as_str(),
                revision: entry.revision.as_str(),
                adapter: entry.adapter.as_str(),
                licence: entry.licence.as_str(),
                features: entry.features().collect(),
                origins: entry.origins.iter().map(Origin::to_string).collect(),
                files: entry
                    .files
                    .iter()
                    .map(|file| WrittenFile {
                        name: file.name.as_str(),
                        size: file.size,
                        sha256: file.sha256.to_string(),
                        from: written(&file.from),
                    })
                    .collect(),
            })
            .collect();
        let catalogue = WrittenCatalogue {
            format: CATALOGUE_FORMAT,
            models,
        };
        // Writing texts, whole numbers and lists of them as JSON cannot fail.
        serde_json::to_vec(&catalogue).unwrap_or_default()
    }

    /// The entries, in the order of their names.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The entry of this name, when the catalogue holds one.
    pub fn entry(&self, name: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|entry| entry.name.as_str() == name)
    }
}

/// The refusal of a text that is not what ADR 0012 takes: for a secret, the refusal
/// of a secret; for anything else, this rule at this place.
fn not_allowed(why: NotAllowed, at: &str, rule: fn(NotAllowed) -> Rule) -> Refused {
    match why {
        NotAllowed::Secret => Refused::HoldsSecret,
        other => Refused::at(at, rule(other)),
    }
}

/// One model of a catalogue, read from the value at this place.
fn entry(json: Json, at: &str, secrets: &Secrets) -> Result<Entry, Refused> {
    let mut object = Object::of(json, at)?;
    let under = |key: &str, refused: Refused| refused.under(&format!("{at}.{key}"));
    let name = ModelName::new(&object.text("name")?, secrets).map_err(|r| under("name", r))?;
    let revision =
        Revision::new(&object.text("revision")?, secrets).map_err(|r| under("revision", r))?;
    let adapter =
        ModelName::new(&object.text("adapter")?, secrets).map_err(|r| under("adapter", r))?;
    let licence =
        Licence::new(&object.text("licence")?, secrets).map_err(|r| under("licence", r))?;
    let mut features = Vec::new();
    for (n, feature) in object
        .list("features", FEATURES_LIMIT)?
        .into_iter()
        .enumerate()
    {
        features.push(json::text(feature, &format!("{at}.features[{n}]"))?);
    }
    let mut origins = Vec::new();
    for (n, origin) in object
        .list("origins", ORIGINS_LIMIT)?
        .into_iter()
        .enumerate()
    {
        let place = format!("{at}.origins[{n}]");
        let written = json::text(origin, &place)?;
        let origin =
            Origin::of(&written, secrets).map_err(|why| not_allowed(why, &place, Rule::Origin))?;
        // An origin is written as an origin. With a path after it, it would read as
        // the approval of a place inside the origin, which it is not.
        let alone = Address::of(&written, secrets).is_ok_and(|address| address.rest().is_empty());
        if !alone {
            return Err(Refused::at(&place, Rule::OriginAlone));
        }
        origins.push(origin);
    }
    let mut files = Vec::new();
    for (n, file) in object.list("files", FILES_LIMIT)?.into_iter().enumerate() {
        files.push(catalogue_file(file, &format!("{at}.files[{n}]"), secrets)?);
    }
    object.done()?;
    let parts = EntryParts {
        name,
        revision,
        adapter,
        licence,
        features,
        origins,
        files,
    };
    Entry::new(parts, secrets).map_err(|refused| refused.under(at))
}

/// One file of a model, read from the value at this place.
fn catalogue_file(json: Json, at: &str, secrets: &Secrets) -> Result<CatalogueFile, Refused> {
    let mut object = Object::of(json, at)?;
    let under = |key: &str, refused: Refused| refused.under(&format!("{at}.{key}"));
    let name = FileName::new(&object.text("name")?, secrets).map_err(|r| under("name", r))?;
    let size = sized(object.whole("size")?).map_err(|r| under("size", r))?;
    let sha256 = hash_of(&object.text("sha256")?).map_err(|r| under("sha256", r))?;
    let place = format!("{at}.from");
    let from = Address::of(&object.text("from")?, secrets)
        .map_err(|why| not_allowed(why, &place, Rule::Address))?;
    object.done()?;
    CatalogueFile::new(name, size, sha256, from, secrets)
}
