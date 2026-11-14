//! `whizbang spend`: a stub (plan 1B, task B0b). The command is there so that its place
//! in the binary is: the ledger it reads, and `status` and `reset`, are built by the
//! tasks of the spend strand, in this file. Until then it takes no argument, reads no
//! store and answers with one complaint.

/// What `whizbang spend` says while it is a stub.
pub const NOT_BUILT: &str = "spend is not built in this version: there is no ledger yet, so nothing was read and nothing was changed";

/// Runs `whizbang spend`: always the complaint, which the binary prints as it prints
/// every complaint, with exit code 1.
pub fn run() -> Result<String, String> {
    Err(NOT_BUILT.to_owned())
}
