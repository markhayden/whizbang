//! `cache::…`: the decision cache (ADR 0031) under a use case's recipe (plan m3, task Q1,
//! item 1): its key takes the recipe's digest.

use std::sync::Arc;

use whizbang_core::capability::written::read_script;
use whizbang_core::contract::{Executor, Secrets};
use whizbang_squint::job::Job;
use whizbang_testkit::capabilities::FakeJudge;

use super::check::{paired, submitting};
use super::runner::{argument, steps_of};
use super::world::World;

/// The setting `decision_cache` of the judge's step of a recipe run on the pair, under a
/// judge that reports its model.
fn decided(
    world: &World,
    source: &whizbang_core::artifact::Image,
    candidate: &whizbang_core::artifact::Image,
) -> (String, Option<String>) {
    let turns = read_script(submitting(&[]).as_bytes()).unwrap();
    let turns = (turns.into_iter())
        .map(|turn| turn.map(|turn| turn.reported_by(Some("fake-judge"))))
        .collect();
    let judge = Arc::new(FakeJudge::scripted(turns));
    let executor = world.engine(judge, Secrets::none());
    let job = Job::shorthand("source-fidelity", true, 0, None);
    let finished = executor
        .submit(&World::recipe(&job, &[candidate.id(), source.id()]))
        .unwrap();
    drop(executor);
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    let steps = steps_of(world, &finished);
    let judged = &steps[1].1;
    let settings = &judged.record().asked().settings;
    let said = (settings.iter())
        .find(|s| s.name() == whizbang_squint::cache::SETTING)
        .map(|s| format!("{:?}", s.value()))
        .unwrap_or_default();
    (said, argument(judged, "recipe_digest"))
}

#[test]
fn a_changed_use_case_recipe_misses_and_an_unchanged_one_hits() {
    let (mut world, source, candidate, _) = paired();
    world.cached = true;
    let (first, digest) = decided(&world, &source, &candidate);
    assert!(first.contains("\"kept\""), "{first}");
    // Unchanged: the same recipe's run is answered from the first.
    let (again, same) = decided(&world, &source, &candidate);
    assert!(again.contains("\"hit\""), "{again}");
    assert_eq!(same, digest);
    // Changed: a recipe whose text differs, though its steps do not, misses.
    let at = world
        .recipes
        .iter()
        .position(|text| text.contains("name = \"source-fidelity\""))
        .unwrap();
    world.recipes[at].push_str("\n# changed\n");
    let (changed, other) = decided(&world, &source, &candidate);
    assert!(changed.contains("\"kept\""), "{changed}");
    assert_ne!(other, digest);
}
