//! `whizbang eval admitted --receipts <dir>` (plan mE, task M1, item 8; the carry-over row on
//! which tools a gate admitted): what the receipts of the gates run admitted, read from the
//! receipts alone. A receipt admits what its gate file's `[admits]` names when its outcome
//! is admitted, its gate is of rung 4, and every set it read is of split `blind` or
//! `heldout` and not exposed ([`whizbang_eval::gate::EXPOSED`]: `blind-v3`, whose split is
//! `blind`, never admits; `SPEC.md` section 8, "Evaluation data"). A receipt of a rung 3
//! gate, of a gate on a dev or exposed set, of a gate that names nothing it admits, or of
//! version 1, admits nothing. An edit gate's receipt (`edit-receipt/1`, plan mE, task M2)
//! admits its recipe when it is admitted on rung 4 with task success read from marks, on a
//! corpus that is not exposed.

use std::path::{Path, PathBuf};

use serde::Serialize;
use whizbang_core::identity::Digest;

use crate::door::printable;

/// The splits whose sets admit.
pub const ADMITTING_SPLITS: [&str; 2] = ["blind", "heldout"];

/// The most receipt files read, and how deep below the folder.
pub const FILES_LIMIT: usize = 4_096;
pub const DEPTH_LIMIT: usize = 4;

/// One thing a receipt admitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Admission {
    /// `tool`, `use_case`, `format`, `wording`, `judge` or, of an edit gate, `recipe`.
    pub kind: String,
    pub name: String,
    /// The SHA-256 of the receipt's bytes.
    pub receipt_sha256: String,
    pub gate_sha256: String,
    /// The day the last cell its gate read ended.
    pub date: String,
}

/// What one receipt admits: its digest and its JSON.
pub fn admits(digest: &str, receipt: &serde_json::Value) -> Option<Admission> {
    let text = |key: &str| receipt.get(key).and_then(serde_json::Value::as_str);
    if text("receipt") == Some(whizbang_eval::edit_gate::RECEIPT) {
        return admits_an_edit(digest, receipt);
    }
    if receipt
        .get("receipt_version")
        .and_then(serde_json::Value::as_u64)
        != Some(2)
        || text("outcome") != Some("admitted")
    {
        return None;
    }
    let admits = receipt.get("admits")?;
    if admits.get("rung").and_then(serde_json::Value::as_u64) != Some(4) {
        return None;
    }
    let splits = receipt.get("splits")?.as_object()?;
    let exposed = |id: &str| whizbang_eval::gate::EXPOSED.contains(&id);
    let admitting = !splits.is_empty()
        && (splits.iter()).all(|(id, s)| {
            !exposed(id) && s.as_str().is_some_and(|s| ADMITTING_SPLITS.contains(&s))
        });
    if !admitting {
        return None;
    }
    Some(Admission {
        kind: admits.get("kind")?.as_str()?.to_owned(),
        name: admits.get("name")?.as_str()?.to_owned(),
        receipt_sha256: digest.to_owned(),
        gate_sha256: text("gate_sha256")?.to_owned(),
        date: text("date")?.to_owned(),
    })
}

/// What an edit gate's receipt admits (plan mE, task M2): its recipe, when its outcome is
/// admitted, its gate and what it admits are of rung 4, and its task success was read from
/// a person's marks (`SPEC.md` section 8, "Scoring edits": a rung 4 result rests on labels
/// or blinded review that squint did not produce).
fn admits_an_edit(digest: &str, receipt: &serde_json::Value) -> Option<Admission> {
    let text = |key: &str| receipt.get(key).and_then(serde_json::Value::as_str);
    let rung = |value: Option<&serde_json::Value>| value.and_then(serde_json::Value::as_u64);
    let admits = receipt.get("admits")?;
    let marked = receipt.get("success")?.get("from")?.as_str() == Some("marks");
    let fourth = rung(receipt.get("rung")) == Some(4) && rung(admits.get("rung")) == Some(4);
    let exposed =
        text("corpus").is_none_or(|c| whizbang_eval::edit_gate::EXPOSED_CORPORA.contains(&c));
    if text("outcome") != Some("admitted") || !marked || !fourth || exposed {
        return None;
    }
    Some(Admission {
        kind: admits.get("kind")?.as_str()?.to_owned(),
        name: admits.get("name")?.as_str()?.to_owned(),
        receipt_sha256: digest.to_owned(),
        gate_sha256: text("gate_sha256")?.to_owned(),
        date: text("date")?.to_owned(),
    })
}

/// Every `*.json` file under `dir`, at most [`DEPTH_LIMIT`] folders down, in order.
fn files_under(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let mut folders = vec![(dir.to_path_buf(), 0usize)];
    while let Some((folder, depth)) = folders.pop() {
        let entries = std::fs::read_dir(&folder)
            .map_err(|e| format!("{}: {e}", printable(&folder.display().to_string())))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() && depth < DEPTH_LIMIT {
                folders.push((path, depth + 1));
            } else if kind.is_file() && path.extension().is_some_and(|e| e == "json") {
                out.push(path);
            }
            if out.len() > FILES_LIMIT {
                return Err(format!(
                    "more than {FILES_LIMIT} files are under the folder"
                ));
            }
        }
    }
    out.sort();
    Ok(out)
}

/// What every receipt under `dir` admitted, and how many receipts were read. A JSON file
/// that is no gate's receipt (it names no `gate_sha256`) is passed over.
pub fn read(dir: &Path) -> Result<(Vec<Admission>, usize), String> {
    let mut out = Vec::new();
    let mut receipts = 0;
    for file in files_under(dir)? {
        let bytes = crate::gating::read_limited(&file, "a receipt")?;
        let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            continue;
        };
        if json.get("gate_sha256").is_none() {
            continue;
        }
        receipts += 1;
        out.extend(admits(&Digest::of_bytes(&bytes).hex(), &json));
    }
    out.sort_by(|a, b| (&a.kind, &a.name, &a.date).cmp(&(&b.kind, &b.name, &b.date)));
    Ok((out, receipts))
}

/// What the command prints.
pub fn text(admitted: &[Admission], receipts: usize) -> String {
    let mut out = format!("receipts read: {receipts}\nadmitted: {}\n", admitted.len());
    for one in admitted {
        out.push_str(&format!(
            "{} {}: receipt {} of gate {}, {}\n",
            printable(&one.kind),
            printable(&one.name),
            &one.receipt_sha256[..16.min(one.receipt_sha256.len())],
            &one.gate_sha256[..16.min(one.gate_sha256.len())],
            printable(&one.date)
        ));
    }
    out
}
