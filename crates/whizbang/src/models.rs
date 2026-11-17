//! `whizbang models list|get|remove`: wiring from arguments to the binary's own
//! functions (`whizbang::model_commands`), which say what each does and when it works
//! out where the models directory is (plan 1C, task N3b; `docs/guides/models.md`).

use std::time::Duration;

use clap::Subcommand;
use whizbang::model_commands::{self, Tools, WAIT_LIMIT_SECONDS};
use whizbang::wiring::{Platform, SignIn};

use crate::rascal::Under;

const EXIT_CODES: &str = "Exit codes:
  0  done: the models were listed, the model is installed, or it was removed
  1  it was not done: the complaint says why
  2  the command line was not understood
  3  what was to be printed could not be written";

#[derive(Subcommand)]
pub enum Command {
    /// List the models of the catalogue this build ships: for each, its name, title,
    /// licence, size, platforms and state, apart by tabs. Nothing is made.
    #[command(after_help = EXIT_CODES)]
    List {
        #[command(flatten)]
        under: Under,
    },
    /// Install a model of the catalogue, or wait for the install of it that is
    /// running and end as that ends. Prints the installation's identity.
    #[command(after_help = EXIT_CODES)]
    Get {
        /// The model, by its name in the catalogue (`whizbang models list`).
        model: String,
        #[command(flatten)]
        under: Under,
    },
    /// Remove a model: no new run is given it, the runs that use it are waited for,
    /// and then its files are deleted. Past the wait nothing is removed.
    #[command(after_help = EXIT_CODES)]
    Remove {
        /// The model, by its name in the catalogue.
        model: String,
        /// How many seconds to wait for runs that use the model, from 0 to 86400.
        #[arg(long, value_name = "SECONDS", default_value_t = 30,
              value_parser = clap::value_parser!(u64).range(0..=WAIT_LIMIT_SECONDS))]
        wait: u64,
        #[command(flatten)]
        under: Under,
    },
}

/// Runs a models command: what to print on success, or the complaint. `platform` says
/// where the models directory in this machine's data folder is; it is `main`'s reading
/// of the environment, and is asked only if a models directory is to be looked in and
/// nothing else named one. What `get` says as it waits is printed on the standard
/// error, a line at a time, through the door's escape. `sign_in` says where the Hub's
/// sign-in keeps its token; it is `main`'s, and is asked only by `get` of a model that
/// needs it (plan 1C, D12).
pub fn run(command: &Command, platform: Platform, sign_in: SignIn) -> Result<String, String> {
    match command {
        Command::List { under } => {
            let tools = Tools::shipped(under.models_dir(), platform, sign_in)?;
            model_commands::list(under.asked(), &tools)
        }
        Command::Get { model, under } => {
            let tools = Tools::shipped(under.models_dir(), platform, sign_in)?;
            model_commands::get(under.asked(), model, &tools, &mut |line| {
                eprintln!("whizbang: {}", whizbang::door::printable(line));
            })
        }
        Command::Remove { model, wait, under } => {
            let tools = Tools::shipped(under.models_dir(), platform, sign_in)?;
            let wait = Duration::from_secs(*wait);
            model_commands::remove(under.asked(), model, wait, &tools)
        }
    }
}
