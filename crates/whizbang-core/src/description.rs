//! The static description: what this build can do.
//!
//! It holds no runtime state (no installed models, credentials or reachability), so it is
//! identical on every machine for the same build. The reference documentation is generated
//! from it.

use serde::Serialize;

use crate::artifact::ArtifactType;
use crate::contract::{ActionDescriptor, Kind, Registry};

/// The version of the description's own shape. It goes up with any change a reader could
/// observe (ADR 0003, "What changes the version"). Version 1 listed an action's name and
/// purpose. Version 2 lists each action as its whole descriptor and adds the artifact
/// types. Version 3 lists actions by kind, recipes first, where 2 listed them as they were
/// registered. Version 4 adds `chosen_by` to each need: the argument that chooses the
/// backend, or null.
pub const DESCRIPTION_VERSION: u32 = 4;

/// Everything a door can offer, as one document.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Description {
    description_version: u32,
    name: String,
    version: String,
    actions: Vec<ActionDescriptor>,
    artifact_types: Vec<ArtifactTypeSummary>,
}

/// What a caller is told of an artifact type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArtifactTypeSummary {
    name: String,
    purpose: String,
    travels_as: String,
}

impl ArtifactTypeSummary {
    /// The type's name, as a port spells it.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// What an artifact of the type is, in a sentence.
    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    /// How an artifact of the type leaves a store and enters another. Every type is one
    /// canonical file (ADR 0002), so this is `canonical_file` for each: a copy of the file
    /// is the artifact, and `whizbang store import` reads it.
    pub fn travels_as(&self) -> &str {
        &self.travels_as
    }

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
    /// The description of a build of this version that registered these actions. It is
    /// made from a registry and in no other way, so it never lists a name twice.
    ///
    /// Actions are listed by kind: recipes, then loops, then primitives, each kind in the
    /// order its actions were registered. Recipes are what is advertised (`SPEC.md`,
    /// "Primitives, recipes and loops"), and that does not depend on the order in which a
    /// build happened to register its tools.
    pub fn new(version: &str, registry: &Registry) -> Self {
        let mut actions: Vec<ActionDescriptor> = registry.descriptors().cloned().collect();
        // A stable sort: within a kind the registration order stands.
        actions.sort_by_key(|action| match action.kind() {
            Kind::Recipe => 0,
            Kind::Loop => 1,
            Kind::Primitive => 2,
        });
        Self {
            description_version: DESCRIPTION_VERSION,
            name: "whizbang".to_owned(),
            version: version.to_owned(),
            actions,
            artifact_types: ArtifactType::ALL
                .into_iter()
                .map(ArtifactTypeSummary::of)
                .collect(),
        }
    }

    /// The version of this document's shape.
    pub fn description_version(&self) -> u32 {
        self.description_version
    }

    /// The build's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The build's version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Every registered action: recipes, then loops, then primitives, each kind in the
    /// order it was registered.
    pub fn actions(&self) -> &[ActionDescriptor] {
        &self.actions
    }

    /// The six artifact types, in the order ADR 0002 lists them.
    pub fn artifact_types(&self) -> &[ArtifactTypeSummary] {
        &self.artifact_types
    }
}
