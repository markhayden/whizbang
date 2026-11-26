"""`python -m whizbang_sets build | rebuild | promote` (`docs/evalsets.md`, "Building a set").

Ported from squint `scripts/sets/src/squint_sets/cli.py`.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from . import build as build_mod
from . import generators
from . import plan as plan_mod
from . import seal as seal_mod
from .egress import GeneratorError, Refused

#: Until the spend guard is built (plan mE, task B1, item 7), nothing is generated live.
LIVE_WAITS = (
    "live generation waits for the spend guard (plan mE, task B1, its second pull request); "
    "only --fake builds for now"
)


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
    return ap


def _generators(args) -> dict:
    if args.fake:
        return generators.fake()
    raise ValueError(LIVE_WAITS)


def _plan(args) -> plan_mod.Plan:
    plan = plan_mod.load(args.plan)
    if plan.set.split == "blind":
        # A blind item's construction gives its label away: its plan is owner material.
        build_mod.outside_every_repository(args.plan, "a blind set's --plan")
    return plan


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    try:
        if args.command == "build":
            plan = _plan(args)
            r = build_mod.build(plan, args.out, args.owner, _generators(args))
            print(f"built {plan.set.id}: {r.items} items, {r.images} pictures; the worksheet is in owner storage")
        elif args.command == "rebuild":
            plan = _plan(args)
            build_mod.rebuild(plan, args.out, args.owner, _generators(args), args.item, args.seed_offset)
            print(f"rebuilt {args.item} of {plan.set.id}; its label is back to draft")
        else:
            r = seal_mod.promote(args.draft, args.set, args.attest, args.labels_out)
            print(f"promoted {r.confirmed} confirmed label(s), struck {r.struck} item(s), sealed")
        return 0
    except (ValueError, OSError, KeyError, GeneratorError, Refused) as e:
        print(f"whizbang-sets: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
