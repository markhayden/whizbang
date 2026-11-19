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
//! row, rounded up; an answer that reports none costs its bound, and so does an answer
//! of a status of success the adapter does not list, as an unreadable answer of 200
//! does. A turn whose reported input passes [`LONG_PROMPT_TOKENS`] is charged the long
//! prompt's prices of the same reading ([`LongPrompt`]), so a charge is never below
//! the price. An answer of an error status costs nothing: the provider charges no
//! request it refused or failed.

use crate::service::spend as service_spend;
pub use crate::service::spend::{
    Ceilings, DEFAULT_CEILINGS, LONG_PROMPT_TOKENS, LongPrompt, OVERHEAD_TOKENS, Prices,
    bound_micros, charge_of, reported_input,
};

/// The models this build has a row of prices for: `claude-haiku-5-5`, the model of the
/// study. The row is squint's catalogue's (`crates/squint-providers/catalog.toml`, at
/// the commit `docs/sources.toml` pins), read from the provider's page of prices on
/// 2026-10-26, its long prompt's prices with it: over 100,000 tokens of input, 0.50
/// for input, 2.50 for output and 0.05 for cached input. The catalogue states no
/// price of a cache write for a long prompt; it is taken as five times the short
/// one, 0.625, the factor every price it does state for a long prompt has, so that
/// it is never below the price. The input ceiling is held under the line, so a long
/// prompt's charge applies only when the provider reports more input than the
/// adapter counted (ADR 0026, "A long prompt").
pub const PRICED: [(&str, Prices); 1] = [(
    "claude-haiku-5-5",
    Prices {
        input: 0.10,
        output: 0.50,
        cache_read: 0.01,
        cache_write: 0.125,
        read: "2026-10-26",
        long: LongPrompt {
            input: 0.50,
            output: 2.50,
            cache_read: 0.05,
            cache_write: 0.625,
        },
    },
)];

/// The row of a model, when this build has one.
pub fn prices_of(model: &str) -> Option<Prices> {
    service_spend::row_of(&PRICED, model)
}

/// The tokens the provider's published formula gives a picture of this prepared size:
/// one for each patch of 28 by 28 pixels, rounded up on each side, and 4,784 at most
/// (squint's catalogue's `tokens` of the model: `patch`, 28 pixels, 4,784 at most).
pub fn picture_tokens(width: u32, height: u32) -> u64 {
    let patches = u64::from(width.div_ceil(28)) * u64::from(height.div_ceil(28));
    patches.min(4_784)
}

/// The count of input of a request by [`picture_tokens`] ([`service_spend::input_count`]).
pub fn input_count(word_bytes: u64, messages: u64, pictures: &[(u32, u32)]) -> u64 {
    service_spend::input_count(word_bytes, messages, pictures, picture_tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    use whizbang_core::capability::Usage;

    #[test]
    fn the_bound_of_a_request_at_the_default_ceilings_is_the_arithmetic_of_the_adr() {
        let prices = prices_of("claude-haiku-5-5").unwrap();
        // 40,000 at 0.125 and 4,096 at 0.50, in millionths of a dollar.
        assert!((bound_micros(DEFAULT_CEILINGS, prices) - 7_048.0).abs() < 1e-9);
        let usage = Usage::new(1_000, 100).cached(10_000, 2_000);
        // 100 + 50 + 100 + 250.
        assert_eq!(charge_of(usage, prices).unwrap().micros(), 500);
        assert_eq!(picture_tokens(64, 48), 6);
        assert_eq!(picture_tokens(2576, 1932), 4_784);
        assert_eq!(input_count(100, 2, &[(28, 28)]), 100 + 32 + 1 + 16);
    }

    #[test]
    fn every_priced_models_check_at_the_default_ceilings_fits_the_example_cap() {
        // The sixteen turns `check` reserves, at the default ceilings, of every model
        // this build prices, are within the example's cap of 2.00 dollars (ADR 0026).
        let rows = PRICED
            .iter()
            .chain(crate::google::spend::PRICED.iter())
            .chain(crate::openai::spend::PRICED.iter());
        let mut dearest: f64 = 0.0;
        for (model, prices) in rows {
            let check = bound_micros(DEFAULT_CEILINGS, *prices) * 16.0;
            assert!(check <= 2_000_000.0, "{model}: {check}");
            dearest = dearest.max(check);
        }
        // gemini-4-argon and gpt-6.1-sol: 40,000 at 2.00 and 4,096 at 10.00, sixteen
        // times; twice the output ceiling would pass the cap.
        assert!((dearest - 1_935_360.0).abs() < 1e-6, "{dearest}");
    }
}
