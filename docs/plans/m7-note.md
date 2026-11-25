# Milestone 7 note

Milestone 7, "Bundles and replay", ends with task N1 of `m7-bundles-and-replay.md`. This
note was written by N1's pull request from its branch, with `main` at 6f40d2b4, the merge of
#311, which fixed what N1's pass by hand found. The same pull request ticks the plan's boxes,
marks the carry-over rows milestone 7 owned, and writes the spec's wording owed. Every box of
the plan is ticked but N1's own, which closes with the tag `m7`. The tag is the
orchestrator's, made on this pull request's merge. The tag `m7-core`, at 360e41fc (the merge
of #309), marks bundles and replay from the command line.

## What whizbang can do at the end of milestone 7

Beyond milestone 6: a run leaves its store as one file, and any run can be made again and
compared with what it made the first time (`docs/guides/bundles-and-replay.md`).

- **Bundles.** `whizbang runs bundle` writes one run that ended, or a chain of up to 16 in the
  order they ran, as one file, `bundle/1` (ADR 0048). It holds each run's record, journal and
  diagnostics and the files of its inputs and outputs. A later run's input that an earlier
  run made is linked to it. The same runs make the same bytes, and the file is named by its
  SHA-256. `whizbang bundle show` reads one and checks it whole without a store; `bundle
  import` brings it into another store in one publication, after every check has passed;
  `bundle list` and `bundle delete` look after them. A bundle's runs are kept under its own
  names, never as the store's runs: no history, statistic or ledger counts them.
- **Replay.** `whizbang runs replay` makes a run again as a new run, from its store or from a
  bundle (ADR 0049). Offline, the default, every model's answer is the one the run
  recorded: nothing is called, nothing is spent, no key is read and no worker starts. With
  `--live`, the call is made again under today's config, as an ordinary run that may spend.
  Either way, the two families are compared run by run, and the report is reproduced,
  differs (each difference by place), diverged or unanswerable. Neither reads the decision
  cache. `runs stats` leaves replayed families out unless `--replayed` says otherwise.
- **The record, `run/7`.** A model's entry names the card it ran on when the worker names
  one, beside the device. A replayed run names the run it makes again, with the bundle and
  step when it came from one. A fill run keeps the picture the filler painted, so that its
  replay can be answered.
- **The playground.** Two runs of one task compared side by side, one pinned as the baseline
  in the page's address. Saved examples, held by the store, that open as drafts and keep
  their pictures after their run is deleted. Home's library lists the pictures of every
  door, and a field's library is the same list. "Bundle this run", "Bundle chain" in History,
  and a Bundles page that takes a bundle in. Guided chain replay: each step opens as an
  ordinary draft and runs only when the person presses Run. "Replay" on a result, offline or
  live, with its report.
- **The engine API's uploads.** The hourly collection now ends in the store's own prune, so
  an upload's bytes go with its name.

## What shipped

| Pull requests | Task | What |
|---|---|---|
| #301 | Plan | The plan of milestone 7, after five rounds of outside review |
| #303 | G1 | The designs, approved by the owner in Paper, and the table of what the engine could not back yet |
| #302, #305 | A0 | The entry check: the pack's budget, the Chrome case that timed out once, a request that never connected charged at nothing; then the local fast tier in CI's two parts, each held to 300 seconds, by the owner's decision |
| #304, #306 | B1 | Invocation bundles: ADR 0048, `run/7` with the card, a fill's kept answer and what the store keeps; then `bundle/1`, its reader, the import and deletion, the commands and the committed fixture |
| #307, #309 | R1 | Replay: ADR 0049, the recorded handles and `Executor::start_replay`, the end's two rows; then the comparison, live replay, replay from a bundle, `runs replay`, and stats without replays |
| #308 | U1 | The playground: saved-run comparisons, saved examples, Home's library, a choice's own refusal |
| #310 | U2 | The playground: bundles, guided chain replay and the replay of a run |
| #311 | N1's fixes | The five findings of N1's pass by hand (below) |
| This one | N1 | This note, the boxes, the carry-over rows, the spec's wording owed |

## The clauses of the spec's row, each with its evidence

The row for milestone 7 in `SPEC.md` section 16: "Invocation bundles and journal replay;
saved-run comparisons, saved examples and guided chain replay in the playground". Tests are
named by module and function, and Chrome cases by file and title; `scripts/tests/test-notes.sh`
fails when one named here no longer exists. No clause needs a live call, and none was made.

| Clause | Shown by |
|---|---|
| Invocation bundles | The writer: `bundle::the_same_runs_make_the_same_bytes_twice`, `bundle::a_long_member_name_round_trips_through_prefix_and_name_and_one_too_long_is_refused`, `bundle::a_chain_links_an_input_to_the_earlier_output_it_took`, `bundle::an_input_two_earlier_outputs_could_be_refuses_the_chain`, `bundle::a_step_of_a_recipe_or_tool_file_is_carried_as_not_replayable`, `bundle::an_opaque_part_a_missing_picture_and_a_cached_check_are_listed_as_withheld`, `bundle::the_manifest_holds_no_path_and_no_secret`. The reader, which trusts nothing: `bundle::a_bundle_reads_back_whole_with_every_member_checked`, `bundle::a_member_named_absolutely_with_dot_dot_twice_or_as_a_link_is_refused_and_nothing_imported`, `bundle::a_member_not_named_by_the_manifest_or_a_named_one_missing_is_refused`, `bundle::an_artifact_whose_file_digest_or_decoded_identity_differs_is_refused`, `bundle::a_diagnostic_whose_bytes_differ_from_its_record_is_refused`, `bundle::a_manifest_that_contradicts_a_record_or_a_link_that_contradicts_both_is_refused`, `bundle::a_family_with_a_missing_parent_a_cycle_or_two_roots_is_refused`, `bundle::a_child_listed_by_one_parent_and_naming_another_or_listed_twice_is_refused`, `bundle::an_interrupted_parent_with_lost_steps_and_a_null_ending_of_a_recovered_child_are_kept`, `bundle::an_unknown_version_is_refused_naming_it`, `bundle::a_bundle_over_its_bounds_is_refused_before_it_is_read_whole`. The import: `runs_bundle::a_bundle_imports_into_an_empty_store_and_survives_its_archive_and_first_store_gone_a_restart_and_a_prune`, `runs_bundle::a_second_import_changes_nothing_and_its_runs_are_not_the_stores_own`, `runs_bundle::two_files_of_one_identity_with_other_origins_survive_import_a_restart_and_a_prune_byte_for_byte`, `runs_bundle::the_committed_fixture_reads`. What the store keeps: `store::collect::a_bundles_entry_keeps_its_artifacts_from_delete_prune_and_the_collection`, `store::collect::a_deleted_bundles_artifacts_go_at_the_next_prune_unless_a_run_names_them`, `store::collect::an_artifact_two_bundles_share_stays_until_both_are_deleted`, `store::collect::an_ordinary_import_before_or_after_a_bundle_keeps_its_artifact_when_the_bundle_goes`. The record: `record::a_record_with_a_card_is_run_7_and_one_without_is_written_as_before`, `remote::the_run_records_the_card_the_fake_host_named`, `fill::a_fill_run_keeps_the_painted_answer_that_a_feathered_mask_was_composited_from`. In the playground: `play_bundles::a_bundle_uploaded_is_read_whole_before_anything_is_imported`, `play_bundles::a_bundle_over_256_mib_is_refused_413_before_it_is_read_whole`, `play_bundles::with_two_bundle_uploads_stalled_a_third_is_refused_429_and_polling_and_stop_are_answered`, `play_bundles::a_deleted_bundle_leaves_the_artifacts_a_run_names`, `play_bundles::no_bundle_route_takes_a_path`, `send.spec.ts: bundle this run downloads one file that imports into another playground`, `security.spec.ts: a bundle holding markup in its words shows them as text` |
| Journal replay | Offline, with no backend configured: `replay::an_offline_replay_of_remove_prompt_reproduces_it_with_no_backend_configured`, `replay::an_offline_replay_of_remove_checked_reproduces_its_attempts_and_verdict_through_the_run_match`, `replay::an_offline_replay_of_a_check_reproduces_its_turns_checklist_and_verdict`, `replay::an_offline_replay_of_a_recipe_whose_check_takes_measurements_reproduces_it`, `replay::an_offline_replay_of_a_fill_through_a_feathered_mask_reproduces_its_picture`, `segment::the_answer_rebuilt_from_a_runs_masks_and_its_diagnostic_is_the_answer_it_was_given`. Nothing reached: `replay::an_offline_replay_sends_nothing_reserves_nothing_reads_no_key_and_starts_no_worker`, `replay::a_replay_neither_reads_nor_writes_the_decision_cache`. What it cannot answer, and where it parts: `replay::a_check_answered_from_the_cache_is_unanswerable_offline_and_asks_afresh_live`, `replay::a_fill_run_made_before_its_answer_was_kept_is_unanswerable_naming_it`, `replay::a_turn_that_held_an_opaque_part_is_unanswerable_naming_it`, `replay::a_request_the_original_did_not_make_is_diverged_and_nothing_is_called`, `replay::a_divergence_in_a_second_look_or_a_loops_check_is_the_replays_report_whatever_the_action_made_of_it`, `replay::a_changed_operation_is_reported_as_a_difference_at_its_step`, `replay::a_changed_measurement_of_an_action_with_no_artifact_output_is_a_difference`. What it refuses: `replay::a_child_an_interrupted_run_a_recipe_files_run_and_a_setting_at_its_limit_are_each_refused`, `replay::a_check_that_called_a_tool_file_is_refused_offline_and_replayed_live_only_with_the_same_digest`, `replay::a_tool_file_call_that_failed_in_the_original_runs_no_file_offline`. Live: `replay::a_live_replay_spends_as_a_run_and_reports_a_changed_verdict_as_a_difference`, `replay::a_live_replay_whose_backend_the_config_no_longer_offers_is_refused`, `replay::two_segment_diagnostics_that_differ_only_in_what_the_backend_read_of_its_machine_are_the_same`. Request ids and provenance: `replay::a_request_id_under_another_mode_or_source_is_a_conflict_and_the_same_replay_returns_its_run`, `replay::a_repeated_replay_is_answered_after_its_source_was_deleted_and_after_the_config_changed`, `replay::a_repeat_by_bundle_digest_after_the_archive_and_the_entry_are_gone_publishes_nothing`, `replay::every_run_of_a_replayed_family_names_it_and_stats_leaves_the_family_out_by_default`, `replay::a_replay_from_a_bundle_holds_its_inputs_after_the_bundle_is_deleted_and_pruned`, `replay::a_replay_in_a_held_store_is_cancelled_waited_for_and_counted_like_any_run`. In the playground: `play_replay::an_offline_replay_from_the_page_spends_nothing_and_shows_reproduced`, `play_replay::a_live_replay_runs_only_under_the_cost_the_page_showed`, `replay.spec.ts: replay of a result shows reproduced, and a changed one shows where it differs` |
| Saved-run comparisons in the playground | `play_compare::two_runs_of_one_task_compare_by_settings_inputs_outputs_time_and_cost`, `play_compare::two_paid_loops_compare_cost_and_models_over_their_families_with_unresolved_spend_said`, `play_compare::two_tasks_are_refused_422_and_a_run_not_ended_409`, `compare.spec.ts: a pinned baseline and an attempt of the same task show their pictures, settings and cost side by side, and the pin survives a reload` |
| Saved examples in the playground | `play_examples::an_example_keeps_its_inputs_after_its_run_is_deleted_and_pruned`, `play_examples::opening_an_example_starts_no_run`, `play_examples::an_example_whose_backend_is_not_offered_now_names_it_and_falls_back`, `play_examples::a_name_or_notes_holding_markup_are_shown_as_text`, `play_examples::past_1000_examples_a_new_one_is_refused_naming_the_bound`, `play_examples::store_list_names_an_example_and_store_delete_example_lets_it_go`, `play_examples::an_example_of_a_check_opened_unchanged_makes_its_recorded_call_after_its_run_is_gone`, `examples.spec.ts: an example saved from a result opens as a draft that runs nothing, after a restart and after its run was deleted` |
| Guided chain replay in the playground | `play_chain::a_step_runs_only_when_started_and_continue_binds_the_new_output`, `play_chain::a_link_that_finds_no_output_or_two_stops_the_chain`, `play_chain::a_run_started_from_a_step_records_the_bundle_and_the_step`, `play_chain::an_unchanged_step_makes_the_call_it_recorded_requirements_and_all`, `chain.spec.ts: a chain of a removal and its check replays one ordinary run a step, binds the new output and shows its changed identity`, `chain.spec.ts: stop leaves the chain and a reopened tab finds its runs` |
| Section 6's control, section 7's two rows | All of the above. `SPEC.md` now marks section 6's "Invocation bundles" and section 7's rascal row "Replay from a journal; invocation bundles" and playground row "Comparisons between saved runs; saved examples; invocation bundles; guided chain replay" kept, milestone 7 |

Beside the row: Home's library of every door,
`play_library::the_library_lists_inputs_of_runs_and_imports_by_pages_with_each_cards_runs`
and `home.spec.ts: home lists the pictures of a store filled from the command line`; a
choice's own refusal, `play::a_choice_refused_for_itself_is_told_apart_from_a_missing_input`;
Run again left as it is,
`play_result::run_again_left_as_it_is_makes_the_recorded_call_with_what_no_field_shows`;
the end's two rows,
`execute::an_end_holds_one_copy_of_what_the_run_kept_and_a_second_end_after_a_panic_writes_it_whole`,
`execute::a_panic_after_the_pointer_was_renamed_and_before_the_sync_is_confirmed_by_a_sync_and_answers_the_durable_end`,
`execute::a_panic_after_the_sync_answers_the_durable_end`,
`execute::a_panic_at_publishing_before_the_pointer_was_renamed_answers_as_before` and
`execute::a_panic_before_the_rename_then_another_publication_is_not_taken_for_the_end`;
the uploads' bytes,
`store::collect::the_hourly_collection_ends_in_a_prune_and_an_uploads_bytes_go_with_its_name`
and `store::collect::a_run_going_keeps_every_file_a_held_reader_reads`; a request that never
connected, `remote::wire::a_request_whose_connection_was_refused_settles_at_nothing` and
`remote::wire::a_request_cut_after_its_first_byte_is_still_charged_at_its_bound`; and an
exchange's output ceiling,
`capability::written::an_exchange_keeps_its_turns_output_ceiling_and_an_older_one_reads_as_none`.

## The pass by hand, and what it found

N1's pass ran on 2026-11-25, on the owner's Mac, from `main` at f062b85a (the merge of #310),
in Chrome. Three playgrounds: A, the release binary with the owner's local SAM 3 on the Mac's
GPU (MPS) and no fill backend; B, the fixture host with its fakes; and C, the fixture host on
a new store, after B's store was moved away. Each of G1's six surfaces was used once as a
person would.

- **SAM 3 on A.** `segment` by phrase took 10.0 seconds cold, and by point 3.5.
- **Replay on A.** Offline, a real SAM 3 run was reproduced in 0.1 seconds, at nothing. Live,
  it ran again in 0.9 seconds. Before #311 the live replay said "Differs" on one line alone,
  the memory the backend read of its machine; since #311 that line is not compared.
- **Bundles.** A's run bundled to 323,584 bytes; `whizbang bundle show` read one step,
  replayable, nothing withheld. On B, a removal sent to "Check an edit" made a chain of 2
  steps and 1 link, bundled to 171,520 bytes.
- **The chain in another store.** C took B's chain in and checked it whole. Each step opened
  as a draft that ran nothing until Run, and Continue bound step 2 to step 1's new output.
  Before #311, step 2 dropped the check's job requirements, which the form does not show;
  #311 fixed that. A's SAM 3 bundle also imported into C, which has no SAM 3, and said it
  replays offline.
- **From the command line.** `whizbang runs replay --bundle <file> --step 0`, and `--step 1`,
  in a fresh store: both reproduced.

Found and fixed in #311, each with a test that fails without it:

1. A field's "From the library" listed only the pictures given through the playground. It now
   reads Home's library, of every door.
2. A live replay of an unchanged local segment said "Differs" on the backend's reading of its
   machine (its memory on the GPU). `segment.txt` is now compared without that reading (ADR
   0049, amended).
3. The chain's card named each step by its number twice. It now names each by its task's
   title.
4. A chain's step, an example or Run again, left as it was, did not make the recorded call
   when the call held more than the form shows: a check's job lost its requirements. Each now
   makes the recorded call.
5. Bundle chain in History said nothing once it had saved. It now says what it saved.

Five more were found and moved to the carry-over list for milestone 8 (below).

## Timing

From CI's run on #311's last head (run 45651492652, at f0db27c9), the last pull request
before this note. Wall time of each job, compilation included; the budget of `SPEC.md`
section 13 is under 10 minutes for pull-request CI.

| Job | `macos-15` | `ubuntu-22.04` | Test time |
|---|---|---|---|
| `check` (every fast test but the sweeps, and the UI's unit tests) | 9 min 25 s | 7 min 12 s | 216.5 s and 153.6 s, limit 300 |
| `sweeps` | 3 min 12 s | 3 min 29 s | 79.1 s and 62.7 s, limit 300 |
| `scripts`, shard 1 | 6 min 13 s | 2 min 56 s | |
| `scripts`, shard 2 | 5 min 40 s | 3 min 39 s | |
| `release` | 4 min 46 s | 5 min 10 s | |
| `e2e`, shard 1 | 3 min 35 s | 2 min 37 s | 99.1 s and 71.0 s, limit 120 |
| `e2e`, shard 2 | 4 min 5 s | 2 min 34 s | 102.1 s and 66.0 s, limit 120 |
| `secrets` | | 2 min 40 s | |
| The run, start to `ci` | 9 min 34 s | | |

Two macOS jobs waited for a runner: `scripts (macos-15, shard 2)` for 3 min 19 s and `e2e
(macos-15, shard 2)` for 3 min 43 s. The pack's tests took 7.87 seconds on `check
(macos-15)` and 5.42 on Linux, of their 12.

**By suite**, the fast tests, each suite's tests' own seconds added up (they run side by
side, so the sum is more than the test time):

| Suite | Tests | `check`, macOS | `check`, Linux | Sweeps | Sweeps, macOS | Sweeps, Linux |
|---|---|---|---|---|---|---|
| `whizbang::it` | 782 | 340.7 | 285.4 | | | |
| `whizbang-core::it` | 1,442 | 108.3 | 96.2 | 29 | 95.8 | 86.4 |
| `whizbang-backends::it` | 360 | 70.9 | 55.6 | 308 | 57.1 | 33.8 |
| `whizbang-squint::it` | 178 | 40.7 | 62.8 | 2 | 6.4 | 8.5 |
| `whizbang-rascal::it` | 152 | 27.8 | 24.7 | 1 | 3.4 | 3.1 |
| `whizbang-testkit::it` | 151 | 14.3 | 25.9 | 1 | 0.0 | 0.0 |
| `whizbang-serve::it` | 48 | 6.8 | 5.4 | | | |
| `whizbang-backends` | 63 | 7.2 | 9.6 | | | |
| `whizbang-eval::it` | 67 | 6.2 | 8.9 | | | |
| `whizbang::bin/whizbang` | 27 | 6.2 | 4.1 | | | |
| The other four (`whizbang`, `whizbang-core`, `whizbang-serve`, `whizbang-testkit`) | 69 | 1.6 | 1.1 | | | |
| The UI's unit tests, as the clock saw them | 231 | 5.4 | 6.6 | | | |

**The Chrome tier by file**, on `macos-15`, in seconds: shard 1, `chain.spec.ts` 14.7,
`compare.spec.ts` 14.0, `history.spec.ts` 15.7, `inputs.spec.ts` 14.8, `examples.spec.ts`
3.5, `home.spec.ts` 2.6 and six more files; shard 2, `run.spec.ts` 32.9, `send.spec.ts` 24.4,
`report.spec.ts` 21.6, `replay.spec.ts` 8.4 and three more. Sixty cases in all.

**On the owner's Mac**, the `just check` of this pull request ran the two parts one after
the other: 195.7 seconds of test time for `fast` and 72.2 for `sweeps`, each of 300. The
pack's tests took 4.17 seconds.

## Spend

None: 0 US dollars in real money, against the plan's estimate of 0 and its cap of 5. No
task and no test made a live call. The pass by hand used the owner's local SAM 3 on the
Mac's GPU, which spends nothing, and the fixture's fakes, which count against a pretend
allowance of 10 dollars and reach nothing. CI runs nothing live.

## What the owner decided

- **The order after milestone 6** (2026-11-24): 7, then E, then 8. Milestone E starts when 7
  ends, and 8 when E ends. `SPEC.md` section 16 now says so in its table's order too.
- **The designs** (2026-11-24): all six surfaces of G1 approved in Paper.
- **The fast tier's local budget** (2026-11-24): `just check` runs the fast tests in CI's two
  parts, each held to 300 seconds (#305), since the whole on the owner's Mac sat at its 250.

## What a reader should not assume

- **A live replay repeats the call, not the answer.** A model's verdict or pixels are not
  promised to come out the same. A live replay that differs is a report, never an error.
- **A bundle holds the person's pictures and words, unencrypted.** It holds no path, account
  or secret, but anyone who has the file can read what the runs saw and said.
- **A bundle says nothing of who made it.** Its reader checks that it is whole and agrees with
  itself, not where it came from. It carries data, never code: a step of a user's recipe or
  tool file is carried as evidence and not replayed.
- **A chain's step is an ordinary run under today's config.** Its backends, prices,
  credentials and approvals are the store's now, not the bundle's. It repeats the recorded
  call, but its answers are new.
- **An imported bundle replays offline only from the command line for now.** Its page in the
  playground opens its step, which is live. `whizbang runs replay --bundle-digest` replays it
  offline (the carry-over list).
- **A fill run made before B1 cannot be replayed offline.** It did not keep the picture the
  filler painted, so its replay is unanswerable, naming that.
- **Offline replay was shown on fakes and on one local model.** In CI it ran against the
  fixture's fakes; the only real model replayed was the owner's local SAM 3, by hand. No paid
  backend was replayed, offline or live.
- **The Chrome tier's macOS shards are near their limit.** 99.1 and 102.1 seconds of their
  120 on #311's run: under the plan's mark of about 110 a shard, but close.

## What moved to the carry-over list

Closed by the tasks of milestone 7, each with its line:

- A0: the Chrome case `run.spec.ts: each of the five squint use cases runs from its form`; a
  segment request whose connection never opened, now charged at nothing; the pack's tests'
  budget; and the fast tier's time on the owner's Mac, by the owner's decision (#305).
- B1: the run record's card; the engine API's collected uploads, whose bytes now go with
  their names.
- R1: a run sent again from what the store holds, which a replay now says it cannot do for an
  opaque part; every run's end cloning what it kept; a panic at `Publishing` answered as
  abandoned.
- U1: `choice_json`'s `Err(_)` arm; Home's library of the playground's pictures alone.

**By this pull request:**

- Re-owned to E: the Google rows of prices. No live run of milestone 7 asked a Google judge,
  so neither row could be read on or after its date. Each is owned by the first live run on
  or after its date, which is milestone E's.
- Opened during the milestone, each owned by milestone 8 and standing as written:
  - From R1's review (#309), three: a bundle's step read through a generation let go before
    the admission holds its inputs; a live replay's tool file checked when the replay is read,
    not when the file runs; a build before R1 that prunes a store a later build replayed in.
  - From U1's review (#308), seven: a choice's refusal told apart by a comparison that is not
    exact in two cases; Home's library built again on every request; the bound of 1,000
    examples checked before the publication; an example's choices prepared together; an
    unreadable example neither listed nor deletable in the playground; keyboard access of
    U1's menus; `ExampleList`'s kit.
  - From U2 and its review (#310), eight: a bundle's upload held whole in memory; a link
    bound by place where the counts differ; a withheld file stopping a chain; a bundle's
    deletion running the prune in the request's worker; a bundle deleted while its chain is
    under way; a chain's place kept only in the browser; the dialogs' focus; a chain step's
    key among the start's arguments.
  - From N1's pass by hand, five: a comparison of two `segment` runs shows their pictures, not
    their masks; the header's words wrap at 1200 pixels; a check's job shown as raw JSON;
    an imported bundle replayed offline only from the command line; a chain's bundle saved
    with no dialog of what it holds.

Left untouched, as the plan said: the front's bridge's cancel window (8); the probe's wording
(the next release); hosting the model's files (no milestone). No row is owned by milestone 7.

## What milestone E, 8 and later owe

- **Milestone E**, next: the loop's finished pictures judged from the owner's marks; the gate
  on fresh data for the default judge and the format; the clicked removal's shaping; the
  Google rows of prices; and the evaluations that decide what 8 builds.
- **Milestone 8:** fill and judge, each local and self-hosted, and any further segmenter E
  calls for. Also the front's fault forms and its bridge's cancel window, and the
  twenty-three rows milestone 7 opened.
- **The next release:** the probe's wording.
- **No milestone yet:** notarisation of the Mac's binary, the owner's; hosting SAM 3's files
  for customers.

`main` at the end of milestone 7: any ended run, or a chain of them, leaves its store as one
checked file, and any run is made again offline with nothing called, or live under today's
config, and compared with what it made.
