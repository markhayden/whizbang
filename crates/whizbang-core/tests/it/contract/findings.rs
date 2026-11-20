//! Findings (ADR 0035, "Findings"; plan m4, task K1, item 3): the third kind of result,
//! one JSON document of the kind `findings/1`. The executor refuses findings from an
//! action that does not declare them, and a document that is not of the schema; what it
//! hands a caller is the document as it was read, and what a record keeps is its counts
//! and its places. The action here is a fake that hands back a document a test writes.

use std::sync::Arc;

use serde_json::{Value as Json, json};
use whizbang_core::contract::record::{Decision, RunRecord};
use whizbang_core::contract::{
    Action, ActionDescriptor, Call, Confidence, Context, Disposition, Engine, Executor, FINDINGS,
    FINDINGS_MAX_BYTES, FINDINGS_SCHEMA, Failure, Findings, Finished, Form, Kind, NoBackends,
    Outcome, PreparedCall, Produced, Registry, ResultKind,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::Store;
use whizbang_testkit::fs_shim::ShimFs;
use whizbang_testkit::schema::validate;
use whizbang_testkit::scratch::Scratch;

/// `finds`: hands back the document it was built with.
struct Finds(ActionDescriptor, Json);

impl Action for Finds {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, _: &PreparedCall, _: &Context) -> Result<Produced, Failure> {
        Ok(Produced::nothing().with_findings(self.1.clone()))
    }
}

/// Runs `finds` over a new store, declared to hand back findings or not.
fn run(declared: bool, document: &Json) -> (Finished, RunRecord) {
    let dir = Scratch::new("findings");
    let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(ShimFs::new())).unwrap());
    let draft = ActionDescriptor::draft("finds", "1.0.0", Kind::Primitive, "A test action.");
    let draft = if declared {
        draft.results(ResultKind::Findings)
    } else {
        draft
    };
    let mut registry = Registry::new();
    let finds = Finds(draft.build().unwrap(), document.clone());
    registry.register(Arc::new(finds)).unwrap();
    let engine = Engine::over(
        registry,
        Arc::new(NoBackends),
        store.clone(),
        Digest::of_bytes(b""),
    );
    let finished = engine.submit(&Call::new("finds")).unwrap();
    let generation = store.snapshot().unwrap();
    let read = |entry: &_| store.read(entry);
    let id = finished.run().as_str();
    let record = whizbang_core::contract::record::stored(&generation, id, read);
    (finished, record.unwrap().unwrap())
}

fn image() -> String {
    Digest::of_bytes(b"a candidate").to_string()
}

/// A checklist item about one obligation with a box, and a separate observation that
/// cites two with a polygon; an observed one with no confidence.
pub fn document() -> Json {
    json!({
        "findings": FINDINGS,
        "decision": "fail",
        "complete": false,
        "left_out": {"findings": 2, "places": 1},
        "items": [
            {"form": "item", "obligations": ["RQ1"], "disposition": "violation",
             "confidence": "confident",
             "place": {"image": image(), "box": [0.25, 0.25, 0.5, 0.75]}},
            {"form": "observation", "obligations": ["SF1", "RQ1"], "disposition": "violation",
             "confidence": "tentative",
             "place": {"image": image(), "polygon": [[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]}},
            {"form": "observation", "obligations": ["SF4"], "disposition": "observed",
             "confidence": null, "place": null},
        ],
    })
}

fn error_of(finished: &Finished) -> String {
    match finished.outcome() {
        Outcome::Error { kind, message } => format!("{kind}: {message}"),
        other => panic!("not an error: {other:?}"),
    }
}

mod executor {
    use super::*;

    #[test]
    fn findings_from_an_action_that_declares_them_are_handed_to_its_caller_and_counted_in_its_record()
     {
        let written = document();
        let schema: Json = serde_json::from_str(FINDINGS_SCHEMA).unwrap();
        validate(&schema, &written).unwrap();
        let (finished, record) = run(true, &written);
        assert_eq!(finished.outcome().name(), "completed");
        let handed = finished.findings().unwrap();
        // As it was written, and every obligation of a finding kept.
        assert_eq!(handed.to_json(), written);
        // The same document, made from its parts.
        let made = Findings::of(
            handed.decision(),
            handed.items().to_vec(),
            handed.left_out(),
        );
        assert_eq!(made.as_ref(), Ok(handed));
        let none = Findings::of(Decision::Pass, Vec::new(), Default::default()).unwrap();
        assert!(none.is_complete() && none.items().is_empty());
        assert_eq!(handed.decision(), Decision::Fail);
        assert!(!handed.is_complete());
        assert_eq!(handed.left_out().findings, 2);
        let second = &handed.items()[1];
        assert_eq!(second.form, Form::Observation);
        assert_eq!(second.obligations, ["SF1", "RQ1"]);
        assert_eq!(second.disposition, Disposition::Violation);
        assert_eq!(second.confidence, Some(Confidence::Tentative));
        // The record keeps counts by obligation, disposition and confidence, and the
        // places: a finding that cites two obligations counts under each.
        assert_eq!(record.version(), "run/6");
        let kept = serde_json::to_value(record.findings().unwrap()).unwrap();
        assert_eq!(kept["decision"], "fail");
        assert_eq!(kept["complete"], false);
        assert_eq!(kept["left_out"], json!({"findings": 2, "places": 1}));
        let count = |obligation: &str, confidence: Json| {
            json!({"obligation": obligation, "disposition": "violation",
                   "confidence": confidence, "count": 1})
        };
        assert_eq!(
            kept["counts"],
            json!([
                count("RQ1", json!("confident")),
                count("RQ1", json!("tentative")),
                count("SF1", json!("tentative")),
                {"obligation": "SF4", "disposition": "observed", "confidence": null, "count": 1},
            ])
        );
        assert_eq!(kept["places"].as_array().unwrap().len(), 2);
        assert_eq!(kept["places"][0], written["items"][0]["place"]);
    }

    #[test]
    fn findings_from_an_action_that_does_not_declare_them_end_the_run_as_an_error() {
        let (finished, record) = run(false, &document());
        assert_eq!(
            error_of(&finished),
            "output: the action handed back findings, and its descriptor declares none"
        );
        assert_eq!(finished.findings(), None);
        assert_eq!(record.findings(), None);
        assert_eq!(record.version(), "run/4");
    }

    #[test]
    fn findings_that_are_not_of_their_schema_end_the_run_as_an_error() {
        let schema: Json = serde_json::from_str(FINDINGS_SCHEMA).unwrap();
        let with = |change: &dyn Fn(&mut Json)| {
            let mut document = document();
            change(&mut document);
            document
        };
        // What the schema refuses, the executor refuses.
        let by_schema: Vec<(&str, Json)> = vec![
            ("not an object", json!([1, 2])),
            (
                "another kind",
                with(&|d| d["findings"] = json!("findings/2")),
            ),
            (
                "a decision that is none",
                with(&|d| d["decision"] = json!("maybe")),
            ),
            (
                "no decision",
                with(&|d| drop(d.as_object_mut().unwrap().remove("decision"))),
            ),
            (
                "a text of the judge's",
                with(&|d| d["items"][0]["why"] = json!("it is there")),
            ),
            ("an unknown field", with(&|d| d["note"] = json!("x"))),
            (
                "no obligation",
                with(&|d| d["items"][0]["obligations"] = json!([])),
            ),
            (
                "a disposition that is none",
                with(&|d| d["items"][0]["disposition"] = json!("bad")),
            ),
            (
                "a box outside the picture",
                with(&|d| d["items"][0]["place"]["box"][2] = json!(1.5)),
            ),
            (
                "a box and a polygon",
                with(&|d| {
                    d["items"][0]["place"]["polygon"] = json!([[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]])
                }),
            ),
            (
                "a polygon of two points",
                with(&|d| d["items"][1]["place"]["polygon"] = json!([[0.0, 0.0], [1.0, 0.0]])),
            ),
            (
                "an image that is no identity",
                with(&|d| d["items"][0]["place"]["image"] = json!("x")),
            ),
            (
                "33 findings",
                with(&|d| {
                    let one = d["items"][2].clone();
                    d["items"] = Json::Array(vec![one; 33]);
                }),
            ),
        ];
        for (what, document) in &by_schema {
            assert!(
                validate(&schema, document).is_err(),
                "the schema takes {what}"
            );
        }
        // What the schema cannot say, and says beside itself.
        let beside: Vec<(&str, Json)> = vec![
            (
                "an id that is none",
                with(&|d| d["items"][0]["obligations"] = json!(["rq 1"])),
            ),
            (
                "an item that cites two",
                with(&|d| d["items"][0]["obligations"] = json!(["RQ1", "RQ2"])),
            ),
            (
                "an obligation cited twice",
                with(&|d| d["items"][1]["obligations"] = json!(["SF1", "SF1"])),
            ),
            (
                "observed with a confidence",
                with(&|d| d["items"][2]["confidence"] = json!("confident")),
            ),
            (
                "a violation without one",
                with(&|d| d["items"][0]["confidence"] = Json::Null),
            ),
            (
                "a box whose left is its right",
                with(&|d| d["items"][0]["place"]["box"][0] = json!(0.5)),
            ),
            (
                "complete, with something left out",
                with(&|d| d["complete"] = json!(true)),
            ),
            (
                "nine places",
                with(&|d| {
                    let one = d["items"][1].clone();
                    d["items"] = Json::Array(vec![one; 9]);
                }),
            ),
            (
                "as large as the shape allows",
                with(&|d| {
                    let points: Vec<Json> = (0..64)
                        .map(|n| json!([f64::from(n) / 977.0, 0.123_456_789_012]))
                        .collect();
                    let mut one = d["items"][1].clone();
                    one["place"]["polygon"] = Json::Array(points);
                    one["obligations"] =
                        (1..=16).map(|n| json!(format!("ABCDEFGH{n:04}"))).collect();
                    d["items"] = Json::Array(vec![one; 8]);
                    let size = serde_json::to_vec(d).unwrap().len();
                    assert!(size <= FINDINGS_MAX_BYTES, "{size}: eight such fit");
                }),
            ),
        ];
        for (what, document) in by_schema.iter().chain(&beside) {
            // The last of them fits: a document of the shape stays under the bound.
            let fits = *what == "as large as the shape allows";
            assert_eq!(Findings::read(document).is_ok(), fits, "{what}");
            let (finished, record) = run(true, document);
            if fits {
                assert_eq!(finished.outcome().name(), "completed");
                continue;
            }
            let said = error_of(&finished);
            assert!(
                said.starts_with("output: the action's findings are not findings/1: "),
                "{what}: {said}"
            );
            // The complaint quotes nothing of the document.
            assert!(
                !said.contains("it is there") && !said.contains("rq 1"),
                "{said}"
            );
            assert_eq!(finished.findings(), None);
            assert_eq!(record.findings(), None);
        }
        // A document over the bound is refused for its size, before its shape is read.
        let mut large = document();
        let one = large["items"][2].clone();
        large["items"] = Json::Array(vec![one; 400]);
        assert!(serde_json::to_vec(&large).unwrap().len() > FINDINGS_MAX_BYTES);
        assert_eq!(
            Findings::read(&large).unwrap_err(),
            format!("it is over {FINDINGS_MAX_BYTES} bytes")
        );
        let (finished, _) = run(true, &large);
        assert_eq!(
            error_of(&finished),
            "output: the action's findings are not findings/1: it is over 32768 bytes"
        );
    }
}
