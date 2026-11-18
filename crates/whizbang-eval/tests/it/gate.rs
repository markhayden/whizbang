//! Gate files and how a gate is decided (plan 2C, task E2, item 2).

use whizbang_eval::gate::{Gate, Outcome};
use whizbang_eval::metrics::{self, Computed, Expected, Row};

use crate::metrics::row;

const FILE: &str = r#"
gate_version = 1

[baseline]
control = "C"
treatment = "F"

[measure]
primary = "balanced_accuracy"
pass = "pass"
fail = "catch"
unsure = "neither"
error = "neither"
missing_label = "refuse"

[confidence]
level = 0.95
correction = "bonferroni"
hypotheses = 1

[families]
minimum = 20
resampling = "family"
seed = 20261118
resamples = 1000

[repetitions]
count = 1
combine = "first"

[admission]
min_gain = 0.03
lower_bound_above = 0.0

[guardrails]
false_reject_increase_max = 0.03
error_rate_max = 0.02

[limits]
cost_ratio_max = 2.0
latency_ratio_max = 2.0

[too_little_data]
outcome = "inconclusive"
"#;

#[test]
fn a_gate_file_missing_an_item_of_the_spec_is_refused() {
    Gate::parse(FILE).unwrap();
    // Each table, and each key in it, is one item or part of one: leaving any out is
    // refused, never filled with a default.
    let lines: Vec<&str> = FILE.lines().collect();
    for (at, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let without: Vec<&str> = lines
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != at)
            .map(|(_, l)| *l)
            .collect();
        let text = without.join("\n");
        // Leaving out a table header moves its keys to the table before it: refused
        // too, as keys that table does not name.
        assert!(Gate::parse(&text).is_err(), "without {line:?}");
    }
    // A key the spec does not name is refused.
    assert!(Gate::parse(&format!("{FILE}\n[extra]\nx = 1\n")).is_err());
    // What the spec fixes cannot be stated otherwise.
    for (from, to) in [
        ("unsure = \"neither\"", "unsure = \"pass\""),
        ("outcome = \"inconclusive\"", "outcome = \"admitted\""),
        ("resampling = \"family\"", "resampling = \"item\""),
        ("level = 0.95", "level = 1.0"),
        ("minimum = 20", "minimum = 0"),
    ] {
        assert!(Gate::parse(&FILE.replace(from, to)).is_err(), "{to}");
    }
}

/// `families` families of four items each, where the treatment catches one fail in each
/// family that the control let through, and changes nothing else.
fn rows(families: usize) -> (Vec<Row>, Vec<Row>) {
    use Computed::{Fail, Pass};
    use Expected::{Fail as F, Pass as P};
    let (mut c, mut x) = (Vec::new(), Vec::new());
    for f in 0..families {
        let fam = format!("fam{f:02}");
        let items = [
            (F, Pass, Fail),
            (F, Fail, Fail),
            (P, Pass, Pass),
            (P, Pass, Pass),
        ];
        for (i, (expected, control, treatment)) in items.into_iter().enumerate() {
            let id = format!("{fam}-{i}");
            c.push(row(&id, &fam, 1, expected, control));
            x.push(row(&id, &fam, 1, expected, treatment));
        }
    }
    (c, x)
}

#[test]
fn too_little_data_is_inconclusive_never_a_pass() {
    let gate = Gate::parse(FILE).unwrap();
    // Twenty families, each gaining: admitted.
    let (c, x) = rows(20);
    let enough = metrics::paired(&c, &x, gate.resampling()).unwrap();
    let verdict = gate.decide(&enough);
    assert_eq!(verdict.outcome, Outcome::Admitted, "{verdict:?}");
    // The same gain on nineteen families is too little: inconclusive, not admitted.
    let (c, x) = rows(19);
    let short = metrics::paired(&c, &x, gate.resampling()).unwrap();
    let verdict = gate.decide(&short);
    assert_eq!(verdict.outcome, Outcome::Inconclusive);
    assert_eq!(verdict.failed, ["families"]);
    // Too few resamples to resolve the tail is too little as well.
    let thin = Gate::parse(&FILE.replace("resamples = 1000", "resamples = 200")).unwrap();
    let (c, x) = rows(20);
    let tail = metrics::paired(&c, &x, thin.resampling()).unwrap();
    assert_eq!(thin.decide(&tail).failed, ["resamples"]);
    // No expected pass among the paired items: no interval, inconclusive.
    let fails_only = |rows: Vec<Row>| -> Vec<Row> {
        rows.into_iter()
            .filter(|r| r.expected == Expected::Fail)
            .collect()
    };
    let none = metrics::paired(&fails_only(c), &fails_only(x), gate.resampling()).unwrap();
    assert_eq!(gate.decide(&none).outcome, Outcome::Inconclusive);
    // Errors past the guardrail reject, however large the gain.
    let (c, mut x) = rows(20);
    x[2].computed = Computed::Error;
    x[3].computed = Computed::Error;
    let errors = metrics::paired(&c, &x, gate.resampling()).unwrap();
    let verdict = gate.decide(&errors);
    assert_eq!(verdict.outcome, Outcome::Rejected);
    assert!(verdict.failed.contains(&"errors".to_string()));
}
