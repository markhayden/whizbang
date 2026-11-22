//! The model tier's committed evidence (plan 1C, task R2b; decision D15; taken again by
//! plan 2A, task R1), held in the fast tier: the record `scripts/model-evidence.sh` wrote
//! is of the tree's adapter, lock and fixture, on a clean head, with each first load
//! after a full check's record; and the fixture picture, which is public, holds no
//! metadata.

use std::path::{Path, PathBuf};

use whizbang::wiring;
use whizbang_core::identity::{DecodeLimits, Digest, identify};

use crate::model_tier::FIXTURE;

/// The version every record here was measured by. A record keeps the version of the
/// binary that made it: these were all made before the first release, when the
/// workspace's version was 0.0.1, and the version became 0.1.0 with plan m4, task R2. A
/// record made after that says its own version, and its test holds it to that one. That
/// a record is of this tree is held by its commit, which each test checks apart.
const MEASURED_BY: &str = "0.0.1";

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
    assert_eq!(field(&fields, "whizbang"), MEASURED_BY);
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
        assert_eq!(field(fields, "whizbang"), MEASURED_BY);
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
    // case.
    the_recipes_lines_are_of_this_tree(&page);
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
    assert_eq!(field(run, "whizbang"), MEASURED_BY);
    assert_eq!(field(run, "action"), "check 1.6.0");
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
    let verdicts = ["pass", "fail", "unsure"].map(|n| number(run, n));
    assert_eq!(verdicts.iter().sum::<u64>(), completed);
    // Within what the plan let G1's gate spend, 15.50 dollars.
    let spent: f64 = field(run, "settled_usd").parse().expect("settled_usd");
    assert!((0.0..=15.5).contains(&spent), "{spent}");
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
        assert_eq!(field(fields, "whizbang"), MEASURED_BY);
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
    assert_eq!(field(run, "whizbang"), MEASURED_BY);
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
        assert_eq!(field(&fields, "whizbang"), MEASURED_BY);
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
    assert_eq!(field(fields, "whizbang"), MEASURED_BY);
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
fn the_record_of_check_says_details_first_is_off_and_names_its_gate() {
    // Plan m4, task A1, item 5: `check`'s record (`docs/results/m2c-use-cases.md`, the
    // record of its use cases' live calls), the guide to the command, and the five use
    // cases' records each say that details first is off by default, name the gate that
    // rejected it and where its run is recorded, and say how to turn it on.
    let read = |path: &str| {
        std::fs::read_to_string(repository().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
    };
    // What the records say is what the door does.
    assert_eq!(
        whizbang::door::Details::DEFAULT,
        whizbang::door::Details::None
    );
    let stated = whizbang::door::Measuring::stated(None, None).unwrap();
    assert_eq!(stated.projection, "visual_only");
    // The gate they name is the committed one, and its record's fourth run rejected.
    const GATE: &str = "gates/details-first.toml";
    assert!(repository().join(GATE).is_file());
    let gates = read("docs/results/m3-gates.md");
    let (_, fourth) = gates
        .split_once("\n## The fourth run, the complete one")
        .expect("the fourth run's part");
    assert!(fourth.contains("**Rejected.**"));
    let mut pages = vec![
        "docs/results/m2c-use-cases.md".to_owned(),
        "docs/guides/config.md".to_owned(),
    ];
    pages.extend(
        whizbang_squint::use_case::NAMES
            .iter()
            .map(|name| format!("docs/reference/tools/{name}.md")),
    );
    for path in &pages {
        let page = read(path);
        // One line of running words, so a phrase broken over two lines is still found.
        let page = page.split_whitespace().collect::<Vec<_>>().join(" ");
        for said in [
            // Off by default, and what the default is.
            "off by default",
            "visual_only",
            // The gate that rejected it, and the run to read.
            "docs/results/m3-gates.md",
            "The fourth run, the complete one",
            // How to turn it on.
            "--details first",
        ] {
            assert!(page.contains(said), "{path} does not say {said:?}");
        }
        if path != "docs/guides/config.md" {
            assert!(page.contains(GATE), "{path} does not name {GATE}");
            // A part of its own: a heading in `check`'s record, and a paragraph under
            // "What it does" in a tool's record, whose headings are the seven of
            // `SPEC.md` section 8 and no other.
            assert!(
                page.contains("## Details first is off by default")
                    || page.contains("**Details first is off by default.**"),
                "{path}: its part on the default"
            );
        }
        // No record still calls details first the default.
        for stale in [
            "`--details first`, the default",
            "measures details first all the same",
        ] {
            assert!(!page.contains(stale), "{path} still says {stale:?}");
        }
    }
}

#[test]
fn the_gates_record_holds_the_details_first_receipt_and_only_aggregates() {
    let page = std::fs::read_to_string(repository().join("docs/results/m3-gates.md"))
        .expect("the record of the gate of details first is committed");
    // The first run, and the run again after the judges' fixes (plan m3, the closing pull
    // request), each its receipt and its table under its own part of the record.
    let (first, again) = page
        .split_once("\n## Run again")
        .expect("a part on the run again");
    // The fourth run, the first complete one (the Google key answering), its own part.
    let (again, fourth) = again
        .split_once("\n## The fourth run")
        .expect("a part on the fourth run");
    a_gates_receipt_as_its_record_states_it(first, "details-first-receipt.json");
    a_gates_receipt_as_its_record_states_it(again, "details-first-rerun-receipt.json");
    a_gates_receipt_as_its_record_states_it(fourth, "details-first-fourth-receipt.json");
}

/// A receipt of the gate of details first, committed under `docs/results/m3-gates/`: named
/// by its digest in `part`, of the frozen gate, its matrix and the published labels of its
/// sets, aggregates only, and each judge's outcome and gain as `part`'s table states them.
fn a_gates_receipt_as_its_record_states_it(part: &str, name: &str) {
    let root = repository();
    let bytes = std::fs::read(root.join("docs/results/m3-gates").join(name))
        .expect("its receipt is committed");
    assert!(part.contains(&Digest::of_bytes(&bytes).hex()), "{name}");
    let receipt: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let frozen = whizbang::gating::frozen(&root.join("gates/details-first.toml")).unwrap();
    assert_eq!(receipt["gate_sha256"], frozen.digest.as_str());
    assert!(part.contains(frozen.digest.as_str()), "{name}");
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
        let row = part
            .lines()
            .find(|line| line.starts_with(&format!("| `{model}` | {outcome} |")))
            .unwrap_or_else(|| panic!("{name}: a row of {model}"));
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
    assert_eq!(field(&fields, "whizbang"), MEASURED_BY);
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
    assert_eq!(field(run, "whizbang"), MEASURED_BY);
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

/// The version the loop's live record was measured by: it was made after the first
/// release's version was set (plan m4, task R2), so it is not [`MEASURED_BY`].
const LOOP_MEASURED_BY: &str = "0.1.0";

/// The judges of the loop's live evidence, each by its name in the evidence's config and
/// the model it names, with the most its 24 runs may settle: twice the plan's estimate
/// (plan m4, "Spend": a line that passes twice its estimate stops).
const LOOP_JUDGES: [(&str, &str, i64); 2] = [
    ("claude-haiku-5-5", "claude-haiku-5-5", 6_400_000),
    ("gpt-6-1-terra", "gpt-6.1-terra", 8_200_000),
];

#[test]
fn the_loops_tool_record_holds_rung_1_and_says_what_of_rung_2_is_shown() {
    // Plan m4, task E1, item 4.
    let (record, rung_2) = recipe_record("remove_checked");
    let history = record
        .split("## Mark and history\n")
        .nth(1)
        .expect("its history");
    for held in [
        "\n| 1. Single examples | Shown |",
        "\n| 2. Scored alone | In part |",
        "**Mark.** Experimental",
    ] {
        assert!(history.contains(held), "{held}");
    }
    // Rung 1: worked examples, one of each kind of retry and a case it handles badly,
    // with their pictures; `tool_records::recipes::…` holds them to the loop.
    let examples = record
        .split("## Examples\n")
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .expect("its examples");
    assert_eq!(examples.matches("\n### ").count(), 3);
    assert!(examples.contains("**This is a case it handles badly.**"));
    assert!(examples.contains("protected subject"));
    // Rung 2, in part, and said so: preservation by a test that is there; the mask and
    // the fill credited to milestone 3's record; what the loop adds reported, not scored.
    let said = rung_2.split_whitespace().collect::<Vec<_>>().join(" ");
    let test = "every_pixel_outside_the_kept_mask_is_the_pictures_own";
    assert!(include_str!("checked.rs").contains(&format!("fn {test}()")));
    for held in [
        "**shown in part**",
        &format!("`checked::{test}`"),
        "`docs/results/m3-edits.md`",
        "reported, not scored",
        "`docs/results/m4-loop.md`",
        "whether a retry makes a result better is not known",
    ] {
        assert!(said.contains(held), "{held}");
    }
    assert!(said.ends_with(OWED), "{said}");
    assert!(!record.contains("pending"));
    // Its cost for each judge is the guide's table, row for row, which
    // `guides::the_loops_guide_states_the_bound_of_an_attempt_…` holds to the catalogue.
    let guide = std::fs::read_to_string(repository().join("docs/guides/loop.md")).unwrap();
    let rows: Vec<&str> = (guide.lines())
        .filter(|line| line.starts_with("| `") && line.matches(" | ").count() == 4)
        .collect();
    assert!(rows.len() >= 5, "{rows:?}");
    for row in rows {
        assert!(
            record.contains(row),
            "the record lacks the guide's row {row}"
        );
    }
}

#[test]
fn the_loops_record_names_a_commit_of_this_tree_and_holds_only_aggregates() {
    // Plan m4, task E1, items 1 and 2. What the evidence is run with is held always:
    // the committed config and the manifest's 24 entries.
    let config = std::fs::read(repository().join("labs/m4-loop/config.toml")).unwrap();
    let config =
        whizbang_core::config::Config::parse(&config, &whizbang_core::contract::Secrets::none())
            .expect("the evidence's config is one");
    let judge = whizbang_core::contract::Capability::Judge;
    assert_eq!(config.call_cap_micros(), Some(3_500_000));
    let judges: Vec<(&str, Option<&str>)> = (config.backends(judge))
        .map(|(name, backend)| (name, backend.model()))
        .collect();
    let named: Vec<(&str, Option<&str>)> = (LOOP_JUDGES.iter())
        .map(|(name, model, _)| (*name, Some(*model)))
        .collect();
    assert_eq!(judges, named);
    let set_text = std::fs::read_to_string(repository().join("labs/m3-edits/edits.toml")).unwrap();
    let set = whizbang::edits::EditSet::parse(&set_text).unwrap();
    let entries = (set.item.iter()).filter(|item| item.recipe == "remove_prompt");
    assert_eq!(entries.count(), 24);

    // The live record: the orchestrator's runs, committed.
    let results = repository().join("docs/results");
    let page = std::fs::read_to_string(results.join("m4-loop.md"))
        .expect("the loop's live record is committed");
    let body = |text: &str| -> String {
        let lines: Vec<&str> = text.lines().filter(|line| !line.starts_with('#')).collect();
        lines.join("\n").trim_matches('\n').to_owned()
    };
    let whole = |table: &toml::Table, name: &str| -> i64 {
        table[name].as_integer().unwrap_or_else(|| panic!("{name}"))
    };
    let sum = |table: &toml::Table, name: &str| -> i64 {
        let fields = table[name].as_table().unwrap_or_else(|| panic!("{name}"));
        fields.values().filter_map(toml::Value::as_integer).sum()
    };
    let mut texts = vec![page.clone()];
    for (name, model, most) in LOOP_JUDGES {
        let text = std::fs::read_to_string(results.join(format!("m4-loop-{name}.toml")))
            .expect("the tally of each judge is committed");
        let tally: toml::Table = text.parse().expect("a tally is TOML");
        assert_eq!(tally["schema"].as_str(), Some("loop-evidence/1"));
        assert!(is_ancestor(tally["commit"].as_str().unwrap()), "{name}");
        assert_eq!(tally["whizbang"].as_str(), Some(LOOP_MEASURED_BY));
        assert_eq!(tally["judge"].as_str(), Some(name));
        assert!(tally["judge_model"].as_str().unwrap().contains(model));
        assert!(
            tally["filler"]
                .as_str()
                .unwrap()
                .starts_with("fal bria-eraser ")
        );
        assert!(
            tally["finder"]
                .as_str()
                .unwrap()
                .starts_with("pack sam3 sha256:")
        );
        assert_eq!(
            tally["manifest_sha256"].as_str(),
            Some(digest_of("labs/m3-edits/edits.toml").as_str())
        );
        // Every entry is accounted for, and each count of the runs adds up to them.
        let runs = whole(&tally, "runs");
        assert_eq!(whole(&tally, "entries"), 24);
        assert_eq!(runs + whole(&tally, "not_run"), 24);
        for counted in ["outcomes", "ended", "stopped_by_row", "stopped_by_reason"] {
            assert_eq!(sum(&tally, counted), runs, "{name}: {counted}");
        }
        assert_eq!(
            sum(&tally, "attempt_verdicts"),
            whole(&tally, "attempts_made")
        );
        assert!(sum(&tally, "retries") <= whole(&tally, "attempts_made"));
        assert!(whole(&tally, "settled_micros") <= most, "{name}");
        // The page holds the tally as it was written.
        assert!(
            page.contains(&body(&text)),
            "the page lacks the tally of {name}"
        );
        texts.push(text);
    }
    // The three cases by hand, each with its fields.
    for case in ["pass", "vague", "no-key"] {
        let part = page
            .split(&format!("### `{case}`\n\n```toml\n"))
            .nth(1)
            .and_then(|rest| rest.split("\n```").next())
            .unwrap_or_else(|| panic!("the page has no case {case}"));
        let fields: toml::Table = part.parse().expect("a case is TOML");
        assert!(is_ancestor(fields["commit"].as_str().unwrap()), "{case}");
        assert_eq!(fields["whizbang"].as_str(), Some(LOOP_MEASURED_BY));
        for name in ["outcome", "verdict", "ended", "ended_as"] {
            assert!(fields[name].is_str(), "{case}: {name}");
        }
        for name in ["attempts", "kept", "settled_micros", "reserved_micros"] {
            assert!(fields[name].is_integer(), "{case}: {name}");
        }
        let ended_as = fields["ended_as"].as_str().unwrap();
        match case {
            // A phrase that names nothing was not passed; a missing key leaves the
            // run unchecked, and says why.
            // Its sentence is the one measured; the page says it was changed after, by
            // a test that is there.
            "vague" => {
                assert!(!ended_as.starts_with("passed"), "{ended_as}");
                let test = "a_phrase_that_finds_nothing_ends_remove_prompt_and_the_loop_with_a_sentence_of_its_own";
                assert!(include_str!("checked.rs").contains(&format!("fn {test}()")));
                assert!(page.contains(&format!("`checked::{test}`")), "{test}");
            }
            "no-key" => {
                assert_eq!(ended_as, "unchecked");
                assert_eq!(fields["checked"].as_bool(), Some(false));
                assert_ne!(fields["unchecked_why"].as_str(), Some("none"));
            }
            _ => {}
        }
    }
    // What it is not, said on the page; and aggregates only: no item of the set, no
    // picture's name, no path, no key.
    for held in ["It scores nothing", "not yet confirmed by a person"] {
        assert!(page.contains(held), "{held}");
    }
    for text in &texts {
        for item in &set.item {
            for held in [&item.id, &item.picture, &item.truth] {
                assert!(!text.contains(held.as_str()), "{held}");
            }
        }
        for held in [
            "/Users/",
            "/home/",
            "/private/",
            "FAL_KEY",
            "API_KEY",
            ".png",
        ] {
            assert!(!text.contains(held), "{held}");
        }
    }
}

/// The page of the judge round (plan e1, task J1, item 3) and the `judges/1` it is written
/// from, and the three matrices it is scored from, in the order the command names them.
const JUDGES_PAGE: &str = "docs/results/e-judges.md";
const JUDGES_SCORES: &str = "docs/results/e-judges.toml";
const JUDGES_MATRICES: [&str; 3] = ["e-submission", "e-judges-dev", "e-judges-blind"];

/// A rate, a share or an estimate's figure as the judge round's page writes it: three
/// places, or `n/a`.
fn judges_share(value: Option<&toml::Value>) -> String {
    value
        .and_then(toml::Value::as_float)
        .map_or_else(|| "n/a".to_owned(), |v| format!("{v:.3}"))
}

/// An estimate as `0.912 (0.870 to 0.950)`, or `n/a`.
fn judges_estimate(value: Option<&toml::Value>) -> String {
    value.map_or_else(
        || "n/a".to_owned(),
        |e| {
            format!(
                "{} ({} to {})",
                judges_share(e.get("point")),
                judges_share(e.get("lower")),
                judges_share(e.get("upper"))
            )
        },
    )
}

fn yes(value: &toml::Value) -> &'static str {
    if value.as_bool().unwrap() {
        "yes"
    } else {
        "no"
    }
}

/// The four sections of the judge round's page that hold a table, in order.
const JUDGES_TABLES: [&str; 4] = [
    "The five judges",
    "By scope",
    "Rejections and causes",
    "The clauses of each judge",
];

/// The text of the page's section `## <title>`, up to the next section.
fn section<'a>(page: &'a str, title: &str) -> &'a str {
    page.split(&format!("\n## {title}\n"))
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .unwrap_or_else(|| panic!("the page has the section {title}"))
}

/// The header of the one table in a section, and its cells' count of `|`.
fn table_header(section: &str) -> (&str, usize) {
    let header = (section.lines())
        .find(|line| line.starts_with('|'))
        .expect("the section has a table");
    (header, header.matches('|').count())
}

/// A whole word of the text, bounded by anything but a letter, a digit, `-` or `_`.
fn holds_word(text: &str, word: &str) -> bool {
    let inside = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    text.match_indices(word).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + word.len()..].chars().next();
        !before.is_some_and(inside) && !after.is_some_and(inside)
    })
}

/// The rows of the judge round's four tables, each written from the scores, grouped by the
/// section that holds it ([`JUDGES_TABLES`], in order).
fn judges_rows(scores: &toml::Table) -> [Vec<String>; 4] {
    let int = |v: &toml::Value| v.as_integer().unwrap();
    let mut judges = Vec::new();
    let mut by_scope = Vec::new();
    let mut counts = Vec::new();
    for scope in scores["scopes"].as_array().expect("scopes") {
        let name = scope["name"].as_str().unwrap();
        for m in scope["measures"].as_array().unwrap() {
            let model = m["model"].as_str().unwrap();
            let cost = m["cost_per_decision_usd"].as_float().unwrap();
            if name == "all" {
                let s = &m["submissions"];
                judges.push(format!(
                    "| {model} | {} | {} | {} | {} | {} | {} ({}) | {} ({}) | {cost:.6} | {:.2} | {:.3} | {} | {} |",
                    int(&m["decisions"]),
                    judges_estimate(m.get("balanced_accuracy")),
                    judges_estimate(m.get("decided_correctly")),
                    int(&m["unsure"]),
                    int(&m["errors"]),
                    int(&s["failures"]),
                    judges_share(s.get("failure_rate")),
                    int(&s["first_rejections"]),
                    judges_share(s.get("first_rejection_rate")),
                    cost * 1_000.0,
                    cost / (cost + 0.04),
                    int(&m["median_ms"]),
                    judges_share(m.get("self_agreement")),
                ));
                for (counted_as, field) in [("class", "rejections_by_class"), ("cause", "causes")] {
                    for (what, count) in s
                        .get(field)
                        .and_then(toml::Value::as_table)
                        .into_iter()
                        .flatten()
                    {
                        counts.push(format!(
                            "| {model} | {counted_as} | {what} | {} |",
                            int(count)
                        ));
                    }
                }
            } else {
                by_scope.push(format!(
                    "| {name} | {model} | {} | {} | {} | {} | {} | {cost:.6} |",
                    int(&m["decisions"]),
                    judges_estimate(m.get("balanced_accuracy")),
                    judges_estimate(m.get("decided_correctly")),
                    int(&m["unsure"]),
                    int(&m["errors"]),
                ));
            }
        }
    }
    let mut clauses = Vec::new();
    for pick in scores["picks"].as_array().expect("picks") {
        let scope = pick["scope"].as_str().unwrap();
        for c in pick["judges"].as_array().unwrap() {
            clauses.push(format!(
                "| {scope} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {:.6} |",
                c["model"].as_str().unwrap(),
                yes(&c["errors_under_2_percent"]),
                yes(&c["decided_correctly_at_least_0_80"]),
                yes(&c["eligible"]),
                (c.get("difference_from_best_lower")
                    .and_then(toml::Value::as_float))
                .map_or_else(|| "n/a".to_owned(), |v| format!("{v:+.3}")),
                yes(&c["lower_bound_at_least_minus_3_points"]),
                yes(&c["median_time_at_most_twice_the_fastest"]),
                yes(&c["decided_correctly_within_5_points_of_the_best"]),
                yes(&c["within_the_margin"]),
                c["cost_per_decision_usd"].as_float().unwrap(),
            ));
        }
    }
    [judges, by_scope, counts, clauses]
}

#[test]
fn the_judges_record_holds_only_aggregates() {
    // Plan e1, task J1, item 3. Held always: the page names the matrices as committed, the
    // arm, the rule and what it is not, and holds no item, family, path or key.
    let page = std::fs::read_to_string(repository().join(JUDGES_PAGE))
        .expect("the judge round's page is committed");
    let block = page
        .split("## The round\n")
        .nth(1)
        .and_then(|rest| rest.split("```toml\n").nth(1))
        .and_then(|rest| rest.split("\n```").next())
        .expect("the page's block of the round");
    let round: toml::Table = block.parse().expect("the round's block is TOML");
    let named: Vec<&str> = (round["matrices"].as_array().unwrap().iter())
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(named, JUDGES_MATRICES);
    let stated: Vec<&str> = (round["matrix_identities"].as_array().unwrap().iter())
        .map(|v| v.as_str().unwrap())
        .collect();
    let outcome = round["outcome"].as_str().expect("an outcome");
    let mut identities = Vec::new();
    for (at, name) in JUDGES_MATRICES.iter().enumerate() {
        // The format round's matrix is on `main` once its round's record is; the judge
        // round's outcome is written only after it.
        let path = repository().join(format!("matrices/{name}.toml"));
        if !path.exists() && outcome == "pending" && *name == "e-submission" {
            identities.push(None);
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let matrix = whizbang_eval::matrix::Matrix::parse(&text).unwrap();
        assert_eq!(matrix.matrix.name, *name);
        assert_eq!(stated[at], &matrix.identity()[..16], "{name}");
        identities.push(Some(matrix.identity()));
    }
    assert_eq!(round["arm"].as_str(), Some("C;submission=v1"));
    assert_eq!(round["rule"].as_str(), Some("judge-pick/1"));
    for held in ["admits nothing", "descriptive", "exposed"] {
        assert!(page.contains(held), "{held}");
    }
    // The binaries that made the cells scored: the format round's and this round's. Each is
    // a commit of this tree once the round is written (the format round's is on `main` with
    // its record).
    let made_by: Vec<&str> = (round["runs_commits"]
        .as_array()
        .expect("runs_commits")
        .iter())
    .map(|v| v.as_str().unwrap())
    .collect();
    assert_eq!(made_by.len(), 2);
    for commit in &made_by {
        assert!(
            commit.len() == 40
                && commit
                    .bytes()
                    .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
            "{commit}"
        );
        if outcome != "pending" {
            assert!(is_ancestor(commit), "{commit}");
        }
    }
    // Each table under its own section, its header first, every row as wide as its header.
    for title in JUDGES_TABLES {
        let (header, bars) = table_header(section(&page, title));
        for line in section(&page, title).lines().filter(|l| l.starts_with('|')) {
            assert_eq!(
                line.matches('|').count(),
                bars,
                "{title}: {line} against {header}"
            );
        }
    }
    let scores_path = repository().join(JUDGES_SCORES);
    let mut texts = vec![page.clone()];
    if outcome == "pending" {
        // Before the round: no scores beside the page, and no row in its tables.
        assert!(
            !scores_path.exists(),
            "the scores are committed: write the round's outcome on the page"
        );
        assert_eq!(page.matches("| Pending |").count(), 4);
    } else {
        // After it: the scores are the round's, and the page is written from them.
        let scores_text = std::fs::read_to_string(&scores_path)
            .expect("the scores the page is written from are committed");
        let scores: toml::Table = scores_text.parse().expect("the scores are TOML");
        assert_eq!(scores["schema"].as_str(), Some("judges/1"));
        assert_eq!(scores["mode"].as_str(), Some("judges"));
        assert_eq!(scores["rule"].as_str(), Some("judge-pick/1"));
        assert_eq!(scores["arm"].as_str(), Some("C;submission=v1"));
        // The format round's 1,320 cells and this round's 220 and 715, all ended and
        // labelled.
        assert_eq!(scores["cells"].as_integer(), Some(2_255));
        assert_eq!(scores["open"].as_integer(), Some(0));
        assert_eq!(scores["unlabelled"].as_integer(), Some(0));
        assert!(scores.get("incomplete").is_none());
        let commit = scores["commit"].as_str().unwrap();
        assert!(is_ancestor(commit), "{commit}");
        assert_eq!(round["commit"].as_str(), Some(commit));
        let read = scores["matrices"].as_array().unwrap();
        assert_eq!(read.len(), 3);
        for ((m, name), identity) in read.iter().zip(JUDGES_MATRICES).zip(&identities) {
            assert_eq!(m["name"].as_str(), Some(name));
            assert_eq!(m["identity"].as_str(), identity.as_deref(), "{name}");
        }
        // Every judge under the one arm, five judges in every scope.
        for scope in scores["scopes"].as_array().unwrap() {
            let measures = scope["measures"].as_array().unwrap();
            assert_eq!(measures.len(), 5, "{:?}", scope["name"]);
            for m in measures {
                assert_eq!(m["arm"].as_str(), Some("C;submission=v1"));
            }
        }
        // The outcomes of both scopes, as the block and the page give them.
        let picks = scores["picks"].as_array().expect("picks");
        let outcome_section = page
            .split("## The outcome\n")
            .nth(1)
            .and_then(|rest| rest.split("\n## ").next())
            .unwrap();
        for (scope, suffix) in [("all", "all"), ("source-fidelity", "source_fidelity")] {
            let pick = (picks.iter())
                .find(|p| p["scope"].as_str() == Some(scope))
                .unwrap_or_else(|| panic!("the pick of {scope}"));
            let model = pick["model"].as_str().unwrap();
            assert_eq!(round[&format!("pick_{suffix}")].as_str(), Some(model));
            assert_eq!(
                round[&format!("outcome_{suffix}")].as_str(),
                pick["outcome"].as_str()
            );
            assert!(outcome_section.contains(model), "{scope}");
        }
        for name in ["started", "ended"] {
            assert!(round[name].is_str() || round[name].is_datetime(), "{name}");
        }
        let all = (scores["scopes"].as_array().unwrap().iter())
            .find(|s| s["name"].as_str() == Some("all"))
            .unwrap();
        let settled: f64 = (all["measures"].as_array().unwrap().iter())
            .map(|m| m["settled_usd"].as_float().unwrap())
            .sum();
        let stated = round["scored_settled_usd"].as_float().expect("settled");
        assert!((stated - settled).abs() < 0.005, "{stated} {settled}");
        assert!(round["round_settled_usd"].as_float().is_some());
        assert!(!page.contains("| Pending |"));
        assert!(!page.contains("Pending:"));
        for (title, rows) in JUDGES_TABLES.iter().zip(judges_rows(&scores)) {
            let held = section(&page, title);
            let (header, bars) = table_header(held);
            for row in &rows {
                assert_eq!(
                    row.matches('|').count(),
                    bars,
                    "{title}: {row} against {header}"
                );
            }
            let missing: Vec<&String> = rows
                .iter()
                .filter(|row| !held.contains(row.as_str()))
                .collect();
            assert!(
                missing.is_empty(),
                "the section {title} lacks the rows the scores give:\n{}",
                rows.join("\n")
            );
            // Nothing else in the table: as many rows as the scores give.
            let lines = held.lines().filter(|l| l.starts_with('|')).count();
            assert_eq!(lines, rows.len() + 2, "{title}");
        }
        texts.push(scores_text);
    }
    // No item, no family, no path, no key: in the page, and in the scores once there.
    for dir in whizbang_eval::sets::discover(&repository().join("evalsets")).unwrap() {
        let set = whizbang_eval::sets::Set::load(&dir).unwrap();
        let families = set.families_by_item();
        for text in &texts {
            for (item, _) in &set.jobs {
                assert!(!text.contains(item.as_str()), "{item}");
            }
            for family in families.values() {
                assert!(
                    !text.contains(&format!("`{family}`"))
                        && !text.contains(&format!("\"{family}\"")),
                    "{family}"
                );
                // A family's name in prose too, as a whole word, when it is long enough to
                // be told from an ordinary word.
                if family.len() >= 5 {
                    assert!(!holds_word(text, family), "{family}");
                }
            }
        }
    }
    for text in &texts {
        for held in [
            "/Users/",
            "/home/",
            "/private/",
            "/tmp/",
            "~/",
            "API_KEY",
            "_KEY=",
            "sk-ant",
            "chk:",
            "reason",
            "\"expected\"",
            "expected =",
        ] {
            assert!(!text.contains(held), "{held}");
        }
    }
}

/// The version of the first release, which the release's record names (plan m4, task N1).
/// A record of a later release names its own, in a test of its own.
const RELEASED: &str = "0.1.0";

/// The two targets of `SPEC.md` section 5, each with an archive in a release.
const TARGETS: [&str; 2] = ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"];

/// Every check the local leg must pass, by the name its receipt gives it (plan m4, task R2,
/// item 4), and the provider each paid check's requests must reach.
const REQUIRED_CHECKS: [(&str, Option<&str>); 8] = [
    ("model-tests", None),
    ("setup", None),
    ("mask", None),
    ("fill", Some("fal")),
    ("check-anthropic", Some("anthropic")),
    ("check-google", Some("google")),
    ("check-openai", Some("openai")),
    ("loop", Some("anthropic")),
];

/// The release's record, `docs/results/m4-release.md`, whole.
fn release_record() -> String {
    std::fs::read_to_string(repository().join("docs/results/m4-release.md"))
        .expect("the release's record is committed")
}

/// The text of the record's one fenced block of the language `language`, without its fences.
fn fenced(page: &str, language: &str) -> String {
    let opening = format!("```{language}\n");
    let blocks: Vec<&str> = page.split(opening.as_str()).skip(1).collect();
    assert_eq!(blocks.len(), 1, "one block of {language}");
    let (body, _) = blocks[0]
        .split_once("\n```")
        .unwrap_or_else(|| panic!("the block of {language} is closed"));
    body.to_owned()
}

/// The record's fields of the release, and its `SHA256SUMS` as each archive's name and
/// digest.
fn release_fields(page: &str) -> (toml::Table, Vec<(String, String)>) {
    let fields: toml::Table = fenced(page, "toml").parse().expect("the fields are TOML");
    let sums = fenced(page, "text")
        .lines()
        .map(|line| {
            let (digest, name) = line
                .split_once("  ")
                .expect("a line is a digest and a name");
            (name.to_owned(), digest.to_owned())
        })
        .collect();
    (fields, sums)
}

fn is_sha256(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

#[test]
fn the_release_record_names_a_candidate_whose_commit_is_of_this_tree() {
    // Plan m4, task N1, item 1.
    let page = release_record();
    let (fields, sums) = release_fields(&page);
    let text = |name: &str| -> &str {
        fields[name]
            .as_str()
            .unwrap_or_else(|| panic!("the record has no {name}"))
    };
    let names: Vec<&str> = fields.keys().map(String::as_str).collect();
    let mut expected = vec![
        "candidate",
        "candidates",
        "ci_run",
        "commit",
        "prerelease_run",
        "published",
        "receipt_sha256",
        "release_run",
        "version",
        "version_tag",
    ];
    expected.sort_unstable();
    assert_eq!(names, expected);
    // The version, its tag and the candidate's, all of one version, the one the
    // workspace's manifest named at the commit.
    assert_eq!(text("version"), RELEASED);
    assert_eq!(text("version_tag"), format!("v{RELEASED}"));
    let candidate = text("candidate");
    let number = candidate
        .strip_prefix(&format!("v{RELEASED}-rc."))
        .expect("a candidate of the version");
    assert!(number.parse::<u32>().is_ok_and(|n| n >= 1), "{candidate}");
    let commit = text("commit");
    assert!(is_ancestor(commit), "the commit {commit} is of this tree");
    let manifest = std::process::Command::new("git")
        .args(["show", &format!("{commit}:Cargo.toml")])
        .current_dir(repository())
        .output()
        .expect("git runs");
    assert!(manifest.status.success());
    let manifest = String::from_utf8(manifest.stdout).unwrap();
    assert!(
        manifest.contains(&format!("\nversion = \"{RELEASED}\"\n")),
        "the workspace's version at {commit} is {RELEASED}"
    );
    // Each run by its number; the receipt by its digest; published as a pre-release.
    for run in ["ci_run", "prerelease_run", "release_run"] {
        assert!(text(run).parse::<u64>().is_ok(), "{run}");
    }
    assert!(is_sha256(text("receipt_sha256")));
    assert_eq!(text("published"), "pre-release");
    // Every candidate tagged is in the table, each with its commit and how it ended; the
    // one that passed is the record's, and it is the last.
    let candidates = fields["candidates"].as_integer().expect("a count");
    let rows: Vec<&str> = (page.lines())
        .filter(|line| line.starts_with(&format!("| `v{RELEASED}-rc.")))
        .collect();
    assert_eq!(rows.len() as i64, candidates);
    let last = rows.last().expect("a candidate was tagged");
    assert!(last.starts_with(&format!("| `{candidate}` | `{commit}` | passed")));
    // The archives the release holds: one for each target, of the version, by digest.
    let named: Vec<String> = (TARGETS.iter())
        .map(|target| format!("whizbang-{RELEASED}-{target}.tar.gz"))
        .collect();
    let given: Vec<String> = sums.iter().map(|(name, _)| name.clone()).collect();
    assert_eq!(given, named);
    assert!(sums.iter().all(|(_, digest)| is_sha256(digest)));
    // What the gate said, and what was held after publication.
    for held in [
        "release-gate: passed",
        "byte for byte",
        "scripts/release-publish.sh check",
    ] {
        assert!(page.contains(held), "{held}");
    }
}

#[test]
fn the_release_record_holds_every_required_check_of_the_receipt_as_passed() {
    // Plan m4, task N1, item 1: the receipt the release gate accepted, as it was attached
    // to the candidate, byte for byte.
    let page = release_record();
    let (fields, sums) = release_fields(&page);
    let text = |name: &str| fields[name].as_str().unwrap().to_owned();
    let bytes = format!("{}\n", fenced(&page, "json"));
    assert_eq!(
        Digest::of_bytes(bytes.as_bytes()).to_string(),
        format!("sha256:{}", text("receipt_sha256")),
        "the receipt is held as it was attached"
    );
    let receipt: serde_json::Value = serde_json::from_str(&bytes).expect("the receipt is JSON");
    let at = |value: &serde_json::Value, name: &str| -> serde_json::Value {
        value
            .get(name)
            .cloned()
            .unwrap_or_else(|| panic!("no {name}"))
    };
    let string = |value: &serde_json::Value, name: &str| -> String {
        at(value, name)
            .as_str()
            .unwrap_or_else(|| panic!("{name} is a string"))
            .to_owned()
    };
    // Of this candidate, this commit and this version, with the archives the release holds.
    assert_eq!(string(&receipt, "schema"), "prerelease-receipt/1");
    assert_eq!(string(&receipt, "tag"), text("candidate"));
    assert_eq!(string(&receipt, "version"), RELEASED);
    assert_eq!(string(&receipt, "commit"), text("commit"));
    let number = text("candidate").rsplit('.').next().unwrap().to_owned();
    assert_eq!(receipt["candidate"].as_u64(), number.parse::<u64>().ok());
    for (target, (name, digest)) in TARGETS.iter().zip(&sums) {
        assert!(name.contains(target));
        assert_eq!(&string(&receipt["archives"], target), digest, "{target}");
    }
    assert_eq!(
        receipt["archives"].as_object().unwrap().len(),
        TARGETS.len()
    );
    assert!(is_sha256(&string(&receipt["binary"], "sha256")));
    assert_eq!(
        string(&receipt["binary"], "version_line"),
        format!("whizbang {RELEASED} (commit {})", text("commit"))
    );
    assert_eq!(receipt["store"]["new"], serde_json::Value::Bool(true));
    assert_eq!(string(&receipt["store"], "cache"), "off");

    // Every required check, exactly once, and passed by the conditions of its kind.
    let checks = receipt["checks"].as_array().expect("a list of checks");
    let given: Vec<String> = checks.iter().map(|check| string(check, "name")).collect();
    let required: Vec<&str> = REQUIRED_CHECKS.iter().map(|(name, _)| *name).collect();
    assert_eq!(given, required);
    let answered = |check: &serde_json::Value, provider: &str| {
        let requests = check["requests"].as_array().expect("its requests");
        assert!(
            requests.iter().any(|request| request["backend"] == provider
                && request["ended"] == "answered"
                && request["stage"] == "answered"),
            "a request to {provider} dispatched and settled with an answer"
        );
        assert_eq!(check["cache_hit"], serde_json::Value::Bool(false));
        assert!(!check["sent_to"].as_array().unwrap().is_empty());
    };
    let mut settled = 0;
    for (check, (name, provider)) in checks.iter().zip(REQUIRED_CHECKS) {
        assert_eq!(check["exit"].as_i64(), Some(0), "{name}");
        match name {
            "model-tests" => {
                let checkout = &check["checkout"];
                assert_eq!(checkout["new"], serde_json::Value::Bool(true));
                assert_eq!(checkout["clean"], serde_json::Value::Bool(true));
                assert_eq!(string(checkout, "commit"), text("commit"));
                assert!(!check["expected"].as_array().unwrap().is_empty());
                assert_eq!(check["ran"], check["expected"]);
                for none in ["failed", "timed_out"] {
                    assert!(check[none].as_array().unwrap().is_empty(), "{none}");
                }
                assert!(is_sha256(&string(check, "report_sha256")));
            }
            "setup" => {
                assert_eq!(string(check, "extra"), "torch");
                assert!(check["hello"].is_object());
                assert_eq!(check["probe_hello"], serde_json::Value::Bool(true));
            }
            "mask" => {
                assert_eq!(string(check, "status"), "completed");
                assert_eq!(string(check, "model"), "sam3");
            }
            "loop" => {
                assert_eq!(string(check, "status"), "completed");
                assert_eq!(check["plan_checks"], serde_json::Value::Bool(true));
                assert!(
                    check["kept"]["unchecked"].is_null(),
                    "the kept attempt is checked"
                );
                let child = &check["check"];
                assert_eq!(string(child, "status"), "completed");
                assert!(["pass", "fail", "unsure"].contains(&string(child, "verdict").as_str()));
                answered(child, provider.unwrap());
                let requests = child["requests"].as_array().unwrap().iter();
                settled += requests
                    .map(|request| request["settled_micros"].as_i64().unwrap())
                    .sum::<i64>();
            }
            _ => {
                assert_eq!(string(check, "status"), "completed");
                assert_eq!(string(check, "backend"), provider.unwrap());
                answered(check, provider.unwrap());
                if name.starts_with("check-") {
                    let verdict = string(check, "verdict");
                    assert!(["pass", "fail", "unsure"].contains(&verdict.as_str()));
                }
                settled += check["settled_micros"].as_i64().unwrap();
            }
        }
    }
    // What the receipt itemises (the fill, the three checks and the requests of the loop's
    // check) is what the record says the leg spent; the rest is said to be an estimate.
    let dollars = format!("{}.{:06}", settled / 1_000_000, settled % 1_000_000);
    assert!(page.contains(&dollars), "the record states {dollars}");
    assert!(page.contains("estimate"));
    // No path and no key anywhere in the record.
    for held in [
        "/Users/",
        "/home/",
        "/private/",
        "/tmp/",
        "FAL_KEY",
        "API_KEY",
        "_KEY=",
    ] {
        assert!(!page.contains(held), "{held}");
    }
}

/// The page of the format round (plan e1, task F2, item 3) and the `judges/1` it is
/// written from.
const SUBMISSION_PAGE: &str = "docs/results/e-submission.md";
const SUBMISSION_SCORES: &str = "docs/results/e-submission.toml";

/// A rate or a share as the format round's page writes it: three places, or `n/a`.
fn share(value: Option<&toml::Value>) -> String {
    value
        .and_then(toml::Value::as_float)
        .map_or_else(|| "n/a".to_owned(), |v| format!("{v:.3}"))
}

/// A count and its rate, as `3 (0.014)`.
fn counted(count: &toml::Value, rate: Option<&toml::Value>) -> String {
    format!("{} ({})", count.as_integer().unwrap(), share(rate))
}

/// The headings of the format round's three tables, in the page's order.
const SUBMISSION_TABLES: [&str; 3] = [
    "## Per judge, before and after",
    "## Rejections, causes and recoveries",
    "## The clauses of each judge",
];

/// The rows of the format round's three tables, each written from the scores' scope `all`
/// and the rule's clauses: one group a table, in the order of [`SUBMISSION_TABLES`].
fn submission_rows(scores: &toml::Table) -> [Vec<String>; 3] {
    let scopes = scores["scopes"].as_array().expect("scopes");
    let all = (scopes.iter())
        .find(|s| s["name"].as_str() == Some("all"))
        .expect("the scope all");
    let mut measures = Vec::new();
    let mut counts = Vec::new();
    for m in all["measures"].as_array().unwrap() {
        let s = &m["submissions"];
        let (model, arm) = (m["model"].as_str().unwrap(), m["arm"].as_str().unwrap());
        let interval = m.get("balanced_accuracy").map_or_else(
            || "n/a".to_owned(),
            |b| format!("{} to {}", share(b.get("lower")), share(b.get("upper"))),
        );
        measures.push(format!(
            "| {model} | {arm} | {} | {} | {} | {} | {} | {interval} | {:.6} | {} |",
            s["decisions"].as_integer().unwrap(),
            counted(&s["failures"], s.get("failure_rate")),
            counted(&s["first_rejections"], s.get("first_rejection_rate")),
            s["unknown"].as_integer().unwrap(),
            share(m.get("balanced_accuracy").and_then(|b| b.get("point"))),
            m["cost_per_decision_usd"].as_float().unwrap(),
            share(m.get("self_agreement")),
        ));
        for (counted_as, field) in [
            ("class", "rejections_by_class"),
            ("also", "also_by_class"),
            ("cause", "causes"),
            ("recovery", "recoveries_by_kind"),
        ] {
            for (name, count) in s
                .get(field)
                .and_then(toml::Value::as_table)
                .into_iter()
                .flatten()
            {
                counts.push(format!(
                    "| {model} | {arm} | {counted_as} | {name} | {} |",
                    count.as_integer().unwrap()
                ));
            }
        }
    }
    let change = |c: &toml::Value, name: &str| -> String {
        c.get(name)
            .and_then(toml::Value::as_float)
            .map_or_else(|| "n/a".to_owned(), |v| format!("{v:+.3}"))
    };
    let clauses = (scores["format"]["judges"].as_array().unwrap().iter()).map(|c| {
        format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            c["model"].as_str().unwrap(),
            c["control_failures"].as_integer().unwrap(),
            c["treatment_failures"].as_integer().unwrap(),
            if c["no_new_failures"].as_bool().unwrap() {
                "yes"
            } else {
                "no"
            },
            change(c, "balanced_accuracy_change"),
            change(c, "false_reject_rate_change"),
            change(c, "false_accept_rate_change"),
            if c["accuracy_kept"].as_bool().unwrap() {
                "yes"
            } else {
                "no"
            },
        )
    });
    [measures, counts, clauses.collect()]
}

#[test]
fn the_submission_record_holds_only_aggregates() {
    // Plan e1, task F2, item 3. Held always: the page names the matrix as committed, its
    // rule, and what it is not, and holds no item, family, path or key.
    let page = std::fs::read_to_string(repository().join(SUBMISSION_PAGE))
        .expect("the format round's page is committed");
    let text = std::fs::read_to_string(repository().join("matrices/e-submission.toml")).unwrap();
    let matrix = whizbang_eval::matrix::Matrix::parse(&text).unwrap();
    let block = page
        .split("## The round\n")
        .nth(1)
        .and_then(|rest| rest.split("```toml\n").nth(1))
        .and_then(|rest| rest.split("\n```").next())
        .expect("the page's block of the round");
    let round: toml::Table = block.parse().expect("the round's block is TOML");
    assert_eq!(round["matrix"].as_str(), Some("e-submission"));
    assert_eq!(
        round["matrix_identity"].as_str(),
        Some(&matrix.identity()[..16])
    );
    assert_eq!(round["rule"].as_str(), Some("submission/1"));
    for held in ["admits nothing", "descriptive"] {
        assert!(page.contains(held), "{held}");
    }
    let scores_path = repository().join(SUBMISSION_SCORES);
    let outcome = round["outcome"].as_str().expect("an outcome");
    let mut texts = vec![page.clone()];
    if outcome == "pending" {
        // Before the round: no scores beside the page, and no row in its tables.
        assert!(
            !scores_path.exists(),
            "the scores are committed: write the round's outcome on the page"
        );
        assert_eq!(page.matches("| Pending |").count(), 3);
    } else {
        // After it: the scores are the round's, and the page is written from them.
        let scores_text = std::fs::read_to_string(&scores_path)
            .expect("the scores the page is written from are committed");
        let scores: toml::Table = scores_text.parse().expect("the scores are TOML");
        assert_eq!(scores["schema"].as_str(), Some("judges/1"));
        assert_eq!(scores["mode"].as_str(), Some("arms"));
        assert_eq!(scores["rule"].as_str(), Some("submission/1"));
        assert_eq!(scores["arm"].as_str(), Some("C;submission=v1"));
        assert_eq!(scores["treatment"].as_str(), Some("C;submission=v2"));
        assert_eq!(scores["cells"].as_integer(), Some(1_320));
        let commit = scores["commit"].as_str().unwrap();
        assert!(is_ancestor(commit), "{commit}");
        assert_eq!(round["commit"].as_str(), Some(commit));
        let runs_commit = round["runs_commit"].as_str().expect("runs_commit");
        assert!(is_ancestor(runs_commit), "{runs_commit}");
        let named = scores["matrices"].as_array().unwrap();
        assert_eq!(named.len(), 1);
        assert_eq!(named[0]["name"].as_str(), Some("e-submission"));
        assert_eq!(
            named[0]["identity"].as_str(),
            Some(matrix.identity().as_str())
        );
        let format = scores["format"].as_table().expect("the rule's outcome");
        for name in ["outcome", "settled_by", "format"] {
            assert_eq!(round[name].as_str(), format[name].as_str(), "{name}");
        }
        for name in ["started", "ended"] {
            assert!(round[name].is_str() || round[name].is_datetime(), "{name}");
        }
        let all = (scores["scopes"].as_array().unwrap().iter())
            .find(|s| s["name"].as_str() == Some("all"))
            .unwrap();
        let settled: f64 = (all["measures"].as_array().unwrap().iter())
            .map(|m| m["settled_usd"].as_float().unwrap())
            .sum();
        let stated = round["settled_usd"].as_float().expect("settled_usd");
        assert!((stated - settled).abs() < 0.005, "{stated} {settled}");
        // The decision on the page names its clause.
        let decision = page
            .split("## The decision\n")
            .nth(1)
            .and_then(|rest| rest.split("\n## ").next())
            .unwrap();
        assert!(decision.contains(format["settled_by"].as_str().unwrap()));
        assert!(!page.contains("| Pending |"));
        // Each table holds exactly the rows the scores give it, under its own heading, each
        // row with as many columns as its header.
        for (heading, rows) in SUBMISSION_TABLES.iter().zip(submission_rows(&scores)) {
            let section = page
                .split(&format!("{heading}\n"))
                .nth(1)
                .and_then(|rest| rest.split("\n## ").next())
                .unwrap_or_else(|| panic!("the page has no {heading}"));
            let table: Vec<&str> = section.lines().filter(|l| l.starts_with('|')).collect();
            assert!(table.len() >= 2, "{heading}: no table");
            let columns = table[0].matches('|').count();
            for line in &table {
                assert_eq!(line.matches('|').count(), columns, "{heading}: {line}");
            }
            let wanted: Vec<&str> = rows.iter().map(String::as_str).collect();
            assert!(
                table[2..] == wanted[..],
                "{heading}: the table is not the rows the scores give:\n{}",
                rows.join("\n")
            );
        }
        texts.push(scores_text);
    }
    // No item, no family, no path, no key: in the page, and in the scores once there.
    for dir in whizbang_eval::sets::discover(&repository().join("evalsets")).unwrap() {
        let set = whizbang_eval::sets::Set::load(&dir).unwrap();
        let families = set.families_by_item();
        for text in &texts {
            for (item, _) in &set.jobs {
                assert!(!text.contains(item.as_str()), "{item}");
            }
            for family in families.values() {
                assert!(
                    !text.contains(&format!("`{family}`"))
                        && !text.contains(&format!("\"{family}\"")),
                    "{family}"
                );
            }
        }
    }
    for text in &texts {
        for held in [
            "/Users/",
            "/home/",
            "/private/",
            "/tmp/",
            "~/",
            "API_KEY",
            "_KEY=",
            "sk-ant",
            "chk:",
            "reason",
        ] {
            assert!(!text.contains(held), "{held}");
        }
    }
}

/// The record of the loop by hand in the playground (plan m5, task E1, items 1 and 2).
const PLAYGROUND_PAGE: &str = "docs/results/m5-playground.md";

/// The cases of the playground's record, each with the fields it holds and no other.
const PLAYGROUND_CASES: [(&str, &[&str]); 6] = [
    (
        "fail",
        &[
            "attempts",
            "outcome",
            "reserved_micros",
            "seconds",
            "verdict",
        ],
    ),
    (
        "pass",
        &[
            "attempts",
            "outcome",
            "reserved_micros",
            "seconds",
            "verdict",
        ],
    ),
    (
        "vague",
        &["checked", "outcome", "reserved_micros", "seconds", "sent"],
    ),
    (
        "unchecked",
        &[
            "attempts",
            "checked",
            "outcome",
            "reserved_micros",
            "seconds",
        ],
    ),
    ("no-key", &["sent"]),
    (
        "quit",
        &[
            "open_reservations",
            "outcome",
            "reserved_micros",
            "seconds_into_fill",
            "store_took_a_writer",
            "unresolved_micros",
        ],
    ),
];

#[test]
fn the_playgrounds_record_holds_only_aggregates_and_run_ids() {
    // The orchestrator's live runs, committed: each case its run id (or none, where no run
    // was started), what the run settled, and the claims of its case; the totals the sum.
    let page = std::fs::read_to_string(repository().join(PLAYGROUND_PAGE))
        .expect("the playground's record is committed");
    let block = |heading: &str| -> toml::Table {
        page.split(&format!("{heading}\n\n```toml\n"))
            .nth(1)
            .and_then(|rest| rest.split("\n```").next())
            .unwrap_or_else(|| panic!("the page has no block under {heading}"))
            .parse()
            .unwrap_or_else(|_| panic!("the block under {heading} is TOML"))
    };
    let whole = |fields: &toml::Table, name: &str| -> i64 {
        fields[name]
            .as_integer()
            .unwrap_or_else(|| panic!("{name} is a whole number"))
    };
    let (mut runs, mut settled, mut unresolved) = (0_i64, 0_i64, 0_i64);
    for (case, own) in PLAYGROUND_CASES {
        let fields = block(&format!("### `{case}`"));
        let mut names: Vec<&str> = fields.keys().map(String::as_str).collect();
        names.sort_unstable();
        let mut wanted: Vec<&str> = ["case", "run", "settled_micros"].to_vec();
        wanted.extend(own);
        wanted.sort_unstable();
        assert_eq!(names, wanted, "{case}: its fields");
        assert_eq!(fields["case"].as_str(), Some(case));
        let spent = whole(&fields, "settled_micros");
        assert!(spent >= 0, "{case}");
        settled += spent;
        let run = fields["run"].as_str().expect("a run id, or none");
        if run == "none" {
            // No run was started, so nothing was sent or spent.
            assert_eq!(case, "no-key");
            assert_eq!(fields["sent"].as_str(), Some("nothing"));
            assert_eq!(spent, 0);
            continue;
        }
        runs += 1;
        let (at, count) = run.split_once('-').expect("a run id is <ms>-<count>");
        for part in [at, count] {
            assert!(
                !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()),
                "{case}: {run}"
            );
        }
        if let Some(reserved) = fields.get("reserved_micros") {
            assert!(spent <= reserved.as_integer().unwrap(), "{case}");
        }
        match case {
            "fail" | "pass" => {
                assert_eq!(fields["outcome"].as_str(), Some("completed"));
                assert_eq!(fields["verdict"].as_str(), Some(case));
                assert!((1..=3).contains(&whole(&fields, "attempts")), "{case}");
            }
            "unchecked" => {
                // No judge ready: one fill, not checked, and the run completed.
                assert_eq!(fields["outcome"].as_str(), Some("completed"));
                assert_eq!(fields["checked"].as_bool(), Some(false));
                assert_eq!(whole(&fields, "attempts"), 1);
                assert!(spent > 0, "the fill was paid for");
            }
            "vague" => {
                // Found nothing: not checked, nothing sent, nothing spent.
                assert_eq!(fields["outcome"].as_str(), Some("error"));
                assert_eq!(fields["checked"].as_bool(), Some(false));
                assert_eq!(fields["sent"].as_str(), Some("nothing"));
                assert_eq!(spent, 0);
            }
            _ => {
                assert_eq!(fields["outcome"].as_str(), Some("cancelled"));
                assert_eq!(whole(&fields, "open_reservations"), 0);
                assert_eq!(fields["store_took_a_writer"].as_bool(), Some(true));
                unresolved += whole(&fields, "unresolved_micros");
            }
        }
    }
    let total = block("## The runs");
    assert!(is_ancestor(total["commit"].as_str().unwrap()));
    assert_eq!(total["whizbang"].as_str(), Some(LOOP_MEASURED_BY));
    assert_eq!(whole(&total, "runs"), runs);
    assert_eq!(whole(&total, "settled_micros"), settled);
    assert_eq!(whole(&total, "unresolved_micros"), unresolved);
    for held in [
        "It scores nothing",
        "kept out of the repository",
        "## Found live",
    ] {
        assert!(page.contains(held), "{held}");
    }
    // Aggregates and run ids only: no item of the set or its phrase, no picture, no path,
    // no key, no token.
    let manifest: toml::Table =
        std::fs::read_to_string(repository().join("labs/e-loop/segment-01.toml"))
            .unwrap()
            .parse()
            .unwrap();
    for item in manifest["item"].as_array().unwrap() {
        for name in ["id", "phrase", "image"] {
            let held = item[name].as_str().unwrap();
            assert!(!page.contains(held), "{held}");
        }
    }
    for held in [
        "/Users/",
        "/home/",
        "/private/",
        "/tmp/",
        "~/",
        "FAL_KEY",
        "API_KEY",
        "_KEY=",
        "#k=",
        ".png",
        ".jpg",
        ".webp",
        "sha256:",
    ] {
        assert!(!page.contains(held), "{held}");
    }
}

/// The loop round's record (plan e1, task L2), the tallies `scripts/loop-evidence.sh`
/// wrote beside it, and the scores `whizbang eval loop score` writes once the round is
/// scored.
const LOOP_ROUND_PAGE: &str = "docs/results/e-loop.md";
const LOOP_ROUND_MANIFEST: &str = "labs/e-loop/segment-01.toml";
/// The judges of the round, the loop's own first (ADR 0044), then milestone 4's first.
const LOOP_ROUND_JUDGES: [&str; 2] = ["gemini-4-argon", "claude-haiku-5-5"];
const LOOP_ROUND_REPETITIONS: [i64; 2] = [1, 2];
/// Twice the plan's estimate of the round (plan e1, "Spend": a round that passes twice its
/// estimate stops and is said).
const LOOP_ROUND_MOST_MICROS: i64 = 10_000_000;
/// The rows of the scored tables: each group in the manifest's order, then the whole round.
const LOOP_ROUND_ALL: &str = "all";

/// The integer field `name` of a table, or of the table `table` in it.
fn integer(tally: &toml::Table, table: Option<&str>, name: &str) -> i64 {
    let fields = match table {
        Some(table) => tally[table].as_table().unwrap_or_else(|| panic!("{table}")),
        None => tally,
    };
    fields[name]
        .as_integer()
        .unwrap_or_else(|| panic!("{table:?} {name}"))
}

/// Dollars of a count of micro-dollars, to the micro-dollar.
fn dollars(micros: i64) -> String {
    format!("{}.{:06}", micros / 1_000_000, micros % 1_000_000)
}

/// The rows of the page's table under `heading`, its header and divider left out, each row
/// with as many columns as its header.
fn rows_under<'a>(page: &'a str, heading: &str) -> Vec<&'a str> {
    let section = page
        .split(&format!("{heading}\n"))
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .unwrap_or_else(|| panic!("the page has no {heading}"));
    let table: Vec<&str> = section.lines().filter(|l| l.starts_with('|')).collect();
    assert!(table.len() >= 2, "{heading}: no table");
    let columns = table[0].matches('|').count();
    for line in &table {
        assert_eq!(line.matches('|').count(), columns, "{heading}: {line}");
    }
    table[2..].to_vec()
}

#[test]
fn the_loop_records_hold_only_aggregates_by_group() {
    // Plan e1, task L2, items 1 and 3. The manifest the round ran: 20 entries in four
    // groups.
    let manifest_text = std::fs::read_to_string(repository().join(LOOP_ROUND_MANIFEST)).unwrap();
    let manifest = whizbang::loop_score::Manifest::parse(&manifest_text).unwrap();
    let groups = manifest.groups();
    assert_eq!(manifest.item.len(), 20);
    // In the manifest's order of groups.
    let mut order: Vec<&str> = Vec::new();
    for entry in &manifest.item {
        if !order.contains(&entry.group.as_str()) {
            order.push(&entry.group);
        }
    }
    let page = std::fs::read_to_string(repository().join(LOOP_ROUND_PAGE))
        .expect("the loop round's record is committed");
    for held in [
        "admits nothing",
        "descriptive",
        "not about the finished picture",
    ] {
        assert!(page.contains(held), "{held}");
    }
    let mut texts = vec![page.clone()];

    // Each tally: of this tree, of the manifest, every entry accounted for, and each count
    // adding up to the runs and attempts it counts.
    let results = repository().join("docs/results");
    let mut tallies: Vec<(&str, i64, toml::Table)> = Vec::new();
    for judge in LOOP_ROUND_JUDGES {
        for repetition in LOOP_ROUND_REPETITIONS {
            let file = results.join(format!("e-loop-{judge}-{repetition}.toml"));
            let text = std::fs::read_to_string(&file).expect("each tally is committed");
            let tally: toml::Table = text.parse().expect("a tally is TOML");
            let named = format!("{judge} {repetition}");
            assert_eq!(tally["schema"].as_str(), Some("loop-evidence/1"));
            assert!(is_ancestor(tally["commit"].as_str().unwrap()), "{named}");
            assert_eq!(tally["whizbang"].as_str(), Some(LOOP_MEASURED_BY));
            assert_eq!(tally["judge"].as_str(), Some(judge));
            assert_eq!(tally["kind"].as_str(), Some("segment-01"));
            assert_eq!(integer(&tally, None, "repetition"), repetition);
            assert!(tally["judge_model"].as_str().unwrap().contains(judge));
            assert_eq!(
                tally["manifest_sha256"].as_str(),
                Some(digest_of(LOOP_ROUND_MANIFEST).as_str())
            );
            let runs = integer(&tally, None, "runs");
            assert_eq!(integer(&tally, None, "entries"), 20);
            assert_eq!(runs + integer(&tally, None, "not_run"), 20, "{named}");
            let sum = |name: &str| -> i64 {
                let fields = tally[name].as_table().unwrap_or_else(|| panic!("{name}"));
                fields.values().filter_map(toml::Value::as_integer).sum()
            };
            for counted in ["outcomes", "ended", "stopped_by_row", "stopped_by_reason"] {
                assert_eq!(sum(counted), runs, "{named}: {counted}");
            }
            let attempts = integer(&tally, None, "attempts_made");
            assert_eq!(sum("attempt_verdicts"), attempts, "{named}");
            let made = |n: &str| integer(&tally, Some("runs_by_attempts_made"), n);
            let (one, two, three) = (made("made_1"), made("made_2"), made("made_3"));
            assert_eq!(one + 2 * two + 3 * three, attempts, "{named}");
            assert_eq!(sum("retries"), two + 2 * three, "{named}");
            // By group: the manifest's groups, whose runs and retried runs add up.
            let by_group = tally["group"].as_table().expect("the tally by group");
            assert_eq!(
                by_group.keys().cloned().collect::<Vec<_>>(),
                groups.keys().cloned().collect::<Vec<_>>()
            );
            let (mut group_runs, mut group_retried, mut group_attempts) = (0, 0, 0);
            for (group, fields) in by_group {
                let fields = fields.as_table().unwrap();
                let field = |n: &str| fields[n].as_integer().unwrap();
                assert_eq!(field("runs") as usize, groups[group], "{named} {group}");
                group_runs += field("runs");
                group_retried += field("retried");
                group_attempts += field("attempts_made");
            }
            assert_eq!(group_runs, runs, "{named}");
            assert_eq!(group_retried, two + three, "{named}");
            assert_eq!(group_attempts, attempts, "{named}");
            texts.push(text);
            tallies.push((judge, repetition, tally));
        }
    }

    // The page's tables are the tallies' numbers, row for row.
    let of_judge = |judge: &str| -> Vec<&toml::Table> {
        (tallies.iter())
            .filter(|t| t.0 == judge)
            .map(|t| &t.2)
            .collect()
    };
    let pooled = |judge: &str, table: Option<&str>, name: &str| -> i64 {
        (of_judge(judge).iter())
            .map(|t| integer(t, table, name))
            .sum()
    };
    let retried = |t: &toml::Table| {
        integer(t, Some("runs_by_attempts_made"), "made_2")
            + integer(t, Some("runs_by_attempts_made"), "made_3")
    };
    let wanted: Vec<String> = (tallies.iter())
        .map(|(judge, repetition, t)| {
            format!(
                "| `{judge}` | {repetition} | {} | {} | {} | {} | {} |",
                integer(t, None, "runs"),
                retried(t),
                integer(t, Some("retries"), "wider"),
                integer(t, Some("retries"), "located"),
                integer(t, None, "attempts_made"),
            )
        })
        .collect();
    assert_eq!(rows_under(&page, "## How many runs retried"), wanted);

    let wanted: Vec<String> = (order.iter())
        .map(|group| {
            let in_group = |t: &toml::Table, name: &str| -> i64 {
                t["group"][*group][name].as_integer().unwrap()
            };
            let runs: Vec<i64> = (LOOP_ROUND_JUDGES.iter())
                .map(|j| of_judge(j).iter().map(|t| in_group(t, "runs")).sum())
                .collect();
            assert_eq!(runs[0], runs[1], "{group}");
            let cells: Vec<String> = (LOOP_ROUND_JUDGES.iter())
                .map(|j| {
                    let r: i64 = of_judge(j).iter().map(|t| in_group(t, "retried")).sum();
                    format!("{r} of {}", runs[0])
                })
                .collect();
            format!("| {group} | {} | {} |", cells[0], cells[1])
        })
        .collect();
    assert_eq!(rows_under(&page, "## Retried, by group"), wanted);

    let wanted: Vec<String> = (LOOP_ROUND_JUDGES.iter())
        .map(|judge| {
            let ended = |n: &str| pooled(judge, Some("ended"), n);
            format!(
                "| `{judge}` | {} | {} | {} | {} | {} | {} |",
                ended("passed_at_first"),
                ended("passed_after_retry"),
                ended("kept_unsure"),
                ended("kept_fail"),
                ended("unchecked"),
                ended("error"),
            )
        })
        .collect();
    assert_eq!(rows_under(&page, "## How the runs ended"), wanted);

    let reasons = tallies[0].2["stopped_by_reason"].as_table().unwrap();
    let wanted: Vec<String> = (reasons.keys())
        .filter_map(|reason| {
            let counts: Vec<i64> = (LOOP_ROUND_JUDGES.iter())
                .map(|j| pooled(j, Some("stopped_by_reason"), reason))
                .collect();
            (counts.iter().any(|&c| c > 0))
                .then(|| format!("| `{reason}` | {} | {} |", counts[0], counts[1]))
        })
        .collect();
    assert_eq!(rows_under(&page, "## Why the attempts stopped"), wanted);

    let wanted: Vec<String> = (tallies.iter())
        .map(|(judge, repetition, t)| {
            format!(
                "| `{judge}` | {repetition} | {} |",
                dollars(integer(t, None, "settled_micros"))
            )
        })
        .collect();
    assert_eq!(rows_under(&page, "## Spend"), wanted);
    let settled: i64 = (tallies.iter())
        .map(|t| integer(&t.2, None, "settled_micros"))
        .sum();
    assert!(settled <= LOOP_ROUND_MOST_MICROS, "{settled}");
    assert!(
        page.contains(&format!("{} dollars settled in all", dollars(settled))),
        "the page states the round's whole spend"
    );

    // The scores, one for each judge, of this manifest, over both repetitions, every run
    // requested and none missing.
    let mut marked = false;
    let mut scores: Vec<(&str, toml::Table)> = Vec::new();
    for judge in LOOP_ROUND_JUDGES {
        let path = results.join(format!("e-loop-score-{judge}.toml"));
        let text = std::fs::read_to_string(&path).expect("each judge's score is committed");
        let score: toml::Table = text.parse().expect("a score is TOML");
        assert_eq!(score["schema"].as_str(), Some("loop-score/1"));
        assert_eq!(score["judge"].as_str(), Some(judge));
        assert_eq!(score["set"].as_str(), Some(manifest.set.as_str()));
        assert_eq!(
            score["images_rev"].as_str(),
            Some(manifest.images_rev.as_str())
        );
        assert_eq!(
            score["manifest_sha256"].as_str(),
            Some(digest_of(LOOP_ROUND_MANIFEST).as_str())
        );
        assert_eq!(score["entries"].as_integer(), Some(20));
        assert_eq!(score["repetitions"].as_integer(), Some(2));
        assert_eq!(score["under_covered_below"].as_float(), Some(0.95));
        assert_eq!(score["all"]["requested"].as_integer(), Some(40));
        assert_eq!(score["all"]["missing"].as_integer(), Some(0));
        let by_group = score["group"].as_table().expect("the score by group");
        assert_eq!(
            by_group.keys().cloned().collect::<Vec<_>>(),
            groups.keys().cloned().collect::<Vec<_>>()
        );
        // The score's runs by group are the tallies'.
        for (group, fields) in by_group {
            let runs: i64 = (of_judge(judge).iter())
                .map(|t| t["group"][group.as_str()]["runs"].as_integer().unwrap())
                .sum();
            assert_eq!(fields["runs"].as_integer(), Some(runs), "{judge} {group}");
        }
        marked |= score["marked"].as_bool().unwrap();
        texts.push(text);
        scores.push((judge, score));
    }

    // The scored tables are the scores' numbers, row for row: for each judge, each group
    // in the manifest's order and then the whole round.
    let mut names: Vec<&str> = order.clone();
    names.push(LOOP_ROUND_ALL);
    let of = |score: &toml::Table, name: &str| -> toml::Table {
        let value = match name {
            LOOP_ROUND_ALL => &score["all"],
            group => &score["group"][group],
        };
        value.as_table().unwrap().clone()
    };
    let count =
        |table: &toml::Table, part: &str, name: &str| -> i64 { integer(table, Some(part), name) };
    let mean = |table: &toml::Table, part: &str| -> String {
        match table[part]
            .get("coverage_mean")
            .and_then(toml::Value::as_float)
        {
            Some(mean) => format!("{mean:.3}"),
            None => "none".to_owned(),
        }
    };
    let each = |row: &dyn Fn(&str, &str, &toml::Table) -> String| -> Vec<String> {
        let mut rows = Vec::new();
        for (judge, score) in &scores {
            for name in &names {
                rows.push(row(judge, name, &of(score, name)));
            }
        }
        rows
    };
    let wanted = each(&|judge, name, g| {
        let r = |n: &str| count(g, "coverage_across_a_retry", n);
        format!(
            "| `{judge}` | {name} | {} | {} | {} | {} | {} | {} | {} |",
            r("compared"),
            r("coverage_up"),
            r("coverage_within"),
            r("coverage_down"),
            r("size_larger"),
            r("size_within"),
            r("size_smaller"),
        )
    });
    assert_eq!(rows_under(&page, "## Mask coverage across a retry"), wanted);
    let wanted = each(&|judge, name, g| {
        let h = |n: &str| count(g, "kept_had_the_highest_coverage", n);
        format!(
            "| `{judge}` | {name} | {} | {} | {} | {} | {} | {} | {} of {} |",
            count(g, "first_shaped_mask", "attempts"),
            mean(g, "first_shaped_mask"),
            count(g, "first_shaped_mask", "under_covered"),
            count(g, "kept_shaped_mask", "attempts"),
            mean(g, "kept_shaped_mask"),
            count(g, "kept_shaped_mask", "under_covered"),
            h("highest"),
            h("runs"),
        )
    });
    assert_eq!(
        rows_under(&page, "## Mask coverage of the first and the kept attempt"),
        wanted
    );
    let wanted = each(&|judge, name, g| {
        let q = |n: &str| count(g, "rq1_against_under_coverage", n);
        format!(
            "| `{judge}` | {name} | {} | {} | {} | {} | {} | {} | {} |",
            q("checked"),
            q("confident_violation_under_covered"),
            q("confident_violation_covered"),
            q("tentative_violation_under_covered"),
            q("tentative_violation_covered"),
            q("no_violation_under_covered"),
            q("no_violation_covered"),
        )
    });
    assert_eq!(
        rows_under(
            &page,
            "## The judge's `RQ1` answer against mask under-coverage"
        ),
        wanted
    );
    let wanted = each(&|judge, name, g| {
        let why = g["ended_under_covered_by_reason"].as_table().unwrap();
        let why: Vec<String> = (why.iter())
            .map(|(reason, n)| format!("`{reason}` {}", n.as_integer().unwrap()))
            .collect();
        format!(
            "| `{judge}` | {name} | {} | {} | {} |",
            count(g, "kept_shaped_mask", "attempts"),
            integer(g, None, "ended_under_covered"),
            if why.is_empty() {
                "none".to_owned()
            } else {
                why.join(", ")
            },
        )
    });
    assert_eq!(
        rows_under(&page, "## The runs that ended under-covered, and why"),
        wanted
    );
    assert!(!page.contains("Pending"));
    // Without the owner's marks, nothing is said of the finished pictures.
    if !marked {
        assert!(page.contains("makes no claim about the finished pictures"));
    }

    // Aggregates only: no item, phrase or picture of the set, no path, no key.
    for text in &texts {
        for entry in &manifest.item {
            for held in [&entry.id, &entry.phrase, &entry.image, &entry.truth] {
                assert!(!text.contains(held.as_str()), "{held}");
            }
        }
        for held in [
            "/Users/",
            "/home/",
            "/private/",
            "/tmp/",
            "~/",
            "API_KEY",
            "_KEY=",
            ".png",
        ] {
            assert!(!text.contains(held), "{held}");
        }
    }
}
