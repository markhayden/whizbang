"""`python -m whizbang_sets build | rebuild | promote | draw | pin`
(`docs/evalsets.md`, "Building a set").

**Output that tells nothing of a blind set** (plan mE, task B1, item 8). For a blind set,
what is printed is counts and the set's id alone: never an item's id, a path, a prompt, a
label, a plan's text or a provider's answer. A failure is printed by its kind (a provider's
by its status alone), and its whole text is kept in owner storage, in the set's
`messages.log`. A plan or a manifest that cannot be read is taken as blind. A provider's
answer is never printed for any set: it goes to that log when there is one.

Ported from squint `scripts/sets/src/squint_sets/cli.py`.
"""

from __future__ import annotations

import argparse
import datetime as dt
import re
import sys
import tomllib
from pathlib import Path

from . import build as build_mod
from . import draw as draw_mod
from . import generators
from . import plan as plan_mod
from . import seal as seal_mod
from . import spend
from .egress import Approved, GeneratorError, Refused, Sender

LOG = "messages.log"
SAID = (ValueError, OSError, KeyError, GeneratorError, Refused, spend.SpendRefused)


def _parser() -> argparse.ArgumentParser:
    ap = argparse.ArgumentParser(prog="whizbang-sets", description="whizbang's set builder")
    sub = ap.add_subparsers(dest="command", required=True)

    def common(p: argparse.ArgumentParser) -> None:
        p.add_argument("--plan", required=True, type=Path)
        p.add_argument("--out", required=True, type=Path, help="the set folder (<root>/<use-case>/<set>)")
        p.add_argument("--owner", required=True, type=Path, help="owner storage, outside every repository")
        how = p.add_mutually_exclusive_group(required=True)
        how.add_argument("--fake", action="store_true", help="the fake generator for every family: no network")
        how.add_argument("--config", type=Path, help="the whizbang config whose recipients the generators may reach")
        p.add_argument("--ledger", type=Path, help="a live build's spend ledger, in owner storage")
        p.add_argument("--cap-usd", type=float, help="the ledger's allowance, set once, at its first use")
        p.add_argument("--prices", type=Path, default=spend.PRICES_FILE, help="the dated bound prices")
        p.add_argument("--account", help="the account to send as, where the config approves several at an origin")

    common(sub.add_parser("build", help="a set from a plan: the set folder and the owner's worksheet"))
    r = sub.add_parser("rebuild", help="one item again, from the family's pictures already built")
    common(r)
    r.add_argument("--item", required=True)
    r.add_argument("--seed-offset", type=int, default=0, help="added to the item's seed, for another draw")
    p = sub.add_parser("promote", help="publish the confirmed labels, drop the struck items, seal the set")
    p.add_argument("--draft", required=True, type=Path, help="the reviewed labels-draft.toml")
    p.add_argument("--set", required=True, type=Path, help="the set folder")
    p.add_argument("--attest", required=True, help="the owner's attestation, kept in the seal")
    p.add_argument("--labels-out", type=Path, help="a blind set's labels go here, in owner storage")
    d = sub.add_parser("draw", help="a blind set's plan, drawn from the owner's pool by a seed")
    d.add_argument("--pool", required=True, type=Path)
    d.add_argument("--seed", required=True, type=int)
    d.add_argument("--out", required=True, type=Path, help="the plan to write, in owner storage")
    n = sub.add_parser("pin", help="name the revision of the pictures' repository the set is at")
    n.add_argument("--set", required=True, type=Path)
    n.add_argument("--rev", required=True)
    return ap


def _generators(args):
    """The generators, and the ledger when they are live."""
    if args.fake:
        return generators.fake(), None
    if args.ledger is None:
        raise ValueError("a live build names its spend ledger, in owner storage: --ledger")
    approved = Approved.from_config(args.config)
    ledger = spend.Ledger(args.ledger, args.cap_usd)
    guard = spend.Guard(ledger, spend.prices(args.prices))
    return generators.live(Sender(approved, account=args.account), guard), ledger


def _plan(args) -> plan_mod.Plan:
    plan = plan_mod.load(args.plan)
    if plan.set.split == "blind":
        # A blind item's construction gives its label away: its plan is owner material.
        build_mod.outside_every_repository(args.plan, "a blind set's --plan")
    return plan


def _stated(path: Path) -> tuple[str | None, str | None]:
    """(split, set id) a plan or a manifest states, or (None, None) when it cannot be read."""
    try:
        with open(path, "rb") as f:
            s = tomllib.load(f)["set"]
        return str(s["split"]), str(s["id"])
    except Exception:  # noqa: BLE001: what cannot be read is taken as blind
        return None, None


def _blind(args) -> tuple[bool, Path | None]:
    """Whether the command is about a blind set (or may be), and where its messages go."""
    if args.command in ("build", "rebuild"):
        split, set_id = _stated(args.plan)
        plain = set_id is not None and all(plan_mod.PLAIN.match(p) for p in set_id.split("/"))
        keep = args.owner / set_id if plain and set_id.count("/") == 1 else args.owner
        return split not in ("dev", "heldout"), keep
    if args.command == "promote":
        split, _ = _stated(args.set / "set.toml")
        return split not in ("dev", "heldout"), args.draft.parent
    if args.command == "draw":
        return True, args.out.parent
    return False, None


def _kind(e: BaseException) -> str:
    if isinstance(e, GeneratorError):
        return "a provider's request failed" + ("" if e.status is None else f": HTTP {e.status}")
    if isinstance(e, spend.SpendRefused):
        return "the spend guard refused a request; nothing was sent"
    if isinstance(e, Refused):
        return "a request the config does not approve was refused; nothing was sent"
    if isinstance(e, ValueError):
        return "the plan, the set or an argument was refused"
    if isinstance(e, OSError):
        return "a file could not be read or written"
    return f"it failed ({type(e).__name__})"


def _keep(folder: Path | None, e: BaseException) -> bool:
    """The failure's whole text, and a provider's answer, appended to the log in owner
    storage; False when there is nowhere outside every repository to keep it."""
    if folder is None:
        return False
    try:
        folder = build_mod.outside_every_repository(folder, "the log's folder")
        folder.mkdir(parents=True, exist_ok=True)
        detail = getattr(e, "detail", "")
        at = dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
        with open(folder / LOG, "a", encoding="utf-8") as f:
            f.write(f"{at} {type(e).__name__}: {e}\n" + (f"  the answer: {detail}\n" if detail else ""))
        return True
    except Exception:  # noqa: BLE001: a log that cannot be written is said, not raised
        return False


def _run(args) -> str:
    if args.command in ("build", "rebuild"):
        plan = _plan(args)
        gens, ledger = _generators(args)
        try:
            if args.command == "build":
                r = build_mod.build(plan, args.out, args.owner, gens)
                said = f"built {plan.set.id}: {len(plan.families)} families, {r.items} items, {r.images} pictures"
            else:
                build_mod.rebuild(plan, args.out, args.owner, gens, args.item, args.seed_offset)
                said = f"rebuilt one item of {plan.set.id}; its label is back to draft"
        finally:
            if ledger is not None:
                print(ledger.said(), file=sys.stderr)
        return said
    if args.command == "promote":
        r = seal_mod.promote(args.draft, args.set, args.attest, args.labels_out)
        return f"promoted {r.confirmed} confirmed label(s), struck {r.struck} item(s), sealed"
    if args.command == "draw":
        families, items = draw_mod.run(args.pool, args.seed, args.out)
        return f"drew a blind plan: {families} families, {items} items, in owner storage"
    if not re.fullmatch(r"[0-9a-f]{40}", args.rev):
        raise ValueError("--rev is a revision of 40 hex digits")
    seal_mod.pin(args.set, args.rev)
    return "pinned the revision of the pictures"


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    blind, keep = _blind(args)
    try:
        print(_run(args))
        return 0
    except Exception as e:  # noqa: BLE001: every failure is said by the rules above
        kept = _keep(keep, e)
        if blind:
            where = "its text is kept in owner storage" if kept else "its text was not kept"
            print(f"whizbang-sets: {_kind(e)}; {where}", file=sys.stderr)
        elif isinstance(e, SAID):
            print(f"whizbang-sets: {e}", file=sys.stderr)
        else:
            print(f"whizbang-sets: {_kind(e)}: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
