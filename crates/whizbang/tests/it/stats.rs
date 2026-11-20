//! `stats::…`: `whizbang runs stats --unsure-by rule` over the runs of a matrix under a
//! judge served on loopback (plan m3, task U1, item 1). The set is made in the test.

use crate::fetch::SET;
use crate::matrix::{
    PLENTY, SF1, SF2, checks, items, matrix_with, never, ran, run_matrix, world_with,
};
use crate::wired::whizbang;

const ITEMS: [&str; 3] = ["porch-alpha", "porch-bravo", "porch-charlie"];

/// A store holding three runs of `check` under arm F: the first unsure for `SF2` held
/// tentatively; the second unsure for `SF1` that it cannot tell and a tentative violation
/// of `SF2`; the third a pass.
fn judged() -> crate::anthropic::Judged {
    let (judged, tiny) = world_with("stats-unsure", &ITEMS);
    let matrix = matrix_with(SET, "\"F\"");
    let answers = vec![
        items(&[(SF1, "held", "confident"), (SF2, "held", "tentative")]),
        items(&[
            (SF1, "cant_tell", "tentative"),
            (SF2, "violation", "tentative"),
        ]),
        items(&[(SF1, "held", "confident"), (SF2, "held", "confident")]),
    ];
    let (report, _) = run_matrix(&judged, answers, &|e| {
        ran(e, &judged, &tiny, &matrix, PLENTY, &never)
    });
    assert_eq!(report.unwrap().verdicts.get("unsure"), Some(&2));
    judged
}

#[test]
fn unsure_by_rule_counts_each_unsure_verdict_by_the_rule_that_made_it() {
    let judged = judged();
    let stats = whizbang(
        &["runs", "stats", "--action", "check", "--unsure-by", "rule"],
        &judged.world.root,
    );
    assert_eq!(stats.code, Some(0), "{}", stats.err);
    // The ordinary report first, then the count.
    assert!(
        stats
            .out
            .contains("verdicts: 3 (pass 1, fail 0, unsure 2); unsure rate 66.7%\n")
    );
    let (_, count) = stats.out.split_once("unsure by rule: ").unwrap();
    assert_eq!(
        count,
        "2 unsure verdicts, 0 of them with no checklist kept\n\
         use_case\trule\tanswer\tdisposition\tconfidence\tanswers\tverdicts\n\
         source-fidelity\tSF1\titem\tcant_tell\ttentative\t1\t1\n\
         source-fidelity\tSF2\titem\theld\ttentative\t1\t1\n\
         source-fidelity\tSF2\titem\tviolation\ttentative\t1\t1\n"
    );
    // Without the flag, no count; with a filter no run passes, an empty one.
    let plain = whizbang(&["runs", "stats", "--action", "check"], &judged.world.root);
    assert!(!plain.out.contains("unsure by rule"));
    let none = whizbang(
        &[
            "runs",
            "stats",
            "--use-case",
            "reality-checks",
            "--unsure-by",
            "rule",
        ],
        &judged.world.root,
    );
    assert!(none.out.ends_with(
        "unsure by rule: 0 unsure verdicts, 0 of them with no checklist kept\n\
         use_case\trule\tanswer\tdisposition\tconfidence\tanswers\tverdicts\n"
    ));
    let refused = whizbang(
        &["runs", "stats", "--unsure-by", "item"],
        &judged.world.root,
    );
    assert_eq!(refused.code, Some(2));
}

#[test]
fn unsure_by_rule_prints_aggregates_and_no_item() {
    let judged = judged();
    let root = &judged.world.root;
    let stats = whizbang(&["runs", "stats", "--unsure-by", "rule"], root);
    assert_eq!(stats.code, Some(0), "{}", stats.err);
    // No item, set, run, reason or picture is named.
    for item in ITEMS {
        assert!(!stats.out.contains(item), "{item}");
    }
    assert!(!stats.out.contains(SET));
    assert!(!stats.out.contains("Looked at."));
    assert!(!stats.out.contains("sha256:"));
    assert!(!stats.out.contains("raw:candidate"));
    for (run, _) in checks(root) {
        assert!(!stats.out.contains(&run), "{run}");
    }
}

#[test]
fn runs_stats_counts_a_loops_attempts_and_retries() {
    use crate::checked::{Looped, Setup, passes, remains};
    use whizbang_core::contract::Value;
    // Plan m4, task L1, item 11. Two runs of the loop in one store: a fail, a retry
    // with a wider mask, and a pass; and one unchecked. They run in memory, and the
    // store's files are written to the disk once, for the binary to read.
    let setup = Setup::answering(vec![remains(), passes()]);
    let world = Looped::new("stats-loop", &setup);
    let retried = world.run(&world.call());
    world.run(&world.call().argument("check", Value::text("off")));
    world.spill();
    // What a door says of the run that retried, from its record.
    let mut said = String::new();
    whizbang::door::said_loop(&mut said, &world.root, retried.run());
    assert!(
        said.starts_with(
            "attempts: 2 made, attempt 2 kept\nended: attempt 2 passed\nchecked: yes\n\
             verdict: pass\n"
        ),
        "{said}"
    );
    let stats = whizbang(
        &["runs", "stats", "--action", "remove_checked"],
        &world.root,
    );
    assert_eq!(stats.code, Some(0), "{}", stats.err);
    assert!(stats.out.starts_with("runs: 2\n"), "{}", stats.out);
    assert!(
        stats
            .out
            .contains("verdicts: 1 (pass 1, fail 0, unsure 0); unsure rate 0.0%\n"),
        "{}",
        stats.out
    );
    let (_, loops) = stats.out.split_once("loops: ").unwrap();
    let loops: Vec<&str> = loops.lines().take(3).collect();
    assert_eq!(
        loops,
        [
            "2 runs, 3 attempts, 1 retries; kept an attempt that was checked 1, not checked 1, \
             none 0",
            "  attempts ended\t1\tattempt N passed",
            "  attempts ended\t1\tone unchecked pass: the check is off",
        ]
    );
    // `runs show` prints a loop's attempts, after the run's calls.
    let shown = crate::wired::shown(&world.root, retried.run().as_str());
    let (_, said) = shown
        .split_once("attempts: 2\n")
        .expect("the attempts are shown");
    let lines: Vec<&str> = said.lines().take(4).collect();
    assert!(lines[0].starts_with("  attempt 1\tfail\tnothing changed\t4 runs\tsettled $"));
    assert!(lines[1].starts_with("  attempt 2\tpass\ta wider mask, grow px 8\t3 runs\tsettled $"));
    assert_eq!(
        lines[2..],
        ["  kept: attempt 2", "  ended: attempt 2 passed"]
    );
    // Runs that are no loop's say nothing of loops.
    let other = whizbang(&["runs", "stats", "--action", "grow"], &world.root);
    assert!(!other.out.contains("loops:"), "{}", other.out);
}
