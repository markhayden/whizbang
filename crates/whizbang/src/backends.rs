//! `whizbang backends`: a stub (plan 1C, task C1a). The command is there so that its
//! place in the binary is: `status`, with and without `--probe`, is built by its own
//! task, in this file. Until then it reads no config, probes nothing and answers with
//! one complaint.

/// What `whizbang backends` says while it is a stub.
pub const NOT_BUILT: &str = "backends is not built in this version: this build holds no kind of backend yet, so no config was read and nothing was probed";

/// Runs `whizbang backends`: always the complaint, which the binary prints as it prints
/// every complaint, with exit code 1.
pub fn run() -> Result<String, String> {
    Err(NOT_BUILT.to_owned())
}
