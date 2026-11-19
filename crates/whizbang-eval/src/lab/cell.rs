//! A lab's cells (rascal's `lab/cell.rs`): one arm on one image. A cell's identity is the
//! SHA-256 over its [`Components`], everything that changes its work and nothing else, all
//! known before anything runs: the protocol's digest; the image's digest; each bound
//! input's digest and each bound text; the overrides as applied; the recipe as it runs (its
//! name, the digest of its text and the runner's version) or the reference handed back;
//! the contents of each constant set a step selects; the version of each action; and the
//! filler's name, revision, resolved settings, provider and endpoint, with each finder's.
//! An arm's name is not in it, nor a backend's name in the config, nor a file's name:
//! their bytes are. Two cells of one identity in a matrix are refused, as is a recipe
//! that needs anything but a filler and a finder, which a cell's forecast would not
//! count.

use std::collections::BTreeMap;

use serde::Serialize;
use whizbang_core::identity::Digest;

use super::digest_of;
use super::matrix::{Checks, RUNNER_VERSION, Validated, override_of};

/// A backend of the config as a cell sees it: what changes the work, and its price.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Backend {
    /// The model it runs, by name.
    pub name: String,
    pub revision: String,
    /// Its settings, as resolved.
    pub settings: BTreeMap<String, String>,
    /// Who serves it: the config's provider, or its kind when it names none (plan m3,
    /// task L2), so two backends of one model at two providers are two cells.
    pub provider: String,
    /// Where its requests go: the recipient's origin, or empty for a local backend.
    pub endpoint: String,
    /// What one request is forecast to cost, in millionths of a US dollar: a fill's
    /// price per call. Not part of the identity.
    #[serde(skip)]
    pub price_micros: u64,
}

/// The recipe a cell runs, as it runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecipeRun {
    pub name: String,
    pub digest: String,
    pub runner: String,
}

/// Everything a cell's identity covers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Components {
    pub protocol: String,
    /// The corpus picture's digest.
    pub image: String,
    /// Each bound input's digest, by input name.
    pub inputs: BTreeMap<String, String>,
    /// Each bound text, by input name.
    pub texts: BTreeMap<String, String>,
    /// The overrides as applied, the arm's and then the image's, by `<step>.<argument>`:
    /// the line the runner is given.
    pub overrides: BTreeMap<String, String>,
    pub recipe: Option<RecipeRun>,
    /// A reference arm's input, whose digest is among `inputs`.
    pub reference: Option<String>,
    /// The constant set each step selects, by step: its name and the digest of its text.
    pub constants: BTreeMap<String, (String, String)>,
    /// The version of each action the recipe runs, by name.
    pub actions: BTreeMap<String, String>,
    pub filler: Option<Backend>,
    /// Each finder's backend, by step.
    pub finders: BTreeMap<String, Backend>,
}

impl Components {
    /// The identity: the SHA-256 of the components as JSON, in hex.
    pub fn identity(&self) -> String {
        let json = serde_json::to_string(self).unwrap_or_default();
        Digest::of_bytes(json.as_bytes()).hex()
    }
}

/// What a cell does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Work {
    /// A run of the recipe: the corpus picture at `image`, each input bound to a corpus
    /// picture, each text argument, and the overrides as the runner's `set`.
    Recipe {
        recipe: String,
        inputs: BTreeMap<String, String>,
        texts: BTreeMap<String, String>,
        set: String,
    },
    /// Nothing run: the corpus picture handed back.
    Reference { picture: String },
}

/// One cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub arm: String,
    pub image: String,
    pub identity: String,
    pub components: Components,
    pub work: Work,
    /// What one attempt is forecast to cost: the filler's price per call for each fill,
    /// and the finder's for each find; nothing for a reference.
    pub forecast_micros: u64,
}

/// What resolves a cell's backends: the config's backend of this name, or its default for
/// the capability when none is named (`fill` or `segment`). A backend it does not know,
/// or whose price it does not know, it refuses: nothing is priced at nothing by default.
pub type Resolve<'a> = &'a dyn Fn(&str, Option<&str>) -> Result<Backend, String>;

/// What gives a constant set's text by its name.
pub type Constants<'a> = &'a dyn Fn(&str) -> Option<String>;

/// Every cell of a checked matrix, arms then images, in the matrix's order.
pub fn cells(
    validated: &Validated,
    registry: &whizbang_core::contract::Registry,
    load: super::matrix::Load<'_>,
    resolve: Resolve<'_>,
    constants: Constants<'_>,
) -> Result<Vec<Cell>, String> {
    let matrix = &validated.matrix;
    let digest = |id: &str| {
        (validated.corpus.picture(id))
            .map(|p| p.sha256.clone())
            .ok_or_else(|| format!("{id} is not in the corpus"))
    };
    let mut out = Vec::new();
    for (arm, checks) in matrix.arm.iter().zip(&validated.arms) {
        for image in &matrix.image {
            let mut components = Components {
                protocol: validated.protocol_digest.clone(),
                image: digest(&image.id)?,
                inputs: BTreeMap::new(),
                texts: BTreeMap::new(),
                overrides: BTreeMap::new(),
                recipe: None,
                reference: None,
                constants: BTreeMap::new(),
                actions: BTreeMap::new(),
                filler: None,
                finders: BTreeMap::new(),
            };
            let (work, forecast_micros) = match checks {
                Checks::Reference(input) => {
                    let picture = image.inputs[input].clone();
                    components.inputs.insert(input.clone(), digest(&picture)?);
                    components.reference = Some(input.clone());
                    (Work::Reference { picture }, 0)
                }
                Checks::Recipe(recipe) => {
                    if let Some(other) =
                        (recipe.needs.iter()).find(|n| !["fill", "segment"].contains(&n.as_str()))
                    {
                        return Err(format!(
                            "arm {}: its recipe {} needs {other}, which a lab's cell cannot \
                             forecast: a lab's recipe needs a filler and a finder at most; \
                             nothing was run",
                            arm.name, recipe.name
                        ));
                    }
                    let action = load(&recipe.name)?;
                    let checked = action.recipe();
                    let mut inputs = BTreeMap::new();
                    for (name, id) in &image.inputs {
                        if recipe.inputs.get(name).is_some_and(|(text, _)| !text) {
                            components.inputs.insert(name.clone(), digest(id)?);
                            inputs.insert(name.clone(), id.clone());
                        }
                    }
                    let mut texts = BTreeMap::new();
                    for (name, text) in &image.texts {
                        if recipe.inputs.get(name).is_some_and(|(text, _)| *text) {
                            texts.insert(name.clone(), text.clone());
                        }
                    }
                    components.texts = texts.clone();
                    for (key, value) in arm.overrides.iter().chain(&image.overrides) {
                        let line = override_of(checked, registry, key, value)?;
                        components.overrides.insert(key.clone(), line);
                    }
                    let mut forecast: u64 = 0;
                    if recipe.fills > 0 {
                        let filler = resolve("fill", arm.filler.as_deref())?;
                        forecast = forecast
                            .saturating_add(filler.price_micros.saturating_mul(recipe.fills));
                        if let (Some(input), Some(named)) = (&recipe.filler_input, &arm.filler) {
                            texts.insert(input.clone(), named.clone());
                        }
                        components.filler = Some(filler);
                    }
                    if recipe.finds > 0 {
                        let finder = resolve("segment", arm.finder.as_deref())?;
                        forecast = forecast
                            .saturating_add(finder.price_micros.saturating_mul(recipe.finds));
                        if let (Some(input), Some(named)) = (&recipe.finder_input, &arm.finder) {
                            texts.insert(input.clone(), named.clone());
                        }
                        for step in recipe.steps.iter().filter(|s| s.finder) {
                            components.finders.insert(step.id.clone(), finder.clone());
                        }
                    }
                    for step in &recipe.steps {
                        components
                            .actions
                            .insert(step.action.clone(), step.version.clone());
                        let chosen = (components.overrides.get(&format!("{}.constants", step.id)))
                            .and_then(|line| line.split_once('=').map(|(_, set)| set.to_owned()))
                            .or_else(|| step.constants.clone());
                        if let Some(set) = chosen {
                            let text = constants(&set).ok_or_else(|| {
                                format!("the step {} selects {set}, no set of this build", step.id)
                            })?;
                            let held = digest_of(text.as_bytes());
                            components.constants.insert(step.id.clone(), (set, held));
                        }
                    }
                    components.recipe = Some(RecipeRun {
                        name: recipe.name.clone(),
                        digest: recipe.digest.clone(),
                        runner: RUNNER_VERSION.to_owned(),
                    });
                    let set: Vec<&str> =
                        components.overrides.values().map(String::as_str).collect();
                    let work = Work::Recipe {
                        recipe: recipe.name.clone(),
                        inputs,
                        texts,
                        set: set.join("\n"),
                    };
                    (work, forecast)
                }
            };
            let identity = components.identity();
            if let Some(twin) = out.iter().find(|c: &&Cell| c.identity == identity) {
                return Err(format!(
                    "arm {} and arm {} make the same cell of image {}: two arms of one matrix \
                     differ in what they run; nothing was run",
                    twin.arm, arm.name, image.id
                ));
            }
            out.push(Cell {
                arm: arm.name.clone(),
                image: image.id.clone(),
                identity,
                components,
                work,
                forecast_micros,
            });
        }
    }
    Ok(out)
}
