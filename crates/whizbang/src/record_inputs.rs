//! `whizbang store record-inputs`: writes the inputs of every tool record's worked examples
//! into a store, as canonical artifacts, and prints each one's identity (plan E, task O1, "The
//! inputs, made before O1.1"). A walkthrough runs a record's examples on them, on the command
//! line and in the playground, so both sides take the inputs the record shows.
//!
//! The inputs are made by the constructors the records' own tests build them with
//! (`whizbang_rascal::examples`, `whizbang_squint::examples`). Each is written with what it
//! depends on: a mask with the image its binding names, a location or a band profile with
//! its image and its line; a mask keeps its protect plane, its band profile and its
//! prescriptions, which a picture of it would not (`store import --mask`). They are written as
//! `store import` writes an artifact file, marked as imported, all in one publication.
//!
//! It covers every row of a tool in plan E's session O1.1, one of them not wholly: `protect`
//! is given a first pass's composite and mask, which only a fill makes, so what is written
//! for it is what that first pass takes. Each record lists its inputs between its `inputs`
//! marks, held to what this writes by `tests/it/record_inputs.rs`.

use std::collections::BTreeSet;
use std::path::Path;

use whizbang_core::artifact::{self, Artifact};
use whizbang_core::contract::Recovery;

use crate::wiring::{self, Make};

/// The records, in the order of plan E's session O1.1: `describe`'s order of the primitives,
/// `tiles` in the place of `details`, whose record it is; then the recipes.
pub const RECORDS: [&str; 34] = [
    "align",
    "change_map",
    "edge_similarity",
    "tiles",
    "close",
    "feather",
    "grow",
    "plug",
    "render",
    "resize",
    "select",
    "subtract",
    "view",
    "paste",
    "cut",
    "put",
    "window",
    "unwindow",
    "apply",
    "line",
    "rect",
    "stroke",
    "rasterize",
    "shrink",
    "invert",
    "carry",
    "tighten",
    "inspect",
    "fan",
    "profile",
    "ramp",
    "measure",
    "mask_rect",
    "protect",
];

/// One input of a record's example: the record, the example, the port of the example's call
/// that takes it (or what it is to the input that does), and the artifact.
pub struct Input {
    pub record: &'static str,
    pub example: &'static str,
    pub port: &'static str,
    pub artifact: Artifact,
}

/// Every input of every record, the records in [`RECORDS`]'s order.
pub fn inputs() -> Vec<Input> {
    let rascal = whizbang_rascal::examples::inputs()
        .into_iter()
        .map(|i| Input {
            record: i.tool,
            example: i.example,
            port: i.port,
            artifact: i.artifact,
        });
    let squint = whizbang_squint::examples::inputs()
        .into_iter()
        .map(|i| Input {
            record: i.tool,
            example: i.example,
            port: i.port,
            artifact: i.artifact,
        });
    let mut all: Vec<Input> = rascal.chain(squint).collect();
    // A stable sort: each record's inputs stay in the order they are given in.
    all.sort_by_key(|input| RECORDS.iter().position(|record| *record == input.record));
    all
}

/// The line printed for an input, tab-separated: record, example, port, type and identity.
pub fn line(input: &Input) -> String {
    format!(
        "{}\t{}\t{}\t{} {}",
        input.record,
        input.example,
        input.port,
        input.artifact.artifact_type(),
        input.artifact.id()
    )
}

/// The list a record holds between its `inputs` marks: its examples' inputs as a table.
pub fn listed(inputs: &[Input], record: &str) -> String {
    let mut out = String::from("\n| Example | Input | Type | Identity |\n|---|---|---|---|\n");
    for input in inputs.iter().filter(|input| input.record == record) {
        out.push_str(&format!(
            "| {} | {} | {} | `{}` |\n",
            input.example,
            input.port,
            input.artifact.artifact_type(),
            input.artifact.id()
        ));
    }
    out.push('\n');
    out
}

/// Writes every input into the store at `root`, which must be there, in one publication,
/// and gives what to print: a line for each input, then how many artifacts were new.
pub fn write(root: &Path, recovery: Recovery) -> Result<String, String> {
    write_these(root, recovery, &inputs())
}

/// [`write`], of these inputs: a test's, made once.
pub fn write_these(root: &Path, recovery: Recovery, inputs: &[Input]) -> Result<String, String> {
    let opening = wiring::opening(root, Make::Nothing, recovery)
        .map_err(|error| wiring::not_opened(&error))?;
    let store = opening.writer().map_err(|why| why.to_string())?;
    let generation = store.snapshot().map_err(|error| error.to_string())?;
    let mut publication = store.begin().map_err(|error| error.to_string())?;
    let (mut seen, mut new) = (BTreeSet::new(), 0);
    for input in inputs {
        let (kind, id) = (input.artifact.artifact_type(), input.artifact.id());
        if !seen.insert((kind.name(), id)) {
            continue;
        }
        let held = artifact::holds(&generation, kind, id);
        new += usize::from(!held);
        artifact::import_with(&mut publication, &input.artifact, held, &[])
            .map_err(|error| error.to_string())?;
    }
    publication.publish().map_err(|error| error.to_string())?;
    let mut printed: String = inputs.iter().map(|input| line(input) + "\n").collect();
    printed.push_str(&format!(
        "{} inputs of {} records: {} artifacts, {new} new to the store\n",
        inputs.len(),
        RECORDS.len(),
        seen.len()
    ));
    Ok(printed)
}
