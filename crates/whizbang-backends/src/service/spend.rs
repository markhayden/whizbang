//! What one request of a judge of a service may cost, and what it cost (plan 2A, task
//! S2, item 1; ADR 0026), the same way for each provider: three ceilings held before a
//! byte is sent, the bound of a request from them at the model's row of prices, and
//! the charge of an answer from its usage at the same row. Each provider's module holds
//! its rows and its count of input (`anthropic::spend`, `google::spend`,
//! `openai::spend`).

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
    /// The prices of a prompt over [`LONG_PROMPT_TOKENS`], of the same reading.
    pub long: LongPrompt,
}

/// The prices of a prompt whose input passes [`LONG_PROMPT_TOKENS`], in US dollars per
/// million tokens, read on the same date as the row they belong to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LongPrompt {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

/// The input tokens above which the row's prices are not the ones that apply.
pub const LONG_PROMPT_TOKENS: u64 = 100_000;

/// The row of a model in these rows, when there is one.
pub fn row_of(rows: &[(&str, Prices)], model: &str) -> Option<Prices> {
    rows.iter()
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

/// The input an answer's usage reports: what was read from the cache, written to it,
/// and neither, together.
pub fn reported_input(usage: Usage) -> u64 {
    usage
        .input_tokens()
        .saturating_add(usage.cache_read_tokens())
        .saturating_add(usage.cache_write_tokens())
}

/// What an answer's usage cost, in millionths of a US dollar, rounded up: at the row,
/// or at its long prompt's prices when the reported input passes
/// [`LONG_PROMPT_TOKENS`].
pub fn charge_of(usage: Usage, prices: Prices) -> Option<Charge> {
    let long = reported_input(usage) > LONG_PROMPT_TOKENS;
    let (input, output, cache_read, cache_write) = if long {
        let long = prices.long;
        (long.input, long.output, long.cache_read, long.cache_write)
    } else {
        (
            prices.input,
            prices.output,
            prices.cache_read,
            prices.cache_write,
        )
    };
    #[allow(clippy::cast_precision_loss)]
    let at = |tokens: u64, price: f64| tokens as f64 * price;
    let micros = at(usage.input_tokens(), input)
        + at(usage.output_tokens(), output)
        + at(usage.cache_read_tokens(), cache_read)
        + at(usage.cache_write_tokens(), cache_write);
    Charge::of_micros(micros.ceil()).ok()
}

/// The fixed overhead counted for each message and each picture.
pub const OVERHEAD_TOKENS: u64 = 16;

/// The count of input of a request: one token for each byte of its words as written,
/// each picture's tokens by the provider's formula on its prepared size, and the
/// overhead for each message and each picture.
pub fn input_count(
    word_bytes: u64,
    messages: u64,
    pictures: &[(u32, u32)],
    formula: fn(u32, u32) -> u64,
) -> u64 {
    let pictured: u64 = pictures
        .iter()
        .map(|&(width, height)| formula(width, height) + OVERHEAD_TOKENS)
        .sum();
    word_bytes + messages * OVERHEAD_TOKENS + pictured
}
