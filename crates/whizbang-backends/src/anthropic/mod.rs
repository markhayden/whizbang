//! The judge of a service (plan 2A, task S1; `SPEC.md` section 4, "Judge", and section
//! 10, "Egress"): the kind `service`, whose one provider is `anthropic`, a backend that
//! sends each request of a judge to the Messages interface at
//! `https://api.anthropic.com/v1/messages`, through the sender its run hands it.
//!
//! **A table of the kind** is `kind = "service"`, `provider = "anthropic"`, a `model`
//! and a `key_env`, under `[backends.judge]`; its settings are `effort` (a text) and the
//! three ceilings of a request, `max_input_tokens`, `max_output_tokens` and
//! `max_pictures`, which fix what one request may cost ([`spend`]). It names no
//! `url`: the address is the kind's own, and a table that names one is refused. A
//! test's build makes the kind at another address ([`ServiceKind::at`], under the
//! feature `fakes`).
//!
//! **The key** is made a [`Credential`] bound to that address's origin, under the
//! header `x-api-key` ([`Kind::credential`]): the backend is never handed it, and the
//! executor attaches it to a request to that origin and to no other (ADR 0012).
//!
//! **A fault says nothing of what came:** every sentence is fixed words and typed
//! fields, never the provider's body, a header or an address; and it is `not_sent` only
//! when no byte left the process (the sender says so, or the request was refused before
//! it was handed over).

pub mod spend;
mod wire;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use whizbang_core::capability::{
    Bounds, Charge, Declaration, Fault, FaultKind, Judge, JudgeRequest, Price, Turn,
};
use whizbang_core::config::{Kind, KindRefused, Setting, Table};
use whizbang_core::contract::{Capability, Made, ModelIdentity, Secrets};
use whizbang_core::egress::{
    Address, Credential, MOVES_LIMIT, NotAllowed, Outgoing, Recipient, Sender,
};

pub use wire::ADAPTER;

/// The name of the kind.
pub const KIND: &str = "service";

/// The one provider of the kind.
pub const PROVIDER: &str = "anthropic";

/// The address the kind sends to.
pub const ADDRESS: &str = "https://api.anthropic.com/v1/messages";

/// The header the key is carried in.
pub const KEY_HEADER: &str = "x-api-key";

/// The version of the adapter: part of the model identity a backend declares.
pub const VERSION: &str = "1";

/// The long edge a picture is fitted to: the native long edge squint's catalog states
/// for the model of the study, `claude-haiku-5-5` (2,576 pixels).
pub const LONG_EDGE: u32 = 2576;

/// The fields of a request, each a constant: the type of its body, the version of the
/// interface, and the one beta of the thinking it asks for (squint's recorded requests
/// of the model, `fixtures/adapters/anthropic-messages/first-look-haiku`).
pub const FIELDS: &[(&str, &str)] = &[
    ("content-type", "application/json"),
    ("anthropic-version", "2023-06-01"),
    ("anthropic-beta", "thinking-binding-controls-2026-08-01"),
];

/// The efforts a table may set.
const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

const NOT_JUDGE: &str = "a service backend is a judge, and is offered under backends.judge";
const NOT_ANTHROPIC: &str = "a service backend's provider is anthropic, the one this build has";
const A_URL: &str = "a service backend names no url: its provider's address is the kind's own";
const NO_MODEL: &str = "a service backend names its model";
const BAD_MODEL: &str = "a service backend's model is 1 to 64 letters, digits, dots and hyphens";
const NO_KEY: &str = "a service backend names its key with key_env, and no token_env";
const BAD_SETTING: &str = "a service backend takes the settings effort, one of low, medium, \
                           high, xhigh and max, and max_input_tokens (1 to 100000), \
                           max_output_tokens (1 to 128000) and max_pictures (1 to 64), each a \
                           whole number";
const NOT_PRICED: &str = "a service backend's model is one this build has a row of prices for: \
                          claude-haiku-5-5";
const NOT_DECLARED: &str = "a service backend cannot declare that model";

/// What a fault says of a request counted over the input ceiling.
pub const OVER_INPUT: &str = "the request is counted over the most input tokens its backend sends";
/// What it says of a request of more pictures than the ceiling.
pub const OVER_PICTURES: &str = "the request holds more pictures than its backend sends";
/// What a fault says of a request whose pieces are more than a request may hold.
pub const TOO_MANY_PIECES: &str = "the request holds more pictures than one request may carry";
/// What it says of an answer of 400, 404, 413 or 422.
pub const REFUSED: &str = "the provider refused the request as it was written: 400, 404, 413 or \
                           422";
/// What it says of an answer of 401 or 403.
pub const CREDENTIAL: &str = "the provider refused the key: 401 or 403";
/// What it says of an answer of 429.
pub const RATE_LIMITED: &str = "the provider is limiting the rate of requests: 429";
/// What it says of an answer of 500, 502, 503, 504 or 529.
pub const BUSY: &str = "the provider could not answer now: 500, 502, 503, 504 or 529";
/// What it says of any other status.
pub const STATUS: &str = "the provider answered with a status the adapter does not act on";
/// What it says of a request pointed elsewhere more often than the gate follows.
pub const MOVED_TOO_OFTEN: &str = "the request was pointed elsewhere more often than is followed";

/// The kind `service`. What `{:?}` prints is its name.
#[derive(Debug, Clone)]
pub struct ServiceKind {
    address: String,
}

impl Default for ServiceKind {
    fn default() -> ServiceKind {
        ServiceKind::new()
    }
}

impl ServiceKind {
    /// The kind, sending to [`ADDRESS`].
    pub fn new() -> ServiceKind {
        ServiceKind {
            address: ADDRESS.to_owned(),
        }
    }

    /// The kind, sending to this address in place of [`ADDRESS`]: for a test, whose
    /// server is on loopback. It is compiled only under the feature `fakes`, which the
    /// binary that ships is built without.
    #[cfg(any(test, feature = "fakes"))]
    pub fn at(address: &str) -> ServiceKind {
        ServiceKind {
            address: address.to_owned(),
        }
    }

    fn address(&self, secrets: &Secrets) -> Result<Address, KindRefused> {
        Address::of(&self.address, secrets).map_err(KindRefused::Credential)
    }
}

/// What a table of the kind says, read.
struct Read {
    model: String,
    effort: Option<String>,
    ceilings: spend::Ceilings,
    prices: spend::Prices,
}

fn read(table: &Table<'_>) -> Result<Read, KindRefused> {
    if table.capability() != Capability::Judge {
        return Err(KindRefused::Table(NOT_JUDGE));
    }
    let backend = table.backend();
    if backend.provider() != Some(PROVIDER) {
        return Err(KindRefused::Table(NOT_ANTHROPIC));
    }
    if backend.address().is_some() {
        return Err(KindRefused::Table(A_URL));
    }
    if table.key().is_none() || table.token().is_some() {
        return Err(KindRefused::Table(NO_KEY));
    }
    let model = backend.model().ok_or(KindRefused::Table(NO_MODEL))?;
    let plain = (1..=64).contains(&model.len())
        && model
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.');
    if !plain || table.secrets().redact(model) != model {
        return Err(KindRefused::Table(BAD_MODEL));
    }
    let prices = spend::prices_of(model).ok_or(KindRefused::Table(NOT_PRICED))?;
    let mut read = Read {
        model: model.to_owned(),
        effort: None,
        ceilings: spend::DEFAULT_CEILINGS,
        prices,
    };
    let within = |value: i64, most: i64| (1..=most).contains(&value);
    for (key, setting) in backend.settings() {
        match (key, setting) {
            ("effort", Setting::Text(effort)) if EFFORTS.contains(&effort.as_str()) => {
                read.effort = Some(effort.clone());
            }
            ("max_input_tokens", Setting::Integer(tokens))
                if within(*tokens, spend::LONG_PROMPT_TOKENS as i64) =>
            {
                read.ceilings.input_tokens = tokens.unsigned_abs();
            }
            ("max_output_tokens", Setting::Integer(tokens)) if within(*tokens, 128_000) => {
                read.ceilings.output_tokens = u32::try_from(*tokens).unwrap_or(1);
            }
            ("max_pictures", Setting::Integer(pictures)) if within(*pictures, 64) => {
                read.ceilings.pictures = usize::try_from(*pictures).unwrap_or(1);
            }
            _ => return Err(KindRefused::Table(BAD_SETTING)),
        }
    }
    Ok(read)
}

impl Kind for ServiceKind {
    fn name(&self) -> &str {
        KIND
    }

    fn make(&self, table: &Table<'_>) -> Result<Made, KindRefused> {
        let read = read(table)?;
        let address = self.address(table.secrets())?;
        let identity = ModelIdentity::new(ADAPTER, &read.model, VERSION);
        let bound = spend::bound_micros(read.ceilings, read.prices);
        let price = Price::at_most(bound).map_err(|_| KindRefused::Table(BAD_SETTING))?;
        let declaration = Declaration::new(identity, &["image", "tool"])
            .map_err(|_| KindRefused::Table(NOT_DECLARED))?
            .sending_to(Recipient::new(address.origin().clone()))
            .priced(price);
        Ok(Made::Judge(Arc::new(Messages {
            declaration,
            address,
            settings: wire::Settings {
                model: read.model,
                effort: read.effort,
                long_edge: LONG_EDGE,
                max_output_tokens: read.ceilings.output_tokens,
            },
            ceilings: read.ceilings,
            prices: read.prices,
        })))
    }

    fn credential(&self, table: &Table<'_>) -> Result<Option<Credential>, KindRefused> {
        read(table)?;
        let key = table.key().ok_or(KindRefused::Table(NO_KEY))?;
        let address = self.address(table.secrets())?;
        key.carried_to(&address, KEY_HEADER, table.secrets())
            .map(Some)
            .map_err(KindRefused::Credential)
    }
}

/// A backend of the kind: one model at one address.
struct Messages {
    declaration: Declaration,
    address: Address,
    settings: wire::Settings,
    ceilings: spend::Ceilings,
    prices: spend::Prices,
}

impl Messages {
    /// One request, sent through `sender`: written, sent, followed where the answer
    /// points (each address asked of the gate again, [`MOVES_LIMIT`] times at most),
    /// and its answer read.
    fn turn_through(
        &self,
        request: &JudgeRequest,
        sender: &dyn Sender,
        bounds: &Bounds,
    ) -> Result<Turn, Fault> {
        // The ceilings, before anything is handed over: the pictures, then the input
        // as counted. The output is held by the writer, which asks for no more.
        let over = |why| Err(Fault::new(FaultKind::TooLarge, why).not_sent());
        if wire::pictures_in(request) > self.ceilings.pictures {
            return over(OVER_PICTURES);
        }
        let written = wire::request(request, &self.settings)?;
        let counted = spend::input_count(written.word_bytes(), written.messages, &written.pictures);
        if counted > self.ceilings.input_tokens {
            return over(OVER_INPUT);
        }
        let pieces = written.pieces;
        let mut to = self.address.clone();
        for _ in 0..=MOVES_LIMIT {
            let outgoing = Outgoing::new(to.clone(), pieces.clone())
                .and_then(|outgoing| outgoing.with_fields(FIELDS))
                .map_err(|why| match why {
                    NotAllowed::TooMany => {
                        Fault::new(FaultKind::TooLarge, TOO_MANY_PIECES).not_sent()
                    }
                    _ => Fault::new(FaultKind::Rejected, wire::NOT_SAID).not_sent(),
                })?;
            let reply = sender.send(&outgoing, bounds)?;
            if let Some(next) = reply.moved_to() {
                to = next.clone();
                continue;
            }
            // An error status is charged nothing: the provider charges no request it
            // refused or failed.
            let nothing = Charge::whole(0).ok();
            let answered = |kind, why| Err(Fault::new(kind, why).answered(nothing));
            return match reply.status() {
                // What a turn cost is its usage at the row; one that reports none
                // cost its bound, which the meter charges when no charge is said.
                Some(200) => wire::turn(reply.body()).map(|turn| {
                    match turn
                        .usage()
                        .and_then(|usage| spend::charge_of(usage, self.prices))
                    {
                        Some(charge) => turn.charged(charge),
                        None => turn,
                    }
                }),
                Some(400 | 404 | 413 | 422) => answered(FaultKind::Rejected, REFUSED),
                Some(401 | 403) => answered(FaultKind::Credential, CREDENTIAL),
                Some(429) => answered(FaultKind::Unavailable, RATE_LIMITED),
                Some(500 | 502 | 503 | 504 | 529) => answered(FaultKind::Unavailable, BUSY),
                _ => answered(FaultKind::Malformed, STATUS),
            };
        }
        Err(Fault::new(FaultKind::Rejected, MOVED_TOO_OFTEN).unresolved())
    }
}

impl Judge for Messages {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn send(&self, request: &JudgeRequest, bounds: &Bounds) -> Result<Turn, Fault> {
        self.turn_through(request, bounds.sender(), bounds)
    }
}
