# The format round: submission v1 against v2

Plan e1, task F2. Submission v1 (the form `check` has answered in since milestone 2) against
v2 (ADR 0042: every problem named in one repair, near-misses with one reading accepted and
recorded, items keyed by checklist id), each under arm C, on the ten dev sets, under the
three small judges, twice. Decided by the rule `submission/1`, frozen in the plan before the
run and applied in code by `whizbang eval judges`. Only aggregates are kept: counts and rates
by judge and arm, never an item, a family, a run or a word of a judge. The labels of these
sets are published, so a count of verdicts against them is published too. The sets are dev
sets earlier rounds used: the round is descriptive and admits nothing. The gate on fresh
data that would admit a format is owed to a later slice of E.

## The round

Run by the orchestrator on the owner's Mac on 2026-11-21, in the evaluation store, from a
binary built at `runs_commit`; scored from the store by `whizbang eval judges` built at
`commit`, which changed only the wording of the rule's `why` (it now names every judge that
broke clause (b) or (c), and each measure it broke, to two places). The scores are
`docs/results/e-submission.toml`, and
`evidence::the_submission_record_holds_only_aggregates` holds this page to them and to the
matrix as committed: every row of the three tables below is one that test writes from the
TOML.

```toml
matrix = "e-submission"
matrix_identity = "c386cd97d9b2d6c5"
rule = "submission/1"
outcome = "not_adopted"
settled_by = "c"
format = "C;submission=v1"
commit = "2c5b2ec06dcd3fbba72568cc7bd6106ee4e6cb8d"
runs_commit = "ac3a49a7b21d0a20e86c48c718eae2bc9e5f833f"
settled_usd = 11.77
started = 2026-11-21T15:54:18Z
ended = 2026-11-21T19:40:45Z
```

All 1,320 cells ended: 1,286 completed and 34 failed, every item labelled, and every
decision kept its rejections diagnostic (no unknown, none mismatched). The round's cells
settled 11.77 dollars, against a forecast of 38.08 and an estimate of 19.0; the store's books
show 11.794984 in all, which also holds 0.023 of the probe below.

## How it is run

`matrices/e-submission.toml`: the five use cases' dev sets and their adversarial sets (110
items, 30 families), arms `C;submission=v1` and `C;submission=v2`, the judges
`claude-haiku-5-5`, `gemini-3-8-flash` and `gpt-6-1-terra` of `labs/e-loop/config.toml`, two
repetitions, two runs at once: 1,320 cells, 660 decisions an arm.
`matrix::the_submission_round_is_1320_cells_forecast_within_its_store_and_capped_by_its_dearest_check`
holds the count, the forecast and the cap below to the build.

- **Prices** (plan e1, "Prices and caps"): the rows of the three judges were read on
  2026-10-26; `claude-haiku-5-5` and `gpt-6.1-terra` carry no end date, and
  `gemini-3.8-flash`'s introductory price holds to 2026-12-31 (ADR 0026; the carry-over row on
  the Google prices). A round run after that date waits for the row to be read again.
- **The cap of one call**: the largest bound of one check among the three judges, plus a cent.
  A check reserves sixteen requests at the judge's ceilings (40,000 input and 4,096 output
  tokens), under either format and with the second look or without: 0.112768 dollars for
  Haiku, 0.725760 for Flash, 0.967680 for Terra. So `call_cap_usd = 0.97768`, set in a copy
  of `labs/e-loop/config.toml` outside the repo; the committed file keeps its own cap. The
  copy also approves `https://raw.githubusercontent.com` as a recipient with `classes = []`,
  so `eval sets fetch` may fetch the sets' pictures by their pins; it is sent nothing but each
  picture's address.
- **The forecast** `eval matrix` sets against what the store has left: each cell at its
  model's settled cost of a decision under arm C, the whole doubled, 38.084640 dollars, under
  the 69.97 the evaluation store had left. What it was expected to settle was half that, about
  19.04; it settled 11.77.

```
for set in source-fidelity/scenes-v1 reference-consistency/pets-v1 reality-checks/people-v1 \
    brief-compliance/stills-v1 product-fidelity/packshots-v1 source-fidelity/adversarial-v1 \
    reference-consistency/adversarial-v1 reality-checks/adversarial-v1 \
    brief-compliance/adversarial-v1 product-fidelity/adversarial-v1; do
  whizbang eval sets fetch --set "$set" --store <EVAL_STORE> --config <CONFIG> \
      --env-file <ENV_FILE> || break
done
whizbang spend status --store <EVAL_STORE>
whizbang eval matrix --matrix matrices/e-submission.toml --store <EVAL_STORE> \
    --config <CONFIG> --env-file <ENV_FILE>
whizbang eval judges --mode arms --arm "C;submission=v1" --rule submission/1 \
    --matrix matrices/e-submission.toml --store <EVAL_STORE> --labels <LABELS_DIR> \
    --out docs/results/e-submission.toml
```

`--labels` names a folder laid out as the sets are, `<LABELS_DIR>/<use case>/<set>/labels.toml`,
outside `evalsets/`; it must exist. Each of the ten sets publishes its own labels, so none is
read from it here.

## The decision

**Not adopted, by clause (c); v1 stays the default.** Clause (a) held: submission failures
fell from 14 under v1 to 4 under v2, pooled over the three judges and both repetitions (660
decisions an arm), at most half. Clause (b) held: no judge had more failures under v2. Clause
(c) failed for all three judges, each moving by more than 2 points (point estimates,
repetition 1):

- `claude-haiku-5-5`: its false-reject rate rose 2.2 points, though its balanced accuracy
  rose 2.3 and its submission failures fell from 14 to 4.
- `gemini-3.8-flash`: its balanced accuracy fell 5.2 points. Its false-reject and false-accept
  rates did not move; the fall is in the errors: 14 of its v2 decisions ended in a fault (one
  under v1), none of them a submission failure (below).
- `gpt-6.1-terra`: its balanced accuracy fell 3.3 points and its false-reject rate rose 11.1.

As the rule writes it: under the treatment these judges move by more than 2 points:
claude-haiku-5-5 (false-reject rate +2.22 points); gemini-3.8-flash (balanced accuracy -5.21
points); gpt-6.1-terra (balanced accuracy -3.33 points, false-reject rate +11.11 points); the
control is kept.

Under v2 Haiku's first rejections rose (81 to 93) while its failures fell: the one repair,
naming every problem, saved more of them. Its one recovery used was `evidence_dropped` (238
citations of a call that did not complete, dropped rather than refused); no other kind was
used by any judge. Flash and Terra rarely or never had a submission rejected under either
format.

**Flash's faults, by kind** (`whizbang runs export --matrix e-submission`, read from the
store and counted; aggregates only). All 14 under v2, and the one under v1, are the same
fault: `judge_fault`, `too_large`, a request counted over the 40,000 input tokens its backend
sends, refused by the adapter before it was sent (stage `not_sent`, so nothing was charged for
it). Under v2 they came after 3 to 8 requests of the run (6 on reference-consistency, 5
product-fidelity, 2 brief-compliance, 1 source-fidelity); the one under v1 after 16, on
reality-checks. Haiku's one fault under v2 is the same kind. **None is the provider refusing
the keyed schema**: no request reached the provider, and every other Flash run under v2
completed with the same tool. The count of input holds the tools' definitions, and v2's keyed
tool is larger than v1's, so a likely reading is that it brought long conversations to the
ceiling sooner; the record does not measure a tool's size, so that is not shown here. Clause
(b) counts only submission failures, so it does not count these 14: they are runs v2 ended
and v1 did not, counted as errors, and they reach the decision through clause (c), where they
are Flash's fall in balanced accuracy.

**The live probe before the round.** One check under v2 for each of the three judges, from a
build at 377a9e7f (2.3 cents): each provider accepted v2's keyed tool, its keys of the
provider-safe form, and each check passed. Every judge answered confidently, so none asked
for the second look and the swap of the tool between turns was not exercised. Nor did the
round exercise it: both arms are arm C, whose second look is off, so no cell of the round ran
a second look.

## Per judge, before and after

Over all ten sets. Submission failures, first rejections and unknowns count every decision of
both repetitions; balanced accuracy, its family-resampled 95% interval and cost per decision
are of repetition 1; self-agreement is the share of items with the same verdict both times.
Each row is written from the TOML's scope `all`.

| Judge | Arm | Decisions | Submission failures | First rejections | Unknown | Balanced accuracy | 95% interval | Cost per decision | Self-agreement |
|---|---|---|---|---|---|---|---|---|---|
| claude-haiku-5-5 | C;submission=v1 | 220 | 14 (0.064) | 81 (0.368) | 0 | 0.533 | 0.437 to 0.633 | 0.003294 | 0.809 |
| gemini-3.8-flash | C;submission=v1 | 220 | 0 (0.000) | 6 (0.027) | 0 | 0.880 | 0.810 to 0.946 | 0.013444 | 0.955 |
| gpt-6.1-terra | C;submission=v1 | 220 | 0 (0.000) | 0 (0.000) | 0 | 0.755 | 0.653 to 0.849 | 0.009324 | 0.945 |
| claude-haiku-5-5 | C;submission=v2 | 220 | 4 (0.018) | 93 (0.423) | 0 | 0.556 | 0.472 to 0.642 | 0.004350 | 0.873 |
| gemini-3.8-flash | C;submission=v2 | 220 | 0 (0.000) | 8 (0.036) | 0 | 0.828 | 0.750 to 0.900 | 0.017073 | 0.891 |
| gpt-6.1-terra | C;submission=v2 | 220 | 0 (0.000) | 0 (0.000) | 0 | 0.721 | 0.619 to 0.818 | 0.010553 | 0.936 |

## Rejections, causes and recoveries

Each rejection by its first class, the further classes it named, each decision by the cause it
ended by, and each recovery v2 made, by kind, over both repetitions.

| Judge | Arm | Counted | Name | Count |
|---|---|---|---|---|
| claude-haiku-5-5 | C;submission=v1 | class | bad_field | 3 |
| claude-haiku-5-5 | C;submission=v1 | class | evidence | 45 |
| claude-haiku-5-5 | C;submission=v1 | class | finding_cites | 32 |
| claude-haiku-5-5 | C;submission=v1 | class | location | 3 |
| claude-haiku-5-5 | C;submission=v1 | class | missing_item | 10 |
| claude-haiku-5-5 | C;submission=v1 | class | not_a_list | 2 |
| claude-haiku-5-5 | C;submission=v1 | cause | concluded | 206 |
| claude-haiku-5-5 | C;submission=v1 | cause | invalid_submission | 14 |
| gemini-3.8-flash | C;submission=v1 | class | bad_field | 1 |
| gemini-3.8-flash | C;submission=v1 | class | location | 5 |
| gemini-3.8-flash | C;submission=v1 | cause | concluded | 219 |
| gemini-3.8-flash | C;submission=v1 | cause | fault | 1 |
| gpt-6.1-terra | C;submission=v1 | cause | concluded | 220 |
| claude-haiku-5-5 | C;submission=v2 | class | bad_field | 1 |
| claude-haiku-5-5 | C;submission=v2 | class | finding_cites | 77 |
| claude-haiku-5-5 | C;submission=v2 | class | location | 14 |
| claude-haiku-5-5 | C;submission=v2 | class | not_a_list | 5 |
| claude-haiku-5-5 | C;submission=v2 | also | finding_cites | 6 |
| claude-haiku-5-5 | C;submission=v2 | also | location | 6 |
| claude-haiku-5-5 | C;submission=v2 | cause | concluded | 215 |
| claude-haiku-5-5 | C;submission=v2 | cause | fault | 1 |
| claude-haiku-5-5 | C;submission=v2 | cause | invalid_submission | 4 |
| claude-haiku-5-5 | C;submission=v2 | recovery | evidence_dropped | 238 |
| gemini-3.8-flash | C;submission=v2 | class | bad_field | 5 |
| gemini-3.8-flash | C;submission=v2 | class | location | 3 |
| gemini-3.8-flash | C;submission=v2 | also | bad_field | 10 |
| gemini-3.8-flash | C;submission=v2 | cause | concluded | 206 |
| gemini-3.8-flash | C;submission=v2 | cause | fault | 14 |
| gpt-6.1-terra | C;submission=v2 | cause | concluded | 220 |

## The clauses of each judge

(b) no more submission failures under v2; (c) balanced accuracy not lower, false-reject and
false-accept rates not higher, by more than 2 points (point estimates, repetition 1).

| Judge | v1 failures | v2 failures | No new failures | Balanced accuracy change | False-reject change | False-accept change | Accuracy kept |
|---|---|---|---|---|---|---|---|
| claude-haiku-5-5 | 14 | 4 | yes | +0.023 | +0.022 | -0.015 | no |
| gemini-3.8-flash | 0 | 0 | yes | -0.052 | +0.000 | +0.000 | no |
| gpt-6.1-terra | 0 | 0 | yes | -0.033 | +0.111 | +0.000 | no |

## What this is not

It admits nothing: the sets are dev sets, scored before, and the decision rests on
descriptive evidence. The counts are pooled over three judges; no claim is made of one model
from them. v2 is a bundle (all problems at once, recoveries and the keyed schema), measured
together; the recoveries by kind show which parts were used.
