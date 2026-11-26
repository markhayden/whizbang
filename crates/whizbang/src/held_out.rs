//! Refusals that tell nothing of a held-out set (plan mE, task M1, item 7). Reading a set and
//! fetching its pictures name full paths, a TOML parser's text and an item's id when
//! something is wrong; a held-out set's construction can be read from any of them. Under
//! `--held-out`, every refusal of `eval sets check`, `eval sets fetch`, `eval matrix`, `eval
//! run --gate` and `eval judges` names only the set's id, an item's place in the set (from 1,
//! in the manifest's order; never its id) and the kind of problem: no path, no parser's
//! text, no job's text.

use std::path::Path;

use whizbang_core::contract::Recovery;
use whizbang_core::identity::Digest;
use whizbang_eval::sets::{self, Set};

use crate::door::printable;
use crate::import;

/// What a refusal says when the kind of problem is not one named here.
pub const WITHHELD: &str = "refused; what is wrong is withheld, the set being held out";

/// The manifest's items in order, each id with its job's path, read without reading the set;
/// `None` when the manifest cannot be read as TOML.
fn manifest_items(dir: &Path) -> Option<Vec<(String, String)>> {
    let bytes = sets::read_under(dir, "set.toml", sets::FILE_LIMIT).ok()?;
    let value: toml::Value = toml::from_str(std::str::from_utf8(&bytes).ok()?).ok()?;
    let items = match value.get("item") {
        None => Vec::new(),
        Some(list) => list.as_array()?.clone(),
    };
    (items.iter())
        .map(|item| {
            let id = item.get("id")?.as_str()?.to_owned();
            let job = item.get("job")?.as_str()?.to_owned();
            Some((id, job))
        })
        .collect()
}

/// What is wrong with the set `id` under `root`, said by its id, an item's place and the kind
/// of problem; `None` when the set reads.
pub fn refusal(root: &Path, id: &str) -> Option<String> {
    let shown = printable(id);
    let dir = root.join(id);
    let Some(items) = manifest_items(&dir) else {
        return Some(format!("{shown}: its manifest cannot be read"));
    };
    let judging = crate::evaluation::judging();
    for (place, (_, job)) in items.iter().enumerate() {
        let n = place + 1;
        let Ok(bytes) = sets::read_under(&dir, job, sets::FILE_LIMIT) else {
            return Some(format!("{shown}: item {n}: its job cannot be read"));
        };
        let Ok(text) = String::from_utf8(bytes) else {
            return Some(format!("{shown}: item {n}: its job cannot be read"));
        };
        if (judging.job)(&text).is_err() {
            return Some(format!(
                "{shown}: item {n}: its job cannot be read as one check takes"
            ));
        }
    }
    match sets::find(root, id) {
        Ok(_) => None,
        Err(_) => Some(format!("{shown}: {WITHHELD}")),
    }
}

/// Every set id under `root`, `<use case>/<set>`, as its folders lay them out.
pub fn ids_under(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(cases) = std::fs::read_dir(root) else {
        return out;
    };
    for case in cases.flatten() {
        let Ok(names) = std::fs::read_dir(case.path()) else {
            continue;
        };
        for name in names.flatten() {
            if name.path().join("set.toml").is_file() {
                let (case, name) = (case.file_name(), name.file_name());
                out.push(format!(
                    "{}/{}",
                    case.to_string_lossy(),
                    name.to_string_lossy()
                ));
            }
        }
    }
    out.sort();
    out
}

/// A refusal of a command over the sets `ids` under `root`, as a held-out set's may be
/// shown: what is wrong with the first set that does not read; else the refusal itself only
/// when it is of a form known to tell nothing ([`safe`]); else that it was refused, and the
/// sets. What is passed is allowed, not what is caught: a path of any folder (the sets', the
/// labels', the store's, the pictures'), a parser's text and a job's are all withheld alike.
pub fn shown(root: &Path, ids: &[String], refused: &str) -> String {
    for id in ids {
        if let Some(said) = refusal(root, id) {
            return said;
        }
    }
    let mut names: Vec<String> = Vec::new();
    for id in ids {
        if let Ok(set) = sets::find(root, id) {
            names.extend(set.families_by_item().into_iter().flat_map(|(i, f)| [i, f]));
        }
    }
    if safe(refused, ids, &names) {
        return refused.to_owned();
    }
    let shown: Vec<String> = ids.iter().map(|id| printable(id)).collect();
    format!("{}: {WITHHELD}", shown.join(", "))
}

/// The refusals that pass under `--held-out`: those about spend and counts (the allowance,
/// the forecast, cells not ended or left to run), on one line, that hold no item or family
/// id of the sets, and none of `/`, `\`, a quote, a backquote or a bar once the sets' own
/// ids are taken out. A path, a TOML parser's text and a job's text each hold one of them.
pub fn safe(refused: &str, ids: &[String], names: &[String]) -> bool {
    const ABOUT: [&str; 4] = ["allowance", "forecast", "have not ended", "left to run"];
    let mut rest = refused.trim_end().to_owned();
    for id in ids {
        rest = rest.replace(id.as_str(), "");
    }
    ABOUT.iter().any(|about| rest.contains(about))
        && !rest.contains(['/', '\\', '"', '`', '|', '\n'])
        && !(names.iter()).any(|name| !name.is_empty() && rest.contains(name.as_str()))
}

/// `eval sets fetch --from <dir> --held-out`: the pictures of a held-out set from a folder,
/// each refusal naming the set's id, the item's place and the kind of problem alone.
pub fn from_folder(
    set: &Set,
    dir: &Path,
    root: &Path,
    recovery: &Recovery,
) -> Result<String, String> {
    let id = printable(set.id());
    let order: Vec<String> = (set.manifest.item.iter())
        .map(|item| item.id.clone())
        .collect();
    let place = |item: &str| order.iter().position(|i| i == item).map_or(0, |at| at + 1);
    let (wanted, present) =
        crate::evaluation::wanted(set, root).map_err(|_| format!("{id}: {WITHHELD}"))?;
    for picture in &wanted {
        let n = place(&picture.item);
        let relative = format!("{}/{}", set.id(), picture.path);
        let Ok(bytes) = sets::read_under(dir, &relative, crate::evaluation::PICTURE_LIMIT) else {
            return Err(format!("{id}: item {n}: a picture is missing"));
        };
        if Digest::of_bytes(&bytes).hex() != picture.sha256 {
            return Err(format!(
                "{id}: item {n}: a picture is not the one its job pins"
            ));
        }
        let named = format!("{id}: item {n}");
        import::picture_of_bytes(
            &bytes,
            &named,
            Some(&picture.sha256),
            root,
            recovery.clone(),
        )
        .map_err(|_| format!("{id}: item {n}: a picture cannot be imported"))?;
    }
    Ok(format!(
        "{id}: imported {} pictures from the folder, {present} already there\n",
        wanted.len()
    ))
}
