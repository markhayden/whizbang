# per_subject

The tool record of per-subject comparison (`SPEC.md` sections 7 and 8; plan
`docs/plans/m2c-milestone-2.md`, task X1, item 7, its record made by task F1). It is a tool
file, `pack/tools/per_subject.py`, run by the tool worker (ADR 0032), which asks whizbang's
segmenter for each subject through its context. The examples' readings below are SAM 3's on
the owner's Mac; the model tier holds the tool to them
(`model_tier::per_subject_comparison_answers_the_records_examples_on_the_mac`, under `just
test-models`), and the fast tests hold the rung 2 numbers to the tree's tool and adapter
(`evidence::the_per_subject_record_names_a_commit_of_this_tree_and_its_rung_2_numbers`).

## What it does

Finds each named subject in the source and in the candidate, and reports, for each, whether
it is there in each, its area in each, how far its centre moved, and how far its two masks
disagree: one minus their IoU, 0 when both are empty and 1 when only one is. The item's value
is the largest of its subjects' (its declared rule, `worst`; larger is changed).

- **Use it** on a source and an edit of it that must keep named things where they were: a
  restyle that keeps the pool, the shed, the chair.
- **Do not use it** between pictures of different scenes or viewpoints: the candidate is first
  registered onto the source by `align`, and a pair that does not register gives no reading.
- **Kind.** An evidence tool (`SPEC.md` section 8): the upper rungs apply to it. Its subjects
  are found by a model, SAM 3, so its readings are the model's.

**How subjects are named.** By name, each a phrase (`phrases`, separated by semicolons); by
location, each a point (`points`, as `X Y`) or a box (`boxes`, as left top right bottom) in
pixels of the source, scaled to the candidate; or from the item's job (`job`, which `whizbang
eval tool` gives it): each requirement of kind `must_hold` whose text is "The ... keeps ...",
the phrase before " keeps ", its leading article dropped. A requirement not of that form is
reported as not applicable, by its id. The item's label is never read.

**The method.** For each subject, the segmenter is asked once on the source and once on the
candidate as `align` registered it onto the source (at most four instances each); the
subject's mask in each is the union of its instances, the candidate's brought to the source's
size by its nearest pixel; the two masks are compared pixel for pixel. At most 16 subjects an
item. The standard library only.

**Outputs.** `one_minus_iou`, the item's value, in the run's diagnostic `tool_file.json`,
with every subject's six readings beside it (`one_minus_iou`, `present_source`,
`present_candidate`, `area_source`, `area_candidate`, `shift`), each held to its declared
bounds before anything uses it. No image.

## Examples

Both compare the chair of the parity fixtures (`fixtures/parity/segment/pictures/chair.png`,
rascal's `chair-blob.png`: a chair and its shadow on a floor of boards, 480 by 320, drawn by a
program) with an edit of it, asked for the subject `chair` by name and for the point
(158, 110), on the chair's back, by location. Each edit was made by a few lines of Python with
Pillow, kept here as pictures: the first paints the chair and its shadow out with boards
copied from the top of the picture (each row from the row 60 above it, over 110 to 345 across
and 60 to 285 down); the second tones every pixel warmer and darker (red at 0.9 times itself
plus 20, green at 0.75 plus 10, blue at 0.6 plus 30).

| The source | The chair painted out | Toned |
|---|---|---|
| ![The source](per_subject/source.png) | ![The chair painted out](per_subject/removed.png) | ![The source toned warmer](per_subject/toned.png) |

What to look for. Painted out, the chair is found in the source and nowhere in the candidate:
its masks disagree wholly, and the item's value is 1. The point does not say so: on the
candidate it lands on the boards, and SAM 3 answers the boards, most of the picture, so the
point's masks disagree by 0.94 for another reason. Toned, the chair is found in both, of the
same area and in the same place: one minus the IoU is under 0.01.

What SAM 3 read, on the Mac's GPU (areas are of the source's pixels; the shift is a share of
its diagonal):

<!-- read: begin -->

| Example | Subject | One minus IoU | In the source | In the candidate | Area, source | Area, candidate | Shift |
|---|---|---|---|---|---|---|---|
| removed | chair | 1.000 | yes | no | 0.052 | 0.000 | 0.000 |
| removed | point 158 110 | 0.937 | yes | yes | 0.052 | 0.828 | 0.150 |
| toned | chair | 0.005 | yes | yes | 0.052 | 0.052 | 0.000 |
| toned | point 158 110 | 0.005 | yes | yes | 0.052 | 0.052 | 0.000 |

<!-- read: end -->

The item's value is 1.000 for the first and 0.005 for the second.

## Rung 2 results

Being run: the rung 2 numbers of this record are taken by `whizbang eval tool per_subject` on
`source-fidelity/scenes-v1` and `source-fidelity/adversarial-v1`, on the Mac, on a commit
whose tree holds this tool and this adapter.

## Speed and cost

It spends nothing: SAM 3 runs on the machine, and the comparison is arithmetic in the tool
worker. Each subject is two requests of the segmenter.

## Alternatives

None run. A subject named by location takes the first instance the model gives at that place,
which on a candidate that lost the subject is whatever is there now (the first example); a
phrase is the stronger way to name a subject that may be gone.

## Limits

- **A pair must register.** The candidate is compared after `align`; a mirrored, rotated or
  recomposed candidate does not register, and its item has no reading (an error, never a
  pass or a fail).
- **Names are the model's.** A subject is what SAM 3 answers for its phrase: a phrase it
  misreads, or a second thing of the same name, moves the reading.
- **A point always finds something.** On a candidate that lost its subject, a point answers
  whatever is now under it (the first example).
- **Every instance of a phrase counts.** The mask is the union of at most four instances, so
  two chairs are one subject.
- **Masks are compared in the source's grid.** The candidate's mask is brought there by its
  nearest pixel after `align`, so a misregistration shows as disagreement.

## Mark and history

**Experimental**: new in milestone 2, with its record and rungs 1 and 2 (`SPEC.md` section
7: per-subject comparison is new and experimental; it climbs rungs 3 and 4 in milestone E).
It runs only when a run names it.

- 2026-11-18, plan m2c task X1: written as a tool file, run by the tool worker, scored by
  `whizbang eval tool` against the fake models.
- 2026-11-19, plan m2c task F1: its record, with two worked examples (rung 1) and rung 2 on
  `source-fidelity/scenes-v1` and `source-fidelity/adversarial-v1`, on SAM 3 on the Mac. The
  run found that SAM 3's mask of a phrase can reach past the box it gives; the adapter `sam3`
  now gives the bounds of the mask as the instance's box.
