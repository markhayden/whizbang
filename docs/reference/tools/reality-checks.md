# reality-checks

The tool record of the use case `reality-checks` as a recipe (`SPEC.md` section 8; plan m3,
task Q1, item 5). The recipe is `crates/whizbang-squint/recipes/reality-checks.toml`. The
example pictures and the table between the marks are written by `scripts/tool-record.sh
reality-checks`, and the fast tests fail when they stop being what the recipe gives
(`crates/whizbang-squint/tests/it/use_case_records.rs`).

## What it does

Checks a picture for the mistakes no real photograph would make, with squint's rules for the
use case (`crates/whizbang-squint/rules/reality-checks.toml`). Two steps, each a run of its
own under the recipe's run:

1. `details`: measures what can be measured before the judge looks. As shipped that is
   nothing: details first is off by default ("Details first is off by default", below), so
   the step starts no tool and hands on nothing. With `--details first` it is still nothing
   here: with one image there is no pair to align, and the judge is given no measured image.
2. `check`: the judge's loop under the use case's rules, offered `view` and `align`, a call
   of `align` answered with its pictures alone (`visual_only`), with the second look off. It
   answers with a verdict, `pass`, `fail` or `unsure`, which the recipe hands back with its
   outcome (ADR 0035).

- **Use it** to look for the marks of a made or edited picture with no other picture to hold
  it to.
- **Do not use it** when a source or references are at hand: the use cases that hold a
  picture to another see more.
- **Kind.** A judge's tool (`SPEC.md` section 8): it calls a model, so rungs 1 and 2 apply
  and it climbs rungs 3 and 4 with milestone E. Each run spends what the judge's turns cost.

```
whizbang squint check reality-checks --candidate <id> \
  --store <dir> --config whizbang.toml --env-file whizbang.env
```

| Name | Meaning |
|---|---|
| `images` (input) | one: the candidate |
| `job` | The job, as `squint check` writes it from its arguments |
| `judge` | Optional. The judge to ask, by its name in the config |

The command's flags make the job (`docs/guides/config.md`, "Checking an edit with a judge");
what is measured first, the tools offered, the projection and the second look are stated
as overrides of the steps on every call (`--second-look on` sets `judge.second_look`), so
the command does not lean on what the recipe's file says.

The judge answers in submission v1, `check`'s default (`submission = "v1"` in the judge's
step). `--submission v2` sets `judge.submission=v2`: the items keyed by checklist id,
near-misses with one reading read and recorded, and every problem named in the one repair
(ADR 0042). v2 is experimental and no default uses it.

The images a recipe's steps make (the aligned frames, the judge's crops) are names of the
recipe's run: its own calls read them, and a run of another call cannot take them as inputs
(plan m3, task Q1, item 3).

**Details first is off by default.** Since plan m4, task A1, the recipe as shipped,
`whizbang squint check` with nothing set and the server's `check` are the control arm of the
gate of details first, whole (arm `C`): nothing is measured before the judge's first turn,
the judge is offered `view` and `align`, and a call of `align` is answered with its pictures
and a fixed sentence (`visual_only`). The gate of details first (`gates/details-first.toml`)
rejected details first in every run of 2026-11-19; the run to read is the fourth, the first
complete one, where it helped on none of the three judges (`docs/results/m3-gates.md`, "The
fourth run, the complete one").

Nothing of details first is removed. To turn it on, whole, as the gate's arm `F` was:

```
whizbang squint check reality-checks --details first ...
```

That measures the details first, offers `resize` beside `view` and `align`, and answers a
call of `align` in `full`. Over the server, `check` takes `"details": "first"`. A call of
the recipe itself sets the three together: `details.details=first`,
`judge.tools=view,align,resize` and `judge.projection=full`.

## Examples

Two worked examples on synthetic drawings made for the tests of `align` (`fixtures/align/`),
through the recipe as shipped, with nothing set, under a scripted judge: the verdict is what
the use case's rules make of the judge's answers, not a model's opinion. The judge answers
every item of the checklist held, confidently, but for the last, which it answers as the
table says.

| Candidate, first example | Candidate, second example |
|---|---|
| ![The drawn scene, registered on another canvas](reality-checks/registered.png) | ![An unrelated drawing](reality-checks/unrelated.png) |

<!-- measured: begin -->
| Example | Candidate | Images | Calls of `details` | Images handed to the judge | Tools offered to the judge | The judge's answer | Verdict |
|---|---|---|---|---|---|---|---|
| held | registered | 1 | none | 0 | `view`, `align` | `chk:ob:RE4@candidate` held | pass |
| breached | unrelated | 1 | none | 0 | `view`, `align` | `chk:ob:RE4@candidate` violation | fail |
<!-- measured: end -->

What to look for:

- `details` makes no call and hands on nothing in either example: nothing is measured
  first, and with one image there would be no pair to align under `--details first` either.
  The judge is offered `view` and `align`, and not `resize`.
- The judge holds every item in the first: `pass`. It finds the last item breached,
  confidently, in the second: `fail`. A breach answered tentatively would be `unsure`
  (`use_case::a_confident_must_not_violation_fails_and_a_tentative_one_is_unsure`).

## Rung 2 results

Credited, not measured anew. With details first set, the recipe runs the steps `check` ran
in milestone 2, and gives the verdict `check` gave for every answer of the judge
(`check::each_use_case_through_its_recipe_gives_the_verdict_it_gave_before`); that test is
what the credit rests on. The figures of milestone 2 below are of arm `F`, which is no
longer the default; the gate's figures are of both arms. Every figure below is of `check`
before the recipe existed, on published sets (dev and adversarial), which admit nothing.

**Milestone 2's evaluation** (`docs/results/m2c-evaluation.md`): arm `F`, details first,
`claude-haiku-5-5`, two repetitions, on the published sets, run once at commit
`6f155085bdb77e6f43c87676f4783763ddd21e19` (E1) and again after task F1 fixed the output
budget. Counts are runs; `agreeing` is a completed run whose verdict is the item's label.

| Set | Run | Runs | Completed | Failed | Pass, fail, unsure | Agreeing |
|---|---|---|---|---|---|---|
| `reality-checks/people-v1` | E1 | 40 | 39 | 1 | 0, 11, 28 | 9 |
| `reality-checks/people-v1` | F1 | 40 | 40 | 0 | not kept by set | 12 |
| `reality-checks/adversarial-v1` | F1 | 4 | 4 | 0 | not kept by set | 0 |

Task U1's screen (`docs/results/m3-unsure.md`, commit
`6af40ae1ae343a08a2e68720cc287b5af4480812`, `claude-haiku-5-5`, three repetitions on the use
case's two sets, `people-v1` and `adversarial-v1`): arm `F` agreed on 15 of 66 decisions
(22.7%), 74.2% unsure, balanced accuracy 0.227 [0.083, 0.361]; with the tiles 20 of 66
(30.3%); with the second look 38 of 66 (57.6%) but 6 of 33 false accepts, against none under
`F`. That is why this recipe keeps the second look off (its own comment says so).

**The gate of details first** (`docs/results/m3-gates.md`, plan m3, task G1): arm `F`
against arm `C`, squint's control, over the five dev sets together, 100 items. Rejected: for
`claude-haiku-5-5` balanced accuracy 0.567 under `C` and 0.550 under `F`; no judge's gain
reached the 3 points the gate asks. The receipt does not break the figures down by use case.
Run a fourth time, complete, every judge answering, it was rejected again: balanced accuracy
0.529 under `C` and 0.483 under `F` for `claude-haiku-5-5`, 0.887 under both for
`gemini-3.8-flash`, 0.725 and 0.738 for `gpt-6.1-terra`. So the recipe no longer measures
details first unless a run asks for it ("Details first is off by default").

## Speed and cost

The examples above take well under a second each with the scripted judge. With a real judge
a run is the judge's turns: in F1's rerun of the evaluation, over all five use cases,
`check` took 15,686 ms at the median and 29,381 ms at the 95th percentile, and the 220
decisions settled 0.809051 dollars, about 0.0037 dollars each, on `claude-haiku-5-5`.
In the judge round of milestone E (plan e1, task J1), under `C;submission=v1` on this
use case's sets, a decision settled 0.0028 to 0.0250 dollars across the five
judges, at a median of 12,581 to 33,967 ms (`docs/results/e-judges.md`).
`details` makes no call on one image.

## Alternatives

- **`check` called with `use_case = reality-checks`**: under the same settings, the same
  verdict from the same requests
  (`check::each_use_case_through_its_recipe_gives_the_verdict_it_gave_before`); with details
  first it measures them inside one run, not in a step of their own. A call of `check`
  states its own settings: the recipe's defaults are not `check`'s.
- **Arm `F`, details first** (`--details first`): what the recipe did until plan m4, task
  A1; the gate above found no gain from the details on the dev sets.
- **The second look on**: `--second-look on` (plan m3, task U1).
- **Submission v2**: `--submission v2` (plan e1, task F1; ADR 0042).
- **Another judge** (plan e1, task J1; ADR 0044): the five judges this build prices,
  under `C;submission=v1`, repetition 1, on this use case's sets (`docs/results/e-judges.md`,
  "By scope", the scope `reality-checks`):

| Judge | Decisions | Balanced accuracy | Decided correctly | Unsure | Errors | Cost per decision |
|---|---|---|---|---|---|---|
| `claude-haiku-5-5` | 22 | 0.227 (0.045 to 0.409) | 0.227 (0.045 to 0.409) | 16 | 0 | 0.002771 |
| `gemini-3.8-flash` | 22 | 0.727 (0.545 to 0.909) | 0.727 (0.545 to 0.909) | 0 | 0 | 0.012805 |
| `gemini-4-argon` | 22 | 0.636 (0.500 to 0.818) | 0.636 (0.500 to 0.818) | 0 | 0 | 0.024958 |
| `gpt-6.1-sol` | 22 | 0.682 (0.450 to 0.909) | 0.682 (0.450 to 0.909) | 1 | 0 | 0.013422 |
| `gpt-6.1-terra` | 22 | 0.591 (0.292 to 0.850) | 0.591 (0.292 to 0.850) | 5 | 0 | 0.006942 |

## Limits

| Limit | Example |
|---|---|
| Rung 2 is credited from `check`'s runs, not run through the recipe | "Rung 2 results" |
| Every figure is on published sets, with one judge for the by-set numbers; nothing here is held out | `reality-checks/people-v1` |
| About a third of milestone 2's verdicts were `unsure` | E1's row above: 28 of 39 |
| A judge that breaks a submission's rules twice ends the run with no verdict | F1's failed runs |
| The recipe's live record (item 7 of task Q1) is the orchestrator's, still to come | `docs/results/m2c-use-cases.md` |

## Mark and history

**Unverified** (`SPEC.md` section 8): its record, with rungs 1 and 2; no gate cleared.

| Rung | Result | Evidence |
|---|---|---|
| 1. Single examples | Shown | The two examples above, run as tests |
| 2. Scored alone | Credited | Milestone 2's evaluation of `check` on `reality-checks/people-v1` and `reality-checks/adversarial-v1` |
| 3 and 4 | Not yet | Milestone E |

**Gates.** The gate of details first (`gates/details-first.toml`): rejected for all five use
cases together.

**History.**

- Plan 2C, task A1: the use case ported from squint as a use case of `check`.
- Plan m3, task Q1: a recipe of two steps (#210); this record (its third pull request).
- Plan m4, task A1: details first is off by default; the recipe as shipped is the gate's
  control arm, whole, and the examples above were made again under it.
