//! `lab::…`: the lab's files read and checked, a cell's identity, and a lab's cells run on
//! the shared harness (plan m3, task L1). Corpora are a few synthetic pictures made in the
//! test; the segmenter is the fake of `recipe.rs`, and no filler is called.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use whizbang::harness::{self, Running, Work as _};
use whizbang::laboratory::{self, LabCell};
use whizbang::wiring;
use whizbang_core::artifact::ArtifactId;
use whizbang_core::contract::Registry;
use whizbang_core::contract::record::ArgumentValue;
use whizbang_core::identity::Digest;
use whizbang_core::recipe::RecipeAction;
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_eval::lab::cell::{self, Backend, Cell, Work};
use whizbang_eval::lab::corpus::Corpus;
use whizbang_eval::lab::digest_of;
use whizbang_eval::lab::matrix::{Matrix, Validated};
use whizbang_eval::lab::protocol::{self, Protocol};
use whizbang_testkit::scratch::Scratch;

use crate::recipe::{CONFIG, over, with_small_picture};
use crate::wired::{whizbang, whizbang_storeless};

/// A corpus manifest of these pictures: id, bytes, group.
pub fn corpus_text(pictures: &[(&str, &[u8], Option<&str>)]) -> String {
    let mut text = "version = 1\n".to_owned();
    for (id, bytes, group) in pictures {
        text.push_str(&format!(
            "\n[[image]]\nid = \"{id}\"\nfile = \"{id}.png\"\nsha256 = \"{}\"\nlicence = \"Made in the test.\"\n",
            digest_of(bytes)
        ));
        if let Some(group) = group {
            text.push_str(&format!("group = \"{group}\"\n"));
        }
    }
    text
}

/// A protocol comparing these two arms over one group of these images.
pub fn protocol_text(arms: [&str; 2], images: &[&str], min_decided: u32) -> String {
    let images: Vec<String> = images.iter().map(|id| format!("\"{id}\"")).collect();
    format!(
        "version = 1\nname = \"tiny-lab\"\nseed = 7\narms = [\"{}\", \"{}\"]\nmin_decided = {min_decided}\n\n\
         [[group]]\nname = \"all\"\nimages = [{}]\n",
        arms[0],
        arms[1],
        images.join(", ")
    )
}

/// The lab's head and its one image `a`, which binds a protect mask, a clean picture and a
/// prompt; then `arms`.
pub fn matrix_text(prompt: &str, image_overrides: &str, arms: &str) -> String {
    format!(
        "version = 1\nname = \"tiny-lab\"\ncorpus = \"corpus.toml\"\nprotocol = \"protocol.toml\"\nseed = 7\n\n\
         [[image]]\nid = \"a\"\ninputs = {{ protect = \"a-protect\", clean = \"a-clean\" }}\n\
         texts = {{ prompt = \"{prompt}\" }}\noverrides = {{ {image_overrides} }}\n\n{arms}"
    )
}

/// A recipe arm `found` of `remove_prompt`, and the reference arm `clean`.
pub const FOUND_AND_CLEAN: &str = "[[arm]]\nname = \"found\"\nrecipe = \"remove_prompt\"\n\n\
     [[arm]]\nname = \"clean\"\nreference = \"clean\"\n";

/// The corpus the matrices bind: `a` judged, and the two files `a` binds.
pub fn corpus_of(protect: &[u8]) -> String {
    corpus_text(&[
        ("a", b"a picture", Some("all")),
        ("a-protect", protect, None),
        ("a-clean", b"a clean picture", None),
    ])
}

/// What the tests resolve a backend to: a model of the capability, its revision, and the
/// price of a request.
pub fn backends(
    revision: &'static str,
    fill_price: u64,
) -> impl Fn(&str, Option<&str>) -> Result<Backend, String> {
    move |capability, _named| {
        Ok(Backend {
            name: format!("{capability}-model"),
            revision: revision.to_owned(),
            settings: BTreeMap::from([("quality".to_owned(), "high".to_owned())]),
            provider: "maker".to_owned(),
            endpoint: format!("https://{capability}.example.org/v1"),
            price_micros: if capability == "fill" { fill_price } else { 0 },
        })
    }
}

/// The lab's matrix checked against its protocol and corpus, and its cells.
pub fn checked_with(
    registry: &Registry,
    matrix: &str,
    protocol: &str,
    corpus: &str,
    load: &dyn Fn(&str) -> Result<RecipeAction, String>,
    resolve: cell::Resolve<'_>,
) -> Result<(Validated, Vec<Cell>), String> {
    let (protocol, digest): (Protocol, String) = Protocol::parse(protocol.as_bytes())?;
    let corpus = Corpus::parse(corpus)?;
    let validated = Matrix::parse(matrix)?.validate(&protocol, &digest, &corpus, registry, load)?;
    let cells = cell::cells(&validated, registry, load, resolve, &laboratory::constants)?;
    Ok((validated, cells))
}

/// The same, with the build's recipes.
pub fn checked(
    matrix: &str,
    protocol: &str,
    corpus: &str,
    resolve: cell::Resolve<'_>,
) -> Result<(Validated, Vec<Cell>), String> {
    let registry = wiring::registry().unwrap();
    let load = |name: &str| laboratory::recipe(&registry, name);
    checked_with(&registry, matrix, protocol, corpus, &load, resolve)
}

/// A matrix whose image binds nothing: for arms of `seam`, which takes nothing but the
/// image.
pub fn bare(matrix: &str) -> String {
    let kept: Vec<&str> = (matrix.lines())
        .filter(|line| !line.starts_with("inputs = ") && !line.starts_with("texts = "))
        .collect();
    kept.join("\n") + "\n"
}

/// The identity of the cell of this arm on `a`.
fn identity(cells: &[Cell], arm: &str) -> String {
    let found = cells
        .iter()
        .find(|c| c.arm == arm && c.image == "a")
        .unwrap();
    found.identity.clone()
}

/// Two seam arms filled by `fal`, the second on the thin set.
const SEAMS: &str = "[[arm]]\nname = \"wide\"\nrecipe = \"seam\"\nfiller = \"fal\"\n\n\
     [[arm]]\nname = \"thin\"\nrecipe = \"seam\"\nfiller = \"fal\"\noverrides = { \"fan.constants\" = \"fan-thin-v1\" }\n";

#[test]
fn a_change_of_override_input_text_constant_set_action_version_or_filler_makes_a_new_cell() {
    let protocol = protocol_text(["found", "clean"], &["a"], 3);
    let base = |prompt: &str, overrides: &str, protect: &[u8]| {
        let matrix = matrix_text(prompt, overrides, FOUND_AND_CLEAN);
        let (_, cells) =
            checked(&matrix, &protocol, &corpus_of(protect), &backends("1", 0)).unwrap();
        cells
    };
    let first = base("collar", "", b"a mask");
    let found = identity(&first, "found");
    // Nothing changed: the same cell.
    assert_eq!(identity(&base("collar", "", b"a mask"), "found"), found);
    // A text, an input's bytes, an override: each a new cell.
    assert_ne!(identity(&base("hat", "", b"a mask"), "found"), found);
    assert_ne!(
        identity(&base("collar", "", b"another mask"), "found"),
        found
    );
    assert_ne!(
        identity(&base("collar", "\"grow.px\" = 6.0", b"a mask"), "found"),
        found
    );
    // An action's version, and a finder's revision.
    let held = &first.iter().find(|c| c.arm == "found").unwrap().components;
    let mut changed = held.clone();
    changed
        .actions
        .insert("grow".to_owned(), "9.9.9".to_owned());
    assert_ne!(changed.identity(), found);
    assert_eq!(held.identity(), found);
    assert_eq!(held.finders["segment"].revision, "1");
    let finder = checked(
        &matrix_text("collar", "", FOUND_AND_CLEAN),
        &protocol,
        &corpus_of(b"a mask"),
        &backends("2", 0),
    )
    .unwrap()
    .1;
    assert_ne!(identity(&finder, "found"), found);

    // The constant set a step selects, and the filler.
    let seam_protocol = protocol_text(["wide", "thin"], &["a"], 3);
    let seams = |arms: &str, revision| {
        let matrix = bare(&matrix_text("collar", "", arms));
        checked(
            &matrix,
            &seam_protocol,
            &corpus_of(b"a mask"),
            &backends(revision, 40_000),
        )
        .unwrap()
        .1
    };
    let cells = seams(SEAMS, "1");
    let (wide, thin) = (identity(&cells, "wide"), identity(&cells, "thin"));
    assert_ne!(wide, thin);
    let sets = &cells
        .iter()
        .find(|c| c.arm == "thin")
        .unwrap()
        .components
        .constants;
    let fan = laboratory::constants("fan-thin-v1").unwrap();
    assert_eq!(
        sets["fan"],
        ("fan-thin-v1".to_owned(), digest_of(fan.as_bytes()))
    );
    let wide_sets = &cells
        .iter()
        .find(|c| c.arm == "wide")
        .unwrap()
        .components
        .constants;
    assert_eq!(wide_sets["fan"].0, "fan-v3");
    assert_eq!(identity(&seams(SEAMS, "1"), "wide"), wide);
    assert_ne!(identity(&seams(SEAMS, "2"), "wide"), wide);
}

#[test]
fn a_matrix_naming_an_arm_the_protocol_does_not_is_refused_before_anything_runs() {
    let protocol = protocol_text(["found", "clean"], &["a"], 3);
    let extra =
        format!("{FOUND_AND_CLEAN}\n[[arm]]\nname = \"extra\"\nrecipe = \"remove_prompt\"\n");
    let refused = checked(
        &matrix_text("collar", "", &extra),
        &protocol,
        &corpus_of(b"a mask"),
        &backends("1", 0),
    )
    .unwrap_err();
    assert_eq!(
        refused,
        "matrix tiny-lab: it runs the arm extra, which the protocol does not compare; nothing \
         was run"
    );
    // And the other way: an arm the protocol compares that the matrix lacks.
    let lacking = protocol_text(["found", "gone"], &["a"], 3);
    let refused = checked(
        &matrix_text("collar", "", FOUND_AND_CLEAN),
        &lacking,
        &corpus_of(b"a mask"),
        &backends("1", 0),
    )
    .unwrap_err();
    assert!(
        refused.contains("runs the arm clean, which the protocol does not"),
        "{refused}"
    );
}

/// A recipe that finds a thing a second time in what its first find made.
const TWICE: &str = r#"version = 1
name = "twice"
purpose = "Finds a thing again in what a first find made."
use = "In a test of the dependent-finder rule."

[inputs]
image = { type = "Image" }
prompt = { type = "Text" }

[[step]]
id = "first"
op = "segment"
inputs = { image = "inputs.image" }
query = "inputs.prompt"

[[step]]
id = "kept"
op = "select"
inputs = { mask = "first.mask" }

[[step]]
id = "applied"
op = "apply"
inputs = { image = "inputs.image", mask = "kept.mask" }

[[step]]
id = "second"
op = "segment"
inputs = { image = "applied.image" }
query = "inputs.prompt"

[outputs]
mask = "second.mask"
"#;

#[test]
fn a_dependent_finder_is_refused_naming_both_steps() {
    let registry = wiring::registry().unwrap();
    let load = |name: &str| match name {
        "twice" => whizbang_rascal::recipes::action_of(TWICE, &registry).map_err(|e| e.to_string()),
        other => laboratory::recipe(&registry, other),
    };
    let arms = "[[arm]]\nname = \"twice\"\nrecipe = \"twice\"\n\n[[arm]]\nname = \"clean\"\nreference = \"clean\"\n";
    let matrix = matrix_text("collar", "", arms).replace("protect = \"a-protect\", ", "");
    let refused = checked_with(
        &registry,
        &matrix,
        &protocol_text(["twice", "clean"], &["a"], 3),
        &corpus_of(b"a mask"),
        &load,
        &backends("1", 0),
    )
    .unwrap_err();
    assert_eq!(
        refused,
        "matrix tiny-lab: arm twice: recipe twice: the finder second (segment) takes image = \
         applied.image, which derives from the finder first; a finder's inputs must be known \
         before any finder runs"
    );
}

/// A store with the test's picture, the lab's cells, the executor over the fake segmenter,
/// and each corpus id of the lab bound to the picture.
struct Round {
    dir: Scratch,
    root: std::path::PathBuf,
    over: whizbang::wiring::Over,
    pictures: BTreeMap<String, ArtifactId>,
}

impl Round {
    fn new(label: &str) -> Round {
        let dir = Scratch::new(label);
        let (root, image) = with_small_picture(&dir);
        let over = over(&dir, CONFIG);
        let id = ArtifactId(Digest::from_hex(image.strip_prefix("sha256:").unwrap()).unwrap());
        let pictures = ["a", "a-protect", "a-clean"]
            .into_iter()
            .map(|name| (name.to_owned(), id))
            .collect();
        Round {
            dir,
            root,
            over,
            pictures,
        }
    }

    /// Runs these cells on the harness under the lab's scope: how many it started.
    fn run(&self, cells: &[&Cell], scope: &str) -> Result<usize, String> {
        let labs: Vec<LabCell<'_>> = (cells.iter())
            .map(|cell| LabCell {
                cell,
                scope,
                pictures: &self.pictures,
            })
            .collect();
        let work: Vec<&dyn harness::Work> = labs.iter().map(|c| c as &dyn harness::Work).collect();
        self.run_work(&work, scope)
    }

    /// Runs this work on the harness, recognised under `scope`.
    fn run_work(&self, work: &[&dyn harness::Work], scope: &str) -> Result<usize, String> {
        let store = Arc::new(Store::open(&self.root, Arc::new(RealFs)).unwrap());
        let registry = wiring::registry_over(&self.over, false).unwrap();
        let executor = wiring::runner(registry, store, &self.over).unwrap();
        let running = Running {
            executor: &executor,
            root: &self.root,
            left_micros: 1_000_000,
            stop: &|| false,
        };
        harness::run(
            work,
            &harness::by_request(scope, work),
            &running,
            1,
            "round",
        )
    }
}

/// The text a run of an action was given for its argument `name`, for each run of it.
fn texts_given(root: &Path, action: &str, name: &str) -> Vec<String> {
    let mut found = Vec::new();
    harness::each_run(root, &mut |record, _, _| {
        if record.asked().action.as_deref() != Some(action) {
            return;
        }
        for argument in &record.asked().arguments {
            if let (true, ArgumentValue::Text(text)) = (argument.name == name, &argument.value) {
                found.push(text.clone());
            }
        }
    })
    .unwrap();
    found
}

/// The protect mask is not one in the store, so the round binds no protect.
fn round_matrix(prompt: &str) -> String {
    matrix_text(prompt, "", FOUND_AND_CLEAN).replace("protect = \"a-protect\", ", "")
}

#[test]
fn a_text_binding_reaches_remove_prompts_prompt() {
    let round = Round::new("lab-text");
    let protocol = protocol_text(["found", "clean"], &["a"], 3);
    let (validated, cells) = checked(
        &round_matrix("collar"),
        &protocol,
        &corpus_of(b"a mask"),
        &backends("1", 0),
    )
    .unwrap();
    let found: Vec<&Cell> = cells.iter().filter(|c| c.arm == "found").collect();
    let Work::Recipe { texts, .. } = &found[0].work else {
        panic!("a recipe's cell")
    };
    assert_eq!(texts["prompt"], "collar");
    let scope = protocol::scope(&validated.protocol.name, &validated.protocol_digest);
    assert_eq!(round.run(&found, &scope), Ok(1));
    // The step `segment` of the recipe's run was given the image's text as its query.
    assert_eq!(texts_given(&round.root, "segment", "query"), ["collar"]);
    assert_eq!(
        texts_given(&round.root, "remove_prompt", "prompt"),
        ["collar"]
    );
    drop(round.dir);
}

#[test]
fn a_protocol_changed_after_a_run_is_refused() {
    let round = Round::new("lab-frozen");
    let protocol = protocol_text(["found", "clean"], &["a"], 3);
    let (_, digest) = Protocol::parse(protocol.as_bytes()).unwrap();
    assert_eq!(laboratory::frozen(&round.root, "tiny-lab", &digest), Ok(()));
    // One cell of the lab run under its scope: the cheapest action that makes a run.
    let scope = protocol::scope("tiny-lab", &digest);
    let cell = crate::runner::Finding {
        image: round.pictures["a"],
        scope: scope.clone(),
    };
    assert_eq!(round.run_work(&[&cell], &scope), Ok(1));
    assert_eq!(laboratory::frozen(&round.root, "tiny-lab", &digest), Ok(()));
    // The protocol edited after the run: refused, whatever was edited.
    let (_, edited) =
        Protocol::parse(protocol_text(["found", "clean"], &["a"], 4).as_bytes()).unwrap();
    let refused = laboratory::frozen(&round.root, "tiny-lab", &edited).unwrap_err();
    assert!(
        refused.contains("the lab tiny-lab ran under another protocol"),
        "{refused}"
    );
    assert!(
        refused.ends_with("a new lab, with a new name; nothing was run"),
        "{refused}"
    );
    // Another lab's name is a lab of its own.
    assert_eq!(
        laboratory::frozen(&round.root, "other-lab", &edited),
        Ok(())
    );
    drop(round.dir);
}

#[test]
fn a_reference_arm_hands_back_its_bound_image_and_calls_no_filler() {
    let round = Round::new("lab-reference");
    let protocol = protocol_text(["wide", "clean"], &["a"], 3);
    let arms = "[[arm]]\nname = \"wide\"\nrecipe = \"seam\"\nfiller = \"fal\"\n\n\
                [[arm]]\nname = \"clean\"\nreference = \"clean\"\n";
    let matrix = matrix_text("collar", "", arms).replace("protect = \"a-protect\", ", "");
    let matrix = matrix.replace("texts = { prompt = \"collar\" }\n", "");
    let (_, cells) = checked(
        &matrix,
        &protocol,
        &corpus_of(b"a mask"),
        &backends("1", 40_000),
    )
    .unwrap();
    let clean = cells.iter().find(|c| c.arm == "clean").unwrap();
    assert_eq!(
        clean.work,
        Work::Reference {
            picture: "a-clean".to_owned()
        }
    );
    assert_eq!(clean.forecast_micros, 0);
    assert_eq!(clean.components.filler, None);
    assert_eq!(
        clean.components.inputs["clean"],
        digest_of(b"a clean picture")
    );
    let held = LabCell {
        cell: clean,
        scope: "lab-test",
        pictures: &round.pictures,
    };
    assert_eq!(held.handed_back(), Ok(Some(round.pictures["a-clean"])));
    assert_eq!(held.request(1), Ok(None));
    // Run on the harness, it starts nothing, and nothing is in the store's runs.
    assert_eq!(round.run(&[clean], "lab-test"), Ok(0));
    let listed = whizbang(&["runs", "list"], &round.root);
    assert_eq!(listed.out, "runs: 0\n");
    drop(round.dir);
}

#[test]
fn corpus_verify_names_each_missing_and_changed_picture() {
    let dir = Scratch::new("lab-verify");
    let pictures = dir.path().join("pictures");
    std::fs::create_dir_all(&pictures).unwrap();
    let manifest = dir.path().join("corpus.toml");
    std::fs::write(
        &manifest,
        corpus_text(&[
            ("kept", b"kept bytes", Some("all")),
            ("gone", b"gone bytes", Some("all")),
            ("edited", b"edited bytes", None),
        ]),
    )
    .unwrap();
    std::fs::write(pictures.join("kept.png"), b"kept bytes").unwrap();
    std::fs::write(pictures.join("edited.png"), b"other bytes").unwrap();
    let args = |root: &Path| {
        whizbang_storeless(&[
            "lab",
            "corpus",
            "verify",
            "--manifest",
            manifest.to_str().unwrap(),
            "--root",
            root.to_str().unwrap(),
        ])
    };
    let ran = args(&pictures);
    assert_eq!(ran.code, Some(1), "{}", ran.err);
    assert_eq!(
        ran.out,
        format!(
            "ok kept\nmissing gone\nchanged edited wanted {} found {}\n\
             pictures: 3, ok: 1, missing or changed: 2\n",
            digest_of(b"edited bytes"),
            digest_of(b"other bytes")
        )
    );
    assert_eq!(ran.err, "whizbang: some pictures are missing or changed\n");
    // With every picture as pinned, it passes.
    std::fs::write(pictures.join("gone.png"), b"gone bytes").unwrap();
    std::fs::write(pictures.join("edited.png"), b"edited bytes").unwrap();
    let ran = args(&pictures);
    assert_eq!(ran.code, Some(0), "{}", ran.err);
    assert!(
        ran.out
            .ends_with("pictures: 3, ok: 3, missing or changed: 0\n"),
        "{}",
        ran.out
    );
}

// --- rounds (plan m3, task L2) ------------------------------------------------------------------

use std::sync::atomic::{AtomicU64, Ordering};

use whizbang::pack::OneWorker;
use whizbang::wiring::Over;
use whizbang_core::artifact::{self as artifacts, ArtifactType};
use whizbang_core::capability::{
    Bounds, Declaration, Fault, FaultKind, FillRequest, Filler, Painted, Price,
};
use whizbang_core::config::{Kind, KindRefused, Table};
use whizbang_core::contract::{Made, ModelIdentity};
use whizbang_core::egress::Recipient;
use whizbang_core::identity::DecodeLimits;
use whizbang_eval::lab::decode::{self, Assignment, Outcome, Pair, Picked, Picks, Shown, Side};
use whizbang_testkit::capabilities::{FakeFiller, Paint};

/// What one fill of the scripted filler is forecast at, in millionths of a dollar.
pub const FILL_PRICE: u64 = 1000;

/// A remote kind at an address of the reserved `.example` domain, which nothing contacts:
/// the test kit's filler, counting its calls, and failing the call of number `fail_at`.
pub struct Counting {
    pub calls: Arc<AtomicU64>,
    pub fail_at: u64,
}

struct CountingFiller {
    inner: FakeFiller,
    calls: Arc<AtomicU64>,
    fail_at: u64,
}

impl Filler for CountingFiller {
    fn declaration(&self) -> &Declaration {
        self.inner.declaration()
    }

    fn fill(&self, request: &FillRequest, bounds: &Bounds) -> Result<Painted, Fault> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if call == self.fail_at {
            return Err(Fault::new(
                FaultKind::Rejected,
                "The scripted filler refused it.",
            ));
        }
        self.inner.fill(request, bounds)
    }
}

impl Kind for Counting {
    fn name(&self) -> &str {
        "counting"
    }

    fn make(&self, table: &Table<'_>) -> Result<Made, KindRefused> {
        let model = ModelIdentity::new("counting", "eraser", "1");
        let address = table.backend().address().expect("a url");
        let declaration = Declaration::new(model, &[])
            .unwrap()
            .sending_to(Recipient::new(address.origin().clone()))
            .priced(Price::at_most(FILL_PRICE as f64).unwrap());
        Ok(Made::Fill(Arc::new(CountingFiller {
            inner: FakeFiller::new(Paint::Inverse).declared_as(declaration),
            calls: self.calls.clone(),
            fail_at: self.fail_at,
        })))
    }
}

/// A config whose one filler is of the kind `counting`, at `url`.
pub fn counting_config(url: &str) -> String {
    format!(
        "[backends.fill]\nkind = \"counting\"\nurl = \"{url}\"\n\n\
         [egress.recipients.fill]\norigin = \"https://fill.example.org\"\n\
         classes = [\"source_images\", \"derived_images\"]\n"
    )
}

/// A picture of 64 by 64, its colours shifted by `seed`.
pub fn picture(seed: u8) -> Vec<u8> {
    use image::ImageEncoder;
    let mut rgba = Vec::new();
    for y in 0..64_u32 {
        for x in 0..64_u32 {
            rgba.extend_from_slice(&[(x * 4) as u8, (y * 4) as u8, seed, 255]);
        }
    }
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&rgba, 64, 64, image::ExtendedColorType::Rgba8)
        .unwrap();
    png
}

/// A recipe of the user's own (`[recipes] paths`): the box rasterized, kept off what is
/// protected, and filled. Three runs a cell, to keep the rounds lean.
pub const FILL_AT: &str = r#"version = 1
name = "fill_at"
purpose = "Fills the place a box says, kept off what is protected."
use = "In a test of a lab round."

[inputs]
image = { type = "Image" }
location = { type = "Location" }
protect = { type = "Mask" }
backend = { type = "Text", optional = true }

[[step]]
id = "rasterize"
op = "rasterize"
inputs = { location = "inputs.location", image = "inputs.image" }
use_case = "remove"

[[step]]
id = "subtract"
op = "subtract"
inputs = { mask = "rasterize.mask", subtract = "inputs.protect" }

[[step]]
id = "fill"
op = "fill"
inputs = { image = "inputs.image", mask = "subtract.mask" }
backend = "inputs.backend"

[outputs]
image = "fill.image"
"#;

/// A round of three pictures: a store with an allowance of a dollar; a corpus folder
/// outside it holding the pictures `a`, `b` and `c` of 64 by 64 and, for each, a box on it
/// as a location's canonical file (`a-at`, ...) and a mask to protect as a mask's
/// (`a-keep`, ...); and beside the matrix its manifest and a protocol comparing the arms
/// `filled` (the user's recipe `fill_at`, filled by the config's filler) and `source` (a
/// reference arm handing back the picture).
pub struct ThreeImages {
    _dir: Scratch,
    pub root: std::path::PathBuf,
    pub corpus: std::path::PathBuf,
    pub matrix: std::path::PathBuf,
    pub calls: Arc<AtomicU64>,
    pub config: std::path::PathBuf,
}

pub const FILLED_AND_SOURCE: &str = "[[arm]]\nname = \"filled\"\nrecipe = \"fill_at\"\n\n\
     [[arm]]\nname = \"source\"\nreference = \"source\"\n";

impl ThreeImages {
    pub fn new(label: &str, arms: &str) -> ThreeImages {
        use whizbang_core::artifact::{
            Artifact, Location, Mask, Origin, Shape, Source, Step, UseCase,
        };
        let dir = Scratch::new(label);
        let root = dir.path().join("store");
        let corpus = dir.path().join("corpus");
        std::fs::create_dir_all(&corpus).unwrap();
        let made = wiring::opening(&root, wiring::Make::TheRoot, wiring::recovery()).unwrap();
        let store = made.writer().unwrap();
        let dollar = whizbang_core::spend::Amount::new(1_000_000).unwrap();
        whizbang_core::spend::reset(&store, dollar, 1).unwrap();
        drop((store, made));
        let mut manifest = Vec::new();
        for (at, id) in ["a", "b", "c"].into_iter().enumerate() {
            let png = picture(40 * at as u8);
            let image =
                whizbang::import::picture_of_bytes(&png, id, None, &root, wiring::recovery())
                    .unwrap();
            let store = Store::open(&root, Arc::new(RealFs)).unwrap();
            let read = artifacts::get(&store, ArtifactType::Image, image, &DecodeLimits::default());
            let Ok(Artifact::Image(image)) = read else {
                panic!("no image");
            };
            let origin = Origin::new(Step::new("fixture", "1")).unwrap();
            let shape = Shape::Box {
                left: 20.0,
                top: 16.0,
                right: 36.0,
                bottom: 28.0,
            };
            let location = Location::new(image.binding(), shape, origin.clone()).unwrap();
            let core = (0..64 * 64)
                .map(|i| if i % 64 < 24 { 255 } else { 0 })
                .collect();
            let keep = Mask::from_core(
                image.binding(),
                UseCase::Remove,
                core,
                Source::default(),
                origin,
            )
            .unwrap();
            let files = [
                (id.to_owned(), "png", png, Some("all")),
                (
                    format!("{id}-at"),
                    "loc",
                    Artifact::from(location).to_bytes(),
                    None,
                ),
                (
                    format!("{id}-keep"),
                    "mask",
                    Artifact::from(keep).to_bytes(),
                    None,
                ),
            ];
            for (name, extension, bytes, group) in files {
                let file = format!("{name}.{extension}");
                std::fs::write(corpus.join(&file), &bytes).unwrap();
                manifest.push((name, file, bytes, group));
            }
        }
        let mut text = "version = 1\n".to_owned();
        for (id, file, bytes, group) in &manifest {
            text.push_str(&format!(
                "\n[[image]]\nid = \"{id}\"\nfile = \"{file}\"\nsha256 = \"{}\"\nlicence = \"Made in the test.\"\n",
                digest_of(bytes)
            ));
            if let Some(group) = group {
                text.push_str(&format!("group = \"{group}\"\n"));
            }
        }
        let lab = dir.path().join("lab");
        std::fs::create_dir_all(&lab).unwrap();
        std::fs::write(lab.join("corpus.toml"), text).unwrap();
        std::fs::write(
            lab.join("protocol.toml"),
            protocol_text(["filled", "source"], &["a", "b", "c"], 3),
        )
        .unwrap();
        let mut matrix = "version = 1\nname = \"tiny-lab\"\ncorpus = \"corpus.toml\"\n\
                          protocol = \"protocol.toml\"\nseed = 7\n"
            .to_owned();
        for id in ["a", "b", "c"] {
            matrix.push_str(&format!(
                "\n[[image]]\nid = \"{id}\"\n\
                 inputs = {{ location = \"{id}-at\", protect = \"{id}-keep\", source = \"{id}\" }}\n"
            ));
        }
        matrix.push('\n');
        matrix.push_str(arms);
        let path = lab.join("matrix.toml");
        std::fs::write(&path, matrix).unwrap();
        let recipe = dir.path().join("fill_at.toml");
        std::fs::write(&recipe, FILL_AT).unwrap();
        let config = dir.path().join("whizbang.toml");
        let text = format!(
            "{}\n[recipes]\npaths = [\"{}\"]\n",
            counting_config("https://fill.example.org/v1"),
            recipe.display()
        );
        std::fs::write(&config, text).unwrap();
        ThreeImages {
            _dir: dir,
            root,
            corpus,
            matrix: path,
            calls: Arc::new(AtomicU64::new(0)),
            config,
        }
    }

    /// The over of the round: the config, and the kind `counting` failing call `fail_at`.
    pub fn over(&self, fail_at: u64) -> Over {
        let mut kinds = wiring::kinds(&OneWorker::installed());
        let counting = Counting {
            calls: self.calls.clone(),
            fail_at,
        };
        kinds.register(Arc::new(counting)).unwrap();
        Over::nothing().config(Some(&self.config)).kinds(kinds)
    }

    /// `whizbang lab run` as the binary does it, under this over, with what the store's
    /// allowance has left or `left`, stopping when `stop` says.
    pub fn run(
        &self,
        over: &Over,
        left: Option<u64>,
        stop: &dyn Fn() -> bool,
    ) -> Result<laboratory::Report, String> {
        let asked = laboratory::Asked {
            matrix: &self.matrix,
            corpus: &self.corpus,
            root: &self.root,
            over,
            recovery: wiring::recovery(),
        };
        let prepared = laboratory::prepare(&asked)?;
        let store = Arc::new(Store::open(&self.root, Arc::new(RealFs)).unwrap());
        let registry = wiring::registry_over(over, false).unwrap();
        let executor = wiring::runner(registry, store, over).unwrap();
        let running = Running {
            executor: &executor,
            root: &self.root,
            left_micros: left.unwrap_or(1_000_000),
            stop,
        };
        laboratory::round(&prepared, &running)
    }

    pub fn calls(&self) -> u64 {
        self.calls.load(Ordering::SeqCst)
    }

    pub fn runs(&self) -> String {
        whizbang(&["runs", "list"], &self.root)
            .out
            .lines()
            .next()
            .unwrap()
            .to_owned()
    }
}

fn never() -> bool {
    false
}

/// The counts of a report: started, completed, failed, interrupted, not run.
fn counts(report: &laboratory::Report) -> (usize, usize, usize, usize, usize) {
    (
        report.started,
        report.completed,
        report.failed,
        report.interrupted,
        report.not_run,
    )
}

#[test]
fn an_unchanged_cell_resumes_without_another_call() {
    let lab = ThreeImages::new("lab-resume", FILLED_AND_SOURCE);
    let over = lab.over(0);
    let report = lab.run(&over, None, &never).unwrap();
    assert_eq!((report.cells, report.handed_back), (6, 3));
    assert_eq!(counts(&report), (3, 3, 0, 0, 0));
    assert_eq!(lab.calls(), 3);
    let runs = lab.runs();
    // Nothing changed: nothing runs and nothing is sent again.
    let again = lab.run(&over, None, &never).unwrap();
    assert_eq!(counts(&again), (0, 3, 0, 0, 0));
    assert_eq!(lab.calls(), 3);
    assert_eq!(lab.runs(), runs);
}

#[test]
fn an_interrupted_round_resumes_and_a_failed_cell_is_kept() {
    let lab = ThreeImages::new("lab-interrupted", FILLED_AND_SOURCE);
    // The second fill fails, and the round is stopped after its second cell.
    let over = lab.over(2);
    let asked = std::sync::Mutex::new(0);
    let stop = || {
        let mut asked = asked.lock().unwrap();
        *asked += 1;
        *asked > 2
    };
    let report = lab.run(&over, None, &stop).unwrap();
    assert_eq!(counts(&report), (2, 1, 1, 0, 1));
    assert_eq!(lab.calls(), 2);
    // Run again, it resumes: only the cell not run runs; the failed one is kept as it
    // was, and is not run again, so no later success replaces it.
    let report = lab.run(&over, None, &never).unwrap();
    assert_eq!(counts(&report), (1, 2, 1, 0, 0));
    assert_eq!(lab.calls(), 3);
    assert_eq!(
        report.text(),
        "cells: 6\nhanded back by a reference arm: 3\nstarted now: 1\ncompleted: 2\n\
         failed: 1\ninterrupted: 0\nnot run: 0\n"
    );
}

#[test]
fn a_rounds_forecast_passing_the_allowance_left_is_refused_before_its_first_cell() {
    let lab = ThreeImages::new("lab-forecast", FILLED_AND_SOURCE);
    let over = lab.over(0);
    // Three fills at the filler's price: one millionth less is refused, and nothing runs.
    let refused = lab
        .run(&over, Some(3 * FILL_PRICE - 1), &never)
        .unwrap_err();
    assert_eq!(
        refused,
        format!(
            "the round's forecast for its 3 runs still to run is {} millionths of a dollar, \
             more than the {} the store's allowance has left; nothing was run",
            3 * FILL_PRICE,
            3 * FILL_PRICE - 1
        )
    );
    assert_eq!(lab.calls(), 0);
    assert_eq!(lab.runs(), "runs: 0");
}

#[test]
fn a_run_some_caller_made_under_a_cells_request_id_is_not_its_attempt() {
    use whizbang_core::contract::{Call, Executor, Request};
    let lab = ThreeImages::new("lab-forged", FILLED_AND_SOURCE);
    let over = lab.over(0);
    let asked = laboratory::Asked {
        matrix: &lab.matrix,
        corpus: &lab.corpus,
        root: &lab.root,
        over: &over,
        recovery: wiring::recovery(),
    };
    let prepared = laboratory::prepare(&asked).unwrap();
    let cell = prepared.cells.iter().find(|c| c.arm == "filled").unwrap();
    // A run of the cell's own recipe, of another request (another image's mask to keep),
    // under the id of the cell's first attempt, made past the doors: its action is the
    // cell's, its fingerprint is not.
    let store = Arc::new(Store::open(&lab.root, Arc::new(RealFs)).unwrap());
    let registry = wiring::registry_over(&over, false).unwrap();
    let executor = wiring::runner(registry, store, &over).unwrap();
    let id = harness::request_id(&prepared.scope, &cell.identity, 1).unwrap();
    let other = if cell.image == "a" {
        "b-keep"
    } else {
        "a-keep"
    };
    let forged = Call::new("fill_at")
        .input("image", prepared.pictures[&cell.image])
        .input("location", prepared.pictures[&format!("{}-at", cell.image)])
        .input("protect", prepared.pictures[other]);
    executor.request(&Request::new(forged).with_id(id)).unwrap();
    drop(executor);
    // Through the doors, such an id is refused before anything runs.
    let refused = whizbang(
        &[
            "rascal",
            "rect",
            "--image",
            &prepared.pictures[&cell.image].0.to_string(),
            "--box",
            "1 1 2 2",
            "--request-id",
            &format!("cell.{}.{}.1", prepared.scope, cell.identity),
        ],
        &lab.root,
    );
    assert_eq!(refused.code, Some(1), "{}", refused.out);
    assert!(
        refused.err.contains("reserved for the cells of a lab"),
        "{}",
        refused.err
    );
    // The round does not take the forged run for the cell's attempt: every cell runs,
    // the forged one's as its second attempt.
    let report = lab.run(&over, None, &never).unwrap();
    assert_eq!(counts(&report), (3, 3, 0, 0, 0));
    let mut requests = Vec::new();
    harness::each_run(&lab.root, &mut |record, _, _| {
        requests.extend(record.identity().request.clone());
    })
    .unwrap();
    let ours = format!("cell.{}.{}.", prepared.scope, cell.identity);
    let mut of_cell: Vec<&String> = requests.iter().filter(|r| r.starts_with(&ours)).collect();
    of_cell.sort();
    assert_eq!(of_cell, [&format!("{ours}1"), &format!("{ours}2")]);
}

#[test]
fn the_config_resolves_a_cells_backends_and_refuses_one_it_does_not_give_or_price() {
    let dir = Scratch::new("lab-resolve");
    let config = dir.path().join("whizbang.toml");
    let two = String::from(
        "[backends.fill.near]\nkind = \"counting\"\nurl = \"https://fill.example.org/v1\"\n\n\
         [backends.fill.far]\nkind = \"counting\"\nurl = \"https://fill.example.org/v2\"\n\n\
         [backends.fill.unpriced]\nkind = \"repaints-free\"\nurl = \"https://fill.example.org/v3\"\n\n\
         [egress.recipients.fill]\norigin = \"https://fill.example.org\"\n\
         classes = [\"source_images\", \"derived_images\"]\n",
    );
    std::fs::write(&config, two).unwrap();
    let mut kinds = wiring::kinds(&OneWorker::installed());
    let calls = Arc::new(AtomicU64::new(0));
    kinds
        .register(Arc::new(Counting { calls, fail_at: 0 }))
        .unwrap();
    kinds.register(Arc::new(Unpriced)).unwrap();
    let over = Over::nothing().config(Some(&config)).kinds(kinds);
    let resolve = laboratory::resolver(&over).unwrap();
    let near = resolve("fill", Some("near")).unwrap();
    assert_eq!(
        (
            near.name.as_str(),
            near.revision.as_str(),
            near.provider.as_str()
        ),
        ("eraser", "1", "counting")
    );
    assert_eq!(near.endpoint, "https://fill.example.org/v1");
    assert_eq!(near.price_micros, FILL_PRICE);
    // One model at two endpoints: two backends, whose cells are two.
    let far = resolve("fill", Some("far")).unwrap();
    assert_eq!(far.endpoint, "https://fill.example.org/v2");
    let protocol = protocol_text(["near", "far"], &["a"], 3);
    let arms = "[[arm]]\nname = \"near\"\nrecipe = \"seam\"\nfiller = \"near\"\n\n\
                [[arm]]\nname = \"far\"\nrecipe = \"seam\"\nfiller = \"far\"\n";
    let matrix = bare(&matrix_text("collar", "", arms));
    let (_, cells) = checked(&matrix, &protocol, &corpus_of(b"a mask"), &*resolve).unwrap();
    assert_ne!(identity(&cells, "near"), identity(&cells, "far"));
    // One the config does not give, and one that states no price: refused, not priced at
    // nothing.
    assert_eq!(
        resolve("fill", Some("elsewhere")).unwrap_err(),
        "the config gives no fill backend elsewhere; nothing was run"
    );
    assert_eq!(
        resolve("fill", Some("unpriced")).unwrap_err(),
        "the fill backend unpriced states no price, so a round's forecast cannot count it; \
         nothing was run"
    );
    // A provider named in the config is the provider.
    let named = Backend {
        provider: "other".to_owned(),
        ..near.clone()
    };
    let mut components = cells[0].components.clone();
    let before = components.identity();
    components.filler = Some(named);
    assert_ne!(components.identity(), before);
}

/// A remote filler that states no price.
struct Unpriced;

impl Kind for Unpriced {
    fn name(&self) -> &str {
        "repaints-free"
    }

    fn make(&self, table: &Table<'_>) -> Result<Made, KindRefused> {
        let model = ModelIdentity::new("unpriced", "eraser", "1");
        let address = table.backend().address().expect("a url");
        let declaration = Declaration::new(model, &[])
            .unwrap()
            .sending_to(Recipient::new(address.origin().clone()));
        let filler = FakeFiller::new(Paint::Inverse).declared_as(declaration);
        Ok(Made::Fill(Arc::new(filler)))
    }
}

#[test]
fn a_recipe_needing_more_than_a_filler_and_a_finder_and_two_alike_arms_are_refused() {
    let protocol = protocol_text(["wide", "same"], &["a"], 3);
    let arms = "[[arm]]\nname = \"wide\"\nrecipe = \"seam\"\nfiller = \"fal\"\n\n\
                [[arm]]\nname = \"same\"\nrecipe = \"seam\"\nfiller = \"fal\"\n";
    let matrix = bare(&matrix_text("collar", "", arms));
    let refused = checked(&matrix, &protocol, &corpus_of(b"a mask"), &backends("1", 0));
    assert_eq!(
        refused.unwrap_err(),
        "arm wide and arm same make the same cell of image a: two arms of one matrix differ \
         in what they run; nothing was run"
    );
    // A recipe that needs a judge would be forecast at nothing: refused.
    let registry = wiring::registry().unwrap();
    let load = |name: &str| laboratory::recipe(&registry, name);
    let (protocol, digest) =
        Protocol::parse(protocol_text(["wide", "clean"], &["a"], 3).as_bytes()).unwrap();
    let matrix = matrix_text("collar", "", "[[arm]]\nname = \"wide\"\nrecipe = \"seam\"\n\n[[arm]]\nname = \"clean\"\nreference = \"clean\"\n")
        .replace("protect = \"a-protect\", ", "")
        .replace("texts = { prompt = \"collar\" }\n", "");
    let corpus = Corpus::parse(&corpus_of(b"a mask")).unwrap();
    let mut validated = Matrix::parse(&matrix)
        .unwrap()
        .validate(&protocol, &digest, &corpus, &registry, &load)
        .unwrap();
    let whizbang_eval::lab::matrix::Checks::Recipe(recipe) = &mut validated.arms[0] else {
        panic!("a recipe arm")
    };
    recipe.needs.push("judge".to_owned());
    let refused = cell::cells(
        &validated,
        &registry,
        &load,
        &backends("1", 0),
        &laboratory::constants,
    );
    assert_eq!(
        refused.unwrap_err(),
        "arm wide: its recipe seam needs judge, which a lab's cell cannot forecast: a lab's \
         recipe needs a filler and a finder at most; nothing was run"
    );
}

#[test]
fn a_lab_file_that_cannot_be_read_says_where_in_fixed_words_and_names_no_full_path() {
    // The reader's own words could quote the owner's line: the place alone is said.
    let corpus = "version = 1\n\n[[image]]\nid = \"a\"\nprivate-note = \"the owner's words\"\n";
    let refused = Corpus::parse(corpus).unwrap_err();
    assert_eq!(
        refused,
        "the corpus is not of the form this build reads, at line 5, column 1"
    );
    let refused = Protocol::parse(b"version = 1\nname = \"the owner's words").unwrap_err();
    assert!(
        refused.starts_with("the protocol is not of the form this build reads, at line 2"),
        "{refused}"
    );
    assert!(!refused.contains("owner"), "{refused}");
    let refused = Matrix::parse("version = \"the owner's words\"\n").unwrap_err();
    assert_eq!(
        refused,
        "the matrix is not of the form this build reads, at line 1, column 11"
    );
    // A picture that cannot be read is named under its root.
    let dir = Scratch::new("lab-unread");
    let pictures = dir.path().join("pictures");
    std::fs::create_dir_all(pictures.join("a.png")).unwrap();
    let manifest = dir.path().join("corpus.toml");
    std::fs::write(&manifest, corpus_text(&[("a", b"a picture", Some("all"))])).unwrap();
    let refused = laboratory::verify(&manifest, &pictures).unwrap_err();
    assert_eq!(refused, "picture a: a.png is not a regular file");
    let full = dir.path().display().to_string();
    assert!(!refused.contains(&full), "{refused}");
}

// --- the decode ---------------------------------------------------------------------------------

/// A protocol of two groups, `sky` of six images and `structured` of three, with these
/// `min_decided` and rules.
fn decode_protocol(min_decided: u32, rules: &str) -> (Protocol, String) {
    let text = format!(
        "version = 1\nname = \"t\"\nseed = 42\narms = [\"a\", \"b\"]\nmin_decided = {min_decided}\n{rules}\
         [[group]]\nname = \"sky\"\nimages = [\"s1\", \"s2\", \"s3\", \"s4\", \"s5\", \"s6\"]\n\
         [[group]]\nname = \"structured\"\nimages = [\"t1\", \"t2\", \"t3\"]\n"
    );
    Protocol::parse(text.as_bytes()).unwrap()
}

/// An assignment with arm `a` on the left of every pair.
fn all_left(protocol: &Protocol, digest: &str) -> Assignment {
    let shown = |cell: String| Shown { cell, attempt: 1 };
    Assignment {
        assignment_version: 1,
        lab: protocol.name.clone(),
        protocol_digest: digest.to_owned(),
        arms: protocol.arms.clone(),
        seed: protocol.seed,
        pair: (protocol.group.iter())
            .flat_map(|g| {
                g.images.iter().map(|id| Pair {
                    id: id.clone(),
                    group: g.name.clone(),
                    left: shown(format!("a-{id}")),
                    right: shown(format!("b-{id}")),
                    a_side: Side::Left,
                })
            })
            .collect(),
    }
}

const ALL: [&str; 9] = ["s1", "s2", "s3", "s4", "s5", "s6", "t1", "t2", "t3"];

/// The picks for the nine pairs, in order.
fn picks_of(picked: [Picked; 9]) -> Picks {
    Picks {
        picks_version: 1,
        assignment_digest: "sha256:assignment".to_owned(),
        pick: (ALL.iter().zip(picked))
            .map(|(pair, pick)| decode::Pick {
                pair: (*pair).to_owned(),
                pick,
            })
            .collect(),
    }
}

/// The groups' outcomes and counts (decided, wins, losses, same, neither) of these picks.
fn decoded(min_decided: u32, rules: &str, picked: [Picked; 9]) -> Vec<(Outcome, [u32; 5])> {
    let (protocol, digest) = decode_protocol(min_decided, rules);
    let assignment = all_left(&protocol, &digest);
    let done = decode::decode(
        &protocol,
        &digest,
        &assignment,
        "sha256:assignment",
        &picks_of(picked),
    );
    (done.unwrap().groups.iter())
        .map(|g| (g.outcome, [g.decided, g.wins, g.losses, g.same, g.neither]))
        .collect()
}

use Picked::{Left as L, Neither as N, Right as R, Same as S};

#[test]
fn a_group_below_min_decided_is_inconclusive() {
    // Sky: six decided, all wins; structured: two decided under three, and a same.
    let groups = decoded(3, "", [L, L, L, L, L, L, L, R, S]);
    assert_eq!(groups[0].0, Outcome::Preferred);
    assert_eq!(groups[1], (Outcome::Inconclusive, [2, 1, 1, 1, 0]));
    // Same and neither are not decided: three of them leave a group inconclusive.
    let groups = decoded(1, "", [L, L, L, L, L, L, S, N, N]);
    assert_eq!(groups[1], (Outcome::Inconclusive, [0, 0, 0, 1, 2]));
}

#[test]
fn a_group_at_the_preferred_rule_is_preferred_and_one_below_it_is_not() {
    // Four wins of six decided is two thirds exactly: preferred.
    let groups = decoded(3, "", [L, L, L, L, R, R, L, L, L]);
    assert_eq!(groups[0], (Outcome::Preferred, [6, 4, 2, 0, 0]));
    // Three of five decided is below it, and two losses of five is not below a third.
    let groups = decoded(3, "", [L, L, L, R, R, N, L, L, L]);
    assert_eq!(groups[0], (Outcome::Worse, [5, 3, 2, 0, 1]));
}

#[test]
fn a_group_at_the_not_worse_rule_is_not_worse_and_one_past_it_is_worse() {
    // Wins at four fifths, under a preferred rule of five sixths, and losses at one in
    // five, below a third: not worse.
    let rules = "[rules]\npreferred = [5, 6]\nnot_worse = [1, 3]\n";
    let groups = decoded(3, rules, [L, L, L, L, R, S, L, L, L]);
    assert_eq!(groups[0], (Outcome::NotWorse, [5, 4, 1, 1, 0]));
    // Losses at two of six, a third exactly, is not below it: worse.
    let groups = decoded(3, rules, [L, L, L, L, R, R, L, L, L]);
    assert_eq!(groups[0], (Outcome::Worse, [6, 4, 2, 0, 0]));
}

#[test]
fn the_decode_gives_rascals_outcomes_on_rascals_picks() {
    // Rascal's cases (`crates/rascal-core/tests/it/lab_decode.rs` at the pinned commit),
    // with arm a on the left of every pair: the sky group's outcome and counts, then the
    // structured group's outcome.
    type Case = ([Picked; 9], (Outcome, [u32; 5]), Outcome);
    let cases: [Case; 4] = [
        (
            [L, L, L, L, R, R, L, R, S],
            (Outcome::Preferred, [6, 4, 2, 0, 0]),
            Outcome::Inconclusive,
        ),
        (
            [L, L, L, R, R, R, L, L, L],
            (Outcome::Worse, [6, 3, 3, 0, 0]),
            Outcome::Preferred,
        ),
        (
            [L, L, L, R, S, S, N, N, N],
            (Outcome::Preferred, [4, 3, 1, 2, 0]),
            Outcome::Inconclusive,
        ),
        (
            [L, L, L, R, R, N, L, L, R],
            (Outcome::Worse, [5, 3, 2, 0, 1]),
            Outcome::Preferred,
        ),
    ];
    for (picked, sky, structured) in cases {
        let groups = decoded(3, "", picked);
        assert_eq!(groups[0], sky, "{picked:?}");
        assert_eq!(groups[1].0, structured, "{picked:?}");
    }
    // Rascal's gap: seven decided, five wins (below three quarters), two losses (below a
    // third): not worse.
    let (protocol, digest) = Protocol::parse(
        b"version = 1\nname = \"t\"\nseed = 1\narms = [\"a\", \"b\"]\nmin_decided = 3\n[rules]\n\
          preferred = [3, 4]\nnot_worse = [1, 3]\n[[group]]\nname = \"g\"\n\
          images = [\"i1\", \"i2\", \"i3\", \"i4\", \"i5\", \"i6\", \"i7\"]\n",
    )
    .unwrap();
    let assignment = all_left(&protocol, &digest);
    let picks = Picks {
        picks_version: 1,
        assignment_digest: "sha256:assignment".to_owned(),
        pick: (1..=7)
            .map(|i| decode::Pick {
                pair: format!("i{i}"),
                pick: if i <= 5 { L } else { R },
            })
            .collect(),
    };
    let done =
        decode::decode(&protocol, &digest, &assignment, "sha256:assignment", &picks).unwrap();
    assert_eq!(done.groups[0].outcome, Outcome::NotWorse);
    assert_eq!(
        done.text(),
        "arms: a against b\ngroup g: pairs 7, decided 7, wins 5, losses 2, same 0, neither 0: \
         not_worse\n"
    );
    // Under the default fractions not_worse alone cannot be reached: wins and losses sum
    // to the decided pairs.
    let (protocol, _) = decode_protocol(1, "");
    for wins in 0..=20_u32 {
        for losses in 0..=(20 - wins) {
            let outcome = decode::outcome(&protocol, wins + losses, wins, losses);
            assert_ne!(outcome, Outcome::NotWorse, "{wins} wins, {losses} losses");
        }
    }
}
