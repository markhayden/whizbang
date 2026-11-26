# Milestone 5 note

Milestone 5, "Playground", ends with task N1 of `m5-the-playground.md`. This note was written
by N1's pull request from its branch, with `main` at 5dade953 (the merge of #278, task E1).
With it every box of the plan is ticked but N1's own, whose last two steps come after this
merge and are the orchestrator's: the tag `m5`, made on this pull request's merge, and the
archiving of the four old repos, on the owner's yes (open question 4). The last clause of the
row, "old repos archived", is met only then, and this note's last line will say so.

## What whizbang can do at the end of milestone 5

Beyond milestone 4: a fourth door, `whizbang play`, the playground (`docs/guides/playground.md`).
It starts a host on loopback inside the `whizbang` process, opens Chrome at an address that
carries a token made at launch, and serves the interface the binary carries in `ui/dist/`.
In it a person picks a kit, gives a task its pictures, words, masks and a location drawn on
the canvas, sees each choice of backend ready or not and why, reads the plan and its bound
before a paid run, runs it, watches its steps and attempts, stops it, closes the tab and
finds it again. A result is compared with its source (side by side, swipe, difference),
sent to another task, run again as a new draft, copied as a command that makes the same
call, and its report is read in place or exported as one document. History, its figures
(those of `runs stats`) and every report survive a restart. The Models screen installs,
cancels and removes a local model, says which cloud backends have a key without ever showing
one, and resets the allowance. Quit, or Ctrl-C in the terminal, cancels what is running,
accounts for every request in the ledger and releases the store at once.

## What shipped

| Pull requests | Task | What |
|---|---|---|
| #259 | Words | Kits, tasks and tools as the words people see (ADR 0040), before the plan |
| #261 | Plan | The plan of milestone 5, after five rounds of outside review |
| #268 | H1 | The host and its guard (seventeen negative tests), `whizbang play`, the token, reconnect, Quit and the terminal's signals, the store held for days; ADR 0041 |
| #265 | K1 | What the screens stand on, in the core: readiness as a kind, each need's bound, typed events, a journal held to a bound, a call that must run under a preparation, the unconfirmed admission |
| #269 | U1 | `ui/`: Preact and Vite, the tokens, the components, the shell, the fixture host, `just ui-*` and `just e2e`, the two `e2e` jobs in CI; ADR 0043 |
| #271 | W1 | Run a task: Home, a kit, the workspace, Running; prepare, admit, poll, stop, reconnect; the description's kits and titles |
| #274 | W2 | The result: Compare, the verdict card, attempts, cost, Send to, Run again, Copy command |
| #273 | W3 | History and its figures, reports in an isolated frame and their export, the tool records in place, the brand on Home |
| #272, #275, #279 | M1 | The Models screen: install, progress, cancel, remove, keys set or not, the allowance and its reset; its review's follow-ups; a cancel that leaves another process's install said as detached (the Linux flake of #275) |
| #277 | CI | The fast tests' sweeps in jobs of their own, `sweeps (<runner>)`, each part under the 300 seconds |
| #281 | Gaps | What an audit against the acceptance table found missing: Quit in the top bar and row 10's two Chrome cases, a picture picked on Home or in search, the canvas's Original, Mask and Result, a task's earlier runs, the binding picker read from the store |
| #282 | Live fixes | The bugs E1's first live runs found (below), and the Chrome tier in two shards on each runner, `e2e (<runner>, shard <k>)`, each held to 90 seconds |
| #278 | E1 | The guide whole, and the record of the live runs in the playground (`docs/results/m5-playground.md`) |
| This one | N1 | This note, the boxes, the carry-over rows, the spec's wording owed |

Beside them, milestone E's first slice ran (#260, #262, #263, #264, #266, #267, #270, #276,
#280); its note is `docs/plans/e1-note.md`. The title of #281 on GitHub names E's loop
round, which is #280; its branch, `m5-gaps`, and its commits are the gaps.

## The clauses of the spec's row, each with its evidence

The row for milestone 5 in `SPEC.md` section 16: "Every kept playground item in section 7
passes its acceptance case in Chrome; the playground API's negative tests pass; old repos
archived". A Chrome case is named `<file>.spec.ts: <title>` and runs in Google Chrome against
the fixture host (the host and the interface as they ship, with fake models), in a new
store; `scripts/tests/test-notes.sh` fails when a case or a test named here no longer exists.
Every Chrome case runs in CI's `e2e` jobs on `macos-15` and `ubuntu-22.04` on every pull
request, and locally under `just e2e`.

**Every kept playground item in section 7 passes its acceptance case in Chrome.** The eleven
kept rows of milestone 5: the playground's ten and squint's "Report viewer and export".

| # | Row | Shown by |
|---|---|---|
| 1 | Run both tools: typed inputs including lists and artifact bindings; drawing a location; readiness per choice; reviewing the bound cost before a paid run; progress, stop and reconnect | `run.spec.ts: a rascal task runs from a picture and words to a result`; `run.spec.ts: a squint task runs from two pictures and a brief to a verdict`; `run.spec.ts: each of the five squint use cases runs from its form`; `inputs.spec.ts: a variadic port takes, orders and removes pictures within its count`; `inputs.spec.ts: an artifact port binds a picture from the library and from a drop`; `inputs.spec.ts: a picture picked in search starts a task, whose mask port lists the store's masks after a reload, and a task shows its earlier runs with their mask and result`; `inputs.spec.ts: a box and a stroke drawn on the canvas become the location the task is given, and a click and a drag become segment's point and box`; `readiness.spec.ts: each choice shows ready, no key or get with its size, and a choice that is not ready can be picked and Run says what is missing`; `readiness.spec.ts: a paid run shows each need's bound and the total before Run, and Run is refused once the preparation is stale`; `run.spec.ts: progress shows each step as it starts and ends`; `run.spec.ts: the running step list shows a step running and an attempt started before the run ends`; `run.spec.ts: stop ends the run as cancelled and nothing is committed`; `reconnect.spec.ts: a reloaded page shows the running run and follows it to its end`. Behind them: `play::a_start_without_a_matching_fresh_preparation_is_refused_as_stale`, `play::a_drawn_shape_is_committed_as_a_location_bound_to_its_picture`, `play::each_choice_is_prepared_and_answered_with_its_readiness_kind`, `play::a_backend_is_ready_by_its_own_readiness_and_a_key_not_set_names_its_variable`, `admit::a_call_needing_a_preparation_without_one_is_refused_stale_and_reserves_nothing`, `prepare::each_need_hands_out_its_request_count_and_bound_and_they_sum_to_the_estimate`, `watch::a_second_caller_sees_a_step_started_before_the_parent_ends`, `play::an_event_message_with_an_escape_sequence_is_answered_escaped` |
| 2 | Closing the tab does not cancel a run; reopening reconnects to it | `reconnect.spec.ts: a run started in a tab that is closed ends completed, and a new tab finds it in Continue and opens its result`; `play::a_run_started_by_the_host_is_found_by_its_id_after_its_caller_is_gone`, `play::a_repeat_after_the_handle_expired_or_the_host_restarted_answers_its_run` |
| 3 | Before and after view of a source and its result | `compare.spec.ts: side by side, swipe and difference show the source and the result at one zoom`; `compare.spec.ts: two pictures wider than half the canvas both fit side by side, at one zoom` (failed once on `main`, below) |
| 4 | Send a result to another action; rerun into an independent draft; hand one tool's output to the other | `send.spec.ts: send to squint opens check an edit with the source, the edit and the words tagged from rascal`; `send.spec.ts: back to the run returns to the sending result`; `send.spec.ts: run again opens a draft whose changes do not touch the result it came from`; `send.spec.ts: a result's mask sent to another rascal task binds as its mask`; `send.spec.ts: a removal sent to check an edit carries the job the loop writes`; `play_result::send_to_offers_each_task_whose_ports_a_results_outputs_fill`, `play_result::send_to_leaves_a_port_empty_when_two_ports_take_the_same_type` |
| 5 | Model installation from the interface | `models.spec.ts: get installs a model with its progress, and a second get attaches to the same install`; `models.spec.ts: cancel leaves the earlier install in place`; `models.spec.ts: remove of a model in use answers busy and the model stays`; `models.spec.ts: no key is shown for a backend without one and no key is ever in the page`; `models.spec.ts: a reset needs the amount and the word reset, and the allowance is then what is spent since it`; `play_models::a_second_get_attaches_to_the_running_install`, `play_models::a_quit_during_an_install_cancels_it_and_leaves_the_earlier_install`, `play_models::a_removal_of_a_model_in_use_answers_busy_and_the_model_stays`, `play_models::no_response_of_any_route_holds_a_key_a_sign_in_or_the_token`, `play_models::a_cancel_of_an_install_another_process_runs_leaves_it_going_and_says_detached` |
| 6 | History that survives a restart; evidence that survives a reopen | `history.spec.ts: runs are listed after the host is stopped and started again, with their result, cost and time`; `history.spec.ts: filters by kit, result and words narrow the list and the figures`; `history.spec.ts: a finished run's evidence (steps, findings, places, pictures) is the same after a restart`; `play::history_filters_by_kit_task_result_and_words`, `play::stats_answers_what_runs_stats_answers_for_the_same_filter`, `play::history_lists_the_runs_that_made_an_output_of_a_type_with_their_outputs` |
| 7 | Reports for both tools shown in place and still readable after a restart; squint's exported as a document | `report.spec.ts: a rascal result's report shows its steps, masks and cost in place, its pictures decoded and its styles applied`; `report.spec.ts: a squint result's report shows its verdict, findings and the places on the picture in place`; `report.spec.ts: both reports read the same after a restart`; `report.spec.ts: export of a squint report downloads one document that opens alone, offline, and holds its pictures`; `play::report::a_squint_report_holds_its_verdict_findings_places_and_turns_and_no_script`, `play::report::the_document_opens_alone_with_its_pictures_inline`, `play::report::a_withheld_part_a_missing_picture_and_a_failed_turn_each_show_as_such` |
| 8 | A saved run gives a usable command for a pipeline, from its saved inputs and resolved settings | `command.spec.ts: copy command gives a command that, run in a shell against the same store, makes the same call` (the case quits the host, runs the copied text with the binary and compares the two runs' calls); `command.spec.ts: the command of a check sent from a removal carries its whole job`; `command_of::tests::the_command_of_a_run_makes_the_same_call`, `play_result::the_command_quotes_every_argument_for_a_posix_shell` |
| 9 | Saved history owns a snapshot of its inputs; moving or editing the original file does not change it | `inputs.spec.ts: a picture changed on disk after it was sent leaves the run's input and result as they were`; `play::the_first_import_into_a_new_store_keeps_the_picture_and_its_name_in_one_publication` |
| 10 | Quit and terminal shutdown cancel and account for running work and release the store | `quit.spec.ts: quit with a run going cancels it, its record says cancelled, the ledger has no open reservation, and the store takes a writer at once`; `quit.spec.ts: an interrupt from the terminal does the same`; `play::a_quit_cancels_every_run_waits_and_releases_the_store`, `play::a_run_that_outlives_the_grace_is_counted_and_ended_as_interrupted_by_the_next_writer`, `play::two_interrupts_and_an_interrupt_with_every_worker_stalled_each_quit`, `play_result::the_record_of_a_run_stopped_while_its_fill_was_out_counts_that_request`. Live: E1's `quit` case (below) |
| 11 | Squint: report viewer and export | Row 7's squint cases, and `report.spec.ts: model and tool text holding markup shows as text and runs nothing`; `report.spec.ts: the report frame cannot read the page's storage or call the api`; `play::report::every_text_from_a_model_or_a_person_is_escaped` |

Beside the rows: `SPEC.md` section 8 asks that the playground show each tool's record,
`records.spec.ts: a task's record opens from the workspace with its examples and limits, its
pictures decoded and its styles applied` (with `play::the_embedded_records_are_the_reference_folder`);
a third kit appears with no change to the interface, `home.spec.ts: a third kit of fake
actions shows on home, in the switcher and in search`; the page loads only from its host,
`home.spec.ts: the page loads with the token, removes it from the address and shows Home`,
`home.spec.ts: a request from the page without the token is refused and the page says to run
whizbang play again`, `home.spec.ts: the page loads nothing from any origin but its host`;
the top bar's allowance, `run.spec.ts: the top bar says what is spent of the allowance, and
says it again once a paid run ends`; a prompt that finds nothing, `run.spec.ts: a prompt that
matches nothing says so plainly, shows the original, and the run keeps its outcome`; and two
uploads at most, `connections.spec.ts: pictures held open on the host take two connections a
tab and leave the two that polls, stop and quit need`. 52 Chrome cases in all, in sixteen
files.

**And live, by hand.** The by-hand check of the loop that milestone 4 left here
(`docs/carry-over.md`, "Acceptance checks moved here"), made by the orchestrator in the
playground on the owner's Mac (`docs/results/m5-playground.md`, held by
`evidence::the_playgrounds_record_holds_only_aggregates_and_run_ids`): `pass`, a clean pass;
`vague`, a prompt that names nothing, which stopped in 1.4 seconds with nothing sent; and an
unset key, `unchecked` (one fill, DONE, unchecked, saying why) and `no-key` (Run refused before
any run started); and `quit`, a quit 8.4 seconds into a paid fill. The `pass` case passed, so
the carry-over line is closed.

**The playground API's negative tests pass.** Seventeen, in the fast tier, against the host
on a real loopback socket (`crates/whizbang-serve/tests/it/play_guard.rs`):
`play_guard::an_api_call_without_the_token_is_refused_401_and_names_nothing`,
`play_guard::an_api_call_with_a_token_of_an_earlier_launch_is_refused_401`,
`play_guard::a_request_whose_host_is_not_loopback_at_the_port_is_refused_421`,
`play_guard::a_request_from_another_origin_is_refused_403_and_carries_no_allow_origin_header`,
`play_guard::a_change_without_an_origin_is_refused_403`,
`play_guard::a_cross_site_fetch_metadata_header_is_refused_403`,
`play_guard::a_picture_over_64_mib_is_refused_413_before_it_is_read_whole`,
`play_guard::a_picture_over_the_pixel_limit_is_refused_413_naming_the_limit`,
`play_guard::a_json_body_over_1_mib_or_deeper_than_the_limit_is_refused`,
`play_guard::a_fifth_running_call_is_answered_429_and_polling_cancel_and_quit_are_still_answered`,
`play_guard::a_client_that_trickles_its_headers_or_reads_nothing_is_closed_at_its_deadline`,
`play_guard::a_third_upload_is_answered_429_after_its_headers`,
`play_guard::two_stalled_uploads_and_ten_trickling_clients_leave_polling_cancel_and_quit_answered`,
`play_guard::a_request_naming_a_path_in_any_argument_is_refused`,
`play_guard::the_token_is_in_no_log_event_record_or_error`,
`play_guard::the_host_listens_on_loopback_only`, and beside them
`play_guard::every_answer_carries_the_guards_headers_and_the_page_its_policy`. The four of
`SPEC.md` section 10 again in Chrome, as a hostile page would try them:
`security.spec.ts: a fetch of the api without the token from another origin is refused`;
`security.spec.ts: a fetch with the token from another origin is refused and the page cannot
read the answer`; `security.spec.ts: a request to the host under another name that resolves
to loopback is refused`; `security.spec.ts: an upload of a picture over the pixel limit is
refused, naming the limit`. And no response holds a key, a sign-in or the token
(`play_models::no_response_of_any_route_holds_a_key_a_sign_in_or_the_token`).

**Old repos archived.** Not yet: after this pull request merges and `m5` is tagged, on the
owner's yes (open question 4), the orchestrator checks that every kept row of section 7 that
names rascal, squint, wafflery or table-manners has its replacement, sets each repository's
description to a pointer to whizbang and archives it. No file in an old repo is changed and
none is deleted.

## The live runs, and what they found

`docs/results/m5-playground.md`: five runs on the binary built at d62a2182, the E loop's
store and config, `gemini-4-argon` the default judge, SAM 3 on the Mac and Bria Eraser
through fal. `fail`, the first, ended FAIL in one attempt and stopped by design, which is a
question of the judge for milestone E; `pass` passed in one attempt at 0.070 dollars, though
part of the named object was left, a concrete case of the judge round's finding that Argon
passes runs whose mask left part of the object; `vague` ended as an error in 1.4 seconds,
nothing sent or spent; `unchecked` ran one fill and ended DONE, not PASS, saying no judge was
ready; `no-key` started no run; `quit` was cancelled 8.4 seconds into its fill, its 5.93
dollars released, the fill booked at its bound as unresolved, the store writable in under a
tenth of a second.

Those runs found eleven bugs, all fixed by #282 before the record merged: the top bar showed
what was left as if it were spent, never read it again, and was missing on a new store;
Compare's side by side clipped the picture after; a task returned to after a run sometimes
showed its lists of runs and masks empty (a race between the first read of history and the
first look at the runs going); the form showed NOT READY before its first preparation; with
no key, the segmenter on the Mac was marked NO KEY and the unready fill called "local" and
"free", with the reason in the code's words; a vague prompt's message was in the code's
words and its canvas stayed loading; `whizbang runs show` of a cancelled run said 0.00 while
its child had 0.04 unresolved; the rows of the binding picker could not be told apart; an
upload answered 429 was not tried again; the run's page asked for news about four times a
second; and an unchecked run gave its reason twice, in the code's words, and still named the
judge. One follow-up was found and not fixed: Home's library lists only the pictures given
through the playground (below).

## Timing

From the last CI run of #278 (run 45028988928, at 5b99d978, the tree `main` now holds), and
the shards of #282 (run 45026373597). Wall time of each job, compilation included; the
budget of `SPEC.md` section 13 is under 10 minutes for pull-request CI.

| Job | `macos-15` | `ubuntu-22.04` | Test time |
|---|---|---|---|
| `check` (every fast test but the sweeps, and the UI's unit tests) | 8 min 7 s | 5 min 41 s | 148.7 s and 99.0 s, limit 300 |
| `sweeps` | 5 min 29 s | 2 min 22 s | 101.3 s and 22.3 s, limit 300 |
| `scripts` | 11 min 25 s | 5 min 29 s | |
| `release` | 4 min 20 s | 4 min 20 s | |
| `e2e`, shard 1 | 2 min 53 s | 1 min 52 s | 75.9 s and 47.4 s, limit 90 |
| `e2e`, shard 2 | 3 min 28 s | 2 min 45 s | 81.4 s and 70.1 s, limit 90 |
| `secrets` | | 2 min 45 s | |
| The run, start to `ci` | 11 min 39 s | | |

**Pull-request CI is over its 10 minutes on macOS.** `scripts (macos-15)` took 11 min 25 s
here, and 11 min 17 s and 12 min 29 s on #278's two runs before; at milestone 4's end and
at the start of this milestone it took 8 to 10 minutes. The job runs the script tests,
which gained the playground's install check and the Chrome tier's scripts. A carry-over row
is opened; the budget is not raised without the owner.

**By suite**, the fast tests, each suite's tests' own seconds added up (they run side by
side, so the sum is more than the test time):

| Suite | Tests | `check`, macOS | `check`, Linux | Sweeps | Sweeps, macOS | Sweeps, Linux |
|---|---|---|---|---|---|---|
| `whizbang::it` | 663 | 214.3 | 163.6 | | | |
| `whizbang-core::it` | 1,384 | 78.2 | 67.9 | 27 | 89.3 | 21.0 |
| `whizbang-backends::it` | 311 | 47.0 | 25.9 | 124 | 108.5 | 22.0 |
| `whizbang-squint::it` | 175 | 31.6 | 56.9 | 2 | 10.8 | 6.3 |
| `whizbang-rascal::it` | 150 | 20.5 | 15.1 | 1 | 3.5 | 1.0 |
| `whizbang-testkit::it` | 151 | 16.0 | 19.4 | 1 | 0.1 | 0.0 |
| `whizbang-eval::it` | 67 | 5.3 | 6.9 | | | |
| `whizbang-backends` | 63 | 5.3 | 7.4 | | | |
| The other five (`whizbang`, `whizbang-core`, `whizbang-serve`, `whizbang-serve::it`, `whizbang-testkit`) | 82 | 2.8 | 1.6 | | | |
| The UI's unit tests, 21 files, as the clock saw them | 205 | 5.8 | 5.8 | | | |

**The Chrome tier, by file** (`macos-15` and `ubuntu-22.04`, seconds): `run.spec.ts` 8 cases,
27.1 and 24.8; `send.spec.ts` 5, 25.5 and 21.4; `report.spec.ts` 6, 24.0 and 21.1;
`history.spec.ts` 3, 13.9 and 10.0; `inputs.spec.ts` 5, 13.6 and 9.5; `command.spec.ts` 2,
11.0 and 4.5; `models.spec.ts` 5, 8.7 and 3.7; `compare.spec.ts` 2, 7.7 and 4.8;
`reconnect.spec.ts` 2, 6.3 and 3.9; `readiness.spec.ts` 2, 5.9 and 5.2; `quit.spec.ts` 2,
3.5 and 2.4; `shell.spec.ts` 3, 3.3 and 1.7; `connections.spec.ts` 1, 2.5 and 2.2;
`records.spec.ts` 1, 1.9 and 0.9; `security.spec.ts` 4, 1.5 and 1.1; `home.spec.ts` 1, 1.0
and 0.3. **The macOS shard 1 came to 88.9 s of its 90** on #282's last run (75.9 s here):
the owner's decision on the tier's limit is pending (the carry-over list).

## Spend

About 0.22 US dollars of the 100 allowed, against the plan's forecast of about 3 ("Spend"):
E1's live runs, 0.176 settled and 0.04 unresolved (the bound of the fill a quit cut off),
booked on the E loop's store and drawn from milestone E's allowance for the loop, all to
approved recipients, each run's figures in `docs/results/m5-playground.md`. Nothing else of
milestone 5 was paid: every other task ran on fakes, in every test and every Chrome case, and
SAM 3 runs on the Mac. The owner's own tries of the playground, which the plan allowed 1.50
for, are not counted here.

## What the owner decided

- **Milestone E's first slice ran beside this milestone,** as decided on 2026-11-20
  (`docs/plans/m4-note.md`).
- **Open questions 1 to 3 were built as their recommended answers:** Node pinned as a tool
  `just check` needs, npm's registry for the UI's packages, Preact and marked in the binary,
  `signal-hook` for the terminal's signals (U1, H1); the allowance shown "since the last
  reset" with `spend reset` from the page and the call cap read-only (M1); the owner's brand
  art committed under `ui/`, the wordmark outlined at weight 900 as the owner asked (W3).
- **Open question 4, the archiving,** waits for the owner's yes, after this merge.
- **Pending:** the Chrome tier's limit for one shard, now that the macOS shard 1 came to
  88.9 s of 90; and the job names `.claude/skills/kickoff` and `review-merge` still give.

## What a reader should not assume

- **The playground runs on one machine, for one person.** It listens on loopback only, takes
  no file path from the page (pictures are uploaded, results downloaded), and shows Chrome
  only. It is not the engine API of milestone 6 and shares no route, token or listener with
  it.
- **The Chrome cases run on fake models.** They show the interface and the host, not a model:
  the fixture host's segmenter, fill and judge are scripted. The real models ran in the
  playground only in E1's five runs, by hand.
- **E1's runs score nothing.** They show that the playground runs the loop, says how each run
  ended and why, refuses what it cannot run and accounts for a quit. Whether Argon was right
  about `fail` and `pass`, and whether a retry makes a picture better, are milestone E's.
- **Home's library is the playground's.** It lists only pictures given through the
  playground, so a store filled from the command line shows "No pictures yet" there; its
  runs are in History.
- **The allowance is one figure since the last reset,** not monthly, and the call cap is read
  from the config, which the playground never writes. To add a key, the playground is
  started again with the same config and env file.
- **Some tests time out under load on a busy Mac.** Two fast tests timed out at their 30
  seconds on several local full checks on 2026-11-22 and passed alone; never in CI. And one
  Chrome case failed once on `main` (the carry-over list). None was retried into a pass.
- **Pull-request CI takes more than its 10 minutes on macOS,** by its `scripts` job (Timing).
- **The old repos are not archived yet** (above).

## What moved to the carry-over list

Closed by the tasks of milestone 5, each with its line: a store refused as held by the
process that had just let it go, a run cancelled only in the process that made it, the
store's two openings to lock, the deletion's sweep with a reader of another process (H1);
typed events, in whizbang's shape, and an admission that was visible and not confirmed (K1);
a progress event's message unescaped, for the playground, and a missing model that could not
be picked (W1); a stored exchange that nothing read, for the report, and the reference page
that printed name and purpose only (W3). Dropped with a line: whether an artifact type has a
preview, `canary.rs` and the localisation of `metrics.rs` (W3). Measured and re-owned to 6
with the number: every publication writes the whole generation (H1). Narrowed: the store's
locks (a replaced `LOCK` is noticed), and what running an action does not do yet (the
journal's bound, a run's stated time) (H1, K1). Closed by E1: the by-hand check of the loop
moved here.

**By this pull request:**

- Closed: `ci` has no Chrome end-to-end step (U1, and #282's shards); the cancel test of
  `play_models` that failed at random (fixed by #279).
- Re-owned to 6: a real prober (M1, item 3's line); a squint job file's inline picture and
  `expect` (no job file reaches the playground); where the models directory is, read once;
  what running an action does not do yet, the part left; the store's locks on a file system
  where they do not hold, when such a store is asked for or with `whizbang serve --http`.
- Re-owned to 7: sending a run again from the store (replay).
- Re-owned to the first task that wants it: the loop's tests' file system in memory.
- Re-owned to the first live run on or after each date, E's or 6's: the Google rows of
  prices. E1's runs asked `gemini-4-argon` on 2026-11-22, before 2026-11-30, so neither row
  could be read.
- Opened, each with its owner: `choice_json`'s `Err(_)` fallback, and Home's library listing
  only the playground's pictures (the next change to the playground, at the latest milestone
  7); the 30-second timeouts under load of the third of four parts of the sweep of a first
  install's crashes and of the test of many runs from two executors at one instant (split
  since by the entry check of milestone 6:
  `models::crashes::a_crash_at_any_event_of_a_first_install_leaves_it_not_installed_or_whole_part_01_of_96`
  to `…part_96_of_96`, and
  `contract::execute::many_runs_from_two_executors_at_one_instant_are_all_kept_part_1_of_5`
  to `…part_5_of_5`), and the
  one failure on `main` of `compare.spec.ts: two pictures wider than half the canvas both fit
  side by side, at one zoom` (the entry check of milestone 6, or the first pull request whose
  CI meets one); the macOS shard 1 near its 90 seconds, and `scripts (macos-15)` over the 10
  minutes (the owner's decisions); `.claude/skills/kickoff` and `review-merge` naming the old
  `e2e` jobs (the owner).
- Left with milestone E, as `docs/plans/e1-note.md` opened them: the gate on fresh data for
  the default judge and for submission v2; the evidence test that does not hold the judge
  round's hand-copied tables; the evaluation-pictures repository's old name; the loop's
  finished pictures, owed to the owner's marks of the contact sheet.

No row is owned by milestone 5.

## What milestone 6, 7, E and later owe

- **Milestone 6:** the engine API, `whizbang serve --http`, with its negative tests; SAM 3 on
  a GPU host within the 1D tolerance; a real prober for the first backend that needs one; the
  rows re-owned above, of a store and an executor held for longer than the playground holds
  them.
- **Milestone 7:** comparisons between saved runs, saved examples, invocation bundles and
  guided chain replay in the playground (section 7's deferred row), and replay from the
  store.
- **Milestone E:** the loop's finished pictures judged from the owner's marks; the gate on
  fresh data for the default judge and the format; the blinded picking rounds of milestone 3;
  `tiles` and the second look; details first gated again on fresh data.
- **No milestone yet:** notarisation of the Mac's binary, the owner's.

`main` at the end of milestone 5: a fourth door, the playground, every kept row of section 7
held by its Chrome cases on both runners, its guard by seventeen negative tests, and the loop
run in it by hand.

The tag `m5` was made at 9a2d397f, and rascal, squint, wafflery and table-manners were archived
on 2026-11-22, on the owner's yes: the row's last clause, "old repos archived", is met, and so
is success criterion 9.
