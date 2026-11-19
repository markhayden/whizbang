//! Rascal's constant sets (`SPEC.md` section 3, "Constant sets"): files of named values an
//! operation reads when a call does not give them. A set is copied byte for byte from
//! rascal's `crates/rascal-ops/constants/`, at the commit `docs/sources.toml` pins, by the
//! pull request that ports the operation reading it; `constants/digests.txt` lists the
//! digest of every set rascal shipped, and which are here (plan m3, "Copies are counted
//! apart"). A change to a value is a new set, never an edit.
//!
//! A call names its set by the action's argument `constants`, a choice of the sets'
//! names (ADR 0033).

/// The argument that names a constant set.
pub const ARGUMENT: &str = "constants";

/// `window-v1`: `window`'s context, as a share of the support's width and height.
pub const WINDOW_V1: (&str, &str) = ("window-v1", include_str!("../constants/window-v1.toml"));

/// The list of every set rascal shipped and its digest.
pub const DIGESTS: &str = include_str!("../constants/digests.txt");

/// The number `key` of a set's text, a whole number taken as a number. A set this build
/// ships is read by a test of this crate, so a key it lacks is a fault of the build.
pub fn number(set: &str, key: &str) -> Option<f64> {
    let table: toml::Table = toml::from_str(set).ok()?;
    match table.get(key)? {
        toml::Value::Float(value) => Some(*value),
        toml::Value::Integer(value) => Some(*value as f64),
        _ => None,
    }
}
