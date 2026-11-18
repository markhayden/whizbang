//! `whizbang rascal segment`, `select`, `grow`, `plug`, `feather` and `render`: wiring
//! from arguments to the door (`whizbang::door`), which runs the action through the
//! executor.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use whizbang::door::{self, GrowAsked, OperationAsked, SegmentAsked};
use whizbang::pack::OneWorker;
use whizbang::wiring::{self, Make, Models, Over, Platform, Uses};
use whizbang_core::contract::{Capability, Engine, Recovery, Value};

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
    /// credentials. A command that will use a backend naming one is not run without it.
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
    /// A config whose backends the command will use (`uses`) names a credential and
    /// no env file was given is said first ([`wiring::env_file_asked`]); then a config
    /// that cannot be used, in the readers' own words, which name the key or the line
    /// and no path; then a models directory that is not used, in the opening's.
    pub fn over(&self, platform: Option<Platform>, uses: Uses<'_>) -> Result<Over, String> {
        let worker = OneWorker::installed();
        let over = Over::nothing()
            .config(self.config.as_deref())
            .env_file(self.env_file.as_deref())
            .kinds(wiring::kinds(&worker))
            .pack(worker);
        wiring::env_file_asked(&over, uses)?;
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
    /// Find each instance of a phrase, or the thing at a point or in a box, in an image
    /// with the configured segmenter: one mask for each, its core the instance,
    /// committed to the store, best first. Each mask's identity is printed with the
    /// instance's score and box (left, top, right, bottom, in pixels of the image), which
    /// the run's diagnostic `segment.txt` keeps. Exactly one of --phrase, --point and
    /// --box is given.
    #[command(after_help = EXIT_CODES)]
    #[command(group(clap::ArgGroup::new("query").required(true).args(["phrase", "point", "box_"])))]
    Segment {
        /// The image to look in, by its identity in the store: sha256: and 64 hex
        /// digits, as `whizbang store import` prints it.
        #[arg(long)]
        image: String,
        /// What to find, in words.
        #[arg(long)]
        phrase: Option<String>,
        /// The thing at this place: X and Y in pixels of the image, such as "120 80".
        #[arg(long, value_name = "X Y")]
        point: Option<String>,
        /// The thing in this box: left, top, right and bottom in pixels of the image, as
        /// `rascal segment` prints a box, such as "10 20 30 40".
        #[arg(long = "box", id = "box_", value_name = "L T R B")]
        box_: Option<String>,
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
    /// Make one mask of the instance masks to keep: the union of their cores, minus any
    /// exclude boxes, with no blend and nothing protected. The masks are named with
    /// --mask, or are those of a segment run (--run), all of them or those picked by
    /// --pick; --at keeps those whose core holds a pixel. The new mask is committed to
    /// the store, and its identity is printed.
    #[command(after_help = EXIT_CODES)]
    Select {
        /// A mask to keep, by its identity in the store, as `rascal segment` prints it:
        /// once for each, 1 to 64 of them, all on one image.
        #[arg(
            long = "mask",
            value_name = "ID",
            required_unless_present = "run",
            conflicts_with = "run"
        )]
        masks: Vec<String>,
        /// A segment run, by its id as `rascal segment` prints it: its masks, in the
        /// order it printed them.
        #[arg(long, value_name = "RUN")]
        run: Option<String>,
        /// The Nth mask of the run, counted from 1: once for each to keep.
        #[arg(
            long,
            value_name = "N",
            requires = "run",
            conflicts_with = "masks",
            value_parser = clap::value_parser!(u32).range(1..=64)
        )]
        pick: Vec<u32>,
        /// Keep only the masks whose core holds the pixel at this place: X and Y in
        /// pixels of the image, such as "120 80".
        #[arg(long, value_name = "X Y")]
        at: Option<String>,
        /// A box to clear from the core: left, top, right and bottom in pixels of the
        /// image, as `rascal segment` prints a box, such as "10 20 30 40". A pixel is
        /// cleared when its centre is inside. Once for each box.
        #[arg(long, value_name = "BOX", allow_hyphen_values = true)]
        exclude: Vec<String>,
        #[command(flatten)]
        common: Common,
    },
    /// Fill the holes of a mask's core: each region outside the core that touches no
    /// edge of the image and is at most a share of the area of the shape around it. The
    /// blend is cleared, the profile and the prescriptions are removed, and protect is
    /// kept. The new mask is committed to the store, and its identity is printed.
    #[command(after_help = EXIT_CODES)]
    Plug {
        /// The mask, by its identity in the store.
        #[arg(long)]
        mask: String,
        /// The largest hole filled, as a share of the area of the shape around it, from
        /// 0 to 1.
        #[arg(long, default_value_t = whizbang_rascal::plug::MAX_SHARE_DEFAULT)]
        max_share: f64,
        #[command(flatten)]
        common: Common,
    },
    /// Set the blend of a mask to a feather of its core's edge, outward, out to three
    /// sigmas. The core is kept, and protect is kept and wins. The new mask is committed
    /// to the store, and its identity is printed.
    #[command(after_help = EXIT_CODES)]
    Feather {
        /// The mask, by its identity in the store.
        #[arg(long)]
        mask: String,
        /// The feather's sigma, in pixels of the mask's image, from 0.1 to 10000.
        #[arg(long)]
        sigma_px: f64,
        #[command(flatten)]
        common: Common,
    },
    /// Make a mask into a picture, an image whose role is a rendering: `alpha`, the
    /// image with the mask's weight as its alpha (the cut-out), or `plain`, the weight in
    /// grey. The image is committed to the store, and its identity is printed; `store
    /// export` writes it as a PNG.
    #[command(after_help = EXIT_CODES)]
    Render {
        /// The mask, by its identity in the store.
        #[arg(long)]
        mask: String,
        /// `alpha` or `plain`.
        #[arg(long, value_parser = ["alpha", "plain"])]
        profile: String,
        /// The image the mask is on, by its identity in the store, which `alpha` needs.
        #[arg(long)]
        image: Option<String>,
        #[command(flatten)]
        common: Common,
    },
}

/// What every operation's command takes besides its own: the store, the config, and the
/// request's id.
#[derive(Args)]
pub struct Common {
    #[command(flatten)]
    at: Where,
    #[command(flatten)]
    under: Under,
    /// An id for this request, of your choosing: 1 to 128 letters, digits, `_`, `-`,
    /// `.` and `:`. The same request sent again under it is answered with the run it
    /// made the first time, and nothing runs again.
    #[arg(long, value_name = "ID")]
    request_id: Option<String>,
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
pub(crate) fn executor(
    at: &Where,
    under: &Under,
    recovery: Recovery,
    platform: Option<Platform>,
    uses: Uses<'_>,
) -> Result<Engine, String> {
    let over = under.over(platform, uses)?;
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
            let executor = executor(at, under, recovery, platform, Uses::Nothing)?;
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
            point,
            box_,
            max_instances,
            backend,
            at,
            under,
            request_id,
        } => {
            let uses = Uses::One(Capability::Segment, backend.as_deref());
            let executor = executor(at, under, recovery, platform, uses)?;
            // Exactly one is given: the command line holds it to that.
            let (kind, query) = match (phrase, point, box_) {
                (Some(phrase), _, _) => ("phrase", phrase.as_str()),
                (None, Some(point), _) => ("point", point.as_str()),
                (None, None, Some(boxed)) => ("box", boxed.as_str()),
                (None, None, None) => {
                    return Err("give one of --phrase, --point and --box".to_owned());
                }
            };
            let asked = SegmentAsked {
                image,
                kind,
                query,
                max_instances: *max_instances,
                backend: backend.as_deref(),
                request_id: request_id.as_deref(),
            };
            let root = at.root().to_path_buf();
            door::segment(&executor, &asked, &|run, name| {
                door::diagnostic(&root, run, name)
            })
        }
        Command::Select {
            masks,
            run,
            pick,
            at,
            exclude,
            common,
        } => {
            let mut arguments = Vec::new();
            if !exclude.is_empty() {
                arguments.push(("exclude", Value::Text(exclude.join(";"))));
            }
            if let Some(at) = at {
                arguments.push(("at", Value::text(at)));
            }
            // The masks of a run, all or those picked, read before anything is run.
            let from_run = match run {
                Some(run) => {
                    let all = door::run_masks(common.at.root(), run)?;
                    if pick.is_empty() {
                        all
                    } else {
                        door::picked(&all, pick)?
                    }
                }
                None => Vec::new(),
            };
            let (flag, masks) = match run {
                Some(_) => ("--run", &from_run),
                None => ("--mask", masks),
            };
            let inputs = masks.iter().map(|m| ("mask", flag, m.as_str()));
            operation(
                common,
                "select",
                arguments,
                inputs.collect(),
                recovery,
                platform,
            )
        }
        Command::Plug {
            mask,
            max_share,
            common,
        } => {
            let arguments = vec![("max_share", Value::Number(*max_share))];
            let inputs = vec![("mask", "--mask", mask.as_str())];
            operation(common, "plug", arguments, inputs, recovery, platform)
        }
        Command::Feather {
            mask,
            sigma_px,
            common,
        } => {
            let arguments = vec![("sigma_px", Value::Number(*sigma_px))];
            let inputs = vec![("mask", "--mask", mask.as_str())];
            operation(common, "feather", arguments, inputs, recovery, platform)
        }
        Command::Render {
            mask,
            profile,
            image,
            common,
        } => {
            let arguments = vec![("profile", Value::text(profile))];
            let mut inputs = vec![("mask", "--mask", mask.as_str())];
            if let Some(image) = image {
                inputs.push(("image", "--image", image.as_str()));
            }
            operation(common, "render", arguments, inputs, recovery, platform)
        }
    }
}

/// Runs one of rascal's operations through the executor the command builds.
fn operation(
    common: &Common,
    action: &str,
    arguments: Vec<(&str, Value)>,
    inputs: Vec<(&str, &str, &str)>,
    recovery: Recovery,
    platform: Option<Platform>,
) -> Result<String, String> {
    let executor = executor(&common.at, &common.under, recovery, platform, Uses::Nothing)?;
    let asked = OperationAsked {
        action,
        arguments,
        inputs,
        request_id: common.request_id.as_deref(),
    };
    door::operation(&executor, &asked)
}
