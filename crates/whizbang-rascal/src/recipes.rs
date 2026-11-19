//! The recipes this build ships (plan m3, task R1, item 5): each a file of the format
//! `recipe/1` under `crates/whizbang-rascal/recipes/`, built into the binary and
//! addressable by its name, read and checked against the build's actions as a user's own
//! recipe is (`whizbang_core::recipe`).

use whizbang_core::contract::Registry;
use whizbang_core::recipe::{Recipe, RecipeAction, RecipeError};

/// `remove_prompt`: the mask that removes what a phrase names (rascal's `remove-prompt`).
pub const REMOVE_PROMPT: &str = include_str!("../recipes/remove_prompt.toml");

/// Each shipped recipe's text, in the order a door lists them.
pub const SHIPPED: [&str; 1] = [REMOVE_PROMPT];

/// A recipe's text, read and checked against the actions of `registry`, as an action.
pub fn action_of(text: &str, registry: &Registry) -> Result<RecipeAction, RecipeError> {
    let checked = Recipe::read(text)?.check(registry)?;
    RecipeAction::new(checked, text, registry)
}

/// Every shipped recipe, as an action over the actions of `registry`.
pub fn shipped(registry: &Registry) -> Result<Vec<RecipeAction>, RecipeError> {
    SHIPPED
        .iter()
        .map(|text| action_of(text, registry))
        .collect()
}
