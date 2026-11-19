# cut

The tool record of `cut` (`SPEC.md` section 8; plan m3, task D2). The example images and
the two generated parts are written by `scripts/tool-record.sh cut`; the fast tests fail
when they stop being what the tool gives
(`crates/whizbang-rascal/tests/it/tool_records/frames.rs`).

## What it does

Cuts a complete panorama at its seam: its last strip and its first side by side on a flat
canvas, so the seam is the canvas's middle column. Nothing is resampled. The plan it hands
back is how `put` writes the canvas back.

- **Use it** to work on a panorama's seam with operations that need a flat image (`fan`,
  `profile`, a fill); the task `seam` does.
- **Do not use it** on a flat picture: only a complete panorama, equirectangular and
  wrapping, is cut.
- **Kind.** A pure pixel operation: rungs 1 and 2 apply. No model, no spend.

The action `cut`, version 1.0.0:

```
whizbang rascal cut --image <identity> --strip 0.07 --store <dir>
```

| Name | Meaning |
|---|---|
| `image` (input) | A complete panorama, at least 4 columns wide |
| `strip` | The strip as a share of the width, over 0 and at most 0.5; 0.07 when not given |

**Output.** The canvas at `image`, flat, `2 s` wide; the plan at `plan`, naming the
panorama and `s`. **The strip** `s` is `min(max(2, 2 round_half_up(strip width / 2)),
2 floor(width / 4))`: even, at least 2, at most half the width. Refused: an image that is
not a complete panorama, and one narrower than 4 columns.

## Examples

Each picture is an image three times its size. The scene stands in for a panorama: its left and right edges differ,
so the seam shows.

| `strip` 0.07: a canvas of 12 by 64 | `strip` 0.5: 96 by 64 |
|---|---|
| ![The default canvas](cut/default.png) | ![The panorama turned by half](cut/half.png) |

What to look for: at 0.07 the strip is 6 columns, and the canvas is the scene's last six
then its first six, the seam in the middle; at 0.5 the strip is 48 and the canvas is the
whole panorama turned by half.

## Rung 2 results

Exact against reference results (`SPEC.md` section 8). Measured on 2026-11-19.

`cut::the_ported_vectors_give_rascals_canvas_and_plan` holds rascal's strip widths and
canvases (`crates/rascal-ops/tests/it/cut_put.rs` at the pinned commit) on rascal's
panorama, to the pixel, and the plan through the executor. No pixel differs.

The examples in numbers, the same on every machine:

<!-- measured: begin -->

| Example | `strip` | Strip in columns | Canvas |
|---|---|---|---|
| default | 0.07 | 6 | 12x64 |
| half | 0.5 | 48 | 96x64 |

<!-- measured: end -->

## Speed and cost

`cut` spends nothing: it copies two strips, so the time follows the height and the strip.

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | `cut`, the default strip |
|---|---|---|
| 1024x768 | 0.8 | 0.2 |
| 4096x3072 | 12.6 | 4.6 |
| 8000x6000 | 48.0 | 18 |

<!-- timed: end -->

## Alternatives

None tried: the rule is rascal's, ported unchanged but for the differences its module names.

## Limits

| Limit | Example |
|---|---|
| The strip is made even and at most a quarter of the width, twice, whatever `strip` asks | `strip` 0.5 gives 48 of 96 |
| A flat image is refused, however its edges match | The action's own check |

## Mark and history

**Mark.** None: a pure pixel operation, complete when its record shows rungs 1 and 2, and
this one does.

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests |
| 2. Scored alone | Shown | Exact against rascal's vectors; the times at three sizes |
| 3 and 4 | Do not apply | `SPEC.md` section 8 |

**Gates.** None faced.

**History.** Plan m3, task O2: ported from rascal's `cut.rs` at the pinned commit.
Plan m3, task D2: this record.
