"""Checked egress for the generators (`SPEC.md` section 10; plan mE, task B1, item 2).

Squint's generators called `urllib` directly and fetched whatever address fal answered
with. Here a request goes only to an origin that the whizbang config approves as a
recipient (`[egress.recipients]`) for every class of data the request carries: a prompt is
`prompts`; a picture given to a generator as its input, as it is, is `source_images`. The
check is made before anything is sent. A download goes only to an approved recipient's
origin, or to an upstream of the recipient the request went to; it carries no key. A
redirect is followed only to an origin the same rule allows, each hop checked, and a key
never follows one: it is sent only on the first hop, to the origin it is for. No proxy is
used, whatever the environment says.

The config is read for its recipients alone, as `whizbang` reads them
(`crates/whizbang-core/src/config/read.rs`): an origin, its upstreams and its classes.
"""

from __future__ import annotations

import sys
import time
import tomllib
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from typing import Callable

CLASSES = ("source_images", "derived_images", "prompts", "evidence")
DEFAULT_PORTS = {"https": 443, "http": 80}
#: The most redirects one request follows.
HOPS = 5
#: The most bytes of one answer that are read: a picture is far smaller.
ANSWER_LIMIT = 64 * 1024 * 1024
#: Statuses worth another try: rate limits and the provider's own passing failures.
RETRY_STATUSES = frozenset({408, 429, 500, 502, 503, 504})
#: Seconds to wait before each retry: a long build outlasts a short outage.
RETRY_WAITS = (5, 20, 60, 120, 240)


class Refused(RuntimeError):
    """A request, a download or a redirect the config does not allow; nothing was sent."""


class GeneratorError(RuntimeError):
    """A provider's failure. Its text names the call and the status alone, never what the
    provider answered; `detail` keeps that, for the construction record in owner storage."""

    def __init__(self, message: str, status: int | None = None, detail: str = ""):
        super().__init__(message)
        self.status = status
        self.detail = detail


ADDRESS_LIMIT = 2048
HOST_LIMIT = 253
LABEL_LIMIT = 63
ACCOUNT_LIMIT = 128


def _plainly_numbered(host: str) -> bool:
    """A host whose last label is a number is taken only as four decimal parts of 0 to 255
    with no leading zero, as `Origin::of` takes it."""
    last = host.rsplit(".", 1)[-1]
    hexed = last[2:] if last.startswith("0x") else None
    number = (last.isascii() and last.isdigit()) or (
        hexed is not None and all(c in "0123456789abcdef" for c in hexed)
    )
    if not number:
        return True
    parts = host.split(".")

    def part(p: str) -> bool:
        return p.isascii() and p.isdigit() and len(p) <= 3 and (p == "0" or not p.startswith("0")) and int(p) <= 255

    return len(parts) == 4 and all(part(p) for p in parts)


def _parse(text: str, query: bool) -> tuple[str, str]:
    """(origin, the rest) of an address, by the rules of whizbang's `Origin::of` and
    `Address::of` (`crates/whizbang-core/src/egress/recipient.rs`): printable ASCII with no
    space and no backslash, at most 2048 bytes; no `@` and no `#` anywhere; a query only
    after a path, and only when `query`; the scheme http or https; the host labels of
    letters, digits and hyphens; the port written plainly."""
    refused = Refused("an address that is not a plain http or https address was refused")
    if not isinstance(text, str) or len(text) > ADDRESS_LIMIT:
        raise refused
    if not all(0x21 <= ord(c) <= 0x7E and c != "\\" for c in text):
        raise refused
    if "@" in text or "#" in text or (not query and "?" in text):
        raise refused
    scheme, sep, after = text.partition("://")
    scheme = scheme.lower()
    if not sep or scheme not in DEFAULT_PORTS:
        raise refused
    end = after.find("/")
    authority, rest = (after, "") if end < 0 else (after[:end], after[end:])
    host, colon, port_text = authority.partition(":")
    host = host.lower()

    def label(lab: str) -> bool:
        return (
            1 <= len(lab) <= LABEL_LIMIT
            and all(c.isascii() and (c.isalnum() or c == "-") for c in lab)
            and not lab.startswith("-")
            and not lab.endswith("-")
        )

    if len(host) > HOST_LIMIT or not all(label(lab) for lab in host.split(".")) or not _plainly_numbered(host):
        raise refused
    port = DEFAULT_PORTS[scheme]
    if colon:
        plain = (
            0 < len(port_text) <= 5
            and port_text.isascii()
            and port_text.isdigit()
            and not port_text.startswith("0")
        )
        if not plain or not 1 <= int(port_text) <= 65535:
            raise refused
        port = int(port_text)
    origin = f"{scheme}://{host}" if port == DEFAULT_PORTS[scheme] else f"{scheme}://{host}:{port}"
    return origin, rest


def origin_of(url: str) -> str:
    """The origin of an address that may carry a path and a query after it."""
    return _parse(url, query=True)[0]


def config_origin(text: str) -> str:
    """An origin as a config gives one: nothing after it."""
    origin, rest = _parse(text, query=False)
    if rest != "":
        raise Refused("an origin in the config has something after it")
    return origin


def _account(text) -> str:
    """A recipient's account, as `Account::new` takes it: 1 to 128 printable ASCII
    characters with no space, no `"` and no backslash."""
    plain = isinstance(text, str) and 0 < len(text) <= ACCOUNT_LIMIT
    if not plain or not all(0x21 <= ord(c) <= 0x7E and c not in '"\\' for c in text):
        raise ValueError("a recipient's account is 1 to 128 printable characters with no space")
    return text


@dataclass(frozen=True)
class Recipient:
    name: str
    origin: str
    upstreams: frozenset[str]
    classes: frozenset[str]
    account: str | None = None

    def reaches(self, origin: str) -> bool:
        return origin == self.origin or origin in self.upstreams

    def shown(self) -> str:
        """The recipient as a record names it: its origin, and its account cut to four."""
        if self.account is None:
            return self.origin
        cut = self.account[:4] + ("..." if len(self.account) > 4 else "")
        return f"{self.origin} as {cut}"


class Approved:
    """The recipients a whizbang config approves."""

    def __init__(self, recipients: list[Recipient]):
        self.recipients = recipients

    @classmethod
    def from_config(cls, path) -> "Approved":
        with open(path, "rb") as f:
            doc = tomllib.load(f)
        table = doc.get("egress", {}).get("recipients", {})
        out: list[Recipient] = []
        for name, r in table.items():
            where = f"[egress.recipients.{name}]"
            if not isinstance(r, dict) or "origin" not in r or "classes" not in r:
                raise ValueError(f"{where}: a recipient has an origin and its classes")
            unknown_keys = sorted(set(r) - {"origin", "account", "upstreams", "classes", "probe"})
            if unknown_keys:
                raise ValueError(f"{where}: unknown key(s) {', '.join(unknown_keys)}")
            classes = list(r["classes"])
            unknown = [c for c in classes if c not in CLASSES]
            if unknown or len(set(classes)) != len(classes):
                raise ValueError(f"{where}: a class that is not known, or one twice")
            try:
                origin = config_origin(r["origin"])
                upstreams = frozenset(config_origin(u) for u in r.get("upstreams", []))
            except Refused:
                raise ValueError(f"{where}: an origin that is not plain") from None
            account = None if "account" not in r else _account(r["account"])
            if any(o.origin == origin and o.account == account for o in out):
                raise ValueError(f"{where}: the same origin and account as another recipient")
            out.append(Recipient(name, origin, upstreams, frozenset(classes), account))
        return cls(out)

    def recipient_for(self, url: str, classes: tuple[str, ...], account: str | None = None) -> Recipient:
        """The recipient at the address's origin that may receive every one of `classes`, of
        `account` when one is named. Where the config approves several accounts at the origin,
        one must be named: which is meant is never guessed."""
        origin = origin_of(url)
        at = [r for r in self.recipients if r.origin == origin]
        if account is not None:
            at = [r for r in at if r.account == account]
        elif len(at) > 1:
            raise Refused(f"the config approves several accounts at {origin}; name one with --account")
        for r in at:
            if set(classes) <= r.classes:
                return r
        raise Refused(
            f"{origin} is not approved by the config as a recipient of {', '.join(classes)} "
            "([egress.recipients]); nothing was sent"
        )

    @staticmethod
    def may_download(origin: str, via: Recipient) -> bool:
        """A picture is downloaded only from the recipient asked, or one of its upstreams."""
        return via.reaches(origin)


class _Hops(urllib.request.HTTPRedirectHandler):
    """Follows a redirect only to an origin `allowed` takes; refuses any other before it is
    asked. A key is an unredirected header, so it is never carried to the next hop."""

    max_redirections = HOPS
    max_repeats = 2

    def __init__(self, allowed: Callable[[str], bool]):
        self.allowed = allowed

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        origin = origin_of(urllib.parse.urljoin(req.full_url, newurl))
        if not self.allowed(origin):
            raise Refused(f"a redirect to {origin}, which the config does not approve, was refused")
        return super().redirect_request(req, fp, code, msg, headers, newurl)


class Sender:
    """Every request and download of the generators. `opener` makes the opener of one
    request from its handlers (a test swaps it); `sleep` waits between tries."""

    def __init__(self, approved: Approved, sleep=time.sleep, opener=urllib.request.build_opener, account=None):
        self.approved = approved
        self.account = account
        self.sleep = sleep
        self.opener = opener

    def post(
        self,
        url: str,
        body: bytes,
        headers: dict[str, str],
        credentials: dict[str, str],
        classes: tuple[str, ...],
        what: str,
        charge: Callable[[Recipient], None] | None = None,
    ) -> tuple[bytes, Recipient]:
        """The answer of a POST to `url`, with the recipient it went to. `charge` is called
        before each attempt, a retry included, and may refuse it (the spend guard). A POST
        with no charge is refused: nothing is sent unguarded."""
        if charge is None:
            raise Refused("a request with no charge to the spend guard was refused; nothing was sent")
        recipient = self.approved.recipient_for(url, classes, self.account)
        req = urllib.request.Request(url, data=body, headers=headers, method="POST")
        for k, v in credentials.items():
            req.add_unredirected_header(k, v)
        allowed = lambda origin: recipient.reaches(origin)  # noqa: E731
        before = lambda: charge(recipient)  # noqa: E731
        return self._open(req, allowed, what, before), recipient

    def get(self, url: str, via: Recipient, what: str) -> bytes:
        """A generated picture at `url`, with no key."""
        allowed = lambda origin: self.approved.may_download(origin, via)  # noqa: E731
        if not allowed(origin_of(url)):
            raise Refused(
                f"a download from {origin_of(url)}, which the config does not approve, was refused"
            )
        return self._open(urllib.request.Request(url), allowed, what)

    def _open(self, req, allowed, what: str, before: Callable[[], None] | None = None) -> bytes:
        """The answer's body, tried again on a passing failure (a status in
        `RETRY_STATUSES`, a dropped connection, a timeout) after each of `RETRY_WAITS`; any
        other failure, or the last passing one, is a `GeneratorError` naming `what`."""
        opener = self.opener(urllib.request.ProxyHandler({}), _Hops(allowed))
        for attempt in range(len(RETRY_WAITS) + 1):
            if before is not None:
                before()
            try:
                with opener.open(req, timeout=300) as r:
                    body = r.read(ANSWER_LIMIT + 1)
                if len(body) > ANSWER_LIMIT:
                    raise GeneratorError(f"{what}: the answer is over {ANSWER_LIMIT} bytes")
                return body
            except urllib.error.HTTPError as e:
                detail = e.read(400).decode("utf-8", "replace")
                if e.code not in RETRY_STATUSES or attempt == len(RETRY_WAITS):
                    raise GeneratorError(f"{what}: HTTP {e.code}", e.code, detail) from None
                reason = f"HTTP {e.code}"
            except (urllib.error.URLError, TimeoutError, ConnectionError) as e:
                # A refused redirect is no URLError: `Refused` passes through untried.
                if attempt == len(RETRY_WAITS):
                    raise GeneratorError(f"{what}: {type(e).__name__}", None, str(e)) from None
                reason = type(e).__name__
            wait = RETRY_WAITS[attempt]
            print(f"{what}: {reason}, trying again in {wait}s", file=sys.stderr, flush=True)
            self.sleep(wait)
        raise AssertionError("unreachable")
