use std::sync::Arc;

use whizbang_testkit::echo::Echo;
use whizbang_testkit::scratch::Scratch;

fn whizbang(args: &[&str]) -> (bool, String) {
    let out = whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang"))
        .args(args)
        .output()
        .expect("the binary runs");
    (
        out.status.success(),
        String::from_utf8(out.stdout).expect("output is UTF-8"),
    )
}

/// The indented form, byte for byte: what the reference holds. Two-space indentation, one
/// field per line, one newline at the end.
fn indented_description() -> &'static str {
    include_str!("../../../../docs/reference/description.json")
}

/// How the one-line form starts and ends, byte for byte: the same fields in the same
/// order, with no space between them, and one newline at the end. The first action is
/// `check`, the recipe.
fn compact_start() -> String {
    format!(
        "{{\"description_version\":12,\"name\":\"whizbang\",\"version\":\"{}\",\"kits\":[{{\"name\":\"rascal\",\"title\":\"Rascal\",\"line\":\"edit pictures\"}},{{\"name\":\"squint\",\"title\":\"Squint\",\"line\":\"check pictures\"}}],\"actions\":[{{\"name\":\"check\",\"version\":\"1.8.0\",\"kind\":\"recipe\",\"purpose\":\"",
        env!("CARGO_PKG_VERSION")
    )
}

const COMPACT_END: &str = "\"travels_as\":\"canonical_file\"}]}\n";

#[test]
fn describe_json_is_one_line_with_one_newline_at_the_end() {
    let (ok, text) = whizbang(&["describe", "--json"]);
    assert!(ok);
    assert_eq!(text.matches('\n').count(), 1, "{text}");
    assert!(text.ends_with("}\n"), "{text}");
    // Not "holds no colon and space": a purpose may hold one.
    assert!(text.starts_with(&compact_start()), "{text}");
    assert!(text.ends_with(COMPACT_END), "{text}");
    // No space between two fields, and none after a colon outside a purpose.
    assert!(!text.contains("\", \"") && !text.contains("\": "), "{text}");
}

#[test]
fn describe_json_and_pretty_hold_the_same_value() {
    let (_, compact) = whizbang(&["describe", "--json"]);
    let (_, pretty) = whizbang(&["describe", "--json", "--pretty"]);
    assert_ne!(compact, pretty);
    let compact: serde_json::Value = serde_json::from_str(&compact).expect("valid JSON");
    let pretty: serde_json::Value = serde_json::from_str(&pretty).expect("valid JSON");
    assert_eq!(compact, pretty);
}

#[test]
fn describe_json_pretty_prints_the_bytes_the_reference_holds() {
    let (ok, pretty) = whizbang(&["describe", "--json", "--pretty"]);
    assert!(ok);
    assert_eq!(pretty, indented_description());
}

#[test]
fn describe_json_pretty_is_byte_identical_across_runs() {
    let (_, first) = whizbang(&["describe", "--json", "--pretty"]);
    let (_, second) = whizbang(&["describe", "--json", "--pretty"]);
    assert_eq!(first, second);
}

#[test]
fn pretty_may_come_before_json() {
    let (ok, text) = whizbang(&["describe", "--pretty", "--json"]);
    assert!(ok);
    assert_eq!(text, indented_description());
}

#[test]
fn pretty_without_json_is_a_usage_error_that_names_json() {
    let out = whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang"))
        .args(["describe", "--pretty"])
        .output()
        .expect("the binary runs");
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let complaint = String::from_utf8(out.stderr).expect("output is UTF-8");
    assert!(complaint.contains("--json"), "{complaint}");
}

#[test]
fn describe_help_lists_pretty() {
    let (ok, text) = whizbang(&["describe", "--help"]);
    assert!(ok);
    assert!(text.contains("--pretty"), "{text}");
}

#[test]
fn describe_json_is_valid_and_lists_the_actions_that_ship() {
    let (ok, text) = whizbang(&["describe", "--json"]);
    assert!(ok);
    let doc: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    assert_eq!(doc["name"], "whizbang");
    assert_eq!(doc["description_version"], 12);
    assert_eq!(doc["version"], env!("CARGO_PKG_VERSION"));
    let names: Vec<&str> = doc["actions"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|action| action["name"].as_str().expect("a name"))
        .collect();
    // The recipes first, then the primitives, in the order the registry lists them.
    assert_eq!(
        names,
        [
            "check",
            "remove_prompt",
            "remove",
            "repair_area",
            "mask_rect",
            "protect",
            "repair",
            "seam",
            "source-fidelity",
            "reference-consistency",
            "reality-checks",
            "brief-compliance",
            "product-fidelity",
            "remove_checked",
            "align",
            "change_map",
            "edge_similarity",
            "details",
            "close",
            "feather",
            "grow",
            "plug",
            "render",
            "resize",
            "segment",
            "labels",
            "select",
            "subtract",
            "view",
            "paste",
            "fill",
            "cut",
            "put",
            "window",
            "unwindow",
            "apply",
            "line",
            "rect",
            "stroke",
            "rasterize",
            "shrink",
            "invert",
            "carry",
            "tighten",
            "inspect",
            "fan",
            "profile",
            "ramp",
            "measure"
        ]
    );
    let kinds: Vec<&str> = doc["actions"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|action| action["kind"].as_str().expect("a kind"))
        .collect();
    // `check`, rascal's seven recipes and the five use cases' (plan m3, task Q1).
    assert_eq!(kinds[..13], ["recipe"; 13]);
    // Then the one loop (plan m4, task L1), and the primitives.
    assert_eq!(kinds[13], "loop");
    assert!(
        kinds[14..].iter().all(|kind| *kind == "primitive"),
        "{kinds:?}"
    );
    // `check` says what it needs: a judge. Whether one is there is no part of a
    // description, which is the same on every machine.
    assert_eq!(doc["actions"][0]["needs"][0]["capability"], "judge");
    // And for how many requests: the turns of its loop.
    assert_eq!(doc["actions"][0]["needs"][0]["requests"], 16);
    // `segment` needs a segmenter that takes what the call's `kind` names, a phrase, a
    // point or a box, chosen by its argument `backend`.
    let segment = (doc["actions"].as_array().unwrap().iter())
        .find(|action| action["name"] == "segment")
        .unwrap();
    let need = &segment["needs"][0];
    assert_eq!(need["capability"], "segment");
    assert_eq!(need["features"], serde_json::json!([]));
    assert_eq!(need["feature_from"], "kind");
    assert_eq!(need["chosen_by"], "backend");
}

#[test]
fn describe_json_is_byte_identical_across_runs() {
    let (_, first) = whizbang(&["describe", "--json"]);
    let (_, second) = whizbang(&["describe", "--json"]);
    assert_eq!(first, second);
    assert!(first.ends_with('\n'));
}

#[test]
fn describe_without_json_names_the_build() {
    let (ok, text) = whizbang(&["describe"]);
    assert!(ok);
    assert!(text.contains("whizbang"));
    assert!(!text.contains("no actions yet"));
    assert!(
        text.contains(
            "\n  grow  Grows the core of a mask by a disc, and further downward for a shadow.\n"
        ),
        "{text}"
    );
    // Each action's line, and under a recipe the line of when to use it; after them, the
    // tools that are no action (`marks::tiles_and_per_subject_are_shown_experimental`).
    let (actions, _) = text
        .split_once("tools a run offers a judge only when it names them:\n")
        .expect("the tools offered");
    let lines: Vec<&str> = (actions.lines().skip(1))
        .filter(|line| !line.starts_with("      use: "))
        .collect();
    let names: Vec<&str> = lines
        .iter()
        .map(|line| line.split_whitespace().next().expect("a name"))
        .collect();
    assert_eq!(
        names,
        [
            "check",
            "remove_prompt",
            "remove",
            "repair_area",
            "mask_rect",
            "protect",
            "repair",
            "seam",
            "source-fidelity",
            "reference-consistency",
            "reality-checks",
            "brief-compliance",
            "product-fidelity",
            "remove_checked",
            "align",
            "change_map",
            "edge_similarity",
            "details",
            "close",
            "feather",
            "grow",
            "plug",
            "render",
            "resize",
            "segment",
            "labels",
            "select",
            "subtract",
            "view",
            "paste",
            "fill",
            "cut",
            "put",
            "window",
            "unwindow",
            "apply",
            "line",
            "rect",
            "stroke",
            "rasterize",
            "shrink",
            "invert",
            "carry",
            "tighten",
            "inspect",
            "fan",
            "profile",
            "ramp",
            "measure"
        ],
        "{text}"
    );
}

#[test]
fn describe_json_lists_the_six_artifact_types() {
    let (_, text) = whizbang(&["describe", "--json"]);
    let doc: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    let names: Vec<&str> = doc["artifact_types"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|summary| summary["name"].as_str().expect("a name"))
        .collect();
    assert_eq!(
        names,
        [
            "mask",
            "location",
            "image",
            "band_profile",
            "series",
            "plan"
        ]
    );
}

#[test]
fn the_shipping_binary_describes_what_the_shared_registry_holds() {
    // The binary and this test build the registry with the same function.
    let registry = whizbang::wiring::registry().expect("the registry builds");
    let expected = serde_json::to_value(whizbang::wiring::description(&registry)).expect("JSON");
    let (_, text) = whizbang(&["describe", "--json"]);
    let doc: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    assert_eq!(doc, expected);
}

#[test]
fn the_shipping_description_does_not_contain_echo() {
    for arguments in [&["describe", "--json"][..], &["describe"][..]] {
        let (ok, text) = whizbang(arguments);
        assert!(ok);
        assert!(!text.contains("echo"), "{text}");
    }
}

#[test]
fn a_test_adds_echo_to_the_registry_the_binary_builds() {
    let mut registry = whizbang::wiring::registry().expect("the registry builds");
    let shipped = registry.len();
    registry
        .register(Arc::new(Echo::new()))
        .expect("echo is registered");
    let description = whizbang::wiring::description(&registry);
    assert_eq!(description.actions().len(), shipped + 1);
    assert_eq!(description.actions()[shipped].name(), "echo");
    assert_eq!(description.version(), env!("CARGO_PKG_VERSION"));
    // And the text a person reads lists it.
    let text = whizbang::wiring::describe_text(&description);
    let shipped = whizbang::wiring::registry().expect("the registry builds");
    let shipped = whizbang::wiring::describe_text(&whizbang::wiring::description(&shipped));
    assert!(shipped.starts_with(&format!("whizbang {}\n", env!("CARGO_PKG_VERSION"))));
    // After the last action, before the tools that are no action (plan mE, task A0).
    let at = shipped
        .find("tools a run offers a judge")
        .expect("the tools offered");
    let (actions, offered) = shipped.split_at(at);
    assert_eq!(
        text,
        format!(
            "{actions}  echo  Returns the text it was given; a fake for tests, which never \
             ships.\n{offered}"
        )
    );
}

/// The indented form, pinned here and not only against the reference: its first lines, and
/// the artifact types whole.
#[test]
fn describe_json_pretty_starts_and_ends_with_these_bytes() {
    let (_, pretty) = whizbang(&["describe", "--json", "--pretty"]);
    let start = format!(
        "{{\n  \"description_version\": 12,\n  \"name\": \"whizbang\",\n  \"version\": \"{}\",\n  \"kits\": [\n    {{\n      \"name\": \"rascal\",\n      \"title\": \"Rascal\",\n      \"line\": \"edit pictures\"\n    }},\n    {{\n      \"name\": \"squint\",\n      \"title\": \"Squint\",\n      \"line\": \"check pictures\"\n    }}\n  ],\n  \"actions\": [\n    {{\n      \"name\": \"check\",\n",
        env!("CARGO_PKG_VERSION")
    );
    let types = r#"    {
      "name": "mask",
      "purpose": "What to change in an image (core), how the change fades (blend) and what to keep (protect), with the band profile and the prescriptions it carries.",
      "travels_as": "canonical_file"
    },
    {
      "name": "location",
      "purpose": "Where something is in an image: a box, a polygon, a view, a line, a stroke, the whole image, or that it is absent.",
      "travels_as": "canonical_file"
    },
    {
      "name": "image",
      "purpose": "A picture file held whole: a photograph, a composite or a crop; or a rendering of another artifact, which is never accepted in that artifact's place.",
      "travels_as": "canonical_file"
    },
    {
      "name": "band_profile",
      "purpose": "A band along a line: its half-width for each row, with the line and the sampling windows it was measured through.",
      "travels_as": "canonical_file"
    },
    {
      "name": "series",
      "purpose": "Values measured along the rows of an image, such as the width of a feather or the step of a tone ramp for each row.",
      "travels_as": "canonical_file"
    },
    {
      "name": "plan",
      "purpose": "How a wrapped cut was taken from a panorama, and so how it goes back: the width of the strip taken from each side of the seam.",
      "travels_as": "canonical_file"
    }
  ]
}
"#;
    assert!(pretty.starts_with(&start), "{pretty}");
    assert!(
        pretty.ends_with(&format!("  ],\n  \"artifact_types\": [\n{types}")),
        "{pretty}"
    );
}

#[test]
fn a_write_that_fails_is_exit_3_with_the_reason() {
    // A file may not grow at all (`ulimit -f 0`), and the signal that would kill the
    // writer is ignored, so the write of the description to a file fails with an error
    // the command sees. No special device is needed. Every output ends in a new line
    // and the standard output is flushed at each one, so the failure is the write's.
    // The folder goes when `dir` is dropped, whether or not an assertion below fails.
    let dir = Scratch::new("full");
    let file = dir.path().join("out.txt");
    for form in ["describe", "describe --json"] {
        let script = format!("trap '' XFSZ; ulimit -f 0; exec \"$WHIZBANG\" {form} > \"$OUT\"");
        let out = whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang"))
            .under_shell(&script)
            .env("OUT", &file)
            .output()
            .unwrap();
        let said = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(3), "{form}: {said}");
        assert!(
            said.starts_with("whizbang: could not write: "),
            "{form}: {said}"
        );
        assert_eq!(std::fs::read(&file).unwrap(), b"", "{form}");
    }
}

// --- marks (plan m2c, task X1, item 6) ---------------------------------------------------------

/// The record of a tool, if it has one.
fn record_of(tool: &str) -> Option<String> {
    let dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/reference/tools");
    std::fs::read_to_string(dir.join(format!("{tool}.md"))).ok()
}

#[test]
fn each_tool_the_upper_rungs_apply_to_shows_its_mark_from_its_record() {
    let (ok, text) = whizbang(&["describe", "--json"]);
    assert!(ok);
    let doc: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    let marks: Vec<(String, String)> = doc["marks"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|m| {
            (
                m["tool"].as_str().unwrap().to_owned(),
                m["mark"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    // Each action whose record states a mark shows that mark, and no other action shows one;
    // after them, the tools that are no action (plan mE, task A0, item 3).
    let mut want = Vec::new();
    for action in doc["actions"].as_array().unwrap() {
        let name = action["name"].as_str().unwrap();
        let stated = record_of(name).and_then(|record| whizbang_squint::marks::stated(&record));
        if let Some(mark) = stated {
            want.push((name.to_owned(), mark.word().to_owned()));
        }
    }
    for name in whizbang_squint::marks::OFFERED {
        let mark = whizbang_squint::marks::stated(&record_of(name).unwrap()).unwrap();
        want.push((name.to_owned(), mark.word().to_owned()));
    }
    let shown: Vec<(&str, &str)> = marks
        .iter()
        .map(|(t, m)| (t.as_str(), m.as_str()))
        .collect();
    assert_eq!(
        shown,
        [
            ("remove_prompt", "experimental"),
            ("remove", "experimental"),
            ("repair_area", "experimental"),
            ("repair", "experimental"),
            ("seam", "experimental"),
            ("source-fidelity", "unverified"),
            ("reference-consistency", "unverified"),
            ("reality-checks", "unverified"),
            ("brief-compliance", "unverified"),
            ("product-fidelity", "unverified"),
            ("remove_checked", "experimental"),
            ("change_map", "unverified"),
            ("edge_similarity", "unverified"),
            ("resize", "experimental"),
            ("segment", "experimental"),
            ("labels", "experimental"),
            ("fill", "experimental"),
            ("per_subject", "experimental"),
            ("outline", "experimental"),
            ("regions", "experimental"),
            ("classes", "experimental"),
            ("tiles", "experimental"),
        ]
    );
    assert_eq!(marks, want);
    // The text form shows each beside the tool's name; the controls carry none.
    let (ok, text) = whizbang(&["describe"]);
    assert!(ok);
    assert!(text.contains("\n  align  Registers "), "{text}");
    assert!(
        text.contains("\n  change_map  [unverified]  Shows "),
        "{text}"
    );
    assert!(text.contains("\n  view  Shows "), "{text}");
}

#[test]
fn a_tool_without_a_record_cannot_be_unverified_or_admitted() {
    use whizbang_squint::marks::{Mark, of, of_among, stated};
    let record = "# t\n\n## Mark and history\n\n**Admitted**: cleared its gate.\n";
    assert_eq!(stated(record), Some(Mark::Admitted));
    // The same tool, its record not among those compiled in: no mark at all.
    assert_eq!(of_among(&[("t", record)], "t"), Some(Mark::Admitted));
    assert_eq!(of_among(&[("u", record)], "t"), None);
    // An action that ships without a record has none, and shows none.
    assert!(record_of("check").is_none(), "check has a record now");
    assert_eq!(of("check"), None);
    // Rascal's records (plan m3, task D1) state their mark after `**Mark.**`, and give it
    // (plan mE, task A0, item 3); its pixel operations state none, and give none.
    for tool in ["fill", "segment"] {
        assert_eq!(of(tool), Some(Mark::Experimental), "{tool}");
    }
    for tool in ["feather", "plug", "render", "select"] {
        assert!(record_of(tool).is_some(), "{tool} has no record");
        assert_eq!(of(tool), None, "{tool}");
    }
    assert_eq!(
        stated("# t\n\n## Mark and history\n\n**Mark.** Experimental. Rung 1 only.\n"),
        Some(Mark::Experimental)
    );
    assert_eq!(
        stated("# t\n\n## Mark and history\n\n**Mark.** Unverified: rungs 1 and 2.\n"),
        Some(Mark::Unverified)
    );
    // A record whose part states no mark (a control, a pixel operation) gives none.
    assert_eq!(of("view"), None);
    assert_eq!(of("align"), None);
    assert!(stated("# t\n\n## Mark and history\n\n**Mark.** None: a control.\n").is_none());
    assert!(stated("# t\n\n## Mark and history\n\n**Mark.**\n").is_none());
    assert!(stated("# t\n\n## Limits\n\n**Unverified**\n").is_none());
}

/// Plan m3, task D2: every one of rascal's pixel operations, and every task, that needs no
/// model has its record, and the record shows rungs 1 and 2. Rascal's operations are
/// [`RASCAL`]; the tasks are the recipes; what needs a model is what the description says
/// needs a capability (the tasks that call `segment` or `fill`), whose rung 2 is task B1's.
/// The description is the reference's copy, which other tests hold to the binary's.
#[test]
fn each_pixel_operation_and_task_without_a_model_shows_its_record_complete() {
    let doc: serde_json::Value = serde_json::from_str(indented_description()).expect("valid JSON");
    let mut held = Vec::new();
    for action in doc["actions"].as_array().unwrap() {
        let name = action["name"].as_str().unwrap();
        let rascals = action["kind"] == "recipe" || RASCAL.contains(&name);
        if !rascals || !action["needs"].as_array().unwrap().is_empty() {
            continue;
        }
        let record = record_of(name).unwrap_or_else(|| panic!("{name} has no record"));
        let history = record
            .split("\n## Mark and history\n")
            .nth(1)
            .unwrap_or_else(|| panic!("the record of {name} has no history"));
        for rung in [
            "| 1. Single examples | Shown |",
            "| 2. Scored alone | Shown |",
        ] {
            let shown = history.lines().any(|line| line.starts_with(rung));
            assert!(shown, "the record of {name} does not show {rung}");
        }
        held.push(name);
    }
    held.sort_unstable();
    assert_eq!(
        held,
        [
            "apply",
            "close",
            "cut",
            "fan",
            "feather",
            "grow",
            "inspect",
            "invert",
            "line",
            "mask_rect",
            "measure",
            "paste",
            "plug",
            "profile",
            "protect",
            "put",
            "ramp",
            "rasterize",
            "rect",
            "render",
            "select",
            "shrink",
            "stroke",
            "subtract",
            "tighten",
            "unwindow",
            "window"
        ]
    );
}

#[test]
fn recipes_are_listed_before_primitives_with_purpose_and_use() {
    let (ok, text) = whizbang(&["describe", "--json"]);
    assert!(ok);
    let doc: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    let actions = doc["actions"].as_array().expect("a list");
    let first_primitive = actions
        .iter()
        .position(|action| action["kind"] == "primitive")
        .expect("a primitive");
    // Before the primitives: the recipes, and after them the one loop.
    assert!(
        actions[..first_primitive]
            .iter()
            .all(|a| a["kind"] == "recipe" || a["kind"] == "loop")
    );
    assert!(
        actions[first_primitive..]
            .iter()
            .all(|a| a["kind"] != "recipe")
    );
    // The shipped recipe says what it is for and when to use it, and takes its inputs,
    // its overrides and the step to stop after.
    let recipe = actions
        .iter()
        .find(|action| action["name"] == "remove_prompt")
        .expect("remove_prompt");
    assert_eq!(recipe["kind"], "recipe");
    assert!(
        recipe["purpose"]
            .as_str()
            .unwrap()
            .starts_with("Removes what a phrase names")
    );
    assert!(
        recipe["use"]
            .as_str()
            .unwrap()
            .starts_with("To remove a thing you can name")
    );
    let arguments: Vec<&str> = (recipe["arguments"].as_array().unwrap().iter())
        .map(|a| a["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        arguments,
        ["backend", "filler", "prompt", "set", "from", "until"]
    );
    // Its own inputs, and then, for a run from a step, one for each output of each step
    // before its last (plan m4, task K1, item 2; ADR 0033, "A run from a step").
    let inputs: Vec<&str> = (recipe["inputs"].as_array().unwrap().iter())
        .map(|a| a["name"].as_str().unwrap())
        .collect();
    assert_eq!(inputs[..2], ["image", "protect"]);
    assert!(inputs[2..].iter().all(|port| port.starts_with("from_")));
    assert!(inputs.contains(&"from_feather_mask"), "{inputs:?}");
    assert_eq!(recipe["plans"], true);
    assert_eq!(recipe["needs"][0]["capability"], "segment");
    assert_eq!(recipe["needs"][0]["chosen_by"], "backend");
    // Its last step fills, chosen by `filler`, and is needed only without `until`.
    assert_eq!(recipe["needs"][1]["capability"], "fill");
    assert_eq!(recipe["needs"][1]["chosen_by"], "filler");
    assert_eq!(
        recipe["needs"][1]["when"],
        serde_json::json!({ "not_given": "until" })
    );
    // A primitive says nothing of when to use it.
    assert!(actions[first_primitive].get("use").is_none());
    // The text the description prints lists the recipes first too.
    let (ok, text) = whizbang(&["describe"]);
    assert!(ok);
    let at = |name: &str| text.find(name).unwrap_or_else(|| panic!("{name}: {text}"));
    assert!(at("remove_prompt") < at("subtract"));
    assert!(
        text.contains("\n      use: To remove a thing you can name in words"),
        "{text}"
    );
}

#[test]
fn the_five_use_cases_are_listed_as_recipes_first() {
    // Squint's use cases are recipes of the build (plan m3, task Q1): each listed among
    // the recipes, before every primitive, with when to use it, its inputs, and the
    // verdict it answers with; and `details`, their first step, among the primitives.
    let (ok, json) = whizbang(&["describe", "--json"]);
    assert!(ok);
    let doc: serde_json::Value = serde_json::from_str(&json).unwrap();
    let actions = doc["actions"].as_array().unwrap();
    let at = |name: &str| actions.iter().position(|a| a["name"] == name).unwrap();
    let first_primitive = actions
        .iter()
        .position(|a| a["kind"] == "primitive")
        .unwrap();
    for name in whizbang_squint::use_case::NAMES {
        let action = &actions[at(name)];
        assert_eq!(action["kind"], "recipe", "{name}");
        assert!(at(name) < first_primitive, "{name}");
        assert!(
            action["use"].as_str().is_some_and(|u| !u.is_empty()),
            "{name}"
        );
        // Each hands its caller the verdict and the findings of its step `check` (plan
        // m4, task K2, item 4).
        let results = serde_json::json!(["verdict", "findings"]);
        assert_eq!(action["results"], results, "{name}");
        let inputs: Vec<&str> = (action["inputs"].as_array().unwrap().iter())
            .map(|port| port["name"].as_str().unwrap())
            .collect();
        assert_eq!(inputs, ["images", "from_details_images"], "{name}");
    }
    let details = &actions[at("details")];
    assert_eq!(details["kind"], "primitive");
    assert_eq!(details["results"], serde_json::json!(["measurements"]));
    for tool in ["align", "change_map", "edge_similarity"] {
        assert_eq!(
            actions[at(tool)]["results"],
            serde_json::json!(["measurements"]),
            "{tool}"
        );
    }
}

// Rascal's operations, as its `crates/rascal-ops/src/lib.rs` registered them at the
// commit `docs/sources.toml` pins (plan m3, task O5): each is an action here, under
// its own name.
const RASCAL: [&str; 25] = [
    "rect",
    "rasterize",
    "render",
    "cut",
    "put",
    "line",
    "stroke",
    "inspect",
    "fan",
    "profile",
    "feather",
    "ramp",
    "apply",
    "window",
    "unwindow",
    "paste",
    "measure",
    "grow",
    "shrink",
    "close",
    "plug",
    "subtract",
    "select",
    "invert",
    "tighten",
];

#[test]
fn each_of_rascals_25_operations_is_an_action() {
    let (ok, text) = whizbang(&["describe", "--json"]);
    assert!(ok);
    let doc: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    let names: Vec<&str> = (doc["actions"].as_array().expect("a list").iter())
        .map(|action| action["name"].as_str().expect("a name"))
        .collect();
    let missing: Vec<&str> = RASCAL
        .into_iter()
        .filter(|name| !names.contains(name))
        .collect();
    assert!(missing.is_empty(), "not actions: {missing:?}");
    let distinct: std::collections::BTreeSet<&str> = RASCAL.into_iter().collect();
    assert_eq!(distinct.len(), 25);
}

#[test]
fn the_loop_is_listed_after_the_recipes_and_before_the_primitives() {
    // Plan m4, task L1, item 10.
    let (ok, text) = whizbang(&["describe", "--json"]);
    assert!(ok);
    let doc: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    let actions = doc["actions"].as_array().expect("a list");
    let at = actions
        .iter()
        .position(|action| action["name"] == "remove_checked")
        .expect("the loop is listed");
    assert!(
        actions[..at]
            .iter()
            .all(|action| action["kind"] == "recipe")
    );
    assert!(
        actions[at + 1..]
            .iter()
            .all(|action| action["kind"] == "primitive")
    );
    let looped = &actions[at];
    assert_eq!(looped["kind"], "loop");
    // It is the one loop, it plans its calls, and it says what it hands back.
    assert_eq!(
        actions
            .iter()
            .filter(|action| action["kind"] == "loop")
            .count(),
        1
    );
    assert_eq!(looped["plans"], true);
    assert_eq!(
        looped["results"],
        serde_json::json!(["verdict", "measurements", "findings"])
    );
    let arguments: Vec<&str> = (looped["arguments"].as_array().unwrap().iter())
        .map(|argument| argument["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        arguments,
        ["prompt", "backend", "filler", "judge", "attempts", "check"]
    );
    let needs: Vec<&str> = (looped["needs"].as_array().unwrap().iter())
        .map(|need| need["capability"].as_str().unwrap())
        .collect();
    assert_eq!(needs, ["segment", "fill", "judge"]);
    // And the words say when to reach for it.
    assert!(text.contains("To remove a thing you can name in words and know whether"));
}

/// Plan m5, task W1, item 2 (ADR 0040): every recipe and loop of the build names its kit,
/// one the description lists, and a title held to the rule of a title; the loop names the
/// kit it uses. A tool names none.
#[test]
fn each_recipe_and_loop_names_its_kit_and_a_title_held_to_the_rule() {
    // The binary's own description, in process.
    let registry = whizbang::wiring::registry().unwrap();
    let doc = serde_json::to_value(whizbang::wiring::description(&registry)).unwrap();
    let kits: Vec<&str> = (doc["kits"].as_array().unwrap().iter())
        .filter_map(|kit| kit["name"].as_str())
        .collect();
    assert_eq!(kits, ["rascal", "squint"]);
    let mut titles = std::collections::BTreeSet::new();
    for action in doc["actions"].as_array().unwrap() {
        let name = action["name"].as_str().unwrap();
        if action["kind"] == "primitive" {
            assert!(action.get("kit").is_none(), "{name}");
            continue;
        }
        let kit = action["kit"]
            .as_str()
            .unwrap_or_else(|| panic!("{name} names no kit"));
        assert!(kits.contains(&kit), "{name}: {kit}");
        let title = action["title"]
            .as_str()
            .unwrap_or_else(|| panic!("{name} has no title"));
        whizbang_core::contract::text::title(title).unwrap();
        assert!(titles.insert(title.to_owned()), "{title} twice");
    }
    let looped = (doc["actions"].as_array().unwrap().iter())
        .find(|action| action["kind"] == "loop")
        .unwrap();
    assert_eq!(looped["kit"], "rascal");
    assert_eq!(looped["title"], "Remove, checked");
    assert_eq!(looped["uses"], "squint");
    // A title that breaks the rule is refused where a descriptor is made.
    for bad in [
        "<b>Remove</b>",
        "Remove what you name in a very long sentence of words",
        " Lead",
    ] {
        assert!(whizbang_core::contract::text::title(bad).is_err(), "{bad}");
    }
}

/// The description is static: the same bytes, kits and titles among them, from one run
/// to the next and as the reference holds them.
#[test]
fn the_description_is_static_with_kits_and_titles() {
    // In process, twice, over two registries the build makes; the binary's own bytes
    // are held to the reference by `describe_json_pretty_prints_the_bytes_the_reference_holds`.
    let made = || {
        let registry = whizbang::wiring::registry().unwrap();
        serde_json::to_string_pretty(&whizbang::wiring::description(&registry)).unwrap() + "\n"
    };
    let plain = made();
    assert_eq!(plain, made());
    assert_eq!(plain, indented_description());
    let doc: serde_json::Value = serde_json::from_str(&plain).unwrap();
    let titled = (doc["actions"].as_array().unwrap().iter())
        .find(|action| action["name"] == "source-fidelity")
        .unwrap();
    assert_eq!(titled["kit"], "squint");
    assert_eq!(titled["title"], "Check an edit");
}
