//! `submission::…`: what a submission cites as evidence (plan m3, task Q1, item 6; the
//! carry-over row on what the port of the submission does not hold): squint held each
//! cited tool call to the calls of the run that completed.

use serde_json::json;
use whizbang_squint::use_case::cited_calls_held;

#[test]
fn a_cited_call_that_is_not_a_completed_call_of_the_run_is_refused() {
    let completed = vec!["toolu_a".to_owned(), "toolu_b".to_owned()];
    let citing = |item: serde_json::Value, finding: serde_json::Value| {
        json!({"items": [{"id": "chk:ob:SF1@candidate", "evidence": item}],
               "findings": [{"evidence": finding}]})
    };
    // Calls of the run that completed, or none at all: held.
    cited_calls_held(&citing(json!(["toolu_a"]), json!(["toolu_b"])), &completed).unwrap();
    cited_calls_held(&citing(json!([]), json!([])), &completed).unwrap();
    cited_calls_held(&json!({"items": [{"id": "x"}]}), &completed).unwrap();
    // A call the run never made, or one that did not complete: refused, naming where.
    let item = cited_calls_held(&citing(json!(["toolu_z"]), json!([])), &completed);
    assert!(item.unwrap_err().contains("chk:ob:SF1@candidate"));
    let finding = cited_calls_held(&citing(json!([]), json!(["toolu_z"])), &completed);
    assert!(finding.unwrap_err().contains("finding 1"));
    // An evidence list that is not a list of names is refused too.
    let wrong = cited_calls_held(&citing(json!("toolu_a"), json!([])), &completed);
    assert!(wrong.is_err());
}
