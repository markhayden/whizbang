//! What the stubs of the binary share (plan 1C, task C1a): `whizbang backends` and
//! `setup` are there so that their places in the binary are, and each is built by its
//! own task, in its own file. This file goes with the last of them. (`models` was the
//! third, until task N3b built it.)

use std::ffi::OsString;

/// Whatever a stub was asked. It is taken, so that a verb of `SPEC.md` section 11
/// (`backends status --probe`) is answered with the stub's one complaint
/// and not with a usage error that would blame the caller; and it is never read, so
/// nothing of it is acted on or printed.
#[derive(clap::Args)]
pub struct Asked {
    #[arg(hide = true, trailing_var_arg = true, allow_hyphen_values = true)]
    rest: Vec<OsString>,
}
