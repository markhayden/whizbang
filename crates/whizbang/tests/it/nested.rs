//! `nested::…`: how deep a checked removal's calls go (plan m4, task L1, item 13): the
//! loop at the first level, squint's recipe at the second, its step `check` at the
//! third, and a tool the judge calls at the fourth, the last there is.

use serde_json::json;
use whizbang_core::contract::LEVELS_LIMIT;
use whizbang_core::contract::record::Status;

use crate::checked::{Looped, Setup, passes, run_of};

#[test]
fn the_deepest_call_of_a_checked_removal_is_at_the_fourth_level_and_is_admitted() {
    // The judge looks closer at the source before it submits: one call of `view`. The
    // picture's identity is the same in every world of the fixture, and a first world
    // is made to learn it before the judge's script is written.
    let world = Looped::new("loop-levels", &Setup::answering(Vec::new()));
    let looks = json!({"tool_call": {"id": "v1", "name": "view", "arguments": {
        "image": world.image_id().0.to_string(), "bbox": [0.25, 0.25, 0.75, 0.75]}}});
    let world = Looped::new("loop-levels-run", &Setup::answering(vec![looks, passes()]));
    let finished = world.run(&world.call());
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    assert_eq!(world.kit.requests(), (1, 1, 2));
    // From the loop down: its call of squint's recipe, that recipe's step `check`, and
    // the tool that step's judge called, each a run under the one before.
    let looped = world.record(finished.run().as_str());
    let recipe = world.record(&run_of(&looped, "a1_check"));
    assert_eq!(recipe.asked().action.as_deref(), Some("source-fidelity"));
    let step = recipe
        .steps()
        .iter()
        .find(|step| step.action() == "check")
        .unwrap();
    let check = world.record(step.run());
    let tools: Vec<_> = (check.steps().iter())
        .map(|step| (step.action().to_owned(), step.status()))
        .collect();
    assert_eq!(tools, [("view".to_owned(), Some(Status::Completed))]);
    let view = world.record(check.steps()[0].run());
    assert_eq!(view.outcome().status, Status::Completed);
    // Four runs deep, which is as deep as calls nest: nothing was refused on the way.
    assert_eq!(LEVELS_LIMIT, 4);
    for record in [&looped, &recipe, &check, &view] {
        assert!(record.outcome().refusals.is_empty());
    }
    assert!(view.steps().is_empty());
}
