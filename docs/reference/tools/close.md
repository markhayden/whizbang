# close

The tool record of `close` (`SPEC.md` section 8; plan m3, task D1). The example images and
the two generated parts of this page are written by `scripts/tool-record.sh close`, and the
fast tests fail when the images or the measurements stop being what the tool gives
(`crates/whizbang-rascal/tests/it/tool_records/close.rs`).

## What it does

Closes the core of a mask by a disc: grows it by the disc and then erodes it by the same
disc, so gaps and notches narrower than the disc fill in and the outer edge stays where it
was.

- **Use it** after `grow` on a segmenter's instance, to shut the cracks and bites the
  model left; `remove` and `remove_prompt` do, with a radius of 3.
- **Do not use it** on two objects that must stay apart and are closer than the disc's
  width: it joins them (the second example). Nor on a feathered mask whose feather you
  want: it clears the blend.
- **Kind.** A pure pixel operation (`SPEC.md` section 8): rungs 1 and 2 apply. It calls no
  model and spends nothing.

The action `close`, version 1.0.0, in `whizbang-rascal`:

```
whizbang rascal close --mask <identity> --radius-px 3 --store <dir>
```

| Name | Meaning |
|---|---|
| `mask` (input) | The mask whose core is closed, by its identity in the store |
| `radius_px` | Required. The radius of the disc in pixels of the mask's image, from 0 to 10000; it need not be whole |

A number outside its range is refused when the call is prepared, and no run is made.

**Output.** One mask at the port `mask`: the same binding, use, source and protect; the
closed core; no blend, no profile and no prescriptions (`Mask::replace_core`); an origin
that ends with a step naming `close`, its version, `radius_px` and the mask it was made
from. A progress event gives the core's pixels before and after.

**The rule.** K is the whole part of `radius_px` squared exactly, as `grow` takes it. A
pixel is near another when the squared distance between their centres is at most K.

1. The grown core is every pixel near a core pixel.
2. The closed core is every pixel near no pixel of the image outside the grown core.
   Outside the image counts as grown, so a shape against the border keeps its edge.
3. The new core is every closed pixel that is not protected.

Both steps are `grow`'s two sweeps, so a call costs the mask's pixels whatever the radius.

## Examples

Each picture shows a mask of 96 by 64, three times its size: the core in white, protected
pixels in red, the blend in blues, lighter where it weighs more, and the rest dark.

### Two blocks, a gap and a notch

Two blocks four columns apart, a notch two columns wide bitten from the top of the left
one, and a protected patch in the gap. `radius_px` 3.

| Before | After |
|---|---|
| ![Two blocks, a gap, a notch and a protected patch](close/blocks.png) | ![The blocks closed](close/blocks-closed.png) |

What to look for:

- The gap and the notch fill, but for one row at each mouth: the top row of the notch and
  of the gap, and the bottom row of the gap, stay open. A disc of radius 3 that would
  close the mouth row reaches a pixel outside the grown core, which dips at a mouth. So a
  gap is shut through its depth and left nicked at its ends: 110 pixels join the core.
- The outer edges and corners of both blocks are where they were.
- The protected patch stays out of the core, though the gap around it closed.

### Two discs that should stay apart, which it handles badly

Two discs of radius 10, four columns apart, feathered by a sigma of 2. `radius_px` 3.

| Before | After |
|---|---|
| ![Two feathered discs](close/discs.png) | ![The discs joined, and the feather gone](close/discs-closed.png) |

What to look for: the two discs are one shape after, joined by a waist across the gap,
and the feather is gone (944 blend pixels before, none after). A fill pasted through this
mask paints the gap and has a hard edge; feather again after closing, as `remove` does.

## Rung 2 results

Scored alone, for a pure pixel operation, means exact against a reference result, and timed
across sizes (`SPEC.md` section 8). Measured on 2026-11-19.

**Exact.** `close::the_ported_vectors_give_rascals_masks` holds rascal's own two vectors
(`crates/rascal-ops/tests/it/grow_shrink_close.rs` at the commit `docs/sources.toml` pins:
a gap narrower than the disc closes and the extent is kept; a shape at the image's border
keeps its edge) to rascal's pixels on its picture of 120 by 80, and every pixel of the
closed planes to a brute force over the disc, apart from the code. No pixel differs. The
disc is `grow`'s, whose own rung 2 is in its record (`grow.md`).

The examples in numbers, the same on every machine:

<!-- measured: begin -->

| Example | `radius_px` | K | Core before | Core after | Blend before | Blend after | Protected before and after |
|---|---|---|---|---|---|---|---|
| blocks | 3 | 9 | 1528 | 1638 | 0 | 0 | 16 |
| discs | 3 | 9 | 634 | 658 | 944 | 0 | 0 |

<!-- measured: end -->

## Speed and cost

`close` spends nothing. The mask of the times is `grow`'s: a blob and a thin line in every
square of 256 pixels, and a protected stripe every 512 rows.

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | `close`, `radius_px` 3 | `close`, `radius_px` 100 |
|---|---|---|---|
| 1024x768 | 0.8 | 5.4 | 4.9 |
| 4096x3072 | 12.6 | 86 | 77 |
| 8000x6000 | 48.0 | 327 | 291 |

<!-- timed: end -->

The time does not depend on the radius: 3 and 100 cost the same. Through the executor a
run also reads the mask's file three times, as `grow`'s record says.

## Alternatives

- **Rascal's disc**, a distance transform in doubles taken through 32-bit floats with a
  slack on the radius squared. Replaced for `grow`, and `close` uses `grow`'s sweeps; the
  comparison of the two, on the same masks, is in `grow`'s record under "Alternatives".
- **The disc stamped from every pixel**, the rule as written: exact and far slower at a
  large radius (`grow`'s record). The brute force of the rung 2 test is this method.

## Limits

| Limit | Example |
|---|---|
| A mouth row stays open: a gap or notch is filled through its depth, not at the row where it opens | The first example: rows 16 and 47 of the gap, row 16 of the notch |
| Objects closer than the disc's width are joined | The second example |
| The blend, the profile and the prescriptions are lost, whatever the radius | The second example: 944 blend pixels, then none |
| A radius that falls a hair under a square root counts by its exact square, as in `grow` | `grow.md`, "Limits" |
| The command waits for its run in its own process, and another process cannot cancel it | ADR 0008, "The first door" |

## Mark and history

**Mark.** None: `close` is a pure pixel operation, complete when its record shows rungs 1
and 2, and this one does.

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests |
| 2. Scored alone | Shown | Exact against rascal's vectors and a brute force; the times at three sizes |
| 3 and 4 | Do not apply | `SPEC.md` section 8: a pure pixel operation |

**Gates.** None faced.

**History.**

- Plan m3, task R1: ported from rascal's `close.rs`, `refine.rs` and
  `primitives.rs` at the pinned commit, on `grow`'s sweeps in whole numbers.
- 2026-11-19, plan m3, task D1: this record.
