//! The Google provider of the judge of a service (plan m2c, task P1; `SPEC.md` section
//! 4, "Judge"): Gemini's `generateContent` at
//! `https://generativelanguage.googleapis.com/v1beta/models/<model>:generateContent`.
//! The kind `service` reads its table and makes its backend ([`crate::service`]); this
//! module holds what is the provider's own: its wire, its rows of prices and its count
//! of input, its address, its header and its fields.
//!
//! **The key** goes in the header `x-goog-api-key`, bound to the address's origin, and
//! never in the address (Google's interface also takes it as a query, which this
//! adapter never writes; ADR 0012, "A backend's credential").

pub mod spend;
pub(crate) mod wire;

#[cfg(test)]
mod tests;

use crate::service::Stated;
pub use wire::{ADAPTER, NO_ID};

/// The provider's name, as a table gives it.
pub const PROVIDER: &str = "google";

/// The origin the kind sends to.
pub const ORIGIN: &str = "https://generativelanguage.googleapis.com";

/// The address of a model: the origin, then `/v1beta/models/<model>:generateContent`.
/// A model is 1 to 64 letters, digits, dots and hyphens, which the kind holds it to
/// before an address is made of it.
pub fn address(model: &str) -> String {
    format!("{ORIGIN}/v1beta/models/{model}:generateContent")
}

/// The header the key is carried in.
pub const KEY_HEADER: &str = "x-goog-api-key";

/// The version of the adapter: part of the model identity a backend declares.
pub const VERSION: &str = "1";

/// The long edge a picture is fitted to: the native long edge squint's catalogue states
/// for both models (1,536 pixels).
pub const LONG_EDGE: u32 = 1536;

/// The fields of a request, each a constant: the type of its body.
pub const FIELDS: &[(&str, &str)] = &[("content-type", "application/json")];

/// What the provider states of itself, for the kind to read a table by.
pub(crate) static STATED: Stated = Stated {
    name: PROVIDER,
    adapter: ADAPTER,
    version: VERSION,
    long_edge: LONG_EDGE,
    efforts: &["low", "medium", "high"],
    most_output: 65_536,
    key_header: KEY_HEADER,
    bad_setting: "a service backend of google takes the settings effort, one of low, medium \
                  and high, and max_input_tokens (1 to 100000), max_output_tokens (1 to \
                  65536) and max_pictures (1 to 64), each a whole number",
    not_priced: "a service backend of google names a model this build has a row of prices \
                 for: gemini-3.8-flash or gemini-4-argon",
    prices_of: spend::prices_of,
};
