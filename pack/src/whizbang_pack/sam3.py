"""The adapter `sam3` (docs/plans/m1c-local-pack.md, task R2a; docs/adr/0022-the-first-runtime.md;
docs/plans/m2b-checkpoint-1d.md, task Q1): SAM 3 through PyTorch and the `transformers`
library, answering phrases, points and boxes.

What it does with a phrase:

- the picture's features are computed once, whatever the count of phrases (`features`);
- each phrase is then asked against those features (`candidates`): every candidate the
  model gives, with its score (the candidate's probability times the model's presence
  probability, as the library's own post-processing has it) and its box in the picture's
  pixels;
- candidates under the setting `threshold` are left out; the rest are ranked in the fixed
  order of `pack/1` by what is known before a plane is made (score, query, top, left,
  right, bottom), and cut to the request's `max_instances` for each query: only then is a
  full-size plane made for each one kept (`plane`). A plane with nothing on gives no
  instance, and an instance's box is the bounds of what its plane has on: the model's box
  ranks the candidates, and the mask, drawn apart from it, can reach past it (plan 2C,
  task F1, on a picture of `source-fidelity/scenes-v1`);
- the answer is in the fixed order (ADR 0020, "The fixed order of an answer"), which the
  worker's writer holds it to again.

What it does with a point or a box (task Q1; ADR 0022, "Consequences"):

- it is asked of `Sam3TrackerModel`, of the same library and the same files: no second
  runtime and no download. The tracker is loaded on the first point or box the adapter is
  asked, and kept resident beside `Sam3Model` until the unload, under the worker's
  watermark (`load_tracker`);
- the picture's features for the tracker are computed once for a batch, whatever the count
  of points and boxes (`places`);
- a point is asked as `X Y` and a box as its corners, in the picture's pixels; the runtime
  puts them in the model's frame. A point outside the picture, or a box not inside it, is
  refused as `rejected` before anything is computed (the core refuses both before a request
  is sent: `SegmentRequest::new`);
- the tracker's candidates, at most three, are ranked by their predicted IoU, best first,
  and cut to `max_instances` before a full-size plane is made for any; each instance's
  score is that predicted IoU, and so is its `iou`. No threshold is applied to them: the
  setting `threshold` is of the phrase's score. A plane with nothing on gives no instance,
  and an instance's box is the bounds of what its plane has on.

**The runtime.** `Torch` is the one place that touches PyTorch and the library, imported
only when a model is loaded, so the pack's core and its tests import neither (D10). It asks
the runtime for 8 threads in each of its two pools before any parallel work, takes the
Apple GPU (`mps`) when the runtime has it and the CPU otherwise, or the CPU whatever the
runtime has when the setting `device` says `cpu` (plan 1D, task V1), places the model and its
processor on that device by name (`.to(torch.device(...))`, never `device_map`), loads only
from the installation's own folder (`local_files_only=True`; the worker's environment also
sets `HF_HUB_OFFLINE=1`), and reports the device the model's parameters are on. The tests
hand the adapter a stand-in for it.

**Settings** (the kind `pack`'s, sent in `load`): `device`, `auto` or `cpu` (above);
`threshold`, a text of a number from 0 to 1, 0.5 when none is given (the library's model card asks with 0.5, and so did task R1's
trial). `pad`, `min_cover` and `outline` are the settings of rascal's adapters that traced a
detector's box with a second model; SAM 3 draws each mask itself, so this adapter takes them
and uses none, and its load says so.

**The revision** is `2`, the one the binary's table of adapters names
(`whizbang::wiring::adapters`): `1` took phrases alone. It is part of an installation's
identity (ADR 0017), so the new revision is a new installation of the same files, which an
install links from the installation of revision `1` and does not download again (ADR 0017,
"A file shared between installations").
"""

import os

from whizbang_pack.adapter import Adapter, Cancelled, Fault, Found, device_name, device_of

REVISION = "2"
# The threads of each of the runtime's two pools (plan 1C, "Rules"; ADR 0022).
POOL_THREADS = 8
THRESHOLD = 0.5
# A mask's probability over this is on (the library's post-processing's default).
MASK_THRESHOLD = 0.5
# The settings of the kind this adapter takes and does not use.
UNUSED = ("min_cover", "outline", "pad")


def threshold_of(settings):
    """The score under which a candidate is left out: the setting `threshold`, a text of a
    number from 0 to 1, or `THRESHOLD`."""
    given = settings.get("threshold")
    if given is None:
        return THRESHOLD
    try:
        value = float(given) if isinstance(given, str) else None
    except ValueError:
        value = None
    if value is None or not 0.0 <= value <= 1.0:
        raise Fault("rejected", "threshold is a text of a number from 0 to 1")
    return value


def _ranked(candidate):
    """The keys of the fixed order known before a plane is made: best score first, then
    the query, then the box's top, left, right and bottom."""
    query, score, box = candidate[0], candidate[1], candidate[2]
    return (-score, query, box[1], box[0], box[2], box[3])


def _bounds(samples, width, height):
    """The box of what a plane has on: left, top, right, bottom, in pixels; `None` when
    nothing is. Each row is searched from each end by the bytes' own search, so a large
    picture costs a few calls a row."""
    samples = bytes(samples)
    left, right, top, bottom = width, 0, None, None
    for y in range(height):
        start = y * width
        first = samples.find(255, start, start + width)
        if first < 0:
            continue
        last = samples.rfind(255, start, start + width)
        left, right = min(left, first - start), max(right, last - start + 1)
        top = y if top is None else top
        bottom = y + 1
    if top is None:
        return None
    return [float(left), float(top), float(right), float(bottom)]


def _inside(kind, given, width, height):
    """Whether a point is in a pixel of the picture, or a box inside it with area."""
    if kind == "point":
        x, y = given
        return 0 <= x < width and 0 <= y < height
    left, top, right, bottom = given
    return 0 <= left < right <= width and 0 <= top < bottom <= height


def _fixed(found):
    """The whole fixed order of an answer, the predicted IoU (`None` for this model) and
    the plane's bytes last."""
    return (-found.score, found.query, found.box[1], found.box[0], found.box[2], found.box[3], bytes(found.samples))


def _clipped(box, width, height):
    """A box held to the picture: each edge within it."""
    left, top, right, bottom = (float(edge) for edge in box)
    return [
        min(max(left, 0.0), float(width)),
        min(max(top, 0.0), float(height)),
        min(max(right, 0.0), float(width)),
        min(max(bottom, 0.0), float(height)),
    ]


class Sam3(Adapter):
    features = frozenset(("phrase", "point", "box"))
    revision = REVISION

    def __init__(self, runtime=None):
        self._runtime = runtime
        self.model = None
        self.folder = None
        self.tracker = False
        self.threshold = THRESHOLD
        self.device = Adapter.device

    def load(self, installation, settings, call):
        threshold = threshold_of(settings)
        device = device_of(settings)
        runtime = self._runtime() if self._runtime is not None else Torch.imported()
        folder = os.path.dirname(installation.file("config.json"))
        runtime.load(folder, device)
        self.model = runtime
        self.folder = folder
        self.tracker = False
        self.threshold = threshold
        self.device = runtime.device
        unused = [name for name in UNUSED if name in settings]
        said = "loaded on %s; threshold %g" % (self.device, threshold)
        if unused:
            said += "; %s taken and not used by sam3" % ", ".join(unused)
        return said

    def segment(self, request, call):
        model = self.model
        if model is None:
            raise Fault("unavailable", "the model is not loaded")
        queries = request["queries"]
        image = request["image"]
        width, height = image.width, image.height
        for query in queries:
            ((kind, given),) = query.items()
            if kind not in self.features:
                raise Fault("unsupported", "sam3 takes no " + kind)
            if kind != "phrase" and not _inside(kind, given, width, height):
                raise Fault("rejected", "the %s is not inside the picture" % kind)
        most = request["max_instances"]
        asks_phrase = any("phrase" in query for query in queries)
        asks_place = any("phrase" not in query for query in queries)
        if asks_place and not self.tracker:
            model.load_tracker(self.folder)
            self.tracker = True
        features = model.features(width, height, image.samples) if asks_phrase else None
        places = model.places(width, height, image.samples) if asks_place else None
        found = []
        for place, query in enumerate(queries):
            if call.cancelled():
                raise Cancelled()
            ((kind, given),) = query.items()
            if kind == "phrase":
                found.extend(self._phrase(model, features, place, given, width, height, most, call))
            else:
                found.extend(self._place(model, places, place, kind, given, width, height, most, call))
        found.sort(key=_fixed)
        said = "%d instances on %s" % (len(found), self.device)
        # What the GPU holds for the process, its two models among it (plan 1D, task V1),
        # which macOS does not count as resident.
        held = model.memory()
        if held is not None:
            said += "; %d MiB on the GPU" % (held // (1024 * 1024))
        return found, said

    def _phrase(self, model, features, place, phrase, width, height, most, call):
        answer, candidates = model.candidates(features, phrase)
        kept = [
            (place, float(score), _clipped(box, width, height), index)
            for index, score, box in candidates
            if float(score) >= self.threshold
        ]
        kept.sort(key=_ranked)
        found = []
        for _, score, _, index in kept[:most]:
            if call.cancelled():
                raise Cancelled()
            samples = model.plane(features, answer, index)
            # The model draws the mask apart from its box, and the mask can reach past
            # it: the instance's box is the bounds of what its plane has on.
            box = _bounds(samples, width, height)
            if box is not None:
                found.append(Found(place, score, None, box, samples))
        return found

    def _place(self, model, places, place, kind, given, width, height, most, call):
        answer, candidates = model.place_candidates(places, kind, [float(number) for number in given])
        # Best predicted IoU first; among equals, the model's own order.
        kept = sorted(((float(iou), index) for index, iou in candidates), key=lambda each: (-each[0], each[1]))
        found = []
        for iou, index in kept[:most]:
            if call.cancelled():
                raise Cancelled()
            samples = model.plane(places, answer, index)
            box = _bounds(samples, width, height)
            if box is not None:
                found.append(Found(place, iou, iou, box, samples))
        return found

    def unload(self, call):
        model, self.model = self.model, None
        self.tracker = False
        if model is not None:
            model.unload()
        return "unloaded"


class Torch:
    """SAM 3 on PyTorch through `transformers`: the runtime, given its two modules. Each
    method is one step of the adapter's; nothing here decides what is kept."""

    def __init__(self, torch, transformers):
        self.torch = torch
        self.transformers = transformers
        self.device = None
        self.model = None
        self.processor = None
        self.tracker = None
        self.tracker_processor = None

    @classmethod
    def imported(cls):
        """The runtime, its modules imported now; `unavailable` where the extra is not
        installed."""
        try:
            import torch
            import transformers
        except ImportError:
            raise Fault("unavailable", "the runtime is not installed; run whizbang setup --extra torch") from None
        return cls(torch, transformers)

    def _pools(self):
        """8 threads in each pool, before any parallel work. The inter-op pool can be sized
        once in a process, so it is asked only while it is another size."""
        torch = self.torch
        if torch.get_num_threads() != POOL_THREADS:
            torch.set_num_threads(POOL_THREADS)
        if torch.get_num_interop_threads() != POOL_THREADS:
            torch.set_num_interop_threads(POOL_THREADS)

    def load(self, folder, asked="auto"):
        torch = self.torch
        self._pools()
        device = torch.device(device_name(torch, asked))
        model = self.transformers.Sam3Model.from_pretrained(folder, local_files_only=True)
        model = model.to(device)
        model.eval()
        processor = self.transformers.Sam3Processor.from_pretrained(folder, local_files_only=True)
        self.model, self.processor = model, processor
        self._device = device
        # The device the model's parameters are on: what the runtime gave.
        self.device = next(iter(model.parameters())).device.type

    def features(self, width, height, samples):
        from PIL import Image

        picture = Image.frombytes("RGBA", (width, height), bytes(samples)).convert("RGB")
        inputs = self.processor(images=picture, return_tensors="pt").to(self._device)
        with self.torch.inference_mode():
            embeds = self.model.get_vision_features(pixel_values=inputs["pixel_values"])
        return embeds, (height, width)

    def load_tracker(self, folder):
        """`Sam3TrackerModel` from the same folder, on the same device, offline, as `load`
        places `Sam3Model`."""
        self._pools()
        tracker = self.transformers.Sam3TrackerModel.from_pretrained(folder, local_files_only=True)
        tracker = tracker.to(self._device)
        tracker.eval()
        processor = self.transformers.Sam3TrackerProcessor.from_pretrained(folder, local_files_only=True)
        self.tracker, self.tracker_processor = tracker, processor

    def places(self, width, height, samples):
        from PIL import Image

        picture = Image.frombytes("RGBA", (width, height), bytes(samples)).convert("RGB")
        inputs = self.tracker_processor(images=picture, return_tensors="pt").to(self._device)
        with self.torch.inference_mode():
            embeds = self.tracker.get_image_embeddings(inputs["pixel_values"])
        return embeds, (height, width)

    def place_candidates(self, places, kind, given):
        """The tracker's candidates for a point or a box in the picture's pixels: its
        masks, and each candidate's index and predicted IoU. The processor puts the point
        or the box in the model's frame, from the picture's size."""
        embeds, (height, width) = places
        if kind == "point":
            asked = {"input_points": [[[given]]], "input_labels": [[[1]]]}
        else:
            asked = {"input_boxes": [[given]]}
        prompt = self.tracker_processor(original_sizes=[[height, width]], return_tensors="pt", **asked)
        prompt = prompt.to(self._device)
        names = ("input_points", "input_labels", "input_boxes")
        with self.torch.inference_mode():
            outputs = self.tracker(
                image_embeddings=embeds, multimask_output=True, **{name: prompt[name] for name in names if name in prompt}
            )
        scores = outputs.iou_scores[0, 0].float().cpu().tolist()
        return outputs.pred_masks[0], list(enumerate(scores))

    def candidates(self, features, phrase):
        embeds, (height, width) = features
        text = self.processor(text=phrase, return_tensors="pt").to(self._device)
        with self.torch.inference_mode():
            outputs = self.model(
                vision_embeds=embeds, input_ids=text["input_ids"], attention_mask=text["attention_mask"]
            )
            scores = outputs.pred_logits[0].sigmoid()
            if outputs.presence_logits is not None:
                scores = scores * outputs.presence_logits[0].sigmoid()
        scale = (width, height, width, height)
        boxes = outputs.pred_boxes[0].float().cpu().tolist()
        listed = [
            (index, score, [edge * size for edge, size in zip(box, scale)])
            for index, (score, box) in enumerate(zip(scores.float().cpu().tolist(), boxes))
        ]
        return outputs.pred_masks, listed

    def plane(self, features, masks, index):
        _, size = features
        torch = self.torch
        with torch.inference_mode():
            low = masks[0, index][None, None].sigmoid()
            full = torch.nn.functional.interpolate(low, size=size, mode="bilinear", align_corners=False)[0, 0]
            on = (full > MASK_THRESHOLD).to(torch.uint8) * 255
        return bytes(on.cpu().numpy().tobytes())

    def memory(self):
        """The bytes the GPU's driver holds for the process, while the model is on the GPU;
        `None` on the CPU."""
        if self.device != "mps":
            return None
        return int(self.torch.mps.driver_allocated_memory())

    def unload(self):
        self.model = None
        self.processor = None
        self.tracker = None
        self.tracker_processor = None
        if self.device == "mps":
            self.torch.mps.empty_cache()
