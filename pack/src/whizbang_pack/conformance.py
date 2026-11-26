"""The conformance suite a model's adapter is held to (plan 1D, tasks T1 and U1): for whoever
writes an adapter, the pack's own and a user's alike.

    python -m whizbang_pack.conformance <adapter.py> <folder of the model's files>

It reads the adapter's file once, as bytes, works out their SHA-256 (the hash `whizbang
models adapter add` names it by) and runs those bytes, as the worker runs a user's
adapter: a module whose class `ADAPTER` is the adapter. It loads the adapter over the
files of the folder, as an installation's files, by their names; and asks it, through the
worker's own handling of a call, each feature it declares alone and then all of them
together, about a picture of eight by six pixels. Each answer is held to the contract of
`pack/1` (ADR 0020) and of the core's answer ([`check`]). An adapter that declares `labels`
(ADR 0053) is asked a phrase that is its first class, and then every class of its list, in
batches of 64 at one instance each: the planes must be disjoint and cover the picture, a
class per pixel ([`check_labels`]). It prints a line for each run and
ends 0 when every run conforms, 1 when one does not, and 2 for a command line it does not
take. Nothing is installed, nothing is written but a temporary folder of its own, removed
when it ends, and nothing is fetched.

The adapter is the user's own code, run in this process: run this only on a file you chose.
"""

import hashlib
import io
import os
import shutil
import sys
import tempfile

from whizbang_pack import installed, proto
from whizbang_pack import worker as _worker
from whizbang_pack.adapter import FEATURES, LABELS

WIDTH, HEIGHT = 8, 6
BINDING = "sha256:" + "92b2fa58028958317e408bd84ecfa70f5ee35b121991dbc232c49d166353708b"
# The model's name and identity the adapter is loaded under: no shipped model's name.
MODEL = "conformance"
IDENTITY = "sha256:" + "c0" * 32
# What each feature asks: a phrase, a point inside the picture and a box inside it.
QUERIES = {"phrase": {"phrase": "a red thing"}, "point": {"point": [5.5, 2.25]}, "box": {"box": [1.0, 1.0, 3.0, 4.0]}}
MOST = 2
# The most queries one request holds (ADR 0020).
QUERIES_MOST = 64


class Nonconforming(Exception):
    """An answer that breaks the contract, with a sentence that says how."""


def picture():
    """Eight by six: red 255 in the left half and 0 in the right."""
    samples = bytearray()
    for _y in range(HEIGHT):
        for x in range(WIDTH):
            samples += bytes((255 if x < WIDTH // 2 else 0, 10, 20, 255))
    return bytes(samples)


def _hold(true, sentence):
    if not true:
        raise Nonconforming(sentence)


def check(request, result):
    """Holds a worker's result for a segment request to the contract of `pack/1` (ADR 0020)
    and of the core's answer, whatever model gave it: at most the request's
    `max_instances` for each query; each instance of a query asked, its score and its
    predicted IoU (or `None`) from 0 to 1, its plane the picture's size with samples 0 and
    255 and something on, and its box the bounds of what its plane has on; and the
    instances in the fixed order (ADR 0020, "The fixed order of an answer"). Raises
    `Nonconforming` at the first thing that breaks it."""
    image = request["image"]
    width, height = image.width, image.height
    _hold(result.get("type") == "result", "the answer is no result: %s" % result.get("fault", result.get("type")))
    _hold((result["id"], result["kind"]) == (request["id"], "segment"), "the result is not the request's")
    instances = result["instances"]
    queries = range(len(request["queries"]))
    for place in queries:
        count = sum(1 for item in instances if item["query"] == place)
        _hold(count <= request["max_instances"], "more instances of a query than the request allows")
    keys = []
    for item in instances:
        _hold(item["query"] in queries, "an instance of a query that was not asked")
        _hold(0.0 <= item["score"] <= 1.0, "a score outside 0 to 1")
        _hold(item["iou"] is None or 0.0 <= item["iou"] <= 1.0, "a predicted IoU outside 0 to 1")
        plane = item["plane"]
        _hold((plane.width, plane.height, plane.binding) == (width, height, image.binding), "a plane not of the picture's size")
        samples = bytes(plane.samples)
        _hold(len(samples) == width * height, "a plane of the wrong length")
        _hold(set(samples) <= {0, 255}, "a plane with samples other than 0 and 255")
        on = [(at % width, at // width) for at, sample in enumerate(samples) if sample == 255]
        _hold(on, "an instance with nothing on")
        xs, ys = [x for x, _ in on], [y for _, y in on]
        bounds = [float(min(xs)), float(min(ys)), float(max(xs) + 1), float(max(ys) + 1)]
        _hold(item["box"] == bounds, "a box that is not the bounds of its plane")
        box = item["box"]
        keys.append((-item["score"], item["query"], box[1], box[0], box[2], box[3]))
    _hold(keys == sorted(keys), "the instances are not in the fixed order")


def requests(features, phrase=None):
    """The segment requests the suite asks: each declared feature alone, in the order of
    `FEATURES`, then all of them together when there are two or more. The phrase asked is
    `phrase` when one is given: a fixed-class segmenter's first class."""
    queries = dict(QUERIES)
    if phrase is not None:
        queries["phrase"] = {"phrase": phrase}
    declared = [feature for feature in FEATURES if feature in features]
    asked = [[feature] for feature in declared]
    if len(declared) > 1:
        asked.append(declared)
    made = []
    for number, kinds in enumerate(asked):
        made.append(
            (
                kinds,
                {
                    "type": "request",
                    "id": "c%d" % (number + 2),
                    "kind": "segment",
                    "model": MODEL,
                    "installation": IDENTITY,
                    "image": proto.Payload(WIDTH, HEIGHT, BINDING, picture()),
                    "queries": [queries[kind] for kind in kinds],
                    "max_instances": MOST,
                },
            )
        )
    return made


def labels_requests(classes):
    """The requests that ask every class of a list: batches of at most 64 phrases, at one
    instance each, in the list's order."""
    made = []
    for start in range(0, len(classes), QUERIES_MOST):
        made.append(
            {
                "type": "request",
                "id": "l%d" % (start // QUERIES_MOST + 1),
                "kind": "segment",
                "model": MODEL,
                "installation": IDENTITY,
                "image": proto.Payload(WIDTH, HEIGHT, BINDING, picture()),
                "queries": [{"phrase": name} for name in classes[start : start + QUERIES_MOST]],
                "max_instances": 1,
            }
        )
    return made


def check_labels(answered):
    """Holds the answers to every class of a fixed-class segmenter's list (`labels_requests`)
    to `labels` (ADR 0053): each answer as `check` holds it, at most one instance a class, and
    the planes of all of them disjoint and together the whole picture."""
    covered = None
    for request, result in answered:
        check(request, result)
        for item in result["instances"]:
            samples = bytes(item["plane"].samples)
            if covered is None:
                covered = bytearray(len(samples))
            for at, sample in enumerate(samples):
                if sample:
                    _hold(not covered[at], "two classes label one pixel")
                    covered[at] = 1
    _hold(covered is not None and all(covered), "a pixel no class labels")


def _through(worker, request):
    """The frame the worker answers this request with, read back as whizbang reads it."""
    chunks = worker._answer(_worker.Call(worker, request))
    stream = io.BytesIO(b"".join(bytes(chunk) for chunk in chunks))
    return proto.Reader(stream, {request["id"]: proto.awaiting(request)}).read()


def run(make, files):
    """Runs the suite on the adapter `make` makes, over the files of this folder: a line for
    each run, and whether all conformed."""
    lines, whole = [], True
    scratch = tempfile.mkdtemp(prefix="whizbang-conformance-")
    try:
        folder = os.path.join(scratch, "models", MODEL, "installations", IDENTITY[len("sha256:") :])
        os.makedirs(folder)
        # The folder's files, as an installation's: a link to the folder, so that weights
        # are not copied; the installation has no record, so the worker takes the adapter
        # it is given for the model.
        os.symlink(os.path.abspath(files), os.path.join(folder, "files"))
        worker = _worker.Worker(scratch, None, None, io.BytesIO(), io.StringIO(), exit=lambda code: None, adapters={MODEL: make})
        load = {"type": "request", "id": "c1", "kind": "load", "model": MODEL, "installation": IDENTITY, "settings": {}}
        loaded = _through(worker, load)
        if loaded.get("type") != "result":
            return ["load: does not conform: %s" % loaded.get("sentence", "no result")], False
        features = worker.resident.adapter.features
        if not features:
            return ["load: does not conform: the adapter declares no feature"], False
        classes = tuple(worker.resident.adapter.classes) if LABELS in features else ()
        if LABELS in features and not classes:
            return ["load: does not conform: the adapter declares labels and no classes"], False
        for kinds, request in requests(features, classes[0] if classes else None):
            try:
                check(request, _through(worker, request))
                lines.append("%s: conforms" % " and ".join(kinds))
            except Nonconforming as nonconforming:
                whole = False
                lines.append("%s: does not conform: %s" % (" and ".join(kinds), nonconforming))
        if classes:
            try:
                check_labels([(request, _through(worker, request)) for request in labels_requests(classes)])
                lines.append("labels: conforms")
            except Nonconforming as nonconforming:
                whole = False
                lines.append("labels: does not conform: %s" % nonconforming)
    finally:
        shutil.rmtree(scratch, ignore_errors=True)
    return lines, whole


def main(argv=None, out=None):
    argv = sys.argv[1:] if argv is None else argv
    out = sys.stdout if out is None else out
    if len(argv) != 2 or not os.path.isfile(argv[0]) or not os.path.isdir(argv[1]):
        out.write("usage: python -m whizbang_pack.conformance <adapter.py> <folder of the model's files>\n")
        return 2
    with open(argv[0], "rb") as file:
        data = file.read(installed.ADAPTER_MAX_BYTES + 1)
    if len(data) > installed.ADAPTER_MAX_BYTES:
        out.write("the adapter is longer than %d bytes\n" % installed.ADAPTER_MAX_BYTES)
        return 1
    revision = hashlib.sha256(data).hexdigest()
    out.write("adapter sha256:%s\n" % revision)
    lines, whole = run(installed.compiled(data, revision, os.path.abspath(argv[0])), argv[1])
    for line in lines:
        out.write(line + "\n")
    out.write("conforms\n" if whole else "does not conform\n")
    return 0 if whole else 1


if __name__ == "__main__":
    sys.exit(main())
