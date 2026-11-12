//! A series: values measured along the rows of an image, such as a feather's width per row
//! or a tone ramp's step per row (ADR 0002). Its type and identity are here; what reads it
//! arrives with the operations of milestone 3.

use crate::identity::{Binding, DecodeLimits, Fields};

use super::wire::{Reader, Writer};
use super::{ArtifactError, ArtifactId, ArtifactType, Origin, Record, Result, identity_of};

/// Values per row. `values` holds `rows.len() × channels` finite numbers, row by row.
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    binding: Binding,
    name: String,
    unit: String,
    channels: u32,
    rows: Vec<u32>,
    values: Vec<f64>,
    origin: Origin,
    id: ArtifactId,
}

impl Series {
    /// A series over the rows `0..values.len() / channels`, in order.
    pub fn new(
        binding: Binding,
        name: &str,
        unit: &str,
        channels: u32,
        values: Vec<f64>,
        origin: Origin,
    ) -> Result<Series> {
        let count = values.len() as u64 / u64::from(channels.max(1));
        let rows = (0..u32::try_from(count).unwrap_or(u32::MAX)).collect();
        Series::with_rows(binding, name, unit, channels, rows, values, origin)
    }

    /// A series over the given rows of the binding's source image, ascending.
    pub fn with_rows(
        binding: Binding,
        name: &str,
        unit: &str,
        channels: u32,
        rows: Vec<u32>,
        values: Vec<f64>,
        origin: Origin,
    ) -> Result<Series> {
        let mut series = Series {
            binding,
            name: name.to_owned(),
            unit: unit.to_owned(),
            channels,
            rows,
            values,
            origin,
            id: ArtifactId::unset(),
        };
        series.check()?;
        series.id = identity_of(&series);
        Ok(series)
    }

    fn check(&self) -> Result<()> {
        let fail = |why: String| {
            Err(ArtifactError::Invalid {
                artifact: ArtifactType::Series,
                why,
            })
        };
        if self.channels == 0 {
            return fail("it has no channel".to_owned());
        }
        if self.values.len() as u64 != self.rows.len() as u64 * u64::from(self.channels) {
            return fail(format!(
                "{} values for {} rows of {} channels",
                self.values.len(),
                self.rows.len(),
                self.channels
            ));
        }
        if self.rows.windows(2).any(|pair| pair[0] >= pair[1]) {
            return fail("its rows are not ascending".to_owned());
        }
        let height = self.binding.source_space().height();
        if self.rows.last().is_some_and(|row| *row >= height) {
            return fail(format!("a row is past the image's {height} rows"));
        }
        if self.values.iter().any(|value| !value.is_finite()) {
            return fail("a value is not finite".to_owned());
        }
        Ok(())
    }

    pub fn id(&self) -> ArtifactId {
        self.id
    }

    /// The binding whose source image the rows are rows of.
    pub fn binding(&self) -> &Binding {
        &self.binding
    }

    /// What was measured, such as `sigma_px`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The unit: `px`, `levels`, `ratio`, `degrees`.
    pub fn unit(&self) -> &str {
        &self.unit
    }

    /// Values per row: 1, or 3 for a colour.
    pub fn channels(&self) -> u32 {
        self.channels
    }

    /// The row of each entry, ascending.
    pub fn rows(&self) -> &[u32] {
        &self.rows
    }

    pub fn values(&self) -> &[f64] {
        &self.values
    }

    /// The values of one row.
    pub fn row(&self, row: u32) -> Option<&[f64]> {
        let index = self.rows.binary_search(&row).ok()?;
        let channels = self.channels as usize;
        self.values.get(index * channels..(index + 1) * channels)
    }

    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// The series' canonical file.
    pub fn to_bytes(&self) -> Vec<u8> {
        super::file_of(self)
    }

    /// Reads a series' canonical file.
    pub fn from_bytes(bytes: &[u8], limits: &DecodeLimits) -> Result<Series> {
        super::parse(bytes, limits)
    }

    /// The same series on another binding of the same space: what a rebind of the mask
    /// that holds it does.
    pub(crate) fn rebound(&self, binding: &Binding) -> Series {
        let mut series = self.clone();
        series.binding = binding.clone();
        series.id = identity_of(&series);
        series
    }
}

impl Record for Series {
    const TYPE: ArtifactType = ArtifactType::Series;

    fn write(&self, writer: &mut Writer) {
        writer.binding(&self.binding);
        writer.text(&self.name);
        writer.text(&self.unit);
        writer.unsigned(u64::from(self.channels));
        writer.count(self.rows.len());
        for row in &self.rows {
            writer.unsigned(u64::from(*row));
        }
        writer.count(self.values.len());
        for value in &self.values {
            writer.float(*value);
        }
        writer.origin(&self.origin);
    }

    fn read(reader: &mut Reader<'_>, _limits: &DecodeLimits) -> Result<Series> {
        let binding = reader.binding()?;
        let (name, unit) = (reader.text()?, reader.text()?);
        let channels = reader.small("count of channels")?;
        let mut rows = Vec::new();
        for _ in 0..reader.count()? {
            rows.push(reader.small("row")?);
        }
        let mut values = Vec::new();
        for _ in 0..reader.count()? {
            values.push(reader.float()?);
        }
        let origin = reader.origin()?;
        Series::with_rows(binding, &name, &unit, channels, rows, values, origin)
    }
}
