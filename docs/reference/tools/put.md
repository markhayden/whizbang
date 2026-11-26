# put

The tool record of `put` (`SPEC.md` section 8; plan m3, task D2). The example images and
the two generated parts are written by `scripts/tool-record.sh put`; the fast tests fail
when they stop being what the tool gives
(`crates/whizbang-rascal/tests/it/tool_records/frames.rs`).

## What it does

Puts a cut's canvas back onto its panorama: the canvas's left half onto the panorama's last
`s` columns, its right half onto its first `s`. No other pixel changes, and a canvas put
back unchanged gives the panorama itself.

- **Use it** after working on a cut's canvas, with the plan `cut` handed back and the mask
  the canvas was worked through.
- **Do not use it** with a canvas changed where its mask weighs nothing: it is refused,
  so a fill that strayed never reaches the panorama.
- **Kind.** A pure pixel operation: rungs 1 and 2 apply. No model, no spend.

The action `put`, version 1.1.0:

```
whizbang rascal put --image <identity> --canvas <identity> --plan <identity> --mask <identity> --store <dir>
```

| Name | Meaning |
|---|---|
| `image` (input) | The panorama the plan was made for |
| `canvas` (input) | The canvas: the cut wherever the mask weighs nothing |
| `plan` (input) | The plan `cut` handed back |
| `mask` (input) | The mask the canvas was worked through: on the cut, or rebound from it, or from `apply`'s working image, onto the canvas |

**Output.** The panorama at `image`: the given one, the same artifact, when nothing
changed. Refused, in a sentence: a plan made for another panorama; a canvas of another
size than the plan's; a mask on another canvas; and a canvas that is not the cut wherever
the mask weighs nothing, outside its core and blend or under protect.

## Examples

Each picture is an image three times its size, with the pixels the put changed in light amber. The cut is the
default one of `cut`'s record, 12 by 64; the mask is its middle four columns.

| The canvas put back as it was | Its middle four columns painted |
|---|---|
| ![The panorama, unchanged](put/unchanged.png) | ![The panorama with its edge columns painted](put/painted.png) |

What to look for: unchanged, nothing differs; painted, the four columns about the canvas's
seam land on the panorama's first two and last two columns, 256 pixels, and nothing else
moves.

### The inputs, in a store

`whizbang store record-inputs --store <dir>` writes the inputs of these examples into a store
that is there, as canonical artifacts made by the constructors this record's tests use, each
with what it depends on, and prints each one's identity (`crates/whizbang/src/record_inputs.rs`).
A walkthrough gives the same inputs to the command line and to the playground. They are:

<!-- inputs: begin -->

| Example | Input | Type | Identity |
|---|---|---|---|
| unchanged | image | image | `sha256:31f1498cbc70550fabce91da76dd45cda0cab258cef8d63e5b12fcfc73312c82` |
| unchanged | canvas | image | `sha256:6cdce5d307e6a3160df21c2eecc118973c57cf27a7b383c960a7d8deefd76651` |
| unchanged | plan | plan | `sha256:3dbf794b86a6c8e37f9264cdbf481b41c570a66e91b506ea6a69dbff5a09bf07` |
| unchanged | mask | mask | `sha256:dcb605e9394522b829adbf58e4f2c02f8f3fde1dbbf2ab303f17c8c0b360c051` |
| unchanged | image of mask | image | `sha256:6cdce5d307e6a3160df21c2eecc118973c57cf27a7b383c960a7d8deefd76651` |
| painted | image | image | `sha256:31f1498cbc70550fabce91da76dd45cda0cab258cef8d63e5b12fcfc73312c82` |
| painted | canvas | image | `sha256:9d5a95ffdc78d5d2ef42d1d3e6ecd89611186f94b482072885943d95cd8c0caf` |
| painted | plan | plan | `sha256:3dbf794b86a6c8e37f9264cdbf481b41c570a66e91b506ea6a69dbff5a09bf07` |
| painted | mask | mask | `sha256:dcb605e9394522b829adbf58e4f2c02f8f3fde1dbbf2ab303f17c8c0b360c051` |
| painted | image of mask | image | `sha256:6cdce5d307e6a3160df21c2eecc118973c57cf27a7b383c960a7d8deefd76651` |

<!-- inputs: end -->

## Rung 2 results

Exact against reference results (`SPEC.md` section 8). Measured on 2026-11-19.

`put::a_canvas_put_back_through_its_plan_is_the_original_outside_the_support` holds
rascal's vectors (`crates/rascal-ops/tests/it/cut_put.rs` at the pinned commit): a cut put
back is the panorama byte for byte, and a canvas changed everywhere lands on the two strips
and nowhere else; and `heal::a_fill_through_a_wrapped_cut_keeps_every_pixel_the_mask_never_reaches`
holds a fill put back through the executor. No pixel differs.

The examples in numbers, the same on every machine:

<!-- measured: begin -->

| Example | Pixels of the image changed | Columns changed |
|---|---|---|
| unchanged | 0 | none |
| painted | 256 | 4 of them, 0 to 95 |

<!-- measured: end -->

## Speed and cost

`put` spends nothing. Its time is the cut made again to check the canvas against it, and the write of the two strips; the mask is the canvas's middle four columns.

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | `put`, the checks and the write |
|---|---|---|
| 1024x768 | 0.8 | 1.0 |
| 4096x3072 | 12.6 | 15 |
| 8000x6000 | 48.0 | 61 |

<!-- timed: end -->

## Alternatives

None tried: the rule is rascal's, ported unchanged but for the differences its module names.

## Limits

| Limit | Example |
|---|---|
| A canvas changed where its mask weighs nothing is refused, by one pixel | `the_records_of_cut_put_window_and_unwindow_are_what_they_give` |
| The plan is checked by the panorama's pixels: a copy of the panorama is the same panorama | The plan names a pixel digest |

## Mark and history

**Mark.** None: a pure pixel operation, complete when its record shows rungs 1 and 2, and
this one does.

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests |
| 2. Scored alone | Shown | Exact against rascal's vectors; the times at three sizes |
| 3 and 4 | Do not apply | `SPEC.md` section 8 |

**Gates.** None faced.

**History.** Plan m3, task O2: ported from rascal's `put.rs` at the pinned commit.
Plan m3, task D2: this record.
