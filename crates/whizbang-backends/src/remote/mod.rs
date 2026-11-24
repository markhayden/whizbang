//! The kind `remote` (ADR 0046, "The kind `remote`"; plan m6, task R1): a segmenter
//! answered by the pack's front on another machine, `pack/1` carried by HTTPS.
//!
//! **A table of the kind** is under `[backends.segment]` alone (a remote fill or judge is
//! milestone 8's): `kind = "remote"`; a `url` that is `https`, the front's base address,
//! to which `/pack/1/request`, `/pack/1/cancel` and `/pack/1/health` are appended;
//! `token_env`, the front's token, sent as `Authorization: Bearer` to the url's origin and
//! to no other; `model`, a model of the catalogue; and settings: the price,
//! `price_microusd_per_second` and `host_limit_s`, both or neither, and the model's own
//! settings the kind `pack` takes (`threshold`, `pad`, `min_cover`, `outline`,
//! `device`), each sent in the `load`.
//!
//! **Its price** is by time under the host's limit (`SPEC.md` section 6, as amended by
//! ADR 0046): `price_microusd_per_second` times `host_limit_s` times [`ATTEMPTS`], the
//! allowance of attempts, the milliseconds worked out with checked multiplications. A
//! table that names neither setting makes a backend with no price, and a backend that
//! names a recipient and no price has no bound, so every call to it is refused (ADR
//! 0012); one that names one without the other, a zero, or a product that does not fit
//! is refused when the config is read, naming the setting.
//!
//! **Its identity** is the files the catalogue's entry names, as the installation of
//! this build's adapter records them: the digest of `<name> sha256:<hex>\n` lines sorted
//! by the bytes of the name ([`files_digest`]), which the front names in
//! `Whizbang-Model-Files` of every answer it verified before its load. An answer that
//! names none, a malformed one or another is refused whole before anything of it is
//! used, and never kept. The backend declares its model at that digest, as its version
//! (`files-` and its hex digits, [`version_of`]), so a run's record names the files asked; the answer's own digest, the
//! request's dispatch id and its device are said beside the response.
//!
//! **What is charged.** A request refused before anything left is settled at nothing.
//! Every other ends unresolved, and is charged at its bound, whatever its outcome record
//! says of its stage: the stage is the worker's, kept as a diagnostic, and the host may
//! have started a machine for the request and billed it.
//!
//! **Its cancel.** When the run's signal is set or the request's deadline passes, the
//! stream is closed and a `cancel` follows it through the checked control send, under
//! bounds of its own ([`Bounds::for_cleanup`], [`CONTROL_WITHIN`]); a host that does not
//! answer it is let go of then.

pub(crate) mod answer;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use sha2::{Digest as _, Sha256};
use whizbang_core::capability::{
    Bounds, Declaration, Device, Fault, FaultKind, Instance, Picture, Price, SegmentRequest,
    SegmentResponse, Segmenter, Served, Stage, TimeLimit,
};
use whizbang_core::config::{Kind, KindRefused, Setting, Table};
use whizbang_core::contract::{Capability, Made, ModelIdentity, Secrets};
use whizbang_core::egress::{
    Address, CONTROL_WITHIN, Control, Credential, DataClass, FrameBody, Frames, Piece, Recipient,
    Sender,
};
use whizbang_core::identity::CanonicalPixels;

use crate::models::{Catalogue, InstallationId, InstallationRecord, ModelName, Revision};
use crate::pack::proto::{self, Answer, Ask, Message, Role, Scalar};
use crate::pack::{Called, Failed};

/// The name a config gives the kind: `kind = "remote"`.
pub const KIND: &str = "remote";

/// The attempts one call is priced for (ADR 0046, "What bounds a request's bill on
/// Modal"): whizbang's, and one restart of the host's.
pub const ATTEMPTS: u64 = 2;

/// The routes of the front, after its base address.
pub const REQUEST_ROUTE: &str = "/pack/1/request";
pub const CANCEL_ROUTE: &str = "/pack/1/cancel";
pub const HEALTH_ROUTE: &str = "/pack/1/health";

/// The field that names a request's dispatch id.
pub const DISPATCH_FIELD: &str = "Whizbang-Dispatch";
/// The field of an answer that names the digest of the files it came from.
pub const FILES_FIELD: &str = "Whizbang-Model-Files";
/// The field of an answer that failed before its files were verified.
pub const UNVERIFIED_FIELD: &str = "Whizbang-Unverified";
/// The header the token is carried in, as `Bearer` and the token.
pub const TOKEN_HEADER: &str = "Authorization";

/// The settings of a model the kind sends in its `load`: the kind `pack`'s.
pub const MODEL_SETTINGS: [&str; 5] = ["device", "min_cover", "outline", "pad", "threshold"];

pub const NOT_SEGMENT: &str = "a remote backend answers segment alone in this build, and is \
                               offered under backends.segment; a remote fill or judge is \
                               milestone 8's";
pub const NO_URL: &str = "a remote backend names its front's url";
pub const NOT_HTTPS: &str = "a remote backend's url is https: the client speaks https only";
pub const NO_TOKEN: &str = "a remote backend names its front's token with token_env, and no \
                            key_env and no provider";
pub const NO_MODEL: &str = "a remote backend names its model";
pub const NOT_IN_CATALOGUE: &str = "a remote backend's model is one the catalogue holds";
pub const PRICE_PAIR: &str = "a remote backend gives price_microusd_per_second and \
                              host_limit_s together, or neither";
pub const BAD_PRICE: &str = "price_microusd_per_second is a whole number of millionths of a US \
                             dollar from 1";
pub const BAD_HOST_LIMIT: &str = "host_limit_s is a whole number of seconds from 1, whose \
                                  price over the allowance of 2 attempts is an amount";
pub const BAD_SETTING: &str = "a remote backend's settings are price_microusd_per_second, \
                               host_limit_s, and the model's own: threshold, pad and \
                               min_cover (a text of a number from 0 to 1), outline (a \
                               text) and device (auto or cpu)";
const NOT_DECLARED: &str = "a remote backend cannot declare that model";

/// What a model the host is to hold is, to this build: its installation, the digest of
/// its files, and its features.
#[derive(Debug, Clone)]
struct Hosted {
    model: ModelName,
    installation: InstallationId,
    digest: String,
    features: Vec<String>,
}

/// The digest an answer names in `Whizbang-Model-Files` (ADR 0046, "The identity"):
/// `sha256:` and the SHA-256, in lower-case hex, of a line `<name> sha256:<hex>\n` for
/// each file, in the order of the bytes of the name.
pub fn files_digest<'a>(files: impl IntoIterator<Item = (&'a str, String)>) -> String {
    let mut lines: Vec<(&str, String)> = files.into_iter().collect();
    lines.sort_by(|one, other| one.0.as_bytes().cmp(other.0.as_bytes()));
    let mut hashed = Sha256::new();
    for (name, digest) in lines {
        hashed.update(name.as_bytes());
        hashed.update(b" ");
        hashed.update(digest.as_bytes());
        hashed.update(b"\n");
    }
    let mut text = String::from("sha256:");
    for byte in hashed.finalize() {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

/// The version a backend of the kind declares its model at: `files-` and the 64 hex
/// digits of the digest of its files. Not the digest's own text, `sha256:` and the
/// digits, which a version would be taken for an installation's identity in, and a
/// local model's (`whizbang_core::capability::Installation::from_version`).
pub fn version_of(digest: &str) -> String {
    format!("files-{}", digest.strip_prefix("sha256:").unwrap_or(digest))
}

impl Hosted {
    fn of(record: &InstallationRecord, features: Vec<String>) -> Hosted {
        let files = record
            .files()
            .iter()
            .map(|file| (file.name().as_str(), file.sha256().to_string()));
        Hosted {
            model: record.model().clone(),
            installation: record.identity(),
            digest: files_digest(files),
            features,
        }
    }
}

/// The kind `remote`, over the models a host may be asked for. What `{:?}` prints is
/// how many.
#[derive(Clone, Default)]
pub struct RemoteKind {
    hosted: Arc<BTreeMap<String, Hosted>>,
}

impl std::fmt::Debug for RemoteKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RemoteKind({} models)", self.hosted.len())
    }
}

impl RemoteKind {
    /// The kind over this catalogue, under these adapters and their revisions: each
    /// entry whose adapter is among them is a model a host may be asked for, as the
    /// installation `whizbang models get` would make of it under this build.
    pub fn over(
        catalogue: &Catalogue,
        adapters: &[(ModelName, Revision)],
        secrets: &Secrets,
    ) -> RemoteKind {
        let mut hosted = BTreeMap::new();
        for entry in catalogue.entries() {
            let Some((_, revision)) = adapters.iter().find(|(name, _)| name == entry.adapter())
            else {
                continue;
            };
            let Ok(record) = InstallationRecord::of(entry, revision.clone(), secrets) else {
                continue;
            };
            let features = entry.features().map(str::to_owned).collect();
            hosted.insert(
                entry.name().as_str().to_owned(),
                Hosted::of(&record, features),
            );
        }
        RemoteKind {
            hosted: Arc::new(hosted),
        }
    }

    /// The kind over these installations, each a model a host may be asked for, with
    /// the features each declares: what a test gives for the pack's fake models.
    pub fn hosting(records: &[(InstallationRecord, Vec<String>)]) -> RemoteKind {
        let hosted = records
            .iter()
            .map(|(record, features)| {
                let hosted = Hosted::of(record, features.clone());
                (record.model().as_str().to_owned(), hosted)
            })
            .collect();
        RemoteKind {
            hosted: Arc::new(hosted),
        }
    }

    /// The installation and the digest of its files a host is asked for under this
    /// model's name: what `whizbang backends status --probe` holds a host's health to.
    pub fn expected(&self, model: &str) -> Option<(InstallationId, String)> {
        let hosted = self.hosted.get(model)?;
        Some((hosted.installation, hosted.digest.clone()))
    }
}

/// What a table of the kind says, read.
struct Read {
    address: Address,
    hosted: Hosted,
    settings: BTreeMap<String, Scalar>,
    price: Option<(u64, u64)>,
}

/// Whether a text is a number from 0 to 1 written as digits with at most one point, as
/// the kind `pack` takes one.
fn fraction(text: &str) -> bool {
    let digits = text.bytes().filter(u8::is_ascii_digit).count();
    let points = text.bytes().filter(|byte| *byte == b'.').count();
    digits >= 1
        && points <= 1
        && digits + points == text.len()
        && text
            .parse::<f64>()
            .is_ok_and(|number| (0.0..=1.0).contains(&number))
}

impl RemoteKind {
    fn read(&self, table: &Table<'_>) -> Result<Read, KindRefused> {
        let refused = KindRefused::Table;
        if table.capability() != Capability::Segment {
            return Err(refused(NOT_SEGMENT));
        }
        let backend = table.backend();
        let address = backend.address().ok_or(refused(NO_URL))?.clone();
        if address.origin().scheme() != "https" {
            return Err(refused(NOT_HTTPS));
        }
        if table.key().is_some() || backend.provider().is_some() {
            return Err(refused(NO_TOKEN));
        }
        if !backend.credentials().any(|(key, _)| key == "token_env") {
            return Err(refused(NO_TOKEN));
        }
        let model = backend.model().ok_or(refused(NO_MODEL))?;
        let hosted = self
            .hosted
            .get(model)
            .ok_or(refused(NOT_IN_CATALOGUE))?
            .clone();
        let mut settings = BTreeMap::new();
        let (mut per_second, mut limit) = (None, None);
        for (key, setting) in backend.settings() {
            match (key, setting) {
                ("price_microusd_per_second", Setting::Integer(micros)) => {
                    let micros = u64::try_from(*micros)
                        .ok()
                        .filter(|micros| *micros >= 1)
                        .ok_or(refused(BAD_PRICE))?;
                    per_second = Some(micros);
                }
                ("price_microusd_per_second", _) => return Err(refused(BAD_PRICE)),
                ("host_limit_s", Setting::Integer(seconds)) => {
                    let seconds = u64::try_from(*seconds)
                        .ok()
                        .filter(|seconds| *seconds >= 1)
                        .ok_or(refused(BAD_HOST_LIMIT))?;
                    limit = Some(seconds);
                }
                ("host_limit_s", _) => return Err(refused(BAD_HOST_LIMIT)),
                ("threshold" | "pad" | "min_cover", Setting::Text(text)) if fraction(text) => {
                    settings.insert(key.to_owned(), Scalar::Text(text.clone()));
                }
                ("outline", Setting::Text(text)) => {
                    settings.insert(key.to_owned(), Scalar::Text(text.clone()));
                }
                ("device", Setting::Text(text)) if text == "auto" || text == "cpu" => {
                    settings.insert(key.to_owned(), Scalar::Text(text.clone()));
                }
                _ => return Err(refused(BAD_SETTING)),
            }
        }
        for (name, value) in &settings {
            proto::check_setting(name, value).map_err(|_| refused(BAD_SETTING))?;
        }
        let price = match (per_second, limit) {
            (Some(per_second), Some(limit)) => Some((per_second, limit)),
            (None, None) => None,
            _ => return Err(refused(PRICE_PAIR)),
        };
        Ok(Read {
            address,
            hosted,
            settings,
            price,
        })
    }
}

/// The price of a call: `per_second` millionths of a dollar a second for
/// `host_limit_s` seconds an attempt, over [`ATTEMPTS`] attempts, a limit the host
/// enforces; the milliseconds worked out with checked multiplications.
pub fn price_of(per_second: u64, host_limit_s: u64) -> Result<Price, KindRefused> {
    let refused = KindRefused::Table(BAD_HOST_LIMIT);
    let ms = host_limit_s
        .checked_mul(1000)
        .and_then(|ms| ms.checked_mul(ATTEMPTS))
        .ok_or(refused)?;
    let limit = TimeLimit::of_host(ms).map_err(|_| refused)?;
    // A price a second is whole millionths; one past what an `f64` holds exactly is no
    // amount.
    if per_second > (1 << 53) {
        return Err(KindRefused::Table(BAD_PRICE));
    }
    Price::by_time(per_second as f64, limit).map_err(|_| refused)
}

impl Kind for RemoteKind {
    fn name(&self) -> &str {
        KIND
    }

    fn make(&self, table: &Table<'_>) -> Result<Made, KindRefused> {
        let read = self.read(table)?;
        let features: Vec<&str> = read.hosted.features.iter().map(String::as_str).collect();
        let version = version_of(&read.hosted.digest);
        let identity = ModelIdentity::new(KIND, read.hosted.model.as_str(), &version);
        let mut declaration = Declaration::new(identity, &features)
            .map_err(|_| KindRefused::Table(NOT_DECLARED))?
            .sending_to(Recipient::new(read.address.origin().clone()));
        let host_limit = match read.price {
            Some((per_second, limit_s)) => {
                declaration = declaration.priced(price_of(per_second, limit_s)?);
                Duration::from_secs(limit_s)
            }
            // No price: the backend has no bound, and no call to it is admitted; its
            // requests are never sent, so their wait is never asked.
            None => Duration::from_secs(1),
        };
        let remote = Remote::new(
            declaration,
            read.address,
            read.hosted.model,
            read.hosted.installation,
            read.hosted.digest,
            read.settings,
            host_limit,
            table.secrets().clone(),
        )
        .map_err(KindRefused::Credential)?;
        Ok(Made::Segment(Arc::new(remote)))
    }

    /// The token, as `Bearer` and the token under `Authorization`, bound to the origin
    /// of the front's url and to no other.
    fn credential(&self, table: &Table<'_>) -> Result<Option<Credential>, KindRefused> {
        let read = self.read(table)?;
        let token = table.token().ok_or(KindRefused::Table(NO_TOKEN))?;
        Credential::new(
            read.address.origin().clone(),
            TOKEN_HEADER,
            &format!("Bearer {}", token.expose()),
            table.secrets(),
        )
        .map(Some)
        .map_err(KindRefused::Credential)
    }
}

/// A backend of the kind: one model at one front, with its settings and its limit.
pub struct Remote {
    declaration: Declaration,
    request_to: Address,
    cancel_to: Address,
    model: ModelName,
    installation: InstallationId,
    digest: String,
    settings: BTreeMap<String, Scalar>,
    host_limit: Duration,
    secrets: Secrets,
    /// How long a cancel has, from its connect to its last byte: [`CONTROL_WITHIN`].
    cancel_within: Duration,
}

impl std::fmt::Debug for Remote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Remote({:?})", self.declaration.model())
    }
}

/// The address of a route under a front's base address.
fn route(
    base: &Address,
    route: &str,
    secrets: &Secrets,
) -> Result<Address, whizbang_core::egress::NotAllowed> {
    let rest = base.rest().trim_end_matches('/');
    Address::of(&format!("{}{rest}{route}", base.origin()), secrets)
}

/// The address of a front's health document, under its base address.
pub fn health_of(
    base: &Address,
    secrets: &Secrets,
) -> Result<Address, whizbang_core::egress::NotAllowed> {
    route(base, HEALTH_ROUTE, secrets)
}

impl Remote {
    /// A backend of this declaration at the front at `base`, asking for this
    /// installation of this model, whose files are this digest, with these settings,
    /// each request's answer waited for at most `host_limit`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        declaration: Declaration,
        base: Address,
        model: ModelName,
        installation: InstallationId,
        digest: String,
        settings: BTreeMap<String, Scalar>,
        host_limit: Duration,
        secrets: Secrets,
    ) -> Result<Remote, whizbang_core::egress::NotAllowed> {
        Ok(Remote {
            request_to: route(&base, REQUEST_ROUTE, &secrets)?,
            cancel_to: route(&base, CANCEL_ROUTE, &secrets)?,
            declaration,
            model,
            installation,
            digest,
            settings,
            host_limit,
            secrets,
            cancel_within: CONTROL_WITHIN,
        })
    }

    /// The same, a cancel given this long in place of [`CONTROL_WITHIN`]: for a test
    /// that shows a cancel let go of at its deadline without waiting five seconds. It
    /// is compiled only under the feature `fakes`.
    #[cfg(any(test, feature = "fakes"))]
    #[must_use]
    pub fn cancelling_within(mut self, within: Duration) -> Remote {
        self.cancel_within = within;
        self
    }

    /// One request of the front: the `load` of this backend's model with its settings,
    /// and `call` (a `segment` of the same model), through `sender` under `bounds`:
    /// the call's answer, or its failure as the local supervisor tells one (its fault,
    /// the worker's sentence, how the worker ended and its error output, redacted).
    /// The conformance suite asks this, as the local transport asks the supervisor's
    /// queue; [`Segmenter::segment`] asks it through the checked sender.
    pub fn call(
        &self,
        sender: &dyn Sender,
        bounds: &Bounds,
        call: Ask,
    ) -> Result<Answered, Box<Failed>> {
        let dispatch = dispatch_id().map_err(|fault| failed(fault, &self.secrets))?;
        // Every failure names the request's dispatch id, which whizbang minted and which
        // is no secret: a request charged at its bound is found by it at the host.
        self.exchange(sender, bounds, call, &dispatch)
            .map_err(|mut failed| {
                failed.fault = naming(&failed.fault, &dispatch);
                failed
            })
    }

    /// One request under this dispatch id: [`Remote::call`] without the naming.
    fn exchange(
        &self,
        sender: &dyn Sender,
        bounds: &Bounds,
        call: Ask,
        dispatch: &str,
    ) -> Result<Answered, Box<Failed>> {
        let dispatch = dispatch.to_owned();
        let load = Ask::Load {
            model: self.model.clone(),
            installation: self.installation,
            settings: self.settings.clone(),
        };
        let body = Body::new(format!("whizbang.{dispatch}"), load, call)
            .map_err(|fault| failed(fault, &self.secrets))?;
        let body = Arc::new(body);
        let frames = Frames::new(self.request_to.clone(), body.clone(), self.host_limit)
            .with_field(DISPATCH_FIELD, &dispatch)
            .map_err(|_| {
                let fault =
                    Fault::new(FaultKind::Malformed, "the dispatch id is no field").not_sent();
                failed(fault, &self.secrets)
            })?;
        let mut read = None;
        let mut take = |head: &whizbang_core::egress::Head, input: &mut dyn std::io::Read| {
            let answered = answer::read(head, input, &self.digest, &body, &self.secrets);
            let ended = match &answered {
                Ok(answer::Came::Called(_)) => Ok(()),
                Ok(answer::Came::Failed(failed)) => Err(failed.fault.clone()),
                Err(fault) => Err(fault.clone()),
            };
            read = Some(answered);
            ended
        };
        let sent = sender.send_frames(&frames, bounds, &mut take);
        let mut ended = sent.ended;
        let read = read.take();
        // A request whizbang gave up on, at its signal or its deadline, before its answer
        // was read whole, is told to stop, through a control send of its own. One whose
        // host answered it whole, a cancelled or timed-out end included, has ended there.
        let answered_whole = matches!(read, Some(Ok(_)));
        if let (Err(fault), Some(dispatched)) = (&ended, &sent.dispatched)
            && matches!(fault.kind(), FaultKind::Cancelled | FaultKind::TimedOut)
            && !answered_whole
        {
            self.cancel(sender, bounds, dispatched, &dispatch, &body.conversation);
        }
        if let Err(fault) = &ended
            && sent.dispatched.is_some()
        {
            // Whatever came of a request that left, it is charged at its bound.
            ended = Err(unresolved(fault.clone()));
        }
        match (ended, read) {
            (Ok(()), Some(Ok(answer::Came::Called(called)))) => Ok(Answered {
                called,
                dispatch,
                digest: self.digest.clone(),
            }),
            (Err(fault), Some(Ok(answer::Came::Failed(mut failed)))) => {
                failed.fault = fault;
                Err(failed)
            }
            (Err(fault), _) => Err(failed(fault, &self.secrets)),
            (Ok(()), _) => Err(failed(
                Fault::new(FaultKind::Malformed, answer::NO_RESULT).unresolved(),
                &self.secrets,
            )),
        }
    }

    /// The `cancel` of a request given up on: whizbang's hello and a `cancel` frame of
    /// the request's call, naming its dispatch id, through the checked control send
    /// under bounds of its own. What it comes to is not waited for past them.
    fn cancel(
        &self,
        sender: &dyn Sender,
        bounds: &Bounds,
        dispatched: &whizbang_core::egress::Dispatched,
        dispatch: &str,
        conversation: &str,
    ) {
        let mut bytes = Vec::new();
        let hello = Message::Hello {
            id: conversation.to_owned(),
            role: Role::Whizbang,
        };
        let cancel = Message::Cancel {
            id: answer::CALL_ID.to_owned(),
        };
        if proto::write(&hello, &mut bytes).is_err() || proto::write(&cancel, &mut bytes).is_err() {
            return;
        }
        let Ok(control) = Control::new(self.cancel_to.clone(), bytes)
            .and_then(|control| control.with_field(DISPATCH_FIELD, dispatch))
        else {
            return;
        };
        let cleanup = bounds.for_cleanup(self.cancel_within);
        let _ = sender.control(dispatched, &control, &cleanup);
    }
}

/// A fault of a request that left, unresolved: charged at its bound.
fn unresolved(fault: Fault) -> Fault {
    match fault.stage() {
        Stage::Unresolved => fault,
        _ => Fault::new(fault.kind(), fault.sentence()).unresolved(),
    }
}

/// The same fault, its sentence led by the request's dispatch id, and its kind, its stage
/// and its charge kept.
fn naming(fault: &Fault, dispatch: &str) -> Fault {
    let named = Fault::new(
        fault.kind(),
        &format!("dispatch {dispatch}: {}", fault.sentence()),
    );
    match fault.stage() {
        Stage::NotSent => named.not_sent(),
        Stage::Unresolved => named.unresolved(),
        Stage::Answered => named.answered(fault.charge()),
    }
}

/// A failure with nothing beside its fault, whose sentence is fixed words.
fn failed(fault: Fault, _secrets: &Secrets) -> Box<Failed> {
    Box::new(Failed {
        fault,
        said: None,
        ended: None,
        error_output: Vec::new(),
    })
}

/// A new dispatch id: 128 bits from the system's randomness, as 32 lower-case hex
/// digits.
fn dispatch_id() -> Result<String, Fault> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| {
        Fault::new(
            FaultKind::Unavailable,
            "the system's randomness could not be read, so no dispatch id was made",
        )
        .not_sent()
    })?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// A call the front answered: what the worker answered, the request's dispatch id and
/// the digest of the files the answer came from.
#[derive(Debug)]
pub struct Answered {
    pub called: Called,
    pub dispatch: String,
    pub digest: String,
}

/// The body of a request: whizbang's hello, the `load` and the call, written by
/// `pack/1`'s own codec from this value alone.
pub(crate) struct Body {
    pub(crate) conversation: String,
    pub(crate) load: Ask,
    pub(crate) call: Ask,
    /// The call's image as a picture, which the check is shown as pixels: made from the
    /// raster the frames carry, so the two are one.
    picture: Option<Picture>,
}

/// What a request whose image is not a picture of the core is refused with.
pub const NOT_A_PICTURE: &str = "the request's image is not a picture the check can be shown, \
                                 so nothing was sent";

impl Body {
    /// The body of these, refused, before anything leaves, when the call's image is
    /// not a picture of the core: the check is shown every part, or nothing is sent.
    fn new(conversation: String, load: Ask, call: Ask) -> Result<Body, Fault> {
        let picture = match &call {
            Ask::Segment { image, .. } => {
                let refused = || Fault::new(FaultKind::Rejected, NOT_A_PICTURE).not_sent();
                let pixels =
                    CanonicalPixels::from_rgba(image.width, image.height, image.samples.clone())
                        .map_err(|_| refused())?;
                Some(Picture::new(pixels).map_err(|_| refused())?)
            }
            _ => None,
        };
        Ok(Body {
            conversation,
            load,
            call,
            picture,
        })
    }

    fn messages(&self) -> [Message; 3] {
        [
            Message::Hello {
                id: self.conversation.clone(),
                role: Role::Whizbang,
            },
            Message::Request {
                id: answer::LOAD_ID.to_owned(),
                ask: self.load.clone(),
            },
            Message::Request {
                id: answer::CALL_ID.to_owned(),
                ask: self.call.clone(),
            },
        ]
    }
}

/// The texts of a request, each as a piece of its class: a phrase is a prompt, a point
/// or a box is too, written as the numbers it holds; a name, an installation and a
/// setting are the model's, and are words of no user's (stated as prompts, the class a
/// segment's words are approved under).
fn texts_of(ask: &Ask, parts: &mut Vec<Piece>) {
    let words = |text: String| Piece::of(DataClass::Prompts, text.into_bytes());
    match ask {
        Ask::Load {
            model,
            installation,
            settings,
        } => {
            parts.push(words(model.as_str().to_owned()));
            parts.push(words(installation.to_string()));
            for (name, value) in settings {
                parts.push(words(name.clone()));
                if let Scalar::Text(text) = value {
                    parts.push(words(text.clone()));
                }
            }
        }
        Ask::Unload {
            model,
            installation,
        } => {
            parts.push(words(model.as_str().to_owned()));
            parts.push(words(installation.to_string()));
        }
        Ask::Segment {
            model,
            installation,
            queries,
            ..
        } => {
            parts.push(words(model.as_str().to_owned()));
            parts.push(words(installation.to_string()));
            for query in queries {
                parts.push(words(match query.asked() {
                    whizbang_core::capability::Asked::Phrase(phrase) => phrase.to_owned(),
                    other => format!("{other:?}"),
                }));
            }
        }
    }
}

impl FrameBody for Body {
    fn parts(&self) -> Vec<Piece> {
        let mut parts = Vec::new();
        parts.push(Piece::of(
            DataClass::Prompts,
            self.conversation.clone().into_bytes(),
        ));
        texts_of(&self.load, &mut parts);
        texts_of(&self.call, &mut parts);
        if let Some(picture) = &self.picture {
            parts.push(Piece::pixels(DataClass::SourceImages, picture));
        }
        parts
    }

    fn write(&self) -> Result<Vec<u8>, Fault> {
        let mut bytes = Vec::new();
        for message in self.messages() {
            proto::write(&message, &mut bytes).map_err(|_| {
                Fault::new(
                    FaultKind::Rejected,
                    "the request breaks a rule of the protocol and was not sent to the front",
                )
                .not_sent()
            })?;
        }
        Ok(bytes)
    }
}

impl Segmenter for Remote {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    /// One request of the front through the checked sender its bounds hand it: the
    /// answer under the files the catalogue names, on the device the worker named,
    /// never from a cache; the answer's digest and its dispatch id said beside it.
    fn segment(&self, request: &SegmentRequest, bounds: &Bounds) -> Result<SegmentResponse, Fault> {
        let call = Ask::segment(request, self.model.clone(), self.installation);
        match self.call(bounds.sender(), bounds, call) {
            Ok(answered) => response(request, answered),
            Err(failed) => Err(failed.fault),
        }
    }
}

/// The core's response to a request, from what the front answered: as the kind `pack`
/// makes it, and with the dispatch id and the digest said beside it.
fn response(request: &SegmentRequest, answered: Answered) -> Result<SegmentResponse, Fault> {
    let malformed = || {
        let words = "the front answered with an instance the request cannot hold";
        Fault::new(FaultKind::Malformed, words).unresolved()
    };
    let Answered {
        called,
        dispatch,
        digest,
    } = answered;
    let Answer::Segment(found) = called.answer else {
        return Err(malformed());
    };
    let mut answers: Vec<Vec<Instance>> = vec![Vec::new(); request.queries().len()];
    let mut predicted: Vec<Vec<Option<f64>>> = vec![Vec::new(); request.queries().len()];
    for one in found {
        let place = usize::try_from(one.query).map_err(|_| malformed())?;
        let plane = one.plane;
        let instance = Instance::new(
            plane.binding,
            plane.width,
            plane.height,
            plane.samples,
            one.bounds,
            one.score,
        )
        .map_err(|_| malformed())?;
        answers.get_mut(place).ok_or_else(malformed)?.push(instance);
        predicted[place].push(one.iou);
    }
    let device = Device::named(&called.device).map_err(|_| malformed())?;
    let served = Served::unknown().on(device).cached(false);
    let mut said = format!("remote: dispatch {dispatch}, files {digest}\n");
    said.push_str(&called.diagnostics);
    Ok(SegmentResponse::new(answers)
        .served_as(served)
        .said(&said)
        .predicted(predicted))
}
