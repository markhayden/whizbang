"""A set's plan: what to generate for each family, and which construction op makes each item.

Read strictly: an unknown key anywhere is refused, so an answer cannot ride in a plan into a
public file. A plan is owner material; a blind set's lives in owner storage.

Ported from squint `scripts/sets/src/squint_sets/plan.py`.
"""

from __future__ import annotations

import re
import tomllib
from dataclasses import dataclass, field
from typing import Any

GENERATORS = ("fal", "openai", "gemini", "fake")
#: The repository the pictures are in, by its current name (`SETS_REPOSITORY` in
#: `crates/whizbang/src/evaluation.rs`). A new set names it, and no other.
REPOSITORY = "github.com/markhayden/whizbang-evalsets"
SPLITS = ("dev", "heldout", "blind")
KINDS = ("scene", "subject", "brief")
#: A use case, a set's name, a family's id or an item's id: each becomes a folder or a file
#: name, so it is plain lower-case letters, digits and hyphens, and nothing that leads out.
PLAIN = re.compile(r"[a-z0-9][a-z0-9-]*\Z")


def _plain(value: Any, what: str) -> str:
    if not isinstance(value, str) or not PLAIN.match(value):
        raise ValueError(f"{what} must be lower-case letters, digits and hyphens")
    return value


@dataclass(frozen=True)
class SetSpec:
    id: str
    use_case: str
    split: str
    licence: str
    images_repo: str
    rules: dict[str, Any]
    images_rev: str | None = None
    #: "full": a protected subject's stated colour, count and lettering are written into its
    #: requirement; "plain": today's wording, so one plan builds both twins.
    requirements: str = "full"


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
    # Scene families only: the generator of this item's candidate, when not the family's;
    # the job's brief, when not the family's; and an earlier item whose candidate this item's
    # job judges again, the same pictures under another brief (a `cover` op only).
    generator: str | None = None
    brief: str | None = None
    same_as: str | None = None


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
    # Protected subjects, one requirement each, and what each states of itself (its
    # `colour`, `count` and `lettering`, in a scene family), in the same order.
    protected: list[str] = field(default_factory=list)
    traits: list[dict[str, str]] = field(default_factory=list)
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
        _plain(it["id"], f"{where}: an item's id")
        if it["id"] in seen:
            raise ValueError(f"{iw}: duplicate item id")
        seen.add(it["id"])
        if kind == "scene":
            _only(it, {"id", "op", "generator", "brief", "same_as"}, iw)
            _require(it, ("op",), iw)
            op = dict(it["op"])
            if "kind" not in op:
                raise ValueError(f"{iw}: op needs a kind")
            if op["kind"] == "cover":
                _only(op, {"kind", "prompt", "covers", "permitted"}, f"{iw} op")
                _require(op, ("prompt", "covers", "permitted"), f"{iw} op")
                if not isinstance(op["permitted"], bool):
                    raise ValueError(f"{iw}: a cover's permitted is true or false")
            same_as = it.get("same_as")
            if same_as is not None:
                first = next((i for i in items if i.id == same_as), None)
                if first is None or op["kind"] != "cover" or first.op.get("kind") != "cover":
                    raise ValueError(f"{iw}: same_as names an earlier cover item, and is one")
                if it.get("generator") is not None:
                    raise ValueError(f"{iw}: an item judged on another's pictures makes none")
            gen = it.get("generator")
            brief = it.get("brief")
            if brief is not None and not isinstance(brief, str):
                raise ValueError(f"{iw}: brief is a text")
            items.append(
                Item(
                    id=it["id"],
                    op=op,
                    generator=None if gen is None else _generator(gen, iw),
                    brief=brief,
                    same_as=same_as,
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


TRAITS = ("colour", "count", "lettering")


def _protected(entries, kind: str, where: str) -> list[tuple[str, dict[str, str]]]:
    """Each protected subject: a phrase, or a table with the phrase as `subject` and what it
    states of itself, in a scene family only."""
    out = []
    for n, entry in enumerate(entries, start=1):
        pw = f"{where} protected subject {n}"
        if isinstance(entry, str):
            out.append((entry, {}))
            continue
        if kind != "scene" or not isinstance(entry, dict):
            raise ValueError(f"{pw}: a phrase, or in a scene family a table of the subject and its traits")
        _only(entry, {"subject", *TRAITS}, pw)
        _require(entry, ("subject",), pw)
        traits = {k: entry[k] for k in TRAITS if k in entry}
        if not all(isinstance(v, str) and v.strip() for v in [entry["subject"], *traits.values()]):
            raise ValueError(f"{pw}: the subject and each trait are texts")
        out.append((entry["subject"], traits))
    return out


def parse(text: str) -> Plan:
    doc = tomllib.loads(text)
    _only(doc, {"set", "family"}, "plan")
    _require(doc, ("set", "family"), "plan")
    s = doc["set"]
    _only(
        s,
        {"id", "use_case", "split", "licence", "images_repo", "images_rev", "rules", "requirements"},
        "[set]",
    )
    if s.get("requirements", "full") not in ("full", "plain"):
        raise ValueError('[set] requirements must be "full" or "plain"')
    _require(s, ("id", "use_case", "split", "licence", "rules"), "[set]")
    if s.get("images_repo", REPOSITORY) != REPOSITORY:
        raise ValueError(f"[set] images_repo is {REPOSITORY}, the repository's current name")
    if s["split"] not in SPLITS:
        raise ValueError(f"[set] split must be one of {', '.join(SPLITS)}")
    _only(s["rules"], {"base", "version"}, "[set] rules")
    _require(s["rules"], ("base", "version"), "[set] rules")
    use_case = _plain(s["use_case"], "[set] use_case")
    parts = s["id"].split("/") if isinstance(s["id"], str) else []
    if len(parts) != 2 or parts[0] != use_case:
        raise ValueError("[set] id must be <use_case>/<name>, of the set's own use case")
    _plain(parts[1], "[set] id's name")
    spec = SetSpec(
        id=s["id"],
        use_case=s["use_case"],
        split=s["split"],
        licence=s["licence"],
        images_repo=s.get("images_repo", REPOSITORY),
        images_rev=s.get("images_rev"),
        rules=dict(s["rules"]),
        requirements=s.get("requirements", "full"),
    )
    families: list[Family] = []
    seen: set[str] = set()
    for f in doc["family"]:
        where = "a family"
        _plain(f.get("id"), "a family's id")
        where = f"family {f['id']}"
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
        protected = _protected(f.get("protected", []), kind, where)
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
                protected=[p for p, _ in protected],
                traits=[t for _, t in protected],
                asks=[str(x) for x in f.get("asks", [])],
                forbids=[str(x) for x in f.get("forbids", [])],
                text=[str(x) for x in f.get("text", [])],
            )
        )
    if spec.split == "blind":
        _opaque(families)
    return Plan(set=spec, families=families)


def _opaque(families: list[Family]) -> None:
    """A blind set's item ids are public (its manifest, its folders) and must say nothing of
    how an item was made: each is its family's id, a hyphen and its place, from 1."""
    for f in families:
        for n, item in enumerate(f.items, start=1):
            if item.id != f"{f.id}-{n}":
                raise ValueError(
                    f"family {f.id}: a blind set's item ids are <family>-<n> in order; item {n} is not"
                )


def load(path) -> Plan:
    with open(path, "rb") as f:
        return parse(f.read().decode("utf-8"))
