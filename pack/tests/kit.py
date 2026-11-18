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
    and of the core's answer, whatever model gave it: at most the request's
    `max_instances` for each query; each instance of a query asked, its score and its
    predicted IoU (or `None`) from 0 to 1, its plane the picture's size with samples 0 and
    255 and something on, and its box the bounds of what its plane has on; and the
    instances in the fixed order (ADR 0020, "The fixed order of an answer")."""
    image = request["image"]
    width, height = image.width, image.height
    test.assertEqual((result["type"], result["id"], result["kind"]), ("result", request["id"], "segment"))
    instances = result["instances"]
    for place in range(len(request["queries"])):
        test.assertLessEqual(sum(1 for item in instances if item["query"] == place), request["max_instances"])
    keys = []
    for item in instances:
        test.assertIn(item["query"], range(len(request["queries"])))
        test.assertTrue(0.0 <= item["score"] <= 1.0, item["score"])
        test.assertTrue(item["iou"] is None or 0.0 <= item["iou"] <= 1.0, item["iou"])
        plane = item["plane"]
        test.assertEqual((plane.width, plane.height, plane.binding), (width, height, image.binding))
        samples = bytes(plane.samples)
        test.assertEqual(len(samples), width * height)
        test.assertEqual(set(samples) - {0, 255}, set())
        on = [(at % width, at // width) for at, sample in enumerate(samples) if sample == 255]
        test.assertTrue(on, "an instance with nothing on")
        xs, ys = [x for x, _ in on], [y for _, y in on]
        test.assertEqual(item["box"], [float(min(xs)), float(min(ys)), float(max(xs) + 1), float(max(ys) + 1)])
        box = item["box"]
        keys.append((-item["score"], item["query"], box[1], box[0], box[2], box[3]))
    test.assertEqual(keys, sorted(keys))


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
