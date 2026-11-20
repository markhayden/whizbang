//! `table::…`: the one table of what may enlarge a mask (plan m4, task L1, item 4; ADR
//! 0039, "The table"), a test for each row, through the loop on the fixture of
//! `checked`. Each case is run twice in one store whose `check` keeps the decision
//! cache: on a check that was made, and on the same check answered from the cache,
//! which sends the judge nothing; the two runs of the loop come to the same.
//!
//! The cases of one test run three at a time, on threads of the test kit's.

use serde_json::{Value as Json, json};
use whizbang_core::contract::record::{Changed, Decision};
use whizbang_core::contract::{
    Confidence, Disposition, Finding, Findings, Finished, Form, LeftOut,
};
use whizbang_rascal::checked::table::{Reading, Row, read, row};
use whizbang_testkit::threads;

use crate::checked::{
    Answered, Came, Looped, Setup, argument, came, items, observation, passes, remains, run_of,
    submission,
};

const CUT: &str =
    "the findings of attempt 1 were cut short, and findings cut short never enlarge a mask";
const DAMAGED: &str =
    "attempt 1 changed something that must be preserved, which painting more cannot mend";
const NOTHING: &str =
    "no finding of attempt 1 says with confidence that the named thing is still there";
const WIDENED: &str = "a wider mask was already used once";

/// What a retry with a wider mask changed.
fn wider() -> Option<Changed> {
    Some(Changed::Wider { px: 8.0 })
}

/// The loop, run twice in one world whose `check` keeps the decision cache, under a
/// judge that plays these submissions: what the first run came to, which the second,
/// answered from the cache with nothing sent to the judge, came to as well.
fn twice(label: &str, answers: Vec<Json>) -> (Looped, Came) {
    let (world, came, _) = twice_keeping(label, answers);
    (world, came)
}

/// [`twice`], with the first run, the one whose checks were made.
fn twice_keeping(label: &str, answers: Vec<Json>) -> (Looped, Came, Finished) {
    let mut setup = Setup::answering(answers);
    setup.cached = true;
    let world = Looped::new(&format!("table-{label}"), &setup);
    let made = world.run(&world.call());
    let first = came(&world, &made);
    let (_, fills, asked) = world.kit.requests();
    assert_eq!(
        asked as usize,
        first.attempts.iter().filter(|a| a.0.is_some()).count()
    );
    let answered = world.run(&world.call());
    // Every check of the second run was answered from the cache: the judge was sent
    // nothing more, while each fill was asked again.
    assert_eq!(world.kit.requests(), (2, fills * 2, asked), "{label}");
    assert_eq!(came(&world, &answered), first, "{label}");
    let same = |finished: &Finished| {
        finished
            .findings()
            .map(|findings| findings.to_json().to_string())
    };
    assert_eq!(same(&answered), same(&made), "{label}");
    (world, first, made)
}

/// The loop run once, under the same world as [`twice`]: what it came to, on checks
/// that were made.
fn once(label: &str, answers: Vec<Json>) -> (Looped, Came) {
    let mut setup = Setup::answering(answers);
    setup.cached = true;
    let world = Looped::new(&format!("table-{label}"), &setup);
    let made = world.run(&world.call());
    let came = came(&world, &made);
    (world, came)
}

/// Each case, by its label, in the order given: what it came to. With `all_twice`,
/// each is run twice, made and then answered from the cache ([`twice`]). Without, the
/// first is and the others are run once, on checks that were made: the two tests of
/// many cases ran the loop sixteen times more for it, and that a check answered from
/// the cache gives the loop the same findings does not depend on which case of a row
/// they are of (plan m4, task L1, "Reworded with the task"). Three run beside each other at a time, on threads of the test
/// kit's: every run of the loop is many small writes to its store, which more threads
/// at once only make wait for each other.
fn each(cases: Vec<(String, Vec<Json>)>, all_twice: bool) -> Vec<(String, Came)> {
    assert!(cases.len() <= 12);
    let mut came = Vec::new();
    let first = cases.first().map(|(label, _)| label.clone());
    let mut cases = cases.into_iter().peekable();
    while cases.peek().is_some() {
        let three: Vec<_> = cases.by_ref().take(3).collect();
        came.extend(threads::scope(|scope| {
            let running: Vec<_> = (three.into_iter())
                .map(|(label, answers)| {
                    let again = all_twice || first.as_deref() == Some(label.as_str());
                    scope.spawn(move || {
                        let (_, came) = match again {
                            true => twice(&label, answers),
                            false => once(&label, answers),
                        };
                        (label, came)
                    })
                })
                .collect();
            let joined = running.into_iter().map(|case| case.join().unwrap());
            joined.collect::<Vec<_>>()
        }));
    }
    came
}

/// One attempt, with this verdict, after which the attempts ended for this reason.
fn one_attempt(came: &Came, verdict: Decision, ended: &str, label: &str) {
    assert_eq!(came.outcome, "completed", "{label}");
    assert_eq!(came.attempts, [(Some(verdict), None)], "{label}");
    assert_eq!(
        (came.verdict, came.kept),
        (Some(verdict), Some(1)),
        "{label}"
    );
    assert_eq!(came.ended, ended, "{label}");
}

/// A tentative violation of `RQ3`, as a separate observation: a finding that says
/// nothing of the named thing and of nothing that must be preserved.
fn filler() -> Json {
    observation(&["RQ3"], "violation", "tentative")
}

#[test]
fn row_0_an_rq1_violation_kept_with_an_sf1_violation_cut_off_ends_the_attempts() {
    // The 33rd finding, which the document cannot hold, is the one that says the
    // removal damaged a protected subject. What is kept reads as a row 2 finding.
    let mut observations = vec![filler(); 31];
    observations.push(observation(&["SF1"], "violation", "confident"));
    let answers = submission(items(&[("RQ1", "violation", "confident")]), observations);
    let (world, came) = twice("row0-sf1-cut", vec![answers, passes()]);
    one_attempt(&came, Decision::Fail, CUT, "row 0");
    assert_eq!(world.kit.unplayed(), 1);
}

#[test]
fn row_0_findings_whose_only_confident_violation_was_cut_off_enlarge_nothing() {
    // Thirty-two tentative violations kept, the only confident one cut: the verdict is
    // the decision the findings carry, fail, where the list alone would read unsure.
    let mut observations = vec![observation(&["RQ1"], "violation", "tentative"); 32];
    observations.push(observation(&["RQ1"], "violation", "confident"));
    let answers = submission(items(&[]), observations);
    let (world, came) = twice("row0-confident-cut", vec![answers, passes()]);
    one_attempt(&came, Decision::Fail, CUT, "row 0");
    assert_eq!(world.kit.unplayed(), 1);
}

#[test]
fn row_0_findings_cut_short_end_the_attempts_whatever_was_kept() {
    // What was kept holds a row 2 finding and nothing of row 1; what was cut is a
    // finding that would have forbidden nothing. The attempts end all the same.
    let answers = submission(
        items(&[("RQ1", "violation", "confident")]),
        vec![filler(); 32],
    );
    let (world, came) = twice("row0-any-cut", vec![answers, passes()]);
    one_attempt(&came, Decision::Fail, CUT, "row 0");
    assert_eq!(world.kit.unplayed(), 1);
}

#[test]
fn row_1_a_violation_of_a_protected_subject_of_the_geometry_or_of_rq2_ends_the_attempts() {
    // Each of the three, confident and tentative, as an item of the checklist and as a
    // separate observation; each beside a confident violation of RQ1, which alone
    // would allow a retry.
    let mut cases = Vec::new();
    for obligation in ["SF1", "SF2", "RQ2"] {
        for confidence in ["confident", "tentative"] {
            let remains: Answered = ("RQ1", "violation", "confident");
            let item = items(&[remains, (obligation, "violation", confidence)]);
            cases.push((
                format!("row1-{obligation}-{confidence}-item"),
                vec![submission(item, Vec::new()), passes()],
            ));
            let observed = vec![observation(&[obligation], "violation", confidence)];
            cases.push((
                format!("row1-{obligation}-{confidence}-observation"),
                vec![submission(items(&[remains]), observed), passes()],
            ));
        }
    }
    let came = each(cases, false);
    assert_eq!(came.len(), 12);
    for (label, came) in came {
        one_attempt(&came, Decision::Fail, DAMAGED, &label);
    }
}

#[test]
fn row_1_beats_row_2_an_rq1_violation_beside_a_separate_sf1_violation_ends_the_attempts() {
    let observed = vec![observation(&["SF1"], "violation", "confident")];
    let answers = submission(items(&[("RQ1", "violation", "confident")]), observed);
    let (world, came) = twice("row1-beats-row2", vec![answers, passes()]);
    one_attempt(&came, Decision::Fail, DAMAGED, "row 1 beside row 2");
    assert_eq!(world.kit.unplayed(), 1);
}

#[test]
fn row_1_a_finding_that_cites_rq1_and_sf1_together_ends_the_attempts() {
    // One finding cites both: every obligation it cites is read, in either order.
    let cases = [("rq1-first", ["RQ1", "SF1"]), ("sf1-first", ["SF1", "RQ1"])];
    let cases = cases.map(|(label, cites)| {
        let observed = vec![observation(&cites, "violation", "confident")];
        let answers = vec![submission(items(&[]), observed), passes()];
        (format!("row1-together-{label}"), answers)
    });
    for (label, came) in each(cases.into(), true) {
        one_attempt(&came, Decision::Fail, DAMAGED, &label);
    }
}

#[test]
fn row_2_a_confident_violation_of_rq1_with_no_place_widens_once_and_is_checked_again() {
    let (world, came, finished) = twice_keeping("row2-widens", vec![remains(), passes()]);
    assert_eq!(
        came.attempts,
        [
            (Some(Decision::Fail), None),
            (Some(Decision::Pass), wider())
        ]
    );
    assert_eq!((came.verdict, came.kept), (Some(Decision::Pass), Some(2)));
    assert_eq!(came.ended, "attempt 2 passed");
    // The retry shaped the same found mask again, with twice the px of its step grow,
    // and nothing else changed: no second find, the same phrase, one more fill.
    let record = world.record(finished.run().as_str());
    let labels: Vec<String> = crate::checked::calls(&record)
        .into_iter()
        .map(|c| c.0)
        .collect();
    assert_eq!(
        labels,
        [
            "a1_find", "a1_shape", "a1_fill", "a1_check", "a2_shape", "a2_fill", "a2_check"
        ]
    );
    let (first, second) = (
        world.record(&run_of(&record, "a1_shape")),
        world.record(&run_of(&record, "a2_shape")),
    );
    assert_eq!(argument(&first, "set"), None);
    assert_eq!(argument(&second, "set").as_deref(), Some("grow.px=8"));
    assert_eq!(
        first.started().unwrap().given,
        second.started().unwrap().given
    );
    assert_ne!(first.outputs()[0].artifact, second.outputs()[0].artifact);
    // A row 2 finding that cites a place gives the located retry (plan m4, task L2):
    // `located::a_confident_violation_of_rq1_with_a_place_…`.
}

#[test]
fn row_2_a_second_widening_in_one_run_is_not_made() {
    // Three attempts are allowed, and the judge would say the thing remains each time.
    let (world, came) = twice("row2-once", vec![remains(), remains(), passes()]);
    assert_eq!(
        came.attempts,
        [
            (Some(Decision::Fail), None),
            (Some(Decision::Fail), wider())
        ]
    );
    assert_eq!((came.verdict, came.kept), (Some(Decision::Fail), Some(1)));
    assert_eq!(came.ended, WIDENED);
    assert_eq!(world.kit.unplayed(), 1);
}

#[test]
fn row_3_a_tentative_violation_of_rq1_enlarges_nothing() {
    let answers = submission(items(&[("RQ1", "violation", "tentative")]), Vec::new());
    let (world, came) = twice("row3", vec![answers, passes()]);
    one_attempt(&came, Decision::Unsure, NOTHING, "row 3");
    assert_eq!(world.kit.unplayed(), 1);
}

/// A findings document of one finding about `RQ1`.
fn document(
    disposition: Disposition,
    confidence: Option<Confidence>,
    decision: Decision,
) -> Findings {
    let finding = Finding {
        form: Form::Observation,
        obligations: vec!["RQ1".to_owned()],
        disposition,
        confidence,
        place: None,
    };
    Findings::of(decision, vec![finding], LeftOut::default()).unwrap()
}

#[test]
fn row_4_held_allowed_change_hidden_cant_tell_and_observed_each_enlarge_nothing() {
    // About RQ1, each disposition a submission can give it, at each confidence. All
    // but a confident `held`, which is no finding and passes, make the verdict unsure.
    let mut cases = Vec::new();
    for disposition in ["held", "hidden", "cant_tell"] {
        for confidence in ["confident", "tentative"] {
            let answers = submission(items(&[("RQ1", disposition, confidence)]), Vec::new());
            cases.push((
                format!("row4-{disposition}-{confidence}"),
                vec![answers, passes()],
            ));
        }
    }
    for (label, came) in each(cases, false) {
        if label == "row4-held-confident" {
            assert_eq!(came.attempts, [(Some(Decision::Pass), None)], "{label}");
            assert_eq!(came.ended, "attempt 1 passed", "{label}");
        } else {
            one_attempt(&came, Decision::Unsure, NOTHING, &label);
        }
    }
    // A submission cannot say `allowed_change` or `observed` of RQ1 (no exemption
    // waives a job's requirement, and only a rule that observes is observed): the
    // table is shown each on the document a check hands back.
    use Confidence::{Confident, Tentative};
    let documents = [
        (Disposition::AllowedChange, Some(Confident), Decision::Pass),
        (
            Disposition::AllowedChange,
            Some(Tentative),
            Decision::Unsure,
        ),
        (Disposition::Observed, None, Decision::Pass),
        (Disposition::Held, Some(Tentative), Decision::Unsure),
        (Disposition::Hidden, Some(Confident), Decision::Unsure),
        (Disposition::CantTell, Some(Tentative), Decision::Unsure),
    ];
    for (disposition, confidence, decision) in documents {
        let findings = document(disposition, confidence, decision);
        assert_eq!(
            row(&findings.items()[0]),
            Row::NotAViolation,
            "{disposition:?}"
        );
        assert_eq!(read(&findings), Reading::Nothing, "{disposition:?}");
    }
}

#[test]
fn row_5_a_violation_of_rq3_beside_a_confident_violation_of_rq1_still_retries() {
    let both = items(&[
        ("RQ1", "violation", "confident"),
        ("RQ3", "violation", "confident"),
    ]);
    let (_, came) = twice("row5-beside", vec![submission(both, Vec::new()), passes()]);
    assert_eq!(
        came.attempts,
        [
            (Some(Decision::Fail), None),
            (Some(Decision::Pass), wider())
        ]
    );
    assert_eq!(came.kept, Some(2));
}

#[test]
fn row_5_a_violation_of_rq3_alone_ends_the_attempts() {
    let answers = submission(items(&[("RQ3", "violation", "confident")]), Vec::new());
    let (world, came) = twice("row5-alone", vec![answers, passes()]);
    one_attempt(&came, Decision::Fail, NOTHING, "row 5");
    assert_eq!(world.kit.unplayed(), 1);
}

#[test]
fn a_finding_is_read_by_its_obligations_disposition_and_confidence_in_either_form() {
    // The same answers about RQ1 that the tests of rows 2 and 3 give as items of the
    // checklist, here as separate observations: confident, a retry either way;
    // tentative, none either way.
    let observed = |confidence: &str| {
        let said = vec![observation(&["RQ1"], "violation", confidence)];
        vec![submission(items(&[]), said), passes()]
    };
    let cases = vec![
        (
            "form-observation-confident".to_owned(),
            observed("confident"),
        ),
        (
            "form-observation-tentative".to_owned(),
            observed("tentative"),
        ),
    ];
    let came = each(cases, true);
    // What `row_2_…widens_once…` holds of the same answer as an item.
    let retried = &came[0].1;
    assert_eq!(
        retried.attempts,
        [
            (Some(Decision::Fail), None),
            (Some(Decision::Pass), wider())
        ]
    );
    assert_eq!(
        (retried.verdict, retried.kept),
        (Some(Decision::Pass), Some(2))
    );
    assert_eq!(retried.ended, "attempt 2 passed");
    // What `row_3_…` holds of the same answer as an item.
    one_attempt(&came[1].1, Decision::Unsure, NOTHING, "tentative");
    // And never by a judge's words: nothing of them is in a finding to be read.
    let cut = json!({"findings": "findings/1"});
    assert!(Findings::read(&cut).is_err());
    let finding = document(
        Disposition::Violation,
        Some(Confidence::Confident),
        Decision::Fail,
    );
    assert!(!finding.to_json().to_string().contains("reason"));
    assert_eq!(row(&finding.items()[0]), Row::Remains);
}
