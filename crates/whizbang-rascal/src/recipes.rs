//! The recipes this build ships (plan m3, task R1, item 5): each a file of the format
//! `recipe/1` under `crates/whizbang-rascal/recipes/`, built into the binary and
//! addressable by its name, read and checked against the build's actions as a user's own
//! recipe is (`whizbang_core::recipe`).

use whizbang_core::contract::Registry;
use whizbang_core::recipe::{Recipe, RecipeAction, RecipeError};

/// `remove_prompt`: the mask that removes what a phrase names (rascal's `remove-prompt`).
pub const REMOVE_PROMPT: &str = include_str!("../recipes/remove_prompt.toml");

/// `remove`: the thing at a location removed, ending in a fill (rascal's `remove`).
pub const REMOVE: &str = include_str!("../recipes/remove.toml");

/// `repair_area`: a box or a polygon as drawn, feathered and filled (rascal's
/// `repair-area`).
pub const REPAIR_AREA: &str = include_str!("../recipes/repair_area.toml");

/// `mask_rect`: a rectangle as a mask (rascal's `rect`).
pub const MASK_RECT: &str = include_str!("../recipes/mask_rect.toml");

/// `protect`: the mask for a second pass (rascal's `protect`).
pub const PROTECT: &str = include_str!("../recipes/protect.toml");

/// Each shipped recipe's text, in the order a door lists them.
pub const SHIPPED: [&str; 5] = [REMOVE_PROMPT, REMOVE, REPAIR_AREA, MASK_RECT, PROTECT];

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
