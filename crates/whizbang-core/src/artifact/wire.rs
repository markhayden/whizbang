//! The canonical artifact file, and the identity hashed beside it (ADR 0002).
//!
//! One function writes an artifact: [`Writer`] sends what it is given to two places, the
//! file and the hash of the identity, so the two cannot drift apart. They differ in four
//! fields only, each of which the file holds whole and the identity holds as a digest or as
//! plain bytes:
//!
//! | Field | In the file | In the identity |
//! |---|---|---|
//! | a plane | `p`, the count of samples, the count of runs, the runs | `b` and the samples |
//! | a held file | `x`, its length, its bytes | `d` and the digest of its bytes |
//! | a binding or an artifact inside another | `a`, then its own file | `d` and its identity |
//! | the origin | `o`, then its fields | nothing |
//!
//! [`Reader`] reads a file back. It checks what it can as it goes; the one check that makes
//! a file canonical is made by its caller: the artifact that was read, written again, must
//! give the same bytes.

use crate::identity::{Binding, Digest, Encoder, FieldSource, Fields, IdentityError, Space};

use super::{ArtifactError, Origin, Result, Step};

/// What every artifact file and every identity starts with, before the kind.
const PREFIX: &[u8] = b"whizbang:";

/// The version of the origin's fields.
const ORIGIN_VERSION: u64 = 1;

/// The fields of a file, as bytes.
struct Plain(Vec<u8>);

impl Fields for Plain {
    fn unsigned(&mut self, value: u64) {
        self.0.push(b'u');
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    fn float(&mut self, value: f64) {
        let value = if value == 0.0 { 0.0 } else { value };
        self.0.push(b'f');
        self.0.extend_from_slice(&value.to_bits().to_be_bytes());
    }

    fn digest(&mut self, value: &Digest) {
        self.0.push(b'd');
        self.0.extend_from_slice(value.as_bytes());
    }

    fn bytes(&mut self, value: &[u8]) {
        self.0.push(b'b');
        self.0
            .extend_from_slice(&(value.len() as u64).to_be_bytes());
        self.0.extend_from_slice(value);
    }
}

/// Writes one artifact: its identity always, and its file when asked.
pub(crate) struct Writer {
    file: Option<Plain>,
    hash: Encoder,
}

impl Writer {
    /// A writer for an artifact of this kind (`mask/1`). Without `file`, only the identity
    /// is worked out, and nothing is copied.
    pub(crate) fn new(kind: &str, file: bool) -> Writer {
        let file = file.then(|| {
            let mut bytes = PREFIX.to_vec();
            bytes.extend_from_slice(kind.as_bytes());
            bytes.push(0);
            Plain(bytes)
        });
        Writer {
            file,
            hash: Encoder::new(kind),
        }
    }

    pub(crate) fn text(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    pub(crate) fn count(&mut self, value: usize) {
        self.unsigned(value as u64);
    }

    /// A plane of samples, one byte each. The identity holds the samples; the file holds
    /// them as runs: a value and how many times it repeats, each run as long as it can be.
    pub(crate) fn plane(&mut self, samples: &[u8]) {
        self.hash.bytes(samples);
        let Some(file) = &mut self.file else {
            return;
        };
        file.0.push(b'p');
        file.0
            .extend_from_slice(&(samples.len() as u64).to_be_bytes());
        let at = file.0.len();
        file.0.extend_from_slice(&[0; 8]);
        let mut runs = 0u64;
        let mut rest = samples;
        while let Some(&value) = rest.first() {
            let length = rest
                .iter()
                .take(u32::MAX as usize)
                .take_while(|sample| **sample == value)
                .count();
            file.0.push(value);
            file.0.extend_from_slice(&(length as u32).to_be_bytes());
            runs += 1;
            rest = &rest[length..];
        }
        file.0[at..at + 8].copy_from_slice(&runs.to_be_bytes());
    }

    /// A file the artifact holds whole, such as an image's. The identity holds its digest.
    pub(crate) fn held_file(&mut self, bytes: &[u8]) {
        self.hash.digest(&Digest::of_bytes(bytes));
        if let Some(file) = &mut self.file {
            file.0.push(b'x');
            file.0
                .extend_from_slice(&(bytes.len() as u64).to_be_bytes());
            file.0.extend_from_slice(bytes);
        }
    }

    /// Something with an identity of its own, inside this artifact: the file holds it
    /// whole, and the identity holds its identity.
    pub(crate) fn inside(&mut self, kind: &str, write: impl FnOnce(&mut Writer)) {
        let mut inner = Writer::new(kind, self.file.is_some());
        write(&mut inner);
        let (bytes, identity) = inner.finish();
        self.hash.digest(&identity);
        if let (Some(file), Some(bytes)) = (&mut self.file, bytes) {
            file.0.push(b'a');
            file.0.extend_from_slice(&bytes);
        }
    }

    /// The binding the artifact belongs to: whole in the file, its identity in the
    /// identity.
    pub(crate) fn binding(&mut self, binding: &Binding) {
        self.inside(Binding::KIND, |inner| binding.encode(inner));
    }

    /// What produced the artifact. It is in the file and in no identity.
    pub(crate) fn origin(&mut self, origin: &Origin) {
        let Some(file) = &mut self.file else {
            return;
        };
        file.0.push(b'o');
        file.unsigned(ORIGIN_VERSION);
        file.unsigned(origin.steps().len() as u64);
        for step in origin.steps() {
            file.bytes(step.action.as_bytes());
            file.bytes(step.version.as_bytes());
            file.unsigned(step.arguments.len() as u64);
            for (name, value) in &step.arguments {
                file.bytes(name.as_bytes());
                file.bytes(value.as_bytes());
            }
            file.unsigned(step.inputs.len() as u64);
            for (port, identity) in &step.inputs {
                file.bytes(port.as_bytes());
                file.digest(identity);
            }
        }
    }

    /// The file, when one was asked for, and the identity.
    pub(crate) fn finish(self) -> (Option<Vec<u8>>, Digest) {
        (self.file.map(|file| file.0), self.hash.finish())
    }
}

impl Fields for Writer {
    fn unsigned(&mut self, value: u64) {
        self.hash.unsigned(value);
        if let Some(file) = &mut self.file {
            file.unsigned(value);
        }
    }

    fn float(&mut self, value: f64) {
        self.hash.float(value);
        if let Some(file) = &mut self.file {
            file.float(value);
        }
    }

    fn digest(&mut self, value: &Digest) {
        self.hash.digest(value);
        if let Some(file) = &mut self.file {
            file.digest(value);
        }
    }

    fn bytes(&mut self, value: &[u8]) {
        self.hash.bytes(value);
        if let Some(file) = &mut self.file {
            file.bytes(value);
        }
    }
}

/// Reads an artifact file from its first byte to its last.
pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    /// Why the file could not give a binding or a space one of its fields, kept while the
    /// identity module reads it so that the refusal is the file's own.
    failed: Option<ArtifactError>,
}

fn refuse(why: impl Into<String>) -> ArtifactError {
    ArtifactError::Format(why.into())
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Reader<'a> {
        Reader {
            bytes,
            at: 0,
            failed: None,
        }
    }

    fn take(&mut self, count: u64) -> Result<&'a [u8]> {
        let left = self.bytes.len() - self.at;
        if count > left as u64 {
            return Err(refuse(format!(
                "the file ends at byte {}, {} short of what it says it holds",
                self.bytes.len(),
                count - left as u64
            )));
        }
        let taken = &self.bytes[self.at..self.at + count as usize];
        self.at += count as usize;
        Ok(taken)
    }

    fn eight(&mut self) -> Result<[u8; 8]> {
        let mut eight = [0; 8];
        eight.copy_from_slice(self.take(8)?);
        Ok(eight)
    }

    fn tag(&mut self, tag: u8, what: &str) -> Result<()> {
        let at = self.at;
        match self.take(1)? {
            [found] if *found == tag => Ok(()),
            _ => Err(refuse(format!("byte {at} does not start {what}"))),
        }
    }

    /// The kind an artifact file, or something inside one, starts with.
    pub(crate) fn kind(&mut self) -> Result<&'a str> {
        let not_one = || refuse("it does not start as an artifact file does");
        if self.take(PREFIX.len() as u64).map_err(|_| not_one())? != PREFIX {
            return Err(not_one());
        }
        let rest = &self.bytes[self.at..];
        // A kind is short: a name, a stroke and a version.
        let end = rest
            .iter()
            .take(64)
            .position(|byte| *byte == 0)
            .ok_or_else(not_one)?;
        let kind = std::str::from_utf8(&rest[..end]).map_err(|_| not_one())?;
        self.at += end + 1;
        Ok(kind)
    }

    pub(crate) fn unsigned(&mut self) -> Result<u64> {
        self.tag(b'u', "a whole number")?;
        Ok(u64::from_be_bytes(self.eight()?))
    }

    /// A whole number that fits 32 bits.
    pub(crate) fn small(&mut self, what: &str) -> Result<u32> {
        let value = self.unsigned()?;
        u32::try_from(value).map_err(|_| refuse(format!("a {what} of {value} is too large")))
    }

    /// How many things follow. Nothing is set aside for them here: a count that the file
    /// cannot back runs out of bytes instead.
    pub(crate) fn count(&mut self) -> Result<u64> {
        self.unsigned()
    }

    pub(crate) fn float(&mut self) -> Result<f64> {
        self.tag(b'f', "a number")?;
        let value = f64::from_bits(u64::from_be_bytes(self.eight()?));
        if !value.is_finite() {
            return Err(refuse("a number that is not finite"));
        }
        Ok(value)
    }

    pub(crate) fn digest(&mut self) -> Result<Digest> {
        self.tag(b'd', "a digest")?;
        let mut bytes = [0; 32];
        bytes.copy_from_slice(self.take(32)?);
        Ok(Digest::from_bytes(bytes))
    }

    pub(crate) fn text(&mut self) -> Result<String> {
        self.tag(b'b', "a text")?;
        let length = u64::from_be_bytes(self.eight()?);
        let bytes = self.take(length)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| refuse("a text that is not UTF-8"))
    }

    /// A plane of exactly `samples` samples. The caller has held `samples` to its limit.
    pub(crate) fn plane(&mut self, samples: u64) -> Result<Vec<u8>> {
        self.tag(b'p', "a plane")?;
        let stated = u64::from_be_bytes(self.eight()?);
        if stated != samples {
            return Err(refuse(format!(
                "a plane of {stated} samples where {samples} are needed"
            )));
        }
        let runs = u64::from_be_bytes(self.eight()?);
        let mut plane = Vec::with_capacity(samples as usize);
        for _ in 0..runs {
            let run = self.take(5)?;
            let length = u64::from(u32::from_be_bytes([run[1], run[2], run[3], run[4]]));
            if length == 0 {
                return Err(refuse("a plane with a run of no samples"));
            }
            // Before anything is set aside for the run.
            if plane.len() as u64 + length > samples {
                return Err(refuse(format!(
                    "a run of {length} samples goes past the end of its plane of {samples}"
                )));
            }
            plane.resize(plane.len() + length as usize, run[0]);
        }
        if plane.len() as u64 != samples {
            return Err(refuse("a plane whose runs do not add up to its size"));
        }
        Ok(plane)
    }

    pub(crate) fn held_file(&mut self) -> Result<&'a [u8]> {
        self.tag(b'x', "a held file")?;
        let length = u64::from_be_bytes(self.eight()?);
        self.take(length)
    }

    /// The start of something inside this artifact, which must be of `kind`.
    pub(crate) fn inside(&mut self, kind: &str) -> Result<()> {
        self.tag(b'a', "what the artifact holds inside it")?;
        let found = self.kind()?;
        if found != kind {
            return Err(refuse(format!("it holds {found} where {kind} belongs")));
        }
        Ok(())
    }

    /// What the identity module reads from this file through [`FieldSource`]. A file that
    /// cannot give a field is refused as a file, in its own words; what the identity
    /// module refuses of the fields it was given is refused in its words.
    fn identity<T>(
        &mut self,
        read: impl FnOnce(&mut Self) -> std::result::Result<T, IdentityError>,
    ) -> Result<T> {
        self.failed = None;
        let read = read(self);
        match (read, self.failed.take()) {
            (Ok(value), _) => Ok(value),
            (Err(_), Some(file)) => Err(file),
            (Err(refusal), None) => Err(ArtifactError::Identity(refusal)),
        }
    }

    /// A binding, checked as a new one is (ADR 0001).
    pub(crate) fn binding(&mut self) -> Result<Binding> {
        self.inside(Binding::KIND)?;
        self.identity(|reader| Binding::decode(reader))
    }

    /// A space, written as its four fields.
    pub(crate) fn space(&mut self) -> Result<Space> {
        self.identity(|reader| Space::decode(reader))
    }

    pub(crate) fn origin(&mut self) -> Result<Origin> {
        self.tag(b'o', "the origin")?;
        let version = self.unsigned()?;
        if version != ORIGIN_VERSION {
            return Err(refuse(format!(
                "origin version {version}; this build reads version {ORIGIN_VERSION}"
            )));
        }
        let mut steps = Vec::new();
        for _ in 0..self.count()? {
            let mut step = Step::new(&self.text()?, &self.text()?);
            for _ in 0..self.count()? {
                step = step.argument(&self.text()?, &self.text()?);
            }
            for _ in 0..self.count()? {
                step = step.input(&self.text()?, self.digest()?);
            }
            steps.push(step);
        }
        Origin::from_steps(steps)
    }

    /// The file must end here.
    pub(crate) fn end(&self) -> Result<()> {
        if self.at != self.bytes.len() {
            return Err(refuse(format!(
                "{} bytes follow the artifact",
                self.bytes.len() - self.at
            )));
        }
        Ok(())
    }
}

/// A binding's fields are read through the identity module's own reader of them.
impl FieldSource for Reader<'_> {
    fn unsigned(&mut self) -> std::result::Result<u64, IdentityError> {
        Reader::unsigned(self).map_err(|error| self.fail(error))
    }

    fn float(&mut self) -> std::result::Result<f64, IdentityError> {
        Reader::float(self).map_err(|error| self.fail(error))
    }

    fn digest(&mut self) -> std::result::Result<Digest, IdentityError> {
        Reader::digest(self).map_err(|error| self.fail(error))
    }
}

impl Reader<'_> {
    /// Keeps the file's refusal for [`Reader::identity`], which reports it in place of
    /// what the identity module is given here.
    fn fail(&mut self, error: ArtifactError) -> IdentityError {
        let said = IdentityError::Binding("the file could not give a field".to_owned());
        self.failed.get_or_insert(error);
        said
    }
}
