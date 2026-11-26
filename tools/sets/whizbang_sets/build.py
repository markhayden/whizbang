"""Build a set from a plan, into two halves: the public set folder (manifest, families, one
pinned job per item, the pictures) and, in owner storage, the owner's worksheet (the
construction record, the draft labels, a contact sheet per family). `rebuild` makes one item
again; `promote` (`seal.py`) publishes the labels the owner confirmed.

Scene families generate one source and make every item from it by an op. Subject families
generate the references of one subject; each item is a new scene composed from them (a pass,
to be confirmed), an edit of an earlier candidate that changes a marking, or a lookalike made
without the references. Brief families generate each candidate from the brief, or from a
brief altered to break one of its rules.

Ported from squint `scripts/sets/src/squint_sets/build.py`.
"""

from __future__ import annotations

import datetime as dt
import hashlib
import io
import shutil
import tomllib
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from PIL import Image, ImageDraw

from . import ops, regions, toml_out
from .ops import DraftLabel
from .plan import Family, Item, Plan

SOURCE_FILE = "source.png"
CANDIDATE_FILE = "candidate.png"
MANIFEST_COMMENT = "Built by whizbang-sets; labels arrive with `promote` after the owner's review."
DRAFT_COMMENT = (
    'Draft labels from the construction record. Set each label\'s status to "confirmed" or '
    '"struck"\nand adjudicated_on to the date, then run `promote`.'
)


@dataclass
class BuildReport:
    items: int = 0
    images: int = 0


@dataclass
class _Built:
    """One item as built: its candidate, its job's other pictures, its draft."""

    id: str
    candidate: bytes
    label: DraftLabel
    brief: str
    record: dict[str, Any]
    #: (image id, role, label, path relative to the set, sha256)
    others: list[tuple[str, str, str | None, str, str]] = field(default_factory=list)
    requirements: list[dict[str, Any]] = field(default_factory=list)


Cells = list[tuple[str, bytes, DraftLabel | None]]


def outside_every_repository(path: Path, what: str) -> Path:
    """`path`, made absolute, when no folder from it up holds a `.git`: owner material is
    never written into, or read from, a work tree."""
    path = Path(path).resolve()
    for at in (path, *path.parents):
        if (at / ".git").exists():
            raise ValueError(
                f"{what} is inside a repository; owner material stays outside every repository"
            )
    return path


def read_text(path) -> str:
    """A file the builder reads, as UTF-8 whatever the locale."""
    return Path(path).read_text(encoding="utf-8")


def write_text(path, text: str) -> None:
    """A file the builder writes, as UTF-8 with its newlines as they are, whatever the locale."""
    Path(path).write_text(text, encoding="utf-8", newline="")


def sha256(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def today() -> str:
    return dt.date.today().isoformat()


def _cap(text: str) -> str:
    return text[0].upper() + text[1:] if text else text


def _job_text(set_id, use_case, brief, requirements, images) -> str:
    tables: list[tuple[str, dict[str, Any], bool]] = [
        ("job", {"use_case": use_case, "brief": brief}, False)
    ]
    tables += [("job.requirements", rq, True) for rq in requirements]
    tables += [("job.images", im, True) for im in images]
    return toml_out.document(f"Item of {set_id}; built by construction, see the owner's record.", tables)


def _contact_sheet(cells: Cells, path: Path) -> None:
    thumb = 256
    sheet = Image.new("RGB", (thumb * len(cells) + 8 * (len(cells) + 1), thumb + 70), (245, 245, 245))
    d = ImageDraw.Draw(sheet)
    for n, (name, b, lab) in enumerate(cells):
        im = Image.open(io.BytesIO(b)).convert("RGB")
        im.thumbnail((thumb, thumb))
        x = 8 + n * (thumb + 8)
        sheet.paste(im, (x, 8))
        caption = name if lab is None else f"{name}\n{lab.expected}: {lab.words[:48]}"
        d.multiline_text((x, thumb + 14), caption, fill=(20, 20, 20))
    sheet.save(path, format="PNG")


def _match_requirements(label: DraftLabel, target: str | None, requirements) -> None:
    if not target:
        return
    for rq in requirements:
        if target.lower() in rq["text"].lower():
            label.obligation_ids.append(rq["id"])


def _keeps(subject: str, traits: dict[str, str], wording: str) -> str:
    """A protected subject's requirement, in the form every tool that reads subjects parses:
    the subject's phrase, then " keeps ", then what it keeps (`pack/tools/per_subject.py`).
    In full wording each trait it states is a clause of its own, checkable alone."""
    kept = "its identity, shape, position and extent relative to the source"
    stated = [f"its {k} ({traits[k]})" for k in ("colour", "count", "lettering") if k in traits]
    if wording == "full" and stated:
        listed = stated[0] if len(stated) == 1 else ", ".join(stated[:-1]) + " and " + stated[-1]
        return f"{_cap(subject)} keeps {kept}, and {listed}."
    return f"{_cap(subject)} keeps {kept}."


def _scene_requirements(family: Family, wording: str = "full") -> list[dict[str, Any]]:
    traits = family.traits or [{} for _ in family.protected]
    return [
        {
            "id": f"RQ{n}",
            "kind": "must_hold",
            "scope": ["candidate"],
            "evidence": ["source"],
            "text": _keeps(s, t, wording),
        }
        for n, (s, t) in enumerate(zip(family.protected, traits), start=1)
    ]


def _brief_requirements(family: Family) -> list[dict[str, Any]]:
    reqs = [
        {"id": f"RQ{n}", "kind": "must_hold", "scope": ["candidate"], "text": f"The candidate shows {ask}."}
        for n, ask in enumerate(family.asks, start=1)
    ]
    for n, forbid in enumerate(family.forbids, start=len(reqs) + 1):
        reqs.append(
            {
                "id": f"RQ{n}",
                "kind": "must_not",
                "scope": ["candidate"],
                "text": f"The candidate shows {forbid}; none was asked for.",
            }
        )
    return reqs


def _subject_requirements(family: Family, ref_ids: list[str], use_case: str) -> list[dict[str, Any]]:
    text = (
        "{s} matches the reference exactly."
        if use_case == "product-fidelity"
        else "{s} is present and unchanged in the candidate, as in the references."
    )
    return [
        {
            "id": f"RQ{n}",
            "kind": "must_hold",
            "scope": ["candidate"],
            "evidence": list(ref_ids),
            "text": text.format(s=_cap(s)),
        }
        for n, s in enumerate(family.protected, start=1)
    ]


def _subject_rules(use_case: str, aspect: str | None) -> tuple[list[str], list[str]]:
    """(the rules a marking edit breaks, the rules a lookalike breaks) under the use case."""
    if use_case == "product-fidelity":
        edit = {"label": "PF2", "colour": "PF3", "shape": "PF1"}.get(aspect or "label", "PF2")
        return [edit], ["PF1", "PF2"]
    return ["RC2"], ["RC3", "RC1"]


def _generator_of(family: Family, item: Item, gen: Any, generators: dict[str, Any]) -> Any:
    """The generator of the item's candidate: its own when the plan names one, else the family's."""
    if item.generator is None or item.generator == family.generator:
        return gen
    g = generators.get(item.generator)
    if g is None:
        raise ValueError(f"item {item.id}: no generator {item.generator!r} available")
    return g


def _scene_item(
    family, item, n, source, gen, generators, use_case, seed_offset=0, wording="full", earlier=None
) -> _Built:
    # Only source fidelity judges against the source; a reality-check job is the candidate alone.
    with_source = use_case == "source-fidelity"
    requirements = _scene_requirements(family, wording) if with_source else []
    item_gen = _generator_of(family, item, gen, generators)
    given = None if item.same_as is None else (earlier or {})[item.same_as]
    candidate, label = ops.apply(item.op, source, item_gen, family.source.seed + n + 1 + seed_offset, given)
    if label.expected == "fail" and label.obligation_ids == ["SF1"]:
        target = item.op.get("removes") or item.op.get("alters") or item.op.get("covers")
        _match_requirements(label, target, requirements)
    rec = {"id": item.id, "family": family.id, "op": dict(item.op), "generated_on": today()}
    if item.generator is not None:
        rec["generator"] = item.generator
    if item.same_as is not None:
        rec["same_as"] = item.same_as
    if item.brief is not None:
        rec["brief"] = item.brief
    if label.notes:
        rec["edit_notes"] = label.notes
    if seed_offset:
        rec["seed_offset"] = seed_offset
    source_rel = f"families/{family.id}/{SOURCE_FILE}"
    return _Built(
        id=item.id,
        candidate=candidate,
        label=label,
        brief=family.brief if item.brief is None else item.brief,
        record=rec,
        others=[("source", "source", None, source_rel, sha256(source))] if with_source else [],
        requirements=requirements,
    )


def _build_scene_family(family, gen, out: Path, use_case, generators, wording="full") -> tuple[list[_Built], dict, Cells]:
    source = gen.generate(family.source.prompt, family.source.seed, family.source.size)
    rel = out / "families" / family.id / SOURCE_FILE
    rel.parent.mkdir(parents=True, exist_ok=True)
    rel.write_bytes(source)
    record = {
        "id": family.id,
        "kind": "scene",
        "generator": family.generator,
        "model": getattr(gen, "model", family.generator),
        "prompt": family.source.prompt,
        "seed": family.source.seed,
        "size": family.source.size,
        "source_sha256": sha256(source),
        "generated_on": today(),
        "protected": list(family.protected),
    }
    if any(family.traits):
        record["traits"] = [dict(t) for t in family.traits]
    built: list[_Built] = []
    earlier: dict[str, bytes] = {}
    for n, item in enumerate(family.items):
        b = _scene_item(family, item, n, source, gen, generators, use_case, 0, wording, earlier)
        built.append(b)
        earlier[item.id] = b.candidate
    return built, record, [("source", source, None)] + [(b.id, b.candidate, b.label) for b in built]


def _brief_item(family: Family, item: Item, gen: Any, requirements, seed_offset: int = 0) -> _Built:
    prompt = item.op.get("prompt") or family.brief
    candidate = gen.generate(prompt, int(item.op["seed"]) + seed_offset, int(item.op.get("size", 1024)))
    violates = item.op.get("violates")
    about = item.op.get("about")
    if violates is None:
        label = DraftLabel(
            expected="pass",
            failure_type=None,
            obligation_ids=[],
            words=(
                "generated from the brief as written; confirm everything asked for is there, "
                "as asked, and nothing forbidden"
            ),
            notes={"prompt": prompt},
        )
    else:
        label = DraftLabel(
            expected="fail",
            failure_type={"BC1": "asked-missing", "BC2": "forbidden-present", "BC3": "text-misspelt"}[violates],
            obligation_ids=[violates],
            words={
                "BC1": f"the brief asked for {about} and the candidate lacks it",
                "BC2": f"the brief forbade {about} and the candidate shows it",
                "BC3": f"the text {about!r} is not spelled as the brief gives it",
            }[violates],
            region_kind="whole_image",
            notes={"prompt": prompt},
        )
        if violates in ("BC1", "BC2"):
            _match_requirements(label, about, requirements)
    rec = {"id": item.id, "family": family.id, "op": dict(item.op), "prompt": prompt, "generated_on": today()}
    if seed_offset:
        rec["seed_offset"] = seed_offset
    return _Built(id=item.id, candidate=candidate, label=label, brief=family.brief, record=rec, requirements=requirements)


def _build_brief_family(family: Family, gen: Any) -> tuple[list[_Built], dict, Cells]:
    requirements = _brief_requirements(family)
    record = {
        "id": family.id,
        "kind": "brief",
        "generator": family.generator,
        "model": getattr(gen, "model", family.generator),
        "brief": family.brief,
        "asks": list(family.asks),
        "forbids": list(family.forbids),
        "text": list(family.text),
        "generated_on": today(),
    }
    built = [_brief_item(family, item, gen, requirements) for item in family.items]
    return built, record, [(b.id, b.candidate, b.label) for b in built]


def _references(family: Family, out: Path, gen: Any | None) -> list[tuple[str, bytes, str, str]]:
    """(image id, bytes, path relative to the set, sha256) of each reference: generated
    when `gen` is given, else read from the set folder."""
    refs = []
    for n, ref in enumerate(family.references, start=1):
        rel = f"families/{family.id}/ref-{n}.png"
        if gen is None:
            image = (out / rel).read_bytes()
        else:
            image = gen.generate(ref.prompt, ref.seed, ref.size)
            (out / rel).parent.mkdir(parents=True, exist_ok=True)
            (out / rel).write_bytes(image)
        refs.append((f"ref:{family.id}-{n}", image, rel, sha256(image)))
    return refs


def _subject_item(family, item, gen, ref_bytes, by_id, others, requirements, use_case, seed_offset=0) -> _Built:
    """One item of a subject family, from the references and, for a from item, the earlier
    candidate; `seed_offset` changes the seed on a rebuild."""
    briefs = {i.id: i.scene.prompt for i in family.items if i.scene is not None}
    if item.from_item is not None:
        base = by_id[item.from_item]
        base_seed = next((i.scene.seed for i in family.items if i.id == item.from_item and i.scene), 0)
        candidate = gen.edit(base, item.op["prompt"], base_seed + 1 + seed_offset)
        alters = item.op.get("alters") or item.op.get("removes")
        label = DraftLabel(
            expected="fail",
            failure_type=f"{alters}-changed" if alters else "edited",
            obligation_ids=list(_subject_rules(use_case, item.op.get("aspect"))[0]),
            words=(
                f"the {alters} is changed in the candidate, unlike the references"
                if alters
                else f"edited: {item.op['prompt']}"
            ),
            notes={"prompt": item.op["prompt"], "from": item.from_item},
        )
        _match_requirements(label, alters, requirements)
        ops._place(label, regions.difference_regions(base, candidate))
        brief = briefs.get(item.from_item, "")
        rec = {
            "id": item.id,
            "family": family.id,
            "from": item.from_item,
            "op": dict(item.op),
            "generated_on": today(),
            "edit_notes": label.notes,
        }
    elif item.lookalike is not None:
        prompt = f"{item.scene.prompt} The subject is {item.lookalike}."
        candidate = gen.generate(prompt, item.scene.seed + seed_offset, item.scene.size)
        label = DraftLabel(
            expected="fail",
            failure_type="lookalike",
            obligation_ids=list(_subject_rules(use_case, None)[1]),
            words=f"a lookalike, not the reference subject: {item.lookalike}",
            region_kind="whole_image",
            notes={"prompt": prompt},
        )
        brief = item.scene.prompt
        rec = {
            "id": item.id,
            "family": family.id,
            "scene": {"prompt": item.scene.prompt, "seed": item.scene.seed},
            "lookalike": item.lookalike,
            "generated_on": today(),
        }
    else:
        prompt = (
            f"{item.scene.prompt} The subject is exactly the one in the reference image(s): "
            f"{family.subject}; keep every marking, colour and feature."
        )
        candidate = gen.compose(ref_bytes, prompt, item.scene.seed + seed_offset)
        label = DraftLabel(
            expected="pass",
            failure_type=None,
            obligation_ids=[],
            words=(
                "the subject placed in a new scene from the references; confirm identity, "
                "markings and features against the references before accepting"
            ),
            notes={"prompt": prompt},
        )
        brief = item.scene.prompt
        rec = {
            "id": item.id,
            "family": family.id,
            "scene": {"prompt": item.scene.prompt, "seed": item.scene.seed},
            "generated_on": today(),
        }
    if seed_offset:
        rec["seed_offset"] = seed_offset
    return _Built(
        id=item.id,
        candidate=candidate,
        label=label,
        brief=brief,
        record=rec,
        others=list(others),
        requirements=requirements,
    )


def _build_subject_family(family, gen, out: Path, use_case) -> tuple[list[_Built], dict, Cells]:
    refs = _references(family, out, gen)
    requirements = _subject_requirements(family, [r[0] for r in refs], use_case)
    record = {
        "id": family.id,
        "kind": "subject",
        "generator": family.generator,
        "model": getattr(gen, "model", family.generator),
        "subject": family.subject,
        "references": [
            {"prompt": ref.prompt, "seed": ref.seed, "size": ref.size, "sha256": r[3]}
            for ref, r in zip(family.references, refs)
        ],
        "generated_on": today(),
        "protected": list(family.protected),
    }
    others = [(rid, "reference", family.description, rel, sha) for rid, _, rel, sha in refs]
    ref_bytes = [r[1] for r in refs]
    built: list[_Built] = []
    by_id: dict[str, bytes] = {}
    cells: Cells = [(rid, b, None) for rid, b, _, _ in refs]
    for item in family.items:
        b = _subject_item(family, item, gen, ref_bytes, by_id, others, requirements, use_case)
        built.append(b)
        by_id[item.id] = b.candidate
        cells.append((item.id, b.candidate, b.label))
    return built, record, cells


def _write_item(set_id: str, use_case: str, out: Path, b: _Built):
    """Writes one item's candidate and job; returns its record entry and its draft tables."""
    item_dir = out / "items" / b.id
    item_dir.mkdir(parents=True, exist_ok=True)
    candidate_rel = f"items/{b.id}/{CANDIDATE_FILE}"
    (out / candidate_rel).write_bytes(b.candidate)
    candidate_sha = sha256(b.candidate)
    images = [{"id": "candidate", "role": "candidate", "source": {"path": candidate_rel, "sha256": candidate_sha}}]
    for image_id, role, label, rel, sha in b.others:
        entry: dict[str, Any] = {"id": image_id, "role": role}
        if label:
            entry["label"] = label
        entry["source"] = {"path": rel, "sha256": sha}
        images.append(entry)
    write_text(item_dir / "job.toml", _job_text(set_id, use_case, b.brief, b.requirements, images))
    fields: dict[str, Any] = {
        "item": b.id,
        "expected": b.label.expected,
        "failure_types": [b.label.failure_type] if b.label.failure_type else [],
        "words": b.label.words,
        "obligation_ids": list(b.label.obligation_ids),
        "adjudicated_by": "owner",
        "adjudicated_on": "",
        "status": "draft",
    }
    drafts: list[tuple[str, dict[str, Any], bool]] = [("label", fields, True)]
    if b.label.region_kind:
        region: dict[str, Any] = {
            "frame": "raw:candidate",
            "kind": b.label.region_kind,
            "defect": b.label.failure_type or "changed",
            "obligation_ids": list(b.label.obligation_ids),
        }
        if b.label.bbox:
            region["bbox"] = b.label.bbox
        drafts.append(("label.regions", region, True))
    return {**b.record, "candidate_sha256": candidate_sha}, drafts


def label_tables(labels: list[dict[str, Any]]) -> list[tuple[str, dict[str, Any], bool]]:
    tables: list[tuple[str, dict[str, Any], bool]] = []
    for label in labels:
        label = dict(label)
        regs = label.pop("regions", [])
        tables.append(("label", label, True))
        tables += [("label.regions", r, True) for r in regs]
    return tables


def _record_text(set_id: str, set_fields, families, items) -> str:
    return toml_out.document(
        f"Construction record of {set_id}: owner material, never published.",
        [("set", set_fields, False)] + [("family", f, True) for f in families] + [("item", i, True) for i in items],
    )


def manifest_text(set_fields: dict[str, Any], items: list[dict[str, Any]], comment=MANIFEST_COMMENT) -> str:
    return toml_out.document(comment, [("set", set_fields, False)] + [("item", i, True) for i in items])


def _family_generator(family: Family, generators: dict[str, Any]) -> Any:
    gen = generators.get(family.generator)
    if gen is None:
        raise ValueError(f"family {family.id}: no generator {family.generator!r} available")
    return gen


def _owner_dir(plan: Plan, owner: Path) -> Path:
    """The set's folder in owner storage: owner storage, and the folder the set's id makes of
    it, both outside every repository and the second inside the first."""
    root = outside_every_repository(owner, "--owner")
    owner_dir = outside_every_repository(root / plan.set.id, "--owner with the set's id")
    if root not in owner_dir.parents:
        raise ValueError("the set's id leads out of owner storage")
    return owner_dir


def build(plan: Plan, out: Path, owner: Path, generators: dict[str, Any]) -> BuildReport:
    out = Path(out)
    owner_dir = _owner_dir(plan, owner)
    out.mkdir(parents=True, exist_ok=True)
    owner_dir.mkdir(parents=True, exist_ok=True)
    report = BuildReport()
    manifest_items: list[dict[str, Any]] = []
    families_out, record_families, record_items = [], [], []
    drafts: list[tuple[str, dict[str, Any], bool]] = []
    for family in plan.families:
        gen = _family_generator(family, generators)
        if family.kind == "subject":
            built, record, cells = _build_subject_family(family, gen, out, plan.set.use_case)
            report.images += len(family.references)
        elif family.kind == "brief":
            built, record, cells = _build_brief_family(family, gen)
        else:
            built, record, cells = _build_scene_family(
                family, gen, out, plan.set.use_case, generators, plan.set.requirements
            )
            report.images += 1
        families_out.append({"id": family.id, "description": family.description})
        record_families.append(record)
        for b in built:
            entry, item_drafts = _write_item(plan.set.id, plan.set.use_case, out, b)
            manifest_items.append({"id": b.id, "family": family.id, "job": f"items/{b.id}/job.toml"})
            record_items.append(entry)
            drafts += item_drafts
            report.items += 1
            report.images += 1
        _contact_sheet(cells, owner_dir / f"contact-{family.id}.png")
    set_fields: dict[str, Any] = {
        "id": plan.set.id,
        "use_case": plan.set.use_case,
        "split": plan.set.split,
        "licence": plan.set.licence,
        "images_repo": plan.set.images_repo,
        "images_rev": plan.set.images_rev,
        "labels": "labels.toml",
        "families": "families.toml",
        "rules": dict(plan.set.rules),
    }
    write_text(out / "set.toml", manifest_text(set_fields, manifest_items))
    write_text(
        out / "families.toml",
        toml_out.document(
            "Scene, subject or brief families; a family is never in more than one split.",
            [("family", f, True) for f in families_out],
        )
    )
    write_text(
        owner_dir / "construction.toml",
        _record_text(plan.set.id, {"id": plan.set.id, "built_on": today()}, record_families, record_items)
    )
    write_text(owner_dir / "labels-draft.toml", toml_out.document(DRAFT_COMMENT, drafts))
    return report


def _resync(entries, key, fresh, item_id, family_ids, plan_ids):
    """`entries` with the family's block following the plan's item list: the rebuilt item
    replaced, a new id put in its place, an id the plan lost dropped; others untouched."""
    own = {e[key]: e for e in entries if e[key] in family_ids}
    own[item_id] = fresh
    block = [own[i] for i in plan_ids if i in own]
    merged: list[dict[str, Any]] = []
    placed = False
    for e in entries:
        if e[key] in family_ids:
            if not placed:
                merged += block
                placed = True
        else:
            merged.append(e)
    return merged if placed else merged + block


def rebuild(plan: Plan, out: Path, owner: Path, generators: dict[str, Any], item_id: str, seed_offset: int = 0) -> BuildReport:
    """Makes one item again from the family's pictures already in the set folder (references
    or source, and the earlier candidate of a from item), with the plan as it now reads and
    an optional seed offset; writes that item's job, record entry, draft label (back to
    draft) and the family's contact sheet again, and nothing else."""
    out = Path(out)
    owner_dir = _owner_dir(plan, owner)
    family = next((f for f in plan.families for i in f.items if i.id == item_id), None)
    if family is None:
        raise ValueError(f"no item {item_id!r} in the plan")
    item = next(i for i in family.items if i.id == item_id)
    gen = _family_generator(family, generators)
    use_case = plan.set.use_case
    cells: Cells = []
    if family.kind == "subject":
        refs = _references(family, out, None)
        others = [(rid, "reference", family.description, rel, sha) for rid, _, rel, sha in refs]
        requirements = _subject_requirements(family, [r[0] for r in refs], use_case)
        by_id = {}
        if item.from_item is not None:
            by_id[item.from_item] = (out / "items" / item.from_item / CANDIDATE_FILE).read_bytes()
        b = _subject_item(family, item, gen, [r[1] for r in refs], by_id, others, requirements, use_case, seed_offset)
        cells = [(rid, data, None) for rid, data, _, _ in refs]
    elif family.kind == "brief":
        b = _brief_item(family, item, gen, _brief_requirements(family), seed_offset)
    else:
        source = (out / "families" / family.id / SOURCE_FILE).read_bytes()
        n = family.items.index(item)
        followers = [i.id for i in family.items if i.same_as == item.id]
        if followers:
            raise ValueError("another item is judged on this item's pictures; build the set again instead")
        earlier = {}
        if item.same_as is not None:
            earlier[item.same_as] = (out / "items" / item.same_as / CANDIDATE_FILE).read_bytes()
        b = _scene_item(
            family, item, n, source, gen, generators, use_case, seed_offset, plan.set.requirements, earlier
        )
        cells = [("source", source, None)]
    entry, drafts = _write_item(plan.set.id, use_case, out, b)
    record = tomllib.loads(read_text(owner_dir / "construction.toml"))
    plan_ids = [i.id for i in family.items]
    family_ids = {i["id"] for i in record.get("item", []) if i.get("family") == family.id} | set(plan_ids)
    sync = lambda entries, key, fresh: _resync(entries, key, fresh, item_id, family_ids, plan_ids)  # noqa: E731
    items = sync(record.get("item", []), "id", entry)
    write_text(
        owner_dir / "construction.toml",
        _record_text(plan.set.id, record["set"], record.get("family", []), items)
    )
    draft = tomllib.loads(read_text(owner_dir / "labels-draft.toml"))
    fresh = dict(drafts[0][1])
    if len(drafts) > 1:
        fresh["regions"] = [d[1] for d in drafts[1:]]
    labels = sync(draft.get("label", []), "item", fresh)
    write_text(owner_dir / "labels-draft.toml", toml_out.document(DRAFT_COMMENT, label_tables(labels)))
    manifest = tomllib.loads(read_text(out / "set.toml"))
    for i in manifest.get("item", []):
        if i["id"] in family_ids and i["id"] not in plan_ids:
            shutil.rmtree(out / "items" / i["id"], ignore_errors=True)
    manifest_items = sync(
        manifest.get("item", []),
        "id",
        {"id": item_id, "family": family.id, "job": f"items/{item_id}/job.toml"},
    )
    write_text(out / "set.toml", manifest_text(manifest["set"], manifest_items))
    by_item = {label["item"]: label for label in labels}
    for i in family.items:
        data = (out / "items" / i.id / CANDIDATE_FILE).read_bytes()
        lab = by_item.get(i.id)
        cells.append(
            (
                i.id,
                data,
                DraftLabel(expected=lab["expected"], failure_type=None, obligation_ids=[], words=lab["words"])
                if lab
                else None,
            )
        )
    _contact_sheet(cells, owner_dir / f"contact-{family.id}.png")
    return BuildReport(items=1, images=1)
