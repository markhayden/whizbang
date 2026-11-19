# mask_rect

The tool record of the task `mask_rect` (`SPEC.md` sections 7 and 8; plan m3, task D1). The
example images and the two generated parts are written by `scripts/tool-record.sh
mask_rect`; the fast tests fail when they stop being what the recipe gives
(`crates/whizbang-rascal/tests/it/tool_records/tasks.rs`).

## What it does

Makes a rectangle on an image a mask: the box's pixels the core, no blend and nothing
protected. A recipe of two steps, `rect` and `rasterize`, shipped in
`crates/whizbang-rascal/recipes/mask_rect.toml`.

- **Use it** to mask a known box, such as a caller's coordinates or a detector's box, with
  no model.
- **Do not use it** for a mask meant to go straight into a fill: it has a hard edge. Feather
  it, or use `repair_area`, which feathers a location before filling.
- **Kind.** A task that uses no model: rungs 1 and 2 apply. It spends nothing.

The recipe `mask_rect`, version 1.0.0:

```
whizbang rascal mask-rect --image <identity> --box "20 10 70 40" --store <dir>
```

| Name | Meaning |
|---|---|
| `image` (input) | The picture |
| `box` | Required. `L T R B` in pixels of the picture, as `rect` takes it |
| `set`, `until` | What every recipe takes: overrides of a step's arguments, and a step to stop after |

**Output:** one mask at `mask`, on
the picture, its use `custom`; each step is a run of its own under the recipe's.

## Examples

Masks of 96 by 64, three times their size: the core white, the rest dark.

| `20 10 70 40` | `60.4 30.5 120 80`, past two edges |
|---|---|
| ![A rectangle of 50 by 30](mask_rect/box.png) | ![A rectangle cut at the edges](mask_rect/edge.png) |

What to look for: the first core is columns 20 to 69 and rows 10 to 39, 1,500 pixels; the
second is cut at the image's edges and covers the pixels whose centres are in the box, 36
by 34. No blend, and nothing protected, in either.

## Rung 2 results

Exact against reference results (`SPEC.md` section 8). Measured on 2026-11-19. The task's
acceptance case, `task::mask_rect_makes_the_rectangle_as_a_mask`, runs the shipped recipe
through the executor; its two steps are exact against rascal's vectors by
`rect::the_ported_vectors_give_rascals_locations` and
`rasterize::the_ported_vectors_give_rascals_masks` (rascal's `rect.toml` at the pinned
commit is the recipe ported). No pixel differs.

<!-- measured: begin -->

| Example | `box` | Core | Blend | Protected | Use case |
|---|---|---|---|---|---|
| box | "20 10 70 40" | 1500 | 0 | 0 | custom |
| edge | "60.4 30.5 120 80" | 1224 | 0 | 0 | custom |

<!-- measured: end -->

## Speed and cost

`mask_rect` spends nothing. A whole run through the executor, on a picture in a store on
this machine's disk, from a fresh store each time: the picture is read from its file, and
the location and the mask are written.

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | A whole run of `mask_rect` through the executor |
|---|---|---|
| 1024x768 | 0.8 | 247 |
| 2048x1536 | 3.1 | 334 |
| 4096x3072 | 12.6 | 1104 |

<!-- timed: end -->

Most of a run is reading the picture and writing the mask's file; drawing the box is under
two milliseconds at these sizes (`rect.md`).

## Alternatives

Rascal's command made the box a location before the recipe ran, and rendered the mask in
grey; here the box is the recipe's text input, and a rendering is the action `render`'s.

## Limits

| Limit | Example |
|---|---|
| A hard edge: no blend | Both examples |
| A box past an edge is cut silently | The second example |
| Axis-aligned only | `rect.md` |

## Mark and history

**Mark.** None: a task that uses no model, complete when its record shows rungs 1 and 2,
and this one does.

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests |
| 2. Scored alone | Shown | Its steps exact against rascal's vectors; the times at three sizes |
| 3 and 4 | Do not apply | It uses no model |

**Gates.** None faced.

**History.** Plan m3, task O1: ported from rascal's `rect.toml` at the pinned commit, named
`mask_rect`. Plan m3, task D1: this record.
