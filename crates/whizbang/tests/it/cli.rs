//! `whizbang rascal segment` (plan 2A, task R1): what the command prints, through the
//! binary's wiring and its door, over a kind of the test's own whose segmenter is the
//! test kit's fake, since the binary ships no fake model
//! (`pack::the_shipped_binary_offers_no_fake_model_and_starts_no_entry_point_that_takes_a_fault`);
//! and the real binary, through the test kit's launcher, for what it says of the run
//! and when no segmenter is configured. The picture is brought into the store by
//! `whizbang store import --picture`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use whizbang::door::{self, SegmentAsked};
use whizbang::pack::OneWorker;
use whizbang::wiring::{self, Over};
use whizbang_core::artifact::{Image, ImageRole, Origin, Step};
use whizbang_core::capability::{Declaration, Price, feature};
use whizbang_core::config::{Kind, KindRefused, Table};
use whizbang_core::contract::{Made, ModelIdentity};
use whizbang_core::egress::Recipient;
use whizbang_core::identity::{CanonicalPixels, DecodeLimits, Projection, Topology, identify};
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::capabilities::FakeSegmenter;
use whizbang_testkit::scratch::Scratch;

use crate::model_tier::FIXTURE;
use crate::wired::{has_line, whizbang, whizbang_storeless};

/// The boxes the fake finds for `circle` in the fixture: around the disc, around its
/// upper half, and around the square.
const FOUND: [[u32; 4]; 3] = [
    [330, 110, 510, 290],
    [330, 110, 510, 200],
    [80, 280, 240, 440],
];

pub(crate) const CONFIG: &str = "[backends.segment]\nkind = \"finding\"\nurl = \"https://segment.example.org/v1\"\n\n\
     [egress.recipients.segment]\norigin = \"https://segment.example.org\"\nclasses = [\"prompts\"]\n";

/// A kind whose one segmenter is the test kit's fake, declaring the feature `phrase`
/// and finding [`FOUND`] for `circle`. It is a remote backend's kind, at an address of
/// the reserved `.example` domain, which nothing contacts: a local one is a model of
/// the catalogue.
struct Finding;

impl Kind for Finding {
    fn name(&self) -> &str {
        "finding"
    }

    fn make(&self, table: &Table<'_>) -> Result<Made, KindRefused> {
        let model = ModelIdentity::new("finding", "m", "1");
        let declaration = Declaration::new(model, &[feature::PHRASE]).unwrap();
        // It sends nothing, and says where it would and what it may cost, as a remote
        // backend's declaration does.
        let address = table.backend().address().expect("a url");
        let declaration = declaration
            .sending_to(Recipient::new(address.origin().clone()))
            .priced(Price::at_most(1000.0).unwrap());
        let mut fake = FakeSegmenter::new().declared_as(declaration);
        for bounds in FOUND {
            fake = fake.finding("circle", bounds);
        }
        Ok(Made::Segment(Arc::new(fake)))
    }
}

/// A store with the fixture picture in it, brought in by `whizbang store import
/// --picture`: the store's path and the image's identity as the import printed it.
pub(crate) fn with_picture(dir: &Scratch) -> (PathBuf, String) {
    let file = dir.path().join("circle.png");
    std::fs::write(&file, FIXTURE).unwrap();
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
    let line = imported.out.lines().last().unwrap();
    let id = line.strip_prefix("imported image ").expect("an image");
    assert!(id.starts_with("sha256:"), "{id}");
    // The owner's allowance, which the admission of a call to a remote backend
    // reserves the bound of its price from: the fake states a thousandth of a dollar,
    // and is paid nothing.
    let reset = whizbang(&["spend", "reset", "--allowance", "1"], &root);
    assert_eq!(reset.code, Some(0), "{}", reset.err);
    (root, id.to_owned())
}

/// The command's run, as `whizbang rascal segment` makes it, over a config whose one
/// segment backend is of the kind [`Finding`], at a recipient the config approves for
/// prompts.
fn segment(dir: &Scratch, root: &Path, asked: &SegmentAsked<'_>) -> Result<String, String> {
    segment_under(dir, root, asked, CONFIG)
}

/// The same, under this config, with no env file.
pub(crate) fn segment_under(
    dir: &Scratch,
    root: &Path,
    asked: &SegmentAsked<'_>,
    text: &str,
) -> Result<String, String> {
    let config = dir.path().join("whizbang.toml");
    std::fs::write(&config, text).unwrap();
    let mut kinds = wiring::kinds(&OneWorker::installed());
    kinds.register(Arc::new(Finding)).unwrap();
    let over = Over::nothing().config(Some(&config)).kinds(kinds);
    let store = Arc::new(Store::open(root, Arc::new(RealFs)).unwrap());
    let executor = wiring::runner(wiring::registry().unwrap(), store, &over).unwrap();
    door::segment(&executor, asked, &|run, name| {
        door::diagnostic(root, run, name)
    })
}

#[test]
fn rascal_segment_prints_each_masks_identity_and_score_under_a_fake_segmenter() {
    let dir = Scratch::new("cli-segment");
    let (root, image) = with_picture(&dir);
    let asked = SegmentAsked {
        image: &image,
        phrase: "circle",
        max_instances: 8,
        backend: None,
        request_id: Some("first-cut"),
    };
    let out = segment(&dir, &root, &asked).unwrap();
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 4, "{out}");
    let run = lines[0].strip_prefix("run ").unwrap();
    let run = run.strip_suffix(" completed").expect("completed");
    // Each mask's identity, then the score and the box the run's diagnostic gives it,
    // in the order the segmenter answered.
    let mut masks = Vec::new();
    for (line, (bounds, score)) in lines[1..]
        .iter()
        .zip(FOUND.iter().zip(["0.9", "0.8", "0.7"]))
    {
        let words: Vec<&str> = line.split(' ').collect();
        assert_eq!(words.len(), 9, "{line}");
        assert_eq!(
            (words[0], words[2], words[3], words[4]),
            ("mask", "score", score, "box")
        );
        let edges: Vec<String> = bounds.iter().map(u32::to_string).collect();
        assert_eq!(
            words[5..],
            edges.iter().map(String::as_str).collect::<Vec<_>>()[..]
        );
        assert!(words[1].starts_with("sha256:"));
        masks.push(words[1].to_owned());
    }
    // `whizbang runs show` names the diagnostic, and prints it.
    let shown = whizbang(&["runs", "show", run], &root);
    assert_eq!(shown.code, Some(0), "{}", shown.err);
    assert!(
        shown.out.contains("diagnostics/segment.txt"),
        "{}",
        shown.out
    );
    let said = whizbang(
        &[
            "runs",
            "show",
            run,
            "--diagnostic",
            "diagnostics/segment.txt",
        ],
        &root,
    );
    assert_eq!(said.code, Some(0), "{}", said.err);
    has_line(&said.out, "instances 3");
    for (mask, line) in masks.iter().zip(&lines[1..]) {
        has_line(&said.out, line);
        assert!(line.contains(mask.as_str()));
    }
    // The same request again is answered with the first run, and nothing runs again.
    let again = segment(&dir, &root, &asked).unwrap();
    assert_eq!(
        again,
        out.replacen(
            " completed",
            " (the request was sent before, and nothing ran again) completed",
            1
        )
    );
    // A phrase it finds nothing for completes, and says so.
    let nothing = SegmentAsked {
        phrase: "a dog",
        request_id: None,
        ..asked
    };
    let out = segment(&dir, &root, &nothing).unwrap();
    assert!(
        out.ends_with(" completed\nno instance was found\n"),
        "{out}"
    );
}

#[test]
fn rascal_segment_with_no_segmenter_configured_exits_1_and_commits_nothing() {
    let dir = Scratch::new("cli-segment-none");
    let (root, image) = with_picture(&dir);
    let ran = whizbang(
        &["rascal", "segment", "--image", &image, "--phrase", "circle"],
        &root,
    );
    assert_eq!(ran.code, Some(1), "{} {}", ran.out, ran.err);
    assert!(ran.err.contains("segment"), "{}", ran.err);
    let runs = whizbang(&["runs", "list"], &root);
    assert!(runs.out.starts_with("runs: 0\n"), "{}", runs.out);
    // What is not an identity is said so, and what was typed is not said back.
    let ran = whizbang(
        &[
            "rascal",
            "segment",
            "--image",
            "not-an-id",
            "--phrase",
            "circle",
        ],
        &root,
    );
    assert_eq!(ran.code, Some(1));
    assert!(
        ran.err.contains("--image is no artifact identity"),
        "{}",
        ran.err
    );
    assert!(!ran.err.contains("not-an-id"), "{}", ran.err);
    // The most instances is from 1 to 64, refused by the command line itself.
    let ran = whizbang(
        &[
            "rascal",
            "segment",
            "--image",
            &image,
            "--phrase",
            "circle",
            "--max-instances",
            "65",
        ],
        &root,
    );
    assert_eq!(ran.code, Some(2));
}

// --- the operations, and the PNG out (plan 2A, task R2) ----------------------------------------

/// The chunk types of a PNG file, in order.
fn chunks(png: &[u8]) -> Vec<String> {
    assert_eq!(png[..8], *b"\x89PNG\r\n\x1a\n");
    let mut at = 8;
    let mut found = Vec::new();
    while at + 8 <= png.len() {
        let length = u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
        found.push(String::from_utf8(png[at + 4..at + 8].to_vec()).unwrap());
        at += 12 + length;
    }
    assert_eq!(at, png.len());
    found
}

/// The pixels of a PNG or JPEG file, as the core reads them.
fn pixels_of(file: &[u8]) -> CanonicalPixels {
    identify(file, &DecodeLimits::default()).unwrap().pixels
}

/// The one output a command printed, by its port.
fn printed(out: &str, port: &str) -> String {
    let line = out
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{port} ")));
    line.unwrap_or_else(|| panic!("no {port} in:\n{out}"))
        .to_owned()
}

#[test]
fn rascal_select_plug_feather_render_and_export_make_a_cut_out_of_the_instances() {
    let dir = Scratch::new("cli-cutout");
    let (root, image) = with_picture(&dir);
    let asked = SegmentAsked {
        image: &image,
        phrase: "circle",
        max_instances: 8,
        backend: None,
        request_id: None,
    };
    let found = segment(&dir, &root, &asked).unwrap();
    let masks: Vec<&str> = found
        .lines()
        .filter_map(|line| line.strip_prefix("mask "))
        .map(|rest| rest.split(' ').next().unwrap())
        .collect();
    assert_eq!(masks.len(), 3, "{found}");
    // Keep the disc and its upper half, not the square; clear a box at the disc's top.
    let ran = |args: &[&str]| {
        let ran = whizbang(args, &root);
        assert_eq!(ran.code, Some(0), "{args:?}: {}", ran.err);
        ran.out
    };
    let selected = ran(&[
        "rascal",
        "select",
        "--mask",
        masks[0],
        "--mask",
        masks[1],
        "--exclude",
        "330 110 510 120",
    ]);
    let selected = printed(&selected, "mask");
    let plugged = printed(&ran(&["rascal", "plug", "--mask", &selected]), "mask");
    let feathered = printed(
        &ran(&["rascal", "feather", "--mask", &plugged, "--sigma-px", "2"]),
        "mask",
    );
    let out = ran(&[
        "rascal",
        "render",
        "--mask",
        &feathered,
        "--profile",
        "alpha",
        "--image",
        &image,
    ]);
    let cut = printed(&out, "image");
    let file = dir.path().join("cut.png");
    let exported = ran(&[
        "store",
        "export",
        "--image",
        &cut,
        "--out",
        file.to_str().unwrap(),
    ]);
    assert!(
        exported.starts_with(&format!("exported image {cut} ")),
        "{exported}"
    );
    let png = std::fs::read(&file).unwrap();
    assert_eq!(chunks(&png), ["IHDR", "IDAT", "IEND"]);
    let shown = pixels_of(&png);
    let picture = pixels_of(FIXTURE);
    assert_eq!(
        (shown.width(), shown.height()),
        (picture.width(), picture.height())
    );
    // The colours are the picture's; the alpha is clear outside the instances, where the
    // feather does not reach, and opaque at the disc's middle, under the box it is not.
    let at = |x: usize, y: usize| (y * picture.width() as usize + x) * 4;
    for (a, b) in shown.rgba().chunks(4).zip(picture.rgba().chunks(4)) {
        assert_eq!(a[..3], b[..3]);
    }
    assert_eq!(shown.rgba()[at(5, 5) + 3], 0);
    assert_eq!(shown.rgba()[at(160, 360) + 3], 0, "the square was not kept");
    assert_eq!(shown.rgba()[at(420, 200) + 3], 255);
    assert_eq!(shown.rgba()[at(420, 111) + 3], 0, "the box is cleared");
    // A plain rendering is the weight in grey.
    let plain = printed(
        &ran(&[
            "rascal",
            "render",
            "--mask",
            &feathered,
            "--profile",
            "plain",
        ]),
        "image",
    );
    let grey = dir.path().join("grey.png");
    ran(&[
        "store",
        "export",
        "--image",
        &plain,
        "--out",
        grey.to_str().unwrap(),
    ]);
    let grey = pixels_of(&std::fs::read(&grey).unwrap());
    assert_eq!(
        grey.rgba()[at(420, 200)..at(420, 200) + 4],
        [255, 255, 255, 255]
    );
    assert_eq!(grey.rgba()[at(5, 5)..at(5, 5) + 4], [0, 0, 0, 255]);
}

#[test]
fn store_export_writes_the_png_and_refuses_to_overwrite_or_write_inside_the_store() {
    let dir = Scratch::new("cli-export");
    let (root, image) = with_picture(&dir);
    let file = dir.path().join("out.png");
    let export = |out: &Path, more: &[&str]| {
        let mut args = vec!["store", "export", "--image", &image, "--out"];
        args.push(out.to_str().unwrap());
        args.extend_from_slice(more);
        whizbang(&args, &root)
    };
    let first = export(&file, &[]);
    assert_eq!(first.code, Some(0), "{}", first.err);
    let png = std::fs::read(&file).unwrap();
    assert_eq!(chunks(&png), ["IHDR", "IDAT", "IEND"]);
    assert_eq!(pixels_of(&png), pixels_of(FIXTURE));

    // A file that is there is not replaced, unless asked.
    std::fs::write(&file, b"the owner's file").unwrap();
    let again = export(&file, &[]);
    assert_eq!(again.code, Some(1));
    assert!(again.err.contains("is there already"), "{}", again.err);
    assert!(again.err.contains("--overwrite"), "{}", again.err);
    assert_eq!(std::fs::read(&file).unwrap(), b"the owner's file");
    let replaced = export(&file, &["--overwrite"]);
    assert_eq!(replaced.code, Some(0), "{}", replaced.err);
    assert_eq!(std::fs::read(&file).unwrap(), png);
    let left: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
    assert!(left.iter().all(|e| {
        !e.as_ref()
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".export")
    }));

    // A link is refused, with or without --overwrite, and what it leads to is untouched.
    let target = dir.path().join("target.txt");
    std::fs::write(&target, b"kept").unwrap();
    let link = dir.path().join("link.png");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    for more in [&[][..], &["--overwrite"][..]] {
        let refused = export(&link, more);
        assert_eq!(refused.code, Some(1));
        assert!(refused.err.contains("is a link"), "{}", refused.err);
    }
    let dangling = dir.path().join("dangling.png");
    std::os::unix::fs::symlink(dir.path().join("nowhere.png"), &dangling).unwrap();
    assert_eq!(export(&dangling, &[]).code, Some(1));
    assert!(!dir.path().join("nowhere.png").exists());
    assert_eq!(std::fs::read(&target).unwrap(), b"kept");

    // Inside the store, by its path or through a link to it, is refused, and nothing is
    // written there.
    let through = dir.path().join("store-link");
    std::os::unix::fs::symlink(&root, &through).unwrap();
    for out in [
        root.join("out.png"),
        root.join("objects").join("out.png"),
        through.join("out.png"),
    ] {
        let refused = export(&out, &["--overwrite"]);
        assert_eq!(refused.code, Some(1), "{}", out.display());
        assert!(refused.err.contains("inside the store"), "{}", refused.err);
        assert!(!out.exists());
    }
    // A folder that is missing is not made; an identity the store does not hold is
    // refused and writes nothing.
    let missing = dir.path().join("no-such-folder").join("out.png");
    assert_eq!(export(&missing, &[]).code, Some(1));
    let other = format!("sha256:{}", "0".repeat(64));
    let file = dir.path().join("none.png");
    let refused = whizbang(
        &[
            "store",
            "export",
            "--image",
            &other,
            "--out",
            file.to_str().unwrap(),
        ],
        &root,
    );
    assert_eq!(refused.code, Some(1));
    assert!(!file.exists());
}

#[test]
fn store_export_writes_a_png_for_an_image_held_as_jpeg() {
    let dir = Scratch::new("cli-export-jpeg");
    let (w, h) = (24, 16);
    let rgb: Vec<u8> = (0..w * h)
        .flat_map(|at| [(at % w * 10) as u8, (at / w * 15) as u8, 90])
        .collect();
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
        .encode(&rgb, w as u32, h as u32, image::ExtendedColorType::Rgb8)
        .unwrap();
    // An image artifact that holds the JPEG itself, as an action may write one, brought
    // in as its canonical file.
    let held = Image::new(
        jpeg,
        Projection::Flat,
        Topology::Bounded,
        ImageRole::Picture,
        Origin::new(Step::new("fixture", "1")).unwrap(),
        &DecodeLimits::default(),
    )
    .unwrap();
    let record = dir.path().join("held.image");
    std::fs::write(&record, held.to_bytes()).unwrap();
    let root = dir.path().join("store");
    let imported = whizbang(
        &["store", "import", record.to_str().unwrap(), "--create"],
        &root,
    );
    assert_eq!(imported.code, Some(0), "{}", imported.err);
    let id = held.id().to_string();
    assert!(imported.out.contains(&id), "{}", imported.out);

    let file = dir.path().join("out.png");
    let ran = whizbang(
        &[
            "store",
            "export",
            "--image",
            &id,
            "--out",
            file.to_str().unwrap(),
        ],
        &root,
    );
    assert_eq!(ran.code, Some(0), "{}", ran.err);
    let png = std::fs::read(&file).unwrap();
    assert_eq!(chunks(&png), ["IHDR", "IDAT", "IEND"]);
    // The PNG holds the JPEG's pixels as the core decodes them, exactly.
    assert_eq!(
        pixels_of(&png),
        held.pixels(&DecodeLimits::default()).unwrap()
    );
}

// --- `whizbang squint check` (plan 2A, task S2, item 4) ----------------------------------------

#[test]
fn squint_check_prints_the_verdict_and_the_run_under_a_recorded_fake() {
    use crate::anthropic::{Judged, checked, submission};
    let judged = Judged::new("cli-squint");
    let (printed, log) = checked(
        &judged,
        vec![submission("held", "Nothing protected moved.")],
    );
    let printed = printed.unwrap();
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines.len(), 3, "{printed}");
    assert!(
        lines[0].starts_with("run ") && lines[0].ends_with(" completed"),
        "{printed}"
    );
    assert_eq!(lines[1], "verdict: pass");
    assert!(
        lines[2].starts_with(
            "reason: chk:ob:SF1@candidate: held (confident): Nothing protected moved."
        ),
        "{printed}"
    );
    // One request reached the judge, with the source-fidelity's tool beside view.
    let sent: serde_json::Value = serde_json::from_slice(&log.lock().unwrap()[0].1).unwrap();
    let tools: Vec<&str> = sent["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(tools, ["view", "submit_verdict"]);
    assert_eq!(sent["max_tokens"], 2_048);
}

/// A key of capitals and digits, as one pasted in place of a variable's name would
/// be: made at run time, the key of nothing.
pub(crate) fn pasted_key() -> String {
    format!("SK0{}", "ABCDEFGHIJ0123456789".repeat(2))
}

/// A judge whose `key_env` holds a key pasted in place of the variable's name.
pub(crate) fn pasted_judge() -> String {
    format!(
        "\n[backends.judge]\nkind = \"service\"\nprovider = \"anthropic\"\n\
         model = \"claude-haiku-5-5\"\nkey_env = \"{}\"\n",
        pasted_key()
    )
}

#[test]
fn squint_check_without_the_env_file_is_refused_naming_the_backend_and_not_the_value() {
    let scratch = Scratch::new("cli-squint-env");
    let root = scratch.path().join("store");
    let config = scratch.path().join("whizbang.toml");
    std::fs::write(&config, pasted_judge()).unwrap();
    let id = format!("sha256:{}", "0".repeat(64));
    let ran = whizbang(
        &[
            "squint",
            "check",
            "source-fidelity",
            "--source",
            &id,
            "--candidate",
            &id,
            "--config",
            config.to_str().unwrap(),
        ],
        &root,
    );
    assert_eq!(ran.code, Some(1), "{}", ran.err);
    assert!(
        ran.err.contains(
            "the judge backend `default` names its credential with `key_env`, and no env \
             file was given: name the env file that sets it with --env-file"
        ),
        "{}",
        ran.err
    );
    assert!(!ran.err.contains(&pasted_key()), "{}", ran.err);
    assert!(ran.out.is_empty());
    // Refused before the store was looked at: there is none, and none was made.
    assert!(!root.exists());
}

#[test]
fn rascal_segment_on_a_pack_backend_runs_without_the_env_file_under_a_config_naming_a_judge_key() {
    // The real binary, whose kind `pack` is over the installed pack: the CI runner has
    // no model installed, so the run's segmenter is not ready. What is shown is that
    // the command goes past the env file to its backend, and says nothing of the key.
    let dir = Scratch::new("cli-segment-pack");
    let (root, image) = with_picture(&dir);
    let models = dir.path().join("models");
    let config = dir.path().join("whizbang.toml");
    let text = format!(
        "[backends.segment]\nkind = \"pack\"\nmodel = \"sam3\"\n{}",
        pasted_judge()
    );
    std::fs::write(&config, text).unwrap();
    let ran = whizbang(
        &[
            "rascal",
            "segment",
            "--image",
            &image,
            "--phrase",
            "circle",
            "--config",
            config.to_str().unwrap(),
            "--models-dir",
            models.to_str().unwrap(),
        ],
        &root,
    );
    assert_eq!(ran.code, Some(1), "{} {}", ran.out, ran.err);
    assert!(!ran.err.contains("env file"), "{}", ran.err);
    assert!(ran.err.contains("not ready"), "{}", ran.err);
    assert!(!ran.err.contains(&pasted_key()), "{}", ran.err);
    assert!(!ran.out.contains(&pasted_key()), "{}", ran.out);
    // A command that uses every backend, the judge among them, is refused for the
    // env file.
    let ran = whizbang_storeless(&[
        "backends",
        "status",
        "--config",
        config.to_str().unwrap(),
        "--models-dir",
        models.to_str().unwrap(),
    ]);
    assert_eq!(ran.code, Some(1), "{} {}", ran.out, ran.err);
    assert!(
        ran.err.contains("`key_env`, and no env file"),
        "{}",
        ran.err
    );
    assert!(!ran.err.contains(&pasted_key()), "{}", ran.err);
}

#[test]
fn commands_that_use_no_backend_run_without_the_env_file_under_a_config_naming_a_key() {
    let dir = Scratch::new("cli-no-backend");
    let (root, image) = with_picture(&dir);
    let asked = SegmentAsked {
        image: &image,
        phrase: "circle",
        max_instances: 8,
        backend: None,
        request_id: None,
    };
    let found = segment(&dir, &root, &asked).unwrap();
    let mask = found
        .lines()
        .find_map(|line| line.strip_prefix("mask "))
        .map(|rest| rest.split(' ').next().unwrap().to_owned())
        .unwrap();
    let config = dir.path().join("keyed.toml");
    std::fs::write(&config, format!("{CONFIG}{}", pasted_judge())).unwrap();
    let config = config.to_str().unwrap();
    let models = dir.path().join("models");
    let ran = |args: &[&str]| {
        let ran = whizbang(args, &root);
        assert_eq!(ran.code, Some(0), "{args:?}: {}", ran.err);
        assert!(!ran.out.contains(&pasted_key()), "{}", ran.out);
        ran.out
    };
    // The operations of rascal, each under the config and no env file.
    ran(&[
        "rascal", "grow", "--mask", &mask, "--px", "2", "--config", config,
    ]);
    ran(&["rascal", "select", "--mask", &mask, "--config", config]);
    ran(&[
        "rascal",
        "plug",
        "--mask",
        &mask,
        "--max-share",
        "0.1",
        "--config",
        config,
    ]);
    ran(&[
        "rascal",
        "feather",
        "--mask",
        &mask,
        "--sigma-px",
        "1",
        "--config",
        config,
    ]);
    ran(&[
        "rascal",
        "render",
        "--mask",
        &mask,
        "--profile",
        "plain",
        "--config",
        config,
    ]);
    let listed = whizbang_storeless(&[
        "models",
        "list",
        "--config",
        config,
        "--models-dir",
        models.to_str().unwrap(),
    ]);
    assert_eq!(listed.code, Some(0), "{}", listed.err);
    assert!(!listed.out.contains(&pasted_key()), "{}", listed.out);
    // `store`, `runs` and `spend` take no config at all.
    ran(&["runs", "list"]);
    ran(&["spend", "status"]);
}
