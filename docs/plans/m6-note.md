# Milestone 6 note

Milestone 6, "HTTP and remote hosting", ends with task N1 of
`m6-http-and-remote-hosting.md`. This note was written by N1's pull request from its branch,
with `main` at deb8267b, the merge of #299, the last fix that L1's live round found. The same
pull request carries L1's two records, `docs/results/m6-remote-sam3.md` and
`docs/results/m6-user-story-1.md`. With it every box of the plan is ticked. The tag `m6` is
the orchestrator's, made on this pull request's merge.

## What whizbang can do at the end of milestone 6

Beyond milestone 5: a fifth door, `whizbang serve --http`, the engine API
(`docs/guides/engine-api.md`). It is the MCP server of ADR 0031 spoken over HTTP (stateless,
revision 2025-06-18), with plain routes beside it: a picture uploaded, a result downloaded,
and a run read or cancelled by its id. A hosted app calls it with one bearer token, on
loopback by default. It takes no path from a caller and offers no local control. It holds one
store and one executor for months: the generation is kept in buckets, abandoned runs are held
to a bound, a panic in the executor's code is caught, and uploads that no run took are let go
of every hour.

Beside it is a backend kind, `remote`. Through it, SAM 3 runs on a GPU host of your own
(`docs/guides/gpu-host.md`): the pack's front speaks `pack/1` over HTTP and supervises the
worker beside the card, and the adapter picks CUDA where there is one. The repository holds a
deployment for Modal, `scripts/gpu-host.sh`, with its own Linux CUDA lock. A request is
charged at a bound that the host's own limits give, and `whizbang backends status --probe`
asks the host which model it holds, without waking a card.

User story 1 runs live: an app with no GPU and no Python uploads a picture, names or clicks
a thing, and downloads the result. SAM 3 finds the thing on an A10G on Modal, and Bria
through fal fills it.

## What shipped

| Pull requests | Task | What |
|---|---|---|
| #284 | CI | CI's limits as the owner decided on 2026-11-22: the Chrome tier held to 240 seconds, 120 a shard; the script tests in two shards on each runner |
| #285, #287 | Plan | The plan of milestone 6, after twenty-five rounds of outside review; decision 5 changed: no hosted copy of SAM 3's files for now |
| #286 | A0 | The entry check: the two fast tests that timed out under load split, the Chrome case that failed once made to wait for its layout |
| #288, #291 | P1 | The pack's front and its supervision, `pack-host/1`, CUDA in the adapter, ADR 0046; a trickled body's first byte sent with its head |
| #290, #294 | S1 | A store and an executor held for months, ADR 0047: the generation in buckets, then a panic in the executor's code and abandoned runs held to a bound |
| #289, #296 | H1 | The engine API, `whizbang serve --http`, its guard and its negative tests, ADR 0045; the third upload sent only once the two hold their places |
| #292 | W1 | Tiny's licence text from the binary, and the catalogue's revisions |
| #293 | R1 | The kind `remote`, the real prober, the conformance suite over HTTP |
| #295 | D1 | The GPU host on Modal, the hourly collection of uploads, user story 1 on fakes, the guides |
| #297, #298, #299 | L1's fixes | What the live round found, each fixed in its own pull request (below) |
| This one | L1, N1 | L1's two records, this note, the boxes, the carry-over rows, the spec's wording owed |

## The clauses of the spec's row, each with its evidence

The row for milestone 6 in `SPEC.md` section 16: "User story 1 runs: the engine API with its
negative tests, SAM 3 on a GPU host with parity inside the 1D tolerance, Bria through fal".
Tests are named by module and function, and `scripts/tests/test-notes.sh` fails when one
named here no longer exists. The pack's tests are in `pack/tests/` and run under
`just test-pack`. The live runs are the orchestrator's, on the owner's Mac, and never run in
CI.

| Clause | Shown by |
|---|---|
| The engine API | Each revision's exchange: `serve_http::the_offered_revision_initializes_lists_and_calls_to_its_end`, `serve_http::an_initialize_asking_another_revision_is_answered_with_2025_06_18`. A call streamed: `serve_http::a_call_with_a_progress_token_streams_its_progress_and_its_result_and_names_its_run`, `serve_http::a_call_without_a_progress_token_streams_no_progress_and_still_names_its_run`. A repeat by request id after a dropped stream: `serve_http::a_call_repeated_by_request_id_after_its_stream_dropped_follows_the_same_run_and_spends_once`, `serve_http::a_repeat_after_a_restart_after_the_table_let_it_go_and_after_its_run_was_pruned_dispatches_nothing_more`, `serve_http::the_same_request_id_with_other_arguments_is_a_conflict_after_a_restart_and_after_a_prune`. A cancel by run id: `serve_http::a_run_whose_stream_dropped_is_read_and_cancelled_by_its_id_and_commits_nothing`. Uploads and downloads: `serve_http::an_upload_is_named_by_a_call_and_its_result_is_downloaded`, `serve_http::an_upload_no_run_took_is_collected_after_its_age_and_one_a_run_took_stays`. A signal's stop: `serve_http::a_signal_cancels_every_run_accounts_the_ledger_and_releases_the_store`. Its start: `serve_http::an_address_off_loopback_is_refused_without_off_loopback_and_a_host_name`, `serve_http::with_neither_token_flag_or_with_both_the_start_is_refused`, `serve_http::a_token_file_others_can_read_or_a_short_token_is_refused_at_start`, `serve_http::a_store_on_a_mount_of_a_network_type_is_refused_before_anything_listens` |
| With its negative tests | The five of section 10. Unauthenticated: `engine_guard::a_call_without_the_token_is_refused_401_and_names_nothing`, `engine_guard::a_call_with_a_wrong_token_is_refused_401`. A wrong Host: `engine_guard::a_request_whose_host_is_not_one_of_the_servers_names_is_refused_421`. Cross-origin: `engine_guard::a_request_from_an_origin_not_listed_is_refused_403_with_no_allow_origin_header`. An oversized image: `engine_guard::an_upload_over_64_mib_is_refused_413_before_it_is_read_whole`, `engine_guard::an_upload_over_the_pixel_limit_is_refused_413_naming_the_limit`. A path: `engine_guard::a_path_in_any_argument_of_any_tool_is_refused_400`. And the rest of "Who may call": `engine_guard::no_local_control_is_listed_callable_or_routed`, `engine_guard::the_token_is_in_no_answer_log_event_record_or_error`, `engine_guard::a_json_body_over_1_mib_or_deeper_than_the_limit_is_refused`, `engine_guard::a_fifth_new_run_is_answered_429_and_a_repeat_run_routes_and_downloads_are_still_answered`, `engine_guard::a_third_upload_is_answered_429_after_its_headers`, `engine_guard::stalled_streams_and_uploads_and_trickling_clients_leave_the_run_routes_answered`, `engine_guard::the_whole_engine_saturated_holds_no_more_than_its_32_threads`, `engine_guard::a_request_with_no_protocol_version_another_one_or_a_batch_is_refused_400`, `engine_guard::a_cancel_of_a_run_this_server_did_not_start_is_answered_409` |
| SAM 3 on a GPU host | The front's supervision, in `pack/tests/test_front.py`. The conformance suite over HTTP against the fake host: `conformance::the_cases_of_the_fake_models_pass_over_http`, `conformance::the_cases_of_the_workers_faults_pass_over_http`, `conformance::the_cases_of_cancels_and_limits_pass_over_http`. The kind: `remote::a_call_is_sent_through_the_gate_with_its_token_bound_to_its_origin`, `remote::a_remote_backend_without_both_price_settings_has_no_bound_and_is_refused`, `remote::the_examples_bound_is_360000_microusd_and_an_unknown_end_settles_at_it`, `remote::the_run_records_the_identity_asked_the_one_reported_and_the_device`, `remote::status_with_probe_reads_the_hosts_document_and_names_the_model_it_holds`, `remote::a_timeout_and_a_cancel_send_the_hosts_cancel_through_the_checked_sender_and_name_the_dispatch`. The wire: `remote::wire::a_redirect_from_the_host_is_refused_and_nothing_follows_it`, `remote::wire::an_answer_with_no_malformed_or_another_identity_is_refused_and_not_kept`, `remote::wire::a_stream_cut_before_its_result_is_truncated_and_charged_at_its_bound`, `remote::wire::a_cold_host_keeps_its_answer_alive_past_the_clients_limit_between_two_reads`. Live, every answer of L1 named `cuda`, and a cancel reached the card's worker (`docs/results/m6-remote-sam3.md`) |
| With parity inside the 1D tolerance | `remote_parity::sam3_answers_each_fixture_inside_the_tolerance_from_a_remote_worker`, run by `scripts/test-models.sh parity-remote` in two rounds of three, before and after #298 changed the wire. Every fixture's least IoU was 1.000000 and its widest score gap at most 0.000002, against 0.99 and 0.01, every answer on `cuda`. Recorded in `docs/results/m6-remote-sam3.md` and held by `evidence::the_records_of_milestone_6_hold_only_aggregates_and_run_ids` |
| Bria through fal | User story 1's fill through the engine API: fal's fake in the fast tier, in `story::a_hosted_editor_uploads_names_and_clicks_and_downloads_with_no_local_worker`; live, six fills, each recorded as sent to fal (`docs/results/m6-user-story-1.md`) |
| User story 1 runs | `story::a_hosted_editor_uploads_names_and_clicks_and_downloads_with_no_local_worker` in the fast tier. Live: three pictures, each named and each clicked, nine runs, all completed, with no pack worker started (`docs/results/m6-user-story-1.md`, held by `evidence::the_records_of_milestone_6_hold_only_aggregates_and_run_ids`) |
| Section 18, item 3 | The conformance suite's behavioural half over HTTP (above), and L1's parity |

Beside the row: section 7's "MCP over HTTP" is the engine API. Section 5's cell "segment,
self-hosted" is the kind `remote` with SAM 3 on the GPU host. Success criterion 8, "User
story 1 runs end to end", is the live run above. And a store held for months:
`store::buckets::a_store_of_the_old_format_reads_and_its_first_publication_writes_the_new`,
`store::buckets::the_generation_file_is_the_same_size_at_100_and_at_2000_paid_runs`,
`store::buckets::a_publication_writes_only_the_buckets_it_changed`,
`execute::a_server_whose_runs_never_return_refuses_new_starts_at_its_bound_and_grows_no_further`,
`execute::a_panic_in_the_executors_code_ends_the_run_as_panicked_after_its_children`.

## The live round, and what it found

L1 ran on 2026-11-24, on an NVIDIA A10G on Modal (driver 620.70.06, CUDA 13.5; PyTorch 2.15.0
with the CUDA runtime 13.2). The host was deployed three times from clean checkouts of
`main`, as each fix landed.

- **Cold calls** (item 2b): before #298, a whole call took 22.5 to 52.2 seconds cold and 2.6
  warm; the first call after a new image took 161.0. One GPU cold start took about five
  minutes, and two calls failed on Modal's redirect at 150.8 seconds.
- **Parity** (item 3): inside the tolerance in both rounds, as above.
- **User story 1** (item 4): nine runs completed, the first 417.2 seconds through a cold card.
- **The cancel** (item 5): one cancelled 15 seconds in, during a cold start. Its cancel frame
  reached worker 6, which was killed at the cancel's deadline of 5 seconds, and the call
  ended 5.117 seconds after the cancel.
- **A stopped host** (item 6): the call ended as an error naming the host's connection, and
  nothing fell back to another backend.

Found live and fixed, each in its own pull request with a test that fails without it:

- **#297, four defects of the first deploy.** The front crashed in the container, reading a
  checkout that is not there. The endpoint's pattern missed Modal's region label. The deploy
  left bytecode that the next deploy refused. And the image builder was not pinned, so the
  workspace's default, which client 1.6.1 refuses, was used.
- **#298, the headers at once.** A Modal web endpoint redirects a request whose answer has
  not begun within 150 seconds, and the kind `remote` refuses a redirect. The front now sends
  its headers as soon as it has read the request, then a keep-alive after every 10 seconds of
  nothing else. The identity of the files moves into the body, ahead of the first frame. The
  front answers `timed_out` itself once a request is past its `not_after` by 5 seconds, and
  the client ends a whole answer at `host_limit_s` plus 30 seconds, so keep-alives cannot
  stretch a request. ADR 0046, "The headers at once".
- **#299, the cancel that overtakes its request.** Through the host's proxy, a cancel's few
  bytes reached the front before its request's body had been read whole. The bridge found no
  open call and dropped the cancel silently, and the request then ran. Such a cancel is now
  kept, the last 64, and a request of that dispatch that comes after it ends `cancelled`
  before the card is asked. Every cancel is a line of the host's log.

## Timing

From the last CI run of `main` before this note (run 45471815829, at deb8267b, the merge of
#299). Wall time of each job, compilation included; the budget of `SPEC.md` section 13 is
under 10 minutes for pull-request CI.

| Job | `macos-15` | `ubuntu-22.04` | Test time |
|---|---|---|---|
| `check` (every fast test but the sweeps, and the UI's unit tests) | 6 min 39 s | 5 min 31 s | 146.3 s and 106.9 s, limit 300 |
| `sweeps` | 6 min 3 s | 3 min 28 s | 154.7 s and 62.8 s, limit 300 |
| `scripts`, shard 1 | 7 min 0 s | 2 min 58 s | |
| `scripts`, shard 2 | 6 min 15 s | 4 min 14 s | |
| `release` | 5 min 18 s | 4 min 57 s | |
| `e2e`, shard 1 | 3 min 17 s | 2 min 14 s | 78.1 s and 56.4 s, limit 120 |
| `e2e`, shard 2 | 3 min 10 s | 2 min 25 s | 75.9 s and 58.0 s, limit 120 |
| `secrets` | | 2 min 41 s | |
| The run, start to `ci` | 12 min 33 s | | |

The run took 12 min 33 s, though no job took more than 7 minutes: `scripts (macos-15, shard
1)` waited 5 min 23 s for a runner before it began. The pack's tests took 9.75 seconds on
`check (macos-15)` and 6.63 on Linux, of their 12 (the carry-over list).

**By suite**, the fast tests, each suite's tests' own seconds added up (they run side by
side, so the sum is more than the test time):

| Suite | Tests | `check`, macOS | `check`, Linux | Sweeps | Sweeps, macOS | Sweeps, Linux |
|---|---|---|---|---|---|---|
| `whizbang::it` | 697 | 205.4 | 189.3 | | | |
| `whizbang-core::it` | 1,425 | 69.5 | 67.1 | 29 | 183.7 | 86.2 |
| `whizbang-backends::it` | 356 | 62.4 | 46.0 | 308 | 116.7 | 34.7 |
| `whizbang-squint::it` | 175 | 29.9 | 46.0 | 2 | 10.7 | 8.1 |
| `whizbang-rascal::it` | 150 | 21.1 | 15.4 | 1 | 6.1 | 3.1 |
| `whizbang-testkit::it` | 151 | 13.5 | 19.3 | 1 | 0.1 | 0.0 |
| `whizbang-serve::it` | 48 | 6.4 | 5.0 | | | |
| `whizbang::bin/whizbang` | 27 | 5.3 | 2.8 | | | |
| `whizbang-backends` | 63 | 5.3 | 6.9 | | | |
| `whizbang-eval::it` | 67 | 4.4 | 6.4 | | | |
| The other four (`whizbang`, `whizbang-core`, `whizbang-serve`, `whizbang-testkit`) | 63 | 1.4 | 1.1 | | | |
| The UI's unit tests, as the clock saw them | | 4.0 | 4.3 | | | |

**On the owner's Mac** a full `just check` runs every fast test at once, and came to about
246 of its 300 seconds at the end of the milestone (the carry-over list).

## Spend

Under 2 US dollars in real money, of the 100 allowed, against the plan's estimate of about
8.50 and its stop at 40 ("Spend"). All of it went to approved recipients, from the owner's
Mac, in L1:

- **Modal:** 0.56 dollars metered for the whole day, all of it covered by the workspace's
  credits, so 0.00 billed.
- **fal:** about 20 fills over every attempt, at 0.04 each, about 0.80.

Whizbang's ledgers counted about 13 dollars over the round, every segment request at its
bound of 0.42: the bound holds far more than Modal metered. User story 1's store counted 3.60
dollars, of which 0.42 is unresolved, a cancel's (`docs/results/m6-user-story-1.md`, "The
ledger"). Every other task ran on fakes, in every test. The owner's own tries of the engine
API are not counted here.

## What the owner decided

- **Modal is the GPU host** (2026-11-22). On 2026-11-23: the Modal account and its tokens are
  the owner's; PyTorch's Linux CUDA build is in the host's image and there alone; SAM 3's
  files were copied from the Mac into a Modal volume; and whizbang hosts no copy of SAM 3's
  files for customers, for now (decision 5, which replaced a yes given earlier that day).
- **CI's limits** (2026-11-22), as #284 made them: the Chrome tier held to 240 seconds, 120 a
  shard, and the script tests in two shards on each runner. No limit was changed by this
  milestone.
- **The order after milestone 6** (2026-11-24): 7, then E, then 8. Milestone E no longer runs
  beside the numbered milestones. It runs after 7 and before 8, so that its evaluations
  decide what 8 builds: a further segmenter, and how local fill and judge are scored.
  `SPEC.md` section 16 says so.

## What a reader should not assume

- **The tolerance was met on one card.** Parity held on an A10G, on one host, on one day.
  Another card, driver or CUDA is not shown to meet it.
- **One GPU host.** The deployment is Modal's, and the kind `remote` is held to the pack's
  front, not to Modal. No other host has been run. Whizbang's own Modal deployment is for
  testing; a customer hosts their own.
- **The app's server is behind a proxy it cannot see.** Off loopback, `--off-loopback` is the
  operator's statement that TLS is done at a reverse proxy in front. The engine API has no
  TLS of its own and cannot check that such a proxy is there.
- **The ledger is not the bill.** Each segment request is charged at its bound, 0.42 dollars
  on L1's host, since a timeout does not prove the host stopped billing. That day's metered
  cost was a small part of it. A request whose connection never opened is charged at the
  bound too (the carry-over list).
- **A clicked removal is unshaped.** `segment` then `fill` paints the mask as found, and left
  a faint red fringe live. A named removal grows and feathers its mask first. Shaping a
  clicked mask is milestone E's to decide.
- **A cancel during a warm call's compute was not caught live.** On a warm call the upload is
  most of the time. The recorded cancel came during a cold start and reached the worker,
  which was killed at its deadline.
- **Some tests are near their limits.** The fast tier on the owner's Mac is at about 246 of
  300 seconds, and the pack's tests on the macOS runner at about 10 of 12. One Chrome case
  timed out once (the carry-over list). None was retried into a pass.

## What moved to the carry-over list

Closed by the tasks of milestone 6, each with its line:

- A0: the tests that timed out or failed at random at the end of milestone 5.
- P1 and R1: the HTTP codec of the pack's protocol.
- R1: the real network client's last part, a real prober.
- S1: every publication writes the whole generation; what running an action does not do yet,
  the part left (a parent's wait, closed with a line, and a panic outside an action); a run
  whose end became visible and was not confirmed; the store's locks on a file system where
  they do not hold; a mask's three planes in memory; where the models directory is, read
  once; a picture or a drawn location that no run took; a fast test that came to its limit in
  a full check.
- H1: a squint job file's inline picture and `expect`.
- D1: the Linux runtime extra, narrowed to the host's image.
- #298: a cold call to the GPU host on Modal. Its row was taken out when the headers went
  out at once.

**By this pull request:**

- Closed: the Apache-2.0 licence text with Tiny's installation (W1, #292).
- Seen live, left with milestone E: the clicked removal filled as `segment` found its mask,
  now seen live as a red fringe.
- Re-owned to 7: the engine API's collected uploads, whose bytes stay until a prune. It was
  owned by milestone 6 and not built.
- Re-owned to 8: the two ways the front and the local supervisor tell a worker's fault
  apart. #298 changed neither, and 8 puts its self-hosted fill and judge behind the same
  front.
- Re-owned to the first live run on or after each date, 7's or E's: the Google rows of
  prices. L1 ran on 2026-11-24 and asked no Google judge.
- Opened, each with its owner:
  - From S1's review (#294), for 7: a run's journal and diagnostics are cloned at every run's
    end; a panic at `Publishing` is answered as abandoned though the end may be durable.
  - For 7: the Chrome case `run.spec.ts: each of the five squint use cases runs from its
    form`, which timed out once on `e2e (macos-15, shard 2)` in `toHaveURL`; and the fast
    tier's time near its limit on the owner's Mac.
  - For 8: the moment, in the front's bridge, between a dispatch opened and the host's call
    begun, in which a cancel finds no call and is not kept.
  - From L1, for 7: the card's name missing from the run record; a request whose connection
    never opened charged at its bound; the pack's test budget at 12 seconds with the macOS
    runner at about 10.2.
  - From L1, for the next release: the probe's "not probed: it is not ready" for a backend
    that has no probe.

Left open with no milestone: hosting the model's files for customers (decision 5). The tag
`m6-engine` that the plan named for the end of P1, S1, H1 and R1 was not made, and `m5` and
`m6` remain the checkpoints. No row is owned by milestone 6.

## What milestone 7, E, 8 and later owe

- **Milestone 7:** invocation bundles and journal replay; saved-run comparisons, saved
  examples and guided chain replay in the playground; replay from the store. Also the rows
  re-owned and opened above: the uploads' bytes, the run record's card, the charge of a
  request that never connected, the journal's copy and the answer at `Publishing`, the Chrome
  case, the fast tier's and the pack's time.
- **Milestone E**, after 7: the loop's finished pictures judged from the owner's marks; the
  gate on fresh data for the default judge and the format; the clicked removal's shaping;
  the evaluations that decide what 8 builds.
- **Milestone 8:** fill and judge, each local and self-hosted, and any further segmenter E
  calls for. Also the front's fault forms and its bridge's cancel window.
- **The next release:** the probe's wording.
- **No milestone yet:** notarisation of the Mac's binary, the owner's; hosting SAM 3's files
  for customers.

`main` at the end of milestone 6: a fifth door, the engine API, held by its negative tests.
SAM 3 runs on a GPU host within the 1D tolerance, and user story 1 runs end to end, live.
