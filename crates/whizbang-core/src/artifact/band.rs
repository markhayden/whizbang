//! A band profile: the half-width, row by row, of a band along a line, with the line, the
//! centre column and the sampling windows it was measured through (ADR 0002). Its type and
//! identity are here; what makes and reads it arrives with the operations of milestone 3.

use crate::identity::{Binding, DecodeLimits, Fields};

use super::wire::{Reader, Writer};
use super::{ArtifactError, ArtifactId, ArtifactType, Origin, Record, Result, identity_of};

/// The sampling windows the band was measured through, in pixels of the image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Windows {
    pub gap: u32,
    pub step_columns: u32,
    pub texture_columns: u32,
}

/// The band along a line: wide on smooth surfaces, thin on detail.
#[derive(Debug, Clone, PartialEq)]
pub struct BandProfile {
    binding: Binding,
    line: ArtifactId,
    column: u32,
    thin: f64,
    half: Vec<f64>,
    windows: Windows,
    constants: String,
    origin: Origin,
    id: ArtifactId,
}

impl BandProfile {
    /// A band on the binding's source image. `line` is the identity of the line location
    /// the band follows; `column` the centre column; `thin` the least half-width; `half`
    /// the half-width of each row, one for every row of the image; `constants` the name of
    /// the set of constants that produced it.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        binding: Binding,
        line: ArtifactId,
        column: u32,
        thin: f64,
        half: Vec<f64>,
        windows: Windows,
        constants: &str,
        origin: Origin,
    ) -> Result<BandProfile> {
        let mut band = BandProfile {
            binding,
            line,
            column,
            thin,
            half,
            windows,
            constants: constants.to_owned(),
            origin,
            id: ArtifactId::unset(),
        };
        band.check()?;
        band.id = identity_of(&band);
        Ok(band)
    }

    fn check(&self) -> Result<()> {
        let fail = |why: String| {
            Err(ArtifactError::Invalid {
                artifact: ArtifactType::BandProfile,
                why,
            })
        };
        let space = self.binding.source_space();
        if !(self.thin.is_finite() && self.thin > 0.0) {
            return fail(format!(
                "the least half-width is {}, not a positive number",
                self.thin
            ));
        }
        if self.half.len() as u64 != u64::from(space.height()) {
            return fail(format!(
                "{} half-widths for an image of {} rows",
                self.half.len(),
                space.height()
            ));
        }
        if let Some((row, half)) = self
            .half
            .iter()
            .enumerate()
            .find(|(_, half)| !half.is_finite() || **half < self.thin - 1e-9)
        {
            return fail(format!(
                "the half-width of row {row} is {half}: not finite, or under the least of {}",
                self.thin
            ));
        }
        if self.column > space.width() {
            return fail(format!(
                "the centre column {} is outside an image {} wide",
                self.column,
                space.width()
            ));
        }
        Ok(())
    }

    pub fn id(&self) -> ArtifactId {
        self.id
    }

    /// The binding whose source image the band lies on.
    pub fn binding(&self) -> &Binding {
        &self.binding
    }

    /// The identity of the line location the band follows.
    pub fn line(&self) -> ArtifactId {
        self.line
    }

    /// The centre column, in pixels.
    pub fn column(&self) -> u32 {
        self.column
    }

    /// The least half-width, in pixels.
    pub fn thin(&self) -> f64 {
        self.thin
    }

    /// The half-width of each row, in pixels.
    pub fn half(&self) -> &[f64] {
        &self.half
    }

    pub fn windows(&self) -> Windows {
        self.windows
    }

    /// The set of constants that produced the band, such as `fan-v1`.
    pub fn constants(&self) -> &str {
        &self.constants
    }

    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// The band profile's canonical file.
    pub fn to_bytes(&self) -> Vec<u8> {
        super::file_of(self)
    }

    /// Reads a band profile's canonical file.
    pub fn from_bytes(bytes: &[u8], limits: &DecodeLimits) -> Result<BandProfile> {
        super::parse(bytes, limits)
    }

    /// The same band on another binding of the same space: what a rebind of the mask that
    /// holds it does.
    pub(crate) fn rebound(&self, binding: &Binding) -> BandProfile {
        let mut band = self.clone();
        band.binding = binding.clone();
        band.id = identity_of(&band);
        band
    }
}

impl Record for BandProfile {
    const TYPE: ArtifactType = ArtifactType::BandProfile;

    fn write(&self, writer: &mut Writer) {
        writer.binding(&self.binding);
        writer.digest(&self.line.0);
        writer.unsigned(u64::from(self.column));
        writer.float(self.thin);
        writer.count(self.half.len());
        for half in &self.half {
            writer.float(*half);
        }
        writer.unsigned(u64::from(self.windows.gap));
        writer.unsigned(u64::from(self.windows.step_columns));
        writer.unsigned(u64::from(self.windows.texture_columns));
        writer.text(&self.constants);
        writer.origin(&self.origin);
    }

    fn read(reader: &mut Reader<'_>, _limits: &DecodeLimits) -> Result<BandProfile> {
        let binding = reader.binding()?;
        let line = ArtifactId(reader.digest()?);
        let column = reader.small("column")?;
        let thin = reader.float()?;
        let mut half = Vec::new();
        for _ in 0..reader.count()? {
            half.push(reader.float()?);
        }
        let windows = Windows {
            gap: reader.small("gap")?,
            step_columns: reader.small("window")?,
            texture_columns: reader.small("window")?,
        };
        let constants = reader.text()?;
        let origin = reader.origin()?;
        BandProfile::new(
            binding, line, column, thin, half, windows, &constants, origin,
        )
    }
}
