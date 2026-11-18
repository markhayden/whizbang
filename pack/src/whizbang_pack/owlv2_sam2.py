"""The adapter `owlv2-sam2` (docs/plans/m2b-checkpoint-1d.md, task T1): the model `tiny`,
OWLv2 finding boxes for a phrase and SAM 2.1 Hiera-Tiny drawing each mask, through PyTorch
and the `transformers` library of the lock (`Owlv2ForObjectDetection`, `Sam2Model`). The
way of rascal's Tiny (rascal, `tools/python/src/rascal_tools/models/adapters.py`,
`outline.py` and `models/owlv2.py` at the commit `docs/sources.toml` pins), rewritten here
over planes of bytes, with no third-party package, so that the pack's core and its tests
import none (D10).

What it does with a phrase:

- OWLv2 is asked every phrase of the request at once, about the picture (`detect`): each of
  its proposals, with its score for the phrase and its box in the picture's pixels;
- for each phrase, the proposals over the setting `threshold` are kept, overlapping ones
  are suppressed (greedy, best score first, at an IoU of `NMS`), and a box under a pixel
  wide or high is dropped; the rest are ranked in the fixed order of `pack/1` by what is
  known before a mask is drawn (score, top, left, right, bottom) and cut to the request's
  `max_instances` for the phrase. Only then is SAM 2.1 asked anything;
- each box kept is padded by the setting `pad` (each side out by that share of the box's
  side on its axis, held to the picture) and drawn by SAM 2.1 (`trace`): of its three
  candidate masks, the setting `outline` picks one (`pick`). When the mask kept covers less
  than the setting `min_cover` of its box, or its largest piece holds under `SPLIT` of it,
  it is drawn once more in the box padded by twice `pad`, with a point on each piece, and
  the second mask replaces the first when it has more pixels and `INSIDE` of them inside
  its box;
- an instance's score is OWLv2's, its `iou` the predicted IoU of the mask kept, and its box
  the bounds of what its mask has on. A mask with nothing on gives no instance.

What it does with a point or a box: it is asked of SAM 2.1 alone, as SAM 3's tracker is
asked one (`sam3.py`): its three candidates, ranked by their predicted IoU, best first, and
cut to `max_instances` before a full-size plane is made for any; each instance's score is
that predicted IoU, and so is its `iou`. No threshold, pad or outline rule is applied to
them: those settings are of a phrase. A point outside the picture, or a box not inside it,
is refused as `rejected` before anything is computed.

The picture's features for SAM 2.1 are computed once for a request, whatever the count of
boxes, points and phrases; OWLv2 is run once for all of its phrases.

**The files** (task T1, item 3): both models' repositories hold a `config.json` and a
`model.safetensors`, so the catalogue installs each file under its model's prefix
(`owlv2.config.json`, `sam2.model.safetensors`, and so on: `FILES`). The runtime builds
each model from its config file and its weights file by name, and its processors from
their own files by name, with the hub's network off (`local_files_only=True`; the worker's
environment also sets `HF_HUB_OFFLINE=1`): never from a folder the library would look up.

**Settings** (the kind `pack`'s, sent in `load`), each a text: `threshold` (0.2), `pad`
(0.05) and `min_cover` (0.2), numbers from 0 to 1; `outline`, `whole` or `best` (`whole`).
The defaults are rascal's Tiny's. And `device`, `auto` (the GPU where the runtime has one)
or `cpu` (plan 1D, task V1).

**The revision** is `1`, the one the binary's table of adapters names
(`whizbang::wiring::adapters`).
"""

from whizbang_pack.adapter import Adapter, Cancelled, Fault, Found, device_name, device_of

REVISION = "1"
# The threads of each of the runtime's two pools (plan 1C, "Rules"; ADR 0022).
POOL_THREADS = 8
# The settings that are numbers from 0 to 1, and their defaults.
NUMBERS = {"threshold": 0.2, "pad": 0.05, "min_cover": 0.2}
OUTLINES = ("whole", "best")
OUTLINE = "whole"
# Proposals of one phrase that overlap one better by this IoU or more are suppressed.
NMS = 0.5
# The share of a mask that must lie inside its box: for `whole`, and for a retry to be kept.
INSIDE = 0.95
# The predicted IoU a mask needs to be the whole-object pick.
MIN_IOU = 0.5
# A mask whose largest piece holds less than this share of it is in pieces.
SPLIT = 0.8
# The most points a retry gives SAM 2.1: one on each of the largest pieces.
MAX_POINTS = 8
# The files of each model, by their names in the model's repository; installed under
# `<prefix>.<name>`.
FILES = {
    "owlv2": (
        "config.json",
        "model.safetensors",
        "preprocessor_config.json",
        "vocab.json",
        "merges.txt",
        "special_tokens_map.json",
        "tokenizer_config.json",
        "added_tokens.json",
    ),
    "sam2": ("config.json", "model.safetensors", "preprocessor_config.json"),
}


def files_of(installation):
    """The path of each file of each model, by the model's prefix and the file's name in its
    repository."""
    return {prefix: {name: installation.file(prefix + "." + name) for name in names} for prefix, names in FILES.items()}


def settings_of(settings):
    """The four settings, each held to its rule; `rejected` for one that breaks it."""
    held = {}
    for name, default in NUMBERS.items():
        given = settings.get(name)
        if given is None:
            held[name] = default
            continue
        try:
            value = float(given) if isinstance(given, str) else None
        except ValueError:
            value = None
        if value is None or not 0.0 <= value <= 1.0:
            raise Fault("rejected", "%s is a text of a number from 0 to 1" % name)
        held[name] = value
    rule = settings.get("outline", OUTLINE)
    if rule not in OUTLINES:
        raise Fault("rejected", "outline is whole or best")
    held["outline"] = rule
    return held


# --- boxes ---------------------------------------------------------------------------------


def pad_box(box, pad, width, height):
    """Each side moved out by `pad` times the box's side on that axis, held to the picture."""
    left, top, right, bottom = box
    across, down = (right - left) * pad, (bottom - top) * pad
    return [
        max(0.0, left - across),
        max(0.0, top - down),
        min(float(width), right + across),
        min(float(height), bottom + down),
    ]


def clipped(box, width, height):
    """A box held to the picture: each edge within it."""
    left, top, right, bottom = (float(edge) for edge in box)
    return [
        min(max(left, 0.0), float(width)),
        min(max(top, 0.0), float(height)),
        min(max(right, 0.0), float(width)),
        min(max(bottom, 0.0), float(height)),
    ]


def overlap(one, other):
    """The intersection over union of two boxes."""
    across = max(0.0, min(one[2], other[2]) - max(one[0], other[0]))
    down = max(0.0, min(one[3], other[3]) - max(one[1], other[1]))
    shared = across * down
    union = (one[2] - one[0]) * (one[3] - one[1]) + (other[2] - other[0]) * (other[3] - other[1]) - shared
    return shared / max(union, 1e-9)


def suppressed(candidates):
    """The candidates (score, box) greedy suppression keeps: best score first, among equals
    the proposal's own order; each drops those that overlap it by `NMS` or more."""
    order = sorted(range(len(candidates)), key=lambda at: (-candidates[at][0], at))
    kept = []
    for at in order:
        box = candidates[at][1]
        if all(overlap(box, candidates[other][1]) < NMS for other in kept):
            kept.append(at)
    return [candidates[at] for at in kept]


def inside(kind, given, width, height):
    """Whether a point is in a pixel of the picture, or a box inside it with area."""
    if kind == "point":
        x, y = given
        return 0 <= x < width and 0 <= y < height
    left, top, right, bottom = given
    return 0 <= left < right <= width and 0 <= top < bottom <= height


# --- planes: bytes of one sample a pixel, 255 on and 0 off -----------------------------------


def _span(low, high, size):
    """The pixels whose centres fall between two edges: first and one past the last."""
    first = min(size, max(0, _ceil(low - 0.5)))
    last = min(size, max(0, _ceil(high - 0.5)))
    return first, max(first, last)


def _ceil(value):
    whole = int(value)
    return whole + 1 if value > whole else whole


def count_in(samples, width, height, box):
    """How many pixels of the plane are on with their centres inside the box, and how many
    pixels have their centres inside it."""
    left, right = _span(box[0], box[2], width)
    top, bottom = _span(box[1], box[3], height)
    on = sum(samples.count(255, y * width + left, y * width + right) for y in range(top, bottom))
    return on, (right - left) * (bottom - top)


def inside_share(samples, width, height, box):
    """The share of the plane's pixels on inside the box; 0 for a plane with none on."""
    total = samples.count(255)
    if total == 0:
        return 0.0
    return count_in(samples, width, height, box)[0] / total


def runs(samples, width, height):
    """The runs of pixels on, row by row: (row, first, one past the last). Each row is
    searched by the bytes' own search, so a plane costs its runs, not its pixels."""
    found = []
    for y in range(height):
        at, end = y * width, (y + 1) * width
        while True:
            first = samples.find(255, at, end)
            if first < 0:
                break
            last = samples.find(0, first, end)
            last = end if last < 0 else last
            found.append((y, first - y * width, last - y * width))
            at = last
    return found


def pieces(samples, width, height):
    """The plane's 8-connected pieces, largest first, among equals the one whose first pixel
    comes first: each a list of its runs. Runs of one row that touch runs of the row before
    are joined, by union and find."""
    every = runs(samples, width, height)
    parent = list(range(len(every)))

    def root(at):
        while parent[at] != at:
            parent[at] = parent[parent[at]]
            at = parent[at]
        return at

    above, row, current = [], None, []
    for at, (y, first, last) in enumerate(every):
        if y != row:
            above = current if row == y - 1 else []
            row, current = y, []
        for other in above:
            _, start, end = every[other]
            if start <= last and first <= end:
                one, two = root(at), root(other)
                if one != two:
                    parent[max(one, two)] = min(one, two)
        current.append(at)
    grouped = {}
    for at in range(len(every)):
        grouped.setdefault(root(at), []).append(every[at])

    def key(piece):
        area = sum(last - first for _, first, last in piece)
        y, first, _ = piece[0]
        return (-area, y * width + first)

    return sorted(grouped.values(), key=key)


def point_of(piece):
    """The piece's pixel nearest its centroid, among equals the one with the smaller y and
    then the smaller x: a point on the piece even when its centroid is not (a ring)."""
    area = sum(last - first for _, first, last in piece)
    cx = sum((first + last - 1) * (last - first) / 2.0 for _, first, last in piece) / area
    cy = sum(y * (last - first) for y, first, last in piece) / area
    best = None
    for y, first, last in piece:
        x = min(max(_ceil(cx - 0.5), first), last - 1)
        key = ((x - cx) ** 2 + (y - cy) ** 2, y, x)
        best = key if best is None or key < best else best
    return (best[2], best[1])


def points(samples, width, height):
    """One point on each piece, in the pieces' order, at most `MAX_POINTS`."""
    return [point_of(piece) for piece in pieces(samples, width, height)[:MAX_POINTS]]


def bounds(samples, width, height):
    """The box of what a plane has on: left, top, right, bottom; `None` when nothing is."""
    found = runs(samples, width, height)
    if not found:
        return None
    left = min(first for _, first, _ in found)
    right = max(last for _, _, last in found)
    return [float(left), float(found[0][0]), float(right), float(found[-1][0] + 1)]


def pick(planes, ious, box, rule, width, height):
    """The index of the mask to keep. `best`: the highest predicted IoU, among equals the
    lower index. `whole`: of the masks with something on, at least `MIN_IOU` predicted and
    `INSIDE` of their pixels inside the box, the largest, among equals the higher predicted
    IoU and then the lower index; `best` when none is."""
    best = max(range(len(planes)), key=lambda at: (ious[at], -at))
    if rule == "best":
        return best
    whole = [
        at
        for at, plane in enumerate(planes)
        if 255 in plane and ious[at] >= MIN_IOU and inside_share(plane, width, height, box) >= INSIDE
    ]
    if not whole:
        return best
    return max(whole, key=lambda at: (planes[at].count(255), ious[at], -at))


def needs_retry(samples, width, height, box, min_cover):
    """Whether a mask covers less than `min_cover` of its box, or its largest piece holds
    under `SPLIT` of it."""
    on, area = count_in(samples, width, height, box)
    if area == 0 or on / area < min_cover:
        return True
    found = pieces(samples, width, height)
    largest = sum(last - first for _, first, last in found[0]) if found else 0
    return bool(found) and largest < SPLIT * samples.count(255)


def keeps_retry(first, retry, box, width, height):
    """Whether the retry's mask replaces the first: more pixels, and `INSIDE` of them inside
    its own box."""
    return retry.count(255) > first.count(255) and inside_share(retry, width, height, box) >= INSIDE


def _fixed(found):
    """The whole fixed order of an answer, the plane's bytes last."""
    return (-found.score, found.query, found.box[1], found.box[0], found.box[2], found.box[3], bytes(found.samples))


class Owlv2Sam2(Adapter):
    features = frozenset(("phrase", "point", "box"))
    revision = REVISION

    def __init__(self, runtime=None):
        self._runtime = runtime
        self.model = None
        self.settings = None
        self.device = Adapter.device

    def load(self, installation, settings, call):
        held = settings_of(settings)
        device = device_of(settings)
        runtime = self._runtime() if self._runtime is not None else Torch.imported()
        runtime.load(files_of(installation), device)
        self.model = runtime
        self.settings = held
        self.device = runtime.device
        return "loaded on %s; threshold %g; pad %g; outline %s; min_cover %g" % (
            self.device,
            held["threshold"],
            held["pad"],
            held["outline"],
            held["min_cover"],
        )

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
                raise Fault("unsupported", "owlv2-sam2 takes no " + kind)
            if kind != "phrase" and not inside(kind, given, width, height):
                raise Fault("rejected", "the %s is not inside the picture" % kind)
        most = request["max_instances"]
        phrases = [(place, query["phrase"]) for place, query in enumerate(queries) if "phrase" in query]
        boxes = {}
        if phrases:
            detected = model.detect(width, height, image.samples, [phrase for _, phrase in phrases])
            for (place, _), proposals in zip(phrases, detected):
                boxes[place] = self._kept(proposals, width, height, most)
        features = None
        if any(boxes.values()) or len(phrases) < len(queries):
            features = model.embed(width, height, image.samples)
        found, retried = [], [0, 0]
        for place, query in enumerate(queries):
            if call.cancelled():
                raise Cancelled()
            ((kind, given),) = query.items()
            if kind == "phrase":
                for score, box in boxes[place]:
                    if call.cancelled():
                        raise Cancelled()
                    plane, iou = self._trace(model, features, box, width, height, retried)
                    drawn = bounds(plane, width, height)
                    if drawn is not None:
                        found.append(Found(place, score, iou, drawn, plane))
            else:
                found.extend(self._place(model, features, kind, place, given, width, height, most, call))
        found.sort(key=_fixed)
        return found, "%d instances on %s; %d retried, %d retries kept" % (len(found), self.device, *retried)

    def _kept(self, proposals, width, height, most):
        """A phrase's proposals over the threshold, suppressed, ranked and cut."""
        over = [(float(score), clipped(box, width, height)) for score, box in proposals if float(score) > self.settings["threshold"]]
        kept = [
            (score, box)
            for score, box in suppressed(over)
            if box[2] - box[0] >= 1.0 and box[3] - box[1] >= 1.0
        ]
        kept.sort(key=lambda each: (-each[0], each[1][1], each[1][0], each[1][2], each[1][3]))
        return kept[:most]

    def _trace(self, model, features, found, width, height, retried):
        """A box OWLv2 found, drawn by SAM 2.1: padded, picked, and retried once."""
        held = self.settings
        box = pad_box(found, held["pad"], width, height)
        planes, ious = self._outlines(model, features, box, [])
        at = pick(planes, ious, box, held["outline"], width, height)
        plane, iou = planes[at], ious[at]
        if 255 not in plane or not needs_retry(plane, width, height, box, held["min_cover"]):
            return plane, iou
        retried[0] += 1
        wider = pad_box(found, 2 * held["pad"], width, height)
        planes, ious = self._outlines(model, features, wider, points(plane, width, height))
        at = pick(planes, ious, wider, held["outline"], width, height)
        if keeps_retry(plane, planes[at], wider, width, height):
            retried[1] += 1
            return planes[at], ious[at]
        return plane, iou

    def _outlines(self, model, features, box, given):
        """Every candidate of a box and its points, full size: the rule needs them all."""
        answer, candidates = model.outlines(features, box, given)
        ious = [float(iou) for _, iou in candidates]
        return [bytes(model.plane(features, answer, index)) for index, _ in candidates], ious

    def _place(self, model, features, kind, place, given, width, height, most, call):
        """A point or a box asked of SAM 2.1 alone: its candidates, best predicted IoU first,
        cut before any is made full size."""
        given = [float(number) for number in given]
        if kind == "point":
            answer, candidates = model.outlines(features, None, [(given[0], given[1])])
        else:
            answer, candidates = model.outlines(features, given, [])
        kept = sorted(((float(iou), index) for index, iou in candidates), key=lambda each: (-each[0], each[1]))
        found = []
        for iou, index in kept[:most]:
            if call.cancelled():
                raise Cancelled()
            samples = bytes(model.plane(features, answer, index))
            drawn = bounds(samples, width, height)
            if drawn is not None:
                found.append(Found(place, iou, iou, drawn, samples))
        return found

    def unload(self, call):
        model, self.model = self.model, None
        if model is not None:
            model.unload()
        return "unloaded"


class Torch:
    """OWLv2 and SAM 2.1 on PyTorch through `transformers`: the runtime, given its modules.
    Each method is one step of the adapter's; nothing here decides what is kept."""

    def __init__(self, torch, transformers, load_weights):
        self.torch = torch
        self.transformers = transformers
        self.load_weights = load_weights
        self.device = None
        self.detector = None
        self.tokenizer = None
        self.detector_images = None
        self.segmenter = None
        self.segmenter_processor = None

    @classmethod
    def imported(cls):
        """The runtime, its modules imported now; `unavailable` where the extra is not
        installed."""
        try:
            import torch
            import transformers
            from safetensors.torch import load_file
        except ImportError:
            raise Fault("unavailable", "the runtime is not installed; run whizbang setup --extra torch") from None
        return cls(torch, transformers, load_file)

    def _pools(self):
        torch = self.torch
        if torch.get_num_threads() != POOL_THREADS:
            torch.set_num_threads(POOL_THREADS)
        if torch.get_num_interop_threads() != POOL_THREADS:
            torch.set_num_interop_threads(POOL_THREADS)

    def _model(self, kind, config_kind, files):
        """A model of this class, from its config file and its weights file by name."""
        config = getattr(self.transformers, config_kind).from_json_file(files["config.json"])
        state = self.load_weights(files["model.safetensors"])
        made = getattr(self.transformers, kind).from_pretrained(
            None, config=config, state_dict=state, local_files_only=True
        )
        made = made.to(self._device)
        made.eval()
        return made

    def load(self, files, asked="auto"):
        import json

        torch = self.torch
        self._pools()
        self._device = torch.device(device_name(torch, asked))
        owlv2, sam2 = files["owlv2"], files["sam2"]
        transformers = self.transformers

        def read(path):
            with open(path, "rb") as file:
                return json.loads(file.read())

        self.detector = self._model("Owlv2ForObjectDetection", "Owlv2Config", owlv2)
        with open(owlv2["merges.txt"], "r", encoding="utf-8") as file:
            # The first line names the file's version; each other is a pair.
            merges = [tuple(line.split(" ")) for line in file.read().split("\n")[1:] if line]
        told = dict(read(owlv2["tokenizer_config.json"]))
        told.update(read(owlv2["special_tokens_map.json"]))
        # A special token is a text, or an object that holds it as `content`.
        told = {
            key: value["content"] if isinstance(value, dict) else value
            for key, value in told.items()
            if key in TOKENIZER_SETTINGS
        }
        self.tokenizer = transformers.CLIPTokenizer(vocab=read(owlv2["vocab.json"]), merges=merges, **told)
        self.detector_images = transformers.Owlv2ImageProcessor(**_processor_settings(read(owlv2["preprocessor_config.json"])))
        self.segmenter = self._model("Sam2Model", "Sam2Config", sam2)
        images = transformers.Sam2ImageProcessor(**_processor_settings(read(sam2["preprocessor_config.json"])))
        self.segmenter_processor = transformers.Sam2Processor(image_processor=images)
        self.device = next(iter(self.detector.parameters())).device.type

    def _picture(self, width, height, samples):
        from PIL import Image

        return Image.frombytes("RGBA", (width, height), bytes(samples)).convert("RGB")

    def detect(self, width, height, samples, phrases):
        """OWLv2's proposals for each phrase: its score and its box in the picture's pixels.
        OWLv2 sees the picture padded to a square at its bottom and right, so a box's
        fractions are of that square's side."""
        torch = self.torch
        picture = self._picture(width, height, samples)
        pixels = self.detector_images(images=picture, return_tensors="pt")["pixel_values"].to(self._device)
        text = self.tokenizer(
            [phrase.lower() for phrase in phrases], padding="max_length", truncation=True, max_length=16, return_tensors="pt"
        ).to(self._device)
        with torch.inference_mode():
            outputs = self.detector(input_ids=text["input_ids"], attention_mask=text["attention_mask"], pixel_values=pixels)
            scores = outputs.logits[0].float().sigmoid().cpu().tolist()
            boxes = outputs.pred_boxes[0].float().cpu().tolist()
        side = float(max(width, height))
        corners = [
            [(cx - w / 2) * side, (cy - h / 2) * side, (cx + w / 2) * side, (cy + h / 2) * side] for cx, cy, w, h in boxes
        ]
        return [[(row[at], box) for row, box in zip(scores, corners)] for at in range(len(phrases))]

    def embed(self, width, height, samples):
        picture = self._picture(width, height, samples)
        inputs = self.segmenter_processor(images=picture, return_tensors="pt").to(self._device)
        with self.torch.inference_mode():
            embeds = self.segmenter.get_image_embeddings(inputs["pixel_values"])
        return embeds, (height, width)

    def outlines(self, features, box, given):
        """SAM 2.1's three candidates for a box, its points or both, in the picture's pixels:
        its masks, and each candidate's index and predicted IoU."""
        embeds, (height, width) = features
        asked = {}
        if box is not None:
            asked["input_boxes"] = [[list(box)]]
        if given:
            asked["input_points"] = [[[list(point) for point in given]]]
            asked["input_labels"] = [[[1] * len(given)]]
        prompt = self.segmenter_processor(original_sizes=[[height, width]], return_tensors="pt", **asked).to(self._device)
        names = ("input_points", "input_labels", "input_boxes")
        with self.torch.inference_mode():
            outputs = self.segmenter(
                image_embeddings=embeds, multimask_output=True, **{name: prompt[name] for name in names if name in prompt}
            )
        ious = outputs.iou_scores[0, 0].float().cpu().tolist()
        return outputs.pred_masks[0, 0], list(enumerate(ious))

    def plane(self, features, masks, index):
        _, size = features
        torch = self.torch
        with torch.inference_mode():
            low = masks[index][None, None].float()
            full = torch.nn.functional.interpolate(low, size=size, mode="bilinear", align_corners=False)[0, 0]
            on = (full > 0.0).to(torch.uint8) * 255
        return bytes(on.cpu().numpy().tobytes())

    def unload(self):
        self.detector = None
        self.tokenizer = None
        self.detector_images = None
        self.segmenter = None
        self.segmenter_processor = None
        if self.device == "mps":
            self.torch.mps.empty_cache()


# The settings of OWLv2's tokenizer its two files may give.
TOKENIZER_SETTINGS = ("unk_token", "bos_token", "eos_token", "pad_token", "model_max_length")


def _processor_settings(told):
    """A processor's settings as its file gives them, without the names of its classes."""
    return {key: value for key, value in told.items() if key not in ("image_processor_type", "processor_class")}
