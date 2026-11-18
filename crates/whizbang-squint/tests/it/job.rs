//! The job a check judges (plan 2C, task A1, item 4): a squint job file, read whole and
//! carried through `check` in the contract `job/1`.

use serde_json::{Value as Json, json};
use whizbang_core::contract::record::ArgumentValue;
use whizbang_core::contract::{Executor, Outcome};
use whizbang_squint::job::{Job, Role};
use whizbang_squint::use_case::{self, Checklist};

use super::check::{judged_call, turns};
use super::world::*;

/// Squint's job of Pip the dog (`fixtures/jobs/pip.toml`) and what squint rendered of
/// it (`fixtures/render/pip.txt`), byte for byte, at the commit `docs/sources.toml` pins.
const PIP: &str = include_str!("fixtures/pip.toml");
const PIP_RENDERED: &str = include_str!("fixtures/pip.txt");

#[test]
fn a_squint_job_keeps_its_requirement_ids_scopes_evidence_and_image_labels_through_check() {
    let job = Job::from_toml(PIP).unwrap();
    // Through the contract and back, whole.
    let carried = Job::from_json(&job.to_json()).unwrap();
    assert_eq!(carried, job);
    let requirement = &carried.requirements[1];
    assert_eq!(requirement.id, "RQ2");
    assert_eq!(
        requirement.scope.as_deref(),
        Some(&["candidate".to_owned()][..])
    );
    assert_eq!(
        requirement.evidence.as_deref(),
        Some(&["ref:pip".to_owned()][..])
    );
    assert_eq!(requirement.waives, ["RC3"]);
    // Through a run: its images bound in order, each told with its label.
    let world = World::new();
    let images = [world.other.id(), world.photo.id(), world.other.id()];
    let checklist = Checklist::build(&use_case::resolve(&job).unwrap(), &job);
    let answers = super::use_case::answers(&checklist, "", "held", "confident");
    let call = json!([{"tool_call": {"id": "s1", "name": "submit_verdict", "arguments": answers}}]);
    let script = turns(&[(call, "tool_use")]);
    let judge = judge(&script, &[]);
    let executor = world.engine(judge.clone(), whizbang_core::contract::Secrets::none());
    let finished = executor.submit(&judged_call(&job, &images)).unwrap();
    drop(executor);
    assert!(
        matches!(finished.outcome(), Outcome::Completed { .. }),
        "{:?}",
        finished.outcome()
    );
    let asked = &judge.received()[0];
    assert!(
        asked
            .system()
            .contains("6. RQ1 (must hold; judges candidate; consults ref:pip)")
    );
    assert!(asked.system().contains("7. RQ2 (allowed; on candidate)"));
    let texts: Vec<String> = asked.messages()[0]
        .parts()
        .iter()
        .filter_map(|part| match part {
            whizbang_core::capability::Part::Text(text) => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(
        texts[1].starts_with("Image `ref:pip` (role Reference, Pip, the dog, frame raw:ref:pip,")
    );
    assert!(texts[2].starts_with("Image `ref:bridge` (role Reference, the bridge set,"));
    let left = left(&world.root(), world.fs.clone(), finished.run());
    let kept: Json =
        serde_json::from_slice(left.diagnostic(whizbang_squint::check::JUDGED)).unwrap();
    let bound: Vec<(String, String, String)> = kept["images"]
        .as_array()
        .unwrap()
        .iter()
        .map(|image| {
            let text = |key: &str| image[key].as_str().unwrap_or("").to_owned();
            (text("id"), text("role"), text("artifact"))
        })
        .collect();
    let expected: Vec<(String, String, String)> = job
        .images
        .iter()
        .zip(images)
        .map(|(image, id)| {
            (
                image.id.clone(),
                image.role.word().to_owned(),
                id.to_string(),
            )
        })
        .collect();
    assert_eq!(bound, expected);
    // The job, whole, is the argument the run's record holds.
    let argument = left
        .record()
        .asked()
        .arguments
        .iter()
        .find(|argument| argument.name == "job")
        .map(|argument| argument.value.clone());
    let Some(ArgumentValue::Text(argument)) = argument else {
        panic!("{argument:?}")
    };
    let held: Json = serde_json::from_str(&argument).unwrap();
    assert_eq!(held["requirements"][0]["evidence"], json!(["ref:pip"]));
    assert_eq!(held["images"][2]["label"], "the bridge set");
    assert_eq!(held["contract"], "job/1");
}

#[test]
fn the_checklist_ids_and_the_prompt_of_a_ported_job_are_squints() {
    let job = Job::from_toml(PIP).unwrap();
    let resolved = use_case::resolve(&job).unwrap();
    // Squint's checklist of this job (`crates/squint-core/tests/it/checklist.rs`).
    let checklist = Checklist::build(&resolved, &job);
    assert_eq!(
        checklist.ids().collect::<Vec<_>>(),
        [
            "chk:ob:RC1@candidate",
            "chk:ob:RC2@candidate",
            "chk:ob:RC3@candidate",
            "chk:ob:RQ1@candidate",
            "chk:ref:ref:pip",
            "chk:ref:ref:bridge"
        ]
    );
    assert_eq!(
        checklist.get("chk:ref:ref:bridge").unwrap().instances,
        ["RC1@candidate", "RC2@candidate", "RC3@candidate"]
    );
    // Squint's prompt of it: its template, with what squint rendered of the job's
    // obligations, and no section of tools.
    let template = include_str!("../../prompts/v1.md");
    let expected = template
        .replace("{{obligations}}", PIP_RENDERED)
        .replace("{{tools}}", "");
    assert_eq!(use_case::system(&resolved, &job), expected);
    assert_eq!(job.images[1].role, Role::Reference);
}
