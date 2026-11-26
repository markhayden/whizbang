//! `whizbang store list|delete|prune|import|record-inputs|export`: wiring from arguments
//! to the core's store (`export` is the module `export`, `record-inputs` the module
//! `record_inputs`).
//!
//! One rule for all six: the store must be there. None of them makes a store at a path
//! with nothing at it or completes one in an empty directory, except `import --create`,
//! which says so when it has (the module `import` says what it makes). The four that
//! write take the writer lock and are refused while another process holds it. Each of
//! the four opens the store through the one entry point (`wiring::opening`), so the
//! store is recovered before the command changes anything; where the recovery fails
//! the command is refused with the reason, and where it could not be published at the
//! limit on a generation file, `delete` and `prune` still work and `import` is refused.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{Args, Subcommand};
use whizbang::wiring::{self, Make};
use whizbang_core::contract::{REQUESTS_PREFIX, Recovery, Remover, SPEND_PREFIX};
use whizbang_core::identity::Projection;
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{HOLDS_PREFIX, Kind, Pruned, Reader, SCOPED_PREFIX};

#[derive(Args)]
pub struct Where {
    /// The store's directory.
    #[arg(long, value_name = "DIR")]
    store: PathBuf,
}

impl Where {
    /// The store's directory.
    pub fn root(&self) -> &Path {
        &self.store
    }

    /// The store at this directory: a test's, and the store `eval baseline --live` names.
    pub fn at(store: &Path) -> Where {
        Where {
            store: store.to_path_buf(),
        }
    }
}

#[derive(Subcommand)]
pub enum Command {
    /// List the runs the store holds, then its saved examples. Reads only: it works
    /// while another process writes.
    List {
        #[command(flatten)]
        at: Where,
    },
    /// Delete runs, and remove what only they referenced. What another run shares is
    /// kept; spend and request-id records are never removed. If the store does not hold
    /// one of the runs, none is deleted. With `--example`, delete a saved example
    /// instead: its run and its pictures stay.
    Delete {
        #[command(flatten)]
        at: Where,
        /// The runs to delete, each by its id, or as `runs list` prints it when that
        /// is no run's id: the same run `runs show` shows for the same text.
        #[arg(required_unless_present = "example", value_name = "RUN")]
        runs: Vec<String>,
        /// A saved example to delete, by the id `store list` prints for it.
        #[arg(long, value_name = "ID", conflicts_with = "runs")]
        example: Option<String>,
    },
    /// Remove what nothing references: older generations and unreferenced content.
    /// With `--unused-imports-older-than`, first let go of every imported picture,
    /// location and upload that no run took and that was imported longer ago than that
    /// many hours, in one publication, with the names that came in with it.
    Prune {
        #[command(flatten)]
        at: Where,
        /// Let go of the imports no run took that are older than this many hours.
        #[arg(long, value_name = "HOURS")]
        unused_imports_older_than: Option<u64>,
    },
    /// Bring a canonical artifact file into a store, under its type and identity (as
    /// `store export ID --to` writes one); with `--picture`, a PNG or JPEG picture as an
    /// image; with `--mask` and `--onto`, a grey PNG or JPEG of an image as a mask on it
    /// (as `store export --mask` writes one). The file is a regular file, or a link to
    /// one, of at most 65 MiB: the largest picture an image artifact holds, and the
    /// record around it. It is opened without waiting, and a FIFO is refused.
    Import {
        /// The canonical file of a mask, a location, an image, a band profile, a series
        /// or a plan: a regular file, or a link to one, of at most 65 MiB. A picture is
        /// not an artifact file: give it with `--picture`.
        #[arg(required_unless_present_any = ["picture", "mask"])]
        file: Option<PathBuf>,
        /// A PNG or JPEG picture to commit as an image whose role is a picture. It is
        /// decoded within the core's limits (64 MiB, 50,000,000 pixels) and written
        /// again as a PNG of its pixels alone, upright and in sRGB: nothing else the file
        /// holds (its text, its EXIF, its colour profile, its name) is kept. It is read
        /// from the file its path names, through no link, and refused unless it is a
        /// regular file.
        #[arg(long, value_name = "FILE", conflicts_with_all = ["file", "mask"])]
        picture: Option<PathBuf>,
        /// What the picture is: flat, or an equirectangular panorama, complete, which
        /// wraps. Without it, a picture whose metadata says it is equirectangular is one,
        /// and any other is flat.
        #[arg(long, value_parser = ["flat", "equirectangular"], requires = "picture")]
        projection: Option<String>,
        /// A grey PNG or JPEG of the image `--onto` names, to commit as a mask on it:
        /// 255 its core, 1 to 254 its blend, 0 nothing. It is read as a picture is, through
        /// no link; decoded within the core's limits, no more pixels than the image has;
        /// must be the image's size exactly; and keeps nothing of the file but its pixels.
        /// Nothing is protected.
        #[arg(long, value_name = "FILE", conflicts_with = "file", requires = "onto")]
        mask: Option<PathBuf>,
        /// The image a mask given with `--mask` is drawn on, by its identity in the store.
        #[arg(long, value_name = "IMAGE", requires = "mask")]
        onto: Option<String>,
        #[command(flatten)]
        at: Where,
        /// Make the store if there is none: at a path with nothing at it, in an empty
        /// directory, or in a directory that holds only what an unfinished creation of
        /// a store left, which is completed. Only the last part of the path is made: a
        /// path whose parent directory is missing is refused. Without this a store
        /// must be there already.
        #[arg(long)]
        create: bool,
    },
    /// Write the inputs of every tool record's worked examples into the store, as
    /// canonical artifacts made by the constructors the records' tests use, each with what
    /// it depends on (a mask with its image, its protect plane, its band profile and its
    /// prescriptions), in one publication; and print one line for each: its record, its
    /// example, its port, its type and its identity. The store must be there.
    RecordInputs {
        #[command(flatten)]
        at: Where,
    },
    /// Write an image in the store as a PNG file of its pixels alone, upright and in
    /// sRGB, whatever form the store holds it in: no text, time, colour profile or EXIF;
    /// a mask, with `--mask`, as a grey PNG a person can edit and bring back with `store
    /// import --mask`; or any artifact, by its identity alone with `--to`, as its
    /// canonical file, which `store import` brings into another store with the same
    /// identity. Reads only: it works while another process writes. It writes only the
    /// file it is given, in a folder that is there: none over another unless
    /// `--overwrite` is given, none through a link, and none inside the store.
    #[command(group(clap::ArgGroup::new("what").required(true).args(["id", "image", "mask"])))]
    Export {
        /// An artifact of any type, by its identity in the store: its canonical file is
        /// written to `--to`.
        #[arg(value_name = "ID", requires = "to", conflicts_with_all = ["image", "mask", "out"])]
        id: Option<String>,
        /// The canonical file to write, in a folder that is there.
        #[arg(long, value_name = "FILE", requires = "id")]
        to: Option<PathBuf>,
        /// The image, by its identity in the store, as `store import` or `rascal render`
        /// prints it.
        #[arg(long, requires = "out", conflicts_with = "mask")]
        image: Option<String>,
        /// A mask, by its identity in the store, written as a grey PNG of its weights: 255
        /// its core, its blend's samples, 0 elsewhere and under protect.
        #[arg(long, requires = "out")]
        mask: Option<String>,
        /// The PNG file to write, in a folder that is there.
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
        /// Replace a file that is there already.
        #[arg(long)]
        overwrite: bool,
        #[command(flatten)]
        at: Where,
    },
}

/// Runs a store command: what to print on success, or the complaint.
pub fn run(command: &Command) -> Result<String, String> {
    run_with(command, wiring::recovery())
}

/// The same, with a store that is opened to write recovered by this.
pub fn run_with(command: &Command, recovery: Recovery) -> Result<String, String> {
    match command {
        Command::List { at } => list(&at.store),
        Command::RecordInputs { at } => whizbang::record_inputs::write(&at.store, recovery),
        Command::Export {
            id,
            to,
            image,
            mask,
            out,
            overwrite,
            at,
        } => match (id, to, image, mask, out) {
            (Some(id), Some(to), None, None, None) => {
                crate::export::export_file(&at.store, id, to, *overwrite)
            }
            (None, None, Some(image), None, Some(out)) => {
                crate::export::export(&at.store, image, out, *overwrite)
            }
            (None, None, None, Some(mask), Some(out)) => {
                crate::export::export_mask(&at.store, mask, out, *overwrite)
            }
            _ => Err(
                "export takes an identity and --to, --image and --out, or --mask and --out"
                    .to_owned(),
            ),
        },
        Command::Delete {
            at,
            example: Some(example),
            ..
        } => delete_example(&at.store, example, recovery),
        Command::Delete { at, runs, .. } => delete(&at.store, runs, recovery),
        Command::Import {
            file,
            picture,
            projection,
            mask,
            onto,
            at,
            create,
        } => match (file, picture, mask, onto) {
            (_, Some(picture), _, _) => {
                let projection = projection.as_deref().map(|p| match p {
                    "equirectangular" => Projection::Equirectangular,
                    _ => Projection::Flat,
                });
                whizbang::import::import_picture_as(
                    picture, &at.store, *create, recovery, projection,
                )
            }
            (_, None, Some(mask), Some(onto)) => {
                whizbang::import::import_mask(mask, onto, &at.store, recovery)
            }
            (Some(file), None, None, _) => {
                whizbang::import::import(file, &at.store, *create, recovery)
            }
            _ => Err("import takes an artifact file, --picture, or --mask and --onto".to_owned()),
        },
        Command::Prune {
            at,
            unused_imports_older_than,
        } => {
            let remover = remover(&at.store, recovery)?;
            let mut text = String::new();
            if let Some(hours) = unused_imports_older_than {
                let older_than_ms = hours.saturating_mul(3_600_000);
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |since| since.as_millis() as u64);
                let collected = remover
                    .collect_unused_imports(older_than_ms, now_ms)
                    .map_err(|e| e.to_string())?;
                text.push_str(&format!(
                    "unused import names let go: {}\n",
                    collected.names
                ));
            }
            let pruned = remover.prune().map_err(|e| e.to_string())?;
            text.push_str(&report(&pruned));
            Ok(text)
        }
    }
}

fn list(root: &Path) -> Result<String, String> {
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let generation = reader.snapshot().map_err(|e| e.to_string())?;
    // The names outside runs, apart by what they record (plan 2C, task F1: the
    // overflow of task E1): the records of request ids (two names for every request
    // that gave one, ADR 0007), those of spend, and any other.
    let under = |prefix: &str| {
        let names = generation.entries().keys();
        names.filter(|name| name.starts_with(prefix)).count()
    };
    let (requests, spend) = (under(REQUESTS_PREFIX), under(SPEND_PREFIX));
    let others = generation.entries().len() - requests - spend;
    let mut text = format!(
        "generation: {}\nnames outside runs: {} (request ids {requests}, spend {spend}, \
         other {others})\nruns: {}\n",
        generation.number(),
        generation.entries().len(),
        generation.runs().len()
    );
    for (id, run) in generation.runs() {
        // A run's holds on the artifacts it took or committed, and what it keeps for the
        // runs it called, reference content its outputs and other runs account for: they
        // are not counted (ADR 0008, "What deleting a run removes").
        let own = || {
            (run.entries().iter())
                .filter(|(name, _)| !name.starts_with(HOLDS_PREFIX))
                .filter(|(name, _)| !name.starts_with(SCOPED_PREFIX))
        };
        let count = |kind: Kind| own().filter(|(_, entry)| entry.kind == kind).count();
        // The sizes are what the store's files say, and a sum of them stops at the
        // largest number there is.
        let bytes = own().fold(0u64, |sum, (_, entry)| sum.saturating_add(entry.bytes));
        // One run, one line: an id is printed with anything that is not printable, and
        // the quote and backslash that the escapes use, escaped.
        text.push_str(&format!(
            "{}\toutputs {}\tdiagnostics {}\tbytes {bytes}\n",
            id.escape_debug(),
            count(Kind::Output),
            count(Kind::Diagnostic)
        ));
    }
    // The saved examples (plan m7, task U1), when the store holds any: each id, its task
    // and its name, escaped.
    let ids = whizbang::play_saved::example_ids(&generation);
    if !ids.is_empty() {
        text.push_str(&format!("examples: {}\n", ids.len()));
    }
    for id in ids {
        let read = (generation.get(&whizbang::play_saved::example_name(&id)))
            .and_then(|entry| reader.read_output(entry).ok())
            .and_then(|bytes| serde_json::from_slice::<whizbang::play_saved::Example>(&bytes).ok());
        match read {
            Some(example) => text.push_str(&format!(
                "example {id}\t{}\t{}\n",
                example.action.escape_debug(),
                example.name.escape_debug()
            )),
            None => text.push_str(&format!("example {id}\tunreadable\n")),
        }
    }
    Ok(text)
}

/// Deletes a saved example: its names, in one publication, then what only they named.
fn delete_example(root: &Path, id: &str, recovery: Recovery) -> Result<String, String> {
    if !whizbang_serve::play::saved::is_example_id(id) {
        return Err(format!(
            "{} is not an example's id: 16 hex digits, as store list prints it",
            id.escape_debug()
        ));
    }
    let store = remover(root, recovery)?;
    let published = store
        .let_go_of_entry(&whizbang::play_saved::example_name(id))
        .map_err(|e| e.to_string())?;
    let mut text = format!(
        "example deleted: {id}\ngeneration: {}\n",
        published.generation
    );
    let pruned = store.prune().map_err(|e| {
        format!("the example is deleted, and the removal of what it left failed: {e}")
    })?;
    text.push_str(&report(&pruned));
    Ok(text)
}

fn delete(root: &Path, runs: &[String], recovery: Recovery) -> Result<String, String> {
    let store = remover(root, recovery)?;
    // Each run is found as `runs show` finds it, by the same function: by its id, or
    // as `runs list` prints it when that is no run's id. One that could mean two runs
    // is refused, and then none is deleted. A run named twice is one run.
    let generation = store.snapshot().map_err(|e| e.to_string())?;
    let mut meant = BTreeSet::new();
    for given in runs {
        meant.insert(crate::runs::meant(&generation, given)?.0);
    }
    let runs: Vec<&str> = meant.into_iter().collect();
    // With the runs each one called, at every depth: a run under it holds what it took.
    let (published, deleted) = store
        .delete_with_descendants(&runs)
        .map_err(|e| e.to_string())?;
    let mut text = format!(
        "runs deleted: {}\ngeneration: {}\n",
        deleted.len(),
        published.generation
    );
    // The deletion is durable from here. What it left without a reference goes now.
    let pruned = store.prune().map_err(|e| {
        format!("the runs are deleted, and the removal of what they left failed: {e}")
    })?;
    text.push_str(&report(&pruned));
    Ok(text)
}

/// Opens the store to write, for deleting and pruning: after its recovery, or, where
/// the recovery could not be published at the limit on a generation file, without it,
/// since these two make the file smaller. A command that removes things makes no
/// store where there is none, and completes none.
fn remover(root: &Path, recovery: Recovery) -> Result<Remover, String> {
    let opening = wiring::opening(root, Make::Nothing, recovery)
        .map_err(|error| wiring::not_opened(&error))?;
    opening.remover().map_err(|why| why.to_string())
}

fn report(pruned: &Pruned) -> String {
    let mut text = format!(
        "generations removed: {}\ncontent files removed: {}\n",
        pruned.generations_removed.len(),
        pruned.objects_removed
    );
    if pruned.buckets_removed > 0 {
        text.push_str(&format!("buckets removed: {}\n", pruned.buckets_removed));
    }
    if !pruned.generations_in_use.is_empty() {
        text.push_str(&format!(
            "generations left for a reader that still holds them: {} (prune again once it has finished)\n",
            pruned.generations_in_use.len()
        ));
    }
    text
}
