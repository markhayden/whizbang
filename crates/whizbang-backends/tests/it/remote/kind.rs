//! The kind's identity of a model: the digest of its files, worked out as the front
//! works it out (ADR 0046, "The identity").

use sha2::{Digest as _, Sha256};
use whizbang_backends::remote::{files_digest, version_of};

use super::{Asking, Host, named, phrase, request};

#[test]
fn the_digest_of_the_files_is_the_sha256_of_their_lines_sorted_by_name() {
    // Given out of order, worked out in the order of the names' bytes: `B` before `a`.
    let files = [
        ("a.bin", "sha256:".to_owned() + &"1".repeat(64)),
        ("B.json", "sha256:".to_owned() + &"2".repeat(64)),
    ];
    let lines = format!("B.json {}\na.bin {}\n", files[1].1, files[0].1);
    let hex: String = Sha256::digest(lines.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let digest = files_digest(files.iter().map(|(name, sha)| (*name, sha.clone())));
    assert_eq!(digest, format!("sha256:{hex}"));
    // The version a backend declares its model at is not taken for an installation's.
    assert_eq!(version_of(&digest), format!("files-{hex}"));
}

#[test]
fn the_front_names_the_digest_the_kind_works_out_of_the_catalogues_files() {
    let model = named(None, "fake-ok");
    let host = Host::serving(std::slice::from_ref(&model));
    let (installation, digest) = host.kind().expected(&model).unwrap();
    assert_eq!(installation, host.records[&model].identity());
    // The kind refuses any answer whose digest is not its own: this one was answered.
    let answered = Asking::of(&host)
        .ask(&model, &request(phrase(), 1))
        .unwrap();
    assert_eq!(answered.digest, digest);
}
