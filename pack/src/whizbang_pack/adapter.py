"""What a model's adapter is to the worker (docs/plans/m1c-local-pack.md, P2).

An adapter answers for one model. The worker makes one when a `load` names its model,
holds it while the model is resident, and drops it after `unload`. An adapter:

- declares `features`, the kinds of query it takes (`phrase`, `point`, `box`), and a
  `revision`, which names its code;
- `load(installation, settings, call)` makes the model ready from the installation's files
  and the settings a `load` carried, and returns its diagnostics (a text, which the worker
  cleans and cuts);
- `segment(request, call)` answers a segment request with a list of `Found` and its
  diagnostics: at most the request's `max_instances` for each query (the adapter cuts its
  candidates before it makes a full-size plane), in any order;
- `unload(call)` lets the model go, and returns its diagnostics;
- names its `device` once loaded, as its runtime names it.

`call` is the `Call` it is handed: it may send events through it, and it stops when its
call is cancelled (`Cancelled`). An adapter may raise `Fault` with one of the protocol's
fault names and a sentence; anything else it raises ends the call as `unavailable`, with
the name of the exception's type and nothing of its text.
"""

import os
import re

FEATURES = ("phrase", "point", "box")
_FILE_NAME = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.+-]{0,127}\Z")


class Cancelled(Exception):
    """The call was cancelled: raised by `Call.wait_for_cancel`, or by an adapter that saw
    `Call.cancelled()`."""


class Stopped(BaseException):
    """The worker is ending, inside a call. Not an `Exception`, so that an adapter's handler
    of its own failures does not take it."""


class Fault(Exception):
    """A call that failed for a reason the adapter can name: one of the protocol's eight
    fault names and a sentence, which the worker cleans."""

    def __init__(self, fault, sentence):
        super().__init__(sentence)
        self.fault = fault
        self.sentence = sentence


class Found:
    """One instance an adapter found: the place of its query, a score from 0 to 1, the
    predicted IoU or `None`, the box (left, top, right, bottom) and the plane's samples,
    one a pixel of the request's image, 255 on the thing and 0 off it."""

    __slots__ = ("query", "score", "iou", "box", "samples")

    def __init__(self, query, score, iou, box, samples):
        self.query = query
        self.score = score
        self.iou = iou
        self.box = box
        self.samples = samples


class Installation:
    """An installation of a model in the models directory, found by the model's name and
    the identity a `load` names (ADR 0017, "The directory"):
    `<models>/models/<model>/installations/<64 hex digits>/`. Both were held to the
    protocol's rules when the frame was read, so neither holds a separator."""

    def __init__(self, models_dir, model, identity):
        self.model = model
        self.identity = identity
        self.folder = os.path.join(models_dir, "models", model, "installations", identity[len("sha256:") :])

    def file(self, name):
        """The path of one of its files, by its name in the catalogue: one plain name."""
        if not _FILE_NAME.match(name):
            raise Fault("rejected", "a file's name is not one plain name")
        return os.path.join(self.folder, "files", name)


class Adapter:
    """The interface, answering nothing. A fake or a real model overrides what it does."""

    features = frozenset()
    revision = "0"
    device = "cpu"

    def load(self, installation, settings, call):
        return ""

    def segment(self, request, call):
        raise Fault("unsupported", "the model answers no segment request")

    def unload(self, call):
        return ""

    def spoil(self, chunks):
        """The bytes of a frame the worker is about to write for this adapter's call. Only a
        fake that answers with a malformed frame changes them."""
        return chunks


def runtime_device():
    """The device the worker's work runs on while no model is resident: what its hello
    states, and each result while nothing is loaded (plan 1C, task P7a). The pack's core
    imports no runtime (D10), so its work runs on the CPU, as `Adapter.device` says until a
    model's adapter names the device its runtime gave it."""
    return Adapter.device
