//! Tool files offered to a judge (plan 2C, task X1, item 4; ADR 0032): what `check` knows
//! of a tool file, and how a judge's call of one is read. The files themselves, their
//! declarations and the worker that runs them are the binary's (`whizbang::tool_file`);
//! `check` reaches them through [`FileTools`], which the binary hands its toolbox.
//!
//! A tool file is offered only to a run that names it (`SPEC.md` section 4, "Reach"); its
//! schema is the one its declaration gives: an image for each binding, by the names the
//! judge knows the run's images and frames by, and each argument it declares. What it
//! answers is its output, held to its declared bounds before the judge is shown it. A
//! capability it asks for is a nested call of the run that called it.

use serde_json::{Map, Value as Json, json};
use whizbang_core::artifact::ArtifactId;
use whizbang_core::capability::ToolDefinition;
use whizbang_core::contract::Context;

use crate::details::{self, Names};

/// One argument a tool file declares: its name, whether it is a number (else a text),
/// what it is for, and whether a call must give it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileArgument {
    pub name: String,
    pub number: bool,
    pub purpose: String,
    pub required: bool,
}

/// What a judge is told of a tool file: its name and purpose, the images it reads by
/// the names it declares them under, and its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTool {
    pub name: String,
    pub purpose: String,
    pub bindings: Vec<String>,
    pub arguments: Vec<FileArgument>,
}

/// One run of a tool file: the digest of the file it ran, `sha256:` and 64 hex digits,
/// and its output, held to its declaration, as JSON.
#[derive(Debug, Clone, PartialEq)]
pub struct FileRan {
    pub digest: String,
    pub output: Json,
}

/// The tool files of a build, as the binary hands them to `check`.
pub trait FileTools: Send + Sync {
    /// Each tool file listed, in the config's order.
    fn listed(&self) -> Vec<FileTool>;

    /// The revision of the tool file of this name now: `sha256:` and the digest of its
    /// bytes as they are on the disk, for the decision cache's key. None when it cannot
    /// be read.
    fn revision(&self, name: &str) -> Option<String>;

    /// Runs the tool file of this name under this context: each image by the binding it
    /// declares, and its arguments. Its capabilities are nested calls of the context's
    /// run. Its output, held to its declaration, or why there is none, in a sentence.
    fn run(
        &self,
        name: &str,
        images: &[(String, ArtifactId)],
        arguments: &Map<String, Json>,
        context: &Context,
    ) -> Result<FileRan, String>;
}

/// The tool a judge is offered for a tool file, with the schema its declaration gives.
pub fn definition(tool: &FileTool) -> Result<ToolDefinition, String> {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for binding in &tool.bindings {
        properties.insert(
            binding.clone(),
            json!({"type": "string", "description": format!(
                "The image the tool reads as {binding}: a frame id (raw:source, \
                 aligned:candidate:t1), or an image id (source, candidate) for its raw frame."
            )}),
        );
        required.push(json!(binding));
    }
    for argument in &tool.arguments {
        let kind = if argument.number { "number" } else { "string" };
        properties.insert(
            argument.name.clone(),
            json!({"type": kind, "description": argument.purpose}),
        );
        if argument.required {
            required.push(json!(argument.name));
        }
    }
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": required,
        "properties": properties,
    });
    ToolDefinition::new(&tool.name, &tool.purpose, schema).map_err(|why| why.to_string())
}

/// What a judge's call of a tool file asks for: each image by its binding, and the
/// arguments.
pub type FileCall = (Vec<(String, ArtifactId)>, Map<String, Json>);

/// The images and arguments a judge's call of a tool file asks for, or why it asks for
/// none: each binding an image the run knows by that name, each argument of its type.
pub fn call_of(tool: &FileTool, arguments: &Json, names: &Names) -> Result<FileCall, String> {
    let Some(given) = arguments.as_object() else {
        return Err("the arguments are not an object".to_owned());
    };
    let known = |key: &str| {
        tool.bindings.iter().any(|b| b == key) || tool.arguments.iter().any(|a| a.name == key)
    };
    if let Some(key) = given.keys().find(|key| !known(key)) {
        let shown: String = key.chars().take(64).collect();
        return Err(format!("{shown:?} is no argument of the tool"));
    }
    let mut images = Vec::new();
    for binding in &tool.bindings {
        let (id, _) = details::image_of(given, binding, names)?;
        images.push((binding.clone(), id));
    }
    let mut chosen = Map::new();
    for argument in &tool.arguments {
        match given.get(&argument.name) {
            None if argument.required => {
                return Err(format!("{} is required", argument.name));
            }
            None => {}
            Some(value @ Json::Number(_)) if argument.number => {
                chosen.insert(argument.name.clone(), value.clone());
            }
            Some(value @ Json::String(_)) if !argument.number => {
                chosen.insert(argument.name.clone(), value.clone());
            }
            Some(_) => return Err(format!("{} is not of its type", argument.name)),
        }
    }
    Ok((images, chosen))
}

/// What the judge is told of a run of a tool file: its name, that it is experimental, the
/// digest it ran, and its output.
pub fn answer(id: &str, tool: &str, ran: &FileRan) -> String {
    format!(
        "{id}: {tool} (experimental; a tool file, {}) measured: {}",
        ran.digest, ran.output
    )
}
