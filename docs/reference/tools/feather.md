# feather

The tool record of `feather` (`SPEC.md` section 8; plan m3, task D1). The example images and
the two generated parts are written by `scripts/tool-record.sh feather`; the fast tests fail
when they stop being what the tool gives (`crates/whizbang-rascal/tests/it/tool_records/shaping.rs`).

## What it does

Sets a mask's blend to a feather of its core's edge, outward, with one sigma for the whole
mask. A fill pasted through the mask then fades into the picture instead of stopping
hard.

- **Use it** last, after the core is final: `remove` and `remove_prompt` feather with a
  sigma of 4, and `repair_area` with 3.
- **Do not use it** before `grow`, `close` or `plug`: each of those clears the blend.
- **Kind.** A pure pixel operation: rungs 1 and 2 apply. No model, no spend.

The action `feather`, version 1.0.0:

```
whizbang rascal feather --mask <identity> --sigma-px 3 --store <dir>
```

| Name | Meaning |
|---|---|
| `mask` (input) | The mask whose core's edge is feathered |
| `sigma_px` | Required. The sigma in pixels of the mask's image, from 0.1 to 10000 |

**Output.** One mask. The blend is replaced. The core, the profile and the prescriptions are
kept, and protect is kept and still wins: the blend is 0 under it. The origin ends with a
step naming `feather`, its version, `sigma_px` and the mask it was made from.

**The rule.** For each pixel outside the core, `k` is the squared distance from its centre
to the nearest core pixel's centre, a whole number, and `d` is `sqrt(k) - 0.5`. The blend
is the nearest whole number to `255 * exp(-d * d / (2 * sigma * sigma))` where `0 < d <= 3 *
sigma`, and 0 elsewhere and in the core. The distances are exact (Meijster, Roerdink and
Hesselink's two passes in whole numbers), and the exponential is this crate's, in
correctly rounded steps, so the same mask gives the same blend on every machine.

## Examples

Masks of 96 by 64, three times their size: the core white, protected pixels red, the blend
in blues, lighter where it weighs more. `sigma_px` 3.

| Given | Made |
|---|---|
| ![An oval with a protected bar across its edge](feather/blob.png) | ![The oval feathered, nothing under the bar](feather/blob-made.png) |
| ![A line one pixel wide](feather/wire.png) | ![The line with a wide feather](feather/wire-made.png) |

What to look for:

- First: the feather fades outward from the oval's edge, and the protected bar has none.
  The core under the bar was already cleared by protect.
- Second: a wire one pixel wide gets a feather nine rows deep on each side, as wide as a
  large blob's. With one sigma for the whole mask, a thin object gets a halo far wider than
  itself: 1,700 blend pixels for a core of 80.

## Rung 2 results

Exact against reference results (`SPEC.md` section 8). Measured on 2026-11-19.
`feather::the_ported_vectors_give_rascals_blends` holds rascal's blob
(`crates/rascal-ops/tests/it/profile_feather.rs`,
`feather_on_a_blob_matches_the_brute_force_and_keeps_protect_and_the_profile`, at the pinned
commit): an ellipse and a corner patch on 40 by 30, its last two rows protected, sigma 2.5.
Every blend equals rascal's brute force, which uses the distance to every core pixel and
the platform's `exp`. It also holds rascal's contour: from a straight edge, the blend
crosses half within half a pixel of 1.1774 sigma, and is 0 past three sigmas. With
`feather::the_squared_distances_are_exact_and_none_without_a_core` and
`feather::the_blend_falls_off_outward_with_sigma_and_protect_wins`, no pixel differs.

<!-- measured: begin -->

| Example | Asked | Masks given | Core before | Core after | Blend after | Protected after |
|---|---|---|---|---|---|---|
| blob | `sigma_px` 3 | 1 | 824 | 824 | 1216 | 160 |
| wire | `sigma_px` 3 | 1 | 80 | 80 | 1700 | 0 |

<!-- measured: end -->

## Speed and cost

`feather` spends nothing. It was timed on `grow`'s mask of the times; the cost is two
passes over the mask and one exponential for each pixel within three sigmas.

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | `feather`, `sigma_px` 3 |
|---|---|---|
| 1024x768 | 0.8 | 12 |
| 4096x3072 | 12.6 | 232 |
| 8000x6000 | 48.0 | 726 |

<!-- timed: end -->

## Alternatives

Rascal's distance transform in doubles through 32-bit floats, and the platform's `exp`.
They were replaced by whole numbers and this crate's exponential, so the identity of a
feathered mask is the same on every machine. On rung 2's blob the two give the same blend
in every pixel.

## Limits

| Limit | Example |
|---|---|
| One sigma for the whole mask: a thin object gets as wide a feather as a large one | The second example |
| The feather goes outward only; the core's own edge stays whole | Both examples |
| Under protect there is no feather | The first example |
| `grow`, `close` and `plug` after it clear the blend | `close.md`, its second example |

## Mark and history

**Mark.** None: a pure pixel operation, complete when its record shows rungs 1 and 2, and
this one does.

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests |
| 2. Scored alone | Shown | Exact against rascal's brute force and contour; the times at three sizes |
| 3 and 4 | Do not apply | `SPEC.md` section 8 |

**Gates.** None faced.

**History.** Plan 2A, task R2: ported from rascal's `feather.rs` and the distance transform
of `primitives.rs` at the pinned commit, in whole numbers and with its own exponential.
Plan m3, task D1: this record.
