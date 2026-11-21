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

The orchestrator fills this block after the run; `outcome` stays `pending` until then, and
`evidence::the_submission_record_holds_only_aggregates` holds the page to the matrix as
committed and, once the outcome is written, to `docs/results/e-submission.toml`: the rows of
the three tables below are the ones that test prints when the page lacks them, each written
from the TOML, and replace the `Pending` rows.

```toml
matrix = "e-submission"
matrix_identity = "c386cd97d9b2d6c5"
rule = "submission/1"
outcome = "pending"
```

Once run, the block also gives `commit` (the commit the binary was built from, as the TOML
names it), `settled_by` (the clause that settled the outcome), `format` (the arm kept or
adopted), `settled_usd` (what the round settled, to the cent), `started` and `ended` (UTC).

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
  the 69.97 the evaluation store has left. What it is expected to settle is half that, about
  19.04.

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

Pending: written from the TOML's `[format]` after the round, with the clause that settled it.

## Per judge, before and after

Over all ten sets. Submission failures, first rejections and unknowns count every decision of
both repetitions; balanced accuracy, its family-resampled 95% interval and cost per decision
are of repetition 1; self-agreement is the share of items with the same verdict both times.
Each row is written from the TOML's scope `all`.

| Judge | Arm | Decisions | Submission failures | First rejections | Unknown | Balanced accuracy | 95% interval | Cost per decision | Self-agreement |
|---|---|---|---|---|---|---|---|---|---|
| Pending | | | | | | | | | |

## Rejections, causes and recoveries

Each rejection by its first class, the further classes it named, each decision by the cause it
ended by, and each recovery v2 made, by kind, over both repetitions.

| Judge | Arm | Counted | Name | Count |
|---|---|---|---|---|
| Pending | | | | |

## The clauses of each judge

(b) no more submission failures under v2; (c) balanced accuracy not lower, false-reject and
false-accept rates not higher, by more than 2 points (point estimates, repetition 1).

| Judge | v1 failures | v2 failures | No new failures | Balanced accuracy change | False-reject change | False-accept change | Accuracy kept |
|---|---|---|---|---|---|---|---|
| Pending | | | | | | | |

## What this is not

It admits nothing: the sets are dev sets, scored before, and the decision rests on
descriptive evidence. The counts are pooled over three judges; no claim is made of one model
from them. v2 is a bundle (all problems at once, recoveries and the keyed schema), measured
together; the recoveries by kind show which parts were used.
