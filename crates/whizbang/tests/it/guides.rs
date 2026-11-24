//! `guides::…`: what a guide states is what the build would do (plan m4, task L1, item
//! 7): the figures of `docs/guides/loop.md` are worked out here from the price tables
//! the backends hold, and no call is made.

use whizbang_backends::service::spend::{DEFAULT_CEILINGS, Prices, bound_micros};
use whizbang_backends::{anthropic, fal, google, openai};
use whizbang_squint::check::TURNS_LIMIT;

/// Millionths of a US dollar as the guide writes them: dollars, to six places.
fn usd(micros: u64) -> String {
    format!("{}.{:06}", micros / 1_000_000, micros % 1_000_000)
}

#[test]
fn the_loops_guide_states_the_bound_of_an_attempt_for_each_judge_of_the_catalogue() {
    let guide = include_str!("../../../../docs/guides/loop.md");
    let priced: Vec<(&str, Prices)> = (anthropic::spend::PRICED.iter())
        .chain(google::spend::PRICED.iter())
        .chain(openai::spend::PRICED.iter())
        .copied()
        .collect();
    assert_eq!(priced.len(), 5);
    // A fill is one request of the filler, at its price.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let fill = fal::spend::bound_micros(fal::spend::PRICE) as u64;
    assert_eq!(usd(fill), "0.040000");
    let words = guide.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        words.contains("a fill reserves 0.04 dollars"),
        "the fill's bound is stated"
    );
    for (model, prices) in priced {
        // A check reserves the bound of one request, at the default ceilings, for each
        // turn its judge may take; an attempt is a fill and a check.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let request = bound_micros(DEFAULT_CEILINGS, prices).round() as u64;
        let check = request * TURNS_LIMIT as u64;
        let row = format!(
            "| `{model}` | {} | {} | {} | {} |",
            usd(check),
            usd(fill + check),
            usd(2 * (fill + check)),
            usd(3 * (fill + check)),
        );
        assert!(guide.contains(&row), "the guide lacks the row: {row}");
    }
    // What the plan's own sentence gives, and that nothing here was measured live.
    for figure in [
        "0.112768", "0.725760", "0.967680", "0.458304", "2.297280", "3.023040",
    ] {
        assert!(guide.contains(figure), "{figure}");
    }
    assert!(guide.contains("No live call was made to work these out"));
    // The guide is listed with the others.
    let index = include_str!("../../../../docs/guides/README.md");
    assert!(index.contains("[The loop: remove, check, retry](loop.md)"));
}

/// The code spans of a guide, each with its white space made single spaces, so a span
/// that a line break cuts is read whole.
fn spans(guide: &str) -> Vec<String> {
    (guide.split('`').skip(1).step_by(2))
        .map(|span| span.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect()
}

#[test]
fn the_playgrounds_guide_names_only_cases_that_exist() {
    // Plan m5, task E1, item 3: each example of the playground's guide is marked by the
    // tier that holds it. A Rust case is named `<module>::<test>`, a Chrome case
    // `<file>.spec.ts: <title>`; each must be in the tree, so a case renamed or dropped
    // fails here and the guide is mended with it.
    let guide = include_str!("../../../../docs/guides/playground.md");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |path: &str| {
        std::fs::read_to_string(root.join(path)).unwrap_or_else(|_| panic!("{path} is read"))
    };
    let modules = [
        ("play", "crates/whizbang/tests/it/play.rs"),
        ("play_result", "crates/whizbang/tests/it/play_result.rs"),
        ("play_models", "crates/whizbang/tests/it/play_models.rs"),
        ("play_guard", "crates/whizbang-serve/tests/it/play_guard.rs"),
        ("guides", "crates/whizbang/tests/it/guides.rs"),
    ];
    let (mut rust, mut chrome) = (0, 0);
    for span in spans(guide) {
        if let Some((module, test)) = span.split_once("::") {
            let Some((_, path)) = modules.iter().find(|(name, _)| *name == module) else {
                continue;
            };
            if test == "…" {
                // A module named as a whole, as the guide's first part names each.
                continue;
            }
            assert!(
                test.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
                "a case is named whole: {span}"
            );
            assert!(
                read(path).contains(&format!("fn {test}()")),
                "the guide names a case that is not there: {span}"
            );
            rust += 1;
        } else if let Some((file, title)) = span.split_once(".spec.ts: ") {
            let text = read(&format!("ui/e2e/{file}.spec.ts"));
            assert!(
                text.contains(&format!("test(\"{title}\"")),
                "the guide names a Chrome case that is not there: {span}"
            );
            chrome += 1;
        }
    }
    // Every screen is held by cases of both tiers.
    assert!(rust >= 40, "{rust} Rust cases named");
    assert!(chrome >= 40, "{chrome} Chrome cases named");
    for tier in ["(fast tier)", "(Chrome tier)", "by hand"] {
        assert!(guide.contains(tier), "{tier}");
    }
    // The guide is listed with the others.
    let index = include_str!("../../../../docs/guides/README.md");
    assert!(index.contains("[The playground](playground.md)"));
}

#[test]
fn the_engine_apis_guide_names_only_cases_that_exist() {
    // Plan m6, task H1, item 9: each example of the engine API's guide is marked by the
    // tier that holds it, and names its case `<module>::<test>`; each must be in the tree.
    let guide = include_str!("../../../../docs/guides/engine-api.md");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |path: &str| {
        std::fs::read_to_string(root.join(path)).unwrap_or_else(|_| panic!("{path} is read"))
    };
    let modules = [
        ("serve_http", "crates/whizbang/tests/it/serve_http.rs"),
        (
            "engine_guard",
            "crates/whizbang-serve/tests/it/engine_guard.rs",
        ),
        ("story", "crates/whizbang/tests/it/story.rs"),
    ];
    let mut named = 0;
    for span in spans(guide) {
        let Some((module, test)) = span.split_once("::") else {
            continue;
        };
        let Some((_, path)) = modules.iter().find(|(name, _)| *name == module) else {
            continue;
        };
        assert!(
            read(path).contains(&format!("fn {test}()")),
            "the guide names a case that is not there: {span}"
        );
        named += 1;
    }
    assert!(named >= 20, "{named} cases named");
    assert!(guide.contains("fast tier"));
    let index = include_str!("../../../../docs/guides/README.md");
    assert!(index.contains("[The engine API](engine-api.md)"));
}
