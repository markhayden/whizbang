//! Spend (`SPEC.md` section 6): the ledger of a store.
//!
//! A root only, so far. The plan of checkpoint 1B (`docs/plans/`) builds it here: the
//! ledger's records and the names they are published under, the fold that works out what
//! is allowed, reserved, spent and unresolved, and what reserves at admission, records a
//! dispatch before a request leaves and settles each attempt. Until those tasks merge,
//! the executor's interface to a ledger is `contract::Ledger` and nothing implements it
//! outside the test kit.
