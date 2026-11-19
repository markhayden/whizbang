//! Rascal's constant sets (plan m3, "Copies are counted apart"): every set rascal shipped
//! is listed with its digest, and each one here is that file, byte for byte.

use std::collections::BTreeSet;
use std::path::Path;

use whizbang_core::identity::Digest;
use whizbang_rascal::constants::{self, DIGESTS, WINDOW_V1};

#[test]
fn every_set_rascal_shipped_is_here_byte_for_byte() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("constants");
    let mut listed = BTreeSet::new();
    let mut here = 0;
    for line in DIGESTS
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let [digest, name, state] = line.split("  ").collect::<Vec<_>>()[..] else {
            panic!("not a digest, a file and a state: {line}");
        };
        listed.insert(name.to_owned());
        let path = folder.join(name);
        match state {
            "here" => {
                let bytes = std::fs::read(&path).unwrap();
                assert_eq!(
                    Digest::of_bytes(&bytes).to_string(),
                    format!("sha256:{digest}"),
                    "{name}"
                );
                here += 1;
            }
            "owed" => assert!(!path.exists(), "{name} is here and listed as owed"),
            other => panic!("{other} is neither here nor owed"),
        }
    }
    // Rascal shipped eleven sets, and every file here is one of them.
    assert_eq!(listed.len(), 11);
    for entry in std::fs::read_dir(&folder).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        assert!(
            name == "digests.txt" || listed.contains(&name),
            "{name} is not listed"
        );
    }
    assert_eq!(
        here, 7,
        "window-v1, with window (O2); fan's four, profile-v1 and ramp-v1 (O3)"
    );
    // The sets fan, profile and ramp read are those files.
    let (sets, profile, ramp) = (constants::FAN, constants::PROFILE_V1, constants::RAMP_V1);
    for (name, text) in sets.iter().chain([&profile, &ramp]) {
        let file = std::fs::read(folder.join(format!("{name}.toml"))).unwrap();
        assert_eq!(text.as_bytes(), file.as_slice(), "{name}");
    }
    assert_eq!(constants::number(profile.1, "feather_ratio"), Some(0.55));
    assert_eq!(constants::number(ramp.1, "share"), Some(0.5));
    // The set the build holds is the file, and its value is rascal's.
    let file = std::fs::read(folder.join("window-v1.toml")).unwrap();
    assert_eq!(WINDOW_V1.1.as_bytes(), file.as_slice());
    assert_eq!(constants::number(WINDOW_V1.1, "context"), Some(0.5));
}
