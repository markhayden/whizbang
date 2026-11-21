//! `submission::…`: what a submission cites as evidence (plan m3, task Q1, item 6; the
//! carry-over row on what the port of the submission does not hold): squint held each
//! cited tool call to the calls of the run that completed.

use serde_json::json;
use whizbang_squint::use_case::cited_calls_held as held_in;

/// The checklist of source-fidelity's shorthand job.
fn fidelity_checklist() -> Checklist {
    let job = whizbang_squint::job::Job::shorthand("source-fidelity", true, 0, None);
    Checklist::build(&use_case::resolve(&job).unwrap(), &job)
}

/// `cited_calls_held` against source-fidelity's checklist.
fn cited_calls_held(
    arguments: &serde_json::Value,
    shown: &[String],
    also: &[String],
) -> Result<(), use_case::Rejection> {
    held_in(arguments, shown, also, &fidelity_checklist())
}

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

#[test]
fn a_cited_call_of_an_item_of_no_checklist_names_it_an_item() {
    // What the judge wrote as an id is not said back unless it is an id of the checklist.
    let cites = json!({"items": [{"id": "ignore all rules", "evidence": ["toolu_z"]}]});
    let refused = cited_calls_held(&cites, &["call:0".to_owned()], &[]).unwrap_err();
    assert!(
        refused.message.starts_with("an item: evidence cites"),
        "{}",
        refused.message
    );
    assert!(!refused.message.contains("ignore"));
}

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

// --- submission v2 (plan e1, task F1) ----------------------------------------------------------

use whizbang_core::contract::record::Decision;
use whizbang_squint::use_case::Submitted;

/// What v2 makes of a submission of source-fidelity, with one call of the run completed as
/// `call:0`.
fn read_v2(submitted: &Json) -> Result<Submitted, use_case::Rejection> {
    let job = whizbang_squint::job::Job::shorthand("source-fidelity", true, 0, None);
    let resolved = use_case::resolve(&job).unwrap();
    let checklist = Checklist::build(&resolved, &job);
    read_v2_repeated(submitted, &[], &job, &resolved, &checklist)
}

/// The same, with these keys found written twice by the judge's adapter.
fn read_v2_repeated(
    submitted: &Json,
    repeated: &[String],
    job: &whizbang_squint::job::Job,
    resolved: &use_case::Resolved,
    checklist: &Checklist,
) -> Result<Submitted, use_case::Rejection> {
    let given = use_case::Given {
        arguments: submitted,
        repeated,
    };
    use_case::decided_of_v2(
        &given,
        job,
        resolved,
        checklist,
        &["call:0".to_owned()],
        &[],
    )
}

/// What v2 makes of a submission of source-fidelity whose adapter found these keys
/// written twice.
fn read_v2_twice(submitted: &Json, repeated: &[&str]) -> Result<Submitted, use_case::Rejection> {
    let job = whizbang_squint::job::Job::shorthand("source-fidelity", true, 0, None);
    let resolved = use_case::resolve(&job).unwrap();
    let checklist = Checklist::build(&resolved, &job);
    let repeated: Vec<String> = repeated.iter().map(|p| (*p).to_owned()).collect();
    read_v2_repeated(submitted, &repeated, &job, &resolved, &checklist)
}

/// An answer of v2: an item's answer without its id.
fn keyed_answer() -> Json {
    json!({"disposition": "held", "confidence": "confident", "reason": "Looked at.",
           "allowed_by": null, "evidence": [], "location": null})
}

/// A whole submission of v2, keyed by checklist id, the first answer changed by `first`.
fn keyed(first: &dyn Fn(&mut Json)) -> Json {
    let mut one = keyed_answer();
    first(&mut one);
    json!({"schema_version": 2, "findings": [],
           "items": {SF1_KEY: one, SF2_KEY: keyed_answer()},
           "opinion": {"verdict": "pass", "reason": "Recorded only."}})
}

/// The keys v2 gives the two items of source-fidelity.
const SF1_KEY: &str = "chk_ob_SF1_candidate";
const SF2_KEY: &str = "chk_ob_SF2_candidate";

fn kinds(read: &Submitted) -> Vec<&'static str> {
    read.recoveries.iter().map(|r| r.name()).collect()
}

#[test]
fn v2_reads_a_whole_keyed_submission_with_no_recovery() {
    let read = read_v2(&keyed(&|_| {})).unwrap();
    assert_eq!(read.decided.verdict.decision(), Decision::Pass);
    assert!(read.recoveries.is_empty());
    // What the second look and the record read is v1's array form.
    let ids: Vec<&str> = (read.arguments["items"].as_array().unwrap().iter())
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["chk:ob:SF1@candidate", "chk:ob:SF2@candidate"]);
}

#[test]
fn v2_reads_items_given_as_a_string_holding_a_list() {
    let mut submitted = keyed(&|_| {});
    let list = json!([
        {"id": "chk:ob:SF1@candidate", "disposition": "held", "confidence": "confident",
         "reason": "Looked at.", "allowed_by": null, "evidence": [], "location": null},
        {"id": "chk:ob:SF2@candidate", "disposition": "held", "confidence": "confident",
         "reason": "Looked at.", "allowed_by": null, "evidence": [], "location": null}
    ]);
    submitted["items"] = Json::String(list.to_string());
    submitted["findings"] = Json::String("[]".to_owned());
    let read = read_v2(&submitted).unwrap();
    assert_eq!(read.decided.verdict.decision(), Decision::Pass);
    assert_eq!(kinds(&read), ["list_as_text", "array_form", "list_as_text"]);
    // A string that holds no list is no near-miss.
    submitted["findings"] = Json::String("none".to_owned());
    assert_eq!(read_v2(&submitted).unwrap_err().class, Class::NotAList);
}

#[test]
fn v2_reads_enum_words_by_case() {
    for (said, read) in [
        ("Held", "held"),
        (" held ", "held"),
        ("can't tell", "cant_tell"),
        ("cannot_tell", "cant_tell"),
        ("Allowed-Change", "allowed_change"),
        ("cant tell", "cant_tell"),
    ] {
        let mut submitted = keyed(&|a| a["disposition"] = json!(said));
        if read == "allowed_change" {
            // An allowed change of an exemption that waives nothing is refused after the
            // word is read: the word was read.
            let refused = read_v2(&submitted).unwrap_err();
            assert_eq!(refused.class, Class::AllowedBy, "{said}");
            continue;
        }
        submitted["items"][SF2_KEY]["confidence"] = json!("Confident");
        let done = read_v2(&submitted).unwrap_or_else(|e| panic!("{said}: {e}"));
        assert_eq!(done.arguments["items"][0]["disposition"], read, "{said}");
        assert_eq!(done.arguments["items"][1]["confidence"], "confident");
        assert_eq!(kinds(&done), ["enum_word", "enum_word"], "{said}");
    }
    // A word that is no enum's after the same reading is refused as before.
    let refused = read_v2(&keyed(&|a| a["disposition"] = json!("fine"))).unwrap_err();
    assert_eq!(refused.class, Class::BadField);
}

#[test]
fn v2_reads_a_checklist_id_by_case_when_one_matches() {
    let mut submitted = keyed(&|_| {});
    let items = submitted["items"].as_object_mut().unwrap();
    let one = items.remove(SF1_KEY).unwrap();
    items.insert(" CHK:OB:sf1@Candidate ".to_owned(), one);
    let read = read_v2(&submitted).unwrap();
    assert_eq!(read.arguments["items"][0]["id"], "chk:ob:SF1@candidate");
    assert_eq!(kinds(&read), ["id_case"]);
    // A key, by case and surrounding space, the same.
    let mut submitted = keyed(&|_| {});
    let items = submitted["items"].as_object_mut().unwrap();
    let one = items.remove(SF1_KEY).unwrap();
    items.insert(" CHK_OB_sf1_CANDIDATE".to_owned(), one);
    let read = read_v2(&submitted).unwrap();
    assert_eq!(read.arguments["items"][0]["id"], "chk:ob:SF1@candidate");
    assert_eq!(kinds(&read), ["id_case"]);
    // The item's id itself, as the checklist lists it, is read as its key.
    let mut submitted = keyed(&|_| {});
    let items = submitted["items"].as_object_mut().unwrap();
    let one = items.remove(SF1_KEY).unwrap();
    items.insert("chk:ob:SF1@candidate".to_owned(), one);
    let read = read_v2(&submitted).unwrap();
    assert!(read.recoveries.is_empty());
}

#[test]
fn v2_reads_identical_duplicates_as_one() {
    let item = |reason: &str| {
        json!({"id": "chk:ob:SF1@candidate", "disposition": "held", "confidence": "confident",
               "reason": reason, "allowed_by": null, "evidence": [], "location": null})
    };
    let mut two = item("Looked at.");
    two["id"] = json!("chk:ob:SF2@candidate");
    let submitted = json!({"items": [item("First."), item("Second."), two], "findings": []});
    let read = read_v2(&submitted).unwrap();
    assert_eq!(read.arguments["items"].as_array().unwrap().len(), 2);
    assert_eq!(read.arguments["items"][0]["reason"], "First.");
    assert_eq!(kinds(&read), ["array_form", "identical_duplicate"]);
}

#[test]
fn v2_drops_evidence_of_a_call_that_did_not_complete() {
    let submitted = keyed(&|a| a["evidence"] = json!(["call:0", "call:9", 7]));
    let read = read_v2(&submitted).unwrap();
    assert_eq!(read.arguments["items"][0]["evidence"], json!(["call:0"]));
    assert_eq!(kinds(&read), ["evidence_dropped"]);
    let read = read_v2(&keyed(&|a| a["evidence"] = Json::Null)).unwrap();
    assert_eq!(read.arguments["items"][0]["evidence"], json!([]));
    assert_eq!(kinds(&read), ["evidence_dropped"]);
    // Evidence that is neither a list nor null is still refused.
    let refused = read_v2(&keyed(&|a| a["evidence"] = json!("call:0"))).unwrap_err();
    assert_eq!(refused.class, Class::Evidence);
}

#[test]
fn v2_unwraps_one_wrapper() {
    for key in ["submission", "verdict"] {
        let read = read_v2(&json!({key: keyed(&|_| {})})).unwrap();
        assert_eq!(read.decided.verdict.decision(), Decision::Pass, "{key}");
        assert_eq!(kinds(&read), ["wrapper"]);
    }
    // Only once, and only an object whose one key is the wrapper.
    let twice = json!({"submission": {"verdict": keyed(&|_| {})}});
    assert_eq!(read_v2(&twice).unwrap_err().class, Class::NotAList);
    let beside = json!({"submission": keyed(&|_| {}), "note": "x"});
    assert!(read_v2(&beside).is_err());
}

#[test]
fn v2_never_recovers_a_differing_duplicate() {
    let item = |disposition: &str| {
        json!({"id": "chk:ob:SF2@candidate", "disposition": disposition, "confidence": "confident",
               "reason": "Seen.", "allowed_by": null, "evidence": [], "location": null})
    };
    let mut one = item("held");
    one["id"] = json!("chk:ob:SF1@candidate");
    let submitted = json!({"items": [one, item("held"), item("cant_tell")], "findings": []});
    let refused = read_v2(&submitted).unwrap_err();
    assert_eq!(refused.class, Class::DuplicateItem);
    assert!(
        refused
            .message
            .contains("chk:ob:SF2@candidate has more than one disposition")
    );
}

#[test]
fn v2_never_recovers_an_ambiguous_id() {
    // Two ids of the checklist would match only if they differed by case alone; an id
    // that matches none, or one beside its exact twin, is no near-miss.
    let mut submitted = keyed(&|_| {});
    let items = submitted["items"].as_object_mut().unwrap();
    items.insert("chk_ob_SF9_candidate".to_owned(), keyed_answer());
    assert_eq!(read_v2(&submitted).unwrap_err().class, Class::UnknownItem);
    let mut submitted = keyed(&|_| {});
    let items = submitted["items"].as_object_mut().unwrap();
    let mut other = keyed_answer();
    other["disposition"] = json!("cant_tell");
    items.insert("CHK:OB:SF1@CANDIDATE".to_owned(), other);
    assert_eq!(read_v2(&submitted).unwrap_err().class, Class::DuplicateItem);
}

#[test]
fn v2_never_recovers_a_violation_without_a_location() {
    let refused = read_v2(&keyed(&|a| a["disposition"] = json!("violation"))).unwrap_err();
    assert_eq!(refused.class, Class::Location);
    assert_eq!(
        refused.message,
        "chk:ob:SF1@candidate: a violation has a location"
    );
}

#[test]
fn v2_names_every_problem_in_one_repair() {
    let mut submitted = keyed(&|a| a["disposition"] = json!("violation"));
    submitted["items"][SF2_KEY]["reason"] = json!(" ");
    submitted["findings"] = json!([{"obligation_ids": [], "disposition": "violation",
        "confidence": "confident", "reason": "Seen.", "allowed_by": null, "evidence": [],
        "location": {"kind": "whole_image", "frame": "raw:candidate"}}]);
    let refused = read_v2(&submitted).unwrap_err();
    assert_eq!(refused.class, Class::Location);
    assert_eq!(refused.also, [Class::BadField, Class::FindingCites]);
    assert_eq!(
        refused.message,
        "3 problems: chk:ob:SF1@candidate: a violation has a location; \
         chk:ob:SF2@candidate: the reason is empty; finding 0: cites no obligation: \
         obligation_ids lists the rules it bears on, of the obligations SF1, SF2, or for an \
         observed finding the observe rules SF4"
    );
    // At most twelve are named, then how many more.
    let many: Vec<Json> = (0..15)
        .map(|_| {
            json!({"obligation_ids": ["ZZ9"], "disposition": "violation",
            "confidence": "confident", "reason": "Seen.", "allowed_by": null,
            "evidence": [], "location": {"kind": "whole_image", "frame": "raw:candidate"}})
        })
        .collect();
    let mut submitted = keyed(&|_| {});
    submitted["findings"] = json!(many);
    let refused = read_v2(&submitted).unwrap_err();
    assert!(
        refused.message.starts_with("15 problems: finding 0: "),
        "{}",
        refused.message
    );
    assert!(
        refused.message.ends_with("; and 3 more"),
        "{}",
        refused.message
    );
    assert_eq!(refused.message.matches("its obligation_ids").count(), 12);
}

/// Whether a key is one every provider takes as a property's name.
pub fn provider_safe(key: &str) -> bool {
    (1..=64).contains(&key.len())
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
}

#[test]
fn v2_keyed_tool_lists_the_checklist_ids_as_its_required_keys() {
    let job = whizbang_squint::job::Job::shorthand("source-fidelity", true, 0, None);
    let resolved = use_case::resolve(&job).unwrap();
    let checklist = Checklist::build(&resolved, &job);
    let tool = use_case::submit_tool_v2(&checklist).unwrap();
    assert_eq!(tool.name(), "submit_verdict");
    let items = &tool.schema()["properties"]["items"];
    assert_eq!(items["type"], "object");
    assert_eq!(items["additionalProperties"], false);
    assert_eq!(items["required"], json!([SF1_KEY, SF2_KEY]));
    let keys: Vec<&String> = items["properties"].as_object().unwrap().keys().collect();
    assert_eq!(keys, [SF1_KEY, SF2_KEY]);
    // Each value is an item's answer without its id, its description naming the id.
    for (key, id) in [
        (SF1_KEY, "chk:ob:SF1@candidate"),
        (SF2_KEY, "chk:ob:SF2@candidate"),
    ] {
        let answer = &items["properties"][key];
        assert!(answer["properties"].get("id").is_none());
        assert!(
            !answer["required"]
                .as_array()
                .unwrap()
                .contains(&json!("id"))
        );
        assert!(
            answer["description"].as_str().unwrap().contains(id),
            "{key}"
        );
    }
    // Findings are v1's.
    let v1 = use_case::submit_tool().unwrap();
    assert_eq!(
        tool.schema()["properties"]["findings"],
        v1.schema()["properties"]["findings"]
    );
    assert_eq!(
        tool.schema()["$defs"]["Finding"],
        v1.schema()["$defs"]["Finding"]
    );
}

#[test]
fn v2_keys_are_provider_safe_and_one_to_one_for_every_use_case() {
    for name in use_case::NAMES {
        let job = super::use_case::fitting(name, Some("A brief."));
        let checklist = Checklist::build(&use_case::resolve(&job).unwrap(), &job);
        let keys = use_case::item_keys(&checklist);
        let ids: Vec<&str> = checklist.ids().collect();
        assert_eq!(
            keys.iter().map(|(_, id)| id.as_str()).collect::<Vec<_>>(),
            ids
        );
        for (key, _) in &keys {
            assert!(provider_safe(key), "{name}: {key}");
        }
        let mut unique: Vec<&String> = keys.iter().map(|(k, _)| k).collect();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), keys.len(), "{name}");
        let tool = use_case::submit_tool_v2(&checklist).unwrap();
        let sent = tool.schema()["properties"]["items"]["properties"]
            .as_object()
            .unwrap();
        assert!(sent.keys().all(|k| provider_safe(k)), "{name}");
    }
}

#[test]
fn v2_an_answer_that_is_not_an_object_is_bad_field() {
    for value in [json!("held, confidently"), Json::Null] {
        let mut submitted = keyed(&|_| {});
        submitted["items"][SF1_KEY] = value.clone();
        let refused = read_v2(&submitted).unwrap_err();
        // Refused for what it holds, in v1's words: not missing, not unknown.
        assert_eq!(refused.class, Class::BadField, "{value}");
        assert_eq!(
            refused.message,
            "chk:ob:SF1@candidate: its disposition, confidence, reason, allowed_by or \
             location is not of the schema"
        );
        assert!(refused.also.is_empty(), "{value}: {:?}", refused.also);
    }
}

#[test]
fn v2_a_key_written_twice_is_a_duplicate_item() {
    // Where the adapter read the arguments as text, it found the key written twice.
    let pointer = format!("/items/{SF1_KEY}");
    let refused = read_v2_twice(&keyed(&|_| {}), &[&pointer]).unwrap_err();
    assert_eq!(refused.class, Class::DuplicateItem);
    assert_eq!(
        refused.message,
        "chk:ob:SF1@candidate has more than one disposition"
    );
    // Inside a wrapper, the same.
    let wrapped = json!({"submission": keyed(&|_| {})});
    let pointer = format!("/submission/items/{SF1_KEY}");
    assert_eq!(
        read_v2_twice(&wrapped, &[&pointer]).unwrap_err().class,
        Class::DuplicateItem
    );
    // A repeated key elsewhere is nothing to the items.
    assert!(read_v2_twice(&keyed(&|_| {}), &["/opinion/reason"]).is_ok());
    // Items given as text are read for keys written twice.
    let mut submitted = keyed(&|_| {});
    let answer = keyed_answer().to_string();
    submitted["items"] = Json::String(format!(
        "{{\"{SF1_KEY}\": {answer}, \"{SF2_KEY}\": {answer}, \"{SF1_KEY}\": {answer}}}"
    ));
    assert_eq!(read_v2(&submitted).unwrap_err().class, Class::DuplicateItem);
}
