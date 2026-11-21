# The judge round: the default judge

Plan e1, task J1. The five judges this build prices, `check` under arm C and the format the
format round settled (`C;submission=v1`: `submission/1` did not adopt v2,
`docs/results/e-submission.md`), on the same 253 items: the ten dev sets (110 items, 30
families) and `source-fidelity/blind-v3` (143 items, 24 families). Picked by the rule
`judge-pick/1`, frozen in the plan before the run and applied in code by `whizbang eval
judges`, over all sets pooled (the default for `check`) and over the `source-fidelity` sets
alone (the loop's judge). Only aggregates are kept: counts, rates and intervals by judge and
scope, never an item, a family, a run, a verdict of an item, a label of `blind-v3` or a word
of a judge.

The evidence is descriptive and admits nothing. The dev sets were scored before, and
`blind-v3` is an exposed regression set: about 40 of its items were seen by an agent, so
they count as dev, and the set never admits a tool (`SPEC.md` section 8). The gate on fresh
data that would admit a default judge is owed to a later slice of E.

## The round

The orchestrator fills this block after the run; `outcome` stays `pending` until then, and
`evidence::the_judges_record_holds_only_aggregates` holds the page to the matrices as
committed and, once the outcome is written, to `docs/results/e-judges.toml`: the rows of
the tables below are the ones that test prints when the page lacks them, each written from
the TOML, and replace the `Pending` rows.

```toml
matrices = ["e-submission", "e-judges-dev", "e-judges-blind"]
matrix_identities = ["c386cd97d9b2d6c5", "b25787b119301f0c", "331ab9bc364b209c"]
arm = "C;submission=v1"
rule = "judge-pick/1"
outcome = "pending"
```

Once run, the block also gives `commit` (the commit the binary was built from, as the TOML
names it), `pick_all` and `pick_source_fidelity` (the model each scope's outcome names: the
pick, or under no pick the default kept), `outcome_all` and `outcome_source_fidelity`
(`pick` or `no_pick`), `scored_settled_usd` (what the scored decisions settled, the format
round's cells of the arm among them, to the cent), `round_settled_usd` (what this round's
two matrices settled, from `whizbang spend status` before and after), and `started` and
`ended` (UTC).

## How it is run

Three matrices, scored together under the one arm `C;submission=v1`:

- `matrices/e-submission.toml`, the format round's, already run: its cells of the arm
  `C;submission=v1`, repetition 1, give `claude-haiku-5-5`, `gemini-3-8-flash` and
  `gpt-6-1-terra` on the ten dev sets. Its cells of v2 and of repetition 2 are read as
  ended, for the completeness check, and not scored here: `--mode judges --arm` scores
  every judge under that one arm, and selection uses repetition 1 only.
- `matrices/e-judges-dev.toml`: the ten dev sets, the arm, `gemini-4-argon` and
  `gpt-6-1-sol`, one repetition: 220 cells.
- `matrices/e-judges-blind.toml`: `source-fidelity/blind-v3`, the arm, all five judges, one
  repetition: 715 cells.

`matrix::the_judge_round_is_220_and_715_cells_forecast_within_its_store_and_capped_by_its_dearest_check`
holds the counts, the forecasts and the cap below to the build. With the format round's
1,320, `eval judges` reads 2,255 cells. The selection measures use 1,265 of them, repetition
1 under the arm (253 items under five judges); the submission measures also count the
format round's repetition 2 of the arm, 330 more, of the three small judges.

- **Prices** (plan e1, "Prices and caps"), checked on 2026-11-21: every row was read on
  2026-10-26. `claude-haiku-5-5`, `gpt-6.1-terra` and `gpt-6.1-sol` carry no end date;
  `gemini-3.8-flash`'s introductory price holds to 2026-12-31, and `gemini-4-argon`'s to
  2026-11-30 (ADR 0026; the carry-over row on the Google prices). No row is past its date.
  At two runs at once the round takes about five hours (the dev matrix about 40 minutes to
  an hour, the blind one about 3.5 to 4.5 hours: the format round's 1,320 cells took 3 h 46
  min, and the tool study's median times on `blind-v3` put the five judges' checks of an
  item at about three minutes together), so it is started no later than 2026-11-29 and
  ended before 2026-11-30 begins. A round that would cross that date waits
  for the row to be read again, changed in its own pull request, and run on a binary with
  the change.
- **The cap of one call**: the largest bound of one check among the five judges, plus a
  cent. A check reserves sixteen requests at the judge's ceilings (40,000 input and 4,096
  output tokens): 0.112768 dollars for Haiku, 0.725760 for Flash, 0.967680 for Terra, and
  1.935360 for Argon and for Sol (40,000 at 2.00 and 4,096 at 10.00 a million, 0.12096 a
  request). So `call_cap_usd = 1.94536`, set in a copy of `labs/e-loop/config.toml` outside
  the repo; the committed file keeps its own cap. The copy also approves
  `https://raw.githubusercontent.com` as a recipient with `classes = []`, so `eval sets
  fetch` may fetch `blind-v3`'s pictures by their pins; it is sent nothing but each
  picture's address.
- **The forecasts** `eval matrix` sets against what the evaluation store has left (58.20
  dollars, after the format round settled 11.79): each cell at its model's settled cost of
  a decision under arm C, the whole doubled. `e-judges-dev`: 13.940740 dollars. Then
  `e-judges-blind`: 30.500470, against at least 44.26 left even had the first settled its
  whole forecast. What the two are expected to settle is half that, about 22.22 (about 29.7
  were Argon's price to double).

```
whizbang eval sets fetch --set source-fidelity/blind-v3 --store <EVAL_STORE> \
    --config <CONFIG> --env-file <ENV_FILE>
whizbang spend status --store <EVAL_STORE>
whizbang eval matrix --matrix matrices/e-judges-dev.toml --store <EVAL_STORE> \
    --config <CONFIG> --env-file <ENV_FILE> --models-dir <MODELS_DIR>
whizbang spend status --store <EVAL_STORE>
whizbang eval matrix --matrix matrices/e-judges-blind.toml --store <EVAL_STORE> \
    --config <CONFIG> --env-file <ENV_FILE> --models-dir <MODELS_DIR>
whizbang spend status --store <EVAL_STORE>
whizbang eval judges --mode judges --arm "C;submission=v1" --rule judge-pick/1 \
    --matrix matrices/e-submission.toml --matrix matrices/e-judges-dev.toml \
    --matrix matrices/e-judges-blind.toml --store <EVAL_STORE> --labels <LABELS_DIR> \
    --out docs/results/e-judges.toml
```

`--labels` names a folder outside `evalsets/` laid out as the sets are; it must exist. Only
`blind-v3`'s labels are read from it, as `<LABELS_DIR>/source-fidelity/blind-v3/labels.toml`,
whose SHA-256 must be the one the set pins; the ten dev sets publish their own. Failed cells
are not run again: each counts as an error, and the 2% clause sees it.

## The outcome

Pending: written from the TOML's `[[picks]]` after the round, for both scopes, with why.

## The five judges

Over all 253 items, repetition 1. Balanced accuracy and decided correctly with their
family-resampled 95% intervals; submission failures and first rejections over every scored
decision (the small judges' include the format round's repetition 2 of the arm), with their
rate; cost per decision settled; a loop attempt's share is the check's cost over the check
and one fill (0.04 dollars); self-agreement where there is a repetition 2. Each row is
written from the TOML's scope `all`.

| Judge | Decisions | Balanced accuracy | Decided correctly | Unsure | Errors | Submission failures | First rejections | Cost per decision | Per 1,000 checks | Share of a loop attempt | Median time (ms) | Self-agreement |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Pending | | | | | | | | | | | | |

## By scope

The dev sets pooled, `blind-v3` alone, the `source-fidelity` sets pooled, and each use case.
Each row is written from the TOML's scopes other than `all`.

| Scope | Judge | Decisions | Balanced accuracy | Decided correctly | Unsure | Errors | Cost per decision |
|---|---|---|---|---|---|---|---|
| Pending | | | | | | | |

## Rejections and causes

Each rejection by its first class, and each decision by the cause it ended by, over all
sets.

| Judge | Counted | Name | Count |
|---|---|---|---|
| Pending | | | |

## The clauses of each judge

`judge-pick/1`, under each scope: errors under 2% of decisions; decided correctly at least
0.80; eligible; the lower bound of the 95% interval of its balanced accuracy minus the best
eligible judge's, the best taken again inside each resample; that bound at least -0.03; its
median time at most twice the fastest eligible judge's; its decided correctly within 0.05 of
the best's; within the margin.

| Scope | Judge | Errors under 2% | Decided correctly at least 0.80 | Eligible | Lower bound of the difference | At least -0.03 | Median time | Decided correctly | Within the margin | Cost per decision |
|---|---|---|---|---|---|---|---|---|---|---|
| Pending | | | | | | | | | | |

## The trade-off

Pending: what the cheaper choice gives up, in points, and what the dearer one costs, in
dollars per 1,000 checks.

## What this is not

It admits nothing: the dev sets were scored before, `blind-v3` is exposed and its roughly 40
items an agent saw count as dev, and the pick rests on descriptive evidence. The small
judges' dev cells are the format round's, of the same arm and binary's format; the other
cells are this round's. The pick is taken again when a judge is added, a price changes by
more than half, the format's default changes, or a later slice gates on fresh data (ADR
0044).
