# plug

The tool record of `plug` (`SPEC.md` section 8; plan m3, task D1). The example images and
the two generated parts are written by `scripts/tool-record.sh plug`; the fast tests fail
when they stop being what the tool gives (`crates/whizbang-rascal/tests/it/tool_records/shaping.rs`).

## What it does

Fills the small holes in a mask's core: each region outside the core that touches no edge
of the image and is at most a share of the area of the shape around it.

- **Use it** after `grow` and `close`, to fill the pinholes a segmenter leaves. `remove`
  and `remove_prompt` call it with a share of 0.1.
- **Do not use it** to fill a hole that must stay, such as the opening of a ring or a
  frame, unless the share keeps it (a frame's hole is larger than the frame). Nor on a
  feathered mask whose feather you want: it clears the blend.
- **Kind.** A pure pixel operation: rungs 1 and 2 apply. No model, no spend.

The action `plug`, version 1.0.0:

```
whizbang rascal plug --mask <identity> [--max-share 0.1] --store <dir>
```

| Name | Meaning |
|---|---|
| `mask` (input) | The mask whose holes are filled |
| `max_share` | The largest hole filled, as a share of its owner's area, from 0 to 1; 0.1 when not given |

**Output.** One mask with the same binding, use, source and protect. Its core is plugged,
and its blend, profile and prescriptions are cleared (`Mask::replace_core`). Its origin
ends with a step naming `plug`, its version, `max_share` and the mask it was made from. A
progress event says how many holes were found and filled.

**The rule.** A hole is a region outside the core joined through its four sides that
touches no edge of the image. A blob is a region of the core joined through sides and
corners. A hole's owner is the largest blob beside it, by sides or corners; of two blobs
as large, the one whose first pixel, row by row, comes first. A hole is filled when its
area is at most `max_share` of its owner's, compared exactly in whole numbers. Every region
is taken from the core given, so the order of filling changes nothing. A protected pixel of
a hole stays out.

## Examples

Masks of 96 by 64, three times their size, with the core white. `max_share` 0.1.

| Given | Made |
|---|---|
| ![A disc with a small and a large hole](plug/holes.png) | ![The small hole filled](plug/holes-made.png) |
| ![A square with a bay and a corner pocket](plug/openings.png) | ![The pocket filled, the bay open](plug/openings-made.png) |

What to look for:

- First: of the disc's two holes, the one of 16 pixels is filled and the one of 196 is
  not, being over a tenth of the disc.
- Second: the bay on the square's left side is open along a side, so it is no hole and
  stays. The small pocket below it reaches the outside only across a corner, so it counts
  as a hole and is filled. Through a corner, one pixel wide, is enough to close it.

### The inputs, in a store

`whizbang store record-inputs --store <dir>` writes the inputs of these examples into a store
that is there, as canonical artifacts made by the constructors this record's tests use, each
with what it depends on, and prints each one's identity (`crates/whizbang/src/record_inputs.rs`).
A walkthrough gives the same inputs to the command line and to the playground. They are:

<!-- inputs: begin -->

| Example | Input | Type | Identity |
|---|---|---|---|
| holes | mask | mask | `sha256:e533fcd80866680856d5729ff9682fe582ad04948aa56e88e7d75a089b1b1d12` |
| holes | image of mask | image | `sha256:5b673f00862bdb77dc9805e663c997175b329f97a6a31eb55469d099fef74f33` |
| openings | mask | mask | `sha256:dce2baf610015b7161148d3d92a22bae09569f1f35c2c1ad0c8a88a3bdcda64f` |
| openings | image of mask | image | `sha256:5b673f00862bdb77dc9805e663c997175b329f97a6a31eb55469d099fef74f33` |

<!-- inputs: end -->

## Rung 2 results

Exact against reference results (`SPEC.md` section 8). Measured on 2026-11-19.
`plug::the_ported_vectors_give_rascals_cores` holds rascal's own cases
(`crates/rascal-ops/tests/it/plug.rs` at the pinned commit) on its picture of 80 by 80:

- a small hole in a ring is filled, to 1,600;
- a frame keeps its large hole, even at a share of 1;
- a hole touching the edge is no hole;
- a pocket open across a corner is a hole, and one open along a side is not;
- merged blobs with several holes give rascal's reference, written apart from the code, at
  the shares 0, 0.05, 0.1, 0.3 and 1;
- a protected hole stays empty.

`plug::a_hole_of_exactly_its_share_is_filled` holds the exact comparison on the bound. No
pixel differs.

<!-- measured: begin -->

| Example | Asked | Masks given | Core before | Core after | Blend after | Protected after |
|---|---|---|---|---|---|---|
| holes | `max_share` 0.1 | 1 | 1305 | 1321 | 0 | 0 |
| openings | `max_share` 0.1 | 1 | 1274 | 1283 | 0 | 0 |

<!-- measured: end -->

## Speed and cost

`plug` spends nothing. It was timed on `grow`'s mask of the times. It labels every region of
the core and of what is outside it, so it costs more than the sweeps of `grow`.

<!-- timed: begin -->

Measured on Apple M4 Max (macos, aarch64), in a release build; each time is the middle one of five runs, in milliseconds.

| Size | Megapixels | `plug`, `max_share` 0.1 |
|---|---|---|
| 1024x768 | 0.8 | 14 |
| 4096x3072 | 12.6 | 231 |
| 8000x6000 | 48.0 | 923 |

<!-- timed: end -->

## Alternatives

Rascal's comparison of the share was in doubles, as a product rounds. Here it is in whole
numbers (`plug::at_most_share`), so a hole on the bound is judged the same everywhere. The
reference of rung 2 keeps rascal's doubles, and agrees on every share it tries.

## Limits

| Limit | Example |
|---|---|
| A pocket open only across a corner is filled | The second example |
| A hole is judged against its owner's area, not the whole mask's | `plug::the_ported_vectors_give_rascals_cores`, the small blob |
| The blend, the profile and the prescriptions are lost | `plug::a_hole_up_to_its_share_of_its_owner_is_filled_and_a_larger_one_is_not` |
| A hole at the image's edge is never filled | `plug::a_hole_touching_the_edge_is_not_filled` |

## Mark and history

**Mark.** None: a pure pixel operation, complete when its record shows rungs 1 and 2, and
this one does.

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests |
| 2. Scored alone | Shown | Exact against rascal's vectors and reference; the times at three sizes |
| 3 and 4 | Do not apply | `SPEC.md` section 8 |

**Gates.** None faced.

**History.** Plan 2A, task R2: ported from rascal's `plug.rs` at the pinned commit, with
the share compared exactly. Plan m3, task D1: this record.
