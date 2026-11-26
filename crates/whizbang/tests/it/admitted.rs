//! `admitted::…`: what the receipts of the gates admitted (plan mE, task M1, item 8), read
//! from receipts made in the test and from the repository's own.

use std::path::Path;

use whizbang::admitted;
use whizbang_core::identity::Digest;

/// A receipt of version 2 as `eval run --gate` writes one: its outcome, its gate's rung and
/// the splits of the sets it read.
fn receipt(outcome: &str, rung: Option<u64>, split: &str) -> serde_json::Value {
    let mut json = serde_json::json!({
        "receipt_version": 2,
        "gate_sha256": "ab".repeat(32),
        "outcome": outcome,
        "splits": {"source-fidelity/confirm-v1": split},
        "date": "2026-12-01",
    });
    if let Some(rung) = rung {
        json["admits"] = serde_json::json!({"rung": rung, "kind": "tool", "name": "per_subject"});
    }
    json
}

#[test]
fn a_rung_3_receipt_admits_nothing() {
    let digest = "cd".repeat(32);
    let admitted = admitted::admits(&digest, &receipt("admitted", Some(4), "heldout")).unwrap();
    assert_eq!(
        (
            admitted.kind.as_str(),
            admitted.name.as_str(),
            admitted.date.as_str()
        ),
        ("tool", "per_subject", "2026-12-01")
    );
    assert_eq!(admitted.receipt_sha256, digest);
    assert!(admitted::admits(&digest, &receipt("admitted", Some(4), "blind")).is_some());
    // A rung 3 gate, a dev or exposed set, an outcome not admitted, a gate that names
    // nothing it admits, a receipt of version 1: nothing.
    let mut v1 = receipt("admitted", Some(4), "blind");
    v1["receipt_version"] = 1.into();
    for none in [
        receipt("admitted", Some(3), "blind"),
        receipt("admitted", Some(4), "dev"),
        receipt("rejected", Some(4), "blind"),
        receipt("inconclusive", Some(4), "heldout"),
        receipt("admitted", None, "blind"),
        v1,
    ] {
        assert_eq!(admitted::admits(&digest, &none), None, "{none}");
    }
    // Read from a folder: each receipt by the digest of its bytes; other JSON passed over.
    let world = crate::wired::World::new("admitted-read");
    let dir = world.beside("receipts");
    std::fs::create_dir_all(dir.join("deeper")).unwrap();
    let four = serde_json::to_string(&receipt("admitted", Some(4), "heldout")).unwrap();
    let three = serde_json::to_string(&receipt("admitted", Some(3), "heldout")).unwrap();
    std::fs::write(dir.join("deeper/four.json"), &four).unwrap();
    std::fs::write(dir.join("three.json"), &three).unwrap();
    std::fs::write(dir.join("other.json"), "{\"schema\": \"judges/1\"}").unwrap();
    std::fs::write(dir.join("gate.toml"), "gate_version = 3\n").unwrap();
    let (list, read) = admitted::read(&dir).unwrap();
    assert_eq!(read, 2);
    assert_eq!(list.len(), 1);
    assert_eq!(
        list[0].receipt_sha256,
        Digest::of_bytes(four.as_bytes()).hex()
    );
    let text = admitted::text(&list, read);
    assert!(
        text.starts_with("receipts read: 2\nadmitted: 1\ntool per_subject: receipt "),
        "{text}"
    );
}

#[test]
fn the_records_marks_are_the_receipts() {
    // Each record that states its tool admitted names a receipt that admitted it; each tool
    // or use case a receipt admitted has a record that states it admitted.
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut admissions = Vec::new();
    for dir in ["gates", "docs/results"] {
        let (list, _) = admitted::read(&repository.join(dir)).unwrap();
        admissions.extend(list);
    }
    for (tool, mark) in whizbang_squint::marks::all() {
        if mark != whizbang_squint::marks::Mark::Admitted {
            continue;
        }
        let record = (whizbang_squint::marks::RECORDS.iter())
            .find(|(name, _)| *name == tool)
            .map(|(_, text)| *text)
            .unwrap();
        let named = admissions
            .iter()
            .any(|a| a.name == tool && record.contains(&a.receipt_sha256));
        assert!(
            named,
            "{tool} is admitted, and its record names no receipt that admits it"
        );
    }
    for one in &admissions {
        if matches!(one.kind.as_str(), "tool" | "use_case") {
            assert_eq!(
                whizbang_squint::marks::of(&one.name),
                Some(whizbang_squint::marks::Mark::Admitted),
                "{} was admitted by a receipt, and its record does not say so",
                one.name
            );
        }
    }
}

#[test]
fn an_exposed_set_admits_nothing_whatever_its_split() {
    // `blind-v3` is of split `blind` and exposed (`SPEC.md` section 8): it never admits.
    let digest = "cd".repeat(32);
    let mut exposed = receipt("admitted", Some(4), "blind");
    exposed["splits"] = serde_json::json!({"source-fidelity/blind-v3": "blind"});
    assert_eq!(admitted::admits(&digest, &exposed), None);
    exposed["splits"] = serde_json::json!({
        "source-fidelity/confirm-v1": "heldout",
        "source-fidelity/blind-v2": "heldout",
    });
    assert_eq!(admitted::admits(&digest, &exposed), None);
    // The list holds every committed set of split blind or held out: each was seen or used
    // before, so none admits.
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = repository.join("evalsets");
    let mut found = Vec::new();
    for id in whizbang::held_out::ids_under(&root) {
        let set = whizbang_eval::sets::find(&root, &id).unwrap();
        let split = set.manifest.set.split.to_string();
        if split == "blind" || split == "heldout" {
            found.push(id);
        }
    }
    assert_eq!(found, whizbang_eval::gate::EXPOSED);
    // A rung 4 gate on one is refused when its matrix is read.
    let world = crate::wired::World::new("admitted-exposed");
    let dir = world.beside("gates");
    std::fs::create_dir_all(&dir).unwrap();
    let gate = crate::gate::gate_v3("control = \"C\"\ntreatment = \"C+resize\"\n", 1)
        + "\n[admits]\nrung = 4\nkind = \"tool\"\nname = \"resize\"\n";
    std::fs::write(dir.join("tool.toml"), &gate).unwrap();
    let matrix = format!(
        "[matrix]\nname = \"exposed\"\nsets = [\"source-fidelity/blind-v3\"]\n\
         arms = [\"C\", \"C+resize\"]\nrepetitions = 1\n\
         [[matrix.judges]]\nbackend = \"default\"\nmodel = \"claude-haiku-5-5\"\n\
         [gates]\n\"tool.toml\" = \"{}\"\n",
        Digest::of_bytes(gate.as_bytes()).hex()
    );
    std::fs::write(dir.join("matrix.toml"), matrix).unwrap();
    let refused = whizbang::gating::frozen(&dir.join("tool.toml")).unwrap_err();
    assert!(refused.contains("blind-v3 is exposed"), "{refused}");
    // A rung 4 gate across two versions names them: one exposed is refused as the file reads.
    let versions = crate::gate::gate_v3(
        "arm = \"C\"\ncontrol_set = \"source-fidelity/blind-v3\"\ntreatment_set = \"source-fidelity/confirm-v1\"\n",
        1,
    ) + "\n[admits]\nrung = 4\nkind = \"wording\"\nname = \"full\"\n";
    let refused = whizbang_eval::gate::Gate::parse(&versions).unwrap_err();
    assert!(refused.contains("exposed"), "{refused}");
}
