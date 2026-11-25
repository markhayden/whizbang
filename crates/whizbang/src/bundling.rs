//! Invocation bundles at the command line (ADR 0048; plan m7, task B1, item 5): `whizbang
//! runs bundle` writes one, and `whizbang bundle import|show|list|delete` takes one in,
//! reads one, and lists and deletes those a store holds. The core's `bundle` module does
//! the work; this is its wiring to files and stores, and what each command prints.
//!
//! A bundle is read from a regular file, or a link to one, by an open that does not wait,
//! so a FIFO is refused and never waited on. Everything printed that came from a bundle or
//! a store is escaped where it is printed.

use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use whizbang_core::bundle::{self, Checked, Made, Manifest, Withheld};
use whizbang_core::contract::Recovery;
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Error, Reader};

use crate::door::printable;
use crate::wiring::{self, Make};

/// Each action this build ships, with its version: what a step must be to be replayable.
/// A recipe file's or a tool file's action is none of them.
pub fn shipped() -> Result<BTreeMap<String, String>, String> {
    let registry = wiring::registry()?;
    let shipped = registry.descriptors().map(|descriptor| {
        (
            descriptor.name().to_owned(),
            descriptor.version().to_owned(),
        )
    });
    Ok(shipped.collect())
}

/// A bundle of these root runs, by their ids, from the store at `root`, read through one
/// reader's snapshot, which keeps what it reads from a prune.
pub fn make(root: &Path, runs: &[&str]) -> Result<Made, String> {
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    bundle::make(&reader, runs, &shipped()?, env!("CARGO_PKG_VERSION"))
}

/// The bundle file at `file`, opened to read: a regular file, or a link to one, opened
/// without waiting and looked at again through its handle.
fn open(file: &Path) -> std::io::Result<File> {
    let not_a_file = || {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "it is not a regular file: a bundle is read from one, or a link to one",
        )
    };
    if !std::fs::metadata(file)?.is_file() {
        return Err(not_a_file());
    }
    let opened = crate::sign_in::open_without_waiting(file)?;
    if !opened.metadata()?.is_file() {
        return Err(not_a_file());
    }
    Ok(opened)
}

/// Reads and checks the bundle at `file`.
pub fn read(file: &Path) -> Result<Checked, String> {
    let opened = open(file).map_err(|why| format!("cannot read {}: {why}", file.display()))?;
    bundle::check(std::io::BufReader::new(opened)).map_err(|why| why.to_string())
}

/// What `bundle show` prints of a checked bundle: its digest, its steps with their links,
/// how many files it carries, and what it withholds, with why.
pub fn shown(checked: &Checked) -> String {
    let manifest = &checked.manifest;
    let mut text = format!(
        "bundle: {}\nwritten by: whizbang {}\nbytes: {}\nmembers: {}\nsteps: {}\n",
        checked.digest,
        printable(&manifest.whizbang),
        checked.bytes,
        checked.members.len(),
        manifest.steps.len()
    );
    text.push_str(&steps_text(manifest));
    text.push_str(&format!("files: {}\n", manifest.artifacts.len()));
    text.push_str(&withheld_text(&manifest.withheld));
    text
}

/// Each step: its run, its action and version, whether it is replayable, how many runs
/// its family has, and each input, an identity or the earlier output it took.
fn steps_text(manifest: &Manifest) -> String {
    let mut text = String::new();
    for (at, step) in manifest.steps.iter().enumerate() {
        let action = step.action.as_deref().unwrap_or("(unknown action)");
        let version = step.version.as_deref().unwrap_or("(unknown version)");
        let replayable = match (&step.replayable, &step.why) {
            (true, _) => "replayable".to_owned(),
            (false, why) => format!(
                "not replayable: {}",
                why.as_deref().unwrap_or("no reason given")
            ),
        };
        text.push_str(&printable(&format!(
            "step {at}: run {} {action} {version}, {replayable}",
            step.run
        )));
        text.push('\n');
        text.push_str(&format!("  runs in its family: {}\n", step.runs.len()));
        for input in &step.inputs {
            let took = match (&input.artifact, &input.from) {
                (_, Some(link)) => format!(
                    "the output {} {} of step {}",
                    link.port, link.place, link.step
                ),
                (Some(identity), None) => identity.to_string(),
                (None, None) => "nothing".to_owned(),
            };
            text.push_str(&printable(&format!(
                "  input {} {}: {took}",
                input.port, input.place
            )));
            text.push('\n');
        }
    }
    text
}

/// What a bundle withholds, one line each.
fn withheld_text(withheld: &[Withheld]) -> String {
    let mut text = format!("withheld: {}\n", withheld.len());
    for item in withheld {
        let what = match item {
            Withheld::OpaquePart { diagnostic, .. } => format!("an opaque part of {diagnostic}"),
            Withheld::Picture {
                diagnostic, pixels, ..
            } => format!("the picture {pixels} {diagnostic} names"),
            Withheld::Diagnostic { name, .. } => format!("the diagnostic {name}"),
            Withheld::FillAnswer { .. } => "the fill's painted answer".to_owned(),
            Withheld::CachedCheck { answered_by, .. } => format!(
                "the turns of the cached check (answered by {})",
                answered_by.as_deref().unwrap_or("a run it does not name")
            ),
            Withheld::Artifact {
                role, port, place, ..
            } => format!("the {} {port} {place}", role.name()),
        };
        text.push_str(&printable(&format!(
            "  {what} of run {}: {}",
            item.run(),
            item.why()
        )));
        text.push('\n');
    }
    text
}

/// `bundle show`: reads and checks the bundle at `file`, reading nothing into a store.
pub fn show(file: &Path) -> Result<String, String> {
    Ok(shown(&read(file)?))
}

/// `bundle import`: checks the bundle at `file`, then brings it into the store at
/// `root` in one publication. The store must be there unless `create` says to make one.
pub fn import(
    file: &Path,
    root: &Path,
    create: bool,
    recovery: Recovery,
) -> Result<String, String> {
    // Checked before the store is touched: a bundle that is refused creates nothing.
    read(file)?;
    let mut printed = String::new();
    let opening = if create {
        let opening = wiring::opening(root, Make::TheRoot, recovery)
            .map_err(|error| wiring::not_opened(&error))?;
        if opening.created() {
            let shown = printable(&root.display().to_string());
            printed.push_str(&format!("created a store at {shown}\n"));
        }
        opening
    } else {
        wiring::opening(root, Make::Nothing, recovery).map_err(|error| match error {
            Error::Missing { .. } | Error::NotAStore { found: None, .. } => {
                format!("{error}; pass --create to have one made there")
            }
            other => wiring::not_opened(&other),
        })?
    };
    let store = opening.writer().map_err(|why| format!("{printed}{why}"))?;
    let reopen = || open(file).map(std::io::BufReader::new);
    let imported = bundle::import(&store, reopen).map_err(|why| format!("{printed}{why}"))?;
    let steps = imported.manifest.steps.len();
    match imported.generation {
        Some(generation) => printed.push_str(&format!(
            "imported bundle {} ({steps} steps) in generation {generation}\n",
            imported.digest
        )),
        None => printed.push_str(&format!(
            "already there: bundle {} ({steps} steps); nothing changed\n",
            imported.digest
        )),
    }
    Ok(printed)
}

/// `bundle list`: each bundle the store holds, one line each, with its steps' actions.
pub fn list(root: &Path) -> Result<String, String> {
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let generation = reader.snapshot().map_err(|e| e.to_string())?;
    let held = bundle::held(&generation);
    let mut text = format!("bundles: {}\n", held.len());
    for digest in held {
        let line = match bundle::manifest_of(&reader, &generation, &digest) {
            Some(Ok(manifest)) => {
                let actions: Vec<&str> = (manifest.steps.iter())
                    .map(|step| step.action.as_deref().unwrap_or("?"))
                    .collect();
                format!(
                    "{digest}\tsteps {}\twithheld {}\t{}\n",
                    manifest.steps.len(),
                    manifest.withheld.len(),
                    printable(&actions.join(" > "))
                )
            }
            Some(Err(why)) => format!(
                "{digest}\tits manifest cannot be read: {}\n",
                printable(&why)
            ),
            None => continue,
        };
        text.push_str(&line);
    }
    Ok(text)
}

/// A bundle's digest as a command is given it: `sha256:` and 64 hex digits, or the 64
/// digits alone.
pub fn digest_of(given: &str) -> Result<Digest, String> {
    given
        .parse::<Digest>()
        .ok()
        .or_else(|| Digest::from_hex(given))
        .ok_or_else(|| {
            format!(
                "{} is no bundle's digest: a digest is sha256: and 64 lower-case hex digits, \
                 as `whizbang bundle list` prints it",
                printable(&given.chars().take(80).collect::<String>())
            )
        })
}

/// `bundle delete`: lets go of the bundle's entry in one publication, then prunes what
/// nothing else names.
pub fn delete(root: &Path, given: &str, recovery: Recovery) -> Result<String, String> {
    let digest = digest_of(given)?;
    let opening = wiring::opening(root, Make::Nothing, recovery)
        .map_err(|error| wiring::not_opened(&error))?;
    let remover = opening.remover().map_err(|why| why.to_string())?;
    let published =
        remover
            .let_go_of_entry(&bundle::entry(&digest))
            .map_err(|error| match error {
                Error::NoSuchEntry { .. } => format!("the store holds no bundle {digest}"),
                other => other.to_string(),
            })?;
    let pruned = remover.prune().map_err(|e| {
        format!("the bundle is deleted, and the removal of what it left failed: {e}")
    })?;
    Ok(format!(
        "deleted bundle {digest}\ngeneration: {}\ncontent files removed: {}\n",
        published.generation, pruned.objects_removed
    ))
}
