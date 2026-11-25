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
/// before its last (ADR 0033, "A run from a step"). Version 11 adds `kits`, each kit of the
/// build with its title and its line, in the order a door lists them; and, to an action
/// that says them, `kit`, the kit it belongs to, `title`, its title as a person reads it,
/// and `uses`, the other kit a loop uses (plan m5, task W1, item 2; ADR 0040): every
/// recipe and loop this build ships says its kit and its title. Version 12 lets `marks`
/// name a tool that is no action, one a run offers a judge only when it names it, a tool
/// file or a detail, so that every mark a record states is shown (plan mE, task A0, item 3).
pub const DESCRIPTION_VERSION: u32 = 12;

/// Everything a door can offer, as one document.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Description {
    description_version: u32,
    name: String,
    version: String,
    kits: Vec<Kit>,
    actions: Vec<ActionDescriptor>,
    marks: Vec<MarkSummary>,
    artifact_types: Vec<ArtifactTypeSummary>,
}

/// A kit (ADR 0040): a family of work with one job, by its name (`rascal`), its title as
/// a person reads it (`Rascal`) and its short line (`edit pictures`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Kit {
    name: String,
    title: String,
    line: String,
}

impl Kit {
    /// A kit, when its name keeps the rule of an action's name, its title the rule of a
    /// title, and its line the rule of a purpose.
    pub fn new(name: &str, title: &str, line: &str) -> Result<Kit, String> {
        crate::contract::text::action_name(name)?;
        crate::contract::text::title(title)?;
        crate::contract::text::plain_line("line", line)?;
        Ok(Kit {
            name: name.to_owned(),
            title: title.to_owned(),
            line: line.to_owned(),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn line(&self) -> &str {
        &self.line
    }
}

/// A tool's mark (`SPEC.md` section 8, "Marks"): `experimental`, `unverified` or `admitted`,
/// as the tool's record states it. A tool without a record has none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MarkSummary {
    tool: String,
    mark: String,
}

impl MarkSummary {
    /// The tool's name, as its action is named, or as a run names a tool that is no action.
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
            kits: Vec::new(),
            actions,
            marks: Vec::new(),
            artifact_types: ArtifactType::ALL
                .into_iter()
                .map(ArtifactTypeSummary::of)
                .collect(),
        }
    }

    /// The same description, with these marks: each a tool and its mark, in this order.
    /// `offered` names the tools of this build that are no action, which a run offers a
    /// judge only when it names them (a tool file, a detail). A mark of a tool that is
    /// neither an action this build registers nor one of those is left out.
    pub fn with_marks<'a>(
        mut self,
        marks: impl IntoIterator<Item = (&'a str, &'a str)>,
        offered: &[&str],
    ) -> Self {
        let shown = |tool: &str| {
            offered.contains(&tool) || self.actions.iter().any(|action| action.name() == tool)
        };
        let marks: Vec<MarkSummary> = marks
            .into_iter()
            .filter(|(tool, _)| shown(tool))
            .map(|(tool, mark)| MarkSummary {
                tool: tool.to_owned(),
                mark: mark.to_owned(),
            })
            .collect();
        self.marks = marks;
        self
    }

    /// The same description, with these kits, in this order: a kit named twice is kept
    /// once, at its first place.
    pub fn with_kits(mut self, kits: &[Kit]) -> Self {
        let mut listed: Vec<Kit> = Vec::new();
        for kit in kits {
            if !listed.iter().any(|one| one.name == kit.name) {
                listed.push(kit.clone());
            }
        }
        self.kits = listed;
        self
    }

    /// The kits, in the order a door lists them.
    pub fn kits(&self) -> &[Kit] {
        &self.kits
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
