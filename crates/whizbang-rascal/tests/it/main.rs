//! Integration tests of whizbang-rascal.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod apply;
mod canvases;
mod carry;
mod close;
mod composite;
mod constants;
mod cut;
mod feather;
mod fill;
mod grow;
mod grow_action;
mod grow_runs;
mod heal;
mod inspect;
mod invert;
mod labels;
mod line;
mod ops_actions;
mod paste;
mod plug;
mod poisson;
mod put;
mod rasterize;
mod record;
mod rect;
mod render;
mod segment;
mod select;
mod shaping;
mod shrink;
mod stroke;
mod subtract;
mod task;
mod tighten;
mod tool_records;
mod unwindow;
mod window;
// The seam (plan m3, task O3).
mod fan;
mod profile;
mod ramp;
// The last of rascal's operations (plan m3, task O5).
mod measure;
