# paste

The tool record of `paste` (`SPEC.md` section 8; plan m3, task D1). The example images and
the two generated parts are written by `scripts/tool-record.sh paste`; the fast tests fail
when they stop being what the tool gives (`crates/whizbang-rascal/tests/it/tool_records/paste.rs`).

## What it does

Pastes a fill onto its base image under a mask. Every pixel the mask weighs nothing,
outside its core and blend or under protect, is the base's four bytes; in the core the
fill's colour; on the blend a mix of the two by its weight. `fill` pastes its filler's
answer by the same code.

- **Use it** to put a fill made elsewhere (another tool, an earlier run) onto its picture.
- **Do not use it** with a fill of another size, or a mask on another image: both are
  refused.
- **Kind.** A pure pixel operation: rungs 1 and 2 apply. No model, no spend.

The action `paste`, version 1.0.0:

```
whizbang rascal paste --image <base> --fill <fill> --mask <mask> [--mode feather|hard] --store <dir>
```

| Name | Meaning |
|---|---|
| `image` (input) | The base image; a rendering is refused |
| `fill` (input) | The fill: a picture of the base's size, read only where the mask weighs over 0 |
| `mask` (input) | The mask, on the base |
| `mode` | How the fill is pasted: `feather`, the default, by the mask's core and blend; or `hard`, on the unprotected core alone, the blend read as nothing |

**Output.** One image at the port `image`: the composite, derived from the base, written
by the core's one encoder, whose origin names `paste`, its version, the mode and the three
inputs. When the fill changes no pixel of the base, nothing is derived and the base itself
is handed back.

**The rule,** in the mode `feather`, which the examples use. For each pixel: under protect, or with no core and no blend, the base's four
bytes; in the core, each of red, green and blue the fill's; on a blend `b`, each channel
`((255 - b) base + b fill + 127) / 255` in whole numbers, the nearest whole number to the
mix (no mix is half way, since 255 is odd). In the mode `hard`, a blend pixel is the base's
too. Alpha is the base's, always. The result is
checked against the base and the mask once more by the core (`Derivation::checked`).

## Examples

Pictures of 96 by 64, three times their size; a mask's core white, protected pixels red,
blend in blues.

### A feathered disc, a brick red fill

| Base | Fill | Mask | Composite |
|---|---|---|---|
| ![The scene](paste/feathered.png) | ![Brick red](paste/feathered-fill.png) | ![A feathered disc](paste/feathered-mask.png) | ![The disc pasted](paste/feathered-out.png) |

What to look for: the disc is the fill's colour; its feather mixes the two, so a flat fill
shows a soft halo; every one of the 4,723 pixels the mask weighs nothing is the base's.

### A fill that repaints everything, under a mask with a protected stripe

The fill is the scene's inverse, every pixel changed; the mask is a block with six rows
protected across it.

| Base | Fill | Mask | Composite |
|---|---|---|---|
| ![The scene](paste/protected.png) | ![The inverse](paste/protected-fill.png) | ![A block with a protected stripe](paste/protected-mask.png) | ![The block inverted but for the stripe](paste/protected-out.png) |

What to look for: only the block's unprotected pixels changed, 1,904 of them; the stripe
and everything outside are the base's, though the fill repainted them.

## Rung 2 results

Exact against reference results (`SPEC.md` section 8). Measured on 2026-11-19.

- `composite::the_mix_is_rascals_feather_for_every_original_fill_and_blend`: rascal's
  `feather` paste (`crates/rascal-ops/src/paste.rs` at the pinned commit), in `f32`, gives
  the same byte as the rule for every base, fill and blend: 255 blends by 256 bases by 256 fills, 16,711,680 cases.
- `composite::the_mix_is_the_stated_value_for_each_fixed_vector` and
  `composite::every_mix_is_the_nearest_whole_number_and_none_is_half_way`: values worked
  out in exact fractions apart from the code.
- `composite::after_a_filler_repaints_every_pixel_only_the_support_is_changed`,
  `composite::a_protected_pixel_is_kept_where_the_core_and_the_blend_would_have_reached_it`
  and `paste::feather_keeps_every_pixel_outside_the_mask_and_every_protected_one`, through
  the executor.

<!-- measured: begin -->

| Example | Painted by | Pixels the mask weighs over 0 | Pixels changed | Pixels it weighs nothing, and the base's | Mask handed back |
|---|---|---|---|---|---|
| feathered | a brick red | 1421 | 1421 | 4723 of 4723 | none |
| protected | the scene's inverse | 1904 | 1904 | 4240 of 4240 | none |

<!-- measured: end -->

## Speed and cost

`paste` spends nothing. The mask is `grow`'s mask of the times feathered by a sigma of 3,
the fill one grey. Through the executor a run also reads three files and writes one.

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | `paste`: the answer pasted under the mask | The composite's PNG written |
|---|---|---|---|
| 1024x768 | 0.8 | 3.6 | 17 |
| 4096x3072 | 12.6 | 58 | 269 |
| 8000x6000 | 48.0 | 224 | 1154 |

<!-- timed: end -->

Writing the composite's PNG costs about five times the paste.

## Alternatives

Rascal's `feather` paste in `f32`, the same byte in every case (rung 2); the integer rule
was kept so that no float is in the path. Rascal's `hard` came with task O4
(`paste::hard_writes_the_fill_where_the_core_is_set_and_nothing_else`); its `poisson` is O4's
second part.

## Limits

| Limit | Example |
|---|---|
| A flat fill under a wide feather shows as a halo: the mix is by weight, not by content | The first example |
| The examples and the rung 2 above are of `feather`; `hard` is held by its own test, not by an example here | `paste::hard_writes_the_fill_where_the_core_is_set_and_nothing_else` |
| The fill's alpha is not read; the base's is kept | `composite::a_transparent_source_pixel_keeps_its_alpha_and_takes_colour_like_any_other` |
| A fill that changes nothing hands back the base, with no new image | `composite::a_composite_that_changed_nothing_states_no_derivation` |

## Mark and history

**Mark.** None: a pure pixel operation, complete when its record shows rungs 1 and 2, and
this one does.

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests |
| 2. Scored alone | Shown | Exact against rascal's paste for every case, and against stated values; the times at three sizes |
| 3 and 4 | Do not apply | `SPEC.md` section 8 |

**Gates.** None faced.

**History.** The composite of ADR 0006, ported from rascal's `paste.rs` at the pinned
commit. Plan m3, task F1: `paste` an action. Plan m3, task D1: this record.
