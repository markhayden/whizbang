//! What the tests of task O1's actions share (plan m3, task O1): a store, an executor
//! over every action the task's recipes call, a picture of a given size, and the one
//! output of a run, or the words a refused call ended with.

use std::sync::Arc;

use whizbang_core::artifact::{self, Artifact, ArtifactId, ArtifactType, Image, Location, Mask};
use whizbang_core::capability::Filler;
use whizbang_core::contract::{Call, Capabilities, Engine, Executor, Outcome, Registry};
use whizbang_core::identity::{CanonicalPixels, DecodeLimits, Digest};
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_rascal::recipes;
use whizbang_testkit::capabilities::roster;
use whizbang_testkit::scratch::Scratch;

use super::paste::image_of;

pub struct Shop {
    _dir: Scratch,
    pub store: Arc<Store>,
    pub engine: Engine,
}

impl Shop {
    /// A store, and an executor over every action the shipped recipes call and the
    /// recipes, filled by `filler` when one is given.
    pub fn new(label: &str, filler: Option<Arc<dyn Filler>>) -> Shop {
        let dir = Scratch::new(label);
        let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap());
        let mut registry = Registry::new();
        for register in [
            whizbang_rascal::line::register,
            whizbang_rascal::rect::register,
            whizbang_rascal::stroke::register,
            whizbang_rascal::rasterize::register,
            whizbang_rascal::shrink::register,
            whizbang_rascal::invert::register,
            whizbang_rascal::segment::register,
            whizbang_rascal::grow::register,
            whizbang_rascal::close::register,
            whizbang_rascal::plug::register,
            whizbang_rascal::subtract::register,
            whizbang_rascal::feather::register,
            whizbang_rascal::paste::register,
            whizbang_rascal::fill::register,
        ] {
            register(&mut registry).unwrap();
        }
        whizbang_rascal::select::register(&mut registry).unwrap();
        for recipe in recipes::shipped(&registry).unwrap() {
            registry.register(Arc::new(recipe)).unwrap();
        }
        let capabilities = match &filler {
            Some(filler) => Capabilities::none().with_fill(filler.clone()),
            None => Capabilities::none(),
        };
        let backends = roster(None, filler.as_deref(), None);
        let engine = Engine::over(
            registry,
            Arc::new(backends),
            store.clone(),
            Digest::of_bytes(b""),
        )
        .with_capabilities(capabilities);
        Shop {
            _dir: dir,
            store,
            engine,
        }
    }

    pub fn put(&self, artifact: Artifact) -> ArtifactId {
        let mut publication = self.store.begin().unwrap();
        let id = artifact::put(&mut publication, &artifact).unwrap();
        publication.publish().unwrap();
        id
    }

    /// The outputs of a run that completed, each by its port.
    pub fn completed(&self, call: &Call) -> Vec<(String, ArtifactId)> {
        let finished = self.engine.submit(call).unwrap();
        let Outcome::Completed { outputs } = finished.outcome() else {
            panic!("{:?}", finished.outcome());
        };
        outputs
            .iter()
            .map(|output| (output.port().to_owned(), output.id()))
            .collect()
    }

    /// The one output of a run that completed.
    pub fn one(&self, call: &Call) -> ArtifactId {
        let outputs = self.completed(call);
        let [(_, id)] = outputs.as_slice() else {
            panic!("{outputs:?}");
        };
        *id
    }

    /// Why a call was refused: before it ran, or by the action, as invalid.
    pub fn refused(&self, call: &Call) -> String {
        match self.engine.submit(call) {
            Err(error) => error.to_string(),
            Ok(finished) => match finished.outcome() {
                Outcome::Invalid { why } => why.clone(),
                other => panic!("{other:?}"),
            },
        }
    }

    pub fn get(&self, artifact: ArtifactType, id: ArtifactId) -> Artifact {
        artifact::get(&*self.store, artifact, id, &DecodeLimits::default()).unwrap()
    }

    pub fn mask(&self, id: ArtifactId) -> Mask {
        self.get(ArtifactType::Mask, id).into_mask().unwrap()
    }

    pub fn location(&self, id: ArtifactId) -> Location {
        match self.get(ArtifactType::Location, id) {
            Artifact::Location(location) => location,
            other => panic!("{other:?}"),
        }
    }

    pub fn image(&self, id: ArtifactId) -> Image {
        match self.get(ArtifactType::Image, id) {
            Artifact::Image(image) => image,
            other => panic!("{other:?}"),
        }
    }
}

/// A picture `width` by `height`, every pixel its own colour.
pub fn picture(width: u32, height: u32) -> Image {
    let rgba = (0..width * height)
        .flat_map(|at| {
            let (x, y) = (at % width, at / width);
            [(x * 3) as u8, (y * 5) as u8, 90, 255]
        })
        .collect();
    image_of(
        &CanonicalPixels::from_rgba(width, height, rgba).unwrap(),
        "import",
    )
}

/// How many bytes of a plane are not 0.
pub fn count(plane: &[u8]) -> usize {
    plane.iter().filter(|sample| **sample != 0).count()
}
