//! `run/3` (ADR 0013): the elements of `asked.settings`, `spend.requests` and `egress`,
//! the refusals of a capability request, `outcome.lost` and the record of a run that a
//! dead process left; and what reading does with a `run/2` record. Every document here
//! is held to the types and to the schema alike.

use serde_json::{Value as Json, json};
use whizbang_core::contract::{RUN_RECORD_SCHEMA, RecordError, RunRecord};
use whizbang_testkit::schema::validate;

/// The schema of `run/2`, as the tag `m1a` froze it.
const RUN_2_SCHEMA: &str = include_str!("../../../schemas/run-2.json");

fn schema() -> Json {
    serde_json::from_str(RUN_RECORD_SCHEMA).unwrap()
}

fn as_json(text: &str) -> Json {
    serde_json::from_str(text).unwrap()
}

fn written(document: &Json) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(document).unwrap();
    bytes.push(b'\n');
    bytes
}

const EMPTY: &str = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// The first fixed vector: an ordinary root, with one of each new element.
pub fn ordinary() -> String {
    [
        r#"{"record":"run/3","#,
        r#""identity":{"run":"1000-1","request":"req-7","parent":null,"whizbang":"0.0.1"},"#,
        r#""asked":{"action":"heal","version":"1.0.0","#,
        r#""arguments":[{"name":"text","value":{"text":"hello"}}],"#,
        r#""settings":[{"name":"backends.fill.model","value":{"text":"bria-erase"}},"#,
        r#"{"name":"backends.fill.steps","value":{"integer":20}}],"#,
        r#""config":""#,
        EMPTY,
        r#""},"inputs":[],"outputs":[],"#,
        r#""outcome":{"status":"completed","error":null,"invalid":null,"#,
        r#""refusals":[{"action":null,"kind":"request_not_approved","#,
        r#""why":"the recipient is not approved for source images"}],"#,
        r#""refusals_omitted":0,"lost":[]},"#,
        r#""time":{"start_ms":1000,"end_ms":1003,"#,
        r#""durations":[{"of":"request","index":0,"start_ms":1001,"ms":1}]},"#,
        r#""models":[{"capability":"fill","backend":"fake","model":"m","version":"1","#,
        r#""device":null,"cached":null,"ended":"answered"}],"#,
        r#""spend":{"reserved_micros":500,"requests":[{"request":0,"reserved_micros":500,"#,
        r#""settled_micros":120,"unresolved_micros":0,"stage":"answered","over_bound":false}]},"#,
        r#""egress":[{"origin":"https://api.example.com","account":"acct...","#,
        r#""classes":["source_images","prompts"],"outcome":"sent"},"#,
        r#"{"origin":"https://eu.example.net:8443","account":null,"#,
        r#""classes":["derived_images"],"outcome":"refused"}],"#,
        r#""steps":[],"judge":[],"diagnostics":[]}"#,
        "\n",
    ]
    .concat()
}

const LOST: &str = r#"["arguments","settings","inputs","refusals","durations","models","requests","egress","steps","judge","diagnostics","version""#;

/// The second fixed vector: a root that a dead process left, made from its admission.
pub fn interrupted() -> String {
    [
        r#"{"record":"run/3","#,
        r#""identity":{"run":"1000-1","request":"req-7","parent":null,"whizbang":"0.0.1"},"#,
        r#""asked":{"action":"heal","version":"1.0.0","arguments":[],"settings":[],"config":null},"#,
        r#""inputs":[],"outputs":[],"#,
        r#""outcome":{"status":"error","error":{"kind":"interrupted","#,
        r#""message":"the process that ran it ended before the run did"},"invalid":null,"#,
        r#""refusals":[],"refusals_omitted":0,"lost":"#,
        LOST,
        r#"]},"time":{"start_ms":1000,"end_ms":5000,"durations":[]},"#,
        r#""models":[],"spend":{"reserved_micros":250000,"requests":[]},"egress":[],"steps":[],"#,
        r#""judge":[],"diagnostics":[]}"#,
        "\n",
    ]
    .concat()
}

/// The third fixed vector: a run whose admission could not be read, made from its name.
pub fn unread() -> String {
    [
        r#"{"record":"run/3","#,
        r#""identity":{"run":"1000-1","request":null,"parent":null,"whizbang":"0.0.1"},"#,
        r#""asked":{"action":null,"version":null,"arguments":[],"settings":[],"config":null},"#,
        r#""inputs":[],"outputs":[],"#,
        r#""outcome":{"status":"error","error":{"kind":"interrupted","#,
        r#""message":"the process that ran it ended before the run did"},"invalid":null,"#,
        r#""refusals":[],"refusals_omitted":0,"lost":"#,
        LOST,
        r#","admission"]},"time":{"start_ms":null,"end_ms":5000,"durations":[]},"#,
        r#""models":[],"spend":{"reserved_micros":null,"requests":[]},"egress":[],"steps":[],"#,
        r#""judge":[],"diagnostics":[]}"#,
        "\n",
    ]
    .concat()
}

/// A `run/2` record as the tag `m1a` wrote one for a completed `echo`.
fn of_the_tag() -> String {
    [
        r#"{"record":"run/2","#,
        r#""identity":{"run":"1000-1","request":null,"parent":null,"whizbang":"0.0.1"},"#,
        r#""asked":{"action":"echo","version":"1.0.0","#,
        r#""arguments":[{"name":"text","value":{"text":"hello"}}],"settings":[],"#,
        r#""config":""#,
        EMPTY,
        r#""},"inputs":[],"outputs":[],"#,
        r#""outcome":{"status":"completed","error":null,"invalid":null,"refusals":[],"refusals_omitted":0},"#,
        r#""time":{"start_ms":1000,"end_ms":1003,"durations":[]},"#,
        r#""models":[],"spend":{"reserved_micros":null,"requests":[]},"egress":[],"steps":[],"#,
        r#""judge":[],"diagnostics":[]}"#,
        "\n",
    ]
    .concat()
}

/// One change to a document, with what it is, in words.
type Change<'a> = (&'a str, &'a dyn Fn(&mut Json));

/// A change to one of the vectors is refused by the types and by the schema.
fn both(vector: &str, what: &str, change: &dyn Fn(&mut Json)) {
    let mut document = as_json(vector);
    change(&mut document);
    // Refused for what was changed, and not for how the test wrote it: a document
    // that is put together here has its fields in another order than a record's.
    let by_types = RunRecord::from_bytes(&written(&document));
    assert!(
        matches!(&by_types, Err(refusal) if *refusal != RecordError::NotCanonical),
        "{what}: the types read it: {by_types:?}"
    );
    assert!(
        validate(&schema(), &document).is_err(),
        "{what}: the schema takes it"
    );
}

/// A change to one of the vectors that both still take.
fn kept(vector: &str, what: &str, change: &dyn Fn(&mut Json)) {
    let mut document = as_json(vector);
    change(&mut document);
    // The types look at a record's shape and at what it says before they look at
    // how it is written: a document put together here, whose fields are in another
    // order than a record's, is taken when it is refused for that order alone.
    let read = RunRecord::from_bytes(&written(&document));
    assert_eq!(read, Err(RecordError::NotCanonical), "{what}");
    validate(&schema(), &document).unwrap_or_else(|why| panic!("{what}: {why}"));
}

#[test]
fn the_three_fixed_vectors_are_read_written_back_and_valid() {
    for (name, vector) in [
        ("ordinary", ordinary()),
        ("interrupted", interrupted()),
        ("unread", unread()),
    ] {
        let record =
            RunRecord::from_bytes(vector.as_bytes()).unwrap_or_else(|why| panic!("{name}: {why}"));
        assert_eq!(record.to_bytes(), vector.as_bytes(), "{name}");
        validate(&schema(), &as_json(&vector)).unwrap_or_else(|why| panic!("{name}: {why}"));
    }
}

#[test]
fn a_run_2_record_written_by_the_tag_is_read_and_written_back_as_it_was() {
    let bytes = of_the_tag();
    let record = RunRecord::from_bytes(bytes.as_bytes()).unwrap();
    assert_eq!(record.to_bytes(), bytes.as_bytes());
    // It is held to its own schema, which is as the tag froze it, and not to this one.
    let frozen: Json = serde_json::from_str(RUN_2_SCHEMA).unwrap();
    validate(&frozen, &as_json(&bytes)).unwrap();
    assert!(validate(&schema(), &as_json(&bytes)).is_err());
}

#[test]
fn a_run_2_record_holds_nothing_that_run_3_added() {
    let frozen: Json = serde_json::from_str(RUN_2_SCHEMA).unwrap();
    let ordinary = as_json(&ordinary());
    let added: [Change<'_>; 6] = [
        ("lost", &|d| d["outcome"]["lost"] = json!([])),
        ("a setting", &|d| {
            d["asked"]["settings"] = json!([{"name": "a", "value": {"integer": 1}}])
        }),
        ("a recipient", &|d| d["egress"] = ordinary["egress"].clone()),
        ("a paid request", &|d| {
            d["models"] = ordinary["models"].clone();
            d["time"]["durations"] = ordinary["time"]["durations"].clone();
            d["spend"]["requests"] = ordinary["spend"]["requests"].clone();
        }),
        ("a refusal of a request", &|d| {
            d["outcome"]["refusals"] = ordinary["outcome"]["refusals"].clone()
        }),
        ("a start that is not known", &|d| {
            d["time"]["start_ms"] = json!(null)
        }),
    ];
    for (what, change) in added {
        let mut document = as_json(&of_the_tag());
        change(&mut document);
        let read = RunRecord::from_bytes(&written(&document));
        assert!(
            matches!(&read, Err(refusal) if *refusal != RecordError::NotCanonical),
            "{what}: {read:?}"
        );
        assert!(validate(&frozen, &document).is_err(), "{what}");
    }
}

#[test]
fn a_record_of_a_version_this_build_does_not_read_is_another_version() {
    for version in ["run/1", "run/4", "run/30"] {
        let bytes = ordinary().replace("run/3", version);
        let refused = RunRecord::from_bytes(bytes.as_bytes()).unwrap_err();
        assert_eq!(
            refused,
            RecordError::OtherVersion {
                found: version.to_owned()
            }
        );
    }
}

#[test]
fn lost_with_anything_in_it_is_refused_unless_the_run_is_an_interrupted_error() {
    let lost = as_json(&interrupted())["outcome"]["lost"].clone();
    // On a run that ended by itself, of each status.
    both(&ordinary(), "lost on a completed run", &|d| {
        d["outcome"]["lost"] = lost.clone()
    });
    both(&ordinary(), "one group lost on a completed run", &|d| {
        d["outcome"]["lost"] = json!(["egress"]);
        d["egress"] = json!([]);
    });
    for status in ["completed", "cancelled", "invalid"] {
        both(&interrupted(), status, &|d| {
            d["outcome"]["status"] = json!(status);
            d["outcome"]["error"] = json!(null);
            if status == "invalid" {
                d["outcome"]["invalid"] = json!("why");
            }
        });
    }
    // On an error of another kind.
    both(&interrupted(), "an error of another kind", &|d| {
        d["outcome"]["error"]["kind"] = json!("panic")
    });
    // And an interrupted error that says nothing is lost says what no record can: the
    // lists of this one are empty and its config is null.
    both(&interrupted(), "interrupted, nothing lost", &|d| {
        d["outcome"]["lost"] = json!([])
    });
    // An ordinary error may be of the kind `interrupted`, when an action says so of
    // itself: nothing of it is lost.
    kept(&ordinary(), "an action's own error of that kind", &|d| {
        d["outcome"]["status"] = json!("error");
        d["outcome"]["error"] = json!({"kind": "interrupted", "message": "by the user"});
    });
}

#[test]
fn what_is_lost_is_all_of_it_in_one_order_with_the_admission_or_without() {
    let groups = as_json(&unread())["outcome"]["lost"].clone();
    let groups = groups.as_array().unwrap();
    assert_eq!(groups.len(), 13);
    // Each group left out in turn; two groups the other way round; one named twice; a
    // group that is none.
    for (at, group) in groups.iter().enumerate().take(groups.len() - 1) {
        both(&interrupted(), &format!("without {group}"), &|d| {
            d["outcome"]["lost"].as_array_mut().unwrap().remove(at);
        });
    }
    both(&interrupted(), "another order", &|d| {
        d["outcome"]["lost"].as_array_mut().unwrap().swap(0, 1)
    });
    both(&interrupted(), "a group twice", &|d| {
        d["outcome"]["lost"]
            .as_array_mut()
            .unwrap()
            .push(json!("egress"))
    });
    both(&interrupted(), "a group that is none", &|d| {
        d["outcome"]["lost"].as_array_mut().unwrap()[0] = json!("outputs")
    });
    both(&unread(), "the admission first", &|d| {
        d["outcome"]["lost"].as_array_mut().unwrap().rotate_right(1)
    });
}

#[test]
fn a_field_that_is_null_only_because_its_group_was_lost_is_refused_where_lost_does_not_name_it() {
    // On a run that ended by itself, nothing is lost, and none of these is null.
    for (group, field) in [
        ("asked", "action"),
        ("asked", "version"),
        ("asked", "config"),
        ("time", "start_ms"),
    ] {
        both(&ordinary(), &format!("no {field}, nothing lost"), &|d| {
            d[group][field] = json!(null)
        });
    }
    // On an interrupted run whose admission was read, `admission` is not lost: what
    // the admission gives is there.
    for (group, field) in [
        ("asked", "action"),
        ("asked", "version"),
        ("time", "start_ms"),
    ] {
        both(
            &interrupted(),
            &format!("no {field}, the admission read"),
            &|d| d[group][field] = json!(null),
        );
    }
    // A null that says something is known to be absent is as before: a root has no
    // parent, a request may have no id, and an executor with no ledger reserved nothing.
    kept(&ordinary(), "no request id", &|d| {
        d["identity"]["request"] = json!(null)
    });
    kept(&interrupted(), "no request id", &|d| {
        d["identity"]["request"] = json!(null)
    });
    kept(&interrupted(), "nothing reserved", &|d| {
        d["spend"]["reserved_micros"] = json!(null)
    });
    kept(&interrupted(), "a parent", &|d| {
        d["identity"]["parent"] = json!("999-1")
    });
}

#[test]
fn a_field_whose_group_is_lost_holds_nothing() {
    let ordinary = as_json(&ordinary());
    // What an interrupted record cannot know is empty or null in it.
    let filled: [Change<'_>; 9] = [
        ("arguments", &|d| {
            d["asked"]["arguments"] = ordinary["asked"]["arguments"].clone()
        }),
        ("settings", &|d| {
            d["asked"]["settings"] = ordinary["asked"]["settings"].clone()
        }),
        ("the config", &|d| d["asked"]["config"] = json!(EMPTY)),
        ("refusals", &|d| {
            d["outcome"]["refusals"] = ordinary["outcome"]["refusals"].clone()
        }),
        ("refusals left out", &|d| {
            d["outcome"]["refusals_omitted"] = json!(1)
        }),
        ("egress", &|d| d["egress"] = ordinary["egress"].clone()),
        ("diagnostics", &|d| {
            d["diagnostics"] = json!([{"name": "journal", "digest": EMPTY, "bytes": 0}])
        }),
        (
            "steps",
            &|d| {
                d["steps"] =
                    json!([{"step": "call", "run": "1001-1", "action": "echo", "status": null}])
            },
        ),
        ("a request and what it cost", &|d| {
            d["models"] = ordinary["models"].clone();
            d["time"]["durations"] = ordinary["time"]["durations"].clone();
            d["spend"]["requests"] = ordinary["spend"]["requests"].clone();
        }),
    ];
    for (what, change) in filled {
        both(&interrupted(), what, change);
        both(&unread(), what, change);
    }
    // And with the admission lost, what the admission would give is null.
    let from_the_admission: [Change<'_>; 6] = [
        ("an action", &|d| d["asked"]["action"] = json!("heal")),
        ("a version", &|d| d["asked"]["version"] = json!("1.0.0")),
        ("a start", &|d| d["time"]["start_ms"] = json!(1000)),
        ("a reservation", &|d| {
            d["spend"]["reserved_micros"] = json!(5)
        }),
        ("a request id", &|d| {
            d["identity"]["request"] = json!("req-7")
        }),
        ("a parent", &|d| d["identity"]["parent"] = json!("999-1")),
    ];
    for (what, change) in from_the_admission {
        both(&unread(), what, change);
    }
}

#[test]
fn a_setting_is_a_name_and_a_value_in_the_order_of_the_names() {
    kept(&ordinary(), "no settings", &|d| {
        d["asked"]["settings"] = json!([])
    });
    kept(&ordinary(), "each kind of value", &|d| {
        d["asked"]["settings"] = json!([
            {"name": "a", "value": {"boolean": true}},
            {"name": "b", "value": {"number": 0.5}},
            {"name": "c", "value": {"text": ""}},
        ])
    });
    // The order, and a name once, are what the schema cannot say: the types refuse.
    let refused = |what: &str, change: &dyn Fn(&mut Json)| {
        let mut document = as_json(&ordinary());
        change(&mut document);
        let read = RunRecord::from_bytes(&written(&document));
        assert!(
            matches!(read, Err(RecordError::Inconsistent(_))),
            "{what}: {read:?}"
        );
    };
    refused("another order", &|d| {
        d["asked"]["settings"].as_array_mut().unwrap().swap(0, 1)
    });
    refused("a name twice", &|d| {
        let first = d["asked"]["settings"][0].clone();
        d["asked"]["settings"]
            .as_array_mut()
            .unwrap()
            .insert(0, first);
    });
    both(&ordinary(), "no name", &|d| {
        d["asked"]["settings"][0]["name"] = json!("")
    });
    both(&ordinary(), "a long name", &|d| {
        d["asked"]["settings"][0]["name"] = json!("a".repeat(129))
    });
    both(&ordinary(), "a value of no kind", &|d| {
        d["asked"]["settings"][0]["value"] = json!({})
    });
    both(&ordinary(), "an unknown field", &|d| {
        d["asked"]["settings"][0]["x"] = json!(1)
    });
    both(&ordinary(), "more than a record lists", &|d| {
        let many: Vec<Json> = (0..97)
            .map(|at| json!({"name": format!("s{at:03}"), "value": {"integer": at}}))
            .collect();
        d["asked"]["settings"] = json!(many);
    });
    kept(&ordinary(), "as many as a record lists", &|d| {
        let many: Vec<Json> = (0..96)
            .map(|at| json!({"name": format!("s{at:03}"), "value": {"integer": at}}))
            .collect();
        d["asked"]["settings"] = json!(many);
    });
}

#[test]
fn a_paid_request_is_one_the_run_sent_with_what_it_held_and_what_it_cost() {
    let paid = |d: &mut Json, field: &str, value: Json| d["spend"]["requests"][0][field] = value;
    // Under its bound, at it, over it, not sent, and unresolved at its bound.
    kept(&ordinary(), "at its bound", &|d| {
        paid(d, "settled_micros", json!(500))
    });
    kept(&ordinary(), "over its bound", &|d| {
        paid(d, "settled_micros", json!(900));
        paid(d, "over_bound", json!(true));
    });
    kept(&ordinary(), "not sent", &|d| {
        paid(d, "settled_micros", json!(0));
        paid(d, "stage", json!("not_sent"));
    });
    kept(&ordinary(), "unresolved", &|d| {
        paid(d, "settled_micros", json!(500));
        paid(d, "unresolved_micros", json!(500));
        paid(d, "stage", json!("unresolved"));
    });
    kept(&ordinary(), "no paid request", &|d| {
        d["spend"]["requests"] = json!([])
    });
    both(&ordinary(), "a stage that is none", &|d| {
        paid(d, "stage", json!("sent"))
    });
    both(&ordinary(), "an amount below nothing", &|d| {
        paid(d, "settled_micros", json!(-1))
    });
    both(&ordinary(), "a field left out", &|d| {
        d["spend"]["requests"][0]
            .as_object_mut()
            .unwrap()
            .remove("over_bound");
    });
    both(&ordinary(), "an unknown field", &|d| {
        paid(d, "currency", json!("usd"))
    });
    both(&ordinary(), "a request past the last", &|d| {
        paid(d, "request", json!(1024))
    });
}

#[test]
fn what_the_schema_cannot_say_of_a_paid_request_the_types_refuse() {
    let refused = |what: &str, change: &dyn Fn(&mut Json)| {
        let mut document = as_json(&ordinary());
        change(&mut document);
        let read = RunRecord::from_bytes(&written(&document));
        assert!(
            matches!(read, Err(RecordError::Inconsistent(_))),
            "{what}: {read:?}"
        );
    };
    let paid = |d: &mut Json, field: &str, value: Json| d["spend"]["requests"][0][field] = value;
    refused("a request the run did not send", &|d| {
        paid(d, "request", json!(1))
    });
    refused("over its bound and not said", &|d| {
        paid(d, "settled_micros", json!(501))
    });
    refused("said over its bound and not", &|d| {
        paid(d, "over_bound", json!(true))
    });
    refused("not sent, and charged", &|d| {
        paid(d, "stage", json!("not_sent"))
    });
    refused("unresolved, and nothing unresolved", &|d| {
        paid(d, "stage", json!("unresolved"))
    });
    refused("unresolved, and only a part of it", &|d| {
        paid(d, "stage", json!("unresolved"));
        paid(d, "settled_micros", json!(500));
        paid(d, "unresolved_micros", json!(300));
    });
    refused("unresolved, and settled under what was held", &|d| {
        paid(d, "stage", json!("unresolved"));
        paid(d, "settled_micros", json!(300));
        paid(d, "unresolved_micros", json!(300));
    });
    refused("answered, and something unresolved", &|d| {
        paid(d, "unresolved_micros", json!(1))
    });
    refused("the largest amount, which is none", &|d| {
        paid(d, "reserved_micros", json!(u64::MAX))
    });
    refused("one request twice", &|d| {
        let first = d["spend"]["requests"][0].clone();
        d["spend"]["requests"].as_array_mut().unwrap().push(first);
    });
}

#[test]
fn where_data_went_is_an_origin_a_cut_account_the_classes_and_whether_it_was_sent() {
    let went = |d: &mut Json, field: &str, value: Json| d["egress"][0][field] = value;
    kept(&ordinary(), "nowhere", &|d| d["egress"] = json!([]));
    kept(&ordinary(), "a short account", &|d| {
        went(d, "account", json!("ab"))
    });
    kept(&ordinary(), "every class", &|d| {
        went(
            d,
            "classes",
            json!(["source_images", "derived_images", "prompts", "evidence"]),
        )
    });
    kept(&ordinary(), "no class: an address alone", &|d| {
        went(d, "classes", json!([]))
    });
    both(&ordinary(), "a class that is none", &|d| {
        went(d, "classes", json!(["pixels"]))
    });
    both(&ordinary(), "an outcome that is none", &|d| {
        went(d, "outcome", json!("lost"))
    });
    both(&ordinary(), "an account that was not cut", &|d| {
        went(d, "account", json!("acct_4f8a1c9e77"))
    });
    both(&ordinary(), "no origin", &|d| went(d, "origin", json!("")));
    both(&ordinary(), "an unknown field", &|d| {
        went(d, "path", json!("/v1"))
    });
    both(&ordinary(), "a field left out", &|d| {
        d["egress"][0].as_object_mut().unwrap().remove("account");
    });
    both(&ordinary(), "more than a record lists", &|d| {
        let many: Vec<Json> = (0..129)
            .map(|at| {
                json!({"origin": format!("https://h{at}.example.com"), "account": null,
                       "classes": [], "outcome": "sent"})
            })
            .collect();
        d["egress"] = json!(many);
    });
}

#[test]
fn what_the_schema_cannot_say_of_where_data_went_the_types_refuse() {
    let refused = |what: &str, change: &dyn Fn(&mut Json)| {
        let mut document = as_json(&ordinary());
        change(&mut document);
        let read = RunRecord::from_bytes(&written(&document));
        assert!(
            matches!(read, Err(RecordError::Inconsistent(_))),
            "{what}: {read:?}"
        );
    };
    let went = |d: &mut Json, field: &str, value: Json| d["egress"][0][field] = value;
    refused("an address that is not an origin", &|d| {
        went(d, "origin", json!("https://api.example.com/v1"))
    });
    refused("an origin not written the one way", &|d| {
        went(d, "origin", json!("https://API.example.com:443"))
    });
    refused("a class twice", &|d| {
        went(d, "classes", json!(["prompts", "prompts"]))
    });
    refused("classes in another order", &|d| {
        went(d, "classes", json!(["prompts", "source_images"]))
    });
    refused("a cut that cuts nothing", &|d| {
        went(d, "account", json!("ab..."))
    });
    refused("an account with a space", &|d| {
        went(d, "account", json!("a b"))
    });
    refused("one recipient and outcome twice", &|d| {
        let first = d["egress"][0].clone();
        d["egress"].as_array_mut().unwrap().push(first);
    });
}

#[test]
fn a_refusal_of_a_capability_request_names_no_action() {
    for kind in [
        "request_no_bound",
        "request_over_allowance",
        "request_not_approved",
        "request_not_ready",
    ] {
        kept(&ordinary(), kind, &|d| {
            d["outcome"]["refusals"][0]["kind"] = json!(kind)
        });
        let mut document = as_json(&ordinary());
        document["outcome"]["refusals"][0]["kind"] = json!(kind);
        document["outcome"]["refusals"][0]["action"] = json!("heal");
        let read = RunRecord::from_bytes(&written(&document));
        assert!(
            matches!(read, Err(RecordError::Inconsistent(_))),
            "{kind}: {read:?}"
        );
    }
    both(&ordinary(), "a kind that is none", &|d| {
        d["outcome"]["refusals"][0]["kind"] = json!("request_refused")
    });
}

#[test]
fn reading_looks_at_the_shape_and_the_meaning_before_how_a_record_is_written() {
    // The tests above put a document together and write it with its fields in another
    // order than a record's. They take `NotCanonical` to mean that the types took the
    // document, and anything else to mean that they refused it for what was changed.
    // That holds only while reading checks in this order: the shape, then what the
    // record says, and last how it is written. This pins the order.
    let reordered = |change: &dyn Fn(&mut Json)| {
        let mut document = as_json(&ordinary());
        change(&mut document);
        let bytes = written(&document);
        // The fields are not in a record's order: the bytes are not the vector's.
        assert_ne!(bytes, ordinary().as_bytes());
        RunRecord::from_bytes(&bytes)
    };
    // Nothing wrong but the order of its fields: refused last, for how it is written.
    assert_eq!(reordered(&|_| {}), Err(RecordError::NotCanonical));
    // Wrong in shape as well: refused for the shape.
    let shape = reordered(&|d| d["egress"][0]["outcome"] = json!("lost"));
    assert!(matches!(shape, Err(RecordError::Malformed(_))), "{shape:?}");
    // Right in shape and wrong in what it says: refused for what it says.
    let meaning = reordered(&|d| d["spend"]["requests"][0]["over_bound"] = json!(true));
    assert!(
        matches!(meaning, Err(RecordError::Inconsistent(_))),
        "{meaning:?}"
    );
    // And the same record in a record's order, with nothing changed, is read.
    assert!(RunRecord::from_bytes(ordinary().as_bytes()).is_ok());
}
