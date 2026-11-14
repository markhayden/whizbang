# Checkpoint 1A note

Checkpoint 1A, "contracts and store", is complete as of 2026-11-14. Its plan is
`m1a-contracts-and-store.md`; this note says what shipped, what the two proofs changed
about the contract, how long things take, what the reviews found, what moved to the
carry-over list, and what checkpoint 1B starts from. The tag `m1a` is meant for the merge
of the pull request that brings this note.

## What whizbang can do at this tag

Little that a person would use, and all of it without a model. Two commands run an action
or would: `whizbang rascal grow` grows a mask in a store and commits the result; no
command runs `check`, squint's judge loop, which is described and cannot run on this
build because the binary holds no judge. `whizbang runs list|show` finds and reads runs,
`whizbang store list|delete|prune|import` looks after a store, and `whizbang describe`
says what the build holds. Under them is what the checkpoint was for: the identities,
the artifact formats, the store, and the execution contract, proven by `grow` and by a
scripted run of `check`.

## What shipped, by task

| Task | Pull requests | What |
|---|---|---|
| A1 Identities | #20 | Image digests, spaces and bindings; ADR 0001 |
| A2 Store: publication and recovery | #19 | Generations, one pointer, the sync order, recovery, the file-system shim |
| A3 Store: locking, deletion, diagnostics | #21, #25, #32 | One writer, runs and their deletion, the two kinds of reference, prune; the review's follow-ups; a lock that is let go when dropped |
| A4 Artifacts and the mask record | #22, #23 | The six types, the canonical file, the checked rebind, `store import`; ADR 0002 |
| A5 The execution contract | #26, #28, #30, #31, #33, #34 | The descriptor (ADR 0003), prepare (ADR 0004), the run record (ADR 0005), `echo` through the executor, `runs list|show` |
| A6 Request ids and admission | #38, #39, #40 | The id and its records, the estimate and a stale preparation, the reservation and the fake ledger; ADR 0007 |
| A7 Nested calls, cancellation, publication | #41, #42, #45, #46, #47, #49 | The commit step, status and wait, cancellation, nested calls and their end, and the fix of a wait by a cancelled run; ADR 0008 |
| A8 Capabilities and the fill composite | #35, #36, #37 | Faults, resolution, fill, segment and judge, the fakes, the composite; ADR 0006 |
| A9 Proof one: `grow` | #48, #51 | The action in whole numbers; the command and the first tool record |
| A10 `view`, extracted | #24, #27 | `view` for flat images with its record; the follow-ups |
| A11 Proof two: the scripted squint run | #50, #52 | What a run asks of a judge, recorded (ADR 0009); `view` as an action and `check` |
| A12 Exit check | #53, and the pull request of this note | The leftovers that change code; the exit check and this note |
| Tooling | #29, #44 | Follow-ups of two reviews; the thread rule in the instruction files, with the guard and the lint |

Pull request #43 is the plan for checkpoint 1B and is not part of this one. #17 and #18
closed milestone 0.

## The clauses of the spec's row, each with its tests

The checkpoint's row in `SPEC.md` section 16, clause by clause. A test is named by its
module and its function; `scripts/tests/test-notes.sh` fails when one named here no longer
exists.

| Clause | Tests |
|---|---|
| Identity types are decided | ADR 0001. `pixels::the_pixel_digest_is_fixed`, `bindings::a_binding_identity_is_fixed`, `bindings::the_identity_of_each_kind_of_mapping_is_fixed` |
| The six artifact formats are decided | ADR 0002. `types::there_are_six_types_and_each_artifact_knows_its_own`, `types::an_artifact_always_records_what_produced_it`, `types::the_origin_is_not_part_of_an_identity_and_the_content_is` |
| Identity acceptance: two different images of the same size | `pixels::two_different_images_of_the_same_size_have_different_identities_and_equal_spaces` |
| One source aligned onto two references of the same size | `bindings::one_source_aligned_onto_two_references_of_the_same_size_gives_two_bindings` |
| A re-encoded image | `pixels::the_same_pixels_in_two_containers_have_different_file_digests_and_equal_canonical_digests` |
| A flat image and a panorama of the same dimensions | `spaces::a_flat_image_and_a_panorama_of_the_same_dimensions_are_different_spaces` |
| Crops | `bindings::a_point_in_a_crop_of_a_crop_maps_to_its_root_and_back_within_one_pixel`, `bindings::a_point_outside_a_crop_is_refused` |
| Homographies, including a singular one | `bindings::a_homography_maps_in_its_stated_direction_and_the_reverse_binding_is_its_inverse`, `mappings::a_singular_homography_is_refused` |
| Panorama rotations | `bindings::a_rotation_composes_with_its_inverse_to_the_identity_and_wraps_at_the_seam` |
| Rascal's wrapped cut and put | `bindings::a_put_after_a_cut_returns_every_pixel_to_its_original_place` |
| A mask reused on a composite derived from its image | `rebind::a_mask_is_accepted_through_a_checked_rebind_on_a_composite_derived_from_its_image`, `rebind::a_mask_is_refused_on_an_unrelated_image_of_the_same_size` |
| A fake rascal operation executes through the executor | `execute::echo_runs_through_the_executor_and_its_record_is_in_the_store`; and the real one, `grow_action::grow_commits_a_mask_whose_origin_names_the_action_its_arguments_and_its_input` |
| A scripted multi-turn squint run executes through the executor | `check::look_then_look_closer_at_part_of_the_first_crop_then_decide`, `check::two_tool_calls_in_one_turn_with_opaque_provider_content_between_them`, `check::a_response_cut_off_by_the_token_limit_then_a_retry_with_a_larger_limit` |
| With atomic outputs | `commit::a_run_crashed_inside_its_commit_reopens_with_neither_output_nor_status_or_with_all`, `grow_runs::a_grow_crashed_inside_its_commit_is_not_reported_before_its_publication_is_durable`, `check::a_completed_check_was_reported_only_once_its_verdict_and_status_were_durable` |
| And kept diagnostics | `execute::a_failed_echo_writes_the_same_record_with_its_error_and_keeps_its_diagnostics`, `grow_runs::a_failed_grow_commits_nothing_and_keeps_its_diagnostics`, `check::a_failure_on_turn_two`, `check::a_cancelled_check_has_no_verdict_and_keeps_its_evidence` |
| Each writes a run record holding every group section 3 lists | `grow_runs::the_records_of_grow_completed_failed_and_cancelled_hold_what_the_spec_lists`, `check::the_records_of_check_completed_failed_and_cancelled_hold_what_the_spec_lists`, `execute::every_group_of_a_real_record_is_filled_or_explicitly_empty` |
| Request ids: simultaneous duplicates | `admit::two_simultaneous_requests_with_one_id_produce_one_run` |
| A conflicting payload | `admit::a_different_request_under_the_same_id_is_a_conflict` |
| A retry after a config change | `admit::the_same_request_again_after_the_config_changed_returns_the_first_run_and_prepares_nothing` |
| Prepare | `prepare::prepare_changes_nothing_in_the_store`, `prepare::prepare_sends_no_capability_request`, `prepare::prepare_makes_no_reservation`, `prepare::remove_prompt_with_and_without_a_filler_needs_different_capabilities_and_is_ready_or_not` |
| Admit | `admit::a_preparation_is_refused_at_admission_when_the_input_changed`, `admit::a_preparation_is_refused_at_admission_when_the_config_changed`, `admit::a_preparation_is_refused_at_admission_when_the_model_identity_changed`, `admit::admission_reserves_the_estimate_in_the_step_that_makes_the_run_and_the_ids_record` |
| Cancellation racing a commit | `cancel::a_cancel_before_the_commit_step_is_accepted_and_the_run_ends_cancelled_with_no_output`, `cancel::a_cancel_after_the_commit_step_began_waits_and_returns_the_completed_outcome` |
| Cancellation with children outstanding | `nested::cancelling_a_run_cancels_every_run_under_it_and_a_call_made_after_is_refused`, `nested::a_run_stays_open_while_a_capability_request_is_outstanding_and_ends_when_it_is_accounted_for`, `nested::a_completed_runs_outputs_remain_when_the_run_that_called_it_does_not_complete` |
| The store reopens and recovers after an interrupted publication | `store::a_crash_at_any_event_leaves_one_whole_generation_and_a_store_that_still_publishes`, `store::opening_to_write_removes_leftover_staging_and_opening_to_read_removes_nothing` |
| Keeps diagnostic references | `prune::a_failed_runs_diagnostics_are_read_after_reopening_and_after_other_runs_are_pruned`, `runs::the_same_bytes_are_a_diagnostic_to_one_run_and_an_output_to_another` |
| Keeps an artifact two runs share when one is deleted | `prune::deleting_one_of_two_runs_that_share_an_artifact_keeps_it_and_removes_what_only_the_one_had` |
| Sets owner-only permissions | `prune::every_path_the_store_creates_is_for_its_owner_only`, `store::what_the_real_file_system_creates_is_readable_by_its_owner_only` |
| Refuses a second writer | `locking::a_second_writer_is_refused_with_the_holder_named_while_a_reader_reads`, `locking::the_real_file_system_refuses_a_second_writer_and_names_the_first` |
| The fill conventions are fixed | ADR 0006. `fill::the_support_plane_is_255_to_paint_and_0_to_keep`, `composite::after_a_filler_repaints_every_pixel_only_the_support_is_changed`, `composite::a_blend_weight_of_one_half_gives_the_stated_mix`, `composite::a_transparent_source_pixel_keeps_its_alpha_and_takes_colour_like_any_other` |

**One clause is met short of the spec's words elsewhere.** The row's clause on an artifact
two runs share is met. The spec's sentence on storage, that delete and prune remove runs
and what only they reference, is not met for an artifact that only a deleted run
committed: its name stays. That is decided and not built (below, and ADR 0008, "What
deleting a run removes"), and has its row on `docs/carry-over.md` for milestone 2.

**What the records hold, against section 3's list.** The two record tests above run
`grow` and `check` to a completed, a failed and a cancelled end, each through an executor
with the fake ledger, and for `check` with a judge that states a bound. They hold each
group: the run, the request and the parent; the action and its exact arguments; each
input with its binding; the outcome with an error's type and message; start, end and a
duration for each request and each call; the fake judge's backend, model and version for
each request; what the ledger reserved (nothing for `grow`, the judge's bound for
`check`), with the reservation in the store; each tool call as a step, with the record of
the run it made; and for `check` every turn, with what was sent and received kept whole in
the exchange it names. Three things "whole" does not cover, by decision (ADR 0009): a
picture is its size and its pixel digest, an opaque part's body is never stored, and a
secret is replaced. Four things are there and say nothing yet: `asked.settings`,
`egress` and `spend.requests` are empty lists, and a model's `device` and `cached` are
null, each with its row and its checkpoint.

## What the two proofs changed about the contract

The plan said a proof is never bent to fit, and that the contract is fixed in its own
pull request when a proof shows it wrong. Both proofs did.

- **The contract needed five records where the plan foresaw one.** The descriptor (ADR
  0003), preparing a call (0004), the run record (0005), request ids and admission (0007)
  and executing a call (0008) each turned out to hold a decision that is costly to
  reverse. ADR 0009 came with the second proof.
- **`grow` (A9)** was the first action to read the store while it ran, and the first with
  a command. It showed that an action's message is a way for a path to reach a record,
  which the exit check then closed for every action: the store and the executor in a
  context answer without paths (ADR 0005, "Redaction"; ADR 0008, "Nested calls"). It
  gave ADR 0008 "The first door": one entry point, a request id through the executor, a
  quiet panic hook, everything printed escaped, and one process that owns its runs. And
  it fixed a deadline to the executor's clock, which an action could not ask before.
- **`check` (A11)** changed more. A run is handed the backend its call resolved and no
  other; every capability request is in the record with its model and its duration; an
  exchange with a judge is a diagnostic the executor writes; a verdict is a result that
  is not an artifact; and a run freezes the actions its calls may be and the artifacts
  they may read, held where a call is dispatched, which binds every run under it (ADR
  0009). The plan's sentence that evidence "cannot be read by another run" did not
  survive: a tool's result is the committed output of the tool's own run, another run's
  judge cannot read it, and a caller that is no judge can (accepted, with a row).
- **The record's bounds came from both.** A record is read back only under 4 MiB, and
  each proof found another list that could pass it: outputs, refusals, steps, a run's
  requests, and at the exit check a call's texts and inputs and a run's diagnostics. Each
  is bounded where it comes in, and the sum is under the limit by a test (ADR 0008, "What
  a call and a run may hold").

**"Within one pixel".** The plan's "done when" for `view` says a location in a crop maps
back to the source "within one pixel". What was built and measured is stronger in one
way and weaker in another, and the plan now says so: the mapping is exact at a crop's
corners and within 1e-9 of a pixel inside it; what a judge sees in a reduced crop is only
as fine as the crop, so a thing seen in one crop pixel is found again within half a crop
pixel, which is more than a source pixel once the crop is reduced more than twice
(`docs/reference/tools/view.md`, "Rung 2 results").

## Three things a caller should know

- **Which run a text means.** `runs show` and `store delete` find a run through one
  function. A run's id always means that run. A text that is no run's id is taken as
  `runs list` prints a run, escaped or cut, when it prints exactly one so. A text that is
  one run's id and how another is listed, and a cut id that several runs share, are
  refused, and nothing is shown or deleted.
- **What an action can reach of a failure of the store:** its words, and nothing with a
  path. The store in a context and the executor in a context both answer with
  `store::Error::WithoutPaths`, and nothing else a context holds has a store's error in
  it.
- **A full store.** A generation file is 64 MiB at most. At that limit a publication that
  adds names is refused (an admission, an import, and a settlement once there is one); a
  run that cannot publish its end is abandoned; a deletion is still published, since it
  makes the file smaller. The names of request ids and of spend are never removed, so a
  store that ids filled shrinks only by the runs deleted from it.

## Timing

Measured on 2026-11-14.

| What | Where | Time |
|---|---|---|
| `just check`, warm build | The owner's Mac | 2 min 44 s, with another build at work on the machine; most of it is the script tests |
| The fast tests alone, compilation not counted | The owner's Mac | 28.5 s, against a limit of 180 s |
| CI, `check (macos-15)` | GitHub, five recent runs | 5 min 38 s to 6 min 41 s |
| CI, `check (ubuntu-22.04)` | GitHub, five recent runs | 2 min 31 s to 3 min 57 s |
| 256 calls from one run, alone | The owner's Mac, the test kit's disk | 3.4 s; 514 generations, the last file 147,480 bytes |

Seconds by suite, summed over tests that run side by side, so the rows add up to more
than the test time:

| Suite | Tests | Seconds |
|---|---|---|
| `whizbang::it` | 75 | 55.9 |
| `whizbang::bin/whizbang` | 4 | 0.1 |
| `whizbang-core` | 12 | 1.0 |
| `whizbang-core::it` | 661 | 122.7 |
| `whizbang-eval::it` | 38 | 1.4 |
| `whizbang-rascal::it` | 44 | 3.9 |
| `whizbang-squint::it` | 55 | 11.9 |
| `whizbang-testkit` | 3 | 0.0 |
| `whizbang-testkit::it` | 46 | 0.6 |

The slowest tests: the crash sweep of the admission step in its four parts (8.4 s for the
first), 256 calls from one run (6.7 s inside a full run), and many runs from two
executors at one instant (5 s). Each of the binary's tests starts a process, which is
most of that suite. Both budgets of `SPEC.md` section 13 are met: under 3 minutes of
test time, and under 10 minutes of pull-request CI on both runners.

## What the reviews found

The pull requests carry no review comments: each was read by a separate reviewer before
it merged, and what follows is from the record the orchestrator kept. Checkpoint 1A was
34 pull requests (#19 to #53, less #43), most of which took two or three rounds. Bugs
that were caught before a merge:

- a record that the code wrote and then refused to read, for about one number argument
  in nine;
- secrets that survived redaction when a text was cut before it was redacted, found three
  separate times, and now a stated rule: redact first, then cut or escape;
- two runs given one id by two executors over one store;
- a repeat answered from another request's run after a run's id was used again;
- a repeat that raced the end of its own run;
- a request told "over allowance" while its own run was executing;
- a cancel and a wait told "completed" before the commit was durable;
- a child's share of an allowance given back while its admission stayed in the store;
- a nested wait answered for the run when the waiter was cancelled: a test that was
  flaky on Linux and turned out to be a fault in the code;
- a permission entry that would have run arbitrary programs.

**The machine-safety incident.** A reviewer's own probe took the build machine down,
twice, by making threads until the system refused one. Four things now stand between a
test and that: `start` is bounded, 64 started runs for an executor unless it is built
otherwise and never more than 256; a test makes a thread only through
`whizbang_testkit::threads`, which holds a process to 64; a lint in `just check` refuses
any other way of making one; and `scripts/thread-guard.sh` runs every test command and
kills a process that holds more than 300 threads. The rule is in `AGENTS.md`: nothing is
ever run to find out how many a system allows.

The exit check added its own: a nested call handed an action the store's path through
`StoreFailure::cause`, and `runs show` could show one run for another's id, both found in
the review of #53 and fixed there.

## Spend

0 US dollars. No model ran in this checkpoint: every capability is a fake from the test
kit.

## What moved to the carry-over list, and to whom

Each has its row in `docs/carry-over.md`. No row is owned by a finished task of 1A.

| What | Owner |
|---|---|
| The config and the roster built from it; `asked.settings`; the real ledger, settlement, and a failed request that says how far it got; what redaction does not find; the cost of a publication and the names of request ids; the secrets pattern after a hyphen | 1B |
| Four tooling rows that no task of 1A built: a scan of a pull request's title and body, a push made around the hooks, a script edited while it runs, `grep -q` after a pipe | 1B |
| A model's device and whether a cache answered; what a segmenter and a filler took beside the interfaces; bounds on what a backend builds; an adapter's keeping of the fault rule; the lock held by a child being started | 1C |
| What `store delete` removes of a run's outputs (decided, not built); the reference page's arguments, ports and needs; `whizbang squint check` and squint's real loop; a tool's mark; a result that is not an artifact; a tool's result that another caller can take; the panorama side of `view`; a cancel routed to the process that owns a run; a door that shows a run's events; a run that waits without a bound | 2 |
| The limit of 256 calls a run; one encoder for image artifacts in the core; what the `seam` and `remove_prompt` descriptors leave out; a mask moved onto a stored composite; export and import of artifacts | 3 |
| Runs abandoned by a process that lives long | 6 |

## What checkpoint 1B starts from

Its first task, B0, answers three open points of its plan from the merged code. As 1A
leaves them:

- **D8, where a store is:** `--store <dir>` on every command that touches one. There is
  no default and no setting.
- **D9, where handles are made and what a run records of a request:** in `Context::new`,
  from the backends the call resolved, which the executor holds
  (`Capabilities::resolved_for`). The handles share one tally, which records each request
  in `run/2`: `models`, `time.durations`, and for a judge `judge` and an exchange among
  the diagnostics (ADR 0009). The roster a call is resolved against and the backends an
  executor holds are still given apart, and nothing holds one to the other (a row for
  1B).
- **D13, what `store delete` removes, and how the store's own files are bounded:** a
  deletion removes a run's names and leaves the artifacts it committed; the rule that
  they should go is decided and is milestone 2's. Nothing touches the names under
  `requests/` and `spend/`. `CURRENT` is 21 bytes at most, a generation file 64 MiB, and
  the first 1,024 bytes of `LOCK` are read. Settlement adds names, so it is to be checked
  against a full store.

And: `main` at the commit tagged `m1a`, with CI green and both scans clean; `run/2`
frozen at the tag, so each fill after it is a new version; three actions registered and
one command that runs one; the frozen repos at their pins.
