//! Rascal's operations and tasks (`SPEC.md` section 3): the composite of a fill onto an
//! image under a mask; the actions `segment`, `select`, `grow`, `close`, `plug`,
//! `subtract`, `feather`, `render`, `paste`, `fill`, `line`, `rect`, `stroke`, `rasterize`,
//! `shrink`, `invert`, `tighten` and `inspect`; the canvases `cut`, `put`, `window`,
//! `unwindow` and `apply`; the seam's `fan`, `profile` and `ramp`; `measure`;
//! rascal's constant sets (`constants`); and the recipes this build ships (`recipes`).
//!
//! Each operation's module holds its own descriptor, its own `register` and, where its
//! command is the common one, its own command's flags (`Args`): so a new operation adds
//! one line to the binary's registry and one to its command (plan m3, "Shared files").

pub mod apply;
pub(crate) mod canvas;
// The loop: remove, check, retry (plan m4, task L1).
pub mod checked;
pub mod close;
pub mod composite;
pub mod constants;
pub mod cut;
pub mod feather;
pub mod fill;
pub mod grow;
pub mod inspect;
pub mod invert;
pub mod line;
pub mod operation;
pub mod paste;
pub mod plug;
pub mod poisson;
pub mod put;
pub mod rasterize;
pub mod recipes;
pub mod rect;
pub mod render;
pub mod segment;
pub mod select;
pub mod shrink;
pub mod stroke;
pub mod subtract;
pub mod tighten;
pub mod unwindow;
pub mod window;
// The seam (plan m3, task O3).
pub mod fan;
pub mod profile;
pub mod ramp;
// The last of rascal's operations (plan m3, task O5).
pub mod measure;
