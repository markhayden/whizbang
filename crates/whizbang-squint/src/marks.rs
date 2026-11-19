//! The marks of squint's tools (`SPEC.md` section 8, "Marks"), each read from the tool's
//! record (`docs/reference/tools/<name>.md`, its part "Mark and history"), which is compiled
//! in: the first word in bold there is the mark. A record whose part opens `**Mark.** None`
//! gives no mark (a control or a pixel operation, to which the upper rungs do not apply).
//!
//! A tool has a mark only through its record: one with no record here has none to show, and
//! [`of`] gives it none, so it cannot be unverified or admitted (`SPEC.md` section 8: "A tool
//! is not finished, and cannot be marked unverified or admitted, without its record"). Who
//! runs an unmarked tool treats it as experimental: it runs only when named.

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

/// The records of squint's tools, by the tool's name: the five use cases' recipes (plan m3,
/// task Q1, item 5) first, in the order the description lists them, then the tools.
const RECORDS: [(&str, &str); 10] = [
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
        "view",
        include_str!("../../../docs/reference/tools/view.md"),
    ),
];

/// The mark a record states, or none: the first bold word of its part "Mark and history".
pub fn stated(record: &str) -> Option<Mark> {
    let (_, part) = record.split_once("\n## Mark and history\n")?;
    let (_, rest) = part.split_once("**")?;
    let (word, _) = rest.split_once("**")?;
    match word.trim_end_matches(['.', ':']) {
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
