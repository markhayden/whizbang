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

An adapter of a fixed-class segmenter declares `labels` beside `phrase` (plan mE, task T2;
ADR 0053), and `classes`, its closed list of class names: it labels every pixel of a
picture with exactly one of them, answers a phrase only when it is one of them, with at
most one instance, the pixels so labelled, its score the class's confidence there, and
refuses any other phrase. So the planes of every class of one picture are disjoint and
cover it. `labels` is no kind of query: a request holds phrases, points and boxes alone.

`call` is the `Call` it is handed: it may send events through it, and it stops when its
call is cancelled (`Cancelled`). An adapter may raise `Fault` with one of the protocol's
fault names and a sentence; anything else it raises ends the call as `unavailable`, with
the name of the exception's type and nothing of its text.
"""

import os
import re
import sys

FEATURES = ("phrase", "point", "box")
# The feature of a fixed-class segmenter, which is no kind of query (ADR 0053).
LABELS = "labels"
# Where a model's adapter places it, by the setting `device` (plan 1D, task V1): `auto`, the
# GPU where its runtime has one and the CPU otherwise, which is the default; or `cpu`.
DEVICES = ("auto", "cpu")
# The most characters of a card's name a load's diagnostics say.
CARD_NAME_MOST = 64
_FILE_NAME = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.+-]{0,127}\Z")


def device_of(settings):
    """The setting `device`, `auto` when none is given; `rejected` for one it is not."""
    given = settings.get("device", "auto")
    if given not in DEVICES:
        raise Fault("rejected", "device is auto or cpu")
    return given


def device_name(torch, device):
    """The name of the device to ask PyTorch for, by the setting `device`: under `auto`, an
    NVIDIA card (`cuda`) where the runtime has one, then the Apple GPU (`mps`), then the CPU;
    under `cpu`, the CPU whatever the runtime has (ADR 0046, "Its device").

    On CUDA the model runs in full precision with TF32 off, set here before the model is
    placed, so that its answers are held to the tolerance measured on the Mac (ADR 0027)."""
    if device == "auto":
        cuda = getattr(torch, "cuda", None)
        if cuda is not None and cuda.is_available():
            torch.backends.cuda.matmul.allow_tf32 = False
            torch.backends.cudnn.allow_tf32 = False
            return "cuda"
        if torch.backends.mps.is_available():
            return "mps"
    return "cpu"


def card_name(torch=None):
    """The name of the NVIDIA card the runtime placed work on, cut to `CARD_NAME_MOST`; `None`
    where there is none. The runtime is the one the worker's adapter imported: the pack's
    core imports none (D10)."""
    torch = sys.modules.get("torch") if torch is None else torch
    cuda = getattr(torch, "cuda", None)
    try:
        if cuda is None or not cuda.is_available():
            return None
        return str(cuda.get_device_name(cuda.current_device()))[:CARD_NAME_MOST]
    except Exception:
        return None


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
    # The closed list of class names, of an adapter that declares `labels`.
    classes = ()

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
