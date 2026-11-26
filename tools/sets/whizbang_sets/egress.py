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
    pass


def origin_of(url: str) -> str:
    """The origin of an address: its scheme and host in lower case, and its port unless it
    is the scheme's own. Anything but http and https, or an address with a user, is refused."""
    parts = urllib.parse.urlsplit(url)
    scheme = parts.scheme.lower()
    if scheme not in DEFAULT_PORTS or not parts.hostname or "@" in parts.netloc:
        raise Refused(f"{url!r} is not an address of an origin over http or https")
    try:
        port = parts.port
    except ValueError:
        raise Refused(f"{url!r} has a port that is not a number") from None
    host = parts.hostname.lower()
    if ":" in host:
        host = f"[{host}]"
    if port is None or port == DEFAULT_PORTS[scheme]:
        return f"{scheme}://{host}"
    return f"{scheme}://{host}:{port}"


@dataclass(frozen=True)
class Recipient:
    name: str
    origin: str
    upstreams: frozenset[str]
    classes: frozenset[str]

    def reaches(self, origin: str) -> bool:
        return origin == self.origin or origin in self.upstreams


class Approved:
    """The recipients a whizbang config approves."""

    def __init__(self, recipients: list[Recipient]):
        self.recipients = recipients

    @classmethod
    def from_config(cls, path) -> "Approved":
        with open(path, "rb") as f:
            doc = tomllib.load(f)
        table = doc.get("egress", {}).get("recipients", {})
        out = []
        for name, r in table.items():
            where = f"[egress.recipients.{name}]"
            if not isinstance(r, dict) or "origin" not in r or "classes" not in r:
                raise ValueError(f"{where}: a recipient has an origin and its classes")
            classes = list(r["classes"])
            unknown = [c for c in classes if c not in CLASSES]
            if unknown:
                raise ValueError(f"{where}: unknown class(es) {', '.join(map(str, unknown))}")
            out.append(
                Recipient(
                    name=name,
                    origin=origin_of(r["origin"]),
                    upstreams=frozenset(origin_of(u) for u in r.get("upstreams", [])),
                    classes=frozenset(classes),
                )
            )
        return cls(out)

    def recipient_for(self, url: str, classes: tuple[str, ...]) -> Recipient:
        """The recipient at the address's origin that may receive every one of `classes`."""
        origin = origin_of(url)
        for r in self.recipients:
            if r.origin == origin and set(classes) <= r.classes:
                return r
        raise Refused(
            f"{origin} is not approved by the config as a recipient of {', '.join(classes)} "
            "([egress.recipients]); nothing was sent"
        )

    def may_download(self, origin: str, via: Recipient) -> bool:
        return via.reaches(origin) or any(r.origin == origin for r in self.recipients)


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

    def __init__(self, approved: Approved, sleep=time.sleep, opener=urllib.request.build_opener):
        self.approved = approved
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
    ) -> tuple[bytes, Recipient]:
        """The answer of a POST to `url`, with the recipient it went to."""
        recipient = self.approved.recipient_for(url, classes)
        req = urllib.request.Request(url, data=body, headers=headers, method="POST")
        for k, v in credentials.items():
            req.add_unredirected_header(k, v)
        allowed = lambda origin: recipient.reaches(origin)  # noqa: E731
        return self._open(req, allowed, what), recipient

    def get(self, url: str, via: Recipient, what: str) -> bytes:
        """A generated picture at `url`, with no key."""
        allowed = lambda origin: self.approved.may_download(origin, via)  # noqa: E731
        if not allowed(origin_of(url)):
            raise Refused(
                f"a download from {origin_of(url)}, which the config does not approve, was refused"
            )
        return self._open(urllib.request.Request(url), allowed, what)

    def _open(self, req, allowed, what: str) -> bytes:
        """The answer's body, tried again on a passing failure (a status in
        `RETRY_STATUSES`, a dropped connection, a timeout) after each of `RETRY_WAITS`; any
        other failure, or the last passing one, is a `GeneratorError` naming `what`."""
        opener = self.opener(urllib.request.ProxyHandler({}), _Hops(allowed))
        for attempt in range(len(RETRY_WAITS) + 1):
            try:
                with opener.open(req, timeout=300) as r:
                    body = r.read(ANSWER_LIMIT + 1)
                if len(body) > ANSWER_LIMIT:
                    raise GeneratorError(f"{what}: the answer is over {ANSWER_LIMIT} bytes")
                return body
            except urllib.error.HTTPError as e:
                detail = e.read()[:400].decode("utf-8", "replace")
                if e.code not in RETRY_STATUSES or attempt == len(RETRY_WAITS):
                    raise GeneratorError(f"{what}: HTTP {e.code}: {detail}") from None
                reason = f"HTTP {e.code}"
            except (urllib.error.URLError, TimeoutError, ConnectionError) as e:
                # A refused redirect is no URLError: `Refused` passes through untried.
                if attempt == len(RETRY_WAITS):
                    raise GeneratorError(f"{what}: {e}") from None
                reason = type(e).__name__
            wait = RETRY_WAITS[attempt]
            print(f"{what}: {reason}, trying again in {wait}s", file=sys.stderr, flush=True)
            self.sleep(wait)
        raise AssertionError("unreachable")
