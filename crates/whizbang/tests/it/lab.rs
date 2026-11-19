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
        harness::run(work, &harness::by_request(scope), &running, 1, "round")
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
