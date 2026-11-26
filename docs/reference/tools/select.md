# select

The tool record of `select` (`SPEC.md` section 8; plan m3, task D1). The example images and
the two generated parts are written by `scripts/tool-record.sh select`; the fast tests fail
when they stop being what the tool gives (`crates/whizbang-rascal/tests/it/tool_records/shaping.rs`).

## What it does

Makes one mask from the instance masks to keep: the union of their cores, minus any exclude
boxes, with no blend and nothing protected.

- **Use it** after `segment`, to keep the instances you mean, or the ones that hold a pixel
  you point at. `remove_prompt` calls it on every instance found.
- **Do not use it** to pick one of two instances that overlap by pointing at the overlap:
  both hold the pixel and both are kept (the second example).
- **Kind.** A pure pixel operation: rungs 1 and 2 apply. It calls no model and spends
  nothing.

The action `select`, version 1.0.0:

```
whizbang rascal select (--mask <identity>... | --run <segment run> [--pick N]...) [--at "X Y"] [--exclude "L T R B"]... --store <dir>
```

| Name | Meaning |
|---|---|
| `mask` (input) | 1 to 64 instance masks, all on one image |
| `exclude` | Optional. Boxes cleared from the core, `L T R B` in pixels, boxes apart by `;` |
| `at` | Optional. Keeps only the masks whose core holds the pixel at `X Y` |
| `use_case` | `seam`, `remove` (the default), `repair`, `protect` or `custom`, recorded on the mask |

These are refused, and the run ends as invalid: masks on two images; a box that is not
four finite numbers; a point outside the image, or one that no mask holds; and nothing
left after the boxes. **Output:** one mask on the masks' binding, whose source is every
instance and location they name, and whose origin names `select`, its version, the
boxes, the use case and every mask.

**The rule.** A pixel is in the core when any mask's core holds it, unless its centre is
inside a box: at or right of the left edge and left of the right one, at or under the top
edge and above the bottom one.

## Examples

Masks of 96 by 64, three times their size, with the core white. The instances are three
ovals, as `segment` would commit them: two that overlap on the left, one apart on the right.
The first picture of each pair shows the cores given, together.

| Given | Made |
|---|---|
| ![The first and third ovals](select/kept.png) | ![The two, the third's top cleared](select/kept-made.png) |
| ![All three ovals](select/at.png) | ![The two overlapping ovals](select/at-made.png) |

What to look for:

- First: the first and third ovals are kept, and the box `70 0 96 24` clears the third
  one's top.
- Second: `at "38 32"` is in the overlap of the first two ovals. Both hold it, so both are
  kept and the union is one shape. The third oval is left out.

### The inputs, in a store

`whizbang store record-inputs --store <dir>` writes the inputs of these examples into a store
that is there, as canonical artifacts made by the constructors this record's tests use, each
with what it depends on, and prints each one's identity (`crates/whizbang/src/record_inputs.rs`).
A walkthrough gives the same inputs to the command line and to the playground. They are:

<!-- inputs: begin -->

| Example | Input | Type | Identity |
|---|---|---|---|
| kept | mask | mask | `sha256:62e98d71855a86abffb53cd7af3967c7202801041717222dab02dafe7505cab1` |
| kept | mask | mask | `sha256:0094018db93f3662bc599ad8a82a72a61885b432ca6ef6ccc1249c9f2a41ae1c` |
| kept | image of mask | image | `sha256:5b673f00862bdb77dc9805e663c997175b329f97a6a31eb55469d099fef74f33` |
| at | mask | mask | `sha256:62e98d71855a86abffb53cd7af3967c7202801041717222dab02dafe7505cab1` |
| at | mask | mask | `sha256:150f59fcc02027cba52bee7132f516584922fa7d1e632ef3f2cb8e5e5aa1788a` |
| at | mask | mask | `sha256:0094018db93f3662bc599ad8a82a72a61885b432ca6ef6ccc1249c9f2a41ae1c` |
| at | image of mask | image | `sha256:5b673f00862bdb77dc9805e663c997175b329f97a6a31eb55469d099fef74f33` |

<!-- inputs: end -->

## Rung 2 results

Exact against reference results (`SPEC.md` section 8). Measured on 2026-11-19.
`select::the_ported_vectors_give_rascals_cores` holds rascal's three planes
(`crates/rascal-ops/tests/it/subtract_select_invert.rs`,
`select_keeps_drops_and_excludes_over_three_planes`, at the pinned commit) on its picture of
100 by 60. All three give 1,200 pixels; two give 800; the box `65 0 100 60`, rascal's
`0.65 0 1 1`, leaves 800; a box over everything is refused. With
`select::a_pixel_whose_centre_is_on_an_exclude_boxs_edge_is_cleared` and
`select::holding_keeps_the_masks_whose_core_holds_the_pixel_in_the_order_given`, no pixel
differs.

<!-- measured: begin -->

| Example | Asked | Masks given | Core before | Core after | Blend after | Protected after |
|---|---|---|---|---|---|---|
| kept | the first and third, `exclude` "70 0 96 24" | 2 | 1118 | 982 | 0 | 0 |
| at | all three, `at` "38 32" | 3 | 1491 | 894 | 0 | 0 |

<!-- measured: end -->

## Speed and cost

`select` spends nothing. It was timed on `grow`'s mask of the times and a mask of stripes,
with a box over a quarter of the image.

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | `select` of two masks and a box |
|---|---|---|
| 1024x768 | 0.8 | 1.4 |
| 4096x3072 | 12.6 | 22 |
| 8000x6000 | 48.0 | 85 |

<!-- timed: end -->

## Alternatives

Rascal's `keep` and `drop` indices over one port of many planes. Here `segment` commits a
mask for each instance, so a call names the masks it keeps, and indices are not needed.

## Limits

| Limit | Example |
|---|---|
| `at` keeps every mask that holds the pixel, so it cannot tell overlapping instances apart | The second example |
| A box clears by pixel centres; one with fractional edges clears the pixels whose centres it holds | `select::a_pixel_whose_centre_is_on_an_exclude_boxs_edge_is_cleared` |
| Nothing left is a refusal, not an empty mask | `select::the_ported_vectors_give_rascals_cores`, its last part |
| The blend and protect of the masks given are dropped | Both examples |

## Mark and history

**Mark.** None: a pure pixel operation, complete when its record shows rungs 1 and 2, and
this one does.

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests |
| 2. Scored alone | Shown | Exact against rascal's vectors; the times at three sizes |
| 3 and 4 | Do not apply | `SPEC.md` section 8 |

**Gates.** None faced.

**History.** Plan 2A, task R2: ported from rascal's `select.rs` at the pinned commit, with a
mask for each instance and boxes in pixels. Plan 1D, task Q1: `at`. Plan m3, task D1: this
record.
