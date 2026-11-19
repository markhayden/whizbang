# invert

The tool record of `invert` (`SPEC.md` section 8; plan m3, task D2). The example images and
the two generated parts are written by `scripts/tool-record.sh invert`; the fast tests fail
when they stop being what the tool gives
(`crates/whizbang-rascal/tests/it/tool_records/masks.rs`).

## What it does

Makes the mask for a second pass on an image: everything the first mask reached, its core,
its blend and its protect, becomes protected, and every other pixel the core. The blend is
empty, and the mask is recorded as made to protect. The task `protect` runs it after a fill.

- **Use it** after a fill, on the composite and the mask `fill` rebound onto it, to edit
  everything else while the first edit is kept.
- **Do not use it** through a cut or a window: the mask and the image must be one, and
  carrying a mask back up a cut or a crop is `put`'s and `unwindow`'s.
- **Kind.** A pure pixel operation: rungs 1 and 2 apply. No model, no spend.

The action `invert`, version 1.0.0:

```
whizbang rascal invert --mask <identity> --image <identity> --store <dir>
```

| Name | Meaning |
|---|---|
| `mask` (input) | The first pass's mask, on the image |
| `image` (input) | The image of the second pass: after a fill, its composite |

**Output.** One mask at `mask`, on the image, its source the first mask's. A mask that
reached every pixel is refused: nothing is left to edit.

## Examples

Each picture shows a mask of 96 by 64, three times its size: the core in white, protected pixels in red, the blend in blues, lighter where it weighs more, and the rest dark.

| First pass | Second pass |
|---|---|
| ![A feathered disc with a protected patch](invert/feathered.png) | ![Its reach protected, the rest the core](invert/feathered-invert.png) |
| ![A block against the top and left edges](invert/cornered.png) | ![The block protected, the rest the core](invert/cornered-invert.png) |

What to look for: the feathered disc's core, its whole feather (even its faintest blues)
and its protected patch are all protected after, 1,288 pixels; the 4,856 others are the
core. A blend weighing a few levels protects its pixel as fully as the core does.

## Rung 2 results

Exact against reference results (`SPEC.md` section 8). Measured on 2026-11-19.

`invert::the_ported_vectors_give_rascals_masks` holds rascal's vectors
(`crates/rascal-ops/tests/it/subtract_select_invert.rs` at the pinned commit) that stay on
the mask's own image: protect counts the core, the blend and protect, every other pixel is
the core, and a mask that reaches every pixel leaves nothing to edit. No pixel differs.

The examples in numbers, the same on every machine:

<!-- measured: begin -->

| Example | Core, blend and protect before | Core, blend and protect after |
|---|---|---|
| feathered | 580, 644, 64 | 4856, 0, 1288 |
| cornered | 720, 0, 0 | 5424, 0, 720 |

<!-- measured: end -->

## Speed and cost

`invert` spends nothing; it reads each pixel's three planes once.

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | `invert` |
|---|---|---|
| 1024x768 | 0.8 | 2.8 |
| 4096x3072 | 12.6 | 42 |
| 8000x6000 | 48.0 | 165 |

<!-- timed: end -->

## Alternatives

None tried: the rule is rascal's, ported unchanged but for the differences its module names.

## Limits

| Limit | Example |
|---|---|
| Any weight at all protects: a feather's faintest edge is kept from the second pass | The first example: 644 blend pixels protected |
| A mask that reached every pixel is refused | `the_records_of_rasterize_shrink_invert_and_inspect_are_what_they_give` |
| The second pass's core has no feather: feather it before a fill | Both examples |

## Mark and history

**Mark.** None: a pure pixel operation, complete when its record shows rungs 1 and 2, and
this one does.

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests |
| 2. Scored alone | Shown | Exact against rascal's vectors; the times at three sizes |
| 3 and 4 | Do not apply | `SPEC.md` section 8 |

**Gates.** None faced.

**History.** Plan m3, task O1: ported from rascal's `invert.rs` at the pinned commit.
Plan m3, task D2: this record.
