# seam

The tool record of the task `seam` (`SPEC.md` sections 7 and 8; plan m3, task B1, item 2).
The example images and the two generated parts are written by `scripts/tool-record.sh
seam`; the fast tests fail when they stop being what the recipe gives
(`crates/whizbang-rascal/tests/it/tool_records/recipes.rs`). The examples run under a
scripted filler: a test makes no live call.

## What it does

Heals the wrap edge of a complete panorama whose left and right edges do not meet: `cut`
makes a canvas of the last and first strips side by side, its middle column the wrap edge;
`line` names that column; `fan` measures the band along it under `fan-v3`; `profile` makes
the band's mask under `profile-v1`; `ramp` prescribes the tone step under `ramp-v1`;
`apply` performs it on the canvas and rebinds the mask onto the result; `fill` paints that
through the mask; and `put` writes the canvas back into the panorama. Shipped in
`crates/whizbang-rascal/recipes/seam.toml`, each step a run of its own under the recipe's.

- **Use it** on a complete equirectangular panorama whose wrap edge shows a step.
- **Do not use it** on a flat picture, which is refused, or on a part of a panorama.
- **Kind.** A model-based task: all four rungs apply. It spends what its filler charges.

The recipe `seam`, version 1.0.0:

```
whizbang rascal seam --image <identity> [--backend <name>] --store <dir>
```

| Name | Meaning |
|---|---|
| `image` (input) | The panorama: equirectangular, its topology a wrap |
| `backend` | Optional. The filler, by its name in `[backends.fill]` |
| `set`, `until` | What every recipe takes: overrides of a step's arguments, and a step to stop after |

**Output:** at `image`, the panorama with the band healed: every pixel outside the cut's two
strips the original's. At `mask`, the seam's mask on the cut's canvas, with its tone ramp.

## Examples

Panoramas of 256 by 128, three times their size; the cut takes 18 columns at each edge, so
the canvas is 36 wide. The scripted filler paints brick red wherever the support is.

### A tone that does not meet

| Panorama | The seam's mask, on the canvas | Healed |
|---|---|---|
| ![A tone brightening to the right](seam/step.png) | ![The band](seam/step-mask.png) | ![Healed at both edges](seam/step-out.png) |

### The same, with light stripes across it

| Panorama | The seam's mask, on the canvas | Healed |
|---|---|---|
| ![Stripes across the tone](seam/striped.png) | ![A narrower band](seam/striped-mask.png) | ![Healed at both edges](seam/striped-out.png) |

What to look for: the fill lands only at the two edges of the panorama, which are one seam;
where the stripes cross it, `fan` finds detail and the band narrows, so less is repainted.

<!-- measured: begin -->

| Example | Given | Core, on the canvas | Blend | Protected | Pixels of the panorama changed | Changed outside the two strips | Mask handed back |
|---|---|---|---|---|---|---|---|
| step | a panorama of 256 by 128 | 1024 | 1536 | 0 | 2560 | 0 | on the canvas, with its tone ramp |
| striped | a panorama of 256 by 128 | 426 | 746 | 0 | 1172 | 0 | on the canvas, with its tone ramp |

<!-- measured: end -->

## Rung 2 results

Plan m3, task B1, item 1: `seam`'s rung 2 is scored on the shipped recipe by task L3's
round, the lab `m3-seam` (`labs/m3-seam/`), on the pictures of rascal's `seam-03` corpus:
the shipped `seam` with `fan-v3` (`seam-bria`) against `fan-thin-v1` (`thin-bria`), a band
of fixed width, both filled by Bria Eraser through fal. Run on the owner's Mac on
2026-11-19 at commit `8957f1f`, interrupted once on purpose and resumed: 94 of 94 cells
completed, 3.76 dollars settled, and 0.04 unresolved, the one fill the interruption cut
off. Of the shipped recipe and its constant sets:

```toml
recipe = "seam"
recipe_digest = "sha256:513ed50833f9a5ff283f80a7bbb511c07ae73b60caad2c7477cd1eb07f91c315"
constant_sets = "fan-v3 sha256:02c719544d9635f5e2158ce4c9eac287497c57e4f04dcb690393276b38f5b6dd, profile-v1 sha256:25e8315cee9a205356f1409fd6894694a681f1e986583bcf6edbb10f462be68d, ramp-v1 sha256:943849ea1518138228f5549ed3cbd22a1fc782f97daa475999e98bf50fcaf794"
lab = "m3-seam"
cells = "94"
completed = "94"
settled_micros = "3760000"
unresolved_micros = "40000"
```

- **Preservation.** Every pixel outside the cut's strips is the panorama's, by the recipe's
  construction (`put`), held on every example above and by
  `task::seam_fills_the_wrap_edge_and_keeps_every_other_pixel`.
- **Task success and visual quality.** The round's decode against the fixed-width band, by
  `seam-03`'s groups and rules: round pending: the owner's picks
- **Earlier evidence, not this recipe's.** Rascal's `seam-03` (`fan-v1`, its default when it
  ran) and `objects-02` (`fan-v3` against `fan-v1`) were rounds of rascal's own recipe on the
  sets they ran; they are cited beside the decode in `docs/results/m3-lab.md`, never as the
  shipped recipe's.

## Speed and cost

With Bria through fal a run is one fill's call, 0.04 dollars: 3.76 dollars for the round's 94
cells. Beside it, the steps' own work, timed below as a whole run through the executor
under the scripted filler, from a fresh store each time:

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | A whole run of `seam` through the executor |
|---|---|---|
| 1024x512 | 0.5 | 662 |
| 2048x1024 | 2.1 | 768 |
| 4096x2048 | 8.4 | 1106 |

<!-- timed: end -->

## Alternatives

- **`fan-thin-v1`**: a band of fixed width in place of `fan-v3`'s measured one; the round
  above compares the two.
- **`--until apply`**: the tone step performed on the canvas and no fill: the seam's
  colours meet, its detail does not.

## Limits

| Limit | Example |
|---|---|
| The band is measured on the cut's canvas: on a panorama far under 1,620 pixels wide `fan` reads one column a side | The examples here: 36 columns |
| Only the two strips change; a step that runs further into the picture stays | The examples |
| Each run spends one fill | The round above |

## Mark and history

**Mark.** Experimental: rung 1 is shown, and rung 2 is shown in part, its decode waiting on
the owner's picks (`SPEC.md` section 8, "Marks").

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests under a scripted filler |
| 2. Scored alone | In part | The round of `m3-seam`, its decode pending |
| 3 and 4 | Not yet | Milestone E |

**Gates.** None faced.

**History.** Ported from rascal's `seam` and the graph of its `heal` at the commit
`docs/sources.toml` pins, as one recipe that heals and puts back, by plan m3, task O3. Plan
m3, task L3: its round. Plan m3, task B1: this record.
