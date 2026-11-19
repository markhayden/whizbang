//! The judge of a service (plan 2A, task S1; plan m2c, task P1; `SPEC.md` section 4,
//! "Judge", and section 10, "Egress"): the kind `service`, whose providers are
//! `anthropic` (the Messages interface, [`crate::anthropic`]), `google` (Gemini's
//! `generateContent`, [`crate::google`]) and `openai` (the Responses interface,
//! [`crate::openai`]). A backend of the kind sends each request of a judge to its
//! provider's one address, through the sender its run hands it. A table under
//! `[backends.fill]` that names the provider `fal` is fal's filler, read and made by
//! [`crate::fal`] (plan m3, task F1).
//!
//! **A table of the kind** is `kind = "service"`, a `provider`, a `model` and a
//! `key_env`, under `[backends.judge]`; its settings are `effort` (a text, of the values
//! the provider takes) and the three ceilings of a request, `max_input_tokens`,
//! `max_output_tokens` and `max_pictures`, which fix what one request may cost
//! ([`spend`]). It names no `url`: the address is the provider's own, and a table that
//! names one is refused. A test's build makes the kind at another address
//! ([`ServiceKind::at`], under the feature `fakes`).
//!
//! **The key** is made a [`Credential`] bound to that address's origin
//! ([`Kind::credential`]), under the provider's own header: `x-api-key` for Anthropic,
//! `x-goog-api-key` for Google, and `Authorization`, as `Bearer` and the key, for
//! OpenAI. The backend is never handed it, and the executor attaches it to a request to
//! that origin and to no other (ADR 0012). It is never written into an address.
//!
//! **A fault says nothing of what came:** every sentence is fixed words and typed
//! fields, never the provider's body, a header or an address; and it is `not_sent` only
//! when no byte left the process (the sender says so, or the request was refused before
//! it was handed over).

pub(crate) mod body;
pub mod spend;
#[cfg(test)]
pub(crate) mod testing;

use std::sync::Arc;

use whizbang_core::capability::{
    Bounds, Charge, Declaration, Fault, FaultKind, Judge, JudgeRequest, Price, StopReason, Turn,
};
use whizbang_core::config::{Kind, KindRefused, Setting, Table};
use whizbang_core::contract::{Capability, Made, ModelIdentity, Secrets};
use whizbang_core::egress::{
    Address, Credential, MOVES_LIMIT, NotAllowed, Outgoing, Recipient, Sender,
};

use crate::{anthropic, google, openai};
use body::Written;

/// The name of the kind.
pub const KIND: &str = "service";

/// The providers of the kind, by the name a table gives.
pub const PROVIDERS: [&str; 3] = [anthropic::PROVIDER, google::PROVIDER, openai::PROVIDER];

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
/// What it says of a status of success other than 200.
pub const OTHER_SUCCESS: &str =
    "the provider answered with a status of success the adapter does not read: 2xx other than 200";
/// What it says of a request pointed elsewhere more often than the gate follows.
pub const MOVED_TOO_OFTEN: &str = "the request was pointed elsewhere more often than is followed";

const NOT_JUDGE: &str = "a service backend is a judge, and is offered under backends.judge, \
                         or fal's filler, under backends.fill";
const NO_PROVIDER: &str =
    "a service backend's provider is one this build has: anthropic, google, openai or fal";
const A_URL: &str = "a service backend names no url: its provider's address is the kind's own";
const NO_MODEL: &str = "a service backend names its model";
const BAD_MODEL: &str = "a service backend's model is 1 to 64 letters, digits, dots and hyphens";
const NO_KEY: &str = "a service backend names its key with key_env, and no token_env";
const NOT_DECLARED: &str = "a service backend cannot declare that model";

/// One provider of the kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Anthropic,
    Google,
    OpenAi,
}

/// What a provider's module states of itself, for the kind to read a table by.
pub(crate) struct Stated {
    /// The name a table gives.
    pub(crate) name: &'static str,
    /// The adapter's name: the backend a model identity names, and the mark of a turn's
    /// opaque parts.
    pub(crate) adapter: &'static str,
    /// The version of the adapter.
    pub(crate) version: &'static str,
    /// The long edge a picture is fitted to.
    pub(crate) long_edge: u32,
    /// The efforts a table may set.
    pub(crate) efforts: &'static [&'static str],
    /// The effort asked for when a table sets none: none, for the provider's own.
    pub(crate) default_effort: Option<&'static str>,
    /// The most output tokens a request of the model may ask for.
    pub(crate) most_output: u32,
    /// The header the key is carried in.
    pub(crate) key_header: &'static str,
    /// What a table that sets a setting the provider does not take is told.
    pub(crate) bad_setting: &'static str,
    /// What a table that names a model with no row of prices is told.
    pub(crate) not_priced: &'static str,
    /// The row of a model's prices, when the build has one.
    pub(crate) prices_of: fn(&str) -> Option<spend::Prices>,
}

impl Provider {
    fn of(name: &str) -> Option<Provider> {
        [Provider::Anthropic, Provider::Google, Provider::OpenAi]
            .into_iter()
            .find(|provider| provider.stated().name == name)
    }

    pub(crate) fn stated(self) -> &'static Stated {
        match self {
            Provider::Anthropic => &anthropic::STATED,
            Provider::Google => &google::STATED,
            Provider::OpenAi => &openai::STATED,
        }
    }

    /// The provider's own address for a model.
    fn address(self, model: &str) -> String {
        match self {
            Provider::Anthropic => anthropic::ADDRESS.to_owned(),
            Provider::Google => google::address(model),
            Provider::OpenAi => openai::ADDRESS.to_owned(),
        }
    }

    /// The version a backend's model identity names: the adapter's, and the
    /// preparation of the pictures it sends (`Piece::prepared`: fitted to the long edge,
    /// a JPEG at quality 90 or the PNG of one with clear pixels), so that a run's record
    /// names the preparation its pictures had (carry-over row on what an adapter said of
    /// itself).
    pub fn identity_version(self) -> String {
        let stated = self.stated();
        format!("{}+{}-{}", stated.version, PREPARATION, stated.long_edge)
    }
}

/// The name of the preparation every provider's pictures have, and its version: the
/// fit to a long edge, then a JPEG at quality 90, or the PNG of a picture with clear
/// pixels (`whizbang_core::egress::Piece::prepared`).
pub const PREPARATION: &str = "fit1-jpeg90";

/// The kind `service`. What `{:?}` prints is its name.
#[derive(Debug, Clone, Default)]
pub struct ServiceKind {
    /// The address every provider sends to in place of its own: a test's.
    address: Option<String>,
}

impl ServiceKind {
    /// The kind, each provider sending to its own address.
    pub fn new() -> ServiceKind {
        ServiceKind { address: None }
    }

    /// The kind, sending to this address in place of each provider's own: for a test,
    /// whose server is on loopback. It is compiled only under the feature `fakes`,
    /// which the binary that ships is built without.
    #[cfg(any(test, feature = "fakes"))]
    pub fn at(address: &str) -> ServiceKind {
        ServiceKind {
            address: Some(address.to_owned()),
        }
    }

    fn address(
        &self,
        provider: Provider,
        model: &str,
        secrets: &Secrets,
    ) -> Result<Address, KindRefused> {
        let address = self
            .address
            .clone()
            .unwrap_or_else(|| provider.address(model));
        Address::of(&address, secrets).map_err(KindRefused::Credential)
    }
}

/// What a table of the kind says, read.
struct Read {
    provider: Provider,
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
    let provider = backend
        .provider()
        .and_then(Provider::of)
        .ok_or(KindRefused::Table(NO_PROVIDER))?;
    let stated = provider.stated();
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
    let prices = (stated.prices_of)(model).ok_or(KindRefused::Table(stated.not_priced))?;
    let mut read = Read {
        provider,
        model: model.to_owned(),
        effort: stated.default_effort.map(str::to_owned),
        ceilings: spend::DEFAULT_CEILINGS,
        prices,
    };
    let within = |value: i64, most: i64| (1..=most).contains(&value);
    for (key, setting) in backend.settings() {
        match (key, setting) {
            ("effort", Setting::Text(effort)) if stated.efforts.contains(&effort.as_str()) => {
                read.effort = Some(effort.clone());
            }
            ("max_input_tokens", Setting::Integer(tokens))
                if within(*tokens, spend::LONG_PROMPT_TOKENS as i64) =>
            {
                read.ceilings.input_tokens = tokens.unsigned_abs();
            }
            ("max_output_tokens", Setting::Integer(tokens))
                if within(*tokens, i64::from(stated.most_output)) =>
            {
                read.ceilings.output_tokens = u32::try_from(*tokens).unwrap_or(1);
            }
            ("max_pictures", Setting::Integer(pictures)) if within(*pictures, 64) => {
                read.ceilings.pictures = usize::try_from(*pictures).unwrap_or(1);
            }
            _ => return Err(KindRefused::Table(stated.bad_setting)),
        }
    }
    Ok(read)
}

/// Whether a table names fal, whose one backend is a filler ([`crate::fal`]).
fn names_fal(table: &Table<'_>) -> bool {
    table.backend().provider() == Some(crate::fal::PROVIDER)
}

impl Kind for ServiceKind {
    fn name(&self) -> &str {
        KIND
    }

    fn make(&self, table: &Table<'_>) -> Result<Made, KindRefused> {
        if names_fal(table) {
            return crate::fal::make(table, self.address.as_deref());
        }
        let read = read(table)?;
        let stated = read.provider.stated();
        let address = self.address(read.provider, &read.model, table.secrets())?;
        let identity = ModelIdentity::new(
            stated.adapter,
            &read.model,
            &read.provider.identity_version(),
        );
        let bound = spend::bound_micros(read.ceilings, read.prices);
        let price = Price::at_most(bound).map_err(|_| KindRefused::Table(stated.bad_setting))?;
        let declaration = Declaration::new(identity, &["image", "tool"])
            .map_err(|_| KindRefused::Table(NOT_DECLARED))?
            .sending_to(Recipient::new(address.origin().clone()))
            .priced(price);
        let wire = Wire::of(read.provider, read.model, read.effort, read.ceilings);
        Ok(Made::Judge(Arc::new(Service {
            declaration,
            address,
            wire,
            ceilings: read.ceilings,
            prices: read.prices,
        })))
    }

    /// The key, bound to the origin of the provider's address, under the provider's own
    /// header: as it is for Anthropic and Google, and as `Bearer` and the key for
    /// OpenAI. The address is the one [`ServiceKind::make`] sends to; the key is in no
    /// address.
    fn credential(&self, table: &Table<'_>) -> Result<Option<Credential>, KindRefused> {
        if names_fal(table) {
            return crate::fal::credential(table, self.address.as_deref());
        }
        let read = read(table)?;
        let key = table.key().ok_or(KindRefused::Table(NO_KEY))?;
        let address = self.address(read.provider, &read.model, table.secrets())?;
        let header = read.provider.stated().key_header;
        let carried = match read.provider {
            Provider::Anthropic | Provider::Google => {
                key.carried_to(&address, header, table.secrets())
            }
            Provider::OpenAi => Credential::new(
                address.origin().clone(),
                header,
                &format!("Bearer {}", key.expose()),
                table.secrets(),
            ),
        };
        carried.map(Some).map_err(KindRefused::Credential)
    }
}

/// How a backend writes its requests and reads its answers: its provider's wire, with
/// what the request names of itself beyond its conversation.
pub(crate) enum Wire {
    Messages(anthropic::wire::Settings),
    Generate(google::wire::Settings),
    Responses(openai::wire::Settings),
}

impl Wire {
    fn of(
        provider: Provider,
        model: String,
        effort: Option<String>,
        ceilings: spend::Ceilings,
    ) -> Wire {
        let long_edge = provider.stated().long_edge;
        let max_output_tokens = ceilings.output_tokens;
        match provider {
            Provider::Anthropic => Wire::Messages(anthropic::wire::Settings {
                model,
                effort,
                long_edge,
                max_output_tokens,
            }),
            Provider::Google => Wire::Generate(google::wire::Settings {
                effort,
                long_edge,
                max_output_tokens,
            }),
            Provider::OpenAi => Wire::Responses(openai::wire::Settings {
                model,
                effort,
                long_edge,
                max_output_tokens,
            }),
        }
    }

    fn write(&self, request: &JudgeRequest) -> Result<Written, Fault> {
        match self {
            Wire::Messages(settings) => anthropic::wire::request(request, settings),
            Wire::Generate(settings) => google::wire::request(request, settings),
            Wire::Responses(settings) => openai::wire::request(request, settings),
        }
    }

    fn read(&self, body: &[u8]) -> Result<Turn, Fault> {
        match self {
            Wire::Messages(_) => anthropic::wire::turn(body),
            Wire::Generate(_) => google::wire::turn(body),
            Wire::Responses(_) => openai::wire::turn(body),
        }
    }

    /// The count of a written request's input, by the provider's formula.
    fn count(&self, written: &Written) -> u64 {
        let formula = match self {
            Wire::Messages(_) => anthropic::spend::picture_tokens,
            Wire::Generate(_) => google::spend::picture_tokens,
            Wire::Responses(_) => openai::spend::picture_tokens,
        };
        spend::input_count(
            written.word_bytes(),
            written.messages,
            &written.pictures,
            formula,
        )
    }

    /// Why a turn is an answer its model could not form, when it is one: a Gemini turn
    /// that stopped for one of [`google::wire::UNFORMED`].
    fn unformed(&self, turn: &Turn) -> Option<&'static str> {
        let Wire::Generate(_) = self else {
            return None;
        };
        match turn.stop() {
            StopReason::Other(reason) if google::wire::UNFORMED.contains(&reason.as_str()) => {
                Some(google::wire::MALFORMED)
            }
            _ => None,
        }
    }

    /// What an answer of an error states of itself, for the provider whose wire reads
    /// one: its own word for the error, one of a fixed list, and how long it says to
    /// wait. Only Gemini's errors are read (`google::wire::stated`); the others' state
    /// their wait in a header, which a reply does not carry.
    fn stated(&self, body: &[u8]) -> (Option<&'static str>, Option<u64>) {
        match self {
            Wire::Generate(_) => google::wire::stated(body),
            Wire::Messages(_) | Wire::Responses(_) => (None, None),
        }
    }

    /// Whether the provider's own word for an error says the same request may be sent
    /// again.
    fn transient(&self, word: &str) -> bool {
        matches!(self, Wire::Generate(_)) && google::wire::TRANSIENT.contains(&word)
    }

    /// The fields of a request, each a constant.
    fn fields(&self) -> &'static [(&'static str, &'static str)] {
        match self {
            Wire::Messages(_) => anthropic::FIELDS,
            Wire::Generate(_) => google::FIELDS,
            Wire::Responses(_) => openai::FIELDS,
        }
    }
}

/// A backend of the kind: one model of one provider at one address.
pub(crate) struct Service {
    pub(crate) declaration: Declaration,
    pub(crate) address: Address,
    pub(crate) wire: Wire,
    pub(crate) ceilings: spend::Ceilings,
    pub(crate) prices: spend::Prices,
}

impl Service {
    /// One request, sent through `sender`: written, sent, followed where the answer
    /// points (each address asked of the gate again, [`MOVES_LIMIT`] times at most),
    /// and its answer read.
    pub(crate) fn turn_through(
        &self,
        request: &JudgeRequest,
        sender: &dyn Sender,
        bounds: &Bounds,
    ) -> Result<Turn, Fault> {
        // The ceilings, before anything is handed over: the pictures, then the input
        // as counted. The output is held by the writer, which asks for no more.
        let over = |why| Err(Fault::new(FaultKind::TooLarge, why).not_sent());
        if body::pictures_in(request) > self.ceilings.pictures {
            return over(OVER_PICTURES);
        }
        let written = self.wire.write(request)?;
        let counted = self.wire.count(&written);
        if counted > self.ceilings.input_tokens {
            return over(OVER_INPUT);
        }
        let pieces = written.pieces;
        let mut to = self.address.clone();
        for _ in 0..=MOVES_LIMIT {
            let outgoing = Outgoing::new(to.clone(), pieces.clone())
                .and_then(|outgoing| outgoing.with_fields(self.wire.fields()))
                .map_err(|why| match why {
                    NotAllowed::TooMany => {
                        Fault::new(FaultKind::TooLarge, TOO_MANY_PIECES).not_sent()
                    }
                    _ => Fault::new(FaultKind::Rejected, body::NOT_SAID).not_sent(),
                })?;
            let reply = sender.send(&outgoing, bounds)?;
            if let Some(next) = reply.moved_to() {
                to = next.clone();
                continue;
            }
            // An error status is charged nothing: the provider charges no request it
            // refused or failed.
            let nothing = Charge::whole(0).ok();
            let (word, wait) = match reply.status() {
                Some(200) | None => (None, None),
                Some(_) => self.wire.stated(reply.body()),
            };
            let answered = |kind, why: &str| {
                let fault = Fault::new(kind, why).answered(nothing);
                // A wait is stated only of a request that may be sent again.
                match wait.filter(|_| kind == FaultKind::Unavailable) {
                    Some(wait) => Err(fault.retry_after_ms(wait)),
                    None => Err(fault),
                }
            };
            // The status, and the provider's word for it when it gave one of its own,
            // named after the sentence: a status the adapter does not act on is then
            // known from the run's record (the gate run of 2026-11-19 kept none).
            let named = |why: &str, status: u16| match word {
                Some(word) => format!("{why}; it answered {status} {word}"),
                None => format!("{why}; it answered {status}"),
            };
            return match reply.status() {
                // What a turn cost is its usage at the row; one that reports none
                // cost its bound, which the meter charges when no charge is said.
                Some(200) => self.wire.read(reply.body()).and_then(|turn| {
                    let turn = turn
                        .counted(counted)
                        .under_output_ceiling(self.ceilings.output_tokens);
                    let charge = turn
                        .usage()
                        .and_then(|usage| spend::charge_of(usage, self.prices));
                    // An answer the model could not form is a fault the caller may send
                    // again, settled at its usage like any answer.
                    if let Some(why) = self.wire.unformed(&turn) {
                        return Err(Fault::new(FaultKind::Unavailable, why).answered(charge));
                    }
                    Ok(match charge {
                        Some(charge) => turn.charged(charge),
                        None => turn,
                    })
                }),
                Some(400 | 404 | 413 | 422) => answered(FaultKind::Rejected, REFUSED),
                Some(401 | 403) => answered(FaultKind::Credential, CREDENTIAL),
                Some(429) => answered(FaultKind::Unavailable, RATE_LIMITED),
                Some(status @ (500 | 502 | 503 | 504 | 529)) => {
                    answered(FaultKind::Unavailable, &named(BUSY, status))
                }
                // A success the adapter does not read may have been charged: it costs
                // its bound, as an unreadable answer of 200 does, and never nothing.
                Some(201..=299) => {
                    Err(Fault::new(FaultKind::Malformed, OTHER_SUCCESS).answered(None))
                }
                // Another status that the provider's own word says is transient is sent
                // again as a busy one is; any other is named, and not.
                Some(status) if word.is_some_and(|word| self.wire.transient(word)) => {
                    answered(FaultKind::Unavailable, &named(BUSY, status))
                }
                Some(status) => answered(FaultKind::Malformed, &named(STATUS, status)),
                None => answered(FaultKind::Malformed, STATUS),
            };
        }
        Err(Fault::new(FaultKind::Rejected, MOVED_TOO_OFTEN).unresolved())
    }
}

impl Judge for Service {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn send(&self, request: &JudgeRequest, bounds: &Bounds) -> Result<Turn, Fault> {
        self.turn_through(request, bounds.sender(), bounds)
    }
}
