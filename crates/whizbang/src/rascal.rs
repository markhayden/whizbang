//! `whizbang rascal grow`: wiring from arguments to the door (`whizbang::door`), which
//! runs the action through the executor.

use std::sync::Arc;

use clap::Subcommand;
use whizbang::door::{self, GrowAsked};
use whizbang::wiring;
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;

use crate::store::Where;

const EXIT_CODES: &str = "Exit codes:
  0  the run completed, and its outputs are in the store
  1  nothing ran, or the run did not complete: the complaint says which, and names the run
  2  the command line was not understood
  3  what was to be printed could not be written
  101  the command itself failed (a panic outside an action): one fixed line is printed";

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
        /// An id for this request, of your choosing: 1 to 128 letters, digits, `_`, `-`,
        /// `.` and `:`. The same request sent again under it is answered with the run
        /// it made the first time, and nothing runs again.
        #[arg(long, value_name = "ID")]
        request_id: Option<String>,
    },
}

/// Runs a rascal command: what to print on success, or the complaint.
pub fn run(command: &Command) -> Result<String, String> {
    match command {
        Command::Grow {
            mask,
            px,
            down_px,
            at,
            request_id,
        } => {
            // A command that runs an action makes no store where there is none.
            let store = Store::open_existing(at.root(), Arc::new(RealFs))
                .map_err(|error| error.to_string())?;
            let registry = wiring::registry().map_err(|error| error.to_string())?;
            let executor = wiring::runner(registry, Arc::new(store));
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
