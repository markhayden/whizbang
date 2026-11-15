//! A fake of the inventory (ADR 0015, "An inventory"). It holds no disk and no
//! catalogue: a test tells it which model is ready, under which installation, and
//! which is not and why; and it keeps the name of each model it was asked about.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use whizbang_core::capability::{Installation, Installed, Inventory, Unready};
use whizbang_core::identity::Digest;

#[derive(Default)]
struct Told {
    models: BTreeMap<String, Installed>,
    asked: Vec<String>,
}

/// A fake inventory. It may be shared across threads and across executors. A model
/// it was told nothing of is not in its catalogue.
#[derive(Default)]
pub struct FakeInventory {
    told: Mutex<Told>,
}

impl FakeInventory {
    /// An inventory that knows no model.
    pub fn new() -> FakeInventory {
        FakeInventory::default()
    }

    fn told(&self) -> MutexGuard<'_, Told> {
        self.told.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// From now on this model is ready, under an installation whose identity is the
    /// digest of these bytes: the same bytes are the same installation, and others
    /// are another. It answers the installation.
    pub fn install(&self, model: &str, content: &[u8]) -> Installation {
        let installation = Installation::of(Digest::of_bytes(content));
        let ready = Installed::Ready(installation);
        self.told().models.insert(model.to_owned(), ready);
        installation
    }

    /// From now on this model is not ready, for this reason.
    pub fn fail(&self, model: &str, why: Unready) {
        let unready = Installed::NotReady(why);
        self.told().models.insert(model.to_owned(), unready);
    }

    /// From now on it knows nothing of this model, as before it was told of it.
    pub fn forget(&self, model: &str) {
        self.told().models.remove(model);
    }

    /// The model it was asked about, each time it was asked, in order.
    pub fn asked(&self) -> Vec<String> {
        self.told().asked.clone()
    }
}

impl Inventory for FakeInventory {
    fn look(&self, model: &str) -> Installed {
        let mut told = self.told();
        told.asked.push(model.to_owned());
        let known = told.models.get(model).copied();
        known.unwrap_or(Installed::NotReady(Unready::NotInCatalogue))
    }
}
