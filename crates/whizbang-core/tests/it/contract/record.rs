//! The run record (ADR 0005): its shape, written one way, read strictly and held to its
//! schema; how one is made from a prepared call and an outcome, with the secrets taken
//! out; and how one is read from a store.

use std::sync::Arc;

use serde_json::{Value as Json, json};
use whizbang_core::artifact::{self, Artifact, ArtifactId, ArtifactType, ImageRole};
use whizbang_core::contract::record::{
    self, ArgumentValue, Diagnostic, Ran, RunRecord, Status, StoredRecordError,
};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Call, Committed, Engine, Executor, InputPort, Kind,
    NoBackends, Outcome, PreparedCall, RUN_RECORD, RUN_RECORD_SCHEMA, RecordError, Registry, RunId,
    Secrets, Value, ValueType, journal_bytes, read_journal,
};
use whizbang_core::contract::{DIAGNOSTIC_NAME_LIMIT, JournalLine, is_diagnostic_name};
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{self, Reader, Store};
use whizbang_testkit::echo::Echo;
use whizbang_testkit::schema::validate;
use whizbang_testkit::scratch::Scratch;

use super::redact::made_up_key;
use crate::artifact::support::{image_of, photo};

/// A record as the executor writes one for a completed `echo`, by hand.
pub fn completed() -> String {
    concat!(
        r#"{"record":"run/2","#,
        r#""identity":{"run":"1000-1","request":null,"parent":null,"whizbang":"0.0.1"},"#,
        r#""asked":{"action":"echo","version":"1.0.0","#,
        r#""arguments":[{"name":"text","value":{"text":"hello"}}],"settings":[],"#,
        r#""config":"sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"},"#,
        r#""inputs":[],"outputs":[],"#,
        r#""outcome":{"status":"completed","error":null,"invalid":null,"refusals":[]},"#,
        r#""time":{"start_ms":1000,"end_ms":1003,"durations":[]},"#,
        r#""models":[],"spend":{"reserved_micros":null,"requests":[]},"egress":[],"steps":[],"#,
        r#""judge":[],"#,
        r#""diagnostics":[{"name":"diagnostics/echo.txt","#,
        r#""digest":"sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824","#,
        r#""bytes":5},{"name":"journal","#,
        r#""digest":"sha256:4e5c3fcb6d7895e1a136823744faf1f979561eabaaf96e46b6ba04c04f511336","#,
        r#""bytes":93}]}"#,
        "\n"
    )
    .to_owned()
}

pub fn schema() -> Json {
    serde_json::from_str(RUN_RECORD_SCHEMA).unwrap()
}

fn as_json(text: &str) -> Json {
    serde_json::from_str(text).unwrap()
}

/// The record with one change made to its JSON, written back on one line.
fn changed(change: impl FnOnce(&mut Json)) -> Vec<u8> {
    let mut document = as_json(&completed());
    change(&mut document);
    let mut bytes = serde_json::to_vec(&document).unwrap();
    bytes.push(b'\n');
    bytes
}

#[test]
fn a_record_is_read_and_written_back_as_the_same_bytes() {
    let bytes = completed();
    let record = RunRecord::from_bytes(bytes.as_bytes()).unwrap();
    assert_eq!(record.to_bytes(), bytes.as_bytes());
    assert_eq!(RUN_RECORD, "run/2");
    assert_eq!(record.spend().reserved_micros, None);
    // The same run with half a dollar reserved for it when it was admitted.
    let reserved = completed().replace(r#""reserved_micros":null"#, r#""reserved_micros":500000"#);
    let record = RunRecord::from_bytes(reserved.as_bytes()).unwrap();
    assert_eq!(record.spend().reserved_micros, Some(500_000));
    assert_eq!(record.to_bytes(), reserved.as_bytes());
    validate(&schema(), &as_json(&reserved)).unwrap();
    assert_eq!(record.identity().run, "1000-1");
    assert_eq!(record.asked().action, "echo");
    assert_eq!(record.outcome().status, Status::Completed);
    assert_eq!(record.outcome().status.name(), "completed");
    assert_eq!((record.time().start_ms, record.time().end_ms), (1000, 1003));
    let names: Vec<&str> = record.diagnostics().iter().map(Diagnostic::name).collect();
    assert_eq!(names, ["diagnostics/echo.txt", "journal"]);
}

#[test]
fn the_hand_written_record_validates_against_the_schema() {
    validate(&schema(), &as_json(&completed())).unwrap();
}

#[test]
fn every_group_of_the_spec_is_in_the_schema_and_required() {
    // `SPEC.md` section 3, "What every run records": eleven groups. Inputs and outputs
    // are two fields; the judge runs are `judge`.
    let schema = schema();
    let required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap())
        .collect();
    assert_eq!(
        required,
        [
            "record",
            "identity",
            "asked",
            "inputs",
            "outputs",
            "outcome",
            "time",
            "models",
            "spend",
            "egress",
            "steps",
            "judge",
            "diagnostics"
        ]
    );
    // The fields of a record are these and no others, written in this order.
    let document = as_json(&completed());
    assert_eq!(document.as_object().unwrap().len(), required.len());
    let written = completed();
    let mut at = 0;
    for name in required {
        at += written[at..]
            .find(&format!("\"{name}\":"))
            .unwrap_or_else(|| panic!("{name} is not after the field before it"));
    }
}

#[test]
fn a_group_that_nothing_fills_yet_cannot_hold_anything() {
    // By the type, on reading, and by the schema: a record of this version with a model,
    // a spend, an egress, a step, a judge turn, a setting, a duration or a refusal in
    // it is not a record of this version.
    for path in [
        "/models",
        "/spend/requests",
        "/egress",
        "/steps",
        "/judge",
        "/asked/settings",
        "/time/durations",
        "/outcome/refusals",
    ] {
        let bytes = changed(|document| {
            *document.pointer_mut(path).unwrap() = json!([{ "anything": 1 }]);
        });
        let error = RunRecord::from_bytes(&bytes).unwrap_err();
        assert!(
            matches!(error, RecordError::Malformed(_)),
            "{path}: {error}"
        );
        let document: Json = serde_json::from_slice(&bytes).unwrap();
        assert!(validate(&schema(), &document).is_err(), "{path}");
    }
}

#[test]
fn a_record_of_another_version_is_another_version_not_damage() {
    // A `run/1` record, which the code of this checkpoint wrote until the reservation,
    // is another version: a store is not kept before the checkpoint's tag (ADR 0005).
    let bytes = changed(|document| document["record"] = json!("run/1"));
    assert_eq!(
        RunRecord::from_bytes(&bytes).unwrap_err(),
        RecordError::OtherVersion {
            found: "run/1".to_owned()
        }
    );
    let document: Json = serde_json::from_slice(&bytes).unwrap();
    assert!(validate(&schema(), &document).is_err());
    // Whatever else it holds: another version need not have this one's shape.
    let other = b"{\"record\":\"run/9\",\"something\":\"new\"}\n";
    assert!(matches!(
        RunRecord::from_bytes(other).unwrap_err(),
        RecordError::OtherVersion { .. }
    ));
    let said = RunRecord::from_bytes(other).unwrap_err().to_string();
    assert!(
        said.contains("run/9") && said.contains("this build reads run/2"),
        "{said}"
    );
}

#[test]
fn what_is_not_a_record_is_refused_by_the_types_and_by_the_schema() {
    let malformed = |bytes: &[u8]| {
        let error = RunRecord::from_bytes(bytes).unwrap_err();
        assert!(matches!(error, RecordError::Malformed(_)), "{error}");
    };
    malformed(b"");
    malformed(b"not json\n");
    // Each change makes a document that is JSON and is not a record: the types refuse
    // it on reading, and the schema refuses the same document. A change to the schema
    // that lets one of these through fails here.
    let both = |what: &str, change: &dyn Fn(&mut Json)| {
        let bytes = changed(change);
        let error = RunRecord::from_bytes(&bytes).unwrap_err();
        assert!(
            matches!(error, RecordError::Malformed(_)),
            "{what}: {error}"
        );
        let document: Json = serde_json::from_slice(&bytes).unwrap();
        assert!(
            validate(&schema(), &document).is_err(),
            "the schema takes a record with {what}"
        );
    };
    malformed(b"{}\n");
    assert!(validate(&schema(), &json!({})).is_err());
    both("an unknown field at the top", &|d| d["extra"] = json!(1));
    both("an unknown field in identity", &|d| {
        d["identity"]["host"] = json!("x")
    });
    both("an unknown field in asked", &|d| {
        d["asked"]["extra"] = json!(1)
    });
    both("an unknown field in outcome", &|d| {
        d["outcome"]["extra"] = json!(1)
    });
    both("an unknown field in time", &|d| {
        d["time"]["extra"] = json!(1)
    });
    both("an unknown field in spend", &|d| {
        d["spend"]["settled_micros"] = json!(1)
    });
    both("spend as the list it was in run/1", &|d| {
        d["spend"] = json!([])
    });
    both("an amount reserved under zero", &|d| {
        d["spend"]["reserved_micros"] = json!(-1)
    });
    both("an amount reserved that is not whole", &|d| {
        d["spend"]["reserved_micros"] = json!(0.5)
    });
    both("an amount reserved as a text", &|d| {
        d["spend"]["reserved_micros"] = json!("500000")
    });
    both("an unknown field in an argument", &|d| {
        d["asked"]["arguments"][0]["extra"] = json!(1)
    });
    both("an unknown field in a diagnostic", &|d| {
        d["diagnostics"][0]["extra"] = json!(1)
    });
    both("an unknown field in an error", &|d| {
        d["outcome"]["status"] = json!("error");
        d["outcome"]["error"] = json!({"kind": "k", "message": "m", "extra": 1});
    });
    both("an unknown field in an input", &|d| {
        d["inputs"] = json!([{"port": "image", "type": "image", "extra": 1,
            "artifact": d["asked"]["config"], "binding": d["asked"]["config"]}]);
    });
    both("an unknown field in an output", &|d| {
        d["outputs"] = json!([{"port": "mask", "type": "mask", "extra": 1,
            "artifact": d["asked"]["config"]}]);
    });
    // A field left out, at the top and in each group.
    for (group, field) in [
        ("", "spend"),
        ("spend", "reserved_micros"),
        ("spend", "requests"),
        ("", "record"),
        ("identity", "run"),
        ("identity", "request"),
        ("identity", "parent"),
        ("identity", "whizbang"),
        ("asked", "settings"),
        ("asked", "config"),
        ("outcome", "error"),
        ("outcome", "invalid"),
        ("outcome", "refusals"),
        ("time", "end_ms"),
        ("time", "durations"),
    ] {
        both(&format!("no {group} {field}"), &|d| {
            let object = if group.is_empty() { d } else { &mut d[group] };
            object.as_object_mut().unwrap().remove(field);
        });
    }
    both("a diagnostic without its digest", &|d| {
        d["diagnostics"][0]
            .as_object_mut()
            .unwrap()
            .remove("digest");
    });
    // A digest that is not one, a type that is none, a status that is none.
    both("a short digest as the config", &|d| {
        d["asked"]["config"] = json!("sha256:abc")
    });
    both("a long digest in a diagnostic", &|d| {
        let digest = format!("{}0", d["diagnostics"][0]["digest"].as_str().unwrap());
        d["diagnostics"][0]["digest"] = json!(digest);
    });
    both("a type that is none", &|d| {
        d["inputs"] = json!([{"port": "image", "type": "picture",
            "artifact": d["asked"]["config"], "binding": d["asked"]["config"]}]);
    });
    both("an output of a type that is none", &|d| {
        d["outputs"] = json!([{"port": "mask", "type": "picture",
            "artifact": d["asked"]["config"]}]);
    });
    both("a fifth status", &|d| {
        d["outcome"]["status"] = json!("refused")
    });
    // A value under two kinds, and under none.
    both("a value under two kinds", &|d| {
        d["asked"]["arguments"][0]["value"] = json!({"text": "a", "integer": 1});
    });
    both("a value under no kind", &|d| {
        d["asked"]["arguments"][0]["value"] = json!({});
    });
    both("a value of the wrong type for its kind", &|d| {
        d["asked"]["arguments"][0]["value"] = json!({"boolean": "yes"});
    });
    // A time before the epoch, a length under zero, a request id that is not a text.
    both("a negative start", &|d| d["time"]["start_ms"] = json!(-1));
    both("a negative end", &|d| d["time"]["end_ms"] = json!(-1));
    both("a negative length", &|d| {
        d["diagnostics"][0]["bytes"] = json!(-1)
    });
    both("a request id that is a number", &|d| {
        d["identity"]["request"] = json!(7)
    });
    both("an end that is a text", &|d| {
        d["time"]["end_ms"] = json!("1003")
    });
    both("a reason that is a number", &|d| {
        d["outcome"]["status"] = json!("invalid");
        d["outcome"]["invalid"] = json!(3);
    });
    // Every field of an error, an input, an output, an argument and a diagnostic, left
    // out in turn: "each field left out" holds at every level.
    let digest = |d: &Json| d["asked"]["config"].clone();
    for field in ["kind", "message"] {
        both(&format!("an error without its {field}"), &|d| {
            d["outcome"]["status"] = json!("error");
            d["outcome"]["error"] = json!({"kind": "k", "message": "m"});
            d["outcome"]["error"].as_object_mut().unwrap().remove(field);
        });
    }
    for field in ["port", "type", "artifact", "binding"] {
        both(&format!("an input without its {field}"), &|d| {
            d["inputs"] = json!([{"port": "image", "type": "image",
                "artifact": digest(d), "binding": digest(d)}]);
            d["inputs"][0].as_object_mut().unwrap().remove(field);
        });
    }
    for field in ["port", "type", "artifact"] {
        both(&format!("an output without its {field}"), &|d| {
            d["outputs"] = json!([{"port": "mask", "type": "mask", "artifact": digest(d)}]);
            d["outputs"][0].as_object_mut().unwrap().remove(field);
        });
    }
    for field in ["name", "value"] {
        both(&format!("an argument without its {field}"), &|d| {
            d["asked"]["arguments"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
        });
    }
    for field in ["name", "digest", "bytes"] {
        both(&format!("a diagnostic without its {field}"), &|d| {
            d["diagnostics"][0].as_object_mut().unwrap().remove(field);
        });
    }
    for field in ["action", "version", "arguments"] {
        both(&format!("asked without its {field}"), &|d| {
            d["asked"].as_object_mut().unwrap().remove(field);
        });
    }
    both("an outcome without its status", &|d| {
        d["outcome"].as_object_mut().unwrap().remove("status");
    });
    both("a time without its start", &|d| {
        d["time"].as_object_mut().unwrap().remove("start_ms");
    });
    // Another version is refused by both too, the types as another version.
    let other = changed(|d| d["record"] = json!("run/3"));
    assert!(validate(&schema(), &serde_json::from_slice(&other).unwrap()).is_err());
}

#[test]
fn a_request_id_in_a_record_is_held_to_the_rule_of_one_by_the_types_and_its_length_by_the_schema() {
    // ADR 0007: a record's `request` is null or a request id. The types hold it to the
    // whole rule; the schema, which has no pattern here, to its two lengths.
    let with = |request: &str| changed(|d| d["identity"]["request"] = json!(request));
    let as_written = |request: &str| {
        completed().replace(r#""request":null"#, &format!(r#""request":"{request}""#))
    };
    for good in ["a", "order-17", "A.b_c:9", &"x".repeat(128)] {
        let record = RunRecord::from_bytes(as_written(good).as_bytes()).unwrap();
        assert_eq!(record.identity().request.as_deref(), Some(good));
        let document: Json = serde_json::from_slice(&with(good)).unwrap();
        validate(&schema(), &document).unwrap();
    }
    for (bad, schema_refuses) in [
        (String::new(), true),
        ("x".repeat(129), true),
        // What the schema cannot say: the alphabet.
        ("order 17".to_owned(), false),
        ("a/b".to_owned(), false),
    ] {
        let bytes = with(&bad);
        let error = RunRecord::from_bytes(&bytes).unwrap_err();
        assert!(
            matches!(&error, RecordError::Inconsistent(why) if why.contains("request's id")),
            "{bad:?}: {error}"
        );
        // The refusal does not show what it refused.
        assert!(error.to_string().len() < 200);
        let document: Json = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            validate(&schema(), &document).is_err(),
            schema_refuses,
            "{bad:?}"
        );
    }
}

#[test]
fn a_name_that_the_schema_holds_to_one_character_cannot_be_empty() {
    // The schema gives these a least length of one; the types hold them to the same.
    for path in [
        "/identity/run",
        "/identity/whizbang",
        "/asked/action",
        "/asked/version",
        "/asked/arguments/0/name",
    ] {
        let bytes = changed(|d| *d.pointer_mut(path).unwrap() = json!(""));
        let error = RunRecord::from_bytes(&bytes).unwrap_err();
        assert!(
            matches!(error, RecordError::Inconsistent(_)),
            "{path}: {error}"
        );
        assert!(error.to_string().contains("is empty"), "{error}");
        let document: Json = serde_json::from_slice(&bytes).unwrap();
        assert!(validate(&schema(), &document).is_err(), "{path}");
    }
    for (list, entry) in [
        ("inputs", json!({"port": "", "type": "image"})),
        ("outputs", json!({"port": "", "type": "mask"})),
    ] {
        let bytes = changed(|d| {
            let mut entry = entry.clone();
            entry["artifact"] = d["asked"]["config"].clone();
            if list == "inputs" {
                entry["binding"] = d["asked"]["config"].clone();
            }
            d[list] = json!([entry]);
        });
        let error = RunRecord::from_bytes(&bytes).unwrap_err();
        assert!(
            matches!(error, RecordError::Inconsistent(_)),
            "{list}: {error}"
        );
        let document: Json = serde_json::from_slice(&bytes).unwrap();
        assert!(validate(&schema(), &document).is_err(), "{list}");
    }
}

#[test]
fn what_the_schema_cannot_say_the_types_still_refuse() {
    // ADR 0005, "The schema": the cases the schema lets through, because its vocabulary
    // here has no pattern and no rule across two fields. Each is a document the schema
    // takes and the types refuse. If the schema learns to refuse one, this test says so.
    let only_the_types = |what: &str, change: &dyn Fn(&mut Json)| {
        let bytes = changed(change);
        // Refused for what it says, not for how `changed` wrote it.
        let error = RunRecord::from_bytes(&bytes).unwrap_err();
        assert!(
            matches!(
                error,
                RecordError::Malformed(_) | RecordError::Inconsistent(_)
            ),
            "the types take {what}: {error}"
        );
        let document: Json = serde_json::from_slice(&bytes).unwrap();
        validate(&schema(), &document)
            .unwrap_or_else(|error| panic!("the schema now refuses {what}: {error}"));
    };
    // A digest of the right length that is not hex digits.
    only_the_types("a digest that is not hex", &|d| {
        d["asked"]["config"] = json!(format!("sha256:{}", "z".repeat(64)));
    });
    // What no run can say: rules across two fields.
    only_the_types("an error without the status", &|d| {
        d["outcome"]["error"] = json!({"kind": "k", "message": "m"});
    });
    only_the_types("the status invalid without a reason", &|d| {
        d["outcome"]["status"] = json!("invalid");
    });
    only_the_types("an output of a cancelled run", &|d| {
        d["outcome"]["status"] = json!("cancelled");
        d["outputs"] = json!([{"port": "mask", "type": "mask",
            "artifact": d["asked"]["config"]}]);
    });
    only_the_types("an end before the start", &|d| {
        d["time"]["end_ms"] = json!(999)
    });
    // The largest amount there is, which is no amount: the validator has no maximum.
    only_the_types("a reservation of the largest amount", &|d| {
        d["spend"]["reserved_micros"] = json!(u64::MAX)
    });
    // The diagnostics: their order, a name twice, and what a name may be.
    only_the_types("diagnostics out of order", &|d| {
        let (first, second) = (d["diagnostics"][0].clone(), d["diagnostics"][1].clone());
        d["diagnostics"] = json!([second, first]);
    });
    only_the_types("a diagnostic named record", &|d| {
        d["diagnostics"][1]["name"] = json!("record");
    });
    only_the_types(
        "a diagnostic with a name of its own outside diagnostics/",
        &|d| {
            d["diagnostics"][1]["name"] = json!("notes.txt");
        },
    );
    only_the_types("a diagnostic whose name is not plain", &|d| {
        d["diagnostics"][0]["name"] = json!("diagnostics/../record");
    });
    // The one way of writing: `changed` writes the same record with its fields in
    // another order.
    let rewritten = changed(|_| {});
    assert_eq!(
        RunRecord::from_bytes(&rewritten).unwrap_err(),
        RecordError::NotCanonical
    );
    validate(&schema(), &serde_json::from_slice(&rewritten).unwrap()).unwrap();
    // And the size, which is of the bytes and not of the document.
    let mut large = completed().into_bytes();
    large.truncate(large.len() - 1);
    large.extend(std::iter::repeat_n(b' ', record::RECORD_MAX_BYTES as usize));
    large.push(b'\n');
    assert_eq!(
        RunRecord::from_bytes(&large).unwrap_err(),
        RecordError::TooLarge {
            bytes: large.len() as u64
        }
    );
    validate(&schema(), &serde_json::from_slice(&large).unwrap()).unwrap();
}

#[test]
fn the_limit_on_a_record_is_exact() {
    // A record padded to exactly the limit is read, and refused for how it is written;
    // one byte more is refused for its size.
    let limit = record::RECORD_MAX_BYTES as usize;
    let padded = |length: usize| {
        let mut bytes = completed().into_bytes();
        bytes.pop();
        bytes.resize(length - 1, b' ');
        bytes.push(b'\n');
        bytes
    };
    assert_eq!(
        RunRecord::from_bytes(&padded(limit)).unwrap_err(),
        RecordError::NotCanonical
    );
    assert_eq!(
        RunRecord::from_bytes(&padded(limit + 1)).unwrap_err(),
        RecordError::TooLarge {
            bytes: limit as u64 + 1
        }
    );
    // From a store: at the limit the content is read; over it, it is not.
    let dir = Scratch::new("record-limit");
    let store = Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap();
    let mut publication = store.begin().unwrap();
    for (run, length) in [("at", limit), ("over", limit + 1)] {
        publication
            .put_in(
                run,
                record::RECORD_NAME,
                record::RECORD_KIND,
                &padded(length),
            )
            .unwrap();
    }
    publication.publish().unwrap();
    let generation = store.snapshot().unwrap();
    let read = std::cell::Cell::new(0);
    let count = |entry: &store::Entry| {
        read.set(read.get() + 1);
        store.read(entry)
    };
    assert!(matches!(
        record::stored(&generation, "at", count),
        Some(Err(StoredRecordError::Record(RecordError::NotCanonical)))
    ));
    assert_eq!(read.get(), 1);
    assert!(matches!(
        record::stored(&generation, "over", count),
        Some(Err(StoredRecordError::Record(RecordError::TooLarge { .. })))
    ));
    assert_eq!(read.get(), 1);
}

#[test]
fn what_a_refusal_quotes_of_stored_bytes_is_escaped_and_cut() {
    // A field's name with an escape sequence in it, which the reader's own sentence
    // quotes; and a version five thousand characters long.
    let bytes = b"{\"record\":\"run/2\",\"\\u001b[2J\\u0007\":1}\n";
    let error = RunRecord::from_bytes(bytes).unwrap_err();
    assert!(matches!(error, RecordError::Malformed(_)), "{error:?}");
    let said = error.to_string();
    assert!(said.contains("unknown field"), "{said}");
    assert!(!said.chars().any(char::is_control), "{said:?}");
    assert!(!format!("{error:?}").chars().any(char::is_control));
    let long = format!("{{\"record\":\"run/{}\\u001b\"}}\n", "9".repeat(5000));
    let error = RunRecord::from_bytes(long.as_bytes()).unwrap_err();
    let RecordError::OtherVersion { found } = &error else {
        panic!("{error:?}");
    };
    assert_eq!(found.chars().count(), 64);
    let said = error.to_string();
    assert!(said.len() < 200, "{said}");
    assert!(!said.chars().any(char::is_control), "{said:?}");
    // A reason that quotes the bytes is cut at 200 characters.
    let long = format!("{{\"record\":\"run/2\",\"{}\":1}}\n", "f".repeat(5000));
    let said = RunRecord::from_bytes(long.as_bytes())
        .unwrap_err()
        .to_string();
    assert!(said.contains("unknown field"), "{said}");
    assert_eq!(said.len(), "not a run record: ".len() + 200 + 3, "{said}");
    assert!(said.ends_with("..."), "{said}");
    let long = format!("{{\"seq\":0,\"at_ms\":5,\"{}\":1}}\n", "f".repeat(5000));
    let said = read_journal(long.as_bytes()).unwrap_err();
    assert!(said.len() < 240 && said.ends_with("..."), "{said}");
    // A reason of ordinary length is whole, with where in the bytes it was met.
    let said = RunRecord::from_bytes(&changed(|d| d["extra"] = json!(1)))
        .unwrap_err()
        .to_string();
    assert!(said.contains("unknown field `extra`"), "{said}");
    assert!(said.contains(" at line 1 column "), "{said}");
    assert!(!said.ends_with("..."), "{said}");
    assert_eq!((record::QUOTED_LIMIT, record::REASON_LIMIT), (64, 200));
    // A journal's refusal quotes a line's field the same way.
    let error = read_journal(b"{\"seq\":0,\"at_ms\":5,\"\\u001b[2J\":1}\n").unwrap_err();
    assert!(!error.chars().any(char::is_control), "{error:?}");
}

#[test]
fn a_record_that_says_what_no_run_can_is_refused() {
    let inconsistent = |bytes: Vec<u8>, what: &str| {
        let error = RunRecord::from_bytes(&bytes).unwrap_err();
        assert!(
            matches!(error, RecordError::Inconsistent(_)),
            "{what}: {error}"
        );
        assert!(error.to_string().contains(what), "{error}");
    };
    inconsistent(
        changed(|d| d["outcome"]["status"] = json!("error")),
        "an error has its type and message",
    );
    inconsistent(
        changed(|d| d["outcome"]["error"] = json!({"kind": "x", "message": "y"})),
        "an error has its type and message",
    );
    inconsistent(
        changed(|d| d["outcome"]["invalid"] = json!("why")),
        "an invalid run has its reason",
    );
    inconsistent(
        changed(|d| d["outcome"]["status"] = json!("invalid")),
        "an invalid run has its reason",
    );
    inconsistent(
        changed(|d| {
            d["outcome"]["status"] = json!("cancelled");
            d["outputs"] = json!([{"port": "mask", "type": "mask",
                "artifact": d["asked"]["config"]}]);
        }),
        "only a completed run has outputs",
    );
    inconsistent(
        changed(|d| d["time"]["end_ms"] = json!(999)),
        "ends before it starts",
    );
    inconsistent(
        changed(|d| {
            let journal = d["diagnostics"][1].clone();
            d["diagnostics"] = json!([journal, journal]);
        }),
        "in the order of their names",
    );
}

#[test]
fn a_record_is_written_one_way() {
    // The same record with a space in it, without its newline, with its fields in
    // another order, or with a letter written as its escape, is refused.
    let canonical = completed();
    let not_canonical = |bytes: &[u8]| {
        assert_eq!(
            RunRecord::from_bytes(bytes).unwrap_err(),
            RecordError::NotCanonical
        );
    };
    not_canonical(canonical.replace("\"record\":", "\"record\": ").as_bytes());
    not_canonical(canonical.trim_end().as_bytes());
    not_canonical(format!("{canonical}\n").as_bytes());
    not_canonical(
        canonical
            .replace(
                "\"start_ms\":1000,\"end_ms\":1003",
                "\"end_ms\":1003,\"start_ms\":1000",
            )
            .as_bytes(),
    );
    // A letter written as its escape.
    not_canonical(canonical.replace("hello", "hell\\u006f").as_bytes());
    // A whole number written with a point is not a whole number at all.
    assert!(matches!(
        RunRecord::from_bytes(
            canonical
                .replace("\"bytes\":5}", "\"bytes\":5.0}")
                .as_bytes()
        ),
        Err(RecordError::Malformed(_))
    ));
    // A completed run with an output, and an invalid one, are records too.
    let with_output = canonical.replace(
        r#""outputs":[]"#,
        concat!(
            r#""outputs":[{"port":"mask","type":"band_profile","artifact":"#,
            r#""sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}]"#
        ),
    );
    let record = RunRecord::from_bytes(with_output.as_bytes()).unwrap();
    assert_eq!(record.outputs()[0].port, "mask");
    validate(&schema(), &as_json(&with_output)).unwrap();
    let invalid = canonical.replace(
        r#""status":"completed","error":null,"invalid":null"#,
        r#""status":"invalid","error":null,"invalid":"the text is empty""#,
    );
    let record = RunRecord::from_bytes(invalid.as_bytes()).unwrap();
    assert_eq!(
        record.outcome().invalid.as_deref(),
        Some("the text is empty")
    );
    validate(&schema(), &as_json(&invalid)).unwrap();
}

#[test]
fn an_argument_of_each_kind_keeps_its_kind() {
    let text = completed().replace(
        r#"[{"name":"text","value":{"text":"hello"}}]"#,
        concat!(
            r#"[{"name":"amount","value":{"number":2.5}},"#,
            r#"{"name":"whole","value":{"number":8.0}},"#,
            r#"{"name":"count","value":{"integer":-3}},"#,
            r#"{"name":"keep","value":{"boolean":true}},"#,
            r#"{"name":"label","value":{"text":"café"}}]"#
        ),
    );
    // A number that is whole is still written as a number, and read back as one.
    let record = RunRecord::from_bytes(text.as_bytes()).unwrap();
    assert_eq!(record.asked().arguments.len(), 5);
    assert_eq!(
        record.asked().arguments[1].value,
        ArgumentValue::Number(8.0)
    );
    assert_eq!(
        record.asked().arguments[2].value,
        ArgumentValue::Integer(-3)
    );
    validate(&schema(), &as_json(&text)).unwrap();
    // A whole number under `number` without its point is not how it is written.
    let loose = text.replace(r#"{"number":8.0}"#, r#"{"number":8}"#);
    assert_eq!(
        RunRecord::from_bytes(loose.as_bytes()).unwrap_err(),
        RecordError::NotCanonical
    );
}

// --- a record made from a run -----------------------------------------------------------------

struct Fake(ActionDescriptor);

impl Action for Fake {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }
}

/// `echo`, and `look`, which takes an image, prepared against a store that holds one
/// photograph.
struct World {
    _dir: Scratch,
    engine: Engine,
    photo: whizbang_core::artifact::Image,
}

fn world() -> World {
    let dir = Scratch::new("record");
    let root = dir.path().join("store");
    let photo = image_of(photo(), ImageRole::Picture);
    let store = Store::open(&root, Arc::new(RealFs)).unwrap();
    let mut publication = store.begin().unwrap();
    artifact::put(&mut publication, &Artifact::from(photo.clone())).unwrap();
    publication.publish().unwrap();
    drop(store);
    let mut registry = Registry::new();
    registry.register(Arc::new(Echo::new())).unwrap();
    let look =
        ActionDescriptor::draft("look", "2.1.0", Kind::Primitive, "Looks at an image.").input(
            InputPort::required("image", ArtifactType::Image, "The image."),
        );
    registry
        .register(Arc::new(Fake(look.build().unwrap())))
        .unwrap();
    let scale = ActionDescriptor::draft("scale", "1.0.0", Kind::Primitive, "Scales.").argument(
        Argument::required("by", "The factor.", ValueType::number(None, None)),
    );
    registry
        .register(Arc::new(Fake(scale.build().unwrap())))
        .unwrap();
    let engine = Engine::new(
        registry,
        Arc::new(NoBackends),
        Arc::new(Reader::open(&root, Arc::new(RealFs)).unwrap()),
        Digest::of_bytes(b""),
    );
    World {
        _dir: dir,
        engine,
        photo,
    }
}

fn echo(world: &World, text: &str) -> PreparedCall {
    let call = Call::new("echo").argument("text", Value::text(text));
    world.engine.prepare(&call).unwrap()
}

/// The journal the hand-written record names.
const JOURNAL: &str = concat!(
    r#"{"seq":0,"at_ms":1001,"message":"echo: hello"}"#,
    "\n",
    r#"{"seq":1,"at_ms":1002,"message":"echo: done"}"#,
    "\n"
);

#[test]
fn a_record_made_from_a_prepared_call_and_an_outcome_is_these_bytes() {
    // The fixed vector: the record of a completed `echo`, written by hand from ADR 0005.
    // The two digests in it were computed from the bytes of the journal and of `hello`
    // with SHA-256 from another library, apart from this code.
    let world = world();
    let prepared = echo(&world, "hello");
    let completed_outcome = Outcome::Completed {
        outputs: Vec::new(),
    };
    let made = RunRecord::of(
        Ran {
            request: None,
            run: "1000-1",
            whizbang: "0.0.1",
            prepared: &prepared,
            reserved_micros: None,
            outcome: &completed_outcome,
            start_ms: 1000,
            end_ms: 1003,
            // Given out of order: a record lists them by name.
            diagnostics: vec![
                Diagnostic::journal(&read_journal(JOURNAL.as_bytes()).unwrap()).0,
                Diagnostic::written("echo.txt", b"hello", &Secrets::none())
                    .unwrap()
                    .0,
            ],
        },
        &Secrets::none(),
    );
    assert_eq!(String::from_utf8(made.to_bytes()).unwrap(), completed());
    assert_eq!(RunRecord::from_bytes(&made.to_bytes()).unwrap(), made);
    // The journal those bytes name is a journal.
    let lines = read_journal(JOURNAL.as_bytes()).unwrap();
    assert_eq!(lines.len(), 2);
    assert_eq!(journal_bytes(&lines), JOURNAL.as_bytes());
}

fn ran<'a>(prepared: &'a PreparedCall, outcome: &'a Outcome) -> Ran<'a> {
    Ran {
        request: None,
        run: "run-7",
        whizbang: "0.0.1",
        prepared,
        reserved_micros: None,
        outcome,
        start_ms: 50,
        end_ms: 75,
        diagnostics: Vec::new(),
    }
}

#[test]
fn a_failed_run_has_the_same_record_with_its_error_and_no_output() {
    let world = world();
    let prepared = echo(&world, "hello");
    let outcome = Outcome::Error {
        kind: "echo".to_owned(),
        message: "it broke".to_owned(),
    };
    let record = RunRecord::of(ran(&prepared, &outcome), &Secrets::none());
    let written = String::from_utf8(record.to_bytes()).unwrap();
    assert!(written.contains(concat!(
        r#""outputs":[],"outcome":{"status":"error","#,
        r#""error":{"kind":"echo","message":"it broke"},"invalid":null,"refusals":[]}"#
    )));
    assert!(!written.contains("completed"));
    // The same thirteen fields as a completed run's.
    let fields = |text: &str| -> Vec<String> {
        as_json(text).as_object().unwrap().keys().cloned().collect()
    };
    assert_eq!(fields(&written), fields(&completed()));
    validate(&schema(), &as_json(&written)).unwrap();
    assert_eq!(RunRecord::from_bytes(written.as_bytes()).unwrap(), record);

    // Invalid, and cancelled.
    let outcome = Outcome::Invalid {
        why: "the text is empty".to_owned(),
    };
    let record = RunRecord::of(ran(&prepared, &outcome), &Secrets::none());
    assert_eq!(record.outcome().status, Status::Invalid);
    assert_eq!(
        record.outcome().invalid.as_deref(),
        Some("the text is empty")
    );
    assert!(record.outcome().error.is_none());
    validate(
        &schema(),
        &as_json(std::str::from_utf8(&record.to_bytes()).unwrap()),
    )
    .unwrap();
    let record = RunRecord::of(ran(&prepared, &Outcome::Cancelled), &Secrets::none());
    assert_eq!(record.outcome().status, Status::Cancelled);
    assert!(record.outcome().error.is_none() && record.outcome().invalid.is_none());
    RunRecord::from_bytes(&record.to_bytes()).unwrap();
}

#[test]
fn the_inputs_and_the_committed_outputs_of_a_run_are_in_its_record() {
    let world = world();
    let call = Call::new("look").input("image", world.photo.id());
    let prepared = world.engine.prepare(&call).unwrap();
    let made = ArtifactId(Digest::of_bytes(b"a mask the run made"));
    let outcome = Outcome::Completed {
        outputs: vec![Committed::new("mask", ArtifactType::Mask, made)],
    };
    let record = RunRecord::of(ran(&prepared, &outcome), &Secrets::none());
    assert_eq!(record.asked().action, "look");
    assert_eq!(record.asked().version, "2.1.0");
    assert!(record.asked().arguments.is_empty());
    // An input is recorded with the identity of the binding it belongs to.
    assert_eq!(record.inputs().len(), 1);
    let input = &record.inputs()[0];
    assert_eq!(input.port, "image");
    assert_eq!(input.artifact_type, ArtifactType::Image);
    assert_eq!(input.artifact, world.photo.id().0);
    assert_eq!(input.binding, world.photo.binding().id().0);
    assert_eq!(prepared.inputs()[0].binding(), world.photo.binding().id());
    let output = &record.outputs()[0];
    assert_eq!(
        (output.port.as_str(), output.artifact_type, output.artifact),
        ("mask", ArtifactType::Mask, made.0)
    );
    let written = record.to_bytes();
    validate(&schema(), &serde_json::from_slice(&written).unwrap()).unwrap();
    assert_eq!(RunRecord::from_bytes(&written).unwrap(), record);
}

#[test]
fn a_record_with_any_number_in_it_is_read_back() {
    // A record is read by comparing what was read, written again, with the bytes. So
    // every double must read back to itself. These are the first two a reader that
    // takes the fast path gets wrong; a ratio, as a scale or a gamma is, is one such in
    // about nine.
    let world = world();
    let mut values = vec![
        1.6290118483412321_f64,
        1.0715660391465826e-75,
        0.1,
        1e300,
        5e-324,
        f64::MAX,
        -2.5,
    ];
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    for _ in 0..2000 {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let a = (state >> 20) % 1_000_000 + 1;
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let b = (state >> 20) % 1_000_000 + 1;
        values.push(a as f64 / b as f64);
    }
    for value in values {
        let call = Call::new("scale").argument("by", Value::Number(value));
        let prepared = world.engine.prepare(&call).unwrap();
        let record = RunRecord::of(ran(&prepared, &Outcome::Cancelled), &Secrets::none());
        let read = RunRecord::from_bytes(&record.to_bytes())
            .unwrap_or_else(|error| panic!("{value:?} was written and not read: {error}"));
        assert_eq!(read, record, "{value:?}");
    }
}

#[test]
fn a_clock_that_went_back_does_not_make_a_run_end_before_it_started() {
    let world = world();
    let prepared = echo(&world, "hello");
    let outcome = Outcome::Cancelled;
    let mut ran = ran(&prepared, &outcome);
    (ran.start_ms, ran.end_ms) = (90, 40);
    let record = RunRecord::of(ran, &Secrets::none());
    assert_eq!((record.time().start_ms, record.time().end_ms), (90, 90));
    RunRecord::from_bytes(&record.to_bytes()).unwrap();
}

#[test]
fn a_secret_is_taken_out_of_every_text_a_record_takes_from_outside() {
    // A made-up key in an argument, in an error's type and message, and in the reason a
    // run was invalid; and a second secret that JSON would escape.
    let key = made_up_key("");
    let quoted = "pa\"ss\\word".to_owned();
    let secrets = Secrets::new([key.clone(), quoted.clone()]).unwrap();
    let world = world();
    let prepared = echo(&world, &format!("my key is {key} and {quoted}"));
    let outcome = Outcome::Error {
        kind: format!("auth {key}"),
        message: format!("the service refused {key}"),
    };
    let record = RunRecord::of(ran(&prepared, &outcome), &secrets);
    let written = String::from_utf8(record.to_bytes()).unwrap();
    assert!(!written.contains(&key), "{written}");
    assert!(!written.contains("ss\\\\word"), "{written}");
    assert!(
        written.contains(r#"{"text":"my key is [redacted] and [redacted]"}"#),
        "{written}"
    );
    let error = record.outcome().error.as_ref().unwrap();
    assert_eq!(error.kind, "auth [redacted]");
    assert_eq!(error.message, "the service refused [redacted]");
    // Still a record, of the same shape.
    validate(&schema(), &as_json(&written)).unwrap();
    assert_eq!(RunRecord::from_bytes(written.as_bytes()).unwrap(), record);

    let outcome = Outcome::Invalid {
        why: format!("{key} is not a prompt"),
    };
    let record = RunRecord::of(ran(&prepared, &outcome), &secrets);
    assert_eq!(
        record.outcome().invalid.as_deref(),
        Some("[redacted] is not a prompt")
    );
    assert!(!String::from_utf8(record.to_bytes()).unwrap().contains(&key));
    // With no secret given, the text is as it was.
    let record = RunRecord::of(ran(&prepared, &outcome), &Secrets::none());
    assert!(String::from_utf8(record.to_bytes()).unwrap().contains(&key));
}

#[test]
fn a_diagnostic_and_a_journal_line_cannot_be_made_with_a_secret_in_them() {
    // Both are made through a constructor that takes the secrets, and in no other way
    // but by reading what was stored: so what a record lists, and what a journal holds,
    // was redacted when it was made.
    let key = made_up_key("");
    let secrets = Secrets::new([key.clone()]).unwrap();
    let line = JournalLine::new(3, 77, &format!("sending {key}"), &secrets);
    assert_eq!(
        (line.seq(), line.at_ms(), line.message()),
        (3, 77, "sending [redacted]")
    );
    let (journal, stored) = Diagnostic::journal(std::slice::from_ref(&line));
    assert_eq!(journal.name(), "journal");
    assert_eq!(
        String::from_utf8(stored.clone()).unwrap(),
        "{\"seq\":3,\"at_ms\":77,\"message\":\"sending [redacted]\"}\n"
    );
    assert_eq!(journal.digest(), Digest::of_bytes(&stored));
    assert_eq!(journal.bytes(), stored.len() as u64);

    // What an action wrote: the name it gave, under `diagnostics/`; the content with the
    // secrets out, cut in the middle of a character or not; and the digest and length
    // of what is stored, not of what was given.
    let mut body = vec![0xc3];
    body.extend_from_slice(format!(" auth {key}").as_bytes());
    let (written, stored) = Diagnostic::written("worker.err", &body, &secrets).unwrap();
    assert_eq!(written.name(), "diagnostics/worker.err");
    assert_eq!(stored, [&[0xc3][..], b" auth [redacted]"].concat());
    assert_eq!(written.digest(), Digest::of_bytes(&stored));
    assert_eq!(written.bytes(), stored.len() as u64);
    // A name that is not plain is refused, and the refusal is short.
    for name in [
        "",
        "../record",
        "a/b",
        "A.txt",
        ".hidden",
        "a b",
        &"x".repeat(65),
    ] {
        let refused = Diagnostic::written(name, b"x", &secrets).unwrap_err();
        assert!(refused.contains("is no name for a diagnostic"), "{refused}");
        assert!(refused.len() < 300);
    }
    for name in [
        "a",
        "echo.txt",
        "turn-3.exchange.json",
        "record",
        &"x".repeat(64),
    ] {
        assert!(is_diagnostic_name(name), "{name}");
        Diagnostic::written(name, b"x", &secrets).unwrap();
    }
    assert_eq!(DIAGNOSTIC_NAME_LIMIT, 64);
}

// --- a record in a store ----------------------------------------------------------------------

#[test]
fn a_stored_record_is_read_by_whatever_id_the_store_holds_it_under() {
    // A store can hold runs that other code made: one with no record, one whose id is no
    // `RunId`, one whose record names another run, one whose record is not a record.
    let dir = Scratch::new("record-stored");
    let store = Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap();
    let record = RunRecord::from_bytes(completed().as_bytes()).unwrap();
    let loose = "Loose Run/1";
    assert!(RunId::parse(loose).is_err());
    let renamed = completed().replace(r#""run":"1000-1""#, r#""run":"Loose Run/1""#);
    let mut publication = store.begin().unwrap();
    let mut put = |run: &str, name: &str, bytes: &[u8]| {
        publication
            .put_in(run, name, record::RECORD_KIND, bytes)
            .unwrap();
    };
    put("1000-1", record::RECORD_NAME, completed().as_bytes());
    put("1000-1", record::JOURNAL_NAME, JOURNAL.as_bytes());
    put("bare", "notes", b"x");
    put(loose, "record", renamed.as_bytes());
    put("stolen", "record", completed().as_bytes());
    put("broken", "record", b"not a record\n");
    publication.publish().unwrap();
    assert_eq!(record::RECORD_KIND, store::Kind::Diagnostic);
    assert_eq!(record::DIAGNOSTICS_PREFIX, "diagnostics/");

    let generation = store.snapshot().unwrap();
    let read = |id: &str| record::stored(&generation, id, |entry| store.read(entry));
    assert!(read("absent").is_none());
    assert!(matches!(
        read("bare"),
        Some(Err(StoredRecordError::NoRecord))
    ));
    assert_eq!(read(loose).unwrap().unwrap().identity().run, loose);
    let Some(Err(StoredRecordError::OtherRun { names })) = read("stolen") else {
        panic!("a record that names another run was taken for this run's");
    };
    assert_eq!(names, "1000-1");
    assert!(matches!(
        read("broken"),
        Some(Err(StoredRecordError::Record(RecordError::Malformed(_))))
    ));
    assert_eq!(read("1000-1").unwrap().unwrap(), record);
    // A record is a diagnostic reference: readable, and refused where an output is asked
    // for, so nothing can take one as an input.
    let entry = generation.run("1000-1").unwrap().get("record").unwrap();
    assert!(matches!(
        store.read_output(entry),
        Err(store::Error::NotAnOutput { .. })
    ));
    let failed = record::stored(&generation, "1000-1", |entry| store.read_output(entry));
    assert!(matches!(failed, Some(Err(StoredRecordError::Store(_)))));
    // The id of another run that a record names is shown escaped.
    let said = StoredRecordError::OtherRun {
        names: "a\u{1b}[2J".to_owned(),
    }
    .to_string();
    assert!(!said.contains('\u{1b}'), "{said:?}");
    // And cut at 64 characters where it is kept.
    let long = completed().replace(
        r#""run":"1000-1""#,
        &format!(r#""run":"{}""#, "r".repeat(5000)),
    );
    let mut publication = store.begin().unwrap();
    publication
        .put_in("long", "record", record::RECORD_KIND, long.as_bytes())
        .unwrap();
    publication.publish().unwrap();
    let generation = store.snapshot().unwrap();
    let Some(Err(StoredRecordError::OtherRun { names })) =
        record::stored(&generation, "long", |entry| store.read(entry))
    else {
        panic!("a record that names another run was read");
    };
    assert_eq!(names, "r".repeat(64));
}

#[test]
fn a_store_that_fails_under_a_record_is_said_without_a_path() {
    // The record's content goes missing under the store. What comes back names the
    // content by its digest and says nothing of where the store is, as its text or as
    // `{:?}`; whoever runs the store asks for the whole of it by name.
    let dir = Scratch::new("record-lost");
    let root = dir.path().join("store");
    let store = Store::open(&root, Arc::new(RealFs)).unwrap();
    let mut publication = store.begin().unwrap();
    let digest = publication
        .put_in(
            "1000-1",
            record::RECORD_NAME,
            record::RECORD_KIND,
            completed().as_bytes(),
        )
        .unwrap();
    publication.publish().unwrap();
    std::fs::remove_file(root.join("objects").join(digest.hex())).unwrap();
    let generation = store.snapshot().unwrap();
    let Some(Err(error)) = record::stored(&generation, "1000-1", |entry| store.read(entry)) else {
        panic!("a record whose content is gone was read");
    };
    let StoredRecordError::Store(failure) = &error else {
        panic!("{error:?}");
    };
    for said in [error.to_string(), format!("{error:?}")] {
        assert!(said.contains(&digest.hex()), "{said}");
        assert!(!said.contains(root.to_str().unwrap()), "{said}");
        assert!(!said.contains(std::path::MAIN_SEPARATOR), "{said}");
    }
    assert!(failure.cause().to_string().contains(root.to_str().unwrap()));
}

#[test]
fn a_record_over_the_limit_is_refused_before_it_is_read() {
    // The store's entry says how long the content is. A record over four mebibytes is
    // refused on that, and the content is never read.
    assert_eq!(record::RECORD_MAX_BYTES, 4 * 1024 * 1024);
    let dir = Scratch::new("record-large");
    let store = Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap();
    let large = vec![b'x'; record::RECORD_MAX_BYTES as usize + 1];
    let mut publication = store.begin().unwrap();
    publication
        .put_in("big", record::RECORD_NAME, record::RECORD_KIND, &large)
        .unwrap();
    publication.publish().unwrap();
    let generation = store.snapshot().unwrap();
    let read = record::stored(&generation, "big", |_| panic!("the content was read"));
    let Some(Err(StoredRecordError::Record(RecordError::TooLarge { bytes }))) = read else {
        panic!("a record over the limit was not refused for its size");
    };
    assert_eq!(bytes, record::RECORD_MAX_BYTES + 1);
    let said = RecordError::TooLarge { bytes }.to_string();
    assert!(
        said.contains("4194305 bytes") && said.contains("4194304"),
        "{said}"
    );
}

#[test]
fn a_journal_is_read_strictly() {
    assert_eq!(read_journal(JOURNAL.as_bytes()).unwrap().len(), 2);
    assert!(read_journal(b"").unwrap().is_empty());
    assert!(journal_bytes(&[]).is_empty());
    // A journal is stored whole: a line cut short is not forgiven, nor a line out of its
    // place, nor a field that is not known.
    let refused = |bytes: &[u8], what: &str| {
        let error = read_journal(bytes).unwrap_err();
        assert!(error.contains(what), "{error}");
    };
    refused(JOURNAL.trim_end().as_bytes(), "ends with a newline");
    refused(
        &JOURNAL.as_bytes()[..JOURNAL.len() - 12],
        "ends with a newline",
    );
    refused(b"{\"seq\":0,\"at_ms\":5,\"mess\n", "line 1 is not an event");
    refused(
        b"{\"seq\":1,\"at_ms\":5,\"message\":\"a\"}\n",
        "out of its place",
    );
    refused(
        b"{\"seq\":0,\"at_ms\":5,\"message\":\"a\",\"extra\":1}\n",
        "line 1 is not an event",
    );
    refused(&[0xff, b'\n'], "UTF-8");
}
