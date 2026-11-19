# 0029. The details handed to the judge first

Status: accepted.
Date: 2026-11-18
Spec: section 3, "What a judge model may call"; plan m2c (`docs/plans/m2c-milestone-2.md`),
task A2, items 4 and 5
Amended: 2026-11-18, by task X1 (item 9). `detail` also names `change_map` and
`edge_similarity`, experimental, each measured on the source and the frame `align` returned
for the pair; a run that names either runs `align` first, named or not, and a pair that does
not register says so in place of their measurements. Each is said as `align` is: the tool,
its mark, squint's sentence, every measurement, and, for the change map, the map as a
picture the judge may name (`map:<image>:<against>`), with squint's guidance on what the
number does not say. Either may also be offered to the judge by the run's `tools`, and a
call of it is answered as squint's harness answered it. Neither runs unless a run names it.

## Context

The owner's goal for squint: it should "provide all the details and the model to make the
final conclusion but it shouldn't just be a bunch of shit it has to figure out". Squint's
baseline arm C did the opposite: the judge was told of `view` and `align` and left to call
them, and `align` answered under the projection `visual_only`, its picture and a fixed
sentence, with every measurement withheld. That arm is what the baseline gate replays, so it
must stay reachable exactly.

## Decision

`check` takes `details`: `none`, squint's arm C as it was (the judge calls the tools), or
`first`, the default of `whizbang squint check`. Under `first`, before the first turn, for
each pair of images the use case relates (the candidate registered onto the source, and
onto each reference, in the job's order), `check` calls `align` through the executor, as a step of the run, and puts
in the opening message, after the images and before the checklist:

- one line that says these are measurements made before the judge looked, each labelled
  with its tool and the tool's mark, never proof, to be weighed against what it sees;
- for each call, the tool and its mark (`align (unverified)`), the pair, the status, squint's
  own sentence (which says, of a registration, that it matches the camera and not the
  content) and every measurement with its unit;
- the aligned frame, named as squint named it (`aligned:candidate:t1`), and its picture.

A pair that does not register is said the same way, with no picture: not registered is
itself evidence that the geometry changed. A use case with one image has no pair, and
`first` adds nothing. The measurements are handed whole: these are the details the owner
asked for. `detail` names the tools measured first; it names `align` alone until X1 ports the
tools that compare content.

`check` also takes `projection`, `full` or `visual_only`, for what a call of `align` by the
judge is answered with: squint's presentation of the envelope, whole or projected.
`--details none --projection visual_only` with the tools `view` and `align` is arm C.

## Consequences

Every default `squint check` runs `align` once per pair before it asks the judge: about a
tenth of a second at 128 by 96 pixels in a release build, nothing spent. The judge's first
request carries one more picture. The record of the run lists the `align` step. Squint's
prompt now has its Tools section whenever evidence tools are offered. Reversing is a change
of the command's default.

## Alternatives

- Hand the details only when the judge asks: that is arm C, and the owner's complaint.
- Hand the measurements and withhold the picture: the judge then has numbers it cannot see.
- Present the measurements as conclusions ("the geometry held"): they are not, and squint's
  own manifest says why.
