use std::process::Command;
use std::sync::Arc;

use whizbang_testkit::echo::Echo;
use whizbang_testkit::scratch::Scratch;

fn whizbang(args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_whizbang"))
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
        "{{\"description_version\":4,\"name\":\"whizbang\",\"version\":\"{}\",\"actions\":[{{\"name\":\"check\",\"version\":\"1.0.0\",\"kind\":\"recipe\",\"purpose\":\"",
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
    let out = Command::new(env!("CARGO_BIN_EXE_whizbang"))
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
    assert_eq!(doc["description_version"], 4);
    assert_eq!(doc["version"], env!("CARGO_PKG_VERSION"));
    let names: Vec<&str> = doc["actions"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|action| action["name"].as_str().expect("a name"))
        .collect();
    // A recipe first, then the primitives, in the order the registry lists them.
    assert_eq!(names, ["check", "grow", "view"]);
    let kinds: Vec<&str> = doc["actions"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|action| action["kind"].as_str().expect("a kind"))
        .collect();
    assert_eq!(kinds, ["recipe", "primitive", "primitive"]);
    // `check` says what it needs: a judge. Whether one is there is no part of a
    // description, which is the same on every machine.
    assert_eq!(doc["actions"][0]["needs"][0]["capability"], "judge");
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
    let lines: Vec<&str> = text.lines().skip(1).collect();
    let names: Vec<&str> = lines
        .iter()
        .map(|line| line.split_whitespace().next().expect("a name"))
        .collect();
    assert_eq!(names, ["check", "grow", "view"], "{text}");
}

#[test]
fn version_flag_prints_the_version() {
    let (ok, text) = whizbang(&["--version"]);
    assert!(ok);
    assert_eq!(
        text.trim(),
        format!("whizbang {}", env!("CARGO_PKG_VERSION"))
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
    assert_eq!(
        text,
        format!(
            "{shipped}  echo  Returns the text it was given; a fake for tests, which never ships.\n"
        )
    );
}

/// The indented form, pinned here and not only against the reference: its first lines, and
/// the artifact types whole.
#[test]
fn describe_json_pretty_starts_and_ends_with_these_bytes() {
    let (_, pretty) = whizbang(&["describe", "--json", "--pretty"]);
    let start = format!(
        "{{\n  \"description_version\": 4,\n  \"name\": \"whizbang\",\n  \"version\": \"{}\",\n  \"actions\": [\n    {{\n      \"name\": \"check\",\n",
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
        let out = Command::new("bash")
            .args(["-c", &script])
            .env("WHIZBANG", env!("CARGO_BIN_EXE_whizbang"))
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
