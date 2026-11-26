//! The marks of the tools the upper rungs apply to (`SPEC.md` section 8, "Marks"), squint's
//! and rascal's, each read from the tool's record (`docs/reference/tools/<name>.md`, its part
//! "Mark and history"), which is compiled in. A record states its mark in one of two ways:
//! its first word in bold is the mark (`**Unverified**`), or it opens `**Mark.**` and the
//! mark is the word after it (`**Mark.** Experimental`). A record whose part opens
//! `**Mark.** None` gives no mark: a control or a pixel operation, to which the upper rungs
//! do not apply, complete when its record shows rungs 1 and 2.
//!
//! A tool has a mark only through its record: one with no record here has none to show, and
//! [`of`] gives it none, so it cannot be unverified or admitted (`SPEC.md` section 8: "A tool
//! is not finished, and cannot be marked unverified or admitted, without its record"). Who
//! runs an unmarked tool that is not a control treats it as experimental: it runs only when
//! named.

/// A mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// Present; nothing shown yet. Runs only when named.
    Experimental,
    /// Ported, with its record and rungs 1 and 2 done; its gate not cleared.
    Unverified,
    /// Cleared its gate.
    Admitted,
}

impl Mark {
    /// The mark as `whizbang describe` prints it.
    pub fn word(self) -> &'static str {
        match self {
            Mark::Experimental => "experimental",
            Mark::Unverified => "unverified",
            Mark::Admitted => "admitted",
        }
    }
}

/// The records of the tools the upper rungs apply to, and of the controls, by the tool's
/// name: first the actions, in the order the description lists them (rascal's recipes, the
/// five use cases' recipes, the loop, then the tools), then the tools that are no action,
/// each offered to a judge only when a run names it: the tool files `per_subject`,
/// `outline` and `regions` (plan mE, task T1) and the detail `tiles` (plan mE, task A0,
/// item 3). Every record under `docs/reference/tools/` that states a mark is here, which a
/// test holds.
pub const RECORDS: [(&str, &str); 22] = [
    (
        "remove_prompt",
        include_str!("../../../docs/reference/tools/remove_prompt.md"),
    ),
    (
        "remove",
        include_str!("../../../docs/reference/tools/remove.md"),
    ),
    (
        "repair_area",
        include_str!("../../../docs/reference/tools/repair_area.md"),
    ),
    (
        "repair",
        include_str!("../../../docs/reference/tools/repair.md"),
    ),
    (
        "seam",
        include_str!("../../../docs/reference/tools/seam.md"),
    ),
    (
        "source-fidelity",
        include_str!("../../../docs/reference/tools/source-fidelity.md"),
    ),
    (
        "reference-consistency",
        include_str!("../../../docs/reference/tools/reference-consistency.md"),
    ),
    (
        "reality-checks",
        include_str!("../../../docs/reference/tools/reality-checks.md"),
    ),
    (
        "brief-compliance",
        include_str!("../../../docs/reference/tools/brief-compliance.md"),
    ),
    (
        "product-fidelity",
        include_str!("../../../docs/reference/tools/product-fidelity.md"),
    ),
    (
        "remove_checked",
        include_str!("../../../docs/reference/tools/remove_checked.md"),
    ),
    (
        "align",
        include_str!("../../../docs/reference/tools/align.md"),
    ),
    (
        "change_map",
        include_str!("../../../docs/reference/tools/change_map.md"),
    ),
    (
        "edge_similarity",
        include_str!("../../../docs/reference/tools/edge_similarity.md"),
    ),
    (
        "resize",
        include_str!("../../../docs/reference/tools/resize.md"),
    ),
    (
        "segment",
        include_str!("../../../docs/reference/tools/segment.md"),
    ),
    (
        "view",
        include_str!("../../../docs/reference/tools/view.md"),
    ),
    (
        "fill",
        include_str!("../../../docs/reference/tools/fill.md"),
    ),
    (
        "per_subject",
        include_str!("../../../docs/reference/tools/per_subject.md"),
    ),
    (
        "outline",
        include_str!("../../../docs/reference/tools/outline.md"),
    ),
    (
        "regions",
        include_str!("../../../docs/reference/tools/regions.md"),
    ),
    (
        "tiles",
        include_str!("../../../docs/reference/tools/tiles.md"),
    ),
];

/// The tools among [`RECORDS`] that are no action: each offered to a judge only when a run
/// names it, the tool files `per_subject`, `outline` and `regions` and the detail `tiles`.
pub const OFFERED: [&str; 4] = ["per_subject", "outline", "regions", crate::details::TILES];

/// The mark a record states, or none: the first bold word of its part "Mark and history",
/// or, when that is `**Mark.**`, the word after it.
pub fn stated(record: &str) -> Option<Mark> {
    let (_, part) = record.split_once("\n## Mark and history\n")?;
    let (_, rest) = part.split_once("**")?;
    let (word, after) = rest.split_once("**")?;
    let word = match word {
        "Mark." => after
            .trim_start_matches(' ')
            .split(|c: char| !c.is_ascii_alphabetic())
            .next()?,
        bold => bold.trim_end_matches(['.', ':']),
    };
    match word {
        "Experimental" => Some(Mark::Experimental),
        "Unverified" => Some(Mark::Unverified),
        "Admitted" => Some(Mark::Admitted),
        _ => None,
    }
}

/// The mark of the tool of this name, as its record states it; none for a tool without a
/// record, or whose record states none.
pub fn of(tool: &str) -> Option<Mark> {
    of_among(&RECORDS, tool)
}

/// The mark of `tool` among these records.
pub fn of_among(records: &[(&str, &str)], tool: &str) -> Option<Mark> {
    let (_, record) = records.iter().find(|(name, _)| *name == tool)?;
    stated(record)
}

/// Every tool with a mark, and its mark, in the order of the records.
pub fn all() -> Vec<(&'static str, Mark)> {
    RECORDS
        .iter()
        .filter_map(|(name, record)| stated(record).map(|mark| (*name, mark)))
        .collect()
}
