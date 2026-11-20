//! `findings::check::…`, `findings::cache::…` and `findings::use_case::…`: the findings
//! `check` hands its caller (plan m4, task K2): both forms the judge answers in, every
//! obligation cited, the decision of the whole submission, what a short list says of
//! itself, and the same findings from a check that was made and from the same check
//! answered from the decision cache.
//!
//! The submissions are scripted, on the pictures the tests of `check` draw. The job is
//! the one a loop gives a check (the plan's task L1, item 3): source-fidelity with three
//! requirements, `RQ1` to `RQ3`.

use std::sync::Arc;

use serde_json::{Value as Json, json};
use whizbang_core::capability::written::read_script;
use whizbang_core::contract::record::Decision;
use whizbang_core::contract::{
    Confidence, Disposition, Executor, FINDINGS_MAX_BYTES, Finding, Findings, Finished, Form,
    LeftOut, Secrets,
};
use whizbang_squint::check::JUDGED;
use whizbang_squint::findings::of_judged;
use whizbang_squint::job::Job;
use whizbang_squint::use_case::{Kind, Obligation};
use whizbang_testkit::capabilities::FakeJudge;

use super::check::{judged_call, submits};
use super::world::{Left, World, left};

/// A requirement of a job: it must hold, and it matters.
fn requirement(id: &str, text: &str) -> Obligation {
    Obligation {
        id: id.to_owned(),
        kind: Kind::MustHold,
        text: text.to_owned(),
        matters: None,
        hidden_ok: false,
        scope: None,
        evidence: None,
        evidence_kinds: None,
        waives: Vec::new(),
        condition: None,
        disabled: false,
    }
}

/// The job a loop gives a check: source-fidelity, and the three requirements of a
/// removal.
fn job() -> Job {
    let mut job = Job::shorthand("source-fidelity", true, 0, None);
    job.requirements = vec![
        requirement("RQ1", "The leather collar is no longer visible."),
        requirement("RQ2", "Nothing else in the picture changed."),
        requirement("RQ3", "The filled area looks like its surroundings."),
    ];
    job
}

/// The checklist of that job: one item for each rule that must hold and each
/// requirement.
const ITEMS: [&str; 5] = ["SF1", "SF2", "RQ1", "RQ2", "RQ3"];

fn boxed(sides: [f64; 4]) -> Json {
    json!({"kind": "bbox", "frame": "raw:candidate", "bbox": sides, "polygon": null,
           "yaw": null, "pitch": null, "fov": null})
}

/// The box an answer of these tests cites when it is given no other.
const SOMEWHERE: [f64; 4] = [0.125, 0.25, 0.5, 0.75];

/// How a test says one item was answered: its obligation, its disposition and its
/// confidence.
type Answered = (&'static str, &'static str, &'static str);

/// The items of the checklist, each held, confidently, but for those given, each with a
/// box when it is a violation.
fn items(but: &[Answered]) -> Vec<Json> {
    (ITEMS.iter())
        .map(|obligation| {
            let given = but.iter().find(|(of, _, _)| of == obligation);
            let (disposition, confidence) =
                given.map_or(("held", "confident"), |(_, d, c)| (*d, *c));
            let location = (disposition == "violation").then(|| boxed(SOMEWHERE));
            json!({"id": format!("chk:ob:{obligation}@candidate"), "disposition": disposition,
                   "confidence": confidence, "reason": "MARMALADE in an item's reason.",
                   "allowed_by": null, "evidence": [], "location": location})
        })
        .collect()
}

/// A separate observation that cites these obligations, at this location.
fn observation(cites: &[&str], disposition: &str, confidence: &str, location: Json) -> Json {
    json!({"obligation_ids": cites, "disposition": disposition, "confidence": confidence,
           "reason": "MARMALADE in an observation's reason.", "allowed_by": null,
           "evidence": [], "location": location})
}

fn submission(items: Vec<Json>, observations: Vec<Json>) -> Json {
    json!({"schema_version": 1, "items": items, "findings": observations,
           "opinion": {"verdict": "pass", "reason": "MARMALADE in the opinion."}})
}

/// What one check came to: how it ended, what it left, and how many requests its judge
/// was sent.
struct Checked {
    finished: Finished,
    left: Left,
    requests: u64,
    /// The identity of the photograph the check took as its candidate.
    candidate: whizbang_core::identity::Digest,
}

impl Checked {
    fn findings(&self) -> &Findings {
        self.finished.findings().expect("the check hands findings")
    }

    /// The setting the run noted of the decision cache.
    fn cache(&self) -> String {
        let settings = &self.left.record().asked().settings;
        (settings.iter())
            .find(|setting| setting.name() == whizbang_squint::cache::SETTING)
            .map(|setting| format!("{:?}", setting.value()))
            .unwrap_or_default()
    }
}

/// `check` of the job on the world's two photographs, the other as the candidate, under
/// a judge that submits these answers at once and says which model answered.
fn checked(world: &World, answers: &Json) -> Checked {
    let turns = read_script(submits(answers.clone()).as_bytes()).unwrap();
    let turns = (turns.into_iter())
        .map(|turn| turn.map(|turn| turn.reported_by(Some("fake-judge"))))
        .collect();
    let judge = Arc::new(FakeJudge::scripted(turns));
    let executor = world.engine(judge.clone(), Secrets::none());
    let call = judged_call(&job(), &[world.other.id(), world.photo.id()]);
    let finished = executor.submit(&call).unwrap();
    drop(executor);
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    let left = left(&world.root(), world.fs.clone(), finished.run());
    Checked {
        finished,
        left,
        requests: judge.requests(),
        candidate: world.other.id().0,
    }
}

/// A world whose `check` reads and keeps the decision cache.
fn caching() -> World {
    let mut world = World::new();
    world.cached = true;
    world
}

/// The same check twice in one world: the one that was made, and the one answered from
/// the decision cache, which sends nothing. Both hand back the same document, and each
/// run's record keeps the same of it.
fn made_and_answered(answers: &Json) -> (Checked, Checked) {
    let world = caching();
    let made = checked(&world, answers);
    assert_eq!(made.requests, 1);
    assert!(made.cache().contains("\"kept\""), "{}", made.cache());
    let answered = checked(&world, answers);
    assert_eq!(answered.requests, 0, "answered from the cache");
    assert!(answered.cache().contains("\"hit\""), "{}", answered.cache());
    assert_eq!(
        answered.findings().to_json().to_string(),
        made.findings().to_json().to_string()
    );
    assert_eq!(
        answered.left.record().findings(),
        made.left.record().findings()
    );
    assert!(made.left.record().findings().is_some());
    (made, answered)
}

/// The verdict a caller would work out from a list of findings alone: fail on a
/// confident violation, unsure on any other finding, pass on none. Right only where
/// the list is complete.
fn worked_out(findings: &Findings) -> Decision {
    let failing = |finding: &Finding| {
        finding.disposition == Disposition::Violation
            && finding.confidence == Some(Confidence::Confident)
    };
    match findings.items() {
        [] => Decision::Pass,
        items if items.iter().any(failing) => Decision::Fail,
        _ => Decision::Unsure,
    }
}

/// What a finding is read by: its form, what it cites, its disposition and its
/// confidence; and whether it has a place.
type Read<'a> = (Form, &'a [&'a str], Disposition, Confidence, bool);

fn read(findings: &Findings) -> Vec<(Form, Vec<String>, Disposition, Confidence, bool)> {
    (findings.items().iter())
        .map(|finding| {
            (
                finding.form,
                finding.obligations.clone(),
                finding.disposition,
                finding.confidence.unwrap(),
                finding.place.is_some(),
            )
        })
        .collect()
}

fn assert_read(findings: &Findings, expected: &[Read<'_>]) {
    let expected: Vec<_> = (expected.iter())
        .map(|(form, cites, disposition, confidence, placed)| {
            let cites: Vec<String> = cites.iter().map(|id| (*id).to_owned()).collect();
            (*form, cites, *disposition, *confidence, *placed)
        })
        .collect();
    assert_eq!(read(findings), expected);
}

/// The verdict the run handed back with its findings.
fn verdict(checked: &Checked) -> Decision {
    checked.finished.verdict().unwrap().decision()
}

mod check {
    use super::*;
    use Confidence::{Confident, Tentative};
    use Disposition::{CantTell, Held, Violation};
    use Form::{Item, Observation};

    #[test]
    fn a_checklist_item_and_a_separate_observation_are_each_handed_back_as_a_finding() {
        // The judge says the named thing is still there, as an item of its checklist,
        // and reports damage to a protected subject as a separate observation: both
        // count against a pass, and both are findings, on a check that was made and on
        // the same check answered from the cache.
        let answers = submission(
            items(&[("RQ1", "violation", "confident")]),
            vec![observation(
                &["SF1"],
                "violation",
                "confident",
                boxed([0.5, 0.5, 0.75, 1.0]),
            )],
        );
        let (made, answered) = made_and_answered(&answers);
        for checked in [&made, &answered] {
            let findings = checked.findings();
            assert_read(
                findings,
                &[
                    (Item, &["RQ1"], Violation, Confident, true),
                    (Observation, &["SF1"], Violation, Confident, true),
                ],
            );
            assert!(findings.is_complete());
            assert_eq!(findings.left_out(), LeftOut::default());
            assert_eq!(findings.decision(), Decision::Fail);
            assert_eq!(verdict(checked), Decision::Fail);
            // Each place is its answer's, on the candidate, in the submission's order.
            let places = findings.places();
            assert_eq!(places[0].image, checked.candidate);
            assert_eq!(places[0].boxed, Some(SOMEWHERE));
            assert_eq!(places[1].boxed, Some([0.5, 0.5, 0.75, 1.0]));
        }
    }

    #[test]
    fn a_finding_keeps_every_obligation_it_cites_its_disposition_and_its_confidence() {
        // An observation that cites three obligations is one finding of the three, in
        // the order it cited them, each once; an item that the judge could not tell is
        // a finding with that disposition and no place; a polygon is kept as one.
        let polygon = json!({"kind": "polygon", "frame": "raw:candidate", "bbox": null,
            "polygon": [[0.25, 0.25], [0.75, 0.25], [0.5, 0.75]],
            "yaw": null, "pitch": null, "fov": null});
        let answers = submission(
            items(&[
                ("RQ2", "cant_tell", "confident"),
                ("SF2", "held", "tentative"),
            ]),
            vec![observation(
                &["RQ1", "RQ3", "SF2", "RQ1"],
                "violation",
                "tentative",
                polygon,
            )],
        );
        let (made, answered) = made_and_answered(&answers);
        for checked in [&made, &answered] {
            let findings = checked.findings();
            assert_read(
                findings,
                &[
                    (Item, &["SF2"], Held, Tentative, false),
                    (Item, &["RQ2"], CantTell, Confident, false),
                    (
                        Observation,
                        &["RQ1", "RQ3", "SF2"],
                        Violation,
                        Tentative,
                        true,
                    ),
                ],
            );
            assert_eq!(
                findings.places()[0].polygon,
                Some(vec![[0.25, 0.25], [0.75, 0.25], [0.5, 0.75]])
            );
            assert!(findings.is_complete());
            assert_eq!(findings.decision(), Decision::Unsure);
            assert_eq!(verdict(checked), Decision::Unsure);
        }
        // The record keeps a count for each obligation an answer cites.
        let counts = made.left.record().findings().unwrap().counts.clone();
        let cited: Vec<(&str, u64)> = (counts.iter())
            .map(|count| (count.obligation.as_str(), count.count))
            .collect();
        assert_eq!(
            cited,
            [("RQ1", 1), ("RQ2", 1), ("RQ3", 1), ("SF2", 1), ("SF2", 1)]
        );
    }

    #[test]
    fn the_verdict_worked_out_from_complete_findings_is_the_decision_they_carry() {
        // Where nothing was left out, a caller that works the verdict out from the list
        // alone gets the decision the document carries, which is the verdict `check`
        // gave: for a pass, for a confident report that the named thing remains, alone,
        // and for each way an answer makes the verdict unsure.
        let world = World::new();
        let cases: [(&[Answered], Decision); 5] = [
            (&[], Decision::Pass),
            (&[("RQ1", "violation", "confident")], Decision::Fail),
            (&[("RQ1", "violation", "tentative")], Decision::Unsure),
            (&[("SF1", "held", "tentative")], Decision::Unsure),
            (
                &[
                    ("RQ3", "hidden", "confident"),
                    ("RQ1", "violation", "confident"),
                ],
                Decision::Fail,
            ),
        ];
        for (but, decided) in cases {
            let checked = checked(&world, &submission(items(but), Vec::new()));
            let findings = checked.findings();
            assert!(findings.is_complete(), "{but:?}");
            assert_eq!(findings.items().len(), but.len(), "{but:?}");
            assert_eq!(findings.decision(), decided, "{but:?}");
            assert_eq!(worked_out(findings), decided, "{but:?}");
            assert_eq!(verdict(&checked), decided, "{but:?}");
        }
        // A confident violation of RQ1 alone is one finding, with its place, and the
        // same from the cache.
        let alone = submission(items(&[("RQ1", "violation", "confident")]), Vec::new());
        let (made, answered) = made_and_answered(&alone);
        for checked in [&made, &answered] {
            assert_read(
                checked.findings(),
                &[(Item, &["RQ1"], Violation, Confident, true)],
            );
            assert_eq!(worked_out(checked.findings()), Decision::Fail);
        }
    }

    /// As many separate observations, each a tentative violation of `SF1` with a box.
    fn tentative(count: usize) -> Vec<Json> {
        (0..count)
            .map(|_| observation(&["SF1"], "violation", "tentative", boxed(SOMEWHERE)))
            .collect()
    }

    #[test]
    fn findings_carry_the_decision_of_the_whole_submission_whatever_was_cut() {
        // 32 tentative violations, then the only confident one: the list keeps the 32
        // and cuts the one that decides. Worked out from the list the verdict would be
        // unsure; the document carries fail, which is the verdict, and says it is not
        // complete.
        let mut observations = tentative(32);
        observations.push(observation(
            &["RQ1"],
            "violation",
            "confident",
            boxed(SOMEWHERE),
        ));
        let answers = submission(items(&[]), observations);
        let (made, answered) = made_and_answered(&answers);
        for checked in [&made, &answered] {
            let findings = checked.findings();
            assert_eq!(findings.items().len(), 32);
            assert!(
                (findings.items().iter()).all(|finding| finding.confidence == Some(Tentative)
                    && finding.obligations == ["SF1"])
            );
            assert_eq!(worked_out(findings), Decision::Unsure);
            assert_eq!(findings.decision(), Decision::Fail);
            assert_eq!(verdict(checked), Decision::Fail);
            assert!(!findings.is_complete());
            // The one finding cut; and of the 32 kept, the places past the eighth.
            let left_out = LeftOut {
                findings: 1,
                places: 24,
            };
            assert_eq!(findings.left_out(), left_out);
        }
    }

    #[test]
    fn an_answer_whose_row_is_recorded_is_no_finding() {
        // Every item held, confidently, and an observation under the observe rule: each
        // row is `recorded`, the verdict is pass, and the document, which is handed
        // back all the same, lists nothing and is complete.
        let noted = observation(&["SF4"], "observed", "confident", boxed(SOMEWHERE));
        let checked = checked(&World::new(), &submission(items(&[]), vec![noted]));
        let findings = checked.findings();
        assert!(findings.items().is_empty());
        assert!(findings.is_complete());
        assert_eq!(findings.decision(), Decision::Pass);
        assert_eq!(verdict(&checked), Decision::Pass);
        // The run kept all six answers, and none is a finding.
        let kept: Json = serde_json::from_slice(checked.left.diagnostic(JUDGED)).unwrap();
        let rows: Vec<&Json> = (kept["answers"].as_array().unwrap().iter())
            .map(|answer| &answer["row"])
            .collect();
        assert_eq!(rows, [&json!("recorded"); 6]);
    }

    /// What a check that reported a violation of `RQ1` and a separate one of `SF1` kept
    /// of what it decided, for a test to change.
    fn kept() -> Json {
        let answers = submission(
            items(&[("RQ1", "violation", "confident")]),
            vec![observation(
                &["SF1"],
                "violation",
                "confident",
                boxed(SOMEWHERE),
            )],
        );
        let checked = checked(&World::new(), &answers);
        serde_json::from_slice(checked.left.diagnostic(JUDGED)).unwrap()
    }

    fn of(kept: &Json) -> Findings {
        of_judged(kept.to_string().as_bytes()).unwrap()
    }

    #[test]
    fn a_place_on_an_image_the_run_was_not_given_is_left_out_and_counted() {
        // A submission that names a frame outside its job is refused before anything is
        // kept, so this is shown on the kept document, which is what the findings are
        // made from on a hit too: an answer whose location is on an image that is not
        // among the run's keeps its finding, loses its place, and is counted.
        let mut kept = kept();
        assert_eq!(of(&kept).places().len(), 2);
        assert_eq!(kept["answers"][5]["location"]["frame"], "raw:candidate");
        kept["answers"][5]["location"]["frame"] = json!("raw:elsewhere");
        let findings = of(&kept);
        assert_read(
            &findings,
            &[
                (Item, &["RQ1"], Violation, Confident, true),
                (Observation, &["SF1"], Violation, Confident, false),
            ],
        );
        assert!(!findings.is_complete());
        let left_out = LeftOut {
            findings: 0,
            places: 1,
        };
        assert_eq!(findings.left_out(), left_out);
        assert_eq!(findings.decision(), Decision::Fail);
        // An image the document lists under an identity that is none is not one the
        // run was given either.
        let mut kept = self::kept();
        kept["images"][0]["artifact"] = json!("nothing");
        assert_eq!(of(&kept).left_out().places, 2);
    }

    #[test]
    fn no_more_than_32_findings_and_eight_places_are_handed_back_and_no_text_of_the_judges() {
        // 40 observations, each with a place and words of the judge's: 32 findings,
        // eight places, in the order of the submission, and nothing the judge wrote.
        let answers = submission(items(&[("RQ1", "violation", "confident")]), tentative(40));
        let checked = checked(&World::new(), &answers);
        let findings = checked.findings();
        assert_eq!(findings.items().len(), 32);
        assert_eq!(findings.places().len(), 8);
        // The first is the item, as the submission has it; the eight places are those
        // of the first eight findings.
        assert_eq!(findings.items()[0].form, Item);
        assert!(findings.items()[..8].iter().all(|f| f.place.is_some()));
        assert!(findings.items()[8..].iter().all(|f| f.place.is_none()));
        let written = findings.to_json().to_string();
        assert!(written.len() <= FINDINGS_MAX_BYTES);
        assert!(!written.contains("MARMALADE"), "{written}");
        assert!(!written.contains("reason"), "{written}");
        // Nor does the record's part of them hold any.
        let record = serde_json::to_string(checked.left.record().findings().unwrap()).unwrap();
        assert!(!record.contains("MARMALADE"), "{record}");
        // The judge did write them: the run's own diagnostic holds its words.
        let reasons = checked.left.diagnostic(whizbang_squint::use_case::REASONS);
        assert!(String::from_utf8_lossy(reasons).contains("MARMALADE"));
    }

    #[test]
    fn findings_cut_short_in_any_way_say_they_are_not_complete_and_count_what_was_left_out() {
        let count = |findings: u64, places: u64| LeftOut { findings, places };
        // A 33rd finding: an item, then 32 observations. Each observation is of the
        // whole image, which is a location and no place.
        let whole = json!({"kind": "whole_image", "frame": "raw:candidate", "bbox": null,
            "polygon": null, "yaw": null, "pitch": null, "fov": null});
        let answers = submission(
            items(&[("RQ1", "violation", "confident")]),
            (0..32)
                .map(|_| observation(&["SF1"], "cant_tell", "confident", whole.clone()))
                .collect(),
        );
        let (made, answered) = made_and_answered(&answers);
        for checked in [&made, &answered] {
            let findings = checked.findings();
            assert_eq!(findings.items().len(), 32);
            assert!(!findings.is_complete());
            assert_eq!(findings.left_out(), count(1, 0));
            assert_eq!(findings.places().len(), 1);
        }
        // A ninth place.
        let answers = submission(items(&[]), tentative(9));
        let (made, answered) = made_and_answered(&answers);
        for checked in [&made, &answered] {
            let findings = checked.findings();
            assert_eq!(findings.items().len(), 9);
            assert_eq!(findings.places().len(), 8);
            assert!(findings.items()[8].place.is_none());
            assert!(!findings.is_complete());
            assert_eq!(findings.left_out(), count(0, 1));
        }
        // A place on another image, and each other way a list is short, on the kept
        // document a hit reads too.
        let mut elsewhere = kept();
        elsewhere["answers"][2]["location"]["frame"] = json!("raw:elsewhere");
        let findings = of(&elsewhere);
        assert!(!findings.is_complete());
        assert_eq!(findings.left_out(), count(0, 1));
        // A box with no area once its numbers are held inside the picture.
        let mut flat = kept();
        flat["answers"][2]["location"]["bbox"] = json!([1.0, 1.0, 1.0, 1.0]);
        assert_eq!(of(&flat).left_out(), count(0, 1));
        // A polygon that encloses no area: its points on one line, or all held to one
        // corner of the picture.
        for points in [
            json!([[0.25, 0.25], [0.5, 0.5], [0.75, 0.75]]),
            json!([[1.5, 1.5], [2.0, 3.0], [4.0, 1.25]]),
        ] {
            let mut line = kept();
            line["answers"][2]["location"] =
                json!({"kind": "polygon", "frame": "raw:candidate", "polygon": points});
            let findings = of(&line);
            assert_eq!(findings.left_out(), count(0, 1), "{points}");
            assert_eq!(findings.places().len(), 1, "{points}");
        }
        // An item that is about no obligation, and an observation that cites none, with
        // a row that counts: each is a finding that cannot be handed back, and is
        // counted, not dropped.
        let mut about_nothing = kept();
        about_nothing["answers"][2]["obligations"] = json!([]);
        about_nothing["answers"][5]["obligation_ids"] = json!([]);
        let findings = of(&about_nothing);
        assert!(findings.items().is_empty());
        assert!(!findings.is_complete());
        assert_eq!(findings.left_out(), count(2, 0));
        assert_eq!(findings.decision(), Decision::Fail);
        // An obligation whose id the kind does not take: the finding is left out.
        let mut named = kept();
        named["answers"][5]["obligation_ids"] = json!(["SF12345"]);
        let findings = of(&named);
        assert_eq!(findings.items().len(), 1);
        assert_eq!(findings.left_out(), count(1, 0));
        assert_eq!(findings.decision(), Decision::Fail);
        // More than fits 32 KiB: 32 findings of 16 obligations each, eight of them with
        // a polygon of 64 points. Places are cut from the end, then findings, and the
        // document fits.
        let mut large = kept();
        let cited: Vec<String> = (1..=16).map(|n| format!("LONGNAME{n:04}")).collect();
        // Numbers that take the most characters a fraction is written with.
        let points = vec![[1.234_567_890_123_456_7e-7, 9.876_543_210_987_654e-7]; 64];
        let one = json!({"obligation_ids": cited, "disposition": "allowed_change",
            "confidence": "tentative", "row": "unsure", "clamped": false,
            "location": {"kind": "polygon", "frame": "raw:candidate", "polygon": points}});
        large["answers"] = Json::Array(vec![one; 32]);
        let findings = of(&large);
        assert!(findings.to_json().to_string().len() <= FINDINGS_MAX_BYTES);
        assert!(!findings.is_complete());
        // The eight places the limit allows do not all fit beside the findings: more
        // than the 24 past the eighth are left out.
        let left_out = findings.left_out();
        assert!(left_out.places > 24, "{left_out:?}");
        assert_eq!(left_out.findings, 0, "{left_out:?}");
        assert_eq!(
            findings.items().len() as u64 + left_out.findings,
            32,
            "{left_out:?}"
        );
        assert_eq!(
            findings.places().len() as u64 + left_out.places,
            32,
            "{left_out:?}"
        );
        assert_eq!(findings.decision(), Decision::Unsure);
        // And a document that lists nothing short says it is complete.
        assert!(of(&kept()).is_complete());
    }

    #[test]
    fn a_check_answered_from_the_cache_hands_back_the_findings_of_the_run_that_answered() {
        // A checklist violation of one obligation beside a separate observation that
        // cites two.
        let answers = submission(
            items(&[("RQ1", "violation", "confident")]),
            vec![observation(
                &["SF1", "RQ2"],
                "violation",
                "tentative",
                boxed([0.0, 0.0, 0.25, 0.25]),
            )],
        );
        let (made, answered) = made_and_answered(&answers);
        assert_read(
            answered.findings(),
            &[
                (Item, &["RQ1"], Violation, Confident, true),
                (Observation, &["SF1", "RQ2"], Violation, Tentative, true),
            ],
        );
        assert_eq!(answered.findings(), made.findings());
        assert_eq!(answered.findings().decision(), Decision::Fail);
        // The run that was answered names the run that answered it, and keeps that
        // run's document, byte for byte: its findings are worked out from it.
        let settings = &answered.left.record().asked().settings;
        let by = (settings.iter())
            .find(|setting| setting.name() == whizbang_squint::cache::ANSWERED_BY)
            .map(|setting| format!("{:?}", setting.value()))
            .unwrap();
        assert!(by.contains(made.finished.run().as_str()), "{by}");
        assert_eq!(
            answered.left.diagnostic(JUDGED),
            made.left.diagnostic(JUDGED)
        );
        assert_eq!(
            &of_judged(answered.left.diagnostic(JUDGED)).unwrap(),
            answered.findings()
        );
    }
}

mod cache {
    use std::sync::Mutex;

    use whizbang_core::contract::{
        Action, ActionDescriptor, Context, Failure, PreparedCall, Produced, Verdict,
    };

    use super::*;

    /// A run of `check` that findings cannot be made from: an action of its name and
    /// descriptor that keeps a key for the decision cache and hands back the verdict
    /// pass, and keeps of what it decided this document, or none.
    struct Earlier {
        check: Arc<dyn Action>,
        key: (String, Vec<u8>),
        judged: Option<Vec<u8>>,
        ran: Arc<Mutex<u32>>,
    }

    impl Action for Earlier {
        fn descriptor(&self) -> &ActionDescriptor {
            self.check.descriptor()
        }

        fn run(&self, _: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
            *self.ran.lock().unwrap() += 1;
            let (name, material) = &self.key;
            context.diagnostics().write(name, material).unwrap();
            if let Some(judged) = &self.judged {
                context.diagnostics().write(JUDGED, judged).unwrap();
            }
            let verdict = Verdict::new(Decision::Pass, "as an earlier build kept it");
            Ok(Produced::nothing().with_verdict(verdict))
        }
    }

    /// A store that holds a completed run of `check` under the key of the check these
    /// tests make, with the verdict pass and this document of what it decided, or none:
    /// that run is passed over. The check runs, its judge is asked, and its verdict and
    /// findings are its own, not the entry's pass; and the new entry replaces it: the
    /// same check again is answered by the run that was made, with that run's findings.
    fn passed_over(judged: Option<&[u8]>) {
        let answers = submission(items(&[("RQ1", "violation", "confident")]), Vec::new());
        // The key of this check, from a world that made it: its name and its material.
        let elsewhere = caching();
        let made = checked(&elsewhere, &answers);
        let key = (made.left.names.iter())
            .find(|(name, _)| name.starts_with("diagnostics/cache-"))
            .map(|(name, (_, bytes))| (name["diagnostics/".len()..].to_owned(), bytes.clone()))
            .expect("the check kept its key");
        // A second world of the same pictures, whose store holds the run.
        let world = caching();
        let ran = Arc::new(Mutex::new(0));
        let judge = Arc::new(FakeJudge::scripted(Vec::new()));
        let earlier = world.engine_wrapping(judge, Secrets::none(), |action| {
            if action.descriptor().name() != "check" {
                return action;
            }
            Arc::new(Earlier {
                check: action,
                key: key.clone(),
                judged: judged.map(<[u8]>::to_vec),
                ran: ran.clone(),
            })
        });
        let call = judged_call(&job(), &[world.other.id(), world.photo.id()]);
        let old = earlier.submit(&call).unwrap();
        drop(earlier);
        assert_eq!(old.outcome().name(), "completed");
        assert_eq!(*ran.lock().unwrap(), 1);
        let old_left = left(&world.root(), world.fs.clone(), old.run());
        assert!(old_left.has(&key.0));
        assert_eq!(old_left.has(JUDGED), judged.is_some());
        let first = checked(&world, &answers);
        assert_eq!(first.requests, 1);
        assert!(first.cache().contains("\"kept\""), "{}", first.cache());
        assert_eq!(verdict(&first), Decision::Fail);
        assert_eq!(first.findings().items().len(), 1);
        let second = checked(&world, &answers);
        assert_eq!(second.requests, 0);
        assert!(second.cache().contains("\"hit\""), "{}", second.cache());
        let settings = &second.left.record().asked().settings;
        let by = (settings.iter())
            .find(|setting| setting.name() == whizbang_squint::cache::ANSWERED_BY)
            .map(|setting| format!("{:?}", setting.value()))
            .unwrap();
        assert!(by.contains(first.finished.run().as_str()), "{by}");
        assert_eq!(second.findings(), first.findings());
        assert_eq!(verdict(&second), Decision::Fail);
    }

    #[test]
    fn an_entry_that_kept_no_judged_document_is_not_answered_from() {
        passed_over(None);
    }

    #[test]
    fn an_entry_whose_judged_document_gives_no_findings_is_not_answered_from() {
        // A document that is not JSON, and one that holds no list of answers.
        passed_over(Some(b"not a document"));
        passed_over(Some(br#"{"images": []}"#));
    }

    #[test]
    fn an_entry_whose_findings_would_not_carry_its_verdict_is_not_answered_from() {
        // A well-formed document whose one answer makes the decision unsure, beside a
        // recorded verdict of pass.
        let unsure = json!({"images": [], "answers": [{"id": "chk:ob:SF1@candidate",
            "obligations": ["SF1"], "disposition": "held", "confidence": "tentative",
            "row": "unsure", "location": null, "clamped": false}]});
        assert_eq!(
            of_judged(unsure.to_string().as_bytes()).unwrap().decision(),
            Decision::Unsure
        );
        passed_over(Some(unsure.to_string().as_bytes()));
    }

    #[test]
    fn a_check_whose_kept_document_held_a_secret_keeps_no_key() {
        // A secret of eight digits that a box's coordinate spells: taken out of the
        // stored document as bytes, it would leave a document no hit could read, so the
        // check would be paid again each time and each run would leave a dead entry.
        // Such a run keeps its document redacted, hands back its findings, and keeps
        // no key.
        let world = caching();
        let secret = "12345678";
        let mut answered = items(&[("RQ1", "violation", "confident")]);
        answered[2]["location"] = boxed([0.12345678, 0.25, 0.5, 0.75]);
        let answers = submission(answered, Vec::new());
        let run = |world: &World| {
            let turns = read_script(submits(answers.clone()).as_bytes()).unwrap();
            let turns = (turns.into_iter())
                .map(|turn| turn.map(|turn| turn.reported_by(Some("fake-judge"))))
                .collect();
            let judge = Arc::new(FakeJudge::scripted(turns));
            let secrets = Secrets::new([secret.to_owned()]).unwrap();
            let executor = world.engine(judge.clone(), secrets);
            let call = judged_call(&job(), &[world.other.id(), world.photo.id()]);
            let finished = executor.submit(&call).unwrap();
            drop(executor);
            assert_eq!(finished.outcome().name(), "completed");
            let left = left(&world.root(), world.fs.clone(), finished.run());
            (finished, left, judge.requests())
        };
        let (finished, kept, requests) = run(&world);
        assert_eq!(requests, 1);
        assert_eq!(finished.findings().unwrap().items().len(), 1);
        assert!(!String::from_utf8_lossy(kept.diagnostic(JUDGED)).contains(secret));
        assert!(!kept.names.keys().any(|name| name.contains("cache-")));
        let said = (kept.record().asked().settings.iter())
            .find(|setting| setting.name() == whizbang_squint::cache::SETTING)
            .map(|setting| format!("{:?}", setting.value()))
            .unwrap();
        assert!(
            said.contains("not kept: what the run decided holds a secret"),
            "{said}"
        );
        // The same check again is made again, and still leaves no entry.
        let (_, again, requests) = run(&world);
        assert_eq!(requests, 1);
        assert!(!again.names.keys().any(|name| name.contains("cache-")));
    }
}

mod use_case {
    use whizbang_core::artifact::ArtifactId;
    use whizbang_core::contract::is_obligation_id;

    use super::*;
    use crate::check::checklist_of;
    use crate::use_case::{answers, fitting};
    use crate::world::judge;

    #[test]
    fn a_use_case_recipe_hands_its_caller_the_findings_of_its_last_step() {
        // Each of the five recipes, under a judge that reports a confident violation of
        // the last item of its checklist: the recipe's caller is handed the findings of
        // the judge's step, and the recipe's record keeps what that step's keeps.
        let world = World::new();
        for name in whizbang_squint::use_case::NAMES {
            let brief = (name == "brief-compliance").then_some("A red square on grey.");
            let job = fitting(name, brief);
            let images: Vec<ArtifactId> = [world.other.id(), world.photo.id()]
                .into_iter()
                .take(job.images.len())
                .collect();
            let checklist = checklist_of(&job);
            let last = checklist.items.last().unwrap();
            let submitted = answers(&checklist, &last.id, "violation", "confident");
            let judge = judge(&submits(submitted), &[]);
            let executor = world.engine(judge, Secrets::none());
            let finished = executor.submit(&World::recipe(&job, &images)).unwrap();
            drop(executor);
            assert_eq!(finished.outcome().name(), "completed", "{name}");
            let findings = finished.findings().unwrap_or_else(|| panic!("{name}"));
            assert_eq!(findings.decision(), Decision::Fail, "{name}");
            assert_eq!(finished.verdict().unwrap().decision(), Decision::Fail);
            assert!(findings.is_complete(), "{name}");
            // One finding of the form `item` for each obligation the item is about:
            // one, but for the item of a reference, which is about each obligation
            // that consults it.
            let about: Vec<String> = (last.instances.iter())
                .map(|instance| instance.split('@').next().unwrap().to_owned())
                .collect();
            let cited: Vec<String> = (findings.items().iter())
                .map(|finding| {
                    assert_eq!(finding.form, Form::Item, "{name}");
                    assert_eq!(finding.disposition, Disposition::Violation, "{name}");
                    assert_eq!(finding.confidence, Some(Confidence::Confident));
                    assert!(finding.place.is_some(), "{name}");
                    assert_eq!(finding.obligations.len(), 1, "{name}");
                    finding.obligations[0].clone()
                })
                .collect();
            assert_eq!(cited, about, "{name}");
            assert_eq!(about.len() > 1, last.id.starts_with("chk:ref:"), "{name}");
            // The judge's step handed the same, and both records keep it.
            let recipe = left(&world.root(), world.fs.clone(), finished.run());
            let step = recipe.record().steps().last().unwrap();
            assert_eq!(step.action(), "check", "{name}");
            let run = whizbang_core::contract::RunId::parse(step.run()).unwrap();
            let judged = left(&world.root(), world.fs.clone(), &run);
            assert!(recipe.record().findings().is_some(), "{name}");
            assert_eq!(recipe.record().findings(), judged.record().findings());
            assert_eq!(
                &of_judged(judged.diagnostic(JUDGED)).unwrap(),
                findings,
                "{name}"
            );
        }
    }

    #[test]
    fn a_requirements_id_is_held_to_what_a_finding_may_cite() {
        // A job's requirement is cited by findings, and `findings/1` takes an id of at
        // most four digits: a requirement of more is refused where the job is resolved,
        // in the job's own words, and not left out of the findings later.
        let with = |id: &str| {
            let mut job = job();
            job.requirements
                .push(requirement(id, "Something else holds."));
            whizbang_squint::use_case::resolve(&job)
        };
        assert!(is_obligation_id("RQ9999") && with("RQ9999").is_ok());
        assert!(!is_obligation_id("RQ12345"));
        assert_eq!(
            with("RQ12345").unwrap_err(),
            "id \"RQ12345\" must be two capital letters and a number of at most four digits \
             (SF1, RQ2)"
        );
    }
}
