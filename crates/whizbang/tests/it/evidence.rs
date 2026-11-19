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
    // No secret, no path and no reply: no key's form, no header, no absolute path, no
    // checklist item's name and no reason.
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
    let cells: Vec<&Fields> = blocks[1..]
        .iter()
        .filter(|b| b.iter().any(|(n, _)| n == "arm"))
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
    // Spend within the plan's estimate for U1.
    assert!(number(run, "settled_micros") <= 31_000_000);
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
