//! `whizbang rascal grow`: wiring from arguments to the door (`whizbang::door`), which
//! runs the action through the executor.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use whizbang::door::{self, GrowAsked};
use whizbang::wiring::{self, Make, Over};
use whizbang_core::contract::Recovery;

use crate::store::Where;

const EXIT_CODES: &str = "Exit codes:
  0  the run completed, and its outputs are in the store
  1  nothing ran, or the run did not complete: the complaint says which, and names the run
  2  the command line was not understood
  3  what was to be printed could not be written
  101  the command itself failed (a panic outside an action): one fixed line is printed";

/// The config a command runs under, and the env file its credentials are in (ADR
/// 0011, "Where the file is"). Neither has a default place: no file in the working
/// folder, none in a user's folder, and no variable names one.
#[derive(Args, Default)]
pub struct Under {
    /// The config file: the backends, the cap of one call and the recipients that
    /// may receive data. With none the command runs under the empty config: no
    /// backend, no cap, and no recipient approved.
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,
    /// The env file that holds the values of the variables the config names for its
    /// credentials. A config that names a variable is not used without it.
    #[arg(long, value_name = "FILE", requires = "config")]
    env_file: Option<PathBuf>,
}

impl Under {
    /// What an executor of this build is built over for a command given these: the
    /// two files, the kinds of this build, and no secret beside those the env file
    /// holds.
    pub fn over(&self) -> Over {
        Over::nothing()
            .config(self.config.as_deref())
            .env_file(self.env_file.as_deref())
            .kinds(wiring::kinds())
    }
}

#[derive(Subcommand)]
pub enum Command {
    /// Grow the core of a mask by a disc, and further downward for a shadow. The blend
    /// is cleared, the profile and the prescriptions are removed, and protect is kept.
    /// The new mask is committed to the store, and its identity is printed.
    #[command(after_help = EXIT_CODES, allow_negative_numbers = true)]
    Grow {
        /// The mask to grow, by its identity in the store: sha256: and 64 hex digits, as
        /// `whizbang store import` prints it.
        #[arg(long)]
        mask: String,
        /// The radius of the disc, in pixels of the mask's image, from 0 to 10000. A
        /// pixel is grown when its centre is at most this far from the centre of a core
        /// pixel.
        #[arg(long)]
        px: f64,
        /// How many pixels further down the grown core reaches, from 0 to 10000: whole
        /// rows, so a part of a row adds nothing.
        #[arg(long)]
        down_px: Option<f64>,
        #[command(flatten)]
        at: Where,
        #[command(flatten)]
        under: Under,
        /// An id for this request, of your choosing: 1 to 128 letters, digits, `_`, `-`,
        /// `.` and `:`. The same request sent again under it is answered with the run
        /// it made the first time, and nothing runs again.
        #[arg(long, value_name = "ID")]
        request_id: Option<String>,
    },
}

/// Runs a rascal command: what to print on success, or the complaint.
pub fn run(command: &Command) -> Result<String, String> {
    run_with(command, wiring::recovery())
}

/// The same, with the store recovered by this before the executor is built over it.
pub fn run_with(command: &Command, recovery: Recovery) -> Result<String, String> {
    match command {
        Command::Grow {
            mask,
            px,
            down_px,
            at,
            under,
            request_id,
        } => {
            // A config that cannot be used is said before the store is opened, in
            // the readers' own words, which name the key or the line and no path.
            let over = under.over();
            wiring::loaded(&over)?;
            // A command that runs an action makes no store where there is none. The
            // executor is built over the writer, which the one entry point hands out
            // once the store is recovered.
            let opening = wiring::opening(at.root(), Make::Nothing, recovery)
                .map_err(|error| wiring::not_opened(&error))?;
            let store = opening.writer().map_err(|why| why.to_string())?;
            let registry = wiring::registry()?;
            let executor = wiring::runner(registry, store, &over)?;
            let asked = GrowAsked {
                mask,
                px: *px,
                down_px: *down_px,
                request_id: request_id.as_deref(),
            };
            door::grow(&executor, &asked)
        }
    }
}
