//! The Anthropic provider of the judge of a service (plan 2A, task S1; `SPEC.md`
//! section 4, "Judge"): the Messages interface at `https://api.anthropic.com/v1/messages`.
//! The kind `service` reads its table and makes its backend ([`crate::service`]); this
//! module holds what is the provider's own: its wire, its rows of prices and its count
//! of input, its address, its header and its fields.
//!
//! **The key** goes in the header `x-api-key`, bound to the address's origin
//! (ADR 0012, "A backend's credential").

pub mod spend;
pub(crate) mod wire;

#[cfg(test)]
mod tests;

use crate::service::Stated;
pub use crate::service::{
    BUSY, CREDENTIAL, KIND, MOVED_TOO_OFTEN, OTHER_SUCCESS, OVER_INPUT, OVER_PICTURES,
    RATE_LIMITED, REFUSED, STATUS, ServiceKind, TOO_MANY_PIECES,
};
pub use wire::ADAPTER;

/// The provider's name, as a table gives it.
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

/// What the provider states of itself, for the kind to read a table by.
pub(crate) static STATED: Stated = Stated {
    name: PROVIDER,
    adapter: ADAPTER,
    version: VERSION,
    long_edge: LONG_EDGE,
    efforts: &["low", "medium", "high", "xhigh", "max"],
    most_output: 128_000,
    key_header: KEY_HEADER,
    bad_setting: "a service backend of anthropic takes the settings effort, one of low, \
                  medium, high, xhigh and max, and max_input_tokens (1 to 100000), \
                  max_output_tokens (1 to 128000) and max_pictures (1 to 64), each a whole \
                  number",
    not_priced: "a service backend of anthropic names a model this build has a row of \
                 prices for: claude-haiku-5-5",
    prices_of: spend::prices_of,
};
