//! `whizbang backends status [--probe]` (plan 1C, task P7b; `SPEC.md` section 6,
//! "Readiness"; `docs/guides/backends.md`): each backend the config offers, with its
//! kind, its model and whether it is ready.
//!
//! **Without `--probe` it contacts nothing and starts nothing.** The config and its env
//! file are read as a call's source reads them; the kinds of the build make what they
//! make of each table, which sends nothing; and a local backend's model is looked up in
//! what is installed (ADR 0015, "An inventory"). No worker is started and no host is
//! asked.
//!
//! **With `--probe`,** after the list, each backend is probed in the way that fits it:
//!
//! - **a local backend** (the kind `pack`, by its own rule, [`crate::pack::is_local`]):
//!   the one worker of the process starts a worker, hears its hello, and stops it and
//!   waits for it to end ([`crate::pack::OneWorker::hello`]), once for the command
//!   whatever the number of local backends; what is printed is the hello: the
//!   protocol's version, the device and the threads. No model is loaded.
//! - **a remote backend that is ready:** one probe of its address, through
//!   `CheckedProber` (ADR 0019, "A download and a probe") under the policy the config's
//!   approvals make, for the recipient the backend declares. So a recipient the config
//!   does not approve for probing is not probed, and the prober behind is not asked. The
//!   probe carries no credential and no user data, and follows no redirect itself. The
//!   binary hands the network's prober (`whizbang_backends::net::prober`, plan m6, task
//!   R1); a test hands the test kit's, or the network's trusting a root of its own.
//!   **A backend of the kind `remote`** is probed at its front's health document,
//!   `<url>/pack/1/health` (ADR 0046), which needs no token and wakes no card: the
//!   document of `pack-host/1` is read, and the line says whether the host holds the
//!   model the config asks, by the catalogue's hashes: an installation of that model
//!   whose files' digest is the one the backend declares its model under.
//! - **a remote backend that is not ready** is not probed: there is nothing to ask.
//!
//! Every text that came from the config (a backend's name, its kind, its model) is
//! printed escaped. Nothing of a credential is ever printed: a probe holds none, and a
//! reason a backend is not ready names a variable, never its value.

use std::sync::Arc;

use whizbang_backends::pack::Said;
use whizbang_backends::pack::proto::VERSION;
use whizbang_backends::remote;
use whizbang_core::capability::{Fault, FaultKind, Installed};
use whizbang_core::config::Backend;
use whizbang_core::config::NotReady;
use whizbang_core::contract::{Cancellation, Capability, MissingKind};
use whizbang_core::egress::{
    CheckedProber, DispatchPolicy, ProbeRequest, Probed, Prober, Recipient,
};

use crate::pack::is_local;
use crate::wiring::{Configured, Over};

/// The order the capabilities are listed in: the config's.
const CAPABILITIES: [Capability; 3] = [Capability::Fill, Capability::Judge, Capability::Segment];

/// What a probe through [`NoProber`] says.
pub const NO_PROBER: &str = "this prober has no network client, so nothing was sent";

/// A prober with no network client: it contacts nothing, and answers every probe with
/// [`NO_PROBER`], never sent. A test hands it where nothing may be probed; the binary
/// hands the network's. It stands behind the checked prober, so what the config does
/// not approve is refused for that before it is reached.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoProber;

impl Prober for NoProber {
    fn probe(&self, _: &ProbeRequest, _: &Cancellation) -> Result<Probed, Fault> {
        Err(Fault::new(FaultKind::Unavailable, NO_PROBER).not_sent())
    }
}

/// What the command was asked.
#[derive(Debug, Clone, Copy, Default)]
pub struct Asked {
    /// `--probe`: probe each backend after listing it.
    pub probe: bool,
}

/// One backend the config offers, as it is listed.
struct Listed<'a> {
    capability: Capability,
    name: &'a str,
    backend: &'a Backend,
    /// The recipient it declares, when it is ready and sends to one.
    recipient: Option<Recipient>,
    /// The version it declares its model at, when it is ready: for the kind `remote`,
    /// the digest of the files a host is held to.
    declared: Option<String>,
}

/// Runs `whizbang backends status` over what the wiring was given: the list, and with
/// `--probe` what each probe came to, through `prober` for a remote backend. Why not,
/// otherwise: a config that cannot be used, in the readers' own words.
pub fn status(over: &Over, asked: Asked, prober: Arc<dyn Prober>) -> Result<String, String> {
    let Some(configured) = over.configured()? else {
        return Ok(
            "backends: none: no config was named, and the empty config offers none\n".to_owned(),
        );
    };
    let config = configured.loaded.config();
    let mut out = String::new();
    let mut listed = Vec::new();
    for capability in CAPABILITIES {
        for (name, backend) in config.backends(capability) {
            let (state, recipient) = state_of(&configured, capability, name);
            let declared = configured
                .offers
                .roster()
                .resolve(capability, Some(name), &[])
                .ok()
                .map(|resolved| resolved.declaration().model().version().to_owned());
            let default = match config.default_of(capability) == Some(name) {
                true => "\tdefault",
                false => "",
            };
            out.push_str(&format!(
                "{}\t{}\t{}\t{}\t{state}{default}\n",
                capability.name(),
                name.escape_debug(),
                backend.kind().escape_debug(),
                backend.model().map_or_else(
                    || "no model".to_owned(),
                    |model| model.escape_debug().to_string()
                ),
            ));
            listed.push(Listed {
                capability,
                name,
                backend,
                recipient,
                declared,
            });
        }
    }
    let head = match listed.len() {
        0 => "backends: none\n".to_owned(),
        count => format!("backends: {count}\n"),
    };
    out.insert_str(0, &head);
    if asked.probe {
        out.push_str(&probes(&configured, &listed, prober));
    }
    Ok(out)
}

/// Each backend the config offers, as the playground's `/api/hello` says it (plan m5,
/// task H1): its capability, its name, its kind and its model, whether it is the
/// capability's default, and whether it is ready; when it is not, the kind of the reason
/// ([`whizbang_core::contract::MissingKind`]: `no_key` for a credential the env file does
/// not set) and the reason in words. Without a probe: it contacts nothing and starts
/// nothing, as `whizbang backends status` without `--probe`. Every text from the config
/// is escaped; nothing of a credential is said. None, under no config.
pub fn readiness(over: &Over) -> Result<Vec<serde_json::Value>, String> {
    let Some(configured) = over.configured()? else {
        return Ok(Vec::new());
    };
    let config = configured.loaded.config();
    let mut listed = Vec::new();
    for capability in CAPABILITIES {
        for (name, backend) in config.backends(capability) {
            let (state, _) = state_of(&configured, capability, name);
            let kind = kind_of(&configured, capability, name);
            listed.push(serde_json::json!({
                "capability": capability.name(),
                "name": name.escape_debug().to_string(),
                "kind": backend.kind().escape_debug().to_string(),
                "model": backend.model().map(|model| model.escape_debug().to_string()),
                "default": config.default_of(capability) == Some(name),
                "ready": kind.is_none(),
                "missing": kind.map(MissingKind::word),
                "why": state,
            }));
        }
    }
    Ok(listed)
}

/// Whether a variable's name has the shape of one (ADR 0011, amended): at least two words
/// joined by single underscores, each of 1 to 12 upper-case letters or digits, the first
/// word starting with a letter, at most 64 characters in all, as `FAL_KEY` or
/// `GEMINI_API_KEY`. Only such a name is ever said back. A key pasted where the name
/// belongs is very unlikely to have this shape: keys hold lower-case letters or symbols,
/// or are one long run of upper-case letters and digits (an AWS access key id is twenty),
/// which no word of twelve at most can hold.
pub fn variable_shaped(name: &str) -> bool {
    let words: Vec<&str> = name.split('_').collect();
    name.len() <= 64
        && words.len() >= 2
        && words.iter().all(|word| {
            (1..=12).contains(&word.len())
                && word
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        })
        && name.starts_with(|c: char| c.is_ascii_uppercase())
}

/// The variable a backend's table names for the credential the env file does not set,
/// by its name alone and only when it has the shape of one ([`variable_shaped`]): what
/// the playground tells a person to set. The env file's values are not looked at.
pub fn missing_variable(over: &Over, capability: &str, name: &str) -> Option<String> {
    let configured = over.configured().ok()??;
    let capability = CAPABILITIES.into_iter().find(|c| c.name() == capability)?;
    let Some(NotReady::Missing { setting }) = configured.offers.not_ready(capability, name) else {
        return None;
    };
    let config = configured.loaded.config();
    let (_, backend) = config
        .backends(capability)
        .into_iter()
        .find(|(offered, _)| *offered == name)?;
    let (_, variable) = backend.credentials().find(|(key, _)| *key == *setting)?;
    variable_shaped(variable).then(|| variable.to_owned())
}

/// Why one backend is not ready, as a kind; none when it is.
fn kind_of(configured: &Configured, capability: Capability, name: &str) -> Option<MissingKind> {
    let offers = &configured.offers;
    if let Some(why) = offers.not_ready(capability, name) {
        return Some(match why {
            NotReady::Missing { .. } => MissingKind::NoKey,
            _ => MissingKind::Other,
        });
    }
    let Ok(resolved) = offers.roster().resolve(capability, Some(name), &[]) else {
        return Some(MissingKind::Other);
    };
    let declaration = resolved.declaration();
    if declaration.recipient().is_some() {
        return None;
    }
    match configured.inventory.look(declaration.model().model()) {
        Installed::Ready(_) => None,
        Installed::NotReady(why) => Some(why.kind()),
    }
}

/// Whether one backend is ready, in words, and the recipient it declares when it is
/// ready and sends to one. Nothing is started and nothing is sent.
fn state_of(
    configured: &Configured,
    capability: Capability,
    name: &str,
) -> (String, Option<Recipient>) {
    let offers = &configured.offers;
    if let Some(why) = offers.not_ready(capability, name) {
        return (format!("not ready: {why}"), None);
    }
    let Ok(resolved) = offers.roster().resolve(capability, Some(name), &[]) else {
        return ("not ready: it is not offered".to_owned(), None);
    };
    let declaration = resolved.declaration();
    if let Some(recipient) = declaration.recipient() {
        let state = format!("ready: sends to {}", recipient.origin());
        return (state, Some(recipient.clone()));
    }
    // A local backend: ready only while its model is installed, as prepare asks it.
    match configured.inventory.look(declaration.model().model()) {
        Installed::Ready(installation) => (format!("ready: installation {installation}"), None),
        Installed::NotReady(why) => (format!("not ready: {why}"), None),
    }
}

/// The probe of each backend, a line for each, after the list.
fn probes(configured: &Configured, listed: &[Listed<'_>], prober: Arc<dyn Prober>) -> String {
    let config = configured.loaded.config();
    let approvals = config.recipients().map(|(_, approval)| approval);
    // A config that was read gives a policy: it refuses two approvals of one recipient.
    let policy =
        DispatchPolicy::from_approvals(approvals).unwrap_or_else(|_| DispatchPolicy::none());
    // The worker is started once for the command, at the first local backend.
    let mut heard: Option<String> = None;
    let mut out = String::new();
    for one in listed {
        let came_to = if is_local(one.capability, one.backend) {
            heard.get_or_insert_with(|| hello(configured)).clone()
        } else {
            match (&one.recipient, one.backend.address()) {
                (Some(recipient), Some(address)) => {
                    let checked =
                        CheckedProber::new(prober.clone(), policy.clone(), recipient.clone());
                    let remote = one.backend.kind() == remote::KIND;
                    let asked = match remote {
                        true => remote::health_of(address, &configured.secrets),
                        false => Ok(address.clone()),
                    };
                    let Ok(asked) = asked else {
                        out.push_str(&format!(
                            "probe {} {}: not probed: its health's address is not one\n",
                            one.capability.name(),
                            one.name.escape_debug()
                        ));
                        continue;
                    };
                    let request = ProbeRequest::new(asked);
                    match checked.probe(&request, &Cancellation::new()) {
                        Ok(Probed::Reached { status, body }) if remote => {
                            let declared = one.declared.as_deref().unwrap_or_default();
                            let model = one.backend.model().unwrap_or_default();
                            holds(status, &body, model, declared)
                        }
                        Ok(Probed::Reached { status, .. }) => {
                            format!("reached: it answered with status {status}")
                        }
                        Ok(Probed::Moved(_)) => "not reached: it was pointed elsewhere".to_owned(),
                        Err(fault) => format!("not reached: {}", fault.sentence()),
                    }
                }
                _ => "not probed: it is not ready".to_owned(),
            }
        };
        out.push_str(&format!(
            "probe {} {}: {came_to}\n",
            one.capability.name(),
            one.name.escape_debug()
        ));
    }
    out
}

/// What a front's health document says of the model a config asks, by the digest of
/// the files the backend declares its model under (ADR 0046, "The identity"): whether an
/// installation of the model the document lists holds those files, and whether the
/// front has checked it. Nothing of the document is printed but the installation's
/// identity, held to the shape of one.
fn holds(status: u16, body: &[u8], model: &str, declared: &str) -> String {
    if status != 200 {
        return format!("reached: its health answered with status {status}");
    }
    let Ok(document) = serde_json::from_slice::<serde_json::Value>(body) else {
        return "reached: its health is not a document of pack-host/1".to_owned();
    };
    if document.get("format").and_then(|f| f.as_str()) != Some("pack-host/1") {
        return "reached: its health is not a document of pack-host/1".to_owned();
    }
    let listed = document
        .get("models")
        .and_then(|models| models.as_array())
        .cloned()
        .unwrap_or_default();
    for installation in listed {
        if installation.get("model").and_then(|m| m.as_str()) != Some(model) {
            continue;
        }
        let files = installation
            .get("files")
            .and_then(|files| files.as_array())
            .cloned()
            .unwrap_or_default();
        let named: Option<Vec<(String, String)>> = files
            .iter()
            .map(|file| {
                Some((
                    file.get("name")?.as_str()?.to_owned(),
                    file.get("sha256")?.as_str()?.to_owned(),
                ))
            })
            .collect();
        let Some(named) = named else {
            continue;
        };
        let digest =
            remote::files_digest(named.iter().map(|(name, sha)| (name.as_str(), sha.clone())));
        if remote::version_of(&digest) != declared {
            continue;
        }
        let identity = installation
            .get("installation")
            .and_then(|i| i.as_str())
            .filter(|identity| {
                identity.strip_prefix("sha256:").is_some_and(|hex| {
                    hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())
                })
            })
            .unwrap_or("not named");
        let checked = match installation.get("checked").and_then(|c| c.as_bool()) {
            Some(true) => "checked by the front",
            _ => "not yet checked by the front",
        };
        return format!(
            "reached: the host holds {} by the catalogue's hashes, installation {identity}, {checked}",
            model.escape_debug()
        );
    }
    format!(
        "reached: the host does not hold {} by the catalogue's hashes",
        model.escape_debug()
    )
}

/// What the worker said in its hello, in words, or why it said none.
fn hello(configured: &Configured) -> String {
    let Some(worker) = &configured.worker else {
        return "not probed: this command has no worker of the pack".to_owned();
    };
    match worker.hello(&configured.secrets) {
        Ok(Said { device, threads }) => format!(
            "the pack's worker said hello: {VERSION}, device {}, {threads} threads; it was stopped",
            device.as_deref().unwrap_or("not known yet"),
        ),
        Err(fault) => format!("the pack's worker said no hello: {}", fault.sentence()),
    }
}
