# /// whizbang-tool
# format = "tool-file/1"
# name = "per_subject"
# version = "0.1.0"
# purpose = "Finds each named subject in the source and in the candidate and reports how far their two masks disagree, one minus their IoU."
# evidence = "measurements"
# bindings = ["source", "candidate"]
# primary = "one_minus_iou"
# changed = "larger"
# reduce = "worst"
# readings_most = 16
# time_limit_s = 300
# segments_most = 32
# [[measurements]]
# name = "one_minus_iou"
# unit = "ratio"
# min = 0.0
# max = 1.0
# [[measurements]]
# name = "present_source"
# unit = "yes or no"
# min = 0.0
# max = 1.0
# [[measurements]]
# name = "present_candidate"
# unit = "yes or no"
# min = 0.0
# max = 1.0
# [[measurements]]
# name = "area_source"
# unit = "ratio"
# min = 0.0
# max = 1.0
# [[measurements]]
# name = "area_candidate"
# unit = "ratio"
# min = 0.0
# max = 1.0
# [[measurements]]
# name = "shift"
# unit = "ratio of the diagonal"
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
# [[arguments]]
# name = "points"
# type = "text"
# purpose = "Subjects by location, each a point as X Y in pixels of the source, separated by semicolons."
# [[arguments]]
# name = "boxes"
# type = "text"
# purpose = "Subjects by location, each a box as left top right bottom in pixels of the source, separated by semicolons."
# ///
"""Per-subject comparison (plan 2C, task X1, item 7; `SPEC.md` section 7): an experimental
tool file. Each subject is found with the segmenter whizbang answers through the context, in
the source and in the candidate (in `try` and `eval tool`, the candidate as `align`
registered it onto the source), by name (a phrase) or by location (a point or a box, in
pixels of the source, scaled to the candidate). For each it reports whether it is there in
each, its area in each, how far its centre moved, and one minus the IoU of its two masks:
0 when both are empty, 1 when one is. The item's value is the largest of its subjects'
(the declaration's rule, `worst`).

Subjects come from the job's requirements of kind `must_hold`, never its labels: the phrase
before " keeps " in the requirement's text, its leading article dropped ("The rectangular
swimming pool keeps its identity, ..." names "rectangular swimming pool"). A requirement
not of that form is reported as not applicable, by its id. The standard library only.
"""

import json

KEEPS = " keeps "
ARTICLES = ("the ", "a ", "an ")
MOST = 4
SUBJECTS_MOST = 16


def subjects_of(job):
    """The subjects a job names, each a phrase, and the ids of its requirements that name
    none: only requirements of kind must_hold are read, and only their text."""
    named, not_applicable = [], []
    for requirement in job.get("requirements", []):
        if requirement.get("kind") != "must_hold":
            continue
        text = str(requirement.get("text", ""))
        before, found, _ = text.partition(KEEPS)
        phrase = before.strip()
        for article in ARTICLES:
            if phrase.lower().startswith(article):
                phrase = phrase[len(article) :].strip()
                break
        if found and phrase:
            named.append(phrase)
        else:
            not_applicable.append(str(requirement.get("id", "a requirement")))
    return named, not_applicable


def _listed(text, count):
    """Each group of `count` numbers in a text of groups separated by semicolons."""
    groups = []
    for part in (text or "").split(";"):
        numbers = [float(n) for n in part.replace(",", " ").split()]
        if numbers:
            if len(numbers) != count:
                raise ValueError("a point is two numbers and a box four")
            groups.append(numbers)
    return groups


def _resized(mask, width, height, to_width, to_height):
    """A mask of 0 and 255 bytes brought to another size by its nearest pixel."""
    if (width, height) == (to_width, to_height):
        return mask
    xs = [min(width - 1, (x * width) // to_width) for x in range(to_width)]
    out = bytearray(to_width * to_height)
    for y in range(to_height):
        start = min(height - 1, (y * height) // to_height) * width
        row = mask[start : start + width]
        out[y * to_width : (y + 1) * to_width] = bytes(row[x] for x in xs)
    return bytes(out)


def _centre(mask, width, height):
    """The centre of what is on, in pixels; None when nothing is."""
    count = sx = sy = 0
    for y in range(height):
        row = mask[y * width : (y + 1) * width]
        on = row.count(255)
        if on:
            count += on
            sy += on * (y + 0.5)
            sx += sum(x + 0.5 for x, v in enumerate(row) if v)
    if not count:
        return None
    return sx / count, sy / count


def compared(source, candidate, width, height):
    """What one subject's two masks, each `width` by `height` bytes of 0 and 255, come to."""
    a, b = int.from_bytes(source, "big"), int.from_bytes(candidate, "big")
    on_a, on_b = a.bit_count() // 8, b.bit_count() // 8
    both, either = (a & b).bit_count() // 8, (a | b).bit_count() // 8
    if not either:
        disagree = 0.0
    elif not (on_a and on_b):
        disagree = 1.0
    else:
        disagree = 1.0 - both / either
    total = float(width * height)
    centres = _centre(source, width, height), _centre(candidate, width, height)
    shift = 0.0
    if centres[0] and centres[1]:
        diagonal = (width**2 + height**2) ** 0.5
        shift = min(1.0, ((centres[0][0] - centres[1][0]) ** 2 + (centres[0][1] - centres[1][1]) ** 2) ** 0.5 / diagonal)
    return {
        "one_minus_iou": disagree,
        "present_source": 1.0 if on_a else 0.0,
        "present_candidate": 1.0 if on_b else 0.0,
        "area_source": on_a / total,
        "area_candidate": on_b / total,
        "shift": shift,
    }


def _mask(context, image, query):
    """The union of what the segmenter finds for one query in the named image, as bytes
    of 0 and 255 on that image's own size."""
    found = context.segment(image, most=MOST, **query)
    shown = context.image(image)
    mask = 0
    for instance in found:
        mask |= int.from_bytes(instance.mask, "big")
    return mask.to_bytes(shown.width * shown.height, "big"), shown.width, shown.height


def read(context, subject, source_query, candidate_query):
    """One subject's reading: its masks in each image, the candidate's brought to the
    source's size."""
    a, width, height = _mask(context, "source", source_query)
    b, cw, ch = _mask(context, "candidate", candidate_query)
    b = _resized(b, cw, ch, width, height)
    return {"subject": subject, "values": compared(a, b, width, height)}


def run(context):
    arguments = context.arguments
    source, candidate = context.image("source"), context.image("candidate")
    sx, sy = candidate.width / source.width, candidate.height / source.height
    readings, not_applicable = [], []
    if arguments.get("job"):
        named, not_applicable = subjects_of(json.loads(arguments["job"]))
    else:
        named = []
    named += [p.strip() for p in (arguments.get("phrases") or "").split(";") if p.strip()]
    asked = [(phrase, {"phrase": phrase}, {"phrase": phrase}) for phrase in named]
    for x, y in _listed(arguments.get("points"), 2):
        asked.append(("point %g %g" % (x, y), {"point": (x, y)}, {"point": (x * sx, y * sy)}))
    for l, t, r, b in _listed(arguments.get("boxes"), 4):
        asked.append(("box %g %g %g %g" % (l, t, r, b), {"box": (l, t, r, b)}, {"box": (l * sx, t * sy, r * sx, b * sy)}))
    for subject, in_source, in_candidate in asked[:SUBJECTS_MOST]:
        readings.append(read(context, subject[:200], in_source, in_candidate))
    return {"readings": readings, "not_applicable": not_applicable[:64]}
