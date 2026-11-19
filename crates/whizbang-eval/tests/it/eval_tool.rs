//! How a tool's measurement is scored on rung 2 (plan `docs/plans/m2c-milestone-2.md`, task
//! X1, item 8): the area under the ROC curve with its interval by family, the balanced
//! accuracy across halves of the families, and errors apart.

use whizbang_eval::sets::Expected;
use whizbang_eval::tool::{Accuracy, Changed, Read, auc, score, shuffled};

/// A read of item `i` of family `f` in set `s`.
fn read(s: &str, f: usize, i: usize, expected: Expected, value: f64) -> Read {
    Read {
        set: s.to_owned(),
        item: format!("{f}-{i}"),
        family: format!("f{f}"),
        expected,
        value: Ok(value),
    }
}

/// Eight families of two items, a pass and a fail, whose fail reads `gap` more than its
/// pass, from a base that moves with the family.
fn separated(gap: f64) -> Vec<Read> {
    (0..8)
        .flat_map(|f| {
            let base = 0.01 * f as f64;
            [
                read("scenes", f, 0, Expected::Pass, base),
                read("scenes", f, 1, Expected::Fail, base + gap),
            ]
        })
        .collect()
}

#[test]
fn a_measurement_that_separates_fail_from_pass_scores_an_auc_of_one_and_a_shuffled_one_near_a_half()
{
    let scored = score(&separated(1.0), Changed::Larger, 7, 200).unwrap();
    assert_eq!(scored.auc, 1.0);
    assert_eq!((scored.auc_lower, scored.auc_upper), (1.0, 1.0));
    assert_eq!(
        (scored.items, scored.fail_items, scored.pass_items),
        (16, 8, 8)
    );
    // The same number where smaller means changed: turned the other way, the same area.
    let turned: Vec<Read> = separated(1.0)
        .into_iter()
        .map(|mut r| {
            r.value = r.value.map(|v| -v);
            r
        })
        .collect();
    assert_eq!(score(&turned, Changed::Smaller, 7, 200).unwrap().auc, 1.0);
    // And read the wrong way, none: an area of zero.
    assert_eq!(score(&turned, Changed::Larger, 7, 200).unwrap().auc, 0.0);
    // A number that says nothing of the label: values in an order unrelated to it.
    let shuffled_reads: Vec<Read> = (0..40)
        .map(|i| {
            let expected = if i % 2 == 0 {
                Expected::Pass
            } else {
                Expected::Fail
            };
            // A fixed scramble of 0..40, the same for both labels' halves.
            let value = f64::from((i * 17 + 5) % 41);
            read("scenes", i as usize, 0, expected, value)
        })
        .collect();
    let scored = score(&shuffled_reads, Changed::Larger, 7, 200).unwrap();
    assert!((scored.auc - 0.5).abs() < 0.1, "{}", scored.auc);
    assert!(
        scored.auc_lower < 0.5 && scored.auc_upper > 0.5,
        "{scored:?}"
    );
    // Ties count half.
    let tied = [(0.3, Expected::Pass), (0.3, Expected::Fail)];
    assert_eq!(auc(&tied), Some(0.5));
}

#[test]
fn the_threshold_is_chosen_on_one_half_of_the_families_and_read_on_the_other() {
    let reads = separated(1.0);
    let scored = score(&reads, Changed::Larger, 7, 50).unwrap();
    let Accuracy::Read {
        threshold,
        balanced_accuracy,
        chosen_accuracy,
        chosen_on,
        read_on,
    } = scored.accuracy
    else {
        panic!("{:?}", scored.accuracy);
    };
    // Four families on each side, none on both, all eight between them.
    assert_eq!((chosen_on.len(), read_on.len()), (4, 4));
    assert!(chosen_on.iter().all(|f| !read_on.contains(f)));
    let mut all: Vec<String> = chosen_on.iter().chain(&read_on).cloned().collect();
    all.sort();
    let want: Vec<String> = (0..8).map(|f| format!("scenes/f{f}")).collect();
    assert_eq!(all, want);
    // The halves are the seed's: the same seed, the same halves; the shuffle is a shuffle.
    assert_eq!(shuffled(want.clone(), 7)[..4], chosen_on[..]);
    assert_ne!(shuffled(want.clone(), 7), want);
    assert_ne!(shuffled(want.clone(), 7), shuffled(want, 8));
    // The threshold separates the half that chose it, and is read on the other.
    assert_eq!((chosen_accuracy, balanced_accuracy), (1.0, 1.0));
    assert!(threshold > 0.07 && threshold < 1.0, "{threshold}");
    // A threshold chosen on one half that does not carry to the other: the families of the
    // half that is read have their labels the other way round, so it reads at 0.
    let flipped: Vec<Read> = reads
        .iter()
        .map(|r| {
            let mut r = r.clone();
            if read_on.contains(&format!("{}/{}", r.set, r.family)) {
                r.expected = match r.expected {
                    Expected::Pass => Expected::Fail,
                    Expected::Fail => Expected::Pass,
                };
            }
            r
        })
        .collect();
    let scored = score(&flipped, Changed::Larger, 7, 50).unwrap();
    let Accuracy::Read {
        balanced_accuracy,
        chosen_accuracy,
        ..
    } = scored.accuracy
    else {
        panic!("{:?}", scored.accuracy);
    };
    assert_eq!((chosen_accuracy, balanced_accuracy), (1.0, 0.0));
    // The families of every set are pooled: a family of the same name in two sets is two.
    let mut two_sets = separated(1.0)[..4].to_vec();
    two_sets.extend(separated(1.0)[..4].iter().map(|r| Read {
        set: "adversarial".to_owned(),
        ..r.clone()
    }));
    assert_eq!(
        score(&two_sets, Changed::Larger, 7, 50).unwrap().families,
        4
    );
}

#[test]
fn fewer_than_four_families_pooled_give_the_area_alone_and_an_inconclusive_accuracy() {
    let three = separated(1.0)[..6].to_vec();
    let scored = score(&three, Changed::Larger, 7, 50).unwrap();
    assert_eq!(scored.families, 3);
    assert_eq!(scored.auc, 1.0);
    let Accuracy::Inconclusive { why } = &scored.accuracy else {
        panic!("{:?}", scored.accuracy);
    };
    assert!(why.contains("fewer than 4"), "{why}");
    let four = separated(1.0)[..8].to_vec();
    let scored = score(&four, Changed::Larger, 7, 50).unwrap();
    assert!(
        matches!(scored.accuracy, Accuracy::Read { .. }),
        "{:?}",
        scored.accuracy
    );
}

#[test]
fn an_item_whose_tool_failed_is_an_error_and_neither_pass_nor_fail() {
    let mut reads = separated(1.0);
    reads[1].value = Err("the tool failed: not registered".to_owned());
    reads[3].value = Ok(f64::NAN);
    let scored = score(&reads, Changed::Larger, 7, 50).unwrap();
    assert_eq!(
        (scored.items, scored.fail_items, scored.pass_items),
        (14, 6, 8)
    );
    let named: Vec<(&str, &str)> = scored
        .errors
        .iter()
        .map(|e| (e.item.as_str(), e.why.as_str()))
        .collect();
    assert_eq!(
        named,
        [
            ("0-1", "the tool failed: not registered"),
            ("1-1", "the tool's number is not finite")
        ]
    );
    // The area stands on the items with a number alone.
    assert_eq!(scored.auc, 1.0);
    // Errors alone are no score.
    let none: Vec<Read> = reads
        .iter()
        .map(|r| Read {
            value: Err("no answer".to_owned()),
            ..r.clone()
        })
        .collect();
    assert!(score(&none, Changed::Larger, 7, 50).is_err());
}
