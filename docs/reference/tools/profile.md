# profile

The tool record of `profile` (`SPEC.md` section 8; plan m3, task D2). The example images and
the two generated parts are written by `scripts/tool-record.sh profile`; the fast tests fail
when they stop being what the tool gives
(`crates/whizbang-rascal/tests/it/tool_records/seams.rs`).

## What it does

Makes the band mask of a band profile: the core at each row's half-width about the line,
`|x + 0.5 - c| <= half[y]`; the blend the feather of `feather`'s rule with a sigma for each
row, `feather_ratio` times the row's half-width, out to three sigmas; the profile and that
sigma kept on the mask for `ramp`.

- **Use it** after `fan`, before `ramp` and a fill.
- **Kind.** A pure pixel operation: rungs 1 and 2 apply. No model, no spend.

The action `profile`, version 1.0.0:

```
whizbang rascal profile --band <identity> --store <dir>
```

| Name | Meaning |
|---|---|
| `band` (input) | The band profile `fan` made |
| `feather_ratio` | From 0.05 to 2; `profile-v1`'s 0.55 when not given |
| `constants` | `profile-v1` |

**Output.** The band mask at `mask`, for a seam, on the band's image, its source the line.

## Examples

The canvas of 192 by 64, three times its size, its seam at column 96: `smooth`, two
smooth halves of different tones; `detail`, the same with a fine texture from row 32 down. Each picture shows a mask of 192 by 64, three times its size: the core in white, protected pixels in red, the blend in blues, lighter where it weighs more, and the rest dark.

| `smooth` | `detail` |
|---|---|
| ![The band mask, wide on every row](profile/smooth.png) | ![The band mask, thin under the texture](profile/detail.png) |

What to look for: the feather of a wide row reaches far past its core, 1.65 half-widths,
so on the smooth canvas the core and the blend together cover 9,856 of its 12,288 pixels;
under the texture the core is about four pixels wide and its feather under a pixel.

### The inputs, in a store

`whizbang store record-inputs --store <dir>` writes the inputs of these examples into a store
that is there, as canonical artifacts made by the constructors this record's tests use, each
with what it depends on, and prints each one's identity (`crates/whizbang/src/record_inputs.rs`).
A walkthrough gives the same inputs to the command line and to the playground. They are:

<!-- inputs: begin -->

| Example | Input | Type | Identity |
|---|---|---|---|
| smooth | band | band_profile | `sha256:d507a45e55aca5aad545bbf5c4555fc211c555a650eb5c413989179b458b09c7` |
| smooth | image of band | image | `sha256:4a1f416814e4de643c1c200890a6ebc1e98a9e7f5e2b86a53a390062cfc2393c` |
| smooth | line of band | location | `sha256:f33f4f79010ab2a22d3e0a6d08fa77eb9da68237b974107a7e367c68020abf1e` |
| detail | band | band_profile | `sha256:610b511816aead4f7851a5b90a69a16b3c0089a1c20f35d3d2df31e2bce07508` |
| detail | image of band | image | `sha256:8a7b5edd4ad1498c199c212d6d2cd9c1875412945a6c1e78291150007bb9f130` |
| detail | line of band | location | `sha256:b9e7f56466326d6c5d1966096202c312d87146bb2f7fdff2aa6b98b98e958e0c` |

<!-- inputs: end -->

## Rung 2 results

Exact against reference results (`SPEC.md` section 8). Measured on 2026-11-19.

`profile::the_ported_vectors_give_rascals_mask` holds rascal's band mask of rascal's band
(`crates/rascal-ops/tests/it/profile_feather.rs` at the pinned commit: 120 by 40, the
half-width from 3 to 15) to rascal's core and blend, row by row. No pixel differs.

The examples in numbers, the same on every machine:

<!-- measured: begin -->

| Example | Core | Blend | Sigma on rows 0, 24, 40 and 63 |
|---|---|---|---|
| smooth | 3712 | 6144 | 0: 15.840; 24: 15.840; 40: 15.840; 63: 15.840 |
| detail | 1946 | 3366 | 0: 15.840; 24: 15.840; 40: 0.978; 63: 0.978 |

<!-- measured: end -->

## Speed and cost

`profile` spends nothing. The band is `fan`'s on the `detail` canvas at each size; the feather's distances are the cost.

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | `profile` of that band |
|---|---|---|
| 1024x768 | 0.8 | 15 |
| 4096x3072 | 12.6 | 283 |
| 8000x6000 | 48.0 | 940 |

<!-- timed: end -->

## Alternatives

None tried: the rule is rascal's, ported unchanged but for the differences its module names.

## Limits

| Limit | Example |
|---|---|
| The feather reaches three sigmas, 1.65 half-widths, past the core: a wide band's mask covers most of a small canvas | `smooth`: core and blend are 9,856 of 12,288 pixels |
| A thin row's feather is under a pixel wide | `detail`, rows 40 and 63: sigma 0.978 |

## Mark and history

**Mark.** None: a pure pixel operation, complete when its record shows rungs 1 and 2, and
this one does.

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests |
| 2. Scored alone | Shown | Exact against rascal's vectors; the times at three sizes |
| 3 and 4 | Do not apply | `SPEC.md` section 8 |

**Gates.** None faced.

**History.** Plan m3, task O3: ported from rascal's `profile.rs` at the pinned commit.
Plan m3, task D2: this record.
