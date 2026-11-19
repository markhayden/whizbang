//! Rascal's operations and tasks (`SPEC.md` section 3): the composite of a fill onto an
//! image under a mask, and the actions `segment`, `select`, `grow`, `plug`, `feather`,
//! `render`, `paste` and `fill`.

pub mod composite;
pub mod feather;
pub mod fill;
pub mod grow;
mod operation;
pub mod paste;
pub mod plug;
pub mod render;
pub mod segment;
pub mod select;
