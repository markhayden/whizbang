//! The mask record (ADR 0002): three planes, what was measured to draw them, what should be
//! done to the pixels under them, and what they were made from. A mask belongs to a
//! binding, and its planes are the size of that binding's source image.
//!
//! - **Core** is binary: 255 where a filler may paint, with a paste weight of 1.
//! - **Blend** holds the weights outside the core as 8-bit fractions. It is 0 inside the
//!   core and under protect.
//! - **Protect** is binary: 255 where nothing may change.
//!
//! One rule gives the weight of a pixel, [`effective_weight`], and everything that renders,
//! refines or pastes through a mask uses it.

use crate::identity::{
    Binding, DecodeLimits, Destination, Digest, Fields, Mapping, PixelDigest, Space,
};

use super::wire::{Reader, Writer};
use super::{
    ArtifactError, ArtifactId, ArtifactType, BandProfile, Origin, Record, Result, Series, Step,
    identity_of, read_inside,
};

/// What a mask was made for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UseCase {
    Seam,
    Remove,
    Repair,
    Protect,
    Custom,
}

/// What was measured to draw a mask that is a band along a line. A mask holds its band and
/// its feather whole, so that it can be read where they are not.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    /// The band's half-width, row by row. It names the line location it follows.
    pub band: BandProfile,
    /// The feather's sigma, row by row, in pixels.
    pub sigma: Series,
}

/// A change to the pixels that belongs with the mask. It is data on the record, not
/// pixels; applying it arrives with the operations of milestone 3.
#[derive(Debug, Clone, PartialEq)]
pub enum Prescription {
    /// A colour step, row by row, carried across the band. `step` has three channels and
    /// one entry for every row; `share` is the part of the step each side moves toward
    /// the other, from 0 to 1.
    ToneRamp {
        version: u32,
        step: Series,
        share: f64,
    },
}

/// The version of a tone ramp this build writes and reads.
const TONE_RAMP_VERSION: u32 = 1;

/// What a mask was made from, by identity.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Source {
    /// The locations it was drawn from.
    pub locations: Vec<ArtifactId>,
    /// The instances a segmenter found that it was drawn from.
    pub instances: Vec<Digest>,
}

/// What replacing a mask's core cleared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refinement {
    pub blend_cleared: bool,
    pub profile_cleared: bool,
}

/// The effective weight of one pixel: 0 where protected, else 1 in the core, else the
/// blend over 255.
pub fn effective_weight(core: u8, blend: u8, protect: u8) -> f32 {
    if protect != 0 {
        0.0
    } else if core != 0 {
        1.0
    } else {
        f32::from(blend) / 255.0
    }
}

/// A mask. Every constructor and every change keeps the invariants and the identity
/// current.
#[derive(Debug, Clone, PartialEq)]
pub struct Mask {
    binding: Binding,
    use_case: UseCase,
    core: Vec<u8>,
    blend: Vec<u8>,
    protect: Vec<u8>,
    profile: Option<Profile>,
    prescriptions: Vec<Prescription>,
    source: Source,
    origin: Origin,
    id: ArtifactId,
}

fn invalid(why: impl Into<String>) -> ArtifactError {
    ArtifactError::Invalid {
        artifact: ArtifactType::Mask,
        why: why.into(),
    }
}

impl Mask {
    /// A mask on the binding's source image with this core, no blend and nothing
    /// protected. The core has one byte for every pixel, row by row; any byte that is not
    /// 0 is in the core.
    pub fn from_core(
        binding: Binding,
        use_case: UseCase,
        core: Vec<u8>,
        source: Source,
        origin: Origin,
    ) -> Result<Mask> {
        let space = binding.source_space();
        let samples = samples(&space);
        if core.len() as u64 != samples {
            return Err(invalid(format!(
                "the core has {} samples, and the image is {}x{}",
                core.len(),
                space.width(),
                space.height()
            )));
        }
        let empty = vec![0; core.len()];
        let mut mask = Mask {
            binding,
            use_case,
            core: binary(core),
            blend: empty.clone(),
            protect: empty,
            profile: None,
            prescriptions: Vec::new(),
            source,
            origin,
            id: ArtifactId::unset(),
        };
        mask.id = identity_of(&mask);
        Ok(mask)
    }

    pub fn id(&self) -> ArtifactId {
        self.id
    }

    /// The binding the mask belongs to.
    pub fn binding(&self) -> &Binding {
        &self.binding
    }

    /// The image the planes lie on: the binding's source, by its canonical pixel digest.
    pub fn image(&self) -> PixelDigest {
        self.binding.source()
    }

    /// The space of that image, which is the size of the planes.
    pub fn space(&self) -> Space {
        self.binding.source_space()
    }

    pub fn use_case(&self) -> UseCase {
        self.use_case
    }

    /// The core plane, one byte a pixel, row by row: 0 or 255.
    pub fn core(&self) -> &[u8] {
        &self.core
    }

    /// The blend plane: 0 inside the core and under protect.
    pub fn blend(&self) -> &[u8] {
        &self.blend
    }

    /// The protect plane: 0 or 255.
    pub fn protect(&self) -> &[u8] {
        &self.protect
    }

    pub fn profile(&self) -> Option<&Profile> {
        self.profile.as_ref()
    }

    pub fn prescriptions(&self) -> &[Prescription] {
        &self.prescriptions
    }

    pub fn source(&self) -> &Source {
        &self.source
    }

    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// The effective weight of the pixel in column `x` of row `y`, which are inside the
    /// image.
    pub fn weight(&self, x: u32, y: u32) -> f32 {
        let at = y as usize * self.space().width() as usize + x as usize;
        effective_weight(self.core[at], self.blend[at], self.protect[at])
    }

    // --- changes ------------------------------------------------------------------------

    fn sized(&self, plane: &[u8], what: &str) -> Result<()> {
        if plane.len() != self.core.len() {
            return Err(invalid(format!(
                "the new {what} has {} samples, and the mask has {}",
                plane.len(),
                self.core.len()
            )));
        }
        Ok(())
    }

    /// Replaces the core. The blend is cleared, and the profile and every prescription are
    /// removed: they were drawn from the old core. Protect is kept, and still wins.
    pub fn replace_core(&mut self, core: Vec<u8>) -> Result<Refinement> {
        self.sized(&core, "core")?;
        let report = Refinement {
            blend_cleared: true,
            profile_cleared: self.profile.is_some() || !self.prescriptions.is_empty(),
        };
        self.core = binary(core);
        self.blend = vec![0; self.core.len()];
        self.profile = None;
        self.prescriptions.clear();
        self.id = identity_of(self);
        Ok(report)
    }

    /// Sets the blend. It is forced to 0 inside the core and under protect.
    pub fn set_blend(&mut self, mut blend: Vec<u8>) -> Result<()> {
        self.sized(&blend, "blend")?;
        for (at, sample) in blend.iter_mut().enumerate() {
            if self.core[at] != 0 || self.protect[at] != 0 {
                *sample = 0;
            }
        }
        self.blend = blend;
        self.id = identity_of(self);
        Ok(())
    }

    /// Adds to protect every pixel of `plane` that is not 0. The core and the blend are
    /// cleared under it; the profile and the prescriptions stay.
    pub fn protect_union(&mut self, plane: &[u8]) -> Result<()> {
        self.sized(plane, "protect")?;
        for (at, sample) in plane.iter().enumerate() {
            if *sample != 0 {
                self.protect[at] = 255;
                self.core[at] = 0;
                self.blend[at] = 0;
            }
        }
        self.id = identity_of(self);
        Ok(())
    }

    /// Sets the profile: the band and the feather's sigma. Both belong to the mask's
    /// binding, and the sigma has one channel and one entry for every row.
    pub fn set_profile(&mut self, band: BandProfile, sigma: Series) -> Result<()> {
        let profile = Profile { band, sigma };
        self.check_profile(&profile)?;
        self.profile = Some(profile);
        self.id = identity_of(self);
        Ok(())
    }

    /// Adds a tone ramp: a colour step for every row, and the share of it each side moves.
    pub fn push_tone_ramp(&mut self, step: Series, share: f64) -> Result<()> {
        let prescription = Prescription::ToneRamp {
            version: TONE_RAMP_VERSION,
            step,
            share,
        };
        self.check_prescription(&prescription)?;
        self.prescriptions.push(prescription);
        self.id = identity_of(self);
        Ok(())
    }

    /// Records one more step in the mask's origin. The identity does not change.
    pub fn record(&mut self, step: Step) -> Result<()> {
        self.origin = self.origin.clone().then(step)?;
        Ok(())
    }

    // --- which image -----------------------------------------------------------------------

    /// Whether the mask may be used on this image: the image it belongs to, by its
    /// canonical pixel digest, in its space. Another image of the same size is refused.
    pub fn require_image(&self, image: PixelDigest, space: Space) -> Result<()> {
        if image != self.image() {
            return Err(ArtifactError::WrongImage(format!(
                "the mask belongs to the image {}, not to {image}; to use it on an image \
                 derived from its own, rebind it",
                self.image()
            )));
        }
        if space != self.space() {
            return Err(ArtifactError::WrongImage(format!(
                "the mask's image is in {:?}, not in {space:?}",
                self.space()
            )));
        }
        Ok(())
    }

    /// The same mask on an image derived from its own, such as the composite a fill made
    /// of it. `derivation` is the binding that places the new image onto the mask's image:
    /// its destination is the mask's image, both its spaces are the mask's, and its
    /// mapping is the identity, so every pixel of the planes keeps its place. Anything
    /// else is refused: an image in its own space, an image placed onto another image, a
    /// crop or any mapping that moves a pixel.
    ///
    /// The rebound mask belongs to `derivation`, so it names both images; it has a new
    /// identity, the same planes, and one more step in its origin.
    pub fn rebind(&self, derivation: &Binding) -> Result<Mask> {
        let refuse = |why: String| Err(ArtifactError::Rebind(why));
        match derivation.destination() {
            Destination::Image(image) if image == self.image() => {}
            Destination::Image(image) => {
                return refuse(format!(
                    "the binding places its image onto {image}, and the mask belongs to {}",
                    self.image()
                ));
            }
            Destination::Root => {
                return refuse(format!(
                    "the binding places its image onto nothing: it does not show that \
                     image to be derived from the mask's image {}",
                    self.image()
                ));
            }
        }
        if *derivation.mapping() != Mapping::Identity {
            return refuse(
                "a rebind keeps every pixel in its place: the binding's mapping must be the \
                 identity"
                    .to_owned(),
            );
        }
        if derivation.source_space() != self.space()
            || derivation.destination_space() != self.space()
        {
            return refuse(format!(
                "the binding's spaces are not the mask's {:?}",
                self.space()
            ));
        }
        if derivation.source() == self.image() {
            return refuse("the binding's image is the mask's own image".to_owned());
        }
        let mut mask = self.clone();
        mask.binding = derivation.clone();
        if let Some(profile) = &mut mask.profile {
            profile.band = profile.band.rebound(derivation);
            profile.sigma = profile.sigma.rebound(derivation);
        }
        for prescription in &mut mask.prescriptions {
            match prescription {
                Prescription::ToneRamp { step, .. } => *step = step.rebound(derivation),
            }
        }
        mask.origin = mask.origin.then(
            Step::new("rebind", "1")
                .argument("binding", &derivation.id().to_string())
                .input("mask", self.id.0),
        )?;
        mask.check()?;
        mask.id = identity_of(&mask);
        Ok(mask)
    }

    // --- invariants ------------------------------------------------------------------------

    fn check_profile(&self, profile: &Profile) -> Result<()> {
        let rows = self.space().height() as usize;
        if *profile.band.binding() != self.binding || *profile.sigma.binding() != self.binding {
            return Err(invalid("the profile does not belong to the mask's binding"));
        }
        // A band has one half-width for every row by its own rule.
        if profile.sigma.channels() != 1 || profile.sigma.rows().len() != rows {
            return Err(invalid(
                "the feather's sigma has one channel and one entry for every row",
            ));
        }
        Ok(())
    }

    fn check_prescription(&self, prescription: &Prescription) -> Result<()> {
        let rows = self.space().height() as usize;
        match prescription {
            Prescription::ToneRamp {
                version,
                step,
                share,
            } => {
                if *version != TONE_RAMP_VERSION {
                    return Err(invalid(format!(
                        "a tone ramp of version {version}; this build reads version \
                         {TONE_RAMP_VERSION}"
                    )));
                }
                if *step.binding() != self.binding {
                    return Err(invalid(
                        "a tone ramp's step does not belong to the mask's binding",
                    ));
                }
                if step.channels() != 3 || step.rows().len() != rows {
                    return Err(invalid(
                        "a tone ramp's step has three channels and one entry for every row",
                    ));
                }
                if !(0.0..=1.0).contains(share) {
                    return Err(invalid(format!(
                        "a tone ramp's share is from 0 to 1, not {share}"
                    )));
                }
            }
        }
        Ok(())
    }

    /// Every invariant of a mask, named so that a refusal says which one failed.
    fn check(&self) -> Result<()> {
        let space = self.space();
        let samples = samples(&space);
        for (what, plane) in [
            ("core", &self.core),
            ("blend", &self.blend),
            ("protect", &self.protect),
        ] {
            if plane.len() as u64 != samples {
                return Err(invalid(format!(
                    "the {what} plane has {} samples, and the image is {}x{}",
                    plane.len(),
                    space.width(),
                    space.height()
                )));
            }
        }
        let width = space.width() as usize;
        for at in 0..self.core.len() {
            let (core, blend, protect) = (self.core[at], self.blend[at], self.protect[at]);
            let place = || format!("at ({}, {})", at % width, at / width);
            if core != 0 && core != 255 {
                return Err(invalid(format!(
                    "the core is binary, and holds {core} {}",
                    place()
                )));
            }
            if protect != 0 && protect != 255 {
                return Err(invalid(format!(
                    "protect is binary, and holds {protect} {}",
                    place()
                )));
            }
            if blend != 0 && core != 0 {
                return Err(invalid(format!(
                    "the blend is zero inside the core, and holds {blend} {}",
                    place()
                )));
            }
            if blend != 0 && protect != 0 {
                return Err(invalid(format!(
                    "the blend is zero under protect, and holds {blend} {}",
                    place()
                )));
            }
        }
        if let Some(profile) = &self.profile {
            self.check_profile(profile)?;
        }
        for prescription in &self.prescriptions {
            self.check_prescription(prescription)?;
        }
        Ok(())
    }

    // --- the file --------------------------------------------------------------------------

    /// The mask's canonical file: everything it holds, in one file that reads the same in
    /// any store.
    pub fn to_bytes(&self) -> Vec<u8> {
        super::file_of(self)
    }

    /// Reads a mask's canonical file. Anything else is refused, and a picture above all: a
    /// rendering of a mask is not a mask.
    pub fn from_bytes(bytes: &[u8], limits: &DecodeLimits) -> Result<Mask> {
        super::parse(bytes, limits)
    }
}

fn samples(space: &Space) -> u64 {
    u64::from(space.width()) * u64::from(space.height())
}

/// Any sample that is not 0 becomes 255.
fn binary(mut plane: Vec<u8>) -> Vec<u8> {
    for sample in &mut plane {
        if *sample != 0 {
            *sample = 255;
        }
    }
    plane
}

impl Record for Mask {
    const TYPE: ArtifactType = ArtifactType::Mask;

    fn write(&self, writer: &mut Writer) {
        writer.binding(&self.binding);
        writer.unsigned(match self.use_case {
            UseCase::Seam => 0,
            UseCase::Remove => 1,
            UseCase::Repair => 2,
            UseCase::Protect => 3,
            UseCase::Custom => 4,
        });
        writer.plane(&self.core);
        writer.plane(&self.blend);
        writer.plane(&self.protect);
        match &self.profile {
            None => writer.unsigned(0),
            Some(profile) => {
                writer.unsigned(1);
                writer.inside(ArtifactType::BandProfile.kind(), |inner| {
                    profile.band.write(inner);
                });
                writer.inside(ArtifactType::Series.kind(), |inner| {
                    profile.sigma.write(inner);
                });
            }
        }
        writer.count(self.prescriptions.len());
        for prescription in &self.prescriptions {
            match prescription {
                Prescription::ToneRamp {
                    version,
                    step,
                    share,
                } => {
                    writer.unsigned(0);
                    writer.unsigned(u64::from(*version));
                    writer.inside(ArtifactType::Series.kind(), |inner| step.write(inner));
                    writer.float(*share);
                }
            }
        }
        writer.count(self.source.locations.len());
        for location in &self.source.locations {
            writer.digest(&location.0);
        }
        writer.count(self.source.instances.len());
        for instance in &self.source.instances {
            writer.digest(instance);
        }
        writer.origin(&self.origin);
    }

    fn read(reader: &mut Reader<'_>, limits: &DecodeLimits) -> Result<Mask> {
        let binding = reader.binding()?;
        let use_case = match reader.unsigned()? {
            0 => UseCase::Seam,
            1 => UseCase::Remove,
            2 => UseCase::Repair,
            3 => UseCase::Protect,
            4 => UseCase::Custom,
            other => {
                return Err(ArtifactError::Format(format!(
                    "{other} is not what a mask is made for"
                )));
            }
        };
        let space = binding.source_space();
        let samples = samples(&space);
        if samples > limits.max_pixels {
            return Err(ArtifactError::Identity(
                crate::identity::IdentityError::Limit(format!(
                    "{}x{} is {samples} pixels, over the limit of {}",
                    space.width(),
                    space.height(),
                    limits.max_pixels
                )),
            ));
        }
        let core = reader.plane(samples)?;
        let blend = reader.plane(samples)?;
        let protect = reader.plane(samples)?;
        let profile = match reader.unsigned()? {
            0 => None,
            1 => Some(Profile {
                band: read_inside(reader, limits)?,
                sigma: read_inside(reader, limits)?,
            }),
            other => {
                return Err(ArtifactError::Format(format!(
                    "{other} does not say whether the mask has a profile"
                )));
            }
        };
        let mut prescriptions = Vec::new();
        for _ in 0..reader.count()? {
            match reader.unsigned()? {
                0 => prescriptions.push(Prescription::ToneRamp {
                    version: reader.small("version")?,
                    step: read_inside(reader, limits)?,
                    share: reader.float()?,
                }),
                other => {
                    return Err(ArtifactError::Format(format!(
                        "{other} is not a kind of prescription"
                    )));
                }
            }
        }
        let mut source = Source::default();
        for _ in 0..reader.count()? {
            source.locations.push(ArtifactId(reader.digest()?));
        }
        for _ in 0..reader.count()? {
            source.instances.push(reader.digest()?);
        }
        let origin = reader.origin()?;
        let mut mask = Mask {
            binding,
            use_case,
            core,
            blend,
            protect,
            profile,
            prescriptions,
            source,
            origin,
            id: ArtifactId::unset(),
        };
        mask.check()?;
        mask.id = identity_of(&mask);
        Ok(mask)
    }
}
