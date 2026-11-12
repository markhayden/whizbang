//! Colour profiles (ADR 0001). Canonical pixels are sRGB. An image with no embedded ICC
//! profile, or with one that is sRGB, keeps its samples. An image with any other RGB matrix
//! profile is converted to sRGB by the steps below. A profile of any other kind has no
//! canonical reading and is refused.
//!
//! The steps, for one pixel of 8-bit samples:
//!
//! 1. Each sample, as a fraction of 255, goes through its channel's tone curve to linear
//!    light.
//! 2. The three linear values are multiplied by the matrix of the profile's colorants,
//!    which gives XYZ under the profile connection white, and then by the inverse of the
//!    matrix of sRGB's colorants.
//! 3. Each result is clipped to 0 to 1, encoded with the sRGB curve, multiplied by 255 and
//!    rounded half up.
//!
//! The arithmetic is IEEE 754 double precision, and powers are computed by `libm`, so the
//! result is the same on every machine.

/// sRGB's red, green and blue colorants as an ICC profile holds them: X, Y and Z under the
/// profile connection white, each a fraction of 65536.
const SRGB_COLORANTS: [[f64; 3]; 3] = [
    [28578.0 / 65536.0, 14581.0 / 65536.0, 912.0 / 65536.0],
    [25241.0 / 65536.0, 46981.0 / 65536.0, 6362.0 / 65536.0],
    [9376.0 / 65536.0, 3972.0 / 65536.0, 46799.0 / 65536.0],
];

/// How far a profile's colorants and curves may be from sRGB's and still be sRGB.
const SRGB_TOLERANCE: f64 = 0.001;

/// The most entries a tone curve's table may have.
const MAX_CURVE_ENTRIES: usize = 65_536;

/// The most tags a profile may list.
const MAX_TAGS: usize = 1_024;

/// sRGB's curve from an encoded value to linear light.
fn srgb_decode(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        libm::pow((v + 0.055) / 1.055, 2.4)
    }
}

/// sRGB's curve from linear light to an encoded value.
fn srgb_encode(v: f64) -> f64 {
    if v <= 0.003_130_8 {
        12.92 * v
    } else {
        1.055 * libm::pow(v, 1.0 / 2.4) - 0.055
    }
}

/// An RGB matrix profile, read: what each 8-bit sample is in linear light, and the
/// colorants.
pub(super) struct MatrixProfile {
    /// For each channel, the linear value of each of the 256 samples.
    linear: [[f64; 256]; 3],
    /// The red, green and blue colorants, each as X, Y and Z.
    colorants: [[f64; 3]; 3],
}

impl MatrixProfile {
    /// Reads an ICC profile. The error says why it has no canonical reading.
    pub(super) fn parse(profile: &[u8]) -> Result<MatrixProfile, String> {
        let header = profile
            .get(..128)
            .ok_or("it is shorter than a profile header")?;
        if &header[36..40] != b"acsp" {
            return Err("it has no profile signature".to_owned());
        }
        if &header[16..20] != b"RGB " {
            return Err(format!(
                "its data is {:?}, not RGB",
                String::from_utf8_lossy(&header[16..20])
            ));
        }
        if &header[20..24] != b"XYZ " {
            return Err("it is not connected through XYZ".to_owned());
        }
        let count = usize::try_from(read_u32(profile, 128)?).map_err(|_| "too many tags")?;
        if count > MAX_TAGS {
            return Err(format!("it lists {count} tags"));
        }
        let element = |signature: &[u8; 4]| -> Result<&[u8], String> {
            for index in 0..count {
                let entry = 132 + 12 * index;
                let name = profile
                    .get(entry..entry + 4)
                    .ok_or("its tag table is cut short")?;
                if name == signature {
                    let offset = read_u32(profile, entry + 4)? as usize;
                    let size = read_u32(profile, entry + 8)? as usize;
                    let end = offset.checked_add(size).ok_or("a tag is out of range")?;
                    return profile
                        .get(offset..end)
                        .ok_or_else(|| "a tag points past the end".to_owned());
                }
            }
            Err(format!(
                "it has no {} tag: only matrix profiles are read",
                String::from_utf8_lossy(signature)
            ))
        };
        let mut colorants = [[0.0; 3]; 3];
        for (colorant, signature) in colorants.iter_mut().zip([b"rXYZ", b"gXYZ", b"bXYZ"]) {
            let data = element(signature)?;
            if data.get(..4) != Some(b"XYZ ") {
                return Err("a colorant is not an XYZ element".to_owned());
            }
            for (index, value) in colorant.iter_mut().enumerate() {
                *value = read_fixed(data, 8 + 4 * index)?;
            }
        }
        if determinant(&colorants).abs() < 1e-6 {
            return Err("its colorants do not span a colour space".to_owned());
        }
        let mut linear = [[0.0; 256]; 3];
        for (channel, signature) in linear.iter_mut().zip([b"rTRC", b"gTRC", b"bTRC"]) {
            let curve = Curve::parse(element(signature)?)?;
            for (sample, value) in channel.iter_mut().enumerate() {
                let light = curve.evaluate(sample as f64 / 255.0);
                if !light.is_finite() {
                    return Err("a tone curve has no value for some sample".to_owned());
                }
                *value = light.clamp(0.0, 1.0);
            }
        }
        Ok(MatrixProfile { linear, colorants })
    }

    /// True when the profile is sRGB: every colorant value within 0.001 of sRGB's, and
    /// every channel's curve within 0.001 of sRGB's at each of the 256 samples.
    pub(super) fn is_srgb(&self) -> bool {
        let colorants = self
            .colorants
            .iter()
            .flatten()
            .zip(SRGB_COLORANTS.iter().flatten())
            .all(|(got, want)| (got - want).abs() <= SRGB_TOLERANCE);
        let curves = self.linear.iter().all(|channel| {
            channel.iter().enumerate().all(|(sample, light)| {
                (light - srgb_decode(sample as f64 / 255.0)).abs() <= SRGB_TOLERANCE
            })
        });
        colorants && curves
    }

    /// Converts RGBA pixels to sRGB in place. Alpha is left as it is.
    pub(super) fn convert(&self, rgba: &mut [u8]) {
        // Columns are colorants: to XYZ, then out through sRGB's inverse.
        let to_xyz = columns(&self.colorants);
        let from_xyz = invert(&columns(&SRGB_COLORANTS));
        let matrix = multiply(&from_xyz, &to_xyz);
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let light = [
                self.linear[0][usize::from(pixel[0])],
                self.linear[1][usize::from(pixel[1])],
                self.linear[2][usize::from(pixel[2])],
            ];
            for (channel, row) in matrix.iter().enumerate() {
                let value = row[0] * light[0] + row[1] * light[1] + row[2] * light[2];
                let encoded = srgb_encode(value.clamp(0.0, 1.0));
                pixel[channel] = (encoded * 255.0 + 0.5).floor().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// A tone curve as a profile stores it.
enum Curve {
    /// Output equals input.
    Identity,
    /// A plain power.
    Power(f64),
    /// Samples of the curve at equal steps from 0 to 1, each a fraction of 65535, joined by
    /// straight lines.
    Table(Vec<u16>),
    /// One of the five parametric forms, with its parameters `g a b c d e f`.
    Parametric(u16, [f64; 7]),
}

impl Curve {
    fn parse(data: &[u8]) -> Result<Curve, String> {
        match data.get(..4) {
            Some(b"curv") => {
                let count = read_u32(data, 8)? as usize;
                if count > MAX_CURVE_ENTRIES {
                    return Err(format!("a tone curve has {count} entries"));
                }
                let entries = data
                    .get(12..12 + 2 * count)
                    .ok_or("a tone curve is cut short")?
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_be_bytes(*pair))
                    .collect::<Vec<u16>>();
                Ok(match entries.as_slice() {
                    [] => Curve::Identity,
                    // One entry is a power, with eight fractional bits.
                    [power] => Curve::Power(f64::from(*power) / 256.0),
                    _ => Curve::Table(entries),
                })
            }
            Some(b"para") => {
                let kind = u16::from_be_bytes([
                    *data.get(8).ok_or("a tone curve is cut short")?,
                    *data.get(9).ok_or("a tone curve is cut short")?,
                ]);
                let count = match kind {
                    0 => 1,
                    1 => 3,
                    2 => 4,
                    3 => 5,
                    4 => 7,
                    other => return Err(format!("parametric curve type {other} is unknown")),
                };
                let mut parameters = [0.0; 7];
                for (index, parameter) in parameters.iter_mut().take(count).enumerate() {
                    *parameter = read_fixed(data, 12 + 4 * index)?;
                }
                if matches!(kind, 1 | 2) && parameters[1] == 0.0 {
                    return Err("a parametric curve divides by zero".to_owned());
                }
                Ok(Curve::Parametric(kind, parameters))
            }
            _ => Err("a tone curve is neither a table nor parametric".to_owned()),
        }
    }

    /// The curve's value at `x`, between 0 and 1. The parametric forms are those of the ICC
    /// specification.
    fn evaluate(&self, x: f64) -> f64 {
        match self {
            Curve::Identity => x,
            Curve::Power(power) => libm::pow(x, *power),
            Curve::Table(entries) => {
                let position = x * (entries.len() - 1) as f64;
                let below = (position.floor() as usize).min(entries.len() - 2);
                let part = position - below as f64;
                let (low, high) = (f64::from(entries[below]), f64::from(entries[below + 1]));
                (low + (high - low) * part) / 65535.0
            }
            Curve::Parametric(kind, [g, a, b, c, d, e, f]) => match kind {
                0 => libm::pow(x, *g),
                1 if x >= -b / a => libm::pow(a * x + b, *g),
                1 => 0.0,
                2 if x >= -b / a => libm::pow(a * x + b, *g) + c,
                2 => *c,
                3 if x >= *d => libm::pow(a * x + b, *g),
                3 => c * x,
                _ if x >= *d => libm::pow(a * x + b, *g) + e,
                _ => c * x + f,
            },
        }
    }
}

fn read_u32(data: &[u8], at: usize) -> Result<u32, String> {
    let bytes = at
        .checked_add(4)
        .and_then(|end| data.get(at..end))
        .ok_or("it is cut short")?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// A signed number with 16 fractional bits.
fn read_fixed(data: &[u8], at: usize) -> Result<f64, String> {
    Ok(f64::from(read_u32(data, at)? as i32) / 65536.0)
}

type Matrix = [[f64; 3]; 3];

/// The matrix whose columns are these three vectors.
fn columns(vectors: &[[f64; 3]; 3]) -> Matrix {
    let mut out = [[0.0; 3]; 3];
    for (column, vector) in vectors.iter().enumerate() {
        for (row, value) in vector.iter().enumerate() {
            out[row][column] = *value;
        }
    }
    out
}

fn determinant(m: &Matrix) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// The inverse, by the adjugate. Only sRGB's own matrix is inverted, and it has one.
fn invert(m: &Matrix) -> Matrix {
    let det = determinant(m);
    let cofactor = |r0: usize, r1: usize, c0: usize, c1: usize| {
        (m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0]) / det
    };
    [
        [
            cofactor(1, 2, 1, 2),
            -cofactor(0, 2, 1, 2),
            cofactor(0, 1, 1, 2),
        ],
        [
            -cofactor(1, 2, 0, 2),
            cofactor(0, 2, 0, 2),
            -cofactor(0, 1, 0, 2),
        ],
        [
            cofactor(1, 2, 0, 1),
            -cofactor(0, 2, 0, 1),
            cofactor(0, 1, 0, 1),
        ],
    ]
}

fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    let mut out = [[0.0; 3]; 3];
    for (row, out_row) in out.iter_mut().enumerate() {
        for (column, value) in out_row.iter_mut().enumerate() {
            *value = (0..3).map(|k| a[row][k] * b[k][column]).sum();
        }
    }
    out
}
