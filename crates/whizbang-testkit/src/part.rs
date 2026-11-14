//! A fake of a part in the admission (ADR 0015, "A part in the admission"). It holds
//! no policy: it refuses, fails or adds a record when a test tells it to, and keeps
//! what it was shown each time it was asked.

use std::sync::{Mutex, MutexGuard, PoisonError};

use whizbang_core::contract::{
    AdmissionPart, Asking, CallError, Estimate, PartDeclined, PartRefusal, PartStep, RefusalKind,
    RequestId, RunId,
};
use whizbang_core::identity::Digest;

/// What a fake part was shown, one time it was asked.
#[derive(Debug, Clone, PartialEq)]
pub struct Seen {
    /// The action the call names.
    pub action: String,
    /// The request's id, when it came with one.
    pub request: Option<RequestId>,
    /// The cap the request came with.
    pub cap: Option<Estimate>,
    /// When the preparation the request came with was made.
    pub expected_made_ms: Option<u64>,
    /// When the preparation made at admission was made, and its fingerprint.
    pub prepared_made_ms: u64,
    pub prepared: Digest,
    /// The time of the admission.
    pub now_ms: u64,
    /// The run the attempt would make.
    pub run: RunId,
    /// The run that made the call, for a call from inside an action.
    pub parent: Option<RunId>,
}

type Hook = Box<dyn FnOnce() + Send>;

#[derive(Default)]
struct Told {
    refuse: Option<(RefusalKind, String)>,
    fail: Option<String>,
    add: Vec<(String, Vec<u8>)>,
    hook: Option<Hook>,
    seen: Vec<Seen>,
}

/// A fake part. It may be shared across threads and across executors.
pub struct FakePart {
    name: &'static str,
    told: Mutex<Told>,
}

impl FakePart {
    /// A part of this name that lets every call through and adds nothing.
    pub fn new(name: &'static str) -> FakePart {
        FakePart {
            name,
            told: Mutex::new(Told::default()),
        }
    }

    fn told(&self) -> MutexGuard<'_, Told> {
        self.told.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// From now on it refuses every call, as this kind and saying this.
    pub fn refuse(&self, kind: RefusalKind, why: &str) {
        self.told().refuse = Some((kind, why.to_owned()));
    }

    /// From now on it fails, saying this, in place of judging.
    pub fn fail(&self, why: &str) {
        self.told().fail = Some(why.to_owned());
    }

    /// From now on it neither refuses nor fails.
    pub fn allow(&self) {
        let mut told = self.told();
        told.refuse = None;
        told.fail = None;
    }

    /// From now on it adds this record, under this name, to every admission, after
    /// those it was already told to add.
    pub fn add(&self, name: &str, bytes: &[u8]) {
        self.told().add.push((name.to_owned(), bytes.to_vec()));
    }

    /// Does this the next time it is asked, once, after it has kept what it was
    /// shown and before it answers. Nothing of the fake is locked meanwhile.
    pub fn when_asked(&self, hook: impl FnOnce() + Send + 'static) {
        self.told().hook = Some(Box::new(hook));
    }

    /// How many times it was asked: once for every attempt at a publication.
    pub fn asked(&self) -> usize {
        self.told().seen.len()
    }

    /// What it was shown, each time it was asked, in order.
    pub fn seen(&self) -> Vec<Seen> {
        self.told().seen.clone()
    }
}

impl AdmissionPart for FakePart {
    fn name(&self) -> &'static str {
        self.name
    }

    fn look(&self, asking: &Asking<'_>, step: &mut PartStep<'_, '_>) -> Result<(), PartDeclined> {
        let (hook, add, fail, refuse) = {
            let mut told = self.told();
            told.seen.push(Seen {
                action: asking.call().action().to_owned(),
                request: asking.request_id().cloned(),
                cap: asking.cap(),
                expected_made_ms: asking.expected().map(|expected| expected.made_ms()),
                prepared_made_ms: asking.prepared().made_ms(),
                prepared: asking.prepared().fingerprint(),
                now_ms: asking.now_ms(),
                run: step.run().clone(),
                parent: asking.parent().cloned(),
            });
            (
                told.hook.take(),
                told.add.clone(),
                told.fail.clone(),
                told.refuse.clone(),
            )
        };
        if let Some(hook) = hook {
            hook();
        }
        for (name, bytes) in add {
            step.put(&name, &bytes)?;
        }
        if let Some(why) = fail {
            return Err(PartDeclined::Failed(CallError::Part {
                part: self.name,
                why,
            }));
        }
        match refuse {
            None => Ok(()),
            Some((kind, why)) => match PartRefusal::new(kind, &why) {
                Some(refusal) => Err(refusal.into()),
                None => Err(PartDeclined::Failed(CallError::Part {
                    part: self.name,
                    why: "the test told it to refuse as a part cannot".to_owned(),
                })),
            },
        }
    }
}
