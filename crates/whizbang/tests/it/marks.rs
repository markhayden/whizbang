//! Every mark a record states is shown by `whizbang describe`, and the controls carry none
//! (`SPEC.md` section 8, "Marks"; plan mE, task A0, item 3).

use std::path::PathBuf;

use whizbang_squint::marks::{self, Mark};

/// The folder of the tools' records.
fn records() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/reference/tools")
}

/// What `whizbang describe` prints, built in this process as the binary builds it:
/// the text form, or with `json` the JSON form. Other tests hold the binary's output to it.
fn describe(args: &[&str]) -> String {
    let registry = whizbang::wiring::registry().expect("the registry builds");
    let description = whizbang::wiring::description(&registry);
    if args.contains(&"--json") {
        serde_json::to_string(&description).unwrap()
    } else {
        whizbang::wiring::describe_text(&description)
    }
}

/// Each mark `whizbang describe --json` shows: the tool and the mark's word.
fn shown() -> Vec<(String, String)> {
    let doc: serde_json::Value =
        serde_json::from_str(&describe(&["describe", "--json"])).expect("valid JSON");
    doc["marks"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|m| {
            (
                m["tool"].as_str().unwrap().to_owned(),
                m["mark"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[test]
fn every_record_that_states_a_mark_is_shown_by_describe() {
    let shown = shown();
    let mut stating = Vec::new();
    for entry in std::fs::read_dir(records()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let name = path.file_stem().unwrap().to_str().unwrap().to_owned();
        let record = std::fs::read_to_string(&path).unwrap();
        let Some(mark) = marks::stated(&record) else {
            continue;
        };
        // Compiled in, as the file has it, and shown with the mark it states.
        let compiled = marks::RECORDS.iter().find(|(tool, _)| *tool == name);
        let (_, text) = compiled.unwrap_or_else(|| panic!("{name} states a mark and is not read"));
        assert_eq!(*text, record, "{name}");
        assert_eq!(marks::of(&name), Some(mark), "{name}");
        assert!(
            shown.contains(&(name.clone(), mark.word().to_owned())),
            "describe does not show {name} {}",
            mark.word()
        );
        stating.push(name);
    }
    // And `describe` shows no mark that no record states.
    for (tool, _) in &shown {
        assert!(
            stating.contains(tool),
            "{tool} is shown and no record states it"
        );
    }
    assert_eq!(stating.len(), shown.len());
    assert_eq!(shown.len(), 21);
}

#[test]
fn the_control_carries_no_mark() {
    // `view` and `align` are the control (`SPEC.md` section 8): complete with rungs 1 and 2,
    // the upper rungs cannot apply. Each record opens `**Mark.** None`, and gives none.
    for tool in ["view", "align"] {
        let record = std::fs::read_to_string(records().join(format!("{tool}.md"))).unwrap();
        let (_, part) = record.split_once("\n## Mark and history\n\n").unwrap();
        assert!(part.starts_with("**Mark.** None: "), "{tool}");
        assert!(part.contains("control"), "{tool}");
        assert_eq!(marks::stated(&record), None, "{tool}");
        assert_eq!(marks::of(tool), None, "{tool}");
    }
    assert!(
        shown()
            .iter()
            .all(|(tool, _)| tool != "view" && tool != "align")
    );
    let text = describe(&["describe"]);
    assert!(text.contains("\n  align  Registers "), "{text}");
    assert!(text.contains("\n  view  Shows "), "{text}");
    // What a judge is told of `align` measured first names it a control, not a mark.
    assert_eq!(whizbang_squint::details::mark("align"), "control");
    assert_eq!(whizbang_squint::details::mark("change_map"), "unverified");
}

#[test]
fn tiles_and_per_subject_are_shown_experimental() {
    // None is an action: three tool files and a detail, each offered only when a run names
    // it (`outline` and `regions`: plan mE, task T1).
    assert_eq!(
        marks::OFFERED,
        ["per_subject", "outline", "regions", "tiles"]
    );
    for tool in marks::OFFERED {
        assert_eq!(marks::of(tool), Some(Mark::Experimental), "{tool}");
    }
    let shown = shown();
    let tail: Vec<(&str, &str)> = shown[shown.len() - 4..]
        .iter()
        .map(|(t, m)| (t.as_str(), m.as_str()))
        .collect();
    assert_eq!(
        tail,
        [
            ("per_subject", "experimental"),
            ("outline", "experimental"),
            ("regions", "experimental"),
            ("tiles", "experimental")
        ]
    );
    // `tiles.md` opens its part with the mark in bold, as every other record does.
    let record = std::fs::read_to_string(records().join("tiles.md")).unwrap();
    assert!(record.contains("\n## Mark and history\n\n**Experimental** ("));
    // The text form lists them after the actions.
    let text = describe(&["describe"]);
    assert!(
        text.ends_with(
            "tools a run offers a judge only when it names them:\n  per_subject  \
             [experimental]\n  outline  [experimental]\n  regions  [experimental]\n  tiles  [experimental]\n"
        ),
        "{text}"
    );
}
