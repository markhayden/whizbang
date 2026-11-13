//! The calls a run froze (ADR 0009): a run that says which actions it calls is held to
//! them where its calls are dispatched, and from then on gives a call only what it may
//! read: an input of its own call, or an output that a run it called committed. Each
//! refusal is in the run's record. What is frozen stays as it was frozen.

use std::sync::{Arc, Mutex};

use whizbang_core::artifact::{self, Artifact, ArtifactId, ArtifactType};
use whizbang_core::contract::record::RunRecord;
use whizbang_core::contract::{
    Action, ActionDescriptor, Call, CallError, Context, Count, Engine, Executor, Failure,
    InputPort, Kind, NoBackends, OutputPort, PreparedCall, Produced, Refusal, RefusalKind,
    Registry, Request,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::Store;

use super::commit::{World, held};
use crate::artifact::support::{on, rect_mask};

type Body = Box<dyn Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync>;

struct Scripted(ActionDescriptor, Body);

impl Action for Scripted {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        (self.1)(call, context)
    }
}

/// An action that takes up to two masks at `masks` and may return one at `made`.
fn action(
    name: &str,
    body: impl Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync + 'static,
) -> Arc<dyn Action> {
    let draft = ActionDescriptor::draft(name, "1.0.0", Kind::Primitive, "A test action.")
        .input(InputPort::variadic(
            "masks",
            ArtifactType::Mask,
            "Some masks.",
            Count::between(0, 2),
        ))
        .output(OutputPort::counted(
            "made",
            ArtifactType::Mask,
            "A mask.",
            Count::between(0, 1),
        ));
    Arc::new(Scripted(draft.build().unwrap(), Box::new(body)))
}

fn engine(store: Arc<Store>, actions: Vec<Arc<dyn Action>>) -> Engine {
    let mut registry = Registry::new();
    for action in actions {
        registry.register(action).unwrap();
    }
    Engine::over(registry, Arc::new(NoBackends), store, Digest::of_bytes(b""))
}

/// Puts a mask on the picture of this name in the store, as an import does.
fn put(store: &Store, name: &str) -> ArtifactId {
    let mut publication = store.begin().unwrap();
    let mask: Artifact = rect_mask(on(name)).into();
    let id = artifact::put(&mut publication, &mask).unwrap();
    publication.publish().unwrap();
    id
}

fn refusal(result: Result<impl std::fmt::Debug, CallError>) -> Refusal {
    match result {
        Err(CallError::Refused(refusal)) => refusal,
        other => panic!("not refused: {other:?}"),
    }
}

fn record(world: &World, run: &str) -> RunRecord {
    let runs = held(&world.root(), world.fs.clone()).runs;
    let found = runs.into_iter().find(|(id, _, _)| id == run);
    found.unwrap().1.unwrap()
}

type Noted = Arc<Mutex<Vec<Refusal>>>;

#[test]
fn a_run_that_froze_its_calls_calls_no_other_action_and_each_refusal_is_in_its_record() {
    let world = World::new();
    let noted = Noted::default();
    let outer = {
        let noted = noted.clone();
        action("outer", move |_, context| {
            // Before it freezes anything a run calls any action, as it always did.
            context.executor().submit(&Call::new("other")).unwrap();
            assert!(context.freeze_calls(["inner"]));
            context.executor().submit(&Call::new("inner")).unwrap();
            let mut noted = noted.lock().unwrap();
            // An action that is registered, and one that is not: refused alike, and
            // not told apart, so nothing is learned of what there is.
            noted.push(refusal(context.executor().submit(&Call::new("other"))));
            noted.push(refusal(context.executor().submit(&Call::new("nowhere"))));
            noted.push(refusal(
                context.executor().start(&Request::new(Call::new("other"))),
            ));
            // What is frozen stays frozen: a second list changes nothing.
            assert!(!context.freeze_calls(["inner", "other"]));
            noted.push(refusal(context.executor().submit(&Call::new("other"))));
            assert!(!context.freeze_calls([]));
            context.executor().submit(&Call::new("inner")).unwrap();
            Ok(Produced::nothing())
        })
    };
    let ran = Arc::new(Mutex::new(Vec::new()));
    let counted = |name: &'static str| {
        let ran = ran.clone();
        action(name, move |_, _| {
            ran.lock().unwrap().push(name);
            Ok(Produced::nothing())
        })
    };
    let executor = engine(
        world.store(),
        vec![outer, counted("inner"), counted("other")],
    );
    let finished = executor.submit(&Call::new("outer")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    // `other` ran once, before the list was frozen, and never after.
    assert_eq!(*ran.lock().unwrap(), ["other", "inner", "inner"]);
    let refused = |action: &str| Refusal::NotOffered {
        action: action.to_owned(),
    };
    assert_eq!(
        *noted.lock().unwrap(),
        [
            refused("other"),
            refused("nowhere"),
            refused("other"),
            refused("other")
        ]
    );
    let record = record(&world, finished.run().as_str());
    assert_eq!(record.steps().len(), 3);
    let kinds: Vec<_> = record.outcome().refusals.iter().map(|r| r.kind()).collect();
    assert_eq!(kinds, [RefusalKind::NotOffered; 4]);
    assert_eq!(record.outcome().refusals[1].action(), Some("nowhere"));
    assert_eq!(
        record.outcome().refusals[0].why(),
        "other was not admitted: the run that made the call froze the actions it calls, and \
         this is not one of them"
    );
}

#[test]
fn a_run_that_froze_its_calls_gives_a_call_only_what_it_was_given_or_its_own_calls_committed() {
    // The store holds three masks. The run is given one. Its calls may take that one,
    // and what a run it called committed; not another mask the store holds, not an
    // identity that is nothing, and not what a run nobody called committed meanwhile.
    let world = World::new();
    let store = world.store();
    let given = put(&store, "given");
    let other = put(&store, "other");
    let guessed = ArtifactId(Digest::of_bytes(b"a guess"));
    let noted = Noted::default();
    let outer = {
        let (noted, store) = (noted.clone(), store.clone());
        action("outer", move |_, context| {
            let takes = |id: ArtifactId| Call::new("takes").input("masks", id);
            assert!(context.freeze_calls(["takes", "makes"]));
            context.executor().submit(&takes(given)).unwrap();
            let mut noted = noted.lock().unwrap();
            noted.push(refusal(context.executor().submit(&takes(other))));
            noted.push(refusal(context.executor().submit(&takes(guessed))));
            // The second of two inputs is looked at as the first is.
            let two = takes(given).input("masks", other);
            noted.push(refusal(context.executor().submit(&two)));
            // What a run this run called committed is readable from then on.
            let made = context.executor().submit(&Call::new("makes")).unwrap();
            let made = made.outcome().outputs()[0].id();
            context.executor().submit(&takes(made)).unwrap();
            // What another run committed since this one was admitted is not.
            let late = put(&store, "late");
            noted.push(refusal(context.executor().submit(&takes(late))));
            Ok(Produced::nothing())
        })
    };
    let takes = action("takes", |_, _| Ok(Produced::nothing()));
    let makes = action("makes", |_, _| {
        Ok(Produced::nothing().with("made", rect_mask(on("made"))))
    });
    let executor = engine(store.clone(), vec![outer, takes, makes]);
    let call = Call::new("outer").input("masks", given);
    let finished = executor.submit(&call).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    let refused: Vec<_> = noted.lock().unwrap().clone();
    let ids: Vec<_> = refused
        .iter()
        .map(|refusal| match refusal {
            Refusal::NotReadable { action, port, id } => {
                assert_eq!((action.as_str(), port.as_str()), ("takes", "masks"));
                *id
            }
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(ids[..3], [other, guessed, other]);
    let record = record(&world, finished.run().as_str());
    assert_eq!(record.steps().len(), 3);
    let kinds: Vec<_> = record.outcome().refusals.iter().map(|r| r.kind()).collect();
    assert_eq!(kinds, [RefusalKind::NotReadable; 4]);
    let why = record.outcome().refusals[1].why();
    assert!(
        why.contains(&guessed.to_string()) && why.contains("was not given"),
        "{why}"
    );
    // A run that froze nothing gives a call anything the store holds.
    let free = action("free", move |_, context| {
        let call = Call::new("takes").input("masks", other);
        context.executor().submit(&call).unwrap();
        Ok(Produced::nothing())
    });
    let takes = action("takes", |_, _| Ok(Produced::nothing()));
    let executor = engine(store, vec![free, takes]);
    let finished = executor.submit(&Call::new("free")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
}
