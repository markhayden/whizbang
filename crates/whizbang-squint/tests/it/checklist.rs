//! `checklist::…`: the evidence items a tool's regions add to the checklist (plan mE, task
//! T1, item 4; squint's `checklist.rs`, `ingest`, and its tests `tests/it/checklist.rs` at the
//! commit `docs/sources.toml` pins, the cases whizbang keeps), and the rules' `[hints]`.

use whizbang_squint::checklist::{self, Evidence, Located};
use whizbang_squint::use_case::{self, Checklist, Hints, Rules};

use crate::use_case::fitting;

fn located(image: &str, bounds: [f64; 4], label: &str) -> Located {
    Located {
        image: image.to_owned(),
        bounds,
        shown: format!(
            "{image} box [{}, {}, {}, {}]",
            bounds[0], bounds[1], bounds[2], bounds[3]
        ),
        label: label.to_owned(),
        score: 0.5,
    }
}

#[test]
fn regions_become_evidence_items_bound_by_their_kind_and_image() {
    // Source fidelity: SF1 on the candidate takes the rule file's default kinds, change
    // regions among them; SF2 takes alignment failures alone.
    let job = fitting("source-fidelity", None);
    let resolved = use_case::resolve(&job).unwrap();
    let sf1 = resolved.instance("SF1@candidate").unwrap();
    assert!(sf1.evidence_kinds.contains(&"change_region".to_owned()));
    assert_eq!(
        resolved.instance("SF2@candidate").unwrap().evidence_kinds,
        ["alignment_failure"]
    );
    let mut list = Checklist::build(&resolved, &job);
    let statics = list.items.len();
    let mut evidence = Evidence::default();
    let regions = [
        located("candidate", [10.0, 10.0, 50.0, 50.0], "pool added"),
        located("source", [10.0, 10.0, 50.0, 50.0], "on the source"),
        located("candidate", [12.0, 10.0, 50.0, 50.0], "pool added again"),
    ];
    let took = checklist::ingest(
        &mut list,
        &mut evidence,
        "call:3",
        "change_region",
        &regions,
        &resolved,
        8,
    )
    .unwrap();
    // The two on the candidate are items bound to SF1 alone; the one on the source binds
    // to nothing (no mandatory instance targets it) and is dropped, never shown.
    assert_eq!(took.added, ["chk:ev:3:0", "chk:ev:3:1"]);
    assert_eq!(
        took.dropped.iter().map(|d| d.region).collect::<Vec<_>>(),
        [1]
    );
    assert!(took.dropped[0].reason.contains("evidence kind"));
    assert_eq!(list.items.len(), statics + 2);
    let item = list.get("chk:ev:3:0").unwrap();
    assert_eq!(item.instances, ["SF1@candidate"]);
    // Two boxes overlapping past 0.7 name each other.
    assert_eq!(item.overlaps, ["chk:ev:3:1"]);
    assert_eq!(list.get("chk:ev:3:1").unwrap().overlaps, ["chk:ev:3:0"]);
    // The judge is shown each with its label, where it is, and what it overlaps.
    let shown = list.render(&resolved);
    assert!(
        shown.contains(
            "- chk:ev:3:0 [pool added] at candidate box [10, 10, 50, 50]: Every protected subject"
        ),
        "{shown}"
    );
    assert!(shown.contains("(overlaps chk:ev:3:1)"), "{shown}");
    // A kind no instance takes adds nothing.
    let none = checklist::ingest(
        &mut list,
        &mut evidence,
        "call:4",
        "anatomy",
        &regions[..1],
        &resolved,
        8,
    )
    .unwrap();
    assert!(none.added.is_empty() && none.dropped.len() == 1);
}

#[test]
fn a_result_past_the_budget_of_evidence_items_is_refused_whole() {
    let job = fitting("source-fidelity", None);
    let resolved = use_case::resolve(&job).unwrap();
    let mut list = Checklist::build(&resolved, &job);
    let before = list.clone();
    let mut evidence = Evidence::default();
    let regions = [
        located("candidate", [0.0, 0.0, 10.0, 10.0], "a"),
        located("candidate", [20.0, 20.0, 30.0, 30.0], "b"),
    ];
    let refused = checklist::ingest(
        &mut list,
        &mut evidence,
        "call:1",
        "change_region",
        &regions,
        &resolved,
        1,
    )
    .unwrap_err();
    assert!(refused.contains("a budget of 1"), "{refused}");
    assert_eq!((list, evidence.count()), (before, 0));
    let iou = checklist::iou;
    assert!((iou(&[0.0, 0.0, 2.0, 2.0], &[1.0, 0.0, 3.0, 2.0]) - 1.0 / 3.0).abs() < 1e-9);
    // The same box is 1; disjoint boxes, and boxes that only touch, are 0; a box inside
    // another is its share of it; boxes with no area are 0, never a division by zero.
    assert_eq!(iou(&[1.0, 1.0, 3.0, 3.0], &[1.0, 1.0, 3.0, 3.0]), 1.0);
    assert_eq!(iou(&[0.0, 0.0, 1.0, 1.0], &[5.0, 5.0, 6.0, 6.0]), 0.0);
    assert_eq!(iou(&[0.0, 0.0, 1.0, 1.0], &[1.0, 0.0, 2.0, 1.0]), 0.0);
    assert_eq!(iou(&[0.0, 0.0, 4.0, 4.0], &[1.0, 1.0, 3.0, 3.0]), 0.25);
    assert_eq!(iou(&[2.0, 2.0, 2.0, 2.0], &[2.0, 2.0, 2.0, 2.0]), 0.0);
    assert_eq!(iou(&[2.0, 2.0, 2.0, 5.0], &[0.0, 0.0, 4.0, 4.0]), 0.0);
}

#[test]
fn hints_are_read_from_a_rule_file_and_given_to_their_tools_as_arguments() {
    // Squint's profile of architecture held these; a base file holds none.
    let text = format!(
        "{}\n[hints.regions]\npreserve = [\"wall\", \"swimming pool\"]\nignore = [\"chair; table\"]\n\n\
         [hints.outline]\nclasses = [\"swimming pool\"]\n",
        use_case::TABLE[0].1
    );
    let rules: Rules = toml::from_str(&text).unwrap();
    assert_eq!(
        rules.hints.arguments("regions"),
        [
            ("preserve".to_owned(), "wall; swimming pool".to_owned()),
            ("ignore".to_owned(), "chair, table".to_owned()),
        ]
    );
    assert_eq!(
        rules.hints.arguments("outline"),
        [("phrases".to_owned(), "swimming pool".to_owned())]
    );
    assert!(rules.hints.arguments("per_subject").is_empty());
    for (name, _) in use_case::TABLE {
        assert_eq!(use_case::rules(name).unwrap().hints, Hints::default());
    }
    // A hint for a tool whizbang has none for, or a key it does not read, is refused.
    for wrong in [
        "[hints.classes]\nboxes = []\n",
        "[hints.regions]\nweights = [1]\n",
    ] {
        let text = format!("{}\n{wrong}", use_case::TABLE[0].1);
        assert!(toml::from_str::<Rules>(&text).is_err(), "{wrong}");
    }
}
