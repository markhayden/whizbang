//! `fetch::…`: a set's pictures brought into a store by hash (plan 2C, task E1, item 3),
//! from a fake of the download client behind the checked downloader, or from a folder.
//! Nothing here reaches a network: the fake serves bytes from memory. And the helpers the
//! matrix's tests make their sets with: three items of source-fidelity, each a source and
//! a candidate of 64 by 48 pixels.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use whizbang::evaluation::{self, Fetching, RAW_ORIGIN};
use whizbang::import::pictures_by_file;
use whizbang_core::contract::Secrets;
use whizbang_core::egress::{Address, CheckedDownloader, NotAllowed, Origin};
use whizbang_core::identity::Digest;
use whizbang_eval::sets::{self, Set};
use whizbang_testkit::downloads::FakeDownloader;

use crate::wired::{World, whizbang};

/// The set's id, and the revision of the repository its pictures are at.
pub const SET: &str = "source-fidelity/tiny-v1";
const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

/// A picture of 64 by 48 as a PNG: its bytes.
fn png(shade: u8) -> Vec<u8> {
    let (width, height) = (64u32, 48u32);
    let rgb: Vec<u8> = (0..width * height)
        .flat_map(|at| [shade, (at % 251) as u8, (at / 64) as u8])
        .collect();
    let mut bytes = Vec::new();
    image::RgbImage::from_raw(width, height, rgb)
        .unwrap()
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    bytes
}

/// A set made in the test: its root, the folder that holds its pictures as the repository
/// does, and each picture's path in the set and bytes.
pub struct Tiny {
    pub root: PathBuf,
    pub from: PathBuf,
    pub pictures: Vec<(String, Vec<u8>)>,
}

/// A set of source-fidelity of these items under `dir`, each a source and a candidate,
/// with a label of `pass` for each; its pictures in a folder beside it.
pub fn tiny(dir: &Path, items: &[&str]) -> Tiny {
    let root = dir.join("evalsets");
    let set = root.join(SET);
    let from = dir.join("pictures");
    let mut manifest = format!(
        "[set]\nid = \"{SET}\"\nuse_case = \"source-fidelity\"\nsplit = \"dev\"\n\
         licence = \"CC-BY-4.0\"\nimages_repo = \"github.com/example/sets\"\n\
         images_rev = \"{REVISION}\"\nlabels = \"labels.toml\"\nfamilies = \"families.toml\"\n\
         rules = {{ base = \"source-fidelity\", version = 1 }}\n"
    );
    let mut labels = String::new();
    let mut pictures = Vec::new();
    for (at, item) in items.iter().enumerate() {
        let shade = u8::try_from(at).unwrap();
        let mut job =
            "[job]\nuse_case = \"source-fidelity\"\nbrief = \"Keep the house.\"\n".to_owned();
        for (role, bytes) in [("candidate", png(200 - shade)), ("source", png(10 + shade))] {
            let path = format!("items/{item}/{role}.png");
            let pin = Digest::of_bytes(&bytes).hex();
            job.push_str(&format!(
                "\n[[job.images]]\nid = \"{role}\"\nrole = \"{role}\"\n\
                 source = {{ path = \"{path}\", sha256 = \"{pin}\" }}\n"
            ));
            let file = from.join(SET).join(&path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, &bytes).unwrap();
            pictures.push((path, bytes));
        }
        std::fs::create_dir_all(set.join("items").join(item)).unwrap();
        std::fs::write(set.join("items").join(item).join("job.toml"), job).unwrap();
        manifest.push_str(&format!(
            "\n[[item]]\nid = \"{item}\"\nfamily = \"house\"\njob = \"items/{item}/job.toml\"\n"
        ));
        labels.push_str(&format!(
            "[[label]]\nitem = \"{item}\"\nexpected = \"pass\"\nwords = \"kept\"\n\
             adjudicated_by = \"owner\"\nadjudicated_on = \"2026-11-18\"\n\n"
        ));
    }
    std::fs::write(set.join("set.toml"), manifest).unwrap();
    std::fs::write(set.join("labels.toml"), labels).unwrap();
    std::fs::write(
        set.join("families.toml"),
        "[[family]]\nid = \"house\"\ndescription = \"a house\"\n",
    )
    .unwrap();
    Tiny {
        root,
        from,
        pictures,
    }
}

/// Every picture of the set, from the folder, into the store at `root`.
pub fn fetched(tiny: &Tiny, root: &Path) {
    let set = sets::find(&tiny.root, SET).unwrap();
    evaluation::from_folder(&set, &tiny.from, root, &whizbang::wiring::recovery()).unwrap();
}

fn address(path: &str) -> Address {
    let text = format!("{RAW_ORIGIN}/example/sets/{REVISION}/{SET}/{path}");
    Address::of(&text, &Secrets::none()).unwrap()
}

/// `fetch` of the set with a fake that serves what `serve` gives for each picture, over
/// the approved origins: what it said, the fake, and how many download clients were made.
fn fetch_with(
    world: &World,
    tiny: &Tiny,
    approved: &[Origin],
    serve: impl Fn(&str, &[u8]) -> Vec<u8>,
) -> (Result<String, String>, Arc<FakeDownloader>, usize) {
    let mut fake = FakeDownloader::new();
    for (path, bytes) in &tiny.pictures {
        fake = fake.serving(&address(path), serve(path, bytes));
    }
    let fake = Arc::new(fake);
    let made = std::sync::atomic::AtomicUsize::new(0);
    let shared = fake.clone();
    let make = |origins: &[Origin], _: &Secrets| -> Result<CheckedDownloader, NotAllowed> {
        made.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        CheckedDownloader::new(shared.clone(), origins)
    };
    let how = Fetching {
        base: RAW_ORIGIN,
        approved,
        secrets: &Secrets::none(),
        make: &make,
    };
    let set = Set::load(&tiny.root.join(SET)).unwrap();
    let said = evaluation::fetch(&set, &world.root, &whizbang::wiring::recovery(), &how);
    (said, fake, made.into_inner())
}

fn raw() -> Vec<Origin> {
    vec![Origin::of(RAW_ORIGIN, &Secrets::none()).unwrap()]
}

/// A world with a store.
fn world(label: &str) -> World {
    let world = World::new(label);
    let made = whizbang(&["spend", "reset", "--allowance", "1"], &world.root);
    assert_eq!(made.code, Some(0), "{}", made.err);
    world
}

#[test]
fn a_picture_is_fetched_by_hash_from_the_pinned_revision_and_imported() {
    let world = world("fetch-pinned");
    let tiny = tiny(&world.beside("set"), &["one"]);
    let (said, fake, _) = fetch_with(&world, &tiny, &raw(), |_, bytes| bytes.to_vec());
    assert_eq!(
        said.unwrap(),
        format!("{SET}: fetched 2 pictures, 0 already there\n")
    );
    // Each picture was asked for once, at the revision the set pins, under its set's id.
    assert_eq!(fake.requests().len(), 2);
    for (path, _) in &tiny.pictures {
        assert_eq!(fake.asked(&address(path)), 1, "{path}");
    }
    assert!(fake.requests().iter().all(|a| a.headers().is_empty()));
    // Each is in the store as a picture, by the digest of the file it came from.
    let held = pictures_by_file(&world.root).unwrap();
    for (_, bytes) in &tiny.pictures {
        assert!(held.contains_key(&Digest::of_bytes(bytes)));
    }
    // Fetched again, nothing is asked for.
    let (said, again, _) = fetch_with(&world, &tiny, &raw(), |_, bytes| bytes.to_vec());
    assert_eq!(
        said.unwrap(),
        format!("{SET}: fetched 0 pictures, 2 already there\n")
    );
    assert!(again.requests().is_empty());
}

#[test]
fn a_picture_whose_hash_differs_is_refused_and_nothing_is_imported() {
    let world = world("fetch-hash");
    let tiny = tiny(&world.beside("set"), &["one"]);
    let other = png(77);
    let (said, _, _) = fetch_with(&world, &tiny, &raw(), |path, bytes| {
        if path.ends_with("candidate.png") {
            other.clone()
        } else {
            bytes.to_vec()
        }
    });
    let refused = said.unwrap_err();
    assert!(
        refused.contains("image candidate: what arrived is not the picture its job pins"),
        "{refused}"
    );
    // Neither what arrived nor the candidate is in the store.
    let held = pictures_by_file(&world.root).unwrap();
    assert!(!held.contains_key(&Digest::of_bytes(&other)));
    assert!(!held.contains_key(&Digest::of_bytes(&tiny.pictures[0].1)));
}

#[test]
fn a_fetch_to_an_origin_the_config_does_not_name_is_refused_before_a_byte_leaves() {
    let world = world("fetch-origin");
    let tiny = tiny(&world.beside("set"), &["one"]);
    let elsewhere = vec![Origin::of("https://example.com", &Secrets::none()).unwrap()];
    for approved in [Vec::new(), elsewhere] {
        let (said, fake, made) = fetch_with(&world, &tiny, &approved, |_, b| b.to_vec());
        let refused = said.unwrap_err();
        assert_eq!(
            refused,
            "https://raw.githubusercontent.com is not an origin the config approves as a \
             recipient (`[egress.recipients]`), so nothing was fetched"
        );
        assert_eq!(made, 0, "no download client was made");
        assert!(fake.requests().is_empty());
    }
    // The binary says the same with a config that approves no such origin, before it
    // makes a client: no network is reached.
    let config = world.beside("config.toml");
    std::fs::write(&config, "").unwrap();
    let ran = whizbang(
        &[
            "eval",
            "sets",
            "fetch",
            "--set",
            SET,
            "--root",
            tiny.root.to_str().unwrap(),
            "--config",
            config.to_str().unwrap(),
        ],
        &world.root,
    );
    assert_eq!(ran.code, Some(1));
    assert!(
        ran.err.contains("is not an origin the config approves"),
        "{}",
        ran.err
    );
}

#[test]
fn from_a_folder_imports_the_same_pictures_by_hash() {
    let fetched_world = world("fetch-net");
    let tiny = tiny(&fetched_world.beside("set"), &["one", "two"]);
    let (said, _, _) = fetch_with(&fetched_world, &tiny, &raw(), |_, bytes| bytes.to_vec());
    said.unwrap();
    let placed = world("fetch-folder");
    let ran = whizbang(
        &[
            "eval",
            "sets",
            "fetch",
            "--set",
            SET,
            "--root",
            tiny.root.to_str().unwrap(),
            "--from",
            tiny.from.to_str().unwrap(),
        ],
        &placed.root,
    );
    assert_eq!(ran.code, Some(0), "{}", ran.err);
    assert_eq!(
        ran.out,
        format!("{SET}: imported 4 pictures from the folder, 0 already there\n")
    );
    assert_eq!(
        pictures_by_file(&placed.root).unwrap(),
        pictures_by_file(&fetched_world.root).unwrap()
    );
    // A file in the folder that is not the pinned picture is refused.
    let (path, _) = &tiny.pictures[0];
    std::fs::write(tiny.from.join(SET).join(path), png(99)).unwrap();
    let other = world("fetch-folder-wrong");
    let ran = whizbang(
        &[
            "eval",
            "sets",
            "fetch",
            "--set",
            SET,
            "--root",
            tiny.root.to_str().unwrap(),
            "--from",
            tiny.from.to_str().unwrap(),
        ],
        &other.root,
    );
    assert_eq!(ran.code, Some(1));
    assert!(
        ran.err.contains("is not the one the job pins"),
        "{}",
        ran.err
    );
}
