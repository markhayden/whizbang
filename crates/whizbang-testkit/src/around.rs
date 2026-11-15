//! A fake of what stands in a place between a handle and its backend (ADR 0015, "A
//! wrapper around each request"). It holds no policy: it refuses, fails or stops at a
//! barrier when a test tells it to, and keeps what it saw.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use whizbang_core::capability::{Fault, FaultKind, Stage};
use whizbang_core::contract::{Around, Capability, Ended, Open, RefusalKind, RunId, Stop, Wrapped};
use whizbang_core::egress::Classes;

use crate::barrier::Barriers;

/// One thing a fake wrapper saw, in the order it saw them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Saw {
    /// A request, before it left.
    Before(Request),
    /// How a request it let through ended: the charge an answer stated, or the
    /// fault's kind and stage.
    Answered(Option<u64>),
    Faulted(FaultKind, Stage),
    /// What it held for a request was dropped without being told how the request
    /// ended: the backend panicked.
    Dropped,
}

/// A request as a fake wrapper was told of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub run: RunId,
    pub root: RunId,
    pub capability: Capability,
    /// The backend, the model and the version the backend declares.
    pub model: (String, String, String),
    /// What one request costs at most, as the backend quotes it.
    pub quote_micros: Option<u64>,
    pub classes: Classes,
    /// What the run can spend, and what its place for facts answers a note: true
    /// while the run takes notes.
    pub left_micros: Option<u64>,
}

#[derive(Default)]
struct Told {
    refuse: Option<(RefusalKind, String)>,
    fail_after: Option<String>,
    /// The barriers it stops at, before a request and after one.
    before: Option<(Barriers, String)>,
    after: Option<(Barriers, String)>,
    saw: Vec<Saw>,
}

/// A fake wrapper. Many may share one journal, which then holds what each saw in
/// the order things happened, under its name.
#[derive(Clone)]
pub struct FakeWrapper {
    name: &'static str,
    told: Arc<Mutex<Told>>,
    journal: Arc<Mutex<Vec<(&'static str, Saw)>>>,
}

impl FakeWrapper {
    /// A wrapper of this name that lets every request through, with a journal of
    /// its own.
    pub fn new(name: &'static str) -> FakeWrapper {
        FakeWrapper {
            name,
            told: Arc::default(),
            journal: Arc::default(),
        }
    }

    /// Another wrapper, of this name, that writes in the same journal.
    pub fn beside(&self, name: &'static str) -> FakeWrapper {
        FakeWrapper {
            name,
            told: Arc::default(),
            journal: self.journal.clone(),
        }
    }

    fn told(&self) -> MutexGuard<'_, Told> {
        self.told.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn note(&self, saw: Saw) {
        self.told().saw.push(saw.clone());
        let mut journal = self.journal.lock().unwrap_or_else(PoisonError::into_inner);
        journal.push((self.name, saw));
    }

    /// From now on it refuses every request, as this kind and saying this, with a
    /// fault that says the request was rejected.
    pub fn refuse(&self, kind: RefusalKind, why: &str) {
        self.told().refuse = Some((kind, why.to_owned()));
    }

    /// From now on it fails, saying this, once a request it let through is over.
    pub fn fail_after(&self, why: &str) {
        self.told().fail_after = Some(why.to_owned());
    }

    /// From now on it stops at this barrier before it answers for a request.
    pub fn stop_before(&self, barriers: &Barriers, name: &str) {
        self.told().before = Some((barriers.clone(), name.to_owned()));
    }

    /// From now on it stops at this barrier once a request is over, before it
    /// returns.
    pub fn stop_after(&self, barriers: &Barriers, name: &str) {
        self.told().after = Some((barriers.clone(), name.to_owned()));
    }

    /// What it saw, in order.
    pub fn saw(&self) -> Vec<Saw> {
        self.told().saw.clone()
    }

    /// The requests it saw before they left.
    pub fn requests(&self) -> Vec<Request> {
        let request = |saw: Saw| match saw {
            Saw::Before(request) => Some(request),
            _ => None,
        };
        self.saw().into_iter().filter_map(request).collect()
    }

    /// What every wrapper of this journal saw, each under its name, in the order
    /// things happened.
    pub fn journal(&self) -> Vec<(&'static str, Saw)> {
        let journal = self.journal.lock().unwrap_or_else(PoisonError::into_inner);
        journal.clone()
    }
}

impl Around for FakeWrapper {
    fn before(&self, asked: &Wrapped<'_>) -> Result<Box<dyn Open>, Stop> {
        let model = asked.declaration.model();
        self.note(Saw::Before(Request {
            run: asked.run.clone(),
            root: asked.root.clone(),
            capability: asked.capability,
            model: (
                model.backend().to_owned(),
                model.model().to_owned(),
                model.version().to_owned(),
            ),
            quote_micros: asked.quote.and_then(|quote| quote.bound_micros()),
            classes: asked.classes,
            left_micros: asked.allowance.left_micros(),
        }));
        let (refuse, stop) = {
            let told = self.told();
            (told.refuse.clone(), told.before.clone())
        };
        if let Some((barriers, name)) = stop {
            barriers.stop_at(&name);
        }
        if let Some((kind, why)) = refuse {
            let fault = Fault::new(FaultKind::Rejected, &why);
            if let Some(stop) = Stop::new(kind, &why, fault) {
                return Err(stop);
            }
        }
        Ok(Box::new(Held {
            wrapper: self.clone(),
            told: false,
        }))
    }
}

/// What a fake wrapper holds for one request.
struct Held {
    wrapper: FakeWrapper,
    told: bool,
}

impl Open for Held {
    fn after(mut self: Box<Self>, _: &Wrapped<'_>, ended: &Ended<'_>) -> Result<(), String> {
        self.told = true;
        self.wrapper.note(match ended {
            Ended::Answered(charge) => Saw::Answered(charge.map(|charge| charge.micros())),
            Ended::Faulted(fault) => Saw::Faulted(fault.kind(), fault.stage()),
        });
        let (fail, stop) = {
            let told = self.wrapper.told();
            (told.fail_after.clone(), told.after.clone())
        };
        if let Some((barriers, name)) = stop {
            barriers.stop_at(&name);
        }
        match fail {
            Some(why) => Err(why),
            None => Ok(()),
        }
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        if !self.told {
            self.wrapper.note(Saw::Dropped);
        }
    }
}
