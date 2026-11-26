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
- **A mask on a crop** of the image, such as one `segment` made inside a crop `view` cut,
  is given with the crop and the crop's corner on the image: it is carried onto the image
  first, as `carry` does it (`carry.md`), and inverted there.
- **Do not use it** through a cut canvas or a window: carrying a canvas back is `put`'s and
  `unwindow`'s, and a mask on one is not carried.
- **Kind.** A pure pixel operation: rungs 1 and 2 apply. No model, no spend.

The action `invert`, version 1.1.0:

```
whizbang rascal invert --mask <identity> --image <identity>
        [--crop <identity> --left <pixels> --top <pixels>] --store <dir>
```

| Name | Meaning |
|---|---|
| `mask` (input) | The first pass's mask, on the image, or on the crop when one is given |
| `image` (input) | The image of the second pass: after a fill, its composite |
| `crop` (input, optional) | The crop of the image the mask is on, when it is on one |
| `left`, `top` | With a crop: its corner on the image, in whole pixels |

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

### The inputs, in a store

`whizbang store record-inputs --store <dir>` writes the inputs of these examples into a store
that is there, as canonical artifacts made by the constructors this record's tests use, each
with what it depends on, and prints each one's identity (`crates/whizbang/src/record_inputs.rs`).
A walkthrough gives the same inputs to the command line and to the playground. They are:

<!-- inputs: begin -->

| Example | Input | Type | Identity |
|---|---|---|---|
| feathered | mask | mask | `sha256:2a80f9215a7f976a4e8a3072c26dfeaf72f98b3fdd3bebfba89d083d8d463edc` |
| feathered | image | image | `sha256:5b673f00862bdb77dc9805e663c997175b329f97a6a31eb55469d099fef74f33` |
| cornered | mask | mask | `sha256:370c8e8c666df7f418814f67b8c65b300ddefc2e0c7f3bcb140f96b2506c6c32` |
| cornered | image | image | `sha256:5b673f00862bdb77dc9805e663c997175b329f97a6a31eb55469d099fef74f33` |

<!-- inputs: end -->

## Rung 2 results

Exact against reference results (`SPEC.md` section 8). Measured on 2026-11-19.

`invert::the_ported_vectors_give_rascals_masks` holds rascal's vectors
(`crates/rascal-ops/tests/it/subtract_select_invert.rs` at the pinned commit) that stay on
the mask's own image: protect counts the core, the blend and protect, every other pixel is
the core, and a mask that reaches every pixel leaves nothing to edit. No pixel differs.
`invert::invert_from_a_crop_mask_lands_protect_on_its_bbox` holds rascal's vector of a
crop: a mask on a crop of 30 by 30 protects its place on the picture of 100 by 60.

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
| A crop that `view` scaled down, or that is not the image's pixels at the corner given, is refused | `carry.md`, "Limits" |

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
Plan m3, task D2: this record. Plan m4, task L2: a mask on a crop of the image, carried
onto it first (rascal's `to`, for a crop), as version 1.1.0.
