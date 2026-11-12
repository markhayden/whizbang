use std::process::Command;
use std::sync::Arc;

use whizbang_testkit::echo::Echo;

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
/// order, with no space between them, and one newline at the end.
fn compact_start() -> String {
    format!(
        "{{\"description_version\":2,\"name\":\"whizbang\",\"version\":\"{}\",\"actions\":[],\"artifact_types\":[{{\"name\":\"mask\",\"purpose\":\"",
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
fn describe_json_is_valid_and_lists_no_actions_yet() {
    let (ok, text) = whizbang(&["describe", "--json"]);
    assert!(ok);
    let doc: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    assert_eq!(doc["name"], "whizbang");
    assert_eq!(doc["description_version"], 2);
    assert_eq!(doc["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(doc["actions"], serde_json::json!([]));
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
    assert!(text.contains("no actions yet"));
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
    assert_eq!(description.actions.len(), shipped + 1);
    assert_eq!(description.actions[shipped].name(), "echo");
    assert_eq!(description.version, env!("CARGO_PKG_VERSION"));
    // And the text a person reads lists it.
    let text = whizbang::wiring::describe_text(&description);
    assert!(text.contains("echo"), "{text}");
    assert!(!text.contains("no actions yet"), "{text}");
}
