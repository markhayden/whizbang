# The 2026-11 tool study

Does giving a judge model more evidence tools make it better at catching a bad image? This
is what the old squint harness measured on 2026-11-10 and 2026-11-11, written up from the
baseline package's records alone.

The short answer: for one model out of seven, one tool helped by a margin the method can
tell from noise. For the other twelve comparisons it cannot tell. The result is
provisional, and the limits at the end say why.

Every number here is an aggregate. The package holds the items, their labels and each
decision, and it is never published. This page holds no item name, no item-level verdict
and no label.

Every table on this page is written by `scripts/baseline/tables.py` from
`2026-11-tool-study.toml`, the file beside it. The check that runs before every commit
fails when a table or a figure in the prose differs from that file.

## What was tested

The job is source fidelity: the judge gets a source image and a candidate made from it, and
decides whether the candidate kept what it had to keep. It answers pass, fail or unsure.

The set is `source-fidelity/blind-v3`. The same items were judged under up to three arms:

- **Control.** The judge has the `view` and `align` tools.
- **With outline.** The control, plus the `outline` tool.
- **With regions.** The control, plus the `regions` tool.

Seven models were run. Six had all three arms on 2026-11-11, once each, except
claude-haiku-5-5, which ran every arm twice to measure how much a model disagrees with
itself. gemini-4-argon ran the day before, as the gate for `outline`, with the control and
the outline arm only.

<!-- generated:scope -->
| Measure | Value |
|---|---|
| Items in the set | 143 |
| Families of items | 24 |
| Expected to fail, after corrections | 73 |
| Expected to pass, after corrections | 70 |
| Labels corrected after the runs | 2 (corrections version 1) |
| Expected to fail, as sealed | 71 |
| Runs | 7 |
| Decisions, every repetition | 3,289 |
| Spend, every run | $158.58 |
| Decisions in the six study runs | 3,003 |
| Spend in the six study runs | $148.00 |
| Decisions that ended in an error | 41 |
<!-- /generated -->

<!-- generated:runs -->
| Run | Decisions | Repetitions | Hypotheses | Seed | Resamples | Spend |
|---|---|---|---|---|---|---|
| `gate-outline-initial-2026-11` | 286 | 1 | 1 | 20261110 | 10,000 | $10.58 |
| `study-flash-2026-11` | 429 | 1 | 2 | 20261111 | 10,000 | $14.80 |
| `study-haiku-2026-11` | 858 | 2 | 3 | 20261111 | 10,000 | $3.98 |
| `study-opus-2026-11` | 429 | 1 | 2 | 20261111 | 10,000 | $68.33 |
| `study-sol-2026-11` | 429 | 1 | 2 | 20261111 | 10,000 | $21.75 |
| `study-sonnet-2026-11` | 429 | 1 | 2 | 20261111 | 10,000 | $28.54 |
| `study-terra-2026-11` | 429 | 1 | 2 | 20261111 | 10,000 | $10.61 |
<!-- /generated -->

## Method

**Scoring.** A decision detects a failure only when its verdict is fail. The policy frozen
before the runs sends an unsure verdict and an error to review: neither one detects a
failure, and neither one rejects a sound image.

- **Catch rate:** of the items expected to fail, the share with a fail verdict.
- **False rejects:** of the items expected to pass, the share with a fail verdict.
- **Balanced accuracy:** the mean of the catch rate and one minus the false-reject rate.
- **Decided correctly:** of all items, the share caught or cleared. An item sent to review
  is not decided, so it does not count here. Balanced accuracy does not see a sound image
  sent to review; this measure does.

Rates per model and arm carry a 95% Wilson interval in the numbers file.

**Labels.** The set's labels were sealed before the runs. Two of them were corrected by
the owner after the runs, both from pass to fail, and the corrections are versioned. The
headline tables use the corrected labels. The tool effects are also given under the sealed
labels, so the effect of the corrections is visible.

**Tool effects.** Each tool arm is compared with the control on the same items, in the
first repetition. Two differences are reported, tool arm minus control, in percentage
points:

- the catch rate, which is the estimand the old harness froze before the runs;
- balanced accuracy, which is the figure first reported for this study.

**Intervals.** Items come in families of near-identical cases, so whole families are
resampled with replacement, not single items. The catch-rate difference resamples the
families that hold an item expected to fail, and weights every item equally. Each run's
matrix froze a seed, a resample count and a number of hypotheses. The interval runs between
the type 7 quantiles at 0.025 divided by the hypotheses, and its complement. That is a 95%
interval for one hypothesis, 97.5% for two and 98.33% for three. An interval that excludes
zero is marked with a star.

**The generator.** The old harness never computed these intervals for the study, because
it did not hold the labels. Its generator is not reproduced here. The resamples are drawn
from SHA-256 in counter mode, described in the script, so the same package gives the same
numbers on any Python.

## Results per model and arm

First repetition only.

<!-- generated:cells -->
| Model | Arm | Catch rate | False rejects | Balanced accuracy | Decided correctly | Unsure | Errors | Median time |
|---|---|---|---|---|---|---|---|---|
| gemini-4-argon | control | 97.3% (71 of 73) | 1.4% (1 of 70) | 97.9% | 97.9% (140 of 143) | 0 | 0 | 47.9 s |
| gemini-4-argon | with outline | 97.3% (71 of 73) | 2.9% (2 of 70) | 97.2% | 97.2% (139 of 143) | 0 | 1 | 50.7 s |
| gemini-3.8-flash | control | 86.3% (63 of 73) | 7.1% (5 of 70) | 89.6% | 88.1% (126 of 143) | 5 | 0 | 58.9 s |
| gemini-3.8-flash | with outline | 91.8% (67 of 73) | 7.1% (5 of 70) | 92.3% | 91.6% (131 of 143) | 1 | 0 | 67.0 s |
| gemini-3.8-flash | with regions | 97.3% (71 of 73) | 7.1% (5 of 70) | 95.1% | 94.4% (135 of 143) | 1 | 0 | 64.3 s |
| claude-haiku-5-5 | control | 79.5% (58 of 73) | 1.4% (1 of 70) | 89.0% | 68.5% (98 of 143) | 33 | 5 | 21.5 s |
| claude-haiku-5-5 | with outline | 79.5% (58 of 73) | 1.4% (1 of 70) | 89.0% | 42.7% (61 of 143) | 75 | 5 | 35.7 s |
| claude-haiku-5-5 | with regions | 78.1% (57 of 73) | 4.3% (3 of 70) | 86.9% | 44.8% (64 of 143) | 68 | 7 | 31.2 s |
| claude-opus-5-5 | control | 94.5% (69 of 73) | 1.4% (1 of 70) | 96.5% | 81.1% (116 of 143) | 26 | 0 | 24.6 s |
| claude-opus-5-5 | with outline | 97.3% (71 of 73) | 4.3% (3 of 70) | 96.5% | 80.4% (115 of 143) | 22 | 2 | 34.3 s |
| claude-opus-5-5 | with regions | 94.5% (69 of 73) | 5.7% (4 of 70) | 94.4% | 75.5% (108 of 143) | 24 | 6 | 31.8 s |
| gpt-6.1-sol | control | 97.3% (71 of 73) | 5.7% (4 of 70) | 95.8% | 95.8% (137 of 143) | 0 | 0 | 27.1 s |
| gpt-6.1-sol | with outline | 94.5% (69 of 73) | 4.3% (3 of 70) | 95.1% | 93.0% (133 of 143) | 3 | 0 | 38.1 s |
| gpt-6.1-sol | with regions | 98.6% (72 of 73) | 2.9% (2 of 70) | 97.9% | 97.9% (140 of 143) | 0 | 0 | 36.4 s |
| claude-sonnet-5-5 | control | 91.8% (67 of 73) | 2.9% (2 of 70) | 94.5% | 81.8% (117 of 143) | 20 | 0 | 17.1 s |
| claude-sonnet-5-5 | with outline | 89.0% (65 of 73) | 2.9% (2 of 70) | 93.1% | 76.9% (110 of 143) | 29 | 0 | 28.8 s |
| claude-sonnet-5-5 | with regions | 91.8% (67 of 73) | 2.9% (2 of 70) | 94.5% | 77.6% (111 of 143) | 28 | 1 | 26.2 s |
| gpt-6.1-terra | control | 97.3% (71 of 73) | 8.6% (6 of 70) | 94.3% | 93.7% (134 of 143) | 1 | 0 | 25.4 s |
| gpt-6.1-terra | with outline | 98.6% (72 of 73) | 7.1% (5 of 70) | 95.7% | 91.6% (131 of 143) | 6 | 0 | 36.8 s |
| gpt-6.1-terra | with regions | 97.3% (71 of 73) | 7.1% (5 of 70) | 95.1% | 93.7% (134 of 143) | 2 | 0 | 35.0 s |
<!-- /generated -->

The same decisions as counts. "To review" is an unsure verdict or an error.

<!-- generated:outcomes -->
| Model | Arm | Fail: caught | Fail: let through | Fail: to review | Pass: cleared | Pass: rejected | Pass: to review |
|---|---|---|---|---|---|---|---|
| gemini-4-argon | control | 71 | 2 | 0 | 69 | 1 | 0 |
| gemini-4-argon | with outline | 71 | 1 | 1 | 68 | 2 | 0 |
| gemini-3.8-flash | control | 63 | 7 | 3 | 63 | 5 | 2 |
| gemini-3.8-flash | with outline | 67 | 6 | 0 | 64 | 5 | 1 |
| gemini-3.8-flash | with regions | 71 | 2 | 0 | 64 | 5 | 1 |
| claude-haiku-5-5 | control | 58 | 6 | 9 | 40 | 1 | 29 |
| claude-haiku-5-5 | with outline | 58 | 1 | 14 | 3 | 1 | 66 |
| claude-haiku-5-5 | with regions | 57 | 1 | 15 | 7 | 3 | 60 |
| claude-opus-5-5 | control | 69 | 0 | 4 | 47 | 1 | 22 |
| claude-opus-5-5 | with outline | 71 | 1 | 1 | 44 | 3 | 23 |
| claude-opus-5-5 | with regions | 69 | 1 | 3 | 39 | 4 | 27 |
| gpt-6.1-sol | control | 71 | 2 | 0 | 66 | 4 | 0 |
| gpt-6.1-sol | with outline | 69 | 4 | 0 | 64 | 3 | 3 |
| gpt-6.1-sol | with regions | 72 | 1 | 0 | 68 | 2 | 0 |
| claude-sonnet-5-5 | control | 67 | 4 | 2 | 50 | 2 | 18 |
| claude-sonnet-5-5 | with outline | 65 | 2 | 6 | 45 | 2 | 23 |
| claude-sonnet-5-5 | with regions | 67 | 1 | 5 | 44 | 2 | 24 |
| gpt-6.1-terra | control | 71 | 2 | 0 | 63 | 6 | 1 |
| gpt-6.1-terra | with outline | 72 | 1 | 0 | 59 | 5 | 6 |
| gpt-6.1-terra | with regions | 71 | 2 | 0 | 63 | 5 | 2 |
<!-- /generated -->

What stands out:

- **Five of the seven controls sit between 94.3% and 97.9% balanced accuracy.** There is
  little room above them for a tool to add anything this set can measure.
- **gemini-3.8-flash and claude-haiku-5-5 are the two with room,** at 89.6% and 89.0%.
- **The three Claude models send many sound images to review.** In the control,
  claude-opus-5-5 is unsure on 26 items and claude-sonnet-5-5 on 20. Balanced accuracy
  hides this. Decided correctly shows it: 81.1% and 81.8%, against 97.9% for
  gemini-4-argon.
- **The tools make claude-haiku-5-5 far less decisive.** It decides 68.5% of items
  correctly in the control, 42.7% with outline and 44.8% with regions. With outline it
  clears 3 of the 70 sound images and sends 66 to review.

## Tool effects

Corrected labels. A star marks an interval that excludes zero.

<!-- generated:effects -->
| Model | Tool | Interval | Catch rate, points | Balanced accuracy, points | Items that changed |
|---|---|---|---|---|---|
| gemini-4-argon | outline | 95% | +0.0 (-4.3 to +4.0) | -0.7 (-2.9 to +1.4) | 3 of 143 |
| gemini-3.8-flash | outline | 97.5% | +5.5 (-2.8 to +13.9) | +2.7 (-2.7 to +7.9) | 14 of 143 |
| gemini-3.8-flash | regions | 97.5% | +11.0 (+2.7 to +18.9) * | +5.5 (+1.4 to +9.5) * | 10 of 143 |
| claude-haiku-5-5 | outline | 98.33% | +0.0 (-9.6 to +9.2) | +0.0 (-4.2 to +3.9) | 16 of 143 |
| claude-haiku-5-5 | regions | 98.33% | -1.4 (-14.1 to +9.6) | -2.1 (-8.3 to +3.3) | 15 of 143 |
| claude-opus-5-5 | outline | 97.5% | +2.7 (+0.0 to +7.7) | -0.1 (-4.2 to +3.6) | 6 of 143 |
| claude-opus-5-5 | regions | 97.5% | +0.0 (+0.0 to +0.0) | -2.1 (-5.0 to +0.0) | 5 of 143 |
| gpt-6.1-sol | outline | 97.5% | -2.7 (-7.8 to +0.0) | -0.7 (-3.4 to +2.1) | 3 of 143 |
| gpt-6.1-sol | regions | 97.5% | +1.4 (+0.0 to +5.2) | +2.1 (+0.0 to +4.9) | 3 of 143 |
| claude-sonnet-5-5 | outline | 97.5% | -2.7 (-9.2 to +2.8) | -1.4 (-4.9 to +2.1) | 6 of 143 |
| claude-sonnet-5-5 | regions | 97.5% | +0.0 (-4.2 to +4.2) | +0.0 (-2.1 to +2.1) | 2 of 143 |
| gpt-6.1-terra | outline | 97.5% | +1.4 (+0.0 to +5.2) | +1.4 (-1.4 to +4.7) | 4 of 143 |
| gpt-6.1-terra | regions | 97.5% | +0.0 (+0.0 to +0.0) | +0.7 (-2.1 to +3.6) | 3 of 143 |
<!-- /generated -->

- **One comparison of thirteen excludes zero: gemini-3.8-flash with regions.** Its catch
  rate rises by 11.0 points and its balanced accuracy by 5.5.
- **No tool made any model measurably worse on these two measures.** Every other interval
  spans zero or touches it.
- **"Items that changed" counts items whose fail-or-not-fail outcome differs between the
  two arms.** For most models it is a handful. The next section shows that a model run
  twice under one arm changes about as many.

The same comparisons under the sealed labels:

<!-- generated:sealed-effects -->
| Model | Tool | Interval | Catch rate, points | Balanced accuracy, points | Items that changed |
|---|---|---|---|---|---|
| gemini-4-argon | outline | 95% | +0.0 (-4.3 to +4.2) | -0.7 (-2.9 to +1.4) | 3 of 143 |
| gemini-3.8-flash | outline | 97.5% | +5.6 (-2.9 to +14.1) | +2.8 (-2.8 to +7.9) | 14 of 143 |
| gemini-3.8-flash | regions | 97.5% | +11.3 (+2.8 to +19.4) * | +5.6 (+1.4 to +9.7) * | 10 of 143 |
| claude-haiku-5-5 | outline | 98.33% | -1.4 (-10.0 to +7.1) | -1.4 (-5.6 to +2.8) | 16 of 143 |
| claude-haiku-5-5 | regions | 98.33% | -2.8 (-15.2 to +8.3) | -3.5 (-9.3 to +2.1) | 15 of 143 |
| claude-opus-5-5 | outline | 97.5% | +1.4 (+0.0 to +5.6) | -1.4 (-4.9 to +2.1) | 6 of 143 |
| claude-opus-5-5 | regions | 97.5% | +0.0 (+0.0 to +0.0) | -2.1 (-4.9 to +0.0) | 5 of 143 |
| gpt-6.1-sol | outline | 97.5% | -1.4 (-5.6 to +0.0) | +0.7 (-2.1 to +3.5) | 3 of 143 |
| gpt-6.1-sol | regions | 97.5% | +1.4 (+0.0 to +5.6) | +2.1 (+0.0 to +4.9) | 3 of 143 |
| claude-sonnet-5-5 | outline | 97.5% | -2.8 (-9.7 to +2.9) | -1.4 (-5.0 to +2.1) | 6 of 143 |
| claude-sonnet-5-5 | regions | 97.5% | +0.0 (-4.3 to +4.3) | +0.0 (-2.1 to +2.1) | 2 of 143 |
| gpt-6.1-terra | outline | 97.5% | +0.0 (+0.0 to +0.0) | +0.0 (-2.8 to +3.5) | 4 of 143 |
| gpt-6.1-terra | regions | 97.5% | +0.0 (+0.0 to +0.0) | +0.7 (-2.1 to +3.5) | 3 of 143 |
<!-- /generated -->

The corrections move single comparisons by up to a point and a half and do not change
which one excludes zero.

## Self-disagreement

claude-haiku-5-5 ran every arm twice. Nothing differs between the two repetitions except
the model's own variation.

<!-- generated:repeat -->
| Model | Arm | Items | Fail or not fail differs | Verdict differs | Catch rate, first and second | Balanced accuracy, first and second |
|---|---|---|---|---|---|---|
| claude-haiku-5-5 | control | 143 | 5 | 29 | 79.5% and 83.6% | 89.0% and 91.1% |
| claude-haiku-5-5 | with outline | 143 | 15 | 24 | 79.5% and 68.5% | 89.0% and 84.2% |
| claude-haiku-5-5 | with regions | 143 | 18 | 37 | 78.1% and 78.1% | 86.9% and 88.3% |
<!-- /generated -->

- **The control disagrees with itself on 5 items** by the fail-or-not-fail outcome. The
  tool arms disagree with themselves on 15 and 18.
- **That is the size of most tool effects in the table above.** A difference of this many
  items between two arms, from one repetition each, is not evidence of an effect.
- **Only this model has a second repetition.** The noise of the other six is not measured.

## Errors

A decision ends in an error when the judge's final submission does not validate and no
repair is left. Every error in the study is of that kind.

<!-- generated:errors-by-model -->
| Model | Errors, every repetition |
|---|---|
| claude-haiku-5-5 | 31 |
| claude-opus-5-5 | 8 |
| claude-sonnet-5-5 | 1 |
| gemini-4-argon | 1 |
| All models | 41 |
<!-- /generated -->

<!-- generated:errors -->
| Model | Arm | Repetition | Errors | On items expected to fail | Class |
|---|---|---|---|---|---|
| gemini-4-argon | with outline | 1 | 1 | 1 | `submission_invalid` |
| claude-haiku-5-5 | control | 1 | 5 | 3 | `submission_invalid` |
| claude-haiku-5-5 | control | 2 | 1 | 1 | `submission_invalid` |
| claude-haiku-5-5 | with outline | 1 | 5 | 3 | `submission_invalid` |
| claude-haiku-5-5 | with outline | 2 | 6 | 6 | `submission_invalid` |
| claude-haiku-5-5 | with regions | 1 | 7 | 5 | `submission_invalid` |
| claude-haiku-5-5 | with regions | 2 | 7 | 4 | `submission_invalid` |
| claude-opus-5-5 | with outline | 1 | 2 | 0 | `submission_invalid` |
| claude-opus-5-5 | with regions | 1 | 6 | 0 | `submission_invalid` |
| claude-sonnet-5-5 | with regions | 1 | 1 | 0 | `submission_invalid` |
<!-- /generated -->

<!-- generated:error-kinds -->
| What the submission was refused for | Errors |
|---|---|
| A finding's location was invalid | 20 |
| A finding used a rule in a way the rule does not allow | 9 |
| The checklist was not filled in as required | 8 |
| Evidence named a tool call that did not complete | 3 |
| A rule was applied to an image it does not judge | 1 |
<!-- /generated -->

- **claude-haiku-5-5 has 31 of the 41 errors,** spread over every arm and both repetitions,
  including the control.
- **claude-opus-5-5 has 8, all in the tool arms and all for a finding's location.** None
  fell on an item expected to fail, so they cost it no catch rate.
- **An error on an item expected to fail lowers the catch rate,** because an error goes to
  review. In its first repetition claude-haiku-5-5 lost 3, 3 and 5 such items across the
  three arms.

## Spend

What each run's ledger settled, with every repetition counted.

<!-- generated:spend -->
| Model | Arm | Decisions | Spend | Per decision |
|---|---|---|---|---|
| gemini-4-argon | control | 143 | $4.25 | $0.0297 |
| gemini-4-argon | with outline | 143 | $6.33 | $0.0442 |
| gemini-3.8-flash | control | 143 | $3.25 | $0.0228 |
| gemini-3.8-flash | with outline | 143 | $5.84 | $0.0408 |
| gemini-3.8-flash | with regions | 143 | $5.71 | $0.0399 |
| claude-haiku-5-5 | control | 286 | $0.94 | $0.0033 |
| claude-haiku-5-5 | with outline | 286 | $1.53 | $0.0053 |
| claude-haiku-5-5 | with regions | 286 | $1.51 | $0.0053 |
| claude-opus-5-5 | control | 143 | $16.96 | $0.1186 |
| claude-opus-5-5 | with outline | 143 | $25.57 | $0.1788 |
| claude-opus-5-5 | with regions | 143 | $25.79 | $0.1804 |
| gpt-6.1-sol | control | 143 | $4.81 | $0.0336 |
| gpt-6.1-sol | with outline | 143 | $8.15 | $0.0570 |
| gpt-6.1-sol | with regions | 143 | $8.79 | $0.0614 |
| claude-sonnet-5-5 | control | 143 | $7.15 | $0.0500 |
| claude-sonnet-5-5 | with outline | 143 | $10.60 | $0.0741 |
| claude-sonnet-5-5 | with regions | 143 | $10.79 | $0.0754 |
| gpt-6.1-terra | control | 143 | $2.46 | $0.0172 |
| gpt-6.1-terra | with outline | 143 | $3.95 | $0.0276 |
| gpt-6.1-terra | with regions | 143 | $4.20 | $0.0293 |
<!-- /generated -->

- **The seven runs cost $158.58;** the six study runs cost $148.00 of it.
- **A tool arm costs more per decision than the control for every model,** between about
  one and a half and two times as much.
- **claude-opus-5-5 accounts for $68.33 of it,** the most of any model, for a control that
  is already near the ceiling.

## Earlier figures

These figures were reported when the runs finished. The script behind the first interval
was not kept, so each is computed again here. Where the two differ, the figure computed
here stands.

<!-- generated:earlier -->
| Figure | First reported | Computed here | Result |
|---|---|---|---|
| Gain from regions for gemini-3.8-flash, balanced accuracy, points | +5.6 | +5.6 as sealed, +5.5 after corrections | Reproduced as sealed |
| Its 97.5% interval | +1.2 to +10.6 | +1.4 to +9.7 as sealed, +1.4 to +9.5 after corrections | Changed |
| Errors, and whose | 31, all claude-haiku-5-5 | 41 in all: 31 claude-haiku-5-5, 10 other models | Changed |
| What the errors were | Submissions in the wrong form | Every error is `submission_invalid` | Reproduced |
| Spend, six study runs | About $148 | $148.00 | Reproduced |
| Spend, with the gate run | $158.58 | $158.58 | Reproduced |
<!-- /generated -->

- **The gain is reproduced under the sealed labels.** The first report used them; the
  corrections came later and move the gain by about a tenth of a point.
- **The interval is narrower here at both ends, and still excludes zero.** This page
  resamples whole families and takes type 7 quantiles, as the old harness's frozen method
  does. How the first interval was drawn is not recorded.
- **Not all errors were claude-haiku-5-5's.** It has 31 over its two repetitions. The
  other 10 belong to claude-opus-5-5, claude-sonnet-5-5 and gemini-4-argon.

## Limits

- **One repetition for six of the seven models.** The one model run twice disagrees with
  itself on as many items as most tools changed. A single comparison that excludes zero
  out of thirteen should be run again before anything is built on it.
- **The correction is per run.** Each run's interval is widened for that run's own
  hypotheses, as frozen. Nothing corrects across the thirteen comparisons on this page.
- **The set is exposed.** Every item of `source-fidelity/blind-v3` has since been seen by
  an agent, and two labels were changed after the results were known. It is a regression
  set now. It cannot stand as held-out evidence for a later claim.
- **The gate run is not the study.** gemini-4-argon ran a day earlier, with its own seed,
  one hypothesis, and no regions arm.
- **The set is near the ceiling for five models,** so it cannot show a gain for them even
  if a tool gives one.
- **Review is not free.** The two headline measures treat an item sent to review as
  neither right nor wrong. A model that sends most sound images to review scores well on
  them and would be little use in practice.
- **Tool results were compared, not the tools.** The study does not say why `regions`
  helped one model, or whether the judge used what the tool returned.

## Reproducing

```
python3 scripts/baseline/tables.py compute <package> --toml docs/results/2026-11-tool-study.toml
python3 scripts/baseline/tables.py render docs/results/2026-11-tool-study.toml docs/results/2026-11-tool-study.md
python3 scripts/baseline/tables.py check docs/results/2026-11-tool-study.toml docs/results/2026-11-tool-study.md --manifest docs/baseline/manifest.toml
```

The first command needs the package and takes a few seconds. It writes the same bytes each
time. The other two need only this folder. The numbers file names the package it was
computed from by the digest in `docs/baseline/manifest.toml`.
