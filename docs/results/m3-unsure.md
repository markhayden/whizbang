# The attack on unsure: its causes, and the screen of tiles and the second look

Plan m3, task U1, items 1 and 5. Only aggregates are kept: counts by use case, rule and arm,
never an item, a run or a reply. The labels of these sets are published, so a count of
verdicts against them is published too. The screen is on dev sets the evaluation of
milestone 2 also used: it is descriptive and admits nothing (item 6).

## Before: what made a third of the verdicts unsure

`whizbang runs stats --action check --unsure-by rule` over the store of milestone 2's
evaluation after its fix (F1's run: arm F, `claude-haiku-5-5`, 220 decisions, 214 completed,
71 unsure; `docs/results/m2c-evaluation.md`), on a copy of that store. Each row is the use
case, the rule of an answer whose row was unsure, whether it was a checklist item or a
finding, its disposition and its confidence; `answers` counts such answers and `verdicts`
the unsure verdicts that held at least one. `reference` is the checklist's item for a
reference image, and `RQ` any requirement of the job.

| Use case | Rule | Answer | Disposition | Confidence | Answers | Verdicts |
|---|---|---|---|---|---|---|
| reality-checks | RE2 | item | held | tentative | 24 | 24 |
| reality-checks | RE4 | item | held | tentative | 19 | 19 |
| reference-consistency | RQ | item | held | tentative | 21 | 18 |
| reference-consistency | reference | item | held | tentative | 31 | 16 |
| reference-consistency | RC1 | item | held | tentative | 16 | 16 |
| reference-consistency | RC2 | item | held | tentative | 16 | 16 |
| product-fidelity | PF1 | item | held | tentative | 11 | 11 |
| product-fidelity | PF3 | item | held | tentative | 11 | 11 |
| product-fidelity | RQ | item | held | tentative | 15 | 10 |
| product-fidelity | reference | item | held | tentative | 6 | 6 |
| source-fidelity | RQ | item | held | tentative | 9 | 5 |
| reality-checks | RE3 | item | held | tentative | 5 | 5 |
| brief-compliance | RQ | item | held | tentative | 5 | 4 |
| product-fidelity | reference | item | cant_tell | tentative | 4 | 4 |
| source-fidelity | SF2 | item | held | tentative | 4 | 4 |
| brief-compliance | BC2 | item | held | tentative | 3 | 3 |
| reality-checks | RE1 | item | held | tentative | 3 | 3 |
| reality-checks | RE3 | item | violation | tentative | 3 | 3 |
| source-fidelity | SF1 | item | held | tentative | 3 | 3 |
| reality-checks | RE1 | item | violation | tentative | 2 | 2 |
| brief-compliance | BC1 | item | held | tentative | 1 | 1 |
| brief-compliance | BC2 | item | cant_tell | tentative | 1 | 1 |
| brief-compliance | BC3 | item | held | tentative | 1 | 1 |
| brief-compliance | RQ | item | cant_tell | tentative | 1 | 1 |
| product-fidelity | PF2 | item | held | tentative | 1 | 1 |
| product-fidelity | reference | item | hidden | confident | 1 | 1 |
| reality-checks | RE3 | item | cant_tell | tentative | 1 | 1 |
| reality-checks | RE4 | finding | violation | tentative | 1 | 1 |
| reality-checks | RE4 | item | cant_tell | tentative | 1 | 1 |
| reality-checks | RE4 | item | violation | tentative | 1 | 1 |
| reference-consistency | RC2 | item | violation | tentative | 1 | 1 |
| reference-consistency | RQ | item | cant_tell | tentative | 1 | 1 |
| source-fidelity | RQ | item | violation | tentative | 1 | 1 |
| source-fidelity | SF1 | item | violation | tentative | 1 | 1 |

What it says: 69 of the 71 unsure verdicts held an item the judge called `held`, with
confidence `tentative`, and in 55 that was all that made them unsure. The judge was mostly
not unsure because it saw a fault it could not confirm; it saw none and would not commit.
Every one of the 71 held a checklist item left tentative: what the second look asks about. Against the published labels, the unsure verdicts were
mostly of items that pass:

| Use case | Unsure on items labelled pass | Unsure on items labelled fail |
|---|---|---|
| reality-checks | 20 | 9 |
| reference-consistency | 21 | 0 |
| product-fidelity | 11 | 0 |
| source-fidelity | 6 | 0 |
| brief-compliance | 2 | 2 |

In reference-consistency the judge gave no item that passes a pass, and failed every item
that fails: its unsure was all hedging on a clean picture. That made the two changes: tiles,
so that a judge of one picture sees its detail at full resolution before it looks; and a
second look, one more turn that shows the judge crops of what it cited for the items it left
tentative (the candidate in four tiles when it cited no region) and asks it to settle those
items alone, the verdict still worked out from the dispositions.

## The screen

`matrices/unsure-screen.toml`, run once on the owner's Mac through the binary built from the
commit below (`cargo build --release -p whizbang`; that commit's sources, tests apart, are
those of the build, by digest), under a config with the judge, `call_cap_usd = 2.00` and the
recipients `https://api.anthropic.com` and `https://raw.githubusercontent.com`, with the
owner's env file, into a new store whose pictures were fetched by hash from the sets' pinned
revisions, with `whizbang spend reset --allowance 12`. It took 63 minutes
(2026-11-19T06:35Z to 07:39Z). Four arms on four sets (44 items), three repetitions. On
reference-consistency the tiles are not measured, so its runs under the two arms with tiles
are further repetitions of `F` and of `F` with the second look: the difference between a
pair of them is the noise of the screen.

```toml
commit = "6af40ae1ae343a08a2e68720cc287b5af4480812"
whizbang = "0.0.1"
matrix = "unsure-screen"
matrix_identity = "dcdac715ed7d49a9"
model = "claude-haiku-5-5"
sets = 4
items = 44
arms = 4
repetitions = 3
decisions = 528
completed = 512
failed = 16
interrupted = 0
pass = 73
fail = 213
unsure = 226
agreeing = 255
retries = 48
repairs = 173
check_time_median_ms = 22954
check_time_p95_ms = 44873
settled_micros = 2636510
```

Of the 16 that failed, 14 were `invalid_submission` on reference-consistency (a second
submission, after one rejected, that broke the rules again; before any second look) and two
`judge_fault`. None was retried.

## By use case and arm

Each block counts the runs of one arm on one use case's two sets. `false_rejects` are fails
of items labelled pass, `false_accepts` passes of items labelled fail. Balanced accuracy is
the mean of the share of passes among items labelled pass and of fails among items labelled
fail, an `unsure` counting as neither. Each `_95` is a 95% interval resampled by family
(2,000 resamples, seed 20261119); each `_minus_f` is the arm less arm `F` on the same use
case, its interval resampled the same way over the families both share. With six families a
use case and three repetitions of each item, these intervals are narrow for what they hold:
on reference-consistency the arm `F;details=tiles@reality-checks` is arm `F` again, and
still comes out 0.058 above it with an interval that leaves out zero.

```toml
use_case = "reality-checks"
arm = "F"
runs = 66
completed = 66
failed = 0
interrupted = 0
pass = 0
fail = 17
unsure = 49
agreeing = 15
labelled_pass = 33
labelled_fail = 33
false_rejects = 2
false_accepts = 0
unsure_on_pass = 31
unsure_on_fail = 18
agreement = 0.227
agreement_95 = [0.083, 0.361]
unsure_rate = 0.742
unsure_rate_95 = [0.591, 0.917]
false_reject_rate = 0.061
false_reject_rate_95 = [0.000, 0.182]
balanced_accuracy = 0.227
balanced_accuracy_95 = [0.083, 0.361]
```

```toml
use_case = "reality-checks"
arm = "F;details=tiles@reality-checks"
runs = 66
completed = 66
failed = 0
interrupted = 0
pass = 4
fail = 19
unsure = 43
agreeing = 20
labelled_pass = 33
labelled_fail = 33
false_rejects = 3
false_accepts = 0
unsure_on_pass = 26
unsure_on_fail = 17
agreement = 0.303
agreement_95 = [0.150, 0.431]
agreement_minus_f = 0.076
agreement_minus_f_95 = [-0.017, 0.200]
unsure_rate = 0.652
unsure_rate_95 = [0.528, 0.803]
unsure_rate_minus_f = -0.091
unsure_rate_minus_f_95 = [-0.200, -0.015]
false_reject_rate = 0.091
false_reject_rate_95 = [0.000, 0.273]
false_reject_rate_minus_f = 0.030
false_reject_rate_minus_f_95 = [0.000, 0.091]
balanced_accuracy = 0.303
balanced_accuracy_95 = [0.150, 0.431]
balanced_accuracy_minus_f = 0.076
balanced_accuracy_minus_f_95 = [-0.017, 0.200]
```

```toml
use_case = "reality-checks"
arm = "F;second_look=on"
runs = 66
completed = 66
failed = 0
interrupted = 0
pass = 24
fail = 26
unsure = 16
agreeing = 38
labelled_pass = 33
labelled_fail = 33
false_rejects = 6
false_accepts = 6
unsure_on_pass = 9
unsure_on_fail = 7
agreement = 0.576
agreement_95 = [0.333, 0.788]
agreement_minus_f = 0.348
agreement_minus_f_95 = [0.194, 0.545]
unsure_rate = 0.242
unsure_rate_95 = [0.083, 0.470]
unsure_rate_minus_f = -0.500
unsure_rate_minus_f_95 = [-0.667, -0.364]
false_reject_rate = 0.182
false_reject_rate_95 = [0.000, 0.545]
false_reject_rate_minus_f = 0.121
false_reject_rate_minus_f_95 = [0.000, 0.364]
balanced_accuracy = 0.576
balanced_accuracy_95 = [0.333, 0.788]
balanced_accuracy_minus_f = 0.348
balanced_accuracy_minus_f_95 = [0.194, 0.545]
```

```toml
use_case = "reality-checks"
arm = "F;details=tiles@reality-checks;second_look=on"
runs = 66
completed = 65
failed = 1
interrupted = 0
pass = 16
fail = 24
unsure = 25
agreeing = 31
labelled_pass = 33
labelled_fail = 32
false_rejects = 5
false_accepts = 4
unsure_on_pass = 16
unsure_on_fail = 9
agreement = 0.477
agreement_95 = [0.310, 0.653]
agreement_minus_f = 0.250
agreement_minus_f_95 = [0.112, 0.448]
unsure_rate = 0.385
unsure_rate_95 = [0.215, 0.554]
unsure_rate_minus_f = -0.358
unsure_rate_minus_f_95 = [-0.534, -0.194]
false_reject_rate = 0.152
false_reject_rate_95 = [0.000, 0.455]
false_reject_rate_minus_f = 0.091
false_reject_rate_minus_f_95 = [0.000, 0.273]
balanced_accuracy = 0.479
balanced_accuracy_95 = [0.311, 0.653]
balanced_accuracy_minus_f = 0.251
balanced_accuracy_minus_f_95 = [0.116, 0.446]
```

```toml
use_case = "reference-consistency"
arm = "F"
runs = 66
completed = 64
failed = 2
interrupted = 0
pass = 2
fail = 31
unsure = 31
agreeing = 33
labelled_pass = 33
labelled_fail = 31
false_rejects = 0
false_accepts = 0
unsure_on_pass = 31
unsure_on_fail = 0
agreement = 0.516
agreement_95 = [0.468, 0.561]
unsure_rate = 0.484
unsure_rate_95 = [0.439, 0.532]
false_reject_rate = 0.000
false_reject_rate_95 = [0.000, 0.000]
balanced_accuracy = 0.530
balanced_accuracy_95 = [0.500, 0.561]
```

```toml
use_case = "reference-consistency"
arm = "F;details=tiles@reality-checks"
runs = 66
completed = 61
failed = 5
interrupted = 0
pass = 3
fail = 33
unsure = 25
agreeing = 35
labelled_pass = 29
labelled_fail = 32
false_rejects = 1
false_accepts = 0
unsure_on_pass = 25
unsure_on_fail = 0
agreement = 0.574
agreement_95 = [0.508, 0.632]
agreement_minus_f = 0.058
agreement_minus_f_95 = [0.018, 0.099]
unsure_rate = 0.410
unsure_rate_95 = [0.333, 0.491]
unsure_rate_minus_f = -0.075
unsure_rate_minus_f_95 = [-0.119, -0.024]
false_reject_rate = 0.034
false_reject_rate_95 = [0.000, 0.103]
false_reject_rate_minus_f = 0.034
false_reject_rate_minus_f_95 = [0.000, 0.103]
balanced_accuracy = 0.552
balanced_accuracy_95 = [0.500, 0.613]
balanced_accuracy_minus_f = 0.021
balanced_accuracy_minus_f_95 = [0.000, 0.055]
```

```toml
use_case = "reference-consistency"
arm = "F;second_look=on"
runs = 66
completed = 59
failed = 7
interrupted = 0
pass = 15
fail = 26
unsure = 18
agreeing = 41
labelled_pass = 33
labelled_fail = 26
false_rejects = 0
false_accepts = 0
unsure_on_pass = 18
unsure_on_fail = 0
agreement = 0.695
agreement_95 = [0.580, 0.803]
agreement_minus_f = 0.179
agreement_minus_f_95 = [0.080, 0.279]
unsure_rate = 0.305
unsure_rate_95 = [0.197, 0.420]
unsure_rate_minus_f = -0.179
unsure_rate_minus_f_95 = [-0.279, -0.080]
false_reject_rate = 0.000
false_reject_rate_95 = [0.000, 0.000]
false_reject_rate_minus_f = 0.000
false_reject_rate_minus_f_95 = [0.000, 0.000]
balanced_accuracy = 0.727
balanced_accuracy_95 = [0.625, 0.833]
balanced_accuracy_minus_f = 0.197
balanced_accuracy_minus_f_95 = [0.100, 0.292]
```

```toml
use_case = "reference-consistency"
arm = "F;details=tiles@reality-checks;second_look=on"
runs = 66
completed = 65
failed = 1
interrupted = 0
pass = 9
fail = 37
unsure = 19
agreeing = 42
labelled_pass = 32
labelled_fail = 33
false_rejects = 4
false_accepts = 0
unsure_on_pass = 19
unsure_on_fail = 0
agreement = 0.646
agreement_95 = [0.529, 0.815]
agreement_minus_f = 0.131
agreement_minus_f_95 = [0.008, 0.277]
unsure_rate = 0.292
unsure_rate_95 = [0.153, 0.414]
unsure_rate_minus_f = -0.192
unsure_rate_minus_f_95 = [-0.295, -0.104]
false_reject_rate = 0.125
false_reject_rate_95 = [0.000, 0.286]
false_reject_rate_minus_f = 0.125
false_reject_rate_minus_f_95 = [0.000, 0.286]
balanced_accuracy = 0.641
balanced_accuracy_95 = [0.517, 0.812]
balanced_accuracy_minus_f = 0.110
balanced_accuracy_minus_f_95 = [-0.014, 0.267]
```

## Before and after, in short

| Use case | Arm | Agreement | Unsure | False rejects | False accepts | Balanced accuracy |
|---|---|---|---|---|---|---|
| reality-checks | `F` | 15 of 66 (22.7%) | 74.2% | 2 of 33 | 0 of 33 | 0.227 [0.083, 0.361] |
| reality-checks | `F` with tiles | 20 of 66 (30.3%) | 65.2% | 3 of 33 | 0 of 33 | 0.303 [0.150, 0.431] |
| reality-checks | `F` with the second look | 38 of 66 (57.6%) | 24.2% | 6 of 33 | 6 of 33 | 0.576 [0.333, 0.788] |
| reality-checks | both | 31 of 65 (47.7%) | 38.5% | 5 of 33 | 4 of 32 | 0.479 [0.311, 0.653] |
| reference-consistency | `F` (both arms that are `F` here) | 68 of 125 (54.4%) | 44.8% | 1 of 62 | 0 of 63 | |
| reference-consistency | `F` with the second look (both arms) | 83 of 124 (66.9%) | 29.8% | 4 of 65 | 0 of 59 | |

The intervals in the table are those of the blocks above; the pooled rows of
reference-consistency are sums of its two blocks each.

## What the second look did

Of the runs under an arm with the second look whose first submission left an item tentative
and the verdict unsure, how the look moved the verdict, against the item's label (counts of
runs; `settled` when the judge answered the items asked about, `kept` when its turn did not
and the first verdict stood):

| Use case | Arm | Looks | Settled | Kept | Labelled pass: to pass, to fail, still unsure | Labelled fail: to fail, to pass, still unsure | Crops: 1, 2 or 3, 4 |
|---|---|---|---|---|---|---|---|
| reality-checks | `F` with the second look | 43 | 43 | 0 | 16, 2, 9 | 3, 6, 7 | 15, 1, 27 |
| reality-checks | both | 50 | 49 | 1 | 11, 1, 16 | 9, 4, 9 | 16, 3, 31 |
| reference-consistency | `F` with the second look | 29 | 22 | 7 | 11, 0, 18 | none labelled fail | 11, 3, 15 |
| reference-consistency | both (no tiles here) | 29 | 29 | 0 | 6, 4, 19 | none labelled fail | 13, 4, 12 |

Four crops are the candidate's tiles, shown when the judge cited no region for the items it
was asked about.

## What it comes to

- **The second look is what the screen favours.** It took the unsure rate of reality-checks
  from 74.2% to 24.2% (by 0.500, [0.364, 0.667]) and its balanced accuracy from 0.227 to
  0.576 (by 0.348, [0.194, 0.545]); on reference-consistency from 48.4% to 30.5% and from
  0.530 to 0.727 (by 0.197, [0.100, 0.292]).
- **It is not free.** In reality-checks it turned unsure into wrong answers as well as right
  ones: of its 43 looks, 19 moved to the label, 8 away from it (6 passes of items labelled
  fail, 2 fails of items labelled pass) and 16 stayed unsure. False accepts went from 0 of 33
  to 6 of 33. On reference-consistency it moved no item away from its label under the arm
  alone, and four (fails of items labelled pass) under its replicate.
- **Tiles alone did little**, within the noise the replicate shows (+0.076 balanced accuracy,
  [-0.017, 0.200]), and with the second look they did worse than the second look alone.
  They stay experimental.
- **The gate** (item 6) freezes `F` with the second look against `F`, on every use case,
  and is run by G1's `eval run --gate` once it lands; its receipt is added here then. The
  screen is on dev sets the evaluation of milestone 2 used: the second look becomes a default
  only through a gate on fresh data, milestone E's. Until then `--second-look off` stays the
  default.

## Spend

The screen settled 2.63651 dollars (2,636,510 millionths), of the plan's estimate of 31 for
U1's screen and gate.
