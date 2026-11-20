//! Recipes through the binary's wiring (plan m3, task R1, items 5 and 6; ADR 0033): the
//! shipped `remove_prompt` and a user's own recipe file listed under `[recipes] paths`,
//! each run over a kind of the test's own whose segmenter is the test kit's fake, on a
//! picture of 64 by 64 brought in by `whizbang store import --picture`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use whizbang::door::{self, OperationAsked};
use whizbang::pack::OneWorker;
use whizbang::wiring::{self, Over};
use whizbang_core::capability::{Declaration, Price, feature};
use whizbang_core::config::{Kind, KindRefused, Table};
use whizbang_core::contract::record::stored;
use whizbang_core::contract::{Capability, Made, ModelIdentity, RunRecord, Value};
use whizbang_core::egress::Recipient;
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Reader, Store};
use whizbang_testkit::capabilities::{FakeFiller, FakeSegmenter, Paint};
use whizbang_testkit::scratch::Scratch;

use crate::wired::whizbang;

/// What the fake finds for `collar` in the picture.
pub(crate) const COLLAR: [[u32; 4]; 2] = [[10, 10, 20, 18], [40, 30, 50, 36]];

/// The config whose one segmenter is of the kind [`Small`].
pub(crate) const CONFIG: &str = "[backends.segment]\nkind = \"small\"\nurl = \"https://segment.example.org/v1\"\n\n\
     [egress.recipients.segment]\norigin = \"https://segment.example.org\"\nclasses = [\"prompts\"]\n";

/// [`CONFIG`] with a filler of the kind [`Small`] too, its recipient approved for the
/// classes a fill sends.
pub(crate) const FILLED: &str = "[backends.segment]\nkind = \"small\"\nurl = \"https://segment.example.org/v1\"\n\n\
     [backends.fill]\nkind = \"small\"\nurl = \"https://fill.example.org/v1\"\n\n\
     [egress.recipients.segment]\norigin = \"https://segment.example.org\"\nclasses = [\"prompts\"]\n\n\
     [egress.recipients.fill]\norigin = \"https://fill.example.org\"\nclasses = [\"source_images\", \"derived_images\"]\n";

/// The colour the filler of the kind [`Small`] paints where the support is.
pub(crate) const PAINT: [u8; 3] = [250, 10, 10];

/// A remote kind at an address of the reserved `.example` domain, which nothing
/// contacts: its segmenter is the fake, finding [`COLLAR`] for `collar`, and its filler
/// the fake that paints [`PAINT`] over the support and nothing else.
struct Small;

impl Kind for Small {
    fn name(&self) -> &str {
        "small"
    }

    fn make(&self, table: &Table<'_>) -> Result<Made, KindRefused> {
        let model = ModelIdentity::new("small", "m", "1");
        let address = table.backend().address().expect("a url");
        let sent = |declaration: Declaration| {
            declaration
                .sending_to(Recipient::new(address.origin().clone()))
                .priced(Price::at_most(1000.0).unwrap())
        };
        if table.capability() == Capability::Fill {
            let declaration = sent(Declaration::new(model, &[]).unwrap());
            let filler = FakeFiller::new(Paint::Support(PAINT)).declared_as(declaration);
            return Ok(Made::Fill(Arc::new(filler)));
        }
        let features = [feature::PHRASE, feature::POINT, feature::BOX];
        let declaration = sent(Declaration::new(model, &features).unwrap());
        let mut fake = FakeSegmenter::new().declared_as(declaration);
        for bounds in COLLAR {
            fake = fake.finding("collar", bounds);
        }
        Ok(Made::Segment(Arc::new(fake)))
    }
}

/// A store with a picture of 64 by 64 in it: the store's path and the image's identity.
pub(crate) fn with_small_picture(dir: &Scratch) -> (PathBuf, String) {
    let mut rgba = Vec::new();
    for y in 0..64_u32 {
        for x in 0..64_u32 {
            rgba.extend_from_slice(&[(x * 4) as u8, (y * 4) as u8, 90, 255]);
        }
    }
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(&rgba, 64, 64, ExtendedColorType::Rgba8)
        .unwrap();
    let file = dir.path().join("small.png");
    std::fs::write(&file, png).unwrap();
    let root = dir.path().join("store");
    let imported = whizbang(
        &[
            "store",
            "import",
            "--picture",
            file.to_str().unwrap(),
            "--create",
        ],
        &root,
    );
    assert_eq!(imported.code, Some(0), "{}", imported.err);
    let id = imported.out.lines().last().unwrap();
    let id = id
        .strip_prefix("imported image ")
        .expect("an image")
        .to_owned();
    // The allowance the admission of a call to the remote fake reserves from.
    let reset = whizbang(&["spend", "reset", "--allowance", "1"], &root);
    assert_eq!(reset.code, Some(0), "{}", reset.err);
    (root, id)
}

/// The over of a command under this config text, with the kind [`Small`].
pub(crate) fn over(dir: &Scratch, text: &str) -> Over {
    let config = dir.path().join("whizbang.toml");
    std::fs::write(&config, text).unwrap();
    let mut kinds = wiring::kinds(&OneWorker::installed());
    kinds.register(Arc::new(Small)).unwrap();
    Over::nothing().config(Some(&config)).kinds(kinds)
}

/// Runs a recipe as a command does, through the door, over this config.
pub(crate) fn run(over: &Over, root: &Path, asked: &OperationAsked<'_>) -> Result<String, String> {
    let store = Arc::new(Store::open(root, Arc::new(RealFs)).unwrap());
    let registry = wiring::registry_over(over, false)?;
    let executor = wiring::runner(registry, store, over)?;
    door::recipe(&executor, asked, root)
}

/// A run's record, by a reader of the test's own.
pub(crate) fn record(root: &Path, run: &str) -> RunRecord {
    let reader = Reader::open(root, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    stored(&generation, run, |entry| reader.read(entry))
        .unwrap()
        .unwrap()
}

/// A user's recipe: the instances of a phrase, selected and grown.
const OUTLINE: &str = r#"version = 1
name = "outline"
purpose = "Grows what a phrase names by a margin."
use = "In a test of a user's own recipe."

[inputs]
image = { type = "Image" }
prompt = { type = "Text" }

[[step]]
id = "found"
op = "segment"
inputs = { image = "inputs.image" }
query = "inputs.prompt"

[[step]]
id = "kept"
op = "select"
inputs = { mask = "found.mask" }

[[step]]
id = "grown"
op = "grow"
inputs = { mask = "kept.mask" }
px = 2

[outputs]
mask = "grown.mask"
"#;

#[test]
fn a_users_recipe_from_config_runs_and_its_digest_is_in_the_run() {
    let dir = Scratch::new("recipe-users");
    let (root, image) = with_small_picture(&dir);
    let file = dir.path().join("outline.toml");
    std::fs::write(&file, OUTLINE).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    let listed = format!("{CONFIG}\n[recipes]\npaths = [\"{}\"]\n", file.display());
    let over = over(&dir, &listed);
    let asked = OperationAsked {
        action: "outline",
        arguments: vec![("prompt", Value::text("collar"))],
        inputs: vec![("image", "--image", image.as_str())],
        request_id: None,
    };
    let out = run(&over, &root, &asked).unwrap();
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 5, "{out}");
    let run_id = lines[0]
        .strip_prefix("run ")
        .unwrap()
        .strip_suffix(" completed")
        .unwrap();
    for (line, (id, action)) in
        lines[1..4]
            .iter()
            .zip([("found", "segment"), ("kept", "select"), ("grown", "grow")])
    {
        assert!(
            line.starts_with(&format!("step {id} {action} run ")),
            "{line}"
        );
    }
    assert!(lines[4].starts_with("mask sha256:"), "{out}");
    // The run pins the file it ran: the digest of its text is a setting of the run.
    let stored = record(&root, run_id);
    let digest = Digest::of_bytes(OUTLINE.as_bytes()).to_string();
    let setting = (stored.asked().settings.iter())
        .find(|setting| setting.name() == "recipe.digest")
        .expect("the recipe's digest");
    assert_eq!(
        serde_json::to_string(setting.value()).unwrap(),
        format!(r#"{{"text":"{digest}"}}"#)
    );

    // A recipe file that names no action of the build is refused, and nothing is built.
    std::fs::write(&file, OUTLINE.replacen("op = \"grow\"", "op = \"heal\"", 1)).unwrap();
    let refused = wiring::registry_over(&over, false).err().unwrap();
    assert_eq!(
        refused,
        "the config cannot be used: a recipe file is not one this build runs: the recipe \
         outline, step grown: heal is no action of this build"
    );
    // One its group or others can write is not run, and its path is not said.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(&file, OUTLINE).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o666)).unwrap();
        let refused = wiring::registry_over(&over, false).err().unwrap();
        assert_eq!(
            refused,
            "the config cannot be used: the recipe file outline.toml: can be written by its \
             group or others, and is not run"
        );
    }
    // A path that is not absolute is refused as the config is read.
    let relative = over_text_refused(
        &dir,
        &format!("{CONFIG}\n[recipes]\npaths = [\"outline.toml\"]\n"),
    );
    assert!(relative.contains("recipes.paths.0"), "{relative}");
}

fn over_text_refused(dir: &Scratch, text: &str) -> String {
    wiring::registry_over(&over(dir, text), false)
        .err()
        .unwrap()
}

#[test]
fn rascal_run_refuses_a_link_and_a_file_over_the_limit_before_reading_it() {
    let dir = Scratch::new("recipe-run-file");
    let root = dir.path().join("store");
    let file = dir.path().join("outline.toml");
    std::fs::write(&file, OUTLINE).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        let link = dir.path().join("linked.toml");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        let ran = whizbang(&["rascal", "run", link.to_str().unwrap()], &root);
        assert_eq!(ran.code, Some(1), "{}", ran.err);
        assert!(
            ran.err
                .contains("the recipe file linked.toml: is a link or not a file"),
            "{}",
            ran.err
        );
        // A device is no file, and nothing of it is read.
        let ran = whizbang(&["rascal", "run", "/dev/zero"], &root);
        assert_eq!(ran.code, Some(1), "{}", ran.err);
        assert!(ran.err.contains("is a link or not a file"), "{}", ran.err);
    }
    let big = dir.path().join("big.toml");
    std::fs::write(&big, vec![b'#'; whizbang_core::recipe::FILE_MAX_BYTES + 1]).unwrap();
    let ran = whizbang(&["rascal", "run", big.to_str().unwrap()], &root);
    assert_eq!(ran.code, Some(1), "{}", ran.err);
    assert!(
        ran.err
            .contains("the recipe file big.toml: is over 65536 bytes"),
        "{}",
        ran.err
    );
}

#[test]
fn a_recipe_that_names_a_loop_as_a_step_is_refused_when_read() {
    // Plan m4, task L1, item 13: a user's recipe whose step is the loop. It is refused
    // when the config is read, in words, before any store is opened or anything is paid.
    let looping = r#"version = 1
name = "twice_checked"
purpose = "Removes what a phrase names, checked."
use = "In a test of a recipe that names a loop."

[inputs]
image = { type = "Image" }
prompt = { type = "Text" }

[[step]]
id = "removed"
op = "remove_checked"
inputs = { image = "inputs.image" }
prompt = "inputs.prompt"

[outputs]
image = "removed.image"
"#;
    let dir = Scratch::new("recipe-names-a-loop");
    let file = dir.path().join("twice_checked.toml");
    std::fs::write(&file, looping).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    let listed = format!("{CONFIG}\n[recipes]\npaths = [\"{}\"]\n", file.display());
    let refused = over_text_refused(&dir, &listed);
    assert_eq!(
        refused,
        "the config cannot be used: a recipe file is not one this build runs: the recipe \
         twice_checked, step removed: remove_checked is a loop, and a recipe may not name a \
         loop as a step: calls nest four levels, a loop's own calls use them all, and nothing \
         calls a loop"
    );
    // The same from `rascal run`, which reads a recipe file against the same actions.
    let registry = wiring::registry().unwrap();
    let read = whizbang_core::recipe::Recipe::read(looping).unwrap();
    let error = read.check(&registry).err().unwrap();
    assert!(error.why.starts_with("remove_checked is a loop"), "{error}");
}
