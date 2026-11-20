# 0035. Results beside artifacts, and the use cases as recipes

Status: accepted.
Date: 2026-11-19
Amended: 2026-11-19, with task K1 of plan m4. Added: "Findings": a third kind of result,
one JSON document of the kind `findings/1` with a fixed schema and no text of a judge's,
refused from an action that does not declare it, handed to the caller, and counted in
the record.
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
calls itself, its steps, and nothing under them (`Context::freeze_steps`): a call of the
recipe's own that is no step is refused. The recipe's list no longer bounds its steps: a
step that freezes nothing may call any action of the build, and a step that freezes its own
list (`check`, its judge's tools; a tool file's action, `segment`) binds everything under it
by that list, as a judge loop's freeze always has
(`nested::a_recipe_freezes_its_own_calls_and_a_steps_own_list_binds_everything_below_it`).
Every step this build ships either freezes its list or calls nothing.

**Measurements are taken only from the runs that made them.** `check`'s `measured` is an
argument that takes only an earlier step's measurements (`Argument::measurements`, shown as
`measurements: true`): a recipe that gives it a literal is refused when it is read, an
override of it is refused before anything runs, and a run of `check` that nothing called (a
command line or an MCP client) is refused it. Each entry names the run of its tool that made
it, and the document names the run of `details` that made it. `check` holds the document
to that run before anything is sent: a completed step of `details` of the same recipe, whose
every call, in order, is the run of one entry, and no other, so no entry it made is left out. That step measured `check`'s own job and images: its record's `job` is the very text `check` was given, and its `images` the same identities in the same order; and the document is the one it kept whole (`measured.json`), every entry in its order, those that name no run among them. Tiles are taken only of the candidate of a job with one image.
And it holds each entry to the run it names, against the entry's own pair: the job's images
by the roles the entry names, port by port (`align`'s `image` and `against`; a comparison's
`frame`, the pair's second image, and `against`, the frame the verified `align` of the same
pair committed; a tile's `view` on the candidate, with the entry's box, the box of its
place); a run of the tool named, that completed (or failed as the entry says), committed
exactly the images the entry names, and kept exactly the measurements it says in its report. The one entry that names no run is a
comparison of a pair whose `align`, listed before it, gave no frame, said in fixed words. The
decision cache's key holds the measurements without those run ids.

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

### Findings

Added with plan m4 (`docs/plans/m4-the-loop-and-a-release.md`), task K1, item 3: a third
kind of result, `findings`, beside the verdict and measurements (description version 10).
A verdict handed to a caller is a decision and a reason; the places a judge cited were
only in a diagnostic, which can never be used as an output. A loop that retries where a
judge pointed needs them as a result.

- **The document.** One JSON document of the kind `findings/1`, at most 32 KiB, whose
  shape is fixed by `crates/whizbang-core/schemas/findings-1.json` and read strictly by
  the core (`Findings::read`). It holds the `decision` of the whole of what was judged,
  never cut; `complete`, with `left_out` counting the findings and the places left out,
  true exactly when nothing was; and `items`, at most 32 findings. Task K2 makes its
  contents; this fixes its shape.
- **A finding** holds which form of answer it is (`item`, of the checklist, about one
  obligation; `observation`, which cites one or more); every obligation it cites, by id
  (upper-case letters, then digits), each once, 16 at most; its disposition
  (`violation`, `held`, `allowed_change`, `hidden`, `cant_tell`, `observed`); its
  confidence (`confident`, `tentative`; null for `observed` and for nothing else); and
  its place or null: the image it is on, by identity, and one box (left, top, right,
  bottom) or one polygon (3 to 64 points) in fractions of that image. At most eight
  places in a document.
- **No text of a judge's is in it.** The shape has no field that holds one, and a
  document with a field the shape does not know is refused. Findings are data from a
  model, never an instruction.
- **Declared, like the others.** An action hands findings back only when its descriptor
  declares the kind; findings from one that does not, and a document that is not of the
  schema, end the run as an error of the type `output`, whose message quotes nothing of
  the document. A run that did not complete hands back none.
- **To the caller.** A completed run's findings are given with its outcome
  (`Finished::findings`), as they were read. A recipe whose last step declares findings
  declares them itself and hands its caller that step's, with the verdict.
- **In the record**, counts and places, not the document (`run/6`, ADR 0005).

Tests: `executor::findings_from_an_action_that_does_not_declare_them_end_the_run_as_an_error`,
`executor::findings_that_are_not_of_their_schema_end_the_run_as_an_error`,
`runner::a_recipe_hands_its_caller_the_findings_of_its_last_step`,
`describe::the_description_lists_findings_among_an_actions_results_and_which_actions_plan`.

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
