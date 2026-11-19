//! What `whizbang lab` does (plan m3, task L1; ADR 0038): the lab's files read and checked
//! (`whizbang_eval::lab`), and a lab's cells run on the shared harness ([`crate::harness`]),
//! each a run of its recipe through the executor, recognised by its request id, whose
//! scope is the lab's name and protocol; a reference arm's cell runs nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use whizbang_core::artifact::ArtifactId;
use whizbang_core::contract::{Call, Registry, Request, Value};
use whizbang_core::recipe::{RecipeAction, SET};
use whizbang_eval::lab::cell::{Cell, Work};
use whizbang_eval::lab::corpus::Corpus;
use whizbang_eval::lab::protocol;

use crate::harness;

/// The most bytes of a lab's file (a manifest, a protocol, a matrix) that are read.
pub const FILE_LIMIT: u64 = 1024 * 1024;

/// A recipe this build ships, by name, as an action over `registry`.
pub fn recipe(registry: &Registry, name: &str) -> Result<RecipeAction, String> {
    for text in whizbang_rascal::recipes::SHIPPED {
        let read = whizbang_core::recipe::Recipe::read(text).map_err(|e| e.to_string())?;
        if read.name == name {
            return whizbang_rascal::recipes::action_of(text, registry).map_err(|e| e.to_string());
        }
    }
    Err(format!(
        "{} is no recipe this build ships",
        name.escape_debug()
    ))
}

/// The text of a constant set this build ships.
pub fn constants(name: &str) -> Option<String> {
    whizbang_rascal::constants::text_of(name).map(str::to_owned)
}

/// One cell of a lab as the harness runs it: its work, the lab's scope, and the identity
/// in the store of each corpus picture it binds.
pub struct LabCell<'a> {
    pub cell: &'a Cell,
    pub scope: &'a str,
    pub pictures: &'a BTreeMap<String, ArtifactId>,
}

impl LabCell<'_> {
    fn picture(&self, id: &str) -> Result<ArtifactId, String> {
        (self.pictures.get(id).copied())
            .ok_or_else(|| format!("the picture {id} is not in the store; nothing was run"))
    }

    /// What a reference arm's cell hands back: the picture it binds. None for a recipe's.
    pub fn handed_back(&self) -> Result<Option<ArtifactId>, String> {
        match &self.cell.work {
            Work::Reference { picture } => self.picture(picture).map(Some),
            Work::Recipe { .. } => Ok(None),
        }
    }
}

impl harness::Work for LabCell<'_> {
    fn identity(&self) -> String {
        self.cell.identity.clone()
    }

    fn request(&self, attempt: u32) -> Result<Option<Request>, String> {
        let Work::Recipe {
            recipe,
            inputs,
            texts,
            set,
        } = &self.cell.work
        else {
            return Ok(None);
        };
        let mut call = Call::new(recipe).input("image", self.picture(&self.cell.image)?);
        for (port, id) in inputs {
            call = call.input(port, self.picture(id)?);
        }
        for (name, text) in texts {
            call = call.argument(name, Value::text(text));
        }
        if !set.is_empty() {
            call = call.argument(SET, Value::text(set));
        }
        let id = harness::request_id(self.scope, &self.cell.identity, attempt)?;
        Ok(Some(Request::new(call).with_id(id)))
    }

    fn forecast(&self) -> Result<u64, String> {
        Ok(self.cell.forecast_micros)
    }

    fn group(&self) -> String {
        self.cell.arm.clone()
    }
}

/// Every scope of a cell whose run the store at `root` holds.
pub fn scopes(root: &Path) -> Result<BTreeSet<String>, String> {
    let mut found = BTreeSet::new();
    harness::each_run(root, &mut |record, _, _| {
        if let Some((scope, _, _)) = harness::named_by_request(record) {
            found.insert(scope);
        }
    })?;
    Ok(found)
}

/// Refused when the store at `root` holds runs of this lab under another protocol.
pub fn frozen(root: &Path, name: &str, protocol_digest: &str) -> Result<(), String> {
    let seen = scopes(root)?;
    protocol::frozen(seen.iter().map(String::as_str), name, protocol_digest)
}

/// `whizbang lab corpus verify`: the manifest's pictures under `root`, each checked
/// against its digest. The report, and whether every picture was there as pinned.
pub fn verify(manifest: &Path, root: &Path) -> Result<(String, bool), String> {
    let folder = (manifest.parent())
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = (manifest.file_name().and_then(|n| n.to_str()))
        .ok_or_else(|| format!("{} names no file", manifest.display()))?;
    let bytes = whizbang_eval::sets::read_under(folder, name, FILE_LIMIT)?;
    let text = String::from_utf8(bytes).map_err(|_| format!("{name} is not UTF-8"))?;
    let corpus = Corpus::parse(&text)?;
    let found = corpus.verify(root, crate::import::MAX_FILE_BYTES)?;
    Ok(whizbang_eval::lab::corpus::report(&found))
}
