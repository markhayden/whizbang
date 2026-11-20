//! The runner of recipes (plan m3, task R1, items 2 and 3; ADR 0033): each step a nested
//! call, a child of the recipe's run, on the recipe's one reservation; values passed
//! between steps by their identities; an absent optional input left unbound; a cancelled
//! recipe cancels the step that runs and starts no other; overrides refused before
//! anything runs when they name no step; and a run until a step hands back that step's
//! outputs. Over the fakes of `recipe`, and a store of the tests' own.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use whizbang_core::artifact::{self, Artifact, ArtifactId, ArtifactType, Mask};
use whizbang_core::contract::record::{self, RunRecord, Status, StepKind};
use whizbang_core::contract::request::with_descendants;
use whizbang_core::contract::{
    Action, ActionDescriptor, Backends, Call, Cancellation, Capability, Context, Engine, Estimate,
    Executor, Failure, InputPort, Kind, ModelIdentity, Outcome, OutputPort, Produced, Request,
    Unavailable, Value,
};
use whizbang_core::identity::{DecodeLimits, Digest};
use whizbang_core::recipe::runner::DIGEST_SETTING;
use whizbang_core::store::fs::Fs;
use whizbang_core::store::{Reader, Store};
use whizbang_testkit::fs_shim::ShimFs;
use whizbang_testkit::ledger::FakeLedger;
use whizbang_testkit::scratch::Scratch;

use crate::artifact::support::{on, rect_mask};
use crate::recipe::{Fake, TWO_MARKS, action_of, fakes, mask_at, recipe};

/// A store of the test's own, on the shim.
pub struct World {
    dir: Scratch,
    pub fs: Arc<ShimFs>,
    pub store: Arc<Store>,
}

impl World {
    pub fn new(label: &str) -> World {
        let dir = Scratch::new(label);
        let fs = Arc::new(ShimFs::new());
        let store = Arc::new(Store::open(&dir.path().join("store"), fs.clone()).unwrap());
        World { dir, fs, store }
    }

    pub fn root(&self) -> std::path::PathBuf {
        self.dir.path().join("store")
    }

    /// Puts a mask in the store, as an import does.
    pub fn put(&self, mask: Mask) -> ArtifactId {
        let mut publication = self.store.begin().unwrap();
        let id = artifact::put(&mut publication, &Artifact::Mask(mask)).unwrap();
        publication.publish().unwrap();
        id
    }

    /// The record of a run.
    pub fn record(&self, run: &str) -> RunRecord {
        let reader = Reader::open(&self.root(), self.fs.clone() as Arc<dyn Fs>).unwrap();
        let generation = reader.snapshot().unwrap();
        record::stored(&generation, run, |entry| reader.read(entry))
            .unwrap()
            .unwrap()
    }

    /// The mask of this identity.
    pub fn mask(&self, id: ArtifactId) -> Mask {
        let read = artifact::get_anywhere(
            self.store.as_ref(),
            ArtifactType::Mask,
            id,
            &DecodeLimits::default(),
        );
        read.unwrap().into_mask().unwrap()
    }
}

/// Backends whose one model states a bound of 100 micros a request.
struct Priced;

impl Backends for Priced {
    fn resolve(
        &self,
        _: Capability,
        _: Option<&str>,
        _: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        Ok(ModelIdentity::new("fake", "fake-model", "1"))
    }

    fn bound(&self, _: Capability, _: &ModelIdentity) -> Estimate {
        Estimate::of_micros(100)
    }
}

/// An executor over the fakes, the recipe of this text, and these actions more.
pub fn engine(world: &World, text: &str, more: Vec<Arc<dyn Action>>) -> Engine {
    let mut registry = fakes(more.clone());
    let recipe = action_of(text, &registry).unwrap();
    registry.register(Arc::new(recipe)).unwrap();
    Engine::over(
        registry,
        Arc::new(Priced),
        world.store.clone(),
        Digest::of_bytes(b""),
    )
}

/// The recipe's call: the seed, and what else the test gives.
pub fn call(seed: ArtifactId) -> Call {
    Call::new("marks").input("seed", seed)
}

fn outputs(outcome: &Outcome) -> Vec<(String, ArtifactId)> {
    let Outcome::Completed { outputs } = outcome else {
        panic!("{outcome:?}");
    };
    outputs
        .iter()
        .map(|o| (o.port().to_owned(), o.id()))
        .collect()
}

fn ids(record: &RunRecord, port: &str) -> Vec<ArtifactId> {
    (record.inputs().iter())
        .filter(|input| input.port == port)
        .map(|input| ArtifactId(input.artifact))
        .collect()
}

#[test]
fn values_pass_between_steps_by_reference() {
    let world = World::new("recipe-values");
    let seed = world.put(rect_mask(on("photo")));
    let executor = engine(&world, &recipe(TWO_MARKS, "second.mask"), Vec::new());
    let finished = executor.submit(&call(seed)).unwrap();
    let handed = outputs(finished.outcome());
    let recipe = world.record(finished.run().as_str());
    let steps = recipe.steps();
    assert_eq!(steps.len(), 2);
    let first = world.record(steps[0].run());
    let second = world.record(steps[1].run());
    // The first step was given the recipe's seed; the second, the first's output, by its
    // identity; and the recipe handed back the second's.
    assert_eq!(ids(&first, "mask"), [seed]);
    let made = ArtifactId(first.outputs()[0].artifact);
    assert_eq!(ids(&second, "mask"), [made]);
    let last = ArtifactId(second.outputs()[0].artifact);
    assert_eq!(handed, [("mask".to_owned(), last)]);
    assert_eq!(&world.mask(last).core()[..2], &[255, 255]);
    // Each step's run is a child of the recipe's.
    for step in [&first, &second] {
        assert_eq!(
            step.identity().parent.as_deref(),
            Some(finished.run().as_str())
        );
    }
}

#[test]
fn an_absent_optional_input_binds_only_to_an_optional_port() {
    let world = World::new("recipe-absent");
    let seed = world.put(rect_mask(on("photo")));
    let mut other = rect_mask(on("photo"));
    other.replace_core(vec![0; other.core().len()]).unwrap();
    let mut core = vec![0; other.core().len()];
    core[5] = 255;
    other.replace_core(core).unwrap();
    let protect = world.put(other);
    let executor = engine(&world, &recipe(TWO_MARKS, "second.mask"), Vec::new());
    // Left out: the optional port is unbound, and every step runs.
    let without = executor.submit(&call(seed)).unwrap();
    let steps = world.record(without.run().as_str());
    let second = world.record(steps.steps()[1].run());
    assert!(ids(&second, "other").is_empty());
    assert_eq!(steps.steps().len(), 2);
    // Given: it reaches the port.
    let with = executor
        .submit(&call(seed).input("protect", protect))
        .unwrap();
    let steps = world.record(with.run().as_str());
    let second = world.record(steps.steps()[1].run());
    assert_eq!(ids(&second, "other"), [protect]);
    let handed = outputs(with.outcome());
    assert_eq!(world.mask(handed[0].1).protect()[5], 255);
    // The optional text left out leaves the argument out; given, it is the argument.
    let first = world.record(steps.steps()[0].run());
    assert!(first.asked().arguments.iter().all(|a| a.name != "note"));
    // A required port bound to an optional input is refused when the recipe is read.
    let required = TWO_MARKS.replacen("inputs.seed", "inputs.protect", 1);
    let error = action_of(&recipe(&required, "second.mask"), &fakes(Vec::new()))
        .err()
        .unwrap();
    assert_eq!(error.step, "first");
    assert!(error.why.contains("is an optional input"), "{error}");
}

#[test]
fn each_step_is_a_child_run_on_the_recipes_one_reservation() {
    let world = World::new("recipe-reservation");
    let seed = world.put(rect_mask(on("photo")));
    // Two steps that may cost, 100 each: the recipe's estimate is 200.
    let steps = r#"
[[step]]
id = "one"
op = "paid"
inputs = { mask = "inputs.seed" }

[[step]]
id = "two"
op = "paid"
inputs = { mask = "one.mask" }
"#;
    let ledger = Arc::new(FakeLedger::with_allowance(1000));
    let executor =
        engine(&world, &recipe(steps, "two.mask"), Vec::new()).with_ledger(ledger.clone());
    let finished = executor.submit(&call(seed)).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    // The ledger was asked once, for the recipe: one reservation, of both steps.
    assert_eq!(ledger.asked(), 1);
    let reserved = ledger.reservations(world.store.as_ref()).unwrap();
    assert_eq!(reserved.len(), 1);
    assert_eq!(
        (reserved[0].run(), reserved[0].micros()),
        (finished.run(), 200)
    );
    let recipe = world.record(finished.run().as_str());
    assert_eq!(recipe.spend().reserved_micros, Some(200));
    // Each step took its share of the recipe's allowance, as a child of its run.
    for step in recipe.steps() {
        let child = world.record(step.run());
        assert_eq!(
            child.identity().parent.as_deref(),
            Some(finished.run().as_str())
        );
        assert_eq!(child.spend().reserved_micros, Some(100));
        assert_eq!(step.kind(), StepKind::Recipe);
    }
}

#[test]
fn a_cancelled_recipe_cancels_its_running_step_and_starts_no_other() {
    let world = World::new("recipe-cancel");
    let seed = world.put(rect_mask(on("photo")));
    let signal = Cancellation::new();
    let seen = Arc::new(AtomicU64::new(0));
    // `stop` cancels the recipe's request, and ends only once its own run's signal, which
    // is under the recipe's, says it is cancelled.
    let stop: Arc<dyn Action> = {
        let (signal, seen) = (signal.clone(), seen.clone());
        let descriptor = ActionDescriptor::draft("stop", "1.0.0", Kind::Primitive, "Stops.")
            .input(InputPort::required("mask", ArtifactType::Mask, "The mask."))
            .output(OutputPort::one("mask", ArtifactType::Mask, "The mask."))
            .build()
            .unwrap();
        Arc::new(Fake(
            descriptor,
            Box::new(move |call, context: &Context| {
                signal.cancel();
                if context.cancellation().is_cancelled() {
                    seen.fetch_add(1, Ordering::SeqCst);
                    return Err(Failure::error("cancelled", "stop was cancelled"));
                }
                Ok(Produced::nothing().with("mask", mask_at(call, context, "mask").unwrap()))
            }),
        ))
    };
    let steps = format!(
        "{TWO_MARKS}\n[[step]]\nid = \"halt\"\nop = \"stop\"\ninputs = {{ mask = \"first.mask\" }}\n\n\
         [[step]]\nid = \"after\"\nop = \"mark\"\ninputs = {{ mask = \"halt.mask\" }}\nx = 3\n"
    );
    let executor = engine(&world, &recipe(&steps, "after.mask"), vec![stop]);
    let request = Request::new(call(seed)).cancelled_by(signal);
    let answer = executor.request(&request).unwrap();
    let whizbang_core::contract::Answer::Ran(finished) = answer else {
        panic!("{answer:?}");
    };
    assert_eq!(*finished.outcome(), Outcome::Cancelled);
    assert_eq!(seen.load(Ordering::SeqCst), 1);
    let recipe = world.record(finished.run().as_str());
    let ran: Vec<(&str, Option<Status>)> = (recipe.steps().iter())
        .map(|step| (step.recipe().unwrap().id.as_str(), step.status()))
        .collect();
    // The step that ran was cancelled with the recipe, and the one after it never ran.
    assert_eq!(
        ran,
        [
            ("first", Some(Status::Completed)),
            ("second", Some(Status::Completed)),
            ("halt", Some(Status::Cancelled)),
        ]
    );
}

#[test]
fn an_override_naming_no_step_of_the_recipe_is_refused_before_anything_runs() {
    let world = World::new("recipe-override");
    let seed = world.put(rect_mask(on("photo")));
    let executor = engine(&world, &recipe(TWO_MARKS, "second.mask"), Vec::new());
    for (set, why) in [
        (
            "third.x=2",
            "the override \"third.x=2\" names the step \"third\", and the recipe marks has no such step",
        ),
        (
            "first.radius=2",
            "the override \"first.radius=2\": the step first (mark) takes no argument radius",
        ),
        (
            "first.x=two",
            "the override \"first.x=two\": the step first: \"two\" is not a whole number",
        ),
        (
            "first",
            "the override \"first\" is not <step>.<argument>=<value>",
        ),
    ] {
        let asked = call(seed).argument("set", Value::text(set));
        let finished = executor.submit(&asked).unwrap();
        assert_eq!(
            *finished.outcome(),
            Outcome::Invalid {
                why: why.to_owned()
            },
            "{set}"
        );
        assert!(world.record(finished.run().as_str()).steps().is_empty());
    }
    // One that names a step and an argument of it is taken, and is the step's argument
    // in its call and in the recipe's record; so is a constant set.
    let asked = call(seed).argument("set", Value::text("first.x=4\nsecond.constants=mark-v1"));
    let finished = executor.submit(&asked).unwrap();
    let recipe = world.record(finished.run().as_str());
    let first = world.record(recipe.steps()[0].run());
    let x = first
        .asked()
        .arguments
        .iter()
        .find(|a| a.name == "x")
        .unwrap();
    assert_eq!(serde_json::to_string(&x.value).unwrap(), r#"{"integer":4}"#);
    assert_eq!(
        recipe.steps()[1].recipe().unwrap().constants.as_deref(),
        Some("mark-v1")
    );
    assert_eq!(world.mask(outputs(finished.outcome())[0].1).core()[4], 255);
}

#[test]
fn until_a_step_hands_back_that_steps_outputs() {
    let world = World::new("recipe-until");
    let seed = world.put(rect_mask(on("photo")));
    let executor = engine(&world, &recipe(TWO_MARKS, "second.mask"), Vec::new());
    let finished = executor
        .submit(&call(seed).argument("until", Value::text("first")))
        .unwrap();
    // The recipe completed after its first step, and handed back none of its own outputs,
    // which apply only without `until`: it handed back the step's as its own.
    let recipe = world.record(finished.run().as_str());
    assert_eq!(recipe.steps().len(), 1);
    let first = world.record(recipe.steps()[0].run());
    let made = ArtifactId(first.outputs()[0].artifact);
    assert_eq!(
        outputs(finished.outcome()),
        [(whizbang_core::recipe::until_port("first", "mask"), made)]
    );
    assert_eq!(world.mask(made).core()[0], 255);
    assert_eq!(recipe.steps()[0].recipe().unwrap().id, "first");
    // A step that is not the recipe's is refused when the call is prepared.
    assert!(
        executor
            .submit(&call(seed).argument("until", Value::text("fifth")))
            .is_err()
    );
    // Every run of a recipe notes the digest of its text.
    let digest = Digest::of_bytes(recipe_text().as_bytes()).to_string();
    let noted: BTreeMap<&str, String> = (recipe.asked().settings.iter())
        .map(|s| (s.name(), serde_json::to_string(s.value()).unwrap()))
        .collect();
    assert_eq!(noted[DIGEST_SETTING], format!(r#"{{"text":"{digest}"}}"#));
}

#[test]
fn a_run_until_a_step_hands_back_its_outputs_which_a_later_call_takes_until_the_recipe_is_deleted()
{
    // The step's own output is a name of the recipe's run alone (plan m3, task Q1, item 3).
    // The recipe hands it back as its own, so its run, which nothing called, commits it
    // outside runs and holds it: what `--until` prints is an input of any later call.
    let world = World::new("recipe-until");
    let seed = world.put(rect_mask(on("photo")));
    let executor = engine(&world, &recipe(TWO_MARKS, "second.mask"), Vec::new());
    let until = executor
        .submit(&call(seed).argument("until", Value::text("first")))
        .unwrap();
    let made = outputs(until.outcome())[0].1;
    let public = |id| artifact::holds(&world.store.snapshot().unwrap(), ArtifactType::Mask, id);
    assert!(public(made));
    // A later call of another tree takes it.
    let later = executor.submit(&call(made)).unwrap();
    assert_eq!(later.outcome().name(), "completed", "{:?}", later.outcome());
    // Each run is deleted with the steps it called, as `store delete` deletes it: the
    // later run lets the mask go; the recipe's run still holds it, until it is deleted.
    let delete = |run: &str, runs: usize| {
        let generation = world.store.snapshot().unwrap();
        let all = with_descendants(&generation, &[run], |e| world.store.read(e));
        assert_eq!(all.len(), runs, "the run and each step it called: {all:?}");
        world
            .store
            .delete(&all.iter().map(String::as_str).collect::<Vec<_>>())
            .unwrap();
    };
    delete(later.run().as_str(), 3);
    assert!(public(made));
    delete(until.run().as_str(), 2);
    assert!(!public(made));
    assert!(world.store.snapshot().unwrap().runs().is_empty());
}

fn recipe_text() -> String {
    recipe(TWO_MARKS, "second.mask")
}

// --- a run from a step (plan m4, task K1, item 2; ADR 0033, "A run from a step") ---------------

/// Four steps, the first and the last of which may cost 100 each.
const STRETCH: &str = r#"
[[step]]
id = "one"
op = "paid"
inputs = { mask = "inputs.seed" }

[[step]]
id = "two"
op = "mark"
inputs = { mask = "one.mask" }
x = 1

[[step]]
id = "three"
op = "mark"
inputs = { mask = "two.mask" }
x = 2

[[step]]
id = "four"
op = "paid"
inputs = { mask = "three.mask" }
"#;

fn stretch() -> String {
    recipe(STRETCH, "four.mask")
}

/// A call of the recipe from `from`, given `mask` in place of the output of `step`, and
/// as its seed: the recipe's own inputs are given as for any run of it.
fn from(step: &str, from: &str, mask: ArtifactId) -> Call {
    call(mask)
        .argument("from", Value::text(from))
        .input(&whizbang_core::recipe::from_port(step, "mask"), mask)
}

fn step_ids(record: &RunRecord) -> Vec<String> {
    let ids = record.steps().iter().filter_map(|step| step.recipe());
    ids.map(|step| step.id.clone()).collect()
}

fn invalid(outcome: &Outcome) -> &str {
    match outcome {
        Outcome::Invalid { why } => why,
        other => panic!("not invalid: {other:?}"),
    }
}

#[test]
fn a_run_from_a_step_runs_that_step_and_those_after_it_with_what_it_was_given() {
    let world = World::new("recipe-from");
    let given = world.put(rect_mask(on("photo")));
    let seed = world.put(rect_mask(on("another photo")));
    let executor = engine(&world, &stretch(), Vec::new());
    // The seed is given, as every run of the recipe gives it, and the step that takes
    // it does not run.
    let asked = Call::new("marks")
        .input("seed", seed)
        .argument("from", Value::text("two"))
        .input(&whizbang_core::recipe::from_port("one", "mask"), given);
    let finished = executor.submit(&asked).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    let record = world.record(finished.run().as_str());
    assert_eq!(step_ids(&record), ["two", "three", "four"]);
    // The step after the start took what was given in place of the output before it.
    let second = world.record(record.steps()[0].run());
    assert_eq!(ids(&second, "mask"), [given]);
    // What came out is the given mask with what the steps of the stretch did to it.
    let out = outputs(finished.outcome());
    assert_eq!(out.len(), 1);
    let (made, was) = (world.mask(out[0].1), world.mask(given));
    assert_eq!((made.core()[1], made.core()[2]), (255, 255));
    assert_eq!((was.core()[1], was.core()[2]), (0, 0));
    // The record says where the run started and what it was given.
    assert_eq!(record.version(), "run/6");
    let started = serde_json::to_value(record.started().unwrap()).unwrap();
    assert_eq!(
        started,
        serde_json::json!({"from": "two", "given": [
            {"step": "one", "port": "mask", "type": "mask", "artifact": given.0.to_string()}
        ]})
    );
    // A run of the whole recipe says nothing of a start, and is of the version before.
    let whole = executor.submit(&call(given)).unwrap();
    let record = world.record(whole.run().as_str());
    assert_eq!(step_ids(&record), ["one", "two", "three", "four"]);
    assert_eq!((record.version(), record.started()), ("run/5", None));
    assert_eq!(record.plan(), None);
}

#[test]
fn a_run_from_one_step_until_another_runs_that_stretch_alone() {
    let world = World::new("recipe-from-until");
    let given = world.put(rect_mask(on("photo")));
    let executor = engine(&world, &stretch(), Vec::new());
    let asked = from("one", "two", given).argument("until", Value::text("three"));
    let finished = executor.submit(&asked).unwrap();
    let record = world.record(finished.run().as_str());
    assert_eq!(step_ids(&record), ["two", "three"]);
    // It hands back the outputs of the step it stopped at, as `until` does.
    let out = outputs(finished.outcome());
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0, whizbang_core::recipe::until_port("three", "mask"));
    let made = world.mask(out[0].1);
    assert_eq!((made.core()[1], made.core()[2]), (255, 255));
    // One step alone is a stretch too.
    let asked = from("one", "two", given).argument("until", Value::text("two"));
    let finished = executor.submit(&asked).unwrap();
    assert_eq!(step_ids(&world.record(finished.run().as_str())), ["two"]);
}

/// The backends of the runner's tests, without a filler.
struct NoFiller;

impl Backends for NoFiller {
    fn resolve(
        &self,
        _: Capability,
        _: Option<&str>,
        _: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        Err(Unavailable::new("no filler is configured"))
    }
}

#[test]
fn a_run_from_a_step_reserves_nothing_for_the_steps_before_it() {
    let world = World::new("recipe-from-reserves");
    let given = world.put(rect_mask(on("photo")));
    let ledger = Arc::new(FakeLedger::with_allowance(1000));
    let executor = engine(&world, &stretch(), Vec::new()).with_ledger(ledger.clone());
    // The whole recipe may cost 200: its first step and its last.
    assert_eq!(
        executor.prepare(&call(given)).unwrap().estimate().micros(),
        200
    );
    // From the second step, the first is not paid for.
    let asked = from("one", "two", given);
    let prepared = executor.prepare(&asked).unwrap();
    assert_eq!(prepared.estimate().micros(), 100);
    assert_eq!(prepared.plan().unwrap().requests(Capability::Fill), Some(1));
    let finished = executor.submit(&asked).unwrap();
    let reserved = ledger.reservations(world.store.as_ref()).unwrap();
    assert_eq!(reserved.len(), 1);
    assert_eq!(
        (reserved[0].run(), reserved[0].micros()),
        (finished.run(), 100)
    );
    let record = world.record(finished.run().as_str());
    assert_eq!(record.spend().reserved_micros, Some(100));
    assert_eq!(record.plan(), prepared.plan());
    // A stretch that no paid step is in needs no filler at all: none is resolved, so
    // the call is ready where none is configured, and nothing is reserved.
    let asked = asked.argument("until", Value::text("three"));
    let prepared = executor.prepare(&asked).unwrap();
    assert!(prepared.needs().is_empty());
    assert_eq!(prepared.estimate().micros(), 0);
    let mut registry = fakes(Vec::new());
    let recipe = action_of(&stretch(), &registry).unwrap();
    registry.register(Arc::new(recipe)).unwrap();
    let without = Engine::over(
        registry,
        Arc::new(NoFiller),
        world.store.clone(),
        Digest::of_bytes(b""),
    );
    let finished = without.submit(&asked).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!(
        world
            .record(finished.run().as_str())
            .spend()
            .reserved_micros,
        None
    );
    // The whole recipe is not ready there.
    assert!(without.submit(&call(given)).is_err());
}

#[test]
fn a_run_from_a_step_that_lacks_an_output_a_later_step_needs_is_refused_before_anything_runs() {
    let world = World::new("recipe-from-refused");
    let given = world.put(rect_mask(on("photo")));
    let executor = engine(&world, &stretch(), Vec::new());
    let from_three = || call(given).argument("from", Value::text("three"));
    let port = |step: &str| whizbang_core::recipe::from_port(step, "mask");
    let refused = |asked: &Call, said: &str| {
        let finished = executor.submit(asked).unwrap();
        let why = invalid(finished.outcome()).to_owned();
        assert!(why.contains(said), "{why}");
        // Before anything ran: the recipe's run made no step.
        let record = world.record(finished.run().as_str());
        assert!(record.steps().is_empty(), "{why}");
        assert_eq!(record.started(), None);
    };
    // Nothing given in place of the output the first step of the stretch takes.
    refused(
        &from_three(),
        "the step three takes the output mask of the step two, which does not run, and \
         nothing was given in its place: give it at from_two_mask",
    );
    // Given for another step than the one whose output is taken.
    refused(
        &from_three().input(&port("one"), given),
        "the step three takes the output mask of the step two",
    );
    // `from` after `until`.
    let backwards = from("two", "three", given).argument("until", Value::text("two"));
    refused(
        &backwards,
        "from names three, which comes after the step the run stops at",
    );
    // Something given for a step that runs, or with no `from` at all.
    let twice = from("two", "three", given).input(&port("three"), given);
    refused(
        &twice,
        "the step three, which is not before the step the run starts from",
    );
    let no_from = call(given).input(&port("one"), given);
    refused(&no_from, "from names no step to run from");
    // A step that is no step of the recipe, and the first step, are no `from`; an
    // artifact that is not of the port's type is refused as any input is. None of
    // them makes a run.
    let runs = world.store.snapshot().unwrap().runs().len();
    for step in ["five", "one"] {
        let asked = from("one", step, given);
        assert!(executor.submit(&asked).is_err(), "{step}");
    }
    let picture = {
        use crate::artifact::support::{image_of, photo};
        let image = Artifact::from(image_of(photo(), artifact::ImageRole::Picture));
        let mut publication = world.store.begin().unwrap();
        let id = artifact::put(&mut publication, &image).unwrap();
        publication.publish().unwrap();
        id
    };
    let wrong = call(given)
        .argument("from", Value::text("two"))
        .input(&port("one"), picture);
    let refusal = executor.submit(&wrong).unwrap_err();
    assert!(
        refusal
            .to_string()
            .contains("the input from_one_mask: a mask is required"),
        "{refusal}"
    );
    assert_eq!(world.store.snapshot().unwrap().runs().len(), runs);
}

#[test]
fn a_recipe_hands_its_caller_the_findings_of_its_last_step() {
    use whizbang_core::contract::record::Decision;
    use whizbang_core::contract::{ResultKind, Verdict};
    let judges = |name: &str| -> Arc<dyn Action> {
        let descriptor = ActionDescriptor::draft(name, "1.0.0", Kind::Primitive, "Judges.")
            .input(InputPort::required("mask", ArtifactType::Mask, "The mask."))
            .results(ResultKind::Verdict)
            .results(ResultKind::Findings)
            .concurrent(true)
            .build()
            .unwrap();
        Arc::new(Fake(
            descriptor,
            Box::new(|_, _| {
                Ok(Produced::nothing()
                    .with_verdict(Verdict::new(Decision::Fail, "it is still there"))
                    .with_findings(crate::contract::findings::document()))
            }),
        ))
    };
    let steps = r#"
[[step]]
id = "first"
op = "mark"
inputs = { mask = "inputs.seed" }
x = 0

[[step]]
id = "judged"
op = "judges"
inputs = { mask = "first.mask" }
"#;
    let world = World::new("recipe-findings");
    let seed = world.put(rect_mask(on("photo")));
    let text = recipe(steps, "first.mask");
    let executor = engine(&world, &text, vec![judges("judges")]);
    let declared = executor.registry().get("marks").unwrap().descriptor();
    assert!(declared.answers_with(ResultKind::Findings));
    let finished = executor.submit(&call(seed)).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    // With the verdict, the findings of the last step, as that step handed them back.
    assert_eq!(
        finished.verdict().map(|v| v.decision()),
        Some(Decision::Fail)
    );
    let handed = finished.findings().unwrap();
    assert_eq!(handed.to_json(), crate::contract::findings::document());
    let record = world.record(finished.run().as_str());
    assert_eq!(record.findings().unwrap().counts.len(), 4);
    // A step that declares findings and is not the last hands the recipe's caller none.
    let middle = format!(
        "{steps}\n[[step]]\nid = \"last\"\nop = \"mark\"\ninputs = {{ mask = \"first.mask\" }}\nx = 3\n"
    );
    let world = World::new("recipe-findings-middle");
    let seed = world.put(rect_mask(on("photo")));
    let executor = engine(
        &world,
        &recipe(&middle, "last.mask"),
        vec![judges("judges")],
    );
    let declared = executor.registry().get("marks").unwrap().descriptor();
    assert!(!declared.answers_with(ResultKind::Findings));
    let finished = executor.submit(&call(seed)).unwrap();
    assert_eq!(finished.findings(), None);
}

#[test]
fn a_run_from_a_step_given_more_than_its_record_lists_is_refused_before_anything_runs() {
    // Sixteen steps that hand back up to four masks each, a join between each two, and
    // a last step: 32 steps, and 65 artifacts given where a record lists 64.
    let mut steps = String::new();
    let mut last = "inputs.seed".to_owned();
    for at in 1..=16 {
        steps.push_str(&format!(
            "[[step]]\nid = \"s{at}\"\nop = \"split\"\ninputs = {{ mask = \"{last}\" }}\n\n"
        ));
        let id = if at == 16 {
            "end".to_owned()
        } else {
            format!("j{at}")
        };
        steps.push_str(&format!(
            "[[step]]\nid = \"{id}\"\nop = \"join\"\ninputs = {{ mask = \"s{at}.mask\" }}\n\n"
        ));
        last = format!("{id}.mask");
    }
    let world = World::new("recipe-from-many");
    let given = world.put(rect_mask(on("photo")));
    let executor = engine(&world, &recipe(&steps, "end.mask"), Vec::new());
    let port = |step: &str| whizbang_core::recipe::from_port(step, "mask");
    let asked = |extra: bool| {
        let mut asked = call(given).argument("from", Value::text("end"));
        for at in 1..=16 {
            for _ in 0..4 {
                asked = asked.input(&port(&format!("s{at}")), given);
            }
        }
        if extra {
            asked = asked.input(&port("j1"), given);
        }
        asked
    };
    let finished = executor.submit(&asked(true)).unwrap();
    let why = invalid(finished.outcome());
    assert!(
        why.contains("65 artifacts are given in place of steps' outputs")
            && why.contains("64 at most"),
        "{why}"
    );
    assert!(world.record(finished.run().as_str()).steps().is_empty());
    // As many as a record lists run, and the record lists them all.
    let finished = executor.submit(&asked(false)).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    let record = world.record(finished.run().as_str());
    assert_eq!(record.started().unwrap().given.len(), 64);
    assert_eq!(step_ids(&record), ["end"]);
}

#[test]
fn a_run_from_a_step_does_not_leave_unbound_what_a_whole_run_would_bind() {
    // The second step's optional input takes the first step's output, which every whole
    // run binds: a run from the second step is refused when nothing is given there.
    let steps = r#"
[[step]]
id = "one"
op = "mark"
inputs = { mask = "inputs.seed" }
x = 0

[[step]]
id = "two"
op = "mark"
inputs = { mask = "inputs.seed", other = "one.mask" }
x = 1
"#;
    let world = World::new("recipe-from-optional");
    let seed = world.put(rect_mask(on("photo")));
    let executor = engine(&world, &recipe(steps, "two.mask"), Vec::new());
    let bare = call(seed).argument("from", Value::text("two"));
    let finished = executor.submit(&bare).unwrap();
    let why = invalid(finished.outcome());
    assert!(
        why.contains("the step two takes the output mask of the step one, which does not run"),
        "{why}"
    );
    assert!(world.record(finished.run().as_str()).steps().is_empty());
    // Given, it runs.
    let finished = executor.submit(&from("one", "two", seed)).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
}
