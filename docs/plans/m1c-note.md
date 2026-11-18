# Checkpoint 1C note

Checkpoint 1C, "the local pack", is complete: its last build tasks (R2a and R2b, pull
request #170) were merged on 2026-11-17 (dates here are the owner's local dates). Its
plan is `m1c-local-pack.md`. This note was written by the exit check (task Z1) from
`main` at e5da9eb, the merge of #170. The tag `m1c` is on the merge of the pull request
that adds this note.

## What whizbang can do at this tag

Beyond `m1b`: `whizbang models list|get|remove|pin` over a models directory, with SAM 3
the one entry of the shipped catalogue; `whizbang setup [--extra torch]`, which installs
the pack from its committed lock; `whizbang backends status [--probe]`; and a config may
name a backend of the kind `pack`, a local segmenter run by one resident Python worker
over `pack/1`. A run's record is `run/4`, which names the device that answered and
whether a cache did, and `runs show` prints an installation's identity whole.

No command sends a phrase until 1D's `segment` action, so the clause on the Mac's GPU is
shown through the binary's wiring plus `runs show`, not by a command a person types.

## What shipped

Thirty-three pull requests, #139 to #171. Each strand's records are its ADRs: 0020 (the
protocol, the worker), 0021 (installing the pack), 0022 (the first runtime), 0024 (the
download client), 0025 (`run/4`), and amendments to 0006, 0011, 0012, 0015 and 0017.

| Strand | Pull requests | What |
|---|---|---|
| Entry and roots (C0, C0b, C1a, C1b) | #139 to #142, #149 | The plan; the entry check; the macOS script tests in a job of their own; the roots; Python in the fast tier |
| Protocol and worker (P1 to P5b) | #144, #145, #153, #155, #159, #161, #163, #164, #167 | `pack/1` with its vectors and both codecs; the Python worker with fake models; the supervisor, its guard, its queue, cancellation, deadline, restart and removal |
| Models and network (N1 to N5) | #143, #146, #147, #150 to #152, #154, #156 | The download client behind the checked downloader; the models directory its owner's; `catalogue/2`; the three commands; a download's credential; SAM 3 in the catalogue |
| Install and runtime (Y1, R1) | #158, #160, #162 | `whizbang setup` and `pack/uv.lock`; PyTorch through `transformers` chosen (ADR 0022); the lock for the Mac alone |
| Through the binary (P6a to R2b) | #165, #166, #169, #170 | The kind `pack`; verify at load; `run/4` and `backends status`; the adapter `sam3` and the model tier |
| Tooling | #148, #157, #168 | One `serde_json`; the macOS limit of 300 s; the faster local check |
| Exit (Z1) | #171 | This note, the carry-over list, the plan's last box |

## The clauses of the spec's row, each with its evidence

The row for 1C in `SPEC.md` section 16. Tests are named by module and function;
`scripts/tests/test-notes.sh` fails when one named here no longer exists.

| Clause | Shown by |
|---|---|
| The protocol is decided | ADR 0020, `pack/1`; the vectors in `pack/conformance/vectors/`, read by both codecs: `proto::every_vector_parses_to_what_it_states_and_is_written_back_byte_for_byte`, `proto::a_hello_of_another_version_is_refused_and_says_both_versions`, `proto::a_frame_cut_at_any_byte_is_an_error_and_never_a_shorter_message`, and `pack/tests/test_proto.py` |
| Its conformance suite passes with fake models | `conformance::the_real_pack_with_a_fake_model_answers_a_segment_request_whose_masks_are_those_the_request_determines`, `conformance::every_case_of_the_table_names_the_fake_model_or_the_fake_fault_that_produces_it`, `conformance::a_malformed_payload_ends_the_call_as_malformed_and_the_worker_is_restarted`, and `pack/tests/test_worker.py` |
| The worker's queue rules pass | `queue::requests_run_one_at_a_time_in_the_order_they_came`, `queue::the_seventeenth_waiting_request_is_refused_as_unavailable_and_not_held`, `queue::two_requests_for_one_model_cause_one_load` |
| Its cancellation rules pass | `queue::a_cancelled_call_that_stops_ends_as_cancelled_and_the_worker_lives_on`, `queue::a_queued_request_that_is_cancelled_leaves_the_queue_and_is_never_sent`, `queue::a_cancelled_call_that_has_not_stopped_at_its_deadline_has_its_worker_restarted`, `queue::a_call_past_its_deadline_ends_as_timed_out_and_its_worker_is_restarted` |
| Its restart rules pass | `queue::a_worker_that_exits_inside_a_call_fails_what_was_in_flight_and_the_next_request_starts_another`, `queue::a_restart_ends_the_current_call_and_every_queued_request_each_with_a_fault_that_says_the_worker_was_restarted`, `queue::nothing_that_was_dispatched_is_sent_again_after_a_restart`, `queue::a_worker_that_ends_before_it_answers_anything_is_not_started_again_in_a_loop` |
| Its removal rules pass | `removal::a_removal_refuses_new_requests_cancels_the_queued_ones_and_waits_for_the_current_call`, `removal::a_removal_from_a_second_process_cancels_the_queued_requests_within_the_interval_while_another_call_holds_the_worker`, `removal::a_removal_whose_wait_times_out_answers_busy_and_the_model_takes_requests_again`, `removal::after_a_remover_was_killed_with_its_mark_set_a_request_for_the_model_is_answered_by_the_worker`, `remove::a_crash_at_any_event_of_a_removal_leaves_the_model_installed_or_not_installed` |
| One runtime is chosen (section 17) | ADR 0022 (PyTorch through `transformers`, candidate A, run by hand on the Mac by task R1), and `SPEC.md` section 17's rows on the runtime |
| SAM 3 answers a phrase request on the Mac's GPU | `model_tier::sam3_answers_a_phrase_on_the_gpu_with_an_instance_inside_the_fixtures_box_and_the_record_names_the_device` and `model_tier::a_second_request_for_the_same_model_loads_nothing`, under `just test-models` on the owner's Mac, through the binary's wiring with the installed model, the record read back by `runs show --json`; the evidence record `docs/results/m1c-local-pack.md` (three runs, `mps` in each, fallback off), held to the tree by `evidence::the_committed_record_names_the_adapters_revision_and_the_lock_the_tree_has` |

## What the entry check changed

C0 measured the macOS job (the script tests were its largest part) and chose to run them
in a job of their own, `scripts (macos-15)`; C0b built that. No test moved tier and no
limit changed with it.

## What the owner decided

| Date | Decision |
|---|---|
| 2026-11-15 | The origins approved: a pinned Python from GitHub's releases, `pypi.org` and `files.pythonhosted.org` for the runtime, Hugging Face for the SAM 3 files with the owner's token |
| 2026-11-15 | The token is read from the Hub's command-line sign-in already on the Mac, "for now" (D12); access to `facebook/sam3` already granted |
| 2026-11-16 | The GitHub release hosts as C1b named them approved; Hugging Face's own storage host pre-approved before it was known ("no need to approve"), named by N5 as `us.aws.cdn.hf.co` |
| 2026-11-17 | Linux on x86-64 dropped from the pack's lock ("drop it"; ADR 0021, "Amended: the Mac alone") |
| 2026-11-17 | The test-time limit on GitHub's hosted macOS runner is 300 s; Linux and the Mac keep 180 s, one test 30 s |
| 2026-11-17 | A faster local check: `just check` runs only the script tests a change can reach; CI runs them all (#168) |

## Timing

Fast tests: 2,233 run, 17 skipped (the model tier among them). On GitHub, `main` at
e5da9eb; on the owner's Mac, one `WHIZBANG_HEAVY_POLL=0.2 just check` on this pull
request's tree, other builds at work on the machine. Suite seconds are summed over tests
that run side by side, so they add up to more than the test time.

| Suite | The Mac | `check (macos-15)` | `check (ubuntu-22.04)` |
|---|---|---|---|
| Fast tests, test time (compilation not counted) | 87.2 s of 180 s | 207.8 s of 300 s | 108.8 s of 180 s |
| `whizbang-core::it` (1,308 tests) | 196.1 s | 289.8 s | 196.5 s |
| `whizbang-backends::it` (342; the protocol, worker, queue, removal, conformance) | 238.1 s | 200.2 s | 120.9 s |
| `whizbang::it` (185; the kind `pack` through the wiring) | 238.0 s | 42.1 s | 25.0 s |
| The pack's Python tests (`just test-pack`, 59) | 1.8 s | 3.9 s | 2.7 s |
| The model tier (`just test-models`, 2) | the load 6.1 to 7.7 s, a first request 0.9 to 1.0 s, a second 0.66 to 0.69 s, about 27 s before the load (`docs/results/m1c-local-pack.md`) | not run | not run |
| The whole job | | 9 min 8 s; `scripts (macos-15)` 6 min 36 s | 12 min 37 s (11 min 51 s on the run before, 11c251d) |

Both macOS jobs are inside the 10 minutes of `SPEC.md` section 13 on `main`; the Linux
job is not (carry-over, for 1D's entry check).

## Spend, and what was downloaded

0 US dollars: every model call is local and no paid call was made. Downloaded, on the
owner's Mac only: the SAM 3 files of the pinned revision of `facebook/sam3` (about 3.4
GB) from `huggingface.co` and `us.aws.cdn.hf.co`; the runtime's packages from `pypi.org`
and `files.pythonhosted.org`. On both CI runners: `uv` and the pinned Python from
GitHub's releases. Nothing else.

## What a reader should not assume

- **No command a person types runs SAM 3.** The model tier's request is made by a test
  action through the binary's wiring; `segment` is 1D's.
- **The GPU clause is the owner's Mac's alone.** The model tier is `#[ignore]`d and CI
  never runs it; the pack's lock covers the Mac only, and Linux has no runtime.
- **The evidence record was taken on a tree with uncommitted changes**; 1D takes it
  again on a clean head (carry-over).
- **The first load of each command is slow**: about 26 s of it hashes 3.4 GB (D14);
  1D caches the check (carry-over).
- **One model is resident and one call runs at a time**; nothing runs side by side.
- **Conformance is shown with fake models**, not with SAM 3.

## What moved to the carry-over list

No row is owned by 1C now. Re-owned by this check: the speed of local models (1D, for
point, box and Tiny); the child's copy of the store's lock, done (the row stays for
milestone 5). Added: the D14 hash on every first load (1D, a cached check); the evidence
record re-taken on a clean head (1D); the Linux CI job over its 10 minutes (1D's entry
check). The macOS row is closed: both macOS jobs are under 10 minutes on `main`.

## What 1D starts from

`main` at the tag `m1c`: `pack/1` and its conformance suite; a worker with its queue;
the kind `pack`; SAM 3 installed by whizbang and answering phrases on the Mac's GPU
through PyTorch and `transformers`, with `run/4` naming the device; the evidence record
and its script. 1D adds point and box, the `segment` action, Tiny as a second entry,
parity fixtures, the runtime on Linux, the `add-adapter` skill, and the three rows above.
