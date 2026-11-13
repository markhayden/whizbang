//! A derivation: the statement that one image was made from another, pixel for pixel in
//! place (ADR 0002). It is what a mask is rebound through.
//!
//! A derivation holds a binding and adds nothing to it: no bytes, no identity of its own.
//! It is a type so that a binding an alignment or a crop returned is not taken for one by
//! accident: [`Mask::rebind`](super::Mask::rebind) takes a derivation and nothing else.
//!
//! One is made in one way: by looking at both pictures ([`Derivation::checked`]; ADR
//! 0006, "Who vouches for a derivation"). The new picture is the size of the one it was
//! made from, and every pixel that a mask on that image weighs nothing is the same in
//! both. So the statement cannot be made of two unrelated pictures of one size, unless
//! the mask it was checked against weighs every pixel: a mask whose support is the whole
//! image leaves no pixel to compare, and vouches for any picture of its size. Whoever
//! holds a derivation holds one that was checked, against a mask that someone made.
//!
//! What that does not do is make a rebound mask a kind of its own. A mask made by hand
//! on a derivation's binding is the same artifact as the rebound one (ADR 0004).

use crate::capability::Picture;
use crate::identity::{Binding, CanonicalPixels, Destination, Direction, Mapping, PixelDigest};

use super::{ArtifactError, Mask, Result};

/// The statement that an image was derived from another with every pixel in its place:
/// the composite a fill made of an image, say.
#[derive(Debug, Clone, PartialEq)]
pub struct Derivation {
    binding: Binding,
    parent: PixelDigest,
}

impl Derivation {
    /// The derivation of `derived` from `parent`, checked against a mask on `parent`:
    /// the two pictures are one size, and wherever the mask's effective weight is 0,
    /// outside its core and its blend or under its protect, the four bytes of the pixel
    /// are the same in both. Where the weight is over 0 the pictures may differ in any
    /// way.
    ///
    /// Refused: a mask that does not belong to `parent`; a picture of another size; the
    /// same picture, which is derived from nothing; and any pixel changed where the mask
    /// weighs nothing, with how many there are and where the first is.
    ///
    /// The derivation is about the two pictures. Any mask on `parent`, in the space of
    /// the mask it was checked with, may be rebound through it.
    pub fn checked(
        parent: &CanonicalPixels,
        derived: &CanonicalPixels,
        mask: &Mask,
    ) -> Result<Derivation> {
        Derivation::check(parent, parent.digest(), derived, mask)
    }

    /// As [`Derivation::checked`], of a picture that holds the digest of its pixels: the
    /// digest is the picture's own, made when the picture was, and is not worked out a
    /// second time.
    pub fn checked_of(
        parent: &Picture,
        derived: &CanonicalPixels,
        mask: &Mask,
    ) -> Result<Derivation> {
        Derivation::check(parent.pixels(), parent.digest(), derived, mask)
    }

    /// The check, with the digest of `parent`'s pixels, which both callers have from the
    /// pixels themselves.
    fn check(
        parent: &CanonicalPixels,
        from: PixelDigest,
        derived: &CanonicalPixels,
        mask: &Mask,
    ) -> Result<Derivation> {
        let space = mask.space();
        let to = derived.digest();
        mask.require_image(from, space)?;
        let refuse = |why: String| Err(ArtifactError::NotDerived(why));
        let size = (space.width(), space.height());
        for (what, picture) in [("the image", parent), ("the new picture", derived)] {
            if (picture.width(), picture.height()) != size {
                return refuse(format!(
                    "{what} is {} by {}, and the mask is {} by {}",
                    picture.width(),
                    picture.height(),
                    size.0,
                    size.1
                ));
            }
        }
        if from == to {
            return refuse(
                "it is the same picture as the image it would be derived from".to_owned(),
            );
        }
        let (mut changed, mut first) = (0_u64, None);
        let pixels = parent.rgba().chunks(4).zip(derived.rgba().chunks(4));
        for (at, (before, after)) in pixels.enumerate() {
            let weighs_nothing =
                mask.protect()[at] != 0 || (mask.core()[at] == 0 && mask.blend()[at] == 0);
            if weighs_nothing && before != after {
                changed += 1;
                first.get_or_insert(at);
            }
        }
        if let Some(at) = first {
            let (x, y) = (at % size.0 as usize, at / size.0 as usize);
            let pixels = if changed == 1 {
                "pixel that the mask weighs nothing differs"
            } else {
                "pixels that the mask weighs nothing differ"
            };
            return refuse(format!(
                "{changed} {pixels} from the image it was made from, the first at {x}, {y}"
            ));
        }
        Derivation::stated(Binding::new(
            to,
            space,
            from,
            space,
            Direction::SourceToDestination,
            Mapping::Identity,
        )?)
    }

    /// States that the binding's source image was derived from its destination image,
    /// pixel for pixel in place. The binding must be able to say so: it places its image
    /// onto another image, and by the identity. Whether it is true is not looked at
    /// here, which is why only [`Derivation::checked`] calls this.
    fn stated(binding: Binding) -> Result<Derivation> {
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    //! What a binding must say to be stated as a derivation. Nothing outside this module
    //! can state one, so these rules are held here.

    use super::*;
    use crate::identity::{Digest, Space};

    fn picture(name: &str) -> PixelDigest {
        PixelDigest(Digest::of_bytes(name.as_bytes()))
    }

    fn placed(image: &str, onto: &str, mapping: Mapping) -> Binding {
        let space = Space::flat(8, 6).unwrap();
        Binding::new(
            picture(image),
            space,
            picture(onto),
            space,
            Direction::SourceToDestination,
            mapping,
        )
        .unwrap()
    }

    fn refused(binding: Binding) -> String {
        match Derivation::stated(binding) {
            Err(ArtifactError::Rebind(why)) => why,
            other => panic!("stated as a derivation: {other:?}"),
        }
    }

    #[test]
    fn a_binding_by_the_identity_onto_another_image_is_a_derivation() {
        let binding = placed("composite", "photo", Mapping::Identity);
        let derivation = Derivation::stated(binding.clone()).unwrap();
        assert_eq!(*derivation.binding(), binding);
        assert_eq!(derivation.image(), picture("composite"));
        assert_eq!(derivation.parent(), picture("photo"));
    }

    #[test]
    fn a_binding_that_cannot_say_derived_in_place_is_not_stated_as_one() {
        let space = Space::flat(8, 6).unwrap();
        let alone = Binding::root(picture("composite"), space);
        assert!(refused(alone).contains("onto nothing"));
        let itself = placed("photo", "photo", Mapping::Identity);
        assert!(refused(itself).contains("onto itself"));
        // A crop of the whole and a shift are placements, and not the identity.
        let whole = Mapping::Crop {
            left: 0.0,
            top: 0.0,
            right: 8.0,
            bottom: 6.0,
        };
        assert!(refused(placed("resized", "photo", whole)).contains("identity"));
        let shift = Mapping::Homography {
            matrix: [1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        };
        assert!(refused(placed("shifted", "photo", shift)).contains("identity"));
    }
}
