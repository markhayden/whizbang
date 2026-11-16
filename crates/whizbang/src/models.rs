//! `whizbang models`: a stub (plan 1C, task C1a). The command is there so that its
//! place in the binary is: `list`, `get` and `remove` are built by the tasks of the
//! network and models strand, in this file. Until then it reads no models directory,
//! reaches nothing and answers with one complaint.

/// What `whizbang models` says while it is a stub.
pub const NOT_BUILT: &str = "models is not built in this version: there is no shipped catalogue and no download client yet, so nothing was listed, nothing was downloaded and nothing was removed";

/// Runs `whizbang models`: always the complaint, which the binary prints as it prints
/// every complaint, with exit code 1.
pub fn run() -> Result<String, String> {
    Err(NOT_BUILT.to_owned())
}
