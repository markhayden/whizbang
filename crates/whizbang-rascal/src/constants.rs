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

/// `tighten-v1`: `tighten`'s least contrast, narrowest and widest defect, smoothing along
/// the path and spacing of its samples.
pub const TIGHTEN_V1: (&str, &str) = ("tighten-v1", include_str!("../constants/tighten-v1.toml"));

/// `poisson-v1`: the tolerance and the ceiling of iterations of `paste`'s mode `poisson`.
pub const POISSON_V1: (&str, &str) = ("poisson-v1", include_str!("../constants/poisson-v1.toml"));

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

/// `fan`'s sets, the default first: `fan-v3`, as at rascal's tag (task O3).
pub const FAN: [(&str, &str); 4] = [
    ("fan-v3", include_str!("../constants/fan-v3.toml")),
    ("fan-v1", include_str!("../constants/fan-v1.toml")),
    ("fan-thin-v1", include_str!("../constants/fan-thin-v1.toml")),
    ("fan-v2", include_str!("../constants/fan-v2.toml")),
];

/// `profile-v1`: the feather's sigma as a share of a row's half-width.
pub const PROFILE_V1: (&str, &str) = ("profile-v1", include_str!("../constants/profile-v1.toml"));

/// `ramp-v1`: the share of the step each side moves, and the rows' smoothing.
pub const RAMP_V1: (&str, &str) = ("ramp-v1", include_str!("../constants/ramp-v1.toml"));

/// The value `key` of a set's text, of whatever type.
pub fn value(set: &str, key: &str) -> Option<toml::Value> {
    let table: toml::Table = toml::from_str(set).ok()?;
    table.get(key).cloned()
}

/// The text of the set of this name this build ships, whichever action reads it: what a
/// lab's cell takes the contents of each step's set from (plan m3, task L1).
pub fn text_of(name: &str) -> Option<&'static str> {
    let sets = [
        WINDOW_V1,
        TIGHTEN_V1,
        POISSON_V1,
        PROFILE_V1,
        RAMP_V1,
        MEASURES_V1,
        FEATHER_V1,
    ];
    (sets.iter().chain(FAN.iter()))
        .find(|(set, _)| *set == name)
        .map(|(_, text)| *text)
}

/// `measures-v1`: the chroma below which `measure` omits a hue difference, and the
/// percentile it reports beside the mean, the median and the largest (task O5).
pub const MEASURES_V1: (&str, &str) =
    ("measures-v1", include_str!("../constants/measures-v1.toml"));

/// `feather-v1`: how many sigmas out a feather reaches, which `feather::SUPPORT` holds;
/// copied with the last of the sets (task O5), and read by a test of this crate.
pub const FEATHER_V1: (&str, &str) = ("feather-v1", include_str!("../constants/feather-v1.toml"));
