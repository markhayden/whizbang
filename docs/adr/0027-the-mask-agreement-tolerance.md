# 0027. The mask-agreement tolerance

Status: accepted.
Date: 2026-11-18
Spec: section 16, row 1D; section 18, item 3; plan 1D, task V1, items 1 and 2

## Context

`SPEC.md` section 16, row 1D, asks for parity fixtures and a mask-agreement tolerance to be
recorded, and section 18, item 3, makes that tolerance the bar a remote worker meets in
milestone 6: its answer to a fixture must agree with the local one. The plan (task V1,
item 2) says how agreement is read and that the tolerance is measured, not chosen: each
fixture asked three times on the Mac's GPU and three times on its CPU, the tolerance the
worst agreement found, rounded down to 0.01, and a stop if that is under 0.90.

What a segmenter answers is a list of instances for each query, each a mask, a score and
a box. A model on another device computes in another order, so its masks may differ at
the edges and its scores in the last digits; and an instance near a tie may change its
place in the order.

## Decision

**Agreement** (`whizbang::parity`). Two answers to one request agree when each query
has the same count of instances in both and, paired in the order each answer gives
them, each pair's masks overlap by an IoU of at least the tolerance and their scores
differ by at most the margin. An answer one instance short does not agree
(`parity::an_answer_one_instance_short_does_not_agree`); two instances in the other order
do not agree (`parity::agreement_is_the_count_and_each_pairs_iou_and_score`). The box and
the predicted IoU are not compared: the box follows from the mask, and the predicted IoU
is a score where the model gives one.

**The fixtures** (`fixtures/parity/segment/`): SAM 3 and Tiny, each asked a phrase, a
point and a box in one request about three pictures (the model tier's `circle.png`;
`shapes.png`, drawn by a program for these fixtures; rascal's `chair-blob.png`, a chair
rascal drew by a program, ported as `chair.png` from `fixtures/images/chair-blob.png` at
the commit `docs/sources.toml` pins), at most three instances a query, and what the GPU
answered: each instance's score, box, predicted IoU and mask as a PNG plane, with the
model's revision in the catalogue and the adapter's in the binary's table
(`parity::the_committed_fixtures_name_the_revisions_the_catalogue_holds`).

**The measurement**, on the owner's Mac on 2026-11-18, by
`parity::sam3_and_tiny_answer_each_fixture_inside_the_tolerance_on_mps_and_on_cpu` under
`scripts/test-models.sh parity` (the CPU by the setting `device = "cpu"`, each run's
record naming the device): 36 answers of each model, every one with the fixture's count
of instances for every query.

| Model, device | Least IoU of a pair | Widest difference of scores |
|---|---|---|
| SAM 3, GPU (`mps`) | 1.000000 | 0.000000 |
| SAM 3, CPU | 1.000000 | 0.000016 |
| Tiny, GPU (`mps`) | 1.000000 | 0.000000 |
| Tiny, CPU | 0.999945 | 0.000005 |

**The tolerance is 0.99** (`IOU_TOLERANCE`): 0.999945 rounded down to 0.01, over the
plan's floor of 0.90. **The margin is 0.01** (`SCORE_MARGIN`): 0.000016 rounded up to
0.01, so that a difference in the last digits of a score is inside it and a score
another model would give is not.

## Consequences

- A remote worker in milestone 6 agrees with these fixtures at an IoU of 0.99 or better
  for each pair and its scores within 0.01, with the same counts: a strict bar, since the
  two devices of one Mac differed by at most a handful of pixels.
- A change of a model's files, its catalogue revision or its adapter's revision makes the
  fixtures stale: the fast test fails, and the model tier's test writes them again
  (`model-tier/out/parity/`) to be committed with that change, and the measurement is
  taken again.
- The pairs are taken in order. An answer whose two instances score within the margin
  of each other may swap them on another device and not agree, though each mask is
  there; none of the fixtures did, and the closest were two candidates of SAM 3 for the
  ring's box at 0.921 and 0.915.
- The measurement is one machine's two devices. It says nothing of another Mac's GPU or
  of Linux, which has no runtime (ADR 0021).

## Alternatives

- **Pair by best overlap, not by order.** Kinder to a swapped tie, but it hides an
  answer whose order changed, and the order is part of what `segment` hands on (the
  first mask is the best). Turned down: the plan reads agreement in order.
- **A tolerance chosen by hand (0.90 or 0.95).** Turned down by the plan: the bar is what
  was measured, and the measurement left a great deal of room above 0.90.
