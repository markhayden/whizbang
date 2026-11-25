# /// whizbang-tool
# format = "tool-file/1"
# name = "regions"
# version = "0.1.0"
# purpose = "Finds where the candidate differs from the source, with high recall: photometric change and the protected subjects' masks disagreeing, boxed, labelled and scored, as regions on the candidate."
# evidence = ["measurements", "regions"]
# bindings = ["source", "candidate"]
# primary = "strongest"
# changed = "larger"
# reduce = "worst"
# readings_most = 1
# max_regions = 12
# max_region_label_bytes = 120
# region_kind = "change_region"
# time_limit_s = 300
# segments_most = 32
# [[measurements]]
# name = "strongest"
# unit = "score"
# min = 0.0
# max = 1.0
# [[measurements]]
# name = "changed_fraction"
# unit = "ratio of the frame"
# min = 0.0
# max = 1.0
# [[measurements]]
# name = "photometric_fraction"
# unit = "ratio of the frame"
# min = 0.0
# max = 1.0
# [[measurements]]
# name = "subject_disagreement"
# unit = "ratio of the subjects' pixels"
# min = 0.0
# max = 1.0
# [[measurements]]
# name = "candidates"
# unit = "count"
# min = 0.0
# max = 1000000.0
# [[arguments]]
# name = "job"
# type = "text"
# purpose = "The item's job, in the contract job/1: each requirement that must hold, written as a subject that keeps something, names one."
# [[arguments]]
# name = "phrases"
# type = "text"
# purpose = "Subjects by name, each a phrase, separated by semicolons."
# [[arguments]]
# name = "preserve"
# type = "text"
# purpose = "Things whose change matters more, each a phrase, separated by semicolons: a region on one scores higher."
# [[arguments]]
# name = "ignore"
# type = "text"
# purpose = "Things whose change matters less, each a phrase, separated by semicolons: a region on one scores lower."
# ///
"""`regions` (plan mE, task T1, item 2; `SPEC.md` section 16, row E): an experimental tool
file. Ported from squint's `tools/python/src/squint_tools/regions.py` at the commit
`docs/sources.toml` pins, on SAM 3 in place of squint's fixed-class segmenter.

Where an aligned pair differs, as high-recall pointers. Two maps on one working grid are
joined: the photometric change map, as `change_map` computes it (the absolute difference of
the locally contrast-normalised luminances, changed past `CHANGED`), and where squint read
its segmenter's class disagreement, the masks of the protected subjects disagreeing (a
subject's pixel on one side and not the other), each subject asked of the segmenter by
phrase through the context. Each map is cleaned of specks; its 4-connected pieces become
boxed regions on `candidate`, labelled by the subject they lie on, scored by how strongly
they changed, raised for a `preserve` subject and lowered for an `ignore` one (the rules'
`[hints.regions]`), and capped at `MAX_REGIONS` by score. The item's value is the score of
its strongest region, 0 when it finds none.

Subjects come from the job's requirements of kind `must_hold` (`whizbang_pack.subjects`),
from `phrases`, and from `preserve` and `ignore`, at most `SUBJECTS_MOST`.

What changed from squint. Squint's labels were classes ("floor -> swimming pool") and its
hints classes; here they are the subjects' phrases, and the class labels come with plan mE's
task T2. The photometric map is `change_map`'s; squint's own `regions` added a blurred
colour difference and took a grade over the whole frame out first, which `change_map` does
not. A picture is brought to the grid by the mean of up to four by four samples of each
working pixel, where `change_map` resamples by a triangle filter. No image is returned.
"""

import json

from whizbang_pack.masks import Grid, components, mask_of, one_grid, printable
from whizbang_pack.subjects import subjects_of

# Squint's constants (its `regions.py`).
MAX_REGIONS = 12
# The longer side of the grid the maps are compared on.
GRID = 256
# Components smaller than this share of the frame are noise.
MIN_AREA = 0.001
# A subjects' disagreement counts in full only with at least this photometric strength
# behind it.
SUPPORT = 0.2
# Two regions overlapping by more than this IoU are one: a disagreement a photometric
# region already covers is not reported again.
OVERLAP = 0.3
# A preserve subject's region scores this much more, an ignore subject's this much.
PRESERVE = 1.25
IGNORE = 0.5
# `change_map`'s constants (`crates/whizbang-squint/src/compare.rs`, squint's): a normalised
# residual beyond `CHANGED` counts as changed, and one at `SATURATION` saturates the score;
# the local contrast normalisation's radius and its floor.
CHANGED = 1.0
SATURATION = 3.0
RADIUS = 4
EPS = 1e-3

# This file's own.
MOST = 4
SUBJECTS_MOST = 16
ASPECT = 0.01
# The most samples taken along each side of a working pixel.
SAMPLES = 4
LABEL_MOST = 120


def working(width, height):
    """The working grid of a picture: its longer side `GRID`, or its own when smaller."""
    longer = max(width, height)
    side = min(GRID, longer)
    return max(2, round(width * side / longer)), max(2, round(height * side / longer))


def luminance(image, gw, gh):
    """The picture's luminance (0 to 1, as `change_map` weighs red, green and blue) on the
    grid: each working pixel the mean of up to `SAMPLES` by `SAMPLES` samples across it."""
    w, h, rgba = image.width, image.height, image.rgba
    kx = max(1, min(SAMPLES, w // gw))
    ky = max(1, min(SAMPLES, h // gh))
    xs = [[min(w - 1, int((x + (i + 0.5) / kx) * w / gw)) for i in range(kx)] for x in range(gw)]
    ys = [[min(h - 1, int((y + (j + 0.5) / ky) * h / gh)) for j in range(ky)] for y in range(gh)]
    out = []
    for y in range(gh):
        rows = [sy * w for sy in ys[y]]
        for x in range(gw):
            total = 0
            for base in rows:
                for sx in xs[x]:
                    at = 4 * (base + sx)
                    total += 0.299 * rgba[at] + 0.587 * rgba[at + 1] + 0.114 * rgba[at + 2]
            out.append(total / (255.0 * kx * ky))
    return out


def blur3(v, w, h):
    """`change_map`'s light blur: 1 2 1 across, then down, the edge repeated."""
    tmp = [0.0] * (w * h)
    for y in range(h):
        row = y * w
        for x in range(w):
            tmp[row + x] = 0.25 * (v[row + max(0, x - 1)] + 2.0 * v[row + x] + v[row + min(w - 1, x + 1)])
    out = [0.0] * (w * h)
    for y in range(h):
        up, down = max(0, y - 1) * w, min(h - 1, y + 1) * w
        for x in range(w):
            out[y * w + x] = 0.25 * (tmp[up + x] + 2.0 * tmp[y * w + x] + tmp[down + x])
    return out


def lcn(v, w, h, radius=RADIUS, eps=EPS):
    """`change_map`'s local contrast normalisation: each value less the mean of the window
    of `radius` round it, over the window's standard deviation and `eps`."""
    stride = w + 1
    total = [0.0] * (stride * (h + 1))
    squares = [0.0] * (stride * (h + 1))
    for y in range(h):
        run = run2 = 0.0
        for x in range(w):
            value = v[y * w + x]
            run += value
            run2 += value * value
            total[(y + 1) * stride + x + 1] = total[y * stride + x + 1] + run
            squares[(y + 1) * stride + x + 1] = squares[y * stride + x + 1] + run2
    out = [0.0] * (w * h)
    for y in range(h):
        y0, y1 = max(0, y - radius), min(h, y + radius + 1)
        for x in range(w):
            x0, x1 = max(0, x - radius), min(w, x + radius + 1)
            n = (x1 - x0) * (y1 - y0)
            a, b, c, d = y1 * stride + x1, y0 * stride + x1, y1 * stride + x0, y0 * stride + x0
            mean = (total[a] - total[b] - total[c] + total[d]) / n
            var = max(0.0, (squares[a] - squares[b] - squares[c] + squares[d]) / n - mean * mean)
            out[y * w + x] = (v[y * w + x] - mean) / (var + eps) ** 0.5
    return out


def cleaned(grid, m):
    """Squint's cleaning of a map: closed by a square of three, then opened by one of five."""
    m = grid.dilated(m)
    m = grid.eroded(grid.eroded(m))
    return grid.dilated(grid.dilated(m))


def _iou(a, b):
    ix = max(0.0, min(a[2], b[2]) - max(a[0], b[0]))
    iy = max(0.0, min(a[3], b[3]) - max(a[1], b[1]))
    inter = ix * iy
    union = (a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1]) - inter
    return inter / union if union > 0 else 0.0


def _phrases(text):
    return [p.strip() for p in (text or "").split(";") if p.strip()]


def find(grid, residual, photometric, subjects, preserve, ignore):
    """The regions of two maps on the grid, best first: each a box (fractions of the grid),
    a label, a score and its area. `residual` is the normalised difference of each working
    pixel; `subjects` each `(name, source mask, candidate mask)`."""
    disagree = 0
    for _, before, after in subjects:
        disagree |= before ^ after
    photometric, disagree = cleaned(grid, photometric), cleaned(grid, disagree)
    w, h, n = grid.w, grid.h, grid.n
    found = []
    for source_mask, by_subject in ((photometric, False), (disagree, True)):
        for pixels, x0, y0, x1, y1, runs in components(grid, source_mask)[0]:
            area = pixels / n
            if area < MIN_AREA:
                continue
            box = [x0 / w, y0 / h, x1 / w, y1 / h]
            if by_subject and any(_iou(box, r["bbox"]) > OVERLAP for r in found):
                continue
            piece = grid.of_runs(runs)
            strength = sum(sum(residual[y * w + a : y * w + b]) for y, a, b in runs) / pixels
            strength = min(1.0, max(0.0, strength / SATURATION))
            share = (piece & disagree).bit_count() / pixels
            # The subject it lies on most, on either side, and which side holds it.
            on, before_on, after_on = None, 0, 0
            for name, before, after in subjects:
                b, a = (piece & before).bit_count(), (piece & after).bit_count()
                if max(b, a) > max(before_on, after_on):
                    on, before_on, after_on = name, b, a
            if by_subject:
                # A disagreement with no change in the pixels behind it is the segmenter
                # changing its mind, not the picture: it counts in proportion to the
                # photometric support.
                share *= min(1.0, strength / SUPPORT)
            score = min(1.0, 0.5 * strength + 0.5 * max(strength, share))
            if on in preserve:
                score = min(1.0, score * PRESERVE)
            if on in ignore:
                score *= IGNORE
            if on is None:
                label = "changed"
            elif by_subject:
                label = "%s %s" % (on, "added" if after_on > before_on else "removed" if before_on > after_on else "moved")
            else:
                label = "%s changed" % on
            found.append({"bbox": box, "label": printable(label, LABEL_MOST), "score": round(score, 3), "area": area})
    found.sort(key=lambda r: (-r["score"], -r["area"]))
    return found, photometric | disagree, photometric


def run(context):
    arguments = context.arguments
    source, candidate = context.image("source"), context.image("candidate")
    one_grid(source, candidate, ASPECT)
    gw, gh = working(source.width, source.height)
    grid = Grid(gw, gh)
    a = lcn(blur3(luminance(source, gw, gh), gw, gh), gw, gh)
    b = lcn(blur3(luminance(candidate, gw, gh), gw, gh), gw, gh)
    residual = [abs(x - y) for x, y in zip(a, b)]
    photometric = int("".join("1" if d > CHANGED else "0" for d in residual), 2)
    named = subjects_of(json.loads(arguments["job"]))[0] if arguments.get("job") else []
    preserve, ignore = _phrases(arguments.get("preserve")), _phrases(arguments.get("ignore"))
    named += _phrases(arguments.get("phrases")) + preserve + ignore
    subjects, seen = [], set()
    for phrase in named:
        if phrase in seen or len(subjects) >= SUBJECTS_MOST:
            continue
        seen.add(phrase)
        before = grid.mask(*mask_of(context, "source", phrase, MOST))
        after = grid.mask(*mask_of(context, "candidate", phrase, MOST))
        subjects.append((printable(phrase, 200), before, after))
    found, changed, photometric = find(grid, residual, photometric, subjects, set(preserve), set(ignore))
    covered = 0
    disagree = 0
    for _, before, after in subjects:
        covered |= before | after
        disagree |= before ^ after
    kept = found[:MAX_REGIONS]
    values = {
        "strongest": kept[0]["score"] if kept else 0.0,
        "changed_fraction": round(changed.bit_count() / grid.n, 4),
        "photometric_fraction": round(photometric.bit_count() / grid.n, 4),
        "subject_disagreement": round(disagree.bit_count() / covered.bit_count(), 4) if covered else 0.0,
        "candidates": float(len(found)),
    }
    regions = [
        {
            "frame": "candidate",
            "box": [round(x0 * candidate.width, 2), round(y0 * candidate.height, 2), round(x1 * candidate.width, 2), round(y1 * candidate.height, 2)],
            "label": r["label"],
            "score": r["score"],
        }
        for r in kept
        for x0, y0, x1, y1 in [r["bbox"]]
    ]
    return {"readings": [{"subject": "the pair", "values": values}], "not_applicable": [], "regions": regions}
