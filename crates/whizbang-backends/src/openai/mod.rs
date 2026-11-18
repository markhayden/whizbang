//! The OpenAI provider of the judge of a service (plan m2c, task P1; `SPEC.md` section
//! 4, "Judge"): the Responses interface at `https://api.openai.com/v1/responses`. The
//! kind `service` reads its table and makes its backend ([`crate::service`]); this
//! module holds what is the provider's own: its wire, its rows of prices and its count
//! of input, its address, its header and its fields.
//!
//! **The key** goes in the header `Authorization`, as `Bearer` and the key, bound to
//! the address's origin (ADR 0012, "A backend's credential").

pub mod spend;
pub(crate) mod wire;

#[cfg(test)]
mod tests;

use crate::service::Stated;
pub use wire::ADAPTER;

/// The provider's name, as a table gives it.
pub const PROVIDER: &str = "openai";

/// The address the kind sends to.
pub const ADDRESS: &str = "https://api.openai.com/v1/responses";

/// The header the key is carried in, as `Bearer` and the key.
pub const KEY_HEADER: &str = "authorization";

/// The version of the adapter: part of the model identity a backend declares.
pub const VERSION: &str = "1";

/// The long edge a picture is fitted to: the native long edge squint's catalogue states
/// for both models (2,048 pixels).
pub const LONG_EDGE: u32 = 2048;

/// The fields of a request, each a constant: the type of its body.
pub const FIELDS: &[(&str, &str)] = &[("content-type", "application/json")];

/// What the provider states of itself, for the kind to read a table by.
pub(crate) static STATED: Stated = Stated {
    name: PROVIDER,
    adapter: ADAPTER,
    version: VERSION,
    long_edge: LONG_EDGE,
    efforts: &["none", "low", "medium", "high", "xhigh", "max"],
    most_output: 128_000,
    key_header: KEY_HEADER,
    bad_setting: "a service backend of openai takes the settings effort, one of none, low, \
                  medium, high, xhigh and max, and max_input_tokens (1 to 100000), \
                  max_output_tokens (1 to 128000) and max_pictures (1 to 64), each a whole \
                  number",
    not_priced: "a service backend of openai names a model this build has a row of prices \
                 for: gpt-6.1-terra or gpt-6.1-sol",
    prices_of: spend::prices_of,
};
