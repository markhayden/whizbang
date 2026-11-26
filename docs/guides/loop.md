# The loop: remove, check, retry

`whizbang rascal remove-checked` removes what a phrase names from a picture, has the result
checked against the picture it was made from, and tries again a bounded number of times. It
is the action `remove_checked`, the one loop this build ships (`SPEC.md` section 3, "Loop
across tools"; ADR 0039). `whizbang rascal remove-prompt` is still one unchecked pass; the
loop is for a caller who wants to know whether the result was checked, and what the check
said.

```
whizbang rascal remove-checked --image <id> --prompt "leather collar"
        [--attempts <1..3>] [--check auto|required|off] [--protect <id>]
        [--backend <segmenter>] [--filler <filler>] [--judge <judge>]
        --store <dir> --config <file> --env-file <file>
```

The same call over the MCP server is the tool `remove_checked`, with the arguments `image`,
`prompt`, `attempts`, `check`, `protect`, `backend`, `filler` and `judge`
([The MCP server over stdio](serve.md)).

## What it does

An attempt is one removal with the check of its result.

1. **Find.** The segmenter finds what the phrase names, and the instances are joined into
   one mask. The whole picture is asked about once, in the first attempt; a retry may ask
   again inside the places the judge named (below).
2. **Shape.** The mask is grown, closed, plugged, kept off `--protect` and feathered, by
   the steps of the recipe `remove_prompt` and with its settings.
3. **Fill.** The filler paints what the mask covers. Nothing is sent to the filler until
   the mask it would paint is made, and a retry whose mask turns out to be the one before
   is not filled a second time.
4. **Check.** Squint's recipe `source-fidelity` holds the filled picture to the picture
   the loop was given, under a job the loop writes from the phrase. The job has three
   requirements: `RQ1`, what was to be removed is no longer visible; `RQ2`, nothing else
   in the picture changed; `RQ3`, the filled area looks like its surroundings.

Every attempt fills the picture the loop was given, never an earlier attempt's result.

**When it tries again.** Only when the judge says, with confidence, that the named thing
is still there, and reports nothing damaged. What the retry does depends on whether the
judge said where.

- **It named a place.** The retry looks for the thing again inside that place. The picture
  the loop was given, not the fill, is cut to the place, a twentieth wider on each side;
  the segmenter is asked the same phrase inside the crop, for at most four instances; and
  each mask it finds is carried onto the picture and joined to the mask of the attempt
  before. A place where the segmenter finds nothing joins as it is, the whole box: the
  judge said with confidence that the thing is there. That holds only for a place of at
  most a tenth of the picture's pixels: a larger one that nothing is found in is not
  joined, so that one finding cannot have a large part of the picture painted. Then the mask is shaped, filled and
  checked as in the first attempt. At most eight places are looked into in one retry, and
  only the places of the findings that say the named thing remains: a place the judge
  named for anything else is never cut, asked about or painted.
- **It named none.** The retry shapes the same found mask again, grown twice as far, once
  in a run.

When some findings name places and some do not, the retry looks into the places. A place
counts as none when it is on another picture than the fill the judge was shown; when it
covers no pixel; when its cut would be over 1,024 pixels on a side, which `view` would
scale down, and a mask on a scaled crop is not carried; when its cut would be the whole
picture; and always when the picture is a panorama, since `view` cuts flat pictures only.
A polygon is used by the box around it. When the places were looked into and nothing
joined, the retry is the wider mask.

A retry whose mask covers the same pixels as the one before it is not filled or checked
again. When that retry looked into places, the wider mask is tried next, if it has not
been used and the plan has an attempt left; otherwise the attempts end.

It does not try again when the verdict is `unsure`;
when the judge reports that the removal changed a protected subject, the scene's geometry
or anything else in the picture, because painting more cannot mend that; when the judge's
findings were cut short; or when the last attempt of the plan has been made.

## The plan, said before anything runs

How many attempts the call may make, and whether it checks, is settled when the call is
prepared, and printed on the error stream before anything runs:

```
plan: 3 of 3 attempts asked, checked; segment 17; fill 3; judge 48
```

That is: three attempts allowed of the three asked, each checked; at most 17 requests of
the segmenter, one to find and eight for each of the two retries, one for each place a
retry may look into; at most three requests of the filler; at most 48 of the judge,
sixteen turns for each check. What the call
reserves is the bound of all of it, once, and every step draws on that one reservation.
The call then runs under that plan or not at all: a config or a key that changes between
the plan and the run refuses the call as stale.

`--attempts` is the most attempts, the first included: 1, 2 or 3, and 3 when not given.

`--check` says what happens when a check cannot be made.

| `--check` | When no judge is ready, or the cap of one call covers no check |
|---|---|
| `auto`, the default | One attempt, not checked, and the plan says why: `plan: 1 of 3 attempts asked, unchecked; fill 1; judge 0; no judge is ready: …` |
| `required` | The call is not ready, naming the judge that is missing; or it is refused as over the cap |
| `off` | One attempt, not checked. No judge is resolved and nothing is reserved for one, even where one is ready |

## What it costs

A call reserves the bound of its whole plan before any request is sent. The bound of a
check is the bound of one request of the judge at the backend's ceilings (40,000 input
tokens and 4,096 output tokens when the config gives none), times the sixteen turns a
check may take ([The config file](config.md), "What one request may cost"). A fill is one
request of the filler: with the shipped filler, Bria Eraser on fal, a fill reserves 0.04
dollars. The segmenter on the Mac costs nothing, so the eight requests each retry may send
it add nothing here; a segmenter with a price adds eight times its bound for each attempt
after the first. So, in US dollars:

| Judge | One check | One attempt | Two attempts | Three attempts |
|---|---|---|---|---|
| `claude-haiku-5-5` | 0.112768 | 0.152768 | 0.305536 | 0.458304 |
| `gemini-3.8-flash` | 0.725760 | 0.765760 | 1.531520 | 2.297280 |
| `gemini-4-argon` | 1.935360 | 1.975360 | 3.950720 | 5.926080 |
| `gpt-6.1-terra` | 0.967680 | 1.007680 | 2.015360 | 3.023040 |
| `gpt-6.1-sol` | 1.935360 | 1.975360 | 3.950720 | 5.926080 |

No live call was made to work these out: they are computed from the prices the build
holds for each model and from the default ceilings, and a test recomputes each row from
the same tables (`guides::the_loops_guide_states_the_bound_of_an_attempt_for_each_judge_of_the_catalogue`).
They are what a call reserves, not what it spends. Each request is settled at what its
answer says it used, a check that takes two turns of its sixteen gives the rest back, and
an attempt that is not made costs nothing; what a run settled is in its record, attempt
by attempt.

**The default judge** is the config's `default` judge backend, used when `--judge` names
none. The config guide's first judge example names `gemini-4-argon`, the judge the judge
round of milestone E picked over all sets under `judge-pick/1` (ADR 0044,
`docs/results/e-judges.md`). The loop's own scope, the `source-fidelity` sets, picked no
judge, so the loop keeps that one. Three checked attempts with
`gemini-4-argon` reserve 5.926080 dollars, so a config that wants them sets
`call_cap_usd` to at least that; with less, the plan says how many attempts the cap covers.

**The cap of one call** (`spend.call_cap_usd`) is held against the bound of the whole
plan. When three attempts would reserve more than the cap, the plan is the most attempts
the cap covers, and it says so:

```
plan: 2 of 3 attempts asked, checked; segment 9; fill 2; judge 32; the call cap covers 2 of the 3 attempts asked
```

With `gpt-6.1-terra` and a cap of 2.50, that is two attempts (2.01536) and not three
(3.02304). When the cap covers a removal and no check, the plan is one attempt, not
checked, with the reason `the call cap does not cover a check`; with `--check required`
that call is refused instead. A cap that does not cover one removal refuses the call, as
it refuses any call. A cap of one cent covers nothing here: a check alone reserves more.

## What it prints

On the error stream, while it runs: the plan, then each attempt as it starts and as it
ends.

```
plan: 3 of 3 attempts asked, checked; segment 17; fill 3; judge 48
remove_checked: attempt 1 of 3 starts
remove_checked: attempt 1 ended: fail
remove_checked: attempt 2 of 3 starts
remove_checked: attempt 2 ended: pass
```

On the output, when it is over:

```
run 1795159553507-1 completed
attempts: 2 made, attempt 2 kept
ended: attempt 2 passed
checked: yes
verdict: pass
reason: …
image sha256:…
mask sha256:…
```

`image` is the kept attempt's picture and `mask` the mask it painted, rebound onto that
picture; both are committed to the store.

**Which attempt is kept.** The first that passed. Failing that, an attempt whose verdict
is `unsure` before one that failed, and either before one whose check could not run; then
the attempt with fewer findings; then the earlier one.

**When every attempt fails, the loop still completes.** It hands back its best attempt
with the verdict `fail` or `unsure`, and the caller decides what to do with a picture the
judge did not pass. So read the line `verdict:`, not the exit code, to know whether the
result is good.

**When the result was not checked,** the last lines say so, and why:

```
attempts: 1 made, attempt 1 kept
ended: one unchecked pass: the check is off
checked: no: the check is off
```

A check that was started and ended in an error leaves its attempt unchecked, with the
check's own error type and message as the reason; the picture is still handed back.

**It is an error only when no removal was made:** the segmenter found nothing, the first
fill failed, or the budget ran out before the first fill. The complaint is the type and
the message of the step that failed. When the segmenter found nothing, that is `invalid`,
and says so: `the step segment (segment) found nothing, so the step select (select) had
no mask to take`.

## Exit codes

They are those of `whizbang squint check`: a verdict does not change the exit code.

| Code | Meaning |
|---|---|
| 0 | The loop completed: the kept attempt's verdict, `pass`, `fail` or `unsure`, is printed, or that it was not checked and why |
| 1 | Nothing ran, or no removal was made: the complaint says which, and names the run |
| 2 | The command line was not understood |
| 3 | What was to be printed could not be written |
| 101 | The command itself failed (a panic outside an action): one fixed line is printed |

## The record

`whizbang runs show <run>` prints the loop's attempts after its calls: each attempt's
verdict, or the reason it has none; what its retry changed (a wider mask, or how many crops
it looked into); how many runs it made; and what its requests settled; then which was kept
and why the attempts ended. Every call an attempt made is a run of its own under the
loop's, shown by its own id. For a retry that looked into places the record lists each
crop it cut, by the crop's identity, with the first mask found in it as it was carried
onto the picture, or nothing where the place joined as itself; where the crop sits is in
the record of the `view` run that cut it, and every mask found is an output of the `carry`
run after it. The record also
holds the plan the run was admitted under, the digest of the template the job was written
from, and the kept attempt's verdict and the counts of its findings. No text of a judge's
is in what the loop reads or records of its attempts.

`whizbang runs stats --action remove_checked` counts the loops of a store: their attempts
and retries, how many kept an attempt that was checked, and the runs by why their
attempts ended. A loop's record says why in a sentence and as one word of a closed set
beside it (`run/8`; ADR 0005, "run/8"), and the runs are counted by the word: `passed`,
`findings_cut_short`, `damaged_something_preserved`, `nothing_says_it_remains`,
`last_attempt_of_the_plan`, `wider_mask_already_used`, `mask_is_the_one_before`,
`one_unchecked_pass`, `check_could_not_run`, `budget_ran_out`, `deadline_passed` or
`retry_could_not_be_made`. A record written before the word, or of a loop stopped before it
said why, is counted by its sentence, each attempt's number as `N`.

```
loops: 3 runs, 4 attempts, 1 retries; kept an attempt that was checked 2, not checked 1, none 0
  attempts ended	2	passed
  attempts ended	1	one_unchecked_pass
```

**A loop that is cancelled** commits nothing. The attempts that completed before it are
in its record, and the pictures they made are named there; they are the loop's own, and no
later call can take one.

## The plan alone

`--plan-only` prepares the call and prints its plan on the output, without running it or
spending: no request is sent, nothing is reserved, and no run is kept. It is how a caller learns, before paying for
anything, whether the cap of one call covers three checked attempts with the judge named.

```
whizbang rascal remove-checked --image <id> --prompt "coffee mug" --judge <judge> --plan-only \
        --store <dir> --config <file> --env-file <file>
plan: 3 of 3 attempts asked, checked; segment 17; fill 3; judge 48
```

## Measuring the loop

The loop's round on `segmentation/segment-01` (plan e1, tasks L1 and L2) measures it on
20 pictures with hand-drawn truth masks, in four groups: whole, parts, thin and
keep-the-hole. The manifest `labs/e-loop/segment-01.toml` pins each picture and truth
mask by SHA-256, copied from the set's own manifest at one revision of the public
pictures' repository; the pictures are read from a folder laid out as that repository is,
outside the work tree, and are never committed.

- `scripts/loop-evidence.sh --manifest labs/e-loop/segment-01.toml --repetition <n> …`
  runs the loop once on each picture, under the request id
  `e-loop-<judge>-<n>-<item>`, and writes a tally of aggregates, by group too. Before the
  first run it prepares one call with `--plan-only` and stops, having run nothing, unless
  the plan is three checked attempts of three. Only the pictures are given to the loop:
  an entry that names a truth mask is refused.
- `whizbang eval loop score --manifest <file> --set <folder> --judge <judge>
  --repetitions <n> --out <file> --store <dir>` reads those runs from the store. Every
  entry under every repetition asked is a requested run, and one the store does not hold
  is counted as missing, overall and by group. It scores each attempt's found and
  shaped masks against the truth mask, which it reads from the folder by its pin and never
  imports: coverage (the share of the truth inside the mask where its weight is at least a
  half), IoU and size; the judge's `RQ1` answer against mask under-coverage (coverage
  below 0.95, with 0.90 and 0.99 as sensitivity); coverage across each retry; and whether
  the kept attempt had the highest coverage. Every measure says its denominator, and an
  attempt without what a measure needs (a retry whose mask was the one before has no fill
  or check; a run with no kept attempt has no kept mask; a mask of another size than its
  truth mask has no measure, for the reason `size`) is counted as unavailable for it, by
  why. A truth mask with no pixel of the object is refused; a grey one that carries a
  colour profile is read by its grey samples, since a mask's profile says nothing of the
  object. It writes `loop-score/1`, aggregates only. These are statements about masks, not
  about the finished pictures: a fill can erase beyond its mask or leave a trace inside it.
- `whizbang eval loop sheet … --out <folder>` writes the contact sheet: every completed
  fill beside its original, in an order drawn from a seed drawn for that sheet and written
  only in its `assignment.json`, with no verdict, coverage,
  attempt number or judge on the page, and marks a person may set, "the named thing is
  still visible" and "something else was damaged". The marks leave the page only as
  `marks.json`, saved through the browser; `eval loop score --labels marks.json --sheet
  assignment.json` then adds what they say of the finished pictures, by group, and refuses
  a picture marked twice. The sheet is never written inside a git work tree, nor into a
  folder that is not empty.
- Any recipe's runs are marked the same way (plan mE, task M2). A cases file
  (`edit-cases/1`) names the action, and for each case its id, its family, the request id
  its run was made under and what was to be removed or mended. `whizbang eval loop sheet
  --cases <file> --out <folder>` shows each case's finished picture beside the picture its
  run was given, both read from the store; `whizbang lab pairs --from-runs <file>
  --protocol <file> --out <folder>` draws the blinded pairs of those runs against the
  picture each was given (or against `--reference-runs`, another cases file's runs of the
  same cases), which `lab decode` reads unchanged; and `whizbang eval edits gate --gate
  <file> --receipt <file>` scores the recipe's gate, `edit-gate/1`, from the store, with
  `--labels marks.json --sheet assignment.json` for a person's marks and `--assignment
  --picks` for the pairs. Its receipt, `edit-receipt/1`, holds aggregates only, and `eval
  admitted` lists the recipe a rung 4 receipt admitted. The marks, the picks, both
  assignments and a rung 4 gate's cases are refused inside a git work tree. The gate's
  `[quality] outcomes` is the list of the decode's outcomes it accepts for every group
  (`preferred`, `not_worse`, or both); its `corpus` names where the cases come from, and a
  rung 4 gate on an exposed corpus is refused.

## What it does not do yet

- A place whose cut would be over 1,024 pixels on a side is not looked into, and a
  polygon is used by the box around it (`docs/carry-over.md`).
- The segmenter is asked inside a crop as it is asked about the picture: with the same
  phrase and no lower threshold.
- The loop is marked experimental. Its tool record,
  `docs/reference/tools/remove_checked.md`, shows it on three worked examples, one of them a
  case it handles badly, and says what of its scoring is shown. What it did on real
  models is reported, not scored, by `scripts/loop-evidence.sh` (plan m4, task E1), and
  its visual quality has not been confirmed by a person.
- Nothing may call the loop: calls nest four levels deep and the loop uses all four, so a
  recipe that names it as a step is refused when it is read.
- On Linux no local segmenter runs, so the loop is not ready there until a remote
  segmenter exists (milestone 6).
