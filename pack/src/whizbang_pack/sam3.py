"""The adapter `sam3` (docs/plans/m1c-local-pack.md, task R2a; docs/adr/0022-the-first-runtime.md):
SAM 3 through PyTorch and the `transformers` library, answering phrases.

What it does with a segment request:

- the picture's features are computed once, whatever the count of phrases (`features`);
- each phrase is then asked against those features (`candidates`): every candidate the
  model gives, with its score (the candidate's probability times the model's presence
  probability, as the library's own post-processing has it) and its box in the picture's
  pixels;
- candidates under the setting `threshold` are left out; the rest are ranked in the fixed
  order of `pack/1` by what is known before a plane is made (score, query, top, left,
  right, bottom), and cut to the request's `max_instances` for each query: only then is a
  full-size plane made for each one kept (`plane`). A plane with nothing on gives no
  instance;
- the answer is in the fixed order (ADR 0020, "The fixed order of an answer"), which the
  worker's writer holds it to again.

It takes phrases alone: a point or a box is refused as `unsupported`, by the worker (the
adapter declares `phrase` only) and by the adapter itself.

**The runtime.** `Torch` is the one place that touches PyTorch and the library, imported
only when a model is loaded, so the pack's core and its tests import neither (D10). It asks
the runtime for 8 threads in each of its two pools before any parallel work, takes the
Apple GPU (`mps`) when the runtime has it and the CPU otherwise, places the model and its
processor on that device by name (`.to(torch.device(...))`, never `device_map`), loads only
from the installation's own folder (`local_files_only=True`; the worker's environment also
sets `HF_HUB_OFFLINE=1`), and reports the device the model's parameters are on. The tests
hand the adapter a stand-in for it.

**Settings** (the kind `pack`'s, sent in `load`): `threshold`, a text of a number from 0 to
1, 0.5 when none is given (the library's model card asks with 0.5, and so did task R1's
trial). `pad`, `min_cover` and `outline` are the settings of rascal's adapters that traced a
detector's box with a second model; SAM 3 draws each mask itself, so this adapter takes them
and uses none, and its load says so.

**The revision** is `1`, the one the binary's table of adapters fixed for it in task N5
(`whizbang::wiring::adapters`): it is part of an installation's identity (ADR 0017), so the
model N5 installed is loaded as it is, and nothing is downloaded again.
"""

import os

from whizbang_pack.adapter import Adapter, Cancelled, Fault, Found

REVISION = "1"
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
    features = frozenset(("phrase",))
    revision = REVISION

    def __init__(self, runtime=None):
        self._runtime = runtime
        self.model = None
        self.threshold = THRESHOLD
        self.device = Adapter.device

    def load(self, installation, settings, call):
        threshold = threshold_of(settings)
        runtime = self._runtime() if self._runtime is not None else Torch.imported()
        runtime.load(os.path.dirname(installation.file("config.json")))
        self.model = runtime
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
        for query in queries:
            (kind,) = query
            if kind != "phrase":
                raise Fault("unsupported", "sam3 takes phrases only, not a " + kind)
        image = request["image"]
        width, height = image.width, image.height
        most = request["max_instances"]
        features = model.features(width, height, image.samples)
        found = []
        for place, query in enumerate(queries):
            if call.cancelled():
                raise Cancelled()
            answer, candidates = model.candidates(features, query["phrase"])
            kept = [
                (place, float(score), _clipped(box, width, height), index)
                for index, score, box in candidates
                if float(score) >= self.threshold
            ]
            kept.sort(key=_ranked)
            for _, score, box, index in kept[:most]:
                if call.cancelled():
                    raise Cancelled()
                samples = model.plane(features, answer, index)
                if 255 in samples:
                    found.append(Found(place, score, None, box, samples))
        found.sort(key=_fixed)
        return found, "%d instances on %s" % (len(found), self.device)

    def unload(self, call):
        model, self.model = self.model, None
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

    def load(self, folder):
        torch = self.torch
        self._pools()
        name = "mps" if torch.backends.mps.is_available() else "cpu"
        device = torch.device(name)
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

    def unload(self):
        self.model = None
        self.processor = None
        if self.device == "mps":
            self.torch.mps.empty_cache()
