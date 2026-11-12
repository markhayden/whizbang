//! Registration: what a build can do is what was registered, in the order it was, and no
//! two actions share a name.

use std::sync::Arc;

use whizbang_core::contract::{Action, ActionDescriptor, Kind, Registry};

struct Fake(ActionDescriptor);

impl Action for Fake {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }
}

fn fake(name: &str) -> Arc<dyn Action> {
    let draft = ActionDescriptor::draft(name, "1.0.0", Kind::Primitive, "A fake action.");
    Arc::new(Fake(draft.build().unwrap()))
}

#[test]
fn a_new_registry_holds_nothing() {
    let registry = Registry::new();
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
    assert!(registry.get("grow").is_none());
}

#[test]
fn actions_are_listed_in_the_order_they_were_registered() {
    let mut registry = Registry::new();
    for name in ["zebra", "apple", "mango"] {
        registry.register(fake(name)).unwrap();
    }
    let names: Vec<&str> = registry.descriptors().map(|d| d.name()).collect();
    assert_eq!(names, ["zebra", "apple", "mango"]);
    assert_eq!(registry.len(), 3);
    assert_eq!(registry.get("apple").unwrap().descriptor().name(), "apple");
    assert!(registry.get("pear").is_none());
}

#[test]
fn a_name_is_registered_once() {
    let mut registry = Registry::new();
    registry.register(fake("grow")).unwrap();
    let error = registry.register(fake("grow")).err().unwrap();
    assert!(error.to_string().contains("grow"), "{error}");
    // The refusal changed nothing.
    assert_eq!(registry.len(), 1);
}

#[test]
fn an_action_that_breaks_the_rule_cannot_be_registered() {
    // Registration takes an action whose descriptor exists, and a descriptor whose name
    // or purpose breaks the rule does not: the refusal comes before the registry is
    // reached, and there is no other way to make one.
    let named = ActionDescriptor::draft("Not a name", "1.0.0", Kind::Primitive, "A fake.");
    assert!(named.build().is_err());
    let purposed = ActionDescriptor::draft("fake", "1.0.0", Kind::Primitive, "# Forged");
    assert!(purposed.build().is_err());
}
