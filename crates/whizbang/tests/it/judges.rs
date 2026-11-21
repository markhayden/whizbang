//! `judges::…`: `whizbang eval judges` and its two rules (plan e1, task S1). The rules and the
//! measures are pure functions, held here over populations made in the test; the reading of
//! the store, the labels and `--out` over a set made in the test, with nothing run. 200
//! resamples.

use std::path::Path;

use whizbang::judges::{
    self, Asked, DEFAULT_JUDGE, Decision, Mode, Population, Reading, Rule, Scored, Submissions,
};
use whizbang_eval::metrics::{Computed, Expected, Resampling};

use crate::fetch::{SET, Tiny, tiny};
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::scratch::Scratch;

/// A scratch folder with an empty store in it, `store`: nothing more is needed to score.
struct World {
    dir: Scratch,
    root: std::path::PathBuf,
}

impl World {
    fn new(label: &str) -> World {
        let dir = Scratch::new(label);
        let root = dir.path().join("store");
        drop(Store::open(&root, std::sync::Arc::new(RealFs)).unwrap());
        World { dir, root }
    }

    fn beside(&self, name: &str) -> std::path::PathBuf {
        self.dir.path().join(name)
    }
}

fn how() -> Resampling {
    Resampling {
        seed: 11,
        resamples: 200,
        hypotheses: 1,
        alpha: 0.05,
    }
}

/// A clean run's diagnostic, as task T1 keeps it: no rejection.
fn clean() -> Option<Submissions> {
    Some(Submissions {
        classes: Vec::new(),
        cause: "concluded".to_owned(),
        recoveries: Vec::new(),
    })
}

/// One decision of `judge` (its backend and its model) under arm C, repetition 1, at a
/// cent and 100 ms.
fn decision(
    judge: &str,
    item: &str,
    family: &str,
    expected: Expected,
    computed: Computed,
) -> Decision {
    Decision {
        set: SET.to_owned(),
        item: item.to_owned(),
        family: family.to_owned(),
        use_case: "source-fidelity".to_owned(),
        split: "dev".to_owned(),
        backend: judge.to_owned(),
        model: judge.to_owned(),
        arm: "C".to_owned(),
        repetition: 1,
        expected,
        computed,
        cost_usd: 0.01,
        settled_usd: 0.01,
        wall_ms: 100,
        submissions: clean(),
    }
}

/// `families` families, each of an expected fail `f<n>` and an expected pass `p<n>`, all
/// decided correctly by `judge`, at `cost` a decision.
fn perfect(judge: &str, families: usize, cost: f64) -> Vec<Decision> {
    let mut out = Vec::new();
    for n in 0..families {
        let family = format!("fam{n}");
        for (item, expected, computed) in [
            (format!("f{n}"), Expected::Fail, Computed::Fail),
            (format!("p{n}"), Expected::Pass, Computed::Pass),
        ] {
            let mut d = decision(judge, &item, &family, expected, computed);
            d.cost_usd = cost;
            out.push(d);
        }
    }
    out
}

/// The same, with each listed item's verdict turned to `computed`.
fn with(mut decisions: Vec<Decision>, items: &[&str], computed: Computed) -> Vec<Decision> {
    for d in &mut decisions {
        if items.contains(&d.item.as_str()) {
            d.computed = computed;
        }
    }
    decisions
}

fn complete(decisions: Vec<Decision>) -> Population {
    Population {
        cells: decisions.len(),
        decisions,
        open: 0,
        unlabelled: 0,
    }
}

fn judged(population: &Population) -> Scored {
    let asked = Asked {
        mode: Mode::Judges,
        arm: Some("C".to_owned()),
        treatment: None,
        rule: Some(Rule::JudgePick),
        resampling: how(),
    };
    judges::score(population, &asked).unwrap()
}

fn pick_of<'a>(scored: &'a Scored, scope: &str) -> &'a judges::Pick {
    scored.picks.iter().find(|p| p.scope == scope).unwrap()
}

#[test]
fn the_pick_is_the_cheapest_judge_within_the_margin() {
    let mut all = Vec::new();
    // Dear and perfect: the best.
    all.extend(perfect("dear", 30, 0.03));
    // Cheap and perfect: within the margin, and the cheapest of those that are.
    all.extend(perfect("cheap", 30, 0.003));
    // Cheaper still, but two errors in 60 decisions are not under 2%: not eligible.
    all.extend(with(
        perfect("erring", 30, 0.0005),
        &["p3", "p4"],
        Computed::Error,
    ));
    // Cheaper still, but three times as slow as the fastest: outside the margin.
    let mut slow = perfect("slow", 30, 0.001);
    for d in &mut slow {
        d.wall_ms = 300;
    }
    all.extend(slow);
    // Eligible (decided correctly 50 of 60), but its balanced accuracy is far below the
    // best's: outside the margin.
    let missed: Vec<String> = (0..10).map(|n| format!("f{n}")).collect();
    let missed: Vec<&str> = missed.iter().map(String::as_str).collect();
    all.extend(with(
        perfect("sloppy", 30, 0.0001),
        &missed,
        Computed::Unsure,
    ));
    let scored = judged(&complete(all));
    for scope in ["all", "source-fidelity"] {
        let pick = pick_of(&scored, scope);
        assert_eq!(
            (pick.outcome.as_str(), pick.model.as_str()),
            ("pick", "cheap"),
            "{pick:?}"
        );
        let clauses = |model: &str| pick.judges.iter().find(|c| c.model == model).unwrap();
        assert!(clauses("dear").within_the_margin && clauses("cheap").within_the_margin);
        let erring = clauses("erring");
        assert!(!erring.errors_under_2_percent && !erring.eligible && !erring.within_the_margin);
        let slow = clauses("slow");
        assert!(slow.eligible && !slow.median_time_at_most_twice_the_fastest);
        let sloppy = clauses("sloppy");
        assert!(sloppy.eligible && sloppy.decided_correctly_at_least_0_80);
        assert!(!sloppy.lower_bound_at_least_minus_3_points, "{sloppy:?}");
        assert!(!sloppy.decided_correctly_within_5_points_of_the_best);
    }
    // Only the two scopes the rule is applied over.
    assert_eq!(scored.picks.len(), 2);

    // Two judges of one cost: the lower false-reject rate wins the tie.
    let mut tied = perfect("first", 100, 0.002);
    tied = with(tied, &["p0"], Computed::Fail);
    tied.extend(perfect("second", 100, 0.002));
    let pick = pick_of(&judged(&complete(tied)), "all").clone();
    assert_eq!(pick.model, "second", "{pick:?}");
}

#[test]
fn the_best_is_reselected_inside_each_resample() {
    // Two judges almost alike: `a` misses three fails, `b` four, each in families of its
    // own. Measured against `a` fixed as the best, `a`'s difference would be 0 in every
    // resample; with the best taken again in each, the resamples where `b` is ahead give
    // `a` a difference below 0.
    let a = with(perfect("a", 40, 0.01), &["f0", "f1", "f2"], Computed::Pass);
    let b = with(
        perfect("b", 40, 0.01),
        &["f10", "f11", "f12", "f13"],
        Computed::Pass,
    );
    let scored = judged(&complete(a.into_iter().chain(b).collect()));
    let pick = pick_of(&scored, "all");
    let a = pick.judges.iter().find(|c| c.model == "a").unwrap();
    assert_eq!(a.difference_from_best_point, Some(0.0));
    let lower = a.difference_from_best_lower.unwrap();
    assert!(lower < 0.0, "{lower}");
    let b = pick.judges.iter().find(|c| c.model == "b").unwrap();
    assert!(b.difference_from_best_point.unwrap() < 0.0);
}

#[test]
fn an_incomplete_matrix_is_no_pick() {
    type Spoil = Box<dyn Fn(&mut Population)>;
    let cases: [(&str, Spoil); 3] = [
        ("not ended", Box::new(|p: &mut Population| p.open = 1)),
        ("no label", Box::new(|p: &mut Population| p.unlabelled = 1)),
        (
            "no decision of repetition 1",
            Box::new(|p: &mut Population| {
                p.decisions
                    .retain(|d| !(d.model == "cheap" && d.item == "p0"));
            }),
        ),
    ];
    for (said, spoil) in cases {
        let mut population = complete(
            perfect("dear", 10, 0.03)
                .into_iter()
                .chain(perfect("cheap", 10, 0.003))
                .collect(),
        );
        spoil(&mut population);
        let scored = judged(&population);
        assert!(
            scored.incomplete.as_deref().unwrap().contains(said),
            "{said}"
        );
        for pick in &scored.picks {
            assert_eq!(pick.outcome, "no_pick");
            assert_eq!(pick.model, DEFAULT_JUDGE);
            assert!(pick.why.contains("incomplete"), "{}", pick.why);
            assert!(pick.judges.is_empty());
        }
    }
}

/// Arms mode over the population, `C;submission=v1` against `C;submission=v2`, with the
/// format rule.
fn armed(population: &Population) -> Scored {
    let asked = Asked {
        mode: Mode::Arms,
        arm: Some("C;submission=v1".to_owned()),
        treatment: Some("C;submission=v2".to_owned()),
        rule: Some(Rule::Submission),
        resampling: how(),
    };
    judges::score(population, &asked).unwrap()
}

/// `judges` small judges, each over `families` families under both arms, two repetitions;
/// `failed` decisions of each arm, spread over the judges, ended by a second rejection.
fn formats(judges: &[&str], families: usize, failed: (usize, usize)) -> Vec<Decision> {
    let mut out = Vec::new();
    for (arm, failing) in [("C;submission=v1", failed.0), ("C;submission=v2", failed.1)] {
        let mut left = failing;
        for judge in judges {
            for repetition in [1, 2] {
                for mut d in perfect(judge, families, 0.003) {
                    d.arm = arm.to_owned();
                    d.repetition = repetition;
                    if left > 0 {
                        left -= 1;
                        d.computed = Computed::Error;
                        d.submissions = Some(Submissions {
                            classes: vec!["unknown_item".to_owned(), "missing_item".to_owned()],
                            cause: "invalid_submission".to_owned(),
                            recoveries: Vec::new(),
                        });
                    }
                    out.push(d);
                }
            }
        }
    }
    out
}

#[test]
fn a_missing_or_interrupted_cell_is_no_decision_under_submission_1() {
    // A population that would adopt v2: ten failures under v1, none under v2.
    let whole = complete(formats(&["haiku"], 20, (10, 0)));
    assert_eq!(armed(&whole).format.unwrap().outcome, "adopted");
    // A cell not run, or whose last attempt was interrupted, is not ended: no decision.
    let mut open = whole.clone();
    open.open = 1;
    let format = armed(&open).format.unwrap();
    assert_eq!(
        (format.outcome.as_str(), format.settled_by.as_str()),
        ("no_decision", "complete")
    );
    assert_eq!(format.format, "C;submission=v1");
    // So is an item with no label.
    let mut unlabelled = whole;
    unlabelled.unlabelled = 2;
    assert_eq!(armed(&unlabelled).format.unwrap().outcome, "no_decision");
}

#[test]
fn no_eligible_judge_keeps_the_default() {
    // Two judges that send a quarter of the items to review: decided correctly 0.75.
    let unsure: Vec<String> = (0..10).map(|n| format!("p{n}")).collect();
    let unsure: Vec<&str> = unsure.iter().map(String::as_str).collect();
    let all: Vec<Decision> = with(perfect("x", 20, 0.01), &unsure, Computed::Unsure)
        .into_iter()
        .chain(with(perfect("y", 20, 0.001), &unsure, Computed::Unsure))
        .collect();
    let scored = judged(&complete(all));
    let pick = pick_of(&scored, "all");
    assert_eq!(
        (pick.outcome.as_str(), pick.model.as_str()),
        ("no_pick", DEFAULT_JUDGE)
    );
    assert!(pick.why.contains("no judge is eligible"), "{}", pick.why);
    assert!(pick.why.contains("is kept"), "{}", pick.why);
    assert_eq!(pick.judges.len(), 2);
    for clauses in &pick.judges {
        assert!(clauses.errors_under_2_percent);
        assert!(!clauses.decided_correctly_at_least_0_80 && !clauses.eligible);
        assert!(!clauses.within_the_margin);
    }
}

#[test]
fn selection_uses_repetition_one_only() {
    // Repetition 1 decides every item correctly; repetition 2 decides every one wrongly.
    let first = perfect("solo", 10, 0.01);
    let second: Vec<Decision> = (first.iter())
        .map(|d| Decision {
            repetition: 2,
            computed: if d.expected == Expected::Fail {
                Computed::Pass
            } else {
                Computed::Fail
            },
            cost_usd: 1.0,
            settled_usd: 1.0,
            ..d.clone()
        })
        .collect();
    let scored = judged(&complete(first.into_iter().chain(second).collect()));
    let all = scored.scopes.iter().find(|s| s.name == "all").unwrap();
    let m = &all.measures[0];
    assert_eq!(m.decisions, 20);
    assert_eq!(m.balanced_accuracy.unwrap().point, 1.0);
    assert_eq!(m.decided_correctly.unwrap().point, 1.0);
    assert_eq!((m.false_rejects, m.false_accepts), (0, 0));
    assert!((m.cost_per_decision_usd - 0.01).abs() < 1e-12);
    // Repetition 2 gives self-agreement, the settled spend and the submission counts.
    assert_eq!((m.repeated_items, m.self_agreement), (20, Some(0.0)));
    assert!((m.settled_usd - 20.2).abs() < 1e-9, "{}", m.settled_usd);
    assert_eq!(m.submissions.decisions, 40);
    assert_eq!(pick_of(&scored, "all").outcome, "pick");
}

#[test]
fn a_run_without_rejections_is_unknown_not_clean() {
    // The diagnostic as T1 writes it, and what is not of its form.
    let read = Submissions::parse(
        br#"{"rejections": [{"turn": 1, "class": "unknown_item"}], "cause": "concluded"}"#,
    )
    .unwrap();
    assert_eq!(read.classes, ["unknown_item"]);
    assert!(read.first_rejected() && !read.failed());
    let failed = Submissions::parse(
        br#"{"rejections": [{"turn": 1, "class": "evidence"}, {"turn": 2, "class": "evidence"}],
            "cause": "invalid_submission", "recoveries": [{"kind": "enum_case"}]}"#,
    )
    .unwrap();
    assert!(failed.failed());
    assert_eq!(failed.recoveries, ["enum_case"]);
    assert_eq!(Submissions::parse(br#"{"cause": "concluded"}"#), None);
    assert_eq!(Submissions::parse(b"not json"), None);

    // Half the decisions kept no diagnostic: counted as unknown, and the rate is over the
    // others alone.
    let mut all = perfect("solo", 10, 0.01);
    for d in all.iter_mut().take(10) {
        d.submissions = None;
    }
    all[10].submissions = Some(Submissions {
        classes: vec!["bad_field".to_owned()],
        cause: "invalid_submission".to_owned(),
        recoveries: Vec::new(),
    });
    let scored = judged(&complete(all));
    let m = &scored.scopes[0].measures[0].submissions;
    assert_eq!((m.decisions, m.unknown, m.failures), (20, 10, 1));
    assert_eq!(m.failure_rate, Some(0.1));
    assert_eq!(m.first_rejection_rate, Some(0.1));
    assert_eq!(m.rejections_by_class.get("bad_field"), Some(&1));
    // With none kept, there is no rate at all: never a rate of 0.
    let mut none = perfect("solo", 10, 0.01);
    for d in &mut none {
        d.submissions = None;
    }
    let scored = judged(&complete(none.clone()));
    let m = &scored.scopes[0].measures[0].submissions;
    assert_eq!(
        (m.unknown, m.failure_rate, m.first_rejection_rate),
        (20, None, None)
    );
    // And under the format rule, an unknown decision makes no decision.
    let mut both = formats(&["haiku"], 20, (10, 0));
    both[5].submissions = None;
    let format = armed(&complete(both)).format.unwrap();
    assert_eq!(
        (format.outcome.as_str(), format.settled_by.as_str()),
        ("no_decision", "d")
    );
}

#[test]
fn the_output_holds_no_item_and_no_verdict_of_an_item() {
    let mut all = with(perfect("dear", 12, 0.03), &["f3"], Computed::Unsure);
    all.extend(perfect("cheap", 12, 0.003));
    for d in &mut all {
        d.item = format!("itm-zulu-{}", d.item);
        d.family = format!("fam-yankee-{}", d.family);
    }
    let scored = judged(&complete(all.clone()));
    let text = judges::toml_of(&scored).unwrap();
    assert!(!text.contains("itm-zulu"), "{text}");
    assert!(!text.contains("fam-yankee"), "{text}");
    // Arms mode too.
    for d in &mut all {
        d.arm = if d.model == "dear" {
            "C;submission=v1".to_owned()
        } else {
            "C;submission=v2".to_owned()
        };
        d.model = "same".to_owned();
        d.backend = "same".to_owned();
    }
    let text = judges::toml_of(&armed(&complete(all))).unwrap();
    assert!(
        !text.contains("itm-zulu") && !text.contains("fam-yankee"),
        "{text}"
    );
}

mod submission_rule {
    use super::*;

    #[test]
    fn halved_failures_with_none_new_and_accuracy_kept_adopt_the_treatment() {
        let format = armed(&complete(formats(
            &["haiku", "flash", "terra"],
            20,
            (12, 6),
        )))
        .format
        .unwrap();
        assert_eq!(
            (format.outcome.as_str(), format.settled_by.as_str()),
            ("adopted", "a, b and c")
        );
        assert_eq!(format.format, "C;submission=v2");
        assert_eq!(
            (format.control_failures, format.treatment_failures),
            (12, 6)
        );
        assert_eq!(format.fewer_failures_by, "failures");
        assert_eq!(format.judges.len(), 3);
    }

    #[test]
    fn failures_not_halved_keep_the_control() {
        let format = armed(&complete(formats(&["haiku"], 20, (12, 7))))
            .format
            .unwrap();
        assert_eq!(
            (format.outcome.as_str(), format.settled_by.as_str()),
            ("not_adopted", "a")
        );
        assert_eq!(format.format, "C;submission=v1");
    }

    #[test]
    fn few_failures_are_decided_by_first_rejections_or_are_too_few() {
        // Four failures (under five), and no other rejection: too few to tell.
        let few = armed(&complete(formats(&["haiku"], 20, (4, 0))))
            .format
            .unwrap();
        assert_eq!(
            (few.outcome.as_str(), few.settled_by.as_str()),
            ("too_few_failures", "a")
        );
        assert_eq!(few.format, "C;submission=v1");
        // Four failures, and ten first rejections under v1 that the repair saved; two under
        // v2: the first rejections decide, and they halved.
        let mut decisions = formats(&["haiku"], 20, (4, 0));
        let mut saved = |arm: &str, count: usize| {
            let mut left = count;
            for d in decisions
                .iter_mut()
                .filter(|d| d.arm == arm && d.computed != Computed::Error)
            {
                if left == 0 {
                    break;
                }
                left -= 1;
                d.submissions = Some(Submissions {
                    classes: vec!["location".to_owned()],
                    cause: "concluded".to_owned(),
                    recoveries: Vec::new(),
                });
            }
        };
        saved("C;submission=v1", 6);
        saved("C;submission=v2", 2);
        let format = armed(&complete(decisions)).format.unwrap();
        assert_eq!(format.fewer_failures_by, "first_rejections");
        assert_eq!(
            (
                format.control_first_rejections,
                format.treatment_first_rejections
            ),
            (10, 2)
        );
        assert_eq!(format.outcome, "adopted");
    }

    #[test]
    fn a_judge_with_more_failures_under_the_treatment_keeps_the_control() {
        // Ten failures under v1, all of `haiku`; two under v2, both of `flash`, which had
        // none: fewer in all, but new for one judge.
        let mut decisions = formats(&["haiku", "flash"], 20, (10, 0));
        for d in decisions
            .iter_mut()
            .filter(|d| d.arm == "C;submission=v2" && d.model == "flash")
            .take(2)
        {
            d.computed = Computed::Error;
            d.submissions = Some(Submissions {
                classes: vec!["not_alone".to_owned(), "no_submission".to_owned()],
                cause: "no_submission_after_repair".to_owned(),
                recoveries: Vec::new(),
            });
        }
        let format = armed(&complete(decisions)).format.unwrap();
        assert!(format.fewer_failures);
        assert_eq!(
            (format.outcome.as_str(), format.settled_by.as_str()),
            ("not_adopted", "b")
        );
        let flash = format.judges.iter().find(|j| j.model == "flash").unwrap();
        assert_eq!((flash.control_failures, flash.treatment_failures), (0, 2));
        assert!(!flash.no_new_failures);
    }

    #[test]
    fn accuracy_lost_under_the_treatment_keeps_the_control() {
        // Failures halve, but v2 passes three of twenty expected fails v1 caught: its
        // balanced accuracy falls by more than two points.
        let mut decisions = formats(&["haiku"], 20, (10, 0));
        for d in decisions.iter_mut().filter(|d| {
            d.arm == "C;submission=v2"
                && d.repetition == 1
                && ["f15", "f16", "f17"].contains(&d.item.as_str())
        }) {
            d.computed = Computed::Pass;
        }
        let format = armed(&complete(decisions)).format.unwrap();
        assert_eq!(
            (format.outcome.as_str(), format.settled_by.as_str()),
            ("not_adopted", "c")
        );
        let haiku = &format.judges[0];
        assert!(!haiku.accuracy_kept);
        assert!(haiku.false_accept_rate_change.unwrap() > 0.02);
    }
}

// --- reading the store ------------------------------------------------------------------------

/// A matrix over the test's set under two judges, arm C, one repetition.
fn matrix_file(dir: &Path) -> std::path::PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let file = dir.join("judges-matrix.toml");
    std::fs::write(
        &file,
        format!(
            "[matrix]\nname = \"tiny-judges\"\nsets = [\"{SET}\"]\narms = [\"C\"]\n\
             repetitions = 1\nconcurrency = 1\n\
             [[matrix.judges]]\nbackend = \"one\"\nmodel = \"claude-haiku-5-5\"\n\
             [[matrix.judges]]\nbackend = \"two\"\nmodel = \"claude-haiku-5-5\"\n"
        ),
    )
    .unwrap();
    file
}

/// The test's set made blind: its labels moved into `labels`, laid out as the sets are, the
/// manifest naming them by their SHA-256 alone.
fn blinded(tiny: &Tiny, labels: &Path) {
    let set = tiny.root.join(SET);
    let text = std::fs::read_to_string(set.join("labels.toml")).unwrap();
    std::fs::create_dir_all(labels.join(SET)).unwrap();
    std::fs::write(labels.join(SET).join("labels.toml"), &text).unwrap();
    std::fs::remove_file(set.join("labels.toml")).unwrap();
    let digest = whizbang_core::identity::Digest::of_bytes(text.as_bytes()).hex();
    let manifest = std::fs::read_to_string(set.join("set.toml")).unwrap();
    let manifest = manifest
        .replace("split = \"dev\"", "split = \"blind\"")
        .replace(
            "labels = \"labels.toml\"",
            &format!("labels_sha256 = \"{digest}\""),
        );
    std::fs::write(set.join("set.toml"), manifest).unwrap();
}

fn reading<'a>(
    world: &'a World,
    tiny: &'a Tiny,
    matrices: &'a [std::path::PathBuf],
    labels: &'a Path,
    out: &'a Path,
) -> Reading<'a> {
    Reading {
        matrices,
        store: &world.root,
        sets_root: &tiny.root,
        labels_root: labels,
        out,
        mode: Mode::Judges,
        arm: Some("C"),
        rule: Some(Rule::JudgePick),
        commit: "0000000000000000000000000000000000000000",
    }
}

#[test]
fn a_blind_set_is_read_through_gating_labelled() {
    let world = World::new("judges-blind");
    let tiny = tiny(&world.beside("set"), &["a", "b"]);
    let labels = world.beside("owner-labels");
    blinded(&tiny, &labels);
    let matrices = [matrix_file(&world.beside("matrices"))];
    let out = world.beside("judges.toml");
    // Labels at or under the sets' folder are refused, as `eval run --gate` refuses them.
    let inside = tiny.root.join("private");
    std::fs::create_dir_all(inside.join(SET)).unwrap();
    std::fs::copy(
        labels.join(SET).join("labels.toml"),
        inside.join(SET).join("labels.toml"),
    )
    .unwrap();
    let refused = judges::run(&reading(&world, &tiny, &matrices, &inside, &out)).unwrap_err();
    assert!(
        refused.contains("never read from under the sets' folder"),
        "{refused}"
    );
    std::fs::remove_dir_all(&inside).unwrap();
    // Labels that are not the ones the set seals are refused.
    let wrong = world.beside("wrong-labels");
    std::fs::create_dir_all(wrong.join(SET)).unwrap();
    let text = std::fs::read_to_string(labels.join(SET).join("labels.toml")).unwrap();
    std::fs::write(
        wrong.join(SET).join("labels.toml"),
        text.replacen("pass", "fail", 1),
    )
    .unwrap();
    let refused = judges::run(&reading(&world, &tiny, &matrices, &wrong, &out)).unwrap_err();
    assert!(
        refused.contains("do not hash to the set's labels_sha256"),
        "{refused}"
    );
    assert!(!out.exists());
    // The owner's labels are read, held to the seal: nothing was run, so no pick.
    let written = judges::run(&reading(&world, &tiny, &matrices, &labels, &out)).unwrap();
    let digest = whizbang_core::identity::Digest::of_bytes(text.as_bytes()).hex();
    assert_eq!(written.sets[0].labels_sha256, digest);
    assert_eq!(written.sets[0].split, "blind");
    assert_eq!((written.scored.cells, written.scored.open), (4, 4));
    for pick in &written.scored.picks {
        assert_eq!(
            (pick.outcome.as_str(), pick.model.as_str()),
            ("no_pick", DEFAULT_JUDGE)
        );
    }
    let record = std::fs::read_to_string(&out).unwrap();
    let read: toml::Value = toml::from_str(&record).unwrap();
    assert_eq!(read["schema"].as_str(), Some("judges/1"), "{record}");
}

#[test]
fn out_under_the_sets_or_labels_is_refused() {
    let world = World::new("judges-out");
    let tiny = tiny(&world.beside("set"), &["a"]);
    let labels = world.beside("labels");
    std::fs::create_dir_all(&labels).unwrap();
    let matrices = [matrix_file(&world.beside("matrices"))];
    for out in [
        tiny.root.join("judges.toml"),
        tiny.root.join(SET).join("judges.toml"),
        labels.join("judges.toml"),
        world.beside("no-such-folder/judges.toml"),
    ] {
        let refused = judges::run(&reading(&world, &tiny, &matrices, &labels, &out)).unwrap_err();
        assert!(refused.contains("not written under"), "{refused}");
        assert!(!out.exists());
    }
}
