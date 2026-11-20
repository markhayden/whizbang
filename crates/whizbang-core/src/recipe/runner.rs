//! The runner of recipes (ADR 0033): a checked recipe as an action of the kind
//! [`Kind::Recipe`], whose run calls each step's action through the executor in its
//! context, in the order the recipe lists them, with the values each step committed
//! passed by reference, their identities, to the steps after it.
//!
//! **One reservation.** The recipe's descriptor needs what its steps' actions need, so
//! the recipe's run is admitted with a reservation that covers every step, and each step
//! is a nested call whose share is taken from the recipe run's allowance (ADR 0008,
//! "Nested calls"; ADR 0016): nothing is reserved twice. A step's run is a child of the
//! recipe's, and the recipe's record lists each step in order: its id, the action, the
//! step's run, which is the reference to its result, how it ended, the arguments the
//! recipe gave it and its constant set (`run/5`).
//!
//! **Cancelling.** A step's run is under the recipe's signal, so cancelling the recipe
//! cancels the step that runs; the runner looks at the signal before each step, and starts
//! no other once it is set.
//!
//! **What a recipe calls.** The run freezes its calls to its steps' actions (ADR 0009):
//! it calls nothing else, and gives a step nothing but the recipe's own inputs and what an
//! earlier step committed. A recipe file names actions of the build and holds no code, so
//! it runs nothing the build does not.
//!
//! **Its own arguments.** Beside an argument for each text input, a recipe takes
//! [`SET`], the overrides of one run, one to a line, each `<step>.<argument>=<value>`
//! (`<step>.constants=<set>` for a step's constant set), refused naming the step before
//! anything runs when the recipe has no such step or the step's action no such argument;
//! and [`UNTIL`], the id of a step before the last: the recipe runs up to and including
//! that step. Its own outputs apply only without it; with it, the recipe hands back that
//! step's outputs as its own, at the ports [`until_port`] names, so that the recipe's run,
//! which nothing called, commits them and holds them outside runs: what it prints is an
//! artifact any later call takes, until the recipe's run is deleted (plan m3, task Q1's
//! third pull request: a step's own outputs are names of the recipe's run alone). A
//! capability only the last step needs is needed only without it, so `until` reserves
//! nothing for a step it never reaches (plan m3, task F1, item 6). A recipe of one step
//! has no `until`.
//!
//! **A run from a step** (ADR 0033, "A run from a step"). [`FROM`] is the mirror of
//! `until`: the id of a step after the first. The recipe runs that step and those after
//! it; the steps before it do not run. In place of their outputs the call gives
//! artifacts, at the input ports [`from_port`] names, one port for each output port of
//! each step before the last; a later step, and an output of the recipe, bound to an
//! output of a step that did not run takes what was given there. `from` and `until` may
//! be given together, so a caller runs one stretch of a recipe. The run is invalid, before
//! any step runs, when `from` comes after `until`, when something is given at a `from`
//! port without `from` or for a step that runs, and when a step of the stretch, or an
//! output the recipe hands back, needs an output of a step before it that was not given,
//! or what such a step measured, which nobody can give. A given artifact that is not of
//! its port's type is refused when the call is prepared, as any input is. The recipe
//! plans such a call (ADR 0004, "A call's plan"): its plan states, for each capability,
//! the requests of the steps of the stretch alone, so nothing is reserved for a step that
//! does not run and a capability only such steps need is not resolved. The features asked
//! of a capability stay those of every step that names it. The run's record says where it
//! started and what it was given (`run/6`). The recipe's own inputs are given as for any
//! run of it, those only a step before `from` takes among them: what a descriptor's port
//! or argument requires does not depend on another argument. A recipe of one step has no
//! `from`.
//!
//! **Its file.** The digest of the recipe's text is a setting of every run of it,
//! [`DIGEST_SETTING`]: a run says which file it ran. A step whose action takes the
//! argument [`DIGEST_ARGUMENT`] is given it too, so what the step keeps of its call (a
//! key of the decision cache, ADR 0031) changes with the file.
//!
//! **A step that found nothing.** A step's port that takes at least one artifact, bound
//! to an output where an earlier step of the same run committed none, ends the run as
//! invalid before the step is called, saying which step found nothing (plan m4, task
//! E1): `remove_prompt` given a phrase that names nothing in the picture says that its
//! step `segment` found nothing, not that `select` was given 0 masks.
//!
//! **What is not an artifact** (ADR 0035). A step's argument bound to an earlier step's
//! measurements is given them as one JSON text, as that step's run handed them back; and
//! the verdict of a last step that answers with one is the recipe's, handed back with its
//! outcome, and so are the findings of a last step that declares them.

use std::collections::{BTreeMap, BTreeSet};

use crate::artifact::{self, ArtifactId};
use crate::contract::record::{GIVEN_LIMIT, Given, Started};
use crate::contract::{
    Action, ActionDescriptor, Argument, Call, CallError, Capability, Context, Count, Failure,
    Findings, InputPort, Kind, Outcome, OutputPort, Plan, Planning, PreparedCall, Produced,
    Registry, Requirement, ResultKind, Value, ValueType, Verdict, When,
};
use crate::identity::{DecodeLimits, Digest};

use super::{Binding, Checked, Declared, Param, RecipeError, typed_word};

/// The runner's argument that holds the overrides of one run.
pub const SET: &str = "set";

/// The runner's argument that names the last step to run.
pub const UNTIL: &str = "until";

/// The port at which a recipe run with [`UNTIL`] hands back the output `port` of its step
/// `step`: `until_<step>_<port>`.
pub fn until_port(step: &str, port: &str) -> String {
    format!("{UNTIL}_{step}_{port}")
}

/// The runner's argument that names the first step to run.
pub const FROM: &str = "from";

/// The port at which a recipe run with [`FROM`] is given, in place of the output `port`
/// of its step `step`, which does not run, the artifacts a later step takes from it:
/// `from_<step>_<port>`.
pub fn from_port(step: &str, port: &str) -> String {
    format!("{FROM}_{step}_{port}")
}

/// The version of every recipe's descriptor: the runner's. What a recipe file says is
/// pinned by its digest, [`DIGEST_SETTING`].
pub const VERSION: &str = "1.0.0";

/// The setting each run of a recipe notes: the digest of the recipe's text.
pub const DIGEST_SETTING: &str = "recipe.digest";

/// The argument a step's action may take to be given the digest of the recipe's text.
pub const DIGEST_ARGUMENT: &str = "recipe_digest";

/// A recipe, checked, as an action.
pub struct RecipeAction {
    descriptor: ActionDescriptor,
    recipe: Checked,
    digest: Digest,
    /// The descriptor of each step's action, in the order of the steps.
    steps: Vec<ActionDescriptor>,
    /// What each step sends each capability, in the order of the steps: what the plan
    /// of a run from a step is counted from.
    sends: Vec<Vec<(Capability, u64)>>,
}

impl RecipeAction {
    /// The action of this recipe, checked against `registry`, whose text was `text`.
    pub fn new(
        recipe: Checked,
        text: &str,
        registry: &Registry,
    ) -> Result<RecipeAction, RecipeError> {
        let refuse = |why: String| RecipeError {
            recipe: recipe.name.clone(),
            step: "-".to_owned(),
            why,
        };
        let mut steps = Vec::new();
        for step in &recipe.steps {
            let descriptor = registry
                .get(&step.action)
                .ok_or_else(|| refuse(format!("{} is no action of this build", step.action)))?;
            steps.push(descriptor.descriptor().clone());
        }
        let mut draft =
            ActionDescriptor::draft(&recipe.name, VERSION, Kind::Recipe, &recipe.purpose)
                .use_when(&recipe.when);
        for (name, input) in &recipe.inputs {
            let purpose = format!("The input {name} of the recipe.");
            if let Declared::Artifact(artifact) = input.declared
                && input.many
            {
                let least = u32::from(!input.optional);
                let port = InputPort::variadic(name, artifact, &purpose, Count::at_least(least));
                draft = draft.input(port);
                continue;
            }
            draft = match (input.declared, input.optional) {
                (Declared::Text, false) => {
                    draft.argument(Argument::required(name, &purpose, ValueType::Text))
                }
                (Declared::Text, true) => {
                    draft.argument(Argument::optional(name, &purpose, ValueType::Text))
                }
                (Declared::Artifact(artifact), false) => {
                    draft.input(InputPort::required(name, artifact, &purpose))
                }
                (Declared::Artifact(artifact), true) => {
                    draft.input(InputPort::optional(name, artifact, &purpose))
                }
            };
        }
        // `until` names a step before the last: running to the last is running the
        // recipe. A recipe of one step has no `until`.
        let ids: Vec<&str> = recipe.steps.iter().map(|step| step.id.as_str()).collect();
        let before_last = &ids[..ids.len().saturating_sub(1)];
        let until = !before_last.is_empty();
        draft = draft.argument(Argument::optional(
            SET,
            "Overrides for this run, one to a line: a step, a dot, an argument, an equals sign and a value.",
            ValueType::Text,
        ));
        if until {
            draft = draft.argument(Argument::optional(
                FROM,
                "Runs the recipe from this step, after its first, given artifacts in place of the outputs of the steps before it.",
                ValueType::choice(&ids[1..]),
            ));
            // With `from`, what stands in place of each output of a step that does not
            // run: any step before the last may be one.
            for (step, descriptor) in recipe.steps.iter().zip(&steps).take(before_last.len()) {
                for port in descriptor.outputs() {
                    let most = (port.count().max()).ok_or_else(|| {
                        refuse(format!("{} of {} has no bound", port.name(), step.id))
                    })?;
                    let purpose = format!(
                        "With from, what is given in place of the output {} of the step {}.",
                        port.name(),
                        step.id
                    );
                    draft = draft.input(InputPort::variadic(
                        &from_port(&step.id, port.name()),
                        port.artifact(),
                        &purpose,
                        Count::between(0, most),
                    ));
                }
            }
            draft = draft.plans();
            draft = draft.argument(Argument::optional(
                UNTIL,
                "Runs the recipe up to and including this step, before its last, and hands back that step's outputs as its own.",
                ValueType::choice(before_last),
            ));
        }
        for output in &recipe.outputs {
            let purpose = format!("The output {name} of the recipe.", name = output.name);
            let port = if output.many {
                let most = match &output.binding {
                    Binding::Step { step, port } => recipe
                        .steps
                        .iter()
                        .position(|s| s.id == *step)
                        .and_then(|at| steps[at].outputs().iter().find(|p| p.name() == port))
                        .and_then(|p| p.count().max()),
                    Binding::Input(_) => Some(1),
                };
                let most = most.ok_or_else(|| refuse(format!("{} has no bound", output.name)))?;
                OutputPort::counted(
                    &output.name,
                    output.artifact,
                    &purpose,
                    Count::between(0, most),
                )
            } else {
                OutputPort::one(&output.name, output.artifact, &purpose)
            };
            draft = draft.output(if until {
                port.when(When::not_given(UNTIL))
            } else {
                port
            });
        }
        // With `until`, the outputs of the step it names, each at a port of its own.
        for (step, descriptor) in recipe.steps.iter().zip(&steps).take(before_last.len()) {
            for port in descriptor.outputs() {
                let most = (port.count().max()).ok_or_else(|| {
                    refuse(format!("{} of {} has no bound", port.name(), step.id))
                })?;
                let purpose = format!(
                    "With until {}, the output {} of that step.",
                    step.id,
                    port.name()
                );
                draft = draft.output(
                    OutputPort::counted(
                        &until_port(&step.id, port.name()),
                        port.artifact(),
                        &purpose,
                        Count::between(0, most),
                    )
                    .when(When::given(UNTIL)),
                );
            }
        }
        // A descriptor states one need of a capability: the steps' needs of one are one
        // need of the recipe, for as many requests as they send together, with every
        // feature any of them asks, and chosen by the recipe's input that chooses each of
        // them, where one does.
        // A capability only the last step needs is needed only without `until`, which
        // stops before it (plan m3, task F1, item 6: `--until feather` asks no filler).
        let mut merged: Vec<(Requirement, Option<String>)> = Vec::new();
        let mut before: BTreeSet<Capability> = BTreeSet::new();
        let mut sends = Vec::new();
        let last = recipe.steps.len().saturating_sub(1);
        for (at, (step, descriptor)) in recipe.steps.iter().zip(&steps).enumerate() {
            let needs = needs_of(&recipe, step, descriptor);
            sends.push((needs.iter().map(|(n, _)| (n.capability(), n.sends()))).collect());
            if at < last {
                before.extend(needs.iter().map(|(need, _)| need.capability()));
            }
            for (need, chooser) in needs_of(&recipe, step, descriptor) {
                match merged
                    .iter_mut()
                    .find(|(held, _)| held.capability() == need.capability())
                {
                    None => merged.push((need, chooser)),
                    Some((held, chosen)) => {
                        let mut joined = Requirement::new(held.capability())
                            .requests(held.sends().saturating_add(need.sends()));
                        let mut features = held.features().to_vec();
                        features.extend(need.features().iter().cloned());
                        features.sort();
                        features.dedup();
                        for feature in &features {
                            joined = joined.feature(feature);
                        }
                        if *chosen != chooser {
                            *chosen = None;
                        }
                        *held = joined;
                    }
                }
            }
        }
        for (need, chooser) in merged {
            let need = if until && !before.contains(&need.capability()) {
                need.when(When::not_given(UNTIL))
            } else {
                need
            };
            draft = draft.needs(match chooser {
                Some(input) => need.chosen_by(&input),
                None => need,
            });
        }
        if recipe.verdict {
            draft = draft.results(ResultKind::Verdict);
        }
        if recipe.findings {
            draft = draft.results(ResultKind::Findings);
        }
        let descriptor = draft
            .concurrent(true)
            .build()
            .map_err(|error| refuse(error.why))?;
        Ok(RecipeAction {
            descriptor,
            recipe,
            digest: Digest::of_bytes(text.as_bytes()),
            steps,
            sends,
        })
    }

    /// The recipe this runs.
    pub fn recipe(&self) -> &Checked {
        &self.recipe
    }

    /// The digest of the recipe's text.
    pub fn digest(&self) -> Digest {
        self.digest
    }
}

/// What a step's action needs, as the recipe's descriptor states it: each need of the
/// action for what the step gives, with the features it asks, the one its argument names
/// where the step gives a value or the action a default, and the recipe's text input
/// that chooses the backend where the step binds the action's chooser to one a caller may
/// leave out.
fn needs_of(
    recipe: &Checked,
    step: &super::CheckedStep,
    descriptor: &ActionDescriptor,
) -> Vec<(Requirement, Option<String>)> {
    let given: BTreeSet<String> = step
        .arguments
        .iter()
        .map(|(name, _)| name.clone())
        .chain(step.inputs.iter().map(|(port, _)| port.clone()))
        .collect();
    let value_of = |name: &str| -> Option<Value> {
        match step.arguments.iter().find(|(n, _)| n == name) {
            Some((_, Param::Value(value))) => Some(value.clone()),
            Some((_, Param::Input(_) | Param::Measured(_))) => None,
            None => descriptor.argument(name).and_then(|a| a.default().cloned()),
        }
    };
    let mut needs = Vec::new();
    for need in descriptor.needs_for(&given) {
        let mut stated = Requirement::new(need.capability()).requests(need.sends());
        for feature in need.features() {
            stated = stated.feature(feature);
        }
        if let Some(Value::Text(feature)) = need.feature_argument().and_then(value_of)
            && !need.features().contains(&feature)
        {
            stated = stated.feature(&feature);
        }
        let chooser = need.chooser().and_then(|chooser| {
            match step.arguments.iter().find(|(n, _)| n == chooser) {
                Some((_, Param::Input(input))) => Some(input.as_str()),
                _ => None,
            }
        });
        let chooser = chooser
            .filter(|input| {
                recipe
                    .inputs
                    .get(*input)
                    .is_some_and(|input| input.optional)
            })
            .map(str::to_owned);
        needs.push((stated, chooser));
    }
    needs
}

/// One override of one run: the step's place, the argument, the value.
type Override = (usize, String, Value);

impl RecipeAction {
    /// The overrides `set` holds, each refused naming the step, before anything runs.
    fn overrides(&self, set: &str) -> Result<Vec<Override>, String> {
        let mut overrides = Vec::new();
        for line in set.lines().filter(|line| !line.trim().is_empty()) {
            let (named, word) = line
                .split_once('=')
                .ok_or_else(|| format!("the override {line:?} is not <step>.<argument>=<value>"))?;
            let (step, argument) = named
                .split_once('.')
                .ok_or_else(|| format!("the override {line:?} is not <step>.<argument>=<value>"))?;
            let at = (self.recipe.steps.iter().position(|s| s.id == step)).ok_or_else(|| {
                format!(
                    "the override {line:?} names the step {step:?}, and the recipe {} has no \
                     such step",
                    self.recipe.name
                )
            })?;
            let descriptor = &self.steps[at];
            let action = descriptor.name();
            let taken = descriptor.argument(argument).ok_or_else(|| {
                if argument == super::CONSTANTS {
                    format!("the override {line:?}: the step {step} ({action}) has no constant sets")
                } else {
                    format!("the override {line:?}: the step {step} ({action}) takes no argument {argument}")
                }
            })?;
            // What a step needs was stated in the recipe's descriptor and reserved for
            // when the recipe was admitted: an override that would change it, the
            // argument that chooses a need's backend or names one of its features, is
            // refused here, before any step runs.
            let needs = descriptor.needs().iter();
            let mut chooses = needs.flat_map(|need| [need.chooser(), need.feature_argument()]);
            if taken.takes_measurements() {
                return Err(format!(
                    "the override {line:?}: the argument {argument} of the step {step} ({action}) \
                     takes only what an earlier step measured, and is not overridden"
                ));
            }
            if argument == DIGEST_ARGUMENT || chooses.any(|named| named == Some(argument)) {
                return Err(format!(
                    "the override {line:?}: the argument {argument} of the step {step} ({action}) \
                     decides what it needs, which the recipe was admitted with, and is not \
                     overridden"
                ));
            }
            let value = typed_word(taken.value_type(), word)
                .map_err(|why| format!("the override {line:?}: the step {step}: {why}"))?;
            overrides.push((at, argument.to_owned(), value));
        }
        Ok(overrides)
    }
}

/// The text of an argument of the call, when it has one.
fn text_of<'a>(call: &'a PreparedCall, name: &str) -> Option<&'a str> {
    call.arguments().iter().find_map(|(n, value)| match value {
        Value::Text(text) if n == name => Some(text.as_str()),
        _ => None,
    })
}

fn stopped(context: &Context, name: &str) -> Failure {
    if context.cancellation().is_cancelled() {
        Failure::error(
            "cancelled",
            &format!("the recipe {name} stopped: its run was cancelled"),
        )
    } else {
        Failure::error(
            "deadline",
            &format!("the recipe {name} stopped: its deadline passed"),
        )
    }
}

impl RecipeAction {
    /// The place of the step an argument of the call names, when it names one.
    fn place(&self, named: Option<&Value>) -> Option<usize> {
        let Some(Value::Text(id)) = named else {
            return None;
        };
        self.recipe.steps.iter().position(|step| step.id == *id)
    }

    /// Whether a step of this run, earlier than the one at `at`, found nothing that the
    /// step at `at` needs: an input port that takes at least one artifact, bound to an
    /// output of a step that ran in this run (at `first` or after) and committed none
    /// there. Said in a sentence of its own, naming the step that found nothing, rather
    /// than as the next step's refusal of a count (plan m4, task E1: a phrase that names
    /// nothing in the picture, in `remove_prompt`).
    fn found_nothing(
        &self,
        at: usize,
        first: usize,
        ids: impl Fn(&Binding) -> Vec<ArtifactId>,
    ) -> Option<String> {
        let steps = &self.recipe.steps;
        let step = &steps[at];
        step.inputs.iter().find_map(|(port, binding)| {
            let Binding::Step { step: of, .. } = binding else {
                return None;
            };
            let ran = (steps.iter().position(|s| s.id == *of))
                .filter(|&earlier| earlier >= first && earlier < at)?;
            let needed = self.steps[at].input(port)?.count().min() > 0;
            (needed && ids(binding).is_empty()).then(|| {
                format!(
                    "the step {of} ({}) found nothing, so the step {} ({}) had no {port} to take",
                    steps[ran].action, step.id, step.action
                )
            })
        })
    }

    /// What is wrong with a run from the step at `first` to the one at `last`, given
    /// these artifacts at the `from` ports, by step and port: none when every step of
    /// the stretch, and every output the recipe then hands back, has what it takes.
    fn start_fault(
        &self,
        first: usize,
        last: usize,
        given: &BTreeMap<&str, BTreeMap<String, Vec<ArtifactId>>>,
    ) -> Option<String> {
        let steps = &self.recipe.steps;
        let before = |id: &str| steps[..first].iter().any(|step| step.id == id);
        if let Some(step) = given.keys().find(|step| !before(step)) {
            return Some(format!(
                "an artifact is given in place of an output of the step {step}, which is \
                 not before the step the run starts from"
            ));
        }
        let has = |step: &str, port: &str| {
            let at = given.get(step).and_then(|by| by.get(port));
            at.is_some_and(|ids| !ids.is_empty())
        };
        for (step, descriptor) in steps.iter().zip(&self.steps).take(last + 1).skip(first) {
            for (port, binding) in &step.inputs {
                let Binding::Step {
                    step: of,
                    port: out,
                } = binding
                else {
                    continue;
                };
                // Whether the port takes it or may be left unbound, a whole run binds
                // what the step before made: a run from a step leaves it unbound only
                // where that step could have made nothing there.
                let made = steps.iter().zip(&self.steps).find(|(s, _)| s.id == *of);
                let made = made.and_then(|(_, d)| d.output(out));
                let may_be_none = made.is_some_and(|p| p.count().min() == 0);
                let needed = descriptor.input(port).is_some_and(|p| p.count().min() > 0);
                if before(of) && (needed || !may_be_none) && !has(of, out) {
                    return Some(format!(
                        "the step {} takes the output {out} of the step {of}, which does \
                         not run, and nothing was given in its place: give it at {}",
                        step.id,
                        from_port(of, out)
                    ));
                }
            }
            let measured = step.arguments.iter().find_map(|(_, param)| match param {
                Param::Measured(of) if before(of) => Some(of),
                _ => None,
            });
            if let Some(of) = measured {
                return Some(format!(
                    "the step {} takes what the step {of} measured, and that step does \
                     not run",
                    step.id
                ));
            }
        }
        if last + 1 < steps.len() {
            return None;
        }
        (self.recipe.outputs.iter()).find_map(|output| match &output.binding {
            Binding::Step { step, port } if before(step) && !output.many && !has(step, port) => {
                Some(format!(
                    "the recipe hands back {} from the output {port} of the step {step}, \
                     which does not run, and nothing was given in its place: give it at {}",
                    output.name,
                    from_port(step, port)
                ))
            }
            _ => None,
        })
    }
}

impl Action for RecipeAction {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    /// The plan of a run from a step: for each capability the call could need, the
    /// requests of the steps of the stretch alone. A call without `from` plans
    /// nothing, and is prepared as the descriptor states.
    fn plan(&self, planning: &Planning<'_>) -> Option<Plan> {
        let first = self.place(planning.argument(FROM))?;
        let last = match planning.argument(UNTIL) {
            Some(until) => self.place(Some(until))?,
            None => self.recipe.steps.len() - 1,
        };
        // A stretch that ends before it starts runs nothing: the run says so.
        let stretch = self.sends.get(first..=last)?;
        let mut plan = Plan::new();
        for able in planning.capabilities() {
            let of = |(capability, sends): &(Capability, u64)| {
                (*capability == able.capability()).then_some(*sends)
            };
            let sends = stretch.iter().flatten().filter_map(of);
            plan = plan.sending(able.capability(), sends.fold(0, u64::saturating_add));
        }
        Some(plan)
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let name = &self.recipe.name;
        context
            .facts()
            .setting(DIGEST_SETTING, &Value::Text(self.digest.to_string()));
        let overrides = self
            .overrides(text_of(call, SET).unwrap_or(""))
            .map_err(|why| Failure::invalid(&why))?;
        let last = match text_of(call, UNTIL) {
            Some(until) => {
                let at =
                    (self.recipe.steps.iter().position(|s| s.id == until)).ok_or_else(|| {
                        Failure::invalid(&format!("the recipe {name} has no step {until}"))
                    })?;
                // Its descriptor offers only the steps before the last, and the runner
                // holds to that itself: a need only the last step has was not reserved
                // for a run with `until`.
                if at + 1 == self.recipe.steps.len() {
                    return Err(Failure::invalid(&format!(
                        "until names {until}, the last step of the recipe {name}: leave until out to run it whole"
                    )));
                }
                at
            }
            None => self.recipe.steps.len() - 1,
        };
        context.freeze_steps(self.recipe.steps.iter().map(|step| step.action.as_str()));

        let mut given: BTreeMap<&str, Vec<ArtifactId>> = BTreeMap::new();
        for input in call.inputs() {
            given.entry(input.port()).or_default().push(input.id());
        }
        // A run from a step (`from`): what was given in place of the outputs of the
        // steps before it, by step and port, and held to what the stretch takes
        // before any step runs.
        let mut made: BTreeMap<&str, BTreeMap<String, Vec<ArtifactId>>> = BTreeMap::new();
        let mut handed = Vec::new();
        for (step, descriptor) in self.recipe.steps.iter().zip(&self.steps) {
            for port in descriptor.outputs() {
                let at = from_port(&step.id, port.name());
                let Some(ids) = given.get(at.as_str()) else {
                    continue;
                };
                handed.extend(ids.iter().map(|id| Given {
                    step: step.id.clone(),
                    port: port.name().to_owned(),
                    artifact_type: port.artifact(),
                    artifact: id.0,
                }));
                let by_port = made.entry(step.id.as_str()).or_default();
                by_port.insert(port.name().to_owned(), ids.clone());
            }
        }
        let first = match text_of(call, FROM) {
            Some(from) => {
                let at = self.place(Some(&Value::text(from))).ok_or_else(|| {
                    Failure::invalid(&format!("the recipe {name} has no step {from}"))
                })?;
                if at > last {
                    return Err(Failure::invalid(&format!(
                        "from names {from}, which comes after the step the run stops at, \
                         in the recipe {name}"
                    )));
                }
                // The record says what the run was given, and lists so many.
                if handed.len() > GIVEN_LIMIT {
                    return Err(Failure::invalid(&format!(
                        "the recipe {name} cannot run from the step {from}: {} artifacts \
                         are given in place of steps' outputs, and a run from a step is \
                         given {GIVEN_LIMIT} at most",
                        handed.len()
                    )));
                }
                if let Some(fault) = self.start_fault(at, last, &made) {
                    return Err(Failure::invalid(&format!(
                        "the recipe {name} cannot run from the step {from}: {fault}"
                    )));
                }
                let noted = context.facts().started(Started {
                    from: from.to_owned(),
                    given: handed,
                });
                // A context no executor made keeps no facts; a run's always takes it.
                let _ = noted;
                at
            }
            None if !handed.is_empty() => {
                return Err(Failure::invalid(&format!(
                    "the recipe {name} was given an artifact in place of a step's output, \
                     and from names no step to run from"
                )));
            }
            None => 0,
        };
        let ids = |binding: &Binding, made: &BTreeMap<&str, BTreeMap<String, Vec<ArtifactId>>>| {
            match binding {
                Binding::Input(input) => given.get(input.as_str()).cloned().unwrap_or_default(),
                Binding::Step { step, port } => made
                    .get(step.as_str())
                    .and_then(|outputs| outputs.get(port))
                    .cloned()
                    .unwrap_or_default(),
            }
        };

        let mut measured: BTreeMap<&str, String> = BTreeMap::new();
        let mut verdict: Option<Verdict> = None;
        let mut findings: Option<Findings> = None;
        let stretch = self.recipe.steps.iter().enumerate();
        for (at, step) in stretch.take(last + 1).skip(first) {
            if context.cancellation().is_cancelled() || context.deadline_passed() {
                return Err(stopped(context, name));
            }
            let mut arguments: Vec<(String, Value)> = Vec::new();
            for (argument, param) in &step.arguments {
                let value = match param {
                    Param::Value(value) => Some(value.clone()),
                    // An absent optional text leaves the argument out.
                    Param::Input(input) => text_of(call, input).map(Value::text),
                    // What the earlier step measured, as its run handed it back; a step
                    // that handed back none gives an empty document.
                    Param::Measured(from) => Some(Value::text(
                        measured.get(from.as_str()).map_or("null", String::as_str),
                    )),
                };
                arguments.extend(value.map(|value| (argument.clone(), value)));
            }
            for (_, argument, value) in overrides.iter().filter(|(of, _, _)| *of == at) {
                arguments.retain(|(name, _)| name != argument);
                arguments.push((argument.clone(), value.clone()));
            }
            let constants = arguments.iter().find_map(|(name, value)| match value {
                Value::Text(set) if name == super::CONSTANTS => Some(set.clone()),
                _ => None,
            });
            if self.steps[at].argument(DIGEST_ARGUMENT).is_some() {
                arguments.retain(|(name, _)| name != DIGEST_ARGUMENT);
                arguments.push((
                    DIGEST_ARGUMENT.to_owned(),
                    Value::text(&self.digest.to_string()),
                ));
            }
            if let Some(fault) = self.found_nothing(at, first, |binding| ids(binding, &made)) {
                return Err(Failure::invalid(&fault));
            }
            let mut asked = Call::new(&step.action);
            for (argument, value) in &arguments {
                asked = asked.argument(argument, value.clone());
            }
            for (port, binding) in &step.inputs {
                // An absent optional input leaves the port unbound.
                for id in ids(binding, &made) {
                    asked = asked.input(port, id);
                }
            }
            context
                .events()
                .progress(&format!("{name}: step {} ({})", step.id, step.action));
            let finished = context
                .executor()
                .submit(&asked)
                .map_err(|error| match error {
                    CallError::Refused(refusal) => Failure::invalid(&format!(
                        "the step {} ({}) was refused: {refusal}",
                        step.id, step.action
                    )),
                    other => Failure::error(
                        "step",
                        &format!(
                            "the step {} ({}) could not be run: {other}",
                            step.id, step.action
                        ),
                    ),
                })?;
            let labelled: Vec<(String, Value)> = arguments
                .into_iter()
                .filter(|(name, _)| name != super::CONSTANTS)
                .collect();
            context.label_step(finished.run(), &step.id, &labelled, constants.as_deref());
            match finished.outcome() {
                Outcome::Completed { outputs } => {
                    let mut by_port: BTreeMap<String, Vec<ArtifactId>> = BTreeMap::new();
                    for output in outputs {
                        by_port
                            .entry(output.port().to_owned())
                            .or_default()
                            .push(output.id());
                    }
                    made.insert(step.id.as_str(), by_port);
                    if let Some(measurements) = finished.measurements() {
                        measured.insert(step.id.as_str(), measurements.to_string());
                    }
                    verdict = finished.verdict().cloned();
                    findings = finished.findings().cloned();
                }
                Outcome::Cancelled => return Err(stopped(context, name)),
                Outcome::Error { kind, message } => {
                    return Err(Failure::error(
                        "step",
                        &format!(
                            "the step {} ({}) ended as an error of the type {kind}: {message}",
                            step.id, step.action
                        ),
                    ));
                }
                Outcome::Invalid { why } => {
                    return Err(Failure::invalid(&format!(
                        "the step {} ({}) could not work on what it was given: {why}",
                        step.id, step.action
                    )));
                }
            }
        }
        let limits = DecodeLimits::default();
        if last + 1 < self.recipe.steps.len() {
            let step = &self.recipe.steps[last];
            context
                .events()
                .progress(&format!("{name}: ran until the step {}", step.id));
            // The step's outputs, as the recipe's own: committed by the recipe's run and
            // held by it outside runs.
            let mut produced = Produced::nothing();
            for port in self.steps[last].outputs() {
                let at = until_port(&step.id, port.name());
                let made = made
                    .get(step.id.as_str())
                    .and_then(|by| by.get(port.name()));
                for id in made.into_iter().flatten() {
                    let read = artifact::get(context.store(), port.artifact(), *id, &limits)
                        .map_err(|error| Failure::error("output", &error.without_paths()))?;
                    produced = produced.with(&at, read);
                }
            }
            return Ok(produced);
        }
        let mut produced = Produced::nothing();
        if self.recipe.verdict
            && let Some(verdict) = verdict
        {
            produced = produced.with_verdict(verdict);
        }
        if self.recipe.findings
            && let Some(findings) = findings
        {
            produced = produced.with_findings(findings.to_json());
        }
        for output in &self.recipe.outputs {
            for id in ids(&output.binding, &made) {
                let read = artifact::get(context.store(), output.artifact, id, &limits)
                    .map_err(|error| Failure::error("output", &error.without_paths()))?;
                produced = produced.with(&output.name, read);
            }
        }
        Ok(produced)
    }
}
