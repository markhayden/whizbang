# source-fidelity

The tool record of the use case `source-fidelity` as a recipe (`SPEC.md` section 8; plan m3,
task Q1, item 5). The recipe is `crates/whizbang-squint/recipes/source-fidelity.toml`. The
example pictures and the table between the marks are written by `scripts/tool-record.sh
source-fidelity`, and the fast tests fail when they stop being what the recipe gives
(`crates/whizbang-squint/tests/it/use_case_records.rs`).

## What it does

Checks whether an edit kept what mattered in its source, with squint's rules for the use
case (`crates/whizbang-squint/rules/source-fidelity.toml`). Two steps, each a run of its own
under the recipe's run:

1. `details`: measures what can be measured before the judge looks. Here that is `align` on
   the candidate and each other image, in the order given; a pair that registers hands its
   aligned frame on to the judge, one that does not hands back its failure and no frame.
2. `check`: the judge's loop under the use case's rules, told what `details` measured and
   handed its images, offered `view`, `align` and `resize`, with the second look off. It
   answers with a verdict, `pass`, `fail` or `unsure`, which the recipe hands back with its
   outcome (ADR 0035).

- **Use it** to hold an edited picture to the picture it was edited from: the candidate
  first, then its one source.
- **Do not use it** on a picture with no source (`reality-checks`), or to hold a subject to
  pictures of it taken apart (`reference-consistency`).
- **Kind.** A judge's tool (`SPEC.md` section 8): it calls a model, so rungs 1 and 2 apply
  and it climbs rungs 3 and 4 with milestone E. Each run spends what the judge's turns cost.

```
whizbang squint check source-fidelity --candidate <id> --source <id> \
  --store <dir> --config whizbang.toml --env-file whizbang.env
```

| Name | Meaning |
|---|---|
| `images` (input) | two: the candidate, then its source |
| `job` | The job, as `squint check` writes it from its arguments |
| `judge` | Optional. The judge to ask, by its name in the config |

The command's flags make the job (`docs/guides/config.md`, "Checking an edit with a judge");
a flag the recipe does not set by itself becomes an override of a step (`--second-look on`
sets `judge.second_look`).

The images a recipe's steps make (the aligned frames, the judge's crops) are names of the
recipe's run: its own calls read them, and a run of another call cannot take them as inputs
(plan m3, task Q1, item 3).

## Examples

Two worked examples on synthetic drawings made for the tests of `align` (`fixtures/align/`),
under a scripted judge: the verdict is what the use case's rules make of the judge's
answers, not a model's opinion. The judge answers every item of the checklist held,
confidently, but for the last, which it answers as the table says.

| Candidate, first example | Candidate, second example | Source or reference |
|---|---|---|
| ![The drawn scene, registered on another canvas](source-fidelity/registered.png) | ![An unrelated drawing](source-fidelity/unrelated.png) | ![The drawn scene](source-fidelity/scene.png) |

<!-- measured: begin -->
| Example | Candidate | Images | Calls of `details` | Images handed to the judge | The judge's answer | Verdict |
|---|---|---|---|---|---|---|
| held | registered | 2 | `align` completed | 1 | `chk:ob:SF2@candidate` held | pass |
| breached | unrelated | 2 | `align` completed | 0 | `chk:ob:SF2@candidate` violation | fail |
<!-- measured: end -->

What to look for:

- In the first example the candidate registers against the scene: `align` completes and its
  aligned frame is handed to the judge's step. The judge holds every item: `pass`.
- In the second the drawing does not register: `align` completes with its failure as its
  measurements and hands on no frame, so no comparison runs and the judge is told the pair
  did not register. The judge finds the last item breached: `fail`.

## Rung 2 results

Credited, not measured anew. The recipe runs the steps `check` ran in milestone 2, and gives
the verdict `check` gave for every answer of the judge
(`check::each_use_case_through_its_recipe_gives_the_verdict_it_gave_before`); that test is
what the credit rests on. Every figure below is of `check` before the recipe existed, on published
sets (dev and adversarial), which admit nothing.

**Milestone 2's evaluation** (`docs/results/m2c-evaluation.md`): arm `F`, details first,
`claude-haiku-5-5`, two repetitions, on the published sets, run once at commit
`6f155085bdb77e6f43c87676f4783763ddd21e19` (E1) and again after task F1 fixed the output
budget. Counts are runs; `agreeing` is a completed run whose verdict is the item's label.

| Set | Run | Runs | Completed | Failed | Pass, fail, unsure | Agreeing |
|---|---|---|---|---|---|---|
| `source-fidelity/scenes-v1` | E1 | 40 | 37 | 3 | 15, 16, 6 | 29 |
| `source-fidelity/scenes-v1` | F1 | 40 | 40 | 0 | not kept by set | 33 |
| `source-fidelity/adversarial-v1` | F1 | 4 | 4 | 0 | not kept by set | 2 |

**The gate of details first** (`docs/results/m3-gates.md`, plan m3, task G1): arm `F`
against arm `C`, squint's control, over the five dev sets together, 100 items. Rejected: for
`claude-haiku-5-5` balanced accuracy 0.567 under `C` and 0.550 under `F`; no judge's gain
reached the 3 points the gate asks. The receipt does not break the figures down by use case.
The recipe measures details first all the same, as `check` did.

## Speed and cost

The examples above take well under a second each with the scripted judge. With a real judge
a run is the judge's turns: in F1's rerun of the evaluation, over all five use cases,
`check` took 15,686 ms at the median and 29,381 ms at the 95th percentile, and the 220
decisions settled 0.809051 dollars, about 0.0037 dollars each, on `claude-haiku-5-5`.
`align` took 400 ms at the median and 985 ms at the 95th percentile in E1's run (172 runs).

## Alternatives

- **`check` called with `use_case = source-fidelity`**: the same verdict from the same
  requests (`check::each_use_case_through_its_recipe_gives_the_verdict_it_gave_before`); it
  measures the same details inside one run, not in steps of their own.
- **Arm `C`, without details first**: squint's control; the gate above found no gain from
  the details on the dev sets.
- **The second look on**: `--second-look on` (plan m3, task U1).

## Limits

| Limit | Example |
|---|---|
| Rung 2 is credited from `check`'s runs, not run through the recipe | "Rung 2 results" |
| Every figure is on published sets, with one judge for the by-set numbers; nothing here is held out | `source-fidelity/scenes-v1` |
| About a third of milestone 2's verdicts were `unsure` | E1's row above: 6 of 37 |
| A judge that breaks a submission's rules twice ends the run with no verdict | F1's failed runs |
| The recipe's live record (item 7 of task Q1) is the orchestrator's, still to come | `docs/results/m2c-use-cases.md` |

## Mark and history

**Unverified** (`SPEC.md` section 8): its record, with rungs 1 and 2; no gate cleared.

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests |
| 2. Scored alone | Credited | Milestone 2's evaluation of `check` on `source-fidelity/scenes-v1` and `source-fidelity/adversarial-v1` |
| 3 and 4 | Not yet | Milestone E |

**Gates.** The gate of details first (`gates/details-first.toml`): rejected for all five use
cases together.

**History.**

- Milestone 2 (`docs/plans/m2a-first-slices.md`): the first use case of `check`, ported from
  squint; its first live verdict is `docs/results/m2a-first-verdict.md`.
- Plan m3, task Q1: a recipe of two steps (#210); this record (its third pull request).
