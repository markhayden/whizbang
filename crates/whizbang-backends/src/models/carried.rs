//! The texts the binary carries, which a catalogue's file may name in place of an
//! address (plan m6, task W1; ADR 0017, "A file the binary carries"). An install writes
//! such a file from the binary, held to the size and the hash its entry pins, as a
//! download is: nothing is asked of any address for it, and no origin is needed.
//!
//! There is one: `apache-2.0`, the terms of the Apache License, Version 2.0, as
//! whizbang's own `LICENSE` holds them, from its first line through `END OF TERMS AND
//! CONDITIONS` and the line break after it. The appendix after that is how a licensor
//! applies the licence to its own work, and names whizbang's licensor: it is not
//! carried, so a model's copy names nobody's copyright.

/// Whizbang's own licence, as the repository holds it.
const LICENSE: &[u8] = include_bytes!("../../../../LICENSE");

/// The last line of the licence's terms.
const END_OF_TERMS: &[u8] = b"END OF TERMS AND CONDITIONS\n";

/// A text the binary carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Carried {
    /// The terms of the Apache License, Version 2.0.
    Apache2,
}

impl Carried {
    /// Every text the binary carries.
    pub const ALL: [Carried; 1] = [Carried::Apache2];

    /// The text of this name, as a catalogue writes it.
    pub fn named(name: &str) -> Option<Carried> {
        Carried::ALL
            .into_iter()
            .find(|carried| carried.name() == name)
    }

    /// The name a catalogue writes it by.
    pub fn name(self) -> &'static str {
        match self {
            Carried::Apache2 => "apache-2.0",
        }
    }

    /// The bytes of the text.
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Carried::Apache2 => terms(),
        }
    }
}

/// The licence's terms: whizbang's `LICENSE` up to and with the end of its terms. A
/// `LICENSE` without that line is carried whole, and the hash an entry pins then
/// refuses its install.
fn terms() -> &'static [u8] {
    LICENSE
        .windows(END_OF_TERMS.len())
        .position(|window| window == END_OF_TERMS)
        .map_or(LICENSE, |at| &LICENSE[..at + END_OF_TERMS.len()])
}
