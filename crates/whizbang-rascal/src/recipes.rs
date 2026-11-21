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

/// `repair`: a stroke tightened to the defect's edges, feathered and filled (rascal's
/// `repair`).
pub const REPAIR: &str = include_str!("../recipes/repair.toml");

/// `seam`: the wrap edge of a panorama healed (rascal's `seam` and its `heal`; task O3).
pub const SEAM: &str = include_str!("../recipes/seam.toml");

/// Each shipped recipe's text, in the order a door lists them.
pub const SHIPPED: [&str; 7] = [
    REMOVE_PROMPT,
    REMOVE,
    REPAIR_AREA,
    MASK_RECT,
    PROTECT,
    REPAIR,
    SEAM,
];

/// The kit the shipped recipes belong to (ADR 0040).
pub const KIT: &str = "rascal";

/// Each shipped recipe's title, as a person reads it, by its name (plan m5, task W1,
/// item 2). The files say neither kit nor title: their text is left as it is, since what
/// was measured of a recipe names it by the digest of its text.
pub const TITLES: [(&str, &str); 7] = [
    ("remove_prompt", "Remove what you name"),
    ("remove", "Remove at a place"),
    ("repair_area", "Repair a spot"),
    ("mask_rect", "Mask a rectangle"),
    ("protect", "Protect a first edit"),
    ("repair", "Repair a scratch"),
    ("seam", "Heal a panorama seam"),
];

/// A shipped recipe's text as its action, with its kit and its title: [`action_of`], then
/// titled from [`TITLES`]. A text that is not a shipped recipe's is given no title.
pub fn shipped_action(text: &str, registry: &Registry) -> Result<RecipeAction, RecipeError> {
    let action = action_of(text, registry)?;
    let name = action.recipe().name.clone();
    match TITLES.iter().find(|(named, _)| *named == name) {
        Some((_, title)) => action.titled(KIT, title),
        None => Ok(action),
    }
}

/// A recipe's text, read and checked against the actions of `registry`, as an action.
pub fn action_of(text: &str, registry: &Registry) -> Result<RecipeAction, RecipeError> {
    let checked = Recipe::read(text)?.check(registry)?;
    RecipeAction::new(checked, text, registry)
}

/// Every shipped recipe, as an action over the actions of `registry`.
pub fn shipped(registry: &Registry) -> Result<Vec<RecipeAction>, RecipeError> {
    SHIPPED
        .iter()
        .map(|text| shipped_action(text, registry))
        .collect()
}
