//! HTTP/1.1 as the host speaks it (ADR 0041): the reading and writing both surfaces share,
//! in [`crate::http`] since the engine API (ADR 0045), and the policy the page is served
//! under. Nothing here decides who may call: that is [`super::guard`].

pub use crate::http::*;

/// The policy the page is served under (ADR 0041; the plan's "Who may call").
pub const PAGE_POLICY: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' blob: data:; frame-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";
