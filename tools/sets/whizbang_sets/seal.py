"""`promote`: the labels the owner confirmed published, the items struck taken out, and the
set sealed. For a blind set the labels leave the set folder for owner storage first
(`--labels-out`), and the manifest keeps only their SHA-256.

The seal is `seal.toml`, as `whizbang eval sets check` verifies it
(`crates/whizbang-eval/src/sets.rs`, `Set::components`): a SHA-256 of each component (the
items, the families, the jobs, the pictures, the rules, the labels) and one of them all.
Whizbang's crates read a seal and write none; this is where one is written.

Ported from squint `scripts/sets/src/squint_sets/build.py` (`promote`) and
`crates/squint-eval/src/sets.rs` (`seal`).
"""

from __future__ import annotations

import datetime as dt
import hashlib
import json
import shutil
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from . import build, toml_out
from .build import read_text, write_text

#: The rule files `check` ships, which the seal's rules component is made of.
RULES_DIR = Path(__file__).resolve().parents[3] / "crates" / "whizbang-squint" / "rules"


@dataclass
class PromoteReport:
    confirmed: int = 0
    struck: int = 0


def _hex(text: str | bytes) -> str:
    return hashlib.sha256(text if isinstance(text, bytes) else text.encode("utf-8")).hexdigest()


def _job_json(job: dict[str, Any]) -> str:
    """The job as the Rust reader serialises it, field by field in its order with its
    defaults, compact: what the seal's jobs component is made of."""

    def requirement(r: dict[str, Any]) -> dict[str, Any]:
        return {
            "id": r["id"],
            "kind": r["kind"],
            "text": r.get("text", ""),
            "matters": r.get("matters"),
            "hidden_ok": r.get("hidden_ok", False),
            "scope": r.get("scope"),
            "evidence": r.get("evidence"),
            "evidence_kinds": r.get("evidence_kinds"),
            "waives": r.get("waives", []),
            "condition": r.get("condition"),
            "disabled": r.get("disabled", False),
        }

    def image(i: dict[str, Any]) -> dict[str, Any]:
        source = {"path": i["source"]["path"]}
        if "sha256" in i["source"]:
            source["sha256"] = i["source"]["sha256"]
        return {
            "id": i["id"],
            "role": i["role"],
            "label": i.get("label"),
            "source": source,
            "projection": i.get("projection"),
        }

    shaped = {
        "use_case": job["use_case"],
        "brief": job.get("brief"),
        "requirements": [requirement(r) for r in job.get("requirements", [])],
        "images": [image(i) for i in job.get("images", [])],
    }
    return json.dumps(shaped, ensure_ascii=False, separators=(",", ":"))


def components(set_dir: Path, rules_dir: Path = RULES_DIR) -> dict[str, str]:
    manifest = tomllib.loads(read_text(set_dir / "set.toml"))
    meta, items = manifest["set"], manifest.get("item", [])
    jobs = {i["id"]: tomllib.loads(read_text(set_dir / i["job"]))["job"] for i in items}
    pictures = sorted(
        {f"{img['source']['path']} {img['source']['sha256'].lower()}" for job in jobs.values() for img in job.get("images", [])}
    )
    # The rule file's bytes, as the binary includes them: no locale and no newline is applied.
    rules = (rules_dir / f"{meta['rules']['base']}.toml").read_bytes()
    if "labels" in meta:
        labels = hashlib.sha256((set_dir / meta["labels"]).read_bytes()).hexdigest()
    else:
        labels = meta["labels_sha256"].lower()
    return {
        "items": _hex("\n".join(sorted(jobs))),
        "families": _hex("\n".join(sorted(f"{i['id']}={i['family']}" for i in items))),
        "jobs": _hex("".join(f"{item}\n{_job_json(jobs[item])}\n" for item in sorted(jobs))),
        "images": _hex("\n".join(pictures)),
        "rules": _hex(f"{meta['rules']['base']}@{meta['rules']['version']}\n".encode("utf-8") + rules),
        "labels": labels,
    }


def overall(c: dict[str, str]) -> str:
    names = ("items", "families", "jobs", "images", "rules", "labels")
    return _hex("".join(f"{n}:{c[n]}\n" for n in names))


def seal(set_dir: Path, attestation: str, rules_dir: Path = RULES_DIR) -> str:
    """Writes `seal.toml` for the set as it stands; returns its SHA-256."""
    c = components(set_dir, rules_dir)
    created = dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    body = {"sha256": overall(c), "created": created, "attestation": attestation}
    write_text(
        set_dir / "seal.toml",
        toml_out.document("", [("seal", body, False), ("seal.components", c, False)])
    )
    return body["sha256"]


def promote(
    draft_path: Path,
    set_dir: Path,
    attestation: str,
    labels_out: Path | None = None,
    rules_dir: Path = RULES_DIR,
) -> PromoteReport:
    """Publishes the confirmed labels as `labels.toml`, takes the struck items out of the set,
    and seals it. A label still a draft is refused before anything changes. A blind set's
    labels go to `labels_out`, outside every repository, and its manifest keeps their hash."""
    set_dir = Path(set_dir)
    manifest = tomllib.loads(read_text(set_dir / "set.toml"))
    blind = manifest["set"]["split"] == "blind"
    if blind and labels_out is None:
        raise ValueError("a blind set's labels leave the set folder: name owner storage with --labels-out")
    if not blind and labels_out is not None:
        raise ValueError("--labels-out is for a blind set only")
    if labels_out is not None:
        labels_out = build.outside_every_repository(labels_out, "--labels-out")
        if (labels_out / "labels.toml").exists():
            raise ValueError("--labels-out already holds a labels.toml; nothing is overwritten")
    if blind:
        # A blind set's draft labels are owner material, read only from owner storage.
        draft_path = build.outside_every_repository(draft_path, "a blind set's --draft")
    draft = tomllib.loads(read_text(draft_path))
    report = PromoteReport()
    confirmed: list[dict[str, Any]] = []
    struck: set[str] = set()
    for n, label in enumerate(draft.get("label", []), start=1):
        status = label.pop("status", "draft")
        if status == "confirmed":
            if not label.get("adjudicated_on"):
                label["adjudicated_on"] = build.today()
            confirmed.append(label)
            report.confirmed += 1
        elif status == "struck":
            struck.add(label["item"])
            report.struck += 1
        else:
            raise ValueError(f"label {n} is still a draft; confirm or strike every label first")
    for i in manifest.get("item", []):
        if i["id"] in struck:
            shutil.rmtree(set_dir / "items" / i["id"], ignore_errors=True)
    kept = [i for i in manifest.get("item", []) if i["id"] not in struck]
    labels_text = toml_out.document(
        "Answer labels adjudicated by the owner from the construction record.",
        build.label_tables(confirmed),
    )
    meta = dict(manifest["set"])
    if labels_out is None:
        write_text(set_dir / "labels.toml", labels_text)
    else:
        labels_out.mkdir(parents=True, exist_ok=True)
        write_text(labels_out / "labels.toml", labels_text)
        meta.pop("labels", None)
        fields = list(meta.items())
        at = [k for k, _ in fields].index("families")
        fields.insert(at, ("labels_sha256", hashlib.sha256(labels_text.encode("utf-8")).hexdigest()))
        meta = dict(fields)
    write_text(
        set_dir / "set.toml",
        build.manifest_text(meta, kept, "Built by whizbang-sets; labels confirmed by the owner.")
    )
    seal(set_dir, attestation, rules_dir)
    return report


def pin(set_dir: Path, rev: str) -> None:
    """Names the revision of the pictures' repository the set's pictures were committed at
    (`images_rev`), which the seal does not cover; nothing else of the set changes. A set
    pinned at another revision already is refused: a pin is never changed unsaid."""
    set_dir = Path(set_dir)
    path = set_dir / "set.toml"
    before_text = read_text(path)
    manifest = tomllib.loads(before_text)
    held = manifest["set"].get("images_rev")
    if held == rev:
        return
    if held is not None:
        raise ValueError("the set is pinned at another revision already; it is not changed")
    fields = list(manifest["set"].items())
    at = [k for k, _ in fields].index("images_repo") + 1
    fields.insert(at, ("images_rev", rev))
    text = build.manifest_text(dict(fields), manifest.get("item", []), "Built by whizbang-sets; labels confirmed by the owner.")
    before = components(set_dir)
    write_text(path, text)
    if components(set_dir) != before:
        write_text(path, before_text)
        raise ValueError("pinning the revision would change what the seal covers; nothing was changed")
