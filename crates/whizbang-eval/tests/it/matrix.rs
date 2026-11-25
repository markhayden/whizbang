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
            "e-judges-blind",
            "e-judges-dev",
            "e-submission",
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
    let (_, haiku) = &read[5];
    let arms: Vec<String> = haiku.matrix.arms.iter().map(|a| a.offered()).collect();
    assert_eq!(
        arms,
        ["view,align", "view,align,outline", "view,align,regions"]
    );
    assert_eq!(haiku.matrix.repetitions, 2);
    // The evidence matrix: 110 items' sets under arm F, twice, two at once.
    let (_, evidence) = &read[3];
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
        assert_eq!(
            gate.run.matrix_identity,
            Some(matrix.identity()),
            "{use_case}"
        );
        assert_eq!(matrix.matrix.sets, [format!("{use_case}/{set}")]);
        let arms: Vec<String> = matrix.matrix.arms.iter().map(ToString::to_string).collect();
        let treatment = format!("F;second_look=on@{use_case}");
        assert_eq!(arms, ["F".to_owned(), treatment.clone()]);
        assert_eq!(gate.compared().unwrap().sides(), ("F", treatment.as_str()));
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

// --- only the cells a matrix names (plan mE, task M1, item 1) ----------------------------------

/// The shape of plan mE's confirmation round: two sets, three arms, three judges; the plain
/// twin under arm C and two judges alone; Flash under arm C, twice, and the format alone.
const LISTED: &str = r#"
[matrix]
name = "confirm"
sets = ["source-fidelity/confirm-v1", "source-fidelity/confirm-plain-v1"]
arms = ["C", { arm = "C", submission = "v2" }, "C+per_subject"]
repetitions = 1

[matrix.set."source-fidelity/confirm-plain-v1"]
arms = ["C"]
judges = ["haiku", "terra"]

[[matrix.judges]]
backend = "haiku"
model = "claude-haiku-5-5"

[[matrix.judges]]
backend = "terra"
model = "gpt-6.1-terra"

[[matrix.judges]]
backend = "flash"
model = "gemini-3.8-flash"
arms = [{ arm = "C", repetitions = 2 }, "C;submission=v2"]
"#;

/// Two items of each set the matrix names.
fn two_items(m: &Matrix) -> Vec<(String, Vec<String>)> {
    (m.matrix.sets.iter())
        .map(|set| (set.clone(), vec!["i1".to_owned(), "i2".to_owned()]))
        .collect()
}

/// Each cell as set, judge, arm and repetition, the item left out.
fn shape(cells: &[matrix::Cell]) -> Vec<(String, String, String, u32)> {
    let mut out: Vec<_> = (cells.iter())
        .filter(|c| c.item == "i1")
        .map(|c| {
            let set = c.set.rsplit('/').next().unwrap().to_owned();
            (set, c.backend.clone(), c.arm.clone(), c.repetition)
        })
        .collect();
    out.sort();
    out
}

#[test]
fn a_judge_runs_only_its_arms() {
    let m = Matrix::parse(LISTED).unwrap();
    let cells = m.cells(&two_items(&m));
    let at = |set: &str, judge: &str, arm: &str, repetition: u32| {
        (set.to_owned(), judge.to_owned(), arm.to_owned(), repetition)
    };
    let (v2, tool) = ("C;submission=v2", "C+per_subject");
    assert_eq!(
        shape(&cells),
        [
            at("confirm-plain-v1", "haiku", "C", 1),
            at("confirm-plain-v1", "terra", "C", 1),
            at("confirm-v1", "flash", "C", 1),
            at("confirm-v1", "flash", "C", 2),
            at("confirm-v1", "flash", v2, 1),
            at("confirm-v1", "haiku", "C", 1),
            at("confirm-v1", "haiku", tool, 1),
            at("confirm-v1", "haiku", v2, 1),
            at("confirm-v1", "terra", "C", 1),
            at("confirm-v1", "terra", tool, 1),
            at("confirm-v1", "terra", v2, 1),
        ]
    );
    assert_eq!(cells.len(), 22);
    // The forecast counts the cells named, and no other: each at its model's rate.
    let (haiku, terra, flash) = (3_293, 17_233, 22_752);
    let per_item = 3 * haiku + 3 * terra + 3 * flash + haiku + terra;
    assert_eq!(matrix::forecast(&cells), Ok(2 * 2 * per_item));
    // What a list names must be the matrix's, once, as it names it, and leave each set,
    // judge and arm a cell.
    for (from, to, says) in [
        (
            r#"arms = [{ arm = "C", repetitions = 2 }, "C;submission=v2"]"#,
            r#"arms = ["C+outline"]"#,
            "is not an arm of the matrix",
        ),
        (
            r#"arms = [{ arm = "C", repetitions = 2 }, "C;submission=v2"]"#,
            r#"arms = ["C", "C"]"#,
            "named once",
        ),
        (
            r#"arms = [{ arm = "C", repetitions = 2 }, "C;submission=v2"]"#,
            "arms = []",
            "at least one",
        ),
        (
            r#"arms = [{ arm = "C", repetitions = 2 }, "C;submission=v2"]"#,
            r#"arms = [{ arm = "C", repetitions = 11 }]"#,
            "repetitions is 1 to 10",
        ),
        (
            r#"[matrix.set."source-fidelity/confirm-plain-v1"]"#,
            r#"[matrix.set."source-fidelity/other-v1"]"#,
            "not a set of the matrix",
        ),
        (
            r#"judges = ["haiku", "terra"]"#,
            r#"judges = ["haiku", "sol"]"#,
            "each judge it runs",
        ),
        (
            r#"judges = ["haiku", "terra"]"#,
            "judges = []",
            "at least one",
        ),
        (
            r#"arms = [{ arm = "C", repetitions = 2 }, "C;submission=v2"]"#,
            r#"arms = [{ arm = "C", repetitions = 2 }, "C;submission=v2", "C+per_subject"]
[[matrix.judges]]
backend = "sol"
model = "gpt-6.1-sol"
arms = ["C+per_subject"]
[matrix.set."source-fidelity/confirm-v1"]
judges = ["haiku", "terra", "flash"]"#,
            "the judge sol has no cell",
        ),
    ] {
        assert!(LISTED.contains(from), "{from}");
        let refused = Matrix::parse(&LISTED.replacen(from, to, 1)).unwrap_err();
        assert!(refused.contains(says), "{to}: {refused}");
    }
}

#[test]
fn an_arm_of_a_judge_repeats_on_its_own() {
    // A judge's own repetitions hold for its arms that give none; an arm that gives its own
    // repeats so, under that judge alone.
    let three = LISTED.replace(
        "model = \"gemini-3.8-flash\"\n",
        "model = \"gemini-3.8-flash\"\nrepetitions = 3\n",
    );
    let m = Matrix::parse(&three).unwrap();
    let flash = &m.matrix.judges[2];
    assert_eq!(flash.repeated_under(&m.matrix, "C"), Some(2));
    assert_eq!(flash.repeated_under(&m.matrix, "C;submission=v2"), Some(3));
    assert_eq!(flash.repeated_under(&m.matrix, "C+per_subject"), None);
    let haiku = &m.matrix.judges[0];
    assert_eq!(haiku.repeated_under(&m.matrix, "C"), Some(1));
    let cells = m.cells(&two_items(&m));
    let flash_cells: Vec<(String, u32)> = (cells.iter())
        .filter(|c| c.backend == "flash" && c.item == "i1")
        .map(|c| (c.arm.clone(), c.repetition))
        .collect();
    assert_eq!(
        flash_cells,
        [
            ("C".to_owned(), 1),
            ("C".to_owned(), 2),
            ("C;submission=v2".to_owned(), 1),
            ("C;submission=v2".to_owned(), 2),
            ("C;submission=v2".to_owned(), 3)
        ]
    );
    // The other judges' cells are as they were.
    let others = |m: &Matrix| -> Vec<matrix::Cell> {
        (m.cells(&two_items(m)).into_iter())
            .filter(|c| c.backend != "flash")
            .collect()
    };
    assert_eq!(others(&m), others(&Matrix::parse(LISTED).unwrap()));
    // An arm's repetitions are part of the identity.
    let once = LISTED.replace(r#"{ arm = "C", repetitions = 2 }"#, r#""C""#);
    assert_ne!(
        Matrix::parse(&once).unwrap().identity(),
        Matrix::parse(LISTED).unwrap().identity()
    );
}

#[test]
fn a_matrix_without_lists_keeps_its_cells_and_identity() {
    // The committed gate of details first was frozen with its matrix's identity before
    // matrices could list their cells: the identity is still that one.
    let read = |name: &str| std::fs::read_to_string(repo().join("gates").join(name)).unwrap();
    let gate = whizbang_eval::gate::Gate::parse(&read("details-first.toml")).unwrap();
    let m = Matrix::parse(&read("details-first-matrix.toml")).unwrap();
    assert!(!m.lists());
    assert_eq!(gate.run.matrix_identity, Some(m.identity()));
    let canonical = serde_json::to_string(&m).unwrap();
    for absent in ["\"set\"", "\"gates\"", "\"arms\":null", "\"arms\":[{"] {
        assert!(!canonical.contains(absent), "{absent}: {canonical}");
    }
    // Every set, item, judge, arm and repetition is a cell.
    let cells = m.cells(&two_items(&m));
    let repetitions: u32 = (m.matrix.judges.iter())
        .map(|j| j.repeated(&m.matrix))
        .sum();
    assert_eq!(
        cells.len(),
        m.matrix.sets.len() * 2 * m.matrix.arms.len() * repetitions as usize
    );
    // A list that names every arm names the same cells, as a matrix of another identity.
    let listed = read("details-first-matrix.toml").replace(
        "model = \"gemini-3.8-flash\"",
        "model = \"gemini-3.8-flash\"\narms = [\"C\", \"F\"]",
    );
    let listed = Matrix::parse(&listed).unwrap();
    assert!(listed.lists());
    assert_eq!(listed.cells(&two_items(&listed)), cells);
    assert_ne!(listed.identity(), m.identity());
}
