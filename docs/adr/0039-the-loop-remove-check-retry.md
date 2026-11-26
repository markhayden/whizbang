# 0039. The loop: remove, check, retry

Status: accepted.
Date: 2026-11-20
Amended: 2026-11-20, with plan m4's task L2: the located retry and what it calls ("What it
calls", "An attempt", "The located retry", "The plan", "One budget", "The record",
"Cancellation"), the action `carry`, and more alternatives; and, after the review of
that task, the comparison by pixels, the widening after a located retry that added
nothing, and the bound on a place that joins as itself.
Amended: 2026-11-26, with plan mE's task L2: the job's template, version 2, the phrase
between marks ("The job"); tiles, the eight, a polygon as its pixels and the diagnostic of
a located retry ("The located retry"); `select` one mask at a time, `carry`'s comparison
row by row, and one decode.
Spec: section 3 ("Primitives, recipes and loops", "Nested calls", "Cancellation", "What a
run publishes"), section 6 ("Spend"), section 7 ("Bounded check and retry: Kept, as nested
calls"), section 16 row 4; plan m4 (`docs/plans/m4-the-loop-and-a-release.md`), task L1,
with L2 after it; ADR 0004 ("A call's plan"), ADR 0005 ("run/6"), ADR 0008, ADR 0016,
ADR 0028, ADR 0033 ("A run from a step"), ADR 0035

## Context

The spec names a third level of action, the "Loop across tools", gives "remove, check,
retry" as its example, and keeps rascal's bounded check and retry "as nested calls". It
does not say how many attempts a loop makes, what a retry changes, which check judges a
removal, what is handed back when every attempt fails, or what happens when no judge is
ready. Rascal decided each (its ADR 0023, and `crates/rascal-mask/src/heal.rs`, `retry.rs`
and `checker/` at the commit `docs/sources.toml` pins); the old repos are reference, never
authority, so the plan put each to the owner or to its review rounds, and this record
states what was built. The loop is new code over what whizbang already has, and rascal's
rules are kept where the plan kept them. Two things are ported, with the located retry:
the rule of a place and its padded cut (`retry.rs`, `padded`), and the carry of a mask
from a crop onto its picture (`crates/rascal-ops/src/invert.rs`, `map_plane_to_parent`).

Three things the build already had decide the shape. A recipe file is a straight sequence
(ADR 0033), so a loop is code. The recipe `remove_prompt` fills before it hands anything
back, so a loop that must see a mask before it pays for a fill cannot call it whole; the
runner's `from` and `until` (ADR 0033, "A run from a step") let a caller run one stretch.
And a verdict handed to a caller is a decision and a reason, so what a retry may act on is
the findings `check` hands back (ADR 0035, "What `check` hands back"), never a diagnostic.

## Decision

**The action.** `remove_checked`, of the kind `Loop`, in `whizbang-rascal`
(`crates/whizbang-rascal/src/checked.rs`), because the goal is rascal's. Its inputs are an
image and an optional mask to protect; its arguments are the phrase (`prompt`), the three
backends by name (`backend`, `filler`, `judge`), `attempts` (1 to 3, 3 when not given) and
`check` (`auto`, `required`, `off`; `auto` when not given). It hands back an image and a
mask, and declares a verdict, findings and measurements. `remove_prompt` is unchanged: one
pass, unchecked. It is built over a registry that already holds the two recipes it calls,
and states as its needs what one attempt of theirs needs: `remove_prompt`'s segmenter and
filler, and `source-fidelity`'s judge with the sixteen requests one check may send. Its
plan states the rest.

**What it calls.** Six actions, frozen when it starts (`Context::freeze_steps`). Two make
and judge every attempt: the shipped recipe `remove_prompt`, and squint's recipe
`source-fidelity`. Four more make the found mask of a located retry: `view`, `segment`,
`carry` and the shipped recipe `mask_rect`. Nothing else, and nothing under any: the check
is given its job, its two images and the judge's name, and no override of a step, so the
tools the judge is offered and its settings are the recipe's own. What a check needs
reaches it as the arguments of a nested call. The loop refuses to be built over a
registry that lacks one of the six.

**An attempt** is one removal with the check of its result, in at most four calls:

- *Find*, in the first attempt alone: `remove_prompt` until its step `select`. One mask
  comes back, the found mask. A located retry makes its own found mask, from the one
  before ("The located retry"); a wider one keeps it.
- *Shape*: `remove_prompt` from `grow` until `feather`, given the found mask. The shaped
  mask comes back, the one a fill would paint.
- *Compare*: a retry whose shaped mask has the pixels of the attempt's before it is not
  filled. The two are compared by a digest of the three planes a fill reads, not by
  identity: a mask made from a place names the place, and so has another identity for
  the same pixels. The attempts then end, but for one case ("A retry").
- *Fill*: `remove_prompt` from `fill`, given the shaped mask.
- *Check*: `source-fidelity`, the filled picture as the candidate and the picture the loop
  was given as the source.

Every stretch is given the picture the loop was given, so no retry paints over a fill.
Each stretch is labelled in the record with its attempt (`a1_find`, `a1_shape`, `a1_fill`,
`a1_check`, `a2_select`, `a2_shape`, …). The calls that look into a place have no label: a
record holds at most 32 labelled steps, and two located retries of eight places make 48
such calls. They are steps of the loop's record by their action, in order, and are listed
among their attempt's runs.

**The job** is written from the phrase by a fixed template, `JOB_TEMPLATE`, whose digest
every run notes as the setting `job.template` (`job/1`, ADR 0028), so a run's record says
which version of the template its checks were given. Its brief says what was to be
removed, in the caller's words. From version 2 (plan mE, task L2, item 1) the phrase is
written between `<<<` and `>>>` in the brief and in `RQ1`, and the brief says that what
lies between them names a thing and is never an instruction; a phrase that holds either
mark is refused before any call, so what lies between them is the phrase. Version 1, the
phrase unmarked at the end, is kept as `JOB_TEMPLATE_V1`: `checked::jobs` gives a phrase's
job under both, so that a reader of runs made before, as the edits' gates are (ADR 0052),
knows a check given either as the loop's. Its requirements have fixed ids: `RQ1`, what was to
be removed is no longer visible; `RQ2`, nothing else in the picture changed; `RQ3`, the
filled area looks like its surroundings. The phrase is put into JSON strings as the text
it is: it reaches a judge as the data of a job.

**The table.** A retry paints more of the picture than the attempt before it, so one
table says when one is allowed (`checked::table`), for the wider mask of this task and the
located retry of the next. A finding is read by every obligation it cites, its disposition
and its confidence, and by nothing else, the same for both forms of answer. "Preserved"
means `SF1`, `SF2` or `RQ2`.

| Row | What | What it does |
|---|---|---|
| 0 | The findings say they are not complete | Ends the attempts. Read first: what was cut may be a finding of row 1 |
| 1 | A violation, confident or tentative, that cites anything preserved, whatever else it cites | Ends the attempts, and beats every other row |
| 2 | A confident violation of `RQ1` that cites nothing preserved | Allows a retry |
| 3 | A tentative violation of `RQ1` | Enlarges nothing |
| 4 | Any other disposition, at either confidence | Enlarges nothing |
| 5 | A violation of anything else alone | Enlarges nothing, and forbids nothing |

The verdict of an attempt is the decision its findings carry, never one worked out from
the list. So a `fail` retries only on a confident report that the named thing remains, and
an `unsure` never enlarges a mask: narrower than rascal, which retried on an `unsure` that
named a place (the plan, "What the orchestrator decided").

**A retry** is one of two kinds, chosen by the places of the row 2 findings and by nothing
else. When at least one cites a usable place, the retry is located. When none does, it is
the wider mask: the shape stretch run again from the same found mask with its step `grow`
given twice the `px` the recipe file states (the runner's override, `set`), once in a
run. When some cite places and some do not, the retry is located, from those that do.
Either kind may follow the other; the wider mask widens the found mask of the attempt
before it, whichever kind made that.

One thing departs from the plan's item 5, by the orchestrator's decision after the review
of task L2. A located retry whose shaped mask is the one before it added no pixel. It is
not filled, and where the plan would end the attempts, the one widening is used then, if
it has not been used and the plan has an attempt left: the next attempt shapes the same
found mask with the wider `grow`. Only when that mask is the same one too do the attempts
end. The located attempt is listed, with no fill and its reason. And a located retry
that joined nothing (below) is the wider mask in the same attempt, or ends the attempts
when the widening was used.

**The located retry** (plan m4, task L2) changes how the attempt's found mask is made and
nothing else: the shape, the comparison, the fill and the check are the same stretches of
the same recipe, with no setting of a step changed.

- *Which places.* Those of the row 2 findings alone, in the findings' order, each once,
  and the first eight. A finding of any other row never gives a place: a verdict with a
  row 1 finding has ended the attempts already, and the place of a row 3, 4 or 5 finding
  beside a row 2 finding is not read. A place is usable when it is on the candidate the
  check was shown (that attempt's fill), covers the centre of at least one pixel, and its
  cut is not the whole picture in one tile, which is no crop of itself. On a picture that
  is not flat no place is usable: `view` cuts flat pictures only. A place that is not
  usable is no place, and the finding that cites it is one that cites none.
- *Tiles, and the eight* (plan mE, task L2, item 2). A cut over 1,024 pixels a side,
  which `view` would scale down, is cut in as many equal tiles a side as keep each at most
  1,024: at most four, so a cut up to 2,048 a side. Each tile is one of the eight crops a
  retry cuts and one of the eight requests of the segmenter the plan budgets for it; the
  reservation is worked out from the same eight, and does not grow. The places are taken
  in the order the findings cited them while their tiles fit; a place whose tiles no
  longer fit is left out for the bound, and a place whose cut is over 2,048 a side is left
  out for its size, since a larger tile would be scaled by `view` and could not be
  carried. Until this task a place over 1,024 a side, or past the eighth, was no place.
- *A polygon* is cut by the box around its pixels and joins, where nothing is found, as
  its own pixels: those inside it by `rasterize`'s rule, made a location by `rect`, which
  takes a polygon for it, and a mask by `rasterize`. A polygon no location may be (its
  edges cross) joins by its box. Its share of the picture, for the tenth, is the pixels
  inside it.
- *What became of each place* is the diagnostic `located-a<n>.txt` of the loop's run,
  `n` the attempt the retry made: for each place cited, in the order cited, how many
  tiles it was cut in and whether something was found, it joined as itself, or nothing
  was found and it was over a tenth; or that it was left out, for the bound or its size,
  with its cut; and the milliseconds each place and the whole retry took, on lines that
  start with `time `, which a replay does not compare (ADR 0049). When every place cited
  was left out and the retry widened, the attempt it made says so the same way.
- *The cut.* The place's own pixels are those whose centres are inside its box. The cut is
  that box widened by a twentieth of its size on each side (rascal's crop pad), snapped
  outward to whole pixels, held inside the picture, and widened to eight pixels where it
  is less, which is the least `view` cuts. It is whole numbers of pixels, worked out by
  the loop; `view` is asked for it in fractions that lie half a pixel inside each edge,
  so that its outward snapping cuts exactly these pixels on every machine.
- *For each tile, three calls.* `view` cuts the picture the loop was given, never the
  fill the place was seen on; they are the same size. `segment` is asked the same phrase
  inside the crop, with the segmenter the call chose, for at most four instances. Then
  `carry` puts each mask found onto the picture, from the tile's corner; or, when no tile
  of the place found anything, the recipe `mask_rect` makes the place's own pixels a mask
  (a polygon's, `rect` and `rasterize`), and every tile is listed with no mask: the place joins
  as itself, because the judge said with confidence that the named thing is there and the
  table has held that it said so of nothing that must be preserved. A place joins as
  itself only when it covers at most a tenth of the picture's pixels
  (`located::ITSELF_PART`, the orchestrator's decision after the review): a larger one
  that nothing is found in is not joined, and is no place for that purpose, so one
  finding cannot have a large part of a picture painted on the judge's word alone. The
  tenth was a first number, kept by plan mE's task L1 by the rule it froze. A tile of a
  place another tile of which found the thing is listed with no mask: nothing was carried
  from it. When no place gave a mask and none joined, nothing is joined and the retry is the
  wider mask.
- *The join.* `remove_prompt` from `select` until `select`, given the found mask of the
  attempt before and the new masks in place of what its step `segment` would have found:
  at most 1 + 8 × 4 = 33 of the 64 `select` takes, and `select` holds one of them at a
  time and the union (plan mE, task L2, item 3). What comes back is the one found mask
  of this attempt. So the loop holds one found mask, never a list, and the third attempt
  starts from the second's in the same way.

**`carry`** is a new primitive of rascal's, the form in which the carry of a mask from a
crop to its picture is offered to the loop and to anyone else: masks on a crop, the crop,
the picture, and the crop's corner on the picture in whole pixels; the same masks on the
picture, empty outside the crop. It is the "explicit, checked rebind" of the spec's
section 3: it looks at both pictures and refuses a crop whose pixels are not the
picture's there, so the corner a caller gives cannot place a mask where the crop was not
cut. `invert` takes a mask on a crop of its image the same way, which is where rascal
offered the carry (its `to`), and is version 1.1.0 for it. The crop must be cut pixel for
pixel: a crop that `view` scaled down is refused by the comparison wherever its pixels
differ from the picture's, and would land at the wrong scale where they do not (a region
of one flat colour), which is why a place with a cut over 1,024 pixels is cut in tiles.
Its comparison looks at the run's signal before each row (plan mE, task L2, item 4).

**One decode** (plan mE, task L2, item 4). The loop keeps what is decoded while it runs
(`identity::keep_decoded`): every file identified in the process while it runs is kept,
the four last at most, and one identified again under the same limits is the kept copy.
So its picture, read by every stretch and by every `view` and `carry` of a located retry,
is decoded once (`located::eight_places_two_in_tiles_fill_the_eight_crops_and_the_rest_are_named_as_left_out`),
where eight places decoded it sixteen times. What is kept is let go when the run ends. Its record is
`docs/reference/tools/carry.md`.

**What ends the attempts:** a verdict of `pass`; the last attempt the plan allows; a
verdict the table allows no retry on; a wider mask already used once; a check that could
not run; a retry whose shaped mask is the one before it; a retry whose mask could not be
made or filled, which leaves the attempts before it as they stand.

**The plan** (ADR 0004, "A call's plan") is how many attempts the call may make and
whether it checks, with the reason. For each attempt the bound holds one request of the
filler and every request one check may send its judge. The segmenter is asked once to
find, and once for each place a retry may look into: the plan states its requests as
1 + 8 × (the attempts allowed − 1), and the bound holds them, which costs nothing with a
local segmenter and will with one that has a price.

- With a judge ready, the plan is the most attempts, up to the number asked, whose whole
  bound is within the config's cap of one call; when that is fewer than asked, the reason
  says how many of how many.
- Under `auto`, when no judge is ready, or the cap covers a removal and no check, the
  plan is one attempt, unchecked, with the reason. A cap that covers no removal refuses
  the call as any call over its cap is refused.
- Under `required`, the judge is a need of the call whatever its state, so a call with no
  judge ready is not ready at prepare, naming it; and a check the cap does not cover is
  planned as one checked attempt and refused as over the cap.
- Under `off`, one attempt, unchecked: no judge is resolved and nothing is reserved for
  one.

The run reads the plan it was prepared with and never works one out.

**One budget.** Admission reserves the plan's bound once. Each nested call takes its
share from the loop's allowance and reserves nothing; what it did not spend is back when
it ends, and the record says what each attempt settled of the loop's allowance, measured
when the attempt ended. A request charged over its bound puts the loop in debt (ADR
0016), and its next call is then refused for want of allowance: the loop starts no
further call, keeps the attempts that completed, and ends with the reason "the budget ran
out after attempt N"; before the first fill has completed it ends as an error of the type
`budget`, with the refusal's own sentence. Three attempts at their largest, two of them
located retries of eight places and four masks each, are 60 calls of the loop's own, of
the 256 one run may make, and 89 runs; with each check dispatching all 32 tool calls a
check may, 185 runs of the 1,024 a tree may hold. A test makes that run and counts it.

**What is handed back.** The loop completes whenever a fill completed, and commits one
attempt's picture and mask as its own. The attempt kept is the first that passed; failing
that, an `unsure` before a `fail`, and either before an attempt whose check could not run;
then the attempt with fewer findings; then the earlier. With them come the kept attempt's
verdict and findings, when it has them, and a short result as the run's measurements
(`remove_checked/1`: whether the kept attempt was checked, why not when it was not, how
many attempts were begun, which was kept, why the attempts ended). So when every attempt
fails the loop still completes, and says `fail` or `unsure`. It ends as an error only when
no fill completed, with the type and the message the call that failed ended with.

**Unchecked, and saying so.** A check that was started and ended in error leaves its
attempt unchecked, with the check's own error type and message as the attempt's reason,
and the attempts end. The command prints the plan before anything runs, tells each attempt
as it starts and ends, and its last lines say whether the kept attempt was checked, and
why not.

**The record** is `run/6` (ADR 0005): the plan; each attempt with its number, the runs it
made in order, its verdict or the reason it has none, what its retry changed and what its
requests settled; which was kept; and why the attempts ended, in a sentence and, from plan
mE's task L1, as one word of a closed set beside it, which makes the record `run/8` (ADR
0005, "run/8") and which `runs stats` counts by. An attempt that was begun
is listed, one that made no fill among them: its reason says so, and it is never the one
kept. What a located retry changed is its crops, at most eight, as `run/6` holds them:
each crop by its identity, with one mask or none. The mask is the first the crop gave,
as it was carried onto the picture; none means the place joined as itself. Where a crop
sits on the picture is in the record of the `view` run that cut it, and every mask a crop
gave is an output of the `carry` run after it, both among the attempt's runs. A place
that was looked into and not joined has its two runs there and no crop. A retry whose
found mask could not be made lists the runs it made and no crops. An attempt the record
would not take is the loop's own error, of the type `loop`: the loop does not go on with
a record that lists other attempts than it made.

**Cancellation.** The loop looks at its signal before each call and starts no other once
it is set; its running child is under its signal. A cancelled loop commits nothing. The
attempts that completed are in its record, and their pictures are names of the loop's own
tree, which no later call can take (ADR 0008). The loop takes no last look at its signal
before it says which attempt it kept: the record of a run that did not complete says that
none was kept, whatever the run said, and that is held where every record is made (ADR
0005), for this loop and any other.

**Levels.** The loop, squint's recipe, its step `check` and a tool the judge calls are
four, which is every level there is. So a recipe that names a loop as a step is refused
when it is read (`Recipe::check`), in words, and nothing calls a loop.

**The doors.** `whizbang rascal remove-checked`; the tool `remove_checked` over
`whizbang serve`, whose answer carries the plan, the short result and the verdict;
`whizbang runs show`, which prints a loop's attempts; and `whizbang runs stats`, which
counts loops, their attempts and retries, and why their attempts ended. Exit codes are
those of `whizbang squint check`: a verdict does not change one.

## Consequences

- A caller learns whether a removal was checked from the result itself, and a missing
  judge never blocks a removal unless the caller asks that it should.
- Whatever a retry is allowed to do is decided in one function over data, which the next
  task extends without touching the rule.
- The loop is tied to the shipped recipe's step names (`segment`, `select`, `grow`,
  `feather`, `fill`) and to the `px` its step `grow` states. It refuses to be built over a recipe
  that lacks them, so a change to the recipe file is caught when the registry is built.
- The check runs as the recipe ships it. A change to what `source-fidelity` measures first
  or offers its judge changes what the loop's check does, with no change here.
- A loop's tool record, its live evidence and its place on the ladder are task E1's.
  Until then it is experimental, and its visual quality is not confirmed by a person.
- To reverse: remove the action from the registry and the command. `remove_prompt` and
  `check` never depended on it.

## Alternatives

- **`remove_prompt` itself checks and retries.** Turned down: a recipe is a straight
  sequence, and a caller who wants one unchecked pass keeps it.
- **A recipe file with a branch and a repeat.** Turned down by the plan: `recipe/1` gains
  neither.
- **Call `remove_prompt` whole and compare afterwards.** Turned down: the fill would be
  paid for before its mask was seen, and an unchanged retry mask would be filled twice.
- **Retry on an `unsure` that names a place,** as rascal did. Turned down by the plan's
  reviews: an `unsure` does not say the named thing is there.
- **Give `select` the masks on their crops, or have the loop place them itself.** Turned
  down: an image names no parent (ADR 0001), so something must say where a crop sits and
  be held to it, and a run can hand a nested call only an artifact that a call of its own
  committed. One action that compares the two pictures does both.
- **Record every mask a crop gave.** Not done: `run/6` holds one mask for a crop, and the
  record's version changes once (plan m4, task K1). The others are outputs of the
  `carry` run the attempt lists.
- **Compare the found masks, and stop before the shape stretch when a located retry added
  no pixel.** Not done: the comparison of the shaped masks, by their pixels, catches it
  one stretch later, at no cost in requests, and one comparison serves both kinds of
  retry.
- **End the attempts when a located retry's mask is the one before it,** as the plan's
  item 5 had it. Changed after the review: the judge said with confidence that the thing
  remains, and the widening is the one thing left to try.
- **Refuse when the cap covers fewer attempts than asked.** Turned down: the spec's spend
  control "errs toward letting work run", and the plan says what it allows before it
  starts.
- **End as an error when every attempt fails.** Turned down: the caller would be handed
  no picture, and the verdict already says what the judge found.
- **State the judge's arm in the loop's call** (the overrides a door states). Turned
  down: a tool that needs an edit evaluated "does not choose squint's tools" (`SPEC.md`
  section 3).
