//! What a request of a Google judge may cost (plan m2c, task P1, item 4; ADR 0026,
//! "Google and OpenAI"): its rows of prices and its count of input. The bound, the
//! charge and the ceilings are the kind's (`crate::service::spend`).

use crate::service::spend::{self as service_spend, LongPrompt, Prices};

/// The rows of prices this build has, from squint's catalogue
/// (`crates/squint-providers/catalog.toml`, at the commit `docs/sources.toml` pins),
/// read from the provider's page of prices on 2026-10-26: `gemini-3.8-flash`, the
/// cheapest of the study, 0.75 dollars a million input tokens, 3.75 output and 0.075
/// read from the cache; and `gemini-4-argon`, 2.00, 10.00 and 0.10. The catalogue
/// gives each as an introductory price (to 2026-12-31 for the first, to 2026-11-30 for
/// the second), and the row is re-read then (carry-over row on the Google prices). It
/// states no price of a cache write, which the adapter never reports (Gemini's usage
/// says what was read from a cache, not what was written to one), and no long prompt's:
/// a cache write is taken at the input price, and a long prompt at the row's own, the
/// input ceiling being held under the line.
pub const PRICED: [(&str, Prices); 2] = [
    (
        "gemini-3.8-flash",
        Prices {
            input: 0.75,
            output: 3.75,
            cache_read: 0.075,
            cache_write: 0.75,
            read: "2026-10-26",
            long: LongPrompt {
                input: 0.75,
                output: 3.75,
                cache_read: 0.075,
                cache_write: 0.75,
            },
        },
    ),
    (
        "gemini-4-argon",
        Prices {
            input: 2.0,
            output: 10.0,
            cache_read: 0.10,
            cache_write: 2.0,
            read: "2026-10-26",
            long: LongPrompt {
                input: 2.0,
                output: 10.0,
                cache_read: 0.10,
                cache_write: 2.0,
            },
        },
    ),
];

/// The row of a model, when this build has one.
pub fn prices_of(model: &str) -> Option<Prices> {
    service_spend::row_of(&PRICED, model)
}

/// The tokens a picture is counted at, whatever its prepared size: 1,120, the most
/// squint's catalogue gives a picture of either model (`tokens.max_tokens`, at the
/// default media resolution, which the adapter does not change). The tile formula
/// beside it (258 for each tile of 768 pixels) is not an upper bound: every recorded
/// picture of 64 by 48 pixels was reported at 1,064.
pub fn picture_tokens(_width: u32, _height: u32) -> u64 {
    1_120
}

/// The count of input of a request by [`picture_tokens`].
pub fn input_count(word_bytes: u64, messages: u64, pictures: &[(u32, u32)]) -> u64 {
    service_spend::input_count(word_bytes, messages, pictures, picture_tokens)
}
