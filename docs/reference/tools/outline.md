# outline

The tool record of `outline` (`SPEC.md` sections 8 and 16, row E; plan
`docs/plans/mE-evidence-and-edit-quality.md`, task T1). It is a tool file,
`pack/tools/outline.py`, run by the tool worker (ADR 0032), which asks whizbang's segmenter
for each subject through its context. It is squint's `outline` (squint
`tools/python/src/squint_tools/outline.py`, at the commit `docs/sources.toml` pins), its
comparison of one subject's shape on an aligned pair, moved from squint's fixed-class
segmenter onto SAM 3's phrase requests. The examples below are answered by the pack's fake
model, and run as tests (`test_outline.Record`, under `just test-pack`).

## What it does

Compares the shape of each protected subject in the source and in the candidate: how much
the two shapes overlap, the area of each, how far the boundary moved, and the pieces added
or removed. Each piece is a region on the candidate, a box with a label and a score, the
strongest first. The item's value is the largest of its subjects' one minus the overlap
(IoU) (its declared rule, `worst`; larger is changed).

- **Use it** on a source and an edit of it that must keep a structure's shape and size: a
  pool widened, an end rounded, a building extended. `change_map` sees pixels change; this
  sees a shape change.
- **Do not use it** for things that may move (furniture, people), on a pair `align` cannot
  register, or to judge texture or colour.
- **Kind.** An evidence tool (`SPEC.md` section 8): the upper rungs apply to it. Its shapes
  are found by a model, SAM 3, so its readings are the model's.

**How subjects are named.** From the item's job (`job`, which `whizbang eval tool` gives it),
by per-subject comparison's rule, shared (`whizbang_pack.subjects`): each requirement of kind
`must_hold` whose text is "The ... keeps ...", the phrase before " keeps ", its leading
article dropped; a requirement not of that form is not applicable, by its id. Or by name,
each a phrase (`phrases`, separated by semicolons). The item's label is never read. A
subject found on neither picture is not applicable, by its name.

**The method.** For each subject, the segmenter is asked once on the source and once on the
candidate as `align` registered it onto the source (at most four instances each, their
union the subject's mask). Both masks are brought by their nearest pixel to a working grid
whose longer side is `WORK`, specks are removed from both by one bar, and the shapes are
compared there:

- the overlap, IoU, and each shape's area as a share of the frame;
- how far the boundary moved, both ways: for each point of either shape's edge (at most
  `MAX_POINTS` a side, none within two pixels of the frame's edge), the distance to the
  nearest point of the other's; the 95th percentile and the largest, in pixels of the
  source;
- the pieces: what was added and what was removed, each opened by a square of `JITTER` so
  that boundary jitter is no piece, taken as 4-connected pieces of at least `MIN_AREA`, and
  those in line along one moved edge joined into one (`CHAIN_GAP`).

A piece's score is half its area (full at `AREA_FULL` of the frame) and half how far the
boundary moved inside it (full at `SHIFT_FULL` pixels), raised to its share of the subject
when that is large (full at `WHOLE`), and halved for a small piece on the frame's edge of a
small object (`EDGE_SMALL`, `EDGE_OBJECT`). The six strongest pieces of the item
(`MAX_REGIONS`) are its regions.

**Its constants, as squint's.** `WORK` 1024; `PRESENT_NAMED` 0.0005 of both frames;
`JITTER` 5 working pixels; `SPECK_REL` 0.03 of the largest fragment and `SPECK_ABS` 0.0003
of the frame; `WHOLE` 0.3; `EDGE_SMALL` 0.005 and `EDGE_OBJECT` 0.02; `MIN_AREA` 0.0005;
`MAX_POINTS` 4000; `MAX_REGIONS` 6; `CHAIN_GAP` 0.08 of the longer side; `AREA_FULL` 0.005
and `SHIFT_FULL` 30 pixels, which squint wrote in its score's formula; a label of at most
120 bytes. Its own: `MOST` 4 instances an ask and `SUBJECTS_MOST` 16 subjects an item, as
per-subject comparison's; `ASPECT` 0.01, how far the two pictures' shapes may differ and
still be one grid; `CELL` 32, the cells the boundary's points are sorted into to find the
nearest. Squint's `PRESENT`, its class groups and its scan of every class read its
segmenter's classes, and come only with task T2's method.

**Outputs.** `one_minus_iou`, the item's value, in the run's diagnostic `tool_file.json`,
with every subject's readings beside it (`one_minus_iou`, `area_source`, `area_candidate`,
`area_change` when the subject is on the source, `shift_p95_px`, `shift_max_px`, `added`,
`removed`) and the regions, each held to its declared bounds before anything uses it. No
image.

## Examples

Both compare a picture drawn by a program, 160 by 120, a red pool on grey, with an edit of
it, for the job's subject `pool` ("The pool keeps its shape."). The segmenter is the pack's
fake model, `fake-ok`, which answers a phrase with every pixel whose red is 128 or more: a
stand-in for SAM 3 that finds exactly the red. In the first, the pool's near edge is moved
12 pixels down; in the second, the pool is untouched and a small red towel is laid beside it.

| The source | The pool widened | A towel beside it |
|---|---|---|
| ![The source](outline/source.png) | ![The pool widened](outline/widened.png) | ![A towel beside the pool](outline/towel.png) |

What to look for. Widened, the shapes overlap by 0.77, the area grew by 30%, the boundary
moved 12 pixels, and one region covers the added strip. **This is a case it handles
badly.** The towel is no part of the pool, but the segmenter answers the phrase with it, so
outline reads it as the pool grown: a region scored 0.75, almost as strong as the real
change's 0.77, with a boundary that "moved" 15 pixels. The item's value tells them apart
(0.03 against 0.23); the regions do not. SAM 3 answering a phrase with a second thing, or a
shadow, does the same, which is what the 2026-11 study saw: noise ranked above the real
change (`docs/results/2026-11-tool-study.md`).

What it read (areas are shares of the frame; shifts are pixels of the source):

<!-- read: begin -->

| Example | Subject | One minus IoU | Area, source | Area, candidate | Area change | Added | Removed | Shift p95 | Shift max |
|---|---|---|---|---|---|---|---|---|---|
| widened | pool | 0.231 | 0.167 | 0.217 | 0.301 | 0.050 | 0.000 | 12.0 | 12.0 |
| towel | pool | 0.030 | 0.167 | 0.172 | 0.031 | 0.005 | 0.000 | 6.9 | 15.0 |

<!-- read: end -->

Its regions, each a box on the candidate (left top right bottom, in its pixels):

<!-- regions: begin -->

| Example | Box | Label | Score |
|---|---|---|---|
| widened | 40 80 120 92.03 | pool added, boundary moved up to 12 px | 0.771 |
| towel | 125 60 135 70 | pool added, boundary moved up to 15 px | 0.750 |

<!-- regions: end -->

The item's value is 0.231 for the first and 0.030 for the second.

## Rung 2 results

Not yet run. Rung 2 is plan mE's task R2: `whizbang eval tool outline` on the dev sets of
source fidelity, on SAM 3 on the owner's Mac, by the method's examples and numbers there.

## Speed and cost

It spends nothing: SAM 3 runs on the machine, and the comparison is arithmetic in the tool
worker, on a grid of at most 1024 by 1024. Each subject is two requests of the segmenter.
Its time on SAM 3 is R2's to record.

## Alternatives

Squint's own `outline` compared one class of its fixed-class segmenter (EoMT on ADE20K),
or every class at once; that method comes back with task T2 as `method = eomt`, beside this
one, and rung 2 chooses between them.

## Limits

- **A pair must register.** The candidate is compared after `align`; a pair whose two
  pictures are not of one shape (within `ASPECT`) is refused, and one that does not register
  has no reading.
- **Names are the model's.** A subject is what the segmenter answers for its phrase: another
  thing it also answers reads as the subject's change (the second example).
- **Every instance of a phrase counts.** The mask is the union of at most four instances.
- **A subject on neither picture is not applicable**, so an item whose every subject is
  missing has no value.
- **The frame's edge is not boundary.** A shape running off the picture has no outline
  there, so a change along the frame's edge shows as area, not as movement.

## Mark and history

**Experimental**: brought in by milestone E (`SPEC.md` section 16, row E: `outline` is
brought in as an experimental tool and scored on rungs 1 and 2). It runs only when a run
names it.

- 2026-11-25, plan mE task T1: ported from squint as a tool file on SAM 3's phrase requests,
  with its regions on the candidate; this record's two examples (rung 1), run against the
  fake model.
