# Milestone 2 note

Milestone 2, "squint and evidence evaluation", ends in code with this note's pull request,
the second of task F1. Its plans are `m2a-first-slices.md`, `m2b-checkpoint-1d.md` (closed;
`m1d-note.md`) and `m2c-milestone-2.md`. This note was written by F1's second pull request
from its branch, with `main` at 93ccde8 (the merge of X1's second, #197): A1, A2, P1, E1,
E2, M1, X1 and F1's first are all in it. Two steps only the owner can take, both on the
baseline, are still open (below, "What the owner still has to do"); the clause of the spec's
row that rests on them is not shown until they are done.

## What whizbang can do at the end of milestone 2

Beyond 1D: `whizbang squint check <use-case>` judges a source fidelity, a reference
consistency, a reality check, a brief compliance or a product fidelity with a judge of
Anthropic, Google or OpenAI, from the command line (`--brief`, `--reference`, `--job` for a
squint job file) and over stdio MCP (`whizbang serve`), handing the judge measured details
first (`align`, and by name `change_map`, `edge_similarity` or a tool file of the user's);
a decision cache answers an unchanged job without a call. The evaluation sets of squint are
here, with matrices, `whizbang eval matrix`, `runs stats` and `runs export`, measures and
paired statistics, gates as decided files, and the replay of the baseline (`whizbang eval
baseline`). A perception tool is one Python file (ADR 0032), run by a tool worker of its own,
tried with `whizbang try` and scored with `whizbang eval tool` with no rebuild; per-subject
comparison is the first, on SAM 3.

## What shipped

| Pull request | Task | What |
|---|---|---|
| #186 | Plan | The plan of the rest of milestone 2, after five rounds of outside review |
| #187 | P1 | The Google and OpenAI (Responses) judges, each with provider conformance |
| #188 | A1 | The four use cases and the job they judge, from the command line |
| #189 | A2 | `resize`, `align`, arm C's projection and the details handed to the judge first |
| #190 | E1 | The evaluation sets, matrices, the runner, `runs stats` and `runs export` |
| #191 | E2 | Measures, paired statistics, gates and transcript format 2 |
| #192 | E1 | E1's evidence: arm F over the ten sets |
| #193 | M1 | The stdio MCP door and the decision cache |
| #194 | E2b | The baseline replay and `eval baseline` |
| #195 | X1 | `change_map` and `edge_similarity` measured first, `try`, `eval tool`, marks |
| #196 | F1 | The output budget, the review follow-ups, the rows of milestone 2 |
| #197 | X1b | Tool files, the tool worker and `tool/1`, per-subject comparison |
| This one | F1b | Per-subject comparison's record on rungs 1 and 2, rung 2 of `change_map` and `edge_similarity`, SAM 3's phrase box, this note |

## The clauses of the spec's row, each with its evidence

The row for milestone 2 in `SPEC.md` section 16. Tests are named by module and function;
`scripts/tests/test-notes.sh` fails when one named here no longer exists. The model tier
(`model_tier::` tests marked ignored) runs under `just test-models` on the owner's Mac, never
in CI.

| Clause | Shown by |
|---|---|
| The baseline reproduces | **Not yet shown: it waits on the owner's run (below).** Built: the replay of each recorded decision of arm C with squint's loop on whizbang's prompt, checklist, submission, `view`, `align` and preparation, compared in transcript format 2 (`baseline::format_2_compares_a_scaled_tool_images_size_and_mapping_and_not_its_bytes`); three decisions captured from frozen squint compare with no difference (`baseline::each_synthetic_decision_replayed_through_the_loop_preparation_and_tools_compares_with_no_difference`), and one that differs fails at its place (`baseline::a_replay_that_differs_at_one_place_fails_and_names_the_place`); the command prints counts only (`cli::eval_baseline_prints_counts_and_no_item`) and writes a receipt of four fields (`baseline::the_receipt_holds_exactly_the_four_fields`) |
| Each of the five use cases returns a verdict on a representative case, from the command line | `check::reference_consistency_reaches_a_verdict_from_a_scripted_judge`, `check::reality_checks_reaches_a_verdict_from_a_scripted_judge`, `check::brief_compliance_reaches_a_verdict_from_a_scripted_judge`, `check::product_fidelity_reaches_a_verdict_from_a_scripted_judge`, `cli::squint_check_takes_each_use_case_with_its_roles_under_a_recorded_fake`, `cli::squint_check_job_takes_a_squint_job_file_whole`; live, one call of each on Anthropic's judge, held by `evidence::the_use_cases_record_names_a_commit_of_this_tree_and_holds_no_secret` (`docs/results/m2c-use-cases.md`) |
| ... and over stdio MCP | `serve::initialize_lists_each_action_as_a_tool_with_its_schema`, `serve::check_over_stdio_returns_the_verdict_of_each_use_case_under_a_scripted_judge`, `serve::nothing_but_the_protocol_is_written_to_stdout`, `serve::a_cancelled_call_ends_its_run_as_cancelled`, `serve::end_of_input_cancels_waits_and_stops_the_worker`, `cli::serve_answers_a_client_over_stdio_and_exits_when_its_input_ends`; live, one use case over stdio, in the same record |
| Anthropic, Google and OpenAI each pass provider conformance | Over squint's recorded exchanges on a loopback listener: `anthropic::conformance::each_recorded_exchange_of_squint_is_read_back_to_its_turn`, `google::conformance::each_recorded_exchange_of_squint_is_read_back_to_its_turn`, `openai::conformance::each_recorded_exchange_of_squint_is_read_back_to_its_turn` and each one's `conformance::a_request_is_written_as_the_recorded_request_was`; each key to its own origin alone (`google::the_key_goes_only_to_the_approved_origin_in_its_header`, `openai::the_key_goes_only_to_the_approved_origin_as_a_bearer`). The owner approved both new recipients on 2026-11-18 and live checks on all three providers passed on the owner's Mac; Anthropic's live call is recorded, and **the record's lines for Google and OpenAI are moved to milestone 3** (below) |
| The decision cache misses when the use case, brief, an image's role or order, a rule or a tool changes, and hits for an unchanged job | `cache::a_change_of_use_case_brief_role_order_rule_or_tool_misses`, `cache::a_tool_files_edit_misses`, `cache::an_unchanged_job_hits_and_records_a_run_naming_the_earlier_one`, `cache::evaluation_never_reads_the_cache`, `cache::a_backend_that_cannot_report_its_identity_is_not_cached` |
| A new tool file is tried, scored, edited and re-run without a rebuild | `cli::a_tool_file_is_tried_scored_edited_and_run_again_without_a_rebuild`, `tool_file::a_declaration_missing_its_bounds_is_refused_when_the_config_is_read`, `tool_file::an_output_outside_its_declared_bounds_is_refused_before_use`, `tool_file::a_tool_reaches_a_capability_only_through_its_context_and_under_the_runs_allowance`, `tool_worker::a_tools_fault_ends_the_group_and_a_child_holding_the_pipes_with_it`, `tool_worker::a_cancellation_ends_the_group_and_waits_for_it` |
| `whizbang runs stats` and `runs export` report on a tool across at least 100 recorded runs | 172 runs of `align` by both, in E1's evaluation and again in F1's: `evidence::the_evaluation_record_reports_at_least_100_runs_of_align_by_stats_and_by_export` (`docs/results/m2c-evaluation.md`) |
| Per-subject comparison exists as an experimental tool and has been scored on rungs 1 and 2 | `pack/tools/per_subject.py`, marked experimental in its record; rung 1, two worked examples answered by SAM 3 on the Mac: `model_tier::per_subject_comparison_answers_the_records_examples_on_the_mac`; rung 2 on `source-fidelity/scenes-v1` and `source-fidelity/adversarial-v1`: `evidence::the_per_subject_record_names_a_commit_of_this_tree_and_its_rung_2_numbers` (`docs/reference/tools/per_subject.md`); against the fake models, `test_per_subject.Compare.test_a_subject_removed_is_reported_absent_by_name_and_by_location` |

## The evaluation, in numbers

Arm F (`details = first`, `align` measured first) on the five dev sets and the five
adversarial sets, 110 items twice, `claude-haiku-5-5` (`docs/results/m2c-evaluation.md`):

| | E1 | After F1's output budget |
|---|---|---|
| Completed, of 220 decisions | 190 | 214 |
| Agreeing with their labels, of those completed | 111 (58.4%) | 131 (61.2%) |
| `unsure`, of those completed | 36.8% | 33.2% |
| Settled | 0.77 dollars | 0.81 dollars |

Rung 2 of the tools, each by `whizbang eval tool` on the owner's Mac from commit 4dd4a7b,
the area under the ROC curve of the tool's number with its 95% interval resampled by family:

| Tool | Sets | Scored, of the items | Area under the ROC curve | Balanced accuracy, threshold read on the other half |
|---|---|---|---|---|
| `per_subject` (SAM 3) | scenes-v1 and adversarial-v1 | 16 of 22 | 1.00 (1.00 to 1.00) | 0.80 |
| `change_map` | scenes-v1 | 15 of 20 | 0.77 (0.61 to 0.94) | 0.75 |
| `edge_similarity` | scenes-v1 | 15 of 20 | 0.73 (0.57 to 0.97) | 0.75 |

The items with no number did not register onto their source (`align`): mirrors, a rotation
and two changes of light. `change_map` and `edge_similarity` are now unverified; `align`
alone stays the default of `--details first`.

## Spend, and what was downloaded

About 1.58 US dollars of the 100 allowed, all to `https://api.anthropic.com`, each settled
by the ledger and recorded: the first verdict, 0.000912 (`docs/results/m2a-first-verdict.md`);
A1's four use cases, 0.003390, and M1's call over stdio, 0.000883, with its cached answer at 0
(`docs/results/m2c-use-cases.md`); E1's evaluation, 0.767424, and F1's, 0.809051
(`docs/results/m2c-evaluation.md`). This pull request spent nothing: SAM 3 ran on the Mac.
The owner's own live checks of Google and OpenAI are not in this count. Downloaded on the owner's Mac only: the sets' pictures,
by hash, from `https://raw.githubusercontent.com` (approved by the owner on 2026-11-18).

## What the owner decided

| Date | Decision |
|---|---|
| 2026-11-17 | The test time of the hosted macOS runner may be 300 s |
| 2026-11-18 | `https://generativelanguage.googleapis.com` and `https://api.openai.com` approved as judge recipients, both keys put in the env file: "keys are both there" (first and second items) |
| 2026-11-18 | `https://raw.githubusercontent.com` approved for the sets' pictures (third item under "What the owner must answer") |
| 2026-11-18 | The runtime on Linux deferred to an external GPU host (carry-over row on the Linux runtime extra) |

## What the owner still has to do

1. **The baseline gate** (fifth item under "What the owner must answer"): on the Mac, with
   the owner's baseline package and its converted transcripts,

   ```
   cargo build --release -p whizbang --features baseline-replay
   ./target/release/whizbang eval baseline --package <package> --transcripts <transcripts> --receipt docs/baseline/receipt.toml --commit "$(git rev-parse HEAD)"
   ```

   The replay is a feature the shipped binary leaves out (ADR 0033), hence the build line.

   It prints counts only and writes the receipt, which is committed, and the fast test the
   carry-over row on the baseline gate's run names is written against it.
2. **The one-number audit** (fourth item): `scripts/baseline/audit.py --count-scaled-cuts
   --transcripts <transcripts>`, which prints how many recorded crops were cut larger than
   1024 pixels on a side; if it is not 0, a yes to transcript format 2 for them.

## What a reader should not assume

- **The baseline has not been shown to reproduce.** The replay agrees with frozen squint on
  three synthetic decisions; the owner's recorded decisions have not been replayed.
- **Google's and OpenAI's live answers are not in a committed record.** Their conformance is
  over squint's recorded exchanges; the owner's live checks passed, but no record of one is
  committed (milestone 3).
- **Rung 2 is small.** Six families at most; an area of 1.00 on 16 items says the readings
  were ordered, not that they will be on the next set. The intervals are wide.
- **A tool that needs `align` cannot read an unregistered pair.** Mirrors, rotations and
  recompositions get no number from `per_subject`, `change_map` or `edge_similarity`.
- **Per-subject comparison reads SAM 3's masks.** Its readings are the model's, on one Mac,
  and SAM 3's phrase box is now the bounds of its mask (below).
- **A third of the evaluation's verdicts are `unsure`.** 33.2% after F1, most in
  reality-checks and reference-consistency.

## What moved to the carry-over list

Closed by this pull request: per-subject comparison's record and tests (the row on what X1
did not build), rung 2 of `change_map` and `edge_similarity`, and the time of `check
(ubuntu-22.04)` on `main` (5 min 33 s and 8 min 39 s on the last two pushes). Re-owned with a
line: the order of a segmenter's answer (milestone 3), two needs of one capability (milestone
3), P1's live evidence, a record of one live call on each new judge, which waits on a run a
builder is permitted to make (milestone 3), and ending an unacknowledged admission in a
long-lived door (milestone 5). Still owned by milestone 2 and named above as the owner's:
the baseline gate's run. M1's box is ticked: its two missing tests were built by X1's second
pull request.
Moved earlier by F1's first pull request, each with its line: E2's gate files and `eval run
--gate`, the live rerun of the baseline and its one gate run, a picture whose scans are short,
a tool's image only its parent reads, `view` decoding once a run (milestone 3); a reduced crop
with transparency and the slack of a crop of a crop (milestone E).

Found in use by this pull request: SAM 3 draws a phrase's mask apart from the box it gives,
and on a picture of `source-fidelity/scenes-v1` a door's mask began five pixels left of its
box, so the core refused the instance and per-subject comparison could not read the item. The
adapter `sam3` now gives the bounds of the mask as the box (`test_sam3.Phrase.test_a_phrases_box_holds_every_pixel_its_plane_has_on`),
as its point and box already did and Tiny's adapter does; the record of the model tier was
taken again (`docs/results/m1c-local-pack.md`, `docs/results/m1d-segment-coverage.md`). The
parity fixtures of SAM 3 keep the boxes the model gave; their agreement is the count, each
mask's IoU and the score (ADR 0027), none of which changed.

## What milestone 3 and E start from

`main` at the end of milestone 2: five use cases judged from the command line and over stdio
MCP by three providers' judges, with details measured first and a cache; the evaluation sets,
matrices, statistics and gates; tool files, tried and scored without a rebuild. Milestone 3
starts the recipes on the shared mechanism, which the rows re-owned to it wait on (a tool's
image only its parent reads, steps that pass artifacts, more than 256 calls a run, gate files
run by `eval run --gate`). Milestone E takes `change_map`, `edge_similarity` and per-subject
comparison up rungs 3 and 4, and brings `regions`, `classes` and `outline` in as experimental
tools.
