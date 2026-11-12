use std::process::Command;

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

/// The indented form, byte for byte: what `describe --json` printed before `--pretty`
/// existed, and what the reference holds. Two-space indentation, one field per line, one
/// newline at the end.
fn indented_description() -> String {
    format!(
        "{{\n  \"description_version\": 1,\n  \"name\": \"whizbang\",\n  \"version\": \"{}\",\n  \"actions\": []\n}}\n",
        env!("CARGO_PKG_VERSION")
    )
}

/// The one-line form, byte for byte: the same fields in the same order, with no space
/// between them, and one newline at the end.
fn compact_description() -> String {
    format!(
        "{{\"description_version\":1,\"name\":\"whizbang\",\"version\":\"{}\",\"actions\":[]}}\n",
        env!("CARGO_PKG_VERSION")
    )
}

#[test]
fn describe_json_is_one_line_with_one_newline_at_the_end() {
    let (ok, text) = whizbang(&["describe", "--json"]);
    assert!(ok);
    assert_eq!(text.matches('\n').count(), 1, "{text}");
    assert!(text.ends_with("}\n"), "{text}");
    // Not "holds no colon and space": a purpose may hold one.
    assert_eq!(text, compact_description());
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
fn describe_json_pretty_prints_the_bytes_json_printed_before() {
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
    assert_eq!(doc["description_version"], 1);
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
