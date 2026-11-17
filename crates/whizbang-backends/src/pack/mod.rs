//! The pack client (`SPEC.md` section 4): whizbang's side of the protocol between it
//! and the Python pack, and the supervisor of the one worker process.
//!
//! So far it holds the protocol's codec ([`proto`]; ADR 0020). The plan of checkpoint
//! 1C (`docs/plans/`) builds the rest here: the supervisor with its queue,
//! cancellation, deadlines and restart, and the kind of backend `pack`.

pub mod proto;
