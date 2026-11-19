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
        let read = artifact::get(
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
    // The recipe completed after its first step, and handed back none of its own outputs:
    // they apply only without `until`. The step's are what its run committed.
    assert_eq!(outputs(finished.outcome()), []);
    let recipe = world.record(finished.run().as_str());
    assert_eq!(recipe.steps().len(), 1);
    let first = world.record(recipe.steps()[0].run());
    let made = ArtifactId(first.outputs()[0].artifact);
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

fn recipe_text() -> String {
    recipe(TWO_MARKS, "second.mask")
}
