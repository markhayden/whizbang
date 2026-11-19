//! What a run reads of what the runs it called committed (plan m3, task R1, item 2; the
//! carry-over row on a parent reading a child's names): each child that commits has its
//! names resolved once, when its end is published, and a look at the run's store then
//! reads no file.

use std::sync::{Arc, Mutex};

use whizbang_core::artifact::{self, ArtifactType};
use whizbang_core::contract::{ActionDescriptor, Call, Executor, Kind, Outcome, Produced, Value};
use whizbang_core::identity::DecodeLimits;

use crate::artifact::support::{on, rect_mask};
use crate::recipe::{Fake, fakes};
use crate::runner::World;

#[test]
fn a_childs_names_are_resolved_once_and_a_look_reads_no_file() {
    let world = World::new("child-names");
    let seed = world.put(rect_mask(on("photo")));
    let fs = world.fs.clone();
    let seen: Arc<Mutex<Vec<usize>>> = Arc::default();
    let parent = {
        let seen = seen.clone();
        let descriptor = ActionDescriptor::draft("parent", "1.0.0", Kind::Primitive, "Calls.")
            .input(whizbang_core::contract::InputPort::required(
                "mask",
                ArtifactType::Mask,
                "The mask.",
            ))
            .build()
            .unwrap();
        Fake(
            descriptor,
            Box::new(move |call, context| {
                let given = call.inputs()[0].id();
                let mut made = Vec::new();
                for x in [3_i64, 4] {
                    let asked = Call::new("mark")
                        .input("mask", given)
                        .argument("x", Value::Integer(x));
                    let finished = context.executor().submit(&asked).unwrap();
                    let Outcome::Completed { outputs } = finished.outcome() else {
                        panic!("{:?}", finished.outcome());
                    };
                    made.push(outputs[0].id());
                }
                // Ten looks at the run's store: none reads a file, and each holds what
                // both children committed.
                fs.forget_reads();
                for _ in 0..10 {
                    let generation = context.store().snapshot().unwrap();
                    for id in &made {
                        let name = artifact::name(ArtifactType::Mask, *id);
                        assert!(generation.get(&name).is_some(), "{name}");
                    }
                }
                seen.lock().unwrap().push(fs.reads().len());
                // What was committed is read as any artifact is: its own file.
                let read = artifact::get(
                    context.store(),
                    ArtifactType::Mask,
                    made[1],
                    &DecodeLimits::default(),
                );
                assert_eq!(read.unwrap().into_mask().unwrap().core()[4], 255);
                Ok(Produced::nothing())
            }),
        )
    };
    let mut registry = fakes(Vec::new());
    registry.register(Arc::new(parent)).unwrap();
    let executor = whizbang_core::contract::Engine::over(
        registry,
        Arc::new(whizbang_core::contract::NoBackends),
        world.store.clone(),
        whizbang_core::identity::Digest::of_bytes(b""),
    );
    let finished = executor
        .submit(&Call::new("parent").input("mask", seed))
        .unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!(*seen.lock().unwrap(), [0]);
}
