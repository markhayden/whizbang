//! What the `whizbang` binary and its tests share. The binary itself (`main.rs`) is
//! argument parsing; this is the wiring it calls.

pub mod backend_status;
pub mod baseline;
pub mod baseline_live;
pub mod door;
pub mod edits;
pub mod evaluation;
pub mod gating;
pub mod harness;
pub mod import;
pub mod laboratory;
pub mod loop_score;
pub mod loop_sheet;
pub mod model_commands;
pub mod pack;
pub mod pack_setup;
pub mod pairs_page;
pub mod parity;
pub mod serving;
pub mod sign_in;
pub mod tool_file;
pub mod transcript;
pub mod trying;
pub mod wiring;
