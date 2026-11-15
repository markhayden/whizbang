//! A fake of the config's source (ADR 0015, "One snapshot of the config"): it answers
//! the snapshot a test gave it, which the test swaps, or refuses, or panics, as it was
//! told; and it counts each time it was asked.

use std::sync::{Mutex, MutexGuard, PoisonError};

use whizbang_core::contract::{ConfigSource, Snapshot, SourceRefused};

type Hook = Box<dyn FnOnce() + Send>;

struct Told {
    answer: Result<Snapshot, SourceRefused>,
    panic: bool,
    hook: Option<Hook>,
    asked: usize,
}

/// A fake source. It may be shared across threads and across executors.
pub struct FakeSource {
    told: Mutex<Told>,
}

impl FakeSource {
    /// A source that answers this snapshot until it is told otherwise.
    pub fn of(snapshot: Snapshot) -> FakeSource {
        FakeSource {
            told: Mutex::new(Told {
                answer: Ok(snapshot),
                panic: false,
                hook: None,
                asked: 0,
            }),
        }
    }

    fn told(&self) -> MutexGuard<'_, Told> {
        self.told.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// From now on it answers this snapshot: the config changed.
    pub fn swap(&self, snapshot: Snapshot) {
        let mut told = self.told();
        (told.answer, told.panic) = (Ok(snapshot), false);
    }

    /// From now on it refuses, saying this.
    pub fn refuse(&self, why: &'static str) {
        let mut told = self.told();
        (told.answer, told.panic) = (Err(SourceRefused::because(why)), false);
    }

    /// From now on it panics when it is asked, until it is told otherwise.
    pub fn panic(&self) {
        self.told().panic = true;
    }

    /// Does this the next time it is asked, once, before it answers. Nothing of the
    /// fake is locked meanwhile.
    pub fn when_asked(&self, hook: impl FnOnce() + Send + 'static) {
        self.told().hook = Some(Box::new(hook));
    }

    /// How many times it was asked.
    pub fn asked(&self) -> usize {
        self.told().asked
    }
}

impl ConfigSource for FakeSource {
    fn snapshot(&self) -> Result<Snapshot, SourceRefused> {
        let hook = {
            let mut told = self.told();
            told.asked += 1;
            told.hook.take()
        };
        if let Some(hook) = hook {
            hook();
        }
        let told = self.told();
        assert!(!told.panic, "the config's source went wrong");
        told.answer.clone()
    }
}
