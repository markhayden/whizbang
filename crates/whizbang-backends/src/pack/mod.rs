//! The pack client (`SPEC.md` section 4): whizbang's side of the protocol between it
//! and the Python pack, and the supervisor of the one worker process.
//!
//! It holds the protocol's codec ([`proto`]; ADR 0020) and the supervisor's process:
//! its start, the hello each way, its error output and its stop ([`worker`]; task P3a),
//! with the threads it makes ([`threads`]). The plan of checkpoint 1C (`docs/plans/`)
//! builds the rest here: one call through the supervisor, its queue, cancellation,
//! deadlines and restart, and the kind of backend `pack`.

pub mod proto;
pub mod threads;
pub mod worker;

pub use worker::{Ended, Launch, Said, Worker};
