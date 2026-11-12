//! `echo`: a fake action. Tests register it; the shipping binary never does, and does not
//! depend on this crate.

use whizbang_core::contract::{Action, ActionDescriptor, Argument, Kind, ValueType};

/// The fake action `echo`. Today it says what it is and does nothing: nothing can run an
/// action yet.
pub struct Echo {
    descriptor: ActionDescriptor,
}

impl Echo {
    pub fn new() -> Echo {
        let draft = ActionDescriptor::draft(
            "echo",
            "1.0.0",
            Kind::Primitive,
            "Returns the text it was given; a fake for tests, which never ships.",
        )
        .argument(Argument::required(
            "text",
            "The text to return.",
            ValueType::Text,
        ))
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Echo { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Echo {
    fn default() -> Echo {
        Echo::new()
    }
}

impl Action for Echo {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }
}
