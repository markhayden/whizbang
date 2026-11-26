//! Gate files and how a gate is decided (plan 2C, task E2, item 2).

use whizbang_eval::gate::{Gate, Outcome};
use whizbang_eval::metrics::{self, Computed, Expected, Row};

use crate::metrics::row;

pub const FILE: &str = r#"
gate_version = 2

[run]
matrix = "matrix.toml"
matrix_identity = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

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
        ("matrix = \"matrix.toml\"", "matrix = \"../matrix.toml\""),
        ("matrix_identity = \"0123", "matrix_identity = \"X123"),
        ("treatment = \"F\"", "treatment = \"C\""),
        ("gate_version = 2", "gate_version = 1"),
        ("gate_version = 2", "gate_version = 4"),
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

#[test]
fn a_gate_over_several_judges_is_admitted_only_when_every_judge_is() {
    use whizbang_eval::gate::Judged;
    // Two hypotheses halve each tail: 2,000 resamples resolve it.
    let two = FILE
        .replace("hypotheses = 1", "hypotheses = 2")
        .replace("resamples = 1000", "resamples = 2000");
    let gate = Gate::parse(&two).unwrap();
    let judged = |name: &str, families: usize| {
        let (control, treatment) = rows(families);
        Judged {
            backend: name.into(),
            model: "m".into(),
            control,
            treatment,
        }
    };
    let both = gate.score(&[judged("a", 20), judged("b", 20)]).unwrap();
    assert_eq!(both.outcome, Outcome::Admitted);
    assert_eq!(both.judges.len(), 2);
    // One judge on too little data: the gate is inconclusive, never admitted.
    let short = gate.score(&[judged("a", 20), judged("b", 19)]).unwrap();
    assert_eq!(
        (short.judges[1].outcome, short.outcome),
        (Outcome::Inconclusive, Outcome::Inconclusive)
    );
    // One judge rejected: the gate is rejected.
    let mut errors = judged("b", 20);
    errors.treatment[2].computed = Computed::Error;
    errors.treatment[3].computed = Computed::Error;
    let rejected = gate.score(&[judged("a", 20), errors]).unwrap();
    assert_eq!(rejected.outcome, Outcome::Rejected);
    // No judge: inconclusive. More judges than the correction covers: refused.
    assert_eq!(gate.score(&[]).unwrap().outcome, Outcome::Inconclusive);
    let one = Gate::parse(FILE).unwrap();
    let refused = one.score(&[judged("a", 20), judged("b", 20)]).unwrap_err();
    assert!(refused.contains("corrects for 1 hypotheses"), "{refused}");
}

#[test]
fn a_rise_in_false_accepts_past_its_guardrail_is_not_admitted() {
    // Plan m3, task U1: a treatment that turns unsure into catches and into passes of items
    // that fail. Each of 20 families: three fails the control left unsure, two caught by the
    // treatment and one passed; two passes passed by both.
    use Computed::{Fail, Pass, Unsure};
    use Expected::{Fail as F, Pass as P};
    let (mut c, mut x) = (Vec::new(), Vec::new());
    for f in 0..20 {
        let fam = format!("fam{f:02}");
        let items = [
            (F, Unsure, Fail),
            (F, Unsure, Fail),
            (F, Unsure, Pass),
            (P, Pass, Pass),
            (P, Pass, Pass),
        ];
        for (i, (expected, control, treatment)) in items.into_iter().enumerate() {
            let id = format!("{fam}-{i}");
            c.push(row(&id, &fam, 1, expected, control));
            x.push(row(&id, &fam, 1, expected, treatment));
        }
    }
    // A gate that states no false-accept guardrail admits it on its gain.
    let plain = Gate::parse(FILE).unwrap();
    assert_eq!(plain.guardrails.false_accept_increase_max, None);
    let paired = metrics::paired(&c, &x, plain.resampling()).unwrap();
    assert_eq!(plain.decide(&paired).outcome, Outcome::Admitted);
    assert_eq!(paired.treatment.false_accepts.count, 20);
    // One that states it rejects it: a third of the fails passed, against 3 points allowed.
    let guarded = FILE.replace(
        "error_rate_max = 0.02",
        "error_rate_max = 0.02\nfalse_accept_increase_max = 0.03",
    );
    let gate = Gate::parse(&guarded).unwrap();
    let verdict = gate.decide(&metrics::paired(&c, &x, gate.resampling()).unwrap());
    assert_eq!(verdict.outcome, Outcome::Rejected);
    assert_eq!(verdict.failed, ["false_accepts"]);
    // And admits the same gain when no fail is passed.
    let mut kept = x.clone();
    for r in kept
        .iter_mut()
        .filter(|r| r.expected == F && r.computed == Pass)
    {
        r.computed = Unsure;
    }
    let verdict = gate.decide(&metrics::paired(&c, &kept, gate.resampling()).unwrap());
    assert_eq!(verdict.outcome, Outcome::Admitted, "{verdict:?}");
}

#[test]
fn a_version_3_gate_names_its_matrix_by_file_alone() {
    // Plan mE, task M1, items 3 and 4: the matrix names the gate by its digest, so the gate
    // names the matrix by file alone.
    use whizbang_eval::gate::Compared;
    let identity =
        "matrix_identity = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"\n";
    let three = FILE
        .replace("gate_version = 2", "gate_version = 3")
        .replace(identity, "");
    let gate = Gate::parse(&three).unwrap();
    assert_eq!(gate.run.matrix_identity, None);
    assert_eq!(
        gate.compared().unwrap(),
        Compared::Arms {
            control: "C".into(),
            treatment: "F".into()
        }
    );
    // Version 3 refuses version 2's identity, and version 2 still needs it.
    let refused = Gate::parse(&FILE.replace("gate_version = 2", "gate_version = 3")).unwrap_err();
    assert!(refused.contains("by file alone"), "{refused}");
    assert!(Gate::parse(&FILE.replace(identity, "")).is_err());
    // Two versions of a set under one arm, in version 3 alone.
    let sets = "arm = \"C\"\ncontrol_set = \"source-fidelity/plain-v1\"\n\
                treatment_set = \"source-fidelity/full-v1\"\n";
    let versions = three.replace("control = \"C\"\ntreatment = \"F\"\n", sets);
    assert_eq!(
        Gate::parse(&versions).unwrap().compared().unwrap(),
        Compared::Sets {
            arm: "C".into(),
            control: "source-fidelity/plain-v1".into(),
            treatment: "source-fidelity/full-v1".into()
        }
    );
    let in_two = FILE.replace("control = \"C\"\ntreatment = \"F\"\n", sets);
    assert!(Gate::parse(&in_two).is_err());
    for refused in [
        // Two arms and two sets at once; a set without its twin; two use cases; one set.
        versions.replace(
            "arm = \"C\"",
            "control = \"C\"\ntreatment = \"F\"\narm = \"C\"",
        ),
        versions.replace("treatment_set = \"source-fidelity/full-v1\"\n", ""),
        versions.replace("source-fidelity/full-v1", "reality-checks/full-v1"),
        versions.replace("source-fidelity/full-v1", "source-fidelity/plain-v1"),
        versions.replace("source-fidelity/full-v1", "full-v1"),
    ] {
        assert!(Gate::parse(&refused).is_err(), "{refused}");
    }
}

#[test]
fn a_catch_rate_gate_resamples_the_families_with_a_failure() {
    // Plan mE, task M1, item 14: the study's measure. Twenty families with two items
    // labelled fail, each caught by the treatment in both repetitions where the control
    // caught it in one; and twenty families of passes alone, which are not resampled.
    use Computed::{Fail, Pass};
    use Expected::{Fail as F, Pass as P};
    let identity =
        "matrix_identity = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"\n";
    let catch = FILE
        .replace("gate_version = 2", "gate_version = 3")
        .replace(identity, "")
        .replace(
            "primary = \"balanced_accuracy\"",
            "primary = \"catch_rate\"",
        )
        .replace(
            "count = 1\ncombine = \"first\"",
            "count = 2\ncombine = \"mean\"",
        );
    let gate = Gate::parse(&catch).unwrap();
    let (mut c, mut x) = (Vec::new(), Vec::new());
    for f in 0..20 {
        let fam = format!("fail{f:02}");
        for i in 0..2 {
            let id = format!("{fam}-{i}");
            c.push(row(&id, &fam, 1, F, Fail));
            c.push(row(&id, &fam, 2, F, Pass));
            x.push(row(&id, &fam, 1, F, Fail));
            x.push(row(&id, &fam, 2, F, Fail));
        }
        let fam = format!("pass{f:02}");
        let id = format!("{fam}-0");
        c.push(row(&id, &fam, 1, P, Pass));
        x.push(row(&id, &fam, 1, P, Pass));
    }
    let judged = whizbang_eval::gate::Judged {
        backend: "b".into(),
        model: "m".into(),
        control: c.clone(),
        treatment: x.clone(),
    };
    let scored = gate.score(&[judged]).unwrap();
    let one = &scored.judges[0];
    let caught = one.catch_rate.as_ref().unwrap();
    // The families with a failure, the items labelled fail, and the mean over repetitions.
    assert_eq!((caught.families, caught.items), (20, 40));
    assert_eq!((caught.control, caught.treatment), (0.5, 1.0));
    let gain = caught.gain.as_ref().unwrap();
    assert_eq!((gain.point, gain.families), (0.5, 20));
    // Balanced accuracy, by repetition 1, sees no gain; the catch rate admits it.
    assert_eq!(one.paired.gain.as_ref().unwrap().point, 0.0);
    assert_eq!(scored.outcome, Outcome::Admitted, "{:?}", one.failed);
    // Repetition 1 alone, combined first: no gain, rejected.
    let first = Gate::parse(&catch.replace(
        "count = 2\ncombine = \"mean\"",
        "count = 2\ncombine = \"first\"",
    ))
    .unwrap();
    let judged = whizbang_eval::gate::Judged {
        backend: "b".into(),
        model: "m".into(),
        control: c,
        treatment: x,
    };
    let scored = first.score(&[judged]).unwrap();
    assert_eq!(scored.judges[0].catch_rate.as_ref().unwrap().treatment, 1.0);
    assert_eq!(scored.judges[0].catch_rate.as_ref().unwrap().control, 1.0);
    assert_ne!(scored.outcome, Outcome::Admitted);
    // A mean is a catch-rate gate's, and the catch rate is version 3's.
    let mean_ba = catch.replace(
        "primary = \"catch_rate\"",
        "primary = \"balanced_accuracy\"",
    );
    assert!(Gate::parse(&mean_ba).is_err());
    assert!(
        Gate::parse(&FILE.replace(
            "primary = \"balanced_accuracy\"",
            "primary = \"catch_rate\""
        ))
        .is_err()
    );
}
