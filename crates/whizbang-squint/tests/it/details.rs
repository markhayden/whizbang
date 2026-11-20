//! `details::…`: what `check` measures before the judge's first turn, under a scripted
//! judge (plan m3, task U1, item 3: the tiles of a job with one image); and `details` as
//! the first step of a use case's recipe (plan m3, task Q1, item 1).

use serde_json::json;
use whizbang_core::artifact::ArtifactId;
use whizbang_core::capability::Part;
use whizbang_core::contract::record::Status;
use whizbang_core::contract::{Executor, Outcome, Secrets, Value};
use whizbang_squint::details::{TILE_BOXES, TILES_INTRO};

use super::check::{judged_call, paired, texts, turns};
use super::runner::{calls, fidelity_recipe, steps_of};
use super::support::look;
use super::world::{World, judge, left};

#[test]
fn tiles_hands_the_judge_four_overlapping_tiles_at_full_resolution_before_the_first_turn() {
    let world = World::new();
    let job = super::use_case::fitting("reality-checks", None);
    let call = judged_call(&job, &[world.photo.id()])
        .argument("details", Value::text("first"))
        .argument("detail", Value::text("tiles"));
    let script = turns(&[(json!([{"text": "no submission"}]), "end_turn")]);
    let judge = judge(&script, &[]);
    let executor = world.engine(judge.clone(), Secrets::none());
    let finished = executor.submit(&call).unwrap();
    drop(executor);
    assert!(!matches!(finished.outcome(), Outcome::Invalid { .. }));
    // Four calls of view, each a step of the run, made before the judge was asked.
    let left = left(&world.root(), world.fs.clone(), finished.run());
    let steps: Vec<_> = (left.record().steps().iter())
        .map(|step| (step.action(), step.status()))
        .collect();
    assert_eq!(steps, [("view", Some(Status::Completed)); 4]);
    // The opening: the candidate, the words that open the tiles, then each tile labelled
    // with its place and box, and its picture: the crop view cuts, at full resolution.
    let opening = judge.received()[0].messages()[0].parts().to_vec();
    let Part::Text(intro) = &opening[2] else {
        panic!("{:?}", opening[2]);
    };
    assert_eq!(intro, TILES_INTRO);
    // The photograph is 96 by 64.
    let (width, height) = (96, 64);
    for (at, (place, bbox)) in TILE_BOXES.iter().enumerate() {
        let tile = look(&world.photo, *bbox).image;
        let Part::Text(said) = &opening[3 + 2 * at] else {
            panic!("{:?}", opening[3 + 2 * at]);
        };
        assert!(
            said.starts_with(&format!(
                "Tile `tile:candidate:{place}` (tiles (experimental)"
            )),
            "{said}"
        );
        // Each tile is named, and given the id a submission cites it by.
        assert!(
            said.ends_with(&format!("named {}. Cite it as detail:{at}.", tile.id())),
            "{said}"
        );
        let Part::Image(picture) = &opening[4 + 2 * at] else {
            panic!("{:?}", opening[4 + 2 * at]);
        };
        assert_eq!(picture.digest(), tile.identity().pixels);
        // Full resolution: the tile is the candidate's pixels in its box, not scaled.
        let [l, t, r, b] = whizbang_squint::view::snapped(bbox, width, height);
        assert_eq!(
            (picture.width(), picture.height()),
            (r - l, b - t),
            "{place}"
        );
        assert_eq!(left.evidence(at + 1).id(), tile.id());
    }
    // Neighbours overlap: no seam cuts the candidate in two.
    assert!(TILE_BOXES[0].1[2] > TILE_BOXES[1].1[0]);
    assert!(TILE_BOXES[0].1[3] > TILE_BOXES[2].1[1]);
    assert!(
        opening[11..]
            .iter()
            .any(|p| matches!(p, Part::Text(t) if t.starts_with("Checklist.")))
    );
    // A job that relates a pair is refused, naming its use case, and nothing is sent.
    let job = whizbang_squint::job::Job::shorthand("source-fidelity", true, 0, None);
    let paired = judged_call(&job, &[world.other.id(), world.photo.id()])
        .argument("details", Value::text("first"))
        .argument("detail", Value::text("tiles"));
    let judge = super::world::judge(&script, &[]);
    let executor = world.engine(judge.clone(), Secrets::none());
    let finished = executor.submit(&paired).unwrap();
    match finished.outcome() {
        Outcome::Invalid { why } => assert!(why.contains("this source-fidelity job"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(judge.requests(), 0);
}

// --- `details` as the first step of a use case's recipe (plan m3, task Q1, item 1) ---------

#[test]
fn a_pair_that_does_not_register_is_judged_with_its_failure_and_no_comparison() {
    let (world, source, _, unrelated) = paired();
    let set = "details.detail=align,change_map";
    let (finished, judge) = fidelity_recipe(&world, unrelated.id(), source.id(), set);
    let steps = steps_of(&world, &finished);
    let (details, judged) = (&steps[0].1, &steps[1].1);
    // align ran and gave no frame; no comparison ran on the pair.
    assert_eq!(calls(details), [("align", Some(Status::Completed))]);
    assert!(details.record().outputs().is_empty());
    let said = texts(judge.received()[0].messages()[0].parts());
    let align = said.iter().find(|t| t.starts_with("align (")).unwrap();
    assert!(
        align.contains("candidate onto source: not_registered"),
        "{align}"
    );
    let compared = said.iter().find(|t| t.starts_with("change_map (")).unwrap();
    assert!(
        compared.ends_with(
            "aligned:candidate:t1 against source: not measured: candidate did not register \
             onto source, so the two are not in one grid"
        ),
        "{compared}"
    );
    assert!(
        !said.iter().any(|t| t.starts_with("Image `aligned:")),
        "{said:?}"
    );
    // The judge's step still ran and concluded.
    assert_eq!(calls(judged), []);
    assert!(finished.verdict().is_some());
}

#[test]
fn details_none_measures_nothing_and_the_judge_step_still_runs() {
    let (world, source, candidate, _) = paired();
    let set = "details.details=none";
    let (finished, judge) = fidelity_recipe(&world, candidate.id(), source.id(), set);
    let steps = steps_of(&world, &finished);
    let (details, judged) = (&steps[0].1, &steps[1].1);
    assert_eq!(calls(details), []);
    assert!(details.record().outputs().is_empty());
    let handed = super::runner::argument(judged, "measured").unwrap();
    assert!(
        whizbang_squint::measured::entries_of(&handed)
            .unwrap()
            .is_empty()
    );
    let said = texts(judge.received()[0].messages()[0].parts());
    assert!(
        !said
            .iter()
            .any(|t| t == whizbang_squint::details::DETAILS_INTRO),
        "{said:?}"
    );
    assert_eq!(steps[1].1.record().outcome().status, Status::Completed);
    assert!(finished.verdict().is_some());
}

#[test]
fn three_references_are_each_aligned_in_the_order_given() {
    let (world, scene, candidate, unrelated) = paired();
    let job = whizbang_squint::job::Job::shorthand("reference-consistency", false, 3, None);
    let references = [scene.id(), unrelated.id(), world.photo.id()];
    let mut images: Vec<ArtifactId> = vec![candidate.id()];
    images.extend(references);
    let call = World::recipe_first(&job, &images, "");
    let checklist = super::check::checklist_of(&job);
    let last = checklist.ids().last().unwrap().to_owned();
    let items = super::use_case::answers(&checklist, &last, "held", "confident");
    let judge = judge(&super::check::submits(items), &[]);
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
    let details = &steps[0].1;
    assert_eq!(calls(details), [("align", Some(Status::Completed)); 3]);
    // Each call of align registered the candidate onto the next reference, in order.
    for (at, step) in details.record().steps().iter().enumerate() {
        let run = whizbang_core::contract::RunId::parse(step.run()).unwrap();
        let aligned = left(&world.root(), world.fs.clone(), &run);
        let against = super::runner::given(&aligned, "against");
        assert_eq!(against, [references[at]], "reference {}", at + 1);
    }
    let said = texts(judge.received()[0].messages()[0].parts());
    let order: Vec<&str> = (said.iter())
        .filter_map(|t| t.strip_prefix("align (unverified), candidate onto "))
        .map(|rest| rest.split(':').next().unwrap())
        .collect();
    assert_eq!(order, ["reference-1", "reference-2", "reference-3"]);
}

#[test]
fn a_detail_named_where_nothing_is_measured_first_is_refused_by_the_step_and_its_default_is_none() {
    // The review of plan m4's task A1: a recipe called by its own name with
    // `details.detail=tiles` and nothing else measured nothing and said nothing, since the
    // recipes measure nothing first now. The step refuses it, in the door's sentence, and
    // nothing reaches the judge.
    let world = World::new();
    let job = super::use_case::fitting("reality-checks", None);
    let judge = super::world::judge(&super::check::submitting(&[]), &[]);
    let executor = world.engine(judge.clone(), Secrets::none());
    for set in ["details.detail=tiles", "details.detail=align,change_map"] {
        let call = World::recipe(&job, &[world.photo.id()]).argument("set", Value::text(set));
        let finished = executor.submit(&call).unwrap();
        assert_ne!(finished.outcome().name(), "completed", "{set}");
        let recipe = super::world::left(&world.root(), world.fs.clone(), finished.run());
        let step = whizbang_core::contract::RunId::parse(recipe.record().steps()[0].run()).unwrap();
        let details = super::world::left(&world.root(), world.fs.clone(), &step);
        assert_eq!(
            details.record().outcome().invalid.as_deref(),
            Some(whizbang_squint::details::DETAIL_WITHOUT_FIRST),
            "{set}"
        );
    }
    assert_eq!(judge.requests(), 0);
    // The step called by itself with no `details` measures nothing: its own default is
    // `none`, as the recipes' is, and a detail named beside that default is refused too.
    let alone = |detail: Option<&str>| {
        let mut call = whizbang_core::contract::Call::new(whizbang_squint::measured::NAME)
            .argument("use_case", Value::text("reality-checks"))
            .argument("job", Value::text(&job.to_json()))
            .input("images", world.photo.id());
        if let Some(detail) = detail {
            call = call.argument("detail", Value::text(detail));
        }
        executor.submit(&call).unwrap()
    };
    let finished = alone(None);
    assert_eq!(finished.outcome().name(), "completed");
    let kept = super::world::left(&world.root(), world.fs.clone(), finished.run());
    assert!(kept.record().steps().is_empty());
    assert!(kept.record().outputs().is_empty());
    match alone(Some("tiles")).outcome() {
        Outcome::Invalid { why } => {
            assert_eq!(why, whizbang_squint::details::DETAIL_WITHOUT_FIRST)
        }
        other => panic!("{other:?}"),
    }
}
