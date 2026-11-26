"""The spend guard of the generators (plan mE, task B1, item 7).

A table of a bound price per generated picture for each generator, dated, which the
orchestrator reads from each provider's page before the first build (`prices.toml`
beside this program). One durable ledger file in owner storage for all of a milestone's
generation (`--ledger`), with one allowance set once (`--cap-usd`, at the ledger's first
use) and shared by every build and rebuild across interruptions. Every request, each retry
of it included, is charged at its generator's bound and written to the ledger, synced,
before it is sent; a request whose cost is not known is kept at its bound, since nothing is
ever settled lower. A request that would take the ledger past its allowance is not sent.

This spend is outside every store's ledger: the milestone's cap counts the allowance whole.
"""

from __future__ import annotations

import datetime as dt
import fcntl
import json
import os
import tomllib
from pathlib import Path

from .build import outside_every_repository

PRICES_FILE = Path(__file__).resolve().parents[1] / "prices.toml"
LEDGER = "whizbang-sets-spend/1"
#: The most an allowance may be, in US dollars: the milestone's own cap.
CAP_LIMIT_USD = 100


class SpendRefused(RuntimeError):
    """A request the guard did not let go: nothing was sent."""


def _micros(usd, what: str) -> int:
    if isinstance(usd, bool) or not isinstance(usd, (int, float)):
        raise ValueError(f"{what} is an amount in US dollars")
    micros = round(usd * 1_000_000)
    if micros <= 0 or micros / 1_000_000 != usd:
        raise ValueError(f"{what} must be more than 0, with at most six decimal places")
    return micros


def prices(path: Path = PRICES_FILE) -> dict[str, int]:
    """Each generator's bound per picture, in millionths of a dollar. A row is a generator's
    table with `usd_per_picture` and `checked_on`, the date it was read from the provider."""
    with open(path, "rb") as f:
        doc = tomllib.load(f)
    out: dict[str, int] = {}
    for name, row in doc.get("generator", {}).items():
        where = f"prices: generator {name}"
        if not isinstance(row, dict) or set(row) != {"usd_per_picture", "checked_on"}:
            raise ValueError(f"{where} has usd_per_picture and checked_on, and nothing else")
        if not isinstance(row["checked_on"], dt.date):
            raise ValueError(f"{where}: checked_on is a date")
        out[name] = _micros(row["usd_per_picture"], f"{where}: usd_per_picture")
    return out


def _sync_folder(folder: Path) -> None:
    fd = os.open(folder, os.O_RDONLY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


class Ledger:
    """The ledger file: a first line that holds the allowance, then one line per request."""

    def __init__(self, path, cap_usd=None, clock=None):
        self.path = outside_every_repository(path, "--ledger")
        self.clock = clock or (lambda: dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"))
        cap = None if cap_usd is None else _micros(cap_usd, "--cap-usd")
        if cap is not None and cap > CAP_LIMIT_USD * 1_000_000:
            raise ValueError(f"--cap-usd is at most {CAP_LIMIT_USD}")
        if not self.path.exists():
            if cap is None:
                raise ValueError("a new ledger needs its allowance: --cap-usd, set once")
            self.path.parent.mkdir(parents=True, exist_ok=True)
            head = {"ledger": LEDGER, "cap_micros": cap, "created": self.clock()}
            with open(self.path, "x", encoding="utf-8") as f:
                f.write(json.dumps(head) + "\n")
                f.flush()
                os.fsync(f.fileno())
            _sync_folder(self.path.parent)
        self.cap, _ = self._read()
        if cap is not None and cap != self.cap:
            raise ValueError("the ledger's allowance is set once; --cap-usd names another")

    def _read(self) -> tuple[int, int]:
        """(the allowance, what the ledger holds), in millionths."""
        with open(self.path, encoding="utf-8") as f:
            lines = f.read().split("\n")
        if lines[-1] != "":
            raise ValueError("the ledger's last line is not whole; it is not used until mended")
        try:
            head = json.loads(lines[0])
            entries = [json.loads(line) for line in lines[1:-1]]
            if head.get("ledger") != LEDGER:
                raise ValueError
            held = sum(int(e["micros"]) for e in entries)
            return int(head["cap_micros"]), held
        except (ValueError, KeyError, TypeError, AttributeError):
            raise ValueError("the ledger is not one this program wrote") from None

    def held(self) -> int:
        return self._read()[1]

    def charge(self, generator: str, micros: int, to: str) -> None:
        """Writes one request's bound to the ledger, synced, or refuses it when it would pass
        the allowance. Held under a lock on the file, so two builds share it."""
        with open(self.path, "a", encoding="utf-8") as f:
            fcntl.flock(f, fcntl.LOCK_EX)
            try:
                cap, held = self._read()
                if held + micros > cap:
                    raise SpendRefused(
                        f"the request was not sent: the ledger holds {held / 1e6:.6f} of its "
                        f"{cap / 1e6:.6f} dollars and the request is bound at {micros / 1e6:.6f}"
                    )
                entry = {"at": self.clock(), "generator": generator, "to": to, "micros": micros}
                f.write(json.dumps(entry) + "\n")
                f.flush()
                os.fsync(f.fileno())
            finally:
                fcntl.flock(f, fcntl.LOCK_UN)

    def said(self) -> str:
        cap, held = self._read()
        return f"the ledger holds {held / 1e6:.6f} of its {cap / 1e6:.6f} dollars"


class Guard:
    """The charge each generator makes before each attempt of each request."""

    def __init__(self, ledger: Ledger, table: dict[str, int]):
        self.ledger = ledger
        self.table = table

    def charge(self, generator: str, recipient) -> None:
        """Charges one attempt of a request of `generator`, or refuses it: a generator with no
        dated bound price is never used live."""
        if generator not in self.table:
            raise SpendRefused(
                f"no dated bound price for the generator {generator}; the orchestrator reads it "
                "from the provider's page into prices.toml first"
            )
        self.ledger.charge(generator, self.table[generator], recipient.shown())
