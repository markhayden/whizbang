//! A derivation: the statement that one image was made from another, pixel for pixel in
//! place (ADR 0002). It is what a mask is rebound through.
//!
//! A derivation holds a binding and adds nothing to it: no bytes, no identity of its own.
//! It is a type so that a binding an alignment or a crop returned is not taken for one by
//! accident: [`Mask::rebind`](super::Mask::rebind) takes a derivation and nothing else.
//!
//! What it does not do is prove the statement. Nothing here can tell a true derivation
//! from an identity binding between two unrelated images of one size.
//!
//! Who may state one (ADR 0004): code, and no caller. A call names artifacts by identity
//! and has no place for a derivation, so none reaches the executor from a door.
//! [`Derivation::stated`] stays open to code because nothing else makes a derivation yet:
//! the fill composite will, after checking the pixels, and `stated` closes with it
//! (`docs/carry-over.md`).

use crate::identity::{Binding, Destination, Mapping, PixelDigest};

use super::{ArtifactError, Result};

/// The statement that an image was derived from another with every pixel in its place:
/// the composite a fill made of an image, say.
#[derive(Debug, Clone, PartialEq)]
pub struct Derivation {
    binding: Binding,
    parent: PixelDigest,
}

impl Derivation {
    /// States that the binding's source image was derived from its destination image,
    /// pixel for pixel in place. The binding must be able to say so: it places its image
    /// onto another image, and by the identity. Whether it is true is the caller's word,
    /// and the caller is code: no call can ask for this.
    pub fn stated(binding: Binding) -> Result<Derivation> {
        let refuse = |why: &str| Err(ArtifactError::Rebind(why.to_owned()));
        let parent = match binding.destination() {
            Destination::Image(image) => image,
            Destination::Root => {
                return refuse(
                    "the binding places its image onto nothing: it does not show that \
                     image to be derived from another",
                );
            }
        };
        if *binding.mapping() != Mapping::Identity {
            return refuse(
                "a derivation keeps every pixel in its place: the binding's mapping must be \
                 the identity",
            );
        }
        if binding.source() == parent {
            return refuse("the binding places an image onto itself, which derives nothing");
        }
        Ok(Derivation { binding, parent })
    }

    /// The binding that places the derived image onto the image it was made from.
    pub fn binding(&self) -> &Binding {
        &self.binding
    }

    /// The derived image.
    pub fn image(&self) -> PixelDigest {
        self.binding.source()
    }

    /// The image it was derived from.
    pub fn parent(&self) -> PixelDigest {
        self.parent
    }
}
