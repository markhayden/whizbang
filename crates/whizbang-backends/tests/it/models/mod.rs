//! The models directory's types (ADR 0017): the names that become names on a disk and
//! the catalogue. Nothing here
//! installs: no test touches a disk or asks anything of a network.

mod catalogue;
mod names;
mod refusals;

use whizbang_backends::models::{
    CatalogueFile, Entry, EntryParts, FileName, Licence, ModelName, Refused, Revision,
};
use whizbang_core::contract::Secrets;
use whizbang_core::egress::{Address, Origin};
use whizbang_core::identity::Digest;

/// A value no test would write by accident, told to the readers as a secret. Every
/// character of it may stand in a model's name and in a file's, so that a name is
/// refused for the secret and for nothing else.
pub const MARKER: &str = "sk-marker-7f3a9c";

pub fn secrets() -> Secrets {
    Secrets::new([MARKER.to_owned()]).unwrap()
}

pub fn none() -> Secrets {
    Secrets::none()
}

pub fn model(name: &str) -> ModelName {
    ModelName::new(name, &none()).unwrap()
}

pub fn file_name(name: &str) -> FileName {
    FileName::new(name, &none()).unwrap()
}

pub fn revision(text: &str) -> Revision {
    Revision::new(text, &none()).unwrap()
}

pub fn licence(text: &str) -> Licence {
    Licence::new(text, &none()).unwrap()
}

pub fn origin(text: &str) -> Origin {
    Origin::of(text, &none()).unwrap()
}

/// The digest of these bytes, as a file's hash is.
pub fn hash(of: &[u8]) -> Digest {
    Digest::of_bytes(of)
}

/// A file of a catalogue entry, downloaded from the origin every entry here names.
pub fn file(name: &str, size: u64, of: &[u8]) -> CatalogueFile {
    let from = Address::of(
        &format!("https://weights.example.org/files/{name}"),
        &none(),
    )
    .unwrap();
    CatalogueFile::new(file_name(name), size, hash(of), from, &none()).unwrap()
}

/// The parts of an entry with two files, which each test changes one of.
pub fn parts(name: &str) -> EntryParts {
    EntryParts {
        name: model(name),
        revision: revision("2026-11.1"),
        adapter: model("owlv2-sam2"),
        licence: licence("Apache-2.0"),
        features: vec!["phrase".to_owned(), "box".to_owned()],
        origins: vec![origin("https://weights.example.org")],
        files: vec![
            file("graph.onnx", 3, b"abc"),
            file("LICENSE.txt", 5, b"terms"),
        ],
    }
}

pub fn entry(name: &str) -> Entry {
    Entry::new(parts(name), &none()).unwrap()
}

/// What is printed of a refusal, both ways.
pub fn printed(refused: &Refused) -> String {
    format!("{refused} {refused:?}")
}

/// A refusal quotes nothing it was given: what is printed of it is one line of printable
/// ASCII, short, and holds none of these texts.
pub fn shows_nothing_of(refused: &Refused, given: &[&str]) {
    let printed = printed(refused);
    assert!(
        printed.bytes().all(|b| b.is_ascii_graphic() || b == b' '),
        "{printed:?}"
    );
    assert!(printed.len() <= 400, "{} characters", printed.len());
    for text in given {
        assert!(!printed.contains(text), "{printed:?} holds {text:?}");
    }
}
