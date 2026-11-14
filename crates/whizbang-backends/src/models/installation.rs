//! An installation (ADR 0017, "An installation and its record", "The identity of an
//! installation"): the record of what one installation of a model holds, and the
//! identity that names it.
//!
//! The record is a file whizbang writes, in one form, and it reads back no other. The
//! identity is the SHA-256 of one encoding of the record, as ADR 0001 hashes every
//! identity; nothing local is in it.

use whizbang_core::contract::Secrets;
use whizbang_core::identity::Digest;

use serde::Serialize;

use super::catalogue::{FILES_LIMIT, hash_of, sized};
use super::json::{self, Json, Object};
use super::name::{files_named, holds_secret};
use super::{Entry, FileName, ModelName, Refused, Revision, Rule, What};

/// The format of an installation record this reader reads, and the kind its identity
/// is hashed under.
pub const INSTALLATION_FORMAT: &str = "installation/1";

/// The most bytes of an installation record. The longest record there can be is under
/// it.
pub const INSTALLATION_MAX_BYTES: u64 = 32 << 10;

/// One file of an installation: its name, its size and its hash. Where it was
/// downloaded from is no part of what is installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledFile {
    name: FileName,
    size: u64,
    sha256: Digest,
}

impl InstalledFile {
    /// A file of this name, size and hash. The size is held to the rule of a
    /// catalogue's file.
    pub fn new(name: FileName, size: u64, sha256: Digest) -> Result<InstalledFile, Refused> {
        Ok(InstalledFile {
            name,
            size: sized(size)?,
            sha256,
        })
    }

    pub fn name(&self) -> &FileName {
        &self.name
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    /// The SHA-256 of the file's bytes.
    pub fn sha256(&self) -> Digest {
        self.sha256
    }
}

/// The record of one installation: the model, the revision of the catalogue's entry
/// it was installed from, the adapter and the adapter's revision, and its files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallationRecord {
    model: ModelName,
    catalogue_revision: Revision,
    adapter: ModelName,
    adapter_revision: Revision,
    files: Vec<InstalledFile>,
}

#[derive(Serialize)]
struct WrittenRecord<'a> {
    format: &'a str,
    model: &'a str,
    catalogue_revision: &'a str,
    adapter: &'a str,
    adapter_revision: &'a str,
    files: Vec<WrittenFile<'a>>,
}

#[derive(Serialize)]
struct WrittenFile<'a> {
    name: &'a str,
    size: u64,
    sha256: String,
}

impl InstallationRecord {
    /// A record of these parts: 1 to [`FILES_LIMIT`] files, no two of one name, where
    /// names that differ only in the case of their letters are one. The files are kept
    /// in the order of their names, whatever order they were given in.
    ///
    /// A record that would spell a secret as it is written is not made: its parts may
    /// each have been made by a constructor that was told of other secrets, and what
    /// is written puts them side by side.
    pub fn new(
        model: ModelName,
        catalogue_revision: Revision,
        adapter: ModelName,
        adapter_revision: Revision,
        mut files: Vec<InstalledFile>,
        secrets: &Secrets,
    ) -> Result<InstallationRecord, Refused> {
        files_named(files.iter().map(InstalledFile::name), FILES_LIMIT)?;
        files.sort_by(|one, other| one.name.cmp(&other.name));
        let record = InstallationRecord {
            model,
            catalogue_revision,
            adapter,
            adapter_revision,
            files,
        };
        let bytes = record.to_bytes();
        if secrets.redact_bytes(&bytes) != bytes {
            return Err(Refused::HoldsSecret);
        }
        Ok(record)
    }

    /// The record of this entry of a catalogue installed under this revision of its
    /// adapter: the entry's name, its revision, its adapter, and each of its files
    /// with its size and its hash.
    pub fn of(
        entry: &Entry,
        adapter_revision: Revision,
        secrets: &Secrets,
    ) -> Result<InstallationRecord, Refused> {
        let files = entry
            .files()
            .iter()
            .map(|file| InstalledFile {
                name: file.name().clone(),
                size: file.size(),
                sha256: file.sha256(),
            })
            .collect();
        InstallationRecord::new(
            entry.name().clone(),
            entry.revision().clone(),
            entry.adapter().clone(),
            adapter_revision,
            files,
            secrets,
        )
    }

    /// Reads a record from what this gives: no more than [`INSTALLATION_MAX_BYTES`]
    /// and one byte is asked of it, however much it has. A failure to read is refused
    /// by its kind, with no path.
    pub fn read_from(
        from: impl std::io::Read,
        secrets: &Secrets,
    ) -> Result<InstallationRecord, Refused> {
        let bytes = json::read(What::Installation, from, INSTALLATION_MAX_BYTES)?;
        InstallationRecord::parse(&bytes, secrets)
    }

    /// Reads these bytes as a record. The length is looked at first, then the bytes
    /// whole for a secret; then they are read as a catalogue's are, strictly; and last
    /// they are held to the one form: bytes that say what a record says in any other
    /// way than [`InstallationRecord::to_bytes`] writes it are refused.
    pub fn parse(bytes: &[u8], secrets: &Secrets) -> Result<InstallationRecord, Refused> {
        let what = What::Installation;
        let json = json::parse(what, bytes, INSTALLATION_MAX_BYTES, secrets)?;
        let mut top = Object::of(json, "")?;
        if top.text("format")? != INSTALLATION_FORMAT {
            return Err(Refused::at("format", Rule::Format));
        }
        let model = ModelName::new(&top.text("model")?, secrets).map_err(|r| r.under("model"))?;
        let catalogue_revision = Revision::new(&top.text("catalogue_revision")?, secrets)
            .map_err(|r| r.under("catalogue_revision"))?;
        let adapter =
            ModelName::new(&top.text("adapter")?, secrets).map_err(|r| r.under("adapter"))?;
        let adapter_revision = Revision::new(&top.text("adapter_revision")?, secrets)
            .map_err(|r| r.under("adapter_revision"))?;
        let mut files = Vec::new();
        for (at, file) in top.list("files", FILES_LIMIT)?.into_iter().enumerate() {
            files.push(installed_file(file, &format!("files[{at}]"), secrets)?);
        }
        top.done()?;
        let record = InstallationRecord::new(
            model,
            catalogue_revision,
            adapter,
            adapter_revision,
            files,
            secrets,
        )?;
        if record.to_bytes() != bytes {
            return Err(Refused::OtherForm { what });
        }
        Ok(record)
    }

    /// The record in its one form: compact JSON, the fields in the order of this
    /// format, the files in the order of their names, and nothing after the last
    /// brace.
    pub fn to_bytes(&self) -> Vec<u8> {
        let record = WrittenRecord {
            format: INSTALLATION_FORMAT,
            model: self.model.as_str(),
            catalogue_revision: self.catalogue_revision.as_str(),
            adapter: self.adapter.as_str(),
            adapter_revision: self.adapter_revision.as_str(),
            files: self
                .files
                .iter()
                .map(|file| WrittenFile {
                    name: file.name.as_str(),
                    size: file.size,
                    sha256: file.sha256.to_string(),
                })
                .collect(),
        };
        // Writing texts, whole numbers and a list of them as JSON cannot fail.
        serde_json::to_vec(&record).unwrap_or_default()
    }

    /// The identity of the installation this record describes: the SHA-256 of the
    /// bytes `whizbang:installation/1` and a zero byte, then, in the encoding of ADR
    /// 0001, the model's name, the catalogue's revision, the adapter's name and the
    /// adapter's revision, each as bytes (`b`); the count of files (`u`); and for each
    /// file in the order of the bytes of its name, its name (`b`), its size (`u`) and
    /// its hash (`d`).
    pub fn identity(&self) -> InstallationId {
        let mut encoding = Vec::new();
        encoding.extend_from_slice(b"whizbang:");
        encoding.extend_from_slice(INSTALLATION_FORMAT.as_bytes());
        encoding.push(0);
        let bytes = |encoding: &mut Vec<u8>, value: &str| {
            encoding.push(b'b');
            encoding.extend_from_slice(&(value.len() as u64).to_be_bytes());
            encoding.extend_from_slice(value.as_bytes());
        };
        let unsigned = |encoding: &mut Vec<u8>, value: u64| {
            encoding.push(b'u');
            encoding.extend_from_slice(&value.to_be_bytes());
        };
        bytes(&mut encoding, self.model.as_str());
        bytes(&mut encoding, self.catalogue_revision.as_str());
        bytes(&mut encoding, self.adapter.as_str());
        bytes(&mut encoding, self.adapter_revision.as_str());
        unsigned(&mut encoding, self.files.len() as u64);
        for file in &self.files {
            bytes(&mut encoding, file.name.as_str());
            unsigned(&mut encoding, file.size);
            encoding.push(b'd');
            encoding.extend_from_slice(file.sha256.as_bytes());
        }
        InstallationId(Digest::of_bytes(&encoding))
    }

    pub fn model(&self) -> &ModelName {
        &self.model
    }

    /// The revision of the catalogue's entry the installation was made from.
    pub fn catalogue_revision(&self) -> &Revision {
        &self.catalogue_revision
    }

    pub fn adapter(&self) -> &ModelName {
        &self.adapter
    }

    /// The revision of the adapter the installation was made under.
    pub fn adapter_revision(&self) -> &Revision {
        &self.adapter_revision
    }

    /// The files, in the order of their names.
    pub fn files(&self) -> &[InstalledFile] {
        &self.files
    }
}

/// One file of a record, read from the value at this place.
fn installed_file(json: Json, at: &str, secrets: &Secrets) -> Result<InstalledFile, Refused> {
    let mut object = Object::of(json, at)?;
    let under = |key: &str, refused: Refused| refused.under(&format!("{at}.{key}"));
    let name = FileName::new(&object.text("name")?, secrets).map_err(|r| under("name", r))?;
    let size = sized(object.whole("size")?).map_err(|r| under("size", r))?;
    let sha256 = hash_of(&object.text("sha256")?).map_err(|r| under("sha256", r))?;
    object.done()?;
    InstalledFile::new(name, size, sha256)
}

/// The identity of an installation: a SHA-256 digest
/// ([`InstallationRecord::identity`]). Its text is `sha256:` and 64 lower-case hex
/// digits, as every digest's is (ADR 0001).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstallationId(Digest);

impl InstallationId {
    /// Reads an identity's text, as [`std::fmt::Display`] writes it and in no other
    /// way: no white space, no upper case, nothing before or after. A text that holds
    /// a secret is refused first, whole.
    pub fn from_text(text: &str, secrets: &Secrets) -> Result<InstallationId, Refused> {
        if holds_secret(text, secrets) {
            return Err(Refused::HoldsSecret);
        }
        hash_of(text).map(InstallationId)
    }

    /// The digest.
    pub fn digest(&self) -> Digest {
        self.0
    }

    /// The 64 lower-case hex digits alone: one plain name, for a folder.
    pub fn hex(&self) -> String {
        self.0.hex()
    }
}

impl std::fmt::Display for InstallationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Debug for InstallationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "InstallationId({})", self.0)
    }
}
