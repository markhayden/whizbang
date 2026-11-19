# shrink

The tool record of `shrink` (`SPEC.md` section 8; plan m3, task D2). The example images and
the two generated parts are written by `scripts/tool-record.sh shrink`; the fast tests fail
when they stop being what the tool gives
(`crates/whizbang-rascal/tests/it/tool_records/masks.rs`).

## What it does

Shrinks the core of a mask by a disc, the erosion that is `grow`'s opposite. Outside the
image counts as outside the core, so a core against the border shrinks from it too.

- **Use it** to pull a mask back from an edge it overran, before a fill.
- **Do not use it** to keep a feather: the blend is cleared, and the profile and the
  prescriptions are removed. Protect is kept and still wins.
- **Kind.** A pure pixel operation: rungs 1 and 2 apply. No model, no spend.

The action `shrink`, version 1.0.0:

```
whizbang rascal shrink --mask <identity> --px 3 --store <dir>
```

| Name | Meaning |
|---|---|
| `mask` (input) | The mask whose core is shrunk |
| `px` | Required. The disc's radius in pixels of the mask's image, from 0 to 10000 |

**Output.** One mask at `mask` with the shrunk core and one more step in its origin. A
core shrunk to nothing is refused, naming the core's pixels before.

**The rule.** K is the whole part of `px` squared, as `grow` takes it, and R the largest
whole number whose square is at most K. A core pixel stays when no pixel within the disc
of it is outside the core and it is at least R pixels from each edge.

## Examples

Each picture shows a mask of 96 by 64, three times its size: the core in white, protected pixels in red, the blend in blues, lighter where it weighs more, and the rest dark. `px` 3.

| Given | Shrunk |
|---|---|
| ![A feathered disc with a protected patch](shrink/feathered.png) | ![Its core shrunk, the feather gone](shrink/feathered-shrink.png) |
| ![A block against the top and left edges](shrink/cornered.png) | ![The block shrunk from every side](shrink/cornered-shrink.png) |

What to look for: the disc loses three pixels all round and its whole feather (644 blend
pixels, then none), and the protected patch stays; the block against the top and left
edges shrinks from them too, to columns 3 to 26 and rows 3 to 20, where `close` would have
kept its edges at the border.

## Rung 2 results

Exact against reference results (`SPEC.md` section 8). Measured on 2026-11-19.

`shrink::the_ported_vectors_give_rascals_masks` holds rascal's two vectors
(`crates/rascal-ops/tests/it/grow_shrink_close.rs` at the pinned commit: shrinking against
a brute force over the disc, and a shape at the border eroded from it) on rascal's picture
of 120 by 80, to rascal's pixels and to the brute force. No pixel differs. The disc is
`grow`'s, whose own rung 2 is in `grow.md`.

The examples in numbers, the same on every machine:

<!-- measured: begin -->

| Example | Core, blend and protect before | Core, blend and protect after |
|---|---|---|
| feathered | 580, 644, 64 | 341, 0, 64 |
| cornered | 720, 0, 0 | 432, 0, 0 |

<!-- measured: end -->

## Speed and cost

`shrink` spends nothing. The mask of the times is `grow`'s; the radius does not move the time, since the erosion is `grow`'s two sweeps.

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | `shrink`, `px` 3 | `shrink`, `px` 40 |
|---|---|---|---|
| 1024x768 | 0.8 | 6.2 | 4.5 |
| 4096x3072 | 12.6 | 63 | 64 |
| 8000x6000 | 48.0 | 244 | 243 |

<!-- timed: end -->

## Alternatives

The disc stamped from every pixel, the rule as written: the brute force of the rung 2 test, far slower at a large radius (`grow.md`).

## Limits

| Limit | Example |
|---|---|
| A core against the border shrinks from it, unlike `close` | The second example |
| The blend, the profile and the prescriptions are lost | The first example |
| A core shrunk to nothing is refused, not handed back empty | `shrink::the_ported_vectors_give_rascals_masks` |

## Mark and history

**Mark.** None: a pure pixel operation, complete when its record shows rungs 1 and 2, and
this one does.

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests |
| 2. Scored alone | Shown | Exact against rascal's vectors and a brute force; the times at three sizes |
| 3 and 4 | Do not apply | `SPEC.md` section 8 |

**Gates.** None faced.

**History.** Plan m3, task O1: ported from rascal's `shrink.rs` at the pinned commit.
Plan m3, task D2: this record.
