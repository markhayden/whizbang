//! Matrices (plan 2C, task E1, item 4): the files, and what a forecast is made of.

use std::path::Path;

use whizbang_eval::matrix::{self, Matrix, SETTLED_PER_DECISION};

fn repo() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

#[test]
fn the_ported_study_matrices_are_read() {
    let mut read = Vec::new();
    for entry in std::fs::read_dir(repo().join("matrices")).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        let matrix = Matrix::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        read.push((
            path.file_stem().unwrap().to_string_lossy().into_owned(),
            matrix,
        ));
    }
    read.sort_by(|a, b| a.0.cmp(&b.0));
    let names: Vec<&str> = read.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        [
            "e1-evidence",
            "study-flash",
            "study-haiku",
            "study-opus",
            "study-sol",
            "study-sonnet",
            "study-terra",
            "study-tools",
            "unsure-screen",
            "unsure-strict"
        ]
    );
    // The models of the Google and OpenAI judges are named, and every model has a
    // settled cost a forecast can be made of.
    let models: Vec<&str> = read
        .iter()
        .flat_map(|(_, m)| m.matrix.judges.iter().map(|j| j.model.as_str()))
        .collect();
    assert!(models.contains(&"gemini-3.8-flash") && models.contains(&"gpt-6.1-terra"));
    for (name, m) in &read {
        let items = vec![(m.matrix.sets[0].clone(), vec!["one".to_owned()])];
        assert!(matrix::forecast(&m.cells(&items)).is_ok(), "{name}");
    }
    // Squint's D_<tool> is the control and that tool.
    let (_, haiku) = &read[2];
    let arms: Vec<String> = haiku.matrix.arms.iter().map(|a| a.offered()).collect();
    assert_eq!(
        arms,
        ["view,align", "view,align,outline", "view,align,regions"]
    );
    assert_eq!(haiku.matrix.repetitions, 2);
    // The evidence matrix: 110 items' sets under arm F, twice, two at once.
    let (_, evidence) = &read[0];
    assert_eq!(evidence.matrix.sets.len(), 10);
    assert_eq!(evidence.matrix.arms[0].details(), "first");
    assert_eq!(evidence.matrix.concurrency, 2);
}

#[test]
fn the_settled_cost_of_a_decision_is_the_studys() {
    // Each model's cell of arm C in the baseline's manifest: its spend over its
    // decisions, in millionths of a dollar, rounded up.
    let manifest = std::fs::read_to_string(repo().join("docs/baseline/manifest.toml")).unwrap();
    let mut found = Vec::new();
    for cell in manifest.split("[[cell]]").skip(1) {
        let field = |key: &str| {
            cell.lines()
                .find_map(|line| line.strip_prefix(&format!("{key} = ")))
                .unwrap()
                .trim_matches('"')
                .to_owned()
        };
        if field("arm") != "C" {
            continue;
        }
        let model = field("model").split_once(':').unwrap().1.to_owned();
        let spend: f64 = field("spend_usd").parse().unwrap();
        let decisions: f64 = field("decisions").parse().unwrap();
        found.push((model, (spend * 1e6 / decisions).ceil() as u64));
    }
    found.sort();
    let table: Vec<(String, u64)> = SETTLED_PER_DECISION
        .iter()
        .map(|(model, rate)| ((*model).to_owned(), *rate))
        .collect();
    assert_eq!(found, table);
}

#[test]
fn a_matrix_is_read_strictly() {
    let good = "[matrix]\nname = \"m\"\nsets = [\"a/b\"]\narms = [\"C\"]\nrepetitions = 1\n\
                [[matrix.judges]]\nbackend = \"default\"\nmodel = \"claude-haiku-5-5\"\n";
    let matrix = Matrix::parse(good).unwrap();
    assert_eq!(matrix.matrix.concurrency, 2);
    for (bad, said) in [
        (good.replace("[\"C\"]", "[\"D_outline\"]"), "is not C or F"),
        (
            good.replace("repetitions = 1", "repetitions = 0"),
            "repetitions",
        ),
        (
            good.replace("name = \"m\"", "name = \"m\"\nspend_cap_usd = 1.0"),
            "unknown field",
        ),
        (
            good.replace("[\"C\"]", "[\"C\", \"C\"]"),
            "each set, judge and arm once",
        ),
    ] {
        let refused = Matrix::parse(&bad).unwrap_err();
        assert!(refused.contains(said), "{refused}");
    }
    // The identity is of the content, not of the file's layout.
    let spaced = good.replace("name = \"m\"", "name   =   \"m\"\n\n");
    assert_eq!(
        Matrix::parse(&spaced).unwrap().identity(),
        matrix.identity()
    );
}

#[test]
fn the_unsure_screen_and_its_gate_are_read() {
    // The screen (plan m3, task U1, item 5): four arms on reality-checks and
    // reference-consistency, Haiku, three times.
    let text = std::fs::read_to_string(repo().join("matrices/unsure-screen.toml")).unwrap();
    let screen = Matrix::parse(&text).unwrap();
    let arms: Vec<String> = screen.matrix.arms.iter().map(ToString::to_string).collect();
    assert_eq!(
        arms,
        [
            "F",
            "F;details=tiles@reality-checks",
            "F;second_look=on",
            "F;details=tiles@reality-checks;second_look=on"
        ]
    );
    assert_eq!(screen.matrix.sets.len(), 4);
    assert_eq!(screen.matrix.repetitions, 3);
    let judges: Vec<&str> = screen
        .matrix
        .judges
        .iter()
        .map(|j| j.model.as_str())
        .collect();
    assert_eq!(judges, ["claude-haiku-5-5"]);
    // Tiles go to reality-checks alone; the rest of each arm is F's.
    for arm in &screen.matrix.arms {
        assert_eq!(arm.detail_for("reference-consistency"), None);
        assert_eq!(arm.offered(), "view,align,resize");
    }
    // Its forecast for the 44 items: Haiku's settled cost per decision, twice under F,
    // three times with the second look, the whole doubled.
    let items: Vec<(String, Vec<String>)> = [20, 2, 20, 2]
        .iter()
        .zip(&screen.matrix.sets)
        .map(|(n, set)| (set.clone(), (0..*n).map(|i| i.to_string()).collect()))
        .collect();
    let cells = screen.cells(&items);
    assert_eq!(cells.len(), 44 * 4 * 3);
    let rate = 3_293;
    assert_eq!(
        matrix::forecast(&cells).unwrap(),
        2 * 44 * 3 * rate * (2 + 2 + 3 + 3)
    );
    // And the strict second look beside it, on reality-checks.
    let text = std::fs::read_to_string(repo().join("matrices/unsure-strict.toml")).unwrap();
    let strict = Matrix::parse(&text).unwrap();
    let arms: Vec<String> = strict.matrix.arms.iter().map(ToString::to_string).collect();
    assert_eq!(arms, ["F", "F;second_look=on", "F;second_look=strict"]);
    assert_eq!(
        strict.matrix.arms[2].second_look_for("reality-checks"),
        "strict"
    );
    // The gates (item 6): one a use case, each F with the second look for that use case
    // against F on its dev set, frozen with its matrix's identity, with false accepts as a
    // guardrail beside false rejects.
    for (use_case, set) in [
        ("source-fidelity", "scenes-v1"),
        ("reference-consistency", "pets-v1"),
        ("reality-checks", "people-v1"),
        ("brief-compliance", "stills-v1"),
        ("product-fidelity", "packshots-v1"),
    ] {
        let read = |name: &str| std::fs::read_to_string(repo().join("gates").join(name)).unwrap();
        let gate = whizbang_eval::gate::Gate::parse(&read(&format!("unsure-{use_case}.toml")))
            .unwrap_or_else(|e| panic!("{use_case}: {e}"));
        let matrix = Matrix::parse(&read(&gate.run.matrix)).unwrap();
        assert_eq!(gate.run.matrix_identity, matrix.identity(), "{use_case}");
        assert_eq!(matrix.matrix.sets, [format!("{use_case}/{set}")]);
        let arms: Vec<String> = matrix.matrix.arms.iter().map(ToString::to_string).collect();
        let treatment = format!("F;second_look=on@{use_case}");
        assert_eq!(arms, ["F".to_owned(), treatment.clone()]);
        assert_eq!(
            (
                gate.baseline.control.as_str(),
                gate.baseline.treatment.as_str()
            ),
            ("F", treatment.as_str())
        );
        assert_eq!(matrix.matrix.arms[1].second_look_for(use_case), "on");
        assert_eq!(gate.guardrails.false_accept_increase_max, Some(0.03));
        assert_eq!(gate.guardrails.false_reject_increase_max, 0.03);
        assert_eq!(gate.confidence.hypotheses, matrix.matrix.judges.len());
        let judges: Vec<(&str, u32)> = (matrix.matrix.judges.iter())
            .map(|j| (j.model.as_str(), j.repeated(&matrix.matrix)))
            .collect();
        assert_eq!(
            judges,
            [
                ("claude-haiku-5-5", 3),
                ("gemini-3.8-flash", 1),
                ("gpt-6.1-terra", 1)
            ]
        );
    }
}

#[test]
fn a_judge_given_its_own_repetitions_is_repeated_so_and_a_matrix_without_keeps_its_identity() {
    use whizbang_eval::matrix::Matrix;
    let plain = "[matrix]\nname = \"m\"\nsets = [\"a/b\"]\narms = [\"C\"]\nrepetitions = 1\n\
                 [[matrix.judges]]\nbackend = \"haiku\"\nmodel = \"claude-haiku-5-5\"\n\
                 [[matrix.judges]]\nbackend = \"flash\"\nmodel = \"gemini-3.8-flash\"\n";
    let before = Matrix::parse(plain).unwrap();
    let three = plain.replace(
        "model = \"claude-haiku-5-5\"\n",
        "model = \"claude-haiku-5-5\"\nrepetitions = 3\n",
    );
    let matrix = Matrix::parse(&three).unwrap();
    let cells = matrix.cells(&[("a/b".to_owned(), vec!["i".to_owned()])]);
    let by: Vec<(&str, u32)> = cells
        .iter()
        .map(|c| (c.backend.as_str(), c.repetition))
        .collect();
    assert_eq!(by, [("haiku", 1), ("haiku", 2), ("haiku", 3), ("flash", 1)]);
    // Giving none leaves the identity a matrix had before judges could be repeated apart.
    assert_ne!(matrix.identity(), before.identity());
    assert_eq!(
        before.identity(),
        Matrix::parse(&plain.replace("repetitions = 1\n", "repetitions = 1\n\n"))
            .unwrap()
            .identity()
    );
    let canonical = serde_json::to_string(&before).unwrap();
    assert!(!canonical.contains("\"repetitions\":null"), "{canonical}");
    // A judge repeated past the limit, or named twice, is refused.
    assert!(Matrix::parse(&three.replace("repetitions = 3", "repetitions = 11")).is_err());
    let twice = plain.replace(
        "backend = \"flash\"\nmodel = \"gemini-3.8-flash\"",
        "backend = \"haiku\"\nmodel = \"claude-haiku-5-5\"\nrepetitions = 2",
    );
    assert!(Matrix::parse(&twice).is_err());
}

/// A matrix of one judge and these arms, as a file writes it.
fn with_arms(arms: &str) -> String {
    format!(
        "[matrix]\nname = \"m\"\nsets = [\"a/b\"]\narms = {arms}\nrepetitions = 1\n\
         [[matrix.judges]]\nbackend = \"haiku\"\nmodel = \"claude-haiku-5-5\"\n"
    )
}

#[test]
fn an_arm_with_submission_v1_or_v2_parses_and_names_it_in_its_identity() {
    // Plan e1, task F1, item 1.
    for value in ["v1", "v2"] {
        let arm: matrix::Arm = format!("C;submission={value}").parse().unwrap();
        assert_eq!(arm.submission.as_deref(), Some(value));
        assert_eq!(arm.to_string(), format!("C;submission={value}"));
        let f: matrix::Arm = format!("F;second_look=on;submission={value}")
            .parse()
            .unwrap();
        assert_eq!(
            f.to_string(),
            format!("F;second_look=on;submission={value}")
        );
    }
    let v1 = Matrix::parse(&with_arms("[\"C;submission=v1\"]")).unwrap();
    let v2 = Matrix::parse(&with_arms("[\"C;submission=v2\"]")).unwrap();
    let plain = Matrix::parse(&with_arms("[\"C\"]")).unwrap();
    assert_ne!(v1.identity(), v2.identity());
    assert_ne!(v1.identity(), plain.identity());
    let cells = v2.cells(&[("a/b".to_owned(), vec!["i".to_owned()])]);
    assert_eq!(cells[0].arm, "C;submission=v2");
    // As a table, the same arm.
    let table = Matrix::parse(&with_arms("[{ arm = \"C\", submission = \"v2\" }]")).unwrap();
    assert_eq!(table.identity(), v2.identity());
    // Any other value, or the setting twice, is refused.
    assert!("C;submission=v3".parse::<matrix::Arm>().is_err());
    assert!("C;submission=".parse::<matrix::Arm>().is_err());
    assert!(
        "C;submission=v1;submission=v2"
            .parse::<matrix::Arm>()
            .is_err()
    );
}

#[test]
fn an_arm_without_submission_keeps_its_identity() {
    let arm: matrix::Arm = "C".parse().unwrap();
    assert_eq!(arm.submission, None);
    assert_eq!(arm.to_string(), "C");
    let plain = Matrix::parse(&with_arms("[\"C\", \"F;second_look=on\"]")).unwrap();
    let canonical = serde_json::to_string(&plain).unwrap();
    assert_eq!(canonical, serde_json::to_string(&plain).unwrap());
    assert!(!canonical.contains("submission"), "{canonical}");
    // The identity of a matrix written before the setting is unchanged: the hash of its
    // canonical form, which names no submission.
    assert!(
        canonical.contains("\"arms\":[\"C\",\"F;second_look=on\"]"),
        "{canonical}"
    );
}
