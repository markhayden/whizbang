# Milestone 3 note

Milestone 3, "Rascal and edit evaluation", ends in code with task B1 of
`m3-milestone-3.md`. This note was written by B1's second pull request from its branch, with
`main` at e8cead9 (the merge of #240): every task of the plan but B1 has landed, Q1's box
waits on its item 7 (the orchestrator's live run of the five recipes) and L3's on its round's
decode. Four lines are left to B1's last pull request, each marked **To be filled by B1's
last pull request**: the seam round's decode, the edits rounds' decodes, the re-run of the
gate of details first on the Google judge, and the milestone's final spend. The clauses of
the spec's row that rest on the owner's picks are not shown until they are filled.

## What whizbang can do at the end of milestone 3

Beyond milestone 2: rascal's 25 operations are actions, each with its record, and its tasks
(`remove`, `remove_prompt`, `repair`, `repair_area`, `seam`, `mask_rect`, `protect`) and
squint's five use cases are recipes, files the core's runner reads, each step a run of its
own, listed by `describe` before the primitives. Masks are found by SAM 3 on the Mac and
filled by Bria Eraser through `https://fal.run`; a mask leaves a store as a file and comes
back with its identity. A lab round runs a matrix of recipes and fillers on a corpus,
resumes after an interruption, gives a failed cell one new attempt when asked, writes
blinded pairs for the owner to pick, checks the picks against the frozen assignment and
decodes them. The edits' rung 2 is scored by a committed program on a drawn set; a stored
run of `check` is written as a transcript the baseline's comparison reads.

## What shipped

| Pull requests | Task | What |
|---|---|---|
| #201 | Plan | The plan of milestone 3, after six rounds of outside review |
| #199, #200, #202, #203, #209 | Baseline | The replay's fixes, format 2, the breakdown, squint's submission rules, the receipt |
| #204 | L0 | The live record of the Google and OpenAI judges |
| #207 | R1 | Recipes: the format and the runner in the core |
| #206, #215 | F1 | Fills through fal; `fill` and `paste`; masks out and in |
| #205, #220, #223 | G1 | Gates as files, the live rerun, squint's wire; the gate's record; the Google judge's fix |
| #208 | U1 | The attack on `unsure`: the count by rule, `tiles` and the second look, screened |
| #212, #216 | O1 | Locations and mask shaping; `remove`, `repair_area`, `mask_rect`, `protect` |
| #211, #213 | O2 | Canvases, and a fill put back through a window or a cut |
| #219 | O3 | `fan`, `profile`, `ramp`; the task `seam` |
| #217, #221 | O4 | `tighten`, `repair`, `inspect`; `paste`'s `hard` and `poisson` |
| #225, #226 | O5 | `measure`, pictures from callers, large results' encoding |
| #210, #236, #239 | Q1 | Squint's use cases as recipes; `view`'s binding; a tool's image only its tree reads |
| #224, #227, #231, #235, #240 | L1 to L3 | The shared harness, `lab run`, its resume and decode, blinded pairs, `--retry-failed` |
| #218, #222, #229, #230 | D1, D2 | The records of the operations |
| #214, #228, #232, #233, #234, #238 | Upkeep | The store's growth, test time, the Mac runner, a folder's normal form, a fast encoder |
| #237 | B1 | The edit set's program, its scoring program and the four labs' files |
| This one | B1 | The edits' record, the five recipes' records, the stored transcript, the rows, this note |

## The clauses of the spec's row, each with its evidence

The row for milestone 3 in `SPEC.md` section 16. Tests are named by module and function;
`scripts/tests/test-notes.sh` fails when one named here no longer exists. The model tier
(`model_tier::` tests marked ignored) runs under `just test-models` on the owner's Mac, never
in CI.

| Clause | Shown by |
|---|---|
| Each task and operation in section 7 passes its acceptance case | Each operation an action: `describe::each_of_rascals_25_operations_is_an_action`; each held to rascal's vectors: `line::the_ported_vectors_give_rascals_locations`, `rect::the_ported_vectors_give_rascals_locations`, `stroke::the_ported_vectors_give_rascals_locations`, `rasterize::the_ported_vectors_give_rascals_masks`, `shrink::the_ported_vectors_give_rascals_masks`, `invert::the_ported_vectors_give_rascals_masks`, `close::the_ported_vectors_give_rascals_masks`, `subtract::the_ported_vectors_of_a_mask_give_rascals_masks`, `cut::the_ported_vectors_give_rascals_canvas_and_plan`, `window::the_ported_vectors_give_rascals_window`, `apply::the_ported_vectors_give_rascals_working_image`, `fan::the_ported_vectors_give_rascals_band_for_each_set`, `profile::the_ported_vectors_give_rascals_mask`, `ramp::the_ported_vectors_give_rascals_prescription`, `tighten::the_ported_vectors_give_rascals_masks`, `inspect::the_ported_vectors_give_rascals_report`, `measure::the_ported_vectors_give_rascals_measures`; and the tasks: `task::remove_fills_inside_its_mask_and_keeps_every_other_pixel`, `task::remove_completes_with_and_without_protect`, `task::remove_prompt_completes_with_and_without_protect`, `task::repair_fills_along_the_stroke_and_keeps_every_other_pixel`, `task::repair_area_fills_inside_its_mask_and_keeps_every_other_pixel`, `task::mask_rect_makes_the_rectangle_as_a_mask`, `task::protect_inverts_an_earlier_mask_onto_its_composite`, `task::seam_fills_the_wrap_edge_and_keeps_every_other_pixel`; every record of D1 and D2 with rungs 1 and 2: `describe::each_pixel_operation_and_task_without_a_model_shows_its_record_complete` and `scripts/tests/test-tool-records.sh` |
| Each model-based recipe has its ladder record with rungs 1 and 2, with SAM 3 for masks and Bria through fal for fills | The records of `remove`, `remove_prompt`, `repair`, `repair_area` and `seam`: rung 1 by `tool_records::recipes::the_record_of_remove_is_what_it_gives` and its four siblings; rung 2's task success and preservation, measured with Bria through fal and SAM 3 on the Mac (`docs/results/m3-edits.md`), by `evidence::the_edits_record_names_a_commit_of_this_tree_and_holds_only_aggregates`, `evidence::each_model_based_recipes_record_holds_rungs_1_and_2` and `evidence::each_recipes_rung_2_names_the_shipped_recipes_and_constant_sets_digests`; the scoring by `edits::preservation_outside_the_mask_is_counted_exactly`, `edits::task_success_is_the_masks_iou_and_the_fills_error_beside_a_blur_fill` and `edits::the_synthetic_set_is_drawn_by_its_program_from_fixed_seeds`. SAM 3's agreement with reference masks is `segment`'s record. The five use cases' recipes are credited from milestone 2's evaluation (`use_case_records::each_use_case_record_holds_two_worked_examples_that_its_recipe_gives`). **Not yet whole: visual quality waits on the owner's picks (below)** |
| A mask exports and imports | `export::a_mask_exported_imports_into_another_store_with_the_same_identity`, `import::a_file_over_the_limit_its_contents_allow_is_refused`, `import::a_fifo_at_the_path_is_refused_and_nothing_waits` |
| Rascal's tasks and squint's use cases run as recipes on the core's runner | `runner::each_step_is_a_child_run_on_the_recipes_one_reservation`, `runner::values_pass_between_steps_by_reference`, `runner::a_cancelled_recipe_cancels_its_running_step_and_starts_no_other`, `recipe::the_ported_recipes_of_rascal_read_as_rascal_read_them`, `record::a_recipe_run_lists_its_steps_in_order_with_arguments_constants_and_results`, `use_case::each_use_case_is_a_recipe_with_its_rules_details_and_tools`, `check::each_use_case_through_its_recipe_gives_the_verdict_it_gave_before` |
| `describe` lists recipes before primitives | `describe::recipes_are_listed_before_primitives_with_purpose_and_use`, `serve::initialize_lists_each_recipe_before_the_primitives` |
| A lab round runs a recipe and filler matrix, resumes after an interruption, presents blinded pairs, collects the picks, checks them against the frozen assignment and decodes the outcomes | `cli::lab_run_runs_a_three_image_round_under_a_scripted_filler_and_resumes`, `lab::an_interrupted_round_resumes_and_a_failed_cell_is_kept`, `lab::the_pairs_are_blinded_in_the_order_and_sides_the_seed_draws`, `lab::picks_for_a_pair_not_in_the_frozen_assignment_are_refused`, `lab::an_assignment_whose_digest_differs_is_refused`, `lab::the_decode_gives_rascals_outcomes_on_rascals_picks`, `cli::lab_run_pairs_and_decode_a_three_image_round_under_a_scripted_filler`. Live: the round of `labs/m3-seam/` on the owner's pictures, 94 of 94 cells, interrupted once on purpose and resumed (2026-11-19, at 8957f1f), and the four rounds of the edits, 96 of 96 cells (at e8cead9). **Not yet whole: the decodes wait on the owner's picks (below)** |

## The rounds, in numbers

The edits' rung 2 (`docs/results/m3-edits.md`): 24 pictures of `source-fidelity/scenes-v1`,
one drawn defect each, four recipes, 96 fills by Bria through fal, 3.84 dollars settled and
none unresolved.

| Recipe | Changed outside the mask | Mask IoU | Coverage | Mask, times the defect | Fill error | Blur error | Fill nearer |
|---|---|---|---|---|---|---|---|
| `remove` | 0 | 0.545 | 1.000 | 1.84 | 29.1 | 38.4 | 20 of 24 |
| `remove_prompt` | 0 | 0.716 | 1.000 | 1.40 | 28.4 | 36.2 | 16 of 24 |
| `repair` | 0 | 0.228 | 0.996 | 5.40 | 18.9 | 23.6 | 15 of 24 |
| `repair_area` | 0 | 0.590 | 1.000 | 1.70 | 27.2 | 31.0 | 14 of 24 |

IoU understates each mask: every recipe grows or feathers it on purpose, and each covers the
drawn defect whole, or nearly. The fill error is the mean distance per channel, 0 to 255,
from the clean picture inside the drawn defect.

**The edits' visual quality.** To be filled by B1's last pull request: the decodes of the
four labs `m3-edits-<recipe>` against the clean picture, by rascal's rule under each frozen
protocol.

**The seam's round.** To be filled by B1's last pull request: the decode of `m3-seam`,
`seam-bria` (`fan-v3`) against `thin-bria` (`fan-thin-v1`), by `seam-03`'s groups, beside
rascal's `seam-03` (`fan-v1`) and `objects-02` in `docs/results/m3-lab.md`.

**The gate of details first, again on the Google judge.** To be filled by B1's last pull
request: the re-run's result after the judge's fix (#223), its errors by kind beside the other
providers'. Before it (`docs/results/m3-gates.md`): rejected; `claude-haiku-5-5` 0.567 under
arm C and 0.550 under F, `gemini-3.8-flash` 0.646 and 0.554 with 40 errors of 100 under F,
`gpt-6.1-terra` 0.721 and 0.733, inconclusive.

## Spend, and what was downloaded

About 45 US dollars of the 100 allowed so far, all to approved recipients, each settled by a
store's ledger: the two new judges' live record, 0.158735 (`docs/results/m2c-use-cases.md`);
the fills of F1's evidence (`docs/results/m3-fills.md`); the gate of details first and the
baseline's live rerun, 11.908215 together (`docs/results/m3-gates.md`,
`docs/results/m2c-baseline.md`); U1's screen and strict look, 3.441386
(`docs/results/m3-unsure.md`); the edits' rounds, 3.84 (`docs/results/m3-edits.md`); the
seam's round, 3.76 and 0.04 unresolved; and the orchestrator's and the owner's other live
runs and tries, which the stores counted and no record lists one by one. This pull request
spent nothing. **The final figure: to be filled by B1's last pull request**, with the
re-run of the gate above.

## What a reader should not assume

- **Squint's judges agree with the labels about 61% of the time, and a third of verdicts are
  `unsure`** (`docs/results/m2c-evaluation.md`). The attack on `unsure` found the judge mostly
  hedging on clean pictures, and its screen ran on the dev sets it was built against, which
  admit nothing (`docs/results/m3-unsure.md`).
- **Details first was not shown to help.** The gate rejected it for two judges and was
  inconclusive for the third; the recipes still measure details first, as `check` did.
- **The second look is unsafe on reality-checks.** It took that use case's unsure rate from
  74% to 24%, and its false accepts from 0 of 33 to 6 of 33: of its 43 looks, 8 moved away
  from the label. The strict look did not mend it. It stays off by default
  (`docs/results/m3-unsure.md`).
- **Fill quality is modest.** Bria's fills beat a plain blur of the same mask on 14 to 20 of
  24 items, and their error is about four fifths of the blur's: better, not good. The visual
  quality rounds are the owner's eye, still to come.
- **Rung 2 of the edits is one synthetic set.** 24 pictures of one published set, one drawn
  defect of each kind; a real scratch or object is harder.
- **IoU is not the measure of these masks.** Each recipe grows and feathers on purpose; the
  coverage and the size beside it say what the mask did.
- **The seam's round is on rascal's corpus.** Its pictures were picked for seams, and its
  decode is pending.

## What moved to the carry-over list

Closed by this pull request: a stored run of `check` written as a transcript; `tighten`'s
cost on a long stroke, found in use (a diagonal stroke across 8000 by 6000 did not finish in
two minutes; now each cap and segment is drawn over its own box, the core the same pixel for
pixel). Dropped with a line: what rascal's heal graph held beside the fill put back, a mask
rebound onto an earlier composite, what a manifest and a task description say beyond the
descriptor, and the record's repairs, checklist and verdict shape (kept in `judged.json`).
Re-owned with a line: `invert` through a cut or a window, an executor built with a digest
alone, the Google introductory prices and the new rows on `lab run`'s progress, its "started
now" and the pairs page (milestone 4); the deletion's follow-ups, every publication writing
the whole generation and a real prober (milestone 5); which tools a gate admitted, a judge's
failed cell on resume, `fan`'s and `ramp`'s windows on small panoramas and `measure`'s
continuity (milestone E); two needs of one capability (when an action ships with them).
Still owned by milestone 3, for B1's last pull request and the boxes it waits on: the
Google judge's re-run, Q1's item 7, and L3's round.

## What milestone 4 and E start from

`main` at the end of milestone 3: rascal's operations and tasks and squint's use cases as
recipes on one runner, with SAM 3 and Bria; every tool with its record; a lab that runs,
resumes and decodes rounds the owner picks. Milestone 4 starts the loop and a release:
remove with a check and bounded retries as nested calls on one budget, and the rows re-owned
to it above. Milestone E takes `tiles` and the second look, and the model-based recipes, up
rungs 3 and 4 on fresh data: a gate on fresh items is the only way any of them becomes a
default.
