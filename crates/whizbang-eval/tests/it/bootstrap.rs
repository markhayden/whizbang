//! The paired bootstrap resamples whole families (plan 2C, task E2, item 1).

use whizbang_eval::bootstrap;
use whizbang_eval::metrics::{self, Computed, Expected, Resampling, Row};

use crate::metrics::row;

fn how(seed: u64) -> Resampling {
    Resampling {
        seed,
        resamples: 200,
        hypotheses: 1,
        alpha: 0.05,
    }
}

#[test]
fn paired_differences_resample_by_family_with_a_fixed_seed() {
    use Computed::{Fail, Pass};
    use Expected::{Fail as F, Pass as P};
    // Six items in one family: the treatment catches two fails the control missed.
    let control = [
        row("a", "only", 1, F, Pass),
        row("b", "only", 1, F, Pass),
        row("c", "only", 1, F, Fail),
        row("d", "only", 1, P, Pass),
        row("e", "only", 1, P, Pass),
        row("g", "only", 1, P, Pass),
    ];
    let mut treatment = control.clone();
    treatment[0].computed = Fail;
    treatment[1].computed = Fail;
    let one = metrics::paired(&control, &treatment, how(7)).unwrap();
    let gain = one.gain.unwrap();
    // One family drawn every time is the whole sample every time: resampled by item, the
    // interval would have width.
    assert_eq!(one.families, 1);
    let third = |x: f64| (x - 1.0 / 3.0).abs() < 1e-12;
    assert!(
        third(gain.lower) && third(gain.point) && third(gain.upper),
        "{gain:?}"
    );

    // The same items as six families: now the draws differ and the interval has width.
    let apart = |rows: &[Row]| -> Vec<Row> {
        rows.iter()
            .map(|r| Row {
                family: r.item.clone(),
                ..r.clone()
            })
            .collect()
    };
    let six = metrics::paired(&apart(&control), &apart(&treatment), how(7)).unwrap();
    let wide = six.gain.unwrap();
    assert_eq!(six.families, 6);
    assert!(third(wide.point));
    assert!(wide.lower < wide.point && wide.point < wide.upper);
    assert_eq!(six.differing_families, 2);

    // The seed fixes the interval; another seed draws other families.
    let again = metrics::paired(&apart(&control), &apart(&treatment), how(7)).unwrap();
    assert_eq!(again.gain.unwrap(), wide);
    assert_ne!(
        bootstrap::resample_indices(7, 6, 200).unwrap(),
        bootstrap::resample_indices(8, 6, 200).unwrap()
    );
    // No false reject moved.
    assert_eq!(six.false_reject_increase.unwrap().point, 0.0);
}
