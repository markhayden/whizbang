//! The use cases as recipes (plan m3, task Q1, item 1; `SPEC.md` section 7, "Use cases
//! expressed as recipes on the shared mechanism"): each a file of the format `recipe/1`
//! under `crates/whizbang-squint/recipes/`, built into the binary, read and checked against
//! the build's actions as any recipe is (`whizbang_core::recipe`). Each has two steps:
//! `details` ([`crate::measured`]), then the judge's step, `check` under the use case.

/// The recipe of each use case of the table, in the table's order, by its name.
pub const SHIPPED: [(&str, &str); 5] = [
    (
        crate::use_case::NAMES[0],
        include_str!("../recipes/source-fidelity.toml"),
    ),
    (
        crate::use_case::NAMES[1],
        include_str!("../recipes/reference-consistency.toml"),
    ),
    (
        crate::use_case::NAMES[2],
        include_str!("../recipes/reality-checks.toml"),
    ),
    (
        crate::use_case::NAMES[3],
        include_str!("../recipes/brief-compliance.toml"),
    ),
    (
        crate::use_case::NAMES[4],
        include_str!("../recipes/product-fidelity.toml"),
    ),
];

/// The kit the use cases belong to (ADR 0040).
pub const KIT: &str = "squint";

/// Each use case's title, as a person reads it, in the table's order (plan m5, task W1,
/// item 2): the recipes' files say none, so that their digests stay those measured.
pub const TITLES: [(&str, &str); 5] = [
    (crate::use_case::NAMES[0], "Check an edit"),
    (crate::use_case::NAMES[1], "Check against references"),
    (crate::use_case::NAMES[2], "Reality check"),
    (crate::use_case::NAMES[3], "Check against a brief"),
    (crate::use_case::NAMES[4], "Check a product shot"),
];

/// The step of each recipe that measures first, and the judge's step: what `--set` names.
pub const DETAILS_STEP: &str = "details";
pub const JUDGE_STEP: &str = "judge";
