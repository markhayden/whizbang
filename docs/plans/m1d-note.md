# Checkpoint 1D note

Checkpoint 1D, "segment coverage", is complete once its last pull request, V1, is
merged. Its plans are `m2a-first-slices.md` (closed) and `m2b-checkpoint-1d.md`. This note
was written by task V1 from its branch, with `main` at cd11448 (the merge of U1, #185)
merged in: Q1, F1, T1 and U1 are all in it.

## What whizbang can do at this checkpoint

Beyond `m1c`: `whizbang rascal segment` asks the configured segmenter a phrase, a point or
a box (`--phrase`, `--point`, `--box`) and commits one mask for each instance it finds,
with each mask's score, box and predicted IoU printed and kept in the run's diagnostic,
beside what the backend said; `whizbang rascal select` keeps the masks of a segment run
by its place or by a pixel. The shipped catalogue holds two models, SAM 3 and Tiny (OWLv2
and SAM 2.1 Hiera-Tiny), both answering phrase, point and box on the Mac's GPU through
PyTorch and `transformers`, or on its CPU by a backend's setting `device = "cpu"`. Parity
fixtures of both and a measured tolerance say when two answers agree.

## What shipped

| Pull request | Task | What |
|---|---|---|
| The first slices | `m2a-first-slices.md` | The `segment` action and the tools of plan 2A, closed before this plan |
| #182 | Q1 | Point, box and pick on SAM 3, from the command line |
| #181 | F1 | Fixes found in use: the env file, the judge's spend and printed reason, CI on Linux in two jobs |
| #183 | T1 | Tiny and the `add-adapter` skill |
| #185 | U1 | A user's weights and a user's adapter, through `whizbang models adapter add`: the adapter's SHA-256 bound to its installation; a user's catalogue refused if it names a shipped model; every origin it names checked against `[egress.recipients]` |
| This one | V1 | The device setting, the parity fixtures, the tolerance (ADR 0027), the timings, a backend's diagnostics to its run, this note |

## The clauses of the spec's row, each with its evidence

The row for 1D in `SPEC.md` section 16. Tests are named by module and function;
`scripts/tests/test-notes.sh` fails when one named here no longer exists. The model tier
(`model_tier::`, `parity::` and `coverage::` tests marked ignored) runs under `just
test-models` on the owner's Mac, never in CI.

| Clause | Shown by |
|---|---|
| SAM 3 answers phrase, point and box requests | `model_tier::sam3_answers_a_phrase_on_the_gpu_with_an_instance_inside_the_fixtures_box_and_the_record_names_the_device`, `model_tier::sam3_answers_a_point_and_a_box_on_the_mac`, `model_tier::a_second_request_for_the_same_model_loads_nothing`; the record `docs/results/m1c-local-pack.md`, retaken on a clean head by V1 and held to the tree by `evidence::the_committed_record_names_the_adapters_revision_and_the_lock_the_tree_has`; through the command line, `cli::rascal_segment_takes_exactly_one_of_phrase_point_and_box` and `cli::rascal_segment_with_a_box_prints_each_masks_identity_and_score_under_a_fake_segmenter`; in the adapter, `test_sam3.Prompts` |
| Tiny passes conformance for the features it declares | `test_tiny.Conformance.test_the_worker_answers_tinys_declared_features_within_the_contract`, `model_tier::tiny_answers_a_phrase_a_point_and_a_box_on_the_mac`, and the same record |
| Parity fixtures are recorded | `fixtures/parity/segment/`: SAM 3 and Tiny, three pictures each, a phrase, a point and a box; `parity::the_committed_fixtures_name_the_revisions_the_catalogue_holds`, and `parity::sam3_and_tiny_answer_each_fixture_inside_the_tolerance_on_mps_and_on_cpu` under `just test-models` |
| A mask-agreement tolerance is recorded | ADR 0027: an IoU of 0.99 for each pair in order, scores within 0.01, the same counts; measured on the Mac's GPU and CPU (`pack::a_device_the_kind_does_not_know_is_refused_when_the_config_is_read`, `test_sam3.Device.test_a_device_of_cpu_runs_on_the_cpu_and_says_so`); `parity::agreement_is_the_count_and_each_pairs_iou_and_score`, `parity::an_answer_one_instance_short_does_not_agree` |
| Timings are measured | `docs/results/m1d-segment-coverage.md`, by `coverage::the_first_load_and_a_second_request_of_each_query_on_sam3_and_tiny` under the memory watch, held by `evidence::the_coverage_record_names_a_commit_of_this_tree_and_every_timing` |

And the carry-over row on what an adapter gave, built by V1:
`segment::the_backends_diagnostics_and_each_predicted_iou_are_in_the_runs_diagnostic` and
`pack::the_diagnostics_a_worker_answered_reach_the_segmenters_caller`.

## What the owner decided

| Date | Decision |
|---|---|
| 2026-11-18 | Linux stays out of 1D: no CPU runtime on Linux, on its carry-over row |
| 2026-11-18 | Tiny's files from Hugging Face's storage host, under the approval of 2026-11-16 |
| 2026-11-18 | The budget of `check (ubuntu-22.04)` may be 15 minutes if splitting the job does not bring it under 10 |

## Timing

The timings of the models are in `docs/results/m1d-segment-coverage.md`: on the GPU, a
load of 2.0 to 3.3 s, a first request with the worker's start and the load in it of 2.3
to 4.7 s, and a second request of 0.6 to 0.7 s on SAM 3 and under 0.25 s on Tiny; SAM 3's two
models hold 6,363 MiB of the GPU, its phrase model alone 4,269. On the CPU (by hand, not
recorded) a request of SAM 3 took 5 to 10 s and its load about 9 s. The parity test runs
in about 95 s. The fast tests' times of this pull request are in its hand-back and its
CI run; the parity fixtures add three fast tests of about 0.6 s together.

## Spend, and what was downloaded

0 US dollars: every model call is local. Downloaded on the owner's Mac only: Tiny's files
(OWLv2 base and SAM 2.1 Hiera-Tiny, public, Apache-2.0) from `huggingface.co` and
`us.aws.cdn.hf.co` (T1). Nothing else beyond 1C's.

## What a reader should not assume

- **The tolerance is of one Mac's two devices.** The GPU and the CPU of the owner's Mac
  differed by at most a handful of pixels; another machine, and a remote worker, are
  held to the same 0.99 without having been measured.
- **Pairs are read in order.** Two instances whose scores lie within 0.01 may change
  places on another device and not agree, though both masks are there.
- **The CPU is a setting, not a fallback.** `auto` takes the GPU where the runtime has
  one; nothing moves a model to the CPU when the GPU fails, and the GPU's runtime falls
  back to nothing (`PYTORCH_ENABLE_MPS_FALLBACK` is never set).
- **The memory of the GPU is the driver's count for the worker,** said by `sam3` beside
  each answer; the resident memory the watch reports does not hold it.
- **The model tier is the owner's Mac's alone.** CI never runs a model; Linux has no
  runtime.
- **Diagnostics are text.** What a backend says beside its answer is kept as it said it,
  cut at 4,096 characters; nothing reads it as fields.

## Open follow-ups

From U1's review, for milestone 2:

- a named test that a user's catalogue using a shipped model's name is refused whole;
- the worker refusing a load with no record when the installation's folder holds
  `files/`;
- ADR 0017 or the guide saying plainly that a user's adapter runs with the worker's full
  privileges: its descriptors, its environment, its file system.

And open beside them:

- ship the Apache-2.0 licence text itself before weights are hosted for customers;
- a test that the worker's map of shipped models (`SHIPPED` in
  `pack/src/whizbang_pack/worker.py`) and the shipped catalogue cannot drift apart.

## What moved to the carry-over list

Closed or re-owned by V1: the row on what a segmenter adapter took and gave (done for the
settings, the diagnostics and the predicted IoU; the order re-owned by milestone 2); the
speed of local models (1D's half done; milestone 8 for the fill model). Added: one row,
owned by milestone 2, for the open follow-ups above.

## What milestone 2 and 3 start from

`main` at the end of 1D: segment with phrase, point and box on two local models through
the command line; masks committed with their scores, boxes, predicted IoUs and the
backend's diagnostics in each run; the device a setting; parity fixtures and a tolerance
for milestone 6 to hold a remote worker to. Milestone 2 starts the judge loop and the
doors that outlive one call (the carry-over rows owned by 2); milestone 3 the operations
that read more than one mask.
