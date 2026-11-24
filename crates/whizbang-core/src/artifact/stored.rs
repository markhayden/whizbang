//! Artifacts in the store. An artifact is stored as its canonical file, under a name made
//! of its type and its identity, so what a store holds of it is what any other store would.

use crate::identity::DecodeLimits;
use crate::store::{self, Entry, Generation, Kind, Publication, Reader, Store};

use super::{Artifact, ArtifactError, ArtifactId, ArtifactType, Result};

/// The name an artifact is stored under: `artifacts/<type>/<the identity's 64 hex digits>`.
pub fn name(artifact: ArtifactType, id: ArtifactId) -> String {
    format!("artifacts/{}/{}", artifact.name(), id.0.hex())
}

/// Whether this generation holds the artifact: its name is there as a committed output.
/// A diagnostic reference under an artifact's name is not the artifact, here as in
/// [`get`], which refuses to read one: what this says is held, `get` will read.
pub fn holds(generation: &Generation, artifact: ArtifactType, id: ArtifactId) -> bool {
    generation
        .get(&name(artifact, id))
        .is_some_and(|entry| entry.kind == Kind::Output)
}

/// Adds the artifact to a publication, as its canonical file under its name, from
/// outside any run: it is marked as imported ([`Publication::mark_imported`]), so no
/// deletion of a run removes it. It is in the store once the publication is published.
/// Every name of an artifact outside runs is so written with a mark, or with the hold of
/// the run that committed it ([`commit`]): no name that nothing accounts for is written.
///
/// One name holds one reference. Where the name held a diagnostic reference, which
/// [`holds`] does not count, this replaces it with the output; what the diagnostic
/// referenced is then named by nothing, unless another name has it, and goes at the
/// next prune.
pub fn put(publication: &mut Publication<'_>, artifact: &Artifact) -> Result<ArtifactId> {
    let id = artifact.id();
    let name = name(artifact.artifact_type(), id);
    publication.put(&name, &artifact.to_bytes())?;
    publication.mark_imported(&name)?;
    Ok(id)
}

/// Adds the artifact to a publication as an output the run `run` commits: two
/// references to its canonical file, both committed outputs (ADR 0008, "The commit
/// step").
///
/// One is the run's own, among its names, under `reference`: it is to the file this run
/// made, and goes when the run is deleted. The other is the artifact's name, which is how
/// a later call takes it; it is given this file unless the store already holds an
/// artifact under that name when the publication is published. Then the name keeps the
/// file it has, which is the same artifact with the origin it was first stored with: an
/// identity does not cover an origin, and a name holds one file. The store decides that
/// inside its publishing step ([`Publication::keep`]).
///
/// Where the name is depends on who called the run (plan m3, task Q1, item 3). For a run
/// that nothing called (`first` is `None`) it is outside runs, and the run holds it
/// ([`Publication::hold_in`]): a deletion removes it once no run holds it and no import
/// marked it. For a run that another run called it is a name of `first`, the run that
/// nothing called at the top of its tree ([`Publication::keep_in`]): the runs under that
/// run read it, no other run does, and it goes when that run is deleted.
pub fn commit(
    publication: &mut Publication<'_>,
    run: &str,
    first: Option<&str>,
    reference: &str,
    artifact: &Artifact,
) -> store::Result<ArtifactId> {
    let id = artifact.id();
    let bytes = artifact.to_bytes();
    let name = name(artifact.artifact_type(), id);
    publication.put_in(run, reference, Kind::Output, &bytes)?;
    match first {
        Some(first) => {
            publication.keep_in(first, &name, &bytes)?;
        }
        None => {
            let digest = publication.keep(&name, &bytes)?;
            let entry = Entry {
                digest,
                bytes: bytes.len() as u64,
                kind: Kind::Output,
            };
            publication.hold_in(run, &name, entry)?;
        }
    }
    Ok(id)
}

/// Adds the artifact to a publication as an import brings it in: under its name, as
/// [`put`] does, and marked as imported, so that no deletion of a run removes it
/// ([`Publication::mark_imported`]). An artifact the store holds already is marked alone,
/// whatever made it.
pub fn import(
    publication: &mut Publication<'_>,
    artifact: &Artifact,
    held: bool,
) -> Result<ArtifactId> {
    import_with(publication, artifact, held, &[])
}

/// As [`import`], with the names outside runs that come in with the import: the store's
/// collection of unused imports lets them go with it ([`Publication::mark_imported_with`]).
pub fn import_with(
    publication: &mut Publication<'_>,
    artifact: &Artifact,
    held: bool,
    with: &[String],
) -> Result<ArtifactId> {
    let id = artifact.id();
    let name = name(artifact.artifact_type(), id);
    if !held {
        publication.put(&name, &artifact.to_bytes())?;
    }
    publication.mark_imported_with(&name, with)?;
    Ok(id)
}

/// What artifacts are read from: a store opened to write, or to read.
pub trait Contents {
    /// The current generation.
    fn snapshot(&self) -> store::Result<Generation>;
    /// The content of an entry that is a committed output, checked against its length
    /// and its digest. A diagnostic reference is refused: an artifact is an output, and
    /// diagnostic evidence under an artifact's name is not one.
    fn read(&self, entry: &Entry) -> store::Result<Vec<u8>>;
    /// The content of an entry of either kind, checked the same way. It is for reading
    /// a record the executor wrote, of a request id or of a run, which is stored as a
    /// diagnostic reference; nothing that reads an artifact uses it.
    fn read_record(&self, entry: &Entry) -> store::Result<Vec<u8>>;
}

impl Contents for Store {
    fn snapshot(&self) -> store::Result<Generation> {
        Store::snapshot(self)
    }

    fn read(&self, entry: &Entry) -> store::Result<Vec<u8>> {
        Store::read_output(self, entry)
    }

    fn read_record(&self, entry: &Entry) -> store::Result<Vec<u8>> {
        Store::read(self, entry)
    }
}

impl Contents for Reader {
    fn snapshot(&self) -> store::Result<Generation> {
        Reader::snapshot(self)
    }

    fn read(&self, entry: &Entry) -> store::Result<Vec<u8>> {
        Reader::read_output(self, entry)
    }

    fn read_record(&self, entry: &Entry) -> store::Result<Vec<u8>> {
        Reader::read(self, entry)
    }
}

/// As [`get`], for a person reading the store and not for a run: where no name outside
/// runs holds the artifact, a name that the first run of a tree keeps for the runs under
/// it ([`commit`]) is read too. What a run takes is read by [`get`] alone, through its
/// own view of the store, so a run of another tree is never handed it (plan m3, task Q1,
/// item 3); a person exporting what a step of a recipe made is.
pub fn get_anywhere(
    store: &(impl Contents + ?Sized),
    expected: ArtifactType,
    id: ArtifactId,
    limits: &DecodeLimits,
) -> Result<Artifact> {
    get(&Anywhere(store), expected, id, limits)
}

/// Whether this generation holds the artifact outside runs or among the names a first run
/// keeps for its tree: what [`get_anywhere`] reads.
pub fn holds_anywhere(generation: &Generation, artifact: ArtifactType, id: ArtifactId) -> bool {
    holds(&everywhere(generation.clone()), artifact, id)
}

/// The type of the artifact of this identity that [`get_anywhere`] would read, when the
/// generation holds one: looked for outside runs first, then, once, among the names every
/// first run keeps for its tree. For a door that is given an identity and not its type.
pub fn type_anywhere(generation: &Generation, id: ArtifactId) -> Option<ArtifactType> {
    let of = |generation: &Generation| {
        ArtifactType::ALL
            .into_iter()
            .find(|kind| holds(generation, *kind, id))
    };
    of(generation).or_else(|| of(&everywhere(generation.clone())))
}

/// A generation with the names every first run keeps for its tree added outside runs.
fn everywhere(generation: Generation) -> Generation {
    let runs: Vec<String> = generation.runs().keys().cloned().collect();
    runs.iter()
        .fold(generation, |generation, run| generation.scoped_to(run))
}

/// The store as a person reads it: see [`get_anywhere`].
struct Anywhere<'a, C: ?Sized>(&'a C);

impl<C: Contents + ?Sized> Contents for Anywhere<'_, C> {
    fn snapshot(&self) -> store::Result<Generation> {
        self.0.snapshot().map(everywhere)
    }

    fn read(&self, entry: &Entry) -> store::Result<Vec<u8>> {
        self.0.read(entry)
    }

    fn read_record(&self, entry: &Entry) -> store::Result<Vec<u8>> {
        self.0.read_record(entry)
    }
}

/// Takes the artifact of type `expected` with this identity from the store's current
/// generation. An artifact of another type with that identity is refused as the wrong
/// type, so a rendering never comes back where a mask was asked for; what comes back has
/// been read whole, checked, and found to have the identity it is stored under.
pub fn get(
    store: &(impl Contents + ?Sized),
    expected: ArtifactType,
    id: ArtifactId,
    limits: &DecodeLimits,
) -> Result<Artifact> {
    let generation = store.snapshot()?;
    let Some(entry) = generation.get(&name(expected, id)) else {
        return Err(
            match ArtifactType::ALL
                .into_iter()
                .find(|other| holds(&generation, *other, id))
            {
                Some(other) => ArtifactError::WrongType {
                    expected,
                    found: other.with_article(),
                },
                None => ArtifactError::Missing {
                    artifact: expected,
                    id,
                },
            },
        );
    };
    let artifact = Artifact::from_bytes_as(expected, &store.read(entry)?, limits)?;
    if artifact.id() != id {
        return Err(ArtifactError::Format(format!(
            "the store holds {} under the name of {id}",
            artifact.id()
        )));
    }
    Ok(artifact)
}
