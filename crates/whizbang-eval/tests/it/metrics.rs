//! Measures (plan 2C, task E2, item 1), and the vectors that hold the ported statistics to
//! squint's own numbers.

use whizbang_eval::bootstrap::{self, PairedItem};
use whizbang_eval::metrics::{self, Computed, Expected, Row};
use whizbang_eval::wilson::{Z_95, wilson};

pub fn row(item: &str, family: &str, rep: u32, expected: Expected, computed: Computed) -> Row {
    Row {
        set: "s".into(),
        item: item.into(),
        family: family.into(),
        repetition: rep,
        expected,
        computed,
        cost_usd: 0.01,
        wall_ms: 100,
    }
}

#[test]
fn balanced_accuracy_counts_unsure_and_errors_as_neither() {
    use Computed::*;
    use Expected::{Fail as F, Pass as P};
    let rows = [
        row("a", "f", 1, F, Fail),
        row("b", "f", 1, F, Unsure),
        row("c", "f", 1, F, Error),
        row("d", "f", 1, F, Pass),
        row("e", "g", 1, P, Pass),
        row("g", "g", 1, P, Unsure),
        row("h", "g", 1, P, Error),
        row("i", "g", 1, P, Fail),
        // A later repetition never moves a headline number.
        row("a", "f", 2, F, Pass),
    ];
    let m = metrics::cell(&rows);
    // One catch in four expected fails; one pass in four expected passes. Were unsure
    // and errors left out, it would be (1/2 + 1/2) / 2 = 0.5 as well, so the shares say it.
    assert_eq!(m.balanced_accuracy, Some((0.25 + 0.25) / 2.0));
    assert_eq!((m.counts.detected, m.counts.passed), (1, 1));
    assert_eq!((m.counts.unsure, m.counts.errors), (2, 2));
    assert_eq!(m.confusion, [[1, 1, 1, 1], [1, 1, 1, 1]]);
    // Moving the unsure fail to a catch moves it by an eighth.
    let mut better = rows.to_vec();
    better[1].computed = Fail;
    assert_eq!(metrics::cell(&better).balanced_accuracy, Some(0.375));
    // Moving an unsure pass to an error changes nothing: both are neither.
    let mut same = rows.to_vec();
    same[5].computed = Error;
    assert_eq!(metrics::cell(&same).balanced_accuracy, Some(0.25));
    // Self-disagreement: the one later repetition differs from its first.
    assert_eq!(m.self_disagreement, Some(1.0));
    assert_eq!(m.repetitions, 2);
    // Without an expected pass there is no balanced accuracy.
    assert_eq!(metrics::cell(&rows[..4]).balanced_accuracy, None);
}

#[test]
fn the_ported_vectors_of_squint_give_squints_numbers() {
    // Squint's Wilson table (`crates/squint-eval/src/wilson.rs`, its test).
    let table: &[(u64, u64, f64, f64)] = &[
        (0, 10, 0.0, 0.2775327998628892),
        (10, 10, 0.7224672001371107, 0.9999999999999999),
        (7, 10, 0.39677814746114537, 0.8922087325936989),
        (1, 2, 0.09453120573423074, 0.9054687942657693),
        (50, 100, 0.4038315303659956, 0.5961684696340044),
        (3, 20, 0.052368745896216595, 0.36041886474075696),
    ];
    for &(s, n, lo, hi) in table {
        let (l, h) = wilson(s, n, Z_95).unwrap();
        assert!((l - lo).abs() < 1e-9 && (h - hi).abs() < 1e-9, "{s}/{n}");
    }
    assert_eq!(wilson(0, 0, Z_95), None);
    assert_eq!(wilson(3, 2, Z_95), None);

    // What squint's `resample_indices` gave for these seeds, run from the frozen squint
    // with its pinned `rand` and `rand_chacha`.
    assert_eq!(
        bootstrap::resample_indices(1, 5, 2).unwrap(),
        [[1, 3, 3, 0, 0], [2, 2, 2, 3, 4]]
    );
    assert_eq!(
        bootstrap::resample_indices(20261118, 7, 3).unwrap(),
        [
            [6, 3, 3, 1, 3, 1, 2],
            [3, 3, 0, 4, 4, 6, 6],
            [5, 1, 4, 4, 4, 6, 6]
        ]
    );
    // A range of three thousand million takes the second word about seven times in ten:
    // squint's `random_range(0..3_000_000_000usize)` from seed 42, twelve times.
    assert_eq!(
        bootstrap::draws(42, 3_000_000_000, 12),
        [
            2527829731, 1911226916, 5191296, 293441599, 2358091073, 502249880, 489457665,
            1035894426, 2482391040, 1342540929, 188824436, 1121951899
        ]
    );

    // Squint's `paired_bootstrap` on twenty-four items in eight families.
    let mut items = Vec::new();
    for k in 0..24u32 {
        let fail = |yes: bool| if yes { Computed::Fail } else { Computed::Pass };
        items.push(PairedItem {
            family: format!("f{}", k / 3),
            id: format!("f{}-{}", k / 3, k % 3),
            expected: if k % 4 != 3 {
                Expected::Fail
            } else {
                Expected::Pass
            },
            control: fail(k % 3 == 0),
            treatment: fail(k % 2 == 0 || k % 5 == 0),
        });
    }
    let b = bootstrap::detection_difference(&items, 7, 200, 2).unwrap();
    assert_eq!(b.point, 0.3888888888888889);
    assert_eq!(b.lower, 0.17647058823529413);
    assert_eq!(b.upper, 0.6182291666666669);
    assert_eq!(b.lower_percentile, 0.0125);
    assert_eq!((b.families, b.differing_families), (8, 8));
    assert!(b.insufficient_tail);
    let b = bootstrap::detection_difference(&items, 20261118, 200, 1).unwrap();
    assert_eq!(
        (b.point, b.lower, b.upper, b.lower_percentile),
        (
            0.3888888888888889,
            0.23529411764705882,
            0.5882352941176471,
            0.025
        )
    );
}

#[test]
fn paired_gives_the_decided_correctly_difference() {
    use Computed::{Fail, Pass, Unsure};
    use Expected::{Fail as F, Pass as P};
    // Four items, each its own family. The control decides two of four correctly (a catch
    // and a pass); the treatment decides all four, turning an unsure fail into a catch and
    // a false reject into a pass.
    let control = [
        row("a", "fa", 1, F, Fail),
        row("b", "fb", 1, F, Unsure),
        row("c", "fc", 1, P, Pass),
        row("d", "fd", 1, P, Fail),
    ];
    let mut treatment = control.clone();
    treatment[1].computed = Fail;
    treatment[3].computed = Pass;
    let how = metrics::Resampling {
        seed: 7,
        resamples: 200,
        hypotheses: 1,
        alpha: 0.05,
    };
    let paired = metrics::paired(&control, &treatment, how).unwrap();
    let difference = paired.decided_correctly_difference.unwrap();
    assert!((difference.point - 0.5).abs() < 1e-12, "{difference:?}");
    assert!(difference.lower <= difference.point && difference.point <= difference.upper);
    // The share itself: catches and passes over every item, unsure and errors neither.
    let counts = metrics::Counts::of(&control);
    assert_eq!(counts.decided_correctly(), Some(0.5));
    assert_eq!(
        metrics::Counts::of(&Vec::<Row>::new()).decided_correctly(),
        None
    );
    // The same arms give no difference.
    let same = metrics::paired(&control, &control, how).unwrap();
    assert_eq!(same.decided_correctly_difference.unwrap().point, 0.0);
}
