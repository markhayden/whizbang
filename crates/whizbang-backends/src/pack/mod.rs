//! The pack client (`SPEC.md` section 4): whizbang's side of the protocol between it
//! and the Python pack, and the supervisor of the one worker process.
//!
//! A root only, so far. The plan of checkpoint 1C (`docs/plans/`) builds it here: the
//! protocol's codec, the supervisor with its queue, cancellation, deadlines and
//! restart, and the kind of backend `pack`.
