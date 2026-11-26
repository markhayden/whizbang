# protect

The tool record of the task `protect` (`SPEC.md` sections 7 and 8; plan m3, task D1). The
example images and the two generated parts are written by `scripts/tool-record.sh
protect`; the fast tests fail when they stop being what the recipe gives
(`crates/whizbang-rascal/tests/it/tool_records/tasks.rs`).

## What it does

Makes the mask for a second pass: everything the first pass's mask reached, its core, its
blend and its protect, is protected, and every other pixel of the image is the core. A
recipe of one step, `invert`, shipped in `crates/whizbang-rascal/recipes/protect.toml`.

- **Use it** after a fill, to edit the rest of its composite while keeping the first edit:
  give it the composite and the mask the fill rebound onto it.
- **Do not use it** with the first mask as it was before the fill: that mask is on the
  original picture, not on the composite, and is refused.
- **Kind.** A task that uses no model: rungs 1 and 2 apply. It spends nothing.

The recipe `protect`, version 1.0.0:

```
whizbang rascal protect --image <composite> --mask <rebound mask> --store <dir>
```

| Name | Meaning |
|---|---|
| `image` (input) | The picture of the second pass: a fill's composite |
| `mask` (input) | The first pass's mask, on that picture |
| `set`, `until` | What every recipe takes |

A mask that reached every pixel leaves nothing to edit and is refused. **Output:** one mask
at `mask`, on the picture, its use `protect`, with no blend.

## Examples

The first pass runs under a scripted filler that paints brick red wherever the support is.
Masks and pictures of 96 by 64, three times their size; a mask's core white, protected
pixels red, blend in blues, the rest dark.

| First pass | Its mask | Its composite | `protect` |
|---|---|---|---|
| `mask_rect` of `24 16 56 44`, then `fill` | ![A hard block](protect/block-first.png) | ![The block painted](protect/block-composite.png) | ![The block protected, the rest the core](protect/block-protect.png) |
| `remove` of the same box | ![The block grown, closed and feathered](protect/feathered-first.png) | ![A larger soft patch painted](protect/feathered-composite.png) | ![A larger area protected](protect/feathered-protect.png) |

What to look for: after the hard block, protect is the block's 896 pixels and the core the
other 5,248. After `remove`, whose mask was grown, extended 12 rows down and feathered,
protect is 3,944 pixels: the 1,888 of the core and all 2,056 of the blend, to its faintest
pixel, so the second pass cannot touch a wide soft ring the eye hardly sees as edited.

### The inputs, in a store

`whizbang store record-inputs --store <dir>` writes the inputs of these examples into a store
that is there, as canonical artifacts made by the constructors this record's tests use, each
with what it depends on, and prints each one's identity (`crates/whizbang/src/record_inputs.rs`).
A walkthrough gives the same inputs to the command line and to the playground. `protect` itself
is given a first pass's composite and mask, which only a fill makes: what is written is what
that first pass takes, the scene and, for `remove`, the box. They are:

<!-- inputs: begin -->

| Example | Input | Type | Identity |
|---|---|---|---|
| block | image of the first pass | image | `sha256:5b673f00862bdb77dc9805e663c997175b329f97a6a31eb55469d099fef74f33` |
| feathered | image of the first pass | image | `sha256:5b673f00862bdb77dc9805e663c997175b329f97a6a31eb55469d099fef74f33` |
| feathered | location of the first pass | location | `sha256:f91d20c2928483d8def20ab172334c5c6eeff87186242364516b354ec13f1a82` |

<!-- inputs: end -->

## Rung 2 results

Exact against reference results (`SPEC.md` section 8). Measured on 2026-11-19. The task's
acceptance case, `task::protect_inverts_an_earlier_mask_onto_its_composite`, runs `remove`
and then the shipped recipe through the executor, pixel by pixel, and refuses the first
mask unbound; its step is exact against rascal's vectors by
`invert::the_ported_vectors_give_rascals_masks` (rascal's `protect.toml` at the pinned
commit is the recipe ported). No pixel differs.

<!-- measured: begin -->

| Example | First pass | Its core | Its blend | Core after | Blend after | Protected after | Use case |
|---|---|---|---|---|---|---|---|
| block | `mask_rect`, then `fill` | 896 | 0 | 5248 | 0 | 896 | protect |
| feathered | `remove` | 1888 | 2056 | 2200 | 0 | 3944 | protect |

<!-- measured: end -->

## Speed and cost

`protect` spends nothing. A whole run through the executor, from a fresh store each time,
the first mask `grow`'s mask of the times: the picture and the mask are read from their
files, and the new mask is written.

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | A whole run of `protect` through the executor |
|---|---|---|
| 1024x768 | 0.8 | 175 |
| 2048x1536 | 3.1 | 282 |
| 4096x3072 | 12.6 | 891 |

<!-- timed: end -->

## Alternatives

Rascal's recipe carried the mask up its chain of frames with `to = "raw"`; here a mask is on
one image, and carrying one from a canvas or a window is task O2's.

## Limits

| Limit | Example |
|---|---|
| The first mask's whole blend is protected, the faintest pixel too | The second example |
| The mask must be on the picture given: after a fill, the rebound one | `task::protect_inverts_an_earlier_mask_onto_its_composite` |
| A first mask that reached every pixel is refused | `invert::the_ported_vectors_give_rascals_masks` |
| The new mask has a hard edge against what is protected | Both examples |

## Mark and history

**Mark.** None: a task that uses no model, complete when its record shows rungs 1 and 2,
and this one does.

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests |
| 2. Scored alone | Shown | Its step exact against rascal's vectors; the times at three sizes |
| 3 and 4 | Do not apply | It uses no model |

**Gates.** None faced.

**History.** Plan m3, task O1: ported from rascal's `protect.toml` at the pinned commit, on
`invert`. Plan m3, task D1: this record.
