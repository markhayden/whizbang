//! `details::…`: what `check` measures before the judge's first turn, under a scripted
//! judge (plan m3, task U1, item 3: the tiles of a job with one image).

use serde_json::json;
use whizbang_core::capability::Part;
use whizbang_core::contract::record::Status;
use whizbang_core::contract::{Executor, Outcome, Secrets, Value};
use whizbang_squint::details::{TILE_BOXES, TILES_INTRO};

use super::check::{judged_call, turns};
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
        assert!(said.ends_with(&format!("named {}.", tile.id())), "{said}");
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
