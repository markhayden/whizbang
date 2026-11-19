# The gate of details first

Plan m3, task G1, item 3 (plan 2C, task E2, item 9): the gate frozen in
`gates/details-first.toml` (SHA-256 `ddd2ca2e0caaf34da3456f1a03c8a549d706b4ef1559da2e289c9341f0059290`)
over its matrix `gates/details-first-matrix.toml` (identity `5b463d736c8e9b13…`). Arm F, the
details of each pair measured before the judge's first turn, against arm C, squint's control,
on the five published dev sets: 100 items in 25 families. `claude-haiku-5-5` ran three
repetitions, `gemini-3.8-flash` and `gpt-6.1-terra` one each: 1,000 decisions. The
orchestrator ran it with `whizbang eval run --gate` on 2026-11-19, with a binary built at main
`3b6824c59178db9f27284b9ae35bf6c4bbcd64bd`, in resumed legs. Its receipt, aggregates and
digests only, is `m3-gates/details-first-receipt.json` (SHA-256
`b4b90ec980b10ec678c228717775441c2b47d4a891fc395354077ea1c63c3a7f`). It settled about 11.39 US
dollars: the store charged 11.908215 for this gate and the baseline's live rerun together, and
the live rerun was 0.514436 (`m2c-baseline.md`).

## What the gate decided

**Rejected.** Details first did not help on these sets, for any of the three judges.

| Judge | Outcome | Balanced accuracy C | F | Gain [corrected interval] | Families differing | Failed |
|---|---|---|---|---|---|---|
| `claude-haiku-5-5` | rejected | 0.567 | 0.550 | -0.017 [-0.082, 0.050] | 13 of 25 | admission, errors |
| `gemini-3.8-flash` | rejected | 0.646 | 0.554 | -0.092 [-0.204, 0.021] | 18 of 25 | admission, errors |
| `gpt-6.1-terra` | inconclusive | 0.721 | 0.733 | 0.013 [-0.046, 0.071] | 5 of 25 | admission |

The false-reject increase is -0.025 [-0.098, 0.000] for each judge: each failed one fewer of
the 40 expected passes under F. No judge's gain reaches the 3 points the gate asks for, and no
corrected interval stays above zero. The gate is rejected because two judges are rejected
(`Gate::score`: any judge rejected rejects the gate).

**The errors guardrail.** An error, in the receipt, is a decision of the treatment whose run
did not end in a verdict: the run failed (an error, or what it was given found invalid), or it
completed and its verdict could not be read. The guardrail fails when the treatment's share is
2% or more. Under F, `claude-haiku-5-5` had 2 errors of 100 (4 under C): exactly at the margin,
which fails. `gemini-3.8-flash` had 40 of 100 under F, and 26 of 100 under C. `gpt-6.1-terra`
had none under either arm. The receipt counts errors and does not say what each one was. Flash's
errors are high under both arms, so they are not the details' doing alone; why its runs ended
without a verdict is in those runs' records, and is the first thing to look at before Flash
is used again.

## What it does not show

- **Dev sets admit nothing.** These are the sets the tools were built against. The result says
  the details did not help here. It is not evidence about held-out pictures.
- **25 families.** The gate asks for 20. Every interval above is wide: a gain of a few points
  either way is inside each one.
- **The details apply to part of the items.** `align` and the comparisons measure a pair.
  `source-fidelity/scenes-v1` (a source), `reference-consistency/pets-v1` and
  `product-fidelity/packshots-v1` (references) relate one: 60 items. `reality-checks/people-v1`
  and `brief-compliance/stills-v1` have a candidate alone: their 40 items have no pair to
  measure, so arm F differs from arm C there only in its opening words. The gate decides over
  all 100 items, as frozen.
- **One repetition decides.** Haiku's later repetitions give its self-disagreement: 0.145 under
  C and 0.095 under F. Flash and Terra ran once and have none.

## The figures, per arm

| Judge | Arm | Detected (of 60) | Passed (of 40) | False rejects | Unsure | Errors | Cost (USD) | Median wall (ms) |
|---|---|---|---|---|---|---|---|---|
| `claude-haiku-5-5` | C | 50 | 12 | 3 | 28 | 4 | 0.3404 | 15,307 |
| `claude-haiku-5-5` | F | 51 | 10 | 2 | 35 | 2 | 0.3568 | 16,128 |
| `gemini-3.8-flash` | C | 43 | 23 | 5 | 1 | 26 | 3.3651 | 75,687 |
| `gemini-3.8-flash` | F | 35 | 21 | 4 | 0 | 40 | 3.5421 | 78,153 |
| `gpt-6.1-terra` | C | 58 | 19 | 10 | 13 | 0 | 1.1247 | 27,848 |
| `gpt-6.1-terra` | F | 58 | 20 | 9 | 12 | 0 | 1.1806 | 27,870 |

Cost is what each arm's decisions of repetition 1 settled.
