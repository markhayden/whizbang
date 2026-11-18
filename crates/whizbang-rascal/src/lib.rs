//! Rascal's operations and tasks (`SPEC.md` section 3): the composite of a fill onto an
//! image under a mask, and the actions `segment`, `select`, `grow`, `plug`, `feather` and
//! `render`.

pub mod composite;
pub mod feather;
pub mod grow;
mod operation;
pub mod plug;
pub mod render;
pub mod segment;
pub mod select;
