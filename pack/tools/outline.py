# /// whizbang-tool
# format = "tool-file/1"
# name = "outline"
# version = "0.1.0"
# purpose = "Compares each protected subject's shape in the source and the candidate: overlap, areas, how far its boundary moved, and the pieces added or removed, as regions on the candidate."
# evidence = ["measurements", "regions"]
# bindings = ["source", "candidate"]
# primary = "one_minus_iou"
# changed = "larger"
# reduce = "worst"
# readings_most = 16
# max_regions = 6
# max_region_label_bytes = 120
# region_kind = "change_region"
# time_limit_s = 300
# segments_most = 32
# [[measurements]]
# name = "one_minus_iou"
# unit = "ratio"
# min = 0.0
# max = 1.0
# [[measurements]]
# name = "area_source"
# unit = "ratio of the frame"
# min = 0.0
# max = 1.0
# [[measurements]]
# name = "area_candidate"
# unit = "ratio of the frame"
# min = 0.0
# max = 1.0
# [[measurements]]
# name = "area_change"
# unit = "ratio"
# min = -1.0
# max = 1048576.0
# [[measurements]]
# name = "shift_p95_px"
# unit = "px of the source"
# min = 0.0
# max = 1000000.0
# [[measurements]]
# name = "shift_max_px"
# unit = "px of the source"
# min = 0.0
# max = 1000000.0
# [[measurements]]
# name = "added"
# unit = "ratio of the frame"
# min = 0.0
# max = 1.0
# [[measurements]]
# name = "removed"
# unit = "ratio of the frame"
# min = 0.0
# max = 1.0
# [[arguments]]
# name = "job"
# type = "text"
# purpose = "The item's job, in the contract job/1: each requirement that must hold, written as a subject that keeps something, names one."
# [[arguments]]
# name = "phrases"
# type = "text"
# purpose = "Subjects by name, each a phrase, separated by semicolons."
# ///
"""`outline` (plan mE, task T1, item 1; `SPEC.md` section 16, row E): an experimental tool
file. Ported from squint's `tools/python/src/squint_tools/outline.py` at the commit
`docs/sources.toml` pins, its single-class comparison, on SAM 3 in place of squint's
fixed-class segmenter.

`change_map` sees pixels change; it does not say that a pool got wider or that its end was
redrawn round, because those show as slivers among ripples and reflections. This tool asks
the segmenter whizbang answers through the context for each protected subject, by phrase,
on the source and on the candidate (in `try` and `eval tool`, the candidate as `align`
registered it onto the source), and compares the two shapes: their overlap, their areas, how
far the boundary moved, and the pieces added or removed, each a region on `candidate`.

Subjects come from the job's requirements of kind `must_hold`, by per-subject comparison's
rule (`whizbang_pack.subjects`), and from `phrases`. The item's value is the largest of its
subjects' one minus IoU (the declaration's rule, `worst`). A subject on neither picture
(under `PRESENT_NAMED` of both) is not applicable.

What changed from squint. A mask is SAM 3's for the phrase, at the picture's own size, the
union of at most four instances, brought to the working grid by its nearest pixel; squint
thresholded its segmenter's per-class scores, upsampled bilinearly, and could compare every
class at once and read hints of the rules (`class`, `all`, the class groups): those come
with plan mE's task T2 and the rules' `[hints]`. No image is returned. The masks are
compared as Python integers, one bit a pixel, so the standard library is enough: an erosion
or a dilation by a square of three is four shifts, the frame's edge repeated past it, as the
image library squint used does.
"""

import json
import math
import re

from whizbang_pack.masks import Grid, components, mask_of, one_grid, printable
from whizbang_pack.subjects import subjects_of

# Squint's constants, each as it was (its `outline.py`).
# The longer side of the grid the masks are compared on.
WORK = 1024
# A named subject covering less than this share of both frames is not there.
PRESENT_NAMED = 0.0005
# Added or removed pieces thinner than this many working pixels are boundary jitter: the
# mask difference is opened by a square of this side before its pieces are taken.
JITTER = 5
# Fragments of a mask smaller than `SPECK_REL` of the largest fragment of either picture, or
# than `SPECK_ABS` of the frame, are specks and are removed from both before comparing.
SPECK_REL = 0.03
SPECK_ABS = 0.0003
# A piece that is this share of the subject's extent is a change to the subject itself.
WHOLE = 0.3
# A piece on the frame's edge, smaller than `EDGE_SMALL` of the frame, of an object no bigger
# than `EDGE_OBJECT` of it, is usually a partial object cut differently: its score is halved.
EDGE_SMALL = 0.005
EDGE_OBJECT = 0.02
# Pieces smaller than this share of the frame are dropped.
MIN_AREA = 0.0005
# Boundary points compared on each side, at most.
MAX_POINTS = 4000
MAX_REGIONS = 6
# Pieces of one kind, thin along one axis, level across it and no further apart along it
# than this share of the longer side, are parts of one moved edge, scored as one.
CHAIN_GAP = 0.08
# A piece scores fully by area at this share of the frame, and by movement at this many
# pixels of the frame.
AREA_FULL = 0.005
SHIFT_FULL = 30.0
# The longest label of a region.
LABEL_MOST = 120

# This file's own.
# Instances asked for each subject on each picture, and subjects an item: per-subject
# comparison's.
MOST = 4
SUBJECTS_MOST = 16
# The source and the candidate are one grid when their aspects differ by at most this.
ASPECT = 0.01
# The side, in working pixels, of the cells boundary points are sorted into.
CELL = 32

_ONE = re.compile("1")

def despeckled_pair(grid, before, after):
    """Both masks without the fragments under one bar, set by the largest fragment of
    either: so a fragment is kept or dropped on both sides alike."""
    found = [components(grid, m)[0] for m in (before, after)]
    largest = max((p[0] for pieces in found for p in pieces), default=0) / grid.n
    bar = max(SPECK_ABS, SPECK_REL * largest)
    kept = []
    for m, pieces in zip((before, after), found):
        dropped = [run for p in pieces if p[0] / grid.n < bar for run in p[5]]
        kept.append(m & ~grid.of_runs(dropped) if dropped else m)
    return kept[0], kept[1]


def points(grid, edge):
    """The edge's pixels as (x, y), row by row, at most `MAX_POINTS` evenly taken."""
    at = [found.start() for found in _ONE.finditer(grid.text(edge))]
    if len(at) > MAX_POINTS:
        step = (len(at) - 1) / (MAX_POINTS - 1)
        at = [at[int(k * step)] for k in range(MAX_POINTS)]
    return [(i % grid.w, i // grid.w) for i in at]


def _ring(cx, cy, r):
    if r == 0:
        return [(cx, cy)]
    cells = []
    for dx in range(-r, r + 1):
        cells += [(cx + dx, cy - r), (cx + dx, cy + r)]
    for dy in range(-r + 1, r):
        cells += [(cx - r, cy + dy), (cx + r, cy + dy)]
    return cells


def nearest(grid, a, b):
    """For each point of `a`, the distance to the nearest point of `b`; infinite when `b`
    is empty. The points of `b` are sorted into square cells, and the cells round each
    point of `a` are looked in, ring by ring, until no nearer point can be left."""
    if not b:
        return [math.inf] * len(a)
    cells = {}
    for x, y in b:
        cells.setdefault((x // CELL, y // CELL), []).append((x, y))
    reach = max(grid.w, grid.h) // CELL + 1
    out = []
    for x, y in a:
        cx, cy, best, r = x // CELL, y // CELL, math.inf, 0
        while True:
            for cell in _ring(cx, cy, r):
                for qx, qy in cells.get(cell, ()):
                    d = (qx - x) * (qx - x) + (qy - y) * (qy - y)
                    if d < best:
                        best = d
            if best <= (r * CELL) ** 2 or r > reach:
                break
            r += 1
        out.append(math.sqrt(best))
    return out


def _percentile(values, share):
    """The value at this share of the sorted values, read between the two nearest."""
    ordered = sorted(values)
    at = share * (len(ordered) - 1)
    low = int(at)
    high = min(low + 1, len(ordered) - 1)
    return ordered[low] + (ordered[high] - ordered[low]) * (at - low)


def _in_line(a, b, h, w):
    """Two piece boxes (fractions) that are thin along one axis, level with each other
    across it and at most `CHAIN_GAP` apart along it: parts of one moved edge."""
    ax0, ay0, ax1, ay1 = a[0] * w, a[1] * h, a[2] * w, a[3] * h
    bx0, by0, bx1, by1 = b[0] * w, b[1] * h, b[2] * w, b[3] * h
    gap, near = CHAIN_GAP * max(h, w), 2 * JITTER

    def across(lo0, hi0, lo1, hi1):
        return min(hi0, hi1) - max(lo0, lo1) >= -near

    def along(lo0, hi0, lo1, hi1):
        return max(lo0, lo1) - min(hi0, hi1) <= gap

    wide = (ax1 - ax0) >= 2 * (ay1 - ay0) and (bx1 - bx0) >= 2 * (by1 - by0)
    tall = (ay1 - ay0) >= 2 * (ax1 - ax0) and (by1 - by0) >= 2 * (bx1 - bx0)
    return (wide and across(ay0, ay1, by0, by1) and along(ax0, ax1, bx0, bx1)) or (
        tall and across(ax0, ax1, bx0, bx1) and along(ay0, ay1, by0, by1)
    )


def chained(pieces, h, w):
    """Pieces of one kind in line along one edge, joined into one: an edge moved 10 px comes
    back from the segmenter as several slivers, each too small to score, that are one
    change."""
    groups = [[p] for p in pieces]
    merged = True
    while merged:
        merged = False
        for i in range(len(groups)):
            for j in range(i + 1, len(groups)):
                if groups[i][0]["kind"] != groups[j][0]["kind"]:
                    continue
                if any(_in_line(p["bbox"], q["bbox"], h, w) for p in groups[i] for q in groups[j]):
                    groups[i] += groups.pop(j)
                    merged = True
                    break
            if merged:
                break
    out = []
    for group in groups:
        if len(group) == 1:
            out.append(group[0])
            continue
        shifts = [p["shift_px"] for p in group if p["shift_px"] is not None]
        out.append(
            {
                "kind": group[0]["kind"],
                "bbox": [
                    min(p["bbox"][0] for p in group),
                    min(p["bbox"][1] for p in group),
                    max(p["bbox"][2] for p in group),
                    max(p["bbox"][3] for p in group),
                ],
                "area": sum(p["area"] for p in group),
                "shift_px": max(shifts) if shifts else None,
                "edge": any(p["edge"] for p in group),
                "object": max(p["object"] for p in group),
                "parts": len(group),
            }
        )
    return out


def compare(grid, before, after, scale):
    """Two masks on the grid: what they come to, and the pieces added and removed, each a
    box (fractions of the grid) with the largest boundary shift inside it, scored and
    sorted, best first. `scale` brings working pixels to pixels of the frame."""
    w, h, n = grid.w, grid.h, grid.n
    inter, union = (before & after).bit_count(), (before | after).bit_count()
    a0, a1 = before.bit_count() / n, after.bit_count() / n
    pb, pa = points(grid, grid.boundary(before)), points(grid, grid.boundary(after))
    # Both ways: how far the new boundary is from the old and the old from the new, so a
    # piece removed shows as much as a piece added.
    shifts = nearest(grid, pa, pb) + nearest(grid, pb, pa)
    at = pa + pb
    finite = [s for s in shifts if s != math.inf]
    out = {
        "iou": inter / union if union else 1.0,
        "area_before": a0,
        "area_after": a1,
        "area_change": (a1 / a0 - 1.0) if a0 else (0.0 if not a1 else math.inf),
        "shift_p95_px": _percentile(finite, 0.95) * scale if finite else 0.0,
        "shift_max_px": max(finite) * scale if finite else 0.0,
    }
    # The objects of the subject, both pictures' extents joined: what each piece is part of.
    whole, whole_rows = components(grid, before | after)
    pieces = []
    for kind, diff in (("added", after & ~before), ("removed", before & ~after)):
        diff = grid.opened(diff, JITTER)
        out[kind] = diff.bit_count() / n
        for pixels, x0, y0, x1, y1, runs in components(grid, diff)[0]:
            area = pixels / n
            if area < MIN_AREA:
                continue
            local = [s for (px, py), s in zip(at, shifts) if x0 <= px < x1 and y0 <= py < y1 and s != math.inf]
            ry, rx, _ = runs[0]
            part = next(i for start, end, i in whole_rows[ry] if start <= rx < end)
            pieces.append(
                {
                    "kind": kind,
                    "bbox": [x0 / w, y0 / h, x1 / w, y1 / h],
                    "area": area,
                    # None: no edge of the other picture near it to measure against.
                    "shift_px": max(local) * scale if local else None,
                    "edge": x0 <= 1 or y0 <= 1 or x1 >= w - 1 or y1 >= h - 1,
                    "object": whole[part][0] / n,
                }
            )
    pieces = chained(pieces, h, w)
    extent = max(a0, a1)
    for p in pieces:
        moved = p["shift_px"] or 0.0
        score = 0.5 * min(1.0, p["area"] / AREA_FULL) + 0.5 * min(1.0, moved / SHIFT_FULL)
        if extent:
            score = max(score, min(1.0, p["area"] / extent / WHOLE))
        if p["edge"] and p["area"] < EDGE_SMALL and p["object"] <= EDGE_OBJECT:
            score *= 0.5
        p["score"] = round(min(1.0, score), 3)
    pieces.sort(key=lambda p: (-p["score"], -p["area"]))
    out["pieces"] = pieces
    return out


def working(width, height):
    """The working grid of a picture of this size: its longer side `WORK`."""
    longer = max(width, height)
    return max(1, round(WORK * width / longer)), max(1, round(WORK * height / longer))


def values_of(found):
    """A subject's reading, each value rounded as squint's were."""
    values = {
        "one_minus_iou": max(0.0, 1.0 - found["iou"]),
        "area_source": found["area_before"],
        "area_candidate": found["area_after"],
        "shift_p95_px": found["shift_p95_px"],
        "shift_max_px": found["shift_max_px"],
        "added": found["added"],
        "removed": found["removed"],
    }
    if found["area_before"]:
        values["area_change"] = found["area_change"]
    return {name: round(value, 4) for name, value in values.items()}


def region_of(subject, piece, candidate):
    """A piece as a region on the candidate, in its pixels."""
    x0, y0, x1, y1 = piece["bbox"]
    if piece["shift_px"] is not None:
        said = "%s %s, boundary moved up to %.0f px" % (subject, piece["kind"], piece["shift_px"])
    else:
        said = "%s %s, with no edge of it in the other image" % (subject, piece["kind"])
    return {
        "frame": "candidate",
        "box": [round(x0 * candidate.width, 2), round(y0 * candidate.height, 2), round(x1 * candidate.width, 2), round(y1 * candidate.height, 2)],
        "label": printable(said, LABEL_MOST),
        "score": piece["score"],
    }


def run(context):
    arguments = context.arguments
    source, candidate = context.image("source"), context.image("candidate")
    one_grid(source, candidate, ASPECT)
    named, not_applicable = subjects_of(json.loads(arguments["job"])) if arguments.get("job") else ([], [])
    named += [p.strip() for p in (arguments.get("phrases") or "").split(";") if p.strip()]
    grid = Grid(*working(source.width, source.height))
    scale = max(source.width, source.height) / max(grid.w, grid.h)
    readings, found_pieces = [], []
    for phrase in named[:SUBJECTS_MOST]:
        subject = printable(phrase, 200)
        before = grid.mask(*mask_of(context, "source", phrase, MOST))
        after = grid.mask(*mask_of(context, "candidate", phrase, MOST))
        before, after = despeckled_pair(grid, before, after)
        if max(before.bit_count(), after.bit_count()) / grid.n < PRESENT_NAMED:
            not_applicable.append(subject)
            continue
        found = compare(grid, before, after, scale)
        readings.append({"subject": subject, "values": values_of(found)})
        found_pieces += [(subject, piece) for piece in found["pieces"]]
    found_pieces.sort(key=lambda sp: (-sp[1]["score"], -sp[1]["area"]))
    regions = [region_of(subject, piece, candidate) for subject, piece in found_pieces[:MAX_REGIONS]]
    return {"readings": readings, "not_applicable": not_applicable[:64], "regions": regions}
