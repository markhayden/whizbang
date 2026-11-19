//! The model tier's committed evidence (plan 1C, task R2b; decision D15; taken again by
//! plan 2A, task R1), held in the fast tier: the record `scripts/model-evidence.sh` wrote
//! is of the tree's adapter, lock and fixture, on a clean head, with each first load
//! after a full check's record; and the fixture picture, which is public, holds no
//! metadata.

use std::path::{Path, PathBuf};

use whizbang::wiring;
use whizbang_core::identity::{DecodeLimits, Digest, identify};

use crate::model_tier::FIXTURE;

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn digest_of(path: &str) -> String {
    let bytes = std::fs::read(repository().join(path)).unwrap();
    Digest::of_bytes(&bytes).to_string()
}

/// Fields of the numbers file, `name` and the value without its quotes, in order.
type Fields = Vec<(String, String)>;

/// The fields of the numbers file before its first table, as `name = value`, each value
/// without its quotes; and the tables `[[round]]`, each the same.
fn record() -> (Fields, Vec<Fields>) {
    let text = std::fs::read_to_string(repository().join("docs/results/m1c-local-pack.toml"))
        .expect("the record of the model tier is committed");
    let mut top = Vec::new();
    let mut rounds: Vec<Fields> = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if line == "[[round]]" {
            rounds.push(Vec::new());
            continue;
        }
        let (name, value) = line.split_once(" = ").expect("a line is name = value");
        let field = (name.to_owned(), value.trim_matches('"').to_owned());
        match rounds.last_mut() {
            Some(round) => round.push(field),
            None => top.push(field),
        }
    }
    (top, rounds)
}

fn field<'a>(fields: &'a [(String, String)], name: &str) -> &'a str {
    fields
        .iter()
        .find(|(given, _)| given == name)
        .map(|(_, value)| value.as_str())
        .unwrap_or_else(|| panic!("the record has no {name}"))
}

#[test]
fn the_committed_record_names_the_adapters_revision_and_the_lock_the_tree_has() {
    let (top, rounds) = record();
    // Of this tree: the lock, the adapter's source and the fixture, by their digests; the
    // adapter's revision in the binary's table. A change to any of them makes the record
    // stale, and `scripts/model-evidence.sh` is run again on the Mac.
    assert_eq!(field(&top, "lock"), digest_of("pack/uv.lock"));
    assert_eq!(
        field(&top, "adapter_source"),
        digest_of("pack/src/whizbang_pack/sam3.py")
    );
    assert_eq!(
        field(&top, "fixture"),
        digest_of("docs/results/m1c-local-pack/circle.png")
    );
    let table = wiring::adapters();
    let sam3 = table
        .iter()
        .find(|adapter| adapter.name.as_str() == "sam3")
        .unwrap();
    assert!(sam3.shipped);
    assert_eq!(field(&top, "adapter"), "sam3");
    assert_eq!(field(&top, "adapter_revision"), sam3.revision.as_str());
    // And of Tiny's adapter (plan 1D, task T1), by its source and its revision.
    assert_eq!(field(&top, "tiny_adapter"), "owlv2-sam2");
    assert_eq!(
        field(&top, "tiny_adapter_source"),
        digest_of("pack/src/whizbang_pack/owlv2_sam2.py")
    );
    let tiny = table
        .iter()
        .find(|adapter| adapter.name.as_str() == "owlv2-sam2")
        .unwrap();
    assert!(tiny.shipped);
    assert_eq!(field(&top, "tiny_adapter_revision"), tiny.revision.as_str());
    // What it shows: the GPU, with the fallback off, nothing contacted, in three runs
    // each with an instance and one load for two requests.
    assert_eq!(field(&top, "device"), "mps");
    assert_eq!(field(&top, "fallback"), "off");
    assert_eq!(field(&top, "hosts_contacted"), "none");
    // Taken on a clean head (plan 2A, task R1): the commit it names is the tree that ran.
    assert_eq!(field(&top, "tree"), "clean");
    assert_eq!(rounds.len(), 3);
    for round in &rounds {
        assert_eq!(field(round, "phrase_device"), "mps");
        assert_ne!(field(round, "phrase_instances"), "0");
        assert_eq!(field(round, "loads"), "1");
        assert_eq!(field(round, "still_running_after"), "0");
        // A point and a box, on the GPU, each with an instance (plan 1D, task Q1).
        assert_eq!(field(round, "places_device"), "mps");
        assert_ne!(field(round, "point_instances"), "0");
        assert_ne!(field(round, "box_instances"), "0");
        // Tiny's phrase, point and box, on the GPU, each with an instance (task T1).
        assert_eq!(field(round, "tiny_device"), "mps");
        for answered in [
            "tiny_phrase_instances",
            "tiny_point_instances",
            "tiny_box_instances",
        ] {
            assert_ne!(field(round, answered), "0", "{answered}");
        }
        // Each first load trusted the record of an earlier full check, and the model
        // was loaded within 10 seconds of the first request (plan 2A, task R1).
        assert_eq!(field(round, "check_record_before"), "yes");
        let first: f64 = field(round, "first_load_seconds").parse().unwrap();
        assert!(first < 10.0, "{first}");
    }
    // The page beside it says the same lock and the same adapter.
    let page =
        std::fs::read_to_string(repository().join("docs/results/m1c-local-pack.md")).unwrap();
    for value in [
        field(&top, "lock"),
        field(&top, "adapter_source"),
        field(&top, "installation"),
        field(&top, "tiny_adapter_source"),
        field(&top, "tiny_installation"),
    ] {
        assert!(page.contains(value), "the page does not name {value}");
    }
    assert!(page.contains(&format!("revision `{}`", sam3.revision.as_str())));
}

#[test]
fn the_fixture_picture_holds_no_metadata() {
    // A PNG of the signature and chunks: the header, the pixels and the end, and nothing
    // else: no EXIF (`eXIf`), no colour profile (`iCCP`, `sRGB`, `gAMA`, `cHRM`), no text
    // (`tEXt`, `zTXt`, `iTXt`), no time and no physical size.
    let bytes = FIXTURE;
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    let mut kinds = Vec::new();
    let mut at = 8;
    while at < bytes.len() {
        let length = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        kinds.push(String::from_utf8(bytes[at + 4..at + 8].to_vec()).unwrap());
        at += 12 + length;
    }
    assert_eq!(at, bytes.len());
    kinds.dedup();
    assert_eq!(kinds, ["IHDR", "IDAT", "IEND"]);
    // And it decodes as the test uses it: 640 by 480, no alpha, no profile.
    let found = identify(bytes, &DecodeLimits::default()).unwrap();
    assert_eq!((found.stored.width, found.stored.height), (640, 480));
    assert!(!found.stored.alpha);
    assert_eq!(found.stored.orientation, 1);
}

/// The fields of the first verdict's record, `name = "value"` each, in order.
fn first_verdict() -> (String, Vec<(String, String)>) {
    let page = std::fs::read_to_string(repository().join("docs/results/m2a-first-verdict.md"))
        .expect("the record of the first verdict is committed");
    let block = page
        .split("```toml\n")
        .nth(1)
        .and_then(|rest| rest.split("```").next())
        .expect("the page holds its fields");
    let fields = block
        .lines()
        .map(|line| {
            let (name, value) = line.split_once(" = ").expect("a line is name = value");
            (name.to_owned(), value.trim_matches('"').to_owned())
        })
        .collect();
    (page, fields)
}

#[test]
fn the_first_verdict_record_names_a_commit_of_this_tree_and_holds_no_secret() {
    use whizbang_backends::anthropic;
    let (page, fields) = first_verdict();
    // A commit, whole; and what ran is this tree's: the build's version, the action's,
    // the adapter's, and the date of the row of prices.
    let commit = field(&fields, "commit");
    assert!(
        commit.len() == 40
            && commit
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
    assert_eq!(field(&fields, "whizbang"), env!("CARGO_PKG_VERSION"));
    // The action as it was then: plan 2C, task A1, took `check` to 1.2.0 after it, and
    // its own record is `docs/results/m2c-use-cases.md`.
    assert_eq!(field(&fields, "action"), "check 1.1.0");
    assert_eq!(field(&fields, "backend"), anthropic::ADAPTER);
    assert_eq!(field(&fields, "adapter_version"), anthropic::VERSION);
    let prices = anthropic::spend::prices_of(field(&fields, "model")).expect("a priced model");
    assert_eq!(field(&fields, "prices_read"), prices.read);
    // The spend is the arithmetic of ADR 0026: the bound of a request at the default
    // ceilings, sixteen of them reserved, and the turn settled at its usage.
    let number = |name: &str| field(&fields, name).parse::<u64>().unwrap();
    assert_eq!(number("request_bound_micros"), 6_024);
    assert_eq!(number("reserved_micros"), 6_024 * 16);
    let usage =
        whizbang_core::capability::Usage::new(number("input_tokens"), number("output_tokens"))
            .cached(number("cache_read_tokens"), number("cache_write_tokens"));
    let charged = anthropic::spend::charge_of(usage, prices).unwrap();
    assert_eq!(number("settled_micros"), charged.micros());
    assert!(number("input_tokens") <= 40_000);
    assert!(["pass", "fail", "unsure"].contains(&field(&fields, "verdict")));
    assert_eq!(
        field(&fields, "egress_recipient"),
        "https://api.anthropic.com"
    );
    // No secret, no path and no reply: no key's form, no header, no absolute path,
    // no checklist item's name and no reason.
    for held in [
        "sk-ant",
        "x-api-key",
        "/Users/",
        "/private/",
        "/home/",
        "chk:ob:",
        "reason",
    ] {
        assert!(!page.contains(held), "the page holds {held}");
    }
}

#[test]
fn the_use_cases_record_names_a_commit_of_this_tree_and_holds_no_secret() {
    use whizbang_backends::anthropic;
    // Plan 2C, task A1, item 6: one live call of each of the four use cases task A1
    // added, each a block of fields.
    let page = std::fs::read_to_string(repository().join("docs/results/m2c-use-cases.md"))
        .expect("the record of the use cases is committed");
    let blocks: Vec<Vec<(String, String)>> = page
        .split("```toml\n")
        .skip(1)
        .map(|rest| {
            let block = rest.split("```").next().unwrap();
            let line = |line: &str| {
                let (name, value) = line.split_once(" = ").expect("a line is name = value");
                (name.to_owned(), value.trim_matches('"').to_owned())
            };
            block.lines().map(line).collect()
        })
        .collect();
    let named: Vec<&str> = blocks.iter().map(|b| field(b, "use_case")).collect();
    assert_eq!(
        named,
        [
            "reference-consistency",
            "reality-checks",
            "brief-compliance",
            "product-fidelity"
        ]
    );
    for fields in &blocks {
        let commit = field(fields, "commit");
        assert!(
            commit.len() == 40
                && commit
                    .bytes()
                    .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        );
        assert_eq!(field(fields, "whizbang"), env!("CARGO_PKG_VERSION"));
        // The action as it ran: plan 2C, task A2, took `check` to 1.3.0 after it.
        assert_eq!(field(fields, "action"), "check 1.2.0");
        assert_eq!(field(fields, "backend"), anthropic::ADAPTER);
        assert_eq!(field(fields, "adapter_version"), anthropic::VERSION);
        let prices = anthropic::spend::prices_of(field(fields, "model")).expect("a priced model");
        assert_eq!(field(fields, "prices_read"), prices.read);
        // Each turn settled at its usage, at the row of prices, within its bound.
        let number = |name: &str| field(fields, name).parse::<u64>().unwrap();
        assert_eq!(number("turns"), 1);
        let usage =
            whizbang_core::capability::Usage::new(number("input_tokens"), number("output_tokens"))
                .cached(number("cache_read_tokens"), number("cache_write_tokens"));
        let charged = anthropic::spend::charge_of(usage, prices).unwrap();
        assert_eq!(number("settled_micros"), charged.micros());
        assert!(number("settled_micros") <= number("request_bound_micros"));
        assert_eq!(
            number("reserved_micros"),
            number("request_bound_micros") * 16
        );
        assert!(["pass", "fail", "unsure"].contains(&field(fields, "verdict")));
        assert_eq!(field(fields, "outcome"), "completed");
        assert_eq!(
            field(fields, "egress_recipient"),
            "https://api.anthropic.com"
        );
    }
    // Plan 2C, task M1, item 5: one call over stdio, then the same call answered from
    // the decision cache, naming the first run, sending nothing and settling nothing.
    let door: Vec<Vec<(String, String)>> = page
        .split("```toml door\n")
        .skip(1)
        .map(|rest| {
            let block = rest.split("```").next().unwrap();
            let line = |line: &str| {
                let (name, value) = line.split_once(" = ").expect("a line is name = value");
                (name.to_owned(), value.trim_matches('"').to_owned())
            };
            block.lines().map(line).collect()
        })
        .collect();
    let [asked, again] = &door[..] else {
        panic!("two calls over stdio are recorded");
    };
    for fields in [asked, again] {
        assert_eq!(field(fields, "action"), "check 1.3.0");
        assert_eq!(field(fields, "outcome"), "completed");
        assert_eq!(field(fields, "use_case"), field(asked, "use_case"));
        assert!(field(fields, "commit").len() == 40);
    }
    assert_eq!(field(asked, "decision_cache"), "kept");
    assert_eq!(
        field(asked, "egress_recipient"),
        "https://api.anthropic.com"
    );
    assert!(field(asked, "settled_micros").parse::<u64>().unwrap() > 0);
    assert_eq!(field(again, "decision_cache"), "hit");
    assert_eq!(field(again, "answered_by_run"), field(asked, "run"));
    assert_eq!(field(again, "verdict"), field(asked, "verdict"));
    assert_eq!(field(again, "turns"), "0");
    assert_eq!(field(again, "settled_micros"), "0");
    assert_eq!(field(again, "egress_recipient"), "none");
    assert_eq!(
        field(again, "reserved_micros"),
        field(asked, "reserved_micros")
    );
    // Under a dollar in all, as the brief of the task allowed.
    let spent: u64 = blocks
        .iter()
        .map(|b| field(b, "settled_micros").parse::<u64>().unwrap())
        .sum();
    assert!(spent < 1_000_000);
    // Plan 3, task L0 (plan 2C, task P1, item 7): each of the five use cases asked once of
    // each new judge, a block of its own.
    the_two_new_judges_lines_are_of_this_tree(&page);
    // Plan m3, task Q1, item 7 (reworded with the task): each use case's recipe run live,
    // credited from the gate of details first run again, a block of the run and one a use
    // case. Until its values are in, Q1's box stays open.
    if page.contains("```toml recipes\n") {
        the_recipes_lines_are_of_this_tree(&page);
    } else {
        let plan =
            std::fs::read_to_string(repository().join("docs/plans/m3-milestone-3.md")).unwrap();
        let q1 = plan.split("\n## Q1. ").nth(1).unwrap();
        let q1 = q1.split("\n## ").next().unwrap();
        assert!(
            q1.contains("\n- [ ] Done when:"),
            "Q1 is ticked without its lines"
        );
    }
    // No secret, no path and no reply: no key's form, no header, no absolute path, no
    // checklist item's name and no reason.
    for held in [
        "sk-",
        "AIza",
        "x-goog-api-key",
        "Bearer",
        "x-api-key",
        "/Users/",
        "/private/",
        "/home/",
        "chk:",
        "reason",
    ] {
        assert!(!page.contains(held), "the page holds {held}");
    }
}

/// The lines of the five use cases run as recipes (plan m3, task Q1, item 7, reworded
/// with the task): the blocks `toml recipes` of `docs/results/m2c-use-cases.md`, the first
/// of the run, the gate of details first run again on the three judges, and one a use case
/// with the digest of its recipe as it ran; each use case completed at least once, and the
/// counts are the gate's receipt's.
fn the_recipes_lines_are_of_this_tree(page: &str) {
    let blocks: Vec<Fields> = (page.split("```toml recipes\n").skip(1))
        .map(|rest| {
            let block = rest.split("```").next().unwrap();
            let line = |line: &str| {
                let (name, value) = line.split_once(" = ").expect("a line is name = value");
                (name.to_owned(), value.trim_matches('"').to_owned())
            };
            block.lines().map(line).collect()
        })
        .collect();
    let (run, cases) = blocks.split_first().expect("a block of the run");
    let commit = field(run, "commit");
    assert!(is_ancestor(commit), "{commit}");
    assert_eq!(field(run, "whizbang"), env!("CARGO_PKG_VERSION"));
    assert_eq!(field(run, "gate"), "gates/details-first.toml");
    assert_eq!(
        field(run, "judges"),
        "claude-haiku-5-5, gemini-3.8-flash, gpt-6.1-terra"
    );
    let number = |fields: &Fields, name: &str| field(fields, name).parse::<u64>().expect(name);
    // The receipt the run wrote, committed beside the first.
    let receipt = repository().join(field(run, "receipt"));
    let bytes = std::fs::read(&receipt).expect("the re-run's receipt is committed");
    assert_eq!(field(run, "receipt_sha256"), Digest::of_bytes(&bytes).hex());
    let read: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(read["cells"].as_u64(), Some(number(run, "decisions")));
    // One block a use case, each its recipe as the commit had it, completed at least once.
    let named: Vec<&str> = cases.iter().map(|b| field(b, "use_case")).collect();
    assert_eq!(
        named,
        [
            "source-fidelity",
            "reference-consistency",
            "reality-checks",
            "brief-compliance",
            "product-fidelity"
        ]
    );
    let (mut decisions, mut completed) = (0, 0);
    for case in cases {
        let use_case = field(case, "use_case");
        let shown = std::process::Command::new("git")
            .args([
                "show",
                &format!("{commit}:crates/whizbang-squint/recipes/{use_case}.toml"),
            ])
            .current_dir(repository())
            .output()
            .expect("git runs");
        assert!(shown.status.success(), "{use_case} at {commit}");
        assert_eq!(
            field(case, "recipe_digest"),
            whizbang_eval::lab::digest_of(&shown.stdout),
            "{use_case}"
        );
        let done = number(case, "completed");
        assert!(done >= 1 && done <= number(case, "decisions"), "{use_case}");
        decisions += number(case, "decisions");
        completed += done;
    }
    assert_eq!(decisions, number(run, "decisions"));
    assert_eq!(completed, number(run, "completed"));
    assert_eq!(completed + number(run, "errors"), decisions);
    // Within what the plan let G1's gate spend, 15.50 dollars.
    assert!(number(run, "settled_micros") <= 15_500_000);
}

/// The ten blocks of the Google and OpenAI judges' live calls (plan 3, task L0), in
/// `docs/results/m2c-use-cases.md`: of a commit this tree descends from, each use case on
/// each judge once, and each run's spend what its usage and its turns allow.
fn the_two_new_judges_lines_are_of_this_tree(page: &str) {
    use whizbang_backends::service::spend::{
        DEFAULT_CEILINGS, LONG_PROMPT_TOKENS, bound_micros, charge_of,
    };
    use whizbang_backends::{google, openai};
    let blocks: Vec<Fields> = page
        .split("```toml judges\n")
        .skip(1)
        .map(|rest| {
            let block = rest.split("```").next().unwrap();
            let line = |line: &str| {
                let (name, value) = line.split_once(" = ").expect("a line is name = value");
                (name.to_owned(), value.trim_matches('"').to_owned())
            };
            block.lines().map(line).collect()
        })
        .collect();
    let use_cases = [
        "source-fidelity",
        "reference-consistency",
        "reality-checks",
        "brief-compliance",
        "product-fidelity",
    ];
    let mut asked: Vec<(&str, &str)> = blocks
        .iter()
        .map(|b| (field(b, "backend"), field(b, "use_case")))
        .collect();
    asked.sort_unstable();
    let mut wanted: Vec<(&str, &str)> = [google::ADAPTER, openai::ADAPTER]
        .iter()
        .flat_map(|backend| use_cases.iter().map(move |case| (*backend, *case)))
        .collect();
    wanted.sort_unstable();
    assert_eq!(asked, wanted, "each use case once on each judge");
    let commit = field(&blocks[0], "commit");
    assert!(
        commit.len() == 40
            && commit
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    );
    // The commit that ran is in this tree's history.
    let ancestor = std::process::Command::new("git")
        .args(["merge-base", "--is-ancestor", commit, "HEAD"])
        .current_dir(repository())
        .output()
        .expect("git runs");
    assert!(ancestor.status.success(), "{commit} is not an ancestor");
    let mut spent = 0;
    for fields in &blocks {
        let backend = field(fields, "backend");
        let (version, prices, recipient) = if backend == google::ADAPTER {
            (
                google::VERSION,
                google::spend::prices_of(field(fields, "model")),
                "https://generativelanguage.googleapis.com",
            )
        } else {
            (
                openai::VERSION,
                openai::spend::prices_of(field(fields, "model")),
                "https://api.openai.com",
            )
        };
        let prices = prices.expect("a priced model");
        assert_eq!(field(fields, "commit"), commit);
        assert_eq!(field(fields, "whizbang"), env!("CARGO_PKG_VERSION"));
        // The action as it ran.
        assert_eq!(field(fields, "action"), "check 1.4.0");
        assert_eq!(field(fields, "adapter_version"), version);
        assert!(
            field(fields, "model_version").starts_with(&format!("{version}+fit1-jpeg90-")),
            "{}",
            field(fields, "model_version")
        );
        assert_eq!(field(fields, "prices_read"), prices.read);
        assert_eq!(field(fields, "outcome"), "completed");
        assert!(["pass", "fail", "unsure"].contains(&field(fields, "verdict")));
        assert_eq!(field(fields, "egress_recipient"), recipient);
        assert_eq!(field(fields, "egress_outcome"), "sent");
        // The bound of one request at the default ceilings and the model's row; sixteen
        // of them reserved; the turns taken within the sixteen, each within its bound.
        let number = |name: &str| field(fields, name).parse::<u64>().unwrap();
        let bound = number("request_bound_micros");
        #[allow(clippy::cast_precision_loss)]
        let stated = bound as f64;
        assert!((stated - bound_micros(DEFAULT_CEILINGS, prices)).abs() < f64::EPSILON);
        assert_eq!(number("reserved_micros"), bound * 16);
        let turns = number("turns");
        assert!((1..=16).contains(&turns), "{turns}");
        let settled = number("settled_micros");
        assert!(settled <= bound * turns);
        // The record holds the usage summed over the turns, not each turn's. Each turn
        // was charged its own usage at the row, rounded up to a millionth; no turn's
        // input passed the long prompt's line, since the sum does not. So the settled
        // sum is at least the charge of the summed usage, and over it by fewer millionths
        // than there were turns.
        let usage =
            whizbang_core::capability::Usage::new(number("input_tokens"), number("output_tokens"))
                .cached(number("cache_read_tokens"), number("cache_write_tokens"));
        assert!(whizbang_backends::service::spend::reported_input(usage) <= LONG_PROMPT_TOKENS);
        let least = charge_of(usage, prices).unwrap().micros();
        assert!(
            least <= settled && settled < least + turns,
            "{backend} {}: {least} {settled}",
            field(fields, "use_case")
        );
        spent += settled;
    }
    // Within the plan's estimate for the task, 0.50 dollars.
    assert!(spent <= 500_000, "{spent}");
}

#[test]
fn the_coverage_record_names_a_commit_of_this_tree_and_every_timing() {
    // Plan 1D, task V1, item 4: the record `scripts/model-evidence.sh` wrote of the
    // timings of phrase, point and box on SAM 3 and Tiny, and the GPU's memory.
    let page = std::fs::read_to_string(repository().join("docs/results/m1d-segment-coverage.md"))
        .expect("the coverage record is committed");
    let block = page
        .split("```toml\n")
        .nth(1)
        .and_then(|rest| rest.split("```").next())
        .expect("the page holds its fields");
    let fields: Vec<(String, String)> = block
        .lines()
        .map(|line| {
            let (name, value) = line.split_once(" = ").expect("a line is name = value");
            (name.to_owned(), value.trim_matches('"').to_owned())
        })
        .collect();
    assert_eq!(field(&fields, "record"), "m1d-segment-coverage");
    let commit = field(&fields, "commit");
    assert!(
        commit.len() == 40
            && commit
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "{commit}"
    );
    assert_eq!(field(&fields, "tree"), "clean");
    assert_eq!(field(&fields, "device"), "mps");
    // Of this tree's models and adapters: the catalogue's revisions, the binary's
    // table, and the adapters' sources by their digests.
    let catalogue = wiring::catalogue().unwrap();
    let table = wiring::adapters();
    for (model, adapter, source) in [
        ("sam3", "sam3", "pack/src/whizbang_pack/sam3.py"),
        ("tiny", "owlv2-sam2", "pack/src/whizbang_pack/owlv2_sam2.py"),
    ] {
        let entry = catalogue.entry(model).unwrap();
        assert_eq!(
            field(&fields, &format!("{model}_revision")),
            entry.revision().as_str()
        );
        let revision = &table
            .iter()
            .find(|each| each.name.as_str() == adapter)
            .unwrap()
            .revision;
        assert_eq!(
            field(&fields, &format!("{model}_adapter_revision")),
            revision.as_str()
        );
        assert_eq!(
            field(&fields, &format!("{model}_adapter_source")),
            digest_of(source)
        );
        // Every timing: the load, the first request and a second, of each kind.
        for kind in ["phrase", "point", "box"] {
            for part in ["load", "first", "second"] {
                let name = format!("{model}_{kind}_{part}_seconds");
                let seconds: f64 = field(&fields, &name).parse().unwrap();
                assert!(seconds > 0.0 && seconds < 300.0, "{name}: {seconds}");
            }
        }
    }
    // The memory: the GPU's with one model of SAM 3 and with its two, and the tree's
    // resident peak; and nothing left running.
    let number = |name: &str| field(&fields, name).parse::<u64>().unwrap();
    assert!(number("sam3_one_model_gpu_mib") > 0);
    assert!(number("sam3_two_models_gpu_mib") >= number("sam3_one_model_gpu_mib"));
    assert!(number("peak_resident_kib") > 0);
    assert_eq!(number("still_running_after"), 0);
    for held in ["/Users/", "/private/", "/home/"] {
        assert!(!page.contains(held), "the page holds {held}");
    }
}

/// The blocks of fields of the evaluation record (plan 2C, task E1, item 7): the first is
/// the run's, each after it one set's.
fn evaluation() -> (String, Vec<Fields>) {
    let page = std::fs::read_to_string(repository().join("docs/results/m2c-evaluation.md"))
        .expect("the record of the evaluation is committed");
    let blocks = page
        .split("```toml\n")
        .skip(1)
        .map(|rest| {
            let block = rest.split("```").next().unwrap();
            let line = |line: &str| {
                let (name, value) = line.split_once(" = ").expect("a line is name = value");
                (name.to_owned(), value.trim_matches('"').to_owned())
            };
            block.lines().map(line).collect()
        })
        .collect();
    (page, blocks)
}

#[test]
fn the_evaluation_record_names_a_commit_of_this_tree_and_holds_only_aggregates() {
    let (page, blocks) = evaluation();
    let run = &blocks[0];
    let commit = field(run, "commit");
    assert!(
        commit.len() == 40
            && commit
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    );
    assert_eq!(field(run, "whizbang"), env!("CARGO_PKG_VERSION"));
    assert_eq!(field(run, "matrix"), "e1-evidence");
    assert_eq!(field(run, "model"), "claude-haiku-5-5");
    assert_eq!(field(run, "arm"), "F");
    // The matrix as committed is the one that ran.
    let text = std::fs::read_to_string(repository().join("matrices/e1-evidence.toml")).unwrap();
    let matrix = whizbang_eval::matrix::Matrix::parse(&text).unwrap();
    assert_eq!(field(run, "matrix_identity"), &matrix.identity()[..16]);
    let number = |fields: &Fields, name: &str| field(fields, name).parse::<u64>().unwrap();
    // Each set's block: its id, its counts and nothing of an item.
    let sets = &blocks[1..];
    assert_eq!(sets.len(), 10);
    let decisions: u64 = sets.iter().map(|b| number(b, "runs")).sum();
    assert_eq!(decisions, number(run, "decisions"));
    assert_eq!(decisions, 220);
    for set in sets {
        assert!(matrix.matrix.sets.iter().any(|id| id == field(set, "set")));
        let ended = ["completed", "failed", "interrupted"].map(|n| number(set, n));
        assert_eq!(ended.iter().sum::<u64>(), number(set, "runs"));
        let verdicts = ["pass", "fail", "unsure"].map(|n| number(set, n));
        assert_eq!(verdicts.iter().sum::<u64>(), number(set, "completed"));
        assert!(number(set, "agreeing") <= number(set, "completed"));
        let names: Vec<&str> = set.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "set",
                "use_case",
                "items",
                "runs",
                "completed",
                "failed",
                "interrupted",
                "pass",
                "fail",
                "unsure",
                "agreeing"
            ]
        );
    }
    // Spend within what the task allowed.
    assert!(number(run, "settled_micros") <= 3_000_000);
    // No item, no reply, no secret, no path.
    for dir in whizbang_eval::sets::discover(&repository().join("evalsets")).unwrap() {
        let set = whizbang_eval::sets::Set::load(&dir).unwrap();
        for (item, _) in &set.jobs {
            assert!(
                !page.contains(&format!("`{item}`")) && !page.contains(&format!("\"{item}\"")),
                "{item}"
            );
        }
    }
    for held in [
        "sk-ant",
        "x-api-key",
        "/Users/",
        "/private/",
        "/home/",
        "chk:",
        "reason",
    ] {
        assert!(!page.contains(held), "the page holds {held}");
    }
}

#[test]
fn the_evaluation_record_reports_at_least_100_runs_of_align_by_stats_and_by_export() {
    let (_, blocks) = evaluation();
    let run = &blocks[0];
    let stats: u64 = field(run, "align_runs_by_stats").parse().unwrap();
    let export: u64 = field(run, "align_runs_by_export").parse().unwrap();
    assert!(stats >= 100, "{stats}");
    assert_eq!(stats, export);
}

/// The fields of the block of numbers in a tool record's part "Rung 2 results".
fn rung_2(tool: &str) -> (String, Fields) {
    let page =
        std::fs::read_to_string(repository().join(format!("docs/reference/tools/{tool}.md")))
            .expect("the tool's record is committed");
    let (_, part) = page
        .split_once("\n## Rung 2 results\n")
        .expect("the record has its part on rung 2");
    let (part, _) = part.split_once("\n## ").unwrap();
    let block = part
        .split("```toml\n")
        .nth(1)
        .and_then(|rest| rest.split("```").next())
        .unwrap_or_else(|| panic!("{tool}: the part on rung 2 holds no numbers"));
    let fields = block
        .lines()
        .map(|line| {
            let (name, value) = line.split_once(" = ").expect("a line is name = value");
            (name.to_owned(), value.trim_matches('"').to_owned())
        })
        .collect();
    (page, fields)
}

#[test]
fn the_per_subject_record_names_a_commit_of_this_tree_and_its_rung_2_numbers() {
    use whizbang_squint::marks::{self, Mark};
    // Plan 2C, task X1, items 7 to 9 (taken by task F1): per-subject comparison on the two
    // sets of source fidelity, and `change_map` and `edge_similarity` on scenes-v1, each run
    // by `whizbang eval tool` from a commit, its numbers in the tool's record.
    let sets_root = repository().join("evalsets");
    for (tool, sets) in [
        (
            "per_subject",
            "source-fidelity/scenes-v1 source-fidelity/adversarial-v1",
        ),
        ("change_map", "source-fidelity/scenes-v1"),
        ("edge_similarity", "source-fidelity/scenes-v1"),
    ] {
        let (page, fields) = rung_2(tool);
        let commit = field(&fields, "commit");
        assert!(
            commit.len() == 40
                && commit
                    .bytes()
                    .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
            "{tool}: {commit}"
        );
        assert_eq!(field(&fields, "whizbang"), env!("CARGO_PKG_VERSION"));
        assert_eq!(field(&fields, "tool"), tool);
        assert_eq!(field(&fields, "sets"), sets);
        // The items are the sets' as this tree has them: each scored or an error.
        let mut items = 0;
        let mut ids = Vec::new();
        for id in sets.split(' ') {
            let set = whizbang_eval::sets::find(&sets_root, id).unwrap();
            items += set.jobs.len();
            ids.extend(set.jobs.iter().map(|(item, _)| item.clone()));
        }
        let number = |name: &str| -> f64 {
            field(&fields, name)
                .parse()
                .unwrap_or_else(|_| panic!("{tool}: {name}"))
        };
        assert_eq!(number("items") as usize, items, "{tool}");
        assert_eq!(
            number("items"),
            number("scored") + number("errors"),
            "{tool}"
        );
        assert_eq!(
            number("scored"),
            number("fail_items") + number("pass_items"),
            "{tool}"
        );
        assert_eq!(
            number("families"),
            number("chosen_on") + number("read_on"),
            "{tool}"
        );
        let (auc, lower, upper) = (number("auc"), number("auc_lower"), number("auc_upper"));
        assert!((0.0..=1.0).contains(&lower) && lower <= auc && auc <= upper && upper <= 1.0);
        assert!((0.0..=1.0).contains(&number("balanced_accuracy")));
        assert_eq!(number("resamples") as usize, whizbang::trying::RESAMPLES);
        assert_eq!(number("seed") as u64, whizbang::trying::SEED);
        // Aggregates alone: no item of the sets is named, and no path.
        for item in &ids {
            assert!(!page.contains(item.as_str()), "{tool} names {item}");
        }
        for held in ["/Users/", "/private/", "/home/"] {
            assert!(!page.contains(held), "{tool}: the page holds {held}");
        }
        // Its mark, from its record: the two ported tools are unverified with rungs 1 and
        // 2; per-subject comparison stays experimental (`SPEC.md` section 7).
        let mark = marks::stated(&page);
        if tool == "per_subject" {
            assert_eq!(mark, Some(Mark::Experimental));
        } else {
            assert_eq!(mark, Some(Mark::Unverified));
            assert_eq!(marks::of(tool), Some(Mark::Unverified));
        }
    }
    // Per-subject comparison's numbers are of this tree's tool file and SAM 3's adapter.
    let (_, fields) = rung_2("per_subject");
    let file = std::fs::read_to_string(repository().join("pack/tools/per_subject.py")).unwrap();
    assert!(file.contains(&format!(
        "# version = \"{}\"\n",
        field(&fields, "tool_version")
    )));
    assert_eq!(
        field(&fields, "tool_digest"),
        digest_of("pack/tools/per_subject.py")
    );
    assert_eq!(field(&fields, "model"), "sam3");
    assert_eq!(
        field(&fields, "adapter_source"),
        digest_of("pack/src/whizbang_pack/sam3.py")
    );
}

#[test]
fn the_fills_record_names_a_commit_of_this_tree_and_holds_no_secret() {
    // Plan m3, task F1, item 8: the live fill through fal, and the live `remove-prompt`
    // with SAM 3 and Bria, a block of fields each.
    let page = std::fs::read_to_string(repository().join("docs/results/m3-fills.md"))
        .expect("the record of the fills is committed");
    let blocks: Vec<Fields> = page
        .split("```toml\n")
        .skip(1)
        .map(|rest| {
            let block = rest.split("```").next().unwrap();
            let line = |line: &str| {
                let (name, value) = line.split_once(" = ").expect("a line is name = value");
                (name.to_owned(), value.trim_matches('"').to_owned())
            };
            block.lines().map(line).collect()
        })
        .collect();
    assert_eq!(blocks.len(), 2, "a fill and a remove-prompt");
    for (fields, action) in blocks.iter().zip(["fill 1.0.0", "remove_prompt 1.0.0"]) {
        a_fills_block(fields, action);
    }
    // The recipe's block names the recipe this tree ships, its eight steps, and SAM 3.
    let recipe = &blocks[1];
    let shipped = Digest::of_bytes(whizbang_rascal::recipes::REMOVE_PROMPT.as_bytes());
    assert_eq!(field(recipe, "recipe_digest"), shipped.to_string());
    assert_eq!(field(recipe, "steps"), "8");
    assert_eq!(field(recipe, "segmenter_model"), "sam3");
    // No secret, no path and no reply: no key's form, no header, no absolute path.
    for held in [
        "authorization",
        "Key ",
        "/Users/",
        "/home/",
        "/private/",
        "data:image",
    ] {
        assert!(!page.contains(held), "{held}");
    }
}

/// One block of the record of the fills: the fields every live fill through fal holds.
fn a_fills_block(fields: &Fields, action: &str) {
    use whizbang_backends::fal;
    let commit = field(fields, "commit");
    assert!(
        commit.len() == 40
            && commit
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
    assert_eq!(field(fields, "whizbang"), env!("CARGO_PKG_VERSION"));
    assert_eq!(field(fields, "action"), action);
    assert_eq!(field(fields, "backend"), fal::ADAPTER);
    assert_eq!(field(fields, "adapter_version"), fal::VERSION);
    assert_eq!(field(fields, "model"), fal::MODEL);
    assert_eq!(field(fields, "model_version"), fal::identity_version());
    assert_eq!(field(fields, "price_read"), fal::spend::PRICE.read);
    let price = fal::spend::bound_micros(fal::spend::PRICE).to_string();
    for name in ["request_bound_micros", "reserved_micros", "settled_micros"] {
        assert_eq!(field(fields, name), price, "{name}");
    }
    assert_eq!(field(fields, "outcome"), "completed");
    assert_eq!(field(fields, "outside_support_original"), "true");
    assert_eq!(field(fields, "egress_recipient"), "https://fal.run");
    assert_eq!(
        field(fields, "egress_classes"),
        "source_images derived_images"
    );
}

#[test]
fn the_unsure_record_names_a_commit_of_this_tree_and_holds_only_aggregates() {
    // Plan m3, task U1, items 1 and 5: the count by rule and the screen, aggregates only.
    let page = std::fs::read_to_string(repository().join("docs/results/m3-unsure.md"))
        .expect("the record of the attack on unsure is committed");
    let blocks: Vec<Fields> = page
        .split("```toml\n")
        .skip(1)
        .map(|rest| {
            let block = rest.split("```").next().unwrap();
            let line = |line: &str| {
                let (name, value) = line.split_once(" = ").expect("a line is name = value");
                (name.to_owned(), value.trim_matches('"').to_owned())
            };
            block.lines().map(line).collect()
        })
        .collect();
    let run = &blocks[0];
    let commit = field(run, "commit");
    assert!(
        commit.len() == 40
            && commit
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    );
    assert_eq!(field(run, "matrix"), "unsure-screen");
    assert_eq!(field(run, "model"), "claude-haiku-5-5");
    let text = std::fs::read_to_string(repository().join("matrices/unsure-screen.toml")).unwrap();
    let matrix = whizbang_eval::matrix::Matrix::parse(&text).unwrap();
    assert_eq!(field(run, "matrix_identity"), &matrix.identity()[..16]);
    let number = |fields: &Fields, name: &str| field(fields, name).parse::<u64>().unwrap();
    // Each block of a use case and an arm: its counts add up, and its arm is the matrix's.
    let arms: Vec<String> = matrix.matrix.arms.iter().map(ToString::to_string).collect();
    let named = |b: &Fields, name: &str| b.iter().any(|(n, _)| n == name);
    let cells: Vec<&Fields> = blocks[1..]
        .iter()
        .filter(|b| named(b, "arm") && !named(b, "matrix"))
        .collect();
    assert_eq!(cells.len(), 2 * arms.len());
    let runs: u64 = cells.iter().map(|b| number(b, "runs")).sum();
    assert_eq!(runs, number(run, "decisions"));
    assert_eq!(runs, 44 * 4 * 3);
    for cell in &cells {
        assert!(arms.iter().any(|arm| arm == field(cell, "arm")));
        let ended = ["completed", "failed", "interrupted"].map(|n| number(cell, n));
        assert_eq!(ended.iter().sum::<u64>(), number(cell, "runs"));
        let verdicts = ["pass", "fail", "unsure"].map(|n| number(cell, n));
        assert_eq!(verdicts.iter().sum::<u64>(), number(cell, "completed"));
        let labelled = ["labelled_pass", "labelled_fail"].map(|n| number(cell, n));
        assert_eq!(labelled.iter().sum::<u64>(), number(cell, "completed"));
        assert!(number(cell, "agreeing") <= number(cell, "completed"));
    }
    // The strict look's run: its matrix as committed, and its three arms' blocks.
    let strict_run = (blocks.iter())
        .find(|b| named(b, "commit") && field(b, "matrix") == "unsure-strict")
        .expect("the strict look's run");
    let text = std::fs::read_to_string(repository().join("matrices/unsure-strict.toml")).unwrap();
    let strict = whizbang_eval::matrix::Matrix::parse(&text).unwrap();
    assert_eq!(
        field(strict_run, "matrix_identity"),
        &strict.identity()[..16]
    );
    let strict_cells: Vec<&Fields> = (blocks.iter())
        .filter(|b| named(b, "arm") && named(b, "matrix"))
        .collect();
    assert_eq!(strict_cells.len(), strict.matrix.arms.len());
    let runs: u64 = strict_cells.iter().map(|b| number(b, "runs")).sum();
    assert_eq!(runs, number(strict_run, "decisions"));
    for cell in &strict_cells {
        let verdicts = ["pass", "fail", "unsure"].map(|n| number(cell, n));
        assert_eq!(verdicts.iter().sum::<u64>(), number(cell, "completed"));
    }
    // Spend within the plan's estimate for U1.
    let spent = number(run, "settled_micros") + number(strict_run, "settled_micros");
    assert!(spent <= 31_000_000);
    // No item, no reply, no secret, no path.
    for dir in whizbang_eval::sets::discover(&repository().join("evalsets")).unwrap() {
        let set = whizbang_eval::sets::Set::load(&dir).unwrap();
        for (item, _) in &set.jobs {
            assert!(
                !page.contains(&format!("`{item}`")) && !page.contains(&format!("\"{item}\"")),
                "{item}"
            );
        }
    }
    for held in [
        "sk-ant",
        "x-api-key",
        "/Users/",
        "/private/",
        "/home/",
        "chk:",
        "reason",
    ] {
        assert!(!page.contains(held), "the page holds {held}");
    }
}

/// The digest the public manifest of the baseline package names.
fn manifests_digest() -> String {
    let text = std::fs::read_to_string(repository().join("docs/baseline/manifest.toml")).unwrap();
    let line = text
        .lines()
        .find(|line| line.starts_with("digest = "))
        .expect("the manifest names a digest");
    line["digest = ".len()..].trim_matches('"').to_owned()
}

fn is_ancestor(commit: &str) -> bool {
    commit.len() == 40
        && commit
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        && std::process::Command::new("git")
            .args(["merge-base", "--is-ancestor", commit, "HEAD"])
            .current_dir(repository())
            .output()
            .expect("git runs")
            .status
            .success()
}

#[test]
fn the_baseline_receipt_names_the_manifests_digest_and_a_commit_of_this_tree() {
    let text = std::fs::read_to_string(repository().join("docs/baseline/receipt.toml"))
        .expect("the receipt of the gate's run is committed");
    let fields: Fields = text
        .lines()
        .map(|line| {
            let (name, value) = line.split_once(" = ").expect("a line is name = value");
            (name.to_owned(), value.trim_matches('"').to_owned())
        })
        .collect();
    let names: Vec<&str> = fields.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        [
            "package_digest",
            "comparison_version",
            "whizbang_commit",
            "result"
        ]
    );
    assert_eq!(field(&fields, "package_digest"), manifests_digest());
    assert_eq!(
        field(&fields, "comparison_version"),
        whizbang_eval::baseline::COMPARISON_VERSION
    );
    assert!(is_ancestor(field(&fields, "whizbang_commit")));
    assert_eq!(field(&fields, "result"), "pass");
}

#[test]
fn the_baseline_record_holds_the_live_reruns_two_numbers() {
    let page = std::fs::read_to_string(repository().join("docs/results/m2c-baseline.md"))
        .expect("the record of the live rerun is committed");
    let blocks: Vec<&str> = page.split("```toml\n").skip(1).collect();
    assert_eq!(blocks.len(), 1);
    let fields: Fields = blocks[0]
        .split("```")
        .next()
        .unwrap()
        .lines()
        .map(|line| {
            let (name, value) = line.split_once(" = ").expect("a line is name = value");
            (name.to_owned(), value.trim_matches('"').to_owned())
        })
        .collect();
    let names: Vec<&str> = fields.iter().map(|(name, _)| name.as_str()).collect();
    // The receipt's fields, the commit and the spend, and nothing of an item.
    assert_eq!(
        names,
        [
            "commit",
            "package_digest",
            "set",
            "model",
            "arm",
            "live_decisions",
            "live_failed",
            "compared",
            "agreed",
            "recorded_later",
            "recorded_differing",
            "settled_usd"
        ]
    );
    assert!(is_ancestor(field(&fields, "commit")));
    assert_eq!(field(&fields, "package_digest"), manifests_digest());
    assert_eq!(
        (
            field(&fields, "set"),
            field(&fields, "model"),
            field(&fields, "arm")
        ),
        ("source-fidelity/blind-v3", "claude-haiku-5-5", "C")
    );
    let number = |name: &str| field(&fields, name).parse::<u64>().unwrap();
    let (compared, agreed, later, differing) = (
        number("compared"),
        number("agreed"),
        number("recorded_later"),
        number("recorded_differing"),
    );
    assert!(agreed <= compared && differing <= later && compared <= number("live_decisions"));
    // The two numbers, as the page states them.
    let share = |part: u64, whole: u64| format!("{:.1}%", 100.0 * part as f64 / whole as f64);
    assert!(page.contains(&format!(
        "{agreed} of {compared} decisions compared ({})",
        share(agreed, compared)
    )));
    assert!(page.contains(&format!(
        "{differing} of {later} ({})",
        share(differing, later)
    )));
}

#[test]
fn the_gates_record_holds_the_details_first_receipt_and_only_aggregates() {
    let root = repository();
    let page = std::fs::read_to_string(root.join("docs/results/m3-gates.md"))
        .expect("the record of the gate of details first is committed");
    let bytes = std::fs::read(root.join("docs/results/m3-gates/details-first-receipt.json"))
        .expect("its receipt is committed");
    // The record names the receipt by its digest, and the receipt is of the frozen gate,
    // its matrix and the published labels of its sets.
    assert!(page.contains(&Digest::of_bytes(&bytes).hex()));
    let receipt: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let frozen = whizbang::gating::frozen(&root.join("gates/details-first.toml")).unwrap();
    assert_eq!(receipt["gate_sha256"], frozen.digest.as_str());
    assert!(page.contains(&frozen.digest));
    assert_eq!(
        receipt["matrix_identity"],
        frozen.matrix.identity().as_str()
    );
    assert_eq!(receipt["cells"], 1000);
    let sets = &frozen.matrix.matrix.sets;
    let mut ids = Vec::new();
    for id in sets {
        let set = whizbang_eval::sets::find(&root.join("evalsets"), id).unwrap();
        let labels = set.manifest.set.labels.clone().unwrap();
        let labelled = std::fs::read(set.dir.join(labels)).unwrap();
        assert_eq!(
            receipt["labels_sha256"][id.as_str()],
            Digest::of_bytes(&labelled).hex().as_str()
        );
        for (item, family) in set.families_by_item() {
            ids.push(item);
            ids.push(family);
        }
    }
    // Aggregates only: no item or family of any set appears in the receipt.
    let text = String::from_utf8(bytes).unwrap();
    for id in &ids {
        assert!(!text.contains(&format!("\"{id}\"")), "{id}");
    }
    // Each judge's outcome and gain as the record states them.
    assert_eq!(receipt["outcome"], "rejected");
    let judges = receipt["judges"].as_array().unwrap();
    assert_eq!(judges.len(), 3);
    for judge in judges {
        let model = judge["model"].as_str().unwrap();
        let outcome = judge["outcome"].as_str().unwrap();
        let gain = judge["paired"]["gain"]["point"].as_f64().unwrap();
        let row = page
            .lines()
            .find(|line| line.starts_with(&format!("| `{model}` | {outcome} |")))
            .unwrap_or_else(|| panic!("a row of {model}"));
        assert!(row.contains(&format!("| {gain:.3} [")), "{row}");
        let keys: Vec<&String> = judge.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["backend", "failed", "model", "outcome", "paired"]);
    }
}

#[test]
fn the_encoding_record_names_a_commit_of_this_tree() {
    // Plan m3, task O5, item 3: the time of encoding a result at three sizes, and the
    // setting chosen from it.
    let page = std::fs::read_to_string(repository().join("docs/results/m3-encoding.md"))
        .expect("the record of the encoding is committed");
    let block = page.split("```toml\n").nth(1).expect("a block of fields");
    let block = block.split("```").next().unwrap();
    let fields: Vec<(String, String)> = block
        .lines()
        .map(|line| {
            let (name, value) = line.split_once(" = ").expect("a line is name = value");
            (name.to_owned(), value.trim_matches('"').to_owned())
        })
        .collect();
    assert!(
        is_ancestor(field(&fields, "commit")),
        "{}",
        field(&fields, "commit")
    );
    assert_eq!(field(&fields, "whizbang"), env!("CARGO_PKG_VERSION"));
    // The record keeps the setting it measured against, the one the core had then; the
    // core has had the chosen one since the owner's yes of 2026-11-19.
    assert_eq!(field(&fields, "current"), "default_adaptive");
    assert_eq!(
        field(&fields, "chosen"),
        whizbang_core::identity::encode::SETTING
    );
    // The choice is the measurement's: the faster setting by far, at a cost in bytes of
    // under five per cent.
    let number = |name: &str| field(&fields, name).parse::<f64>().expect(name);
    assert_eq!(field(&fields, "chosen"), "fast_adaptive");
    assert!(number("fast_adaptive_8192x4096_ms") * 10.0 < number("default_adaptive_8192x4096_ms"));
    assert!(
        number("fast_adaptive_8192x4096_bytes") < 1.05 * number("default_adaptive_8192x4096_bytes")
    );
    for held in ["/Users/", "/home/", "/private/"] {
        assert!(!page.contains(held), "{held}");
    }
}

// --- The edits' rung 2 and the records of the five recipes that end in a fill (plan m3,
// task B1, items 1 and 2).

/// What each record's visual quality says (plan m3, tasks L3 and B1, "Amended by the
/// owner", 2026-11-19): the blinded picking rounds moved to milestone E, so no recipe's
/// visual quality was confirmed by a person in milestone 3. Rung 2 stays "In part", the
/// mark experimental, and the edits' record has no part on visual quality, until E's
/// decodes replace this line (`docs/carry-over.md`, the row on the blinded rounds).
const OWED: &str = "not yet confirmed by a person; owed by milestone E (the owner's decision of \
                    2026-11-19)";
/// The four recipes of the edit set, and the fifth, whose rung 2 is the lab's round.
const EDIT_RECIPES: [&str; 4] = ["remove", "remove_prompt", "repair", "repair_area"];
const FILL_RECIPES: [&str; 5] = ["remove", "remove_prompt", "repair", "repair_area", "seam"];

/// The blocks of fields of a page, each `name = value` with its quotes taken off.
fn blocks_of(page: &str) -> Vec<Fields> {
    let line = |line: &str| {
        let (name, value) = line.split_once(" = ").expect("a line is name = value");
        (name.to_owned(), value.trim_matches('"').to_owned())
    };
    (page.split("```toml\n").skip(1))
        .map(|rest| {
            rest.split("```")
                .next()
                .unwrap()
                .lines()
                .map(line)
                .collect()
        })
        .collect()
}

/// The edits' record: the page and its numbers file, as text.
fn edits() -> (String, String) {
    let read = |name: &str| std::fs::read_to_string(repository().join("docs/results").join(name));
    let page = read("m3-edits.md").expect("the edits' record is committed");
    let numbers = read("m3-edits.toml").expect("its numbers are committed");
    (page, numbers)
}

/// A recipe's record and the part of it on rung 2.
fn recipe_record(recipe: &str) -> (String, String) {
    let path = repository().join(format!("docs/reference/tools/{recipe}.md"));
    let record = std::fs::read_to_string(path).expect("the recipe's record is committed");
    let rung_2 = record
        .split("## Rung 2 results\n")
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .expect("a part on rung 2")
        .to_owned();
    (record, rung_2)
}

/// The text of the shipped recipe `name`.
fn shipped(name: &str) -> &'static str {
    (whizbang_rascal::recipes::SHIPPED.iter())
        .find(|text| text.contains(&format!("\nname = \"{name}\"\n")))
        .unwrap_or_else(|| panic!("{name} is no shipped recipe"))
}

/// The constant sets the shipped recipe's steps name, each with the digest of its text,
/// in the order of their names, as the edits' record writes them.
fn constant_sets_of(name: &str) -> Vec<String> {
    let mut names: Vec<&str> = (shipped(name).lines())
        .filter_map(|line| line.strip_prefix("constants = \""))
        .map(|rest| rest.trim_end_matches('"'))
        .collect();
    names.sort_unstable();
    names.dedup();
    (names.iter())
        .map(|set| {
            let text = whizbang_rascal::constants::text_of(set).expect("a shipped set");
            format!("{set} {}", whizbang_eval::lab::digest_of(text.as_bytes()))
        })
        .collect()
}

#[test]
fn the_edits_record_names_a_commit_of_this_tree_and_holds_only_aggregates() {
    let (page, numbers) = edits();
    let blocks = blocks_of(&page);
    let run = &blocks[0];
    assert!(
        is_ancestor(field(run, "commit")),
        "{}",
        field(run, "commit")
    );
    assert_eq!(field(run, "whizbang"), env!("CARGO_PKG_VERSION"));
    // The set the round ran is the committed one.
    let set_text = std::fs::read_to_string(repository().join("labs/m3-edits/edits.toml")).unwrap();
    let set = whizbang::edits::EditSet::parse(&set_text).unwrap();
    assert_eq!(field(run, "set"), set.set);
    assert_eq!(field(run, "seed"), set.seed.to_string());
    assert_eq!(field(run, "phrase"), set.phrase);
    let number = |fields: &Fields, name: &str| field(fields, name).parse::<u64>().expect(name);
    let real = |fields: &Fields, name: &str| field(fields, name).parse::<f64>().expect(name);
    assert_eq!((number(run, "pictures"), number(run, "items")), (24, 96));
    // One block a recipe, in the order of the set; their counts add up, nothing changed
    // outside a mask, and the spend is within the plan's estimate for B1.
    let recipes = &blocks[1..];
    let named: Vec<&str> = recipes.iter().map(|b| field(b, "recipe")).collect();
    assert_eq!(named, EDIT_RECIPES);
    let mut settled = 0;
    for (recipe, block) in EDIT_RECIPES.iter().zip(recipes) {
        let ended = ["completed", "failed", "not_run"].map(|n| number(block, n));
        assert_eq!(
            ended.iter().sum::<u64>(),
            number(block, "items"),
            "{recipe}"
        );
        assert_eq!(number(block, "items"), 24, "{recipe}");
        assert_eq!(number(block, "changed_outside_mask"), 0, "{recipe}");
        assert!(number(block, "items_preserved") <= number(block, "completed"));
        assert!(number(block, "fill_beats_blur") <= number(block, "completed"));
        for share in [
            "mask_iou_mean",
            "mask_iou_min",
            "coverage_mean",
            "coverage_min",
        ] {
            assert!(
                (0.0..=1.0).contains(&real(block, share)),
                "{recipe} {share}"
            );
        }
        assert!(real(block, "mask_iou_min") <= real(block, "mask_iou_mean"));
        assert!(real(block, "mask_iou_mean") <= real(block, "coverage_mean"));
        // The lab is the committed one, under its frozen protocol.
        assert_eq!(field(block, "lab"), whizbang::edits::lab_name(recipe));
        let protocol = format!("labs/m3-edits/{recipe}-protocol.toml");
        let bytes = std::fs::read(repository().join(&protocol)).unwrap();
        assert_eq!(
            field(block, "protocol_digest"),
            whizbang_eval::lab::digest_of(&bytes)
        );
        // The backends are named: Bria through fal for every fill, SAM 3 for the phrase.
        assert!(field(block, "filler").starts_with("fal bria-eraser "));
        let finder = field(block, "finder");
        assert_eq!(
            finder.starts_with("pack sam3 sha256:"),
            *recipe == "remove_prompt"
        );
        settled += number(block, "settled_micros");
    }
    assert_eq!(settled, number(run, "settled_micros"));
    assert!(settled <= 4_000_000, "{settled}");
    // The numbers file says what the page says, to the page's four places.
    let file: toml::Table = numbers.parse().expect("the numbers file is TOML");
    assert_eq!(file["commit"].as_str(), Some(field(run, "commit")));
    assert_eq!(file["settled_micros"].as_integer(), Some(settled as i64));
    let rows = file["recipe"].as_array().expect("a table a recipe");
    assert_eq!(rows.len(), EDIT_RECIPES.len());
    for (row, block) in rows.iter().zip(recipes) {
        for (name, value) in block {
            match &row[name.as_str()] {
                toml::Value::Float(f) => assert_eq!(&format!("{f:.4}"), value, "{name}"),
                toml::Value::Integer(i) => assert_eq!(&i.to_string(), value, "{name}"),
                toml::Value::String(s) => assert_eq!(s, value, "{name}"),
                toml::Value::Array(sets) if sets.is_empty() => assert_eq!(value, "none"),
                toml::Value::Array(sets) => {
                    let sets: Vec<&str> = sets.iter().filter_map(toml::Value::as_str).collect();
                    assert_eq!(&sets.join(", "), value, "{name}");
                }
                other => panic!("{name}: {other:?}"),
            }
        }
    }
    // No part on visual quality: no person picked in milestone 3.
    assert!(!page.contains("## Visual quality"));
    // Aggregates only: no item of the set, no picture's name, no path.
    for item in &set.item {
        for held in [&item.id, &item.picture, &item.truth] {
            assert!(!page.contains(held.as_str()) && !numbers.contains(held.as_str()));
        }
    }
    for held in ["/Users/", "/home/", "/private/", "FAL_KEY", "Key "] {
        assert!(!page.contains(held) && !numbers.contains(held), "{held}");
    }
}

#[test]
fn each_model_based_recipes_record_holds_rungs_1_and_2() {
    let (page, _) = edits();
    let blocks = blocks_of(&page);
    for recipe in FILL_RECIPES {
        let (record, rung_2) = recipe_record(recipe);
        let history = record
            .split("## Mark and history\n")
            .nth(1)
            .expect("its history");
        assert!(
            history.contains("\n| 1. Single examples | Shown |"),
            "{recipe}"
        );
        // Rung 2 in part: its visual quality is owed by milestone E, said in the one way.
        for held in ["\n| 2. Scored alone | In part |", "**Mark.** Experimental"] {
            assert!(history.contains(held), "{recipe}: {held}");
        }
        let line = (rung_2.lines())
            .find(|line| line.contains("visual quality.**") || line.contains("Visual quality.**"))
            .unwrap_or_else(|| panic!("{recipe}: no line on visual quality"));
        let said = (rung_2.split(line).nth(1).unwrap().lines().skip(1))
            .take_while(|l| !l.trim().is_empty() && !l.starts_with("- **"))
            .fold(line.to_owned(), |all, l| format!("{all} {}", l.trim()));
        assert!(said.ends_with(OWED), "{recipe}: {said}");
        assert!(!record.contains("pending"), "{recipe}");
        if recipe == "seam" {
            // The seam's round: the lab of task L3, on the shipped recipe.
            let block = &blocks_of(&rung_2)[0];
            assert_eq!(field(block, "lab"), "m3-seam");
            assert_eq!(field(block, "completed"), field(block, "cells"));
            continue;
        }
        // The edits' figures, as the edits' record has them.
        let ours = blocks_of(&rung_2);
        let theirs = (blocks.iter()).find(|b| b.iter().any(|(n, v)| n == "recipe" && v == recipe));
        assert_eq!(Some(&ours[0]), theirs, "{recipe}");
        for name in ["changed_outside_mask", "items_preserved", "fill_beats_blur"] {
            let value = field(&ours[0], name);
            assert!(rung_2.contains(&format!(" {value} ")), "{recipe}: {name}");
        }
    }
}

#[test]
fn each_recipes_rung_2_names_the_shipped_recipes_and_constant_sets_digests() {
    let (_, numbers) = edits();
    let file: toml::Table = numbers.parse().unwrap();
    let rows = file["recipe"].as_array().unwrap();
    for recipe in FILL_RECIPES {
        let digest = whizbang_eval::lab::digest_of(shipped(recipe).as_bytes());
        let sets = constant_sets_of(recipe);
        let (_, rung_2) = recipe_record(recipe);
        let block = &blocks_of(&rung_2)[0];
        assert_eq!(field(block, "recipe"), recipe);
        assert_eq!(field(block, "recipe_digest"), digest, "{recipe}");
        let named = if sets.is_empty() {
            "none".to_owned()
        } else {
            sets.join(", ")
        };
        assert_eq!(field(block, "constant_sets"), named, "{recipe}");
        if recipe == "seam" {
            continue;
        }
        let row = (rows.iter())
            .find(|row| row["recipe"].as_str() == Some(recipe))
            .unwrap();
        assert_eq!(row["recipe_digest"].as_str(), Some(digest.as_str()));
        let written: Vec<&str> = (row["constant_sets"].as_array().unwrap().iter())
            .filter_map(toml::Value::as_str)
            .collect();
        assert_eq!(written, sets, "{recipe}");
    }
}

#[test]
fn the_blinded_rounds_of_milestone_3_are_owed_by_e_with_their_protocols_digests() {
    // The owner's decision of 2026-11-19 (plan m3, tasks L3 and B1, "Amended by the
    // owner"): one row of the carry-over list, owned by milestone E, names both rounds by
    // their labs and the digests of their frozen protocols, and says both ways to finish
    // them; every record's visual quality points at it ([`OWED`]).
    let list = std::fs::read_to_string(repository().join("docs/carry-over.md")).unwrap();
    let mut protocols = vec!["labs/m3-seam/protocol.toml".to_owned()];
    protocols.extend(EDIT_RECIPES.map(|recipe| format!("labs/m3-edits/{recipe}-protocol.toml")));
    let digests: Vec<String> = (protocols.iter())
        .map(|path| whizbang_eval::lab::digest_of(&std::fs::read(repository().join(path)).unwrap()))
        .collect();
    let rows: Vec<&str> = (list.lines())
        .filter(|line| digests.iter().all(|digest| line.contains(digest.as_str())))
        .collect();
    let [row] = rows[..] else {
        panic!("{} rows name every protocol's digest", rows.len());
    };
    let cells: Vec<&str> = row.trim_matches('|').split(" | ").collect();
    assert_eq!(cells.len(), 4, "{row}");
    assert!(cells[2].starts_with("E"), "owned by {}", cells[2]);
    for held in [
        "`m3-seam`",
        "`m3-edits-<recipe>`",
        "not yet confirmed by a person",
        "2026-11-19",
    ] {
        assert!(cells[1].contains(held), "{held}");
    }
    // Both ways: the pages drawn again, or the kept stores decoded.
    for held in ["drawn again", "kept stores", "decoded"] {
        assert!(cells[3].contains(held), "{held}");
    }
    assert!(OWED.contains("milestone E"));
}
