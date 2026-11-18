"""What the pack's tests share: the vectors, pipes read with a deadline, and the child's
environment. Not a test file: `run.py` finds only `test_*.py`."""

import hashlib
import io
import json
import os
import select
import time

HERE = os.path.dirname(os.path.abspath(__file__))
PACK = os.path.dirname(HERE)
SRC = os.path.join(PACK, "src")
VECTORS = os.path.join(PACK, "conformance", "vectors")

# Every wait in a test is bounded by this, far under the runner's 60 seconds; what is
# waited for takes a fraction of a second.
WAIT_SECONDS = 5.0

# The variables `scripts/test-pack.sh` hands on to the runner, and so the only ones a
# child of a test is given.
ALLOWED = ("PATH", "HOME", "TMPDIR", "UV_PYTHON_INSTALL_DIR")


def vectors():
    """Every vector, by name, in the order of their names."""
    found = []
    for name in sorted(os.listdir(VECTORS)):
        if name.endswith(".json"):
            with open(os.path.join(VECTORS, name), "rb") as file:
                found.append(json.loads(file.read()))
    return found


def open_table(vector):
    """The requests a vector's reader has open, by id."""
    return {entry["id"]: dict(entry) for entry in vector["open"]}


def described(value):
    """A message as a vector's `message` states it: each payload by its size and digest."""
    from whizbang_pack.proto import Payload

    if isinstance(value, Payload):
        return {
            "width": value.width,
            "height": value.height,
            "binding": value.binding,
            "bytes": len(value.samples),
            "sha256": hashlib.sha256(value.samples).hexdigest(),
        }
    if isinstance(value, dict):
        return {key: described(item) for key, item in value.items()}
    if isinstance(value, list):
        return [described(item) for item in value]
    return value


def conforming(test, request, result):
    """Holds a worker's result for a segment request to the contract of `pack/1` (ADR 0020)
    and of the core's answer, whatever model gave it: the pack's own conformance suite
    (`whizbang_pack.conformance.check`, which a user's adapter is given too; plan 1D,
    task U1), failing the test at the first thing that breaks it."""
    from whizbang_pack import conformance

    try:
        conformance.check(request, result)
    except conformance.Nonconforming as nonconforming:
        test.fail(str(nonconforming))


def child_environment():
    return {name: os.environ[name] for name in ALLOWED if name in os.environ}


class FdStream(io.RawIOBase):
    """The read end of a pipe, read with a deadline: a read that would wait past it raises
    TimeoutError, so no test blocks for ever on a worker that says nothing."""

    def __init__(self, fd, seconds=WAIT_SECONDS):
        super().__init__()
        self.fd = fd
        self.deadline = time.monotonic() + seconds

    def readable(self):
        return True

    def readinto(self, buffer):
        left = self.deadline - time.monotonic()
        if left <= 0:
            raise TimeoutError("nothing came within the test's wait")
        ready, _, _ = select.select([self.fd], [], [], left)
        if not ready:
            raise TimeoutError("nothing came within the test's wait")
        data = os.read(self.fd, len(buffer))
        buffer[: len(data)] = data
        return len(data)

    def at_end(self):
        """True when the writer has closed its end and nothing is left to read."""
        return self.read(1) == b""
