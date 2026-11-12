//! An image as an artifact: a picture file, held whole, with the space it is in and what it
//! is for (ADR 0002). A rendering of a mask is an image, and never a mask.

use crate::identity::{
    Binding, CanonicalPixels, DecodeLimits, Fields, ImageIdentity, Projection, Space, Topology,
    identify,
};

use super::wire::{Reader, Writer};
use super::{ArtifactError, ArtifactId, ArtifactType, Origin, Record, Result, identity_of};

/// What an image is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageRole {
    /// A picture: a photograph, a composite, a crop.
    Picture,
    /// A picture of another artifact, made for a person or a model to look at: a mask as
    /// greyscale, a mask tinted over its image. It is never accepted where the artifact it
    /// pictures is required.
    Rendering,
}

/// A picture file and what whizbang knows of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    file: Vec<u8>,
    identity: ImageIdentity,
    space: Space,
    role: ImageRole,
    origin: Origin,
    id: ArtifactId,
}

impl Image {
    /// An image from the bytes of its file, which is decoded here, within the limits, to
    /// find its two digests and its size (ADR 0001). The projection and the topology are
    /// stated, because no file can be trusted to say them.
    pub fn new(
        file: Vec<u8>,
        projection: Projection,
        topology: Topology,
        role: ImageRole,
        origin: Origin,
        limits: &DecodeLimits,
    ) -> Result<Image> {
        let identified = identify(&file, limits)?;
        let space = Space::new(
            identified.pixels.width(),
            identified.pixels.height(),
            projection,
            topology,
        )?;
        let mut image = Image {
            file,
            identity: identified.identity,
            space,
            role,
            origin,
            id: ArtifactId::unset(),
        };
        image.id = identity_of(&image);
        Ok(image)
    }

    pub fn id(&self) -> ArtifactId {
        self.id
    }

    /// The bytes of the picture file, exactly as it was given.
    pub fn file(&self) -> &[u8] {
        &self.file
    }

    /// The two digests of ADR 0001: of the file, and of its canonical pixels.
    pub fn identity(&self) -> ImageIdentity {
        self.identity
    }

    /// The space of the upright picture.
    pub fn space(&self) -> Space {
        self.space
    }

    pub fn role(&self) -> ImageRole {
        self.role
    }

    /// The image in its own space: the binding a mask or a location made on it belongs to.
    pub fn binding(&self) -> Binding {
        Binding::root(self.identity.pixels, self.space)
    }

    /// The canonical pixels, decoded again from the file.
    pub fn pixels(&self, limits: &DecodeLimits) -> Result<CanonicalPixels> {
        Ok(identify(&self.file, limits)?.pixels)
    }

    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// The image's canonical file.
    pub fn to_bytes(&self) -> Vec<u8> {
        super::file_of(self)
    }

    /// Reads an image's canonical file. A picture file alone is not one.
    pub fn from_bytes(bytes: &[u8], limits: &DecodeLimits) -> Result<Image> {
        super::parse(bytes, limits)
    }
}

impl Record for Image {
    const TYPE: ArtifactType = ArtifactType::Image;

    fn write(&self, writer: &mut Writer) {
        writer.held_file(&self.file);
        writer.digest(&self.identity.pixels.0);
        self.space.encode(writer);
        writer.unsigned(match self.role {
            ImageRole::Picture => 0,
            ImageRole::Rendering => 1,
        });
        writer.origin(&self.origin);
    }

    fn read(reader: &mut Reader<'_>, limits: &DecodeLimits) -> Result<Image> {
        let file = reader.held_file()?.to_vec();
        let pixels = reader.digest()?;
        let space = reader.space()?;
        let role = match reader.unsigned()? {
            0 => ImageRole::Picture,
            1 => ImageRole::Rendering,
            other => {
                return Err(ArtifactError::Format(format!(
                    "{other} is not what an image is for"
                )));
            }
        };
        let origin = reader.origin()?;
        let image = Image::new(
            file,
            space.projection(),
            space.topology(),
            role,
            origin,
            limits,
        )?;
        // What the record says of the picture must be what the picture is.
        if image.identity.pixels.0 != pixels || image.space != space {
            return Err(ArtifactError::Format(
                "the record does not describe the picture it holds".to_owned(),
            ));
        }
        Ok(image)
    }
}
