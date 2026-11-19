//! An action's descriptor as a tool, and a tool's arguments as a call (ADR 0031, "What
//! is offered"). Each argument of the action is a property of its type; each input port
//! is a property that names an artifact by its identity, or, for a port that takes
//! several, an array of them; and `request_id` is the caller's id for the request. What
//! the client sends is turned into a call by these types alone: the executor holds the
//! call to its descriptor again when it prepares it.

use serde_json::{Map, Value as Json, json};
use whizbang_core::artifact::ArtifactId;
use whizbang_core::contract::{ActionDescriptor, Call, Value, ValueType};
use whizbang_core::identity::Digest;

/// The property every tool takes beside its action's: the caller's id for the request.
pub const REQUEST_ID: &str = "request_id";
/// The most characters of a request id, as the executor takes one.
pub const REQUEST_ID_LIMIT: usize = 128;

/// One tool as the client is told of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Tool {
    pub name: String,
    pub description: String,
    /// The JSON Schema of its arguments: an object.
    pub schema: Json,
}

/// The schema of an artifact's identity.
pub fn identity_schema(purpose: &str) -> Json {
    json!({"type": "string", "pattern": "^sha256:[0-9a-f]{64}$", "description": purpose})
}

/// The schema of the caller's id for a request.
pub fn request_id_schema() -> Json {
    json!({
        "type": "string", "minLength": 1, "maxLength": REQUEST_ID_LIMIT,
        "description": "An id for this request, of your choosing. The same request sent again under it is answered with the run it made the first time, and nothing runs again."
    })
}

fn value_json(value: &Value) -> Json {
    match value {
        Value::Number(number) => json!(number),
        Value::Integer(number) => json!(number),
        Value::Boolean(boolean) => json!(boolean),
        Value::Text(text) => json!(text),
    }
}

fn type_schema(value_type: &ValueType) -> Map<String, Json> {
    let mut schema = Map::new();
    let mut bound = |name: &str, value: Option<Json>| {
        if let Some(value) = value {
            schema.insert(name.to_owned(), value);
        }
    };
    let kind = match value_type {
        ValueType::Number { min, max } => {
            bound("minimum", min.map(|n| json!(n)));
            bound("maximum", max.map(|n| json!(n)));
            "number"
        }
        ValueType::Integer { min, max } => {
            bound("minimum", min.map(|n| json!(n)));
            bound("maximum", max.map(|n| json!(n)));
            "integer"
        }
        ValueType::Boolean => "boolean",
        ValueType::Text => "string",
        ValueType::Choice { choices } => {
            bound("enum", Some(json!(choices)));
            "string"
        }
    };
    schema.insert("type".to_owned(), json!(kind));
    schema
}

/// The action as a tool: its name, its purpose, and the schema of its arguments.
pub fn tool_of(descriptor: &ActionDescriptor) -> Tool {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for argument in descriptor.arguments() {
        let mut schema = type_schema(argument.value_type());
        schema.insert("description".to_owned(), json!(argument.purpose()));
        if let Some(default) = argument.default() {
            schema.insert("default".to_owned(), value_json(default));
        }
        if argument.is_required() {
            required.push(json!(argument.name()));
        }
        properties.insert(argument.name().to_owned(), Json::Object(schema));
    }
    for port in descriptor.inputs() {
        let count = port.count();
        let one = identity_schema(port.purpose());
        let schema = if count.max() == Some(1) {
            one
        } else {
            let mut many = json!({"type": "array", "items": one, "minItems": count.min(),
                                  "description": port.purpose()});
            if let Some(most) = count.max() {
                many["maxItems"] = json!(most);
            }
            many
        };
        if count.min() > 0 {
            required.push(json!(port.name()));
        }
        properties.insert(port.name().to_owned(), schema);
    }
    properties.insert(REQUEST_ID.to_owned(), request_id_schema());
    Tool {
        name: descriptor.name().to_owned(),
        description: descriptor.purpose().to_owned(),
        schema: json!({"type": "object", "properties": properties, "required": required,
                       "additionalProperties": false}),
    }
}

/// An artifact named by its identity, or none when the text is not one.
pub fn identity(value: &Json) -> Option<ArtifactId> {
    value.as_str()?.parse::<Digest>().ok().map(ArtifactId)
}

/// A value of this type, from what the client sent, or none when it is not one. A
/// whole number is taken as a number where a number is asked for; nothing else is
/// changed.
pub fn value_of(value_type: &ValueType, value: &Json) -> Option<Value> {
    match value_type {
        ValueType::Number { .. } => value.as_f64().map(Value::Number),
        ValueType::Integer { .. } => value.as_i64().map(Value::Integer),
        ValueType::Boolean => value.as_bool().map(Value::Boolean),
        ValueType::Text | ValueType::Choice { .. } => value.as_str().map(Value::text),
    }
}

/// A tool's arguments as a call of its action, and the caller's id for the request when
/// it gave one; or why not, in fixed words that name the property and never what was
/// sent in it.
pub fn call_of(
    descriptor: &ActionDescriptor,
    arguments: &Map<String, Json>,
) -> Result<(Call, Option<String>), String> {
    let mut call = Call::new(descriptor.name());
    let mut request_id = None;
    for (name, sent) in arguments {
        if name == REQUEST_ID {
            let Some(text) = sent.as_str() else {
                return Err(format!("{REQUEST_ID} is a string"));
            };
            request_id = Some(text.to_owned());
            continue;
        }
        if let Some(argument) = descriptor.arguments().iter().find(|a| a.name() == name) {
            let Some(value) = value_of(argument.value_type(), sent) else {
                return Err(format!(
                    "the argument {} is not of its type",
                    argument.name()
                ));
            };
            call = call.argument(argument.name(), value);
            continue;
        }
        let Some(port) = descriptor.inputs().iter().find(|p| p.name() == name) else {
            return Err(
                "an argument was given that the tool does not take; its schema lists those it takes"
                    .to_owned(),
            );
        };
        let not_identity = || {
            format!(
                "{} is no artifact identity: an identity is sha256: and 64 lower-case hex digits",
                port.name()
            )
        };
        let sent: Vec<&Json> = match sent {
            Json::Array(many) if port.count().max() != Some(1) => many.iter().collect(),
            one if port.count().max() == Some(1) => vec![one],
            _ => return Err(not_identity()),
        };
        for one in sent {
            call = call.input(port.name(), identity(one).ok_or_else(not_identity)?);
        }
    }
    Ok((call, request_id))
}
