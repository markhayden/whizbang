//! What the `whizbang` binary and its tests share. The binary itself (`main.rs`) is
//! argument parsing; this is the wiring it calls.

pub mod backend_status;
#[cfg(feature = "baseline-replay")]
pub mod baseline;
pub mod door;
pub mod evaluation;
pub mod import;
pub mod model_commands;
pub mod pack;
pub mod pack_setup;
pub mod parity;
pub mod serving;
pub mod sign_in;
pub mod tool_file;
pub mod trying;
pub mod wiring;
