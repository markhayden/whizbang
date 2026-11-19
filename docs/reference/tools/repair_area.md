# repair_area

The tool record of the task `repair_area` (`SPEC.md` sections 7 and 8; plan m3, task B1, item 2).
The example images and the two generated parts are written by `scripts/tool-record.sh
repair_area`; the fast tests fail when they stop being what the recipe gives
(`crates/whizbang-rascal/tests/it/tool_records/recipes.rs`). The examples run under a
scripted filler: a test makes no live call.

## What it does

Repairs a stain or a spot at a location: the box or polygon rasterized as drawn, feathered by a sigma of 3, and filled by the configured filler. A recipe of its steps, `rasterize`, `feather` and `fill`, shipped in
`crates/whizbang-rascal/recipes/repair_area.toml`, each step a run of its own under the recipe's.

- **Use it** to repaint a small area whose place you know, keeping its edges as drawn.
- **Do not use it** for a thing with a shadow, which `remove` grows downward for; nor for a thin scratch, which `repair` narrows to its edges.
- **Kind.** A model-based task: all four rungs apply. It spends what its filler charges.

The recipe `repair_area`, version 1.0.0:

```
whizbang rascal repair-area --image <identity> (--location <identity> | --box "L T R B") [--backend <name>] --store <dir>
```

| Name | Meaning |
|---|---|
| `image` (input) | The picture |
| `location` (input) | The area, a box or a polygon on the picture |
| `backend` | Optional. The filler, by its name in `[backends.fill]` |
| `set`, `until` | What every recipe takes: overrides of a step's arguments, and a step to stop after |

**Output:** at `image`, the composite: every pixel the mask weighs nothing the picture's. At
`mask`, the mask, rebound onto the composite through the derivation `fill` checked.

## Examples

Pictures of 96 by 64, three times their size; a mask's core white, protected pixels red,
blend in blues. The scripted filler paints brick red wherever the support is, so the
composite shows exactly where the fill landed and how the feather mixed it.

### A spot

| Picture | Mask handed back | Composite |
|---|---|---|
| ![A dark spot and its box](repair_area/spot.png) | ![The mask](repair_area/spot-mask.png) | ![The composite](repair_area/spot-out.png) |

### A stain, as a polygon

| Picture | Mask handed back | Composite |
|---|---|---|
| ![A darkened triangle](repair_area/stain.png) | ![The mask](repair_area/stain-mask.png) | ![The composite](repair_area/stain-out.png) |

What to look for: the core is the location as drawn, not grown; the feather spreads the fill a few pixels outside it.

<!-- measured: begin -->

| Example | Given | Core | Blend | Protected | Pixels changed | Pixels it weighs nothing, and the base's | Mask handed back |
|---|---|---|---|---|---|---|---|
| spot | a box 53 17 67 31 | 196 | 760 | 0 | 956 | 5188 of 5188 | on the composite |
| stain | a polygon (12, 36) (44, 36) (28, 60) | 384 | 1032 | 0 | 1416 | 4728 of 4728 | on the composite |

<!-- measured: end -->

## Rung 2 results

Plan m3, task B1, item 1 (`SPEC.md` section 8, "The ladder"): the synthetic edit set of 24
pictures of `source-fidelity/scenes-v1`, each with a defect drawn by a committed program
(`labs/m3-edits/edits.toml`) and the clean picture kept, run as the lab `m3-edits-repair_area`
on the owner's Mac on 2026-11-19, filled by Bria Eraser through fal, and scored by
`whizbang eval edits score`. The figures are the record's (`docs/results/m3-edits.md`),
aggregates only, of the shipped recipe and its constant sets as named:

```toml
recipe = "repair_area"
recipe_digest = "sha256:c973be2dc1d0f6b6d92e2b519ecd8d75fc625d79f85ed5d10a92c16ee4d56342"
constant_sets = "none"
lab = "m3-edits-repair_area"
protocol_digest = "sha256:dac24ed17d355417f36550f861149078939aa5837395a0963d353858c1b552e3"
filler = "fal bria-eraser 1+fit1-png-binary1-2048"
finder = "none"
items = "24"
completed = "24"
failed = "0"
not_run = "0"
changed_outside_mask = "0"
items_preserved = "24"
mask_iou_mean = "0.5904"
mask_iou_min = "0.5115"
coverage_mean = "1.0000"
coverage_min = "1.0000"
size_ratio_mean = "1.7004"
fill_error_mean = "27.1919"
blur_error_mean = "30.9960"
fill_beats_blur = "14"
settled_micros = "960000"
unresolved_micros = "0"
```

- **Preservation.** 0 pixels changed where the mask weighs nothing, on 24 of 24 items.
- **Task success.** The mask's IoU with the drawn defect is 0.590 on average (0.511 at
  the least), and its coverage of the defect 1.000. IoU understates the mask: the drawn box is larger than the spot and the feather of sigma 3 reaches past it, so the mask is on average 1.70 times the spot and covers all of it.
- **Fill error.** Inside the drawn defect the fill is 27.2 levels from the clean picture
  on average, per channel of 0 to 255, against 31.0 for a plain blur of the same mask; the fill
  was nearer than the blur on 14 of the 24 items.
- **Visual quality.** Its lab's round was run and its pairs drawn, and nobody picked them:
  not yet confirmed by a person; owed by milestone E (the owner's decision of 2026-11-19)

## Speed and cost

With Bria through fal a run is the fill's call: 0.04 dollars a fill, 960000 millionths of a
dollar settled for the 24 items above. Beside it, the steps' own work, timed below as a
whole run through the executor under the scripted filler, from a fresh store each time:

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | A whole run of `repair_area` through the executor |
|---|---|---|
| 1024x768 | 0.8 | 357 |
| 2048x1536 | 3.1 | 655 |
| 4096x3072 | 12.6 | 1709 |

<!-- timed: end -->

## Alternatives

- **`remove`**: grows the location, for a thing with a shadow.
- **`mask_rect` and `fill`**: the box with a hard edge.
- **A plain blur of the same mask**: the baseline of the fill error above; the fill was
  nearer the clean picture on 14 items of 24.

## Limits

| Limit | Example |
|---|---|
| The location is filled as drawn: one drawn too small leaves the defect's edge | The plan's set draws each box around its spot |
| Each run spends one fill | `docs/results/m3-edits.md` |
| Rung 2 is one synthetic set: 24 pictures of one published set, one kind of defect each | `labs/m3-edits/edits.toml` |

## Mark and history

**Mark.** Experimental: rung 1 is shown, and rung 2 is shown in part: its visual quality is
owed by milestone E, by the owner's decision of 2026-11-19 (`docs/carry-over.md`, the row on
the blinded rounds of milestone 3; `SPEC.md` section 8, "Marks").

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests under a scripted filler |
| 2. Scored alone | In part | Task success and preservation, `docs/results/m3-edits.md`; visual quality owed by milestone E |
| 3 and 4 | Not yet | Milestone E |

**Gates.** None faced.

**History.** Ported from rascal's recipe at the commit `docs/sources.toml` pins; made a
recipe of this build's actions ending in `fill` by plan m3, task O1. Plan m3, task B1:
rung 2 of the edits and this record.
