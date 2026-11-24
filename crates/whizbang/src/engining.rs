//! What `whizbang serve --http` offers through the engine API (`whizbang_serve::engine`;
//! plan m6, task H1; ADR 0045): the stdio server's door, unchanged ([`Actions`]), and what
//! the store holds for the plain routes: an upload imported as `whizbang store import`
//! imports a picture or a mask, an artifact's bytes, a run's record, and what a request
//! id's admission names. Everything is written through the store's one writer, the one the
//! executor holds, let go when the engine stops.

use std::sync::{Arc, Mutex};

use image::{ExtendedColorType, ImageEncoder};
use serde_json::{Map, Value as Json, json};
use whizbang_core::artifact::{self, Artifact, ArtifactId, ArtifactType};
use whizbang_core::contract::record::RECORD_NAME;
use whizbang_core::contract::{Call, JournalLine, Outcome, RunId};
use whizbang_core::identity::{DecodeLimits, Digest, IdentityError, Projection, identify};
use whizbang_core::store::Store;
use whizbang_serve::engine::{Hosted, Refused};
use whizbang_serve::{Door, Said, Tool};

use crate::import;
use crate::serving::Actions;

/// The engine API's door: the actions as tools, over the store the executor writes.
pub struct Served {
    actions: Actions,
    store: Mutex<Option<Arc<Store>>>,
}

impl Served {
    /// The actions as the stdio server offers them, and the store's writer the executor
    /// holds, through which uploads are imported.
    pub fn new(actions: Actions, store: Arc<Store>) -> Served {
        Served {
            actions,
            store: Mutex::new(Some(store)),
        }
    }

    fn store(&self) -> Result<Arc<Store>, Refused> {
        self.store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .ok_or_else(|| Refused::new(503, "the engine is stopping"))
    }

    /// The image of this identity, as the store holds it.
    fn image(&self, id: &str) -> Result<artifact::Image, Refused> {
        let not = || Refused::new(404, "the store holds no image of that identity");
        let id = id.parse::<Digest>().map(ArtifactId).map_err(|_| not())?;
        let store = self.store()?;
        let read = artifact::get(
            store.as_ref(),
            ArtifactType::Image,
            id,
            &DecodeLimits::default(),
        );
        match read {
            Ok(Artifact::Image(image)) => Ok(image),
            _ => Err(not()),
        }
    }
}

fn png_of(width: u32, height: u32, bytes: &[u8]) -> Result<Vec<u8>, Refused> {
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(bytes, width, height, ExtendedColorType::L8)
        .map_err(|_| Refused::new(500, "cannot draw the mask"))?;
    Ok(png)
}

fn kept(
    store: &Store,
    artifact: &Artifact,
    from_file: Option<(&str, Digest)>,
) -> Result<(), Refused> {
    import::commit_to(store, artifact, from_file, &[])
        .map(|_| ())
        .map_err(|_| Refused::new(500, "the store could not keep the upload"))
}

impl Door for Served {
    fn tools(&self) -> Vec<Tool> {
        self.actions.tools()
    }

    fn call(
        &self,
        name: &str,
        arguments: &Map<String, Json>,
    ) -> Result<(Call, Option<String>), String> {
        self.actions.call(name, arguments)
    }

    fn said(&self, name: &str, run: &RunId, outcome: &Outcome) -> Said {
        self.actions.said(name, run, outcome)
    }

    fn journal(&self, run: &RunId) -> Vec<JournalLine> {
        self.actions.journal(run)
    }

    fn closing(&self) {
        // The store's writer goes with the executor's: the engine has let that go.
        self.store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        self.actions.closing();
    }
}

impl Hosted for Served {
    fn upload(&self, bytes: &[u8], pixel_limit: u64) -> Result<Json, Refused> {
        let store = self.store()?;
        let limits = DecodeLimits {
            max_pixels: pixel_limit.min(DecodeLimits::default().max_pixels),
            ..DecodeLimits::default()
        };
        // Decoded once, within the limits: what judges the bytes is what is kept, its
        // canonical pixels as a PNG and nothing of the file's metadata.
        let identified = identify(bytes, &limits).map_err(|error| match error {
            IdentityError::Limit(why) => Refused::new(
                413,
                &format!(
                    "the picture is over the decode limits, of at most {pixel_limit} pixels: {why}"
                ),
            ),
            _ => Refused::new(400, "the body is not a picture this build reads"),
        })?;
        let image = import::image_of_identified(&identified, &limits, Projection::Flat)
            .map_err(|_| Refused::new(400, "the body is not a picture this build reads"))?;
        let id = image.id();
        let file = Digest::of_bytes(bytes);
        kept(
            &store,
            &Artifact::Image(image),
            Some((import::IMPORTED, file)),
        )?;
        Ok(json!({ "image": id.to_string() }))
    }

    fn mask(&self, bytes: &[u8], image_id: &str, pixel_limit: u64) -> Result<Json, Refused> {
        let image = self.image(image_id)?;
        let onto = image.id().0;
        let space = image.binding().source_space();
        let (width, height) = (space.width(), space.height());
        let pixels = u64::from(width) * u64::from(height);
        if pixels > pixel_limit {
            return Err(Refused::new(
                413,
                &format!("the mask is {pixels} pixels, over the limit of {pixel_limit}"),
            ));
        }
        let limits = DecodeLimits {
            max_pixels: pixels,
            ..DecodeLimits::default()
        };
        let identified = identify(bytes, &limits)
            .map_err(|_| Refused::new(400, "the body is not a grey PNG of its image's size"))?;
        let mask = import::mask_of(
            &identified.pixels,
            (width, height),
            &image,
            onto,
            Digest::of_bytes(bytes),
        )
        .map_err(|why| Refused::new(400, &why))?;
        let mask = Artifact::from(mask);
        let id = mask.id();
        kept(self.store()?.as_ref(), &mask, None)?;
        Ok(json!({ "mask": id.to_string() }))
    }

    fn image_size(&self, image: &str) -> Option<(u32, u32)> {
        let image = self.image(image).ok()?;
        let space = image.binding().source_space();
        Some((space.width(), space.height()))
    }

    fn artifact(&self, id: &str) -> Result<(&'static str, Vec<u8>), Refused> {
        let not = || Refused::new(404, "the store holds no artifact of that identity");
        let id = id.parse::<Digest>().map(ArtifactId).map_err(|_| not())?;
        let store = self.store()?;
        let generation = store
            .snapshot()
            .map_err(|_| Refused::new(500, "the store could not answer"))?;
        let kind = artifact::type_anywhere(&generation, id).ok_or_else(not)?;
        let read = artifact::get_anywhere(store.as_ref(), kind, id, &DecodeLimits::default())
            .map_err(|_| not())?;
        match read {
            Artifact::Image(image) => Ok(("image/png", image.file().to_vec())),
            Artifact::Mask(mask) => {
                // Its grey PNG, as it is uploaded: 255 its core, its blend elsewhere.
                let space = mask.space();
                let grey: Vec<u8> = (mask.core().iter().zip(mask.blend()))
                    .map(|(core, blend)| if *core == 255 { 255 } else { *blend })
                    .collect();
                Ok(("image/png", png_of(space.width(), space.height(), &grey)?))
            }
            _ => Err(Refused::new(415, "this artifact is no picture or mask")),
        }
    }

    fn record(&self, run: &RunId) -> Option<Json> {
        let store = self.store().ok()?;
        let generation = store.snapshot().ok()?;
        let entry = generation.run(run.as_str())?.get(RECORD_NAME)?.clone();
        serde_json::from_slice(&store.read(&entry).ok()?).ok()
    }
}
