# inspect

The tool record of `inspect` (`SPEC.md` section 8; plan m3, task D2). The example images and
the two generated parts are written by `scripts/tool-record.sh inspect`; the fast tests fail
when they stop being what the tool gives
(`crates/whizbang-rascal/tests/it/tool_records/masks.rs`).

## What it does

Shows a mask as a person wants to see it: the overlay, the image tinted toward magenta by
half the mask's weight with the core's edge in white; the core's width on each row, as a
series; and a report of the counts of each zone, the box of what the mask weighs, its band
profile and its prescriptions, said in the run's journal.

- **Use it** to check a mask by eye before a fill, or to compare two by their counts.
- **Do not use it** for a verdict: it measures nothing about the edit.
- **Kind.** A pure pixel operation: rungs 1 and 2 apply. No model, no spend.

The action `inspect`, version 1.0.0:

```
whizbang rascal inspect --mask <identity> --image <identity> --store <dir>
```

| Name | Meaning |
|---|---|
| `mask` (input) | The mask, on the image |
| `image` (input) | The image the overlay is drawn on |

**Output.** The overlay at `overlay`, an opaque image whose role is a rendering, and the
widths at `widths`, a series on the image. The report is in the journal only: a result of
its own waits for the result kinds of task Q1.

## Examples

Each picture is an image three times its size: the overlay of each mask on the scene.

| A feathered disc with a protected patch | A disc with a protected hole |
|---|---|
| ![The overlay of the feathered disc](inspect/feathered.png) | ![The overlay of the holed disc](inspect/holed.png) |

What to look for: the white edge runs round the outside of each core and round the
protected hole as well, since a protected pixel is outside the core; the feather is a
tint that fades out, and the protected patch and hole are the image untouched.

## Rung 2 results

Exact against reference results (`SPEC.md` section 8). Measured on 2026-11-19.

`inspect::the_ported_vectors_give_rascals_report` holds rascal's report of rascal's box on
its picture of 320 by 200 (`crates/rascal-mask/tests/it/inspect_json.rs` at the pinned
commit), and `inspect::the_overlay_is_rascals_tint_with_the_core_edge_in_white` its overlay
(`rendering.rs`'s `overlay`), pixel for pixel. No pixel differs.

The examples in numbers, the same on every machine:

<!-- measured: begin -->

| Example | The report | Widest row of the core |
|---|---|---|
| feathered | inspect: 96x64; core 580, blend 644, protect 64, painted 1224 (weight 819.8588277418166); support [20, 10, 60, 51); no band; 0 prescriptions | 27 |
| holed | inspect: 96x64; core 1144, blend 0, protect 113, painted 1144 (weight 1144); support [28, 12, 69, 53); no band; 0 prescriptions | 38 |

<!-- measured: end -->

## Speed and cost

`inspect` spends nothing. The mask is `grow`'s mask of the times on a grey image of its size.

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | The overlay | The report and the widths |
|---|---|---|---|
| 1024x768 | 0.8 | 2.6 | 1.4 |
| 4096x3072 | 12.6 | 38 | 23 |
| 8000x6000 | 48.0 | 144 | 86 |

<!-- timed: end -->

## Alternatives

None tried: the rule is rascal's, ported unchanged but for the differences its module names.

## Limits

| Limit | Example |
|---|---|
| The edge is white whatever the image: on a white image it does not show | Rascal's colours, kept |
| The report is said in the journal, not handed back | Task Q1's result kinds |
| The weight is printed as summed, to every digit | The first example: 819.8588277418166 |

## Mark and history

**Mark.** None: a pure pixel operation, complete when its record shows rungs 1 and 2, and
this one does.

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests |
| 2. Scored alone | Shown | Exact against rascal's report and overlay; the times at three sizes |
| 3 and 4 | Do not apply | `SPEC.md` section 8 |

**Gates.** None faced.

**History.** Plan m3, task O4: ported from rascal's `inspect.rs` at the pinned commit.
Plan m3, task D2: this record.
