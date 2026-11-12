//! Digests and their text.

use whizbang_core::identity::{Digest, FileDigest, IdentityError, PixelDigest};

#[test]
fn a_digest_of_bytes_is_plain_sha256_with_a_prefix() {
    // The SHA-256 of the empty input and of "abc", from FIPS 180-4.
    assert_eq!(
        Digest::of_bytes(b"").to_string(),
        "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        Digest::of_bytes(b"abc").hex(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn a_digest_reads_back_from_its_own_text_and_from_nothing_else() {
    let digest = Digest::of_bytes(b"abc");
    assert_eq!(digest.to_string().parse::<Digest>().unwrap(), digest);
    let hex = digest.hex();
    for wrong in [
        hex.clone(),
        format!("md5:{hex}"),
        format!("sha256:{}", hex.to_uppercase()),
        format!("sha256:{}", &hex[1..]),
        format!("sha256:{hex}0"),
        format!("sha256:{}g", &hex[1..]),
        format!("sha256:{}é", &hex[2..]),
        String::new(),
    ] {
        assert!(
            matches!(wrong.parse::<Digest>(), Err(IdentityError::Digest(_))),
            "{wrong:?}"
        );
    }
}

#[test]
fn the_hex_digits_alone_read_back_and_nothing_else_does() {
    let digest = Digest::of_bytes(b"abc");
    let hex = digest.hex();
    assert_eq!(Digest::from_hex(&hex), Some(digest));
    for wrong in [
        digest.to_string(),
        hex.to_uppercase(),
        hex[1..].to_owned(),
        format!("{hex}0"),
        format!("{}g", &hex[1..]),
        format!("{}é", &hex[2..]),
        String::new(),
    ] {
        assert_eq!(Digest::from_hex(&wrong), None, "{wrong:?}");
    }
}

#[test]
fn digests_are_written_as_their_text_and_checked_when_read() {
    let digest = Digest::of_bytes(b"abc");
    let text = format!("\"{digest}\"");
    assert_eq!(serde_json::to_string(&digest).unwrap(), text);
    assert_eq!(serde_json::to_string(&FileDigest(digest)).unwrap(), text);
    assert_eq!(serde_json::to_string(&PixelDigest(digest)).unwrap(), text);
    assert_eq!(
        serde_json::from_str::<PixelDigest>(&text).unwrap().0,
        digest
    );
    assert!(serde_json::from_str::<Digest>("\"sha256:00\"").is_err());
}
