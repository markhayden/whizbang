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

/// What `describe --json` printed before `--pretty` existed, byte for byte: two-space
/// indentation, one field per line, one newline at the end.
fn indented_description() -> String {
    format!(
        "{{\n  \"description_version\": 1,\n  \"name\": \"whizbang\",\n  \"version\": \"{}\",\n  \"actions\": []\n}}\n",
        env!("CARGO_PKG_VERSION")
    )
}

#[test]
fn describe_json_without_pretty_prints_the_same_bytes_as_before() {
    let (ok, text) = whizbang(&["describe", "--json"]);
    assert!(ok);
    assert_eq!(text, indented_description());
}

#[test]
fn describe_json_pretty_is_the_indented_form_and_equals_plain_json() {
    let (ok, pretty) = whizbang(&["describe", "--json", "--pretty"]);
    assert!(ok);
    assert_eq!(pretty, indented_description());
    let (_, plain) = whizbang(&["describe", "--json"]);
    assert_eq!(pretty, plain);
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
