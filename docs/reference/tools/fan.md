# fan

The tool record of `fan` (`SPEC.md` section 8; plan m3, task D2). The example images and
the two generated parts are written by `scripts/tool-record.sh fan`; the fast tests fail
when they stop being what the tool gives
(`crates/whizbang-rascal/tests/it/tool_records/seams.rs`).

## What it does

Measures the band along a column line, row by row: a half-width wide where the surface is
smooth and the two sides differ, `thin` where it is detailed or the sides already agree.
From `fan-v2` on, an object's edge inside the opening caps it. The sampling windows are
kept in the band profile, so `ramp` reads the same columns.

- **Use it** on a cut's canvas at its middle column, before `profile`; the task `seam` does.
- **Do not use it** on an image that wraps: cut the panorama first.
- **Kind.** A pure pixel operation: rungs 1 and 2 apply. No model, no spend.

The action `fan`, version 1.2.1:

```
whizbang rascal fan --image <identity> --line <identity> --constants fan-v3 --store <dir>
```

| Name | Meaning |
|---|---|
| `image` (input) | The image, which does not wrap |
| `line` (input) | A location on it that is a whole column |
| `constants` | `fan-v3` (the default), `fan-v1`, `fan-thin-v1` or `fan-v2`, rascal's four sets |
| `wide_max`, `thin_px`, `tex_low`, `tex_high`, `row_sigma`, `width_sigma`, `ref_width`, `object_edge`, `object_margin_px`, `gate` | Each replaces the set's value; the command gives `constants` and `wide_max`, a recipe or a call any of them |

**Output.** The band profile at `band`. Lengths in pixels are at `ref_width` (1620 in
`fan-v3`) and scaled by the image's width: `thin` is 15 of 1620, the widest is
`wide_max` (0.15) of the width. Refused: a line not on the image, an image that wraps, a
line that is not a column, a window clipped to fewer than two columns, `tex_high` not over
`tex_low`.

## Examples

The canvas of 192 by 64, three times its size, its seam at column 96: `smooth`, two
smooth halves of different tones; `detail`, the same with a fine texture from row 32 down. The band's core, as `profile` draws it, in light amber.

| `smooth` | `detail` |
|---|---|
| ![The band open wide on every row](fan/smooth.png) | ![The band wide above, thin under the texture](fan/detail.png) |

What to look for: on the smooth canvas every row opens to the widest, 28.8 pixels each
way; under the texture the band closes to `thin`, 1.778, and the smoothing along the rows
eases it over the rows about 32, so 34 rows are widened, not 32.

### The inputs, in a store

`whizbang store record-inputs --store <dir>` writes the inputs of these examples into a store
that is there, as canonical artifacts made by the constructors this record's tests use, each
with what it depends on, and prints each one's identity (`crates/whizbang/src/record_inputs.rs`).
A walkthrough gives the same inputs to the command line and to the playground. They are:

<!-- inputs: begin -->

| Example | Input | Type | Identity |
|---|---|---|---|
| smooth | image | image | `sha256:4a1f416814e4de643c1c200890a6ebc1e98a9e7f5e2b86a53a390062cfc2393c` |
| smooth | line | location | `sha256:f33f4f79010ab2a22d3e0a6d08fa77eb9da68237b974107a7e367c68020abf1e` |
| detail | image | image | `sha256:8a7b5edd4ad1498c199c212d6d2cd9c1875412945a6c1e78291150007bb9f130` |
| detail | line | location | `sha256:b9e7f56466326d6c5d1966096202c312d87146bb2f7fdff2aa6b98b98e958e0c` |

<!-- inputs: end -->

## Rung 2 results

Exact against reference results (`SPEC.md` section 8). Measured on 2026-11-19.

`fan::the_ported_vectors_give_rascals_band_for_each_set` holds rascal's band for each of
its four sets (rascal's `fan_profile` at the pinned commit) on a picture of 256 by 128: a
step that grows down the rows, texture in the lower half, a dark disc beside the line and
one across it. Every half-width is rascal's.

The examples in numbers, the same on every machine:

<!-- measured: begin -->

| Example | Column | Thin | Half-width on rows 0, 24, 40 and 63 | Rows widened |
|---|---|---|---|---|
| smooth | 96 | 1.778 | 0: 28.800; 24: 28.800; 40: 28.800; 63: 28.800 | 64 |
| detail | 96 | 1.778 | 0: 28.800; 24: 28.800; 40: 1.778; 63: 1.778 | 34 |

<!-- measured: end -->

## Speed and cost

`fan` spends nothing. The canvas of the times is the `detail` canvas at each size.

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | `fan` under `fan-v3` |
|---|---|---|
| 1024x768 | 0.8 | 8.5 |
| 4096x3072 | 12.6 | 65 |
| 8000x6000 | 48.0 | 256 |

<!-- timed: end -->

## Alternatives

Rascal's four constant sets are the alternatives, each held by the rung 2 test; `fan-v3` is the default, as at rascal's tag.

## Limits

| Limit | Example |
|---|---|
| At a width far under `ref_width` the windows shrink to a column or two, and a texture reads in few pixels. The scale is meant: the windows are rascal's geometry at `ref_width`, scaled with the picture, so a small canvas is read as a large one would be at its own resolution. No floor is set | The seam's canvas of a panorama 800 wide (strip 80) is 160 wide: `thin` is 1.48 pixels, and with the line at column 80 the step windows are column 77 and column 83, one a side, and the texture windows 63 to 79 and 82 to 98 (`on_a_small_panorama_the_windows_are_the_scale_meant_one_column_a_side`) |
| The band is symmetric about the line: one side's detail closes both | `detail` |
| Only a whole column: a seam that is not straight is not measured | The action's refusal of a path |

## Mark and history

**Mark.** None: a pure pixel operation, complete when its record shows rungs 1 and 2, and
this one does.

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests |
| 2. Scored alone | Shown | Exact against rascal's vectors for its four sets; the times at three sizes |
| 3 and 4 | Do not apply | `SPEC.md` section 8 |

**Gates.** None faced.

**History.** Plan m3, task O3: ported from rascal's `fan.rs` at the pinned commit. Plan
mE, task P1: the scale of the windows on a small panorama stated as meant, with an example.
Plan m3, task D2: this record.
