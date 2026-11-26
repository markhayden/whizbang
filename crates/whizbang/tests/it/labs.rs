//! `labs::…`: the committed manifests of the labs (plan e1, task L1, item 2).

use std::collections::BTreeSet;

use whizbang::loop_score::Manifest;

#[test]
fn the_segment_01_manifest_has_twenty_in_four_groups() {
    let file =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../labs/e-loop/segment-01.toml");
    let (manifest, _) = Manifest::read(&file).unwrap();
    assert_eq!(manifest.kind, "segment-01");
    assert_eq!(manifest.set, "segmentation/segment-01");
    // Its digest is bound by the loop round's records, so it keeps the old name, read as
    // the current one.
    assert_eq!(
        whizbang::evaluation::sets_repository(&manifest.images_repo),
        Some(whizbang::evaluation::SETS_REPOSITORY)
    );
    assert_eq!(
        manifest.images_rev,
        "d5413a4f0806c0c8d4e0376eb71bffe3d541b98e"
    );
    assert_eq!(manifest.item.len(), 20);
    let groups: Vec<(String, usize)> = manifest.groups().into_iter().collect();
    assert_eq!(
        groups,
        [
            ("keep-the-hole".to_owned(), 4),
            ("parts".to_owned(), 6),
            ("thin".to_owned(), 5),
            ("whole".to_owned(), 5)
        ]
    );
    let ids: BTreeSet<&str> = manifest.item.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids.len(), 20);
    let pin = |p: &str| {
        p.strip_prefix("sha256:").is_some_and(|hex| {
            hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        })
    };
    let truths: BTreeSet<&str> = manifest
        .item
        .iter()
        .map(|e| e.truth_sha256.as_str())
        .collect();
    for entry in &manifest.item {
        assert!(
            pin(&entry.image_sha256) && pin(&entry.truth_sha256),
            "{}",
            entry.id
        );
        assert!(
            !truths.contains(entry.image_sha256.as_str()),
            "{}",
            entry.id
        );
        assert_eq!(entry.image, format!("items/{}/image.png", entry.id));
        assert_eq!(entry.truth, format!("items/{}/truth.png", entry.id));
    }
    // A manifest that names a truth mask as a picture is refused.
    let text = std::fs::read_to_string(&file).unwrap();
    let first = &manifest.item[0];
    let turned = text.replacen(&first.image_sha256, &manifest.item[1].truth_sha256, 1);
    assert!(Manifest::parse(&turned).unwrap_err().contains("truth mask"));
}

#[test]
fn the_e_loop_config_is_a_template_of_the_five_priced_judges() {
    let file =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../labs/e-loop/config.toml");
    let bytes = std::fs::read(&file).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    let config =
        whizbang_core::config::Config::parse(&bytes, &whizbang_core::contract::Secrets::none())
            .expect("the round's config is one");
    let mut judges: Vec<Option<&str>> = (config
        .backends(whizbang_core::contract::Capability::Judge))
    .map(|(_, backend)| backend.model())
    .collect();
    judges.sort();
    assert_eq!(
        judges,
        [
            Some("claude-haiku-5-5"),
            Some("gemini-3.8-flash"),
            Some("gemini-4-argon"),
            Some("gpt-6.1-sol"),
            Some("gpt-6.1-terra")
        ]
    );
    // A template: it names where each key is, never a key.
    assert!(config.call_cap_micros().is_some());
    assert!(!text.lines().any(|l| l.trim() == "[cache]"));
    for line in text.lines().filter(|l| l.contains("key")) {
        assert!(
            line.starts_with('#') || line.starts_with("key_env = \""),
            "{line}"
        );
    }
}

/// Every committed set names the one repository the pictures are in, by its current name or
/// an old one (plan mE, task B1, item 6), and none is edited for it.
#[test]
fn every_committed_set_resolves_to_the_one_repository() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evalsets");
    let found = whizbang_eval::sets::discover(&root).unwrap();
    assert!(found.len() > 10, "{}", found.len());
    for dir in found {
        let set = whizbang_eval::sets::Set::load(&dir).unwrap();
        assert_eq!(
            whizbang::evaluation::sets_repository(&set.manifest.set.images_repo),
            Some(whizbang::evaluation::SETS_REPOSITORY),
            "{}",
            set.id()
        );
    }
    assert_eq!(
        whizbang::evaluation::sets_repository("github.com/someone/else"),
        None
    );
}
