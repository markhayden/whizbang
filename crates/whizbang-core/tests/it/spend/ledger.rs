//! The real ledger after many paid runs (plan 2C, task E1, item 6; the carry-over row on
//! every publication writing the whole generation, its second half): an admission reads
//! the books from the newest reset, so the content of no name of an epoch a reset closed
//! is read.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use whizbang_core::contract::{Call, Engine, Estimate, Executor, Request, RequestId};
use whizbang_core::spend::{Name, StoreLedger};

use super::support::{Bounds, World, allow, books, ran};

/// The content files of the store's names under `spend/` for which `keep` holds.
fn objects(
    world: &World,
    root: &std::path::Path,
    keep: impl Fn(&Name) -> bool,
) -> BTreeSet<PathBuf> {
    let generation = world.store.snapshot().unwrap();
    generation
        .entries()
        .iter()
        .filter(|(text, _)| Name::parse(text).is_ok_and(|name| keep(&name)))
        .map(|(_, entry)| root.join("objects").join(entry.digest.hex()))
        .collect()
}

#[test]
fn an_admission_after_many_paid_runs_and_a_reset_reads_no_closed_epoch() {
    const RUNS: usize = 40;
    let world = World::new("ledger-epochs");
    assert_eq!(allow(&world.store, 100_000_000), 1);
    let engine: Engine = world
        .engine(
            Bounds(Estimate::of_micros(10_000)),
            vec![world.paints("paint")],
        )
        .with_ledger(Arc::new(StoreLedger));
    for at in 0..RUNS {
        let id = RequestId::parse(&format!("paid-{at}")).unwrap();
        ran(engine.request(&Request::new(Call::new("paint")).with_id(id)));
    }
    // Epoch 1 holds forty reservations, each closed; the owner's reset opens epoch 2.
    assert_eq!(books(&world.store).reservations().len(), RUNS);
    assert_eq!(allow(&world.store, 50_000_000), 2);
    let store_root = world.store_root();
    let closed: BTreeSet<PathBuf> = objects(&world, &store_root, |name| match name {
        Name::Reset { epoch, .. } | Name::Epoch(epoch) => epoch.get() == 1,
        Name::Turn { epoch, .. } => *epoch <= 1,
        other => other.run().is_some(),
    });
    assert!(closed.len() > RUNS * 4, "{}", closed.len());
    world.fs.forget_reads();
    let one = ran(engine.request(&Request::new(Call::new("paint"))));
    let read: BTreeSet<PathBuf> = world.fs.reads().into_iter().collect();
    let of_closed: Vec<&PathBuf> = read.intersection(&closed).collect();
    assert!(
        of_closed.is_empty(),
        "the admission read {} files of epoch 1",
        of_closed.len()
    );
    // It did read the books of epoch 2, and it holds there.
    let newest = objects(&world, &store_root, |name| match name {
        Name::Reset { epoch, .. } | Name::Epoch(epoch) => epoch.get() == 2,
        _ => false,
    });
    assert!(
        read.is_superset(&newest),
        "the admission did not read the reset of epoch 2"
    );
    let books = books(&world.store);
    assert_eq!(books.reservation(one.run()).unwrap().epoch(), Some(2));
    assert_eq!(
        books.balance().counted.least().micros(),
        0,
        "it closed, released"
    );
}
