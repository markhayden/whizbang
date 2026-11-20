//! The records of the five use cases' recipes (plan m3, task Q1, item 5;
//! `docs/reference/tools/<use case>.md`): rung 1, two worked examples of each, run here
//! through the recipe as shipped, with nothing set (plan m4, task A1: the control arm of
//! the gate of details first), on synthetic pictures under a scripted judge. The pictures beside
//! each record are held byte for byte, and its table of what each example gave is held to
//! what the recipe gives. `write_the_records`, which `scripts/tool-record.sh` runs for any
//! of the five, writes both.

use whizbang_core::artifact::{ArtifactId, Image};
use whizbang_core::contract::Executor;
use whizbang_core::contract::record::Status;
use whizbang_core::identity::{Projection, Topology};
use whizbang_squint::use_case;

use super::check::{checklist_of, submits};
use super::record::{between, replaced, tools};
use super::runner::{calls, steps_of};
use super::support::image_in;
use super::world::{World, judge, left};

/// A picture of `fixtures/align/`, made for the tests of `align`: a drawn scene, the same
/// scene registered on another canvas, and an unrelated drawing.
fn plane(name: &str) -> Image {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/align");
    let file = std::fs::read(dir.join(format!("{name}.png"))).unwrap();
    image_in(file, Projection::Flat, Topology::Bounded)
}

/// One worked example: its name, the candidate's picture, and what the scripted judge
/// answers for the last item of the checklist (every other item held, confidently).
struct Example {
    name: &'static str,
    candidate: &'static str,
    disposition: &'static str,
}

/// The two examples of every record: the scene registered on another canvas, which the
/// judge finds faithful; and an unrelated drawing, which does not register against the
/// scene and which the judge finds in breach of the last item.
const EXAMPLES: [Example; 2] = [
    Example {
        name: "held",
        candidate: "registered",
        disposition: "held",
    },
    Example {
        name: "breached",
        candidate: "unrelated",
        disposition: "violation",
    },
];

/// What one example gave, as the record's table states it.
fn row(world: &World, name: &str, example: &Example, scene: &Image) -> String {
    let brief = (name == "brief-compliance").then_some("A scene, kept as it was.");
    let job = super::use_case::fitting(name, brief);
    let candidate = plane(example.candidate);
    world.hold(&[&candidate]);
    let images: Vec<ArtifactId> = [candidate.id(), scene.id()]
        .into_iter()
        .take(job.images.len())
        .collect();
    let checklist = checklist_of(&job);
    let last = checklist.ids().last().unwrap().to_owned();
    let answers = super::use_case::answers(&checklist, &last, example.disposition, "confident");
    let judge = judge(&submits(answers), &[]);
    let executor = world.engine(judge.clone(), whizbang_core::contract::Secrets::none());
    let finished = executor.submit(&World::recipe(&job, &images)).unwrap();
    drop(executor);
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{name} {}",
        example.name
    );
    let recipe = left(&world.root(), world.fs.clone(), finished.run());
    // The reason the verdict gives is the rule's own sentence over the scripted answers:
    // the record states the decision.
    let (decision, _) = recipe.record().verdict().unwrap();
    let steps = steps_of(world, &finished);
    assert_eq!(steps.len(), 2, "{name}: details, then the judge's step");
    let details = &steps[0].1;
    let called: Vec<String> = calls(details)
        .into_iter()
        .map(|(action, status)| {
            let ended = match status {
                Some(Status::Completed) => "completed",
                Some(_) => "failed",
                None => "not ended",
            };
            format!("`{action}` {ended}")
        })
        .collect();
    let called = if called.is_empty() {
        "none".to_owned()
    } else {
        called.join(", ")
    };
    let handed = details.record().outputs().len();
    // The tools the judge was offered beside the one it answers with.
    let received = judge.received();
    let offered: Vec<String> = (received[0].tools().iter())
        .filter(|tool| tool.name() != "submit_verdict")
        .map(|tool| format!("`{}`", tool.name()))
        .collect();
    let offered = offered.join(", ");
    format!(
        "| {} | {} | {} | {called} | {handed} | {offered} | `{last}` {} | {} |\n",
        example.name,
        example.candidate,
        images.len(),
        example.disposition,
        format!("{decision:?}").to_lowercase(),
    )
}

/// The table of one record: a row for each example.
fn table(name: &str) -> String {
    let mut text = String::from(
        "| Example | Candidate | Images | Calls of `details` | Images handed to the judge | \
         Tools offered to the judge | The judge's answer | Verdict |\n\
         |---|---|---|---|---|---|---|---|\n",
    );
    for example in &EXAMPLES {
        let world = World::new();
        let scene = plane("scene");
        world.hold(&[&scene]);
        text.push_str(&row(&world, name, example, &scene));
    }
    text
}

/// The pictures beside a record: each example's candidate, and the scene for a use case
/// that is given a second image.
fn pictures(name: &str) -> Vec<(String, Vec<u8>)> {
    let pair = super::use_case::fitting(name, Some("A scene."))
        .images
        .len()
        > 1;
    let mut files: Vec<(String, Vec<u8>)> = (EXAMPLES.iter())
        .map(|example| {
            let file = plane(example.candidate).file().to_vec();
            (format!("{}.png", example.candidate), file)
        })
        .collect();
    if pair {
        files.push(("scene.png".to_owned(), plane("scene").file().to_vec()));
    }
    files
}

#[test]
fn each_use_case_record_holds_two_worked_examples_that_its_recipe_gives() {
    for name in use_case::NAMES {
        let path = tools().join(format!("{name}.md"));
        let record =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            between(&record, "measured"),
            table(name),
            "the examples of {name}'s record are stale: run scripts/tool-record.sh {name}"
        );
        for (file, bytes) in pictures(name) {
            let beside = tools().join(name).join(&file);
            let committed =
                std::fs::read(&beside).unwrap_or_else(|e| panic!("{}: {e}", beside.display()));
            assert!(
                committed == bytes,
                "{name}/{file}: run scripts/tool-record.sh {name}"
            );
            assert!(
                record.contains(&format!("({name}/{file})")),
                "{name}: {file}"
            );
        }
    }
}

#[test]
#[ignore = "rewrites the committed records' examples; run it with scripts/tool-record.sh <use case>"]
fn write_the_records() {
    for name in use_case::NAMES {
        let folder = tools().join(name);
        std::fs::create_dir_all(&folder).unwrap();
        for (file, bytes) in pictures(name) {
            std::fs::write(folder.join(file), bytes).unwrap();
        }
        let path = tools().join(format!("{name}.md"));
        let record = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, replaced(&record, "measured", &table(name))).unwrap();
    }
}
