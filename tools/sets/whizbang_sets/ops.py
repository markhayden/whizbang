"""Construction ops: each turns a source picture into a candidate and says what the change
means under the rules, which is the draft label. The label comes from the construction
alone: no judge and no tool looks at the picture. The ops that change pixels themselves need
no generator; `edit`, `restyle` and `generate` ask the family's generator, and an edit's
region is drawn from the pixel difference.

Ported from squint `scripts/sets/src/squint_sets/ops.py`.
"""

from __future__ import annotations

import io
import math
from dataclasses import dataclass, field
from typing import Any

from PIL import Image, ImageEnhance

from . import regions

#: The reality-check defects an edit or a generation can plant, and the rule each breaks.
REALITY_DEFECTS = {"anatomy": "RE1", "shadow": "RE2", "text": "RE3", "merge": "RE4"}


@dataclass
class DraftLabel:
    expected: str
    failure_type: str | None
    obligation_ids: list[str]
    words: str
    region_kind: str | None = None
    bbox: list[float] | None = None
    notes: dict[str, Any] = field(default_factory=dict)


def _load(b: bytes) -> Image.Image:
    return Image.open(io.BytesIO(b)).convert("RGB")


def _png(im: Image.Image) -> bytes:
    buf = io.BytesIO()
    im.save(buf, format="PNG")
    return buf.getvalue()


def apply(op: dict[str, Any], source: bytes, generator, seed: int) -> tuple[bytes, DraftLabel]:
    """The op's candidate and draft label, then any delivery damage the op names (`jpeg`,
    `resample`), which changes no label: a pass saved again or rescaled is still a pass."""
    out, label = _apply(op, source, generator, seed)
    if "jpeg" in op or "resample" in op:
        out, said = deliver(out, op.get("jpeg"), op.get("resample"))
        label.words += f"; delivered {said}"
        label.notes["delivery"] = said
    return out, label


def deliver(image: bytes, jpeg: Any = None, resample: Any = None) -> tuple[bytes, str]:
    """The picture as a delivery would damage it: scaled by `resample` and back (detail
    lost), then saved as JPEG at quality `jpeg`; as PNG at its own size, with what was done."""
    im = _load(image)
    said = []
    if resample is not None:
        f = float(resample)
        if not 0.25 <= f < 1.0:
            raise ValueError(f"resample must be in [0.25, 1), got {f}")
        w, h = im.size
        im = im.resize((max(1, round(w * f)), max(1, round(h * f))), Image.Resampling.LANCZOS)
        im = im.resize((w, h), Image.Resampling.BICUBIC)
        said.append(f"downscaled to {int(f * 100)}% and upscaled back")
    if jpeg is not None:
        q = int(jpeg)
        if not 20 <= q <= 95:
            raise ValueError(f"jpeg quality must be in [20, 95], got {q}")
        buf = io.BytesIO()
        im.save(buf, format="JPEG", quality=q)
        im = _load(buf.getvalue())
        said.append(f"saved again as JPEG at quality {q}")
    return _png(im), " and ".join(said)


Mat = list[list[float]]


def _mul(a: Mat, b: Mat) -> Mat:
    return [[sum(a[i][k] * b[k][j] for k in range(3)) for j in range(3)] for i in range(3)]


def _inv(m: Mat) -> Mat:
    (a, b, c), (d, e, f), (g, h, i) = m
    det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g)
    adj = [
        [e * i - f * h, c * h - b * i, b * f - c * e],
        [f * g - d * i, a * i - c * g, c * d - a * f],
        [d * h - e * g, b * g - a * h, a * e - b * d],
    ]
    return [[v / det for v in row] for row in adj]


def _about_centre(m: Mat, w: int, h: int) -> Mat:
    """`m` applied about the picture's centre rather than its corner."""
    to = [[1, 0, w / 2], [0, 1, h / 2], [0, 0, 1]]
    back = [[1, 0, -w / 2], [0, 1, -h / 2], [0, 0, 1]]
    return _mul(to, _mul(m, back))


def _filled(im: Image.Image, h: Mat) -> Image.Image:
    """The picture under the homography `h` (source pixels to candidate pixels) at its own
    size, zoomed about the centre just enough that every corner of the output comes from
    inside the source, so the frame has no empty wedge. At most 400 steps."""
    w, hh = im.size
    inv: Mat = _inv(h)
    for step in range(400):
        z = 1 + 0.0025 * step
        inv = _inv(_mul(_about_centre([[z, 0, 0], [0, z, 0], [0, 0, 1]], w, hh), h))
        inside = True
        for x, y in ((0, 0), (w, 0), (0, hh), (w, hh)):
            px, py, pw = (inv[r][0] * x + inv[r][1] * y + inv[r][2] for r in range(3))
            if not (-0.5 <= px / pw <= w + 0.5 and -0.5 <= py / pw <= hh + 0.5):
                inside = False
                break
        if inside:
            break
    coeffs = [inv[r][c] / inv[2][2] for r in range(3) for c in range(3)][:8]
    return im.transform((w, hh), Image.Transform.PERSPECTIVE, coeffs, Image.Resampling.BICUBIC)


def _structural(failure: str, words: str) -> DraftLabel:
    return DraftLabel(
        expected="fail",
        failure_type=failure,
        obligation_ids=["SF2"],
        words=words,
        region_kind="whole_image",
    )


def _defect_rule(defect: str, op_kind: str) -> str:
    rule = REALITY_DEFECTS.get(defect)
    if rule is None:
        raise ValueError(f"{op_kind}: defect must be one of {sorted(REALITY_DEFECTS)}, got {defect!r}")
    return rule


def _needs(generator, op_kind: str) -> None:
    if generator is None:
        raise ValueError(f"{op_kind}: the family's generator is needed")


def _generate(op: dict[str, Any], generator, seed: int) -> tuple[bytes, DraftLabel]:
    # The candidate from its own prompt (reality checks: a defect the generators draw when
    # asked for it outright but leave out when asked to edit it in). No region: the whole.
    _needs(generator, "generate")
    prompt = op["prompt"]
    out = generator.generate(prompt, seed, int(op.get("size", 1024)))
    defect = op.get("defect")
    if defect:
        return out, DraftLabel(
            expected="fail",
            failure_type=defect,
            obligation_ids=[_defect_rule(defect, "generate")],
            words=op.get("words") or f"a generated {defect} defect: {prompt}",
            region_kind="whole_image",
            notes={"prompt": prompt},
        )
    return out, DraftLabel(
        expected="pass",
        failure_type=None,
        obligation_ids=[],
        words="generated clean from its own prompt; confirm nothing in it breaks the rules",
        notes={"prompt": prompt},
    )


def _restyle(op: dict[str, Any], source: bytes, generator, seed: int) -> tuple[bytes, DraftLabel]:
    _needs(generator, "restyle")
    prompt = op["prompt"]
    out = generator.edit(source, prompt, seed)
    changes = regions.difference_regions(source, out)
    label = DraftLabel(
        expected="pass",
        failure_type=None,
        obligation_ids=[],
        words=(
            f"a permitted restyle ({prompt.split('.')[0].strip().lower()}); the structure and "
            "every protected subject must be unchanged, confirm against the source"
        ),
        notes={"prompt": prompt},
    )
    label.notes["changed_area"] = (
        "whole_image" if changes.whole or changes.primary is None else changes.primary
    )
    if changes.secondary:
        label.notes["secondary_changes"] = changes.secondary
    return out, label


def _edit(op: dict[str, Any], source: bytes, generator, seed: int) -> tuple[bytes, DraftLabel]:
    _needs(generator, "edit")
    prompt = op["prompt"]
    removes, alters, adds, defect = (op.get(k) for k in ("removes", "alters", "adds", "defect"))
    out = generator.edit(source, prompt, seed)
    changes = regions.difference_regions(source, out)
    if defect:
        # A planted reality-check defect: the rule it breaks is the defect's.
        obligations = [_defect_rule(defect, "edit")]
        words = op.get("words") or f"a planted {defect} defect: {prompt}"
        failure_type = defect
    else:
        obligations = ["SF1"]
        if adds:
            words, failure_type = f"the candidate has {adds} that the source does not", f"{adds}-added"
        elif removes:
            words, failure_type = f"the {removes} is gone from the candidate", f"{removes}-changed"
        elif alters:
            words, failure_type = f"the {alters} is changed in the candidate", f"{alters}-changed"
        else:
            words, failure_type = f"edited: {prompt}", "edited"
    label = DraftLabel(
        expected="fail",
        failure_type=failure_type,
        obligation_ids=obligations,
        words=words,
        notes={"prompt": prompt},
    )
    _place(label, changes)
    return out, label


def _place(label: DraftLabel, changes: regions.Changes) -> None:
    """The label's region from the changes, and the reviewer told of any other change."""
    if changes.primary is None:
        label.region_kind = "whole_image"
    else:
        label.region_kind = "bbox"
        label.bbox = list(changes.primary)
    if changes.secondary:
        label.notes["secondary_changes"] = changes.secondary
        label.words += (
            f"; the generator also changed {len(changes.secondary)} other area(s), check them"
        )


def _apply(op: dict[str, Any], source: bytes, generator, seed: int) -> tuple[bytes, DraftLabel]:
    kind = op.get("kind")
    if kind == "as_is":
        return source, DraftLabel(
            expected="pass",
            failure_type=None,
            obligation_ids=[],
            words="the generated picture as it is; confirm nothing in it breaks the rules",
        )
    if kind == "generate":
        return _generate(op, generator, seed)
    if kind == "restyle":
        return _restyle(op, source, generator, seed)
    if kind == "edit":
        return _edit(op, source, generator, seed)
    if kind == "grade":
        warmth = float(op.get("warmth", 0.1))
        r, g, b = _load(source).split()
        r = r.point(lambda v: min(255, int(v * (1 + warmth))))
        b = b.point(lambda v: max(0, int(v * (1 - warmth))))
        im = ImageEnhance.Contrast(Image.merge("RGB", (r, g, b))).enhance(1 + warmth / 2)
        return _png(im), DraftLabel(
            expected="pass",
            failure_type=None,
            obligation_ids=[],
            words=f"a mild colour grade (warmth {warmth:+.2f}); structure and subjects unchanged",
        )
    if kind == "mirror":
        im = _load(source).transpose(Image.Transpose.FLIP_LEFT_RIGHT)
        return _png(im), _structural("mirrored", "the candidate is the source mirrored left to right")
    if kind == "rotate":
        degrees = int(op.get("degrees", 90))
        if degrees not in (90, 180, 270):
            raise ValueError(f"rotate: degrees must be 90, 180 or 270, got {degrees}")
        im = _load(source).rotate(degrees, expand=True)
        return _png(im), _structural("rotated", f"the candidate is the source rotated by {degrees} degrees")
    if kind == "crop":
        keep = float(op.get("keep", 0.75))
        if not 0.2 <= keep < 1.0:
            raise ValueError(f"crop: keep must be in [0.2, 1), got {keep}")
        im = _load(source)
        w, h = im.size
        cw, ch = round(w * keep), round(h * keep)
        x0, y0 = (w - cw) // 2, (h - ch) // 2
        im = im.crop((x0, y0, x0 + cw, y0 + ch))
        return _png(im), _structural(
            "cropped",
            f"the candidate is the central {int(keep * 100)}% of the source; the edges are cropped away",
        )
    if kind == "tilt":
        # A small roll of the camera, filled to the frame: what a regenerated shot drifts by.
        degrees = float(op.get("degrees", 2.0))
        if not 0.5 <= abs(degrees) <= 10:
            raise ValueError(f"tilt: degrees must be 0.5 to 10 either way, got {degrees}")
        im = _load(source)
        r = math.radians(degrees)
        m = [[math.cos(r), -math.sin(r), 0], [math.sin(r), math.cos(r), 0], [0, 0, 1]]
        return _png(_filled(im, _about_centre(m, *im.size))), _structural(
            "tilted",
            f"the candidate is the source rolled by {degrees:+.1f} degrees and refilled to the frame",
        )
    if kind == "zoom":
        factor = float(op.get("factor", 1.1))
        if not 1.03 <= factor <= 1.5:
            raise ValueError(f"zoom: factor must be in [1.03, 1.5], got {factor}")
        im = _load(source)
        m = [[factor, 0, 0], [0, factor, 0], [0, 0, 1]]
        return _png(_filled(im, _about_centre(m, *im.size))), _structural(
            "zoomed",
            f"the candidate is the source zoomed in {factor:.2f}x at the same size; the edges are lost",
        )
    if kind == "keystone":
        # A horizontal keystone: one side of the frame nearer the camera than the other.
        k = float(op.get("strength", 0.04))
        if not 0.01 <= abs(k) <= 0.15:
            raise ValueError(f"keystone: strength must be 0.01 to 0.15 either way, got {k}")
        im = _load(source)
        w, h = im.size
        m = [[1, 0, 0], [0, 1, 0], [2 * k / w, 0, 1]]
        side = "right" if k > 0 else "left"
        return _png(_filled(im, _about_centre(m, w, h))), _structural(
            "keystoned",
            f"the candidate is the source under a {abs(k) * 100:.0f}% keystone (the {side} side "
            "pushed back), refilled to the frame",
        )
    raise ValueError(f"unknown op kind {kind!r}")
