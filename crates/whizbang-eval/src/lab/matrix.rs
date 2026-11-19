//! The lab's matrix, `matrix.toml` (rascal's `lab/matrix.rs`): the corpus and protocol it
//! runs under, the seed, the images with what each binds, and the arms. An arm is a recipe
//! of the build with its overrides, its filler and its finder (backends of the config), or
//! a reference arm, which hands back a picture the matrix binds for each image and runs
//! nothing. [`Matrix::validate`] checks it against its protocol, its corpus and the
//! recipes before anything runs, the dependent-finder rule included.
//!
//! ```toml
//! version = 1
//! name = "m3-seam"
//! corpus = "corpus.toml"        # beside this file
//! protocol = "protocol.toml"    # beside this file
//! seed = 20261031
//!
//! [[image]]
//! id = "sky-03"                             # a corpus id
//! inputs = { protect = "sky-03-protect" }   # a recipe input: a corpus id
//! texts = { prompt = "the tripod" }         # a recipe's text input: the text
//! overrides = { "fan.wide_max" = 0.0 }      # "<step>.<argument>" = value
//!
//! [[arm]]
//! name = "seam-bria"
//! recipe = "seam"                           # a recipe of the build, by name
//! overrides = { "fan.constants" = "fan-v3" }
//! filler = "fal"                            # the config's backend that fills
//!
//! [[arm]]
//! name = "clean"
//! reference = "clean"                       # hands back each image's `inputs.clean`
//! ```

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use whizbang_core::contract::{Action, Capability, Registry};
use whizbang_core::recipe::{
    self, Binding, CONSTANTS, Checked, Declared, RecipeAction, Written, runner,
};

use super::corpus::Corpus;
use super::protocol::Protocol;
use super::{is_name, is_plain_relative};

/// The most images and arms of a matrix, and the most bindings of an image.
pub const IMAGES_LIMIT: usize = 4096;
pub const ARMS_LIMIT: usize = 8;
pub const BINDINGS_LIMIT: usize = 16;
/// The most characters of a text an image binds.
pub const TEXT_LIMIT: usize = 2000;

/// `matrix.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Matrix {
    pub version: u32,
    /// The lab's name; the protocol's must be the same.
    pub name: String,
    /// The corpus manifest, beside this file.
    pub corpus: String,
    /// The protocol, beside this file.
    pub protocol: String,
    /// The seed; the protocol's must be the same.
    pub seed: u64,
    #[serde(default)]
    pub image: Vec<Image>,
    #[serde(default)]
    pub arm: Vec<Arm>,
}

/// One image of the matrix: a corpus id, with what the arms need for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Image {
    pub id: String,
    /// Inputs bound to a picture of the corpus, by the recipe's input name (or a
    /// reference arm's): the corpus id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, String>,
    /// Text inputs of the recipe, by name: the text (`remove_prompt`'s `prompt`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub texts: BTreeMap<String, String>,
    /// This image's overrides, `"<step>.<argument>" = value`, over the arm's.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, toml::Value>,
}

/// One arm: a recipe with its overrides, filler and finder; or a reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Arm {
    pub name: String,
    /// A recipe of the build, by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe: Option<String>,
    /// A reference arm: the input each image binds, handed back with nothing run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    /// `"<step>.<argument>" = value`; `"<step>.constants"` names a constant set.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, toml::Value>,
    /// The config's backend the recipe's fills go to; its default when none is named.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filler: Option<String>,
    /// The config's backend the recipe's finders go to; its default when none is named.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finder: Option<String>,
}

/// One step of an arm's recipe, as a cell's identity needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub id: String,
    pub action: String,
    pub version: String,
    /// The constant set the step selects without an override: its own, else its action's
    /// default; none for an action without sets.
    pub constants: Option<String>,
    /// Whether the step's action needs a segmenter: a finder.
    pub finder: bool,
}

/// What a recipe arm runs, as checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    pub name: String,
    /// The SHA-256 of the recipe's text.
    pub digest: String,
    pub steps: Vec<Step>,
    /// Each declared input, and whether it is text.
    pub inputs: BTreeMap<String, (bool, bool)>,
    /// The text input that chooses the filler, and the one that chooses the finder.
    pub filler_input: Option<String>,
    pub finder_input: Option<String>,
    /// How many requests one run sends to the filler and to the finder.
    pub fills: u64,
    pub finds: u64,
}

/// What an arm is, checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checks {
    Recipe(Recipe),
    Reference(String),
}

/// A matrix checked against its protocol, its corpus and its recipes.
#[derive(Debug, Clone)]
pub struct Validated {
    pub matrix: Matrix,
    pub protocol: Protocol,
    pub protocol_digest: String,
    pub corpus: Corpus,
    /// Each arm as checked, in the matrix's order.
    pub arms: Vec<Checks>,
}

/// What loads a recipe of the build by name, as an action over the registry.
pub type Load<'a> = &'a dyn Fn(&str) -> Result<RecipeAction, String>;

impl Matrix {
    /// A matrix's text, read strictly: version 1, plain names, images and arms present,
    /// unique and within their limits, the corpus and protocol plain files beside it.
    pub fn parse(text: &str) -> Result<Matrix, String> {
        let matrix: Matrix = toml::from_str(text).map_err(|e| format!("the matrix: {e}"))?;
        let refuse = |why: String| Err(format!("the matrix: {why}"));
        if matrix.version != 1 {
            return refuse(format!("version {} is not 1", matrix.version));
        }
        if !is_name(&matrix.name) {
            return refuse("its name is no plain name".to_owned());
        }
        if !is_plain_relative(&matrix.corpus) || !is_plain_relative(&matrix.protocol) {
            return refuse("corpus and protocol are plain relative paths".to_owned());
        }
        if matrix.image.is_empty() || matrix.image.len() > IMAGES_LIMIT {
            return refuse(format!("it has 1 to {IMAGES_LIMIT} images"));
        }
        if matrix.arm.is_empty() || matrix.arm.len() > ARMS_LIMIT {
            return refuse(format!("it has 1 to {ARMS_LIMIT} arms"));
        }
        let mut ids = BTreeSet::new();
        for image in &matrix.image {
            if !is_name(&image.id) || !ids.insert(image.id.as_str()) {
                return refuse("each image is a plain corpus id, once".to_owned());
            }
            let bound = image.inputs.len() + image.texts.len() + image.overrides.len();
            let names = image.inputs.iter().chain(&image.texts).map(|(n, _)| n);
            if bound > BINDINGS_LIMIT || !names.clone().all(|n| is_name(n)) {
                return refuse(format!(
                    "image {}: at most {BINDINGS_LIMIT} bindings, each a plain name",
                    image.id
                ));
            }
            if image.inputs.values().any(|id| !is_name(id)) {
                return refuse(format!(
                    "image {}: an input is bound to no corpus id",
                    image.id
                ));
            }
            if (image.texts.values()).any(|t| t.is_empty() || t.chars().count() > TEXT_LIMIT) {
                return refuse(format!(
                    "image {}: a text is 1 to {TEXT_LIMIT} characters",
                    image.id
                ));
            }
        }
        let mut names = BTreeSet::new();
        for arm in &matrix.arm {
            if !is_name(&arm.name) || !names.insert(arm.name.as_str()) {
                return refuse("each arm has a plain name of its own".to_owned());
            }
            match (&arm.recipe, &arm.reference) {
                (Some(_), None) => {}
                (None, Some(input)) if is_name(input) => {
                    let extra = !arm.overrides.is_empty() || arm.filler.is_some();
                    if extra || arm.finder.is_some() {
                        return refuse(format!(
                            "arm {}: a reference arm runs nothing, so it has no overrides, \
                             filler or finder",
                            arm.name
                        ));
                    }
                }
                _ => {
                    return refuse(format!(
                        "arm {}: an arm names a recipe or a reference, not both",
                        arm.name
                    ));
                }
            }
            if arm.overrides.len() > BINDINGS_LIMIT {
                return refuse(format!(
                    "arm {}: at most {BINDINGS_LIMIT} overrides",
                    arm.name
                ));
            }
        }
        Ok(matrix)
    }

    /// The arm of this name.
    pub fn arm(&self, name: &str) -> Option<&Arm> {
        self.arm.iter().find(|a| a.name == name)
    }

    /// The matrix against its protocol, its corpus and its recipes, before anything runs:
    /// one name and one seed; the arms the protocol compares and no other; every image in
    /// the corpus, and every image the protocol judges in the matrix; every binding one an
    /// arm takes, of the right kind, to a picture of the corpus; every input an arm's
    /// recipe requires bound for every image; every override a step's argument that does
    /// not decide what the step needs, of a value it takes; a filler or a finder only for
    /// a recipe whose caller chooses one; and no finder whose input derives from another
    /// finder's output.
    pub fn validate(
        &self,
        protocol: &Protocol,
        protocol_digest: &str,
        corpus: &Corpus,
        registry: &Registry,
        load: Load<'_>,
    ) -> Result<Validated, String> {
        let refuse = |why: String| Err(format!("matrix {}: {why}", self.name));
        if protocol.name != self.name {
            return refuse(format!(
                "the protocol is {}: one lab, one name",
                protocol.name
            ));
        }
        if protocol.seed != self.seed {
            return refuse(format!(
                "its seed differs from the protocol's {}",
                protocol.seed
            ));
        }
        for arm in &self.arm {
            if !protocol.arms.contains(&arm.name) {
                return refuse(format!(
                    "it runs the arm {}, which the protocol does not compare; nothing was run",
                    arm.name
                ));
            }
        }
        for arm in &protocol.arms {
            if self.arm(arm).is_none() {
                return refuse(format!(
                    "the protocol compares the arm {arm}, which it lacks"
                ));
            }
        }
        for image in &self.image {
            let pictures = std::iter::once(&image.id).chain(image.inputs.values());
            for id in pictures {
                if corpus.picture(id).is_none() {
                    return refuse(format!("image {}: {id} is not in the corpus", image.id));
                }
            }
        }
        for (id, _) in protocol.images() {
            if !self.image.iter().any(|i| i.id == id) {
                return refuse(format!(
                    "the protocol judges the image {id}, which it lacks"
                ));
            }
        }
        let mut arms = Vec::new();
        for arm in &self.arm {
            let checked = match (&arm.recipe, &arm.reference) {
                (Some(name), _) => {
                    let action = load(name).map_err(|why| {
                        format!(
                            "matrix {}: arm {}: recipe {name}: {why}",
                            self.name, arm.name
                        )
                    })?;
                    let recipe = self
                        .check_arm(arm, &action, registry)
                        .map_err(|why| format!("matrix {}: arm {}: {why}", self.name, arm.name))?;
                    Checks::Recipe(recipe)
                }
                (None, Some(input)) => {
                    if let Some(image) = self.image.iter().find(|i| !i.inputs.contains_key(input)) {
                        return refuse(format!(
                            "arm {}: image {} binds no inputs.{input}, which the arm hands back",
                            arm.name, image.id
                        ));
                    }
                    Checks::Reference(input.clone())
                }
                (None, None) => return refuse(format!("arm {}: no recipe", arm.name)),
            };
            arms.push(checked);
        }
        // Every binding is one an arm takes, of its kind.
        for image in &self.image {
            for (name, text) in [(&image.inputs, false), (&image.texts, true)]
                .into_iter()
                .flat_map(|(bound, text)| bound.keys().map(move |n| (n, text)))
            {
                let taken = arms.iter().any(|arm| match arm {
                    Checks::Reference(input) => !text && input == name,
                    Checks::Recipe(r) => r.inputs.get(name).is_some_and(|(t, _)| *t == text),
                });
                if !taken || name == "image" {
                    let kind = if text { "texts" } else { "inputs" };
                    return refuse(format!(
                        "image {}: {kind}.{name} is no input of that kind any arm takes",
                        image.id
                    ));
                }
            }
        }
        Ok(Validated {
            matrix: self.clone(),
            protocol: protocol.clone(),
            protocol_digest: protocol_digest.to_owned(),
            corpus: corpus.clone(),
            arms,
        })
    }

    fn check_arm(
        &self,
        arm: &Arm,
        action: &RecipeAction,
        registry: &Registry,
    ) -> Result<Recipe, String> {
        let checked = action.recipe();
        let mut steps = Vec::new();
        for step in &checked.steps {
            let descriptor = (checked.descriptor_of(step, registry))
                .ok_or_else(|| format!("{} is no action of this build", step.action))?;
            let default = (descriptor.argument(CONSTANTS))
                .and_then(|a| a.default())
                .and_then(|v| match v {
                    whizbang_core::contract::Value::Text(set) => Some(set.clone()),
                    _ => None,
                });
            steps.push(Step {
                id: step.id.clone(),
                action: step.action.clone(),
                version: descriptor.version().to_owned(),
                constants: step.constants.clone().or(default),
                finder: (descriptor.needs().iter()).any(|n| n.capability() == Capability::Segment),
            });
        }
        let chooser = |capability: Capability| {
            (action.descriptor().needs().iter())
                .find(|n| n.capability() == capability)
                .map(|n| (n.chooser().map(str::to_owned), n.sends()))
        };
        let (filler_input, fills) = chooser(Capability::Fill).unwrap_or((None, 0));
        let (finder_input, finds) = chooser(Capability::Segment).unwrap_or((None, 0));
        for (given, input, what) in [
            (&arm.filler, &filler_input, "filler"),
            (&arm.finder, &finder_input, "finder"),
        ] {
            if given.is_some() && input.is_none() {
                return Err(format!(
                    "it names a {what}, and its recipe {} lets no caller choose one",
                    checked.name
                ));
            }
            if given.as_deref().is_some_and(|g| !is_name(g)) {
                return Err(format!("its {what} is no plain name"));
            }
        }
        let inputs: BTreeMap<String, (bool, bool)> = (checked.inputs.iter())
            .map(|(name, i)| (name.clone(), (i.declared == Declared::Text, i.optional)))
            .collect();
        if checked.inputs.get("image").map(|i| i.declared)
            != Some(Declared::Artifact(
                whizbang_core::artifact::ArtifactType::Image,
            ))
        {
            return Err(format!(
                "its recipe {} takes no input image, which the corpus picture is",
                checked.name
            ));
        }
        for (key, value) in &arm.overrides {
            override_of(checked, registry, key, value)?;
        }
        for image in &self.image {
            for (key, value) in &image.overrides {
                override_of(checked, registry, key, value)
                    .map_err(|why| format!("image {}: {why}", image.id))?;
            }
            for (name, (text, optional)) in &inputs {
                let chosen = [&filler_input, &finder_input].contains(&&Some(name.clone()));
                let bound = if *text {
                    image.texts.contains_key(name)
                } else {
                    image.inputs.contains_key(name)
                };
                if name != "image" && !optional && !chosen && !bound {
                    let kind = if *text { "texts" } else { "inputs" };
                    return Err(format!(
                        "image {} binds no {kind}.{name}, which its recipe {} requires",
                        image.id, checked.name
                    ));
                }
            }
        }
        reject_dependent_finders(checked, &steps)
            .map_err(|why| format!("recipe {}: {why}", checked.name))?;
        Ok(Recipe {
            name: checked.name.clone(),
            digest: action.digest().hex(),
            steps,
            inputs,
            filler_input,
            finder_input,
            fills,
            finds,
        })
    }
}

/// An override checked against a recipe: `<step>.<argument>` of a step it has, an
/// argument its action takes that does not decide what the step needs, and a value the
/// argument takes. Gives the line the runner's `set` takes.
pub fn override_of(
    recipe: &Checked,
    registry: &Registry,
    key: &str,
    value: &toml::Value,
) -> Result<String, String> {
    let shown = key.escape_debug().to_string();
    let (step, argument) = (key.split_once('.'))
        .filter(|(s, a)| !s.is_empty() && !a.is_empty() && !a.contains('.'))
        .ok_or_else(|| format!("the override {shown:?} is not <step>.<argument>"))?;
    let found = (recipe.steps.iter().find(|s| s.id == step)).ok_or_else(|| {
        format!(
            "the override {shown:?}: the recipe {} has no step {step}",
            recipe.name
        )
    })?;
    let descriptor = (recipe.descriptor_of(found, registry))
        .ok_or_else(|| format!("{} is no action of this build", found.action))?;
    let taken = descriptor.argument(argument).ok_or_else(|| {
        format!(
            "the override {shown:?}: the step {step} ({}) takes no argument {argument}",
            found.action
        )
    })?;
    let mut decides = (descriptor.needs().iter()).flat_map(|n| [n.chooser(), n.feature_argument()]);
    if decides.any(|named| named == Some(argument)) {
        return Err(format!(
            "the override {shown:?}: {argument} decides what the step needs; an arm names its \
             filler or finder instead"
        ));
    }
    let written = match value {
        toml::Value::Integer(n) => Written::Integer(*n),
        toml::Value::Float(n) => Written::Float(*n),
        toml::Value::Boolean(b) => Written::Boolean(*b),
        toml::Value::String(t) if !t.contains(['\n', '\r']) => Written::Text(t.clone()),
        _ => {
            return Err(format!(
                "the override {shown:?} is a number, a boolean or a text of one line"
            ));
        }
    };
    recipe::typed(taken.value_type(), &written)
        .map_err(|why| format!("the override {shown:?}: {why}"))?;
    let word = match &written {
        Written::Integer(n) => n.to_string(),
        Written::Float(n) => n.to_string(),
        Written::Boolean(b) => b.to_string(),
        Written::Text(t) => t.clone(),
    };
    Ok(format!("{step}.{argument}={word}"))
}

/// The dependent-finder rule (rascal's): a finder whose input binds, through any chain of
/// steps, to a finder's output is refused, naming the binding, the finder and the finder it
/// derives from; a cell's identity, its finders' included, must be known before any finder
/// runs.
pub fn reject_dependent_finders(recipe: &Checked, steps: &[Step]) -> Result<(), String> {
    let mut derives: BTreeMap<&str, &str> = BTreeMap::new();
    for (step, about) in recipe.steps.iter().zip(steps) {
        let mut from: Option<&str> = None;
        for (port, binding) in &step.inputs {
            let Binding::Step { step: source, .. } = binding else {
                continue;
            };
            if let Some(finder) = derives.get(source.as_str()) {
                if about.finder {
                    return Err(format!(
                        "the finder {} ({}) takes {port} = {binding}, which derives from the \
                         finder {finder}; a finder's inputs must be known before any finder runs",
                        step.id, step.action
                    ));
                }
                from = from.or(Some(finder));
            }
        }
        if about.finder {
            derives.insert(&step.id, &step.id);
        } else if let Some(finder) = from {
            derives.insert(&step.id, finder);
        }
    }
    Ok(())
}

/// The version of the runner every recipe arm runs on.
pub const RUNNER_VERSION: &str = runner::VERSION;
