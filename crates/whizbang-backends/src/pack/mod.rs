//! The pack client (`SPEC.md` section 4): whizbang's side of the protocol between it
//! and the Python pack, and the supervisor of the one worker process.
//!
//! It holds the protocol's codec ([`proto`]; ADR 0020) and the supervisor's process:
//! its start, the hello each way, its error output and its stop ([`worker`]; task P3a),
//! with the threads it makes ([`threads`]), and one call through it (task P3b); and the
//! queue in front of the one worker, with cancellation, deadlines and restart
//! ([`queue`]; task P4). The plan of checkpoint 1C (`docs/plans/`) builds the rest here:
//! removal against the queue, and the kind of backend `pack`.

pub mod proto;
pub mod queue;
pub mod threads;
pub mod tool;
pub mod worker;

pub use queue::Supervisor;
pub use tool::{ToolLaunch, ToolWorker};
pub use worker::{Called, Ended, Failed, Launch, Limits, Said, Worker, card_of};
