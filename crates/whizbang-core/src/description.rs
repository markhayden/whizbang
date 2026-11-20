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
/// backend, or null. Version 5 adds `requests` to each need: how many requests one
/// call may send the backend that answers it. Version 6 adds `feature_from` to each need:
/// the argument whose value names one more feature, or null (plan 1D, task Q1). Version 7
/// adds `marks`: each tool the upper rungs apply to, with the mark its record states
/// (`SPEC.md` section 8, "Marks"; plan m2c, task X1, item 6). Version 8 adds `use` to an
/// action that says when to use it, as every recipe of a file does (plan m3, task R1,
/// item 7; ADR 0033); an action that says nothing of it has no `use`. Version 9 adds
/// `results` to an action that hands back a result beside its artifacts, `verdict` or
/// `measurements` (plan m3, task Q1, item 2; ADR 0035); an action that hands back none has
/// no `results`. Version 10 adds `findings` to what `results` may list, and `plans`,
/// true, to an action that plans its calls (plan m4, task K1, items 1 and 3; ADR 0004, "A
/// call's plan"); an action that plans nothing has no `plans`. With it every recipe of
/// two steps or more gains the argument `from` and an input for each output of each step
/// before its last (ADR 0033, "A run from a step").
pub const DESCRIPTION_VERSION: u32 = 10;

/// Everything a door can offer, as one document.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Description {
    description_version: u32,
    name: String,
    version: String,
    actions: Vec<ActionDescriptor>,
    marks: Vec<MarkSummary>,
    artifact_types: Vec<ArtifactTypeSummary>,
}

/// A tool's mark (`SPEC.md` section 8, "Marks"): `experimental`, `unverified` or `admitted`,
/// as the tool's record states it. A tool without a record has none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MarkSummary {
    tool: String,
    mark: String,
}

impl MarkSummary {
    /// The tool's name, as its action is named.
    pub fn tool(&self) -> &str {
        &self.tool
    }

    /// The mark.
    pub fn mark(&self) -> &str {
        &self.mark
    }
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
            marks: Vec::new(),
            artifact_types: ArtifactType::ALL
                .into_iter()
                .map(ArtifactTypeSummary::of)
                .collect(),
        }
    }

    /// The same description, with these marks: each a tool and its mark, in this order. A
    /// mark of a tool this build does not register is left out.
    pub fn with_marks<'a>(mut self, marks: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let registered = |tool: &str| self.actions.iter().any(|action| action.name() == tool);
        let marks: Vec<MarkSummary> = marks
            .into_iter()
            .filter(|(tool, _)| registered(tool))
            .map(|(tool, mark)| MarkSummary {
                tool: tool.to_owned(),
                mark: mark.to_owned(),
            })
            .collect();
        self.marks = marks;
        self
    }

    /// Each tool the upper rungs apply to, with its mark.
    pub fn marks(&self) -> &[MarkSummary] {
        &self.marks
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
