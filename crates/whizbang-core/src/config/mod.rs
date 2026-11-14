//! Config (`SPEC.md` section 4): the one file that chooses a backend for each capability
//! and names, never holds, the credentials they need.
//!
//! A root only, so far. The plan of checkpoint 1B (`docs/plans/`) builds it here: the
//! file's type and its strict reader, its digest, the keys read from an env file, and
//! the snapshot an executor prepares a call under. The file is TOML, read with the `toml`
//! crate, which this crate depends on for that and for nothing else.
