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

Run on 2026-11-21 and 22 from the binary of commit `edb46f94`, scored by the binary of
commit `65db535a` (`docs/results/e-judges.toml`, which every table below is written from;
`evidence::the_judges_record_holds_only_aggregates` holds the page to it). Both matrices
ended with every cell: `e-judges-dev` 220 completed; `e-judges-blind` 709 completed and 6
failed, each failure counted as an error. The round settled 17.261687 dollars in the
evaluation store (29.056671 counted after it, 11.794984 before), against a forecast of
44.44 and an expectation of about 22.22.

```toml
matrices = ["e-submission", "e-judges-dev", "e-judges-blind"]
matrix_identities = ["c386cd97d9b2d6c5", "b25787b119301f0c", "331ab9bc364b209c"]
arm = "C;submission=v1"
runs_commits = [
  "ac3a49a7b21d0a20e86c48c718eae2bc9e5f833f",
  "edb46f94f11679a20de5bd25a1de6525123bff1b",
]
rule = "judge-pick/1"
outcome = "pick"
commit = "65db535a3171b4f2b52fe119ebf6ba690c18ae14"
pick_all = "gemini-4-argon"
outcome_all = "pick"
pick_source_fidelity = "claude-haiku-5-5"
outcome_source_fidelity = "no_pick"
loop_judge = "gemini-4-argon"
scored_settled_usd = 22.64
round_settled_usd = 17.261687
started = 2026-11-21T21:59:36Z
ended = 2026-11-22T02:21:22Z
```

`runs_commits` are the commits of the binaries that made the scored cells: the format
round's, which made the three small judges' dev cells, and this round's, which made the
rest. Nothing in `whizbang-squint` or `whizbang-backends`, where `check` and the judges'
adapters are, changed between the two, nor `eval matrix` or `eval judges`: what changed is
the playground's host (`whizbang play`), what it needs of the store, and `backends status`.

`commit` is the scoring binary's, as the TOML names it. `pick_all` and
`pick_source_fidelity` are the model each scope's outcome names: the pick, or under no pick
the default kept. `loop_judge` is the judge the loop takes from this round (below,
"The outcome"). `scored_settled_usd` is what the scored decisions settled, the format
round's cells of the arm among them; `round_settled_usd` is what this round's two matrices
settled, from `whizbang spend status` before and after. `started` and `ended` are UTC.

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

- **All sets pooled, the default for `check`: pick, `gemini-4-argon`.** One of the five
  judges is within the margin, and it is the cheapest of those (it is the only one).
  `gpt-6.1-sol` is eligible and its decided correctly is within 5 points of the best, but
  the lower bound of its difference from the best is -0.046, past -0.03; `gemini-3.8-flash`
  and `gpt-6.1-terra` fail both that bound and the decided-correctly clause;
  `claude-haiku-5-5` is not eligible (7 errors of 253 decisions, 2.8%; decided correctly
  0.700).
- **The `source-fidelity` sets, the loop's scope: no pick, so the rule keeps the current
  default, `claude-haiku-5-5`.** No judge is within the margin. `gemini-4-argon` and
  `gpt-6.1-sol` are tied for best (0.964 balanced accuracy each), and the lower bounds of
  their differences from the best, taken again inside each resample, are -0.038 and -0.035:
  just past -0.03. `gpt-6.1-terra` is at -0.076. `gemini-3.8-flash` is not eligible (5
  errors of 165, 3.0%), nor is `claude-haiku-5-5` (decided correctly 0.794).
- **The loop's judge: `gemini-4-argon`.** The plan (task J1, item 4) gives the loop the
  same model as `check` "unless the source-fidelity scope picks another". A no pick is not
  a pick of another judge: it picks none. So the loop's judge is the pick over all sets,
  `gemini-4-argon`, and the loop's round (task L2) runs under it, with `claude-haiku-5-5`
  beside it as the plan says. The default the no pick names is a judge the rule found
  ineligible in that very scope, so it is not read as a pick.

## The five judges

Over all 253 items, repetition 1. Balanced accuracy and decided correctly with their
family-resampled 95% intervals; submission failures and first rejections over every scored
decision (the small judges' include the format round's repetition 2 of the arm), with their
rate; cost per decision settled; a loop attempt's share is the check's cost over the check
and one fill (0.04 dollars); self-agreement where there is a repetition 2. Each row is
written from the TOML's scope `all`.

| Judge | Decisions | Balanced accuracy | Decided correctly | Unsure | Errors | Submission failures | First rejections | Cost per decision | Per 1,000 checks | Share of a loop attempt | Median time (ms) | Self-agreement |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| claude-haiku-5-5 | 253 | 0.690 (0.622 to 0.751) | 0.700 (0.635 to 0.758) | 60 | 7 | 14 (0.039) | 104 (0.287) | 0.003517 | 3.52 | 0.081 | 15428 | 0.809 |
| gemini-3.8-flash | 253 | 0.871 (0.832 to 0.910) | 0.870 (0.831 to 0.909) | 0 | 5 | 0 (0.000) | 19 (0.052) | 0.016589 | 16.59 | 0.293 | 29026 | 0.955 |
| gemini-4-argon | 253 | 0.924 (0.879 to 0.960) | 0.929 (0.886 to 0.963) | 0 | 0 | 0 (0.000) | 16 (0.063) | 0.028513 | 28.51 | 0.416 | 32218 | n/a |
| gpt-6.1-sol | 253 | 0.911 (0.867 to 0.949) | 0.917 (0.876 to 0.952) | 4 | 0 | 0 (0.000) | 16 (0.063) | 0.021164 | 21.16 | 0.346 | 22248 | n/a |
| gpt-6.1-terra | 253 | 0.861 (0.805 to 0.911) | 0.870 (0.816 to 0.915) | 11 | 0 | 0 (0.000) | 1 (0.003) | 0.010494 | 10.49 | 0.208 | 19367 | 0.945 |

## By scope

The dev sets pooled, `blind-v3` alone, the `source-fidelity` sets pooled, and each use case.
Each row is written from the TOML's scopes other than `all`.

| Scope | Judge | Decisions | Balanced accuracy | Decided correctly | Unsure | Errors | Cost per decision |
|---|---|---|---|---|---|---|---|
| dev | claude-haiku-5-5 | 110 | 0.533 (0.437 to 0.633) | 0.582 (0.491 to 0.673) | 34 | 6 | 0.003294 |
| dev | gemini-3.8-flash | 110 | 0.880 (0.810 to 0.946) | 0.891 (0.823 to 0.953) | 0 | 0 | 0.013444 |
| dev | gemini-4-argon | 110 | 0.844 (0.756 to 0.925) | 0.873 (0.797 to 0.939) | 0 | 0 | 0.026597 |
| dev | gpt-6.1-sol | 110 | 0.829 (0.745 to 0.909) | 0.855 (0.778 to 0.925) | 3 | 0 | 0.018899 |
| dev | gpt-6.1-terra | 110 | 0.755 (0.653 to 0.849) | 0.791 (0.696 to 0.875) | 11 | 0 | 0.009324 |
| blind | claude-haiku-5-5 | 143 | 0.791 (0.717 to 0.860) | 0.790 (0.716 to 0.860) | 26 | 1 | 0.003687 |
| blind | gemini-3.8-flash | 143 | 0.853 (0.803 to 0.902) | 0.853 (0.804 to 0.902) | 0 | 5 | 0.019007 |
| blind | gemini-4-argon | 143 | 0.972 (0.937 to 1.000) | 0.972 (0.937 to 1.000) | 0 | 0 | 0.029987 |
| blind | gpt-6.1-sol | 143 | 0.965 (0.938 to 0.993) | 0.965 (0.937 to 0.993) | 1 | 0 | 0.022906 |
| blind | gpt-6.1-terra | 143 | 0.930 (0.882 to 0.972) | 0.930 (0.881 to 0.972) | 0 | 0 | 0.011395 |
| brief-compliance | claude-haiku-5-5 | 22 | 0.604 (0.429 to 0.802) | 0.727 (0.591 to 0.864) | 2 | 2 | 0.002864 |
| brief-compliance | gemini-3.8-flash | 22 | 0.833 (0.667 to 1.000) | 0.909 (0.818 to 1.000) | 0 | 0 | 0.010375 |
| brief-compliance | gemini-4-argon | 22 | 0.667 (0.500 to 0.833) | 0.818 (0.700 to 0.917) | 0 | 0 | 0.021304 |
| brief-compliance | gpt-6.1-sol | 22 | 0.583 (0.500 to 0.750) | 0.773 (0.667 to 0.875) | 0 | 0 | 0.014317 |
| brief-compliance | gpt-6.1-terra | 22 | 0.583 (0.500 to 0.750) | 0.773 (0.667 to 0.875) | 0 | 0 | 0.007215 |
| product-fidelity | claude-haiku-5-5 | 22 | 0.406 (0.344 to 0.472) | 0.591 (0.500 to 0.700) | 5 | 4 | 0.003648 |
| product-fidelity | gemini-3.8-flash | 22 | 1.000 (1.000 to 1.000) | 1.000 (1.000 to 1.000) | 0 | 0 | 0.012362 |
| product-fidelity | gemini-4-argon | 22 | 1.000 (1.000 to 1.000) | 1.000 (1.000 to 1.000) | 0 | 0 | 0.026514 |
| product-fidelity | gpt-6.1-sol | 22 | 0.917 (0.750 to 1.000) | 0.955 (0.864 to 1.000) | 0 | 0 | 0.020895 |
| product-fidelity | gpt-6.1-terra | 22 | 0.802 (0.625 to 0.972) | 0.864 (0.722 to 0.958) | 1 | 0 | 0.011030 |
| reality-checks | claude-haiku-5-5 | 22 | 0.227 (0.045 to 0.409) | 0.227 (0.045 to 0.409) | 16 | 0 | 0.002771 |
| reality-checks | gemini-3.8-flash | 22 | 0.727 (0.545 to 0.909) | 0.727 (0.545 to 0.909) | 0 | 0 | 0.012805 |
| reality-checks | gemini-4-argon | 22 | 0.636 (0.500 to 0.818) | 0.636 (0.500 to 0.818) | 0 | 0 | 0.024958 |
| reality-checks | gpt-6.1-sol | 22 | 0.682 (0.450 to 0.909) | 0.682 (0.450 to 0.909) | 1 | 0 | 0.013422 |
| reality-checks | gpt-6.1-terra | 22 | 0.591 (0.292 to 0.850) | 0.591 (0.292 to 0.850) | 5 | 0 | 0.006942 |
| reference-consistency | claude-haiku-5-5 | 22 | 0.545 (0.500 to 0.636) | 0.545 (0.500 to 0.636) | 10 | 0 | 0.004393 |
| reference-consistency | gemini-3.8-flash | 22 | 0.955 (0.864 to 1.000) | 0.955 (0.864 to 1.000) | 0 | 0 | 0.017077 |
| reference-consistency | gemini-4-argon | 22 | 1.000 (1.000 to 1.000) | 1.000 (1.000 to 1.000) | 0 | 0 | 0.031929 |
| reference-consistency | gpt-6.1-sol | 22 | 0.909 (0.778 to 1.000) | 0.909 (0.778 to 1.000) | 1 | 0 | 0.026878 |
| reference-consistency | gpt-6.1-terra | 22 | 0.773 (0.583 to 0.955) | 0.773 (0.583 to 0.955) | 4 | 0 | 0.012551 |
| source-fidelity | claude-haiku-5-5 | 165 | 0.794 (0.730 to 0.854) | 0.794 (0.730 to 0.854) | 27 | 1 | 0.003569 |
| source-fidelity | gemini-3.8-flash | 165 | 0.854 (0.810 to 0.899) | 0.855 (0.810 to 0.899) | 0 | 5 | 0.018420 |
| source-fidelity | gemini-4-argon | 165 | 0.964 (0.929 to 0.988) | 0.964 (0.929 to 0.988) | 0 | 0 | 0.029759 |
| source-fidelity | gpt-6.1-sol | 165 | 0.964 (0.937 to 0.988) | 0.964 (0.937 to 0.988) | 2 | 0 | 0.022383 |
| source-fidelity | gpt-6.1-terra | 165 | 0.934 (0.890 to 0.971) | 0.933 (0.890 to 0.971) | 1 | 0 | 0.011060 |

## Rejections and causes

Each rejection by its first class, and each decision by the cause it ended by, over all
sets.

| Judge | Counted | Name | Count |
|---|---|---|---|
| claude-haiku-5-5 | class | bad_field | 3 |
| claude-haiku-5-5 | class | evidence | 54 |
| claude-haiku-5-5 | class | finding_cites | 46 |
| claude-haiku-5-5 | class | location | 3 |
| claude-haiku-5-5 | class | missing_item | 10 |
| claude-haiku-5-5 | class | not_a_list | 2 |
| claude-haiku-5-5 | cause | concluded | 348 |
| claude-haiku-5-5 | cause | cut_off | 1 |
| claude-haiku-5-5 | cause | invalid_submission | 14 |
| gemini-3.8-flash | class | bad_field | 1 |
| gemini-3.8-flash | class | finding_cites | 1 |
| gemini-3.8-flash | class | location | 17 |
| gemini-3.8-flash | cause | concluded | 357 |
| gemini-3.8-flash | cause | fault | 6 |
| gemini-4-argon | class | location | 16 |
| gemini-4-argon | cause | concluded | 253 |
| gpt-6.1-sol | class | finding_cites | 2 |
| gpt-6.1-sol | class | no_submission | 14 |
| gpt-6.1-sol | cause | concluded | 253 |
| gpt-6.1-terra | class | finding_cites | 1 |
| gpt-6.1-terra | cause | concluded | 363 |

## The clauses of each judge

`judge-pick/1`, under each scope: errors under 2% of decisions; decided correctly at least
0.80; eligible; the lower bound of the 95% interval of its balanced accuracy minus the best
eligible judge's, the best taken again inside each resample; that bound at least -0.03; its
median time at most twice the fastest eligible judge's; its decided correctly within 0.05 of
the best's; within the margin.

| Scope | Judge | Errors under 2% | Decided correctly at least 0.80 | Eligible | Lower bound of the difference | At least -0.03 | Median time | Decided correctly | Within the margin | Cost per decision |
|---|---|---|---|---|---|---|---|---|---|---|
| all | claude-haiku-5-5 | no | no | no | n/a | no | no | no | no | 0.003517 |
| all | gemini-3.8-flash | yes | yes | yes | -0.090 | no | yes | no | no | 0.016589 |
| all | gemini-4-argon | yes | yes | yes | -0.028 | yes | yes | yes | yes | 0.028513 |
| all | gpt-6.1-sol | yes | yes | yes | -0.046 | no | yes | yes | no | 0.021164 |
| all | gpt-6.1-terra | yes | yes | yes | -0.109 | no | yes | no | no | 0.010494 |
| source-fidelity | claude-haiku-5-5 | yes | no | no | n/a | no | no | no | no | 0.003569 |
| source-fidelity | gemini-3.8-flash | no | yes | no | n/a | no | no | no | no | 0.018420 |
| source-fidelity | gemini-4-argon | yes | yes | yes | -0.038 | no | yes | yes | no | 0.029759 |
| source-fidelity | gpt-6.1-sol | yes | yes | yes | -0.035 | no | yes | yes | no | 0.022383 |
| source-fidelity | gpt-6.1-terra | yes | yes | yes | -0.076 | no | yes | yes | no | 0.011060 |

## The trade-off

`claude-haiku-5-5`, the default until now, is ineligible in both scopes. Over all sets its
6 errors on the dev sets alone, from the format round's cells, are more than 2% of its 253
decisions (7 in all), and it decides correctly 0.700; over `source-fidelity` it decides
correctly 0.794, under the 0.80 the rule asks. So where the rule finds no pick, the
default it keeps is a judge it found ineligible, and nothing here recommends it on its
accuracy. What it gives up against `gemini-4-argon` over all sets is 23.4 points of
balanced accuracy (0.690 against 0.924) and 22.9 of decided correctly (0.700 against
0.929); what it saves is about 25 dollars per 1,000 checks (3.52 against 28.51): Argon
costs about 8 times as much a decision.

Among the eligible judges the cheaper ones give up little: `gpt-6.1-sol` is 1.3 points of
balanced accuracy behind Argon over all sets (0.911 against 0.924) and tied with it over
`source-fidelity`, for 21.16 dollars per 1,000 checks against 28.51; it misses the margin
by its interval, not its point. `gpt-6.1-terra` is 6.3 points behind for 10.49 dollars per
1,000. Argon's lead is `blind-v3`'s: on the dev sets alone its balanced accuracy is 0.844,
behind `gemini-3.8-flash`'s 0.880, and its false-reject rate there is 0.311, the highest of
the five, most of it under `reality-checks` and `brief-compliance`. Since
`blind-v3` is 143 of the 253 items, the pooled scope leans on one exposed source-fidelity
set.

Argon's cost is at its introductory price, which ends on 2026-11-30. After that date its
row is read again from the provider's page, changed in its own pull request, and its
cost a decision is expected to roughly double, to about 57 dollars per 1,000 checks. A
price change of more than half chooses the judge again (ADR 0044).

## What this is not

It admits nothing: the dev sets were scored before, `blind-v3` is exposed and its roughly 40
items an agent saw count as dev, and the pick rests on descriptive evidence. The small
judges' dev cells are the format round's, of the same arm and binary's format; the other
cells are this round's. The pick is taken again when a judge is added, a price changes by
more than half, the format's default changes, or a later slice gates on fresh data (ADR
0044).
