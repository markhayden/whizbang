//! `tool_file::…`: tool files (plan 2C, task X1, items 1, 2 and 5; ADR 0032): a
//! declaration held to its rules when the config is read, an output held to its bounds
//! before it is used, and a capability reached only through the tool's context. The tool
//! worker is the pack's own, started from the test kit's Python; a segment is answered by
//! a fake segmenter of the test's own kind. This file holds the helpers `tool_worker.rs`,
//! `check.rs`, `cli.rs` and `serve.rs` use too.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;
use whizbang::pack::{OneWorker, ToolStart};
use whizbang::tool_file::{self, Declaration, REPORT};
use whizbang::wiring::{self, Over};
use whizbang_backends::pack::{ToolLaunch, ToolWorker};
use whizbang_core::capability::{Declaration as Declared, Fault, FaultKind, feature};
use whizbang_core::config::{Kind, KindRefused, Table};
use whizbang_core::contract::{
    Call, Engine, Executor, Made, ModelIdentity, Outcome, RunId, SystemClock, Value,
};
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::capabilities::FakeSegmenter;
use whizbang_testkit::scratch::Scratch;
use whizbang_testkit::worker::{self as kit, Guard};

use crate::cli::with_picture;

/// The declaration of a tool that reads `source` and `candidate`, with `more` after its
/// measurement, and the code after it.
pub(crate) fn tool_text(name: &str, measure: &str, code: &str) -> String {
    format!(
        "# /// whizbang-tool\n# format = \"tool-file/1\"\n# name = \"{name}\"\n# version = \"0.1.0\"\n\
         # purpose = \"Measures how much of the source a phrase covers.\"\n# evidence = \"measurements\"\n\
         # bindings = [\"source\", \"candidate\"]\n# primary = \"covered\"\n# changed = \"larger\"\n\
         # reduce = \"worst\"\n# readings_most = 4\n# time_limit_s = 20\n\
         # [[measurements]]\n# name = \"covered\"\n# unit = \"ratio\"\n{measure}\
         # [[arguments]]\n# name = \"phrase\"\n# type = \"text\"\n# purpose = \"What to find.\"\n\
         # [[arguments]]\n# name = \"job\"\n# type = \"text\"\n# purpose = \"The item's job.\"\n# ///\n{code}"
    )
}

/// The bounds of `covered`.
pub(crate) const BOUNDS: &str = "# min = 0.0\n# max = 1.0\n";

/// A tool that asks for a segment of the source and reports the share it covers, and
/// the names of the keys its environment holds as its subject.
pub(crate) const COVERS: &str = r#"import os
def run(context):
    keys = ['MADE_UP_API_KEY'] if 'MADE_UP_API_KEY' in os.environ else []
    found = context.segment('source', phrase=context.arguments.get('phrase', 'circle'))
    image = context.image('source')
    on = sum(1 for sample in found[0].mask if sample) if found else 0
    return {'readings': [{'subject': 'keys:' + ','.join(keys), 'values': {'covered': on / (image.width * image.height)}}]}
"#;

/// Writes a tool file into `dir`, readable by its owner and others and writable by its
/// owner alone.
pub(crate) fn write_tool(dir: &Path, file: &str, text: &str) -> PathBuf {
    let path = dir.join(file);
    std::fs::write(&path, text).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    path
}

/// What starts the tool worker in a test: the pack's own, from the test kit's Python,
/// its threads counted with the test's, and a parent environment that holds a made-up
/// key, which the worker must not be given.
pub(crate) fn tool_start() -> ToolStart {
    Arc::new(|_root: Option<&Path>, secrets| {
        let launch =
            ToolLaunch::worker(kit::python().to_path_buf(), kit::source(), kit::LIFETIME_S);
        let parent = [
            ("MADE_UP_API_KEY".into(), "made-up-key-0".into()),
            ("LANG".into(), "C".into()),
        ];
        Ok(
            ToolWorker::new(launch, Arc::new(SystemClock), secrets.clone())
                .with_parent_environment(parent)
                .with_spawn(kit::spawn),
        )
    })
}

/// The one worker of a test: no model worker, and the tool worker of [`tool_start`];
/// held by the test kit's guard, which stops it and fails the test if a process of the
/// pack outlives it.
pub(crate) fn guarded_tools() -> Guard<Arc<OneWorker>> {
    kit::installed(
        || {
            let none: whizbang::pack::Start = Arc::new(|_: &Path, _: &_| {
                Err(Fault::new(FaultKind::Unavailable, "no model worker in this test").not_sent())
            });
            OneWorker::starting_with_tools(none, tool_start())
        },
        |worker| {
            worker.kill();
        },
    )
}

/// A kind named `finding` whose fake finds the disc of the fixture for `circle`, holding
/// each request at the barrier `segment` when it is given one.
pub(crate) struct Finding(pub(crate) Option<Barriers>);

impl Kind for Finding {
    fn name(&self) -> &str {
        "finding"
    }

    fn make(&self, table: &Table<'_>) -> Result<Made, KindRefused> {
        let model = ModelIdentity::new("finding", "m", "1");
        let features = [feature::PHRASE, feature::POINT, feature::BOX];
        let address = table.backend().address().expect("a url");
        let declaration = Declared::new(model, &features)
            .unwrap()
            .sending_to(whizbang_core::egress::Recipient::new(
                address.origin().clone(),
            ))
            .priced(whizbang_core::capability::Price::at_most(1000.0).unwrap());
        let mut fake = FakeSegmenter::new()
            .declared_as(declaration)
            .finding("circle", [330, 110, 510, 290])
            .finding("pool", [32, 24, 96, 72]);
        if let Some(barriers) = &self.0 {
            fake = fake.stopping_at(barriers, "segment");
        }
        Ok(Made::Segment(Arc::new(fake)))
    }
}

/// A store with the fixture picture, a config listing `tools` beside the segmenter of
/// [`Finding`], and the runner over both.
pub(crate) struct Tools {
    pub(crate) worker: Guard<Arc<OneWorker>>,
    pub(crate) dir: Scratch,
    pub(crate) root: PathBuf,
    pub(crate) image: String,
    pub(crate) config: PathBuf,
}

impl Tools {
    pub(crate) fn new(label: &str, tools: &[(&str, String)]) -> Tools {
        let dir = Scratch::new(label);
        let (root, image) = with_picture(&dir);
        let mut paths = Vec::new();
        for (file, text) in tools {
            paths.push(format!(
                "\"{}\"",
                write_tool(dir.path(), file, text).display()
            ));
        }
        let config = dir.path().join("whizbang.toml");
        let text = format!(
            "{}\n[tools]\npaths = [{}]\n",
            crate::cli::CONFIG,
            paths.join(", ")
        );
        std::fs::write(&config, text).unwrap();
        Tools {
            worker: guarded_tools(),
            dir,
            root,
            image,
            config,
        }
    }

    /// What the wiring is over: this config, the kinds of the build and [`Finding`].
    pub(crate) fn over(&self, barriers: Option<Barriers>) -> Over {
        let mut kinds = wiring::kinds(&self.worker);
        kinds.register(Arc::new(Finding(barriers))).unwrap();
        Over::nothing()
            .config(Some(&self.config))
            .kinds(kinds)
            .pack((*self.worker).clone())
    }

    pub(crate) fn runner(&self, over: &Over) -> Engine {
        let store = Arc::new(Store::open(&self.root, Arc::new(RealFs)).unwrap());
        let registry = wiring::registry_over(over, false).unwrap();
        wiring::runner(registry, store, over).unwrap()
    }

    pub(crate) fn image_id(&self) -> whizbang_core::artifact::ArtifactId {
        whizbang_core::artifact::ArtifactId(self.image.parse().unwrap())
    }
}

/// Runs a call and waits for its end.
pub(crate) fn ran(executor: &Engine, call: &Call) -> (RunId, Outcome) {
    let finished = executor.submit(call).unwrap();
    (finished.run().clone(), finished.outcome().clone())
}

#[test]
fn a_declaration_missing_its_bounds_is_refused_when_the_config_is_read() {
    let missing = tool_text(
        "unbounded",
        "# min = 0.0\n",
        "def run(context):\n    return {}\n",
    );
    let tools = Tools::new("tool-file-bounds", &[("unbounded.py", missing)]);
    let over = tools.over(None);
    // Refused where the config is read, before any registry or executor is made.
    let refused = over.tool_files().err().expect("refused");
    assert!(refused.contains("unbounded.py"), "{refused}");
    assert!(refused.contains("missing field `max`"), "{refused}");
    assert!(wiring::registry_over(&over, false).is_err());
    // The same file with its bounds is read, and its declaration is its action's.
    let bounded = tool_text("bounded", BOUNDS, "def run(context):\n    return {}\n");
    let declaration = Declaration::parse(bounded.as_bytes()).unwrap();
    assert_eq!(
        (
            declaration.measurements[0].min,
            declaration.measurements[0].max
        ),
        (0.0, 1.0)
    );
    // A bound that is no bound, and a primary measurement that is not declared, are
    // refused the same way.
    let inverted = tool_text("inverted", "# min = 1.0\n# max = 0.0\n", "");
    assert!(
        Declaration::parse(inverted.as_bytes())
            .unwrap_err()
            .contains("min is under max")
    );
    let elsewhere = bounded.replace("primary = \"covered\"", "primary = \"other\"");
    assert!(Declaration::parse(elsewhere.as_bytes()).is_err());
    // The tool file the pack ships, per-subject comparison, is read by the same rules.
    let shipped = include_bytes!("../../../../pack/tools/per_subject.py");
    let per_subject = Declaration::parse(shipped).unwrap();
    assert_eq!(
        (per_subject.name.as_str(), per_subject.primary.as_str()),
        ("per_subject", "one_minus_iou")
    );
    assert_eq!(per_subject.bindings, ["source", "candidate"]);
}

#[test]
fn an_output_outside_its_declared_bounds_is_refused_before_use() {
    let text = tool_text("covers", BOUNDS, "");
    let declaration = Declaration::parse(text.as_bytes()).unwrap();
    let reading = |value: serde_json::Value| json!({"readings": [{"subject": "a door", "values": {"covered": value}}]});
    let fine = tool_file::checked(&declaration, &reading(json!(0.25)), &[]).unwrap();
    assert_eq!(fine.primary(&declaration), Some(0.25));
    for (output, said) in [
        (reading(json!(1.5)), "outside its declared bounds"),
        (reading(json!(-0.1)), "outside its declared bounds"),
        (reading(json!("0.5")), "outside its declared bounds"),
        (
            json!({"readings": [{"subject": "a", "values": {"other": 0.5}}]}),
            "does not declare",
        ),
        (
            json!({"readings": [], "verdict": "pass"}),
            "does not declare",
        ),
        (
            json!({"readings": vec![json!({"subject": "a", "values": {}}); 5]}),
            "more than the 4",
        ),
        (json!([1, 2]), "not an object"),
    ] {
        let refused = tool_file::checked(&declaration, &output, &[]).unwrap_err();
        assert!(refused.contains(said), "{output}: {refused}");
    }
    // Through a run: the tool answers outside its bounds, and the run fails, stating
    // nothing it read.
    let code = "def run(context):\n    return {'readings': [{'subject': 'x', 'values': {'covered': 7}}]}\n";
    let tools = Tools::new(
        "tool-file-out",
        &[("over.py", tool_text("over", BOUNDS, code))],
    );
    let executor = tools.runner(&tools.over(None));
    let call = Call::new("over")
        .input("source", tools.image_id())
        .input("candidate", tools.image_id());
    let (run, outcome) = ran(&executor, &call);
    let Outcome::Error { message, .. } = outcome else {
        panic!("{outcome:?}");
    };
    assert!(message.contains("outside its declared bounds"), "{message}");
    assert!(whizbang::door::diagnostic(&tools.root, &run, REPORT).is_err());
}

#[test]
fn a_tool_reaches_a_capability_only_through_its_context_and_under_the_runs_allowance() {
    let tools = Tools::new(
        "tool-file-reach",
        &[("covers.py", tool_text("covers", BOUNDS, COVERS))],
    );
    let over = tools.over(None);
    let executor = tools.runner(&over);
    let call = Call::new("covers")
        .input("source", tools.image_id())
        .input("candidate", tools.image_id())
        .argument("phrase", Value::text("circle"));
    let (_, outcome) = ran(&executor, &call);
    // The segmenter costs, and a tool file that declares no segments reserves nothing:
    // the tool's ask is a call under the run's allowance, which has none, and it is
    // refused.
    let Outcome::Error { message, .. } = &outcome else {
        panic!("{outcome:?}");
    };
    assert!(message.contains("CapabilityRefused"), "{message}");
    assert!(
        message.contains("0 are left of its caller's allowance"),
        "{message}"
    );
    // The worker was handed no key of whizbang's environment: a tool that answers
    // without asking says which keys it sees, and sees none.
    let bare = r#"import os
def run(context):
    said = 'key:%s lang:%s' % ('MADE_UP_API_KEY' in os.environ, os.environ.get('LANG', ''))
    return {'readings': [{'subject': said, 'values': {'covered': 0.0}}]}
"#;
    let path = tools.dir.path().join("covers.py");
    write_tool(
        tools.dir.path(),
        "covers.py",
        &tool_text("covers", BOUNDS, bare),
    );
    assert!(path.is_file());
    let (run, outcome) = ran(&executor, &call);
    assert!(matches!(outcome, Outcome::Completed { .. }), "{outcome:?}");
    let report = whizbang::door::diagnostic(&tools.root, &run, REPORT).unwrap();
    let report: serde_json::Value = serde_json::from_slice(&report).unwrap();
    assert_eq!(report["readings"][0]["subject"], json!("key:False lang:C"));
}

// --- regions (plan mE, task T1, item 0) ----------------------------------------------------

/// The declaration of [`tool_text`], returning regions as well: at most two, each label at
/// most 16 bytes, of the kind `change_region`.
fn regions_text(name: &str, code: &str) -> String {
    tool_text(name, BOUNDS, code).replace(
        "# evidence = \"measurements\"\n",
        "# evidence = [\"measurements\", \"regions\"]\n# max_regions = 2\n# max_region_label_bytes = 16\n# region_kind = \"change_region\"\n",
    )
}

/// The images a run hands a tool of two bindings, each 200 by 100.
fn frames() -> Vec<tool_file::Frame> {
    ["source", "candidate"]
        .into_iter()
        .map(|name| tool_file::Frame {
            name: name.to_owned(),
            width: 200,
            height: 100,
        })
        .collect()
}

/// An output of one reading and these regions.
fn located(regions: serde_json::Value) -> serde_json::Value {
    json!({"readings": [{"subject": "pool", "values": {"covered": 0.5}}], "regions": regions})
}

#[test]
fn a_tool_returns_regions_within_its_bounds() {
    use tool_file::RegionShape;
    let declaration = Declaration::parse(regions_text("located", "").as_bytes()).unwrap();
    assert!(declaration.regions());
    let output = located(json!([
        {"frame": "candidate", "box": [10, 20, 200, 100], "label": "pool added", "score": 0.75},
        {"frame": "source", "polygon": [[0, 0], [200, 0], [100, 100]], "label": "a", "score": 0},
    ]));
    let read = tool_file::checked(&declaration, &output, &frames()).unwrap();
    assert_eq!(read.regions.len(), 2);
    assert_eq!(read.regions[0].frame, "candidate");
    assert_eq!(
        read.regions[0].shape,
        RegionShape::Box([10.0, 20.0, 200.0, 100.0])
    );
    assert_eq!(
        (read.regions[0].label.as_str(), read.regions[0].score),
        ("pool added", 0.75)
    );
    assert_eq!(
        read.regions[1].shape,
        RegionShape::Polygon(vec![[0.0, 0.0], [200.0, 0.0], [100.0, 100.0]])
    );
    // What a judge is shown and the diagnostic holds: the regions beside the readings.
    let shown = read.to_json(&declaration);
    assert_eq!(
        shown["regions"][0]["box"],
        json!([10.0, 20.0, 200.0, 100.0])
    );
    assert_eq!(shown["regions"][1]["polygon"][2], json!([100.0, 100.0]));
    // A tool that declares no regions is shown as before, with no list of them.
    let plain = Declaration::parse(tool_text("plain", BOUNDS, "").as_bytes()).unwrap();
    let unlocated = json!({"readings": [{"subject": "pool", "values": {"covered": 0.5}}]});
    let fine = tool_file::checked(&plain, &unlocated, &frames()).unwrap();
    assert!(fine.to_json(&plain).get("regions").is_none());
    // The tool file the pack ships, `outline`, declares regions and their bounds.
    let shipped = include_bytes!("../../../../pack/tools/outline.py");
    let outline = Declaration::parse(shipped).unwrap();
    assert!(outline.regions());
    assert_eq!(
        (
            outline.max_regions,
            outline.max_region_label_bytes,
            outline.region_kind.as_deref()
        ),
        (Some(6), Some(120), Some("change_region"))
    );
    assert_eq!(
        (outline.primary.as_str(), outline.bindings.as_slice()),
        (
            "one_minus_iou",
            ["source", "candidate"].map(String::from).as_slice()
        )
    );
    // Through a run: the tool locates a region on the candidate it was handed, in that
    // image's pixels, and the run's diagnostic records it.
    let code = r#"def run(context):
    image = context.image('candidate')
    return {'readings': [{'subject': 'pool', 'values': {'covered': 0.5}}], 'regions': [{'frame': 'candidate', 'box': [0, 0, image.width, image.height], 'label': 'all of it', 'score': 1.0}]}
"#;
    // One that locates a region on an image the run did not hand it fails its run, and
    // nothing it read is stated (both under one config, so one worker's start).
    let astray = "def run(context):\n    return {'readings': [], 'regions': [{'frame': 'elsewhere', 'box': [0, 0, 1, 1], 'label': 'a', 'score': 0.5}]}\n";
    let tools = Tools::new(
        "tool-file-regions",
        &[
            ("located.py", regions_text("located", code)),
            ("astray.py", regions_text("astray", astray)),
        ],
    );
    let executor = tools.runner(&tools.over(None));
    let call = |name: &str| {
        Call::new(name)
            .input("source", tools.image_id())
            .input("candidate", tools.image_id())
    };
    let (run, outcome) = ran(&executor, &call("astray"));
    let Outcome::Error { message, .. } = outcome else {
        panic!("{outcome:?}");
    };
    assert!(
        message.contains("no image the run handed the tool"),
        "{message}"
    );
    assert!(whizbang::door::diagnostic(&tools.root, &run, REPORT).is_err());
    let (run, outcome) = ran(&executor, &call("located"));
    assert!(matches!(outcome, Outcome::Completed { .. }), "{outcome:?}");
    let report = whizbang::door::diagnostic(&tools.root, &run, REPORT).unwrap();
    let report: serde_json::Value = serde_json::from_slice(&report).unwrap();
    let region = &report["regions"][0];
    assert_eq!(
        (region["frame"].as_str(), region["label"].as_str()),
        (Some("candidate"), Some("all of it"))
    );
    assert_eq!(region["box"][0], json!(0.0));
    assert!(region["box"][2].as_f64().unwrap() > 0.0);
}

#[test]
fn regions_past_the_bounds_or_on_an_unknown_frame_are_refused() {
    // Declarations: regions need both bounds, within theirs, and their kind; those need
    // regions; and measurements come first, always.
    let base = regions_text("located", "");
    for (text, said) in [
        (
            base.replace("# max_regions = 2\n", ""),
            "declares max_regions and max_region_label_bytes",
        ),
        (
            base.replace("max_regions = 2", "max_regions = 65"),
            "max_regions is from 1 to 64",
        ),
        (
            base.replace(
                "max_region_label_bytes = 16",
                "max_region_label_bytes = 201",
            ),
            "max_region_label_bytes is from 1 to 200",
        ),
        (
            base.replace("[\"measurements\", \"regions\"]", "\"measurements\""),
            "are for a tool whose evidence names regions",
        ),
        (
            base.replace("[\"measurements\", \"regions\"]", "[\"regions\"]"),
            "evidence is measurements, or measurements and regions",
        ),
        (
            base.replace("[\"measurements\", \"regions\"]", "\"labels\""),
            "evidence is measurements, or measurements and regions",
        ),
        (
            base.replace("# region_kind = \"change_region\"\n", ""),
            "declares region_kind",
        ),
        (
            base.replace("change_region", "Change Region"),
            "declares region_kind",
        ),
    ] {
        let refused = Declaration::parse(text.as_bytes()).unwrap_err();
        assert!(refused.contains(said), "{said}: {refused}");
    }
    // Outputs: each region is held to the declared bounds and to an image the run holds.
    let declaration = Declaration::parse(base.as_bytes()).unwrap();
    let boxed = |frame: &str, edges: serde_json::Value, label: &str, score: serde_json::Value| json!({"frame": frame, "box": edges, "label": label, "score": score});
    let fine = boxed("candidate", json!([0, 0, 10, 10]), "a", json!(0.5));
    for (regions, said) in [
        (json!([fine, fine, fine]), "3 regions, more than the 2"),
        (
            json!([boxed(
                "candidate",
                json!([0, 0, 10, 10]),
                "seventeen bytes!!",
                json!(0.5)
            )]),
            "1 to the 16 bytes",
        ),
        (
            json!([boxed(
                "candidate",
                json!([0, 0, 10, 10]),
                "tab\there",
                json!(0.5)
            )]),
            "printable text",
        ),
        (
            json!([boxed("candidate", json!([0, 0, 10, 10]), "", json!(0.5))]),
            "printable text of 1 to the 16 bytes",
        ),
        (
            json!([boxed("candidate", json!([0, 0, 10, 10]), "a", json!(1.5))]),
            "score is a number from 0 to 1",
        ),
        (
            json!([boxed("candidate", json!([0, 0, 201, 10]), "a", json!(0.5))]),
            "not inside the 200x100 image candidate",
        ),
        (
            json!([boxed("candidate", json!([5, 0, 5, 10]), "a", json!(0.5))]),
            "with area",
        ),
        (
            json!([boxed("candidate", json!([0, 0, 10]), "a", json!(0.5))]),
            "box is four numbers",
        ),
        (
            json!([boxed(
                "aligned:candidate:t1",
                json!([0, 0, 10, 10]),
                "a",
                json!(0.5)
            )]),
            "no image the run handed the tool",
        ),
        (
            json!([{"frame": "source", "polygon": [[0, 0], [10, 0]], "label": "a", "score": 0.5}]),
            "3 to 64 points",
        ),
        (
            json!([{"frame": "source", "polygon": [[0, 0], [10, 0], [10, 101]], "label": "a", "score": 0.5}]),
            "not inside the 200x100 image source",
        ),
        (
            json!([{"frame": "source", "polygon": [[0, 0], [5, 5], [10, 10]], "label": "a", "score": 0.5}]),
            "polygon has no area",
        ),
        (
            json!([{"frame": "source", "box": [0, 0, 1, 1], "polygon": [[0, 0], [1, 0], [1, 1]], "label": "a", "score": 0.5}]),
            "a box or a polygon, one of the two",
        ),
        (
            json!([{"frame": "source", "box": [0, 0, 1, 1], "label": "a", "score": 0.5, "verdict": "fail"}]),
            "which a region does not",
        ),
        (json!({"frame": "source"}), "regions is not a list"),
    ] {
        let refused =
            tool_file::checked(&declaration, &located(regions.clone()), &frames()).unwrap_err();
        assert!(refused.contains(said), "{regions}: {refused}");
    }
    // A tool that declares no regions may not return them.
    let plain = Declaration::parse(tool_text("plain", BOUNDS, "").as_bytes()).unwrap();
    let refused = tool_file::checked(&plain, &located(json!([])), &frames()).unwrap_err();
    assert!(refused.contains("which it does not declare"), "{refused}");
}

#[test]
fn the_rules_hints_are_given_to_a_tool_as_the_arguments_of_text_it_declares() {
    // Plan mE, task T1, item 4: what `eval tool` adds to each item's arguments.
    use whizbang::trying::{Declared, hinted_arguments};
    use whizbang_core::contract::Value;
    use whizbang_squint::use_case::{Hints, OutlineHints, RegionHints};
    let hints = Hints {
        regions: Some(RegionHints {
            preserve: vec!["wall".into(), "pool".into()],
            ignore: vec!["chair".into()],
        }),
        outline: Some(OutlineHints {
            classes: vec!["pool".into()],
        }),
    };
    let declared = |tool: &str, texts: &[&str]| Declared {
        tool: tool.to_owned(),
        primary: "strongest".to_owned(),
        changed: whizbang_eval::tool::Changed::Larger,
        report: REPORT.to_owned(),
        file: true,
        takes_job: true,
        texts: texts.iter().map(|t| (*t).to_owned()).collect(),
    };
    assert_eq!(
        hinted_arguments(&declared("regions", &["job", "preserve", "ignore"]), &hints),
        [
            ("preserve".to_owned(), Value::text("wall; pool")),
            ("ignore".to_owned(), Value::text("chair")),
        ]
    );
    // Only what the tool declares: a tool that takes no `ignore` is given none.
    assert_eq!(
        hinted_arguments(&declared("regions", &["preserve"]), &hints),
        [("preserve".to_owned(), Value::text("wall; pool"))]
    );
    assert_eq!(
        hinted_arguments(&declared("outline", &["job", "phrases"]), &hints),
        [("phrases".to_owned(), Value::text("pool"))]
    );
    assert!(hinted_arguments(&declared("per_subject", &["phrases"]), &hints).is_empty());
    assert!(hinted_arguments(&declared("regions", &["preserve"]), &Hints::default()).is_empty());
}

// --- a model by name, and labels (plan mE, task T2, item 2) --------------------------------

/// A kind named `labelling` whose fake is a fixed-class segmenter: it declares `phrase` and
/// `labels`, and labels the top 200 rows of the fixture `sky` and the 280 below `floor`.
/// It is a remote backend's kind, as [`Finding`] is (a local one is a model of the
/// catalogue, installed), at an address of the reserved `.example` domain, which nothing
/// contacts, and states the same price.
struct Labelling;

impl Kind for Labelling {
    fn name(&self) -> &str {
        "labelling"
    }

    fn make(&self, table: &Table<'_>) -> Result<Made, KindRefused> {
        let model = ModelIdentity::new("labelling", "eomt-ade20k", "1");
        let address = table.backend().address().expect("a url");
        let declaration = Declared::new(model, &[feature::PHRASE, feature::LABELS])
            .unwrap()
            .sending_to(whizbang_core::egress::Recipient::new(
                address.origin().clone(),
            ))
            .priced(whizbang_core::capability::Price::at_most(1000.0).unwrap());
        let fake = FakeSegmenter::new()
            .declared_as(declaration)
            .finding("sky", [0, 0, 640, 200])
            .finding("floor", [0, 200, 640, 480]);
        Ok(Made::Segment(Arc::new(fake)))
    }
}

/// The config of the routing test: the default, the test's `finding`; `eomt`, the
/// fixed-class fake, whose model is `eomt-ade20k`; and `sam`, of `finding` again, whose
/// model is `sam3` and which declares no `labels`.
const ROUTED: &str = "[backends.segment]\ndefault = \"finding\"\n\n\
     [backends.segment.finding]\nkind = \"finding\"\nurl = \"https://segment.example.org/v1\"\n\n\
     [backends.segment.eomt]\nkind = \"labelling\"\nurl = \"https://labels.example.org/v1\"\nmodel = \"eomt-ade20k\"\n\n\
     [backends.segment.sam]\nkind = \"finding\"\nurl = \"https://segment.example.org/v1\"\nmodel = \"sam3\"\n\n\
     [egress.recipients.segment]\norigin = \"https://segment.example.org\"\nclasses = [\"prompts\"]\n\n\
     [egress.recipients.labels]\norigin = \"https://labels.example.org\"\nclasses = [\"prompts\"]\n";

/// A tool that asks the labels of its source of the model its argument `phrase` names,
/// and a segment of `sky` of it, and reports what it was answered as its subject.
const LABELLED: &str = r#"from whizbang_pack.tool_worker import CapabilityRefused
def run(context):
    model = context.arguments.get('phrase')
    try:
        found = context.labels('source', ['sky', 'floor', 'wall'], model=model)
        counts = {name: found.plane.count(k + 1) for k, (name, _) in enumerate(found.classes)}
        said = 'labels %dx%d %s; at 0,0 %s; at 0,479 %s' % (found.width, found.height, sorted(counts.items()), found.name_at(0, 0), found.name_at(0, 479))
        said += '; sky %d' % len(context.segment('source', phrase='sky', model=model))
    except CapabilityRefused as refused:
        said = 'refused: ' + refused.sentence
    return {'readings': [{'subject': said[:200], 'values': {'covered': 0.0}}]}
"#;

#[test]
fn a_tool_asks_a_model_by_name_and_reads_each_pixels_class() {
    let dir = Scratch::new("tool-file-labels");
    let (root, image) = with_picture(&dir);
    // It declares segments enough for its asks, which its admission reserves.
    let text = tool_text("labelled", BOUNDS, LABELLED).replace(
        "# time_limit_s = 20\n",
        "# time_limit_s = 20\n# segments_most = 16\n",
    );
    let path = write_tool(dir.path(), "labelled.py", &text);
    let config = dir.path().join("whizbang.toml");
    let text = format!("{ROUTED}\n[tools]\npaths = [\"{}\"]\n", path.display());
    std::fs::write(&config, text).unwrap();
    let worker = guarded_tools();
    let mut kinds = wiring::kinds(&worker);
    kinds.register(Arc::new(Finding(None))).unwrap();
    kinds.register(Arc::new(Labelling)).unwrap();
    let over = Over::nothing()
        .config(Some(&config))
        .kinds(kinds)
        .pack((*worker).clone());
    let store = Arc::new(Store::open(&root, Arc::new(RealFs)).unwrap());
    let registry = wiring::registry_over(&over, false).unwrap();
    let executor = wiring::runner(registry, store, &over).unwrap();
    // The routing: a model is its backend's, the default first when it is that model.
    let files = over.tool_files().unwrap().unwrap();
    assert_eq!(files.backend_for("eomt-ade20k").unwrap(), "eomt");
    assert_eq!(files.backend_for("sam3").unwrap(), "sam");
    let id = whizbang_core::artifact::ArtifactId(image.parse().unwrap());
    let said = |model: &str| {
        let call = Call::new("labelled")
            .input("source", id)
            .input("candidate", id)
            .argument("phrase", Value::text(model));
        let (run, outcome) = ran(&executor, &call);
        assert!(matches!(outcome, Outcome::Completed { .. }), "{outcome:?}");
        let report = whizbang::door::diagnostic(&root, &run, REPORT).unwrap();
        let report: serde_json::Value = serde_json::from_slice(&report).unwrap();
        report["readings"][0]["subject"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    // Named, the fixed-class model labels every pixel: the classes some pixel has, a plane
    // of the picture's size, each pixel's class; and its phrase is a class.
    assert_eq!(
        said("eomt-ade20k"),
        "labels 640x480 [('floor', 179200), ('sky', 128000)]; at 0,0 sky; at 0,479 floor; sky 1"
    );
    // A model no backend of the config is, and one whose backend declares no labels, are
    // refused to the tool, which goes on.
    let none = said("tiny");
    assert!(
        none.starts_with("refused: the tool asked for the model tiny"),
        "{none}"
    );
    let plain = said("sam3");
    assert!(plain.starts_with("refused: labels was refused"), "{plain}");
    assert!(plain.contains("labels"), "{plain}");
}
