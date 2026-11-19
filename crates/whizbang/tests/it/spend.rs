//! `whizbang spend status` and `reset`, through the binary (plan 1B, task S9): the
//! command line, the exit codes, and one round of a reset and a status over a store
//! on disk. What the commands say of each state of the books is held where they are
//! called in process (`spending.rs`, among the binary's own tests).

use std::process::Output;
use std::sync::Arc;

use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::scratch::Scratch;

fn whizbang(args: &[&str]) -> Output {
    whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang"))
        .args(args)
        .output()
        .expect("the binary runs")
}

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).expect("output is UTF-8")
}

#[test]
fn the_help_lists_spend_with_status_and_reset_and_says_what_a_reset_does() {
    let out = whizbang(&["--help"]);
    assert_eq!(out.status.code(), Some(0));
    let help = text(&out.stdout);
    let line = help
        .lines()
        .find(|line| line.trim_start().starts_with("spend "))
        .unwrap_or_else(|| panic!("no line for spend in:\n{help}"));
    assert!(line.contains("allowance"), "{line}");

    let out = whizbang(&["spend", "--help"]);
    assert_eq!(out.status.code(), Some(0));
    let help = text(&out.stdout);
    assert!(help.contains("status") && help.contains("reset"), "{help}");
    assert!(!help.contains("Not built"), "{help}");

    let out = whizbang(&["spend", "reset", "--help"]);
    assert_eq!(out.status.code(), Some(0));
    let help = text(&out.stdout);
    assert!(help.contains("--allowance <USD>"), "{help}");
    assert!(help.contains("--store <DIR>"), "{help}");
    assert!(help.contains("close the books"), "{help}");
    assert!(help.contains("is not stopped"), "{help}");

    let out = whizbang(&["spend", "status", "--help"]);
    let help = text(&out.stdout);
    assert!(help.contains("--run <RUN>"), "{help}");
    assert!(help.contains("Reads only"), "{help}");
    // A reservation shown as open may be a dead process's, and a reset clears it.
    let flat = help.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains(
            "shown as \"open\" belongs to a run that is still executing, or to a process \
             that has died"
        ),
        "{help}"
    );
    assert!(flat.contains("`spend reset` clears it"), "{help}");
}

#[test]
fn spend_without_a_command_or_a_store_is_the_parsers_usage_error() {
    for args in [
        &["spend"][..],
        &["spend", "status"],
        &["spend", "reset", "--allowance", "1"],
        &["spend", "reset", "--store", "somewhere"],
        &["spend", "adjust"],
    ] {
        let out = whizbang(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert_eq!(text(&out.stdout), "", "{args:?}");
    }
}

#[test]
fn a_reset_and_a_status_through_the_binary_and_a_complaint_that_is_escaped() {
    let dir = Scratch::new("spend-binary");
    let root = dir.path().join("store");
    let store = root.to_str().unwrap();
    // No store there: a complaint, exit 1, and nothing is made.
    for args in [
        &["spend", "status", "--store", store][..],
        &["spend", "reset", "--allowance", "5", "--store", store],
    ] {
        let out = whizbang(args);
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        assert_eq!(text(&out.stdout), "");
        let said = text(&out.stderr);
        assert!(
            said.starts_with("whizbang: there is no store at "),
            "{said}"
        );
    }
    assert!(!root.exists());
    drop(Store::open(&root, Arc::new(RealFs)).expect("a store"));

    let out = whizbang(&["spend", "reset", "--allowance", "5", "--store", store]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(text(&out.stdout).starts_with("allowance reset: $5.00\nepoch: 1\n"));
    let out = whizbang(&["spend", "status", "--store", store]);
    assert_eq!(out.status.code(), Some(0));
    let said = text(&out.stdout);
    assert!(
        said.starts_with("epoch: 1\nallowance: $5.00\ncounted against it: $0.00\n"),
        "{said}"
    );
    assert!(said.contains("left: $5.00\n"), "{said}");
    let out = whizbang(&["spend", "status", "--store", store, "--run", "7000-1"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(text(&out.stdout).starts_with("run: 7000-1\n"));

    // An allowance that is none is a complaint, and what was typed is escaped.
    let out = whizbang(&[
        "spend",
        "reset",
        "--allowance",
        "lots\u{1b}[31m",
        "--store",
        store,
    ]);
    assert_eq!(out.status.code(), Some(1));
    let said = text(&out.stderr);
    assert!(
        said.starts_with("whizbang: lots\\u{1b}[31m is no allowance: "),
        "{said}"
    );
    assert!(!said.contains('\u{1b}'), "{said}");
}

// --- the judge of a service (plan 2A, task S2, item 1) ----------------------------------------

mod judge {
    use serde_json::Value as Json;
    use whizbang_core::contract::{Call, CallError, Executor, Value};

    use crate::anthropic::{
        GOOGLE, Judged, OPENAI, Provider, import_picture, judges_call, record_of, said_by, serving,
        turn_with,
    };

    /// The bound of one request at the default ceilings: 40,000 input tokens at the
    /// dearest input price of the row, 0.125 dollars a million, and 4,096 output tokens
    /// at 0.50 (ADR 0026).
    const BOUND: u64 = 7_048;

    fn paid(record: &Json) -> Vec<(u64, u64, String)> {
        record["spend"]["requests"]
            .as_array()
            .unwrap()
            .iter()
            .map(|paid| {
                (
                    paid["reserved_micros"].as_u64().unwrap(),
                    paid["settled_micros"].as_u64().unwrap(),
                    paid["stage"].as_str().unwrap().to_owned(),
                )
            })
            .collect()
    }

    fn one_turn(label: &str, answer: Vec<u8>) -> (Json, Judged) {
        one_turn_of(crate::anthropic::ANTHROPIC, label, answer)
    }

    fn one_turn_of(provider: Provider, label: &str, answer: Vec<u8>) -> (Json, Judged) {
        let judged = Judged::of(label, provider);
        let (listener, _) = serving(&judged.pki, vec![answer]);
        let origin = format!("https://127.0.0.1:{}", listener.port());
        let over = judged.over(&judged.address(listener.port()), Some(&origin));
        let executor = judged.runner(&over);
        let ran = executor.submit(&judges_call()).unwrap();
        assert_eq!(ran.outcome().name(), "completed");
        drop(executor);
        assert_eq!(listener.finish(), 1);
        (record_of(&judged.world.root, ran.run().as_str()), judged)
    }

    #[test]
    fn a_judge_turn_reserves_its_fixed_bound_and_settles_at_its_usage() {
        // 30,000 input tokens at 0.10 and 1,000 output at 0.50: 3,500 millionths.
        let (record, _) = one_turn(
            "spend-usage",
            turn_with("Seen.", "end_turn", Some((30_000, 1_000))),
        );
        assert_eq!(record["spend"]["reserved_micros"], BOUND);
        assert_eq!(paid(&record), [(BOUND, 3_500, "answered".to_owned())]);
    }

    #[test]
    fn a_google_and_an_openai_turn_reserve_their_bound_and_settle_at_their_usage() {
        // Each reserves the bound of its default ceilings at its model's row: 40,000
        // input tokens and 4,096 output, at 0.75 and 3.75 for gemini-3.8-flash (30,000
        // and 15,360) and at 1.00 and 5.00 for gpt-6.1-terra (40,000 and 20,480). Each
        // settles at its usage: 30,000 input of which 10,000 read from the cache, and
        // 1,000 output (thoughts with it, for Google).
        let usage = (30_000, 10_000, 1_000);
        // 20,000 at 0.75, 10,000 at 0.075 and 1,000 at 3.75.
        let (record, _) = one_turn_of(GOOGLE, "spend-google", said_by(GOOGLE, "Seen.", usage));
        assert_eq!(record["spend"]["reserved_micros"], 45_360);
        assert_eq!(paid(&record), [(45_360, 19_500, "answered".to_owned())]);
        // 20,000 at 1.00, 10,000 at 0.05 and 1,000 at 5.00.
        let (record, _) = one_turn_of(OPENAI, "spend-openai", said_by(OPENAI, "Seen.", usage));
        assert_eq!(record["spend"]["reserved_micros"], 60_480);
        assert_eq!(paid(&record), [(60_480, 25_500, "answered".to_owned())]);
    }

    #[test]
    fn a_turn_whose_reported_input_passes_the_long_prompt_line_is_charged_the_long_prompt_prices() {
        // 100,001 input tokens, one over the line, at the long prompt's 0.50, and
        // 1,000 output at its 2.50: 50,000.5 and 2,500, rounded up. At the row's own
        // prices the same usage is 10,000.1 and 500: under the price of a long prompt.
        let (record, _) = one_turn(
            "spend-long",
            turn_with("Seen.", "end_turn", Some((100_001, 1_000))),
        );
        assert_eq!(paid(&record), [(BOUND, 52_501, "answered".to_owned())]);
        // On the line itself, the row's prices: 10,000 and 500.
        let (record, _) = one_turn(
            "spend-line",
            turn_with("Seen.", "end_turn", Some((100_000, 1_000))),
        );
        assert_eq!(paid(&record), [(BOUND, 10_500, "answered".to_owned())]);
        // The input read from the cache counts toward the line: 50,000 not read from it
        // and 60,000 read, at 0.50 and 0.05, and 1,000 output at 2.50.
        let mut answer =
            String::from_utf8(turn_with("Seen.", "end_turn", Some((50_000, 1_000)))).unwrap();
        let usage = "\"output_tokens\":1000";
        let cached = "\"output_tokens\":1000,\"cache_read_input_tokens\":60000";
        let (head, body) = answer.split_once("\r\n\r\n").unwrap();
        let body = body.replace(usage, cached);
        assert!(body.contains("cache_read_input_tokens"), "{body}");
        let head = head.replace(
            &format!(
                "Content-Length: {}",
                body.len() - (cached.len() - usage.len())
            ),
            &format!("Content-Length: {}", body.len()),
        );
        answer = format!("{head}\r\n\r\n{body}");
        let (record, _) = one_turn("spend-cached", answer.into_bytes());
        assert_eq!(paid(&record), [(BOUND, 30_500, "answered".to_owned())]);
    }

    #[test]
    fn a_judge_turn_with_no_usage_is_charged_its_bound() {
        let (record, _) = one_turn("spend-no-usage", turn_with("Seen.", "end_turn", None));
        assert_eq!(paid(&record), [(BOUND, BOUND, "answered".to_owned())]);
    }

    #[test]
    fn a_turn_whose_usage_no_charge_can_hold_is_charged_its_bound() {
        // The review of plan 1D, task F1 (#181): a usage so large that its charge is no
        // amount (`Charge::of_micros` refuses it) is not settled at nothing, nor at the
        // count: the turn says no charge, and the meter charges it its bound.
        let (record, _) = one_turn(
            "spend-huge",
            turn_with("Seen.", "end_turn", Some((u64::MAX, u64::MAX))),
        );
        assert_eq!(paid(&record), [(BOUND, BOUND, "answered".to_owned())]);
    }

    #[test]
    fn a_judge_call_whose_bound_is_over_the_call_cap_is_refused_before_dispatch() {
        let judged = Judged::new("spend-cap");
        let listener = whizbang_testkit::loopback::Listener::serve(1, |_, _| {});
        let origin = format!("https://127.0.0.1:{}", listener.port());
        // A cap of half a cent, under the bound of one request.
        let over = judged.over_with(
            &format!("{origin}/v1/messages"),
            Some(&origin),
            &["prompts"],
            "[spend]\ncall_cap_usd = 0.005\n",
        );
        let executor = judged.runner(&over);
        let refused = executor.submit(&judges_call());
        assert!(matches!(refused, Err(CallError::Refused(_))), "{refused:?}");
        drop(executor);
        assert_eq!(listener.finish(), 0);
    }

    #[test]
    fn a_retry_with_a_larger_limit_is_admitted_as_its_own_attempt() {
        // `check` asks with 1,024 output tokens, is cut off, and asks again with four
        // times that, 4,096, which the ceiling allows: two requests, each reserved
        // its own bound and settled at its own usage.
        let judged = Judged::new("spend-retry");
        let answers = vec![
            turn_with("Looking", "max_tokens", Some((10_000, 1_024))),
            turn_with("pass\nThe two agree.", "end_turn", Some((10_000, 200))),
        ];
        let (listener, log) = serving(&judged.pki, answers);
        let origin = format!("https://127.0.0.1:{}", listener.port());
        let over = judged.over_with(
            &format!("{origin}/v1/messages"),
            Some(&origin),
            &["prompts", "source_images"],
            "",
        );
        let scratch = judged.world.beside("pictures");
        std::fs::create_dir_all(&scratch).unwrap();
        let picture = import_picture(&judged.world.root, &scratch, "one", 10);
        let executor = judged.runner(&over);
        let call = Call::new("check")
            .input(
                "images",
                picture
                    .parse::<whizbang_core::identity::Digest>()
                    .map(whizbang_core::artifact::ArtifactId)
                    .unwrap(),
            )
            .argument("tools", Value::text(""));
        let ran = executor.submit(&call).unwrap();
        assert_eq!(ran.outcome().name(), "completed", "{:?}", ran.outcome());
        drop(executor);
        assert_eq!(listener.finish(), 2);
        let sent: Vec<Json> = log
            .lock()
            .unwrap()
            .iter()
            .map(|(_, body)| serde_json::from_slice(body).unwrap())
            .collect();
        assert_eq!(
            (
                sent[0]["max_tokens"].as_u64(),
                sent[1]["max_tokens"].as_u64()
            ),
            (Some(1_024), Some(4_096))
        );
        let record = record_of(&judged.world.root, ran.run().as_str());
        // The call reserved the bound of each of check's sixteen turns.
        assert_eq!(record["spend"]["reserved_micros"], BOUND * 16);
        // 1,000 + 512 and 1,000 + 100.
        assert_eq!(
            paid(&record),
            [
                (BOUND, 1_512, "answered".to_owned()),
                (BOUND, 1_100, "answered".to_owned())
            ]
        );
    }

    #[test]
    fn the_example_configs_whole_call_reservation_fits_its_cap_for_each_judge() {
        // The examples of the guide, as the owner writes them, one for each provider:
        // the bound of one request of its judge, times the sixteen turns `check`
        // reserves, is within its cap.
        let guide = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/guides/config.md"
        ))
        .unwrap();
        let section = guide
            .split("## Checking an edit with a judge")
            .nth(1)
            .and_then(|rest| rest.split("\n## ").next())
            .unwrap();
        let examples: Vec<&str> = section
            .split("```toml\n")
            .skip(1)
            .map(|rest| rest.split("```").next().unwrap())
            .collect();
        let expected = [
            ("claude-haiku-5-5", "ANTHROPIC_API_KEY", BOUND),
            ("gemini-3.8-flash", "GEMINI_API_KEY", 45_360),
            ("gpt-6.1-terra", "OPENAI_API_KEY", 60_480),
        ];
        assert_eq!(examples.len(), expected.len());
        for (toml, (model, variable, bound)) in examples.into_iter().zip(expected) {
            assert!(
                toml.contains(&format!("model = \"{model}\""))
                    && toml.contains("call_cap_usd = 2.00"),
                "{toml}"
            );
            let judged = Judged::new(&format!("spend-example-{model}"));
            std::fs::write(judged.world.config_path(), toml).unwrap();
            let key = ["made", "up", "example", "key", "0123456789"].join("-");
            std::fs::write(judged.world.env_path(), format!("{variable}={key}\n")).unwrap();
            let loaded = whizbang_core::config::Loaded::of_files(
                &judged.world.config_path(),
                Some(&judged.world.env_path()),
            )
            .unwrap();
            let worker = whizbang::pack::OneWorker::installed();
            let offers = whizbang_core::config::Offers::of(
                loaded.config(),
                loaded.credentials(),
                &whizbang::wiring::kinds(&worker),
            )
            .unwrap();
            let resolved = offers
                .resolve(whizbang_core::contract::Capability::Judge, None, &[])
                .unwrap();
            let quoted = resolved.declaration().quote().unwrap().bound_micros();
            assert_eq!(quoted, Some(bound), "{model}");
            let turns = whizbang_squint::check::TURNS_LIMIT as u64;
            let cap = loaded.config().call_cap_micros().unwrap();
            assert_eq!(cap, 2_000_000);
            assert!(
                bound * turns <= cap,
                "{model}: {} over {cap}",
                bound * turns
            );
        }
    }
}
