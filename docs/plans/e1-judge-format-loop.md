# Plan: milestone E, slice 1: the default judge, the submission format, the loop's retries

Draft 5, 2026-11-20, written from `main` at 722083fc (the merge of #259), after four outside
review rounds ("Review rounds", at the end). Milestone E is `SPEC.md` section 16, row E,
"Evidence and edit quality". This is its first slice, run beside milestone 5 as decided
with the owner on 2026-11-20 (`docs/plans/m4-note.md`, "What milestone 5, E and later owe").

**What this slice is, and what it is not.** It is not the tool-by-tool evaluation. It closes
the three items milestone 4 left open: the default judge, chosen from evidence; the
submission format, so small models stop failing it; and the loop's retries, which no live
run has yet reached. Taking each unverified tool up the ladder (section 8) follows in later
slices of E, as the m4 note lists them: the loop's rungs 2 to 4, its visual quality
confirmed by a person in blinded rounds, the picking rounds of milestone 3, `tiles` and the
second look, and details first gated again on fresh data.

**Words this plan uses.**

- **A judge** is a model behind `[backends.judge]` that answers `check`. **The default
  judge** is the model the config guide's first example names, which is what a user who
  copies it gets, and what `remove_checked` uses when `--judge` is not given. No code picks a
  model today: the config's `default` judge backend answers, whatever model it names. Today
  that example is `claude-haiku-5-5`.
- **A decision** is one run of `check` on one item. **A submission** is the judge's call of
  `submit_verdict`. **A rejection** is a submission `decided_of` or `cited_calls_held`
  refuses, a turn that calls `submit_verdict` with other calls or twice, or a turn that ends
  without one; `check` answers the first with one repair today.
- **A submission failure** is a decision that ended because of a rejection: as
  `invalid_submission` (a second rejected submission), or as `judge_output` whose cause is a
  turn ended without a submission after the repair. Other `judge_output` causes (a cut-off
  turn, an unknown stop) are not submission failures. Task T1 records the cause, so the two
  are told apart by the record and not by the error type. **The submission failure rate** is
  submission failures over decisions. **The first rejection rate** is the share of decisions
  whose first submission was rejected, whether or not the repair saved it.
- **Arm C** is `check` as it ships: nothing measured first, `view` and `align` offered,
  second look off (`docs/reference/tools/source-fidelity.md`, "Details first is off by
  default"). Task S1 holds that by a test.
- **An attempt**, **a retry**, **the shaped mask**, **the found mask**: as in plan m4 and
  `crates/whizbang-rascal/src/checked.rs`.
- **Mask coverage** of an attempt is the share of the truth mask's pixels inside the
  attempt's shaped mask where its weight is at least a half (`edits::coverage`). **Under-
  covered** means coverage under 0.95, a threshold frozen here, before any run. It is a
  statement about the mask, not about the finished picture: a fill can erase beyond its
  mask or leave a trace inside it.

## What the spec says

- Section 8, "Evaluation data": `blind-v3` is an exposed regression set and never admits a
  tool; sealed sets are never edited; nothing public discloses an item-level answer for an
  unreleased set. Section 15: using a held-out set is asked first, so `blind-v1` and
  `blind-v2` (split `heldout`) are not used.
- Section 8, "Marks": an unverified tool "may be a default while nothing admitted replaces
  it". Every use case of `check` is unverified (each record's "Mark and history"), so a new
  default judge or format is allowed on descriptive evidence and is recorded as such. The
  gate that would admit either needs fresh data, which no set offers today; it is owed to a
  later slice (a carry-over row, opened by the slice's note in L2).
- Section 16, row E: "the submission format is redesigned so small models can follow it".
  Section 17: the format and its budget were "ported as is, then redesigned through the
  ladder".
- Section 3, "What every run records": judge runs keep "repairs; excluded turns", and
  diagnostics are kept for failed runs exactly as for completed ones. Section 6: a limit is
  enforced, an estimate is advice; an allowance is reset only by the owner's command.
  Section 15: 100 US dollars a milestone on approved recipients.
- Section 13: fakes through the production path; the fast tier's 300 s limit. `AGENTS.md`:
  tests run only through `just check` or `just test-only`, never a bare `cargo`.

## What the evidence already says

- **The 2026-11 tool study** (`docs/results/2026-11-tool-study.md`, old harness, arm C,
  `blind-v3`): balanced accuracy 97.9% for `gemini-4-argon`, 95.8% `gpt-6.1-sol`, 94.3%
  `gpt-6.1-terra`, 89.6% `gemini-3.8-flash`, 89.0% `claude-haiku-5-5`; Haiku sent 33 of 143
  items to review. Settled per decision under arm C (`docs/baseline/manifest.toml`): Haiku
  0.0033, Terra about 0.017, Flash 0.0228, Argon 0.0297, Sol 0.0336 dollars.
- **Milestone 3's gate, its fourth and complete run** (`docs/results/m3-gates.md`, whizbang,
  the five dev sets): arm C balanced accuracy 0.529 for Haiku and 0.887 for Flash.
- **Milestone 2's evaluation** (`docs/results/m2c-evaluation.md`): Haiku, arm F, 220
  decisions: 51 repairs, 10 `invalid_submission`.
- **Milestone 4's loop** (`docs/results/m4-loop.md`): 48 runs, no retry; Haiku kept 23 of
  24 `unsure`, Terra failed 6 on `RQ2`. The synthetic pictures were too easy to reach a retry.

None compared the five judges on whizbang's `check` as it now ships, on every use case, with
cost beside accuracy; and no record says why a submission was rejected. Tasks T1, S1, F2 and
J1 do.

**The order of the rounds.** The format is settled first (F2), then the judge is picked
under the settled format (J1), so the default judge and the default format are measured
together, on one population, and no number of one format stands in for the other.

## How the work is split

Seven pull requests, three of them the orchestrator's records. Sizes are changed lines of
source, tests and hand-written documents.

| PR | What | Builds on | Size |
|---|---|---|---|
| T1 | Rejections recorded on every exit of `check`, each with its class and cause | `main` | Small: about 600 |
| S1 | `whizbang eval judges`: per-judge and per-arm scores, two comparison modes, cost, submission counts; both rules as code | `main` | Medium: about 1,400 |
| L1 | The loop on `segmentation/segment-01`: its pinned manifest, the evidence script generalised, `whizbang eval loop score` and `eval loop sheet` | `main` | Medium: about 1,600 |
| F1 | Submission v2: every problem in one repair, near-misses recovered and recorded, a schema keyed by checklist id; the setting `submission` and its arm syntax | T1 | Large: about 1,800 |
| F2 | The orchestrator's format round, the decision, and, if adopted, v2 as the default | S1, F1 | Small to medium: about 400 |
| J1 | The orchestrator's judge round under the settled format, and its record: results, ADR 0041, the guides and records | F2 | Medium: about 600, no code but matrices |
| L2 | The orchestrator's loop round under the chosen judge and format, its record, the contact sheet and the slice's note | L1, J1 | Medium: about 700 |

**Waves.** (1) T1, S1 and L1 from `main`, side by side. T1 is in `check.rs` and
`submission.rs`; S1 is a new `crates/whizbang/src/judges.rs` and `metrics.rs`'s one added
statistic; L1 is new files and `scripts/loop-evidence.*`. The one file two of them touch,
`crates/whizbang/src/eval.rs`'s command enum, is merged by whichever lands second (a merge
commit, `just docs` again). (2) F1 when T1 has landed. (3) F2 when S1 and F1 have landed.
(4) J1 when F2 has landed. (5) L2 when L1 and J1 have landed. The code is three PRs in
wave 1 and one in wave 2; the rounds then run in series, as the order of the rounds needs.

**Live runs are the orchestrator's.** Builders make no provider call and load no real model
in CI. Every live round is run by the orchestrator on the owner's Mac by the committed
command the task names.

**Stores and allowances.** Two stores, each made new: the evaluation store, for F2 and J1,
and the loop store, for L2. Setting a store's allowance is a reset (`whizbang spend reset`),
and a reset is the owner's command (section 6), so the first allowance of each is an owner
item ("What the owner provides"): 70 dollars for the evaluation store, 25 for the loop
store. The allowances are guards; what counts against the milestone's 100 is what is
settled. A top-up is asked of the owner the same way.

**Prices and caps, before each paid round.** Before F2, J1 and L2 each, the orchestrator
reads the price rows of the judges it will call against their dates (the carry-over row on
Google's introductory prices: `gemini-4-argon` to 2026-11-30, `gemini-3.8-flash` to
2026-12-31). A row past its date is read again from the provider's page and the build's row,
its date and ADR 0026 changed in their own PR before the round, as plan m4's task E1
required; the round runs on a binary with that change. Each round's `call_cap_usd` is then
set from that binary's bounds, not from a number in this plan: for a matrix, the largest
one-check bound among its judges plus a cent; for the loop, the three-attempt bound of its
judge plus a cent (`docs/guides/loop.md`, "What it costs", which a test recomputes from the
build's tables).

**Review rule.** Every PR is reviewed whole by a reviewer that did not write it, as
`AGENTS.md` says, `just check` and `just scan-range` run by the reviewer. Each task names
what its reviewer looks at hardest.

**Test time.** At most 10 seconds of fast-tier test time for the slice. Every test that
calls a provider or a model is `#[ignore]`d.

## T1. Rejections recorded on every exit (small)

### Goal

Record why each submission was rejected, on every way `check` ends, so the failure rate per
model can be measured before any change to the format, and failed runs count.

1. **A class for every rejection**, beside its message: `not_a_list` (items or findings not a
   list, or a string), `unknown_item`, `missing_item`, `duplicate_item`, `bad_field` (an
   enum or a type not of the schema), `location`, `allowed_by`, `finding_cites`,
   `evidence`, `not_alone` (submit with other calls, or twice), `no_submission` (a turn
   ended without one). `decided_of`, `cited_calls_held` and the loop's own checks each
   return the class; the messages the judge sees are unchanged, byte for byte.
2. **Kept on every exit.** Each rejection, in order, as `{turn, class}`, and the run's
   **cause** of ending (`concluded`, `invalid_submission`, `no_submission_after_repair`,
   `cut_off`, `ceiling`, `refused`, `turns`, `fault`, `other`), written as a diagnostic of
   the run (`rejections.json`) on every return of `check`'s loop, completed or failed,
   including a second rejection, a turn ended without a submission, and the turn budget
   spent. Classes and causes only: no word of the judge.
3. **Read by `runs stats`**: `--rejections` prints counts by class and cause over the filter,
   aggregates only.

**Done when:** `check::a_second_rejected_submission_keeps_its_rejections`,
`check::a_turn_ended_without_a_submission_after_the_repair_keeps_its_cause`,
`check::a_cut_off_run_is_not_a_submission_failure`,
`check::the_turn_budget_spent_keeps_its_rejections`,
`submission::every_rejection_has_a_class`,
`submission::the_judge_sees_the_same_messages_as_before` (v1's messages byte for byte), the
baseline gate test unchanged and passing, and `stats::rejections_by_class_and_cause`.

**Files:** `crates/whizbang-squint/src/check.rs`, `use_case/submission.rs`,
`crates/whizbang/src/stats.rs`, tests in `crates/whizbang-squint/tests/it/` and
`crates/whizbang/tests/it/`; `docs/guides` where `runs stats` is described. **Reviewer looks
hardest at:** every return of the loop writes the diagnostic; nothing the judge wrote is in
it.

**Verify:** `just check`; `just test-only -p whizbang-squint check submission`;
`just test-only -p whizbang stats baseline`.

## S1. `whizbang eval judges` and its two rules (medium)

### Goal

Score judges against each other, and a format against another, from the runs matrices made,
with cost beside accuracy, and apply the pick rule in code so the choice is not made by hand.

1. **The command.** `whizbang eval judges --matrix <file> [--matrix <file> ...] --store
   <dir> --labels <dir> --mode judges|arms [--arm <arm>] [--rule judge-pick/1|submission/1]
   --out <file>`.
   Reads the store only. Labels through `gating::labelled` (as `eval run --gate` reads
   them): a set that publishes its labels by its own file with its corrections, a blind
   set's from `<labels>/<set id>/labels.toml` held to the set's seal, the labels' folder at
   or below the sets' folder refused. `--out` under the sets' folder or the labels' folder
   is refused, as the gate's receipt is.
2. **The sample, frozen.** Selection measures (balanced accuracy, decided correctly, catch
   rate, false rejects, false accepts, unsure, errors, cost per decision, time) use
   **repetition 1 only**, so every judge is scored on the same 253 items with the same
   weight. Repetition 2, where a matrix has one, gives **self-agreement** (the share of items
   with the same verdict both times) and, in `arms` mode, enters the submission measures.
   In `judges` mode every judge's measures come from one arm, named by `--arm`, so no judge
   is scored partly under one format and partly under another; a judge with no cell of
   that arm on an item of the population makes the matrices incomplete.
   Submission measures (submission failure rate, first rejection rate, rejections by class,
   recoveries by kind) use every decision of the compared cells, from T1's diagnostic. A run
   without that diagnostic is counted as unknown, never as clean. Total settled spend is
   reported apart from cost per decision.
3. **What it writes**, aggregates only, as TOML (`judges/1`), per judge and arm, for each
   use case, for the dev sets pooled, for `blind-v3` alone, for the source-fidelity sets
   pooled (`scenes-v1`, its adversarial set, `blind-v3`) and for all pooled: the measures of
   item 2; for balanced accuracy and decided correctly, a 95% interval resampled by family
   (`whizbang_eval::bootstrap`); counts beside each rate. Also the matrices' identities,
   each set's seal and labels digest and corrections digest, the commit, and each judge's
   backend, model and reported version as the records hold them. Never an item, a verdict
   of an item, or a word of a judge.
4. **Two comparison modes.** `judges`: each judge against the best (item 5), on the same
   items, repetition 1. `arms`: within each judge, the treatment arm against the control
   arm, on the same items: repetition 1 for the accuracy differences, both repetitions for
   the submission counts. Each gives the difference in balanced accuracy,
   decided correctly, false rejects and false accepts, resampled by family. `metrics::paired`
   gains the decided-correctly difference, with a test.
5. **The pick rule, `judge-pick/1`, a pure function with tests,** applied twice: over all
   sets pooled (the default for `check`) and over the source-fidelity sets alone (the
   loop's judge, since the loop checks with `source-fidelity` only).
   - **Complete first.** The completeness check, shared by both rules: every expected cell
     and repetition of the named matrices has a last attempt that completed or failed
     (failed counts as an error), and every item a label. A cell not run or interrupted, an
     item without a label, or an interval that cannot be worked out makes the outcome **no
     pick**. The check runs before any comparison, since `metrics::paired` would otherwise
     compare only the items both sides have.
   - **Eligible**: errors under 2% of decisions, and decided correctly at least 0.80.
   - **Within the margin**: for each eligible judge j, the family-resampled 95% interval of
     BA(j) minus the highest BA among the eligible judges, with the highest taken again
     inside each resample (so the selection of the best is inside the interval); j is
     within the margin when that interval's lower bound is at least -3 points, its median
     time is at most twice the fastest eligible judge's, and its decided correctly is at
     least the best eligible decided-correctly minus 5 points.
   - **The pick** is the judge within the margin with the lowest cost per decision; ties go
     to the lower false-reject rate. No eligible judge, or no judge within the margin, is
     **no pick**.
   - **No pick keeps the current default** (`claude-haiku-5-5`) and says why.
   The output states the outcome, and for each judge each clause it passed or failed.
6. **The format rule, `submission/1`** (F2's, item 2), a pure function with tests, in `arms`
   mode.
7. **No matrix is frozen here.** F2's matrix and J1's are each committed and pushed to their
   own task's branch before its run, their identities in the PR body; F2's uses F1's arm
   syntax, and J1's uses the arm F2 settles.

   Only the five models this build prices can run (`docs/guides/config.md`); Sonnet and
   Opus have no price and a paid request with no bound is refused (section 6). Effort is
   each provider's default as the config guide states it.
8. **Arm C is `check` as it ships**: a test that the call a matrix makes for arm C is the
   call `whizbang squint check` makes with no option but the judge.

**Done when:** `judges::the_pick_is_the_cheapest_judge_within_the_margin`,
`judges::the_best_is_reselected_inside_each_resample`,
`judges::an_incomplete_matrix_is_no_pick`,
`judges::a_missing_or_interrupted_cell_is_no_decision_under_submission_1`, `judges::no_eligible_judge_keeps_the_default`,
`judges::selection_uses_repetition_one_only`,
`judges::a_run_without_rejections_is_unknown_not_clean`,
`judges::a_blind_set_is_read_through_gating_labelled`,
`judges::out_under_the_sets_or_labels_is_refused`,
`judges::the_output_holds_no_item_and_no_verdict_of_an_item`,
`metrics::paired_gives_the_decided_correctly_difference`,
`submission_rule::` tests (F2, item 2), `matrix::arm_c_is_check_as_it_ships`, and
`cli::eval_judges_scores_two_scripted_judges` (two scripted judges over a fixture set,
through the binary).

**Files:** `crates/whizbang/src/eval.rs`, `crates/whizbang/src/judges.rs` (new),
`crates/whizbang-eval/src/metrics.rs`, `crates/whizbang/tests/it/judges.rs` (new),
`docs/evalsets.md` (a section on the command). **Reviewer looks
hardest at:** the two rules against this text, and that nothing item-level reaches the output.

**Verify:** `just check`; `just test-only -p whizbang judges matrix::arm_c cli::eval_judges`;
`just test-only -p whizbang-eval metrics`.

## L1. The loop on segment-01: manifest, script, score and contact sheet (medium)

### Goal

Give the loop pictures where one pass tends to leave something behind, score each
attempt's mask against a hand-drawn truth mask, and put every finished picture in front of
a person, since the truth masks score the mask and not the picture.

1. **The data**: `segmentation/segment-01` of the public evaluation-pictures repository the
   sets already name (`images_repo = "github.com/markhayden/wafflery-evalsets"`;
   `SETS_REPOSITORY` in `crates/whizbang/src/evaluation.rs`): 20 pictures with truth masks,
   in four groups: whole 5, parts 6, thin 5, keep-the-hole 4. Each item has an id, a group,
   a prompt (the phrase), `items/<id>/image.png` and `items/<id>/truth.png`, each pinned by
   SHA-256 in the set's own `set.toml`. The set and its truth masks are public.
2. **The manifest**, `labs/e-loop/segment-01.toml`, in the sets' convention: the repository,
   `images_rev = "d5413a4f0806c0c8d4e0376eb71bffe3d541b98e"` (read on 2026-11-20), and for
   each of the 20 items its id, group, phrase, image path and SHA-256, truth path and
   SHA-256, copied from the set's `set.toml` at that revision; nothing of the set is changed.
   A test holds the manifest: 20 entries, groups of 5, 6, 5 and 4, unique ids, well-formed
   pins, no image pin equal to a truth pin.
3. **The pictures** are in a folder laid out as the repository
   (`<folder>/segmentation/segment-01/items/<id>/...`), outside the work tree; the
   orchestrator obtains it at the pinned revision. Nothing of it is committed.
4. **The evidence script, generalised.** `scripts/loop-evidence.sh` and `.py` take a manifest
   of kind `segment-01` beside the edit set's: the count is the manifest's own (20, not 24),
   only `image.png` files are imported, an entry that names a truth mask is refused, as
   today. `--repetition <n>` puts the repetition in the request id
   (`e-loop-<judge>-<n>-<id>`). **Before the first paid run** the script prepares one call
   and stops, having run nothing, unless its plan says checked and as many attempts as asked
   (3): the config's call cap must cover three attempts with the judge named
   (`docs/guides/loop.md`, "What it costs"). Every guard stays (heavy lock, memory watch,
   thread guard, binary of this tree, nothing paid twice). The tally gains a breakdown by
   group.
5. **`whizbang eval loop score --manifest <file> --set <folder> --store <dir> --judge <name>
   [--labels <file>] --out <file>`**, reading the store only. For each run of the manifest's
   request ids, for each attempt: the found mask's and the shaped mask's coverage, IoU and
   size as a multiple of the truth (`edits::coverage`, `iou`, `size_ratio`, reused), and the
   check's `RQ1` answer (confident violation, tentative violation, none). Truth masks are
   read from the folder by their pins and never imported. **Denominators.** Not every
   attempt has every artefact: a retry whose shaped mask is the attempt's before is recorded
   without a fill or a check, a failed shape or fill on a retry records an unchecked
   attempt, and one on the first attempt ends the run with no attempt recorded
   (`checked.rs`). So the score counts every requested run and every recorded attempt, and
   each measure says its own denominator: mask measures over the attempts whose mask
   exists, `RQ1` answers over checked attempts, picture measures over completed fills; an
   attempt without the artefact a measure needs is counted as unavailable for it, by why.
   Written as TOML (`loop-score/1`), by group and over all, aggregates only, every measure
   named for what it is:
   - mask coverage of attempt 1 and of the kept attempt (mean, least, share under-covered;
     the share also at 0.90 and 0.99, as sensitivity, never as the measure);
   - **the judge's `RQ1` answer against mask under-coverage**, on every checked attempt;
   - **mask coverage across a retry**: for each attempt after the first whose shaped mask
     exists, up by more than 0.01, within 0.01, down by more than 0.01, against the last
     earlier attempt whose shaped mask exists; and its size against that one;
   - **runs that ended with the kept attempt's mask under-covered**, by why the attempts
     ended (the tally's reasons, among them a stop for damage to what was kept, which is
     right to stop on);
   - whether the kept attempt had the highest mask coverage of its run.
   With `--labels`, the person's labels of item 6 (a file outside the repo), it adds, by
   group: the judge's `RQ1` answer against "still visible"; "still visible" and "damaged
   elsewhere" for the first completed fill and for the kept attempt; and, from one completed
   fill to the next completed fill of the same run, whether the picture went from visible to
   not, the reverse, or stayed. Without labels it makes no
   claim about the finished picture.
6. **`whizbang eval loop sheet --manifest <file> --set <folder> --store <dir> --judge
   <name> --out <dir>`**: the contact sheet, one static page with, for every completed fill
   of every run, the source and the filled picture side by side (each re-encoded from pixels),
   in an order drawn from a seed and with no verdict, coverage, attempt number or judge on
   the page, so the person who looks is blind to what the loop and the judge said. Each
   picture has two marks a person may set: "the named thing is still visible" and "something
   else was damaged". The marks leave only as a file the person saves through the browser
   (as the pairs page's picks do); `assignment.json` beside the page maps each picture to
   its run and attempt. Written only under `--out`, refused inside a git work tree (as
   `eval edits draw` refuses); built like the pairs page (no script but its own, by digest;
   every string escaped). Never committed: the fills are new pictures, not the set's.

**Done when:** `loop_score::coverage_is_the_share_of_truth_under_the_shaped_mask`,
`loop_score::rq1_answers_are_counted_against_under_coverage_once_per_checked_attempt`,
`loop_score::a_retry_is_counted_up_within_or_down_by_its_coverage`,
`loop_score::an_unchanged_mask_retry_counts_as_an_attempt_with_no_fill`,
`loop_score::a_failed_shape_or_fill_is_unavailable_for_the_measures_that_need_it`,
`loop_score::a_first_attempt_that_fails_gives_no_picture_measures`,
`loop_score::fills_one_and_three_around_an_unchanged_mask_retry_make_one_transition`,
`loop_score::a_truth_mask_is_read_by_its_pin_and_never_imported`,
`loop_score::without_labels_no_claim_about_the_picture_is_written`,
`loop_score::labels_add_the_visible_and_damaged_tables`,
`loop_score::the_output_holds_no_item`, `loop_sheet::refuses_a_folder_inside_a_work_tree`,
`loop_sheet::the_page_shows_no_verdict_coverage_or_attempt`,
`loop_sheet::every_string_is_escaped`,
`labs::the_segment_01_manifest_has_twenty_in_four_groups`, the script's tests
(`scripts/tests/test-loop-evidence.sh`: a manifest of 20, a repetition in the request id, a
truth mask refused, a cap that plans fewer than three attempts stops before any run), and
`cli::eval_loop_score_reads_a_scripted_loop_with_one_retry` (fake segmenter, scripted filler
and judge, 64 by 64 pictures).

**Files:** `labs/e-loop/segment-01.toml`, `labs/e-loop/config.toml` (a template without
keys, as `labs/m4-loop/config.toml`), `scripts/loop-evidence.sh`, `scripts/loop-evidence.py`,
`scripts/tests/test-loop-evidence.sh`, `crates/whizbang/src/eval.rs`,
`crates/whizbang/src/loop_score.rs` (new), `crates/whizbang/src/loop_sheet.rs` (new), tests;
`docs/guides/loop.md` (a short section on measuring the loop). **Seam** if it passes about
2,500 lines: the sheet (item 6) into its own PR, landing before L2. **Reviewer looks hardest
at:** no truth mask reaches the loop; the sheet is blind; the output is aggregates only.

**Verify:** `just check`; `just test-only -p whizbang loop_score loop_sheet labs cli::eval_loop`;
the script tests run by `just check` (it changes `scripts/`).

## F1. Submission v2 (large)

### Goal

Make the structured answer easier for a small model to produce, and accept near-misses that
are unambiguous, recording each. v1 stays exactly as it is, the default, and the one the
baseline replay uses.

1. **The setting `submission`**, `v1` (default) or `v2`, on `check` and the five use-case
   recipes, in the decision cache's key and the run's resolved settings; in a matrix arm as
   `C;submission=v1` or `C;submission=v2` (`crates/whizbang-eval/src/matrix.rs`), in the
   arm's identity only when given, so every existing matrix keeps its identity. Both values
   are passed through to the call when named. An arm that names none runs the default of
   the binary, and each run records the value it resolved; a matrix resumed on a binary
   whose default differs from the one its completed or failed cells resolved (both are kept
   on a resume, and both are scored) is refused before
   anything runs, naming the setting, so one arm identity never holds two formats. F2's
   and J1's matrices name the value explicitly.
2. **v2: every problem at once.** `decided_of` collects every problem of a submission, not
   the first, and the repair lists them (at most 12, then a count), each naming only ids the
   judge was given. Still one repair, and the repaired submission is a whole submission, as
   in v1.
3. **v2: near-misses accepted, each recorded** in T1's diagnostic as a recovery, by kind, and
   counted by S1. Each is read only where one reading is possible:
   - `items` or `findings` given as a string that holds a JSON list: read as that list;
   - an enum word differing only in case, surrounding space, or a space or hyphen for an
     underscore, and `can't tell` or `cannot_tell` for `cant_tell`: read as the word;
   - a checklist id differing only in case or surrounding space, when exactly one id
     matches;
   - two answers of one item identical in disposition, confidence, location and
     `allowed_by`: read as one, the first reason kept;
   - `evidence` that cites a call that did not complete, or `null`: the id is dropped and
     recorded, not rejected (evidence does not enter the verdict);
   - the submission wrapped once in an object whose only key is `submission` or `verdict`:
     unwrapped.
   A recovered answer is described as accepted under v2; nothing claims it is the verdict a
   v1 repair would have reached.
4. **v2: a schema keyed by checklist id.** The tool the judge is given lists the job's
   checklist ids as the required keys of an object `items` (each value an item answer
   without `id`), so a missing, duplicate or unknown item cannot be written. `findings` is
   unchanged. The schema is written per job; the cache key already holds the tools sent.
   The adapters send it as they send v1 (strict off for OpenAI, definitions written in place
   for Google), with a conformance fixture of v2's form for each. The rest of `decided_of`'s
   rules hold unchanged.
5. **The second look under v2.** The second look (off in arm C, on or strict when a run
   asks) keeps its own turn, tool and contract unchanged: it asks the tentative items in the
   array form `merged` reads, under v1's item schema, whatever `submission` is, and v2
   applies to the first submission and its repair only. The first submission's v2 answers
   are turned into v1's array form before the second look reads them, so `tentative_items`,
   `merged` and the strict rule see what they see today.
6. **What does not change.** v1's schema, description, messages and its one repair, byte for
   byte; the baseline replay on v1, held by its gate test; the turn budget.

**Done when:** a test per kind (`submission::v2_reads_items_given_as_a_string_holding_a_list`,
`..._enum_words_by_case`, `..._a_checklist_id_by_case_when_one_matches`,
`..._identical_duplicates_as_one`, `..._drops_evidence_of_a_call_that_did_not_complete`,
`..._unwraps_one_wrapper`), and the refusals that stay
(`submission::v2_never_recovers_a_differing_duplicate`, `..._an_ambiguous_id`,
`..._a_violation_without_a_location`); `submission::v2_names_every_problem_in_one_repair`;
`check::v2_keyed_schema_lists_the_jobs_checklist_ids`;
`check::v2_recoveries_are_recorded_by_kind`;
`check::v2_with_the_second_look_on_settles_the_tentative_items`,
`check::v2_with_the_second_look_strict_keeps_the_strict_rule`;
`cache::the_submission_setting_is_in_the_key`;
`matrix::an_arm_with_submission_v1_or_v2_parses_and_names_it_in_its_identity`;
`evaluation::a_resume_under_another_default_submission_is_refused`,
`evaluation::a_resume_with_only_failed_cells_under_another_default_is_refused`;
`matrix::an_arm_without_submission_keeps_its_identity`; each adapter's v2 conformance fixture;
the baseline gate test unchanged and passing; `cli::check_with_submission_v2_under_a_scripted_judge`.

**Files:** `crates/whizbang-squint/src/use_case/submission.rs`, a v2 tool file beside
`submit_verdict.json`, `use_case/mod.rs`, `check.rs`, `cache.rs`, the five recipe files,
`crates/whizbang-eval/src/matrix.rs`, the adapters' conformance fixtures, tests;
`docs/reference/tools/<use case>.md` (the setting; generated parts by `just docs`); ADR 0042,
"Submission v2". **Seam** if it passes about 2,500 lines: item 4 into its own PR, landing
before F2. **Reviewer looks hardest at:** every recovery has one reading; v1 is byte for byte
unchanged.

**Verify:** `just check`; `just test-only -p whizbang-squint submission check cache`;
`just test-only -p whizbang baseline cli::check_with_submission`.

## F2. The format round and the decision (small to medium; the orchestrator's run)

1. **The run**, in the evaluation store, after the prices and caps step ("How the work is
   split"): `matrices/e-submission.toml`, committed to this task's branch before the run: the
   ten dev sets, arms `C;submission=v1` and `C;submission=v2`, the three small judges, two
   repetitions: 1,320 cells.
   Scored by `whizbang eval judges --mode arms --rule submission/1 --matrix
   matrices/e-submission.toml --store <store> --labels <owner storage> --out
   docs/results/e-submission.toml`.
2. **The decision rule, `submission/1`, frozen here.** First, S1's completeness check: all
   1,320 cells have a last attempt that completed or failed, and every item a label;
   otherwise the outcome is **no decision**, and v1 stays. Submission counts in (a), (b) and
   (d) are pooled over the three small judges and both repetitions (660 decisions an arm);
   the accuracy figures in (c) use repetition 1, as every selection measure does (S1,
   item 2).
   - (a) **Fewer failures, measurably.** If v1 has at least 5 submission failures, v2's count
     is at most half of v1's. Else, if v1 has at least 10 first rejections, v2's first
     rejections are at most half of v1's. Else the outcome is **too few failures to tell**,
     and v1 stays.
   - (b) **No new failures**: for each judge, v2's submission failures are no more than v1's.
   - (c) **Accuracy kept**: for no judge is v2's balanced accuracy lower than v1's by more
     than 2 points, nor its false-reject or false-accept rate higher by more than 2 points
     (point estimates; the family-resampled intervals reported beside them).
   - (d) **Unknowns**: a decision with no T1 diagnostic in either arm makes the outcome **no
     decision**, and v1 stays.
   v2 is adopted when (a), (b) and (c) hold. The record names the clause that settled it.
   The rule decides the format for every judge: J1 then runs the two judges F2 did not
   (`gemini-4-argon`, `gpt-6.1-sol`) under the settled format only, and its pick rule reads
   every judge under that one format, so a judge that does worse under v2 shows it there.
3. **The record.** `docs/results/e-submission.md`: per model, before and after, the
   submission failure rate, the first rejection rate, rejections by class, recoveries by
   kind, balanced accuracy, cost per decision, self-agreement under each arm; the decision
   and its clause; descriptive, admits nothing.
4. **If adopted:** the default of `submission` becomes `v2`: `check`'s constant and the
   `submission` of the judge's step in all five recipe files, which state `v1` themselves
   (changing the constant alone leaves every recipe, and so every door, on v1); ADR 0042
   amended with the evidence; the use-case records' "Mark and history"; and
   `docs/guides/config.md`'s note on small models. The baseline replay stays on v1.

**Done when:** the record is committed, `evidence::the_submission_record_holds_only_aggregates`
passes, and, if adopted, `check::the_default_submission_is_v2` and the baseline gate test pass.
**Reviewer looks hardest at:** the rule applied as frozen.

## J1. The judge round under the settled format, and its record (medium; the orchestrator's run)

1. **The matrices, committed to this task's branch before the run**, with the arm F2 settled
   (`C;submission=v1` or `C;submission=v2`, named explicitly; called the settled arm below):
   - `matrices/e-judges-dev.toml`: the ten dev sets, the settled arm, `gemini-4-argon` and
     `gpt-6.1-sol`, one repetition. The three small judges' dev cells are F2's, of the same
     arm, repetition 1.
   - `matrices/e-judges-blind.toml`: `source-fidelity/blind-v3` (143 items, 24 families),
     the settled arm, all five judges, one repetition.
   So every judge is scored on the same 253 items under one arm.
2. **The run.** In the evaluation store, after the prices and caps step, the pictures of
   `blind-v3` fetched by pin (`whizbang eval sets fetch`, the origin already approved; the
   dev sets' pictures are there from F2), under a config with the five judges and the three
   providers as recipients (approved for milestone 2's evaluation and milestone 3's gate):

   ```
   whizbang eval matrix --matrix matrices/e-judges-dev.toml --store <store> --config <file> --env-file <file>
   whizbang eval matrix --matrix matrices/e-judges-blind.toml ...
   whizbang eval judges --mode judges --arm <settled arm> --rule judge-pick/1 \
       --matrix matrices/e-submission.toml --matrix matrices/e-judges-dev.toml \
       --matrix matrices/e-judges-blind.toml --store <store> --labels <owner storage> \
       --out docs/results/e-judges.toml
   ```

   `<owner storage>` is the owner's folder outside the repo that holds `blind-v3`'s labels;
   its path is never written down. Failed cells are not retried (a matrix never retries a
   failure; carry-over row): they count as errors, and the 2% clause sees them.
3. **The record.** `docs/results/e-judges.md` beside the TOML: what was run (commit, binary,
   matrices and identities, sets and their digests, dates, settled spend), the table of the
   five judges (balanced accuracy and decided correctly with their intervals, unsure,
   errors, submission failures and first rejections by class, cost per decision and per
   1,000 checks, the judge's share of one loop attempt, median time, self-agreement where
   there is one), the outcome under both scopes with each judge's clauses, and **the
   trade-off in one paragraph**: what the cheaper choice gives up, in points, and what the
   dearer one costs, in dollars per 1,000 checks. It states that the evidence is descriptive
   (dev sets and an exposed set), that `blind-v3`'s roughly 40 items an agent saw count as
   dev, and that it admits nothing. The page's tables are written from the TOML, or held to
   it by a test that reads both.
4. **ADR 0041, "The default judge"**: the outcome for `check` and for the loop (the same
   model unless the source-fidelity scope picks another), under the settled format, the
   rule, the evidence, and that it is chosen again when a judge is added, a price changes by
   more than half, the format's default changes, or a later slice gates on fresh data.
   (Note, 2026-11-21: written as ADR 0044, `docs/adr/0044-the-default-judge.md`, since
   0041 to 0043 were taken before J1.)
   (Note, 2026-11-22: the round picked over all sets and found no pick over
   `source-fidelity`; the loop's judge is `check`'s pick, carried over, and not S1 item 5's
   kept default, which was ineligible in that scope. ADR 0044, "Decision", item 2.)
5. **The documents.** `docs/guides/config.md`: the first example names the pick, the others
   follow; `docs/guides/loop.md`: the default judge, and the call cap three attempts need
   with it; each use case's record, `docs/reference/tools/<use case>.md`, "Alternatives"
   (the five judges on that use case's sets, from the TOML) and "Speed and cost";
   `docs/reference/tools/remove_checked.md`: the loop's judge.

**Done when:** the matrices, TOML, page and ADR are committed, and
`evidence::the_judges_record_holds_only_aggregates` passes. **Reviewer looks hardest at:** no
item, no blind label, no owner path; every judge under the one settled arm.

## L2. The loop round, the contact sheet, and the slice's note (medium; the orchestrator's run)

1. **The runs**, in a new store, under the judge the rules picked for the loop and the format
   F2 settled, with SAM 3 on the Mac and Bria Eraser through fal, two repetitions. The
   config's `call_cap_usd` is the three-attempt bound of that judge in the binary that runs
   the round, plus a cent (the prices and caps step; at 722083fc's prices, 0.46 with Haiku,
   2.30 Flash, 3.03 Terra, 5.93 Argon or Sol), and the script's own preflight (L1, item 4)
   refuses to start unless the plan is three checked attempts:

   ```
   WHIZBANG_THREAD_LIMIT=200 scripts/heavy.sh e-loop scripts/loop-evidence.sh \
       --binary <whizbang> --store <store> --set <folder> --manifest labs/e-loop/segment-01.toml \
       --judge <judge> --repetition <1|2> --config <file> --env-file <file> \
       --out docs/results/e-loop-<judge>-<n>.toml
   whizbang eval loop score --manifest labs/e-loop/segment-01.toml --set <folder> \
       --store <store> --judge <judge> --out docs/results/e-loop-score-<judge>.toml
   whizbang eval loop sheet ... --out <a folder outside the repo>
   ```

   Then the same under `claude-haiku-5-5`, milestone 4's first judge, so the two can be read
   side by side (or under `gpt-6.1-terra` if Haiku is the pick).
2. **The contact sheet** is written outside the repo and given to the owner (an owner item,
   below; not a blocker). If the owner marks it, the marks file stays in owner storage and
   `eval loop score --labels` adds the tables about the finished picture; only those
   aggregates are committed.
3. **The record.** `docs/results/e-loop.md` beside the TOMLs: how many runs retried, by kind
   and group; mask coverage across retries; the judge's `RQ1` answer against mask
   under-coverage; with the owner's marks, against "still visible"; the runs that ended
   under-covered and why; spend. Descriptive: the loop's rung 2 in part, admits nothing. It
   says plainly when segment-01 rarely reached a retry (fewer than 5 runs of the 40 under
   the chosen judge).
4. **The documents.** `docs/reference/tools/remove_checked.md` ("Rung 2 results", "Limits",
   "Mark and history"); `docs/guides/loop.md` if a number it states moved; the carry-over
   row "The loop's retries, wider and located, have no live evidence" closed or narrowed with
   its line.
5. **The slice's note**, `docs/plans/e1-note.md`: the three outcomes, spend by round against
   this plan, the timing report, and the carry-over rows opened: the gate on fresh data owed
   to the default judge and to v2; the owner's photos if not used; `SETS_REPOSITORY` and the
   sets' `images_repo` still naming the repository's old name; anything a round found.

**Done when:** the records are committed;
`evidence::the_loop_records_hold_only_aggregates_by_group` passes; the note is in.
**Reviewer looks hardest at:** aggregates only, no picture, no owner path.

## Commit strategy

- One task, one worktree (`scripts/worktree.sh add <slug>`), one branch, one PR, built tests
  first, pushed once after one full `just check` on the finished tree, reviewed whole,
  merged by the orchestrator on green CI and both scans (`just scan-range`). Never a commit
  to `main`, never `--no-verify`.
- An ADR is in the PR of the task that names it (0041 in J1, 0042 in F1, amended in F2).
- Every state of `main` between tasks passes `just check`, and `check` with no new option
  answers the judge exactly as at 722083fc until F2 changes its default on evidence (T1 adds
  a diagnostic, not a word to the conversation).
- **Rollback checkpoints.** The tag `m4` (exists). `e1-tools`, on the last of T1, S1, L1 and
  F1 to merge: the slice's code, no default changed, no round run. `e1`, on L2's merge. A pushed tag is
  never moved.
- If a task shows a decision here is wrong, this file is changed first, in its own PR.

| Task | To undo |
|---|---|
| T1 | Revert the merge: one diagnostic and a `runs stats` option; the judge sees nothing new |
| S1 | Revert the merge: a reporting command, one statistic and matrix files |
| L1 | Revert the merge: the script's edit-set mode is unchanged; the rest is new commands |
| F1 | Revert the merge: v2 is a setting no default uses before F2 |
| F2 | If adopted, revert its merge, or set `submission = "v1"` per call without a revert; J1's record then names a format that is no longer the default, and its pick is taken again |
| J1 | Revert the merge: the guides name `claude-haiku-5-5` again |
| L2 | Records and a note |

## Spend

All on approved recipients, on the owner's Mac, by the orchestrator. These are estimates
from settled rates, not limits: the limits are each store's allowance and each call's cap.
Per-decision rates from `docs/baseline/manifest.toml`, arm C: Haiku 0.0033, Terra 0.017,
Flash 0.0228, Argon 0.0297 (about double once its introductory price ends on 2026-11-30;
the prices and caps step), Sol 0.0336; Bria Eraser 0.04 a fill. A round that passes twice its estimate stops
and is said; the slice stops at 90 dollars settled in all, and the orchestrator asks before
going on.

| Round | What is called | Estimate (dollars) |
|---|---|---|
| T1, S1, L1, F1 | Nothing paid: fakes; SAM 3 under `just test-models` | 0 |
| F2, `e-submission` | 220 decisions each of the three small judges under each of two arms | 19.0 |
| J1, `e-judges-dev` | 110 decisions each of Argon and Sol | 7.0 (10.2 after Nov 30) |
| J1, `e-judges-blind` | 143 decisions of each of the five | 15.3 (19.5 after Nov 30) |
| L2 | 20 pictures, 2 judges, 2 repetitions: 80 runs, about 0.06 a run settled, most of a run's 3 fills and 3 checks only where it retries | 5 (up to 18 if every run made three attempts with a dear judge) |
| Optional: the owner's 3 to 5 photos | 2 judges, one repetition | 2 |
| One rerun of a round after a fix | Allowance | 10 |
| | **Expected** | **about 46 (54 after Nov 30); about 80 if L2 retries on every run, the price rises and one round is rerun, of the 100** |

What a matrix forecasts and a loop reserves is more than it settles (a matrix's forecast
doubles the settled rate; a loop reserves its whole plan's bound). A matrix's forecast is
worked from the study's fixed rates (`matrix::forecast`), not from the build's prices, and
each matrix is set against what its store has left when it starts. In the evaluation store
(70): `e-submission`'s forecast is about 38; then, with about 51 left, `e-judges-dev`'s is
about 14; then, with about 44 left (about 41 after the Argon rise), `e-judges-blind`'s is
about 31. The price rise changes what is settled and each call's bound, not the forecast.
The loop store's 25 covers L2's settled spend at most. A reset replaces what is left rather
than adding to it, so if a rerun needs more, the owner is asked for an exact new allowance;
the milestone's spend is the sum settled across every store and reset, kept in the note.

## Dependencies

```
T1 ──► F1 ──┐
S1 ─────────┴──► F2 ──► J1 ──► L2 ──► the note
L1 ────────────────────────────┘
```

- F1 after T1: v2's recoveries are written in T1's diagnostic.
- F2 after S1 and F1: it runs v2 and is scored by S1.
- J1 after F2: it runs under the format F2 settled, and reads F2's cells of the small judges.
- L2 after L1 and J1: it runs under the picked judge and the settled format.

## Risks

- **The dev sets are small** (110 items, 30 families; 5 families a use case), so intervals
  are wide and several judges may fall within the margin; then cost decides, which is the
  intent. The best is chosen again inside each resample so its selection is in the interval.
- **`blind-v3` is exposed** and source-fidelity only, 143 of the 253 items. The record gives
  the dev-only and blind-only figures beside the pooled ones and says if the pick would
  differ by scope.
- **Haiku's unsure rate.** Its arm C balanced accuracy on the dev sets was 0.529 in
  milestone 3 because it sends sound pictures to review; the decided-correctly clauses keep a
  cheap judge that rarely decides from being picked. In the loop an `unsure` with no
  violation ends the attempts (row 4), so this matters twice there.
- **Submission failures are rare** (about 5% of Haiku's decisions in milestone 2). 660
  decisions an arm give tens of rejections; the rule's minimum counts turn too few into "too
  few to tell", never into a change. No per-model claim is made from these counts.
- **v2 is a bundle**: all problems at once, recoveries and the keyed schema are measured
  together. Recoveries are counted by kind, so the record shows which were used; separating
  the schema's effect is a later slice's, if the bundle fails or a narrower v2 is wanted.
- **The keyed schema is new per job.** Hits in the decision cache are per schema; a
  provider's strict mode may reject a form; each adapter has a v2 conformance fixture.
- **segment-01 may rarely reach a retry**: a retry needs a confident `RQ1` violation, which
  no milestone 4 check gave. Then the finding is how often a mask was under-covered and the
  judge did not say so; the owner's photos are the optional fallback.
- **Mask coverage is not the finished picture.** It is named for what it is everywhere; the
  claims about the picture wait on the owner's marks of the blind contact sheet, and are not
  made without them. Hand-drawn truth edges are imprecise: hence 0.95, with 0.90 and 0.99 as
  sensitivity.
- **Phrases with two parts** (two objects separated by a full stop in segment-01) are passed
  to `remove_checked` as the set gives them; how SAM 3 reads them is part of what the round
  measures. No splitting is added.
- **The pictures' repository was renamed**; the sets and `SETS_REPOSITORY` name the old name,
  which redirects. L1 follows the sets' convention; if the redirect stops, the orchestrator
  reads the folder by `--set`. A carry-over row, opened by the note.

## What the owner provides

- **Needed before the first paid round: I need your yes to give the two new stores of this
  slice their first allowances, 70 dollars for the evaluation store and 25 for the loop
  store (yes/no).** Setting an allowance is a reset, which is yours (section 6). Nothing is paid
  for without it; the code tasks do not wait on it.
- **Optional, not a blocker: I need 3 to 5 of your own photos, each with the phrase of what
  to remove, in your owner storage (yes/no).** Used only if segment-01 under the chosen judge
  reaches fewer than 5 retries in 40 runs, or as a real-world check before the defaults are
  locked. Only aggregates are committed; never a picture, a phrase or a path.
- **Optional, not a blocker: I need you to mark the loop's contact sheet, about 20 minutes:
  for each finished picture, whether the named thing is still visible and whether something
  else was damaged (yes/no).** Without it the record makes no claim about the finished
  pictures.

## What the owner is told, not asked

- The default judge and the format are chosen by the rules frozen above, on descriptive
  evidence; neither is "admitted". The gate on fresh data that would admit them is owed to a
  later slice of E.
- The format is settled before the judge, so the default judge is picked under the default
  format.
- Expected spend about 46 dollars (54 after Argon's price rise), about 80 at most likely, of
  the milestone's 100.

## Review rounds

### Review round 1

On draft 1. Verdict: revise. Sixteen findings: 2 blockers, 11 majors, 3 minors. Each was
checked against the code; fourteen taken in full, two in part.

1. **Blocker: an allowance reset is the owner's.** Taken in part in draft 2 (new stores, no
   reset of an existing one); taken in full in draft 3 after round 2 (its finding 1).
2. **Blocker: a 3.50 cap plans one attempt with Argon or Sol.** Right (`docs/guides/loop.md`).
   Taken: the cap is set from the table for the picked judge, and the script prepares one call
   and refuses to start unless it plans three checked attempts (L1, item 4; L2, item 1).
3. **Major: rejections are kept only on a completed run, and `judge_output` mixes causes.**
   Right (`check.rs`, `conclude` and the early returns). Taken: new task T1 records classes
   and causes on every exit, before J1; a run without it is unknown.
4. **Major: partial repair cannot reuse `merged`.** Right. Taken by dropping partial repair
   (also finding 14): a repair is a whole submission, as in v1.
5. **Major: Wilson is not an interval for balanced accuracy.** Right. Taken: family-resampled
   intervals for balanced accuracy and decided correctly; counts beside rates.
6. **Major: S1 lacks the arm comparison F2 needs, and `paired` has no decided-correctly
   difference.** Right. Taken: modes `judges` and `arms`; the statistic added with a test.
7. **Major: repetitions weight judges differently.** Right. Taken: selection on repetition 1
   only; repetition 2 for self-agreement and submission measures; spend apart.
8. **Major: the pick rule has undefined outcomes and ignores selection.** Right. Taken:
   completeness check, "no pick keeps the current default", eligibility, and the best chosen
   again inside each resample.
9. **Major: the format rule could adopt new failures, or "halve" zero.** Right. Taken:
   minimum counts, "too few to tell", no new failures per judge, unknowns.
10. **Major: the final judge and format may never be run together.** Right. Taken in draft 2
    by running a non-small pick under v2; replaced in draft 3 by settling the format first
    (round 2, its finding 2).
11. **Major: mask coverage is treated as the finished picture.** Right. Taken: measures named
    for the mask; the contact sheet made blind with two marks; claims about the picture only
    from the owner's marks.
12. **Major: prices are compiled; a change must be made before the run.** Right. Taken: the
    prices and caps step, before each paid round, as plan m4's E1 did.
13. **Major: the new command lacks the gate's protections for labels and output.** Right.
    Taken: `gating::labelled`, digests recorded, output under the sets or labels refused.
14. **Minor: F1 bundles too much; "without a changed verdict" has no counterfactual.** Taken
    in the parts that matter: telemetry first (T1), partial repair dropped, the wording
    fixed. Not taken: building v2 only after J1's classes are read, which would put F1 after
    J1 and the whole slice in series for a smaller change; the bundle is stated as a risk.
15. **Minor: bare `cargo nextest`.** Right. Taken: `just test-only`.
16. **Minor: gate figures from two runs.** Right. Taken: both from the fourth run.

### Review round 2

On draft 2. Verdict: revise. Five findings: 1 blocker, 3 majors, 1 minor. All five were
checked against the code and spec and taken.

1. **Blocker: setting a new store's allowance is still a reset, and a reset is the owner's.**
   Right: `whizbang spend reset` is the only way to set one, and section 6 names no exception
   for an empty store. Taken: the two first allowances and a possible top-up are an owner
   item, needed before the first paid round; the code tasks do not wait on it.
2. **Major: the final pick could still mix formats, or change the population.** Right. Taken
   by reordering the rounds: F2 settles the format on the small judges first; J1 then runs
   Argon and Sol on the dev sets, and all five on `blind-v3`, under the settled arm, and
   `eval judges --arm` scores every judge under that one arm on the same 253 items. F2's old
   item 3 and the second pick are gone.
3. **Major: the keyed v2 schema conflicts with the second look's subset and `merged`.** Right.
   Taken: F1, item 5: the second look keeps its turn, tool and array contract under v2; the
   first submission's v2 answers are turned into v1's form before it reads them; tests under
   `on` and `strict`.
4. **Major: fixed caps break once prices change, and price dates were checked before J1
   only.** Right. Taken: the prices and caps step runs before F2, J1 and L2 each, and every
   round's cap is derived from the bounds of the binary that runs it.
5. **Minor: the contingency budgeted one non-small pick of two.** Moot after finding 2: the
   contingency is gone, and J1 runs both Argon and Sol, budgeted.

### Review round 3

On draft 3. Verdict: revise, with no blocker. Five findings: 3 majors, 2 minors. All five were
checked against the code and taken.

1. **Major: bare `C` changes meaning once the default changes.** Right (an implicit setting is
   not in a cell's identity). Taken: F2's and J1's arms name `submission` explicitly, both
   values are passed through, and a resume under another default is refused, with a test.
2. **Major: `submission/1` had no completeness check, and `paired` intersects items.** Right.
   Taken: one completeness check shared by both rules, run before any comparison; F2 needs
   all 1,320 cells.
3. **Major: not every attempt has a mask, a fill or a check.** Right (`checked.rs`: an
   unchanged-mask retry is not filled; a failed shape or fill is unchecked). Taken:
   denominators per measure, unavailable outcomes by why, transitions across completed
   fills, the sheet of completed fills only, and three fixtures.
4. **Minor: repetitions for the format's accuracy were stated two ways.** Taken: accuracy on
   repetition 1, submission counts over both.
5. **Minor: the forecast and the top-up did not match the code.** Right (`matrix::forecast`
   uses fixed study rates; each matrix is checked against what is left; a reset replaces).
   Taken: the forecasts worked per matrix, no top-up needed at either price, and a rerun's
   new allowance asked as an exact amount.

### Review round 4

On draft 4. Verdict: revise, with no blocker. Three findings: 1 major, 2 minors. All taken.

1. **Major: the resume guard looked at completed cells only.** Right (a resume keeps failed
   cells, and they are scored). Taken: the guard reads completed and failed cells, with a
   failed-only fixture.
2. **Minor: one loop fixture was unreachable** (a first attempt's failed shape or fill ends
   the run). Taken: two reachable fixtures in its place, and the wording on unchecked
   attempts limited to retries.
3. **Minor: the balance before `e-judges-blind` was understated.** Taken: about 44, or 41
   after the Argon rise.

Round 4 found no blocker. Its one major was a narrow gap in a guard, fixed as the reviewer
proposed; under the review bar, this plan goes forward on draft 5.

