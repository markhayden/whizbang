use serde_json::json;
use whizbang_testkit::schema::validate;

#[test]
fn a_document_is_held_to_each_keyword() {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["name", "count"],
        "properties": {
            "name": { "type": "string", "minLength": 1, "maxLength": 4 },
            "count": { "type": "integer", "minimum": 0 },
            "ratio": { "type": "number" },
            "kind": { "enum": ["a", "b"] },
            "version": { "const": "x/1" },
            "maybe": { "type": ["string", "null"] },
            "none": { "type": "array", "maxItems": 0 },
            "list": { "type": "array", "items": { "$ref": "#/$defs/item" } },
            "one": { "type": "object", "minProperties": 1, "maxProperties": 1 }
        },
        "$defs": { "item": { "type": "boolean" } }
    });
    let good = json!({
        "name": "ab", "count": 0, "ratio": 2, "kind": "b", "version": "x/1",
        "maybe": null, "none": [], "list": [true, false], "one": { "k": 1 }
    });
    validate(&schema, &good).unwrap();
    let refused = |change: &dyn Fn(&mut serde_json::Value), what: &str| {
        let mut document = good.clone();
        change(&mut document);
        let error = validate(&schema, &document).unwrap_err();
        assert!(error.contains(what), "{error}");
    };
    refused(
        &|d| d["name"] = json!(""),
        "$.name: a string of 0 characters",
    );
    refused(
        &|d| d["name"] = json!("abcde"),
        "$.name: a string of 5 characters",
    );
    refused(&|d| d["name"] = json!(1), "$.name: integer where");
    refused(&|d| d["count"] = json!(1.5), "$.count: number where");
    refused(&|d| d["count"] = json!(-1), "$.count: -1 is under 0");
    refused(&|d| d["ratio"] = json!("2"), "$.ratio: string where");
    refused(&|d| d["kind"] = json!("c"), "$.kind");
    refused(&|d| d["version"] = json!("x/2"), "$.version");
    refused(&|d| d["maybe"] = json!(3), "$.maybe");
    refused(&|d| d["none"] = json!([1]), "$.none: 1 items");
    refused(
        &|d| d["list"] = json!([true, 1]),
        "$.list[1]: integer where",
    );
    refused(&|d| d["one"] = json!({}), "$.one: an object of 0 fields");
    refused(
        &|d| d["one"] = json!({"a": 1, "b": 2}),
        "$.one: an object of 2 fields",
    );
    refused(
        &|d| d["extra"] = json!(1),
        "$: the field extra is not allowed",
    );
    refused(
        &|d| {
            d.as_object_mut().unwrap().remove("count");
        },
        "$: the field count is missing",
    );
}

#[test]
fn a_schema_this_validator_cannot_hold_a_document_to_is_refused() {
    // A keyword it does not know would pass everything: it is an error instead.
    let error = validate(&json!({ "pattern": "^a" }), &json!("b")).unwrap_err();
    assert!(
        error.contains("does not know the keyword pattern"),
        "{error}"
    );
    let error = validate(&json!({ "$ref": "#/$defs/missing" }), &json!(1)).unwrap_err();
    assert!(error.contains("no definition missing"), "{error}");
    let error = validate(&json!({ "$ref": "other.json" }), &json!(1)).unwrap_err();
    assert!(error.contains("a reference is"), "{error}");
    assert!(validate(&json!(true), &json!(1)).is_err());
}
