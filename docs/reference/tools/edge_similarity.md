# edge_similarity

The tool record of `edge_similarity` (`SPEC.md` section 8). The example images, what
`edge_similarity` said of each example and the times are written by
`scripts/tool-record.sh edge_similarity`; the fast tests fail when the images or the
sentences stop being what the tool gives (`crates/whizbang-squint/tests/it/edge_similarity.rs`
and `compare_record.rs`).

## What it does

Scores how much two images in one grid share the same edges: the correlation of their
edge-strength maps, 1 for the same structure, near 0 for unrelated or featureless images; and
the weaker image's mean edge strength.

- **Use it** after `align`, between the source and the candidate aligned onto it, to tell a
  restyle that kept the structure from one that moved or removed it.
- **Do not use it** between images that are not in one grid: the score would measure the
  misalignment.
- **Kind.** An evidence tool for a judge (`SPEC.md` section 8): the upper rungs apply to it.

It is a function, `whizbang_squint::compare::edge_similarity`, and an action that `check`
runs first, under `--details first --detail edge_similarity`, on the source and the frame
`align` returned for each pair the use case relates (`align` is then run first, named or
not); and that a judge may call when a run offers it.

**Arguments.**

| Argument | Meaning |
|---|---|
| `frame` | One image, usually the source: flat |
| `against` | The image compared with it, usually the candidate aligned onto it: flat, of the same aspect within 1%, and not the same picture |

**Outputs.** `edge_similarity` (a ratio, -1 to 1) and `edge_energy` (the weaker image's mean
edge strength, intensity per pixel), in the run's diagnostic `edge_similarity.json`; squint's
sentence of them (`whizbang_squint::compare::edges_summary`). No image.

**The method.** Squint's (`crates/squint-tools/src/builtin/compare.rs`, unchanged in its
arithmetic and order): both images brought to the first's grid at a working size of at most
768 pixels on the longer side, their Rec. 601 luminance blurred by a 3 by 3 box; each pixel's
edge strength is the length of its central-difference gradient; the score is the normalised
cross-correlation of the two edge maps, 0 when either has no variation. On squint's own
pictures it gives squint's numbers (`edge_similarity::the_ported_vectors_give_squints_ratio`).

## Examples

Pictures painted by squint's test kit, 128 by 96 pixels.

### A restyle against an unrelated scene

The scene against its tone-mapped copy, and against another scene.

| The scene | Restyled | Another scene |
|---|---|---|
| ![The scene](edge_similarity/scene.png) | ![Restyled](edge_similarity/toned.png) | ![Another scene](edge_similarity/other.png) |

What to look for: the restyle scores near 1, the other scene near 0.

### A blob added and one moved, which it handles badly

| The source | The edit |
|---|---|
| ![The source](edge_similarity/before.png) | ![The edit](edge_similarity/after.png) |

What to look for: the score stays near 1. A whole-image correlation hardly moves for a local
change; `change_map` shows where it is.

<!-- said: begin -->

- "Edge similarity of scene and toned: 0.919 (1 is the same structure; the weaker frame's edge energy is 0.0265)."
- "Edge similarity of scene and other: 0.004 (1 is the same structure; the weaker frame's edge energy is 0.0320)."
- "Edge similarity of before and after: 0.975 (1 is the same structure; the weaker frame's edge energy is 0.0238)."

<!-- said: end -->

## Rung 2 results

Not yet run. Scored alone, for an evidence tool, means its read separates good from bad on a
development set (`SPEC.md` section 8): `whizbang eval tool edge_similarity --set
source-fidelity/scenes-v1`, scored by the area under the ROC curve of `edge_similarity`
(smaller is changed). It is the overflow of task X1 of `docs/plans/m2c-milestone-2.md`, owned
by task F1 (`docs/carry-over.md`).

## Speed and cost

It spends nothing: it is arithmetic on the machine, at most 768 pixels on the longer side
whatever the image.

<!-- timed: begin -->

Measured in a release build on macos aarch64; each time is the middle one of five runs, in milliseconds, of `edge_similarity` on a scene and its restyled copy.

| Image | `edge_similarity` |
|---|---|
| 256x192 | 2 |
| 512x384 | 7 |
| 1024x768 | 32 |
| 2048x1536 | 97 |

<!-- timed: end -->

## Alternatives

None run. Squint's method is kept because it is what squint's judges were handed.

## Limits

- **A local change scarcely moves it** (second example).
- **Colour is not seen**: it is luminance edges only.
- **Small, flat images score high against anything smooth.** At 128 by 96 a featureless
  gradient scores 0.20 against a textured scene, where squint's own test, at 384 by 288,
  scored under 0.05: read `edge_energy` beside it.
- **The two images must share one grid**, and be flat.

## Mark and history

**Experimental**: ported with its record and rung 1 shown; rung 2 is owed (above). It runs
only when a run names it.

- 2026-11-18, plan m2c task X1: ported from squint (`crates/squint-tools/src/builtin/
  compare.rs`, `manifests/edge_similarity.toml`) at the pinned commit; measured first by
  `check` under `--detail edge_similarity`, and offered to a judge when a run names it.
