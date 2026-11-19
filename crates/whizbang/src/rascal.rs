//! `whizbang rascal segment`, `select`, `grow`, `close`, `plug`, `subtract`, `feather`,
//! `render`, `paste`, `fill`, `line`, `rect`, `stroke`, `rasterize`, `shrink`, `invert`,
//! `cut`, `put`, `window`, `unwindow` and `apply`, and the recipes: wiring from arguments
//! to the door (`whizbang::door`), which runs the action through the executor.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use whizbang::door::{self, GrowAsked, OperationAsked, SegmentAsked};
use whizbang::pack::OneWorker;
use whizbang::wiring::{self, Make, Models, Over, Platform, Uses};
use whizbang_core::contract::{Capability, Engine, Recovery, Value};
use whizbang_rascal::operation::Asked;

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
    #[command(about = whizbang_rascal::plug::ABOUT, after_help = EXIT_CODES)]
    Plug(Op<whizbang_rascal::plug::Args>),
    #[command(about = whizbang_rascal::feather::ABOUT, after_help = EXIT_CODES)]
    Feather(Op<whizbang_rascal::feather::Args>),
    #[command(about = whizbang_rascal::render::ABOUT, after_help = EXIT_CODES)]
    Render(Op<whizbang_rascal::render::Args>),
    #[command(about = whizbang_rascal::close::ABOUT, after_help = EXIT_CODES)]
    Close(Op<whizbang_rascal::close::Args>),
    #[command(about = whizbang_rascal::subtract::ABOUT, after_help = EXIT_CODES)]
    Subtract(Op<whizbang_rascal::subtract::Args>),
    #[command(about = whizbang_rascal::line::ABOUT, after_help = EXIT_CODES)]
    Line(Op<whizbang_rascal::line::Args>),
    #[command(about = whizbang_rascal::rect::ABOUT, after_help = EXIT_CODES)]
    Rect(Op<whizbang_rascal::rect::Args>),
    #[command(about = whizbang_rascal::stroke::ABOUT, after_help = EXIT_CODES)]
    Stroke(Op<whizbang_rascal::stroke::Args>),
    #[command(about = whizbang_rascal::rasterize::ABOUT, after_help = EXIT_CODES)]
    Rasterize(Op<whizbang_rascal::rasterize::Args>),
    #[command(about = whizbang_rascal::shrink::ABOUT, after_help = EXIT_CODES)]
    Shrink(Op<whizbang_rascal::shrink::Args>),
    #[command(about = whizbang_rascal::invert::ABOUT, after_help = EXIT_CODES)]
    Invert(Op<whizbang_rascal::invert::Args>),
    /// Remove the thing at a location from a picture and fill where it was: the recipe
    /// `remove`, whose steps are rasterize, grow, close, plug, subtract, feather and fill,
    /// each a run of its own under the recipe's. With --box, `rect` makes the location
    /// first, a run of its own. Each run is printed, and then the composite and the mask
    /// rebound onto it, committed to the store.
    #[command(after_help = EXIT_CODES)]
    Remove {
        #[command(flatten)]
        at: Located,
        /// A mask on the same picture whose pixels the mask keeps off, by its identity in
        /// the store.
        #[arg(long)]
        protect: Option<String>,
        /// The filler, by its name in the config's `[backends.fill]` tables. With none,
        /// the config's default for fill answers.
        #[arg(long, value_name = "NAME")]
        backend: Option<String>,
        #[command(flatten)]
        steps: Steps,
        #[command(flatten)]
        common: Common,
    },
    /// Repair a stain or a spot at a location and fill it: the recipe `repair_area`,
    /// whose steps are rasterize, feather and fill. With --box, `rect` makes the location
    /// first, a run of its own. Each run is printed, and then the composite and the mask
    /// rebound onto it, committed to the store.
    #[command(after_help = EXIT_CODES)]
    RepairArea {
        #[command(flatten)]
        at: Located,
        /// The filler, by its name in the config's `[backends.fill]` tables.
        #[arg(long, value_name = "NAME")]
        backend: Option<String>,
        #[command(flatten)]
        steps: Steps,
        #[command(flatten)]
        common: Common,
    },
    /// Make a rectangle on a picture a mask: the recipe `mask_rect`, whose steps are rect
    /// and rasterize. Each step's run is printed, and then the mask.
    #[command(after_help = EXIT_CODES)]
    MaskRect {
        /// The picture, by its identity in the store.
        #[arg(long)]
        image: String,
        /// The box: left, top, right and bottom in pixels of the picture.
        #[arg(long = "box", value_name = "L T R B", allow_hyphen_values = true)]
        box_: String,
        #[command(flatten)]
        steps: Steps,
        #[command(flatten)]
        common: Common,
    },
    /// Make the mask for a second pass: the recipe `protect`, whose step is invert. Give
    /// the composite a fill made and the mask it rebound onto it. The run is printed, and
    /// then the mask.
    #[command(after_help = EXIT_CODES)]
    Protect {
        /// The picture of the second pass: a fill's composite.
        #[arg(long)]
        image: String,
        /// The first pass's mask, on the picture.
        #[arg(long)]
        mask: String,
        #[command(flatten)]
        steps: Steps,
        #[command(flatten)]
        common: Common,
    },
    #[command(about = whizbang_rascal::cut::ABOUT, after_help = EXIT_CODES)]
    Cut(Op<whizbang_rascal::cut::Args>),
    #[command(about = whizbang_rascal::put::ABOUT, after_help = EXIT_CODES)]
    Put(Op<whizbang_rascal::put::Args>),
    #[command(about = whizbang_rascal::window::ABOUT, after_help = EXIT_CODES)]
    Window(Op<whizbang_rascal::window::Args>),
    #[command(about = whizbang_rascal::unwindow::ABOUT, after_help = EXIT_CODES)]
    Unwindow(Op<whizbang_rascal::unwindow::Args>),
    #[command(about = whizbang_rascal::apply::ABOUT, after_help = EXIT_CODES)]
    Apply(Op<whizbang_rascal::apply::Args>),
    /// Make the mask that removes what a phrase names from a picture: the recipe
    /// `remove_prompt`, whose steps are segment, select, grow, close, plug, subtract and
    /// feather, each a run of its own under the recipe's. The configured segmenter finds
    /// the instances. Each step's run is printed, and then the mask, committed to the
    /// store; with --until, the outputs of that step.
    #[command(after_help = EXIT_CODES)]
    RemovePrompt {
        /// The picture, by its identity in the store: sha256: and 64 hex digits, as
        /// `whizbang store import` prints it.
        #[arg(long)]
        image: String,
        /// What to remove, in words.
        #[arg(long)]
        prompt: String,
        /// A mask on the same picture whose pixels the mask keeps off, by its identity in
        /// the store.
        #[arg(long)]
        protect: Option<String>,
        /// The segmenter, by its name in the config's `[backends.segment]` tables. With
        /// none, the config's default for segment answers.
        #[arg(long, value_name = "NAME")]
        backend: Option<String>,
        #[command(flatten)]
        steps: Steps,
        #[command(flatten)]
        common: Common,
    },
    /// Run a recipe: a file of the format recipe/1, read and checked against this
    /// build's actions before anything runs, each of its steps a run of its own under the
    /// recipe's. Each step's run is printed, and then the recipe's outputs, committed to
    /// the store; with --until, the outputs of that step.
    #[command(after_help = EXIT_CODES)]
    Run {
        /// The recipe file.
        recipe: PathBuf,
        /// One of the recipe's inputs: NAME=VALUE, where the value is an artifact's
        /// identity in the store, or the text of a text input. Once for each.
        #[arg(long = "input", value_name = "NAME=VALUE")]
        inputs: Vec<String>,
        #[command(flatten)]
        steps: Steps,
        #[command(flatten)]
        common: Common,
    },
    /// Paste a fill onto its base image under a mask: every pixel the mask weighs
    /// nothing, outside its core and blend or under protect, is the base's. The
    /// composite, an image derived from the base, is committed to the store, and its
    /// identity is printed; when the fill changed nothing, the base's is.
    #[command(after_help = EXIT_CODES)]
    Paste {
        /// The base image, by its identity in the store.
        #[arg(long)]
        image: String,
        /// The fill, an image of the base's size, by its identity in the store.
        #[arg(long)]
        fill: String,
        /// The mask on the base, by its identity in the store.
        #[arg(long)]
        mask: String,
        /// How the fill is pasted: `feather`, by the mask's core and blend.
        #[arg(long, value_parser = ["feather"], default_value = "feather")]
        mode: String,
        #[command(flatten)]
        common: Common,
    },
    /// Fill what a mask covers in an image with the configured filler, and paste the
    /// answer: every pixel the mask weighs nothing is the original's. The composite and
    /// the mask rebound onto it are committed to the store, and both identities are
    /// printed; when the fill changed nothing, the image's and the mask's are.
    #[command(after_help = EXIT_CODES)]
    Fill {
        /// The image, by its identity in the store.
        #[arg(long)]
        image: String,
        /// The mask on the image, by its identity in the store.
        #[arg(long)]
        mask: String,
        /// The filler, by its name in the config's `[backends.fill]` tables. With none,
        /// the config's default for fill answers.
        #[arg(long, value_name = "NAME")]
        backend: Option<String>,
        #[command(flatten)]
        common: Common,
    },
}

/// Where a recipe that takes a location works: the picture, and the location given or
/// the box `rect` makes it of.
#[derive(Args)]
#[command(group(clap::ArgGroup::new("where").required(true).args(["location", "box_"])))]
pub struct Located {
    /// The picture, by its identity in the store.
    #[arg(long)]
    image: String,
    /// A location on the picture, by its identity in the store, as `rascal rect`,
    /// `stroke` or `line` prints it.
    #[arg(long)]
    location: Option<String>,
    /// A box on the picture: left, top, right and bottom in pixels, such as "10 20 30
    /// 40". `rect` makes it a location first.
    #[arg(
        long = "box",
        id = "box_",
        value_name = "L T R B",
        allow_hyphen_values = true
    )]
    box_: Option<String>,
}

/// What a recipe's command takes of its steps.
#[derive(Args)]
pub struct Steps {
    /// Run up to and including this step, and print its outputs.
    #[arg(long, value_name = "STEP")]
    until: Option<String>,
    /// Change one step's argument for this run: STEP.ARGUMENT=VALUE, or
    /// STEP.constants=SET. Once for each; the run's record keeps them.
    #[arg(long = "set", value_name = "STEP.ARGUMENT=VALUE")]
    set: Vec<String>,
}

impl Steps {
    /// The recipe's own arguments these give.
    fn arguments(&self) -> Vec<(&'static str, Value)> {
        let mut arguments = Vec::new();
        if let Some(until) = &self.until {
            arguments.push((whizbang_core::recipe::UNTIL, Value::text(until)));
        }
        if !self.set.is_empty() {
            arguments.push((whizbang_core::recipe::SET, Value::Text(self.set.join("\n"))));
        }
        arguments
    }
}

/// One operation's command: its own flags, which its module holds, and what every
/// operation's command takes.
#[derive(Args)]
pub struct Op<T: Args> {
    #[command(flatten)]
    asked: T,
    #[command(flatten)]
    common: Common,
}

impl<T: Args + Asked> Op<T> {
    fn run(&self, recovery: Recovery, platform: Option<Platform>) -> Result<String, String> {
        let asked = &self.asked;
        operation(
            &self.common,
            asked.action(),
            asked.arguments(),
            asked.inputs(),
            recovery,
            platform,
        )
    }
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
    built(at, &over, recovery, false)
}

/// As [`executor`], with `check`'s decision cache on where the config's `[cache]`
/// turns it on (plan 2C, task M1): what `squint check` and `serve` run under. The
/// executor comes back with what it was built over, whose worker `serve` stops.
pub(crate) fn executor_caching(
    at: &Where,
    under: &Under,
    recovery: Recovery,
    platform: Option<Platform>,
    uses: Uses<'_>,
) -> Result<(Engine, wiring::Over), String> {
    let over = under.over(platform, uses)?;
    let cached = over.decisions_cached()?;
    Ok((built(at, &over, recovery, cached)?, over))
}

fn built(
    at: &Where,
    over: &wiring::Over,
    recovery: Recovery,
    cached: bool,
) -> Result<Engine, String> {
    let opening = wiring::opening(at.root(), Make::Nothing, recovery)
        .map_err(|error| wiring::not_opened(&error))?;
    let store = opening.writer().map_err(|why| why.to_string())?;
    let registry = wiring::registry_over(over, cached)?;
    wiring::runner(registry, store, over)
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
        Command::Plug(op) => op.run(recovery, platform),
        Command::Feather(op) => op.run(recovery, platform),
        Command::Render(op) => op.run(recovery, platform),
        Command::Close(op) => op.run(recovery, platform),
        Command::Subtract(op) => op.run(recovery, platform),
        Command::Line(op) => op.run(recovery, platform),
        Command::Rect(op) => op.run(recovery, platform),
        Command::Stroke(op) => op.run(recovery, platform),
        Command::Rasterize(op) => op.run(recovery, platform),
        Command::Shrink(op) => op.run(recovery, platform),
        Command::Invert(op) => op.run(recovery, platform),
        Command::Remove {
            at,
            protect,
            backend,
            steps,
            common,
        } => {
            let filled = Filled {
                action: "remove",
                backend: backend.as_deref(),
                steps,
                common,
            };
            filled.run(at, protect.as_deref(), recovery, platform)
        }
        Command::RepairArea {
            at,
            backend,
            steps,
            common,
        } => {
            let filled = Filled {
                action: "repair_area",
                backend: backend.as_deref(),
                steps,
                common,
            };
            filled.run(at, None, recovery, platform)
        }
        Command::MaskRect {
            image,
            box_,
            steps,
            common,
        } => {
            let mut arguments = vec![("box", Value::text(box_))];
            arguments.extend(steps.arguments());
            let inputs = vec![("image", "--image", image.as_str())];
            let executor = executor(&common.at, &common.under, recovery, platform, Uses::Nothing)?;
            let asked = OperationAsked {
                action: "mask_rect",
                arguments,
                inputs,
                request_id: common.request_id.as_deref(),
            };
            door::recipe(&executor, &asked, common.at.root())
        }
        Command::Protect {
            image,
            mask,
            steps,
            common,
        } => {
            let inputs = vec![
                ("image", "--image", image.as_str()),
                ("mask", "--mask", mask.as_str()),
            ];
            let executor = executor(&common.at, &common.under, recovery, platform, Uses::Nothing)?;
            let asked = OperationAsked {
                action: "protect",
                arguments: steps.arguments(),
                inputs,
                request_id: common.request_id.as_deref(),
            };
            door::recipe(&executor, &asked, common.at.root())
        }
        Command::Cut(op) => op.run(recovery, platform),
        Command::Put(op) => op.run(recovery, platform),
        Command::Window(op) => op.run(recovery, platform),
        Command::Unwindow(op) => op.run(recovery, platform),
        Command::Apply(op) => op.run(recovery, platform),
        Command::RemovePrompt {
            image,
            prompt,
            protect,
            backend,
            steps,
            common,
        } => {
            let mut arguments = vec![("prompt", Value::text(prompt))];
            if let Some(backend) = backend {
                arguments.push(("backend", Value::text(backend)));
            }
            arguments.extend(steps.arguments());
            let mut inputs = vec![("image", "--image", image.as_str())];
            if let Some(protect) = protect {
                inputs.push(("protect", "--protect", protect.as_str()));
            }
            let uses = Uses::One(Capability::Segment, backend.as_deref());
            let over = common.under.over(platform, uses)?;
            let executor = built(&common.at, &over, recovery, false)?;
            let asked = OperationAsked {
                action: "remove_prompt",
                arguments,
                inputs,
                request_id: common.request_id.as_deref(),
            };
            door::recipe(&executor, &asked, common.at.root())
        }
        Command::Run {
            recipe,
            inputs,
            steps,
            common,
        } => {
            let text = wiring::recipe_file(recipe)?;
            let read = whizbang_core::recipe::Recipe::read(&text).map_err(|e| e.to_string())?;
            let mut arguments = Vec::new();
            let mut ports = Vec::new();
            for given in inputs {
                let Some((name, value)) = given.split_once('=') else {
                    return Err("--input is NAME=VALUE".to_owned());
                };
                let Some((declared, input)) = read.inputs.get_key_value(name) else {
                    return Err(format!(
                        "--input names {}, which is no input of the recipe",
                        door::printable(name)
                    ));
                };
                match input.declared {
                    whizbang_core::recipe::Declared::Text => {
                        arguments.push((declared.as_str(), Value::text(value)));
                    }
                    whizbang_core::recipe::Declared::Artifact(_) => {
                        ports.push((declared.as_str(), "--input", value));
                    }
                }
            }
            arguments.extend(steps.arguments());
            let over = common
                .under
                .over(platform, Uses::Every)?
                .recipe(text.clone());
            let executor = built(&common.at, &over, recovery, false)?;
            let asked = OperationAsked {
                action: &read.name,
                arguments,
                inputs: ports,
                request_id: common.request_id.as_deref(),
            };
            door::recipe(&executor, &asked, common.at.root())
        }
        Command::Paste {
            image,
            fill,
            mask,
            mode,
            common,
        } => {
            let arguments = vec![("mode", Value::text(mode))];
            let inputs = vec![
                ("image", "--image", image.as_str()),
                ("fill", "--fill", fill.as_str()),
                ("mask", "--mask", mask.as_str()),
            ];
            operation(common, "paste", arguments, inputs, recovery, platform)
        }
        Command::Fill {
            image,
            mask,
            backend,
            common,
        } => {
            let mut arguments = Vec::new();
            if let Some(backend) = backend {
                arguments.push(("backend", Value::text(backend)));
            }
            let inputs = vec![
                ("image", "--image", image.as_str()),
                ("mask", "--mask", mask.as_str()),
            ];
            let uses = Uses::One(Capability::Fill, backend.as_deref());
            operation_using(common, "fill", arguments, inputs, recovery, platform, uses)
        }
    }
}

/// A recipe that takes a picture and a location and ends in a fill: `remove` and
/// `repair_area`.
struct Filled<'a> {
    action: &'static str,
    backend: Option<&'a str>,
    steps: &'a Steps,
    common: &'a Common,
}

impl Filled<'_> {
    /// Runs the recipe on the picture and location of `at`, and `protect`. With a box,
    /// `rect` makes the location first, as a run of its own, printed before the recipe's.
    /// The request's id, if one was given, is the recipe's.
    fn run(
        &self,
        at: &Located,
        protect: Option<&str>,
        recovery: Recovery,
        platform: Option<Platform>,
    ) -> Result<String, String> {
        let common = self.common;
        let uses = Uses::One(Capability::Fill, self.backend);
        let over = common.under.over(platform, uses)?;
        let executor = built(&common.at, &over, recovery, false)?;
        let (printed, location) = match (&at.location, &at.box_) {
            (Some(location), _) => (String::new(), location.clone()),
            (None, Some(boxed)) => {
                let asked = OperationAsked {
                    action: "rect",
                    arguments: vec![("box", Value::text(boxed))],
                    inputs: vec![("image", "--image", at.image.as_str())],
                    request_id: None,
                };
                let printed = door::operation(&executor, &asked)?;
                let made = printed
                    .lines()
                    .find_map(|line| line.strip_prefix("location "))
                    .ok_or_else(|| "rect made no location".to_owned())?
                    .to_owned();
                (printed, made)
            }
            (None, None) => return Err("give one of --location and --box".to_owned()),
        };
        let mut inputs = vec![
            ("image", "--image", at.image.as_str()),
            ("location", "--location", location.as_str()),
        ];
        if let Some(protect) = protect {
            inputs.push(("protect", "--protect", protect));
        }
        let mut arguments = Vec::new();
        if let Some(backend) = self.backend {
            arguments.push(("backend", Value::text(backend)));
        }
        arguments.extend(self.steps.arguments());
        let asked = OperationAsked {
            action: self.action,
            arguments,
            inputs,
            request_id: common.request_id.as_deref(),
        };
        let ran = door::recipe(&executor, &asked, common.at.root())?;
        Ok(printed + &ran)
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
    operation_using(
        common,
        action,
        arguments,
        inputs,
        recovery,
        platform,
        Uses::Nothing,
    )
}

/// The same, for an operation that uses the backends `uses` names.
fn operation_using(
    common: &Common,
    action: &str,
    arguments: Vec<(&str, Value)>,
    inputs: Vec<(&str, &str, &str)>,
    recovery: Recovery,
    platform: Option<Platform>,
    uses: Uses<'_>,
) -> Result<String, String> {
    let executor = executor(&common.at, &common.under, recovery, platform, uses)?;
    let asked = OperationAsked {
        action,
        arguments,
        inputs,
        request_id: common.request_id.as_deref(),
    };
    door::operation(&executor, &asked)
}
