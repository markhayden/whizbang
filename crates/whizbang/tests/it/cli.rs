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

/// A kind whose one segmenter is the test kit's fake, declaring the features `phrase`,
/// `point` and `box`, and finding [`FOUND`] for `circle` and for every point and box. It is a remote backend's kind, at an address of
/// the reserved `.example` domain, which nothing contacts: a local one is a model of
/// the catalogue.
struct Finding;

impl Kind for Finding {
    fn name(&self) -> &str {
        "finding"
    }

    fn make(&self, table: &Table<'_>) -> Result<Made, KindRefused> {
        let model = ModelIdentity::new("finding", "m", "1");
        let features = [feature::PHRASE, feature::POINT, feature::BOX];
        let declaration = Declaration::new(model, &features).unwrap();
        // It sends nothing, and says where it would and what it may cost, as a remote
        // backend's declaration does.
        let address = table.backend().address().expect("a url");
        let declaration = declaration
            .sending_to(Recipient::new(address.origin().clone()))
            .priced(Price::at_most(1000.0).unwrap());
        let mut fake = FakeSegmenter::new().declared_as(declaration);
        for bounds in FOUND {
            fake = fake.finding("circle", bounds).finding_at_a_place(bounds);
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
pub(crate) fn segment(
    dir: &Scratch,
    root: &Path,
    asked: &SegmentAsked<'_>,
) -> Result<String, String> {
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
        kind: "phrase",
        query: "circle",
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
        query: "a dog",
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
fn rascal_segment_with_a_box_prints_each_masks_identity_and_score_under_a_fake_segmenter() {
    let dir = Scratch::new("cli-segment-box");
    let (root, image) = with_picture(&dir);
    for (kind, query) in [("box", "300 80 540 320"), ("point", "420 200")] {
        let asked = SegmentAsked {
            image: &image,
            kind,
            query,
            max_instances: 8,
            backend: None,
            request_id: None,
        };
        let out = segment(&dir, &root, &asked).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 4, "{out}");
        assert!(lines[0].ends_with(" completed"), "{out}");
        // Each mask with its score and its box, best first.
        for (line, (bounds, score)) in lines[1..]
            .iter()
            .zip(FOUND.iter().zip(["0.9", "0.8", "0.7"]))
        {
            let edges: Vec<String> = bounds.iter().map(u32::to_string).collect();
            let rest = line.strip_prefix("mask sha256:").expect("a mask");
            assert_eq!(
                rest.split_once(' ').unwrap().1,
                format!("score {score} box {}", edges.join(" ")),
                "{kind}"
            );
        }
    }
}

#[test]
fn rascal_segment_takes_exactly_one_of_phrase_point_and_box() {
    let dir = Scratch::new("cli-segment-one");
    let (root, image) = with_picture(&dir);
    let base = ["rascal", "segment", "--image", image.as_str()];
    for extra in [
        &[][..],
        &["--phrase", "circle", "--point", "1 1"][..],
        &["--point", "1 1", "--box", "1 1 4 4"][..],
        &["--phrase", "circle", "--box", "1 1 4 4"][..],
    ] {
        let args: Vec<&str> = base.iter().chain(extra).copied().collect();
        let ran = whizbang(&args, &root);
        assert_eq!(ran.code, Some(2), "{args:?}: {} {}", ran.out, ran.err);
    }
    // One of them is taken: with no segmenter configured, the call is refused for it.
    for extra in [["--point", "1 1"], ["--box", "1 1 4 4"]] {
        let args: Vec<&str> = base.iter().chain(&extra).copied().collect();
        let ran = whizbang(&args, &root);
        assert_eq!(ran.code, Some(1), "{args:?}: {}", ran.err);
        assert!(ran.err.contains("segment"), "{}", ran.err);
    }
    let runs = whizbang(&["runs", "list"], &root);
    assert!(runs.out.starts_with("runs: 0\n"), "{}", runs.out);
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
        kind: "phrase",
        query: "circle",
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
    assert_eq!(lines.len(), 4, "{printed}");
    assert!(
        lines[0].starts_with("run ") && lines[0].ends_with(" completed"),
        "{printed}"
    );
    assert_eq!(lines[1], "verdict: pass");
    assert_eq!(
        lines[2..],
        [
            "reason: chk:ob:SF1@candidate: held (confident): Nothing protected moved.",
            "reason: chk:ob:SF2@candidate: held (confident): Nothing protected moved."
        ]
    );
    // One request reached the judge, with the source-fidelity's tool beside view and align.
    let sent: serde_json::Value = serde_json::from_slice(&log.lock().unwrap()[0].1).unwrap();
    let tools: Vec<&str> = sent["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(tools, ["view", "align", "submit_verdict"]);
    assert_eq!(sent["max_tokens"], 2_048);
}

#[test]
fn squint_check_details_first_prints_the_verdict_under_a_recorded_fake() {
    use crate::anthropic::{Judged, checked_as, submission};
    let judged = Judged::new("cli-squint-details");
    let (printed, log) = checked_as(
        &judged,
        vec![submission("held", "Nothing protected moved.")],
        None,
        whizbang::door::Details::First,
        "full",
    );
    let printed = printed.unwrap();
    let lines: Vec<&str> = printed.lines().collect();
    assert!(lines[0].starts_with("run ") && lines[0].ends_with(" completed"));
    assert_eq!(lines[1], "verdict: pass");
    // One request reached the judge. It may call view, align and resize; and before it
    // looked, align registered the candidate onto the source: what it found and every
    // measurement are in the opening, labelled with the tool and its mark, and the
    // aligned picture beside them.
    let log = log.lock().unwrap();
    assert_eq!(log.len(), 1);
    let sent: serde_json::Value = serde_json::from_slice(&log[0].1).unwrap();
    let tools: Vec<&str> = sent["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(tools, ["view", "align", "resize", "submit_verdict"]);
    let opening = sent["messages"][0]["content"].as_array().unwrap();
    let texts: Vec<&str> = opening
        .iter()
        .filter_map(|part| part["text"].as_str())
        .collect();
    assert_eq!(texts[2], whizbang_squint::details::DETAILS_INTRO);
    assert!(
        texts[3].starts_with("align (unverified), candidate onto source: registered "),
        "{}",
        texts[3]
    );
    assert!(
        texts[3].contains("Measurements: translation_px "),
        "{}",
        texts[3]
    );
    assert!(
        texts[4].starts_with("Image `aligned:candidate:t1`"),
        "{}",
        texts[4]
    );
    let pictures = opening
        .iter()
        .filter(|part| part["type"] == "image")
        .count();
    assert_eq!(pictures, 3);
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
fn squint_check_sends_the_brief_it_is_given_and_none_when_it_is_given_none() {
    use crate::anthropic::{Judged, checked, checked_under, submission};
    let brief = "Change only the sky. The parrot's tail keeps its colour.";
    let system = |log: &crate::anthropic::Log| {
        let sent: serde_json::Value = serde_json::from_slice(&log.lock().unwrap()[0].1).unwrap();
        sent["system"].to_string()
    };
    let judged = Judged::new("cli-squint-brief");
    let (printed, log) = checked_under(
        &judged,
        vec![submission("held", "Nothing protected moved.")],
        Some(brief),
    );
    printed.unwrap();
    assert!(system(&log).contains(brief), "{}", system(&log));
    let judged = Judged::new("cli-squint-no-brief");
    let (printed, log) = checked(
        &judged,
        vec![submission("held", "Nothing protected moved.")],
    );
    printed.unwrap();
    let without = system(&log);
    assert!(!without.contains(brief), "{without}");
    assert!(
        without.contains(&whizbang_squint::use_case::NO_BRIEF[..40]),
        "{without}"
    );
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
        kind: "phrase",
        query: "circle",
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

// --- the verdict's reason, whole (plan 1D, task F1, item 7) -------------------------------------

/// A reason of `words` words, each `word` and a space between: so long that the
/// verdict's summary cuts it, and with nothing a door escapes.
fn long_reason(word: &str, words: usize) -> String {
    vec![word; words].join(" ")
}

/// The lines `squint check` printed after its run's: the verdict, then the reason's.
fn after_the_run(printed: &str) -> Vec<String> {
    printed.lines().skip(1).map(str::to_owned).collect()
}

#[test]
fn squint_check_prints_each_reason_item_whole_on_its_own_line() {
    use crate::anthropic::{Judged, checked, record_of, submission};
    // 80 words of five letters and a space: 479 characters, over the summary's 300.
    let reason = long_reason("tails", 80);
    let judged = Judged::new("cli-reason-whole");
    let (printed, _) = checked(&judged, vec![submission("held", &reason)]);
    let printed = printed.unwrap();
    assert_eq!(
        after_the_run(&printed),
        [
            "verdict: pass".to_owned(),
            format!("reason: chk:ob:SF1@candidate: held (confident): {reason}"),
            format!("reason: chk:ob:SF2@candidate: held (confident): {reason}"),
        ]
    );
    // The record's reason is the summary: each item cut at a space before 300
    // characters, with three dots, and the two joined.
    let run = printed.lines().next().unwrap().split(' ').nth(1).unwrap();
    let record = record_of(&judged.world.root, run);
    let verdict = record["judge"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["judge"] == "verdict")
        .unwrap();
    let cut = long_reason("tails", 50);
    assert_eq!(
        verdict["why"],
        format!(
            "chk:ob:SF1@candidate: held (confident): {cut}...; \
             chk:ob:SF2@candidate: held (confident): {cut}..."
        )
    );
}

#[test]
fn reason_items_over_4096_characters_in_all_are_each_printed_whole_from_the_stored_run() {
    use crate::anthropic::{Judged, checked, submission};
    // Two items of 3,000 characters and more: over the 4,096 a record keeps of a
    // verdict's reason in all, and each under the door's print limit.
    let reason = long_reason("feathers", 340);
    assert!(reason.len() > 3_000 && reason.len() < 4_900);
    let judged = Judged::new("cli-reason-4096");
    let (printed, _) = checked(&judged, vec![submission("held", &reason)]);
    let printed = printed.unwrap();
    let lines = after_the_run(&printed);
    assert_eq!(lines.len(), 3, "{printed}");
    for (line, item) in lines[1..]
        .iter()
        .zip(["chk:ob:SF1@candidate", "chk:ob:SF2@candidate"])
    {
        assert_eq!(*line, format!("reason: {item}: held (confident): {reason}"));
    }
    // Read again from the stored run by a reader of its own: the same items.
    let run = printed.lines().next().unwrap().split(' ').nth(1).unwrap();
    let run = whizbang_core::contract::RunId::parse(run).unwrap();
    let stated = whizbang::door::verdict(&judged.world.root, &run).unwrap();
    assert_eq!(stated.items.len(), 2);
    assert!(stated.items.iter().all(|item| item.ends_with(&reason)));
}

#[test]
fn a_reason_item_over_the_print_limit_is_cut_there_and_escaped() {
    use crate::anthropic::{Judged, checked, submission};
    // A reason of 6,000 characters and more, with a line break and an escape near its
    // start: printed on one line, each escaped, and cut at the door's 5,000
    // characters of the item, with three dots.
    let reason = format!(
        "Line one.\nAn escape \u{1b}[31m here. {}",
        long_reason("plumes", 900)
    );
    let judged = Judged::new("cli-reason-cut");
    let (printed, _) = checked(&judged, vec![submission("held", &reason)]);
    let printed = printed.unwrap();
    let lines = after_the_run(&printed);
    assert_eq!(lines.len(), 3, "{printed}");
    let item = format!("chk:ob:SF1@candidate: held (confident): {reason}");
    let kept: String = item.chars().take(5_000).collect();
    let shown = whizbang::door::printable(&kept);
    assert!(
        shown.contains("Line one.\\nAn escape \\u{1b}[31m here."),
        "{shown}"
    );
    assert_eq!(lines[1], format!("reason: {shown}..."));
    assert!(!printed.contains('\u{1b}'));
}

#[test]
fn squint_check_brief_prints_the_same_items() {
    use crate::anthropic::{Judged, checked, checked_under, submission};
    let reason = long_reason("crest", 90);
    let judged = Judged::new("cli-reason-brief");
    let (without, _) = checked(&judged, vec![submission("held", &reason)]);
    let judged = Judged::new("cli-reason-brief-given");
    let (with, _) = checked_under(
        &judged,
        vec![submission("held", &reason)],
        Some("Change only the sky."),
    );
    let (without, with) = (without.unwrap(), with.unwrap());
    assert_eq!(after_the_run(&with), after_the_run(&without));
    assert_eq!(after_the_run(&with).len(), 3, "{with}");
}

#[test]
fn a_key_pasted_into_key_env_with_an_env_file_that_does_not_set_it_is_never_said_back() {
    // The security review of F1: with an env file given that does not set it, a key
    // pasted where a variable's name belongs is not known to any reader as a secret,
    // so nothing may say the variable's name. Each command that would use it says the
    // setting, `key_env`, and never what it holds; nothing of it reaches the store.
    let dir = Scratch::new("cli-pasted-env");
    let (root, image) = with_picture(&dir);
    let key = pasted_key();
    let env = dir.path().join("keys.env");
    std::fs::write(&env, "OTHER=made-up-value-not-a-key\n").unwrap();
    let env = env.to_str().unwrap().to_owned();
    let models = dir.path().join("models");
    let models = models.to_str().unwrap().to_owned();
    let config = dir.path().join("pasted.toml");
    let text = format!(
        "[backends.segment]\nkind = \"pack\"\nmodel = \"sam3\"\nkey_env = \"{key}\"\n{}\n\
         [[models.tokens]]\norigin = \"https://huggingface.co\"\nkey_env = \"{key}\"\n",
        pasted_judge()
    );
    std::fs::write(&config, text).unwrap();
    let config = config.to_str().unwrap().to_owned();
    let under = ["--config", config.as_str(), "--env-file", env.as_str()];
    let said = |ran: &crate::wired::Ran, what: &str| {
        assert_ne!(ran.code, Some(0), "{what}: {} {}", ran.out, ran.err);
        assert!(!ran.out.contains(&key), "{what}: {}", ran.out);
        assert!(!ran.err.contains(&key), "{what}: {}", ran.err);
        format!("{}{}", ran.out, ran.err)
    };
    let checked = whizbang(
        &[
            &[
                "squint",
                "check",
                "source-fidelity",
                "--source",
                &image,
                "--candidate",
                &image,
            ][..],
            &under[..],
        ]
        .concat(),
        &root,
    );
    let text = said(&checked, "squint check");
    assert!(text.contains("`key_env` names"), "{text}");
    let status = whizbang_storeless(
        &[
            &["backends", "status", "--models-dir", &models][..],
            &under[..],
        ]
        .concat(),
    );
    // `backends status` lists each backend and ends well; what it prints holds no key.
    assert!(
        !status.out.contains(&key) && !status.err.contains(&key),
        "{}{}",
        status.out,
        status.err
    );
    assert!(status.out.contains("`key_env` names"), "{}", status.out);
    let segmented = whizbang(
        &[
            &[
                "rascal",
                "segment",
                "--image",
                &image,
                "--phrase",
                "circle",
                "--models-dir",
                &models,
            ][..],
            &under[..],
        ]
        .concat(),
        &root,
    );
    let text = said(&segmented, "rascal segment");
    assert!(text.contains("`key_env` names"), "{text}");
    let got = whizbang_storeless(
        &[
            &["models", "get", "sam3", "--models-dir", &models][..],
            &under[..],
        ]
        .concat(),
    );
    let text = said(&got, "models get");
    // The token is read only on a platform the model runs on: on any other the get is
    // refused before it, and says so. `credential::` holds the token's refusal on every
    // platform, with a catalogue of its own.
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        assert!(
            text.contains("`[[models.tokens]]` `key_env` names"),
            "{text}"
        );
    } else {
        assert!(text.contains("does not run on this platform"), "{text}");
    }
    // Nothing of it in any file of the store.
    let mut folders = vec![root.clone()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else {
                let bytes = std::fs::read(&path).unwrap();
                assert!(!bytes.windows(key.len()).any(|at| at == key.as_bytes()));
            }
        }
    }
    // A judge the config does not give is refused at once, without the env file, and
    // what was typed is not said back.
    let ran = whizbang(
        &[
            "squint",
            "check",
            "source-fidelity",
            "--source",
            &image,
            "--candidate",
            &image,
            "--judge",
            "nobody-here",
            "--config",
            &config,
        ],
        &root,
    );
    assert_eq!(ran.code, Some(1), "{}", ran.err);
    assert!(
        ran.err
            .contains("the config gives no judge backend of the name the command line gives"),
        "{}",
        ran.err
    );
    assert!(!ran.err.contains("nobody-here"), "{}", ran.err);
}

#[test]
fn a_users_adapter_is_added_its_model_installed_and_a_segment_runs_through_admission_against_a_fake_model()
 {
    // Plan 1D, task U1: the binary adds a user's adapter (a made-up fake model's, in a
    // test's folder) to the models directory; a model of the directory's own catalogue
    // that names it is installed under its hash; and a run admitted over it is answered
    // by it, through the worker, which ran the copy only after its hash matched.
    use crate::models::{
        USERS_ADAPTER, get_under, users_config, users_entry, write_users_catalogue,
    };
    use whizbang_core::contract::Executor as _;
    let local = crate::pack::Local::new("cli-users-adapter");
    let file = local.models_dir().with_file_name("mine.py");
    std::fs::write(&file, USERS_ADAPTER).unwrap();
    let hex = whizbang_core::identity::Digest::of_bytes(USERS_ADAPTER.as_bytes()).hex();
    let mut add = crate::models::binary();
    add.args(["models", "adapter", "add"])
        .arg(&file)
        .args(["--name", "mine", "--models-dir"])
        .arg(local.models_dir());
    let added = add.output().unwrap();
    assert_eq!(added.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&added.stdout),
        format!("added user-mine: sha256:{hex}\n")
    );
    let (mine, fake) = users_entry("mine", "user-mine");
    write_users_catalogue(&local.models_dir(), &[mine]);
    std::fs::write(local.config_path(), users_config("mine")).unwrap();
    let got = get_under(
        local.models(),
        Some(&local.config_path()),
        &Arc::new(fake),
        "mine",
    )
    .unwrap();
    let over = local.over(&users_config("mine"));
    let identity = local.identity(&over, "mine");
    assert_eq!(got, format!("installed mine: {identity}\n"));
    let record = over.installations().unwrap().ready("mine").unwrap();
    assert_eq!(
        (
            record.adapter().as_str(),
            record.adapter_revision().as_str()
        ),
        ("user-mine", hex.as_str())
    );
    let executor = local.runner(&over);
    let finished = executor
        .submit(&whizbang_core::contract::Call::new("finds"))
        .unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!(local.came(), [Ok(crate::pack::determined())]);
}

// --- the four other use cases, and the job (plan 2C, task A1) ---------------------------------

#[test]
fn squint_check_takes_each_use_case_with_its_roles_under_a_recorded_fake() {
    use crate::anthropic::{Judged, checked_job, checklist_of, import_picture, submission_of};
    let judged = Judged::new("cli-use-cases");
    let scratch = judged.world.beside("pictures");
    std::fs::create_dir_all(&scratch).unwrap();
    let candidate = import_picture(&judged.world.root, &scratch, "candidate", 200);
    let reference = import_picture(&judged.world.root, &scratch, "reference", 10);
    let brief = "A grey picture with a red band. No text.";
    for (use_case, references) in [
        ("reference-consistency", vec![reference.as_str()]),
        ("reality-checks", vec![]),
        ("brief-compliance", vec![]),
        (
            "product-fidelity",
            vec![reference.as_str(), candidate.as_str()],
        ),
    ] {
        let asked = door::CheckAsked {
            use_case,
            candidate: &candidate,
            source: None,
            references: &references,
            judge: None,
            brief: Some(brief),
            request_id: None,
            details: whizbang::door::Details::None,
            detail: None,
            projection: "full",
        };
        let (job, images) = door::shorthand(&asked).unwrap();
        let ids = checklist_of(&job);
        let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
        let answer = submission_of(&ids, "held", "Looked at.");
        let (printed, log) = checked_job(&judged, &job, &images, vec![answer]);
        let printed = printed.unwrap_or_else(|why| panic!("{use_case}: {why}"));
        assert!(printed.contains("verdict: pass"), "{use_case}: {printed}");
        assert_eq!(
            printed.matches("reason: chk:").count(),
            ids.len(),
            "{printed}"
        );
        // What the judge was sent: the rules of the use case, and each image's line
        // with its role, references labelled in the order given.
        let sent: serde_json::Value = serde_json::from_slice(&log.lock().unwrap()[0].1).unwrap();
        let rule = &ids[0][7..10];
        assert!(
            sent["system"].to_string().contains(&format!("1. {rule} (")),
            "{use_case}"
        );
        let opening = sent["messages"][0].to_string();
        assert!(opening.contains("Image `candidate` (role Candidate, frame raw:candidate"));
        for (at, reference) in references.iter().enumerate() {
            let line = format!(
                "Image `reference-{n}` (role Reference, reference {n}, frame raw:reference-{n}, \
                 64x48, Flat); for the tool view it is named {reference}.",
                n = at + 1
            );
            assert!(opening.contains(&line), "{use_case}: {opening}");
        }
    }
}

#[test]
fn squint_check_job_takes_a_squint_job_file_whole() {
    use crate::anthropic::{Judged, checked_job, checklist_of, submission_of};
    let judged = Judged::new("cli-job-file");
    let folder = judged.world.beside("set");
    std::fs::create_dir_all(folder.join("items")).unwrap();
    let picture = folder.join("items/candidate.png");
    image::RgbImage::from_fn(64, 48, |x, y| {
        image::Rgb([(x * 4) as u8, (y * 5) as u8, 90])
    })
    .save_with_format(&picture, image::ImageFormat::Png)
    .unwrap();
    let digest = whizbang_core::identity::Digest::of_bytes(&std::fs::read(&picture).unwrap());
    // A squint job file of brief-compliance, as its sets write them.
    let written = |sha256: &str| {
        format!(
            "# An item, made for this test.\n[job]\nuse_case = \"brief-compliance\"\n\
             brief = \"A gradient. No people.\"\n\n\
             [[job.requirements]]\nid = \"RQ1\"\nkind = \"must_hold\"\nscope = [\"candidate\"]\n\
             text = \"The candidate shows a gradient.\"\n\n\
             [[job.requirements]]\nid = \"RQ2\"\nkind = \"must_not\"\nscope = [\"candidate\"]\n\
             text = \"The candidate shows people; none was asked for.\"\n\n\
             [[job.images]]\nid = \"candidate\"\nrole = \"candidate\"\nlabel = \"the gradient\"\n\
             source = {{ path = \"items/candidate.png\", sha256 = \"{sha256}\" }}\n"
        )
    };
    let file = folder.join("job.toml");
    std::fs::write(&file, written(&digest.hex())).unwrap();
    let recovery = wiring::recovery();
    let root = &judged.world.root;
    let (job, images) = door::job_file(&file, "brief-compliance", root, &recovery).unwrap();
    assert_eq!(images.len(), 1);
    let ids = checklist_of(&job);
    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    assert_eq!(
        ids,
        [
            "chk:ob:BC1@candidate",
            "chk:ob:BC2@candidate",
            "chk:ob:BC3@candidate",
            "chk:ob:RQ1@candidate",
            "chk:ob:RQ2@candidate"
        ]
    );
    let answer = submission_of(&ids, "held", "Looked at.");
    let (printed, log) = checked_job(&judged, &job, &images, vec![answer]);
    assert!(printed.unwrap().contains("verdict: pass"));
    let sent: serde_json::Value = serde_json::from_slice(&log.lock().unwrap()[0].1).unwrap();
    let system = sent["system"].to_string();
    assert!(system.contains("Job requirements:"), "{system}");
    assert!(system.contains("7. RQ2 (must not occur; judges candidate; consults nothing)"));
    assert!(system.contains("A gradient. No people."));
    assert!(
        sent["messages"][0]
            .to_string()
            .contains("(role Candidate, the gradient, frame")
    );
    // A file whose digest is not the one pinned, and a job of another use case, are
    // refused before anything is sent.
    std::fs::write(&file, written(&"0".repeat(64))).unwrap();
    let refused = door::job_file(&file, "brief-compliance", root, &recovery).unwrap_err();
    assert!(refused.contains("is not the one the job pins"), "{refused}");
    let refused = door::job_file(&file, "reality-checks", root, &recovery).unwrap_err();
    assert!(
        refused.contains("is a job of another use case than reality-checks"),
        "{refused}"
    );
}

#[test]
fn squint_check_brief_compliance_without_a_brief_is_refused_before_anything_runs() {
    let scratch = Scratch::new("cli-no-brief");
    let root = scratch.path().join("store");
    let id = format!("sha256:{}", "0".repeat(64));
    let ran = whizbang(
        &["squint", "check", "brief-compliance", "--candidate", &id],
        &root,
    );
    assert_eq!(ran.code, Some(1), "{}", ran.err);
    assert!(
        ran.err
            .contains("brief-compliance holds the picture to a brief: give it with --brief"),
        "{}",
        ran.err
    );
    assert!(ran.out.is_empty());
    // Refused before the config, the store or a judge was looked at.
    assert!(!root.exists());
}

#[test]
fn squint_check_with_nine_references_is_refused_naming_the_limit() {
    let scratch = Scratch::new("cli-nine-references");
    let root = scratch.path().join("store");
    let id = format!("sha256:{}", "0".repeat(64));
    let mut args = vec!["squint", "check", "product-fidelity", "--candidate", &id];
    for _ in 0..9 {
        args.extend(["--reference", id.as_str()]);
    }
    let ran = whizbang(&args, &root);
    assert_eq!(ran.code, Some(1), "{}", ran.err);
    assert!(
        ran.err
            .contains("--reference is given at most 8 times, and it was given 9"),
        "{}",
        ran.err
    );
    assert!(!root.exists());
    // Eight are taken past that check: what stops them is the empty store.
    let eight = &args[..args.len() - 2];
    let ran = whizbang(eight, &root);
    assert!(!ran.err.contains("at most 8"), "{}", ran.err);
}
