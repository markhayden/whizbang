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

#[test]
fn squint_check_with_detail_change_map_hands_its_measurements_to_a_scripted_judge() {
    use crate::anthropic::{Judged, checked_measuring, submission};
    let judged = Judged::new("cli-squint-change-map");
    let (printed, log) = checked_measuring(
        &judged,
        vec![submission("held", "Nothing protected moved.")],
        None,
        whizbang::door::Details::First,
        Some("change_map"),
        "full",
    );
    let printed = printed.unwrap();
    assert!(printed.lines().nth(1) == Some("verdict: pass"), "{printed}");
    // Before the judge looked, align registered the candidate onto the source, and
    // change_map compared the source with the aligned frame: the opening holds what each
    // measured, labelled with the tool and its mark, the aligned frame and the map.
    let log = log.lock().unwrap();
    assert_eq!(log.len(), 1);
    let sent: serde_json::Value = serde_json::from_slice(&log[0].1).unwrap();
    let opening = sent["messages"][0]["content"].as_array().unwrap();
    let texts: Vec<&str> = opening
        .iter()
        .filter_map(|part| part["text"].as_str())
        .collect();
    assert!(texts[3].starts_with("align (unverified), "), "{}", texts[3]);
    let measured = texts
        .iter()
        .find(|text| {
            text.starts_with(
                "change_map (unverified), aligned:candidate:t1 against source: Change map of ",
            )
        })
        .unwrap_or_else(|| panic!("{texts:?}"));
    assert!(
        measured.contains("% of pixels differ in structure")
            && measured.contains("\nMeasurements: changed_fraction 0."),
        "{measured}"
    );
    assert!(
        texts
            .iter()
            .any(|text| text.starts_with("Image `map:candidate:source`")),
        "{texts:?}"
    );
    let pictures = opening
        .iter()
        .filter(|part| part["type"] == "image")
        .count();
    assert_eq!(pictures, 4);
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
fn squint_check_prints_the_counts_of_its_findings() {
    use crate::anthropic::{Judged, checked, submission};
    use whizbang_core::contract::record::{Decision, Found};
    use whizbang_core::contract::{Confidence, Disposition, FindingCount, LeftOut};
    // Plan m4, task K2, item 4. A judge that reports each item of source-fidelity's
    // checklist as a confident violation: after the verdict and its reasons, the door
    // prints that the findings are complete, and a count for each obligation cited.
    let judged = Judged::new("cli-findings");
    let said = "The collar is still there.";
    let (printed, _) = checked(&judged, vec![submission("violation", said)]);
    let printed = printed.unwrap();
    assert_eq!(
        after_the_run(&printed),
        [
            "verdict: fail".to_owned(),
            format!("reason: chk:ob:SF1@candidate: violation (confident): {said}"),
            format!("reason: chk:ob:SF2@candidate: violation (confident): {said}"),
            "findings: complete".to_owned(),
            "finding: SF1 violation confident: 1".to_owned(),
            "finding: SF2 violation confident: 1".to_owned(),
        ]
    );
    // They are the record's: read again from the stored run by a reader of its own.
    let run = printed.lines().next().unwrap().split(' ').nth(1).unwrap();
    let run = whizbang_core::contract::RunId::parse(run).unwrap();
    let stated = whizbang::door::verdict(&judged.world.root, &run).unwrap();
    let found = stated.findings.unwrap();
    assert_eq!(found.decision, Decision::Fail);
    assert!(found.complete);
    assert_eq!(found.counts.len(), 2);
    // A check with no finding prints no line of them: the pass of
    // `squint_check_prints_the_verdict_and_the_run_under_a_recorded_fake` is four lines.
    // A list that was cut short says so, with what was left out, and a disposition
    // that takes no confidence is printed with a dash.
    let count = |obligation: &str, disposition, confidence, count| FindingCount {
        obligation: obligation.to_owned(),
        disposition,
        confidence,
        count,
    };
    let short = Found {
        decision: Decision::Fail,
        complete: false,
        left_out: LeftOut {
            findings: 1,
            places: 24,
        },
        counts: vec![
            count(
                "RQ1",
                Disposition::Violation,
                Some(Confidence::Tentative),
                30,
            ),
            count("SF4", Disposition::Observed, None, 2),
        ],
        places: Vec::new(),
    };
    let mut text = String::new();
    whizbang::door::said_findings(&mut text, &short);
    assert_eq!(
        text,
        "findings: not complete, 1 findings and 24 places left out\n\
         finding: RQ1 violation tentative: 30\n\
         finding: SF4 observed -: 2\n"
    );
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
fn squint_check_detail_tiles_on_a_pair_is_refused_naming_the_use_case() {
    let scratch = Scratch::new("cli-tiles-pair");
    let root = scratch.path().join("store");
    let id = format!("sha256:{}", "0".repeat(64));
    for (use_case, pair) in [
        ("source-fidelity", "--source"),
        ("reference-consistency", "--reference"),
        ("brief-compliance", "--reference"),
    ] {
        let ran = whizbang(
            &[
                "squint",
                "check",
                use_case,
                "--candidate",
                &id,
                pair,
                &id,
                "--brief",
                "Keep it.",
                "--detail",
                "tiles",
            ],
            &root,
        );
        assert_eq!(ran.code, Some(1), "{use_case}: {}", ran.err);
        assert!(
            ran.err.contains(&format!(
                "the detail tiles is for a use case with one image, and this {use_case} job \
                 relates a pair"
            )),
            "{}",
            ran.err
        );
        assert!(ran.out.is_empty());
        // Refused before the config, the store or a judge was looked at.
        assert!(!root.exists());
    }
    // A job with one image takes it past that check: what stops it is the empty store.
    let ran = whizbang(
        &[
            "squint",
            "check",
            "reality-checks",
            "--candidate",
            &id,
            "--detail",
            "tiles",
        ],
        &root,
    );
    assert!(!ran.err.contains("the detail tiles"), "{}", ran.err);
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

#[test]
fn eval_matrix_runs_a_three_item_set_under_a_scripted_judge_and_runs_stats_reports_it() {
    use crate::anthropic::submission;
    use crate::matrix::{matrix_of, ran, run_matrix, world_with};
    use crate::wired::whizbang;
    // The matrix runs through the runner the command builds, over the binary's
    // executor and a judge on loopback: three items under arm C.
    let (judged, tiny) = world_with("cli-eval-matrix", &["a", "b", "c"]);
    let matrix = matrix_of("C", 1, 2);
    let answers = vec![submission("held", "Kept."); 3];
    let (report, _) = run_matrix(&judged, answers, &|e| {
        ran(e, &judged, &tiny, &matrix, 20_000_000, &|| false)
    });
    assert_eq!(
        report.unwrap().text(),
        "cells: 3\nstarted now: 3\ncompleted: 3\nfailed: 0\ninterrupted: 0\nnot run: 0\n\
         verdicts: pass 3\nagreeing with their labels: 3 of 3\n"
    );
    // And the binary reports on it, by the matrix's name.
    let stats = whizbang(&["runs", "stats", "--matrix", "tiny"], &judged.world.root);
    assert_eq!(stats.code, Some(0), "{}", stats.err);
    assert!(
        stats.out.starts_with("runs: 3\ncompleted: 3 (100.0%)\n"),
        "{}",
        stats.out
    );
    assert!(
        stats.out.contains("verdicts: 3 (pass 3, fail 0, unsure 0)"),
        "{}",
        stats.out
    );
    // The command itself refuses a matrix whose judge the config does not give as the
    // model it names, before anything runs.
    let file = judged.world.beside("matrix.toml");
    std::fs::write(
        &file,
        format!(
            "[matrix]\nname = \"m\"\nsets = [\"{}\"]\narms = [\"C\"]\nrepetitions = 1\n\
             [[matrix.judges]]\nbackend = \"default\"\nmodel = \"gpt-6.1-terra\"\n",
            crate::fetch::SET
        ),
    )
    .unwrap();
    let config = judged.world.beside("eval-config.toml");
    std::fs::write(
        &config,
        "[backends.judge]\nkind = \"service\"\nprovider = \"anthropic\"\nmodel = \"claude-haiku-5-5\"\n",
    )
    .unwrap();
    let refused = whizbang(
        &[
            "eval",
            "matrix",
            "--matrix",
            file.to_str().unwrap(),
            "--root",
            tiny.root.to_str().unwrap(),
            "--config",
            config.to_str().unwrap(),
        ],
        &judged.world.root,
    );
    assert_eq!(refused.code, Some(1));
    assert!(
        refused.err.contains("is to be of the model gpt-6.1-terra"),
        "{}",
        refused.err
    );
}

#[test]
fn eval_run_gate_runs_a_three_item_gate_under_a_scripted_judge_and_writes_its_receipt() {
    use crate::anthropic::submission;
    use crate::gate::{frozen_gate, gate_matrix, gated};
    use crate::matrix::world_with;
    use crate::wired::whizbang;
    // The gate runs through what the command calls, over the binary's executor and a judge
    // on loopback: three items under arms C and F.
    let (judged, tiny) = world_with("cli-eval-gate", &["a", "b", "c"]);
    let gates = judged.world.beside("gates");
    let gate = frozen_gate(&gates, &gate_matrix());
    let receipt = judged.world.beside("receipt.json");
    let answers = vec![submission("held", "Kept."); 6];
    let (written, sent) = gated(&judged, &tiny, &gate, &tiny.root, &receipt, answers);
    let written = written.unwrap();
    assert_eq!(sent, 6);
    let text = whizbang::gating::text(&written);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], format!("gate: {}", &written.gate_sha256[..16]));
    assert_eq!(lines[2..4], ["cells scored: 6", "outcome: inconclusive"]);
    assert!(
        lines[4].starts_with(
            "judge default (claude-haiku-5-5): inconclusive; items 3, families 1, differing 0"
        ),
        "{text}"
    );
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&receipt).unwrap()).unwrap();
    assert_eq!(json["cells"], 6);
    // The command refuses, before anything runs: a receipt under the sets' folder, and a
    // matrix changed after its gate was frozen.
    let config = judged.world.beside("gate-config.toml");
    std::fs::write(
        &config,
        "[backends.judge]\nkind = \"service\"\nprovider = \"anthropic\"\nmodel = \"claude-haiku-5-5\"\n",
    )
    .unwrap();
    let path = |p: &Path| p.to_str().unwrap().to_owned();
    let command = |receipt: &Path| {
        whizbang(
            &[
                "eval",
                "run",
                "--gate",
                &path(&gate),
                "--labels",
                &path(&tiny.root),
                "--receipt",
                &path(receipt),
                "--root",
                &path(&tiny.root),
                "--config",
                &path(&config),
            ],
            &judged.world.root,
        )
    };
    let inside = command(&tiny.root.join("receipt.json"));
    assert_eq!(inside.code, Some(1));
    assert!(inside.err.contains("not written under"), "{}", inside.err);
    std::fs::write(
        gates.join("matrix.toml"),
        gate_matrix().replace("tiny-gate", "other"),
    )
    .unwrap();
    let changed = command(&judged.world.beside("other.json"));
    assert_eq!(changed.code, Some(1));
    assert!(changed.err.contains("frozen with"), "{}", changed.err);
    assert!(!judged.world.beside("other.json").exists());
}

// --- `whizbang serve` (plan 2C, task M1) --------------------------------------------------------

/// A child that is killed and waited for when this is dropped, on every path out of a
/// test, a panic among them.
struct Killed(std::process::Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn serve_answers_a_client_over_stdio_and_exits_when_its_input_ends() {
    use std::io::{Read, Write};
    use std::process::Stdio;
    let scratch = Scratch::new("cli-serve");
    let root = scratch.path().join("store");
    drop(Store::open(&root, Arc::new(RealFs)).unwrap());
    // The command is kept until the child has been waited for: its home goes with it.
    let mut command = crate::models::binary();
    let child = command
        .args(["serve", "--store", root.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary runs");
    let mut child = Killed(child);
    let mut stdout = child.0.stdout.take().unwrap();
    let mut stderr = child.0.stderr.take().unwrap();
    let (read, has_read) = std::sync::mpsc::channel();
    let reader = whizbang_testkit::threads::spawn(move || {
        let (mut out, mut err) = (String::new(), String::new());
        let _ = stdout.read_to_string(&mut out);
        let _ = stderr.read_to_string(&mut err);
        let _ = read.send((out, err));
    });
    let messages = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"check","arguments":{"use_case":"reality-checks","images":[{"id":"sha256:0000000000000000000000000000000000000000000000000000000000000000","role":"candidate"}]}}}"#,
        r#"{"jsonrpc":"2.0","id":4,"method":"ping"}"#,
    ];
    {
        let mut stdin = child.0.stdin.take().unwrap();
        for message in messages {
            writeln!(stdin, "{message}").unwrap();
        }
        // The input ends here.
    }
    let (out, err) = has_read
        .recv_timeout(std::time::Duration::from_secs(20))
        .expect("the server ended within 20 seconds of the end of its input");
    reader.join().unwrap();
    let status = child.0.wait().unwrap();
    assert_eq!(status.code(), Some(0), "{err}");
    assert_eq!(err, "");
    // Every line it wrote is a message of the protocol, and each request is answered.
    let lines: Vec<serde_json::Value> = out.lines().map(crate::serve::protocol).collect();
    let ids: Vec<i64> = lines.iter().filter_map(|m| m["id"].as_i64()).collect();
    assert_eq!(ids, [1, 2, 3, 4], "{out}");
    assert_eq!(lines[0]["result"]["serverInfo"]["name"], "whizbang");
    assert!(lines[1]["result"]["tools"].as_array().unwrap().len() >= 10);
    // No judge is configured: the call is refused before anything runs, as a result.
    assert_eq!(lines[2]["result"]["isError"], true, "{out}");
    assert_eq!(lines[3]["result"], serde_json::json!({}));
}

// --- `whizbang eval baseline` (plan 2C, task E2, item 7) ---------------------------------------

#[test]
fn eval_baseline_prints_counts_and_no_item() {
    let fixtures = crate::baseline::fixtures();
    let scratch = Scratch::new("cli-baseline");
    let at = scratch.path().join("receipt.toml");
    let path = |p: &Path| p.to_str().unwrap().to_owned();
    // One decision of the three, the one that calls `view` alone: the rest are not replayed,
    // so the gate fails, and says so in counts.
    let one = scratch.path().join("transcripts");
    let from = fixtures.join("transcripts/historical/synthetic/cut-short");
    std::fs::create_dir(&one).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), one.join(entry.file_name())).unwrap();
    }
    let ran = whizbang_storeless(&[
        "eval",
        "baseline",
        "--package",
        &path(&fixtures.join("package")),
        "--transcripts",
        &path(&one),
        "--receipt",
        &path(&at),
        "--commit",
        crate::baseline::COMMIT,
        "--sets",
        &path(&fixtures.join("package/evalsets")),
    ]);
    assert_eq!(ran.code, Some(0), "{}", ran.err);
    assert_eq!(
        ran.out,
        "fixtures: 3\ntranscripts: 1\ntranscript version: 1\nsame: 1\ndiffered: 0\nrefused: 0\n\
         model claude-haiku-5-5: same 1, differed 0\nuse case source-fidelity: same 1, differed 0\n\
         result: fail\n"
    );
    // Nothing of a decision is printed: no set, item, image, frame, verdict or file. The model
    // and the use case are named, and nothing else.
    for word in [
        "synthetic",
        "kept",
        "moved",
        "cut-short",
        "candidate",
        "raw:",
        "crop",
        "unsure",
        "decision",
    ] {
        assert!(!ran.out.contains(word) && !ran.err.contains(word), "{word}");
    }
    assert!(
        std::fs::read_to_string(&at)
            .unwrap()
            .ends_with("result = \"fail\"\n")
    );
}

// --- `whizbang try` and `whizbang eval tool` (plan m2c, task X1, item 5) ------------------------

/// A scratch folder and an empty store in it.
fn bare(label: &str) -> (Scratch, PathBuf) {
    let dir = Scratch::new(label);
    let root = dir.path().join("store");
    drop(Store::open(&root, Arc::new(RealFs)).unwrap());
    (dir, root)
}

/// A picture of `fixtures/compare/`, squint's painted scenes.
fn compared(name: &str) -> Vec<u8> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/compare");
    std::fs::read(dir.join(format!("{name}.png"))).unwrap()
}

/// Brings a picture into the store at `root`, as `store import --picture` does: its
/// identity.
pub(crate) fn imported(dir: &Scratch, root: &Path, name: &str) -> String {
    let file = dir.path().join(format!("{name}.png"));
    std::fs::write(&file, compared(name)).unwrap();
    let said = whizbang::import::import_picture(&file, root, false, wiring::recovery()).unwrap();
    let id = said
        .split_whitespace()
        .find(|word| word.starts_with("sha256:"))
        .unwrap()
        .to_owned();
    id.trim_end_matches(['.', ',']).to_owned()
}

#[test]
fn try_runs_one_tool_on_a_pair_with_no_judge() {
    let (dir, root) = bare("cli-try");
    let source = imported(&dir, &root, "before");
    let candidate = imported(&dir, &root, "after");
    // No config: no judge and no backend is there to be asked.
    let tried = whizbang(
        &[
            "try",
            "change_map",
            "--source",
            &source,
            "--candidate",
            &candidate,
        ],
        &root,
    );
    assert_eq!(tried.code, Some(0), "{}", tried.err);
    let lines: Vec<&str> = tried.out.lines().collect();
    assert!(
        lines[0].starts_with("run ") && lines[0].ends_with(" align"),
        "{}",
        tried.out
    );
    assert!(
        lines[1].starts_with("run ") && lines[1].ends_with(" change_map"),
        "{}",
        tried.out
    );
    // The fraction is the tool's on the source and the frame align returned.
    let measured: f64 = lines[2]
        .strip_prefix("changed_fraction ")
        .and_then(|rest| rest.strip_suffix(" ratio"))
        .unwrap_or_else(|| panic!("{}", tried.out))
        .parse()
        .unwrap();
    assert!(measured > 0.0 && measured < 0.05, "{measured}");
    assert!(lines[3].starts_with("map sha256:"), "{}", tried.out);
    assert_eq!(lines.len(), 4, "{}", tried.out);
    // A tool that cannot be tried is refused, naming those that can.
    let refused = whizbang::trying::declared("grow").unwrap_err();
    assert!(refused.contains("change_map, edge_similarity"), "{refused}");
}

/// A set of source-fidelity under `dir` of three items in three families, each its source
/// and candidate from `fixtures/compare/`, labelled as given; its pictures in a folder beside.
pub(crate) fn three_items(dir: &Path) -> (PathBuf, PathBuf) {
    const SET: &str = "source-fidelity/compare-v1";
    let (root, from) = (dir.join("evalsets"), dir.join("pictures"));
    let set = root.join(SET);
    let mut manifest = format!(
        "[set]\nid = \"{SET}\"\nuse_case = \"source-fidelity\"\nsplit = \"dev\"\n\
         licence = \"CC-BY-4.0\"\nimages_repo = \"github.com/markhayden/wafflery-evalsets\"\n\
         images_rev = \"0123456789abcdef0123456789abcdef01234567\"\nlabels = \"labels.toml\"\n\
         families = \"families.toml\"\nrules = {{ base = \"source-fidelity\", version = 1 }}\n"
    );
    let (mut labels, mut families) = (String::new(), String::new());
    let items = [
        ("restyled", "scene", "toned", "pass"),
        ("darkened", "toned", "scene", "pass"),
        ("added", "before", "after", "fail"),
    ];
    for (item, source, candidate, expected) in items {
        let mut job =
            "[job]\nuse_case = \"source-fidelity\"\nbrief = \"Keep the scene.\"\n".to_owned();
        for (role, name) in [("candidate", candidate), ("source", source)] {
            let bytes = compared(name);
            let path = format!("items/{item}/{role}.png");
            let pin = whizbang_core::identity::Digest::of_bytes(&bytes).hex();
            job.push_str(&format!(
                "\n[[job.images]]\nid = \"{role}\"\nrole = \"{role}\"\n\
                 source = {{ path = \"{path}\", sha256 = \"{pin}\" }}\n"
            ));
            let file = from.join(SET).join(&path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, &bytes).unwrap();
        }
        std::fs::create_dir_all(set.join("items").join(item)).unwrap();
        std::fs::write(set.join("items").join(item).join("job.toml"), job).unwrap();
        manifest.push_str(&format!(
            "\n[[item]]\nid = \"{item}\"\nfamily = \"{item}\"\njob = \"items/{item}/job.toml\"\n"
        ));
        families.push_str(&format!(
            "[[family]]\nid = \"{item}\"\ndescription = \"a scene\"\n\n"
        ));
        labels.push_str(&format!(
            "[[label]]\nitem = \"{item}\"\nexpected = \"{expected}\"\nwords = \"as painted\"\n\
             adjudicated_by = \"owner\"\nadjudicated_on = \"2026-11-18\"\n\n"
        ));
    }
    std::fs::write(set.join("set.toml"), manifest).unwrap();
    std::fs::write(set.join("labels.toml"), labels).unwrap();
    std::fs::write(set.join("families.toml"), families).unwrap();
    (root, from)
}

#[test]
fn eval_tool_scores_a_tool_on_a_three_item_set() {
    let (dir, root) = bare("cli-eval-tool");
    let (sets_root, from) = three_items(&dir.path().join("set"));
    let set = "source-fidelity/compare-v1";
    let found = whizbang_eval::sets::find(&sets_root, set).unwrap();
    whizbang::evaluation::from_folder(&found, &from, &root, &wiring::recovery()).unwrap();
    let sets_root = sets_root.to_str().unwrap();
    let scored = whizbang(
        &[
            "eval",
            "tool",
            "change_map",
            "--set",
            set,
            "--root",
            sets_root,
        ],
        &root,
    );
    assert_eq!(scored.code, Some(0), "{}", scored.err);
    let out = &scored.out;
    // Each item a read with its run, then the score: the added blob reads more changed
    // than the two passes, so the area is 1; three families are too few to halve.
    for item in ["restyled", "darkened", "added"] {
        assert!(
            out.lines()
                .any(|line| line.starts_with(&format!("item {set}/{item} "))
                    && line.contains(" changed_fraction ")
                    && line.contains(" run ")),
            "{item}: {out}"
        );
    }
    assert!(
        out.contains("change_map changed_fraction (larger is changed): 3 items (1 fail, 2 pass) in 3 families\n"),
        "{out}"
    );
    assert!(out.contains("\nauc 1.0000 (95% "), "{out}");
    assert!(
        out.contains("balanced accuracy inconclusive: 3 families pooled, fewer than 4"),
        "{out}"
    );
    // Each item a run of its own.
    let runs: std::collections::BTreeSet<&str> = out
        .lines()
        .filter_map(|line| line.split(" run ").nth(1))
        .collect();
    assert_eq!(runs.len(), 3, "{out}");
}

// --- a tool file, tried, scored, edited and run again (plan m2c, task X1, item 5) ----------

/// A tool file that reads the pair and says how far apart their mean reds are.
const REDNESS: &str = r#"def run(context):
    def red(image):
        samples = image.rgba[0::4]
        return sum(samples) / (255.0 * len(samples))
    apart = abs(red(context.image('source')) - red(context.image('candidate')))
    return {'readings': [{'subject': 'red', 'values': {'covered': apart}}]}
"#;

/// The digest a run of a tool file noted.
fn noted_digest(root: &Path, run: &str) -> String {
    let record = crate::anthropic::record_of(root, run);
    let settings = record["asked"]["settings"].as_array().unwrap().clone();
    let noted = settings.iter().find(|s| s["name"] == "tool_file_digest");
    let value = &noted.unwrap_or_else(|| panic!("{record}"))["value"];
    value
        .get("text")
        .unwrap_or(value)
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn a_tool_file_is_tried_scored_edited_and_run_again_without_a_rebuild() {
    use crate::tool_file::{BOUNDS, Tools, tool_text, write_tool};
    let first = tool_text("redness", BOUNDS, REDNESS);
    let tools = Tools::new("cli-tool-file", &[("redness.py", first.clone())]);
    let root = tools.root.clone();
    let source = imported(&tools.dir, &root, "before");
    let candidate = imported(&tools.dir, &root, "after");
    let (source, candidate) = (source.parse().unwrap(), candidate.parse().unwrap());
    let (source, candidate) = (
        whizbang_core::artifact::ArtifactId(source),
        whizbang_core::artifact::ArtifactId(candidate),
    );
    // The set's pictures are brought in first: the executor holds the store's writer.
    let (sets_root, from) = three_items(&tools.dir.path().join("set"));
    let set = "source-fidelity/compare-v1";
    let found = whizbang_eval::sets::find(&sets_root, set).unwrap();
    whizbang::evaluation::from_folder(&found, &from, &root, &wiring::recovery()).unwrap();
    // One executor, built once: nothing is rebuilt or installed between the runs.
    let over = tools.over(None);
    let files = over.tool_files().unwrap();
    let declared = whizbang::trying::declared_in("redness", files.as_deref()).unwrap();
    let executor = tools.runner(&over);
    // Tried on a pair, with no judge: align, then the tool, each a run.
    let tried =
        whizbang::trying::read_pair(&executor, &root, &declared, source, candidate).unwrap();
    let text = whizbang::trying::tried_text(&declared, &tried).unwrap();
    assert!(text.lines().nth(1).unwrap().ends_with(" redness"), "{text}");
    assert!(
        text.lines().nth(2).unwrap().starts_with("covered "),
        "{text}"
    );
    let ran = tried.tool.clone().unwrap();
    let digest = whizbang_core::identity::Digest::of_bytes(first.as_bytes()).to_string();
    assert_eq!(noted_digest(&root, ran.as_str()), digest);
    // Scored on a set of three items, each a run.
    let (scored, said) = whizbang::trying::eval_tool(
        &executor,
        &root,
        &sets_root,
        &declared,
        &[set.to_owned()],
        50,
    )
    .unwrap();
    assert_eq!((scored.items, scored.families), (3, 3), "{said}");
    assert!(
        said.contains("redness covered (larger is changed): 3 items"),
        "{said}"
    );
    // Edited: the next run is the edited file's, under the same executor.
    let edited = tool_text("redness", BOUNDS, &REDNESS.replace("abs(", "0.5 * abs("));
    write_tool(tools.dir.path(), "redness.py", &edited);
    let again =
        whizbang::trying::read_pair(&executor, &root, &declared, source, candidate).unwrap();
    let ran_again = again.tool.clone().unwrap();
    let edited_digest = whizbang_core::identity::Digest::of_bytes(edited.as_bytes()).to_string();
    assert_ne!(edited_digest, digest);
    assert_eq!(noted_digest(&root, ran_again.as_str()), edited_digest);
    let (before, after) = (
        tried.primary(&declared).unwrap(),
        again.primary(&declared).unwrap(),
    );
    assert!((after - before / 2.0).abs() < 1e-9, "{before} {after}");
}

#[test]
fn rascal_remove_prompt_until_feather_hands_back_the_mask() {
    // Under a config that gives no filler: with --until feather the recipe stops before
    // its step `fill`, asks no filler, and hands back the feathered mask.
    use crate::recipe::{CONFIG as SMALL, over, record, run, with_small_picture};
    use whizbang_core::contract::Value;
    let dir = Scratch::new("cli-remove-prompt");
    let (root, image) = with_small_picture(&dir);
    let over = over(&dir, SMALL);
    // What `whizbang rascal remove-prompt --image <id> --prompt collar --until feather`
    // asks.
    let asked = door::OperationAsked {
        action: "remove_prompt",
        arguments: vec![
            ("prompt", Value::text("collar")),
            ("until", Value::text("feather")),
        ],
        inputs: vec![("image", "--image", image.as_str())],
        request_id: None,
    };
    let out = run(&over, &root, &asked).unwrap();
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 9, "{out}");
    let recipe_run = lines[0]
        .strip_prefix("run ")
        .unwrap()
        .strip_suffix(" completed")
        .unwrap();
    let steps = [
        "segment", "select", "grow", "close", "plug", "subtract", "feather",
    ];
    for (line, step) in lines[1..8].iter().zip(steps) {
        assert!(
            line.starts_with(&format!("step {step} {step} run ")),
            "{line}"
        );
    }
    let mask = lines[8].strip_prefix("mask ").expect("the mask");
    assert!(mask.starts_with("sha256:"), "{mask}");
    let stored = record(&root, recipe_run);
    assert_eq!(stored.version(), "run/5");
    assert_eq!(stored.steps().len(), 7);

    // With --until select: the selection, the union of both instances' boxes.
    let until = door::OperationAsked {
        arguments: vec![
            ("prompt", Value::text("collar")),
            ("until", Value::text("select")),
            ("set", Value::text("segment.max_instances=1")),
        ],
        ..asked
    };
    let out = run(&over, &root, &until).unwrap();
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 4, "{out}");
    assert!(lines[2].starts_with("step select select run "), "{out}");
    let selected = lines[3].strip_prefix("mask ").expect("the selection");
    let selected: whizbang_core::identity::Digest = selected.parse().unwrap();
    // The recipe's run handed the step's output back as its own, so it is an artifact any
    // later command takes: `rascal grow --mask <it>` shapes it by hand (plan m3, task Q1's
    // third pull request).
    let grown = door::OperationAsked {
        action: "grow",
        arguments: vec![("px", Value::Number(1.0))],
        inputs: vec![("mask", "--mask", lines[3].strip_prefix("mask ").unwrap())],
        request_id: None,
    };
    let out = run(&over, &root, &grown).unwrap();
    assert!(out.lines().next().unwrap().ends_with(" completed"), "{out}");
    let store = Store::open(&root, Arc::new(RealFs)).unwrap();
    let read = whizbang_core::artifact::get(
        &store,
        whizbang_core::artifact::ArtifactType::Mask,
        whizbang_core::artifact::ArtifactId(selected),
        &DecodeLimits::default(),
    );
    let selection = read.unwrap().into_mask().unwrap();
    // The override kept one instance: the first box alone, 10 by 8.
    assert_eq!(selection.core().iter().filter(|s| **s != 0).count(), 80);
    // The real binary takes the command, and refuses a store that is not there.
    let refused = whizbang(
        &[
            "rascal",
            "remove-prompt",
            "--image",
            &image,
            "--prompt",
            "collar",
            "--until",
            "feather",
        ],
        &dir.path().join("nowhere"),
    );
    assert_eq!(refused.code, Some(1), "{}", refused.err);
    // --until fill names the last step, which is the recipe whole: refused before any
    // step runs, and no step's run is made.
    drop(store);
    let last = door::OperationAsked {
        arguments: vec![
            ("prompt", Value::text("collar")),
            ("until", Value::text("fill")),
        ],
        ..until.clone()
    };
    let refused = run(&over, &root, &last).unwrap_err();
    assert!(refused.contains("until"), "{refused}");
    // Without --until the recipe needs a filler, and this config gives none: refused
    // when it is admitted, before any step runs.
    let whole = door::OperationAsked {
        arguments: vec![("prompt", Value::text("collar"))],
        ..until
    };
    assert!(run(&over, &root, &whole).is_err());
}

#[test]
fn rascal_remove_prompt_fills_under_a_scripted_filler() {
    // The whole recipe, its last step `fill` answered by a scripted filler that paints
    // one colour over the support: the composite and the mask rebound onto it come out;
    // every pixel the mask weighs nothing is the picture's, and the core is the paint.
    use crate::recipe::{FILLED, PAINT, over, record, run, with_small_picture};
    use whizbang_core::artifact::{Artifact, ArtifactId, ArtifactType, get};
    use whizbang_core::contract::Value;
    let dir = Scratch::new("cli-remove-prompt-fill");
    let (root, image) = with_small_picture(&dir);
    let over = over(&dir, FILLED);
    let asked = door::OperationAsked {
        action: "remove_prompt",
        arguments: vec![("prompt", Value::text("collar"))],
        inputs: vec![("image", "--image", image.as_str())],
        request_id: None,
    };
    let out = run(&over, &root, &asked).unwrap();
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 11, "{out}");
    assert!(lines[8].starts_with("step fill fill run "), "{out}");
    let id = |line: &str, kind: &str| -> ArtifactId {
        let digest = line.strip_prefix(&format!("{kind} ")).expect(kind);
        ArtifactId(digest.parse().unwrap())
    };
    let (made, mask) = (id(lines[9], "image"), id(lines[10], "mask"));
    let recipe_run = lines[0]
        .strip_prefix("run ")
        .unwrap()
        .strip_suffix(" completed")
        .unwrap();
    assert_eq!(record(&root, recipe_run).steps().len(), 8);
    let store = Store::open(&root, Arc::new(RealFs)).unwrap();
    let limits = DecodeLimits::default();
    let picture = |id: ArtifactId| match get(&store, ArtifactType::Image, id, &limits).unwrap() {
        Artifact::Image(image) => image,
        _ => panic!("no image"),
    };
    let (before, after) = (picture(ArtifactId(image.parse().unwrap())), picture(made));
    let mask = get(&store, ArtifactType::Mask, mask, &limits)
        .unwrap()
        .into_mask()
        .unwrap();
    // The mask is rebound onto the composite.
    assert_eq!(mask.image(), after.identity().pixels);
    let (was, now) = (
        before.pixels(&limits).unwrap(),
        after.pixels(&limits).unwrap(),
    );
    let (mut kept, mut painted) = (0, 0);
    for (at, (a, b)) in now.rgba().chunks(4).zip(was.rgba().chunks(4)).enumerate() {
        let (x, y) = ((at % 64) as u32, (at / 64) as u32);
        let weight = mask.weight(x, y).unwrap();
        if weight == 0.0 {
            assert_eq!(a, b, "{x}, {y}");
            kept += 1;
        } else if weight == 1.0 {
            assert_eq!(&a[..3], PAINT, "{x}, {y}");
            painted += 1;
        }
    }
    assert!(kept > 0 && painted > 0, "{kept} {painted}");
}

#[test]
fn rascal_paste_composites_a_fill_onto_its_base() {
    // A picture of 64 by 48 with a mask on it, a block from 8, 8 to 24, 20, and a fill of
    // one colour of its size, brought in as a picture: `rascal paste` prints the
    // composite, whose every pixel outside the mask is the picture's and whose block is
    // the fill's colour.
    let dir = Scratch::new("cli-paste");
    let root = dir.path().join("store");
    std::fs::create_dir_all(dir.path().join("in")).unwrap();
    let ran = |args: &[&str]| {
        let ran = whizbang(args, &root);
        assert_eq!(ran.code, Some(0), "{args:?}: {}", ran.err);
        ran.out
    };
    drop(Store::open(&root, Arc::new(RealFs)).unwrap());
    let image = crate::anthropic::import_picture(&root, &dir.path().join("in"), "base", 40);
    let mask = crate::fal::put_mask(
        &root,
        whizbang_core::artifact::ArtifactId(image.parse().unwrap()),
    );
    let fill = dir.path().join("in").join("fill.png");
    image::RgbImage::from_pixel(64, 48, image::Rgb([7, 200, 9]))
        .save_with_format(&fill, image::ImageFormat::Png)
        .unwrap();
    let fill = ran(&["store", "import", "--picture", fill.to_str().unwrap()]);
    let fill = fill
        .lines()
        .last()
        .unwrap()
        .strip_prefix("imported image ")
        .unwrap()
        .to_owned();
    let mask = mask.0.to_string();
    let out = ran(&[
        "rascal", "paste", "--image", &image, "--fill", &fill, "--mask", &mask,
    ]);
    let made = printed(&out, "image");
    assert_ne!(made, image);
    let file = dir.path().join("pasted.png");
    ran(&[
        "store",
        "export",
        "--image",
        &made,
        "--out",
        file.to_str().unwrap(),
    ]);
    let shown = pixels_of(&std::fs::read(&file).unwrap());
    let base = dir.path().join("base.png");
    ran(&[
        "store",
        "export",
        "--image",
        &image,
        "--out",
        base.to_str().unwrap(),
    ]);
    let picture = pixels_of(&std::fs::read(&base).unwrap());
    for (at, (a, b)) in shown
        .rgba()
        .chunks(4)
        .zip(picture.rgba().chunks(4))
        .enumerate()
    {
        let (x, y) = (at % 64, at / 64);
        if (8..24).contains(&x) && (8..20).contains(&y) {
            assert_eq!(a, [7, 200, 9, b[3]], "{x}, {y}");
        } else {
            assert_eq!(a, b, "{x}, {y}");
        }
    }
    // A mode this build does not have is the command line's error.
    let other = whizbang(
        &[
            "rascal", "paste", "--image", &image, "--fill", &fill, "--mask", &mask, "--mode",
            "smudge",
        ],
        &root,
    );
    assert_eq!(other.code, Some(2), "{}", other.err);
}

#[test]
fn rascal_window_unwindow_and_apply_run_from_the_command_line() {
    // A picture of 64 by 48 and a mask on it, a block from 8, 8 to 24, 20: `rascal
    // window` prints the crop and the mask on it; `rascal unwindow` writes the crop back,
    // unchanged, as the picture itself; `rascal apply` of a mask with no prescription
    // hands back the picture and the mask as they were.
    let dir = Scratch::new("cli-window");
    let root = dir.path().join("store");
    std::fs::create_dir_all(dir.path().join("in")).unwrap();
    let ran = |args: &[&str]| {
        let ran = whizbang(args, &root);
        assert_eq!(ran.code, Some(0), "{args:?}: {}", ran.err);
        ran.out
    };
    drop(Store::open(&root, Arc::new(RealFs)).unwrap());
    let image = crate::anthropic::import_picture(&root, &dir.path().join("in"), "base", 40);
    let mask = crate::fal::put_mask(
        &root,
        whizbang_core::artifact::ArtifactId(image.parse().unwrap()),
    )
    .0
    .to_string();
    let out = ran(&[
        "rascal",
        "window",
        "--image",
        &image,
        "--mask",
        &mask,
        "--max-side",
        "40",
    ]);
    let (crop, on_crop) = (printed(&out, "image"), printed(&out, "mask"));
    assert_ne!(crop, image);
    let out = ran(&[
        "rascal", "unwindow", "--image", &image, "--crop", &crop, "--mask", &on_crop,
    ]);
    assert_eq!(printed(&out, "image"), image);
    let out = ran(&["rascal", "apply", "--image", &image, "--mask", &mask]);
    assert_eq!(
        (printed(&out, "image"), printed(&out, "mask")),
        (image.clone(), mask.clone())
    );
    // A max side of 0 is the command line's error.
    let zero = whizbang(
        &[
            "rascal",
            "window",
            "--image",
            &image,
            "--mask",
            &mask,
            "--max-side",
            "0",
        ],
        &root,
    );
    assert_eq!(zero.code, Some(2), "{}", zero.err);
}

/// A remote kind at an address of the reserved `.example` domain, which nothing
/// contacts, whose filler is the test kit's, repainting every pixel (plan m3, task O1).
struct Repaints;

impl Kind for Repaints {
    fn name(&self) -> &str {
        "repaints"
    }

    fn make(&self, table: &Table<'_>) -> Result<Made, KindRefused> {
        let model = ModelIdentity::new("repaints", "m", "1");
        let address = table.backend().address().expect("a url");
        let declaration = Declaration::new(model, &[])
            .unwrap()
            .sending_to(Recipient::new(address.origin().clone()))
            .priced(Price::at_most(1000.0).unwrap());
        let filler = whizbang_testkit::capabilities::FakeFiller::new(
            whizbang_testkit::capabilities::Paint::Inverse,
        )
        .declared_as(declaration);
        Ok(Made::Fill(Arc::new(filler)))
    }
}

#[test]
fn rascal_remove_with_a_box_fills_under_a_scripted_filler() {
    use crate::recipe::{record, run, with_small_picture};
    use whizbang_core::contract::Value;
    let dir = Scratch::new("cli-remove");
    let (root, image) = with_small_picture(&dir);
    let config = dir.path().join("whizbang.toml");
    std::fs::write(
        &config,
        "[backends.fill]\nkind = \"repaints\"\nurl = \"https://fill.example.org/v1\"\n\n\
         [egress.recipients.fill]\norigin = \"https://fill.example.org\"\n\
         classes = [\"source_images\", \"derived_images\"]\n",
    )
    .unwrap();
    let mut kinds = wiring::kinds(&OneWorker::installed());
    kinds.register(Arc::new(Repaints)).unwrap();
    let over = Over::nothing().config(Some(&config)).kinds(kinds);
    // What `whizbang rascal remove --image <id> --box "20 16 36 28"` asks: `rect` makes
    // the location, a run of its own, and then the recipe runs on it.
    let store = Arc::new(Store::open(&root, Arc::new(RealFs)).unwrap());
    let executor =
        wiring::runner(wiring::registry_over(&over, false).unwrap(), store, &over).unwrap();
    let rect = door::OperationAsked {
        action: "rect",
        arguments: vec![("box", Value::text("20 16 36 28"))],
        inputs: vec![("image", "--image", image.as_str())],
        request_id: None,
    };
    let made = door::operation(&executor, &rect).unwrap();
    drop(executor);
    let location = made
        .lines()
        .find_map(|line| line.strip_prefix("location "))
        .expect("the location")
        .to_owned();
    let asked = door::OperationAsked {
        action: "remove",
        arguments: Vec::new(),
        inputs: vec![
            ("image", "--image", image.as_str()),
            ("location", "--location", location.as_str()),
        ],
        request_id: None,
    };
    let out = run(&over, &root, &asked).unwrap();
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 10, "{out}");
    let recipe_run = lines[0]
        .strip_prefix("run ")
        .unwrap()
        .strip_suffix(" completed")
        .unwrap();
    let steps = [
        "rasterize",
        "grow",
        "close",
        "plug",
        "subtract",
        "feather",
        "fill",
    ];
    for (line, step) in lines[1..8].iter().zip(steps) {
        assert!(
            line.starts_with(&format!("step {step} {step} run ")),
            "{line}"
        );
    }
    let composite = lines[8].strip_prefix("image ").expect("the composite");
    let mask = lines[9].strip_prefix("mask ").expect("the mask");
    let stored = record(&root, recipe_run);
    assert_eq!(stored.steps().len(), 7);
    // The composite: the box's middle repainted, a far corner the picture's own.
    let store = Store::open(&root, Arc::new(RealFs)).unwrap();
    let get = |kind, id: &str| {
        whizbang_core::artifact::get(
            &store,
            kind,
            whizbang_core::artifact::ArtifactId(id.parse().unwrap()),
            &DecodeLimits::default(),
        )
        .unwrap()
    };
    let pixels = |id: &str| match get(whizbang_core::artifact::ArtifactType::Image, id) {
        whizbang_core::artifact::Artifact::Image(image) => image
            .pixels(&DecodeLimits::default())
            .unwrap()
            .rgba()
            .to_vec(),
        other => panic!("{other:?}"),
    };
    let (before, after) = (pixels(&image), pixels(composite));
    let at = |x: usize, y: usize| (y * 64 + x) * 4;
    let middle = at(28, 22);
    assert_eq!(after[middle], 255 - before[middle]);
    assert_eq!(
        after[at(2, 60)..at(2, 60) + 4],
        before[at(2, 60)..at(2, 60) + 4]
    );
    let rebound = get(whizbang_core::artifact::ArtifactType::Mask, mask)
        .into_mask()
        .unwrap();
    assert_eq!(rebound.image().to_string(), {
        let image = get(whizbang_core::artifact::ArtifactType::Image, composite);
        match image {
            whizbang_core::artifact::Artifact::Image(image) => image.identity().pixels.to_string(),
            other => panic!("{other:?}"),
        }
    });
    drop(store);
    // The real binary takes the commands: `mask-rect` runs with no filler, and `remove`
    // refuses a store that is not there.
    let rect = whizbang(
        &[
            "rascal",
            "mask-rect",
            "--image",
            &image,
            "--box",
            "20 16 36 28",
        ],
        &root,
    );
    assert_eq!(rect.code, Some(0), "{}", rect.err);
    assert!(
        rect.out
            .lines()
            .any(|line| line.starts_with("mask sha256:")),
        "{}",
        rect.out
    );
    let refused = whizbang(
        &[
            "rascal",
            "remove",
            "--image",
            &image,
            "--box",
            "20 16 36 28",
        ],
        &dir.path().join("nowhere"),
    );
    assert_eq!(refused.code, Some(1), "{}", refused.err);
}

#[test]
fn rascal_tighten_inspect_and_a_hard_paste_run_from_the_command_line() {
    // A picture of 64 by 48 (plan m3, task O4): `rascal stroke` makes a stroke on it,
    // `rascal tighten` a mask of it for a repair, `rascal inspect` the overlay and the
    // widths of that mask, and `rascal paste --mode hard` of the picture onto itself hands
    // the picture back. `rascal repair` with no filler configured runs nothing.
    let dir = Scratch::new("cli-tighten");
    let root = dir.path().join("store");
    std::fs::create_dir_all(dir.path().join("in")).unwrap();
    let ran = |args: &[&str]| {
        let ran = whizbang(args, &root);
        assert_eq!(ran.code, Some(0), "{args:?}: {}", ran.err);
        ran.out
    };
    drop(Store::open(&root, Arc::new(RealFs)).unwrap());
    let image = crate::anthropic::import_picture(&root, &dir.path().join("in"), "base", 40);
    let out = ran(&[
        "rascal",
        "stroke",
        "--image",
        &image,
        "--path",
        "8 24; 56 24",
        "--radius-px",
        "4",
    ]);
    let stroke = printed(&out, "location");
    let out = ran(&["rascal", "tighten", "--stroke", &stroke, "--image", &image]);
    let mask = printed(&out, "mask");
    let out = ran(&["rascal", "inspect", "--mask", &mask, "--image", &image]);
    assert!(printed(&out, "overlay").starts_with("sha256:"), "{out}");
    assert!(printed(&out, "widths").starts_with("sha256:"), "{out}");
    let out = ran(&[
        "rascal", "paste", "--image", &image, "--fill", &image, "--mask", &mask, "--mode", "hard",
    ]);
    assert_eq!(printed(&out, "image"), image);
    let out = ran(&[
        "rascal", "paste", "--image", &image, "--fill", &image, "--mask", &mask, "--mode",
        "poisson",
    ]);
    assert_eq!(printed(&out, "image"), image);
    // An unknown mode is the command line's error.
    let unknown = whizbang(
        &[
            "rascal", "paste", "--image", &image, "--fill", &image, "--mask", &mask, "--mode",
            "smudge",
        ],
        &root,
    );
    assert_eq!(unknown.code, Some(2), "{}", unknown.err);
    let repair = whizbang(
        &["rascal", "repair", "--image", &image, "--stroke", &stroke],
        &root,
    );
    assert_eq!(repair.code, Some(1), "{}", repair.err);
}

#[test]
fn rascal_seam_fills_a_synthetic_panorama_under_a_scripted_filler() {
    // A panorama of 128 by 64 whose wrap edge is a step. What `whizbang rascal seam
    // --image <id>` asks, through the door, under a filler of the test's own kind, which
    // the real binary cannot be given: the recipe's eight steps, each a run, and the
    // panorama put back, changed only in the two strips. The real binary then runs the
    // steps of the band itself, `cut`, `line`, `fan`, `profile` and `ramp`, and refuses
    // `rascal seam` on a store that is not there.
    use crate::recipe::{record, run};
    use whizbang_core::artifact::{self, Artifact, ArtifactId, ArtifactType};
    let dir = Scratch::new("cli-seam");
    let root = dir.path().join("store");
    let (w, h) = (128_u32, 64_u32);
    let rgba: Vec<u8> = (0..w * h)
        .flat_map(|at| [(100 + (at % w) / 4) as u8, 120, 90, 255])
        .collect();
    let pixels = CanonicalPixels::from_rgba(w, h, rgba).unwrap();
    let panorama = Image::new(
        whizbang_core::identity::encode::png(&pixels).unwrap(),
        Projection::Equirectangular,
        Topology::Wrap,
        ImageRole::Picture,
        Origin::new(Step::new("fixture", "1")).unwrap(),
        &DecodeLimits::default(),
    )
    .unwrap();
    let image = {
        let store = Store::open(&root, Arc::new(RealFs)).unwrap();
        let mut publication = store.begin().unwrap();
        let id = artifact::put(&mut publication, &Artifact::Image(panorama)).unwrap();
        publication.publish().unwrap();
        id.0.to_string()
    };
    let reset = whizbang(&["spend", "reset", "--allowance", "1"], &root);
    assert_eq!(reset.code, Some(0), "{}", reset.err);
    let config = dir.path().join("whizbang.toml");
    std::fs::write(
        &config,
        "[backends.fill]\nkind = \"repaints\"\nurl = \"https://fill.example.org/v1\"\n\n\
         [egress.recipients.fill]\norigin = \"https://fill.example.org\"\n\
         classes = [\"source_images\", \"derived_images\"]\n",
    )
    .unwrap();
    let mut kinds = wiring::kinds(&OneWorker::installed());
    kinds.register(Arc::new(Repaints)).unwrap();
    let over = Over::nothing().config(Some(&config)).kinds(kinds);
    let asked = door::OperationAsked {
        action: "seam",
        arguments: Vec::new(),
        inputs: vec![("image", "--image", image.as_str())],
        request_id: None,
    };
    let out = run(&over, &root, &asked).unwrap();
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 11, "{out}");
    let steps = [
        ("cut", "cut"),
        ("line", "line"),
        ("fan", "fan"),
        ("band", "profile"),
        ("ramp", "ramp"),
        ("apply", "apply"),
        ("fill", "fill"),
        ("put", "put"),
    ];
    for (line, (step, action)) in lines[1..9].iter().zip(steps) {
        assert!(
            line.starts_with(&format!("step {step} {action} run ")),
            "{line}"
        );
    }
    let recipe_run = lines[0]
        .strip_prefix("run ")
        .and_then(|l| l.strip_suffix(" completed"))
        .unwrap();
    assert_eq!(record(&root, recipe_run).steps().len(), 8);
    let healed = lines[9].strip_prefix("image ").expect("the panorama");
    assert!(lines[10].starts_with("mask sha256:"), "{out}");
    // Only the strips at the wrap edge changed: a strip is 8 columns of 128.
    let store = Store::open(&root, Arc::new(RealFs)).unwrap();
    let read = artifact::get(
        &store,
        ArtifactType::Image,
        ArtifactId(healed.parse().unwrap()),
        &DecodeLimits::default(),
    )
    .unwrap();
    let Artifact::Image(after) = read else {
        panic!("not an image");
    };
    let after = after.pixels(&DecodeLimits::default()).unwrap();
    let (mut changed, mut outside) = (0, 0);
    for (at, (was, is)) in pixels
        .rgba()
        .chunks(4)
        .zip(after.rgba().chunks(4))
        .enumerate()
    {
        let x = at as u32 % w;
        if was != is {
            changed += 1;
            outside += usize::from((8..w - 8).contains(&x));
        }
    }
    assert!(changed >= 2 * 64, "{changed} pixels changed");
    assert_eq!(outside, 0);
    drop(store);

    // The real binary: the band's steps, each printing what it made.
    let ran = |args: &[&str], what: &str| {
        let ran = whizbang(args, &root);
        assert_eq!(ran.code, Some(0), "{args:?}: {}", ran.err);
        let prefix = format!("{what} ");
        ran.out
            .lines()
            .find_map(|l| l.strip_prefix(prefix.as_str()))
            .unwrap_or_else(|| panic!("{args:?}: {}", ran.out))
            .to_owned()
    };
    let canvas = ran(&["rascal", "cut", "--image", &image], "image");
    let line = ran(
        &[
            "rascal",
            "line",
            "--image",
            &canvas,
            "--column",
            "8",
            "--width-px",
            "2",
        ],
        "location",
    );
    let band = ran(
        &["rascal", "fan", "--image", &canvas, "--line", &line],
        "band",
    );
    let mask = ran(&["rascal", "profile", "--band", &band], "mask");
    let ramped = ran(
        &["rascal", "ramp", "--image", &canvas, "--mask", &mask],
        "mask",
    );
    assert_ne!(ramped, mask);
    let refused = whizbang(
        &["rascal", "seam", "--image", &image],
        &dir.path().join("nowhere"),
    );
    assert_eq!(refused.code, Some(1), "{}", refused.err);
}

#[test]
fn lab_run_runs_a_three_image_round_under_a_scripted_filler_and_resumes() {
    use crate::lab::{FILLED_AND_SOURCE, ThreeImages};
    // What `whizbang lab run` does, under a filler of the test's own kind, which the real
    // binary cannot be given: the round stopped after its first cell, then resumed.
    let lab = ThreeImages::new("cli-lab-run", FILLED_AND_SOURCE);
    let over = lab.over(0);
    let asked = std::sync::Mutex::new(0);
    let stop = || {
        let mut asked = asked.lock().unwrap();
        *asked += 1;
        *asked > 1
    };
    let first = lab.run(&over, None, &stop).unwrap();
    assert_eq!((first.started, first.completed, first.not_run), (1, 1, 2));
    let resumed = lab.run(&over, None, &|| false).unwrap();
    assert_eq!(
        resumed.text(),
        "cells: 6\nhanded back by a reference arm: 3\nstarted now: 2\ncompleted: 3\n\
         failed: 0\ninterrupted: 0\nnot run: 0\n"
    );
    assert_eq!(lab.calls(), 3);
    // The real binary: the same round under fal's filler, with nothing left to spend and
    // no recipient approved, is refused before anything runs, and says no corpus id and
    // no path.
    let reset = whizbang(&["spend", "reset", "--allowance", "0"], &lab.root);
    assert_eq!(reset.code, Some(0), "{}", reset.err);
    let before = whizbang(&["runs", "list"], &lab.root).out;
    let config = std::fs::read_to_string(&lab.config).unwrap();
    let recipes = config.lines().last().unwrap();
    let fal = format!(
        "[backends.fill]\nkind = \"service\"\nprovider = \"fal\"\nmodel = \"bria-eraser\"\n\
         key_env = \"LAB_TEST_KEY\"\n\n[spend]\ncall_cap_usd = 0.10\n\n[recipes]\n{recipes}\n"
    );
    std::fs::write(&lab.config, fal).unwrap();
    let env = lab.config.with_file_name("lab.env");
    std::fs::write(&env, "LAB_TEST_KEY=made-up-key-0123456789abcdef\n").unwrap();
    let ran = whizbang(
        &[
            "lab",
            "run",
            "--matrix",
            lab.matrix.to_str().unwrap(),
            "--corpus",
            lab.corpus.to_str().unwrap(),
            "--config",
            lab.config.to_str().unwrap(),
            "--env-file",
            env.to_str().unwrap(),
            "--retry-failed",
        ],
        &lab.root,
    );
    assert_eq!(ran.code, Some(1), "{}", ran.out);
    assert_eq!(ran.out, "");
    // Fal's price, from the config's backend: 40,000 millionths a fill, three fills (the
    // flag retries failed cells only, and none has run).
    assert_eq!(
        ran.err,
        "whizbang: the round's forecast for its 3 runs still to run is 120000 millionths of a \
         dollar, more than the 0 the store's allowance has left; nothing was run\n"
    );
    assert_eq!(whizbang(&["runs", "list"], &lab.root).out, before);
}

#[test]
fn lab_run_pairs_and_decode_remove_prompt_against_a_reference_arm_under_a_scripted_filler() {
    use crate::lab::{FOUND_AND_CLEAN, corpus_text, picture, protocol_text};
    use crate::recipe::{COLLAR, FILLED, PAINT, over};
    use whizbang::harness::Running;
    use whizbang::laboratory::{self, Asked};
    use whizbang::pairs_page;
    use whizbang_eval::lab::decode::{Assignment, Side};
    // The shape of the edits' labs (plan m3, task B1, item 1; task L3): the shipped
    // `remove_prompt`, its phrase given by a text binding, against the reference arm
    // `clean` handing back each picture's clean twin. Three pictures of 64 by 64; the
    // segmenter is the fake that finds `collar`, the filler the fake that paints one
    // colour over the mask's support; nothing is contacted.
    let dir = Scratch::new("cli-lab-remove-prompt");
    let root = dir.path().join("store");
    let corpus = dir.path().join("corpus");
    std::fs::create_dir_all(&corpus).unwrap();
    let made = wiring::opening(&root, wiring::Make::TheRoot, wiring::recovery()).unwrap();
    let store = made.writer().unwrap();
    let dollar = whizbang_core::spend::Amount::new(1_000_000).unwrap();
    whizbang_core::spend::reset(&store, dollar, 1).unwrap();
    drop((store, made));
    let ids = ["a", "b", "c"];
    let mut files: Vec<(String, Vec<u8>, Option<&str>)> = Vec::new();
    for (at, id) in ids.iter().enumerate() {
        let seed = 60 * at as u8;
        files.push(((*id).to_owned(), picture(seed), Some("all")));
        files.push((format!("{id}-clean"), picture(seed + 30), None));
    }
    for (id, bytes, _) in &files {
        std::fs::write(corpus.join(format!("{id}.png")), bytes).unwrap();
    }
    let listed: Vec<(&str, &[u8], Option<&str>)> = (files.iter())
        .map(|(id, bytes, group)| (id.as_str(), bytes.as_slice(), *group))
        .collect();
    let lab = dir.path().join("lab");
    std::fs::create_dir_all(&lab).unwrap();
    std::fs::write(lab.join("corpus.toml"), corpus_text(&listed)).unwrap();
    std::fs::write(
        lab.join("protocol.toml"),
        protocol_text(["found", "clean"], &ids, 3),
    )
    .unwrap();
    let mut matrix = "version = 1\nname = \"tiny-lab\"\ncorpus = \"corpus.toml\"\n\
                      protocol = \"protocol.toml\"\nseed = 7\n"
        .to_owned();
    for id in ids {
        matrix.push_str(&format!(
            "\n[[image]]\nid = \"{id}\"\ninputs = {{ clean = \"{id}-clean\" }}\n\
             texts = {{ prompt = \"collar\" }}\n"
        ));
    }
    matrix.push('\n');
    matrix.push_str(FOUND_AND_CLEAN);
    let matrix_file = lab.join("matrix.toml");
    std::fs::write(&matrix_file, matrix).unwrap();
    let over = over(&dir, FILLED);
    let asked = Asked {
        matrix: &matrix_file,
        corpus: &corpus,
        root: &root,
        over: &over,
        recovery: wiring::recovery(),
    };

    // The round, as `lab run` runs it: three recipe cells, each its eight steps, and three
    // handed back by the reference arm without a run.
    let prepared = laboratory::prepare(&asked).unwrap();
    let store = Arc::new(Store::open(&root, Arc::new(RealFs)).unwrap());
    let registry = wiring::registry_over(&over, false).unwrap();
    let executor = wiring::runner(registry, store, &over).unwrap();
    let running = Running {
        executor: &executor,
        root: &root,
        left_micros: 1_000_000,
        stop: &|| false,
    };
    let report = laboratory::round(&prepared, &running).unwrap();
    assert_eq!(
        report.text(),
        "cells: 6\nhanded back by a reference arm: 3\nstarted now: 3\ncompleted: 3\n\
         failed: 0\ninterrupted: 0\nnot run: 0\n"
    );
    drop(executor);

    // The pairs, as `lab pairs` writes them: the recipe's side is the picture with the
    // collar painted, the reference arm's is the clean twin, pixel for pixel.
    let out = dir.path().join("pages");
    let prepared = laboratory::prepare(&asked).unwrap();
    let written = pairs_page::pairs(
        &prepared,
        &pairs_page::Asked {
            root: &root,
            out: &out,
            limit: None,
            force: false,
        },
    )
    .unwrap();
    let assignment_bytes = std::fs::read(out.join("assignment.json")).unwrap();
    let digest = whizbang_eval::lab::digest_of(&assignment_bytes);
    assert!(
        written.text().starts_with(&format!(
            "pairs: 3 of the round's 3\nassignment: {digest}\n"
        )),
        "{}",
        written.text()
    );
    let page = std::fs::read_to_string(out.join("index.html")).unwrap();
    for named in ["found", "clean", "collar", "remove_prompt"] {
        assert!(!page.contains(named), "the page names {named}");
    }
    let assignment = Assignment::parse(&assignment_bytes).unwrap();
    let rgba = |bytes: &[u8]| image::load_from_memory(bytes).unwrap().to_rgba8();
    let [x0, y0, x1, y1] = COLLAR[0];
    let (cx, cy) = ((x0 + x1) / 2, (y0 + y1) / 2);
    for (n, pair) in assignment.pair.iter().enumerate() {
        let file = |side: &str| {
            let path = out.join(format!("pictures/{:03}-{side}.png", n + 1));
            rgba(&std::fs::read(path).unwrap())
        };
        let (found, clean) = match pair.a_side {
            Side::Left => ("left", "right"),
            Side::Right => ("right", "left"),
        };
        let twin = (files.iter())
            .find(|(id, _, _)| *id == format!("{}-clean", pair.id))
            .unwrap();
        assert_eq!(file(clean), rgba(&twin.1), "{}", pair.id);
        let painted = file(found);
        assert_eq!(&painted.get_pixel(cx, cy).0[..3], PAINT, "{}", pair.id);
        assert_eq!(painted.get_pixel(63, 63), file("source").get_pixel(63, 63));
    }

    // The picks: arm a's side on one pair, b's on two, so a loses to the clean picture;
    // then the real binary's decode.
    let pick: Vec<String> = (assignment.pair.iter().enumerate())
        .map(|(n, p)| {
            let (a, b) = match p.a_side {
                Side::Left => ("left", "right"),
                Side::Right => ("right", "left"),
            };
            let side = if n == 0 { a } else { b };
            format!("{{\"pair\": \"{}\", \"pick\": \"{side}\"}}", p.id)
        })
        .collect();
    let picks = out.join("picks.json");
    std::fs::write(
        &picks,
        format!(
            "{{\"picks_version\": 1, \"assignment_digest\": \"{digest}\", \"pick\": [{}]}}",
            pick.join(", ")
        ),
    )
    .unwrap();
    let decoded = whizbang_storeless(&[
        "lab",
        "decode",
        "--protocol",
        lab.join("protocol.toml").to_str().unwrap(),
        "--assignment",
        out.join("assignment.json").to_str().unwrap(),
        "--picks",
        picks.to_str().unwrap(),
    ]);
    assert_eq!(decoded.code, Some(0), "{}", decoded.err);
    assert_eq!(
        decoded.out,
        "arms: found against clean\ngroup all: pairs 3, decided 3, wins 1, losses 2, same 0, \
         neither 0: worse\n"
    );
}

#[test]
fn lab_run_pairs_and_decode_a_three_image_round_under_a_scripted_filler() {
    use crate::lab::{FILLED_WITH_AND_SOURCE, ThreeImages};
    use whizbang::laboratory::{self, Asked};
    use whizbang::pairs_page;
    use whizbang_eval::lab::decode::{Assignment, Side};
    // The round as `lab run` runs it, under the test's filler (which the real binary
    // cannot be given), then its pairs as `lab pairs` writes them, then the real binary's
    // decode of picks made on them.
    let lab = ThreeImages::new("cli-lab-pairs", FILLED_WITH_AND_SOURCE);
    let over = lab.over(0);
    let report = lab.run(&over, None, &|| false).unwrap();
    assert_eq!((report.completed, report.handed_back), (3, 3));
    let asked = Asked {
        matrix: &lab.matrix,
        corpus: &lab.corpus,
        root: &lab.root,
        over: &over,
        recovery: wiring::recovery(),
    };
    let prepared = laboratory::prepare(&asked).unwrap();
    let pages = lab.corpus.with_file_name("pages");
    let write = |out: &Path, limit: Option<u32>| {
        let asked = pairs_page::Asked {
            root: &lab.root,
            out,
            limit,
            force: false,
        };
        pairs_page::pairs(&prepared, &asked)
    };
    let whole = pages.join("whole");
    let written = write(&whole, None).unwrap();
    let assignment_bytes = std::fs::read(whole.join("assignment.json")).unwrap();
    let digest = whizbang_eval::lab::digest_of(&assignment_bytes);
    assert!(
        written.text().starts_with(&format!(
            "pairs: 3 of the round's 3\nassignment: {digest}\nwritten: index.html, \
             assignment.json and pictures/, "
        )),
        "{}",
        written.text()
    );
    // The page names no arm, no cell and no side of an arm; its pictures are named by
    // their place and side.
    let page = std::fs::read_to_string(whole.join("index.html")).unwrap();
    let assignment = Assignment::parse(&assignment_bytes).unwrap();
    assert!(page.contains(&digest));
    assert!(!page.contains("filled") && !page.contains("a_side"));
    for cell in &prepared.cells {
        assert!(!page.contains(&cell.identity));
    }
    // Each side shows what the assignment says: the reference arm's side is the source
    // itself, the recipe's is not, and its focus is written.
    for (n, pair) in assignment.pair.iter().enumerate() {
        let file =
            |name: &str| std::fs::read(whole.join(format!("pictures/{:03}-{name}.png", n + 1)));
        let (filled, source) = match pair.a_side {
            Side::Left => ("left", "right"),
            Side::Right => ("right", "left"),
        };
        assert_eq!(file(source).unwrap(), file("source").unwrap());
        assert_ne!(file(filled).unwrap(), file("source").unwrap());
        assert!(file(&format!("{filled}-focus")).is_ok());
    }
    // Picks of arm a's side on two pairs and of b's on one: two wins of three.
    let picks = |assignment: &Assignment, digest: &str, out: &Path| {
        let pick: Vec<String> = (assignment.pair.iter().enumerate())
            .map(|(n, p)| {
                let a = if p.a_side == Side::Left {
                    "left"
                } else {
                    "right"
                };
                let b = if a == "left" { "right" } else { "left" };
                format!(
                    "{{\"pair\": \"{}\", \"pick\": \"{}\"}}",
                    p.id,
                    if n == 0 { b } else { a }
                )
            })
            .collect();
        let text = format!(
            "{{\"picks_version\": 1, \"assignment_digest\": \"{digest}\", \"pick\": [{}]}}",
            pick.join(", ")
        );
        std::fs::write(out.join("picks.json"), text).unwrap();
    };
    picks(&assignment, &digest, &whole);
    let protocol = lab.matrix.with_file_name("protocol.toml");
    let decode = |out: &Path| {
        whizbang_storeless(&[
            "lab",
            "decode",
            "--protocol",
            protocol.to_str().unwrap(),
            "--assignment",
            out.join("assignment.json").to_str().unwrap(),
            "--picks",
            out.join("picks.json").to_str().unwrap(),
        ])
    };
    let decoded = decode(&whole);
    assert_eq!(decoded.code, Some(0), "{}", decoded.err);
    assert_eq!(
        decoded.out,
        "arms: filled against source\ngroup all: pairs 3, decided 3, wins 2, losses 1, same 0, \
         neither 0: preferred\n"
    );
    // A lighter round of two pairs: it says, before anything is picked, that its one group
    // cannot reach min_decided, and its decode says it is a subset.
    let lighter = pages.join("lighter");
    let written = write(&lighter, Some(2)).unwrap();
    assert!(
        written.text().contains(
            "group all: 2 of 3 pairs drawn, fewer than min_decided 3: inconclusive whatever \
             is picked\n"
        ),
        "{}",
        written.text()
    );
    let bytes = std::fs::read(lighter.join("assignment.json")).unwrap();
    let drawn = Assignment::parse(&bytes).unwrap();
    picks(&drawn, &whizbang_eval::lab::digest_of(&bytes), &lighter);
    assert!(
        std::fs::read_to_string(lighter.join("index.html"))
            .unwrap()
            .contains("This round is a subset: 2 of its 3 pairs")
    );
    let decoded = decode(&lighter);
    assert_eq!(
        decoded.out,
        "arms: filled against source\na subset of the round: 2 of its 3 pairs, drawn by group \
         from the protocol's seed; a group with fewer than 3 decided pairs is inconclusive, \
         never a win\ngroup all: pairs 2, decided 2, wins 1, losses 1, same 0, neither 0: \
         inconclusive\n"
    );
    // Never in the store's folder (the real binary, before the round is read), and never
    // inside a git work tree unless forced.
    let refused = whizbang(
        &[
            "lab",
            "pairs",
            "--matrix",
            lab.matrix.to_str().unwrap(),
            "--corpus",
            lab.corpus.to_str().unwrap(),
            "--out",
            lab.root.join("pairs").to_str().unwrap(),
        ],
        &lab.root,
    );
    assert_eq!(refused.code, Some(1), "{}", refused.out);
    assert_eq!(
        refused.err,
        "whizbang: the pairs page is never written in the store's folder; name another with \
         --out\n"
    );
    let tree = pages.join("a-repository");
    std::fs::create_dir_all(tree.join(".git")).unwrap();
    let inside = tree.join("pairs");
    let refused = pairs_page::checked_out(&inside, &lab.root, false).unwrap_err();
    assert!(
        refused.starts_with("the folder of --out is inside a git work tree"),
        "{refused}"
    );
    assert!(pairs_page::checked_out(&inside, &lab.root, true).is_ok());
}

#[test]
fn a_planned_call_prints_its_plan_before_it_runs() {
    // Plan m4, task K1, item 1: on a fake action that plans, through the door every
    // command of an operation goes through.
    let (_dir, executor, _door, events) = crate::serve::planning("cli-plan");
    let asked = door::OperationAsked {
        action: "plans",
        arguments: vec![("attempts", whizbang_core::contract::Value::Integer(3))],
        inputs: Vec::new(),
        request_id: None,
    };
    let said = events.clone();
    let mut say = |line: &str| said.lock().unwrap().push(format!("said {line}"));
    let printed = door::operation_saying(executor.as_ref(), &asked, None, &mut say).unwrap();
    assert!(
        printed.starts_with("run ") && printed.ends_with(" completed\n"),
        "{printed}"
    );
    // The plan was said first, and the run was handed that plan.
    let plan = "plan: 2 of 3 attempts asked, unchecked; a test's plan";
    assert_eq!(
        *events.lock().unwrap(),
        [format!("said {plan}"), format!("ran under Some({plan:?})")]
    );
    // A call the action plans nothing for says nothing before it runs.
    events.lock().unwrap().clear();
    let plain = door::OperationAsked {
        arguments: vec![("plain", whizbang_core::contract::Value::Boolean(true))],
        ..asked
    };
    let mut say = |line: &str| said.lock().unwrap().push(format!("said {line}"));
    door::operation_saying(executor.as_ref(), &plain, None, &mut say).unwrap();
    assert_eq!(*events.lock().unwrap(), ["ran under None"]);
}

#[test]
fn a_repeated_request_id_is_told_the_plan_of_its_first_run() {
    // The review of plan m4, task K1: the plan said for a request that was sent before
    // is the one its run had, from that run's record, not the one its call has now.
    use std::sync::atomic::Ordering;
    let (dir, executor, _door, events) = crate::serve::planning("cli-plan-repeat");
    let root = dir.path().join("store");
    let asked = door::OperationAsked {
        action: "plans",
        arguments: vec![("attempts", whizbang_core::contract::Value::Integer(3))],
        inputs: Vec::new(),
        request_id: Some("order-1"),
    };
    let said = events.clone();
    let mut say = |line: &str| said.lock().unwrap().push(format!("said {line}"));
    door::operation_saying(executor.as_ref(), &asked, Some(&root), &mut say).unwrap();
    // What would be planned changes, as when a cap of one call does.
    crate::serve::ALLOWS.store(1, Ordering::SeqCst);
    events.lock().unwrap().clear();
    let mut say = |line: &str| said.lock().unwrap().push(format!("said {line}"));
    let again = door::operation_saying(executor.as_ref(), &asked, Some(&root), &mut say).unwrap();
    assert!(again.contains("the request was sent before"), "{again}");
    // The first run's plan, once; nothing of the plan as it would be now, and no run.
    assert_eq!(
        *events.lock().unwrap(),
        ["said plan: 2 of 3 attempts asked, unchecked; a test's plan"]
    );
    // A new request is told the plan as it is now, before it runs.
    events.lock().unwrap().clear();
    let fresh = door::OperationAsked {
        request_id: Some("order-2"),
        ..asked
    };
    let mut say = |line: &str| said.lock().unwrap().push(format!("said {line}"));
    door::operation_saying(executor.as_ref(), &fresh, Some(&root), &mut say).unwrap();
    let plan = "plan: 1 of 3 attempts asked, unchecked; a test's plan";
    assert_eq!(
        *events.lock().unwrap(),
        [format!("said {plan}"), format!("ran under Some({plan:?})")]
    );
}
