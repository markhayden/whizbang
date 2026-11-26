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

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use whizbang_core::artifact::{self, Artifact};
use whizbang_core::contract::Recovery;
use whizbang_core::identity::Digest;
use whizbang_core::store::{Generation, Publication, Store};
use whizbang_serve::play::saved::EXAMPLES_LIMIT;

use crate::play_kept::{Named, PICTURES};
use crate::play_saved::{
    EXAMPLE_VERSION, Example, ExampleInput, example_ids, example_in, example_name, input_name,
};
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
#[derive(Clone)]
pub struct Input {
    pub record: &'static str,
    pub example: &'static str,
    pub port: &'static str,
    pub artifact: Artifact,
}

/// Every input of every record, the records in [`RECORDS`]'s order. Squint's pictures are
/// read from the binary's own copy of the records (`whizbang_serve::play::records`).
pub fn inputs() -> Result<Vec<Input>, String> {
    let rascal = whizbang_rascal::examples::inputs()
        .into_iter()
        .map(|i| Input {
            record: i.tool,
            example: i.example,
            port: i.port,
            artifact: i.artifact,
        });
    let find =
        |path: &str| whizbang_serve::play::records::find(path).map(|(_, file)| file.to_vec());
    let squint = whizbang_squint::examples::inputs(&find)?
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
    Ok(all)
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

/// The action a record's example is a call of: the record's own, but for `tiles`, whose
/// tool is `details`, and `protect`'s, whose inputs are those of its first pass.
pub fn action_of(record: &str, example: &str) -> &'static str {
    match (record, example) {
        ("tiles", _) => "details",
        ("protect", "block") => "mask_rect",
        ("protect", _) => "remove",
        _ => RECORDS
            .into_iter()
            .find(|name| *name == record)
            .unwrap_or_default(),
    }
}

/// Whether the input is one the example's call takes, and not what one of those is on.
pub fn is_port(input: &Input) -> bool {
    !input.port.contains(" of ")
}

/// The saved example a record's example is, in the playground (ADR 0048): an id of its
/// own, made of its record and example, so writing again finds it.
pub fn example_id(record: &str, example: &str) -> String {
    let digest = Digest::of_bytes(format!("record-inputs/{record}/{example}").as_bytes());
    digest.hex()[..16].to_owned()
}

/// Writes every input into the store at `root`, which must be there, and gives what to
/// print: a line for each input, then how many artifacts were new.
pub fn write(root: &Path, recovery: Recovery) -> Result<String, String> {
    write_these(root, recovery, inputs)
}

/// [`write`], of the inputs `made` makes once the store is open: a test's, made once.
///
/// In the same publication, for the playground: each image a name in its library
/// (`play/pictures/`), the first example that takes it; and each record's example a saved
/// example (ADR 0048) whose inputs are its call's, by port, so the playground opens it with
/// the inputs the command line takes. Its arguments are left to the form's defaults, and the
/// record says what to set. A name or an example already there is kept as it is.
pub fn write_these(
    root: &Path,
    recovery: Recovery,
    made: impl FnOnce() -> Result<Vec<Input>, String>,
) -> Result<String, String> {
    let opening = wiring::opening(root, Make::Nothing, recovery)
        .map_err(|error| wiring::not_opened(&error))?;
    let store = opening.writer().map_err(|why| why.to_string())?;
    let inputs = made()?;
    let generation = store.snapshot().map_err(|error| error.to_string())?;
    let mut publication = store.begin().map_err(failed)?;
    let (mut seen, mut new) = (BTreeSet::new(), 0);
    let (mut named, mut changed) = (0, false);
    for input in &inputs {
        let (kind, id) = (input.artifact.artifact_type(), input.artifact.id());
        if !seen.insert((kind.name(), id)) {
            continue;
        }
        let held = artifact::holds(&generation, kind, id);
        new += usize::from(!held);
        // An image's name in the playground's library comes in with it, in its import's
        // mark, so a prune of the image lets the name go too, as the playground's own
        // import does.
        let picture = matches!(input.artifact, Artifact::Image(_))
            .then(|| format!("{PICTURES}{}", id.0.hex()));
        let with: Vec<String> = picture.iter().cloned().collect();
        // An input marked as imported within the hour, with its name, is left as it is, as
        // `store import` leaves it, so that writing again changes nothing.
        let name = artifact::name(kind, id);
        if held && crate::import::marked_recently(&generation, &name, &with) {
            continue;
        }
        artifact::import_with(&mut publication, &input.artifact, held, &with).map_err(failed)?;
        changed = true;
        if let (Some(picture), Artifact::Image(image)) = (&picture, &input.artifact)
            && generation.get(picture).is_none()
        {
            let kept = Named {
                id: id.to_string(),
                name: format!("{} · {} · {}", input.record, input.example, input.port),
                width: image.space().width(),
                height: image.space().height(),
                at_ms: now_ms(),
            };
            let bytes = serde_json::to_vec(&kept).map_err(|error| error.to_string())?;
            publication.put(picture, &bytes).map_err(failed)?;
            named += 1;
        }
    }
    let (examples, saved) = save_examples(&store, &generation, &mut publication, &inputs)?;
    if changed || saved > 0 {
        publication.publish().map_err(failed)?;
    }
    let mut printed: String = inputs.iter().map(|input| line(input) + "\n").collect();
    printed.push_str(&format!(
        "{} inputs of {} records: {} artifacts, {new} new to the store; {named} pictures named \
         and {saved} of {examples} saved examples written for the playground\n",
        inputs.len(),
        RECORDS.len(),
        seen.len()
    ));
    Ok(printed)
}

/// A refusal of the store's, or of an artifact's, in words.
fn failed(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

/// Puts a saved example for each record's example the store does not hold already with the
/// same inputs: how many examples there are, and how many were written.
fn save_examples(
    store: &Store,
    generation: &Generation,
    publication: &mut Publication<'_>,
    inputs: &[Input],
) -> Result<(usize, usize), String> {
    let mut examples: Vec<(&str, &str, Vec<&Input>)> = Vec::new();
    for input in inputs.iter().filter(|input| is_port(input)) {
        match examples.last_mut() {
            Some((record, example, given))
                if (*record, *example) == (input.record, input.example) =>
            {
                given.push(input);
            }
            _ => examples.push((input.record, input.example, vec![input])),
        }
    }
    let held = example_ids(generation).len();
    let mut saved = 0;
    for (record, example, given) in &examples {
        let id = example_id(record, example);
        let ports: Vec<ExampleInput> = (given.iter())
            .map(|input| ExampleInput {
                port: input.port.to_owned(),
                artifact_type: input.artifact.artifact_type().name().to_owned(),
                id: input.artifact.id().to_string(),
            })
            .collect();
        let kept = example_in(store, generation, &id);
        if kept.as_ref().is_some_and(|kept| kept.inputs == ports) {
            continue;
        }
        if kept.is_none() && held + saved >= EXAMPLES_LIMIT {
            return Err(format!(
                "the store holds {held} saved examples and can keep {EXAMPLES_LIMIT}: delete some first"
            ));
        }
        let action = action_of(record, example);
        let many = given.iter().filter(|input| input.port == "mask").count();
        let one_mask = if many > 1 {
            format!(
                " The playground opens it with the first of its {many} masks alone, a form \
                 keeping one mask (docs/carry-over.md): so its call there is not the command \
                 line's until that is fixed."
            )
        } else {
            String::new()
        };
        let saved_example = Example {
            example: EXAMPLE_VERSION.to_owned(),
            id: id.clone(),
            name: format!("{record} · {example}"),
            notes: format!(
                "The inputs of the example {example} of the record of {record} \
                 (docs/reference/tools/{record}.md), as whizbang store record-inputs wrote \
                 them: a call of {action}. Its arguments are the form's defaults: set them as \
                 the record says. No run made it.{one_mask}"
            ),
            action: action.to_owned(),
            arguments: Vec::new(),
            backends: BTreeMap::new(),
            models: Vec::new(),
            inputs: ports,
            run: String::new(),
            at_ms: now_ms(),
        };
        let bytes = serde_json::to_vec(&saved_example).map_err(|error| error.to_string())?;
        publication
            .put(&example_name(&id), &bytes)
            .map_err(failed)?;
        for (at, input) in given.iter().enumerate() {
            let file = input.artifact.to_bytes();
            publication
                .put(&input_name(&id, at), &file)
                .map_err(failed)?;
        }
        saved += 1;
    }
    Ok((examples.len(), saved))
}
