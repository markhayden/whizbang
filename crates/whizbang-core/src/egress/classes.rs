//! The four classes of data (`SPEC.md` section 10, "Egress"; ADR 0012).
//!
//! A config lists which classes each recipient may receive. Each part of what is sent
//! states its class, or states none; a part that states none needs every class
//! approved.

use std::fmt;

/// One class of data that may leave the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataClass {
    /// The pixels of an image a call was given as an input, as they are.
    SourceImages,
    /// Any other pixels: a crop, a composite, a rendering, a support plane, a mask.
    DerivedImages,
    /// Words a caller or an action wrote: a prompt, a phrase, a conversation's texts,
    /// its tool definitions and its tool calls.
    Prompts,
    /// What a tool returned to a judge.
    Evidence,
}

impl DataClass {
    /// Every class, in the order `SPEC.md` lists them.
    pub const ALL: [DataClass; 4] = [
        DataClass::SourceImages,
        DataClass::DerivedImages,
        DataClass::Prompts,
        DataClass::Evidence,
    ];

    /// The class's name, as a config and a record spell it.
    pub fn name(self) -> &'static str {
        match self {
            DataClass::SourceImages => "source_images",
            DataClass::DerivedImages => "derived_images",
            DataClass::Prompts => "prompts",
            DataClass::Evidence => "evidence",
        }
    }

    fn bit(self) -> u8 {
        match self {
            DataClass::SourceImages => 1,
            DataClass::DerivedImages => 2,
            DataClass::Prompts => 4,
            DataClass::Evidence => 8,
        }
    }
}

/// A set of classes: what a request holds, or what a recipient may receive. What
/// `{:?}` prints is the names of the classes it holds, in the order of
/// [`DataClass::ALL`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Classes {
    bits: u8,
}

impl Classes {
    /// No class.
    pub fn none() -> Classes {
        Classes { bits: 0 }
    }

    /// All four.
    pub fn all() -> Classes {
        Classes::of(&DataClass::ALL)
    }

    /// These classes. One named twice is held once.
    pub fn of(classes: &[DataClass]) -> Classes {
        classes
            .iter()
            .fold(Classes::none(), |set, class| set.with(*class))
    }

    /// The same set, holding this class too.
    pub fn with(self, class: DataClass) -> Classes {
        Classes {
            bits: self.bits | class.bit(),
        }
    }

    /// Whether the set holds the class.
    pub fn holds(self, class: DataClass) -> bool {
        self.bits & class.bit() != 0
    }

    /// Whether the set holds every class of `other`: what an approval is asked, of the
    /// classes a request needs.
    pub fn covers(self, other: Classes) -> bool {
        other.bits & !self.bits == 0
    }

    /// Whether the set holds no class.
    pub fn is_empty(self) -> bool {
        self.bits == 0
    }

    /// The classes it holds, in the order of [`DataClass::ALL`].
    pub fn iter(self) -> impl Iterator<Item = DataClass> {
        DataClass::ALL
            .into_iter()
            .filter(move |class| self.holds(*class))
    }

    /// The classes a recipient must be approved for before it is handed parts that
    /// state these classes: each class stated, and **all four when any part states
    /// none**. No part needs no class.
    pub fn needed_for(parts: impl IntoIterator<Item = Option<DataClass>>) -> Classes {
        let mut needed = Classes::none();
        for stated in parts {
            match stated {
                Some(class) => needed = needed.with(class),
                None => return Classes::all(),
            }
        }
        needed
    }
}

impl fmt::Debug for Classes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.iter().map(DataClass::name).collect();
        write!(f, "Classes({})", names.join(", "))
    }
}
