//! A plan: how a wrapped cut was taken from a panorama, and so how it goes back (ADR 0002).
//! Its type and identity are here; cutting and putting arrive with the operations of
//! milestone 3. The mapping a plan describes is ADR 0001's wrap.

use crate::identity::{Binding, DecodeLimits, Fields, Projection, Topology};

use super::wire::{Reader, Writer};
use super::{ArtifactError, ArtifactId, ArtifactType, Origin, Record, Result, identity_of};

/// The plan of a cut: the panorama it is cut from, and the width of the strip taken from
/// each side of the seam.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    binding: Binding,
    strip: u32,
    origin: Origin,
    id: ArtifactId,
}

impl Plan {
    /// A plan to cut the binding's source image, which is a complete panorama. The strip
    /// is even, at least 2, and at most half the panorama's width.
    pub fn new(binding: Binding, strip: u32, origin: Origin) -> Result<Plan> {
        let fail = |why: String| {
            Err(ArtifactError::Invalid {
                artifact: ArtifactType::Plan,
                why,
            })
        };
        let space = binding.source_space();
        if (space.projection(), space.topology()) != (Projection::Equirectangular, Topology::Wrap) {
            return fail("a wrapped cut needs a complete panorama".to_owned());
        }
        if strip < 2 || !strip.is_multiple_of(2) || 2 * u64::from(strip) > u64::from(space.width())
        {
            return fail(format!(
                "strip {strip} does not fit a panorama {} wide: it is even, at least 2, and \
                 at most half the width",
                space.width()
            ));
        }
        let mut plan = Plan {
            binding,
            strip,
            origin,
            id: ArtifactId::unset(),
        };
        plan.id = identity_of(&plan);
        Ok(plan)
    }

    pub fn id(&self) -> ArtifactId {
        self.id
    }

    /// The binding whose source image is the panorama that is cut.
    pub fn binding(&self) -> &Binding {
        &self.binding
    }

    /// The width of the strip taken from each side of the seam.
    pub fn strip(&self) -> u32 {
        self.strip
    }

    /// The panorama's columns `[start, end)` that the canvas's left half holds: its last
    /// strip.
    pub fn left_span(&self) -> [u32; 2] {
        let width = self.binding.source_space().width();
        [width - self.strip, width]
    }

    /// The panorama's columns `[start, end)` that the canvas's right half holds: its first
    /// strip.
    pub fn right_span(&self) -> [u32; 2] {
        [0, self.strip]
    }

    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// The plan's canonical file.
    pub fn to_bytes(&self) -> Vec<u8> {
        super::file_of(self)
    }

    /// Reads a plan's canonical file.
    pub fn from_bytes(bytes: &[u8], limits: &DecodeLimits) -> Result<Plan> {
        super::parse(bytes, limits)
    }
}

impl Record for Plan {
    const TYPE: ArtifactType = ArtifactType::Plan;

    fn write(&self, writer: &mut Writer) {
        writer.binding(&self.binding);
        writer.unsigned(u64::from(self.strip));
        writer.origin(&self.origin);
    }

    fn read(reader: &mut Reader<'_>, _limits: &DecodeLimits) -> Result<Plan> {
        let binding = reader.binding()?;
        let strip = reader.small("strip")?;
        let origin = reader.origin()?;
        Plan::new(binding, strip, origin)
    }
}
