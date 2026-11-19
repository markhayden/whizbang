# tighten

The tool record of `tighten` (`SPEC.md` section 8; plan m3, task D2). The example images and
the two generated parts are written by `scripts/tool-record.sh tighten`; the fast tests fail
when they stop being what the tool gives
(`crates/whizbang-rascal/tests/it/tool_records/tighten.rs`).

## What it does

Tightens a rough stroke to the defect's edges. The path is resampled about a pixel apart;
at each sample the luminance is read across the stroke, its derivative taken, and the best
pair of opposite edges inside the stroke chosen. A sample without one keeps the full
radius. Runs of tightened samples are smoothed, and a crossing reverts. The core is the
tube between the samples' extents, with round joins and caps, so a stroke tightened
nowhere is exactly the mask `rasterize` draws for it.

- **Use it** on a stroke drawn over a scratch, a crack or a wire, before a fill; the task
  `repair` does.
- **Do not use it** on a long stroke over a large image without counting its cost (Speed).
- **Kind.** A pure pixel operation: rungs 1 and 2 apply. No model, no spend.

The action `tighten`, version 1.0.0:

```
whizbang rascal tighten --stroke <identity> --image <identity> --store <dir>
```

| Name | Meaning |
|---|---|
| `stroke` (input) | A stroke location, on the image |
| `image` (input) | The image the edges are read from |
| `min_contrast` | The least derivative, in levels a pixel, an edge needs; `tighten-v1`'s 12 |
| `min_defect_px`, `max_defect_factor` | The narrowest defect in pixels (1), the widest as a multiple of the radius (2) |
| `path_sigma`, `sample_spacing_px` | The smoothing along the path in samples (4), the samples' spacing (1) |
| `constants` | `tighten-v1` |

**Output.** One mask at `mask` for a repair, on the image, its source the stroke, no blend.
Refused: a location that is not a stroke, a path with no direction, a stroke that covers no
pixel.

## Examples

The scene of `stroke`'s record, with its dark scratch about two pixels wide from (20, 20) to
(76, 44), three times its size, the core in light amber. Both strokes have a radius of 6.

| Along the scratch | Across the plain upper part |
|---|---|
| ![The core held to the scratch](tighten/scratch.png) | ![The whole stroke, untightened](tighten/plain.png) |

What to look for: along the scratch every sample is tightened and the core keeps to the
scratch, 127 pixels where the stroke covers 840, every dark pixel inside; where there is
no edge nothing is tightened and the core is the stroke's inside exactly.

## Rung 2 results

Exact against reference results (`SPEC.md` section 8). Measured on 2026-11-19.

`tighten::the_ported_vectors_give_rascals_masks` holds rascal's vectors
(`crates/rascal-ops/tests/it/tighten.rs` at the pinned commit), drawn at 256 by 192 for
rascal's 400 by 300, every threshold rascal's: edges within a pixel on a smooth and a
textured scratch, an edgeless stroke exactly `rasterize`'s. Its neighbours in
`tighten.rs` hold the direction of a reversing path, the order of ties, the smoothing and
the reverting of a crossing.

The examples in numbers, the same on every machine:

<!-- measured: begin -->

| Example | Samples | Tightened | Core | The stroke's inside, as `rasterize` draws it |
|---|---|---|---|---|
| scratch | 62 | 62 | 127 | 840 |
| plain | 78 | 0 | 1028 | 1028 |

<!-- measured: end -->

## Speed and cost

`tighten` spends nothing, but its cost is not the image's: each pixel of the stroke's box is tested against every sample's cap and segment, so it grows with the box times the length. The scratch runs along the image's diagonal; each stroke is centred on it.

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | A stroke 100 px long, radius 6, on the scratch | A stroke 400 px long |
|---|---|---|---|
| 1024x768 | 0.8 | 1.6 | 47 |
| 4096x3072 | 12.6 | 13 | 58 |
| 8000x6000 | 48.0 | 46 | 91 |

<!-- timed: end -->

## Alternatives

None tried: the rule is rascal's, ported unchanged but for the differences its module names.

## Limits

| Limit | Example |
|---|---|
| The time grows with the stroke's box times its samples: at 1024 by 768 a stroke four times as long costs about thirty times as much, and one across a large image takes minutes | Speed: 100 px against 400 px; a stroke along the whole diagonal of 8000 by 6000 did not finish in two minutes |
| A sample with no pair of edges keeps the whole radius, so a stroke off its defect is not narrowed | The second example |
| Edges are luminance only: a defect of another hue at the same brightness is not seen | The rule |

## Mark and history

**Mark.** None: a pure pixel operation, complete when its record shows rungs 1 and 2, and
this one does.

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests |
| 2. Scored alone | Shown | Exact against rascal's vectors; the times at three sizes |
| 3 and 4 | Do not apply | `SPEC.md` section 8 |

**Gates.** None faced.

**History.** Plan m3, task O4: ported from rascal's `tighten.rs` at the pinned commit.
Plan m3, task D2: this record.
