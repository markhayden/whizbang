//! The `segment` action keeps what a backend said beside its answer (plan 1D, task V1;
//! the carry-over row on what a segmenter adapter took and gave): the backend's
//! diagnostics and each instance's predicted IoU are in the run's diagnostic
//! `segment.txt`, which `whizbang runs show` prints. Over a kind of the test's own whose
//! segmenter is the test kit's fake, told what to say, as `cli.rs` does.

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

use crate::cli::{CONFIG, with_picture};
use crate::wired::{has_line, whizbang};

/// What the fake says beside its answer: two lines.
const SAID: &str = "2 instances on mps\nthreshold 0.5";

/// A kind named `finding`, as `cli.rs`'s, whose fake finds two boxes for `circle`, says
/// [`SAID`] and gives each instance a predicted IoU of 0.875.
struct Saying;

impl Kind for Saying {
    fn name(&self) -> &str {
        "finding"
    }

    fn make(&self, table: &Table<'_>) -> Result<Made, KindRefused> {
        let model = ModelIdentity::new("finding", "m", "1");
        let features = [feature::PHRASE, feature::POINT, feature::BOX];
        let address = table.backend().address().expect("a url");
        let declaration = Declaration::new(model, &features)
            .unwrap()
            .sending_to(Recipient::new(address.origin().clone()))
            .priced(Price::at_most(1000.0).unwrap());
        let fake = FakeSegmenter::new()
            .declared_as(declaration)
            .finding("circle", [330, 110, 510, 290])
            .finding("circle", [80, 280, 240, 440])
            .saying(SAID, 0.875);
        Ok(Made::Segment(Arc::new(fake)))
    }
}

#[test]
fn the_backends_diagnostics_and_each_predicted_iou_are_in_the_runs_diagnostic() {
    let dir = Scratch::new("segment-said");
    let (root, image) = with_picture(&dir);
    let config = dir.path().join("whizbang.toml");
    std::fs::write(&config, CONFIG).unwrap();
    let mut kinds = wiring::kinds(&OneWorker::installed());
    kinds.register(Arc::new(Saying)).unwrap();
    let over = Over::nothing().config(Some(&config)).kinds(kinds);
    let store = Arc::new(Store::open(&root, Arc::new(RealFs)).unwrap());
    let executor = wiring::runner(wiring::registry().unwrap(), store, &over).unwrap();
    let asked = SegmentAsked {
        image: &image,
        kind: "phrase",
        query: "circle",
        max_instances: 8,
        backend: None,
        request_id: None,
    };
    let out = door::segment(&executor, &asked, &|run, name| {
        door::diagnostic(&root, run, name)
    })
    .unwrap();
    drop(executor);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3, "{out}");
    let run = lines[0].strip_prefix("run ").unwrap();
    let run = run.strip_suffix(" completed").expect("completed");
    // Each mask's line ends with its predicted IoU, and the command prints it.
    for line in &lines[1..] {
        assert!(line.ends_with(" iou 0.875"), "{line}");
    }
    // The run's diagnostic, as `whizbang runs show` prints it: the masks with their
    // IoU, then each line the backend said.
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
    let kept: Vec<&str> = said.out.lines().collect();
    assert_eq!(kept.len(), 5, "{}", said.out);
    assert_eq!(kept[0], "instances 2");
    let ends = [
        " score 0.9 box 330 110 510 290 iou 0.875",
        " score 0.8 box 80 280 240 440 iou 0.875",
    ];
    for ((line, printed), end) in kept[1..3].iter().zip(&lines[1..]).zip(ends) {
        assert!(line.starts_with("mask sha256:"), "{line}");
        assert!(line.ends_with(end), "{line}");
        assert_eq!(line, printed);
    }
    assert_eq!(
        kept[3..],
        ["backend 2 instances on mps", "backend threshold 0.5"]
    );
    has_line(&said.out, "backend threshold 0.5");
}
