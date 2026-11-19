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
    assert!(item.unwrap_err().contains("chk:ob:SF1@candidate"));
    let finding = cited_calls_held(&citing(json!([]), json!(["toolu_z"])), &shown, &also);
    assert!(finding.unwrap_err().contains("finding 1"));
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
        refused.unwrap_err(),
        "chk:ob:SF1@candidate: evidence cites a tool call that is not a completed call of \
         this run; cite only call:0, call:2, or none"
    );
    let refused = cited_calls_held(&citing(json!(["view"]), json!([])), &[], &also);
    assert_eq!(
        refused.unwrap_err(),
        "chk:ob:SF1@candidate: evidence cites a tool call that is not a completed call of \
         this run; no call of this run completed, so evidence is an empty list"
    );
}
