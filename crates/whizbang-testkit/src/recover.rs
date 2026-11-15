//! A fake of a part in the recovery of a store (ADR 0015, "One way to a writer, with
//! recovery before it"). It recovers nothing of its own: it makes the names a test
//! tells it to, each unless the store holds it, fails, panics or stops at a barrier
//! when a test says so, and keeps what it was shown each time it was asked.

use std::sync::{Mutex, MutexGuard, PoisonError};

use whizbang_core::contract::{Recovering, RecoveryFailed, RecoveryPart};

use crate::barrier::Barriers;

/// What a fake part was shown, one time it was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    /// The number of the generation the store was found at.
    pub generation: u64,
    /// The runs that generation holds, and whether each is open in this process.
    pub runs: Vec<(String, bool)>,
    /// The time of the recovery.
    pub now_ms: u64,
    /// How many names it made, this time.
    pub made: usize,
}

type Hook = Box<dyn FnOnce() + Send>;

#[derive(Default)]
struct Told {
    make: Vec<(String, Vec<u8>)>,
    make_in_runs: Vec<(String, Vec<u8>)>,
    blindly: Vec<(Option<String>, String, Vec<u8>)>,
    fail: Option<String>,
    panic: Option<String>,
    stop: Option<(Barriers, String)>,
    hook: Option<Hook>,
    shown: Vec<Shown>,
}

/// A fake part of a recovery. It may be shared across threads and across openings.
pub struct FakeRecovery {
    name: &'static str,
    told: Mutex<Told>,
}

impl FakeRecovery {
    /// A part of this name that makes nothing.
    pub fn new(name: &'static str) -> FakeRecovery {
        FakeRecovery {
            name,
            told: Mutex::new(Told::default()),
        }
    }

    fn told(&self) -> MutexGuard<'_, Told> {
        self.told.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// From now on it makes this name, outside runs, with this content, in every
    /// recovery that finds a store without it.
    pub fn make(&self, name: &str, bytes: &[u8]) {
        self.told().make.push((name.to_owned(), bytes.to_vec()));
    }

    /// From now on it makes this name, with this content, in every run the store
    /// holds that has no such name.
    pub fn make_in_runs(&self, name: &str, bytes: &[u8]) {
        let made = (name.to_owned(), bytes.to_vec());
        self.told().make_in_runs.push(made);
    }

    /// From now on it makes this name without looking whether the store holds it:
    /// outside runs, or in this run. What a part does that does not keep to the
    /// rule of a name made once.
    pub fn make_blindly(&self, run: Option<&str>, name: &str, bytes: &[u8]) {
        let made = (run.map(str::to_owned), name.to_owned(), bytes.to_vec());
        self.told().blindly.push(made);
    }

    /// From now on it fails, saying this, after it has made what it was told to.
    pub fn fail(&self, why: &str) {
        self.told().fail = Some(why.to_owned());
    }

    /// From now on it panics, with this text, after it has made what it was told to.
    pub fn panic_with(&self, text: &str) {
        self.told().panic = Some(text.to_owned());
    }

    /// From now on it neither fails nor panics.
    pub fn allow(&self) {
        let mut told = self.told();
        told.fail = None;
        told.panic = None;
    }

    /// From now on it stops at this barrier each time it is asked, before it makes
    /// anything.
    pub fn stop_at(&self, barriers: &Barriers, name: &str) {
        self.told().stop = Some((barriers.clone(), name.to_owned()));
    }

    /// Does this the next time it is asked, once, before it makes anything. Nothing
    /// of the fake is locked meanwhile.
    pub fn when_asked(&self, hook: impl FnOnce() + Send + 'static) {
        self.told().hook = Some(Box::new(hook));
    }

    /// How many times it was asked: once for every recovery.
    pub fn asked(&self) -> usize {
        self.told().shown.len()
    }

    /// What it was shown, each time it was asked, in order.
    pub fn shown(&self) -> Vec<Shown> {
        self.told().shown.clone()
    }
}

impl RecoveryPart for FakeRecovery {
    fn name(&self) -> &'static str {
        self.name
    }

    fn recover(&self, step: &mut Recovering<'_, '_>) -> Result<(), RecoveryFailed> {
        let runs: Vec<String> = step.generation().runs().keys().cloned().collect();
        let (hook, stop) = {
            let mut told = self.told();
            (told.hook.take(), told.stop.clone())
        };
        if let Some(hook) = hook {
            hook();
        }
        if let Some((barriers, name)) = stop {
            barriers.stop_at(&name);
        }
        let (make, make_in_runs, blindly, fail, panic) = {
            let told = self.told();
            (
                told.make.clone(),
                told.make_in_runs.clone(),
                told.blindly.clone(),
                told.fail.clone(),
                told.panic.clone(),
            )
        };
        let mut made = 0;
        let mut failed = None;
        for (name, bytes) in &make {
            if step.generation().get(name).is_none() {
                match step.make(name, bytes) {
                    Ok(()) => made += 1,
                    Err(error) => failed = failed.or(Some(error)),
                }
            }
        }
        for (name, bytes) in &make_in_runs {
            for run in &runs {
                let held = step.generation().run(run).and_then(|run| run.get(name));
                if held.is_none() {
                    match step.make_in(run, name, bytes) {
                        Ok(()) => made += 1,
                        Err(error) => failed = failed.or(Some(error)),
                    }
                }
            }
        }
        for (run, name, bytes) in &blindly {
            let answered = match run {
                Some(run) => step.make_in(run, name, bytes),
                None => step.make(name, bytes),
            };
            match answered {
                Ok(()) => made += 1,
                Err(error) => failed = failed.or(Some(error)),
            }
        }
        let shown = Shown {
            generation: step.generation().number(),
            runs: runs
                .iter()
                .map(|run| (run.clone(), step.open_here(run)))
                .collect(),
            now_ms: step.now_ms(),
            made,
        };
        self.told().shown.push(shown);
        if let Some(text) = panic {
            panic!("{text}");
        }
        match (failed, fail) {
            (Some(error), _) => Err(error),
            (None, Some(why)) => Err(RecoveryFailed::because(&why)),
            (None, None) => Ok(()),
        }
    }
}
