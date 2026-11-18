//! `whizbang rascal grow` and `whizbang rascal segment`: wiring from arguments to the
//! door (`whizbang::door`), which runs the action through the executor.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use whizbang::door::{self, GrowAsked, SegmentAsked};
use whizbang::pack::OneWorker;
use whizbang::wiring::{self, Make, Models, Over, Platform};
use whizbang_core::contract::{Engine, Recovery};

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
    /// The models directory: where installed models are kept, an absolute path. With
    /// none it is the one the config names (`dir` in its table `models`), and then
    /// the one in this machine's data folder for whizbang. It must be yours alone to
    /// write.
    #[arg(long, value_name = "DIR")]
    models_dir: Option<PathBuf>,
}

impl Under {
    /// The config and the env file, as the models commands take them.
    pub fn asked(&self) -> whizbang::model_commands::Under<'_> {
        whizbang::model_commands::Under {
            config: self.config.as_deref(),
            env_file: self.env_file.as_deref(),
        }
    }

    /// The directory `--models-dir` names, or none.
    pub fn models_dir(&self) -> Option<&std::path::Path> {
        self.models_dir.as_deref()
    }

    /// What an executor of this build is built over for a command given these: the
    /// two files, the kinds of this build over the one worker of this process, made
    /// here from the installation `whizbang setup` made, and given to the wiring
    /// too; no secret beside those the env file holds; and the models this build
    /// ships, in the models directory the flag names, or the config, or else the one
    /// in the machine's data folder, which `platform` says when it is asked, and it
    /// is asked only then.
    ///
    /// A config that cannot be used is said first, in the readers' own words, which
    /// name the key or the line and no path; then a models directory that is not
    /// used, in the opening's.
    pub fn over(&self, platform: Option<Platform>) -> Result<Over, String> {
        let worker = OneWorker::installed();
        let over = Over::nothing()
            .config(self.config.as_deref())
            .env_file(self.env_file.as_deref())
            .kinds(wiring::kinds(&worker))
            .pack(worker);
        wiring::loaded(&over)?;
        let models = Models::shipped()?.flag(self.models_dir.as_deref());
        over.installed(match platform {
            Some(says) => models.platform(says),
            None => models,
        })
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
    /// Find each instance of a phrase in an image with the configured segmenter: one
    /// mask for each, its core the instance, committed to the store. Each mask's
    /// identity is printed with the instance's score and box (left, top, right,
    /// bottom, in pixels of the image), which the run's diagnostic `segment.txt` keeps.
    #[command(after_help = EXIT_CODES)]
    Segment {
        /// The image to look in, by its identity in the store: sha256: and 64 hex
        /// digits, as `whizbang store import` prints it.
        #[arg(long)]
        image: String,
        /// What to find, in words.
        #[arg(long)]
        phrase: String,
        /// The most instances to find, from 1 to 64.
        #[arg(long, default_value_t = 8, value_parser = clap::value_parser!(u32).range(1..=64))]
        max_instances: u32,
        /// The segmenter, by its name in the config's `[backends.segment]` tables. With
        /// none, the config's default for segment answers.
        #[arg(long, value_name = "NAME")]
        backend: Option<String>,
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

/// Runs a rascal command: what to print on success, or the complaint. `platform`
/// says where the models directory in this machine's data folder is; it is `main`'s
/// reading of the environment, and is asked only if a models directory is to be
/// opened and nothing else named one.
pub fn run(command: &Command, platform: Platform) -> Result<String, String> {
    run_at(command, wiring::recovery(), Some(platform))
}

/// The same, with the store recovered by this before the executor is built over it,
/// on a machine with no data folder: what a test of this crate runs.
#[cfg(test)]
pub fn run_with(command: &Command, recovery: Recovery) -> Result<String, String> {
    run_at(command, recovery, None)
}

/// The executor a command that runs an action builds: over the config and the models
/// directory `under` names, said first when either cannot be used, and then over the
/// writer of the store at `at`. A command that runs an action makes no store where
/// there is none. The executor is built over the writer, which the one entry point
/// hands out once the store is recovered.
fn executor(
    at: &Where,
    under: &Under,
    recovery: Recovery,
    platform: Option<Platform>,
) -> Result<Engine, String> {
    let over = under.over(platform)?;
    let opening = wiring::opening(at.root(), Make::Nothing, recovery)
        .map_err(|error| wiring::not_opened(&error))?;
    let store = opening.writer().map_err(|why| why.to_string())?;
    let registry = wiring::registry()?;
    wiring::runner(registry, store, &over)
}

fn run_at(
    command: &Command,
    recovery: Recovery,
    platform: Option<Platform>,
) -> Result<String, String> {
    match command {
        Command::Grow {
            mask,
            px,
            down_px,
            at,
            under,
            request_id,
        } => {
            let executor = executor(at, under, recovery, platform)?;
            let asked = GrowAsked {
                mask,
                px: *px,
                down_px: *down_px,
                request_id: request_id.as_deref(),
            };
            door::grow(&executor, &asked)
        }
        Command::Segment {
            image,
            phrase,
            max_instances,
            backend,
            at,
            under,
            request_id,
        } => {
            let executor = executor(at, under, recovery, platform)?;
            let asked = SegmentAsked {
                image,
                phrase,
                max_instances: *max_instances,
                backend: backend.as_deref(),
                request_id: request_id.as_deref(),
            };
            let root = at.root().to_path_buf();
            door::segment(&executor, &asked, &|run, name| {
                door::diagnostic(&root, run, name)
            })
        }
    }
}
