"""The adapter `eomt-ade20k` (plan mE, task T2; docs/adr/0052-the-fixed-class-segmenter.md):
EoMT large, trained on ADE20K at 512, through PyTorch and the `transformers` library. A
fixed-class segmenter: it labels every pixel of a picture with one of the 150 classes of
ADE20K, and so answers a phrase only when the phrase is one of them.

It is the model squint's `classes` and `regions` used (squint, `tools/python/MODEL.md` and
`src/squint_tools/segmenter.py`, at the commit `docs/sources.toml` pins), run here from the
upstream safetensors through the library rather than from squint's ONNX export, with
squint's preprocessing and its reading of the two heads.

**Its features** are `phrase` and `labels`. `labels` (ADR 0052) says what its phrases are:
its class names are the closed list `CLASSES`, every pixel of a picture is labelled with
exactly one of them, and a phrase that is one of them is answered with at most one
instance, the pixels so labelled. So the planes of every class of one picture are disjoint
and together cover it: asking every class is the whole picture's labels, a class per pixel
and each class's confidence, which whizbang's action `labels` asks in batches. A phrase
that is not one of the classes is refused, `rejected`, before anything is computed, and the
sentence names where the list is. It takes no point and no box.

What it does with a picture, once (`Torch.label`):

- the picture is brought to the model's square input as squint brought it: its longer edge
  resized to 512 (bilinear), the shorter padded to 512 with the picture's mean colour,
  centred; scaled to 0 to 1 and normalised by ImageNet's mean and deviation;
- the model gives, for each of its 100 queries, a class's logits (150 classes and no
  object) and a mask's logits at a quarter of the input. Each class's score at a pixel is
  the sum over queries of the query's probability of the class, without the no-object
  column, times its mask's sigmoid (squint's `class_scores`); the scores are resized to
  the input (bilinear) and the pixel's class is the one with the highest, its confidence
  that class's share of the pixel's summed scores;
- the part of the input the picture is in is cut out and brought to the picture's own size,
  nearest, so every pixel of the picture has a class and a confidence.

The last picture's labels are kept until another picture is asked or the model is unloaded,
by the binding and the size a request names (`pack/1` sends a picture by its binding), so
the batches of one `labels` call compute the picture once. An instance's score is the mean
confidence of its class's pixels, its predicted IoU `None`, its box the bounds of its
plane. The answer is in the fixed order (ADR 0020).

**The runtime.** `Torch` is the one place that touches PyTorch, the library, NumPy and
Pillow, imported only when the model is loaded (D10). It asks for 8 threads in each pool,
places the model on the device the setting `device` asks for (`mps`, `cuda` or the CPU, as
`adapter.device_name` picks), loads from the installation's own folder with the network
off (`local_files_only=True`), and reports the device the parameters are on. The tests hand
the adapter a stand-in for it.

**The load** holds the installation's `config.json` to `CLASSES`: the labels its `id2label`
gives, in order and each trimmed, are the list, or the load is refused, `unavailable`, and
nothing is placed. **Settings**: `device`; `threshold`, `pad`, `min_cover` and `outline` are
taken and not used, and the load says so.

**The revision** is `1`, the one the binary's table of adapters names.
"""

import json
import os

from whizbang_pack.adapter import Adapter, Cancelled, Fault, Found, device_name, device_of

REVISION = "1"
POOL_THREADS = 8
# The model's square input, and the normalisation of its samples (squint's `segmenter.py`).
SIDE = 512
MEAN = (0.485, 0.456, 0.406)
STD = (0.229, 0.224, 0.225)
UNUSED = ("min_cover", "outline", "pad", "threshold")
# Where the sentence of a refused phrase says the list is.
LISTED = "whizbang_pack.eomt_ade20k.CLASSES"

# The 150 classes of ADE20K, in the model's label order: the `id2label` of the model's
# `config.json` at the revision the catalogue pins, each trimmed (one is `bed ` there).
CLASSES = (
    "wall", "building", "sky", "floor", "tree", "ceiling", "road", "bed", "windowpane", "grass",
    "cabinet", "sidewalk", "person", "earth", "door", "table", "mountain", "plant", "curtain", "chair",
    "car", "water", "painting", "sofa", "shelf", "house", "sea", "mirror", "rug", "field",
    "armchair", "seat", "fence", "desk", "rock", "wardrobe", "lamp", "bathtub", "railing", "cushion",
    "base", "box", "column", "signboard", "chest of drawers", "counter", "sand", "sink", "skyscraper", "fireplace",
    "refrigerator", "grandstand", "path", "stairs", "runway", "case", "pool table", "pillow", "screen door", "stairway",
    "river", "bridge", "bookcase", "blind", "coffee table", "toilet", "flower", "book", "hill", "bench",
    "countertop", "stove", "palm", "kitchen island", "computer", "swivel chair", "boat", "bar", "arcade machine", "hovel",
    "bus", "towel", "light", "truck", "tower", "chandelier", "awning", "streetlight", "booth", "television receiver",
    "airplane", "dirt track", "apparel", "pole", "land", "bannister", "escalator", "ottoman", "bottle", "buffet",
    "poster", "stage", "van", "ship", "fountain", "conveyer belt", "canopy", "washer", "plaything", "swimming pool",
    "stool", "barrel", "basket", "waterfall", "tent", "bag", "minibike", "cradle", "oven", "ball",
    "food", "step", "tank", "trade name", "microwave", "pot", "animal", "bicycle", "lake", "dishwasher",
    "screen", "blanket", "sculpture", "hood", "sconce", "vase", "traffic light", "tray", "ashcan", "fan",
    "pier", "crt screen", "plate", "monitor", "bulletin board", "shower", "radiator", "glass", "clock", "flag",
)
_INDEX = {name: index for index, name in enumerate(CLASSES)}


def classes_of(config):
    """The labels a model's config gives, in the order of their ids, each trimmed; `None`
    when it gives none that can be read so."""
    labels = config.get("id2label") if isinstance(config, dict) else None
    if not isinstance(labels, dict):
        return None
    try:
        return tuple(str(labels[str(index)]).strip() for index in range(len(labels)))
    except KeyError:
        return None


def _fixed(found):
    """The fixed order of an answer (ADR 0020): best score first, then the query, the box's
    top, left, right and bottom, and the plane's bytes last; no predicted IoU is given."""
    return (-found.score, found.query, found.box[1], found.box[0], found.box[2], found.box[3], bytes(found.samples))


class EomtAde20k(Adapter):
    features = frozenset(("phrase", "labels"))
    revision = REVISION
    classes = CLASSES

    def __init__(self, runtime=None):
        self._runtime = runtime
        self.model = None
        self.device = Adapter.device
        self._last = None

    def load(self, installation, settings, call):
        device = device_of(settings)
        path = installation.file("config.json")
        try:
            with open(path, "rb") as file:
                config = json.loads(file.read().decode("utf-8"))
        except (OSError, UnicodeError, ValueError):
            raise Fault("unavailable", "the installation's config cannot be read") from None
        if classes_of(config) != CLASSES:
            raise Fault("unavailable", "the installation's classes are not the 150 of ADE20K the adapter names")
        runtime = self._runtime() if self._runtime is not None else Torch.imported()
        runtime.load(os.path.dirname(path), device)
        self.model = runtime
        self.device = runtime.device
        self._last = None
        unused = [name for name in UNUSED if name in settings]
        said = "loaded on %s; %d classes" % (self.device, len(CLASSES))
        if unused:
            said += "; %s taken and not used by eomt-ade20k" % ", ".join(unused)
        return said

    def segment(self, request, call):
        model = self.model
        if model is None:
            raise Fault("unavailable", "the model is not loaded")
        asked = []
        for query in request["queries"]:
            ((kind, given),) = query.items()
            if kind != "phrase":
                raise Fault("unsupported", "eomt-ade20k takes no " + kind)
            if given not in _INDEX:
                raise Fault("rejected", "the phrase is none of the 150 classes eomt-ade20k labels, listed in " + LISTED)
            asked.append(_INDEX[given])
        image = request["image"]
        key = (image.binding, image.width, image.height)
        if self._last is None or self._last[0] != key:
            self._last = None
            labelled = model.label(image.width, image.height, image.samples)
            self._last = (key, labelled, {index: (score, box) for index, score, box in model.present(labelled)})
        _, labelled, present = self._last
        found = []
        for place, index in enumerate(asked):
            if call.cancelled():
                raise Cancelled()
            if index not in present:
                continue
            score, box = present[index]
            score = min(max(float(score), 0.0), 1.0)
            found.append(Found(place, score, None, [float(edge) for edge in box], model.plane(labelled, index)))
        found.sort(key=_fixed)
        return found, "%d of %d classes asked are in the picture, on %s" % (len(found), len(asked), self.device)

    def unload(self, call):
        model, self.model, self._last = self.model, None, None
        if model is not None:
            model.unload()
        return "unloaded"


class Torch:
    """EoMT on PyTorch through `transformers`: the runtime, given its modules. Each method is
    one step of the adapter's; nothing here decides what is answered."""

    def __init__(self, torch, transformers, numpy, image):
        self.torch = torch
        self.transformers = transformers
        self.numpy = numpy
        self.image = image
        self.device = None
        self.model = None

    @classmethod
    def imported(cls):
        """The runtime, its modules imported now; `unavailable` where the extra is not
        installed."""
        try:
            import numpy
            import torch
            import transformers
            from PIL import Image
        except ImportError:
            raise Fault("unavailable", "the runtime is not installed; run whizbang setup --extra torch") from None
        return cls(torch, transformers, numpy, Image)

    def load(self, folder, asked="auto"):
        torch = self.torch
        if torch.get_num_threads() != POOL_THREADS:
            torch.set_num_threads(POOL_THREADS)
        if torch.get_num_interop_threads() != POOL_THREADS:
            torch.set_num_interop_threads(POOL_THREADS)
        device = torch.device(device_name(torch, asked))
        model = self.transformers.EomtForUniversalSegmentation.from_pretrained(folder, local_files_only=True)
        model = model.to(device)
        model.eval()
        self.model = model
        self._device = device
        self.device = next(iter(model.parameters())).device.type

    def label(self, width, height, samples):
        """Each pixel's class and confidence, as two arrays of the picture's size."""
        np, torch, image = self.numpy, self.torch, self.image
        rgb = image.frombytes("RGBA", (width, height), bytes(samples)).convert("RGB")
        scale = SIDE / max(width, height)
        inner_w, inner_h = max(1, round(width * scale)), max(1, round(height * scale))
        resized = np.asarray(rgb.resize((inner_w, inner_h), image.BILINEAR))
        square = np.empty((SIDE, SIDE, 3), np.uint8)
        square[:] = np.asarray(rgb).reshape(-1, 3).mean(0).round().astype(np.uint8)
        x, y = (SIDE - inner_w) // 2, (SIDE - inner_h) // 2
        square[y : y + inner_h, x : x + inner_w] = resized
        normal = (square.astype(np.float32) / 255.0 - np.array(MEAN, np.float32)) / np.array(STD, np.float32)
        pixels = torch.from_numpy(np.ascontiguousarray(normal.transpose(2, 0, 1)[None])).to(self._device)
        functional = torch.nn.functional
        with torch.inference_mode():
            outputs = self.model(pixel_values=pixels)
            probs = outputs.class_queries_logits.float().softmax(-1)[..., :-1]
            masks = outputs.masks_queries_logits.float().sigmoid()
            scores = torch.einsum("bqc,bqhw->bchw", probs, masks)
            scores = functional.interpolate(scores, size=(SIDE, SIDE), mode="bilinear", align_corners=False)
            best, labels = scores.max(1)
            confidence = best / scores.sum(1).clamp(min=1e-6)
            labels = labels[:, y : y + inner_h, x : x + inner_w].float()
            confidence = confidence[:, y : y + inner_h, x : x + inner_w]
            labels = functional.interpolate(labels[:, None], size=(height, width), mode="nearest")[0, 0]
            confidence = functional.interpolate(confidence[:, None], size=(height, width), mode="nearest")[0, 0]
        return labels.to(torch.uint8).cpu().numpy(), confidence.cpu().numpy()

    def present(self, labelled):
        """Each class some pixel has: its index, the mean confidence of its pixels, and the
        box of its pixels (left, top, right, bottom)."""
        np = self.numpy
        labels, confidence = labelled
        flat = labels.ravel()
        counts = np.bincount(flat, minlength=len(CLASSES))
        sums = np.bincount(flat, weights=confidence.ravel().astype(np.float64), minlength=len(CLASSES))
        found = []
        for index in np.nonzero(counts)[0]:
            rows = np.nonzero((labels == index).any(1))[0]
            columns = np.nonzero((labels == index).any(0))[0]
            box = [int(columns[0]), int(rows[0]), int(columns[-1]) + 1, int(rows[-1]) + 1]
            found.append((int(index), float(sums[index] / counts[index]), box))
        return found

    def plane(self, labelled, index):
        labels, _ = labelled
        return bytes(((labels == index).astype(self.numpy.uint8) * 255).tobytes())

    def unload(self):
        self.model = None
        if self.device == "mps":
            self.torch.mps.empty_cache()
