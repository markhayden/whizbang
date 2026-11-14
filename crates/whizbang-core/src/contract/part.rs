//! A part in the admission (ADR 0015, "A part in the admission").
//!
//! The executor holds a list of parts ([`super::Engine::with_part`]). Each is shown
//! every call that is about to be admitted, before each attempt at the admission's
//! publication: what was asked ([`Asking`]), and the attempt ([`PartStep`]). A part
//! may refuse the call, and it may add records among the names of the run the
//! publication makes. It holds no policy of the executor's: the cap of one call and
//! the age of a preparation are parts, and the executor knows neither.

use std::collections::BTreeSet;

use crate::capability::Shown;
use crate::store::Publication;

use super::record::{RECORD_KIND, REFUSAL_WHY_LIMIT, RefusalKind, is_diagnostic_name};
use super::{Call, CallError, Estimate, PreparedCall, RequestId, RunId, Secrets, StoreFailure};

/// What the name of every record a part adds to a run starts with: the name the part
/// gave follows.
pub const PART_RECORDS_PREFIX: &str = "admission/";

/// The most records the parts of one admission add to its run, between them.
pub const PART_RECORDS_LIMIT: usize = 16;

/// The most bytes one record of a part may hold.
pub const PART_RECORD_MAX_BYTES: usize = 64 * 1024;

/// The name, among a run's names, of the record a part added as `name`. None when
/// `name` is not one a part may give ([`PartStep::put`]).
pub fn part_record_name(name: &str) -> Option<String> {
    is_diagnostic_name(name).then(|| format!("{PART_RECORDS_PREFIX}{name}"))
}

/// What takes part in every admission.
///
/// **When it is asked.** Inside admission, once for every attempt at the admission's
/// publication, after the publication was begun and before the ledger is asked. An
/// attempt the store refuses, because a name was taken meanwhile, is followed by
/// another, and every part is asked again: what it added to the attempt that was
/// refused was published by nothing. So a part keeps no count of what it was asked,
/// and adds its records every time.
///
/// **What it may do.** Return, and the admission goes on to the next part. Refuse
/// the call ([`PartRefusal`]): nothing is published, no run exists, nothing is
/// reserved, and the request's id is not taken by this request. Or fail
/// ([`PartDeclined::Failed`]), which is not a judgement of the call, and publishes
/// nothing either.
///
/// Parts are asked in the order they were given, and the first that does not return
/// is the answer: those after it are not asked.
pub trait AdmissionPart: Send + Sync {
    /// What the part is called, in a refusal and in a failure: a word of the build,
    /// such as `cap`. It is shown escaped.
    fn name(&self) -> &'static str;

    /// Looks at a call that is about to be admitted.
    fn look(&self, asking: &Asking<'_>, step: &mut PartStep<'_, '_>) -> Result<(), PartDeclined>;
}

/// What a part is shown of a call that is about to be admitted.
#[derive(Debug, Clone, Copy)]
pub struct Asking<'a> {
    pub(super) call: &'a Call,
    pub(super) id: Option<&'a RequestId>,
    pub(super) expected: Option<&'a PreparedCall>,
    pub(super) cap: Option<Estimate>,
    pub(super) prepared: &'a PreparedCall,
    pub(super) now_ms: u64,
    pub(super) parent: Option<&'a RunId>,
}

impl<'a> Asking<'a> {
    /// The call, as it was given.
    pub fn call(&self) -> &'a Call {
        self.call
    }

    /// The caller's id for the request, when it gave one.
    pub fn request_id(&self) -> Option<&'a RequestId> {
        self.id
    }

    /// The preparation the request came with, when it came with one
    /// ([`super::Request::expecting`]): what its caller was shown, with the time it
    /// was made ([`PreparedCall::made_ms`]). It prepares the same as
    /// [`Asking::prepared`]: a request whose preparation no longer holds was refused
    /// as stale before any part was asked.
    pub fn expected(&self) -> Option<&'a PreparedCall> {
        self.expected
    }

    /// The cap the request came with, when it came with one
    /// ([`super::Request::capped_at`]), as it was given.
    pub fn cap(&self) -> Option<Estimate> {
        self.cap
    }

    /// The preparation made at admission: what the run will be admitted with.
    pub fn prepared(&self) -> &'a PreparedCall {
        self.prepared
    }

    /// The time of the admission, in milliseconds since the Unix epoch, by the
    /// executor's clock: what the admission will say the run started at.
    pub fn now_ms(&self) -> u64 {
        self.now_ms
    }

    /// The run whose action made the call, for a call from inside an action.
    pub fn parent(&self) -> Option<&'a RunId> {
        self.parent
    }
}

/// A part's refusal of a call: what a record spells it as, and a sentence.
///
/// The kind is one of the kinds a run's record has for a refused call (`run/3`, ADR
/// 0013), chosen by the part: a call from inside an action that a part refuses is
/// listed in its caller's record under it. The sentence is the part's own. It names
/// what was over or missing by a key or a variable, never by a value that could be
/// a secret; the executor takes its secrets out of it all the same, and cuts it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartRefusal {
    kind: RefusalKind,
    why: String,
}

impl PartRefusal {
    /// A refusal of this kind, saying this. None when the kind is of a capability
    /// request and not of a call ([`RefusalKind::is_of_a_request`]), or the
    /// sentence is empty.
    pub fn new(kind: RefusalKind, why: &str) -> Option<PartRefusal> {
        (!kind.is_of_a_request() && !why.is_empty()).then(|| PartRefusal {
            kind,
            why: why.to_owned(),
        })
    }

    pub fn kind(&self) -> RefusalKind {
        self.kind
    }

    /// The sentence as the part gave it: not redacted, not cut and not escaped.
    pub fn why(&self) -> &str {
        &self.why
    }
}

/// Why a part did not let an admission go on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartDeclined {
    /// The call is refused ([`super::Refusal::ByPart`]).
    Refused(PartRefusal),
    /// The part could not judge, or did what a part may not ([`CallError::Part`]),
    /// or the store failed. This is not a judgement of the call.
    Failed(CallError),
}

impl From<CallError> for PartDeclined {
    fn from(error: CallError) -> PartDeclined {
        PartDeclined::Failed(error)
    }
}

impl From<PartRefusal> for PartDeclined {
    fn from(refusal: PartRefusal) -> PartDeclined {
        PartDeclined::Refused(refusal)
    }
}

/// One attempt at an admission, as a part sees it: the run the attempt would make,
/// and a way to add records among that run's names.
pub struct PartStep<'a, 's> {
    publication: &'a mut Publication<'s>,
    run: &'a RunId,
    secrets: &'a Secrets,
    part: &'static str,
    /// The names the parts asked before this one added in this attempt, and this one.
    added: &'a mut BTreeSet<String>,
}

impl<'a, 's> PartStep<'a, 's> {
    pub(super) fn new(
        publication: &'a mut Publication<'s>,
        run: &'a RunId,
        secrets: &'a Secrets,
        part: &'static str,
        added: &'a mut BTreeSet<String>,
    ) -> PartStep<'a, 's> {
        PartStep {
            publication,
            run,
            secrets,
            part,
            added,
        }
    }

    /// The run this attempt would make. Another attempt can name it otherwise.
    pub fn run(&self) -> &RunId {
        self.run
    }

    /// Adds a record to the run this attempt makes, among the run's own names, as
    /// `admission/<name>`: it is published with the run or not at all, it is
    /// readable and never an output, and it goes when the run is deleted.
    ///
    /// The name is one a diagnostic may have ([`is_diagnostic_name`]). A name or a
    /// content that holds a secret is refused before anything else is looked at,
    /// and not shown. A record holds [`PART_RECORD_MAX_BYTES`] at most, the parts
    /// of one admission add [`PART_RECORDS_LIMIT`] between them, and a name is
    /// given once.
    pub fn put(&mut self, name: &str, bytes: &[u8]) -> Result<(), CallError> {
        let part = self.part;
        let refuse = |why: String| Err(CallError::Part { part, why });
        if self.secrets.redact(name) != name || self.secrets.redact_bytes(bytes) != bytes {
            return refuse(
                "it was to write something that must not be written: a record of a part \
                 is stored as it is, so it cannot hold a secret"
                    .to_owned(),
            );
        }
        let Some(full) = part_record_name(name) else {
            return refuse(format!(
                "{} is no name for a record of a part: a lower-case letter or a digit, \
                 then lower-case letters, digits, _, - or ., 64 characters at most",
                Shown(name)
            ));
        };
        if bytes.len() > PART_RECORD_MAX_BYTES {
            return refuse(format!(
                "the record {name} holds {} bytes, and a record of a part holds \
                 {PART_RECORD_MAX_BYTES} at most",
                bytes.len()
            ));
        }
        if self.added.contains(&full) {
            return refuse(format!(
                "the record {name} was added twice to one admission: a name is given once"
            ));
        }
        if self.added.len() >= PART_RECORDS_LIMIT {
            return refuse(format!(
                "the record {name} is one more than the {PART_RECORDS_LIMIT} that the \
                 parts of one admission add"
            ));
        }
        self.publication
            .put_in(self.run.as_str(), &full, RECORD_KIND, bytes)
            .map_err(|error| CallError::Store(StoreFailure::new(error)))?;
        self.added.insert(full);
        Ok(())
    }
}

/// A part's sentence as a refusal keeps it: the secrets taken out while it is whole,
/// then cut, as every refusal's is.
pub(super) fn sentence(secrets: &Secrets, why: &str) -> String {
    let why = super::engine::said(secrets, why);
    why.chars().take(REFUSAL_WHY_LIMIT).collect()
}
