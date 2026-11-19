//! Rascal's operations and tasks (`SPEC.md` section 3): the composite of a fill onto an
//! image under a mask; the actions `segment`, `select`, `grow`, `close`, `plug`,
//! `subtract`, `feather`, `render`, `paste`, `fill`, `line`, `rect`, `stroke`, `rasterize`,
//! `shrink` and `invert`; and the recipes this build ships (`recipes`).
//!
//! Each operation's module holds its own descriptor, its own `register` and, where its
//! command is the common one, its own command's flags (`Args`): so a new operation adds
//! one line to the binary's registry and one to its command (plan m3, "Shared files").

pub mod close;
pub mod composite;
pub mod feather;
pub mod fill;
pub mod grow;
pub mod invert;
pub mod line;
pub mod operation;
pub mod paste;
pub mod plug;
pub mod rasterize;
pub mod recipes;
pub mod rect;
pub mod render;
pub mod segment;
pub mod select;
pub mod shrink;
pub mod stroke;
pub mod subtract;
