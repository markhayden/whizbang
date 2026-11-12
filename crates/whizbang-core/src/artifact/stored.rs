//! Artifacts in the store. An artifact is stored as its canonical file, under a name made
//! of its type and its identity, so what a store holds of it is what any other store would.

use crate::identity::DecodeLimits;
use crate::store::{self, Entry, Generation, Publication, Reader, Store};

use super::{Artifact, ArtifactError, ArtifactId, ArtifactType, Result};

/// The name an artifact is stored under: `artifacts/<type>/<the identity's 64 hex digits>`.
pub fn name(artifact: ArtifactType, id: ArtifactId) -> String {
    format!("artifacts/{}/{}", artifact.name(), id.0.hex())
}

/// Whether this generation holds the artifact.
pub fn holds(generation: &Generation, artifact: ArtifactType, id: ArtifactId) -> bool {
    generation.get(&name(artifact, id)).is_some()
}

/// Adds the artifact to a publication, as its canonical file under its name. It is in the
/// store once the publication is published.
pub fn put(publication: &mut Publication<'_>, artifact: &Artifact) -> Result<ArtifactId> {
    let id = artifact.id();
    publication.put(&name(artifact.artifact_type(), id), &artifact.to_bytes())?;
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
}

impl Contents for Store {
    fn snapshot(&self) -> store::Result<Generation> {
        Store::snapshot(self)
    }

    fn read(&self, entry: &Entry) -> store::Result<Vec<u8>> {
        Store::read_output(self, entry)
    }
}

impl Contents for Reader {
    fn snapshot(&self) -> store::Result<Generation> {
        Reader::snapshot(self)
    }

    fn read(&self, entry: &Entry) -> store::Result<Vec<u8>> {
        Reader::read_output(self, entry)
    }
}

/// Takes the artifact of type `expected` with this identity from the store's current
/// generation. An artifact of another type with that identity is refused as the wrong
/// type, so a rendering never comes back where a mask was asked for; what comes back has
/// been read whole, checked, and found to have the identity it is stored under.
pub fn get(
    store: &impl Contents,
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
