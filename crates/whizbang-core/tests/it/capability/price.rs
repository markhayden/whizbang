//! What a backend says of cost (`SPEC.md` section 6, "Spend"; ADR 0010): a quote is
//! free, an enforceable bound or an estimate only; a price by time is a bound only under
//! a limit its host enforces or a fixed maximum charge; and an estimate only, asked
//! through `Backends::bound`, is the amount that is no bound.

use whizbang_core::capability::{Declaration, Price, Quote, QuoteKind, Roster, TimeLimit, Unfit};
use whizbang_core::contract::{Backends, Capability, Estimate, ModelIdentity};

fn model(name: &str) -> ModelIdentity {
    ModelIdentity::new("fake", name, "1")
}

fn declared(name: &str) -> Declaration {
    Declaration::new(model(name), &[]).unwrap()
}

/// A roster that offers one filler, with this declaration.
fn offering(declaration: Declaration) -> Roster {
    Roster::new()
        .offer(Capability::Fill, "one", declaration)
        .unwrap()
        .default_for(Capability::Fill, "one")
        .unwrap()
}

/// What the roster answers as the bound of the filler `m`.
fn bound_of(roster: &Roster) -> Estimate {
    Backends::bound(roster, Capability::Fill, &model("m"))
}

/// 2 to the power 64: one more than the largest amount, and the least number of
/// millionths that no quote holds.
const TWO_TO_THE_64: f64 = 18_446_744_073_709_551_616.0;

#[test]
fn a_quote_cannot_be_made_from_a_number_that_is_not_finite_or_is_negative() {
    let refused = [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -1.0,
        -0.000_001,
        -f64::MIN_POSITIVE,
    ];
    for number in refused {
        for made in [Quote::bound(number), Quote::estimate_only(number)] {
            assert!(
                matches!(made, Err(Unfit::OutOfRange { .. })),
                "{number} made {made:?}"
            );
        }
        assert!(Price::at_most(number).is_err(), "{number}");
        assert!(Price::about(number).is_err(), "{number}");
        let limit = TimeLimit::of_host(1000).unwrap();
        assert!(Price::by_time(number, limit).is_err(), "{number}");
        assert!(Price::free().charged_at_most(number).is_err(), "{number}");
    }
}

#[test]
fn a_refusal_of_a_number_says_which_number_and_what_is_allowed() {
    let refused = Quote::bound(-2.5).unwrap_err();
    assert_eq!(
        refused.to_string(),
        "a quote is -2.5: a value is a finite number of millionths of a US dollar, from 0 to \
         under 2 to the power 64"
    );
}

#[test]
fn a_quote_made_from_a_fraction_of_the_unit_is_rounded_up() {
    let cases = [
        (0.0, 0),
        (0.000_001, 1),
        (f64::MIN_POSITIVE, 1),
        (0.5, 1),
        (1.0, 1),
        (2.1, 3),
        (2.999, 3),
        (5.0, 5),
        (1_000_000.25, 1_000_001),
    ];
    for (number, micros) in cases {
        assert_eq!(Quote::bound(number).unwrap().micros(), micros, "{number}");
        assert_eq!(
            Quote::estimate_only(number).unwrap().micros(),
            micros,
            "{number}"
        );
    }
    // Nothing less than nothing: a zero with a sign is nothing.
    assert_eq!(Quote::bound(-0.0).unwrap().micros(), 0);
}

#[test]
fn a_quote_is_under_the_largest_amount_which_is_no_amount() {
    assert!(Quote::bound(TWO_TO_THE_64).is_err());
    assert!(Quote::estimate_only(TWO_TO_THE_64).is_err());
    assert!(Quote::bound(f64::MAX).is_err());
    // The largest number under it that the machine can write.
    let largest = 18_446_744_073_709_549_568.0;
    let quote = Quote::bound(largest).unwrap();
    assert_eq!(quote.micros(), u64::MAX - 2047);
    assert!(!quote.most().is_unbounded());
}

#[test]
fn a_quote_says_its_kind_and_its_amount() {
    let free = Quote::free();
    assert_eq!((free.kind(), free.micros()), (QuoteKind::Free, 0));
    let bound = Quote::bound(250_000.0).unwrap();
    assert_eq!((bound.kind(), bound.micros()), (QuoteKind::Bound, 250_000));
    let estimate = Quote::estimate_only(250_000.0).unwrap();
    assert_eq!(
        (estimate.kind(), estimate.micros()),
        (QuoteKind::EstimateOnly, 250_000)
    );
    // A bound of nothing is a bound, said so, and is not the word "free".
    let nothing = Quote::bound(0.0).unwrap();
    assert_eq!(nothing.kind(), QuoteKind::Bound);
    assert_ne!(nothing, free);
    assert_ne!(bound, estimate);
}

#[test]
fn the_most_a_quote_may_cost_is_nothing_its_bound_or_no_bound() {
    assert_eq!(Quote::free().most(), Estimate::of_micros(0));
    assert_eq!(Quote::bound(7.0).unwrap().most(), Estimate::of_micros(7));
    let estimate = Quote::estimate_only(7.0).unwrap().most();
    assert!(estimate.is_unbounded());
    assert_eq!(estimate, Estimate::unbounded());
    // An estimate of nothing is still not a bound.
    assert!(Quote::estimate_only(0.0).unwrap().most().is_unbounded());
}

#[test]
fn a_quote_of_an_estimate_only_asked_through_backends_bound_is_the_amount_that_is_no_bound() {
    let price = Price::about(40_000.0).unwrap();
    let roster = offering(declared("m").priced(price));
    assert!(bound_of(&roster).is_unbounded());
    assert_eq!(bound_of(&roster), Estimate::of_micros(u64::MAX));
}

#[test]
fn a_bound_and_a_backend_that_says_it_is_free_are_answered_through_backends_bound() {
    let bounded = offering(declared("m").priced(Price::at_most(40_000.5).unwrap()));
    assert_eq!(bound_of(&bounded), Estimate::of_micros(40_001));
    let free = offering(declared("m").priced(Price::free()));
    assert_eq!(bound_of(&free), Estimate::of_micros(0));
}

#[test]
fn a_backend_of_before_that_states_no_price_answers_as_it_did() {
    let declaration = declared("m");
    assert_eq!(declaration.price(), None);
    assert_eq!(declaration.quote(), None);
    let roster = offering(declaration);
    assert_eq!(bound_of(&roster), Estimate::default());
    assert_eq!(bound_of(&roster).micros(), 0);
    // A model the roster does not offer, and one it offers for another capability.
    let other = Backends::bound(&roster, Capability::Fill, &model("other"));
    assert_eq!(other, Estimate::default());
    let elsewhere = Backends::bound(&roster, Capability::Judge, &model("m"));
    assert_eq!(elsewhere, Estimate::default());
    // And nothing about resolution changed.
    let resolved = Backends::resolve(&roster, Capability::Fill, None, &[]).unwrap();
    assert_eq!(resolved, model("m"));
}

#[test]
fn a_price_is_asked_of_the_capability_and_the_model_it_was_declared_for() {
    let roster = Roster::new()
        .offer(
            Capability::Fill,
            "paid",
            declared("m").priced(Price::at_most(9.0).unwrap()),
        )
        .unwrap()
        .offer(Capability::Judge, "plain", declared("m"))
        .unwrap();
    assert_eq!(bound_of(&roster), Estimate::of_micros(9));
    let judge = Backends::bound(&roster, Capability::Judge, &model("m"));
    assert_eq!(judge, Estimate::of_micros(0));
    // Another model, which no filler on offer declares, has no price here.
    let other = Backends::bound(&roster, Capability::Fill, &model("other"));
    assert_eq!(other, Estimate::of_micros(0));
    let version = ModelIdentity::new("fake", "m", "2");
    let later = Backends::bound(&roster, Capability::Fill, &version);
    assert_eq!(later, Estimate::of_micros(0));
}

#[test]
fn a_declaration_says_the_price_it_was_given() {
    let price = Price::at_most(12.0).unwrap();
    let declaration = declared("m").priced(price);
    assert_eq!(declaration.price(), Some(&price));
    assert_eq!(declaration.quote(), Some(Quote::bound(12.0).unwrap()));
    assert_ne!(declaration, declared("m"));
    // Stated again, the later price is the price.
    let again = declaration.priced(Price::free());
    assert_eq!(again.quote(), Some(Quote::free()));
}

#[test]
fn a_price_by_time_with_only_a_timeout_of_whizbangs_own_gives_an_estimate() {
    // 300 millionths a second, and whizbang gives up after 90 seconds.
    let timeout = TimeLimit::own_timeout(90_000).unwrap();
    assert!(!timeout.is_enforced_by_host());
    let price = Price::by_time(300.0, timeout).unwrap();
    let quote = price.quote();
    assert_eq!(quote.kind(), QuoteKind::EstimateOnly);
    assert_eq!(quote.micros(), 27_000);
    assert!(quote.most().is_unbounded());
    let roster = offering(declared("m").priced(price));
    assert!(bound_of(&roster).is_unbounded());
}

#[test]
fn a_price_by_time_with_a_hosts_limit_gives_the_bound_of_price_times_limit() {
    let limit = TimeLimit::of_host(90_000).unwrap();
    assert!(limit.is_enforced_by_host());
    assert_eq!(limit.ms(), 90_000);
    let price = Price::by_time(300.0, limit).unwrap();
    assert_eq!(price.micros_per_second(), Some(300));
    let quote = price.quote();
    assert_eq!(quote.kind(), QuoteKind::Bound);
    assert_eq!(quote.micros(), 27_000);
    let roster = offering(declared("m").priced(price));
    assert_eq!(bound_of(&roster), Estimate::of_micros(27_000));
}

#[test]
fn a_price_by_time_is_rounded_up_where_the_rate_enters_and_where_the_product_is_made() {
    // A rate of a fraction of a millionth a second is a whole one.
    let limit = TimeLimit::of_host(1000).unwrap();
    let price = Price::by_time(0.25, limit).unwrap();
    assert_eq!(price.micros_per_second(), Some(1));
    assert_eq!(price.quote().micros(), 1);
    // 3 a second for 1.5 seconds is 4.5, so 5; for a millisecond it is 0.003, so 1.
    let limit = TimeLimit::of_host(1500).unwrap();
    assert_eq!(Price::by_time(3.0, limit).unwrap().quote().micros(), 5);
    let limit = TimeLimit::of_host(1).unwrap();
    assert_eq!(Price::by_time(3.0, limit).unwrap().quote().micros(), 1);
    // A rate of nothing costs nothing, and is still a bound, not the word "free".
    let limit = TimeLimit::of_host(1000).unwrap();
    let nothing = Price::by_time(0.0, limit).unwrap().quote();
    assert_eq!((nothing.kind(), nothing.micros()), (QuoteKind::Bound, 0));
}

#[test]
fn a_fixed_maximum_charge_makes_a_price_by_time_a_bound() {
    // With whizbang's own timeout alone it was an estimate of 27,000.
    let timeout = TimeLimit::own_timeout(90_000).unwrap();
    let price = Price::by_time(300.0, timeout)
        .unwrap()
        .charged_at_most(50_000.0)
        .unwrap();
    assert_eq!(price.quote(), Quote::bound(50_000.0).unwrap());
    // Under a host's limit as well, the bound is the smaller of the two.
    let limit = TimeLimit::of_host(90_000).unwrap();
    let by_time = Price::by_time(300.0, limit).unwrap();
    let dearer = by_time.charged_at_most(50_000.0).unwrap();
    assert_eq!(dearer.quote(), Quote::bound(27_000.0).unwrap());
    let cheaper = by_time.charged_at_most(20_000.5).unwrap();
    assert_eq!(cheaper.quote(), Quote::bound(20_001.0).unwrap());
}

#[test]
fn a_fixed_maximum_charge_bounds_any_price_and_leaves_a_free_one_free() {
    let about = Price::about(40_000.0).unwrap();
    assert_eq!(about.quote().kind(), QuoteKind::EstimateOnly);
    let capped = about.charged_at_most(60_000.0).unwrap();
    assert_eq!(capped.quote(), Quote::bound(60_000.0).unwrap());
    let lower = Price::at_most(9.0).unwrap().charged_at_most(4.0).unwrap();
    assert_eq!(lower.quote(), Quote::bound(4.0).unwrap());
    let higher = Price::at_most(9.0).unwrap().charged_at_most(40.0).unwrap();
    assert_eq!(higher.quote(), Quote::bound(9.0).unwrap());
    let free = Price::free().charged_at_most(40.0).unwrap();
    assert_eq!(free.quote(), Quote::free());
    // Said twice, the lower charge is the charge.
    let twice = about
        .charged_at_most(60_000.0)
        .unwrap()
        .charged_at_most(70_000.0)
        .unwrap();
    assert_eq!(twice.quote(), Quote::bound(60_000.0).unwrap());
}

#[test]
fn a_limit_of_no_time_is_refused() {
    assert!(matches!(TimeLimit::of_host(0), Err(Unfit::Empty { .. })));
    assert!(matches!(
        TimeLimit::own_timeout(0),
        Err(Unfit::Empty { .. })
    ));
}

#[test]
fn a_price_by_time_that_reaches_the_largest_amount_is_refused_when_it_is_stated() {
    // The largest rate, for one second: under the largest amount, by 2,047.
    let second = TimeLimit::of_host(1000).unwrap();
    let largest = 18_446_744_073_709_549_568.0;
    let price = Price::by_time(largest, second).unwrap();
    assert_eq!(price.quote().micros(), u64::MAX - 2047);
    // For two seconds it is over, as a bound and as an estimate.
    let two = TimeLimit::of_host(2000).unwrap();
    assert!(matches!(
        Price::by_time(largest, two),
        Err(Unfit::OutOfRange { .. })
    ));
    let own = TimeLimit::own_timeout(2000).unwrap();
    assert!(Price::by_time(largest, own).is_err());
    // Exactly the largest amount is no amount: 1,000 a second for the largest count of
    // milliseconds is 2 to the power 64, less 1.
    let limit = TimeLimit::of_host(u64::MAX).unwrap();
    assert!(Price::by_time(1000.0, limit).is_err());
    let under = Price::by_time(999.0, limit).unwrap();
    assert!(!under.quote().most().is_unbounded());
}

#[test]
fn debug_of_each_new_type_shows_no_more_than_its_numbers() {
    assert_eq!(
        format!("{:?}", Quote::bound(5.5).unwrap()),
        "Quote { kind: Bound, micros: 6 }"
    );
    assert_eq!(
        format!("{:?}", Quote::free()),
        "Quote { kind: Free, micros: 0 }"
    );
    assert_eq!(
        format!("{:?}", Quote::estimate_only(2.0).unwrap()),
        "Quote { kind: EstimateOnly, micros: 2 }"
    );
    assert_eq!(
        format!("{:?}", TimeLimit::of_host(250).unwrap()),
        "TimeLimit { enforced_by_host: true, ms: 250 }"
    );
    assert_eq!(
        format!("{:?}", Price::free()),
        "Price { stated: Free, most: None }"
    );
    assert_eq!(
        format!("{:?}", Price::at_most(8.0).unwrap()),
        "Price { stated: AtMost { micros: 8 }, most: None }"
    );
    assert_eq!(
        format!(
            "{:?}",
            Price::about(8.0).unwrap().charged_at_most(9.0).unwrap()
        ),
        "Price { stated: About { micros: 8 }, most: Some(9) }"
    );
    let own = TimeLimit::own_timeout(250).unwrap();
    assert_eq!(
        format!("{:?}", Price::by_time(4.0, own).unwrap()),
        "Price { stated: ByTime { micros_per_second: 4, limit: TimeLimit { enforced_by_host: \
         false, ms: 250 }, micros: 1 }, most: None }"
    );
    // A declaration shows its price beside what it showed before.
    let shown = format!("{:?}", declared("m").priced(Price::free()));
    assert!(
        shown.ends_with("price: Some(Price { stated: Free, most: None }) }"),
        "{shown}"
    );
    assert!(format!("{:?}", declared("m")).ends_with("price: None }"));
}

#[test]
fn known_two_backends_of_one_capability_that_declare_one_model_are_bounded_by_the_dearer() {
    // `Backends::bound` is asked with the capability and the model, not with the name
    // a call chose (ADR 0007), so two offers of one model cannot be told apart: the
    // answer is the larger, and no bound where either states none.
    let roster = Roster::new()
        .offer(
            Capability::Fill,
            "cheap",
            declared("m").priced(Price::at_most(5.0).unwrap()),
        )
        .unwrap()
        .offer(
            Capability::Fill,
            "dear",
            declared("m").priced(Price::at_most(50.0).unwrap()),
        )
        .unwrap()
        .offer(Capability::Fill, "plain", declared("m"))
        .unwrap();
    assert_eq!(bound_of(&roster), Estimate::of_micros(50));
    let with_estimate = roster
        .offer(
            Capability::Fill,
            "guess",
            declared("m").priced(Price::about(1.0).unwrap()),
        )
        .unwrap();
    assert!(bound_of(&with_estimate).is_unbounded());
}

#[test]
fn known_a_fraction_that_floating_point_arithmetic_made_is_rounded_up_a_whole_unit() {
    // 0.0079 dollars is 7,900 millionths. Worked out as 0.0079 times a million, in the
    // machine's arithmetic, it is a little over, and a quote of it is one unit more:
    // over, never under. A caller that holds dollars converts in whole numbers.
    let product = 0.0079_f64 * 1e6;
    assert!(product > 7900.0);
    assert_eq!(Quote::bound(product).unwrap().micros(), 7901);
}
