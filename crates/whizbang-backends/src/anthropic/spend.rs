//! What one request of the judge of a service may cost, and what it cost (plan 2A,
//! task S2, item 1; ADR 0026).
//!
//! **A fixed bound for each request.** A price is stated before any request exists, so
//! the bound of a request is fixed by three ceilings of the backend, each held before a
//! byte is sent: the most input tokens it may carry by the adapter's count
//! ([`Ceilings::input_tokens`]), the most output tokens it may ask for, and the most
//! pictures. The bound is the input ceiling at the dearest input price of the model's
//! row (its cache writes) and the output ceiling at its output price.
//!
//! **The count of input is an upper bound by construction:** one token for each byte of
//! the request's words as they are written (its JSON, keys and all), and for each
//! picture the provider's published formula on its prepared size, rounded up, with a
//! fixed overhead for each message and each picture. It holds while the formula holds
//! (the plan's risk 1): `anthropic::the_input_count_is_at_least_the_usage_of_every_recorded_exchange`.
//!
//! **What a request cost** is worked out from the usage its answer reports, at the same
//! row, rounded up; an answer that reports none costs its bound. An answer of an error
//! status costs nothing: the provider charges no request it refused or failed.

use whizbang_core::capability::{Charge, Usage};

/// One row of a model's prices, in US dollars per million tokens, and the date the row
/// was read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prices {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    /// The date the row was read from the provider's page of prices.
    pub read: &'static str,
}

/// The models this build has a row of prices for: `claude-haiku-5-5`, the model of the
/// study. The row is squint's catalogue's (`crates/squint-providers/catalog.toml`, at
/// the commit `docs/sources.toml` pins), read from the provider's page of prices on
/// 2026-10-26; its prices for a prompt over 100,000 tokens are not needed, since the
/// input ceiling is under that and is held.
pub const PRICED: [(&str, Prices); 1] = [(
    "claude-haiku-5-5",
    Prices {
        input: 0.10,
        output: 0.50,
        cache_read: 0.01,
        cache_write: 0.125,
        read: "2026-10-26",
    },
)];

/// The input tokens above which the row's prices are not the ones that apply.
pub const LONG_PROMPT_TOKENS: u64 = 100_000;

/// The row of a model, when this build has one.
pub fn prices_of(model: &str) -> Option<Prices> {
    PRICED
        .iter()
        .find(|(priced, _)| *priced == model)
        .map(|(_, prices)| *prices)
}

/// The three ceilings of a backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ceilings {
    pub input_tokens: u64,
    pub output_tokens: u32,
    pub pictures: usize,
}

/// The ceilings when the table gives none: 40,000 input tokens, 2,048 output tokens
/// and 24 pictures.
pub const DEFAULT_CEILINGS: Ceilings = Ceilings {
    input_tokens: 40_000,
    output_tokens: 2_048,
    pictures: 24,
};

/// The bound of one request, in millionths of a US dollar: the input ceiling at the
/// dearer of the input and the cache write prices, and the output ceiling at the
/// output price.
pub fn bound_micros(ceilings: Ceilings, prices: Prices) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let (input, output) = (
        ceilings.input_tokens as f64,
        f64::from(ceilings.output_tokens),
    );
    input * prices.input.max(prices.cache_write) + output * prices.output
}

/// What an answer's usage cost, in millionths of a US dollar, rounded up.
pub fn charge_of(usage: Usage, prices: Prices) -> Option<Charge> {
    #[allow(clippy::cast_precision_loss)]
    let at = |tokens: u64, price: f64| tokens as f64 * price;
    let micros = at(usage.input_tokens(), prices.input)
        + at(usage.output_tokens(), prices.output)
        + at(usage.cache_read_tokens(), prices.cache_read)
        + at(usage.cache_write_tokens(), prices.cache_write);
    Charge::of_micros(micros.ceil()).ok()
}

/// The tokens the provider's published formula gives a picture of this prepared size:
/// one for each patch of 28 by 28 pixels, rounded up on each side, and 4,784 at most
/// (squint's catalogue's `tokens` of the model: `patch`, 28 pixels, 4,784 at most).
pub fn picture_tokens(width: u32, height: u32) -> u64 {
    let patches = u64::from(width.div_ceil(28)) * u64::from(height.div_ceil(28));
    patches.min(4_784)
}

/// The fixed overhead counted for each message and each picture.
pub const OVERHEAD_TOKENS: u64 = 16;

/// The count of input of a request: one token for each byte of its words as written,
/// each picture's tokens by the formula on its prepared size, and the overhead for each
/// message and each picture.
pub fn input_count(word_bytes: u64, messages: u64, pictures: &[(u32, u32)]) -> u64 {
    let pictured: u64 = pictures
        .iter()
        .map(|&(width, height)| picture_tokens(width, height) + OVERHEAD_TOKENS)
        .sum();
    word_bytes + messages * OVERHEAD_TOKENS + pictured
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bound_of_a_request_at_the_default_ceilings_is_the_arithmetic_of_the_adr() {
        let prices = prices_of("claude-haiku-5-5").unwrap();
        // 40,000 at 0.125 and 2,048 at 0.50, in millionths of a dollar.
        assert!((bound_micros(DEFAULT_CEILINGS, prices) - 6_024.0).abs() < 1e-9);
        let usage = Usage::new(1_000, 100).cached(10_000, 2_000);
        // 100 + 50 + 100 + 250.
        assert_eq!(charge_of(usage, prices).unwrap().micros(), 500);
        assert_eq!(picture_tokens(64, 48), 6);
        assert_eq!(picture_tokens(2576, 1932), 4_784);
        assert_eq!(input_count(100, 2, &[(28, 28)]), 100 + 32 + 1 + 16);
    }
}
