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
            "study-tools"
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
