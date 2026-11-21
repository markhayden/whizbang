//! `submission::…`: what a submission cites as evidence (plan m3, task Q1, item 6; the
//! carry-over row on what the port of the submission does not hold): squint held each
//! cited tool call to the calls of the run that completed.

use serde_json::json;
use whizbang_squint::use_case::cited_calls_held;

fn citing(item: serde_json::Value, finding: serde_json::Value) -> serde_json::Value {
    json!({"items": [{"id": "chk:ob:SF1@candidate", "evidence": item}],
           "findings": [{"evidence": finding}]})
}

#[test]
fn a_cited_call_that_is_not_a_completed_call_of_the_run_is_refused() {
    let shown = vec!["call:0".to_owned(), "call:1".to_owned()];
    let also = vec!["toolu_a".to_owned(), "toolu_b".to_owned()];
    // Calls of the run that completed, or none at all: held.
    cited_calls_held(
        &citing(json!(["toolu_a"]), json!(["toolu_b"])),
        &shown,
        &also,
    )
    .unwrap();
    cited_calls_held(&citing(json!([]), json!([])), &shown, &also).unwrap();
    cited_calls_held(&json!({"items": [{"id": "x"}]}), &shown, &also).unwrap();
    // A call the run never made, or one that did not complete: refused, naming where.
    let item = cited_calls_held(&citing(json!(["toolu_z"]), json!([])), &shown, &also);
    assert!(item.unwrap_err().message.contains("chk:ob:SF1@candidate"));
    let finding = cited_calls_held(&citing(json!([]), json!(["toolu_z"])), &shown, &also);
    assert!(finding.unwrap_err().message.contains("finding 1"));
    // An evidence list that is not a list of names is refused too.
    let wrong = cited_calls_held(&citing(json!("toolu_a"), json!([])), &shown, &also);
    assert!(wrong.is_err());
}

#[test]
fn a_call_is_cited_by_the_id_its_result_gives_the_judge_or_by_the_providers_own() {
    // The gate of the details first on 2026-11-19: 66 of the 67 runs refused for their
    // citations cited what the judge was shown (`call:0`, the id the result of `align`
    // gives) or what no result gave it an id for, while the run held only the
    // provider's own ids of its calls, which no judge was shown.
    let shown = vec!["call:0".to_owned(), "1795-6".to_owned()];
    let also = vec!["toolu_a".to_owned()];
    let cites = citing(json!(["call:0", "toolu_a"]), json!(["1795-6"]));
    cited_calls_held(&cites, &shown, &also).unwrap();
}

#[test]
fn a_refused_citation_names_the_ids_the_judge_may_cite_and_none_of_the_providers() {
    let shown = vec!["call:0".to_owned(), "call:2".to_owned()];
    let also = vec!["toolu_a".to_owned()];
    let refused = cited_calls_held(&citing(json!(["view"]), json!([])), &shown, &also);
    assert_eq!(
        refused.unwrap_err().message,
        "chk:ob:SF1@candidate: evidence cites a tool call that is not a completed call of \
         this run; cite only call:0, call:2, or none"
    );
    let refused = cited_calls_held(&citing(json!(["view"]), json!([])), &[], &also);
    assert_eq!(
        refused.unwrap_err().message,
        "chk:ob:SF1@candidate: evidence cites a tool call that is not a completed call of \
         this run; no call of this run completed, so evidence is an empty list"
    );
}

// --- each rejection's class, and the words the judge sees (plan e1, task T1) -------------------

use serde_json::Value as Json;
use whizbang_squint::use_case::{self, Checklist, Class};

/// An answer of an item of source-fidelity, held confidently unless changed.
fn answer(id: &str) -> Json {
    json!({"id": id, "disposition": "held", "confidence": "confident", "reason": "Looked at.",
           "allowed_by": null, "evidence": [], "location": null})
}

/// The two items, the first changed by `first`.
fn with_first(first: &dyn Fn(&mut Json)) -> Json {
    let mut one = answer("chk:ob:SF1@candidate");
    first(&mut one);
    json!({"items": [one, answer("chk:ob:SF2@candidate")], "findings": []})
}

/// The two items held, and one finding: a violation of SF1 on the candidate as a whole,
/// changed by `change`.
fn with_finding(change: &dyn Fn(&mut Json)) -> Json {
    let mut finding = json!({"obligation_ids": ["SF1"], "disposition": "violation",
        "confidence": "confident", "reason": "Seen.", "allowed_by": null, "evidence": [],
        "location": {"kind": "whole_image", "frame": "raw:candidate"}});
    change(&mut finding);
    let mut submitted = with_first(&|_| {});
    submitted["findings"] = json!([finding]);
    submitted
}

fn located(item: &mut Json, location: Json) {
    item["disposition"] = json!("violation");
    item["location"] = location;
}

/// Every way `decided_of` and `cited_calls_held` refuse a submission of source-fidelity:
/// what is submitted, the class, and the words the judge is sent, as they were before
/// classes were recorded.
fn refusals() -> Vec<(&'static str, Json, Class, &'static str)> {
    use Class::*;
    vec![
        (
            "a number",
            json!(7),
            NotAList,
            "the submission is not an object of items and findings",
        ),
        (
            "items as text",
            json!({"items": "[]", "findings": []}),
            NotAList,
            "the submission is not an object of items and findings: items is text; each is a \
             list of objects, not a string that holds one",
        ),
        (
            "no items",
            json!({"findings": []}),
            NotAList,
            "items is not a list",
        ),
        (
            "an item of no checklist",
            {
                let mut s = with_first(&|_| {});
                s["items"]
                    .as_array_mut()
                    .unwrap()
                    .push(answer("chk:ob:SF9@candidate"));
                s
            },
            UnknownItem,
            "an item is not on the checklist",
        ),
        (
            "a confidence of no schema",
            with_first(&|i| i["confidence"] = json!("very")),
            BadField,
            "chk:ob:SF1@candidate: its disposition, confidence, reason, allowed_by or location \
             is not of the schema",
        ),
        (
            "an empty reason",
            with_first(&|i| i["reason"] = json!("  ")),
            BadField,
            "chk:ob:SF1@candidate: the reason is empty",
        ),
        (
            "observed for an item",
            with_first(&|i| i["disposition"] = json!("observed")),
            BadField,
            "chk:ob:SF1@candidate: the disposition is not one of violation, held, \
             allowed_change, hidden, cant_tell",
        ),
        (
            "a violation with no location",
            with_first(&|i| i["disposition"] = json!("violation")),
            Location,
            "chk:ob:SF1@candidate: a violation has a location",
        ),
        (
            "a frame of an aligned picture",
            with_first(&|i| {
                located(
                    i,
                    json!({"kind": "whole_image", "frame": "aligned:candidate"}),
                )
            }),
            Location,
            "chk:ob:SF1@candidate: the location's frame is not one of raw:candidate, \
             raw:source: raw: and the id of an image of the job, never the frame of a view or \
             of an aligned picture",
        ),
        (
            "a box upside down",
            with_first(&|i| {
                located(
                    i,
                    json!({"kind": "bbox", "frame": "raw:candidate", "bbox": [0.5, 0.5, 0.1, 0.1]}),
                )
            }),
            Location,
            "chk:ob:SF1@candidate: a bbox has left under right and top under bottom",
        ),
        (
            "a kind of no schema",
            with_first(&|i| located(i, json!({"kind": "circle", "frame": "raw:candidate"}))),
            Location,
            "chk:ob:SF1@candidate: the location's kind is bbox, polygon, whole_image or absent",
        ),
        (
            "an allowed change naming nothing",
            with_first(&|i| i["disposition"] = json!("allowed_change")),
            AllowedBy,
            "chk:ob:SF1@candidate: an allowed change names its allowed_by",
        ),
        (
            "an allowed change by an exemption that waives nothing",
            with_first(&|i| {
                i["disposition"] = json!("allowed_change");
                i["allowed_by"] = json!("SF3");
            }),
            AllowedBy,
            "chk:ob:SF1@candidate: SF3 waives nothing and is never an allowed_by; if SF1 still \
             holds there, the item is `held`",
        ),
        (
            "an allowed change by no exemption",
            with_first(&|i| {
                i["disposition"] = json!("allowed_change");
                i["allowed_by"] = json!("XX1");
            }),
            AllowedBy,
            "chk:ob:SF1@candidate: allowed_by \"XX1\" is not an enabled exemption",
        ),
        (
            "allowed_by on a held item",
            with_first(&|i| i["allowed_by"] = json!("SF3")),
            AllowedBy,
            "chk:ob:SF1@candidate: allowed_by is only valid with disposition allowed_change",
        ),
        (
            "an item left out",
            json!({"items": [answer("chk:ob:SF1@candidate")], "findings": []}),
            MissingItem,
            "chk:ob:SF2@candidate has no disposition",
        ),
        (
            "an item twice",
            {
                let mut s = with_first(&|_| {});
                s["items"]
                    .as_array_mut()
                    .unwrap()
                    .push(answer("chk:ob:SF1@candidate"));
                s
            },
            DuplicateItem,
            "chk:ob:SF1@candidate has more than one disposition",
        ),
        (
            "a finding with an empty reason",
            with_finding(&|f| f["reason"] = json!("")),
            BadField,
            "finding 0: the reason is empty",
        ),
        (
            "a finding that cites nothing",
            with_finding(&|f| f["obligation_ids"] = json!([])),
            FindingCites,
            "finding 0: cites no obligation: obligation_ids lists the rules it bears on, of the \
             obligations SF1, SF2, or for an observed finding the observe rules SF4",
        ),
        (
            "a finding with no location",
            with_finding(&|f| f["location"] = Json::Null),
            Location,
            "finding 0: has no location",
        ),
        (
            "a finding on an aligned picture",
            with_finding(&|f| f["location"]["frame"] = json!("aligned:candidate")),
            Location,
            "finding 0: the location's frame is not one of raw:candidate, raw:source: raw: and \
             the id of an image of the job, never the frame of a view or of an aligned picture",
        ),
        (
            "a finding on an image its rule does not judge",
            with_finding(&|f| f["location"]["frame"] = json!("raw:source")),
            FindingCites,
            "finding 0: SF1 does not judge image \"source\" where the finding is located",
        ),
        (
            "a finding that cites an exemption",
            with_finding(&|f| f["obligation_ids"] = json!(["SF3"])),
            FindingCites,
            "finding 0: SF3 is an exemption; cite the obligation the change bears on; SF3 waives \
             nothing, so it is never an allowed_by",
        ),
        (
            "a finding that cites no rule of the list",
            with_finding(&|f| f["obligation_ids"] = json!(["ZZ9"])),
            FindingCites,
            "finding 0: its obligation_ids are not rules of the list",
        ),
        (
            "an observed finding on an obligation",
            with_finding(&|f| f["disposition"] = json!("observed")),
            FindingCites,
            "finding 0: observed findings cite observe rules only (SF4); a finding on an \
             obligation is a violation, cant_tell or allowed_change",
        ),
        (
            "a violation that cites only an observe rule",
            with_finding(&|f| f["obligation_ids"] = json!(["SF4"])),
            FindingCites,
            "finding 0: it must cite a mandatory obligation",
        ),
        (
            "an allowed change of a finding naming nothing",
            with_finding(&|f| f["disposition"] = json!("allowed_change")),
            AllowedBy,
            "finding 0: an allowed change names its allowed_by",
        ),
        (
            "a held finding",
            with_finding(&|f| f["disposition"] = json!("held")),
            BadField,
            "finding 0: the disposition is not one of violation, allowed_change, cant_tell, \
             observed",
        ),
        (
            "allowed_by on a violation of a finding",
            with_finding(&|f| f["allowed_by"] = json!("SF3")),
            AllowedBy,
            "finding 0: allowed_by is only valid with allowed_change",
        ),
        (
            "evidence that is not a list",
            with_first(&|i| i["evidence"] = json!("call:0")),
            Evidence,
            "chk:ob:SF1@candidate: evidence is a list of tool call ids",
        ),
        (
            "evidence of a call that did not complete",
            with_first(&|i| i["evidence"] = json!(["call:7"])),
            Evidence,
            "chk:ob:SF1@candidate: evidence cites a tool call that is not a completed call of \
             this run; cite only call:0, or none",
        ),
    ]
}

/// What the run makes of a submission: its citations held first, then its verdict, as
/// `check` holds them, with one call of the run completed as `call:0`.
fn judged(submitted: &Json) -> Result<use_case::Decided, use_case::Rejection> {
    let job = whizbang_squint::job::Job::shorthand("source-fidelity", true, 0, None);
    let resolved = use_case::resolve(&job).unwrap();
    let checklist = Checklist::build(&resolved, &job);
    cited_calls_held(submitted, &["call:0".to_owned()], &[])?;
    use_case::decided_of(submitted, &job, &resolved, &checklist)
}

#[test]
fn every_rejection_has_a_class() {
    let mut seen = Vec::new();
    for (what, submitted, class, _) in refusals() {
        let refused = judged(&submitted).expect_err(what);
        assert_eq!(refused.class, class, "{what}: {}", refused.message);
        seen.push(class);
    }
    // Every class but the two the loop gives (a call beside the submission, a turn
    // without one) is one of what `decided_of` and `cited_calls_held` give.
    for class in Class::ALL {
        let of_the_loop = matches!(class, Class::NotAlone | Class::NoSubmission);
        assert_eq!(seen.contains(&class), !of_the_loop, "{}", class.name());
    }
    // Each class is named in snake case, and the names are distinct.
    let names: Vec<&str> = Class::ALL.iter().map(|c| c.name()).collect();
    assert_eq!(
        names,
        [
            "not_a_list",
            "unknown_item",
            "missing_item",
            "duplicate_item",
            "bad_field",
            "location",
            "allowed_by",
            "finding_cites",
            "evidence",
            "not_alone",
            "no_submission"
        ]
    );
    // A whole submission is no rejection.
    assert!(judged(&with_first(&|_| {})).is_ok());
}

#[test]
fn the_judge_sees_the_same_messages_as_before() {
    for (what, submitted, _, said) in refusals() {
        let refused = judged(&submitted).expect_err(what);
        assert_eq!(refused.message, said, "{what}");
        assert_eq!(refused.to_string(), said, "{what}");
    }
}
