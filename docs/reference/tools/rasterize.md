# rasterize

The tool record of `rasterize` (`SPEC.md` section 8; plan m3, task D2). The example images and
the two generated parts are written by `scripts/tool-record.sh rasterize`; the fast tests fail
when they stop being what the tool gives
(`crates/whizbang-rascal/tests/it/tool_records/masks.rs`).

## What it does

Makes a location's inside the core of a new mask on the location's image: a box or a
polygon as drawn, the whole image, a line on a column as a band of its width, a line on a
path widened by half its width, a stroke widened by its radius.

- **Use it** to turn a caller's or a detector's location into a mask (`mask_rect` does,
  with `rect`), or a stroke into a repair mask before `fill`.
- **Do not use it** for a view or an absent thing, which have no inside: the call is
  refused.
- **Kind.** A pure pixel operation: rungs 1 and 2 apply. No model, no spend.

The action `rasterize`, version 1.0.0:

```
whizbang rascal rasterize --location <identity> --image <identity> --store <dir>
```

| Name | Meaning |
|---|---|
| `location` (input) | The location, on the image |
| `image` (input) | The image the location is on |
| `use_case` | `seam`, `remove`, `repair`, `protect` or `custom` (the default), recorded on the mask |

**Output.** One mask at `mask`, on the image: the inside as its core, no blend, nothing
protected, its source the location. Refused, in a sentence: a location with no inside, and
one that covers no pixel.

**The rule.** A pixel is inside when its centre, `(x + 0.5, y + 0.5)`, is: inside a box's
left and top edges and short of its right and bottom ones; inside a polygon by the
even-odd rule along each row's centre; within half the width of a column; within the
radius of a path's segments, the ends rounded, with rascal's slack of 0.000000001 on the
radius squared.

## Examples

The scene, 96 by 64, three times its size, with the location's inside in light amber.

| A triangle, (12, 56), (48, 6), (84, 56) | A chevron, (10, 10), (50, 32), (10, 54), (30, 32) |
|---|---|
| ![A triangle's inside](rasterize/triangle.png) | ![A chevron's inside, its notch left out](rasterize/chevron.png) |

What to look for: the triangle's apex at row 6 covers no pixel, since no centre is inside
it there, so the inside starts at row 7; the chevron's notch, between (10, 10), (30, 32)
and (10, 54), is outside. A polygon whose edges cross, a star drawn in one stroke, is no
location at all: `Location::new` refuses it, so the even-odd rule never meets one.

## Rung 2 results

Exact against reference results (`SPEC.md` section 8). Measured on 2026-11-19.

`rasterize::the_ported_vectors_give_rascals_masks` holds rascal's vectors
(`crates/rascal-ops/tests/it/rasterize.rs` at the pinned commit) on rascal's picture of 60
by 40, each of rascal's fractions taken to pixels as rascal's own step took it, to
rascal's pixels and to the brute-force references rascal's test held them to. No pixel
differs.

The examples in numbers, the same on every machine:

<!-- measured: begin -->

| Example | Pixels inside | Their box |
|---|---|---|
| triangle | 1800 | Some([12, 7, 84, 56]) |
| chevron | 440 | Some([10, 10, 49, 54]) |

<!-- measured: end -->

## Speed and cost

`rasterize` spends nothing. Its cost is the box the shape spans, not the image: the chevron is tested pixel by pixel within its box; a stroke's segments within theirs.

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | The chevron, scaled to the image | A stroke of radius 20 across it |
|---|---|---|---|
| 1024x768 | 0.8 | under 0.1 | 1.5 |
| 4096x3072 | 12.6 | 0.1 | 23 |
| 8000x6000 | 48.0 | 0.4 | 87 |

<!-- timed: end -->

## Alternatives

None tried: the rule is rascal's, ported unchanged but for the differences its module names.

## Limits

| Limit | Example |
|---|---|
| A pixel is inside by its centre: a thin shape between centres covers nothing and is refused | The triangle's apex: row 6 is empty |
| A polygon must be simple; one whose edges cross is refused before `rasterize` sees it | `the_records_of_rasterize_shrink_invert_and_inspect_are_what_they_give`, the star |
| The mask has no blend: feather it afterwards | Both examples |

## Mark and history

**Mark.** None: a pure pixel operation, complete when its record shows rungs 1 and 2, and
this one does.

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests |
| 2. Scored alone | Shown | Exact against rascal's vectors and brute-force references; the times at three sizes |
| 3 and 4 | Do not apply | `SPEC.md` section 8 |

**Gates.** None faced.

**History.** Plan m3, task O1: ported from rascal's `rasterize.rs` at the pinned commit.
Plan m3, task D2: this record.
