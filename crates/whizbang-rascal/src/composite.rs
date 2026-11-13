//! The composite: a fill pasted onto an image under a mask (ADR 0006, "The composite").
//!
//! A filler is not trusted to leave pixels alone (`SPEC.md` section 4). So what it
//! returned is read only where the mask's effective weight is over 0, and every other
//! pixel of the result is a copy of the image's four bytes: every pixel outside the
//! support, and every protected pixel, is exactly the original. The result is then
//! checked against the image and the mask once more, by the core, which is how the
//! derivation it returns is made.
//!
//! The arithmetic is rascal's `feather` paste (`crates/rascal-ops/src/paste.rs`), in
//! whole numbers.

use whizbang_core::artifact::{ArtifactError, Derivation, Mask};
use whizbang_core::capability::{Painted, Picture};
use whizbang_core::identity::CanonicalPixels;

/// One channel of one pixel under a blend: the nearest whole number to
/// `((255 - blend) original + blend fill) / 255`.
///
/// It is computed as `((255 - blend) * original + blend * fill + 127) / 255` in whole
/// numbers, the division dropping its remainder. No exact mix is half way between two
/// whole numbers, because 255 is odd, so "nearest" needs no rule for a tie. A blend of 0
/// gives the original and a blend of 255 the fill.
pub fn mix(original: u8, fill: u8, blend: u8) -> u8 {
    let (original, fill, blend) = (u32::from(original), u32::from(fill), u32::from(blend));
    // At most 255 * 255 + 127, and the quotient at most 255.
    (((255 - blend) * original + blend * fill + 127) / 255) as u8
}

/// Why a composite was not made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompositeError {
    /// The mask does not belong to the image.
    WrongImage(String),
    /// The fill is not the size of the image.
    FillSize {
        expected: (u32, u32),
        found: (u32, u32),
    },
    /// The result did not pass the check that every pixel the mask weighs nothing is
    /// the original. The composite copies those pixels, so this is a fault of this
    /// code and never of a filler.
    NotPreserved(String),
}

impl std::fmt::Display for CompositeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompositeError::WrongImage(why) => write!(f, "{why}"),
            CompositeError::FillSize { expected, found } => write!(
                f,
                "the fill is {} by {}, and the image is {} by {}",
                found.0, found.1, expected.0, expected.1
            ),
            CompositeError::NotPreserved(why) => {
                write!(f, "the composite did not keep what it must: {why}")
            }
        }
    }
}

impl std::error::Error for CompositeError {}

/// What a composite made.
#[derive(Debug, Clone, PartialEq)]
pub struct Composite {
    pixels: CanonicalPixels,
    derivation: Option<Derivation>,
    painted: u64,
}

impl Composite {
    /// The picture: canonical pixels, with the alpha of the image it was made from.
    pub fn pixels(&self) -> &CanonicalPixels {
        &self.pixels
    }

    /// The statement that the picture was derived from the image, pixel for pixel in
    /// place: what a mask on the image is rebound through to be used on the picture.
    /// There is none when the picture is the image, unchanged: nothing was derived, and
    /// a mask on the image is already a mask on the picture.
    pub fn derivation(&self) -> Option<&Derivation> {
        self.derivation.as_ref()
    }

    /// How many pixels were read from the fill: those the mask weighs over 0.
    pub fn painted(&self) -> u64 {
        self.painted
    }
}

/// Pastes `fill` onto `image` under `mask`, which belongs to the image.
///
/// For each pixel, by the mask's three planes:
///
/// - protected: the image's four bytes;
/// - else in the core: the fill's red, green and blue, and the image's alpha;
/// - else with a blend of 1 to 255: each of red, green and blue is [`mix`] of the
///   image's and the fill's, and the alpha is the image's;
/// - else: the image's four bytes.
///
/// Colours are mixed as they are stored, sRGB values of eight bits, straight and not
/// multiplied by alpha. A pixel's alpha is never changed, whatever it is.
pub fn composite(
    image: &Picture,
    fill: &Painted,
    mask: &Mask,
) -> Result<Composite, CompositeError> {
    let space = mask.space();
    mask.require_image(image.digest(), space)
        .map_err(|error| CompositeError::WrongImage(reason(error)))?;
    let size = (image.width(), image.height());
    if (space.width(), space.height()) != size {
        return Err(CompositeError::WrongImage(format!(
            "the image is {} by {}, and the mask is {} by {}",
            size.0,
            size.1,
            space.width(),
            space.height()
        )));
    }
    if (fill.width(), fill.height()) != size {
        return Err(CompositeError::FillSize {
            expected: size,
            found: (fill.width(), fill.height()),
        });
    }

    let mut rgba = image.rgba().to_vec();
    let mut painted = 0;
    let planes = mask.core().iter().zip(mask.blend()).zip(mask.protect());
    let pixels = rgba.chunks_mut(4).zip(fill.rgb().chunks(3));
    for ((pixel, fill), ((core, blend), protect)) in pixels.zip(planes) {
        let blend = match (*protect, *core, *blend) {
            (0, 0, 0) => continue,
            (0, 0, blend) => blend,
            (0, _, _) => 255,
            _ => continue,
        };
        for (channel, fill) in pixel[..3].iter_mut().zip(fill) {
            *channel = mix(*channel, *fill, blend);
        }
        painted += 1;
    }

    let pixels = CanonicalPixels::from_rgba(size.0, size.1, rgba)
        .map_err(|error| CompositeError::NotPreserved(error.to_string()))?;
    let derivation = if pixels == *image.pixels() {
        None
    } else {
        // The core looks at both pictures: every pixel the mask weighs nothing is the
        // same in both, or there is no derivation and no composite.
        let checked = Derivation::checked(image.pixels(), &pixels, mask)
            .map_err(|error| CompositeError::NotPreserved(reason(error)))?;
        Some(checked)
    };
    Ok(Composite {
        pixels,
        derivation,
        painted,
    })
}

/// The sentence of an artifact's error, without the kind in front of it.
fn reason(error: ArtifactError) -> String {
    match error {
        ArtifactError::WrongImage(why) | ArtifactError::NotDerived(why) => why,
        other => other.to_string(),
    }
}
