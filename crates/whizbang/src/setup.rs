//! `whizbang setup`: a stub (plan 1C, task C1a). The command is there so that its
//! place in the binary is: installing the Python pack is built by its own task, in
//! this file. Until then it installs nothing, starts nothing and answers with one
//! complaint.

/// What `whizbang setup` says while it is a stub.
pub const NOT_BUILT: &str = "setup is not built in this version: there is no Python pack to install yet, so nothing was installed and nothing was started";

/// Runs `whizbang setup`: always the complaint, which the binary prints as it prints
/// every complaint, with exit code 1.
pub fn run() -> Result<String, String> {
    Err(NOT_BUILT.to_owned())
}
