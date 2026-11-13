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

#[test]
fn a_value_fits_exactly_one_of_the_schemas_of_a_one_of() {
    // The entries of the record's `judge` list are of two shapes, told apart by a word.
    let schema = json!({ "oneOf": [
        { "type": "object", "additionalProperties": false, "required": ["kind", "n"],
          "properties": { "kind": { "const": "turn" }, "n": { "type": "integer" } } },
        { "type": "object", "additionalProperties": false, "required": ["kind"],
          "properties": { "kind": { "const": "verdict" }, "why": { "type": "string" } } },
    ] });
    validate(&schema, &json!({ "kind": "turn", "n": 1 })).unwrap();
    validate(&schema, &json!({ "kind": "verdict", "why": "x" })).unwrap();
    for neither in [
        json!({ "kind": "turn" }),
        json!({ "kind": "verdict", "n": 1 }),
        json!({ "kind": "other" }),
        json!(7),
    ] {
        let error = validate(&schema, &neither).unwrap_err();
        assert!(error.contains("fits 0 of the schemas"), "{error}");
    }
    // A value that fits two is refused as well: one, and no more.
    let loose = json!({ "oneOf": [{ "type": "integer" }, { "minimum": 0 }] });
    let error = validate(&loose, &json!(3)).unwrap_err();
    assert!(error.contains("fits 2 of the schemas"), "{error}");
    validate(&loose, &json!(-3)).unwrap();
    // It takes a list of schemas, each read whole.
    for (wrong, says) in [
        (json!({ "oneOf": [] }), "oneOf takes a list of schemas"),
        (json!({ "oneOf": {} }), "oneOf takes a list of schemas"),
        (
            json!({ "oneOf": [{ "pattern": "^a" }] }),
            "#/oneOf/0: this validator does not know",
        ),
    ] {
        let error = validate(&wrong, &json!(1)).unwrap_err();
        assert!(error.contains(says), "{error}");
    }
}

#[test]
fn a_length_is_counted_in_characters() {
    // Four letters of two bytes each are four characters, and eight bytes.
    let schema = json!({ "type": "string", "minLength": 4, "maxLength": 4 });
    validate(&schema, &json!("\u{e9}\u{e9}\u{e9}\u{e9}")).unwrap();
    let error = validate(&schema, &json!("\u{e9}\u{e9}")).unwrap_err();
    assert!(error.contains("a string of 2 characters"), "{error}");
    let error = validate(&schema, &json!("\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}")).unwrap_err();
    assert!(error.contains("a string of 5 characters"), "{error}");
}

#[test]
fn a_schema_is_read_whole_before_a_document_is_held_to_it() {
    // The keyword is under a property the document does not have, in a definition
    // nothing refers to, and in the items of a list that is empty: it is refused all
    // the same, with where in the schema it is.
    let document = json!({ "list": [] });
    for (schema, at) in [
        (
            json!({ "properties": { "absent": { "pattern": "^a" } } }),
            "#/properties/absent",
        ),
        (
            json!({ "$defs": { "unused": { "format": "date" } } }),
            "#/$defs/unused",
        ),
        (
            json!({ "properties": { "list": { "items": { "anyOf": [] } } } }),
            "#/properties/list/items",
        ),
    ] {
        let error = validate(&schema, &document).unwrap_err();
        assert!(error.starts_with(at), "{error}");
        assert!(error.contains("does not know the keyword"), "{error}");
    }
}

#[test]
fn a_keyword_with_a_value_of_the_wrong_kind_is_refused_and_not_ignored() {
    let refused = |schema: serde_json::Value, what: &str| {
        // Held against a document that would pass if the keyword were ignored.
        let error = validate(&schema, &json!({ "a": "text", "n": 1, "l": [1] }))
            .err()
            .unwrap_or_else(|| panic!("{schema} was taken"));
        assert!(error.contains(what), "{schema}: {error}");
    };
    refused(
        json!({ "additionalProperties": { "type": "string" } }),
        "additionalProperties takes true or false",
    );
    refused(json!({ "required": "a" }), "required takes a list of names");
    refused(
        json!({ "required": ["a", 1] }),
        "required takes a list of names",
    );
    refused(json!({ "enum": "a" }), "enum takes a list of values");
    refused(json!({ "enum": [] }), "enum takes a list of values");
    refused(json!({ "properties": ["a"] }), "properties takes an object");
    refused(
        json!({ "properties": { "a": true } }),
        "a schema is an object",
    );
    refused(json!({ "$defs": [] }), "$defs takes an object");
    refused(json!({ "items": [] }), "a schema is an object");
    refused(json!({ "$ref": 3 }), "$ref is a string");
    refused(json!({ "type": "text" }), "type takes the name of a type");
    refused(
        json!({ "type": ["string", "text"] }),
        "type takes the name of a type",
    );
    refused(json!({ "type": [] }), "type takes the name of a type");
    refused(json!({ "type": 3 }), "type takes the name of a type");
    refused(json!({ "title": 1 }), "title takes a string");
    for keyword in [
        "minLength",
        "maxLength",
        "maxItems",
        "minProperties",
        "maxProperties",
    ] {
        for value in [json!("1"), json!(0.0), json!(1.5), json!(-1), json!(null)] {
            refused(
                json!({ "properties": { "a": { keyword: value } } }),
                &format!("{keyword} takes a whole number"),
            );
        }
    }
    refused(json!({ "minimum": "0" }), "minimum takes a number");
    // And a schema that is fine is taken.
    validate(
        &json!({ "additionalProperties": true, "minimum": -1.5, "maxItems": 0 }),
        &json!({ "a": 1 }),
    )
    .unwrap();
}

#[test]
fn a_reference_that_leads_back_to_itself_is_refused() {
    let itself = json!({ "$ref": "#/$defs/a", "$defs": { "a": { "$ref": "#/$defs/a" } } });
    let error = validate(&itself, &json!(1)).unwrap_err();
    assert!(error.contains("leads back to itself"), "{error}");
    let round = json!({
        "$ref": "#/$defs/a",
        "$defs": { "a": { "$ref": "#/$defs/b" }, "b": { "$ref": "#/$defs/a" } }
    });
    assert!(
        validate(&round, &json!(1))
            .unwrap_err()
            .contains("leads back to itself")
    );
    // A schema that refers to itself through the document's own depth is fine: a list
    // of lists, as deep as the document goes.
    let nested = json!({
        "$ref": "#/$defs/list",
        "$defs": { "list": { "type": "array", "items": { "$ref": "#/$defs/list" } } }
    });
    validate(&nested, &json!([[[], []], []])).unwrap();
    assert!(validate(&nested, &json!([[1]])).is_err());
}
