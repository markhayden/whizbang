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
use whizbang_core::capability::{Declaration, Price, feature};
use whizbang_core::config::{Kind, KindRefused, Table};
use whizbang_core::contract::{Made, ModelIdentity};
use whizbang_core::egress::Recipient;
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::capabilities::FakeSegmenter;
use whizbang_testkit::scratch::Scratch;

use crate::model_tier::FIXTURE;
use crate::wired::{has_line, whizbang};

/// The boxes the fake finds for `circle` in the fixture: around the disc, around its
/// upper half, and around the square.
const FOUND: [[u32; 4]; 3] = [
    [330, 110, 510, 290],
    [330, 110, 510, 200],
    [80, 280, 240, 440],
];

const CONFIG: &str = "[backends.segment]\nkind = \"finding\"\nurl = \"https://segment.example.org/v1\"\n\n\
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
fn with_picture(dir: &Scratch) -> (PathBuf, String) {
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
    let config = dir.path().join("whizbang.toml");
    std::fs::write(&config, CONFIG).unwrap();
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
