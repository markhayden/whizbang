//! `edits_gate::…`: an edit recipe's gate, `edit-gate/1`, decided on what its cases' runs
//! did (plan mE, task M2, item 1). Pure arithmetic: no store, no picture.

use whizbang_eval::edit_gate::{
    Cases, EditGate, Finished, GroupRead, Observation, Observed, decide, normal_quantile,
};

const PIN: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

fn gate_text(rung: u8, from: &str, min_families: usize) -> String {
    format!(
        "gate = \"edit-gate/1\"\nname = \"e-edit-rung3-remove\"\nrecipe = \"remove\"\n\
         rung = {rung}\ncorpus = \"owners/fresh-edits\"\ncases = \"cases.toml\"\ncases_sha256 = \"{PIN}\"\nhypotheses = 6\n\
         level = 0.95\nseed = 20261125\nmin_families = {min_families}\n\n\
         [success]\nfrom = \"{from}\"\nbar = 0.75\nlower_above = 0.4\n\n\
         [preservation]\nexact = true\n\n\
         [quality]\nprotocol = \"protocol.toml\"\nprotocol_sha256 = \"{PIN}\"\n\
         outcomes = [\"preferred\", \"not_worse\"]\nmin_picked = 8\n\n\
         [errors]\nbelow = 0.10\n\n[cost]\nguide_micros = 40000\ntimes = 2.0\n\n\
         [latency]\ntimes = 2.0\n\n[[latency.record]]\npixels = 1000000\nms = 1000\n\n\
         [[latency.record]]\npixels = 4000000\nms = 4000\n"
    )
}

fn gate() -> EditGate {
    EditGate::parse(&gate_text(3, "check", 4)).unwrap()
}

fn good() -> Finished {
    Finished {
        success: Some(true),
        changed_outside: 0,
        local_ms: 900,
        backend_ms: 30_000,
        pixels: 1_000_000,
    }
}

/// `n` cases, each in a family of its own, every one a good edit.
fn cases(n: usize) -> Vec<Observation> {
    (0..n)
        .map(|i| Observation {
            family: format!("f{i:02}"),
            observed: Observed::Finished(good()),
            spent_micros: Some(40_000),
        })
        .collect()
}

fn decoded(outcome: &str, picked: u32) -> Option<(u32, Vec<GroupRead>)> {
    Some((
        picked,
        vec![GroupRead {
            group: "all".to_owned(),
            pairs: picked,
            decided: picked,
            wins: picked,
            losses: 0,
            same: 0,
            neither: 0,
            outcome: outcome.to_owned(),
        }],
    ))
}

/// What breaks one measure of a round's cases.
type Breaking = fn(&mut Vec<Observation>);

fn finished(observation: &mut Observation) -> &mut Finished {
    match &mut observation.observed {
        Observed::Finished(f) => f,
        Observed::Error => panic!("an error"),
    }
}

#[test]
fn a_recipe_passes_only_with_every_measure() {
    let gate = gate();
    // Eight good edits in eight families, picked as preferred: every measure passes.
    let passed = decide(&gate, &cases(8), decoded("preferred", 8)).unwrap();
    assert_eq!(passed.outcome, "admitted", "{:?}", passed.why);
    assert!(passed.why.is_empty());
    assert_eq!(passed.success.successes, 8);
    assert!(passed.success.lower.unwrap() > 0.4);
    // Each measure broken alone rejects it, and says which.
    let broken: [(&str, Breaking); 5] = [
        ("a_pixel_outside_the_mask_changed", |o| {
            finished(&mut o[3]).changed_outside = 1;
        }),
        ("operational_errors_over_their_bar", |o| {
            o[0].observed = Observed::Error;
            o[1].observed = Observed::Error;
        }),
        ("cost_over_its_limit", |o| {
            for one in o.iter_mut() {
                one.spent_micros = Some(80_001);
            }
        }),
        ("latency_over_its_limit", |o| {
            for one in o.iter_mut() {
                // Of the larger size: 4,000 ms is its record, so 8,001 is past twice.
                finished(one).pixels = 3_900_000;
                finished(one).local_ms = 8_001;
            }
        }),
        ("task_success_below_its_bar", |o| {
            for one in o.iter_mut().take(3) {
                finished(one).success = Some(false);
            }
        }),
    ];
    for (why, breaking) in broken {
        let mut observations = cases(8);
        breaking(&mut observations);
        let decided = decide(&gate, &observations, decoded("preferred", 8)).unwrap();
        assert_eq!(decided.outcome, "rejected", "{why}");
        assert!(
            decided.why.iter().any(|w| w == why),
            "{why}: {:?}",
            decided.why
        );
    }
    // The backends' time is reported, never gated.
    let mut slow_backends = cases(8);
    for one in &mut slow_backends {
        finished(one).backend_ms = 10_000_000;
    }
    let decided = decide(&gate, &slow_backends, decoded("preferred", 8)).unwrap();
    assert_eq!(decided.outcome, "admitted");
    // Blinded quality: worse rejects; not_worse passes when the gate allows it.
    let worse = decide(&gate, &cases(8), decoded("worse", 8)).unwrap();
    assert_eq!(worse.outcome, "rejected");
    assert!(
        worse
            .why
            .contains(&"blinded_quality_below_its_bar".to_owned())
    );
    let alike = decide(&gate, &cases(8), decoded("not_worse", 8)).unwrap();
    assert_eq!(alike.outcome, "admitted");
    // Share at its bar but a lower bound not above 0.4: six of eight, R5's sample.
    let mut six = cases(8);
    for one in six.iter_mut().take(2) {
        finished(one).success = Some(false);
    }
    let decided = decide(&gate, &six, decoded("preferred", 8)).unwrap();
    assert!((decided.success.share - 0.75).abs() < 1e-12);
    assert_eq!(decided.outcome, "rejected", "{:?}", decided.success);
    assert_eq!(decided.why, ["task_success_bound_not_above_its_bar"]);
}

#[test]
fn too_few_runs_is_inconclusive() {
    let gate = gate();
    // Three families of the four the gate needs, every edit good: inconclusive, though
    // the bound of so few would also miss its bar.
    let decided = decide(&gate, &cases(3), decoded("preferred", 8)).unwrap();
    assert_eq!(decided.outcome, "inconclusive");
    assert!(decided.why.contains(&"too_few_families".to_owned()));
    // Eight cases, but in two families: still too few families.
    let mut two = cases(8);
    for (i, one) in two.iter_mut().enumerate() {
        one.family = format!("f{}", i % 2);
    }
    assert_eq!(
        decide(&gate, &two, decoded("preferred", 8))
            .unwrap()
            .outcome,
        "inconclusive"
    );
    // An edit with no check or mark is unscored, never counted a success or a failure.
    let mut unscored = cases(8);
    finished(&mut unscored[0]).success = None;
    let decided = decide(&gate, &unscored, decoded("preferred", 8)).unwrap();
    assert_eq!(decided.outcome, "inconclusive");
    assert_eq!(decided.success.unscored, 1);
    // No decode, too few pairs picked, or a group the decode leaves inconclusive.
    for quality in [None, decoded("preferred", 7), decoded("inconclusive", 8)] {
        let decided = decide(&gate, &cases(8), quality).unwrap();
        assert_eq!(decided.outcome, "inconclusive", "{:?}", decided.why);
    }
    // Nothing finished: never a pass.
    let none: Vec<Observation> = (0..4)
        .map(|i| Observation {
            family: format!("f{i}"),
            observed: Observed::Error,
            spent_micros: None,
        })
        .collect();
    assert_ne!(
        decide(&gate, &none, decoded("preferred", 8))
            .unwrap()
            .outcome,
        "admitted"
    );
}

#[test]
fn the_lower_bound_is_the_lower_of_the_corrected_bootstrap_and_wilson() {
    // The corrected quantile of six hypotheses at 95%: 1 - 0.025 / 6.
    assert!((normal_quantile(0.975) - 1.959_964).abs() < 1e-5);
    assert!((normal_quantile(1.0 - 0.025 / 6.0) - 2.638_257).abs() < 1e-5);
    assert!((normal_quantile(0.01) + 2.326_348).abs() < 1e-5);
    let gate = gate();
    let decided = decide(&gate, &cases(8), decoded("preferred", 8)).unwrap();
    let success = &decided.success;
    // Every family succeeded, so every resample is 1; Wilson's bound is below it.
    assert_eq!(success.bootstrap_lower, Some(1.0));
    let wilson = success.wilson_lower.unwrap();
    assert!(wilson < 1.0 && wilson > 0.5, "{wilson}");
    assert_eq!(success.lower, Some(wilson));
    assert_eq!(success.resamples, 20_000);
    assert!((success.lower_percentile - 0.025 / 6.0).abs() < 1e-12);
    // One hypothesis gives a higher bound than six.
    let alone = EditGate {
        hypotheses: 1,
        ..gate.clone()
    };
    let one = decide(&alone, &cases(8), decoded("preferred", 8)).unwrap();
    assert!(one.success.wilson_lower.unwrap() > wilson);
}

#[test]
fn a_gate_file_states_what_its_rung_needs() {
    let refused = |text: String, why: &str| {
        let err = EditGate::parse(&text).unwrap_err();
        assert!(err.contains(why), "{err}");
    };
    // A rung 4 gate rests on a person's marks, never on a check (section 8).
    refused(gate_text(4, "check", 4), "rung 4");
    assert!(EditGate::parse(&gate_text(4, "marks", 4)).is_ok());
    refused(gate_text(5, "marks", 4), "rung is 3 or 4");
    refused(gate_text(3, "check", 0), "min_families");
    refused(
        gate_text(3, "check", 4).replace("exact = true", "exact = false"),
        "exact",
    );
    refused(
        gate_text(3, "check", 4).replace("\"not_worse\"", "\"worse\""),
        "outcomes",
    );
    refused(
        gate_text(3, "check", 4) + "\n[admits]\nrung = 3\nkind = \"recipe\"\nname = \"repair\"\n",
        "its own recipe",
    );
    refused(
        gate_text(3, "check", 4).replace("cases.toml", "../cases.toml"),
        "beside it",
    );
}

#[test]
fn a_cases_file_names_each_run_once() {
    let text = |runs: [&str; 2], what: &str| {
        format!(
            "kind = \"edit-cases/1\"\naction = \"remove\"\n\n\
             [[case]]\nid = \"c01\"\nfamily = \"f01\"\nrun = \"{}\"\nwhat = \"{what}\"\n\n\
             [[case]]\nid = \"c02\"\nfamily = \"f01\"\nrun = \"{}\"\n",
            runs[0], runs[1]
        )
    };
    let cases = Cases::parse(&text(["e-r5-c01", "e-r5-c02"], "a mug")).unwrap();
    assert_eq!(cases.families(), 1);
    for bad in [
        text(["e-r5-c01", "e-r5-c01"], "a mug"),
        text(["e r5", "e-r5-c02"], "a mug"),
        text(["e-r5-c01", "e-r5-c02"], "two\\nlines"),
    ] {
        assert!(Cases::parse(&bad).is_err(), "{bad}");
    }
}

/// `n` cases in families of their own, the first `fails` of them failures.
fn of(n: usize, fails: usize) -> Vec<Observation> {
    let mut out = cases(n);
    for one in out.iter_mut().take(fails) {
        finished(one).success = Some(false);
    }
    out
}

#[test]
fn what_clears_r5s_and_r6s_bars() {
    // R5: 8 cases at up to 8 hypotheses: 7 of 8 is admitted, 6 of 8 is not.
    for hypotheses in [6, 8] {
        let gate = EditGate {
            hypotheses,
            ..gate()
        };
        let seven = decide(&gate, &of(8, 1), decoded("preferred", 8)).unwrap();
        assert_eq!(
            seven.outcome, "admitted",
            "{hypotheses}: {:?}",
            seven.success
        );
        let six = decide(&gate, &of(8, 2), decoded("preferred", 8)).unwrap();
        assert_eq!(six.outcome, "rejected", "{hypotheses}");
    }
    // R6: 10 cases at 6 hypotheses: 9 of 10 is admitted, 8 of 10 is not, and 9 of 10 with
    // the tenth an operational error is not, since 1 of 10 is not below 0.10.
    let gate = EditGate {
        min_families: 8,
        ..gate()
    };
    assert_eq!(
        decide(&gate, &of(10, 1), decoded("preferred", 10))
            .unwrap()
            .outcome,
        "admitted"
    );
    assert_eq!(
        decide(&gate, &of(10, 2), decoded("preferred", 10))
            .unwrap()
            .outcome,
        "rejected"
    );
    let mut errored = cases(10);
    errored[0].observed = Observed::Error;
    let decided = decide(&gate, &errored, decoded("preferred", 10)).unwrap();
    assert_eq!(decided.outcome, "rejected");
    assert!(
        decided
            .why
            .contains(&"operational_errors_over_their_bar".to_owned())
    );
}

#[test]
fn a_recipe_that_has_lost_is_rejected_before_it_is_picked() {
    let gate = gate();
    // Four failures of eight with one unscored: even that one a success, 5 of 8 misses
    // the bar, so no pairs are needed to say so.
    let mut lost = of(8, 4);
    finished(&mut lost[7]).success = None;
    let decided = decide(&gate, &lost, None).unwrap();
    assert_eq!(decided.outcome, "rejected", "{:?}", decided.why);
    // One failure and one unscored: it could still clear its bars, so inconclusive.
    let mut open = of(8, 1);
    finished(&mut open[7]).success = None;
    assert_eq!(decide(&gate, &open, None).unwrap().outcome, "inconclusive");
    // On too few families a miss stays inconclusive.
    assert_eq!(
        decide(&gate, &of(3, 3), None).unwrap().outcome,
        "inconclusive"
    );
    // A failed run's spend counts toward the mean cost.
    let mut costly = cases(8);
    costly[0].observed = Observed::Error;
    costly[0].spent_micros = Some(700_000);
    let decided = decide(&gate, &costly, decoded("preferred", 8)).unwrap();
    assert!(decided.why.contains(&"cost_over_its_limit".to_owned()));
}

#[test]
fn a_rung_4_gate_names_a_fresh_corpus() {
    let text = gate_text(4, "marks", 4);
    for exposed in whizbang_eval::edit_gate::EXPOSED_CORPORA {
        let on = text.replace("owners/fresh-edits", exposed);
        let refused = EditGate::parse(&on).unwrap_err();
        assert!(refused.contains("fresh"), "{refused}");
        // A rung 3 gate may run on one.
        let three = gate_text(3, "check", 4).replace("owners/fresh-edits", exposed);
        assert!(EditGate::parse(&three).is_ok());
    }
}
