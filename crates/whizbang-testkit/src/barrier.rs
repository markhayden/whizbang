//! Named barriers: places where a fake action or a fake capability stops until a test
//! lets it go on. A test waits for something to arrive at a barrier, does what it came
//! to do while that thing is stopped there, and releases it. Nothing here sleeps: every
//! wait is for an event, with a limit that fails the test instead of hanging it.

use std::collections::BTreeMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// How long a wait may last before it is taken for a test that cannot pass. It is far
/// beyond what any wait here takes, and is a limit, not a pause.
const LIMIT: Duration = Duration::from_secs(60);

#[derive(Default)]
struct Gate {
    /// How many times something came to the barrier.
    arrived: u64,
    /// How many of them were let go on.
    released: u64,
    /// Whether everything that comes from now on goes straight through.
    open: bool,
}

/// A set of barriers, each known by its name. A clone is the same set.
#[derive(Clone, Default)]
pub struct Barriers {
    shared: Arc<(Mutex<BTreeMap<String, Gate>>, Condvar)>,
}

impl Barriers {
    pub fn new() -> Barriers {
        Barriers::default()
    }

    fn gates(&self) -> MutexGuard<'_, BTreeMap<String, Gate>> {
        self.shared.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Waits until `done` says so of the barrier `name`, and panics when the limit
    /// passes first.
    fn until(&self, name: &str, what: &str, done: impl Fn(&Gate) -> bool) {
        let limit = Instant::now() + LIMIT;
        let mut gates = self.gates();
        loop {
            if done(gates.entry(name.to_owned()).or_default()) {
                return;
            }
            let left = limit.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "nothing {what} the barrier {name}");
            gates = self
                .shared
                .1
                .wait_timeout(gates, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    /// For what is stopped: arrives at the barrier `name` and stays there until a test
    /// releases it. One release lets one arrival through, in the order they came.
    pub fn stop_at(&self, name: &str) {
        let turn = {
            let mut gates = self.gates();
            let gate = gates.entry(name.to_owned()).or_default();
            gate.arrived += 1;
            gate.arrived
        };
        self.shared.1.notify_all();
        self.until(name, "released", |gate| gate.open || gate.released >= turn);
    }

    /// For a test: waits until something has arrived at the barrier `name` for the
    /// `count`th time, whether or not it is still there.
    pub fn arrived(&self, name: &str, count: u64) {
        self.until(name, "arrived at", |gate| gate.arrived >= count);
    }

    /// For a test: lets one arrival at the barrier `name` go on, one that is there or
    /// the next to come.
    pub fn release(&self, name: &str) {
        self.gates().entry(name.to_owned()).or_default().released += 1;
        self.shared.1.notify_all();
    }

    /// For a test: from now on nothing stops at the barrier `name`.
    pub fn open(&self, name: &str) {
        self.gates().entry(name.to_owned()).or_default().open = true;
        self.shared.1.notify_all();
    }

    /// How many times something has arrived at the barrier `name`.
    pub fn arrivals(&self, name: &str) -> u64 {
        self.gates().get(name).map_or(0, |gate| gate.arrived)
    }
}
