"""What the builder's tests share. They run under the pack's runner
(`python -I -B pack/tests/run.py tools/sets/tests`, from scripts/tests/test-sets.sh), which
puts this folder first on the path; this puts the builder's own folder after it."""

import io
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
TOOL = os.path.dirname(HERE)
REPO = os.path.dirname(os.path.dirname(TOOL))
PLANS = os.path.join(HERE, "plans")
if TOOL not in sys.path:
    sys.path.insert(1, TOOL)

from PIL import Image  # noqa: E402


def png(im) -> bytes:
    buf = io.BytesIO()
    im.save(buf, format="PNG")
    return buf.getvalue()


def load(b: bytes):
    return Image.open(io.BytesIO(b)).convert("RGB")


def plan_text(name: str) -> str:
    with open(os.path.join(PLANS, name + ".plan.toml"), encoding="utf-8") as f:
        return f.read()


class Scratch:
    """A folder of the system's temp, outside every repository, removed at the end."""

    def __enter__(self):
        self._dir = tempfile.TemporaryDirectory()
        return os.path.realpath(self._dir.name)

    def __exit__(self, *exc):
        self._dir.cleanup()
        return False
