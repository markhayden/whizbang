//! What a request of fal's filler costs (plan m3, task F1, item 3; ADR 0010, "A price for
//! each call"): a fixed price for each call, which is the bound of a request. The answer
//! states no usage, so a call is settled at its bound.

/// The price of one call, in US dollars, and the date it was read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PerCall {
    pub usd: f64,
    /// The date the price was read from the model's page.
    pub read: &'static str,
}

/// Bria Eraser's price: "$0.04 per generation", read from fal's page of the model
/// (`https://fal.ai/models/fal-ai/bria/eraser`) on 2026-11-18.
pub const PRICE: PerCall = PerCall {
    usd: 0.04,
    read: "2026-11-18",
};

/// The bound of one request, in millionths of a US dollar: the price.
pub fn bound_micros(price: PerCall) -> f64 {
    (price.usd * 1_000_000.0).round()
}
