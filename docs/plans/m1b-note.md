# Checkpoint 1B note

Checkpoint 1B, "policy and controls", is complete: its build tasks were all merged by
2026-11-15 (dates in this note are the owner's local dates, as the commits record them;
in UTC, pull requests #134 to #137 are 2026-11-16). Its plan is `m1b-policy-and-controls.md`; this note says what shipped, what the entry check
changed in the plan, what the owner decided on the way and what each decision cost, the
limits that were accepted, how long things take, what was spent, what moved to the
carry-over list, a test that failed twice on one runner, and what a reader should not
assume. It was written by the exit check
(task Z1) from `main` at commit c9ecf9c, the merge of pull request #135, and brought up
to date by the two pull requests after it. The tag `m1b` is on the merge of pull
request #138, the last of the checkpoint, which made the last edit to this note.

## What whizbang can do at this tag

For a person at a terminal, one thing more than at `m1a`: `whizbang spend status` and
`whizbang spend reset --allowance <usd>` show and set what a store may spend. The other
commands are those of 1A (`rascal grow`, `runs list|show`, `store
list|delete|prune|import`, `describe`); `rascal grow` now takes `--config`, and `runs
show` prints a run's settings, its paid requests and where its data went.

What the checkpoint was for is under the commands: the policy around a call, each part
proven against a fake. No model runs, nothing is downloaded and nothing is sent. Every
backend, every download and every recipient in every test is a fake from the test kit.

## What shipped, by strand

Eighty-three pull requests, #56 to #138. Each strand's records are its ADRs.

| Strand | Pull requests | What |
|---|---|---|
| Entry and roots (B0, B0b) | #56, #57 | The carry-over rows re-owned; the spec's sentences on the models directory and on a run a dead process left; the empty modules and the crate `whizbang-backends` |
| What a backend says (B1, B1b, B2, B3) | #59, #62, #64, #65 | A backend states a price that is free, a bound or an estimate only (ADR 0010); a fault says how far its request got and an answer what it cost; recipients, classes of data and the three ways out as interfaces (ADR 0012); fakes that quote, charge, fail at a stage and name a recipient |
| The run record (B4, B4b) | #70, #71 | `run/3`, with a run's settings, its paid requests and its recipients, and a run's place to note them (ADR 0013) |
| The executor's interfaces (I1 to I7, I4b) | #77, #82, #86, #90, #98, #99, #107, #112, #115, #118, #119, #120, #121, #123 | A part in the admission; the ledger's design, with an executable model and a table of histories (ADR 0016); a wrapper around each request, with places for a gate, a lease and a meter; one way to a writer with recovery before it, and a run a dead process left ended as `interrupted`; a policy before a send; one snapshot of the config for a call; an inventory and a lease (ADR 0015) |
| Spend (S1b, S2, S3, S4, S5, S5b, S8, S9, S10, S11) | #105, #106, #110, #114, #116, #117, #124, #125, #126, #127, #128, #129, #130, #131 | The real fold and the real ledger in the store; the per-call cap; the hour of an estimate; a paid request with no bound refused; the meter, which records a dispatch before a request leaves and settles each attempt; a run's end closing its reservation; a run's spend lines; `spend status` and `reset`. Cut down on 2026-11-15 (below) |
| Config and secrets (C1 to C5) | #61, #68, #72, #73, #76, #81, #83, #84, #94, #96, #97 | One config file, its shape and its digest; keys from an env file, never written (ADR 0011); what redaction finds, JSON and UTF-16 among it (ADR 0014); the roster, the cap, the policy, the secrets and a run's settings from one snapshot |
| Egress (E2, E3, E4, E6) | #88, #91, #92, #93, #95 | Denied by default; every hop checked, with a probe and a download as their own traffic; each run's recipients; a picture sent as pixels and nothing else (ADR 0019) |
| Models (M1 to M6b) | #74, #78, #79, #80, #85, #87, #89, #100, #101, #102, #103, #104, #108, #109, #111, #122 | A models directory outside any store: a catalogue, an installation and its identity; staging a download checked by size and hash; publication with a crash at each step; attach and cancel, across processes; holds as file locks; removal that drains and answers busy at its deadline; readiness from what is installed; a lease on the admitted installation (ADRs 0017 and 0018) |
| The binary wired (W1) | #132, #133, #134, #135 | The binary's executor built over the config's source, the real ledger, the cap and the hour, the gate, the meter, and the inventory and the lease where it is given a models directory |
| Tooling (X1 to X6) | #58, #60, #63, #66, #67, #69, #75, #113 | A key directly after a word character; a planted registry fails `just deny`; a pull request's title and body scanned; a push made around the hooks; a script read whole before it runs; no quiet search after a pipe; the crash sweeps two at a time |
| Exit (Z1) | #136, #137, #138 | The exit check: two tests through the binary's wiring, one crash sweep split, the carry-over list and the records brought in line, and this note; the fix of a store refused as held by the process that had let it go (below); two more tests of that fix, and the note and the plan as they are at the tag |

## The clauses of the spec's row, each with its tests

The checkpoint's row in `SPEC.md` section 16, clause by clause. A test is named by its
module and its function; `scripts/tests/test-notes.sh` fails when one named here no
longer exists. Each was found by a search and run by name at the exit check, not taken
from a pull request's text. Those named `spending::…` are in the binary's source
(`crates/whizbang/src/spending.rs`); those named `wired::…` are in the binary's tests
and go through the function that builds the binary's executor, with a fake kind of
backend that a test registers.

**The spend tests of section 6 pass against fakes.** The five cases, as section 6 lists
them since 2026-11-15:

| Case | Tests |
|---|---|
| Nested retries | `settle::a_parent_a_child_and_a_grandchild_each_send_one_request_under_one_reservation_and_the_parent_spends_what_the_child_did_not`, `settle::a_child_that_retries_a_failed_request_has_an_attempt_for_each_and_a_retry_that_does_not_fit_is_refused_before_dispatch`; through the binary's wiring, `wired::a_run_called_from_inside_a_run_takes_the_sum_of_its_own_needs_from_what_its_caller_reserved` and `wired::a_paid_call_from_inside_a_run_that_may_cost_nothing_is_refused_as_over_its_caller_with_nothing_sent_and_the_books_unchanged` |
| Simultaneous calls | `reserve::two_executors_each_told_yes_for_the_last_of_the_allowance_end_with_one_admitted`, `reserve::two_executors_that_judge_together_under_an_allowance_that_covers_both_are_both_admitted` |
| Cancellation after dispatch | `settle::a_run_cancelled_after_dispatch_whose_request_ends_with_no_answer_is_charged_the_bound_as_unresolved`; what the run's end then does with its reservation, `close::a_run_with_an_unresolved_request_releases_only_what_is_not_unresolved` and the cancelled case of `close::a_run_that_reserved_ten_and_spent_three_leaves_seven_for_the_next_call_however_it_ended` |
| A store left by a process that died between dispatch and settlement | `spending::an_open_reservation_a_dead_process_left_with_a_dispatch_is_named_and_a_reset_clears_it` (the store's names planted); `settle::a_settlement_whose_publication_fails_leaves_the_dispatch_unsettled_counted_at_its_bound_and_the_run_abandoned` (made by the meter); through the binary's wiring and the real commands, `wired::a_store_left_by_a_process_that_died_between_a_dispatch_and_its_settlement_counts_the_request_at_its_bound_until_the_owners_reset` |
| A backend that offers only an estimate | `quote::a_call_that_needs_a_backend_offering_only_an_estimate_is_refused_at_admission_with_no_run`, `meter::a_request_to_a_backend_with_no_bound_is_refused_by_the_meter_whether_or_not_its_run_reserved` |

**One of the five was short of its words until the exit check.** The plan's exit box
asks the fourth case of task S9 "and W1 through the binary". Task W1's test,
`wired::the_reservation_of_a_run_a_killed_process_left_is_named_by_spend_status_and_cleared_by_spend_reset_and_recovery_makes_no_name_of_the_ledgers`,
leaves its store after the request was answered and settled, so the moment between a
dispatch and its settlement was shown by S9 with planted names and by the core's meter,
and not through the binary's wiring. The exit check added the third test of that row,
in which the store's file system refuses the settlement's publication after the fake
was asked: the dispatch is in the store with no settlement, `spend status` counts the
request at its bound, nothing sends it again, the next command that writes ends the run
as interrupted and leaves the books alone, and the owner's reset clears it.

The rest of the row:

| Clause | Tests |
|---|---|
| Estimates expire | `age::a_preparation_is_admitted_one_millisecond_before_the_hour_and_refused_at_the_hour`, `age::the_hour_is_a_constant_and_the_part_holds_nothing_that_could_shorten_it`, `age::preparing_again_after_a_refusal_gives_a_preparation_that_is_admitted`; through the binary's wiring, `wired::a_request_that_comes_with_a_preparation_an_hour_old_is_refused_as_stale_by_the_binarys_executor` |
| And are refused when stale | `quote::a_price_that_changed_since_the_preparation_is_stale_at_admission`, `source::a_backend_changed_in_the_file_between_prepare_and_admission_is_a_stale_config`, `inventory::an_installation_replaced_or_removed_under_a_prepared_call_makes_the_preparation_stale`, and from 1A `admit::a_preparation_is_refused_at_admission_when_the_input_changed` |
| A model installs, attaches, cancels and drains with fake downloads | In one test, from this process and a second one: `leases::a_model_installs_is_attached_to_is_cancelled_and_is_drained_from_this_process_and_a_second_one`. Each alone: `publishing::an_install_of_two_files_ends_with_the_pointer_naming_a_verified_installation`; `attaching::two_requests_in_one_process_cause_one_download_of_each_file_and_end_with_one_result`, `attaching::a_second_process_that_asks_while_the_first_is_stopped_sees_progress_and_ends_with_the_same_installation`; `attaching::a_cancel_while_the_download_is_stopped_inside_a_file_ends_the_install_as_cancelled`; `remove::a_removal_during_a_held_installation_refuses_a_new_hold_and_goes_on_when_the_hold_is_let_go`, `remove::the_two_cases_of_a_removal_with_the_hold_in_a_real_second_process` |
| Egress is denied by default | `gate::a_config_that_names_no_recipient_makes_the_policy_that_approves_nothing`, `gate::with_the_empty_policy_every_request_to_a_backend_with_a_recipient_is_refused_and_the_sending_fake_is_handed_none`, `around::with_the_default_policy_a_backend_that_names_a_recipient_is_refused_at_the_gates_place_and_one_that_names_none_is_sent`, `gate::a_refusal_answers_the_action_with_a_fault_and_no_other_backend_is_asked_in_its_place`; through the binary's wiring, `wired::a_recipient_the_config_does_not_approve_is_refused_at_the_gate_and_one_its_egress_table_approves_is_let_through` |

## The controls of section 6 that 1B owns

The kept rows of `SPEC.md` section 6, "Controls from table-manners", that this
checkpoint builds.

| Control | Tests |
|---|---|
| Estimates that bind one exact call | `admit::a_preparation_is_refused_at_admission_when_the_input_changed`, `admit::a_preparation_is_refused_at_admission_when_the_config_changed`, `admit::a_preparation_is_refused_at_admission_when_the_model_identity_changed` (1A), and `quote::a_price_that_changed_since_the_preparation_is_stale_at_admission` |
| Expire after an hour, and are refused when stale | The two rows above on estimates |
| Per-call cap | `cap::a_call_whose_estimate_is_over_the_cap_is_refused_with_no_run_no_reservation_and_its_id_free`, `cap::a_calls_own_cap_lowers_the_cap_for_that_call`, `cap::a_calls_own_cap_above_the_configured_one_is_refused_not_lowered`; through the binary's wiring, `wired::a_call_over_the_cap_of_the_binarys_config_is_refused_before_a_run_exists` |
| A budget, one per store | `reserve::a_store_with_no_allowance_admits_a_call_of_nothing_and_refuses_one_over_nothing`, `spending::a_store_no_reset_was_made_in_allows_nothing_and_status_says_how_to_set_an_allowance`, `spending::after_a_reset_the_earlier_records_are_all_still_read_and_the_balances_start_again`, `spending::reset_is_a_command_and_no_registered_action`; through the binary, `wired::a_paid_call_is_refused_without_an_allowance_and_admitted_with_one_and_runs_show_lists_its_request_with_what_it_was_charged` and `spend::a_reset_and_a_status_through_the_binary_and_a_complaint_that_is_escaped` |
| Model installs: atomic | `crashes::a_crash_at_any_event_of_a_first_install_leaves_it_not_installed_or_whole_part_01_of_96` to part 96 (in four parts until milestone 6's entry check), `crashes::a_crash_at_any_event_of_an_install_leaves_the_previous_or_the_new_whole_part_1_of_4` to part 4, `publishing::an_install_answers_only_when_the_folders_that_name_the_model_are_durable` |
| Checked by hash | `staging::a_wrong_hash_fails_the_download_and_leaves_no_staged_file_that_passed`, `staging::a_short_file_fails_the_download_and_leaves_no_staged_file_that_passed`, `staging::a_file_one_byte_too_long_fails_the_download_and_leaves_no_staged_file_that_passed`, `publishing::a_staged_file_that_is_not_on_the_disk_as_it_was_downloaded_is_not_published` |
| Cancellable | `attaching::a_cancel_while_the_download_is_stopped_inside_a_file_ends_the_install_as_cancelled`, `publishing::a_cancel_before_the_staged_folder_is_renamed_ends_the_install_and_one_after_it_does_not`, `attaching::a_cancel_after_the_pointer_was_replaced_returns_the_installed_result` |
| The previous install left in place on failure | `publishing::a_failed_install_leaves_the_previous_installation_named_and_readable_and_its_status_says_how_it_ended`, `crashes::a_process_that_dies_inside_a_download_leaves_the_previous_installation_and_the_next_installs` |
| A second request for the same model attaches to the running install | `attaching::two_requests_in_one_process_cause_one_download_of_each_file_and_end_with_one_result`, `attaching::a_second_process_that_asks_while_the_first_is_stopped_sees_progress_and_ends_with_the_same_installation`, `attaching::an_attached_request_that_goes_away_does_not_cancel_the_install` |
| Removing a model in use: it drains, then is removed | `remove::a_removal_during_a_held_installation_refuses_a_new_hold_and_goes_on_when_the_hold_is_let_go`, `leases::a_removal_that_starts_while_a_request_holds_the_model_waits_for_it` |
| A timeout answers "busy" | `remove::with_the_hold_kept_past_the_deadline_the_removal_answers_busy_the_mark_is_gone_and_the_next_hold_is_given` |

Two things about those rows. A model is installed and removed through the interface of
`whizbang-backends` and through no command: `whizbang models` is checkpoint 1C's. And
"a budget" is an allowance that only `whizbang spend reset` sets: the config holds a
per-call cap, which can lower what a call may spend and never raise it.

## What the entry check changed in the plan

Task B0 read the plan against `main` after the tag `m1a`. No code had moved, and it
corrected two lines, neither of which changed what was built: task I5 depends on I4b
and not on I4, as the order table already said; and the four new carry-over rows were
added by B0 itself, where the plan had said each would come with its task. It re-owned
the twenty-five rows of the carry-over list that named 1B. B0b then made the edits to
the spec that the reviewed plan had decided, and no others.

The plan changed far more after that, and not at the entry check: the next section.

## What the owner decided, and what each decision cost or saved

- **Recovery does not reconstruct (2026-11-13).** Four review rounds each found a deeper
  hole in a design that tried to work out what a crashed run had done. A run a dead
  process left is ended as `interrupted`, with a record made from its admission alone.
  Saved: the durable facts every task would have kept for recovery's sake. Cost: what
  such a run sent, spent and wrote is declared lost (the carry-over row on a fuller
  record, owner none).
- **The ledger's arithmetic was settled with code in hand (2026-11-13).** Six rounds had
  each gone one level deeper into names and sums written as prose. Task I2 wrote ADR
  0016 beside an executable model of the fold and a table of named histories, 89 in
  the end, run as tests. Cost: the longest record of the project and a strand that
  started late.
- **The less strict rule for a damaged spend record (2026-11-14, and again on
  2026-11-15: "less strict is fine").** A damaged spend record freezes its own
  reservation and stops nothing else. The reviews of ADR 0016 had replaced that by a
  rule under which one damaged record stopped every paid call in the store. Saved: a
  store that one bad name cannot stop. Cost: the books can count less than was spent
  where the names that stated the money are the ones that are gone, by 10 in history 86
  and by 13 in history 87 of the table (ADR 0016, "Limits", (a) and (b)).
- **The spend strand cut down (2026-11-15).** Asked about an edge case of the ledger,
  the owner said that cost would not be heavily enforced in production and that
  penny-accurate accounting should not be over-weighted, and later that day called
  cost control "the least important part of all of this". Twelve spend tasks had not
  started, planned as fifteen pull requests; six were built, as six, each at the
  ordinary review bar. Cut: S5c, S6, S7, S7b, S11b and S12, with the second pull
  requests of S5, S6 and S7. Saved: nine pull requests of crash sweeps, races at a
  barrier, generated suites and a correction command. Cost: nothing closes the
  reservation of a run a dead process left, so it counts in full, which can be more
  than the run spent; nothing corrects one reservation; and the meter, a settlement, a
  closing and the commands have no crash sweep of their own (the carry-over row on what
  the cut left; ADR 0016, "The strand cut down").
- **`spend reset` is the one way out.** With S6 and S12 cut, the owner's reset clears
  every reservation made before it: an open one, an interrupted one, a frozen one, one
  with no hold. Cost: what was outstanding under them is forgotten by the books, and a
  reset cannot clear one reservation and keep another.
- **A call that may cost nothing does not touch the books (2026-11-15, after the review
  of W1).** As first wired, every run wrote six names under `spend/` and made every
  later run slower. A free call is now admitted without the ledger. Saved: about a
  third of the time of a `rascal grow` in a store with history, and a free call is no
  longer refused where the books need their owner. Cost: none known; a paid call still
  pays for the store's history (the carry-over row on what a publication costs).
- **The hook-setup gap accepted (2026-11-14).** Task X4 closed a push made from another
  folder. What no script closes was accepted: a clone where `just setup` never ran, or
  where the hooks are switched off on purpose, can publish unscanned on a branch other
  than `main`. `main` stays covered by the scan CI runs over every pull request's
  commits. And the scan's pattern for a key after a word character keeps one stated
  gap, accepted with task X1, rather than flag ordinary hyphenated text.
- **Cut and moved before building (2026-11-13).** The static scan for sockets was cut.
  The `whizbang models` commands and the sharing of one file between two installs went
  to 1C; the measurement at 100,000 request ids and the ending at once of an admission
  that was not confirmed went to milestone 2.

## The limits accepted, each with where it is written

- Spend control is a guard and a record. It may count more than was spent, never
  promises to match a provider's bill, and is not a hard control for production:
  `SPEC.md` section 6.
- The reservation of a run a dead process left counts in full until the owner's reset;
  no command corrects one reservation; two states of a store have no way out but a new
  store (the largest epoch a name can say, and a store filled to its generation limit):
  ADR 0016, "The strand cut down" and "Epochs"; the carry-over rows on what the cut
  left and on the money in a recovery.
- Where a damaged record took the names that stated the money, the books can be under:
  ADR 0016, "Limits", (a) and (b).
- The meter judges a request against what its process holds and its own reservation's
  names, not against the store's books, and an action can ask for a dispatch outside a
  wrapper: the carry-over rows on what the cut left and on what the meter does.
- An overrun is taken whole from every run above it: its carry-over row, owner none.
- What a run a dead process left did is lost: ADR 0015, "The interrupted record"; its
  carry-over row, owner none.
- A redaction finds the forms it is told of and no others: ADR 0014, "What is
  accepted", with what it does not find pinned by
  `redact::what_redaction_does_not_find_is_known` and
  `redact::what_redaction_of_a_json_body_does_not_find_is_known`.
- The gate governs what whizbang itself sends. A kind of backend is trusted code, and
  nothing stops one that opens its own connection: `SPEC.md` section 10; ADR 0019,
  "Limits".
- A models directory on a network file system is not supported, a folder of its layout
  that is a link is refused by a look and not by a guard, and nothing reads an
  installation's files between their publication and the first adapter that loads
  them: ADR 0017, and their carry-over rows for 1C.
- A mark left by a remover that died keeps the model from use until someone installs or
  removes it: ADR 0017, "Removal"; its carry-over row, for 1C.
- A push that runs no hook at all, and a key of one shape after a word character: the
  two lines kept as accepted at the top of the carry-over list.

## Timing

Measured on 2026-11-15 by the exit check, on the tree of its pull request, #136 (1,993
fast tests; `main` at c9ecf9c had 1,987). The tree at the tag has 1,998: the fix of
the store refused as held added three, and #138 two. With them, one run of `just check`
on the owner's Mac, other builds at work on the machine, took 75.8 seconds of tests.

| What | Where | Time |
|---|---|---|
| The fast tests alone, compilation not counted | The owner's Mac, three runs of `just check`, other builds at work on the machine | 77.8 s on `main` at c9ecf9c, then 77.6 s and 78.6 s on this tree, against a limit of 180 s |
| The same, `check (macos-15)` | GitHub, `main` at c9ecf9c | 99.0 s; the job took 9 min 8 s |
| The same, `check (ubuntu-22.04)` | GitHub, `main` at c9ecf9c | 42.1 s; the job took 3 min 46 s |

**The macOS runner's total is not steady, and the limit has not moved.** On unchanged
code its test time has ranged from about 80 to about 200 seconds, and two good pull
requests failed there on total time alone and passed on a rerun with no change: both
from the record the orchestrator kept of the checkpoint. What the exit check read
itself, from the job logs of the last twenty-six finished runs before this note: 71 to
149 seconds, on a tree that grew by a few tests between them. The
limit of 180 seconds is the owner's to change and is unchanged; so is the limit of 30
seconds for one test. The budget of `SPEC.md` section 13 for test time, under 3 minutes
on the owner's Mac, is met on every run measured here. Its budget for pull-request CI,
under 10 minutes on both runners, is met on the Linux runner and is not reliably met on
the macOS runner. The macOS job took 9 min 8 s on `main` at c9ecf9c; of the last 44
finished runs of the checkpoint, up to the run of pull request #138 at commit 5c1cd05,
22 took more than 10 minutes there, the longest 13 min 32 s, and that run itself took
10 min 9 s. Section 13 says what follows when a budget is exceeded, and the budget is
the owner's to change: the carry-over list has the row.

Seconds by suite, summed over tests that run side by side, so the rows add up to more
than the test time (the third local run, on the exit check's tree):

| Suite | Tests | Seconds |
|---|---|---|
| `whizbang::it` | 110 | 128.7 |
| `whizbang::bin/whizbang` | 26 | 46.0 |
| `whizbang-backends::it` | 247 | 178.5 |
| `whizbang-core` | 34 | 1.4 |
| `whizbang-core::it` | 1,292 | 207.1 |
| `whizbang-eval::it` | 38 | 1.4 |
| `whizbang-rascal::it` | 44 | 4.9 |
| `whizbang-squint::it` | 56 | 12.9 |
| `whizbang-testkit` | 3 | 0.1 |
| `whizbang-testkit::it` | 143 | 11.9 |

On the two runners, for `main` at c9ecf9c (1,987 tests), in the same order: macOS 10.2,
3.6, 107.7, 0.9, 133.5, 1.6, 4.6, 17.8, 0.0 and 14.8; Linux 2.5, 0.7, 44.5, 0.2, 88.4,
1.0, 2.4, 15.2, 0.0 and 9.8. Each of the binary's tests starts a process, which costs
most on the owner's Mac.

**The crash sweeps with nothing else beside them, two at a time.** The sweeps crash a store or a models directory before every
step of something and try every way the disk can lose what was not durable. They run
two at a time (`.config/nextest.toml`). Run with nothing else beside them, on the
owner's Mac, as the sum of a sweep's parts and its slowest part:

| Sweep | Parts | Seconds, all parts | Slowest part |
|---|---|---|---|
| The store: any event of a publication | 1 | 2.7 | 2.7 |
| The store: its creation; its first publication | 1 each | 0.1; 2.1 | 0.1; 2.1 |
| The store: a prune; a deletion; a publication that only keeps a name | 1 each | 0.4; 0.7; 1.1 | the same |
| The store: random runs of publications, refusals and a crash | 1 | 1.0 | 1.0 |
| A run crashed inside its commit; a cancelled run inside its end | 1 each | 1.4; 1.5 | the same |
| An admission with a part, even steps and odd steps | 2 | 2.0 | 1.3 |
| A recovery followed by another open, the parts' records | 4 | 2.8 | 1.2 |
| The recovery of an interrupted run followed by another open | 4 | 3.2 | 1.3 |
| An admission through the real ledger | 6 | 25.0 | 4.4 |
| Models: staging, a crash at any event; each step refused | 1 each | 1.1; 0.1 | the same |
| Models: a first install | 4 | 5.9 | 2.4 |
| Models: an install over a previous one | 4 | 9.0 | 3.3 |
| Models: a repair | 4 | 8.8 | 3.6 |
| Models: after a refused step of a first install | 8 | 5.5 | 0.9 |
| Models: after a refused step of an install over a previous one | 4 | 7.8 | 2.5 |
| Models: after a refused step of a repair | 4 | 8.9 | 2.9 |
| Models: after a refused step of a publication | 1 | 1.3 | 1.3 |
| Models: each step of a publication refused, first and over a previous one | 1 each | 0.3; 0.5 | the same |
| Models: the pointer's sync refused and then a crash | 1 | 2.4 | 2.4 |
| Models: a removal, a crash at any event; each step refused | 1 each | 3.5; 1.3 | the same |
| `grow` crashed inside its commit | 1 | 1.6 | 1.6 |
| `check`, its end refused at any step | 1 | 0.6 | 0.6 |
| A streamed file on the test kit's disk | 1 | 0.0 | 0.0 |

Sixty-four tests, 102.5 seconds summed and 51.2 elapsed. The slowest is the admission
through the real ledger, which no later spend task added to.

**Tests that have come within 8 seconds of the limit of 30 for one test, under load.**
The reviews of the checkpoint named three, each on a busy machine inside a full run:
parts 1 and 2 of 4 of the sweep after a refused step of a first install, and
`nested::a_run_calls_as_many_actions_as_its_record_lists_and_one_more_is_refused`.
In the runs measured for this note none came that near: the slowest single test of three local runs was 16.7 seconds, and of the two runners
on `main` 7.9 seconds on macOS and 5.5 on Linux. The exit check split the
sweep into eight parts (in 96 since plan m4's task A1, which cut it again:
`crashes::a_crash_at_any_event_after_a_refused_step_of_a_first_install_leaves_it_not_installed_or_whole_part_01_of_96`
to part 96), a change to its tests alone: inside a full check on the owner's Mac its two slowest parts took 14.1 and 16.7 seconds
before, as parts 1 and 2 of 4, and 12.3 and 12.7, then 11.7 and 12.6, after, as parts 1
and 2 of 8. That is less than the halving the split was for. Alone, each part takes under a second. What makes two of
them slow is that they are the first sweeps to start, when every core is busy with the
rest of the run, and a smaller part does not change that: the split took about a
quarter off the slowest and leaves it where the next two sweeps to start already are.
No limit was touched. The test of 256 calls
is as it was: its time is the cost of a publication, which is milestone 2's (its
carry-over row).

## A test that failed twice on Linux: a fault of the code, found and fixed before the tag

Two pushes to `main` on 2026-11-15 failed on the Linux runner, and on nothing else: the
merges of pull requests #130 (commit 6808ee8) and #131 (commit 7873c2a). It was not a
limit on time: both runs stopped at the first failure after about 22 seconds of tests.
It was one test, the same both times, failing in 0.04 seconds:
`interrupted::a_run_that_is_executing_in_this_process_and_one_this_process_abandoned_are_left_alone`.

Both times it panicked in the helper that opens its store again as the next process
would (`crates/whizbang-core/tests/it/contract/interrupted.rs`, line 97): the store was
refused as held to write. The first run said `Held { root: ".../store", holder: None }`,
the second `Held { root: ".../store", holder: Some("process <the test's own> (it-…)") }`.
So the writer's lock was still held by the test's own process when it asked again.

It was intermittent. The runs of both pull requests were green on their own heads, the
macOS runner passed on both pushes, and `main` was green before them and on every run
after them until the fix. Nothing under the core's `contract/` or `store/`, and nothing
of the test kit but one test of its own, changed between the last green `main` (commit
44cfe7b) and the two failures: what those two merges changed is the spend strand's
ledger and meter and the wrappers around a capability request, and this test uses the
test kit's fake ledger and asks no capability. After the failures `contract/engine.rs`
did change (commit 85eab0f, merged with #135), but only for a call whose estimate is
nothing, and this test's calls reserve 250,000, so its path is the same. The fault was
there from the day the test was added (task I4b); the two merges changed what else ran
beside it on the runner, and no more.

**The cause, found before the tag: a fault of the code, and fixed.** The helper is
called twice in that test, and it was the second call that failed: after the test
waited for a run it had started on a thread of its own, dropped its executor and its
opening, and opened the store again. A started run's thread holds a clone of its
executor, which holds the store's writer and with it the lock; and whoever waited for
the run was answered while the thread still had that clone, and while the run's guard
held the writer too. The test then asked for the lock a moment before the thread let it
go. It was told of its own process while the thread still held the writer, and of
nobody when the thread was dropping it at that moment: a writer takes its name out of
the lock file before its lock ends. Both were shown on a Mac by holding the thread at
each of the two points: the test then failed every time, with each of the two holders
in turn. No child process is in it, so it is not the case of the row on the store's
locks. No command of the binary starts a run on a thread of its own, so no user of
this build's binary could meet it; a process that keeps a store and starts runs could.

The executor now lets go of everything a started run holds of the writer before a wait
for the run can answer: the thread drops its clone first, and the guard takes the run
out of the table of open runs through a value that holds the table and no writer
(`store::Closer`). Two tests fail without it, each on an assertion and without a race:
`started::a_wait_for_a_started_run_answers_only_when_its_thread_holds_nothing_of_the_store`
and, among the core's unit tests,
`engine::tests::the_executors_hold_on_a_run_holds_nothing_of_the_store_when_a_wait_can_answer`.
The test that failed is unchanged, and passed 200 times of 200 after the fix. Pull
request #138 added two tests for what those two leave open, each
shown to fail on the change it is there for:
`engine::tests::a_started_run_that_was_admitted_leaves_the_table_with_its_thread_holding_nothing_of_the_store`
takes the hold that an admission makes on a run, where the unit test above builds one
by hand; and
`started::a_started_run_whose_executor_panics_outside_its_action_is_abandoned_only_when_its_thread_holds_nothing_of_the_store`
has the executor's clock panic once the action has come back, so that the thread lets
go by unwinding. ADR 0015
says what is left ("One way to a writer, with recovery before it", limits): only a wait
is that event, and the thread of a started run that nobody waited for holds the writer
until it is done (its carry-over row, beside the one on the locks).

## Spend

0 US dollars. No paid call was made and none could have been: the lock file names no
network client among its 70 crates, no code under `crates/` or `scripts/` opens a
socket or starts a program that would, the shipped binary registers no kind of backend
(`wiring::kinds`), and the names of providers' key variables appear only in the
config's tests, as names, with made-up values. Checked by search at the exit check.

## What moved to the carry-over list, and to whom

Each has its row in `docs/carry-over.md`. No row is owned by a task of 1B, finished or
cut.

| What | Owner |
|---|---|
| `whizbang models list`, `get` and `remove` with their guide, and how one who attaches is paced; `whizbang backends status`; the real network client and the shipped catalogue, and with them a command that names a models directory; what the worker's queue owes to removal; installs that share a file; a model's device and whether a cache answered; the rule that an adapter says `not_sent` only when nothing left; a backend made from a config sending with no credential; a recipient compared with its upstreams; the settings of the first real kind; a local backend loading the installation it was handed under; a removal that takes no cancellation; a mark left by a remover that died; `runs show` cutting an installation's identity; the lock of the models directory taken by trying, its links and its unread files | 1C |
| Pull-request CI on the macOS runner over its budget of 10 minutes about every second run, and its test time near the limit of 180 seconds | 1C, at its entry check; the limits are the owner's |
| What every publication costs, and the ledger's fold reading closed epochs; an admission that was visible and not confirmed, ended at once; image preparation and a quote from a request's shape; a provider's sending; repairs and the checklist in the run record; the bound on a ledger being asked again | 2 |
| Recipe steps in the run record; the limit of 256 calls | 3 |
| What the cut of the spend strand left unbuilt; the money in a recovery; an overrun taken whole from every run above it; a dispatch asked outside a wrapper and a record's kinds for a refused dispatch; a fuller record of what an interrupted run did | None; reopen on need |
| A push that runs no hook; a key of one shape after a word character | Dropped: accepted by the owner, each kept as a line |
| A reader that ends early after a pipe, in the forms the rule does not find | When a pipeline fails at random |
| A writer let go, and the store opened again, while the thread of a started run that nobody waited for still holds it | 5, before `whizbang serve`; sooner with the first door that starts a run and lets its writer go |

The exit check removed one row as met (a refused request has no dispatch record:
`meter::a_request_the_gate_refused_has_no_dispatch_record`), took the finished halves
out of five rows and gave what was left its later owner, and added one (`runs show`).
It also added the row on the test that failed on Linux; the fix left in it what remains,
for milestone 5.

## What the fake ledger is still for

The test kit's fake ledger is kept (`whizbang_testkit::ledger`). Twelve test files
still use it, for a ledger with an allowance set in memory, one that counts how often
it was asked, or a dispatch made by hand: the core's `contract` tests of the executor
(`around`, `cancel`, `interrupted`, `ledger`, `nested`, `part`, `prepare`), its `spend`
tests of the cap, the hour and two cases of the admission, and the run tests of `grow`
and `check`. The binary, and every test of the money itself, uses the real one. The
module's own comment says the same.

## What a reader should not assume

- **The binary has no real backend.** It registers no kind, so a config that names a
  backend makes it "not ready" under the binary's own commands. Every paid call, every
  send and every refusal at the gate in this checkpoint is through a fake kind that a
  test registers with the function that builds the binary's executor. No command runs
  an action that needs a backend.
- **No command names a models directory.** The inventory and the lease are built and
  wired, and are in place only where the wiring is given a directory, which only a test
  does. Under the binary's own commands no local model is ready and no lease is taken.
  Nothing was ever downloaded: a download is a fake that serves bytes from memory.
- **Spend control is a guard and a record, not a hard control.** It was never compared
  with a provider's bill, because there is no provider. It errs to counting more. It
  has stated cases where it counts less. Its proofs are the model, the table of
  histories and the crash sweep of an admission; the meter, a settlement, a closing and
  the two commands have ordinary tests and no sweep.
- **A free run does not touch the spend books, and a paid one reads all of them.** A
  call that may cost nothing is admitted without the ledger and makes no name under
  `spend/`, so the books do not list it. A paid admission folds every name the store
  holds under `spend/`, those of epochs a reset closed among them, so it is slower the
  longer a store has been paid from. The remedy is milestone 2's (the carry-over row
  on what every publication costs).
- **"Through the binary" mostly means through its wiring.** `spend status`, `spend
  reset`, `runs show`, `runs list` and `store prune` are run as the real binary in
  those tests; the call itself is made by a test on the executor the binary's own
  function builds.
- **Egress is denied by default for what whizbang sends, and whizbang sends nothing
  yet.** The binary's executor has a policy and no sender. The checks on a hop, a
  download and a probe have met fakes only.
- **A crash is a shim's.** The sweeps crash a file system written for the tests, which
  drops what was not synced in each way it knows. A real child process is killed in
  a few tests of the models directory and of the store's lock. In the spend tests
  "a process that died" is a store whose file system refused a publication, or names
  planted in a store.
- **The record of an interrupted run says little on purpose,** and its reservation is
  not closed by anything but the owner's reset.
- **The limits on time are close on one runner.** A pull request that fails the macOS
  job on total test time alone may be a good one. And the macOS job as a whole is over
  the 10 minutes of `SPEC.md` section 13 about every second run.

## What checkpoint 1C starts from

`main` at the commit tagged `m1b`, with CI green and both scans clean; `run/3` as the
record's version; a build with no kind of backend, no network client and no catalogue;
the interfaces a first local backend meets, each proven against a fake: a kind made
from a config's table, the inventory and the lease over a models directory, the gate
and the checked ways out, the meter, and the installer; and the rows for 1C above,
the first of which is a command that names a models directory.
