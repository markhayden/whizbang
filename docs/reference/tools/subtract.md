# subtract

The tool record of `subtract` (`SPEC.md` section 8; plan m3, task D1). The example images and
the two generated parts are written by `scripts/tool-record.sh subtract`; the fast tests fail
when they stop being what the tool gives
(`crates/whizbang-rascal/tests/it/tool_records/subtract.rs`).

## What it does

Makes a mask's protect larger by a second mask's support, its core or its blend, or by a
location's inside, grown by a margin. The core and the blend are cleared under the new
protect; the profile and the prescriptions stay, since what applies them honours protect.

- **Use it** to keep a fill off something beside what is removed: a face, a sign, a
  hand. `remove` and `remove_prompt` call it with their optional `protect` and a margin
  of 2.
- **Do not use it** with a feathered second mask unless you mean its whole feather: the
  faintest blend counts (the second example).
- **Kind.** A pure pixel operation: rungs 1 and 2 apply. It calls no model and spends
  nothing.

The action `subtract`, version 1.1.0:

```
whizbang rascal subtract --mask <identity> [--subtract <mask> | --location <location>] [--grow-px 2] --store <dir>
```

| Name | Meaning |
|---|---|
| `mask` (input) | The mask to protect pixels in |
| `subtract` (input) | Optional. A mask on the same image whose core and blend become protect |
| `location` (input) | Optional, not with `subtract`. A location on the same image whose inside, as `rasterize` draws it, becomes protect |
| `grow_px` | How far what is protected is grown, from 0 to 10000 pixels; 0 when not given |

Both inputs given are refused ("give subtract or location, not both"); so is a second mask
or a location on another image, and a location with no inside (a view, an absent thing).
With neither, the mask is handed back unchanged, the same artifact, so a recipe whose
optional `protect` was left out reads the same either way.

**Output.** One mask: the same binding, use and source; protect grown; core and blend
cleared under it; an origin ending with a step naming `subtract`, its version, `grow_px`,
the mask and what was subtracted.

**The rule.** The support is every pixel where the second mask's core or blend is not 0,
or the location's inside. It is grown by `grow_px` as `grow` grows a core with no
`down_px`: every pixel whose centre is within the whole part of `grow_px` squared, exactly,
of a support pixel's centre. Every grown pixel joins protect (`Mask::protect_union`).

## Examples

Masks of 96 by 64, three times their size: the core white, protected pixels red, the blend
in blues, the rest dark. The mask is an oval feathered by a sigma of 2.

### A bar across the oval

The second mask is a bar four columns wide from top to bottom, with no blend; `grow_px` 2.

| The mask | The second mask | After |
|---|---|---|
| ![The feathered oval](subtract/bar.png) | ![The bar](subtract/bar-second.png) | ![The oval with a protected band](subtract/bar-subtracted.png) |

What to look for: protect is columns 44 to 51 in every row, the bar's four columns and two
on each side; neither the core nor the blend is set there, and the rest of the oval and its
feather are as they were.

### A feathered second mask, which reaches further than it looks

The second mask is a disc of radius 4 feathered by a sigma of 3; `grow_px` 0.

| The mask | The second mask | After |
|---|---|---|
| ![The feathered oval](subtract/faint.png) | ![A small disc with a wide feather](subtract/faint-second.png) | ![The oval with a wide protected disc](subtract/faint-subtracted.png) |

What to look for: the disc's core is 49 pixels, but protect is 561. Its blend reaches nine
pixels past the core, down to weights of 5 in 255 that barely show, and every one of them
is protected: in the middle row protect runs from column 17 to 43 around a core of columns
26 to 34. To protect the shape alone, subtract its mask before it is feathered, or give a
location.

## Rung 2 results

Exact against reference results, and timed across sizes (`SPEC.md` section 8). Measured on
2026-11-19.

**Exact.** Rascal's own vectors (`crates/rascal-ops/tests/it/subtract_select_invert.rs` at
the pinned commit) on rascal's picture of 100 by 60, held to rascal's pixels:
`subtract::the_ported_vectors_of_a_mask_give_rascals_masks` (a band's core and blend,
grown, become protect, the profile is kept, a grown box clips at the edge, another image
is refused) and `subtract::the_ported_vectors_of_a_location_give_rascals_masks`; and
`subtract::with_no_mask_to_subtract_the_mask_is_handed_back_unchanged`. No pixel differs.
The growth is `grow`'s, exact by its own record.

<!-- measured: begin -->

| Example | `grow_px` | Second mask's core | Its blend | Core before | Core after | Blend before | Blend after | Protected before | Protected after | Profile and prescriptions |
|---|---|---|---|---|---|---|---|---|---|---|
| bar | 2 | 256 | 0 | 1193 | 943 | 924 | 820 | 0 | 512 | kept |
| faint | 0 | 49 | 512 | 1193 | 841 | 924 | 730 | 0 | 561 | kept |

<!-- measured: end -->

## Speed and cost

`subtract` spends nothing. The mask is `grow`'s mask of the times; the second mask sets
the first 16 columns of every 256; the box is a quarter of the image.

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | `subtract` of a mask, `grow_px` 4 | `subtract` of a box, `grow_px` 4 |
|---|---|---|---|
| 1024x768 | 0.8 | 2.9 | 3.0 |
| 4096x3072 | 12.6 | 47 | 47 |
| 8000x6000 | 48.0 | 179 | 181 |

<!-- timed: end -->

The growth is two sweeps over the mask's pixels, so a mask and a box cost the same.

## Alternatives

None tried beyond rascal's: the growth is `grow`'s, whose comparison of methods is in its
record. Rascal handed back a new mask when there was nothing to protect; here the same
artifact comes back, on purpose.

## Limits

| Limit | Example |
|---|---|
| The second mask's faintest blend counts as support | The second example: 49 core pixels protect 561 |
| Protect only grows: nothing here takes a pixel out of protect | `Mask::protect_union` |
| A location must be on the mask's image; a view or an absent thing has no inside and is refused | `subtract::the_ported_vectors_of_a_location_give_rascals_masks` |
| A margin counts by its exact square, as `grow`'s radius does | `grow.md`, "Limits" |

## Mark and history

**Mark.** None: a pure pixel operation, complete when its record shows rungs 1 and 2, and
this one does.

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests |
| 2. Scored alone | Shown | Exact against rascal's vectors; the times at three sizes |
| 3 and 4 | Do not apply | `SPEC.md` section 8: a pure pixel operation |

**Gates.** None faced.

**History.**

- Plan m3, task R1: ported from rascal's `subtract.rs` at the pinned commit, the form with
  a mask (version 1.0.0).
- Plan m3, task O1: the form with a location, by `rasterize` (version 1.1.0).
- Plan m3, task D1: this record.
