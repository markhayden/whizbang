# fill

The tool record of `fill` (`SPEC.md` section 8; plan m3, task D1). The example images and the
two generated parts are written by `scripts/tool-record.sh fill`; the fast tests fail when
they stop being what the action gives (`crates/whizbang-rascal/tests/it/tool_records/paste.rs`).
The examples run under a scripted filler: a test makes no live call.

## What it does

Sends an image and a mask's support to the configured filler, pastes the answer by
`paste`'s code, and hands back the composite and the mask rebound onto it.

- **Use it** to repaint what a mask covers: the last step of `remove` and `repair_area`.
- **Do not use it** to judge a filler: what a filler paints is the backend's, and its
  quality is scored on rung 2 by task B1, not here.
- **Kind.** A model-based tool: all four rungs apply. It spends what its backend charges.

The action `fill`, version 1.0.0:

```
whizbang rascal fill --image <identity> --mask <identity> [--backend <name>] --store <dir>
```

| Name | Meaning |
|---|---|
| `image` (input) | The picture to fill; a rendering is refused |
| `mask` (input) | The mask, on the picture: the filler paints where it weighs over 0 |
| `backend` | Optional. The filler, by its name in the config's `[backends.fill]`; the default for fill when not given |

**What is sent** is the picture and the mask's support (`Support::of`: every pixel the mask
weighs over 0, as 255), through the handle the executor hands the run, so its cancellation,
deadline, gate, spend and record are the executor's.

**Output.** At `image`, the composite: every pixel outside the support and every protected
one the original's, whatever the filler answered. At `mask`, the mask with a step of
`fill` in its origin, rebound onto the composite through the derivation the core checked
of the two pictures. A filler that answers with the picture it was sent changes nothing:
the picture and the mask are handed back as they were.

**What the run keeps.** The picture the filler painted, as the diagnostic `fill-answer.png`:
its colours with an opaque alpha, written as a canonical image's PNG, and only when it is
the image's size. The composite cannot be undone into it, and a replay answers the fill
from it (ADR 0048). `whizbang runs show <run> --diagnostic diagnostics/fill-answer.png >
answer.png` saves it.

## Examples

Pictures of 96 by 64, three times their size; a mask's core white, protected pixels red,
blend in blues. The scripted filler (`FakeFiller` of `whizbang-testkit`) paints brick red
wherever the support is, or, in the second, repaints every pixel of the picture, inside the
support and out, as a filler that cannot be trusted might.

### A feathered disc

| Picture | Mask | Composite |
|---|---|---|
| ![The scene](fill/feathered.png) | ![A feathered disc](fill/feathered-mask.png) | ![The disc filled](fill/feathered-out.png) |

What to look for: the filler painted its whole support flat, the faintest blend included;
the composite mixes that by the blend's weight, as `paste` does, and the mask handed back is
on the composite.

### A filler that repaints everything

| Picture | Mask | Composite |
|---|---|---|
| ![The scene](fill/protected.png) | ![A block with a protected stripe](fill/protected-mask.png) | ![Only the block changed](fill/protected-out.png) |

What to look for: the filler inverted the whole picture, and the composite took the answer
only inside the block and off the stripe: 1,904 pixels changed, the other 4,240 the
original's.

## Rung 2 results

Not yet: rung 2 of a model-based tool is the edit's task success, preservation outside the
mask and visual quality (`SPEC.md` section 8, "The ladder"), which task B1 scores for the
fill recipes with Bria through fal. What is exact here is the action's own work, held by
`fill::pixels_outside_the_support_and_protected_ones_are_the_originals`,
`fill::the_mask_is_rebound_onto_the_composite_through_a_checked_derivation`,
`fill::a_fill_answered_with_the_picture_it_was_sent_keeps_the_masks_binding`, and `paste`'s
rung 2.

<!-- measured: begin -->

| Example | Painted by | Pixels the mask weighs over 0 | Pixels changed | Pixels it weighs nothing, and the base's | Mask handed back |
|---|---|---|---|---|---|
| feathered | the scripted filler | 1421 | 1421 | 4723 of 4723 | on the composite |
| protected | the scripted filler | 1904 | 1904 | 4240 of 4240 | on the composite |

<!-- measured: end -->

## Speed and cost

A fill costs the filler's call. The one live call made so far, fal's Bria Eraser on a
synthetic picture of 256 by 192, took 10,135 ms and settled 0.04 US dollars
(`docs/results/m3-fills.md`). The rest is the paste, timed below as `paste`'s record times
it, with a grey answer.

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | `fill`: the answer pasted under the mask | The composite's PNG written |
|---|---|---|---|
| 1024x768 | 0.8 | 3.7 | 17 |
| 4096x3072 | 12.6 | 58 | 269 |
| 8000x6000 | 48.0 | 222 | 1157 |

<!-- timed: end -->

## Alternatives

None compared yet: one filler, Bria Eraser through fal, is wired. Comparing fillers is the
lab's (tasks L1 to L3).

## Limits

| Limit | Example |
|---|---|
| The filler paints the whole support, the faintest blend included; only the paste weighs it | The first example |
| What a filler paints is not checked here, only where it may | The second example |
| A filler that answers with the picture it was sent makes no new image and no rebind | `fill::a_fill_answered_with_the_picture_it_was_sent_keeps_the_masks_binding` |
| Each call spends, up to the call's cap, and needs an approved recipient for the picture | `docs/results/m3-fills.md` |

## Mark and history

**Mark.** Experimental: rung 1 is shown and rung 2 is not, so it cannot yet be unverified
(`SPEC.md` section 8, "Marks").

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests under a scripted filler; one live call, `docs/results/m3-fills.md` |
| 2. Scored alone | Task B1's | Task success, preservation and quality of the fill recipes |
| 3 and 4 | Not yet | |

**Gates.** None faced.

**History.** Plan m3, task F1: the action, its adapter for fal and its live call. Plan m3,
task D1: this record. Plan m7, task B1: the run keeps the painted answer
(`fill::a_fill_run_keeps_the_painted_answer_that_a_feathered_mask_was_composited_from`).
