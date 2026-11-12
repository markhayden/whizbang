use whizbang_core::contract::{Action, Kind};
use whizbang_testkit::echo::Echo;

#[test]
fn echo_is_a_valid_action_named_echo() {
    let echo = Echo::new();
    let descriptor = echo.descriptor();
    assert_eq!(descriptor.name(), "echo");
    assert_eq!(descriptor.kind(), Kind::Primitive);
    assert!(descriptor.argument("text").unwrap().is_required());
    assert!(descriptor.needs().is_empty());
}
