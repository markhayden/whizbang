# Milestone 4 note

Milestone 4, "The loop and a release", ends with task N1 of
`m4-the-loop-and-a-release.md`. This note was written by N1's pull request from its branch,
with `main` at 3ee263a8 (the merge of #257, task E1), which is also the commit of the
release `v0.1.0`. The note lands after that commit and is in no archive of `0.1.0`: it
records the release from outside (`docs/results/m4-release.md`). With it every box of the
plan is ticked: **milestone 4 is closed**, and the tag `m4` is made on this pull request's
merge.

## What whizbang can do at the end of milestone 4

Beyond milestone 3: one action removes what a phrase names and checks the result,
`remove_checked` (`whizbang rascal remove-checked`; `docs/guides/loop.md`). It runs
`remove_prompt`'s steps, asks squint's `source-fidelity` recipe whether the result is good,
and tries again at most twice more: with a wider mask once, or inside the places a judge
named. Each attempt and its check is a nested call under the loop, all on the one
reservation the loop made when it was admitted, and the number of attempts is settled
before anything runs and printed. `check` hands its caller its findings, the same from the
cache, and details first is no longer its default. And whizbang has a version: `0.1.0`, the
first release, whose two archives (the Mac on Apple silicon, Linux on x86-64) a person
downloads from the repository's GitHub releases, checks against `SHA256SUMS` and installs
by the guide inside it.

## What shipped

| Pull requests | Task | What |
|---|---|---|
| #245 | Plan | The plan of milestone 4, after four rounds of outside review |
| #249 | A1 | Details first is no longer `check`'s default; the lab's progress and counts; the pairs page |
| #247 | K1 | A call's plan settled at preparation, a recipe run from a named step, findings as a kind of result, the record `run/6` |
| #250 | K2 | `check` hands its caller its findings, the same from the cache; an executor built with a digest alone (K1's item 5) |
| #252, #253 | L1 | The loop `remove_checked` and its record; its doors: the command, the server, `runs stats` and the guide |
| #255, #256 | L2 | `carry`, a mask on a crop carried onto its picture; the located retry |
| #248 | R1 | The release archive, its checksums, the bootstrap and the install check, in CI's `release` job on each runner |
| #251, #254 | R2 | Version `0.1.0`, the `prerelease` and `release` workflows, the local leg, its receipt and the gate; the skill `release` |
| #257 | E1 | The loop run live by a committed script, three cases by hand, the loop's record, the spec's wording owed |
| This one | N1 | The release's record, this note, the boxes, the last row |

## The clauses of the spec's row, each with its evidence

The row for milestone 4 in `SPEC.md` section 16: "Remove with a check and bounded retries
runs as nested calls on one budget; the `prerelease` workflow passes and a release archive
installs and runs on both targets". Tests are named by module and function;
`scripts/tests/test-notes.sh` fails when one named here no longer exists. The model tier
(`model_tier::` tests marked ignored) runs under `just test-models` on the owner's Mac,
never in CI; the candidate's local leg ran it whole, 11 of 11.

| Clause | Shown by |
|---|---|
| Remove with a check | One attempt is a find, a shaping, a fill and a check: `checked::a_first_attempt_that_passes_is_one_find_one_shape_one_fill_and_one_check`, `checked::no_fill_is_asked_before_its_shaped_mask_is_in_hand`; the check is squint's recipe and nothing under it is the loop's: `checked::the_check_is_squints_recipe_and_the_loop_calls_nothing_under_it`, `checked::the_job_the_loop_writes_is_valid_and_holds_its_three_requirements`; what the check hands back: `check::a_checklist_item_and_a_separate_observation_are_each_handed_back_as_a_finding`, `check::a_check_answered_from_the_cache_hands_back_the_findings_of_the_run_that_answered`, `check::findings_carry_the_decision_of_the_whole_submission_whatever_was_cut`; a loop that cannot check says so or is refused: `checked::a_judge_that_is_not_ready_plans_one_attempt_unchecked_and_the_reason`, `checked::check_required_is_not_ready_at_prepare_naming_what_is_missing`, `checked::a_check_that_ends_in_error_leaves_its_attempt_unchecked_with_the_checks_own_error`; nothing outside the kept mask changes: `checked::every_pixel_outside_the_kept_mask_is_the_pictures_own`; the doors: `cli::rascal_remove_checked_prints_its_plan_each_attempt_and_whether_the_result_was_checked`, `serve::the_loop_is_offered_as_a_tool_and_answers_with_its_plan_its_verdict_and_whether_it_was_checked`, `stats::runs_stats_counts_a_loops_attempts_and_retries`; on SAM 3: `model_tier::remove_checked_makes_its_mask_with_sam3_under_a_scripted_judge`. Live: 48 runs under two judges and three cases by hand (`docs/results/m4-loop.md`, `evidence::the_loops_record_names_a_commit_of_this_tree_and_holds_only_aggregates`); the loop's record (`evidence::the_loops_tool_record_holds_rung_1_and_says_what_of_rung_2_is_shown`, `tool_records::recipes::the_record_of_remove_checked_is_what_it_gives`); and the loop checked once more by the candidate's local leg |
| and bounded retries | At most the plan's number of attempts, three by default, the first counted: `checked::the_attempts_never_pass_the_plans_number`; only a confident violation of `RQ1` retries, by the table: `table::row_2_a_confident_violation_of_rq1_with_no_place_widens_once_and_is_checked_again`, `table::row_2_a_second_widening_in_one_run_is_not_made`, `table::row_3_a_tentative_violation_of_rq1_enlarges_nothing`, `table::row_4_held_allowed_change_hidden_cant_tell_and_observed_each_enlarge_nothing`, `table::row_1_a_violation_of_a_protected_subject_of_the_geometry_or_of_rq2_ends_the_attempts`, `table::row_5_a_violation_of_rq3_alone_ends_the_attempts`, `table::row_0_findings_cut_short_end_the_attempts_whatever_was_kept`; the located retry and its bound: `located::a_confident_violation_of_rq1_with_a_place_segments_inside_it_and_the_mask_grows_there`, `located::a_place_of_a_tentative_violation_or_of_any_other_disposition_is_never_cut_segmented_or_painted`, `located::no_more_than_eight_places_are_used_and_the_plans_bound_covers_them`, `carry::a_mask_on_a_crop_lands_on_its_picture_and_is_empty_outside_the_crop`, `cli::rascal_remove_checked_retries_inside_a_named_place_under_a_scripted_judge`, `model_tier::a_located_retry_finds_the_phrase_inside_a_crop_with_sam3`; what is kept when every attempt fails: `checked::when_every_attempt_fails_the_loop_completes_with_the_kept_attempt_its_verdict_and_findings`, `checked::the_kept_attempt_is_the_first_pass_then_unsure_then_fewer_findings_then_the_earlier`. **No live run retried** (below): the retries are shown by these tests with fakes alone |
| runs as nested calls | Each attempt's steps and its check are child runs of the loop, and its deepest call is at the fourth level: `nested::the_deepest_call_of_a_checked_removal_is_at_the_fourth_level_and_is_admitted`; nothing may call the loop: `recipe::a_recipe_that_names_a_loop_as_a_step_is_refused_when_read`; a recipe run in stretches: `runner::a_run_from_a_step_runs_that_step_and_those_after_it_with_what_it_was_given`, `runner::a_run_from_one_step_until_another_runs_that_stretch_alone`; cancellation reaches every child: `checked::a_cancelled_loop_cancels_its_running_child_starts_no_other_and_commits_nothing`, `checked::attempts_completed_before_a_cancellation_are_in_the_record`; the record lists them: `record::a_loops_record_lists_each_attempt_its_runs_its_verdict_and_what_the_retry_changed`, `record::run_6_holds_a_plan_a_start_findings_and_a_loops_attempts` |
| on one budget | One reservation for the plan, on which every child draws: `budget::the_loop_reserves_once_for_its_plan_and_each_child_draws_on_it`, `budget::what_a_child_did_not_spend_is_back_before_the_next_attempt`, `budget::a_child_refused_for_want_of_allowance_ends_the_attempts_and_keeps_what_completed`, `budget::three_attempts_at_their_largest_stay_inside_a_trees_runs_and_a_runs_calls`; the plan settled at preparation and fitted to the call cap: `prepare::an_action_that_plans_is_prepared_with_the_needs_and_the_estimate_of_its_plan`, `prepare::a_plan_fitted_to_the_call_cap_says_what_was_asked_and_what_it_allows`, `budget::the_plan_is_the_most_attempts_the_call_cap_covers_and_says_asked_and_allowed`, `budget::a_cap_that_covers_no_check_plans_one_attempt_unchecked`; and held from preparation to admission: `prepare::the_plan_is_in_the_fingerprint_of_the_prepared_call`, `admit::a_preparation_whose_plan_would_now_differ_is_refused_as_stale`, `budget::a_judges_key_that_goes_between_prepare_and_admit_is_a_stale_preparation` |
| the `prerelease` workflow passes | The workflows' shape: `scripts/tests/test-release-workflows.sh`; the local leg against stand-ins: `scripts/tests/test-prerelease-local.sh`; a tag read: `scripts/tests/test-tag-version.sh`; the gate's every refusal: `scripts/tests/test-release-gate.sh`; publication: `scripts/tests/test-release-publish.sh`. Live: `prerelease` run 44779028704 succeeded on `v0.1.0-rc.1` at 3ee263a8, `build` and `install` on both runners and then `candidate`; the local leg passed on the owner's Mac and its receipt is attached to the candidate; the `release` run 44780876409 passed its gate and published (`docs/results/m4-release.md`; `evidence::the_release_record_names_a_candidate_whose_commit_is_of_this_tree`, `evidence::the_release_record_holds_every_required_check_of_the_receipt_as_passed`) |
| and a release archive installs and runs on both targets | The archive: `scripts/tests/test-package.sh`, `scripts/tests/test-third-party-licences.sh`; the bootstrap and the install check: `scripts/tests/test-install-prereqs.sh`, `scripts/tests/test-install-check.sh`, held in CI by `scripts/tests/test-ci-jobs.sh`; the binary: `cli::the_version_line_names_the_version_and_the_commit`, `release::the_linux_binary_needs_only_the_c_library_of_ubuntu_22_04`, `release::the_mac_binary_states_macos_15_as_its_minimum`; Linux refuses the extra in plain words: `setup::the_extra_is_for_the_mac_alone_and_the_lock_names_no_nvidia_or_cuda_package`. Live: the `install` job of run 44779028704 installed the candidate's archive and ran it on `macos-15` and on `ubuntu-22.04`, as CI's `release` job does on every pull request; the local leg ran the Mac's archive with SAM 3, a fill, three judges and the loop; and the published release holds the same bytes, held by `scripts/release-publish.sh check`. On Linux "runs" means what the plan's open question 2 asked: it installs, runs a recipe that needs no model, serves a call over stdio, installs the pack's core and refuses what needs a local model |

## The release, and the loop live, in numbers

**The release** (`docs/results/m4-release.md`). `v0.1.0`, of 3ee263a8, on `main`, from its
one candidate `v0.1.0-rc.1`, published on 2026-11-20 as a pre-release with the two archives
and `SHA256SUMS`, which is the candidate's byte for byte. Before the candidate's tag, CI on
the commit passed its eight checks and the range scan from `m3` was clean. The local leg's
model tests ran 11 of 11; its three judges answered `unsure` (Anthropic's), `pass`
(Google's) and `pass` (OpenAI's); its loop kept its first attempt, checked, `unsure`.

**The loop live** (`docs/results/m4-loop.md`). 48 runs, 24 under each judge, all completed
in one attempt, and none retried, of either kind. Under `claude-haiku-5-5` 1 passed and 23
kept an `unsure` on row 4 (no violation found); under `gpt-6.1-terra` 18 passed and 6 kept a
`fail` on row 1 (damage to something to be kept). No check found a confident violation of
`RQ1`, the one finding that asks for a retry. Of the three cases by hand, `pass` kept an
`unsure`, `vague` ended with the sentence of another step's refusal, since fixed and
held by a test, and `no-key` ran unchecked and said why.

## Spend

About 2.39 US dollars of the 100 allowed, all to approved recipients, each settled by a
store's ledger, against the plan's forecast of about 22 ("Spend"): E1's live runs,
2.281578 settled (1.045275 under Haiku, 1.153401 under Terra, 0.082902 for the three
cases); and the candidate's local leg, about 0.10: 0.063819 itemised by its receipt (the
fill 0.040000; the checks 0.002555, 0.012586 and 0.004548; the loop's check 0.004130) and
the loop's own fill, which the receipt does not itemise, estimated at Bria's 0.04. Nothing
else was paid: every other task ran on fakes, and SAM 3 runs on the Mac. One candidate was
enough where the plan allowed three, and nothing of E1 was run again.

## What the owner decided

On 2026-11-19, on draft 1 of the plan, "1, 2, 3 good":

- **The first version is `0.1.0`,** with candidates `v0.1.0-rc.<n>`, published on the
  repository's GitHub releases and marked as a pre-release.
- **Hosting SAM 3's files moves to milestone 6.** The release fetches the model from its
  maker with the user's own sign-in; the licence was read for what redistribution asks (E1).
- **No notarisation by Apple in this milestone,** with a carry-over row.

On 2026-11-20, "yes on all but lets push the optional to the phase E where we beat up each
tool individually":

- **The pre-release checks in two parts:** GitHub builds and installs on both targets; the
  model tests and the live calls are the local leg on the owner's Mac, whose receipt the
  gate reads. `SPEC.md` section 14 says so.
- **The spec's wording owed since K1 and L1:** section 3's Prepare bullet states a call's
  plan, and section 7 names `remove_checked` and `carry`.
- **The owner's own pictures move to milestone E,** where each tool is tested hard on its
  own.

And on 2026-11-20, after the release: **milestone E's first slice runs beside milestone 5**
(below).

## What a reader should not assume

- **The loop is experimental and unscored.** Its record holds rung 1 and rung 2 in part:
  preservation is shown, and each attempt's mask and fill are `remove_prompt`'s, whose
  rung 2 is milestone 3's. Whether a retry makes a result better is not measured, and its
  visual quality is not yet confirmed by a person.
- **No live run retried.** None of the 48 live runs, and not the local leg's loop, made a
  second attempt, so the wider and the located retry are shown only by tests with fakes and
  by one model test on a scripted judge. The owner's own pictures, where one pass leaves
  something behind, are owed by milestone E.
- **The first judge was unsure 23 times in 24.** `claude-haiku-5-5`, the cheaper judge,
  kept an `unsure` on 23 of its 24 runs, and an `unsure` with no violation ends the
  attempts; it answered `unsure` in the local leg too. A loop under it mostly stops after
  one attempt without a pass. `gpt-6.1-terra` passed 18 and failed 6.
- **Linux runs no local model.** The Linux archive installs, serves and runs what needs no
  model, and refuses `setup --extra`, `remove_prompt` and the loop in plain words
  (`SPEC.md` section 5; the carry-over row "Linux runtime extra").
- **The Mac's binary is not notarised,** and is not signed with a developer identity. It
  needs macOS 15 or later on Apple silicon. A browser's download is refused by the system;
  the install guide downloads with `curl`, which sets no mark.
- **The receipt is not signed.** The gate holds a release to what the orchestrator did, not
  against the orchestrator (the skill `release`; the carry-over row on a receipt written by
  hand).
- **`0.1.0` is a pre-release,** from one candidate; nothing was released before it.

## What moved to the carry-over list

Closed by the tasks of milestone 4, each with its line: say on the terminal when a run was
not checked, keep the cause when a check fails, a check cannot receive the variables it
needs, the documented per-call cap for a check (L1); `remove_prompt`'s `check` argument
(answered by L1); an executor built with a digest alone (K1, built in K2's pull request);
`whizbang lab run`'s progress and its "started now", the pairs page, and details first as
`check`'s default (A1); the loop's unreached branches and its third attempt (L2); a
result record's version, and the spec's wording (E1). Narrowed: `invert`'s `to`, the crop
half taken by L2, the cut half re-owned to the first recipe that needs it. Re-owned by E1:
hosting the model's files (milestone 6, after the licence was read); the places of a
located retry (milestone E). New with milestone 4: notarisation (the owner's, no
milestone); the loop's retries without live evidence (E); a receipt written by hand, and a
target folder outside the checkout (the first release that needs them). **Re-owned by this
pull request:** the Google rows of introductory prices. The candidate's local leg, the last
live run of milestone 4, ran on 2026-11-20, before both dates, so neither row could be read;
each is owned by the first live run on or after its date, milestone 5's or E's. No row is
owned by milestone 4.

## What milestone 5, E and later owe

- **Milestone 5 starts with the playground's design.** Its screens are designed with the
  owner in a new Paper project before any is built. The loop's three cases by hand are run
  again there.
- **Milestone E's first slice runs beside milestone 5,** as decided with the owner on
  2026-11-20. It holds three things: the default judge, chosen again (Gemini's judge was the
  strongest in milestone 3's gate, 0.887 under both arms); the submission format, so that
  small models stop failing it; and the loop's retries on the owner's own pictures, which
  are the live evidence the retries lack.
- **Milestone E, after that:** the loop's rung 2 scored, and rungs 3 and 4; its visual
  quality confirmed by a person; the blinded picking rounds of milestone 3; `tiles` and the
  second look; details first fixed and gated again on fresh data.
- **Milestone 6:** a local model off the Mac, through a remote segmenter, and so
  `remove_prompt` and the loop on Linux; hosting the model's files for customers.
- **No milestone yet:** notarisation of the Mac's binary, the owner's.

`main` at the end of milestone 4: the loop on one reservation, held by tests and run live
once under two judges; a version released from a candidate that passed on GitHub and on the
owner's Mac.
