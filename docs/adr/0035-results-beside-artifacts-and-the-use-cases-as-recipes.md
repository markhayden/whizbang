# 0035. Results beside artifacts, and the use cases as recipes

Status: accepted.
Date: 2026-11-19
Spec: section 3 ("Primitives, recipes and loops", "What a judge model may call"), section 7
("Use cases expressed as recipes on the shared mechanism"); plan m3
(`docs/plans/m3-milestone-3.md`), task Q1, items 1 and 2; ADR 0003, ADR 0009, ADR 0029,
ADR 0031, ADR 0033

## Context

A verdict was handed back beside an action's artifacts and written in its run's record (ADR
0009), but no descriptor said which actions answer with one, so any action could, and a
caller read it only from the record. What an evidence tool measured had no place at all:
`align` wrote its report in its run's diagnostic, which a calling run cannot read, so
`check` worked the numbers out again by the same function on the same images
(`details::report_of`, plan 2C, task A2) before telling the judge. The owner's goal for
squint is that the judge is handed every detail measured, and makes the final conclusion
from them; plan m3 asks for each use case to be a recipe whose first step measures and whose
second judges, the measurements handed from one to the other by the step that made them.

## Decision

**Result kinds** (`contract::ResultKind`). A descriptor lists what its action hands back
beside its artifacts, `results`: `verdict`, `measurements`, or neither (description version
9; a descriptor that lists none has no `results`). The executor holds what an action hands
back to that list before it commits anything: a verdict or measurements from an action that
does not declare it ends the run as an error of its output (`output`), and nothing is
committed. Measurements are one JSON document of at most 32 KiB
(`MEASUREMENTS_MAX_BYTES`). A caller is given both with the outcome (`Finished::verdict`,
`Finished::measurements`), of a run that completed and of no other; the record keeps the
verdict as before. `align`, `change_map` and `edge_similarity` declare and hand back their
measurements, the same report each keeps in its diagnostic; `check` declares its verdict.
No artifact type is added: a measurement is no artifact, held by no store name, and goes
from one step to the next through the run that made it.

**A recipe binds them** (`recipe/1`, additions to ADR 0033). A step's argument written
`"<step>.measurements"`, naming an earlier step whose action declares measurements, is
given what that step's run handed back, as one JSON text: a text argument only, checked when
the recipe is read. A recipe whose last step declares a verdict declares one too, hands that
step's verdict back with its outcome, and may hand back no artifact. An input may say
`many = true`: every artifact a caller gives at its port, in order, bound only to a port
that takes several. A step whose action takes `recipe_digest` is given the digest of the
recipe's text, which no override sets.

**What a recipe freezes** (amends ADR 0033 and ADR 0009). A recipe freezes the actions it
calls itself, its steps, and leaves what a step calls to the step (`Context::freeze_steps`):
`check` as a step freezes its judge's tools for itself, as it does when it is called alone.
A judge loop's freeze still binds every run under it.

**The use cases as recipes** (`crates/whizbang-squint/recipes/`, `whizbang_squint::recipes`).
Each of the five use cases is a recipe of two steps under the use case's name: `details`,
the action `whizbang_squint::measured::DetailsStep`, which calls `align` for each pair the
job relates, each comparison it is told to on each pair that registered, and `view` for the
tiles of a job with one image, each a nested call of its own run, and hands back in order the
images they committed (at `images`) and what each found or why it found nothing (its
measurements); then `judge`, `check` under the use case, given those images at
`measured_images` and those measurements at `measured`, which it says to the judge as it
said its own details before and never measures again. `check` called alone with
`details = first` measures through the same function (`measured::measure`), as nested calls
of its own run, so the two say the same: one test runs each use case both ways under the same
scripted judge and holds the verdict and every request to be equal
(`check::each_use_case_through_its_recipe_gives_the_verdict_it_gave_before`). The second
look is off in every recipe, as `check`'s default is (plan m3, task U1: on reality-checks it
raised false accepts); a run sets it with `--set judge.second_look=on`.

`whizbang squint check` and `whizbang serve`'s `check` run the use case's recipe; what a
command line changes from the recipe's own steps (`--details none`, `--detail`,
`--projection`, `--second-look`, `--tool`) is the run's overrides (`set`). An evaluation's
cell is still a call of `check` itself. The decision cache's key (ADR 0031) holds the recipe's
digest through `recipe_digest`: a changed recipe misses and an unchanged one hits
(`cache::a_changed_use_case_recipe_misses_and_an_unchanged_one_hits`). The reasons and what
the judge decided are the judge's step's diagnostics; a door reads them from that step's run
(`door::judged_run`).

## Consequences

The versions: `check` is 1.6.0 (a port and two arguments); `align`, `change_map` and
`edge_similarity` keep theirs, since what they commit and how is unchanged and their
version is in their artifacts' origins; the description is version 9. A record of a use
case's run is `run/5`, its two steps listed; its judge's turns are in the record of the
step `judge`. `details::report_of` and `details::compare_report_of` are gone. A tool file's
declared output is not yet handed on as measurements: tool files are offered to the judge
and are no step of a recipe.

## Alternatives

- **Measurements as an artifact type.** A seventh type with a file and an identity, for a
  document that only the next step reads; the carry-over row allowed it, and it would bind
  through the store at the cost of a type every door and the reference describe.
- **The judge's step reading the earlier step's diagnostic.** A calling run cannot read
  another run's diagnostics (ADR 0008), and a sibling's would need a new kind of read.
- **A recipe's freeze binding its steps' calls.** No recipe could hold a judge, whose tools
  the recipe does not know.
