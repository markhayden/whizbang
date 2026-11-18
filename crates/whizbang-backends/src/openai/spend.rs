//! What a request of an OpenAI judge may cost (plan m2c, task P1, item 4; ADR 0026,
//! "Google and OpenAI"): its rows of prices and its count of input. The bound, the
//! charge and the ceilings are the kind's (`crate::service::spend`).

use crate::service::spend::{self as service_spend, LongPrompt, Prices};

/// The rows of prices this build has, from squint's catalogue
/// (`crates/squint-providers/catalog.toml`, at the commit `docs/sources.toml` pins),
/// read from the provider's page of prices on 2026-10-26: `gpt-6.1-terra`, the cheapest
/// of the study, 1.00 dollar a million input tokens, 5.00 output and 0.05 read from the
/// cache; and `gpt-6.1-sol`, 2.00, 10.00 and 0.10. The catalogue states no price of a
/// cache write: the adapter counts what the provider says was written to a cache as
/// input, at the input price, and takes a cache write at the same. It states no long
/// prompt's price: a long prompt is taken at the row's own, the input ceiling being held
/// under the line.
pub const PRICED: [(&str, Prices); 2] = [
    (
        "gpt-6.1-terra",
        Prices {
            input: 1.0,
            output: 5.0,
            cache_read: 0.05,
            cache_write: 1.0,
            read: "2026-10-26",
            long: LongPrompt {
                input: 1.0,
                output: 5.0,
                cache_read: 0.05,
                cache_write: 1.0,
            },
        },
    ),
    (
        "gpt-6.1-sol",
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

/// The tokens the provider's published formula gives a picture of this prepared size:
/// one for each patch of 32 by 32 pixels, rounded up on each side, times 1.2, rounded
/// up (squint's catalogue's `tokens` of both models: `patch`, 32 pixels, `multiplier`
/// 1.2). The catalogue's most of 2,500 patches is not applied: the recorded pictures of
/// 2,000 by 1,500 pixels (2,961 patches) were each reported at about 3,550 tokens. A
/// picture is fitted to 2,048 pixels a side, so it is 4,916 tokens at most.
pub fn picture_tokens(width: u32, height: u32) -> u64 {
    let patches = u64::from(width.div_ceil(32)) * u64::from(height.div_ceil(32));
    (patches * 6).div_ceil(5)
}

/// The count of input of a request by [`picture_tokens`].
pub fn input_count(word_bytes: u64, messages: u64, pictures: &[(u32, u32)]) -> u64 {
    service_spend::input_count(word_bytes, messages, pictures, picture_tokens)
}
