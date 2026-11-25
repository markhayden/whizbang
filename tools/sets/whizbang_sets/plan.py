"""A set's plan: what to generate for each family, and which construction op makes each item.

Read strictly: an unknown key anywhere is refused, so an answer cannot ride in a plan into a
public file. A plan is owner material; a blind set's lives in owner storage.

Ported from squint `scripts/sets/src/squint_sets/plan.py`.
"""

from __future__ import annotations

import tomllib
from dataclasses import dataclass, field
from typing import Any

GENERATORS = ("fal", "openai", "gemini", "fake")
SPLITS = ("dev", "heldout", "blind")
KINDS = ("scene", "subject", "brief")


@dataclass(frozen=True)
class SetSpec:
    id: str
    use_case: str
    split: str
    licence: str
    images_repo: str
    rules: dict[str, Any]
    images_rev: str | None = None


@dataclass(frozen=True)
class Source:
    prompt: str
    seed: int
    size: int = 1024


@dataclass(frozen=True)
class Item:
    id: str
    # Scene families: the op applied to the family's source. Subject families: `scene` makes
    # the candidate from the references; `from_item` with an `edit` op changes an earlier
    # candidate; `lookalike` makes the scene from a changed description, without references.
    op: dict[str, Any] | None = None
    scene: Source | None = None
    from_item: str | None = None
    lookalike: str | None = None
    # Scene families only: the generator of this item's candidate, when not the family's.
    generator: str | None = None


@dataclass(frozen=True)
class Family:
    id: str
    generator: str
    description: str
    items: list[Item] = field(default_factory=list)
    kind: str = "scene"
    brief: str = ""
    source: Source | None = None
    subject: str = ""
    references: list[Source] = field(default_factory=list)
    # Protected subjects, one requirement each.
    protected: list[str] = field(default_factory=list)
    # Brief families: what the brief asks for and forbids, and the exact text it gives.
    asks: list[str] = field(default_factory=list)
    forbids: list[str] = field(default_factory=list)
    text: list[str] = field(default_factory=list)


@dataclass(frozen=True)
class Plan:
    set: SetSpec
    families: list[Family]


def _only(table: dict[str, Any], allowed: set[str], where: str) -> None:
    extra = sorted(set(table) - allowed)
    if extra:
        raise ValueError(f"{where}: unknown key(s) {', '.join(extra)}")


def _require(table: dict[str, Any], keys: tuple[str, ...], where: str) -> None:
    missing = [k for k in keys if k not in table]
    if missing:
        raise ValueError(f"{where}: missing {', '.join(missing)}")


def _source(t: dict[str, Any], where: str, size: int = 1024) -> Source:
    _only(t, {"prompt", "seed", "size"}, where)
    _require(t, ("prompt", "seed"), where)
    return Source(prompt=t["prompt"], seed=int(t["seed"]), size=int(t.get("size", size)))


def _generator(name: Any, where: str) -> str:
    if name not in GENERATORS:
        raise ValueError(f"{where}: generator must be one of {', '.join(GENERATORS)}")
    return name


def _items(f: dict[str, Any], kind: str, where: str, seen: set[str]) -> list[Item]:
    items: list[Item] = []
    earlier: set[str] = set()
    for it in f.get("item", []):
        iw = f"{where} item {it.get('id', '?')}"
        _require(it, ("id",), iw)
        if it["id"] in seen:
            raise ValueError(f"{iw}: duplicate item id")
        seen.add(it["id"])
        if kind == "scene":
            _only(it, {"id", "op", "generator"}, iw)
            _require(it, ("op",), iw)
            if "kind" not in it["op"]:
                raise ValueError(f"{iw}: op needs a kind")
            gen = it.get("generator")
            items.append(
                Item(
                    id=it["id"],
                    op=dict(it["op"]),
                    generator=None if gen is None else _generator(gen, iw),
                )
            )
        elif kind == "brief":
            _only(it, {"id", "op"}, iw)
            _require(it, ("op",), iw)
            op = dict(it["op"])
            if op.get("kind") != "generate" or "seed" not in op:
                raise ValueError(f"{iw}: a brief item's op is generate with a seed")
            if op.get("violates") not in (None, "BC1", "BC2", "BC3"):
                raise ValueError(f"{iw}: violates must be BC1, BC2 or BC3")
            items.append(Item(id=it["id"], op=op))
        else:
            _only(it, {"id", "scene", "from", "op", "lookalike"}, iw)
            scene = None
            if "scene" in it:
                scene = _source(it["scene"], f"{iw} scene", int(f.get("size", 1024)))
            from_item = it.get("from")
            op = dict(it["op"]) if "op" in it else None
            lookalike = it.get("lookalike")
            if from_item is not None:
                if from_item not in earlier:
                    raise ValueError(f"{iw}: from {from_item!r} is not an earlier item of this family")
                if op is None or op.get("kind") != "edit":
                    raise ValueError(f"{iw}: an item built from another needs an edit op")
                if scene is not None or lookalike is not None:
                    raise ValueError(f"{iw}: from takes an op only")
            elif scene is None:
                raise ValueError(f"{iw}: a subject item needs a scene or a from")
            elif op is not None:
                raise ValueError(f"{iw}: an op applies to a from item")
            items.append(
                Item(id=it["id"], op=op, scene=scene, from_item=from_item, lookalike=lookalike)
            )
        earlier.add(it["id"])
    if not items:
        raise ValueError(f"{where}: no items")
    return items


SCENE_KEYS = {"id", "kind", "generator", "description", "brief", "source", "item", "protected"}
BRIEF_KEYS = {"id", "kind", "generator", "description", "brief", "asks", "forbids", "text"}
BRIEF_KEYS |= {"item", "size"}
SUBJECT_KEYS = {"id", "kind", "generator", "description", "subject", "references", "item"}
SUBJECT_KEYS |= {"protected", "size"}


def parse(text: str) -> Plan:
    doc = tomllib.loads(text)
    _only(doc, {"set", "family"}, "plan")
    _require(doc, ("set", "family"), "plan")
    s = doc["set"]
    _only(s, {"id", "use_case", "split", "licence", "images_repo", "images_rev", "rules"}, "[set]")
    _require(s, ("id", "use_case", "split", "licence", "images_repo", "rules"), "[set]")
    if s["split"] not in SPLITS:
        raise ValueError(f"[set] split must be one of {', '.join(SPLITS)}")
    _only(s["rules"], {"base", "version"}, "[set] rules")
    _require(s["rules"], ("base", "version"), "[set] rules")
    spec = SetSpec(
        id=s["id"],
        use_case=s["use_case"],
        split=s["split"],
        licence=s["licence"],
        images_repo=s["images_repo"],
        images_rev=s.get("images_rev"),
        rules=dict(s["rules"]),
    )
    families: list[Family] = []
    seen: set[str] = set()
    for f in doc["family"]:
        where = f"family {f.get('id', '?')}"
        kind = f.get("kind", "scene")
        if kind not in KINDS:
            raise ValueError(f"{where}: kind must be scene, subject or brief")
        gen = _generator(f.get("generator"), where)
        source = None
        references: list[Source] = []
        if kind == "scene":
            _only(f, SCENE_KEYS, where)
            _require(f, ("id", "generator", "description", "brief", "source"), where)
            source = _source(f["source"], f"{where} source")
        elif kind == "brief":
            _only(f, BRIEF_KEYS, where)
            _require(f, ("id", "generator", "description", "brief", "asks"), where)
        else:
            _only(f, SUBJECT_KEYS, where)
            _require(f, ("id", "generator", "description", "subject", "references"), where)
            size = int(f.get("size", 1024))
            references = [
                _source(r, f"{where} reference {n}", size)
                for n, r in enumerate(f["references"], start=1)
            ]
            if not 1 <= len(references) <= 4:
                raise ValueError(f"{where}: one to four references")
        families.append(
            Family(
                id=f["id"],
                kind=kind,
                generator=gen,
                description=f["description"],
                brief=f.get("brief", ""),
                source=source,
                subject=f.get("subject", ""),
                references=references,
                items=_items(f, kind, where, seen),
                protected=[str(x) for x in f.get("protected", [])],
                asks=[str(x) for x in f.get("asks", [])],
                forbids=[str(x) for x in f.get("forbids", [])],
                text=[str(x) for x in f.get("text", [])],
            )
        )
    return Plan(set=spec, families=families)


def load(path) -> Plan:
    with open(path, "rb") as f:
        return parse(f.read().decode("utf-8"))
