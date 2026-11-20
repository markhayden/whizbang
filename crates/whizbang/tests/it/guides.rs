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
