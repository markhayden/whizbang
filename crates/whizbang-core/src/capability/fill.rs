//! Fill (`SPEC.md` section 4; ADR 0006, "Fill"): an image, one support plane and an
//! optional prompt go out; pixels of the same dimensions come back.
//!
//! A filler is not trusted to leave pixels alone. What it returns is only ever read
//! inside the support, by the composite, which keeps every other pixel itself.

use std::sync::Arc;

use crate::artifact::Mask;
use crate::contract::Cancellation;

use super::{Declaration, Fault, FaultKind, Picture, Unfit, feature, pixel_count};

/// The most bytes a prompt holds.
pub const PROMPT_LIMIT: usize = 4096;

/// The support plane: the area a filler is to paint. One byte a pixel, row by row from
/// the top left, the size of the image. A byte is [`Support::PAINT`] or
/// [`Support::KEEP`], and nothing else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Support {
    width: u32,
    height: u32,
    plane: Vec<u8>,
}

impl Support {
    /// Paint here.
    pub const PAINT: u8 = 255;
    /// Keep this pixel: whatever is painted here is thrown away.
    pub const KEEP: u8 = 0;

    /// A support plane of these bytes. Refused: a size of no pixels, or over
    /// [`PIXEL_LIMIT`](super::PIXEL_LIMIT), before a byte is read; another count of
    /// bytes than the size has pixels; and a byte that is neither 0 nor 255.
    pub fn from_plane(width: u32, height: u32, plane: Vec<u8>) -> Result<Support, Unfit> {
        let what = "the support plane";
        let expected = pixel_count(what, width, height)?;
        if plane.len() as u64 != expected {
            return Err(Unfit::WrongLength {
                what,
                expected,
                found: plane.len() as u64,
            });
        }
        if let Some(at) = plane
            .iter()
            .position(|sample| *sample != Support::PAINT && *sample != Support::KEEP)
        {
            return Err(Unfit::OutOfRange {
                what,
                at: Some(at as u64),
                found: plane[at].to_string(),
                allowed: "0 or 255",
            });
        }
        Ok(Support {
            width,
            height,
            plane,
        })
    }

    /// The support of a mask: 255 wherever the mask's effective weight is over 0, and 0
    /// elsewhere (ADR 0002's rule for the weight). So it is 255 in the core, 255 where
    /// the blend is 1 or more, and 0 under protect and where the mask holds nothing.
    pub fn of(mask: &Mask) -> Support {
        let space = mask.space();
        let plane = mask
            .core()
            .iter()
            .zip(mask.blend())
            .zip(mask.protect())
            .map(|((core, blend), protect)| {
                if *protect == 0 && (*core != 0 || *blend != 0) {
                    Support::PAINT
                } else {
                    Support::KEEP
                }
            })
            .collect();
        Support {
            width: space.width(),
            height: space.height(),
            plane,
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// One byte a pixel, row by row from the top left: 255 to paint, 0 to keep.
    pub fn plane(&self) -> &[u8] {
        &self.plane
    }

    /// How many pixels are to be painted.
    pub fn painted(&self) -> u64 {
        self.plane
            .iter()
            .filter(|sample| **sample == Support::PAINT)
            .count() as u64
    }
}

/// What a filler is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FillRequest {
    image: Picture,
    support: Support,
    prompt: Option<String>,
}

impl FillRequest {
    /// A request to paint the support of this image. Refused: a support plane of
    /// another size than the image; a support with nothing to paint; a prompt of no
    /// bytes, or of more than [`PROMPT_LIMIT`].
    pub fn new(
        image: Picture,
        support: Support,
        prompt: Option<&str>,
    ) -> Result<FillRequest, Unfit> {
        let size = (image.width(), image.height());
        if (support.width, support.height) != size {
            return Err(Unfit::WrongSize {
                what: "the support plane",
                expected: size,
                found: (support.width, support.height),
            });
        }
        if !support.plane.contains(&Support::PAINT) {
            return Err(Unfit::Empty {
                what: "the area to paint",
            });
        }
        if let Some(prompt) = prompt {
            if prompt.is_empty() {
                return Err(Unfit::Empty { what: "the prompt" });
            }
            if prompt.len() > PROMPT_LIMIT {
                return Err(Unfit::TooLarge {
                    what: "the prompt, in bytes,",
                    size: prompt.len() as u64,
                    limit: PROMPT_LIMIT as u64,
                });
            }
        }
        Ok(FillRequest {
            image,
            support,
            prompt: prompt.map(str::to_owned),
        })
    }

    /// The image: canonical pixels, with their alpha.
    pub fn image(&self) -> &Picture {
        &self.image
    }

    /// The area to paint.
    pub fn support(&self) -> &Support {
        &self.support
    }

    /// What to paint, in words, when the call says.
    pub fn prompt(&self) -> Option<&str> {
        self.prompt.as_deref()
    }
}

/// The pixels a filler returned: three bytes a pixel, red, green and blue, sRGB, eight
/// bits a channel, row by row from the top left. There is no alpha: a filler paints
/// colour, and the composite keeps the alpha of the image it paints onto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Painted {
    width: u32,
    height: u32,
    rgb: Vec<u8>,
}

impl Painted {
    /// A fill of these bytes. Refused: a size of no pixels, or over
    /// [`PIXEL_LIMIT`](super::PIXEL_LIMIT), before a byte is read; and any count of
    /// bytes that is not three for each pixel, which is how four channels, or one, are
    /// refused.
    pub fn from_rgb(width: u32, height: u32, rgb: Vec<u8>) -> Result<Painted, Unfit> {
        let what = "the fill";
        let expected = pixel_count(what, width, height)? * 3;
        if rgb.len() as u64 != expected {
            return Err(Unfit::WrongLength {
                what,
                expected,
                found: rgb.len() as u64,
            });
        }
        Ok(Painted { width, height, rgb })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Red, green and blue for each pixel, row by row from the top left.
    pub fn rgb(&self) -> &[u8] {
        &self.rgb
    }
}

/// A backend that fills. It answers one request at a time and returns when it has its
/// answer; it is expected to stop early when the cancellation is set.
pub trait Filler: Send + Sync {
    /// The model that answers, and the features it supports.
    fn declaration(&self) -> &Declaration;

    /// Paints the request's support. The pixels it returns outside the support are
    /// never used.
    fn fill(&self, request: &FillRequest, cancellation: &Cancellation) -> Result<Painted, Fault>;
}

/// The handle through which an action asks a filler.
#[derive(Clone)]
pub struct FillHandle {
    backend: Arc<dyn Filler>,
}

impl FillHandle {
    pub fn new(backend: Arc<dyn Filler>) -> FillHandle {
        FillHandle { backend }
    }

    /// What the filler behind the handle declares.
    pub fn declaration(&self) -> &Declaration {
        self.backend.declaration()
    }

    /// Sends the request and hands back the fill. Nothing is sent once the run is
    /// cancelled, and a prompt is not sent to a filler that declares none. A fill of
    /// other dimensions than the request's image is a malformed answer.
    pub fn fill(
        &self,
        request: &FillRequest,
        cancellation: &Cancellation,
    ) -> Result<Painted, Fault> {
        before_sending(
            self.declaration(),
            cancellation,
            request.prompt.as_ref().map(|_| feature::PROMPT),
        )?;
        let painted = self.backend.fill(request, cancellation)?;
        let (expected, found) = (
            (request.image.width(), request.image.height()),
            (painted.width, painted.height),
        );
        if found != expected {
            return Err(Unfit::WrongSize {
                what: "the fill",
                expected,
                found,
            }
            .into());
        }
        Ok(painted)
    }
}

impl std::fmt::Debug for FillHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FillHandle({:?})", self.declaration())
    }
}

/// What every handle checks before it sends: the run is not cancelled, and the backend
/// declares each feature the request needs.
pub(super) fn before_sending<'a>(
    declaration: &Declaration,
    cancellation: &Cancellation,
    needed: impl IntoIterator<Item = &'a str>,
) -> Result<(), Fault> {
    if cancellation.is_cancelled() {
        return Err(Fault::new(
            FaultKind::Cancelled,
            "the run was cancelled before the request was sent",
        ));
    }
    for feature in needed {
        if !declaration.has(feature) {
            return Err(Fault::new(
                FaultKind::Unsupported,
                &format!(
                    "the backend {} does not declare the feature {feature}",
                    declaration.model().model()
                ),
            ));
        }
    }
    Ok(())
}
