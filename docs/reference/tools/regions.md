# regions

The tool record of `regions` (`SPEC.md` sections 8 and 16, row E; plan
`docs/plans/mE-evidence-and-edit-quality.md`, task T1). It is a tool file,
`pack/tools/regions.py`, run by the tool worker (ADR 0032), which asks whizbang's segmenter
for each subject through its context. It is squint's `regions` (squint
`tools/python/src/squint_tools/regions.py`, at the commit `docs/sources.toml` pins), moved
from squint's fixed-class segmenter onto SAM 3's phrase requests and onto `change_map`'s
photometric map. The examples below are answered by the pack's fake model, and run as tests
(`test_regions.Record`, under `just test-pack`).

## What it does

Finds where the candidate differs from the source, with high recall, as regions on the
candidate: each a box, a label naming the subject it lies on, and a score of how strongly it
changed. Each region is a pointer, not a verdict: a change the rules allow shows up like one
they forbid, so a judge looks at it with `view` before deciding. The item's value is the
score of its strongest region, 0 when it finds none (its declared rule, `worst`; larger is
changed).

- **Use it** after `align` registers the candidate onto the source, to learn where to look.
- **Do not use it** without a source, or on a pair `align` could not register: the two
  pictures must share one grid.
- **Kind.** An evidence tool (`SPEC.md` section 8): the upper rungs apply to it. Its
  subjects are found by a model, SAM 3, so the part of its readings that names them is the
  model's.

**How subjects are named.** From the item's job (`job`), by per-subject comparison's rule
(`whizbang_pack.subjects`); by name (`phrases`); and from `preserve` and `ignore`, which
`whizbang eval tool` fills from the rules' `[hints.regions]` (`try` and `squint check --tool`
do not; `check` takes the hints up with plan mE's task F1). At most 16, each asked once on
each picture, as printable text: the hints' first, then the job's, then `phrases`. The item's label is never read.

**The method.** Both pictures are brought to a working grid whose longer side is `GRID`
(the picture's own when smaller), each working pixel the mean luminance of up to four by
four samples across it. Two maps are made there:

- **photometric change,** as `change_map` computes it: each luminance lightly blurred (1 2 1
  across and down), normalised by its local mean and contrast over a window of `RADIUS`, and
  the two subtracted; a pixel past `CHANGED` has changed, and `SATURATION` saturates its
  strength;
- **the subjects disagreeing:** for each subject, its mask on the source and on the
  candidate, brought to the grid by the pixel under each working pixel's centre, and the
  pixels on one side and not the other. Squint read its segmenter's class disagreement here.

Each map is cleaned: closed by a square of three, then opened by one of five. Its
4-connected pieces of at least `MIN_AREA` become regions, the photometric ones first, then
the disagreements no photometric box already covers (`OVERLAP`). A region's score is half
its photometric strength and half the larger of that and its share of disagreement; a
disagreement counts in full only with `SUPPORT` of strength behind it, so a subject the
segmenter merely read differently scores low. A region on a `preserve` subject scores
`PRESERVE` times more (at most 1), one on an `ignore` subject `IGNORE` times. Its label is
the subject it lies on most, `changed` on the photometric map and `added` or `removed` on the
disagreement, or `changed` alone on no subject. The `MAX_REGIONS` strongest are kept.

**Its constants.** Squint's: `MAX_REGIONS` 12, `GRID` 256, `MIN_AREA` 0.001 of the frame,
`SUPPORT` 0.2, `OVERLAP` 0.3, `PRESERVE` 1.25 and `IGNORE` 0.5 (squint's factors, written in
its formula), a label of at most 120 bytes. `change_map`'s: `CHANGED` 1.0, `SATURATION` 3.0,
`RADIUS` 4, `EPS` 0.001. Its own: `MOST` 4 instances an ask, `SUBJECTS_MOST` 16, `ASPECT`
0.01, `SAMPLES` 4. Squint's `COLOUR_CHANGED`, its grade taken out of the frame (`GRADE_KEEP`,
`GRADE_GAIN`), `CONFIDENT` and its class labels and relabelling read its segmenter's classes
or are not `change_map`'s: they are left out, and the class labels come with task T2's
method.

**Outputs.** One reading, of `the pair`: `strongest`, the item's value; `changed_fraction`
and `photometric_fraction`, the shares of the frame the joined and the photometric maps
mark; `subject_disagreement`, the share of the subjects' pixels on one side only;
`candidates`, the regions found before the cap. Beside it the regions, each held to its
declared bounds before anything uses it, of the kind `change_region`. No image.

**Its methods** (plan mE, task T2; `SPEC.md` section 8, "Comparing methods"), the argument
`method`, which `whizbang eval tool --method` gives each run: `sam3`, the default, as above;
or `eomt`, squint's own, where the second map is the classes of the catalogue's fixed-class
segmenter `eomt-ade20k` disagreeing (both pictures labelled through `context.labels`, a
working pixel counted where its two classes differ and both are sure at 0.5 or more). A
region is labelled by the majority class on each side (`floor -> swimming pool`), a
disagreement between classes the segmenter confuses scores 0.35 of what it would, and
`preserve` and `ignore` name classes. Its measurement is `class_disagreement`, in place of
`subject_disagreement`. Its examples under `eomt` run as tests
(`test_classes.RegionsByClass`); rung 2 chooses between the two (task R2).

## Examples

Both compare a picture drawn by a program, 160 by 120, a red pool on a checked lawn, with an
edit of it, for the job's subject `pool` ("The pool keeps its shape."). The segmenter is the
pack's fake model, `fake-ok`, which answers a phrase with every pixel whose red is 128 or
more. In the first a red hot tub is added beside the pool; in the second nothing is added,
but the lawn's checks are drawn again four pixels across, as a model drawing a picture again
redraws a texture.

| The source | A tub added | The lawn drawn again |
|---|---|---|
| ![The source](regions/source.png) | ![A tub added beside the pool](regions/tub.png) | ![The lawn's texture drawn again](regions/redrawn.png) |

What to look for. With the tub, one region boxes it, scored 0.64; the fake reads the red tub
as more pool, so it is labelled `pool changed`. **This is a case it handles badly.** With
the lawn drawn again, nothing in the scene changed, but its texture moved everywhere: the
photometric map marks 85% of the frame, joined into one region over the whole picture,
scored 0.45 and named after the pool it surrounds. A judge shown it learns nothing of where
to look, and the item's value (0.45) is not far under a real addition's (0.64). Re-rendered
texture, ripples and reflections are what squint's study found it ranking above real changes.

What it read:

<!-- read: begin -->

| Example | Strongest | Changed fraction | Photometric fraction | Subject disagreement | Candidates |
|---|---|---|---|---|---|
| tub | 0.642 | 0.031 | 0.020 | 0.135 | 1 |
| redrawn | 0.453 | 0.846 | 0.846 | 0.000 | 1 |

<!-- read: end -->

Its regions, each a box on the candidate (left top right bottom, in its pixels):

<!-- regions: begin -->

| Example | Box | Label | Score |
|---|---|---|---|
| tub | 125 20 151 40 | pool changed | 0.642 |
| redrawn | 0 0 160 120 | pool changed | 0.453 |

<!-- regions: end -->

## Rung 2 results

Not yet run. Rung 2 is plan mE's task R2: `whizbang eval tool regions` on the dev sets of
source fidelity, on SAM 3 on the owner's Mac, by the method's examples and numbers there.

## Speed and cost

It spends nothing: SAM 3 runs on the machine, and the maps are arithmetic in the tool worker
on a grid of at most 256 on its longer side. Each subject is two requests of the segmenter.
Its time on SAM 3 is R2's to record.

## Alternatives

Squint's own `regions` read class disagreement from its fixed-class segmenter (EoMT on
ADE20K) and labelled regions by class ("floor -> swimming pool"); that method comes back with
task T2 as `method = eomt`, beside this one, and rung 2 chooses between them.

## Limits

- **A pair must register.** The two pictures must be of one shape (within `ASPECT`); a pair
  `align` could not register has no reading.
- **High recall, not precision.** A texture drawn again, a ripple, a reflection or a change
  of light that is not even across the frame is marked as change (the second example).
- **Flat areas are blind to the photometric map.** A flat colour replaced by another flat
  colour normalises to nothing inside; only the subjects' masks can find it.
- **Labels are the subjects'.** A region is named after the subject it lies on most; on none,
  it is `changed`.
- **Hints are phrases.** `preserve` and `ignore` move the score of a region on a subject of
  that name, and each costs two requests of the segmenter.

## Mark and history

**Experimental**: brought in by milestone E (`SPEC.md` section 16, row E: `regions` is
brought in as an experimental tool and scored on rungs 1 and 2). It runs only when a run
names it.

- 2026-11-25, plan mE task T1: ported from squint as a tool file on SAM 3's phrase requests
  and `change_map`'s photometric map, with its regions on the candidate; this record's two
  examples (rung 1), run against the fake model.
- 2026-11-26, plan mE task T2: the argument `method`, `sam3` (the default) or `eomt`, on the
  catalogue's fixed-class segmenter; version 0.2.0.
