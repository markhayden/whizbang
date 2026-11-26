# classes

The tool record of `classes` (`SPEC.md` sections 8 and 16, row E; plan
`docs/plans/mE-evidence-and-edit-quality.md`, task T2). It is a tool file,
`pack/tools/classes.py`, run by the tool worker (ADR 0032), which asks the catalogue's
fixed-class segmenter, `eomt-ade20k`, for the classes of both pictures through its context
(`context.labels`, ADR 0053). It is squint's `classes` (squint
`tools/python/src/squint_tools/classes.py`, at the commit `docs/sources.toml` pins). The
examples below are answered by a fake fixed-class segmenter, and run as tests
(`test_classes.Record`, under `just test-pack`).

## What it does

Names what the segmenter sees in one box of an aligned pair, before and after: the classes
covering the box on each side with their shares, the segmenter's mean confidence over the
box on each side, and the share of the box whose class changed. When the box's main class
differs between the two sides, one region on the candidate, the box, labelled
`before -> after`.

- **Use it** on a region `regions` pointed at, or any box you want named, to learn what was
  there and what is there now.
- **Do not use it** to judge whether a change is allowed: the rules decide that. Nor on a
  pair `align` cannot register, or on a thing outside ADE20K's 150 classes, which the
  segmenter names by its nearest.
- **Kind.** An evidence tool (`SPEC.md` section 8): the upper rungs apply to it. Its classes
  are a model's, so its readings are the model's.

**Its argument.** `bbox`: left, top, right and bottom as fractions of the frame, from 0 to
1, separated by spaces; the whole frame when not given. `whizbang eval tool` gives none, so
each item is read over the whole frame and has one number. **Its value**, `class_changed`,
is the share of the box whose class differs between the two sides (larger is changed).

**The method.** Both pictures are labelled by `eomt-ade20k`, every class of ADE20K, and read
on a working grid whose longer side is 256, each working pixel its centre's class. On each
side the classes covering the box are named largest first, at most four, those under 5% of
the box folded away. A class's confidence is its mean over its pixels, so the box's
confidence is the share-weighted mean of its classes' (squint averaged each pixel's own).

## Examples

A floor (grey) with a pool (red) on the right; the candidate pours a second pool on the
left. The fake segmenter calls each red pixel `swimming pool`, sure at 0.9, and every other
`floor`, sure at 0.8.

| The source | The candidate |
|---|---|
| ![A grey floor with one red pool on the right](classes/source.png) | ![The same with a second red pool on the left](classes/poured.png) |

Read over the box of the new pool (`bbox` 0.125 0.3333 0.5 0.8333), then over the whole
frame:

<!-- read: begin -->

| Example | Subject | Values |
|---|---|---|
| box | the box | class_changed 0.9836 confidence_candidate 0.8984 confidence_source 0.8 |
| box | floor | share_candidate 0.0 share_source 1.0 |
| box | swimming pool | share_candidate 0.9836 share_source 0.0 |
| box | region | floor -> swimming pool 0.8 |
| whole | the box | class_changed 0.1875 confidence_candidate 0.8313 confidence_source 0.8125 |
| whole | floor | share_candidate 0.6875 share_source 0.875 |
| whole | swimming pool | share_candidate 0.3125 share_source 0.125 |

<!-- read: end -->

What to look for: over the new pool's box the class changed almost wholly (the working
grid's edge takes in a row of floor), and the region names it; over the whole frame the
main class is the floor on both sides, so there is no region, and the value is the pool's
added share.

## Rung 2 results

Not scored yet: rung 2 of `classes` on the source-fidelity dev sets is plan mE's task R2,
with `eomt-ade20k` on the Mac.

## Speed and cost

The fixed-class segmenter through the local pack charges nothing. One run asks for two
labels, one a picture, each one call of the action `labels` (three requests of the model,
which computes each picture once). Its time on the Mac is R2's.

## Alternatives

`regions` points at where a pair changed, by its photometric map and, under its method
`eomt`, the same classes disagreeing; `classes` names what one box holds on each side.
`per_subject` and `outline` compare named subjects found by phrase.

## Limits

| Limit | Example |
|---|---|
| The classes are ADE20K's 150: a thing outside them is named by its nearest | No example here: the fake names two |
| The same class on both sides does not mean nothing changed: a small patch is named by its surroundings | Over the whole frame above, the floor stays the main class though a pool was poured |
| A swap between related classes (path and dirt track, building and house) is often the segmenter, not the picture | `whizbang_pack.class_groups.GROUPS` lists them; `regions` scores such a swap down |
| The box is read on a grid of 256 at most | `class_changed` 0.9836 over a box the pool fills |

## Mark and history

**Experimental**: brought in by milestone E (`SPEC.md` section 16, row E: `classes` is
brought in as an experimental tool and scored on rungs 1 and 2). It runs only when a run
names it.

- 2026-11-26, plan mE task T2: ported from squint as a tool file on the catalogue's
  `eomt-ade20k`, reached through `context.labels`; this record's two examples (rung 1), run
  against a fake fixed-class segmenter.
