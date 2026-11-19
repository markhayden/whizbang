//! `runner::…`: a use case's recipe on the core's runner (plan m3, task Q1, items 1 and 2):
//! what `details` measures and makes reaches the judge's step as that step's own inputs,
//! by the run that made it.

use serde_json::Value as Json;
use whizbang_core::artifact::ArtifactId;
use whizbang_core::capability::Part;
use whizbang_core::contract::record::{ArgumentValue, Status};
use whizbang_core::contract::{Executor, Finished, Outcome, Secrets, Value};
use whizbang_squint::check::MEASURED_IMAGES;
use whizbang_squint::job::Job;

use super::check::{paired, submitting, texts};
use super::world::{Left, World, judge, left};

/// What the run of each step of a recipe left, by the step's action, in order.
pub fn steps_of(world: &World, finished: &Finished) -> Vec<(String, Left)> {
    let recipe = left(&world.root(), world.fs.clone(), finished.run());
    (recipe.record().steps().iter())
        .map(|step| {
            let run = whizbang_core::contract::RunId::parse(step.run()).unwrap();
            let kept = left(&world.root(), world.fs.clone(), &run);
            (step.action().to_owned(), kept)
        })
        .collect()
}

/// The actions a run called, with how each ended.
pub fn calls(left: &Left) -> Vec<(&str, Option<Status>)> {
    (left.record().steps().iter())
        .map(|step| (step.action(), step.status()))
        .collect()
}

/// A text argument of a run, as its record holds it.
pub fn argument(left: &Left, name: &str) -> Option<String> {
    let found = left
        .record()
        .asked()
        .arguments
        .iter()
        .find(|a| a.name == name)?;
    match &found.value {
        ArgumentValue::Text(text) => Some(text.clone()),
        _ => None,
    }
}

/// The ids a run was given at a port, in order.
pub fn given(left: &Left, port: &str) -> Vec<ArtifactId> {
    (left.record().inputs().iter())
        .filter(|input| input.port == port)
        .map(|input| ArtifactId(input.artifact))
        .collect()
}

/// The source-fidelity recipe on the pair that registers, under a judge that submits at
/// once, with these overrides.
pub fn fidelity_recipe(
    world: &World,
    candidate: ArtifactId,
    source: ArtifactId,
    set: &str,
) -> (
    Finished,
    std::sync::Arc<whizbang_testkit::capabilities::FakeJudge>,
) {
    let job = Job::shorthand("source-fidelity", true, 0, None);
    let mut call = World::recipe(&job, &[candidate, source]);
    if !set.is_empty() {
        call = call.argument("set", Value::text(set));
    }
    let judge = judge(&submitting(&[]), &[]);
    let executor = world.engine(judge.clone(), Secrets::none());
    let finished = executor.submit(&call).unwrap();
    drop(executor);
    assert!(
        matches!(finished.outcome(), Outcome::Completed { .. }),
        "{:?}",
        finished.outcome()
    );
    (finished, judge)
}

#[test]
fn the_judge_step_is_handed_aligns_measurements_and_align_runs_once_a_pair() {
    let (world, source, candidate, _) = paired();
    let (finished, judge) = fidelity_recipe(&world, candidate.id(), source.id(), "");
    let steps = steps_of(&world, &finished);
    let actions: Vec<&str> = steps.iter().map(|(action, _)| action.as_str()).collect();
    assert_eq!(actions, ["details", "check"]);
    let (details, judged) = (&steps[0].1, &steps[1].1);
    // align ran once, for the one pair, under `details`; the judge's step called nothing.
    assert_eq!(calls(details), [("align", Some(Status::Completed))]);
    assert_eq!(calls(judged), []);
    // What the judge's step was handed is what align's own run kept: never worked out
    // again.
    let align_run =
        whizbang_core::contract::RunId::parse(details.record().steps()[0].run()).unwrap();
    let align = left(&world.root(), world.fs.clone(), &align_run);
    let report: Json =
        serde_json::from_slice(align.diagnostic(whizbang_squint::align::REPORT)).unwrap();
    let handed = argument(judged, "measured").unwrap();
    let entries = whizbang_squint::measured::entries_of(&handed).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].tool, "align");
    assert_eq!(entries[0].measured.as_ref(), Some(&report));
    // And it is what the judge was told.
    let said = texts(judge.received()[0].messages()[0].parts());
    let told = whizbang_squint::details::align_detail(&report, "candidate", "source");
    assert!(
        said.contains(&format!("{told} Cite it as detail:0.")),
        "{said:?}"
    );
    // The recipe hands back the judge's verdict with its outcome.
    assert_eq!(
        finished.verdict().map(|v| v.decision()),
        Some(whizbang_core::contract::record::Decision::Pass)
    );
}

#[test]
fn the_aligned_frames_and_tiles_of_details_reach_the_judge_step() {
    let (world, source, candidate, _) = paired();
    let (finished, judge) = fidelity_recipe(&world, candidate.id(), source.id(), "");
    let steps = steps_of(&world, &finished);
    let (details, judged) = (&steps[0].1, &steps[1].1);
    let made: Vec<ArtifactId> = (details.record().outputs().iter())
        .map(|output| ArtifactId(output.artifact))
        .collect();
    assert_eq!(made.len(), 1, "the aligned frame");
    assert_eq!(given(judged, MEASURED_IMAGES), made);
    let opening = judge.received()[0].messages()[0].parts().to_vec();
    let frame = (opening.iter())
        .find_map(|part| match part {
            Part::Text(text) if text.starts_with("Image `aligned:candidate:t1`") => {
                Some(text.clone())
            }
            _ => None,
        })
        .unwrap();
    assert!(frame.ends_with(&format!("named {}.", made[0])), "{frame}");

    // Tiles of a job with one image, cut by `details` and shown by the judge's step.
    let world = World::new();
    let job = super::use_case::fitting("reality-checks", None);
    let call = World::recipe(&job, &[world.photo.id()])
        .argument("set", Value::text("details.detail=tiles"));
    let checklist = super::check::checklist_of(&job);
    let last = checklist.ids().last().unwrap().to_owned();
    let items = super::use_case::answers(&checklist, &last, "held", "confident");
    let judge = super::world::judge(&super::check::submits(items), &[]);
    let executor = world.engine(judge.clone(), Secrets::none());
    let finished = executor.submit(&call).unwrap();
    drop(executor);
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    let steps = steps_of(&world, &finished);
    let (details, judged) = (&steps[0].1, &steps[1].1);
    assert_eq!(calls(details), [("view", Some(Status::Completed)); 4]);
    let tiles: Vec<ArtifactId> = (details.record().outputs().iter())
        .map(|output| ArtifactId(output.artifact))
        .collect();
    assert_eq!(tiles.len(), 4);
    assert_eq!(given(judged, MEASURED_IMAGES), tiles);
    let said = texts(judge.received()[0].messages()[0].parts());
    for (at, (place, _)) in whizbang_squint::details::TILE_BOXES.iter().enumerate() {
        let line = said
            .iter()
            .find(|text| text.starts_with(&format!("Tile `tile:candidate:{place}`")))
            .unwrap();
        let named = format!("named {}. Cite it as detail:{at}.", tiles[at]);
        assert!(line.ends_with(&named), "{line}");
    }
}
