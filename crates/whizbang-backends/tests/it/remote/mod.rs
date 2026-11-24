//! `remote::…`: the kind `remote` (ADR 0046, "The kind `remote`"; plan m6, task R1)
//! against the fake host ([`host`]): the pack's own front on loopback over HTTPS, with
//! the pack's fake models behind it, asked through the production HTTPS client. The
//! gate, the ledger and the run's record are the binary's wiring's, and are shown there
//! (`crates/whizbang/tests/it/remote.rs`). Nothing here contacts a host off the machine.

mod cancel;
pub(crate) mod host;
mod kind;
pub(crate) mod scripted;
mod wire;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use whizbang_backends::models::ModelName;
use whizbang_backends::net::testing::{Checking, Resolve, frames_client_on};
use whizbang_backends::pack::Failed;
use whizbang_backends::pack::proto::{Ask, Scalar};
use whizbang_backends::remote::{Answered, Remote, files_digest};
use whizbang_core::capability::{
    Bounds, Declaration, Fault, FaultKind, Query, SegmentHandle, SegmentRequest, SegmentResponse,
    Segmenter,
};
use whizbang_core::contract::{Cancellation, ModelIdentity, Secrets};
use whizbang_core::egress::{Address, Credential, Recipient, SendCheck, Sender};

pub(crate) use host::{Host, named};

/// One model of the host, asked through a sender of the test's: what a run's handle
/// asks a backend, the answer kept whole for the test.
pub(crate) struct Through {
    pub(crate) remote: Remote,
    pub(crate) sender: Arc<dyn Sender>,
    pub(crate) model: ModelName,
    pub(crate) installation: whizbang_backends::models::InstallationId,
    pub(crate) kept: Mutex<Option<Result<Answered, Box<Failed>>>>,
}

impl Segmenter for Through {
    fn declaration(&self) -> &Declaration {
        Segmenter::declaration(&self.remote)
    }

    fn segment(&self, request: &SegmentRequest, bounds: &Bounds) -> Result<SegmentResponse, Fault> {
        let call = Ask::segment(request, self.model.clone(), self.installation);
        let answered = self.remote.call(&*self.sender, bounds, call);
        let ended = match &answered {
            Ok(_) => Err(Fault::new(FaultKind::Malformed, "kept for the test")),
            Err(failed) => Err(failed.fault.clone()),
        };
        *self.kept.lock().unwrap() = Some(answered);
        ended
    }
}

/// How a test asks a front.
pub(crate) struct Asking {
    pub(crate) base: String,
    pub(crate) pki: Arc<whizbang_backends::net::testing::Pki>,
    pub(crate) records: BTreeMap<String, whizbang_backends::models::InstallationRecord>,
    pub(crate) settings: BTreeMap<String, Scalar>,
    pub(crate) token: String,
    pub(crate) digest: Option<String>,
    pub(crate) resolve: Option<Resolve>,
    pub(crate) dial: Option<whizbang_backends::net::testing::Dial>,
    pub(crate) clock: Arc<dyn whizbang_core::contract::Clock>,
    pub(crate) host_limit: std::time::Duration,
    pub(crate) secrets: Secrets,
    pub(crate) check: SendCheck,
    pub(crate) cancel_within: Option<std::time::Duration>,
}

impl Asking {
    /// Asking the fake host.
    pub(crate) fn of(host: &Host) -> Asking {
        Asking::at(&host.base(), host.pki.clone(), host.records.clone())
    }

    /// Asking the front at `base`, whose certificate is of `pki`'s root, for these
    /// installations.
    pub(crate) fn at(
        base: &str,
        pki: Arc<whizbang_backends::net::testing::Pki>,
        records: BTreeMap<String, whizbang_backends::models::InstallationRecord>,
    ) -> Asking {
        Asking {
            base: base.to_owned(),
            pki,
            records,
            settings: BTreeMap::new(),
            token: host::TOKEN.to_owned(),
            digest: None,
            resolve: None,
            dial: None,
            clock: Arc::new(whizbang_core::contract::SystemClock),
            host_limit: std::time::Duration::from_secs(600),
            secrets: Secrets::none(),
            check: SendCheck::new(),
            cancel_within: None,
        }
    }

    /// Asking a scripted front for the model `none--fake-ok`.
    pub(crate) fn scripted(
        front: &scripted::Scripted,
        pki: &Arc<whizbang_backends::net::testing::Pki>,
    ) -> Asking {
        let model = named(None, "fake-ok");
        let mut records = BTreeMap::new();
        records.insert(model.clone(), host::record(&model).0);
        Asking::at(&front.base(), pki.clone(), records)
    }

    /// The digest of the files of this model's installation.
    pub(crate) fn digest_of(&self, model: &str) -> String {
        files_digest(
            self.records[model]
                .files()
                .iter()
                .map(|file| (file.name().as_str(), file.sha256().to_string())),
        )
    }

    /// The backend of this model and the sender it is asked through.
    pub(crate) fn through(&self, model: &str) -> Through {
        let none = Secrets::none();
        let record = &self.records[model];
        let digest = self.digest.clone().unwrap_or_else(|| self.digest_of(model));
        let base = Address::of(&self.base, &none).unwrap();
        let recipient = Recipient::new(base.origin().clone());
        let identity = ModelIdentity::new(
            "remote",
            model,
            &whizbang_backends::remote::version_of(&digest),
        );
        let features = host::features();
        let features: Vec<&str> = features.iter().map(String::as_str).collect();
        let declaration = Declaration::new(identity, &features)
            .unwrap()
            .sending_to(recipient.clone());
        let remote = Remote::new(
            declaration,
            base.clone(),
            record.model().clone(),
            record.identity(),
            digest,
            self.settings.clone(),
            self.host_limit,
            self.secrets.clone(),
        )
        .unwrap();
        let remote = match self.cancel_within {
            Some(within) => remote.cancelling_within(within),
            None => remote,
        };
        let credential = Credential::new(
            base.origin().clone(),
            "Authorization",
            &format!("Bearer {}", self.token),
            &none,
        )
        .unwrap();
        let client = frames_client_on(
            &self.pki,
            self.clock.clone(),
            self.resolve.clone(),
            self.dial.clone(),
        );
        let sender = Arc::new(Checking::new(
            client,
            recipient,
            Some(credential),
            self.check.clone(),
        ));
        Through {
            remote,
            sender,
            model: record.model().clone(),
            installation: record.identity(),
            kept: Mutex::new(None),
        }
    }

    /// One request of `model` for these queries, under this cancellation and deadline.
    pub(crate) fn ask_under(
        &self,
        model: &str,
        request: &SegmentRequest,
        cancellation: Cancellation,
        deadline: Option<Instant>,
    ) -> Result<Answered, Box<Failed>> {
        let through = Arc::new(self.through(model));
        let mut handle = SegmentHandle::new(through.clone(), cancellation);
        if let Some(deadline) = deadline {
            handle = handle.until(deadline);
        }
        let handled = handle.segment(request);
        let kept = through.kept.lock().unwrap().take();
        kept.unwrap_or_else(|| panic!("the backend was not asked: {handled:?}"))
    }

    pub(crate) fn ask(
        &self,
        model: &str,
        request: &SegmentRequest,
    ) -> Result<Answered, Box<Failed>> {
        self.ask_under(model, request, Cancellation::new(), None)
    }
}

/// The conformance suite's picture and a request for these queries.
pub(crate) fn request(queries: Vec<Query>, most: u32) -> SegmentRequest {
    SegmentRequest::new(crate::pack::conformance::picture(), queries, most).unwrap()
}

/// A phrase.
pub(crate) fn phrase() -> Vec<Query> {
    vec![Query::phrase("a red thing").unwrap()]
}

/// The digest of the files of `none--fake-ok`, which a scripted front names.
pub(crate) fn scripted_digest() -> String {
    let record = host::record(&named(None, "fake-ok")).0;
    files_digest(
        record
            .files()
            .iter()
            .map(|file| (file.name().as_str(), file.sha256().to_string())),
    )
}

/// A clock that moves on by `step` each time it is read, from the machine's time when it
/// was made: what a test of a long silence of a host hands the client, so that minutes
/// pass on it while the test waits a moment.
pub(crate) struct Stepping {
    base: Instant,
    ms: std::sync::atomic::AtomicU64,
    step: u64,
}

impl Stepping {
    pub(crate) fn by(step_ms: u64) -> Arc<Stepping> {
        Arc::new(Stepping {
            base: Instant::now(),
            ms: std::sync::atomic::AtomicU64::new(0),
            step: step_ms,
        })
    }

    /// How far it has moved.
    pub(crate) fn moved(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.ms.load(std::sync::atomic::Ordering::SeqCst))
    }
}

impl whizbang_core::contract::Clock for Stepping {
    fn now_ms(&self) -> u64 {
        self.ms
            .fetch_add(self.step, std::sync::atomic::Ordering::SeqCst)
    }

    fn now(&self) -> Instant {
        self.base + std::time::Duration::from_millis(self.now_ms())
    }
}
