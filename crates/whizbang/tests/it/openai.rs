//! `openai::…`: the OpenAI judge through the binary's wiring (plan m2c, task P1), as
//! `anthropic::…` does for Anthropic's: a config that names the kind `service` with the
//! provider `openai`, its key in the env file, and a server on loopback with a
//! certificate of a root made in the test. The shipped kind's address is never asked.

use whizbang_backends::openai::KEY_HEADER;

use crate::anthropic::{OPENAI, asked_once};

#[test]
fn the_key_goes_only_to_the_approved_origin_as_a_bearer() {
    let (judged, head, body) = asked_once(OPENAI, "openai-key");
    // In its one header, `Authorization`, as `Bearer` and the key.
    let line = format!("{KEY_HEADER}: Bearer {}\r\n", judged.key);
    assert!(head.contains(&line), "{head}");
    assert!(
        head.starts_with("POST /v1/responses HTTP/1.1\r\n"),
        "{head}"
    );
    assert!(!head.to_ascii_lowercase().contains("x-api-key"), "{head}");
    // Nothing the provider would keep: `store` is false.
    let sent: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(sent["store"], false);
}

#[test]
fn every_tool_of_the_build_is_sent_and_its_arguments_read_by_its_schema() {
    use crate::google::{every_tool_sent, schemas};
    let (printed, first, second) = every_tool_sent(OPENAI, "openai-every-tool");
    // Each tool is sent once, its schema as the build wrote it, not strict.
    let sent = first["tools"].as_array().unwrap();
    let wanted = schemas();
    assert_eq!(sent.len(), wanted.len(), "{sent:?}");
    for (name, schema) in &wanted {
        let found = sent.iter().find(|t| t["name"] == name.as_str()).unwrap();
        assert_eq!(&found["parameters"], schema, "{name}");
        assert_eq!(found["strict"], false, "{name}");
    }
    // Each call was read as its schema says: answered with what the tool gave, never
    // refused or left uncalled, and the submission decided the verdict.
    let second: serde_json::Value = serde_json::from_str(&second).unwrap();
    let outputs: Vec<&serde_json::Value> = (second["input"].as_array().unwrap().iter())
        .filter(|item| item["type"] == "function_call_output")
        .collect();
    assert_eq!(outputs.len(), 5, "{second}");
    for output in outputs {
        let text = output.to_string();
        assert!(
            !text.contains("not called") && !text.contains("refused"),
            "{text}"
        );
    }
    assert!(printed.contains("verdict: pass"), "{printed}");
}

#[test]
fn a_v2_keyed_submission_is_sent_as_written_not_strict_and_read() {
    use crate::google::{v2_schema, v2_sent};
    let (printed, first) = v2_sent(OPENAI, "openai-v2");
    let sent = first["tools"].as_array().unwrap();
    let submit = sent.iter().find(|t| t["name"] == "submit_verdict").unwrap();
    assert_eq!(submit["parameters"], v2_schema());
    assert_eq!(submit["strict"], false);
    assert!(printed.contains("verdict: pass"), "{printed}");
}
