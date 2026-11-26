# unwindow

The tool record of `unwindow` (`SPEC.md` section 8; plan m3, task D2). The example images and
the two generated parts are written by `scripts/tool-record.sh unwindow`; the fast tests fail
when they stop being what the tool gives
(`crates/whizbang-rascal/tests/it/tool_records/frames.rs`).

## What it does

Writes a crop back into exactly the box of the image that `window` cut it from, read from
the mask `window` handed back. A crop written back unchanged gives the image itself.

- **Use it** after a fill of a window, with the composite and the mask the fill rebound.
- **Do not use it** with a crop changed where its mask weighs nothing: it is refused.
- **Kind.** A pure pixel operation: rungs 1 and 2 apply. No model, no spend.

The action `unwindow`, version 1.0.0:

```
whizbang rascal unwindow --image <identity> --crop <identity> --mask <identity> --store <dir>
```

| Name | Meaning |
|---|---|
| `image` (input) | The image the window was cut from |
| `crop` (input) | The crop to write back: the window's wherever its mask weighs nothing |
| `mask` (input) | The mask `window` handed back |

**Output.** The image at `image`: the given one, the same artifact, when nothing changed.
Refused, in a sentence: a mask that places no crop of this image in whole pixels, or a
scaled one; a crop of another size than the box; a mask not on the crop the box cuts; and
a crop changed where the mask weighs nothing.

## Examples

Each picture is an image three times its size, with the pixels the write changed in light amber. The window is the
middle one of `window`'s record.

| The crop written back as it was | Painted inside its mask's core |
|---|---|
| ![The scene, unchanged](unwindow/unchanged.png) | ![The scene with the block painted](unwindow/painted.png) |

What to look for: unchanged, nothing differs; painted, exactly the block of 12 by 10 the
mask's core covers changes, and nothing else of the box or the image.

### The inputs, in a store

`whizbang store record-inputs --store <dir>` writes the inputs of these examples into a store
that is there, as canonical artifacts made by the constructors this record's tests use, each
with what it depends on, and prints each one's identity (`crates/whizbang/src/record_inputs.rs`).
A walkthrough gives the same inputs to the command line and to the playground. They are:

<!-- inputs: begin -->

| Example | Input | Type | Identity |
|---|---|---|---|
| unchanged | image | image | `sha256:5b673f00862bdb77dc9805e663c997175b329f97a6a31eb55469d099fef74f33` |
| unchanged | crop | image | `sha256:4cfc13c34d82390bbbf8ebc8dc5917b145865a6b810d47096a26899ff03ceb37` |
| unchanged | mask | mask | `sha256:14d3fbe545680c7c3d4cffeb698973818219c9be64610c3d1263d0ca2f221626` |
| unchanged | image of mask | image | `sha256:4cfc13c34d82390bbbf8ebc8dc5917b145865a6b810d47096a26899ff03ceb37` |
| painted | image | image | `sha256:5b673f00862bdb77dc9805e663c997175b329f97a6a31eb55469d099fef74f33` |
| painted | crop | image | `sha256:efd8b1878b44f4202c3fea45eb2c37447f2763730ba8744f78ecbe855292d17f` |
| painted | mask | mask | `sha256:14d3fbe545680c7c3d4cffeb698973818219c9be64610c3d1263d0ca2f221626` |
| painted | image of mask | image | `sha256:4cfc13c34d82390bbbf8ebc8dc5917b145865a6b810d47096a26899ff03ceb37` |

<!-- inputs: end -->

## Rung 2 results

Exact against reference results (`SPEC.md` section 8). Measured on 2026-11-19.

`unwindow::a_window_unwound_is_the_original_outside_the_support` holds rascal's window
written back (`crates/rascal-ops/tests/it/window.rs` at the pinned commit): unchanged it is
the image; changed inside the support it lands in the box alone; changed where the mask
weighs nothing it is refused. `heal::a_fill_through_a_window_keeps_every_pixel_the_mask_never_reaches`
holds a fill written back through the executor. No pixel differs.

The examples in numbers, the same on every machine:

<!-- measured: begin -->

| Example | Pixels of the image changed | Columns changed |
|---|---|---|
| unchanged | 0 | none |
| painted | 120 | 12 of them, 40 to 51 |

<!-- measured: end -->

## Speed and cost

`unwindow` spends nothing. Its time is the window cut again to check the crop, and the write.

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | `unwindow`, the checks and the write |
|---|---|---|
| 1024x768 | 0.8 | 1.4 |
| 4096x3072 | 12.6 | 20 |
| 8000x6000 | 48.0 | 78 |

<!-- timed: end -->

## Alternatives

None tried: the rule is rascal's, ported unchanged but for the differences its module names.

## Limits

| Limit | Example |
|---|---|
| A crop changed where its mask weighs nothing is refused, by one pixel | `unwindow::a_window_unwound_is_the_original_outside_the_support` |
| Only a crop in whole pixels at the image's scale is written back | The action's refusal of a scaled crop |

## Mark and history

**Mark.** None: a pure pixel operation, complete when its record shows rungs 1 and 2, and
this one does.

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests |
| 2. Scored alone | Shown | Exact against rascal's vectors; the times at three sizes |
| 3 and 4 | Do not apply | `SPEC.md` section 8 |

**Gates.** None faced.

**History.** Plan m3, task O2: ported from rascal's `unwindow.rs` at the pinned commit.
Plan m3, task D2: this record.
