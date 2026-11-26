"""The drawer of a blind set's plan (plan mE, task B1; `docs/evalsets.md`, "Blind sets").

A blind item's construction gives its label away, so the agent that builds a blind set
never reads its plan. The owner writes a pool in owner storage: the set, and each scene
family with its source, its protected subjects, and the edits and restyles it may be given.
`draw` chooses, from a seed, how many of each kind each family gets, which edit or restyle,
which structural op and with what strength, which generator, and in what order; it writes
the plan beside the pool, in owner storage, and prints counts alone. The same pool and seed
draw the same plan.

Each family gets at least one pass (a `grade` or a `restyle`), one edit that breaks its
identity rule, and one structural op; the rest are drawn from all three. Item ids are
`<family>-<n>` in the drawn order, as a blind set's must be.
"""

from __future__ import annotations

import random
import tomllib
from pathlib import Path
from typing import Any

from . import plan as plan_mod
from . import toml_out
from .build import outside_every_repository, write_text

STRUCTURAL = ("mirror", "rotate", "crop", "tilt", "zoom", "keystone")
POOL_SET_KEYS = {"id", "use_case", "licence", "rules", "requirements"}
POOL_FAMILY_KEYS = {"id", "generator", "description", "brief", "source", "protected", "edits", "restyles"}


def _structural(rng: random.Random) -> dict[str, Any]:
    kind = rng.choice(STRUCTURAL)
    if kind == "rotate":
        return {"kind": kind, "degrees": rng.choice((90, 180, 270))}
    if kind == "crop":
        return {"kind": kind, "keep": round(rng.uniform(0.6, 0.9), 2)}
    if kind == "tilt":
        return {"kind": kind, "degrees": round(rng.choice((-1, 1)) * rng.uniform(1.0, 6.0), 1)}
    if kind == "zoom":
        return {"kind": kind, "factor": round(rng.uniform(1.05, 1.3), 2)}
    if kind == "keystone":
        return {"kind": kind, "strength": round(rng.choice((-1, 1)) * rng.uniform(0.03, 0.1), 3)}
    return {"kind": kind}


def _pass(rng: random.Random, restyles: list[dict[str, Any]]) -> dict[str, Any]:
    if restyles and rng.random() < 0.5:
        return {"kind": "restyle", "prompt": rng.choice(restyles)["prompt"]}
    return {"kind": "grade", "warmth": round(rng.uniform(0.05, 0.15), 2)}


def _edit(rng: random.Random, edits: list[dict[str, Any]]) -> dict[str, Any]:
    return {"kind": "edit", **rng.choice(edits)}


def draw(pool: dict[str, Any], seed: int) -> str:
    """The plan's text, drawn from the pool by the seed."""
    s = dict(pool.get("set", {}))
    if set(s) - POOL_SET_KEYS or not {"id", "use_case", "licence", "rules"} <= set(s):
        raise ValueError("the pool's [set] has id, use_case, licence and rules, and may say requirements")
    d = pool.get("draw", {})
    per = d.get("items_per_family", 5)
    generators = d.get("generators", ["fal", "openai", "gemini"])
    if set(d) - {"items_per_family", "generators"} or not isinstance(per, int) or not 3 <= per <= 12:
        raise ValueError("the pool's [draw] has items_per_family, 3 to 12, and generators")
    if not generators or any(g not in plan_mod.GENERATORS for g in generators):
        raise ValueError("the pool's generators are among the builder's")
    families = pool.get("family", [])
    if not families:
        raise ValueError("the pool has no family")
    rng = random.Random(seed)
    tables: list[tuple[str, dict[str, Any], bool]] = [("set", {**s, "split": "blind"}, False)]
    for f in families:
        if set(f) - POOL_FAMILY_KEYS:
            raise ValueError("a pool family has only the keys of a scene family, edits and restyles")
        edits = list(f.get("edits", []))
        restyles = list(f.get("restyles", []))
        if not edits:
            raise ValueError("each pool family has at least one edit")
        kinds = ["pass", "edit", "structural"] + [rng.choice(("pass", "edit", "structural")) for _ in range(per - 3)]
        rng.shuffle(kinds)
        family = {k: v for k, v in f.items() if k not in ("edits", "restyles")}
        family.setdefault("generator", rng.choice(generators))
        tables.append(("family", family, True))
        for n, kind in enumerate(kinds, start=1):
            if kind == "pass":
                op = _pass(rng, restyles)
            elif kind == "edit":
                op = _edit(rng, edits)
            else:
                op = _structural(rng)
            tables.append(("family.item", {"id": f"{family['id']}-{n}", "op": op}, True))
    text = toml_out.document("A blind set's plan, drawn by `whizbang_sets draw`: owner material.", tables)
    plan_mod.parse(text)  # what is drawn is a plan the builder takes
    return text


def run(pool_path: Path, seed: int, out: Path) -> tuple[int, int]:
    """Draws the plan of the pool at `pool_path` into `out`, both outside every repository;
    returns (families, items)."""
    pool_path = outside_every_repository(pool_path, "--pool")
    out = outside_every_repository(out, "--out")
    if out.exists():
        raise ValueError("--out is there already; a drawn plan is never written over")
    with open(pool_path, "rb") as f:
        pool = tomllib.load(f)
    text = draw(pool, seed)
    parsed = plan_mod.parse(text)
    out.parent.mkdir(parents=True, exist_ok=True)
    write_text(out, text)
    return len(parsed.families), sum(len(f.items) for f in parsed.families)
