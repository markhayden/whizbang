//! `stats::…`: `whizbang runs stats --unsure-by rule` over the runs of a matrix under a
//! judge served on loopback (plan m3, task U1, item 1). The set is made in the test.

use crate::fetch::SET;
use crate::matrix::{
    PLENTY, SF1, SF2, checks, items, matrix_with, never, ran, run_matrix, world_with,
};
use crate::wired::whizbang;
use std::sync::Arc;
use whizbang_core::contract::{Call, Executor, Value};
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;

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
fn rejections_by_class_and_cause() {
    // Plan e1, task T1, item 3. Three runs that kept what `check` keeps of its rejections,
    // written by a fake action of the test: the first repaired after an item was left
    // out; the second left it out twice and failed; the third whole at once.
    let scratch = whizbang_testkit::scratch::Scratch::new("stats-rejections");
    let root = scratch.path().to_path_buf();
    let store = Arc::new(Store::open(&root, Arc::new(RealFs)).unwrap());
    let mut registry = whizbang::wiring::registry().unwrap();
    registry.register(Arc::new(Keeps::new())).unwrap();
    let executor = crate::runner(registry, store);
    let missing = serde_json::json!({"turn": 1, "class": "missing_item"});
    let again = serde_json::json!({"turn": 2, "class": "missing_item"});
    for (rejections, cause) in [
        (vec![missing.clone()], "concluded"),
        (vec![missing, again], "invalid_submission"),
        (vec![], "concluded"),
    ] {
        let kept = serde_json::json!({"contract": "rejections/2", "submission": "v1",
            "rejections": rejections, "recoveries": [], "cause": cause});
        let call = Call::new("keeps").argument("kept", Value::text(&kept.to_string()));
        assert_eq!(
            executor.submit(&call).unwrap().outcome().name(),
            "completed"
        );
    }
    drop(executor);
    let root = &root;
    let stats = whizbang(&["runs", "stats", "--rejections"], root);
    assert_eq!(stats.code, Some(0), "{}", stats.err);
    let (_, counted) = stats.out.split_once("rejections: ").unwrap();
    assert_eq!(
        counted,
        "3 in 2 of 3 runs that kept them; 0 runs of check under a use case kept none\n\
         \x20 cause\tconcluded\t2\n\
         \x20 cause\tinvalid_submission\t1\n\
         \x20 cause\tno_submission_after_repair\t0\n\
         \x20 cause\tcut_off\t0\n\
         \x20 cause\tceiling\t0\n\
         \x20 cause\trefused\t0\n\
         \x20 cause\tturns\t0\n\
         \x20 cause\tfault\t0\n\
         \x20 cause\tother\t0\n\
         \x20 cause\tunknown\t0\n\
         \x20 class\tnot_a_list\t0\n\
         \x20 class\tunknown_item\t0\n\
         \x20 class\tmissing_item\t3\n\
         \x20 class\tduplicate_item\t0\n\
         \x20 class\tbad_field\t0\n\
         \x20 class\tlocation\t0\n\
         \x20 class\tallowed_by\t0\n\
         \x20 class\tfinding_cites\t0\n\
         \x20 class\tevidence\t0\n\
         \x20 class\tnot_alone\t0\n\
         \x20 class\tno_submission\t0\n\
         \x20 class\tunknown\t0\n"
    );
    // No run is named; without the flag, no count.
    assert!(!stats.out.contains("chk:"));
    for (run, _) in crate::matrix::runs_of(root, "keeps") {
        assert!(!stats.out.contains(&run), "{run}");
    }
    let plain = whizbang(&["runs", "stats"], root);
    assert!(!plain.out.contains("rejections:"), "{}", plain.out);
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

/// A fake action of these tests, `keeps`: it writes the text it is given as the diagnostic
/// `check` keeps of its rejections, and completes.
struct Keeps {
    descriptor: whizbang_core::contract::ActionDescriptor,
}

impl Keeps {
    fn new() -> Keeps {
        use whizbang_core::contract::{ActionDescriptor, Argument, Kind, ValueType};
        let draft = ActionDescriptor::draft(
            "keeps",
            "1.0.0",
            Kind::Primitive,
            "Keeps the text it is given as a run's rejections; a fake for tests.",
        )
        .argument(Argument::required("kept", "What to keep.", ValueType::Text));
        Keeps {
            descriptor: draft.build().unwrap(),
        }
    }
}

impl whizbang_core::contract::Action for Keeps {
    fn descriptor(&self) -> &whizbang_core::contract::ActionDescriptor {
        &self.descriptor
    }

    fn run(
        &self,
        call: &whizbang_core::contract::PreparedCall,
        context: &whizbang_core::contract::Context,
    ) -> Result<whizbang_core::contract::Produced, whizbang_core::contract::Failure> {
        let kept = call
            .arguments()
            .iter()
            .find_map(|(name, value)| match value {
                Value::Text(text) if name == "kept" => Some(text.clone()),
                _ => None,
            });
        let kept = kept.unwrap_or_default();
        let written = context
            .diagnostics()
            .write(whizbang_squint::check::REJECTIONS, kept.as_bytes());
        written.map_err(|why| whizbang_core::contract::Failure::error("kept", &why))?;
        Ok(whizbang_core::contract::Produced::nothing())
    }
}
