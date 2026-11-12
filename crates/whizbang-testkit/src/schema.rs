//! A small JSON Schema validator, for the tests: enough of the 2020-12 vocabulary to
//! hold a document to the schemas this repository writes by hand, and no more. A keyword
//! it does not know is refused, so a schema never passes for holding less than it says.

use serde_json::Value;

/// Checks `document` against `schema`. The error says where the document fails, as a
/// path from its root, and why.
pub fn validate(schema: &Value, document: &Value) -> Result<(), String> {
    check(schema, schema, document, "$")
}

const KNOWN: &[&str] = &[
    "$schema",
    "$defs",
    "$ref",
    "title",
    "type",
    "const",
    "enum",
    "required",
    "properties",
    "additionalProperties",
    "minProperties",
    "maxProperties",
    "items",
    "maxItems",
    "minLength",
    "maxLength",
    "minimum",
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

fn check(root: &Value, schema: &Value, value: &Value, at: &str) -> Result<(), String> {
    let Some(schema) = schema.as_object() else {
        return Err(format!("{at}: a schema is an object"));
    };
    if let Some(unknown) = schema.keys().find(|key| !KNOWN.contains(&key.as_str())) {
        return Err(format!(
            "{at}: this validator does not know the keyword {unknown}"
        ));
    }
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let name = reference
            .strip_prefix("#/$defs/")
            .ok_or_else(|| format!("{at}: a reference is #/$defs/<name>"))?;
        let target = root
            .get("$defs")
            .and_then(|defs| defs.get(name))
            .ok_or_else(|| format!("{at}: there is no definition {name}"))?;
        check(root, target, value, at)?;
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
    if let Value::Array(items) = value {
        if limit("maxItems").is_some_and(|max| items.len() as u64 > max) {
            return Err(format!("{at}: {} items", items.len()));
        }
        if let Some(each) = schema.get("items") {
            for (index, item) in items.iter().enumerate() {
                check(root, each, item, &format!("{at}[{index}]"))?;
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
                Some(property) => check(root, property, field, &format!("{at}.{name}"))?,
                None if schema.get("additionalProperties") == Some(&Value::Bool(false)) => {
                    return Err(format!("{at}: the field {name} is not allowed"));
                }
                None => {}
            }
        }
    }
    Ok(())
}
