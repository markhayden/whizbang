//! The table of use cases (plan 2C, task A1, item 1): its rows, squint's rule files byte
//! for byte, and the aggregation for every kind.

use serde_json::{Value as Json, json};
use whizbang_core::contract::record::Decision;
use whizbang_core::identity::Digest;
use whizbang_squint::job::{Job, Role};
use whizbang_squint::use_case::{
    self, Checklist, Confidence, Disposition, Instance, Kind, Row, TABLE, row,
};

/// The job the command line makes for a use case: a source where it takes one, and one
/// reference where it takes them.
pub fn fitting(name: &str, brief: Option<&str>) -> Job {
    let roles = use_case::rules(name).unwrap().rules.requires_roles;
    let references = usize::from(roles.contains(&Role::Reference));
    Job::shorthand(name, roles.contains(&Role::Source), references, brief)
}

/// A row of the table as the test states it: the name, the roles, the rules with their
/// kinds, and the checklist of a fitting job.
type Stated = (
    &'static str,
    &'static [Role],
    &'static [(&'static str, Kind)],
    &'static [&'static str],
);

#[test]
fn each_of_the_five_use_cases_is_in_the_table_with_its_roles_rules_and_checklist() {
    use Kind::{Allowed as A, MustHold as H, MustNot as N, Observe as O};
    use Role::{Candidate as C, Reference as R, Source as S};
    let rows: [Stated; 5] = [
        (
            "source-fidelity",
            &[C, S],
            &[("SF1", H), ("SF2", H), ("SF3", A), ("SF4", O)],
            &["chk:ob:SF1@candidate", "chk:ob:SF2@candidate"],
        ),
        (
            "reference-consistency",
            &[C, R],
            &[("RC1", H), ("RC2", H), ("RC3", H), ("RC4", A), ("RC5", O)],
            &[
                "chk:ob:RC1@candidate",
                "chk:ob:RC2@candidate",
                "chk:ob:RC3@candidate",
                "chk:ref:reference-1",
            ],
        ),
        (
            "reality-checks",
            &[C],
            &[("RE1", N), ("RE2", N), ("RE3", N), ("RE4", N), ("RE5", O)],
            &[
                "chk:ob:RE1@candidate",
                "chk:ob:RE2@candidate",
                "chk:ob:RE3@candidate",
                "chk:ob:RE4@candidate",
            ],
        ),
        (
            "brief-compliance",
            &[C],
            &[("BC1", H), ("BC2", N), ("BC3", H), ("BC4", A), ("BC5", O)],
            &[
                "chk:ob:BC1@candidate",
                "chk:ob:BC2@candidate",
                "chk:ob:BC3@candidate",
            ],
        ),
        (
            "product-fidelity",
            &[C, R],
            &[("PF1", H), ("PF2", H), ("PF3", H), ("PF4", A), ("PF5", O)],
            &[
                "chk:ob:PF1@candidate",
                "chk:ob:PF2@candidate",
                "chk:ob:PF3@candidate",
                "chk:ref:reference-1",
            ],
        ),
    ];
    assert_eq!(use_case::NAMES, rows.map(|(name, ..)| name));
    for (name, roles, rules, checklist) in rows {
        let read = use_case::rules(name).unwrap();
        assert_eq!(read.rules.use_case, name);
        assert_eq!(read.rules.requires_roles, roles, "{name}");
        let kinds: Vec<(&str, Kind)> = read
            .obligations
            .iter()
            .map(|o| (o.id.as_str(), o.kind))
            .collect();
        assert_eq!(kinds, rules, "{name}");
        let job = fitting(name, Some("A brief."));
        let resolved = use_case::resolve(&job).unwrap();
        let built = Checklist::build(&resolved, &job);
        assert_eq!(built.ids().collect::<Vec<_>>(), checklist, "{name}");
        // The prompt numbers every rule and names the kind of each in words.
        let system = use_case::system(&resolved, &job);
        for (id, kind) in rules {
            let said = match kind {
                H => format!("{id} (must hold;"),
                N => format!("{id} (must not occur;"),
                A => format!("{id} (allowed;"),
                O => format!("{id} (observe;"),
            };
            assert!(system.contains(&said), "{name}: {said}");
        }
    }
    assert!(use_case::rules("general").is_none());
}

fn instance(kind: Kind, matters: bool, hidden_ok: bool) -> Instance {
    Instance {
        obligation_id: "XX1".into(),
        target: "candidate".into(),
        kind,
        matters,
        hidden_ok,
        text: String::new(),
        evidence: Vec::new(),
        provenance: "rules",
    }
}

/// An answer for each item of the checklist: `held`, confidently, but for the one
/// named, which is given this disposition and confidence.
pub fn answers(checklist: &Checklist, named: &str, disposition: &str, confidence: &str) -> Json {
    let items: Vec<Json> = checklist
        .ids()
        .map(|id| {
            let (d, c) = if id == named {
                (disposition, confidence)
            } else {
                ("held", "confident")
            };
            let location = (d == "violation").then(|| {
                json!({"kind": "bbox", "frame": "raw:candidate", "bbox": [0.1, 0.1, 0.4, 0.4],
                       "polygon": null, "yaw": null, "pitch": null, "fov": null})
            });
            json!({"id": id, "disposition": d, "confidence": c, "reason": "Looked at.",
                   "allowed_by": null, "evidence": [], "location": location})
        })
        .collect();
    json!({"schema_version": 1, "items": items, "findings": [],
           "opinion": {"verdict": "pass", "reason": "Recorded only."}})
}

#[test]
fn a_confident_must_not_violation_fails_and_a_tentative_one_is_unsure() {
    // Squint's table, cell by cell, for a must_hold and a must_not alike: the kind
    // never changes the row.
    use Confidence::{Confident, Tentative};
    use Disposition::{AllowedChange, CantTell, Held, Hidden, Observed, Violation};
    let expected = |d, c, matters: bool, hidden_ok: bool| match (d, c) {
        (Violation, Confident) => Row::Fail,
        (Violation, Tentative) => Row::Unsure,
        (CantTell, _) | (Held | AllowedChange, Tentative) if matters => Row::Unsure,
        (Hidden, _) if matters && !hidden_ok => Row::Unsure,
        _ => Row::Recorded,
    };
    let mut cells = 0;
    for kind in [Kind::MustHold, Kind::MustNot] {
        for d in [Violation, Held, AllowedChange, Hidden, CantTell, Observed] {
            for c in [Confident, Tentative] {
                for (matters, hidden_ok) in
                    [(true, true), (true, false), (false, true), (false, false)]
                {
                    cells += 1;
                    let got = row(&instance(kind, matters, hidden_ok), d, c);
                    assert_eq!(
                        got,
                        expected(d, c, matters, hidden_ok),
                        "{kind:?} {d:?} {c:?}"
                    );
                }
            }
        }
    }
    assert_eq!(cells, 96);
    // Through a submission of reality-checks, whose rules are all must_not.
    let job = fitting("reality-checks", None);
    let resolved = use_case::resolve(&job).unwrap();
    let checklist = Checklist::build(&resolved, &job);
    let decided = |confidence: &str| {
        let submitted = answers(&checklist, "chk:ob:RE3@candidate", "violation", confidence);
        use_case::decided_of(&submitted, &job, &resolved, &checklist).unwrap()
    };
    let confident = decided("confident");
    assert_eq!(confident.verdict.decision(), Decision::Fail);
    assert!(
        confident
            .verdict
            .why()
            .starts_with("chk:ob:RE3@candidate: violation (confident)")
    );
    assert_eq!(decided("tentative").verdict.decision(), Decision::Unsure);
    let held = answers(&checklist, "", "held", "confident");
    let held = use_case::decided_of(&held, &job, &resolved, &checklist).unwrap();
    assert_eq!(held.verdict.decision(), Decision::Pass);
}

/// The SHA-256 of each file at the commit `docs/sources.toml` pins for squint:
/// `crates/squint-core/rules/<name>.toml`, and `crates/squint-core/prompts/v1.md`.
const SQUINTS: [(&str, &str); 5] = [
    (
        "source-fidelity",
        "21e48f14cae39a05e5ae160c2e3a1973a96adf8036916575b0d817ae419e1da7",
    ),
    (
        "reference-consistency",
        "15c89f179ef9d51467367a457ada8677e9b386335ab914f461861ef942134dc5",
    ),
    (
        "reality-checks",
        "b9ff72502160e27cdb00e3496c774c40fa013b906f4d8d693056c0d0295cfc54",
    ),
    (
        "brief-compliance",
        "09cad5a029f0afec4f9cd4fba97f8c9401158495bf6901371eb2381aac2e9ea0",
    ),
    (
        "product-fidelity",
        "8af991822da4f66476a905ba935cb7c786a85009d99ab871d7d24745986f2084",
    ),
];
const SQUINTS_TEMPLATE: &str = "4c5297e8d5829cafb4be5b9d5763a83bbc9a6c181ed103959c0c412279b05de7";

#[test]
fn the_ported_rules_are_squints_rules_byte_for_byte_less_the_fields_dropped() {
    for ((name, text), (squints, digest)) in TABLE.iter().zip(SQUINTS) {
        assert_eq!(*name, squints);
        assert_eq!(Digest::of_bytes(text.as_bytes()).hex(), digest, "{name}");
    }
    let template = include_str!("../../prompts/v1.md");
    assert_eq!(
        Digest::of_bytes(template.as_bytes()).hex(),
        SQUINTS_TEMPLATE
    );
    // Nothing of the files is lost in reading: no base file holds a field that was
    // dropped (a profile, `extends`, `[hints]`, `[screen]`), and a file that holds one
    // is not read.
    for (name, text) in TABLE {
        assert!(use_case::rules(name).is_some(), "{name}");
        for dropped in ["profile", "extends", "[hints", "[screen"] {
            assert!(!text.contains(dropped), "{name}: {dropped}");
        }
    }
    let screened = format!("{}\n[screen]\ntools = []\n", TABLE[0].1);
    assert!(toml::from_str::<use_case::Rules>(&screened).is_err());
}

#[test]
fn a_rejection_names_the_frames_and_rules_the_judge_may_use() {
    // E1's live run (plan 2C, task F1): of ten decisions ended as invalid submissions,
    // the first rejection of five was a frame that was not raw:, and the second of
    // most a finding that cited nothing or the wrong kind of rule. Each rejection names
    // what the judge may give instead, of the ids it was given, and nothing it wrote.
    let job = fitting("reference-consistency", None);
    let resolved = use_case::resolve(&job).unwrap();
    let checklist = Checklist::build(&resolved, &job);
    let rejected = |finding: Json| {
        let mut submitted = answers(&checklist, "", "held", "confident");
        submitted["findings"] = json!([finding]);
        use_case::decided_of(&submitted, &job, &resolved, &checklist).unwrap_err()
    };
    let finding = |ids: Json, disposition: &str, frame: &str| {
        json!({"obligation_ids": ids, "disposition": disposition, "confidence": "confident",
               "reason": "Seen.", "allowed_by": null, "evidence": [],
               "location": {"kind": "whole_image", "frame": frame, "bbox": null,
                            "polygon": null, "yaw": null, "pitch": null, "fov": null}})
    };
    let said = rejected(finding(json!(["RC1"]), "violation", "aligned:candidate:t1"));
    assert!(
        said.contains("not one of raw:candidate, raw:reference-1:"),
        "{said}"
    );
    let said = rejected(finding(json!([]), "violation", "raw:candidate"));
    assert!(
        said.ends_with("cites no obligation: obligation_ids lists the rules it bears on, of the obligations RC1, RC2, RC3, or for an observed finding the observe rules RC5"),
        "{said}"
    );
    let said = rejected(finding(json!(["RC4"]), "allowed_change", "raw:candidate"));
    assert!(said.contains("RC4 is an exemption"), "{said}");
    assert!(
        said.ends_with("RC4 waives nothing, so it is never an allowed_by"),
        "{said}"
    );
    let said = rejected(finding(json!(["RC1"]), "observed", "raw:candidate"));
    assert!(
        said.contains("observed findings cite observe rules only (RC5);"),
        "{said}"
    );
    let mut text = answers(&checklist, "", "held", "confident");
    text["items"] = Json::String(text["items"].to_string());
    let said = use_case::decided_of(&text, &job, &resolved, &checklist).unwrap_err();
    assert!(said.contains(": items is text;"), "{said}");
}
