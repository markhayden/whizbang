# window

The tool record of `window` (`SPEC.md` section 8; plan m3, task D2). The example images and
the two generated parts are written by `scripts/tool-record.sh window`; the fast tests fail
when they stop being what the tool gives
(`crates/whizbang-rascal/tests/it/tool_records/frames.rs`).

## What it does

Crops an image and its mask around the mask's support, with context, in whole pixels and
with nothing resampled. The mask handed back is on the crop, and its binding places the
crop onto the image, so `unwindow` can write it back.

- **Use it** to send a filler a small picture about a small mask, not the whole image.
- **Do not use it** to shrink a large support: a support larger than `max_side` is
  refused, and a window is never scaled.
- **Kind.** A pure pixel operation: rungs 1 and 2 apply. No model, no spend.

The action `window`, version 1.0.0:

```
whizbang rascal window --image <identity> --mask <identity> --context 0.5 --store <dir>
```

| Name | Meaning |
|---|---|
| `image` (input) | The image to window |
| `mask` (input) | The mask on it |
| `context` | The context on each side, as a share of the support's width and height, from 0 to 4; `window-v1`'s 0.5 when not given |
| `max_side` | The window's largest side in pixels, from 1 to 1000000; the image's when not given |
| `constants` | `window-v1` |

**Output.** The crop at `image`, flat unless it is the whole image; the mask at `mask`,
its three planes cut to the box, its profile and prescriptions left behind. **The box** is
the support grown by the context on each side (rounded half up), made square at its larger
side, capped to `max_side` and to the image, and slid inside the image. Refused: a mask
that weighs nothing anywhere, and a support larger than `max_side`.

## Examples

Each picture is an image three times its size: the crop, with the mask's core in light amber.

| A support of 12 by 10 in the middle | A support of 10 by 8 by the top right corner |
|---|---|
| ![A crop of 24 by 24](window/middle.png) | ![A crop of 20 by 20 against the corner](window/corner.png) |

What to look for: the first window is a square of 24, the support centred in it; the
second is a square of 20, its own support's larger side and context, slid down and left to
fit, so the support sits in its top right corner.

### The inputs, in a store

`whizbang store record-inputs --store <dir>` writes the inputs of these examples into a store
that is there, as canonical artifacts made by the constructors this record's tests use, each
with what it depends on, and prints each one's identity (`crates/whizbang/src/record_inputs.rs`).
A walkthrough gives the same inputs to the command line and to the playground. They are:

<!-- inputs: begin -->

| Example | Input | Type | Identity |
|---|---|---|---|
| middle | image | image | `sha256:5b673f00862bdb77dc9805e663c997175b329f97a6a31eb55469d099fef74f33` |
| middle | mask | mask | `sha256:16c497dda63e7a8f125e0c5753d5dec26dfeb281e67090ec2b6c68fadae1e02d` |
| corner | image | image | `sha256:5b673f00862bdb77dc9805e663c997175b329f97a6a31eb55469d099fef74f33` |
| corner | mask | mask | `sha256:060e63f6c557b517fc2dae1f4ce6ca73684b51596a689bdb18d131f10b021b43` |

<!-- inputs: end -->

## Rung 2 results

Exact against reference results (`SPEC.md` section 8). Measured on 2026-11-19.

`window::the_ported_vectors_give_rascals_window` holds rascal's window contract
(`crates/rascal-ops/tests/it/window.rs` at the pinned commit) to the pixel: the box, the
crop and the mask cut by the same box. No pixel differs.

The examples in numbers, the same on every machine:

<!-- measured: begin -->

| Example | Support | Window | Crop's projection |
|---|---|---|---|
| middle | [40, 26, 52, 36] | [34, 19, 58, 43] | Flat |
| corner | [84, 2, 94, 10] | [76, 0, 96, 20] | Flat |

<!-- measured: end -->

## Speed and cost

`window` spends nothing. Its time is the support found in the mask, then the crop; the support is a block a tenth of each side.

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | `window` of a support a tenth of the image |
|---|---|---|
| 1024x768 | 0.8 | 1.8 |
| 4096x3072 | 12.6 | 26 |
| 8000x6000 | 48.0 | 102 |

<!-- timed: end -->

## Alternatives

None tried: the rule is rascal's, ported unchanged but for the differences its module names.

## Limits

| Limit | Example |
|---|---|
| A window is square, so a long thin support brings much context across it | The rule: square at the larger side |
| The profile and the prescriptions do not travel to the crop | Measured on the whole image |
| A support over `max_side` is refused, not scaled | `window::the_ported_vectors_give_rascals_window` |

## Mark and history

**Mark.** None: a pure pixel operation, complete when its record shows rungs 1 and 2, and
this one does.

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests |
| 2. Scored alone | Shown | Exact against rascal's vectors; the times at three sizes |
| 3 and 4 | Do not apply | `SPEC.md` section 8 |

**Gates.** None faced.

**History.** Plan m3, task O2: ported from rascal's `window.rs` at the pinned commit.
Plan m3, task D2: this record.
