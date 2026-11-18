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
        assert_eq!(
            field(fields, "action"),
            format!("check {}", whizbang_squint::check::VERSION)
        );
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
