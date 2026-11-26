# line

The tool record of `line` (`SPEC.md` section 8; plan m3, task D1). The example images and the
two generated parts are written by `scripts/tool-record.sh line`; the fast tests fail when
they stop being what the tool gives (`crates/whizbang-rascal/tests/it/tool_records/locations.rs`).

## What it does

Makes a line location on an image: a whole column from top to bottom, or a path of two or
more points, with its full width in pixels. A location is a shape on an image, not a mask:
`rasterize` makes its inside a mask, and `subtract` protects it.

- **Use it** for a feature that runs straight or nearly: a seam's column on a cut canvas, a
  wire, a pole, an edge.
- **Do not use it** for a rough mark along a defect (use `stroke`, whose width is a radius
  about the path) or for an area (use `rect`, or a polygon).
- **Kind.** A pure operation on a shape: rungs 1 and 2 apply. It calls no model, spends
  nothing, and reads only the image's size.

The action `line`, version 1.0.0:

```
whizbang rascal line --image <identity> (--column 48 | --path "8 56; 88 8") --width-px 6 --store <dir>
```

| Name | Meaning |
|---|---|
| `image` (input) | The image the line is on |
| `column` | The column, in pixels from the left edge, from 0 to the image's width |
| `path` | Two or more points, `X Y`, points apart by `;`, each inside the image (its edges included) |
| `width_px` | Required. The full width, over 0 and at most the image's diagonal |

Exactly one of `column` and `path` is given; neither, both, a column outside the image, a
path of one point and a width that is not over 0 are refused, in a sentence, and the run
ends as invalid. **Output:** one location at the port `location`, on the image, whose
origin is one step naming `line`, its version, its arguments and the image.

**Its inside**, as `rasterize` draws it: a pixel is covered when its centre is no further
than half the width from the column, or within half the width of a segment of the path,
the ends rounded.

## Examples

The scene, 96 by 64, three times its size, with the location's inside in light amber.

| A column, 48, 6 wide | A path from (8, 56) to (88, 8), 3 wide |
|---|---|
| ![A band six columns wide](line/column.png) | ![A thin diagonal band](line/path.png) |

What to look for: the column covers the six columns whose centres are within 3 of 48,
columns 45 to 50, in every row; the path is a band 3 wide whose ends are round, so it
covers columns 7 to 88 for a path from column 8 to 88.

### The inputs, in a store

`whizbang store record-inputs --store <dir>` writes the inputs of these examples into a store
that is there, as canonical artifacts made by the constructors this record's tests use, each
with what it depends on, and prints each one's identity (`crates/whizbang/src/record_inputs.rs`).
A walkthrough gives the same inputs to the command line and to the playground. They are:

<!-- inputs: begin -->

| Example | Input | Type | Identity |
|---|---|---|---|
| column | image | image | `sha256:5b673f00862bdb77dc9805e663c997175b329f97a6a31eb55469d099fef74f33` |
| path | image | image | `sha256:5b673f00862bdb77dc9805e663c997175b329f97a6a31eb55469d099fef74f33` |

<!-- inputs: end -->

## Rung 2 results

Exact against reference results (`SPEC.md` section 8). Measured on 2026-11-19.
`line::the_ported_vectors_give_rascals_locations` holds rascal's own vectors
(`crates/rascal-ops/tests/it/rasterize.rs`, `the_line_and_stroke_ops_make_validated_locations`,
at the pinned commit) on rascal's picture of 60 by 40: each shape exactly, and rascal's
refusals. `rasterize::the_ported_vectors_give_rascals_masks` holds the insides of rascal's
lines to rascal's pixels and to a brute force. No pixel differs.

<!-- measured: begin -->

| Example | Arguments | The location | Pixels inside | Where |
|---|---|---|---|---|
| column | `column` 48, `width_px` 6 | a column at 48, 6 wide | 384 | columns 45 to 50, rows 0 to 63 |
| path | `path` "8 56; 88 8", `width_px` 3 | a path 8 56; 88 8, 3 wide | 276 | columns 7 to 88, rows 7 to 56 |

<!-- measured: end -->

## Speed and cost

`line` spends nothing. Making the location costs its points; drawing its inside is
`rasterize`'s cost, by the path's box, shown for a path of three points across the image.

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | `line`: the location made and checked | Its inside drawn, as `rasterize` draws it |
|---|---|---|---|
| 1024x768 | 0.8 | under 0.1 | 1.4 |
| 4096x3072 | 12.6 | under 0.1 | 22 |
| 8000x6000 | 48.0 | under 0.1 | 84 |

<!-- timed: end -->

## Alternatives

None tried: the shapes and their drawing are rascal's, after its step from fractions of a
frame to pixels.

## Limits

| Limit | Example |
|---|---|
| A path is straight segments; a curve needs a point at each bend | The second example is one segment |
| A column runs top to bottom; a line across is given as a path | `--path "0 32; 96 32"` |
| The drawing rounds a path's ends, so it reaches past the end points | The second example: columns 7 to 88 |
| A width or a point outside the image is refused, not clipped | `line::the_ported_vectors_give_rascals_locations` |

## Mark and history

**Mark.** None: a pure operation, complete when its record shows rungs 1 and 2, and this
one does.

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests |
| 2. Scored alone | Shown | Exact against rascal's vectors; the times at three sizes |
| 3 and 4 | Do not apply | `SPEC.md` section 8 |

**Gates.** None faced.

**History.** Plan m3, task O1: ported from rascal's `line.rs` at the pinned commit, in
pixels of the image where rascal's was in fractions of a frame. Plan m3, task D1: this
record.
