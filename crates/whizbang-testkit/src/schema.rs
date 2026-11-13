//! A small JSON Schema validator, for the tests: enough of the 2020-12 vocabulary to
//! hold a document to the schemas this repository writes by hand, and no more.
//!
//! A schema is read whole before any document is held to it. A keyword this validator
//! does not know, and a keyword whose value is not of the kind the keyword takes, is
//! refused wherever in the schema it is, whether or not the document reaches it: a
//! schema never passes for holding less than it says.

use serde_json::Value;

/// Checks `document` against `schema`. The error says where the document fails, as a
/// path from its root, and why; or where the schema is one this validator cannot hold a
/// document to, as a path from the schema's root.
pub fn validate(schema: &Value, document: &Value) -> Result<(), String> {
    well_formed(schema, schema, "#")?;
    check(schema, schema, document, "$", 0)
}

/// How many references in a row are followed before the schema is taken to refer to
/// itself.
const MAX_REFERENCES: u32 = 16;

const TYPES: &[&str] = &[
    "null", "boolean", "integer", "number", "string", "array", "object",
];

fn type_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// The definition a reference names, when it is `#/$defs/<name>` and there is one.
fn target<'a>(root: &'a Value, reference: &Value, at: &str) -> Result<&'a Value, String> {
    let reference = reference
        .as_str()
        .ok_or_else(|| format!("{at}: $ref is a string"))?;
    let name = reference
        .strip_prefix("#/$defs/")
        .ok_or_else(|| format!("{at}: a reference is #/$defs/<name>"))?;
    root.get("$defs")
        .and_then(|defs| defs.get(name))
        .ok_or_else(|| format!("{at}: there is no definition {name}"))
}

/// Reads a schema whole: every keyword is one this validator knows, with a value of the
/// kind it takes, here and in every schema inside this one.
fn well_formed(root: &Value, schema: &Value, at: &str) -> Result<(), String> {
    let Some(schema) = schema.as_object() else {
        return Err(format!("{at}: a schema is an object"));
    };
    for (keyword, value) in schema {
        let wrong = |takes: &str| Err(format!("{at}: {keyword} takes {takes}"));
        match keyword.as_str() {
            "$schema" | "title" => {
                if !value.is_string() {
                    return wrong("a string");
                }
            }
            "$ref" => {
                target(root, value, at)?;
            }
            "$defs" | "properties" => {
                let Some(schemas) = value.as_object() else {
                    return wrong("an object of schemas");
                };
                for (name, inner) in schemas {
                    well_formed(root, inner, &format!("{at}/{keyword}/{name}"))?;
                }
            }
            "items" => well_formed(root, value, &format!("{at}/items"))?,
            "oneOf" => {
                let Some(schemas) = value.as_array().filter(|schemas| !schemas.is_empty()) else {
                    return wrong("a list of schemas");
                };
                for (index, inner) in schemas.iter().enumerate() {
                    well_formed(root, inner, &format!("{at}/oneOf/{index}"))?;
                }
            }
            "type" => {
                let known = |name: &Value| name.as_str().is_some_and(|n| TYPES.contains(&n));
                let fine = match value {
                    Value::String(_) => known(value),
                    Value::Array(names) => !names.is_empty() && names.iter().all(known),
                    _ => false,
                };
                if !fine {
                    return wrong("the name of a type, or a list of them");
                }
            }
            "const" => {}
            "enum" => {
                if !value.as_array().is_some_and(|values| !values.is_empty()) {
                    return wrong("a list of values");
                }
            }
            "required" => {
                let names = value.as_array();
                if !names.is_some_and(|names| names.iter().all(Value::is_string)) {
                    return wrong("a list of names");
                }
            }
            "additionalProperties" => {
                if !value.is_boolean() {
                    return wrong("true or false: a schema there is not supported");
                }
            }
            "minProperties" | "maxProperties" | "maxItems" | "minLength" | "maxLength" => {
                if !value.is_u64() {
                    return wrong("a whole number that is not negative");
                }
            }
            "minimum" | "maximum" => {
                if !value.is_number() {
                    return wrong("a number");
                }
            }
            unknown => {
                return Err(format!(
                    "{at}: this validator does not know the keyword {unknown}"
                ));
            }
        }
    }
    Ok(())
}

/// Holds a value to a schema that [`well_formed`] has read.
fn check(
    root: &Value,
    schema: &Value,
    value: &Value,
    at: &str,
    references: u32,
) -> Result<(), String> {
    if let Some(reference) = schema.get("$ref") {
        if references >= MAX_REFERENCES {
            return Err(format!(
                "{at}: a reference in the schema leads back to itself"
            ));
        }
        check(
            root,
            target(root, reference, at)?,
            value,
            at,
            references + 1,
        )?;
    }
    if let Some(schemas) = schema.get("oneOf").and_then(Value::as_array) {
        let fits = |inner: &&Value| check(root, inner, value, at, references).is_ok();
        let fitting = schemas.iter().filter(fits).count();
        if fitting != 1 {
            return Err(format!(
                "{at}: the value fits {fitting} of the schemas of which it must fit one"
            ));
        }
    }
    if let Some(types) = schema.get("type") {
        let found = type_of(value);
        // An integer is a number too.
        let is = |name: &str| name == found || (name == "number" && found == "integer");
        let allowed = match types {
            Value::String(name) => is(name),
            Value::Array(names) => names.iter().filter_map(Value::as_str).any(is),
            _ => false,
        };
        if !allowed {
            return Err(format!("{at}: {found} where {types} is required"));
        }
    }
    if let Some(constant) = schema.get("const")
        && constant != value
    {
        return Err(format!("{at}: {value} where {constant} is required"));
    }
    if let Some(choices) = schema.get("enum").and_then(Value::as_array)
        && !choices.contains(value)
    {
        return Err(format!("{at}: {value} is not one of the values allowed"));
    }
    let limit = |key: &str| schema.get(key).and_then(Value::as_u64);
    if let Value::String(text) = value {
        // A length is counted in characters, not in bytes.
        let length = text.chars().count() as u64;
        if limit("minLength").is_some_and(|min| length < min)
            || limit("maxLength").is_some_and(|max| length > max)
        {
            return Err(format!("{at}: a string of {length} characters"));
        }
    }
    if let Some(minimum) = schema.get("minimum").and_then(Value::as_f64)
        && value.as_f64().is_some_and(|number| number < minimum)
    {
        return Err(format!("{at}: {value} is under {minimum}"));
    }
    if let Some(maximum) = schema.get("maximum").and_then(Value::as_f64)
        && value.as_f64().is_some_and(|number| number > maximum)
    {
        return Err(format!("{at}: {value} is over {maximum}"));
    }
    if let Value::Array(items) = value {
        if limit("maxItems").is_some_and(|max| items.len() as u64 > max) {
            return Err(format!("{at}: {} items", items.len()));
        }
        if let Some(each) = schema.get("items") {
            for (index, item) in items.iter().enumerate() {
                check(root, each, item, &format!("{at}[{index}]"), 0)?;
            }
        }
    }
    if let Value::Object(fields) = value {
        let count = fields.len() as u64;
        if limit("minProperties").is_some_and(|min| count < min)
            || limit("maxProperties").is_some_and(|max| count > max)
        {
            return Err(format!("{at}: an object of {count} fields"));
        }
        let none = serde_json::Map::new();
        let properties = schema
            .get("properties")
            .and_then(Value::as_object)
            .unwrap_or(&none);
        for name in schema
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if !fields.contains_key(name) {
                return Err(format!("{at}: the field {name} is missing"));
            }
        }
        for (name, field) in fields {
            match properties.get(name) {
                Some(property) => check(root, property, field, &format!("{at}.{name}"), 0)?,
                None if schema.get("additionalProperties") == Some(&Value::Bool(false)) => {
                    return Err(format!("{at}: the field {name} is not allowed"));
                }
                None => {}
            }
        }
    }
    Ok(())
}
