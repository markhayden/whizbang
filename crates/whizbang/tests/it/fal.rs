//! `fal::…`: fal's filler through the binary's wiring (plan m3, task F1), as
//! `anthropic::…` does for a judge: a config that names the kind `service` with the
//! provider `fal` under `[backends.fill]`, its key in the env file, the action `fill` as
//! the build ships it, and a server on loopback with a certificate of a root made in the
//! test. The shipped kind's address is never asked; every key here is made up.

use std::path::Path;
use std::sync::Arc;

use whizbang::wiring::{self, Over};
use whizbang_backends::fal::KEY_HEADER;
use whizbang_backends::net::testing::Pki;
use whizbang_backends::service::ServiceKind;
use whizbang_core::artifact::{
    self, Artifact, ArtifactId, ArtifactType, Mask, Origin, Source, Step, UseCase,
};
use whizbang_core::config::Kinds;
use whizbang_core::contract::{Call, Engine, Executor, Finished};
use whizbang_core::identity::{DecodeLimits, Digest};
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;

use crate::anthropic::{KEY_VARIABLE, Log, import_picture, made_up_key, serving};
use crate::wired::{World, whizbang};

/// The classes a fill sends: the picture and its mask.
pub const FILL_CLASSES: [&str; 2] = ["source_images", "derived_images"];

/// A world whose config names fal's filler at an address of the test's, its key in the
/// env file, with an allowance to spend, a picture of 64 by 48 and a mask on it.
pub struct Filled {
    pub world: World,
    pub key: String,
    pub pki: Arc<Pki>,
    pub image: ArtifactId,
    pub mask: ArtifactId,
}

impl Filled {
    pub fn new(label: &str) -> Filled {
        let world = World::new(label);
        let reset = whizbang(&["spend", "reset", "--allowance", "1"], &world.root);
        assert_eq!(reset.code, Some(0), "{}", reset.err);
        let pictures = world.beside("pictures");
        std::fs::create_dir_all(&pictures).unwrap();
        let image: Digest = import_picture(&world.root, &pictures, "base", 40)
            .parse()
            .unwrap();
        let image = ArtifactId(image);
        let mask = put_mask(&world.root, image);
        Filled {
            world,
            key: made_up_key(),
            pki: Arc::new(Pki::loopback()),
            image,
            mask,
        }
    }

    /// The config, written, with fal's filler and a cap, approving `approved` for the
    /// classes of a fill when it names one; the env file; the kind at `address`.
    pub fn over(&self, address: &str, approved: Option<&str>) -> Over {
        let mut config = format!(
            "[backends.fill]\nkind = \"service\"\nprovider = \"fal\"\n\
             model = \"bria-eraser\"\nkey_env = \"{KEY_VARIABLE}\"\n\n[spend]\ncall_cap_usd = 0.10\n"
        );
        if let Some(origin) = approved {
            let classes: Vec<String> = FILL_CLASSES.iter().map(|c| format!("\"{c}\"")).collect();
            config.push_str(&format!(
                "\n[egress.recipients.fal]\norigin = \"{origin}\"\nclasses = [{}]\n",
                classes.join(", ")
            ));
        }
        std::fs::write(self.world.config_path(), config).unwrap();
        self.over_written(address)
    }

    /// The same over the config as it is written now.
    pub fn over_written(&self, address: &str) -> Over {
        std::fs::write(
            self.world.env_path(),
            format!("{KEY_VARIABLE}={}\n", self.key),
        )
        .unwrap();
        let mut kinds = Kinds::new();
        kinds.register(Arc::new(ServiceKind::at(address))).unwrap();
        Over::nothing()
            .config(Some(&self.world.config_path()))
            .env_file(Some(&self.world.env_path()))
            .kinds(kinds)
            .trusting(self.pki.clone())
    }

    /// The binary's runner over the world's store, with the actions the build ships.
    pub fn runner(&self, over: &Over) -> Engine {
        let store = Arc::new(Store::open(&self.world.root, Arc::new(RealFs)).unwrap());
        wiring::runner(wiring::registry().unwrap(), store, over).expect("the executor builds")
    }

    /// One `fill` of the picture and its mask.
    pub fn fill(&self, executor: &Engine) -> Finished {
        let call = Call::new("fill")
            .input("image", self.image)
            .input("mask", self.mask);
        executor.submit(&call).unwrap()
    }

    /// Whether any file of the store holds the key.
    pub fn store_holds_the_key(&self) -> bool {
        fn walk(dir: &Path, key: &[u8]) -> bool {
            std::fs::read_dir(dir).unwrap().any(|entry| {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, key)
                } else {
                    let bytes = std::fs::read(&path).unwrap();
                    bytes.windows(key.len()).any(|at| at == key)
                }
            })
        }
        walk(&self.world.root, self.key.as_bytes())
    }
}

/// A mask on the image, its core a block of 16 by 12, put into the store at `root`.
pub fn put_mask(root: &Path, image: ArtifactId) -> ArtifactId {
    let store = Store::open(root, Arc::new(RealFs)).unwrap();
    let read = artifact::get(&store, ArtifactType::Image, image, &DecodeLimits::default());
    let Ok(Artifact::Image(image)) = read else {
        panic!("no image");
    };
    let core = (0..48u32)
        .flat_map(|y| {
            (0..64u32).map(move |x| {
                if (8..24).contains(&x) && (8..20).contains(&y) {
                    255
                } else {
                    0
                }
            })
        })
        .collect();
    let origin = Origin::new(Step::new("fixture", "1")).unwrap();
    let mask = Mask::from_core(
        image.binding(),
        UseCase::Remove,
        core,
        Source::default(),
        origin,
    )
    .unwrap();
    let mut publication = store.begin().unwrap();
    let id = artifact::put(&mut publication, &Artifact::from(mask)).unwrap();
    publication.publish().unwrap();
    id
}

/// An answer of 200 of the model's form whose picture is 64 by 48 of one grey.
pub fn answered() -> Vec<u8> {
    use image::ImageEncoder;
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&[90; 64 * 48 * 3], 64, 48, image::ExtendedColorType::Rgb8)
        .unwrap();
    let data = base64(&png);
    let body = serde_json::json!({
        "image": { "url": format!("data:image/png;base64,{data}"), "content_type": "image/png" }
    })
    .to_string();
    format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

/// The base64 of RFC 4648, with its padding.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let all = chunk
            .iter()
            .enumerate()
            .fold(0u32, |all, (at, b)| all | u32::from(*b) << (16 - 8 * at));
        for at in 0..4 {
            if at <= chunk.len() {
                out.push(ALPHABET[(all >> (18 - 6 * at) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// One `fill` under fal's filler at a server on loopback whose origin the config
/// approves: the world, the run, and what the server saw of the one request.
pub fn filled_once(label: &str) -> (Filled, Finished, Log) {
    let filled = Filled::new(label);
    let (listener, log) = serving(&filled.pki, vec![answered()]);
    let origin = format!("https://127.0.0.1:{}", listener.port());
    let over = filled.over(&format!("{origin}/fal-ai/bria/eraser"), Some(&origin));
    let executor = filled.runner(&over);
    let ran = filled.fill(&executor);
    assert_eq!(ran.outcome().name(), "completed", "{:?}", ran.outcome());
    drop(executor);
    assert_eq!(listener.finish(), 1);
    (filled, ran, log)
}

#[test]
fn the_key_goes_only_to_the_approved_origin_in_its_header() {
    // In its one header, `authorization`, as `Key` and the key; in no address and no
    // body; and the store holds no byte of it.
    let (filled, _, log) = filled_once("fal-key");
    let (head, body) = log.lock().unwrap()[0].clone();
    let line = format!("{KEY_HEADER}: Key {}\r\n", filled.key);
    assert!(head.contains(&line), "{head}");
    assert_eq!(head.matches(&filled.key).count(), 1, "{head}");
    assert!(
        head.starts_with("POST /fal-ai/bria/eraser HTTP/1.1\r\n"),
        "{head}"
    );
    assert!(
        !body
            .windows(filled.key.len())
            .any(|at| at == filled.key.as_bytes())
    );
    assert!(!filled.store_holds_the_key());
}

#[test]
fn a_redirect_to_another_origin_sends_nothing_there() {
    // fal's approved origin answers by pointing the fill to another origin, on another
    // port of loopback: the backend asks the gate of that address, which is not the
    // recipient's, and is refused; the other server sees no connection, so neither the
    // key nor the picture reached it. The same rule as every provider's
    // (`wired::a_redirect_to_another_origin_is_refused_and_the_key_is_not_sent_there`),
    // through fal's own wire.
    let filled = Filled::new("fal-redirect");
    let elsewhere = whizbang_testkit::loopback::Listener::serve(1, |_, _| {});
    let there = format!("https://127.0.0.1:{}/fal-ai/bria/eraser", elsewhere.port());
    let moved = format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: {there}\r\n\r\n");
    let (listener, log) = serving(&filled.pki, vec![moved.into_bytes()]);
    let origin = format!("https://127.0.0.1:{}", listener.port());
    let over = filled.over(&format!("{origin}/fal-ai/bria/eraser"), Some(&origin));
    let executor = filled.runner(&over);
    let refused = filled.fill(&executor);
    assert_eq!(refused.outcome().name(), "error", "{:?}", refused.outcome());
    drop(executor);
    assert_eq!(listener.finish(), 1);
    assert_eq!(elsewhere.finish(), 0);
    assert!(log.lock().unwrap()[0].0.contains(&filled.key));
    let text = crate::wired::shown(&filled.world.root, refused.run().as_str());
    assert!(text.contains("\trequest_not_approved\t"), "{text}");
    assert!(!filled.store_holds_the_key());
}
