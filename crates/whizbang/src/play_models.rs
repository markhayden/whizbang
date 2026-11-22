//! What the playground's Models screen is given (plan m5, task M1;
//! `whizbang_serve::play::models`): the catalogue's models as `whizbang models list` sees
//! them, an install and a removal as `models get` and `models remove` make them, each
//! backend the config offers as `whizbang backends status` sees it without a probe, and the
//! allowance as `whizbang spend status` reads it, with the owner's `spend reset`.
//!
//! **No key leaves here.** A backend is said to have its key set or not, from whether the
//! config's variable is set in the env file; neither the key nor the variable's name is
//! answered (a key pasted where a name goes cannot be told from a name). A model's
//! download that needs a sign-in is said to have one named or not; the sign-in itself is
//! read only by an install, as `models get` reads it, and never answered. Every text has
//! the config's secrets taken out by the commands it comes from, and is escaped.
//!
//! **The page writes no config and no env file.** The only thing it changes beside the
//! models directory is the store's allowance, by a reset, which is `spend reset`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value as Json, json};
use whizbang_core::capability::QuoteKind;
use whizbang_core::config::NotReady;
use whizbang_core::contract::{Cancellation, Capability, Clock, SystemClock};
use whizbang_core::spend::fold::Folded;
use whizbang_core::spend::{Amount, Counted, Name, Reading, Record, Sum};
use whizbang_core::store::Store;
use whizbang_serve::escaped;
use whizbang_serve::play::Refused;
use whizbang_serve::play::models::{Models, NotInstalled};

use crate::model_commands::{self, FailedKind, Tools, Under};
use crate::wiring::Over;

/// How long a removal from the page waits for the runs that hold the model before it
/// answers busy: short, since it holds a connection of the host's while it waits.
pub const REMOVE_WAIT: Duration = Duration::from_secs(5);

/// The order the capabilities are listed in.
const CAPABILITIES: [Capability; 3] = [Capability::Segment, Capability::Fill, Capability::Judge];

/// The Models screen's door: the commands' tools, the config and env file the host runs
/// under, and, once the playground holds them, its wiring and its store.
pub struct PlayModels {
    tools: Tools,
    config: Option<PathBuf>,
    env_file: Option<PathBuf>,
    wait: Duration,
    over: Option<Arc<Over>>,
    store: Mutex<Option<Arc<Store>>>,
}

impl PlayModels {
    /// The models these tools reach, under this config and env file.
    pub fn new(tools: Tools, config: Option<&Path>, env_file: Option<&Path>) -> PlayModels {
        PlayModels {
            tools,
            config: config.map(Path::to_path_buf),
            env_file: env_file.map(Path::to_path_buf),
            wait: REMOVE_WAIT,
            over: None,
            store: Mutex::new(None),
        }
    }

    /// The same, with a removal waiting this long for the runs that hold a model.
    pub fn waiting(mut self, wait: Duration) -> PlayModels {
        self.wait = wait;
        self
    }

    /// The same, over the playground's wiring and its store's writer.
    pub(crate) fn over(mut self, over: Arc<Over>, store: Option<Arc<Store>>) -> PlayModels {
        self.over = Some(over);
        self.store = Mutex::new(store);
        self
    }

    /// The host has stopped: the store's writer is let go.
    pub(crate) fn closing(&self) {
        self.store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }

    fn under(&self) -> Under<'_> {
        Under {
            config: self.config.as_deref(),
            env_file: self.env_file.as_deref(),
        }
    }

    fn store(&self) -> Result<Arc<Store>, Refused> {
        self.store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .ok_or_else(|| Refused::new(503, "the playground is stopping"))
    }
}

/// Millionths of a US dollar as dollars, for the page.
#[allow(clippy::cast_precision_loss)]
fn dollars(micros: u128) -> f64 {
    micros as f64 / 1_000_000.0
}

fn sum(sum: Sum) -> Json {
    json!({"usd": dollars(sum.micros()), "stopped": sum.stopped()})
}

/// The allowance a person typed, in US dollars: digits, and at most six more after a
/// point, as `whizbang spend reset --allowance` takes it. Anything else is refused, and so
/// is the largest number there is, which is no amount.
pub fn allowance(text: &str) -> Result<Amount, String> {
    let cut: String = text.chars().take(64).collect();
    let more = if text.chars().count() > 64 { "..." } else { "" };
    let refuse = |why: &str| {
        Err(format!(
            "{}{more} is no allowance: {why}. Give US dollars as digits, with at most six \
             after a point, as in 20 or 12.50",
            cut.escape_debug()
        ))
    };
    let (whole, part) = match text.split_once('.') {
        Some((whole, part)) => (whole, Some(part)),
        None => (text, None),
    };
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    if !digits(whole) || part.is_some_and(|part| !digits(part)) {
        return refuse("it is not a number of dollars");
    }
    let part = part.unwrap_or("0");
    if part.len() > 6 {
        return refuse("a millionth of a dollar is the least that is counted");
    }
    let micros = whole.parse::<u64>().ok().and_then(|whole| {
        let part = format!("{part:0<6}").parse::<u64>().ok()?;
        whole.checked_mul(1_000_000)?.checked_add(part)
    });
    let Some(micros) = micros else {
        return refuse("it is more than can be counted");
    };
    match Amount::new(micros) {
        Some(amount) => Ok(amount),
        None => refuse("it is the largest number there is, which is no amount"),
    }
}

/// When the newest epoch's reset was made, where its record can be read.
fn reset_at(store: &Store, reading: &Reading, newest: u64) -> Option<u64> {
    let generation = store.snapshot().ok()?;
    reading
        .epochs
        .iter()
        .find_map(|(name, damaged)| match name {
            Name::Reset { epoch, .. } if epoch.get() == newest && !damaged => {
                let entry = generation.get(&name.to_string())?;
                let bytes = store.read(entry).ok()?;
                match Record::read(name, &bytes).ok()? {
                    Record::Reset { at_ms, .. } => Some(at_ms),
                    _ => None,
                }
            }
            _ => None,
        })
}

/// The status a refusal of the models commands is answered with, by its kind.
fn status_of(kind: FailedKind) -> u16 {
    match kind {
        FailedKind::NotInCatalogue => 404,
        FailedKind::NotInstallable | FailedKind::Busy | FailedKind::NotInstalled => 409,
        FailedKind::Left | FailedKind::Other => 500,
    }
}

impl Models for PlayModels {
    fn models(&self) -> Result<Json, Refused> {
        let rows = model_commands::rows(self.under(), &self.tools)
            .map_err(|why| Refused::new(500, &escaped(&why)))?;
        let rows: Vec<Json> = rows
            .into_iter()
            .map(|row| {
                json!({
                    "name": escaped(&row.name),
                    "title": escaped(&row.title),
                    "licence": escaped(&row.licence),
                    "bytes": row.bytes,
                    "files": row.files,
                    "state": row.state,
                    "why": escaped(&row.why),
                    "files_done": row.files_done,
                    "sign_in": row.sign_in,
                })
            })
            .collect();
        Ok(json!({ "models": rows }))
    }

    fn backends(&self) -> Result<Json, Refused> {
        let Some(over) = &self.over else {
            return Ok(json!({"backends": []}));
        };
        let configured = over
            .configured()
            .map_err(|why| Refused::new(500, &escaped(&why)))?;
        let Some(configured) = configured else {
            return Ok(json!({"backends": []}));
        };
        let config = configured.loaded.config();
        let offers = &configured.offers;
        let mut listed = Vec::new();
        for capability in CAPABILITIES {
            for (name, backend) in config.backends(capability) {
                let resolved = offers.roster().resolve(capability, Some(name), &[]);
                let declaration = resolved.as_ref().ok().map(|r| r.declaration());
                let local = declaration.is_some_and(|d| d.recipient().is_none());
                let names_a_key = backend.credentials().next().is_some();
                let access = match offers.not_ready(capability, name) {
                    Some(NotReady::Missing { .. }) => "no_key",
                    Some(_) => "not_set_up",
                    None if resolved.is_err() => "not_set_up",
                    None if local => "local",
                    None if names_a_key => "key_set",
                    None => "no_key_needed",
                };
                let price = declaration.and_then(|d| d.price()).map(|price| {
                    let quote = price.quote();
                    let (kind, micros) = match quote.kind() {
                        QuoteKind::Free => ("free", Some(0)),
                        QuoteKind::Bound => ("at_most", quote.bound_micros()),
                        QuoteKind::EstimateOnly => ("about", quote.estimated_micros()),
                    };
                    json!({"kind": kind, "usd": micros.map(|m| dollars(u128::from(m)))})
                });
                listed.push(json!({
                    "capability": capability.name(),
                    "name": escaped(name),
                    "kind": escaped(backend.kind()),
                    "model": backend.model().map(escaped),
                    "default": config.default_of(capability) == Some(name),
                    "local": local,
                    "access": access,
                    "price": price,
                }));
            }
        }
        Ok(json!({ "backends": listed }))
    }

    fn spend(&self) -> Result<Json, Refused> {
        let store = self.store()?;
        let generation = store
            .snapshot()
            .map_err(|_| Refused::new(500, "the store could not answer"))?;
        let reading = Reading::of(&generation, |entry| store.read(entry));
        let balance = Folded::of(reading.clone()).balance();
        let counted = match balance.counted {
            Counted::Exactly(exactly) => json!({"exactly": true, "sum": sum(exactly)}),
            Counted::AtLeast(least) => json!({"exactly": false, "sum": sum(least)}),
        };
        let cap = self
            .over
            .as_ref()
            .and_then(|over| over.configured().ok().flatten())
            .and_then(|configured| configured.loaded.config().call_cap_micros());
        let config = self
            .config
            .as_deref()
            .and_then(Path::file_name)
            .map(|name| escaped(&name.to_string_lossy()));
        let newest = balance.epoch;
        Ok(json!({
            "epoch": newest,
            "allowance": (newest > 0).then(|| dollars(u128::from(balance.allowed.micros()))),
            "since_ms": (newest > 0).then(|| reset_at(&store, &reading, newest)).flatten(),
            "counted": counted,
            "charged": sum(balance.charged),
            "held": sum(balance.held),
            "unresolved": sum(balance.unresolved),
            "left": dollars(u128::from(balance.left.micros())),
            "needs_owner": balance.needs_owner,
            "call_cap": cap.map(|micros| dollars(u128::from(micros))),
            "config": config,
        }))
    }

    fn reset(&self, typed: &str) -> Result<Json, Refused> {
        let amount = allowance(typed).map_err(|why| Refused::new(400, &why))?;
        let store = self.store()?;
        let opened = whizbang_core::spend::reset(&store, amount, SystemClock.now_ms())
            .map_err(|why| Refused::new(409, &escaped(&why.to_string())))?;
        Ok(json!({
            "allowance": dollars(u128::from(opened.allowance.micros())),
            "epoch": opened.epoch,
        }))
    }

    fn installable(&self, model: &str) -> Result<(), Refused> {
        model_commands::installable(self.under(), model, &self.tools)
            .map_err(|failed| Refused::new(status_of(failed.kind), &escaped(&failed.why)))
    }

    fn install(&self, model: &str, cancellation: &Cancellation) -> Result<String, NotInstalled> {
        model_commands::get_until(self.under(), model, &self.tools, cancellation, &mut |_| {})
            .map_err(|failed| NotInstalled {
                left: failed.kind == FailedKind::Left,
                why: failed.why,
            })
    }

    fn remove(&self, model: &str) -> Result<Json, Refused> {
        match model_commands::remove_typed(self.under(), model, self.wait, &self.tools) {
            Ok(said) => Ok(json!({"model": escaped(model), "removed": true,
                                  "said": escaped(said.trim_end())})),
            Err(failed) => Err(Refused::new(status_of(failed.kind), &escaped(&failed.why))),
        }
    }
}
