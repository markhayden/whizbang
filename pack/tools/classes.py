# /// whizbang-tool
# format = "tool-file/1"
# name = "classes"
# version = "0.1.0"
# purpose = "Names what the fixed-class segmenter sees in one box of an aligned pair, before and after: the classes covering the box on each side with their shares, and where its main class changed."
# evidence = ["measurements", "regions"]
# bindings = ["source", "candidate"]
# primary = "class_changed"
# changed = "larger"
# reduce = "worst"
# readings_most = 9
# max_regions = 1
# max_region_label_bytes = 120
# region_kind = "class_difference"
# time_limit_s = 300
# segments_most = 8
# [[measurements]]
# name = "class_changed"
# unit = "ratio of the box"
# min = 0.0
# max = 1.0
# [[measurements]]
# name = "confidence_source"
# unit = "ratio"
# min = 0.0
# max = 1.0
# [[measurements]]
# name = "confidence_candidate"
# unit = "ratio"
# min = 0.0
# max = 1.0
# [[measurements]]
# name = "share_source"
# unit = "ratio of the box"
# min = 0.0
# max = 1.0
# [[measurements]]
# name = "share_candidate"
# unit = "ratio of the box"
# min = 0.0
# max = 1.0
# [[arguments]]
# name = "bbox"
# type = "text"
# purpose = "The box to read: left top right bottom, as fractions of the frame from 0 to 1, separated by spaces; the whole frame when not given."
# ///
"""`classes` (plan mE, task T2, item 4; `SPEC.md` section 16, row E): an experimental tool
file. Ported from squint's `tools/python/src/squint_tools/classes.py` at the commit
`docs/sources.toml` pins, on the catalogue's fixed-class segmenter, `eomt-ade20k`, asked
through the context (`context.labels`).

For a box on an aligned pair (`bbox`, fractions of the frame; the whole frame when not
given, as `whizbang eval tool` gives it), the classes covering the box on each side, each
with its share of the box, the largest first, those under `MIN_SHARE` folded away, at most
`MAX_CLASSES` a side: one reading a class, its `share_source` and `share_candidate`. A
reading of the box itself holds `class_changed`, the share of the box whose class differs
between the two sides (the item's value; larger is changed), and the segmenter's mean
confidence over the box on each side. When the dominant class of the box differs, one
region of kind `class_difference` on `candidate`, the box, labelled `before -> after` and
scored by the lesser of the two confidences. It answers "what was this, and what is it
now" after `regions` has pointed somewhere; it decides nothing.

What changed from squint. The confidence over the box is the share-weighted mean of the
classes' confidences, each the class's mean over its pixels (`labels` gives one figure a
class), where squint averaged each pixel's own. The box is read on a working grid whose
longer side is `GRID`, each working pixel its centre's class; squint read the map at its own
resolution, a quarter of the model's input. `class_changed` is new: squint returned the
shares alone. No image and no summary is returned.
"""

import math

from whizbang_pack import class_groups
from whizbang_pack.masks import one_grid, printable

# Squint's constants (its `classes.py`): classes covering less of the box than this are
# folded away, and at most this many are named a side.
MIN_SHARE = 0.05
MAX_CLASSES = 4

# This file's own: the longer side of the working grid, and how far the aspects of the
# pair may differ.
GRID = 256
ASPECT = 0.01
LABEL_MOST = 120


def box_of(text):
    """The box an argument gives, `[left, top, right, bottom]` as fractions of the frame,
    with area; the whole frame when none is given. `ValueError` for any other."""
    if not text or not text.strip():
        return [0.0, 0.0, 1.0, 1.0]
    try:
        box = [float(v) for v in text.replace(",", " ").split()]
    except ValueError:
        box = []
    if len(box) != 4 or not (0.0 <= box[0] < box[2] <= 1.0 and 0.0 <= box[1] < box[3] <= 1.0):
        raise ValueError("bbox is left top right bottom as fractions of the frame, with left under right and top under bottom")
    return box


def working(width, height):
    longer = max(width, height)
    side = min(GRID, longer)
    return max(1, round(width * side / longer)), max(1, round(height * side / longer))


def cells(box, gw, gh):
    """The working pixels of the box, row by row: at least one."""
    x0, y0 = int(box[0] * gw), int(box[1] * gh)
    x1 = max(x0 + 1, min(gw, math.ceil(box[2] * gw)))
    y1 = max(y0 + 1, min(gh, math.ceil(box[3] * gh)))
    return [y * gw + x for y in range(y0, y1) for x in range(x0, x1)]


def composition(names):
    """The classes covering some cells, largest share first, those under `MIN_SHARE`
    folded away, at most `MAX_CLASSES`; the largest alone when every one is under it."""
    counted = {}
    for name in names:
        counted[name] = counted.get(name, 0) + 1
    ranked = sorted(counted.items(), key=lambda item: (-item[1], str(item[0])))
    shares = [(name, count / len(names)) for name, count in ranked]
    kept = [(name, share) for name, share in shares[:MAX_CLASSES] if share >= MIN_SHARE]
    return kept or shares[:1]


def run(context):
    box = box_of(context.arguments.get("bbox"))
    source, candidate = context.image("source"), context.image("candidate")
    one_grid(source, candidate, ASPECT)
    gw, gh = working(candidate.width, candidate.height)
    inside = cells(box, gw, gh)
    sides = {}
    for side in ("source", "candidate"):
        names, sure = class_groups.on_grid(class_groups.labels_of(context, side), gw, gh)
        boxed = [names[at] or "nothing" for at in inside]
        sides[side] = (boxed, sum(sure[at] for at in inside) / len(inside), composition(boxed))
    before, after = sides["source"], sides["candidate"]
    changed = sum(1 for b, a in zip(before[0], after[0]) if b != a) / len(inside)
    readings = [
        {
            "subject": "the box",
            "values": {
                "class_changed": round(changed, 4),
                "confidence_source": round(min(1.0, before[1]), 4),
                "confidence_candidate": round(min(1.0, after[1]), 4),
            },
        }
    ]
    shares = {}
    for side, (_, _, kept) in (("source", before), ("candidate", after)):
        for name, share in kept:
            shares.setdefault(name, {"share_source": 0.0, "share_candidate": 0.0})["share_" + side] = round(share, 4)
    for name, values in sorted(shares.items(), key=lambda item: -max(item[1].values()))[: 2 * MAX_CLASSES]:
        readings.append({"subject": printable(name, 200), "values": values})
    regions = []
    was, now = before[2][0][0], after[2][0][0]
    if was != now:
        regions.append(
            {
                "frame": "candidate",
                "box": [round(box[0] * candidate.width, 2), round(box[1] * candidate.height, 2), round(box[2] * candidate.width, 2), round(box[3] * candidate.height, 2)],
                "label": printable("%s -> %s" % (was, now), LABEL_MOST),
                "score": round(min(before[1], after[1], 1.0), 3),
            }
        )
    return {"readings": readings, "not_applicable": [], "regions": regions}
