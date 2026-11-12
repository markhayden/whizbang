//! The static description: what this build can do.
//!
//! It holds no runtime state (no installed models, credentials or reachability), so it is
//! identical on every machine for the same build. The reference documentation is generated
//! from it.

use serde::Serialize;

use crate::artifact::ArtifactType;
use crate::contract::{ActionDescriptor, Registry};

/// The version of the description's own shape. Version 2 lists each action as its whole
/// descriptor and adds the artifact types; version 1 listed an action's name and purpose.
pub const DESCRIPTION_VERSION: u32 = 2;

/// Everything a door can offer, as one document.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Description {
    pub description_version: u32,
    pub name: String,
    pub version: String,
    /// Every registered action, in the order it was registered.
    pub actions: Vec<ActionDescriptor>,
    /// The six artifact types, in the order ADR 0002 lists them.
    pub artifact_types: Vec<ArtifactTypeSummary>,
}

/// What a caller is told of an artifact type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArtifactTypeSummary {
    /// The type's name, as a port spells it.
    pub name: String,
    /// What an artifact of the type is, in a sentence.
    pub purpose: String,
    /// How an artifact of the type leaves a store and enters another. Every type is one
    /// canonical file (ADR 0002), so this is `canonical_file` for each: a copy of the file
    /// is the artifact, and `whizbang store import` reads it.
    pub travels_as: String,
}

impl ArtifactTypeSummary {
    fn of(artifact: ArtifactType) -> ArtifactTypeSummary {
        let purpose = match artifact {
            ArtifactType::Mask => {
                "What to change in an image (core), how the change fades (blend) and what to keep (protect), with the band profile and the prescriptions it carries."
            }
            ArtifactType::Location => {
                "Where something is in an image: a box, a polygon, a view, a line, a stroke, the whole image, or that it is absent."
            }
            ArtifactType::Image => {
                "A picture file held whole: a photograph, a composite or a crop; or a rendering of another artifact, which is never accepted in that artifact's place."
            }
            ArtifactType::BandProfile => {
                "A band along a line: its half-width for each row, with the line and the sampling windows it was measured through."
            }
            ArtifactType::Series => {
                "Values measured along the rows of an image, such as the width of a feather or the step of a tone ramp for each row."
            }
            ArtifactType::Plan => {
                "How a wrapped cut was taken from a panorama, and so how it goes back: the width of the strip taken from each side of the seam."
            }
        };
        ArtifactTypeSummary {
            name: artifact.name().to_owned(),
            purpose: purpose.to_owned(),
            travels_as: "canonical_file".to_owned(),
        }
    }
}

impl Description {
    /// The description of a build of this version that registered these actions.
    pub fn new(version: &str, registry: &Registry) -> Self {
        Self {
            description_version: DESCRIPTION_VERSION,
            name: "whizbang".to_owned(),
            version: version.to_owned(),
            actions: registry.descriptors().cloned().collect(),
            artifact_types: ArtifactType::ALL
                .into_iter()
                .map(ArtifactTypeSummary::of)
                .collect(),
        }
    }
}
