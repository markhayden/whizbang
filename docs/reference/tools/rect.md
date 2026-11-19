# rect

The tool record of `rect` (`SPEC.md` section 8; plan m3, task D1). The example images and the
two generated parts are written by `scripts/tool-record.sh rect`; the fast tests fail when
they stop being what the tool gives (`crates/whizbang-rascal/tests/it/tool_records/locations.rs`).

## What it does

Makes a box on an image a location, from its left, top, right and bottom edges in pixels,
as `segment` prints a box. `rasterize` makes its inside a mask (the task `mask_rect` does
both); `remove` and `repair_area` take it as their location.

- **Use it** for a caller's coordinates or a detector's box.
- **Do not use it** for a shape that is not a box: use a polygon, or `stroke` for a mark.
- **Kind.** A pure operation on a shape: rungs 1 and 2 apply. No model, no spend.

The action `rect`, version 1.0.0:

```
whizbang rascal rect --image <identity> --box "20 10 70 40" --store <dir>
```

| Name | Meaning |
|---|---|
| `image` (input) | The image the box is on |
| `box` | Required. `L T R B`: four finite numbers apart by spaces or commas, in pixels of the image |

Refused, in a sentence, with the run ended as invalid: a text that is not four finite
numbers; edges out of order (`left < right` and `top < bottom` are needed); a box with no
part inside the image. A box that reaches past an edge is cut at that edge. **Output:**
one location at the port `location`, on the image, whose origin names `rect`, its
version, the box and the image.

**Its inside**, as `rasterize` draws it: the pixels whose centres are at or right of the
left edge and left of the right one, at or under the top and above the bottom.

## Examples

The scene, 96 by 64, three times its size, with the box's inside in light amber.

| `20 10 70 40` | `60.4 30.5 120 80`, past two edges |
|---|---|
| ![A box of 50 by 30](rect/box.png) | ![A box cut at the right and bottom edges](rect/edge.png) |

What to look for: the first covers columns 20 to 69 and rows 10 to 39, 1,500 pixels; the
second is held as `60.4 30.5 96 64`, cut at the image's edges, and covers column 60, whose
centre 60.5 is past 60.4, and row 30, whose centre is on the edge 30.5: 36 by 34 pixels.

## Rung 2 results

Exact against reference results (`SPEC.md` section 8). Measured on 2026-11-19.
`rect::the_ported_vectors_give_rascals_locations` holds rascal's rule (`crates/rascal-ops/src/rect.rs`
at the pinned commit) on rascal's boxes of its `rasterize` test, in pixels of its picture of
60 by 40, and its refusals; `rasterize::the_ported_vectors_give_rascals_masks` holds the
insides of rascal's boxes to rascal's pixels. No pixel differs.

<!-- measured: begin -->

| Example | Arguments | The location | Pixels inside | Where |
|---|---|---|---|---|
| box | `box` "20 10 70 40" | a box, 20 10 70 40 | 1500 | columns 20 to 69, rows 10 to 39 |
| edge | `box` "60.4 30.5 120 80" | a box, 60.4 30.5 96 64 | 1224 | columns 60 to 95, rows 30 to 63 |

<!-- measured: end -->

## Speed and cost

`rect` spends nothing. Drawing its inside is `rasterize`'s cost, by the box's pixels; the
box timed is half the image's width by four tenths of its height.

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | `rect`: the location made and checked | Its inside drawn, as `rasterize` draws it |
|---|---|---|---|
| 1024x768 | 0.8 | under 0.1 | under 0.1 |
| 4096x3072 | 12.6 | under 0.1 | 0.2 |
| 8000x6000 | 48.0 | under 0.1 | 1.3 |

<!-- timed: end -->

## Alternatives

None tried: the rule is rascal's, in pixels of the image.

## Limits

| Limit | Example |
|---|---|
| A pixel is inside by its centre, so a box with fractional edges covers the pixels whose centres it holds, not the ones it touches | The second example: 60.4 covers column 60, and a box from 60.6 would not |
| A box past an edge is cut silently, and only one wholly outside is refused | The second example |
| Axis-aligned only | A tilted box is a polygon |

## Mark and history

**Mark.** None: a pure operation, complete when its record shows rungs 1 and 2, and this
one does.

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests |
| 2. Scored alone | Shown | Exact against rascal's vectors; the times at three sizes |
| 3 and 4 | Do not apply | `SPEC.md` section 8 |

**Gates.** None faced.

**History.** Plan m3, task O1: ported from rascal's `rect.rs` at the pinned commit, in pixels
of the image and held inside it. Plan m3, task D1: this record.
