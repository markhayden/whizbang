"""The fake models (docs/plans/m1c-local-pack.md, D3, and `fake-mps` of task P7a),
answered by model name. They take no third-party package and open no file. The shipped
catalogue names none of them.

| Model | What it does |
|---|---|
| `fake-ok` | Answers a segment request with the masks the request determines (`FakeOk`) |
| `fake-mps` | Answers as `fake-ok` does, and names the device `mps` in each result |
| `fake-wait` | Says it is waiting, and waits until its call is cancelled |
| `fake-deaf` | Says it is waiting, and waits, ignoring a cancel, until the worker ends |
| `fake-exit` | Ends the worker's process inside the call, with `EXIT_CODE` |
| `fake-big` | Answers with one instance more than its request allows for a query |
| `fake-bad` | Answers with a plane whose stated length is one byte more than its entry gives |
"""

from whizbang_pack.adapter import Adapter, Found

# The code `fake-exit` ends the process with.
EXIT_CODE = 4


def _plane_of(on, width, height):
    """The samples of a plane and the box of what is on, from the set of pixels on; or
    `None` when nothing is."""
    if not on:
        return None
    samples = bytearray(width * height)
    for x, y in on:
        samples[y * width + x] = 255
    xs = [x for x, _ in on]
    ys = [y for _, y in on]
    box = [float(min(xs)), float(min(ys)), float(max(xs) + 1), float(max(ys) + 1)]
    return bytes(samples), box


def determined(request):
    """What `fake-ok` answers, worked out from the request alone. One instance at most for
    each query, so never more than a request allows:

    - a phrase: every pixel whose red sample is 128 or more; score 0.7, no predicted IoU;
    - a point: the one pixel it is in, when that is inside the image; score 0.8, IoU 0.5;
    - a box: every pixel whose centre is inside it (left and top in, right and bottom out);
      score 0.9, IoU 0.75.

    A query that finds no pixel gives no instance. Each box is the bounds of what is on."""
    image = request["image"]
    width, height, samples = image.width, image.height, image.samples
    found = []
    for place, query in enumerate(request["queries"]):
        ((kind, given),) = query.items()
        if kind == "phrase":
            on = [(x, y) for y in range(height) for x in range(width) if samples[4 * (y * width + x)] >= 128]
            score, iou = 0.7, None
        elif kind == "point":
            x, y = int(given[0]), int(given[1])
            on = [(x, y)] if x < width and y < height else []
            score, iou = 0.8, 0.5
        else:
            left, top, right, bottom = given
            on = [
                (x, y)
                for y in range(height)
                for x in range(width)
                if left <= x + 0.5 < right and top <= y + 0.5 < bottom
            ]
            score, iou = 0.9, 0.75
        plane = _plane_of(on, width, height)
        if plane is not None:
            found.append(Found(place, score, iou, plane[1], plane[0]))
    return found


class FakeOk(Adapter):
    features = frozenset(("phrase", "point", "box"))
    revision = "fake-1"

    def __init__(self):
        self.loaded = False
        self.settings = None

    def load(self, installation, settings, call):
        self.loaded = True
        self.settings = dict(settings)
        return "loaded with %d settings" % len(settings)

    def segment(self, request, call):
        return determined(request), ""

    def unload(self, call):
        self.loaded = False
        return "unloaded"


class FakeMps(FakeOk):
    """`fake-ok`, on another device than the worker's own: as a model its runtime placed on
    the Apple GPU would be, so that a test can tell the device a result names from the one
    a hello names."""

    device = "mps"


class FakeWait(FakeOk):
    features = frozenset(("phrase",))

    def segment(self, request, call):
        call.event("waiting for a cancel")
        call.wait_for_cancel()
        raise AssertionError("wait_for_cancel returned")


class FakeDeaf(FakeOk):
    features = frozenset(("phrase",))

    def segment(self, request, call):
        call.event("waiting, deaf to a cancel")
        call.wait_for_stop()
        raise AssertionError("wait_for_stop returned")


class FakeExit(FakeOk):
    def segment(self, request, call):
        call.end_process(EXIT_CODE)
        raise AssertionError("end_process returned")


class FakeBig(FakeOk):
    def segment(self, request, call):
        image = request["image"]
        plane = bytes([255]) * (image.width * image.height)
        box = [0.0, 0.0, float(image.width), float(image.height)]
        count = request["max_instances"] + 1
        return [Found(0, 0.5, None, box, plane) for _ in range(count)], ""


class FakeBad(FakeOk):
    def segment(self, request, call):
        image = request["image"]
        plane = bytes([255]) * (image.width * image.height)
        return [Found(0, 0.5, None, [0.0, 0.0, float(image.width), float(image.height)], plane)], ""

    def spoil(self, chunks):
        # The eight bytes of the last plane's length state one byte more than its entry.
        stated = int.from_bytes(chunks[-2], "big")
        return chunks[:-2] + [(stated + 1).to_bytes(8, "big"), chunks[-1]]


MODELS = {
    "fake-ok": FakeOk,
    "fake-mps": FakeMps,
    "fake-wait": FakeWait,
    "fake-deaf": FakeDeaf,
    "fake-exit": FakeExit,
    "fake-big": FakeBig,
    "fake-bad": FakeBad,
}
