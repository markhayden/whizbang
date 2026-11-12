//! `echo`: a fake action. Tests register it; the shipping binary never does, and does not
//! depend on this crate.

use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Failure, Kind, PreparedCall, Produced, Value,
    ValueType,
};

/// The fake action `echo`. It reports the text it was given as a progress event and
/// writes it as a diagnostic named `echo.txt`, and makes no artifact. Given `fail`, it
/// does both and then fails with that message; given `invalid`, it finds its text
/// invalid for that reason.
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
        .argument(Argument::optional(
            "fail",
            "Fail with this message, after reporting the text.",
            ValueType::Text,
        ))
        .argument(Argument::optional(
            "invalid",
            "Find the text invalid, for this reason.",
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

/// The text of an argument, when the call has it.
fn text<'a>(call: &'a PreparedCall, name: &str) -> Option<&'a str> {
    call.arguments().iter().find_map(|(n, value)| match value {
        Value::Text(text) if n == name => Some(text.as_str()),
        _ => None,
    })
}

impl Action for Echo {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let said = text(call, "text").unwrap_or_default();
        context.events().progress(&format!("echo: {said}"));
        context
            .diagnostics()
            .write("echo.txt", said.as_bytes())
            .map_err(|why| Failure::error("diagnostic", &why))?;
        if let Some(why) = text(call, "invalid") {
            return Err(Failure::invalid(why));
        }
        if let Some(message) = text(call, "fail") {
            return Err(Failure::error("echo", message));
        }
        context.events().progress("echo: done");
        Ok(Produced::nothing())
    }
}
