//! `located::…`: the located retry of the loop `remove_checked` (plan m4, task L2; ADR
//! 0039, "The located retry"), through the loop on the fixture of `checked`: a retry
//! aimed at the places a finding named, and at no other. The kit's segmenter finds
//! what a test says inside a crop, and nothing when it says nothing.

use serde_json::Value as Json;
use whizbang_core::artifact::ArtifactId;
use whizbang_core::contract::record::{Changed, Decision, RunRecord};
use whizbang_core::contract::{Capability, Executor, Finished, FoundAt};
use whizbang_core::identity::Digest;
use whizbang_rascal::checked::located::{
    CUT_MAX, CUT_MIN, Left, LeftOut, PLACES_MAX, Place, TILED_MAX, Usable, usable,
};
use whizbang_rascal::checked::{CHECK, ITSELF, REMOVE, VIEW};

use crate::checked::{
    Came, Looped, Setup, argument, bound_of, boxed, calls, came, items, observation, output,
    passes, remains, run_of, submission, weights,
};

/// A place on the candidate, right of what the fake finds in the picture: the pixels
/// from 40 to 56 across and from 16 to 32 down.
pub const PLACE: [f64; 4] = [0.625, 0.25, 0.875, 0.5];

/// The same place in pixels, and what is cut to look into it: a twentieth wider each way.
const OWN: [u32; 4] = [40, 16, 56, 32];
const CUT: [u32; 4] = [39, 15, 57, 33];

/// What the kit's segmenter finds inside a crop, in the crop's pixels; in the cut of
/// [`PLACE`] that is from 43 to 49 across and from 19 to 25 down of the picture.
const INSIDE: [u32; 4] = [4, 4, 10, 10];

/// A second place, under the first and to the left: from 8 to 24 across, 40 to 56 down.
const SECOND: [f64; 4] = [0.125, 0.625, 0.375, 0.875];

/// A place in the picture's far corner, which no mask of these tests comes near.
const FAR: [f64; 4] = [0.75, 0.75, 1.0, 1.0];

const NOTHING: &str =
    "no finding of attempt 1 says with confidence that the named thing is still there";
const DAMAGED: &str =
    "attempt 1 changed something that must be preserved, which painting more cannot mend";

/// The checklist with these answers, and each obligation's answer citing this box.
pub fn citing(but: &[crate::checked::Answered], at: &[(&str, [f64; 4])]) -> Vec<Json> {
    let mut answered = items(but);
    for (obligation, sides) in at {
        let id = format!("chk:ob:{obligation}@candidate");
        let item = answered.iter_mut().find(|item| item["id"] == id.as_str());
        item.unwrap()["location"] = boxed(*sides);
    }
    answered
}

/// A submission whose one finding is a confident violation of `RQ1` at this box: the
/// named thing is still there, and here.
pub fn remains_at(sides: [f64; 4]) -> Json {
    submission(
        citing(&[("RQ1", "violation", "confident")], &[("RQ1", sides)]),
        Vec::new(),
    )
}

/// A separate observation that cites these obligations at this box.
pub fn seen_at(cites: &[&str], disposition: &str, confidence: &str, sides: [f64; 4]) -> Json {
    let mut seen = observation(cites, disposition, confidence);
    seen["location"] = boxed(sides);
    seen
}

/// A kit whose segmenter finds [`INSIDE`] inside every crop.
pub fn finding(answers: Vec<Json>) -> Setup {
    let mut setup = Setup::answering(answers);
    setup.inside = vec![INSIDE];
    setup
}

fn labels(record: &RunRecord) -> Vec<String> {
    calls(record).into_iter().map(|call| call.0).collect()
}

/// The identity a call of the loop's committed at a port.
fn made(world: &Looped, record: &RunRecord, label: &str, port: &str) -> ArtifactId {
    let run = world.record(&run_of(record, label));
    let mut outputs = run.outputs().iter();
    let found = outputs.find(|output| output.port == port);
    ArtifactId(
        found
            .unwrap_or_else(|| panic!("{label} has no {port}"))
            .artifact,
    )
}

/// The calls an attempt made to look into its places, in order, each with its record:
/// those that are no stretch of a recipe and have no label in the loop's record.
pub fn looked(world: &Looped, finished: &Finished, attempt: usize) -> Vec<(String, RunRecord)> {
    let attempts = world.attempts(finished);
    let runs = attempts.list[attempt - 1].runs.iter();
    runs.map(|run| world.record(run))
        .filter_map(|run| {
            let action = run.asked().action.clone()?;
            (action != REMOVE && action != CHECK).then_some((action, run))
        })
        .collect()
}

/// The actions of [`looked`].
fn looks(world: &Looped, finished: &Finished, attempt: usize) -> Vec<String> {
    let looked = looked(world, finished, attempt);
    looked.into_iter().map(|(action, _)| action).collect()
}

/// An argument of the call a record is of, whatever its kind, as it is printed.
fn asked(record: &RunRecord, name: &str) -> String {
    let mut arguments = record.asked().arguments.iter();
    let found = arguments.find(|argument| argument.name == name);
    found.map_or_else(String::new, |argument| format!("{:?}", argument.value))
}

/// The mask a first attempt that passes paints: what every retry of these tests
/// starts from.
fn first_mask() -> Vec<f32> {
    let (world, finished) = crate::checked::looped("located-first", &finding(vec![passes()]));
    weights(&world, output(&finished, "mask"))
}

/// What a mask weighs at a pixel of the picture.
fn weight(weights: &[f32], x: usize, y: usize) -> f32 {
    weights[y * 64 + x]
}

/// The four calls of a first attempt, and those of a retry that looks into one place.
const FIRST: [&str; 4] = ["a1_find", "a1_shape", "a1_fill", "a1_check"];

fn one_attempt(world: &Looped, finished: &Finished, verdict: Decision, ended: &str, case: &str) {
    let came = came(world, finished);
    assert_eq!(came.attempts, [(Some(verdict), None)], "{case}");
    assert_eq!(came.ended, ended, "{case}");
    let record = world.record(finished.run().as_str());
    assert_eq!(labels(&record), FIRST, "{case}");
    // Nothing was cut, and the segmenter was asked once, for the picture.
    assert!(world.kit.crops().is_empty(), "{case}");
    assert_eq!(world.kit.requests().0, 1, "{case}");
}

#[test]
fn a_confident_violation_of_rq1_with_a_place_segments_inside_it_and_the_mask_grows_there() {
    let mut setup = finding(vec![remains_at(PLACE), passes()]);
    setup.cached = true;
    let world = Looped::new("located-grows", &setup);
    let finished = world.run(&world.call());
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    let record = world.record(finished.run().as_str());
    // The retry cuts the place, asks the phrase inside it, carries what it finds onto
    // the picture and joins it to the found mask; then the recipe's own shape, fill
    // and check.
    let mut wanted: Vec<&str> = FIRST.to_vec();
    wanted.extend([
        "-",
        "-",
        "-",
        "a2_select",
        "a2_shape",
        "a2_fill",
        "a2_check",
    ]);
    assert_eq!(labels(&record), wanted);
    let actions: Vec<String> = calls(&record).into_iter().map(|call| call.1).collect();
    assert_eq!(
        actions[4..8],
        [VIEW, "segment", "carry", REMOVE].map(str::to_owned)
    );
    // The cut is the place, a twentieth wider each way, of the picture's own pixels;
    // the phrase is asked inside it, for four instances at most; and what is found is
    // carried from the cut's corner.
    assert_eq!(world.kit.crops(), [(CUT[2] - CUT[0], CUT[3] - CUT[1])]);
    let looked = looked(&world, &finished, 2);
    let (segment, carry) = (&looked[1].1, &looked[2].1);
    assert_eq!(
        argument(segment, "query").as_deref(),
        Some(crate::checked::PHRASE)
    );
    assert!(asked(segment, "max_instances").contains('4'));
    assert!(asked(carry, "left").contains(&CUT[0].to_string()));
    assert!(asked(carry, "top").contains(&CUT[1].to_string()));
    // The found mask of the retry is the one before it and what was carried: `select`
    // is given the two.
    let select = world.record(&run_of(&record, "a2_select"));
    let given: Vec<Digest> = (select.started().unwrap().given.iter())
        .map(|given| given.artifact)
        .collect();
    assert_eq!(
        given,
        [
            made(&world, &record, "a1_find", "until_select_mask").0,
            carry.outputs()[0].artifact
        ]
    );
    // So the mask that was filled weighs where the thing was found inside the place,
    // from 43 to 49 across and 19 to 25 down, where the first attempt's did not; and
    // still where the first attempt's did.
    let first = first_mask();
    let second = weights(&world, output(&finished, "mask"));
    assert_eq!(
        output(&finished, "mask"),
        made(&world, &record, "a2_fill", "mask")
    );
    let there = (weight(&first, 45, 21), weight(&second, 45, 21));
    assert!(there.0 < 0.1 && there.1 == 1.0, "{there:?}");
    assert!(weight(&first, 25, 24) == 1.0 && weight(&second, 25, 24) == 1.0);
    assert!(weight(&second, 2, 2) == 0.0 && weight(&second, 60, 60) == 0.0);
    let first_came = came(&world, &finished);
    assert_eq!(first_came.kept, Some(2));
    assert!(matches!(
        &first_came.attempts[1],
        (Some(Decision::Pass), Some(Changed::Located { crops })) if crops.len() == 1
    ));
    assert_eq!(world.kit.requests(), (2, 2, 2));

    // The same check answered from the cache: the judge is sent nothing more, and the
    // loop comes to the same, crop for crop and mask for mask.
    let answered = world.run(&world.call());
    assert_eq!(world.kit.requests(), (4, 4, 2));
    assert_eq!(came(&world, &answered), first_came);
    assert_eq!(output(&answered, "mask"), output(&finished, "mask"));
}

#[test]
fn a_place_of_a_tentative_violation_or_of_any_other_disposition_is_never_cut_segmented_or_painted()
{
    // Each names a place, about the named thing, and none says with confidence that it
    // is still there: nothing is cut, the segmenter is not asked again, and no mask
    // comes near the place.
    let cases: Vec<(&str, Json, Decision)> = vec![
        (
            "tentative",
            submission(
                citing(&[("RQ1", "violation", "tentative")], &[("RQ1", FAR)]),
                vec![],
            ),
            Decision::Unsure,
        ),
        (
            "hidden",
            submission(
                citing(&[("RQ1", "hidden", "confident")], &[("RQ1", FAR)]),
                vec![],
            ),
            Decision::Unsure,
        ),
        (
            "cant-tell",
            submission(
                citing(&[("RQ1", "cant_tell", "tentative")], &[("RQ1", FAR)]),
                vec![],
            ),
            Decision::Unsure,
        ),
    ];
    for (case, answers, verdict) in cases {
        let (world, finished) = crate::checked::looped(
            &format!("located-not-{case}"),
            &finding(vec![answers, passes()]),
        );
        one_attempt(&world, &finished, verdict, NOTHING, case);
        let painted = weights(&world, output(&finished, "mask"));
        assert!(weight(&painted, 56, 56) == 0.0, "{case}");
        assert_eq!(world.kit.unplayed(), 1, "{case}");
    }
    // Beside a finding the table allows a retry on, such a place changes nothing: the
    // retry looks into the allowed finding's place, and into no other.
    let both = submission(
        citing(&[("RQ1", "violation", "confident")], &[("RQ1", PLACE)]),
        vec![seen_at(&["RQ1"], "violation", "tentative", FAR)],
    );
    let (world, finished) =
        crate::checked::looped("located-not-beside", &finding(vec![both, passes()]));
    let came = came(&world, &finished);
    assert!(matches!(
        &came.attempts[1],
        (Some(Decision::Pass), Some(Changed::Located { crops })) if crops.len() == 1
    ));
    assert_eq!(world.kit.crops().len(), 1);
    let painted = weights(&world, output(&finished, "mask"));
    assert!(weight(&painted, 45, 21) > 0.0 && weight(&painted, 56, 56) == 0.0);
}

#[test]
fn a_place_of_a_finding_that_cites_anything_to_be_preserved_is_never_used() {
    let remains = [("RQ1", "violation", "confident")];
    let cases: Vec<(&str, Json)> = vec![
        // One finding cites the named thing and a protected subject together.
        (
            "together",
            submission(
                items(&[]),
                vec![seen_at(&["RQ1", "SF1"], "violation", "confident", PLACE)],
            ),
        ),
        // The named thing remains, here; and something else changed, there.
        (
            "beside",
            submission(
                citing(
                    &[remains[0], ("RQ2", "violation", "confident")],
                    &[("RQ1", PLACE), ("RQ2", FAR)],
                ),
                vec![],
            ),
        ),
        // The same, of the scene's geometry, and only tentatively.
        (
            "tentative",
            submission(
                citing(
                    &[remains[0], ("SF2", "violation", "tentative")],
                    &[("RQ1", PLACE), ("SF2", FAR)],
                ),
                vec![],
            ),
        ),
    ];
    for (case, answers) in cases {
        let (world, finished) = crate::checked::looped(
            &format!("located-preserved-{case}"),
            &finding(vec![answers, passes()]),
        );
        // The attempts end before anything of the retry runs: no place is used, the
        // allowed finding's own among them.
        one_attempt(&world, &finished, Decision::Fail, DAMAGED, case);
        assert_eq!(world.kit.unplayed(), 1, "{case}");
    }
}

#[test]
fn beside_a_violation_of_rq3_the_retry_uses_the_rq1_findings_place_alone() {
    let answers = submission(
        citing(
            &[
                ("RQ1", "violation", "confident"),
                ("RQ3", "violation", "confident"),
            ],
            &[("RQ1", PLACE), ("RQ3", FAR)],
        ),
        vec![],
    );
    let (world, finished) =
        crate::checked::looped("located-rq3", &finding(vec![answers, passes()]));
    // One place is looked into, the one of the finding about the named thing.
    assert_eq!(looks(&world, &finished, 2), [VIEW, "segment", "carry"]);
    assert_eq!(world.kit.crops(), [(18, 18)]);
    let attempts = world.attempts(&finished);
    assert!(matches!(
        &attempts.list[1].changed,
        Some(Changed::Located { crops }) if crops.len() == 1
    ));
    // The place of the finding about the fill's look is not painted.
    let painted = weights(&world, output(&finished, "mask"));
    assert!(weight(&painted, 45, 21) > 0.0 && weight(&painted, 56, 56) == 0.0);
}

#[test]
fn a_place_where_nothing_is_found_joins_as_itself() {
    // The segmenter finds nothing inside the crop. A second run in the same world has
    // the place cited as a polygon (below).
    let setup = Setup::answering(vec![remains_at(PLACE), passes(), polygon_at(), passes()]);
    let (world, finished) = crate::checked::looped("located-itself", &setup);
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    let record = world.record(finished.run().as_str());
    assert_eq!(looks(&world, &finished, 2), [VIEW, "segment", ITSELF]);
    // The crop was cut and asked about; nothing was found, and the record says so.
    assert_eq!(world.kit.crops(), [(18, 18)]);
    let attempts = world.attempts(&finished);
    let Some(Changed::Located { crops }) = &attempts.list[1].changed else {
        panic!("{:?}", attempts.list[1].changed);
    };
    assert_eq!(crops.len(), 1);
    let looked = looked(&world, &finished, 2);
    assert_eq!(crops[0].crop, looked[0].1.outputs()[0].artifact);
    // What joins is the place's own pixels, not the wider cut: the box of whole pixels
    // the recipe `mask_rect` is given, and its mask beside the found mask at `select`.
    let itself = &looked[2].1;
    let own = OWN.map(|side| side.to_string()).join(" ");
    assert_eq!(argument(itself, "box").as_deref(), Some(own.as_str()));
    let select = world.record(&run_of(&record, "a2_select"));
    let given = &select.started().unwrap().given;
    assert_eq!(given.len(), 2);
    assert_eq!(given[1].artifact, itself.outputs()[0].artifact);
    // So the mask that was filled covers the place whole, where the first attempt's
    // did not, and nothing far from both.
    let (first, painted) = (first_mask(), weights(&world, output(&finished, "mask")));
    for (x, y) in [(40, 16), (55, 16), (40, 31), (55, 31), (48, 24)] {
        assert!(
            weight(&first, x, y) < 1.0 && weight(&painted, x, y) == 1.0,
            "{x},{y}"
        );
    }
    assert!(weight(&painted, 2, 2) == 0.0 && weight(&painted, 4, 60) == 0.0);
    let said = world
        .diagnostic(finished.run().as_str(), "located-a2.txt")
        .unwrap();
    assert!(
        said.contains("place 1 tiles 1 joined_as_itself\n"),
        "{said}"
    );

    // A place cited as a polygon, where nothing is found, joins as its own pixels (plan
    // mE, task L2, item 2): `rect` makes the polygon a location and `rasterize` a mask of
    // the pixels inside it, and not of the box around it.
    let finished = world.run(&world.call());
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    assert_eq!(
        looks(&world, &finished, 2),
        [VIEW, "segment", "rect", "rasterize"]
    );
    let calls = crate::located::looked(&world, &finished, 2);
    assert_eq!(
        argument(&calls[2].1, "polygon").as_deref(),
        Some("40 16;56 16;40 32")
    );
    let joined = weights(&world, ArtifactId(calls[3].1.outputs()[0].artifact));
    // Inside the triangle, and in its box but past its long side.
    assert_eq!(weight(&joined, 42, 18), 1.0);
    assert_eq!(weight(&joined, 54, 30), 0.0);
    let said = world
        .diagnostic(finished.run().as_str(), "located-a2.txt")
        .unwrap();
    assert!(
        said.contains("place 1 tiles 1 joined_as_itself\n"),
        "{said}"
    );
}

/// The answer that fails an attempt with the named thing in a right triangle over
/// [`PLACE`], cited as a polygon: its corners at 40,16, 56,16 and 40,32.
fn polygon_at() -> Json {
    let triangle = [[40.0, 16.0], [56.0, 16.0], [40.0, 32.0]].map(|[x, y]| [x / 64.0, y / 64.0]);
    let mut answer = remains_at(PLACE);
    let item = answer["items"]
        .as_array_mut()
        .and_then(|items| {
            items
                .iter_mut()
                .find(|item| item["id"] == "chk:ob:RQ1@candidate")
        })
        .unwrap();
    item["location"] = serde_json::json!({"kind": "polygon", "frame": "raw:candidate",
        "bbox": null, "polygon": triangle, "yaw": null, "pitch": null, "fov": null});
    answer
}

#[test]
fn a_row_2_finding_with_no_place_widens_once() {
    let wider = Some(Changed::Wider { px: 8.0 });
    // No place: the wider mask of the loop's first task, and nothing is cut.
    let (world, finished) =
        crate::checked::looped("located-none", &finding(vec![remains(), passes()]));
    let none: Came = came(&world, &finished);
    assert_eq!(
        none.attempts,
        [
            (Some(Decision::Fail), None),
            (Some(Decision::Pass), wider.clone())
        ]
    );
    assert!(world.kit.crops().is_empty());
    // A place on another picture than the candidate, the source, is no place.
    let mut on_source = citing(&[("RQ1", "violation", "confident")], &[("RQ1", PLACE)]);
    on_source[2]["location"]["frame"] = Json::from("raw:source");
    let (world, finished) = crate::checked::looped(
        "located-on-source",
        &finding(vec![submission(on_source, vec![]), passes()]),
    );
    assert_eq!(came(&world, &finished).attempts, none.attempts);
    assert!(world.kit.crops().is_empty());
    // Some cite places and some do not: the located retry, from those that do.
    let mixed = submission(
        items(&[("RQ1", "violation", "confident")]),
        vec![seen_at(&["RQ1"], "violation", "confident", PLACE)],
    );
    let (world, finished) =
        crate::checked::looped("located-mixed", &finding(vec![mixed, passes()]));
    assert!(matches!(
        &came(&world, &finished).attempts[1],
        (Some(Decision::Pass), Some(Changed::Located { crops })) if crops.len() == 1
    ));
    // After a located retry, a finding with no place widens the mask the located
    // retry found, once: a second finding with no place ends the attempts.
    let answers = vec![remains_at(PLACE), remains(), passes()];
    let (world, finished) = crate::checked::looped("located-then-wider", &finding(answers));
    let came = came(&world, &finished);
    assert_eq!(came.attempts.len(), 3);
    assert!(matches!(came.attempts[1].1, Some(Changed::Located { .. })));
    assert_eq!(came.attempts[2], (Some(Decision::Pass), wider));
    let record = world.record(finished.run().as_str());
    let shape = world.record(&run_of(&record, "a3_shape"));
    assert_eq!(argument(&shape, "set").as_deref(), Some("grow.px=8"));
    assert_eq!(
        shape.started().unwrap().given[0].artifact,
        made(&world, &record, "a2_select", "until_select_mask").0
    );
}

#[test]
fn every_attempt_fills_the_picture_the_loop_was_given() {
    let answers = vec![remains_at(PLACE), remains_at(SECOND), passes()];
    let (world, finished) = crate::checked::looped("located-source", &finding(answers));
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    let record = world.record(finished.run().as_str());
    let picture = world.image_id().0;
    let given = |label: &str, port: &str| -> Vec<Digest> {
        let run = world.record(&run_of(&record, label));
        let inputs = run.inputs().iter().filter(|input| input.port == port);
        inputs.map(|input| input.artifact).collect()
    };
    let mut fills = Vec::new();
    for attempt in 1..=3 {
        // The fill paints the loop's own picture, and its check holds the fill to it.
        assert_eq!(given(&format!("a{attempt}_fill"), "image"), [picture]);
        let fill = made(&world, &record, &format!("a{attempt}_fill"), "image").0;
        assert_eq!(
            given(&format!("a{attempt}_check"), "images"),
            [fill, picture]
        );
        fills.push(fill);
    }
    assert!(!fills.contains(&picture));
    // A place is on the fill the judge was shown; what is cut, and what the masks are
    // carried onto and joined on, is the loop's own picture.
    for attempt in 2..=3 {
        for (action, run) in looked(&world, &finished, attempt) {
            let images = run.inputs().iter().filter(|input| input.port == "image");
            let images: Vec<Digest> = images.map(|input| input.artifact).collect();
            // `segment` is asked about the crop; the cut and the carry are of the picture.
            match action.as_str() {
                "segment" => assert!(!images.contains(&picture) && !fills.contains(&images[0])),
                _ => assert_eq!(images, [picture], "{action}"),
            }
        }
        assert_eq!(given(&format!("a{attempt}_select"), "image"), [picture]);
    }
    // The third attempt starts from the second's found mask, and adds its own place.
    let select = world.record(&run_of(&record, "a3_select"));
    let started = select.started().unwrap();
    assert_eq!(started.given.len(), 2);
    assert_eq!(
        started.given[0].artifact,
        made(&world, &record, "a2_select", "until_select_mask").0
    );
    assert_eq!(came(&world, &finished).kept, Some(3));
}

#[test]
fn the_shaping_and_the_fill_of_a_retry_are_the_recipes_own_steps() {
    let (world, finished) =
        crate::checked::looped("located-steps", &finding(vec![remains_at(PLACE), passes()]));
    let record = world.record(finished.run().as_str());
    let stretch = |label: &str| world.record(&run_of(&record, label));
    let steps = |label: &str| -> Vec<String> {
        let run = stretch(label);
        run.steps()
            .iter()
            .map(|step| step.action().to_owned())
            .collect()
    };
    // The retry's shape and fill are the same stretches of the same recipe as the
    // first attempt's, with no setting of a step changed.
    for (label, from, until, ran) in [
        ("a2_select", "select", Some("select"), vec!["select"]),
        (
            "a2_shape",
            "grow",
            Some("feather"),
            vec!["grow", "close", "plug", "subtract", "feather"],
        ),
        ("a2_fill", "fill", None, vec!["fill"]),
    ] {
        let run = stretch(label);
        assert_eq!(run.asked().action.as_deref(), Some(REMOVE), "{label}");
        assert_eq!(argument(&run, "from").as_deref(), Some(from), "{label}");
        assert_eq!(argument(&run, "until").as_deref(), until, "{label}");
        assert_eq!(argument(&run, "set"), None, "{label}");
        assert_eq!(steps(label), ran, "{label}");
    }
    assert_eq!(steps("a2_shape"), steps("a1_shape"));
    assert_eq!(steps("a2_fill"), steps("a1_fill"));
    assert_eq!(stretch("a2_check").asked().action.as_deref(), Some(CHECK));
    // The loop itself paints nothing and shapes nothing: beside the two recipes it
    // calls what looks into a place, and nothing else.
    let others: Vec<String> = (calls(&record).into_iter())
        .map(|call| call.1)
        .filter(|action| action != REMOVE && action != CHECK)
        .collect();
    assert_eq!(others, [VIEW, "segment", "carry"]);
}

/// Eight places of a quarter of the picture's side each: the two rows of four that
/// start `down` quarters of the picture from its top.
pub fn eight(down: u32) -> Json {
    let cell = |at: u32| {
        let (x, y) = (f64::from(at % 4) * 0.25, f64::from(down + at / 4) * 0.25);
        [x, y, x + 0.25, y + 0.25]
    };
    let first = citing(&[("RQ1", "violation", "confident")], &[("RQ1", cell(0))]);
    let more = (1..8).map(|at| seen_at(&["RQ1"], "violation", "confident", cell(at)));
    submission(first, more.collect())
}

#[test]
fn eight_places_of_four_masks_on_the_second_and_the_third_attempt_reach_the_fill() {
    // The judge names eight places after the first attempt and eight others after the
    // second, and the segmenter finds four things inside each.
    let mut setup = Setup::answering(vec![eight(0), eight(2), passes()]);
    setup.inside = vec![[2, 2, 5, 5], [6, 2, 9, 5], [2, 6, 5, 9], [6, 6, 9, 9]];
    let (world, finished) = crate::checked::looped("located-largest", &setup);
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    let record = world.record(finished.run().as_str());
    let attempts = world.attempts(&finished);
    assert_eq!(attempts.list.len(), 3);
    assert_eq!(
        (attempts.kept, attempts.list[2].verdict),
        (Some(3), Some(Decision::Pass))
    );
    for (attempt, listed) in attempts.list.iter().enumerate().skip(1) {
        // Each retry: eight places of three calls, the join, the shape, the fill and
        // the check.
        assert_eq!(listed.runs.len(), 8 * 3 + 1 + 3);
        let Some(Changed::Located { crops }) = &listed.changed else {
            panic!("{:?}", listed.changed);
        };
        assert_eq!(crops.len(), PLACES_MAX);
        assert!(crops.iter().all(|crop| crop.mask.is_some()));
        // `select` is given the found mask of the attempt before and four masks for
        // each of the eight places: 33 of the 64 it takes.
        let select = world.record(&run_of(&record, &format!("a{}_select", attempt + 1)));
        let given = &select.started().unwrap().given;
        assert_eq!(given.len(), 33);
        assert!(
            given
                .iter()
                .all(|given| given.step == "segment" && given.port == "mask")
        );
        let distinct: std::collections::BTreeSet<Digest> =
            given.iter().map(|given| given.artifact).collect();
        assert_eq!(distinct.len(), 33);
    }
    // Every attempt reached its fill and its check, and the segmenter was asked what
    // the plan states: once, and eight times for each retry.
    assert_eq!(world.kit.requests(), (17, 3, 3));
    assert_eq!(world.kit.crops().len(), 16);
    let own = world.record(finished.run().as_str());
    let plan = own.plan().unwrap();
    assert_eq!(plan.requests(Capability::Segment), Some(17));
}

#[test]
fn no_more_than_eight_places_are_used_and_the_plans_bound_covers_them() {
    let candidate = Digest::of_bytes(b"candidate");
    let at = |sides: [f64; 4]| {
        Some(FoundAt {
            image: candidate,
            boxed: Some(sides),
            polygon: None,
        })
    };
    // Nine places, each a column of the picture: the first eight are used, in order.
    let nine: Vec<Option<FoundAt>> = (0..9)
        .map(|n| at([f64::from(n) / 16.0, 0.0, f64::from(n + 2) / 16.0, 1.0]))
        .collect();
    let used = usable(&nine, candidate, 64, 64);
    assert_eq!(used.places.len(), PLACES_MAX);
    assert_eq!(used.places[0].own, [0, 0, 8, 64]);
    assert_eq!(used.places[7].own, [28, 0, 36, 64]);
    // The ninth is left out for the bound, and said so (plan mE, task L2, item 2).
    assert_eq!(
        used.left_out,
        [LeftOut {
            cited: 9,
            cut: [31, 0, 41, 64],
            why: Left::Bound
        }]
    );
    // A place is its own pixels, and a cut a twentieth wider each way, held inside
    // the picture and to what `view` cuts and shows pixel for pixel, in one tile.
    let one = |sides: [f64; 4], width: u32, height: u32| {
        let used = usable(&[at(sides)], candidate, width, height);
        (used.places.iter())
            .map(|place| (place.own, place.cut, place.tiles.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(one(PLACE, 64, 64), [(OWN, CUT, vec![CUT])]);
    assert_eq!(CUT_MIN, whizbang_squint::view::MIN_CROP);
    assert_eq!(CUT_MAX, whizbang_squint::view::MAX_SIDE);
    let small = [29, 29, 37, 37];
    assert_eq!(
        one([0.5, 0.5, 0.53, 0.53], 64, 64),
        [([32, 32, 34, 34], small, vec![small])]
    );
    let corner = [56, 0, 64, 8];
    assert_eq!(
        one([0.97, 0.0, 1.0, 0.03], 64, 64),
        [([62, 0, 64, 2], corner, vec![corner])]
    );
    // Not used: no place; a place twice; one on another picture; one that covers no
    // pixel's centre.
    let twice = [None, at(PLACE), at(PLACE)];
    assert_eq!(usable(&twice, candidate, 64, 64).places.len(), 1);
    let other = usable(&[at(PLACE)], Digest::of_bytes(b"other"), 64, 64);
    assert_eq!(other, Usable::default());
    let none = usable(&[at([0.501, 0.501, 0.502, 0.502])], candidate, 64, 64);
    assert_eq!(none, Usable::default());
    // A cut over 1,024 a side is cut in tiles of at most that, each the picture's own
    // pixels; one over 2,048 a side is left out for its size.
    let tiled = one([0.1, 0.1, 0.5, 0.2], 4096, 4096);
    assert_eq!(tiled.len(), 1);
    let (_, cut, tiles) = &tiled[0];
    assert_eq!(*cut, [328, 389, 2130, 840]);
    assert_eq!(*tiles, [[328, 389, 1229, 840], [1229, 389, 2130, 840]]);
    assert!(
        tiles
            .iter()
            .all(|[l, t, r, b]| r - l <= CUT_MAX && b - t <= CUT_MAX)
    );
    assert_eq!(one([0.1, 0.1, 0.3, 0.2], 4096, 4096)[0].2.len(), 1);
    let large = usable(&[at([0.1, 0.1, 0.7, 0.2])], candidate, 4096, 4096);
    assert!(large.places.is_empty());
    assert_eq!(large.left_out[0].why, Left::Size);
    // A place of 3,072 pixels a side is left out for its size, and named by its number
    // among those cited and its cut; the loop says so in its diagnostic, as it says the
    // places left out for the bound (`eight_places_two_in_tiles_…`).
    let side = |n: f64| n / 4096.0;
    let huge = usable(
        &[
            at(PLACE),
            at([side(512.0), side(512.0), side(3584.0), side(3584.0)]),
        ],
        candidate,
        4096,
        4096,
    );
    assert_eq!(huge.places.len(), 1);
    assert_eq!(
        huge.left_out,
        [LeftOut {
            cited: 2,
            cut: [358, 358, 3738, 3738],
            why: Left::Size
        }]
    );
    assert_eq!(TILED_MAX, 2 * CUT_MAX);
    // A polygon is its own pixels: the box of them, and how many.
    let polygon = Some(FoundAt {
        image: candidate,
        boxed: None,
        polygon: Some(vec![[0.625, 0.25], [0.875, 0.375], [0.75, 0.5]]),
    });
    let used = usable(&[polygon], candidate, 64, 64);
    let place = &used.places[0];
    assert_eq!(place.own, OWN);
    assert!(
        place.covers > 0 && place.covers < 16 * 16,
        "{}",
        place.covers
    );
    assert_eq!(place.polygon_text().as_deref(), Some("40 16;56 24;48 32"));
    // What `view` is asked cuts exactly the cut: each edge half a pixel inside it.
    let asked = Place::fractions(CUT, 64, 64);
    assert_eq!(whizbang_squint::view::snapped(&asked, 64, 64), CUT);
    // And so for every cut of eight pixels or more of a side of 8 to 40 pixels, and
    // for cuts at the ends of long sides.
    let cuts = |side: u32| {
        (0..=side - 8).flat_map(move |from| (from + 8..=side).map(move |to| (side, from, to)))
    };
    let long = [
        (4096, 0, 1024),
        (4096, 3072, 4096),
        (7919, 6895, 7919),
        (65_535, 1, 9),
    ];
    for (side, from, to) in (8..=40).flat_map(cuts).chain(long) {
        let cut = [from, 0, to, 8];
        let asked = Place::fractions(cut, side, 8);
        let snapped = whizbang_squint::view::snapped(&asked, side, 8);
        assert_eq!(snapped, cut, "a side of {side}");
        assert!(asked.iter().all(|edge| (0.0..=1.0).contains(edge)));
    }

    // The plan states the segmenter's requests: one to find, and eight for each retry
    // it allows; and the loop reserves their bound with the rest.
    let world = Looped::new("located-bound", &finding(vec![passes()]));
    let engine = world.engine();
    for (attempts, segments) in [(1, 1), (2, 9), (3, 17)] {
        let prepared = engine
            .prepare(&crate::checked::asking(&world, attempts))
            .unwrap();
        let plan = prepared.plan().unwrap();
        assert_eq!(
            plan.requests(Capability::Segment),
            Some(segments),
            "{attempts}"
        );
    }
    drop(engine);
    let finished = world.run(&world.call());
    let own = world.record(finished.run().as_str());
    assert_eq!(own.spend().reserved_micros, Some(bound_of(3)));
    // A check that names a ninth place hands back findings that are not complete, and
    // findings cut short never enlarge a mask: nothing is cut.
    let cell = |n: u32| [f64::from(n) / 16.0, 0.0, f64::from(n + 1) / 16.0, 1.0];
    let first = citing(&[("RQ1", "violation", "confident")], &[("RQ1", cell(0))]);
    let more = (1..9).map(|n| seen_at(&["RQ1"], "violation", "confident", cell(n)));
    let nine = submission(first, more.collect());
    let (world, finished) = crate::checked::looped("located-ninth", &finding(vec![nine, passes()]));
    let came = came(&world, &finished);
    assert_eq!(came.attempts.len(), 1);
    assert!(came.ended.contains("were cut short"), "{}", came.ended);
    assert!(world.kit.crops().is_empty());
}

/// A place of whole pixels of the picture of 64 by 64, as a finding cites it.
fn pixels(left: u32, top: u32, right: u32, bottom: u32) -> [f64; 4] {
    [left, top, right, bottom].map(|side| f64::from(side) / 64.0)
}

/// What each attempt of a run came to: its verdict and what its retry changed, a
/// located retry by how many of its crops gave a mask and how many joined as themselves.
fn attempts_of(world: &Looped, finished: &Finished) -> Vec<(Option<Decision>, String)> {
    let attempts = world.attempts(finished);
    let changed = |changed: &Option<Changed>| match changed {
        None => "nothing".to_owned(),
        Some(Changed::Wider { px }) => format!("wider {px}"),
        Some(Changed::Located { crops }) => {
            let gave = crops.iter().filter(|crop| crop.mask.is_some()).count();
            format!("located {gave} {}", crops.len() - gave)
        }
    };
    (attempts.list.iter())
        .map(|attempt| (attempt.verdict, changed(&attempt.changed)))
        .collect()
}

#[test]
fn a_place_whose_cut_is_the_whole_picture_is_no_place() {
    // A crop that is the picture itself cannot be carried from: such a place is no
    // place, whether it is the whole picture or its padded cut comes to it.
    let candidate = Digest::of_bytes(b"candidate");
    for sides in [[0.0, 0.0, 1.0, 1.0], pixels(1, 1, 63, 63)] {
        let at = Some(FoundAt {
            image: candidate,
            boxed: Some(sides),
            polygon: None,
        });
        let used = usable(&[at], candidate, 64, 64);
        assert_eq!(used, Usable::default(), "{sides:?}");
    }
    // Through the loop: nothing is cut, the segmenter is not asked again, and the
    // retry is the wider mask.
    let answers = vec![remains_at(pixels(1, 1, 63, 63)), passes()];
    let (world, finished) = crate::checked::looped("located-whole", &finding(answers));
    assert_eq!(
        attempts_of(&world, &finished),
        [
            (Some(Decision::Fail), "nothing".to_owned()),
            (Some(Decision::Pass), "wider 8".to_owned())
        ]
    );
    assert!(world.kit.crops().is_empty());
    assert_eq!(world.kit.requests(), (1, 2, 2));
}

#[test]
fn on_a_panorama_no_place_is_looked_into_and_the_mask_is_widened() {
    // `view` cuts flat pictures only: on a panorama a place is no place, and a finding
    // that names one widens the mask, as it did before places were looked into.
    let mut setup = finding(vec![remains_at(PLACE), passes()]);
    setup.panorama = true;
    let (world, finished) = crate::checked::looped("located-panorama", &setup);
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    assert_eq!(
        attempts_of(&world, &finished),
        [
            (Some(Decision::Fail), "nothing".to_owned()),
            (Some(Decision::Pass), "wider 8".to_owned())
        ]
    );
    assert!(world.kit.crops().is_empty());
    assert_eq!(looks(&world, &finished, 2), Vec::<String>::new());
}

#[test]
fn a_located_retry_whose_mask_is_the_one_before_uses_the_widening_then() {
    // The judge names a place about what was found already, and the segmenter finds
    // there only pixels the mask has: from 21 to 25 each way, inside 20, 20 to 30, 28.
    let around = pixels(20, 20, 30, 28);
    let mut setup = finding(vec![remains_at(around), passes()]);
    setup.inside = vec![[2, 2, 6, 6]];
    let (world, finished) = crate::checked::looped("located-same", &setup);
    // The located retry's mask is the one before it: it is not filled. The one
    // widening has not been used, so the next attempt widens the same found mask.
    assert_eq!(
        attempts_of(&world, &finished),
        [
            (Some(Decision::Fail), "nothing".to_owned()),
            (None, "located 1 0".to_owned()),
            (Some(Decision::Pass), "wider 8".to_owned())
        ]
    );
    let attempts = world.attempts(&finished);
    assert_eq!(
        attempts.list[1].unchecked.as_deref(),
        Some("its shaped mask is the one before it, and nothing was filled")
    );
    assert_eq!((attempts.kept, world.kit.requests()), (Some(3), (2, 2, 2)));
    let record = world.record(finished.run().as_str());
    let (join, shape) = (
        world.record(&run_of(&record, "a2_select")),
        world.record(&run_of(&record, "a3_shape")),
    );
    assert_eq!(argument(&shape, "set").as_deref(), Some("grow.px=8"));
    assert_eq!(
        shape.started().unwrap().given[0].artifact,
        join.outputs()[0].artifact
    );
    // When the widened mask is the same one too, the attempts end: the segmenter
    // finds the whole picture, and nothing can be added to that.
    let mut setup = finding(vec![remains_at(PLACE), passes()]);
    setup.found = [0, 0, 64, 64];
    let (world, finished) = crate::checked::looped("located-same-twice", &setup);
    assert_eq!(
        attempts_of(&world, &finished),
        [
            (Some(Decision::Fail), "nothing".to_owned()),
            (None, "located 1 0".to_owned()),
            (None, "wider 8".to_owned())
        ]
    );
    let attempts = world.attempts(&finished);
    assert_eq!(
        attempts.ended,
        "the mask of attempt 3 is the one before it, and an attempt is not made twice"
    );
    assert_eq!((attempts.kept, world.kit.requests().1), (Some(1), 1));
    // With two attempts allowed there is none left to widen in.
    let mut setup = finding(vec![remains_at(PLACE), passes()]);
    setup.found = [0, 0, 64, 64];
    let world = Looped::new("located-same-last", &setup);
    let finished = world.run(&crate::checked::asking(&world, 2));
    let attempts = world.attempts(&finished);
    assert_eq!(attempts.list.len(), 2);
    assert_eq!(
        attempts.ended,
        "the mask of attempt 2 is the one before it, and an attempt is not made twice"
    );
}

#[test]
fn a_place_that_joins_as_itself_inside_the_mask_before_is_not_filled_again() {
    // Nothing is found in the crop, and the place is wholly inside what the first
    // attempt found: joined as itself it adds no pixel. The mask it makes has another
    // identity than the one before, since it names the place it came from, and the
    // same pixels: it is compared by its pixels, and neither filled nor checked.
    let inside = pixels(21, 21, 29, 27);
    let setup = Setup::answering(vec![remains_at(inside), passes()]);
    let (world, finished) = crate::checked::looped("located-itself-inside", &setup);
    assert_eq!(
        attempts_of(&world, &finished),
        [
            (Some(Decision::Fail), "nothing".to_owned()),
            (None, "located 0 1".to_owned()),
            (Some(Decision::Pass), "wider 8".to_owned())
        ]
    );
    let attempts = world.attempts(&finished);
    assert_eq!(
        attempts.list[1].unchecked.as_deref(),
        Some("its shaped mask is the one before it, and nothing was filled")
    );
    // Two fills and two checks: the first attempt's and the widened one's.
    assert_eq!(world.kit.requests(), (2, 2, 2));
    let record = world.record(finished.run().as_str());
    let made = |label: &str| world.record(&run_of(&record, label)).outputs()[0].artifact;
    assert_ne!(made("a1_shape"), made("a2_shape"));
}

#[test]
fn a_place_over_a_tenth_of_the_picture_does_not_join_as_itself() {
    use whizbang_rascal::checked::located::ITSELF_PART;
    assert_eq!(ITSELF_PART, 10);
    // Nothing is found in either crop. A place of 400 of the picture's 4,096 pixels
    // joins as itself; one of 420, over a tenth, does not, and is no place for that:
    // no other place gave a mask, so the retry is the wider mask.
    let (under, over) = (pixels(32, 40, 52, 60), pixels(32, 40, 53, 60));
    let answers = |place| vec![remains_at(place), passes()];
    let (world, finished) =
        crate::checked::looped("located-tenth-under", &Setup::answering(answers(under)));
    assert_eq!(
        attempts_of(&world, &finished)[1],
        (Some(Decision::Pass), "located 0 1".to_owned())
    );
    assert_eq!(looks(&world, &finished, 2), [VIEW, "segment", ITSELF]);
    let (world, finished) =
        crate::checked::looped("located-tenth-over", &Setup::answering(answers(over)));
    assert_eq!(
        attempts_of(&world, &finished)[1],
        (Some(Decision::Pass), "wider 8".to_owned())
    );
    // The place was cut and asked about, and nothing of it was painted for that.
    assert_eq!(looks(&world, &finished, 2), [VIEW, "segment"]);
    assert_eq!(world.kit.crops().len(), 1);
    let painted = weights(&world, output(&finished, "mask"));
    assert!(weight(&painted, 50, 58) == 0.0);
    // Beside a place that gave a mask it is left out, and the retry is located.
    let both = submission(
        citing(&[("RQ1", "violation", "confident")], &[("RQ1", over)]),
        vec![seen_at(
            &["RQ1"],
            "violation",
            "confident",
            pixels(0, 0, 24, 24),
        )],
    );
    let mut setup = finding(vec![both, passes()]);
    setup.inside_least = 26;
    let (world, finished) = crate::checked::looped("located-tenth-beside", &setup);
    assert_eq!(
        attempts_of(&world, &finished)[1],
        (Some(Decision::Pass), "located 1 0".to_owned())
    );
    assert_eq!(
        looks(&world, &finished, 2),
        [VIEW, "segment", VIEW, "segment", "carry"]
    );
}

/// Eight places cited on a picture 2,200 pixels wide (plan mE, task L2, item 2): the
/// first two are cut in two tiles each, since their cuts are over 1,024 across; the
/// third, across the whole picture, is over 2,048 and left out for its size; the next
/// four are cut in one tile each; the last no longer fits among the eight crops and is
/// left out for the bound. The requests, the crops and the masks are counted; the record is
/// published; the places left out are named in the retry's diagnostic, with what became
/// of the others and the times; and the picture is decoded once in the run.
#[test]
fn eight_places_two_in_tiles_fill_the_eight_crops_and_the_rest_are_named_as_left_out() {
    let (width, height) = (2200_u32, 16_u32);
    let wide = |top: f64| [0.0, top, 0.5, top + 0.25];
    let small = |n: u32| {
        let left = 0.55 + f64::from(n) * 0.08;
        [left, 0.6, left + 0.01, 0.9]
    };
    // Two places of 1,100 pixels across, cut in two tiles each; one across the whole
    // picture, over 2,048, left out for its size; then five small ones.
    let mut places = vec![wide(0.0), wide(0.3), [0.0, 0.6, 1.0, 0.9]];
    places.extend((0..5).map(small));
    let first = citing(&[("RQ1", "violation", "confident")], &[("RQ1", places[0])]);
    let more = places[1..]
        .iter()
        .map(|sides| seen_at(&["RQ1"], "violation", "confident", *sides));
    let cited = submission(first, more.collect());
    let mut setup = finding(vec![cited, passes()]);
    setup.size = (width, height);
    setup.found = [20, 2, 30, 8];
    setup.inside = vec![[1, 1, 4, 4]];
    // A run decodes its picture once (plan mE, task L2, item 4): every stretch and every
    // `view` and `carry` of the eight crops reads it, and while the loop runs what is
    // decoded is kept. Once when the call is prepared, and once in the run, at most.
    let world = Looped::new("located-tiles", &setup);
    let file = {
        use whizbang_core::artifact::{Artifact, ArtifactType, get_anywhere};
        let limits = whizbang_core::identity::DecodeLimits::default();
        let read = get_anywhere(
            &world.reader(),
            ArtifactType::Image,
            world.image_id(),
            &limits,
        );
        let Ok(Artifact::Image(picture)) = read else {
            panic!("the picture");
        };
        picture.identity().file.0
    };
    let before = whizbang_core::identity::decodes_of(file);
    let finished = world.run(&world.call());
    let decoded = whizbang_core::identity::decodes_of(file) - before;
    assert!((1..=2).contains(&decoded), "{decoded}");
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    // Eight crops: two tiles of each wide place, each at most 1,024 across, then one of
    // each of four small places; and eight requests of the segmenter beside the find.
    let crops = world.kit.crops();
    assert_eq!(crops.len(), PLACES_MAX);
    assert!(crops.iter().all(|(w, h)| *w <= CUT_MAX && *h <= CUT_MAX));
    assert_eq!(crops[0].0 + crops[1].0, crops[2].0 + crops[3].0);
    assert!(crops[0].0 > 512 && crops[4].0 < 40, "{crops:?}");
    assert_eq!(world.kit.requests(), (9, 2, 2));
    let attempts = world.attempts(&finished);
    let Some(Changed::Located { crops: listed }) = &attempts.list[1].changed else {
        panic!("{:?}", attempts.list[1].changed);
    };
    assert_eq!(listed.len(), PLACES_MAX);
    assert!(listed.iter().all(|crop| crop.mask.is_some()));
    // `select` is given the mask before and one carried from each crop.
    let record = world.record(finished.run().as_str());
    let select = world.record(&run_of(&record, "a2_select"));
    assert_eq!(select.started().unwrap().given.len(), 1 + PLACES_MAX);
    assert_eq!(record.version(), whizbang_core::contract::RUN_8);
    // What became of each place, in the order cited, and the times apart.
    let said = world
        .diagnostic(finished.run().as_str(), "located-a2.txt")
        .unwrap();
    let (places, times): (Vec<&str>, Vec<&str>) = said
        .lines()
        .skip(1)
        .partition(|line| !line.starts_with("time "));
    assert_eq!(said.lines().next(), Some("located/1"));
    assert_eq!(
        places,
        [
            "place 1 tiles 2 found_something",
            "place 2 tiles 2 found_something",
            "place 3 left_out_for_size cut 2200 by 8",
            "place 4 tiles 1 found_something",
            "place 5 tiles 1 found_something",
            "place 6 tiles 1 found_something",
            "place 7 tiles 1 found_something",
            "place 8 left_out_for_the_bound cut 26 by 8",
        ]
    );
    assert_eq!(times.len(), 7);
    assert!(times.last().unwrap().starts_with("time retry ms "));
}
