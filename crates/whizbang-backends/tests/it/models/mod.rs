//! The models directory's types (ADR 0017): the names that become names on a disk, and
//! what a refusal says. Nothing here installs: no test touches a disk or asks anything
//! of a network.

mod names;
mod refusals;

use whizbang_backends::models::Refused;
use whizbang_core::contract::Secrets;

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
