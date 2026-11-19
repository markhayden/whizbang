# Plan: the rest of milestone 2

Draft 6, 2026-11-18, from `main` at the tag `m1d` (916b17c). What milestone 2 still owes
after the first slices (`docs/plans/m2a-first-slices.md`, closed; its table "What
milestone 2 still owes after S") and after checkpoint 1D (`docs/plans/m2b-checkpoint-1d.md`,
closed; `docs/plans/m1d-note.md`), up to `SPEC.md` section 16, row 2: "The baseline
reproduces; each of the five use cases returns a verdict on a representative case, from
the command line and over stdio MCP; Anthropic, Google and OpenAI each pass provider
conformance; the decision cache misses when the use case, brief, an image's role or
order, a rule or a tool changes, and hits for an unchanged job; a new tool file is tried,
scored, edited and re-run without a rebuild; `whizbang runs stats` and `runs export`
report on a tool across at least 100 recorded runs; per-subject comparison exists as an
experimental tool and has been scored on rungs 1 and 2". It ends with the note of
milestone 2. Eight pull requests, no more.

**What the owner asked for, and the order it sets.** "Squint [should] provide all the
details and the model to make the final conclusion but it shouldn't just be a bunch of
shit it has to figure out." So the work is ordered for the owner to run real checks
early: first the four remaining use cases from `whizbang squint check <use-case>` with
`--brief`, and the tools that hand the judge measured details before it looks (A1, A2);
then the Google and OpenAI judges (P1); then the evaluation sets, matrices, gates, `runs
stats` and `runs export` and the baseline gate (E1, E2), with the tool files that make
trying a tool cheap (X1); then the stdio MCP door and the decision cache (M1); then the
follow-ups and the note (F1).

## How the work is split

| Pull request | What | Builds on | Size |
|---|---|---|---|
| A1 | The four use cases and the job they judge, from the command line, with `--brief` and `--reference` | `main` | Large: about 2,500 changed lines |
| A2 | `resize`, `align` and the details handed to the judge first | `main` | Large: about 2,500 (its budget below) |
| P1 | The Google and OpenAI judge kinds, each with provider conformance | `main` | Large: about 2,500 |
| E1 | Evaluation sets moved, matrices, the runner, `runs stats` and `runs export` | A1, A2 | Large: about 2,400, and the sets' copy counted apart (below) |
| E2 | Metrics, paired statistics, gates, `eval run`, the baseline gate | E1, P1 | Large: about 2,500 (its budget below) |
| X1 | Tool files: the shape, the tool worker, `try`, `eval tool`, marks; `change_map`, `edge_similarity`; per-subject comparison on rungs 1 and 2 | E2 | Large: about 2,500 |
| M1 | The stdio MCP door (`whizbang serve`) and the decision cache | E1 | Large: about 2,400 |
| F1 | The follow-ups owned by milestone 2, and the note | all of the above | Large: about 2,400 |

**Waves.** Four, each started from `main` once what it builds on has landed.

1. **A1, A2 and P1** from `main`. A1 and A2 share `crates/whizbang-squint/src/check.rs`
   (A1 the use-case table, the prompt and the submission; A2 the toolbox, the tool calls,
   the projection and the details step), `crates/whizbang/src/squint.rs` and
   `crates/whizbang/src/door.rs` (A1 `--reference`; A2 `--detail` and `--details`), and
   the generated reference (`docs/reference/`, made again by `just docs` after merging
   `main`, never merged by hand). P1 shares `crates/whizbang/src/wiring.rs` (one line in
   `kinds`) and `docs/guides/config.md` with them, and `docs/results/m2c-use-cases.md`
   with A1: A1 creates it; P1's evidence (its item 7) needs A1's use cases, so P1 merges
   `main` once A1 has landed and only then takes it, adding its own lines.
2. **E1** when A1 and A2 have landed (its arms name `align`).
3. **E2 and M1** when E1 has landed (E2 also waits for P1). They share
   `crates/whizbang/src/main.rs` (a line each), `docs/results/m2c-use-cases.md` (M1's lines)
   beside `docs/results/m2c-evaluation.md` (E2's), and
   `crates/whizbang/tests/it/evidence.rs`, where each adds test functions of its own and
   edits none of another's.
4. **X1** when E2 has landed (its scoring is E2's metrics); it shares
   `crates/whizbang/src/eval.rs` with E2 (E2 owns `eval run` and `eval baseline`, X1 adds
   `eval tool` and `try`) and `evidence.rs` as above. **F1** when the other seven have
   landed.

In each wave the second to land merges `main` (a merge commit), runs `just docs` again,
and reruns its checks.

**Copies are counted apart.** E1 copies squint's tracked `evalsets/` byte for byte:
about 8,900 lines of manifests, jobs, families, seals and labels, none written by hand.
That copy is one commit of its own, held by a test against a committed list of squint's
digests (`sets::every_file_squint_tracked_under_evalsets_is_here_byte_for_byte`), and its
lines are reported apart in the hand-back; the 2,500 is of the rest. E2's synthetic
replay fixtures (about 2,500 lines of generated transcripts, made by the capture program
and `scripts/baseline/convert.py` and held by E2's replay test) are counted apart the same
way. The owner is told of these two exceptions here; no other pull request has one.

**Overflow, never a ninth pull request.** If a pull request would pass about 2,500
changed lines, its named overflow moves to F1, and its builder says so in the hand-back:
A1's port of squint's typed journal events and its open review follow-ups (carry-over
rows on the journal's typed events and on squint's milestone 4 follow-ups); A2's
`resize` record's alternatives section; P1's quote that sees the request (item 6); E1's
`store list` naming request ids (the second half of item 6); E2's descriptive live rerun
of the baseline and its one gate run (items 8 and 9); X1's rung 2 of `change_map` and
`edge_similarity` (item 9) and the carry-over row on the reference page (item 10); M1's
carry-over row on cancelling a run from another process. F1's own
overflow is re-owned on the carry-over list with a line saying why, and the owner is
told. Nothing splits into two pull requests.

**Review rule.** As in the first slices and 1D: a separate reviewer reads a pull request
only for four things: the handling of a token or key, egress (what leaves the machine and
to whom), processes or threads left running, and what ships in the binary or the
release. P1 (two new keys and two new recipients) gets one; E1 (a new origin the sets'
pictures are fetched from) gets one; X1 (a user's tool file run by a second process,
the tool worker, and its requests to whizbang) gets one; M1 (a process that stays up across runs,
and a new crate and its dependencies in the binary) gets one. A1, A2, E2 and F1 are
checked by the orchestrator and merged on green CI, with `just scan-range` over
`base..head`. A "Done when" is a list of tests by name.

**Test time.** The hosted macOS runner's tests take about 180 s of their 300 s today.
Milestone 2 may add at most 60 s there, about 7 s a pull request. Every test that calls a
provider, a model or the network beyond loopback is `#[ignore]`d and run by hand or by
`just test-models` on the Mac, never by CI. Pictures in tests are synthetic and at most
256 pixels on a side; judges are scripted or served by a recorded fake on a loopback
listener; sets in tests are three to six items made in the test; statistics in tests
resample at most 200 times. Each pull request reports its `just check` and CI test times,
says in its hand-back how it kept them lean, and stops and says so if the hosted macOS
runner passes about 270 s, or Linux or a local run about 150 s of its 180 s.

**Spend.** The owner's standing allowance is 100 US dollars a milestone without asking;
milestone 2 has spent under a cent so far (the first verdict, `docs/results/m2a-first-verdict.md`).
The live runs this plan asks for come to about 22 dollars, itemised under "Spend"; each
pull request that spends records what its ledger settled in its results file, and stops
and says so if its own spend passes twice its estimate.

## A1. The four use cases, from the command line (large)

### Goal

`whizbang squint check <use-case>` answers each of the five use cases of `SPEC.md`
section 7 with `pass`, `fail` or `unsure` and its reason, from the configured judge:

```
whizbang squint check reference-consistency --candidate <id> --reference <id> [--reference <id> …] [--brief <text>]
whizbang squint check reality-checks        --candidate <id> [--brief <text>]
whizbang squint check brief-compliance      --candidate <id> --brief <text>
whizbang squint check product-fidelity      --candidate <id> --reference <id> [--reference <id> …] [--brief <text>]
whizbang squint check source-fidelity       --source <id> --candidate <id> [--brief <text>]
```

1. **One table of use cases.** `source-fidelity`'s module becomes the first row of a table
   in `whizbang-squint`: each use case's name, the roles it requires (squint's
   `requires_roles`), its rules with their kinds (`must_hold`, `must_not`, `allowed`,
   `observe`), its checklist, its prompt and its submission. The four rule files are
   ported from squint (`crates/squint-core/rules/*.toml`, its checklist,
   `obligations.rs`, `render.rs` and `aggregate.rs`, by `port-module`, at the commit
   `docs/sources.toml` pins). The verdict is worked out from the judge's dispositions by
   squint's table for every kind, `must_not` included: the code decides, never the judge's
   own opinion.
2. **Bindings in order, with their roles.** `check` takes its images as an ordered list,
   each with a role (`source`, `candidate`, `reference`) and, for a reference, a label
   (`reference 1`, `reference 2`, … in the order given). A use case refuses a call that
   lacks a role it requires, or names one it does not take, when the call is prepared,
   and nothing is sent. `--reference` may be given up to eight times; the kind's
   `max_pictures` still bounds the conversation.
3. **The brief.** `--brief` is the job's requirements in words, as #179 made it for source
   fidelity. `brief-compliance` refuses a call without one when it is prepared. The brief
   is redacted like any text the executor knows, and recorded in the run.
4. **The job, whole.** A brief in words is not all a job holds. Squint's jobs
   (`crates/squint-core/src/job.rs`, `contract.rs`; the jobs of every set) carry structured requirements, each with its id, its scope, the images it takes as
   evidence and its kind, and each image's id and semantic label; these choose the
   obligations, the checklist's ids and the prompt. So `check` takes a job argument in
   that contract (`job/1`, ported, with its ADR), and the command builds one: the
   bindings from `--source`, `--candidate` and each `--reference`, and `--brief` as one
   requirement of free text. `--job <file>` takes a squint job file whole. E1's runner
   and E2's replay pass each set's jobs through it unchanged.
5. **Squint's loop rules the four need.** Repairs, the checklist and the shape of a
   verdict (carry-over row on the run record's lists); the retry rule from a fault's
   kind and wait, with the cases squint told apart by status (row on the status behind a
   failed request); a violation located wholly outside the candidate's frame accepted and
   clamped to the frame, never refused (row on a violation outside the frame); and what
   squint's configs held for the judge loop's budget, rules and tools that whizbang's
   config has no key for (row on what the old configs held); the journal's typed events
   (row on the journal's typed events) and squint's open review follow-ups from its
   milestone 4, as their modules are ported (row on those follow-ups; both overflow to
   F1): each built with a test, or dropped with a line on its row.
6. **Evidence, by hand, once.** The builder runs each of the four new use cases once on
   the owner's Mac with the owner's env file and the Anthropic judge, on synthetic
   pictures made for the call (no photograph), as task S2 did. The fields go in
   `docs/results/m2c-use-cases.md`: the commit, the use case, the model, the verdict, the
   tokens and the spend settled, the run, the recipient. No reply text.

- [x] Done when:
  `use_case::each_of_the_five_use_cases_is_in_the_table_with_its_roles_rules_and_checklist`,
  `use_case::a_confident_must_not_violation_fails_and_a_tentative_one_is_unsure`,
  `use_case::the_ported_rules_are_squints_rules_byte_for_byte_less_the_fields_dropped`,
  `check::reference_consistency_reaches_a_verdict_from_a_scripted_judge`,
  `check::reality_checks_reaches_a_verdict_from_a_scripted_judge`,
  `check::brief_compliance_reaches_a_verdict_from_a_scripted_judge`,
  `check::product_fidelity_reaches_a_verdict_from_a_scripted_judge`,
  `check::a_call_missing_a_required_role_is_refused_at_prepare_and_sends_nothing`,
  `check::references_are_sent_in_the_order_given_with_their_labels`,
  `job::a_squint_job_keeps_its_requirement_ids_scopes_evidence_and_image_labels_through_check`,
  `job::the_checklist_ids_and_the_prompt_of_a_ported_job_are_squints`,
  `cli::squint_check_job_takes_a_squint_job_file_whole`,
  `check::a_violation_located_outside_the_candidate_is_clamped_and_kept`,
  `check::a_retryable_fault_is_retried_after_its_wait_and_a_final_one_ends_the_loop`,
  `check::a_repair_of_an_invalid_submission_is_recorded_in_the_run`,
  `cli::squint_check_takes_each_use_case_with_its_roles_under_a_recorded_fake`,
  `cli::squint_check_brief_compliance_without_a_brief_is_refused_before_anything_runs`,
  `cli::squint_check_with_nine_references_is_refused_naming_the_limit`,
  `record::the_brief_and_each_bindings_role_and_label_are_in_the_run`,
  `evidence::the_use_cases_record_names_a_commit_of_this_tree_and_holds_no_secret`,
  `just docs` regenerated, and the carry-over rows above closed or narrowed with a line.

**Keeping tests lean (A1).** Scripted judges in process; pictures of 64 by 64; the
command's tests drive the door in process as S2's do.

## A2. The tools, and the details handed to the judge first (large)

### Goal

The judge is handed the measured details before it looks, so that it concludes from
them rather than working them out: for each pair a use case relates (the source and the
candidate; each reference and the candidate), `check` first aligns the candidate on the
other image and hands the judge the aligned pair and what the alignment found. The judge
may still call `view`, `resize` and `align` itself.

1. **`resize`** (squint's `builtin/resize.rs`): an image fitted to a size, as an action
   with a descriptor, offered to a judge.
2. **`align`, the control** (`builtin/align.rs` and `align/flat.rs`, `gray.rs`,
   `linalg.rs`): the candidate registered on a reference frame by squint's method, its
   homography and its residual, the aligned frame bound onto the source by a homography
   mapping (carry-over row on locations through a homography: the mapping's kind in
   `binding/1`, its ADR). `SPEC.md` section 17 leaves keeping squint's code or a library
   to the port: it is kept, since the baseline gate compares what it produced. Rungs 1
   and 2 for a control: two worked examples, and the registration error on 24 synthetic
   pairs made by the test from known transforms, in `docs/reference/tools/align.md`.
3. **What A2 does not do, and its budget.** `change_map` and `edge_similarity` are X1's,
   ported where they are scored; the carry-over rows on what a judge's tool hands back
   are F1's (item 2 there). A2's budget: `resize` about 120, `align` about 1,450 (squint's
   `builtin/align.rs`, `align/flat.rs`, `gray.rs`, `linalg.rs`, `mod.rs`, ported in its
   arithmetic and order; `sphere.rs` is not, its row says why), the homography mapping
   about 200, the projection and the details step about 300, tests about 350, the two
   records about 150: about 2,550. Overflow to F1: the record of `resize`'s alternatives
   section.
4. **The projection of a tool's result, as the baseline's arm had it.** Squint's arm C
   offered `align` under the projection `visual_only` (`crates/squint-core/src/tool/projection.rs`):
   the judge was handed the pictures and a fixed summary, and the measurements and the
   quality were withheld. `check` gains the argument `projection` (`visual_only`, as arm
   C, or `full`), ported with squint's summaries, and arm C is `details = none` with
   `projection = visual_only`. Its test is a whole exchange: a call of `align` from a
   scripted judge, answered as squint's recorded `align` call was answered, compared by
   E2's transcript comparison's rules on a transcript made in the test.
5. **The details first.** `check` gains the argument `details`: `none` (squint's arm C, as
   the baseline recorded it: the judge is told of the tools and calls them) or `first`
   (the default of the command): before the first turn, for each pair the use case
   relates, `check` calls `align` and each tool the run names with `--detail`, and puts
   their results in the opening message as labelled measurements, each with the tool's
   mark ("experimental", "unverified"), never presented as proof. Until X1 ports
   `change_map` and `edge_similarity`, `--detail` names `align` alone. A use case with one image
   (`reality-checks`, and `brief-compliance` without a reference) has no pair, and `first`
   adds nothing there. A decision in a new ADR, with the reason: the owner's goal above.
   `--details none --projection visual_only` gives the baseline's arm exactly. With
   `details = first` the measurements are handed whole (`projection = full`): they are
   the details the owner asked for.
- [x] Done when:
  `resize::a_picture_is_fitted_to_the_size_asked_keeping_its_aspect`,
  `align::a_known_homography_is_recovered_within_the_records_error_on_the_synthetic_pairs`,
  `align::the_ported_vectors_of_squint_give_squints_homography_and_residual`,
  `align::the_aligned_frame_is_bound_onto_the_source_by_its_homography`,
  `binding::a_location_maps_through_a_homography_and_back`,
  `check::details_first_puts_the_aligned_pair_in_the_opening_message_before_the_first_turn`,
  `check::details_none_opens_as_the_baseline_recorded_arm_c`,
  `check::under_visual_only_a_call_of_align_is_answered_with_its_pictures_and_squints_summary_alone`,
  `check::a_whole_exchange_of_align_under_arm_c_has_the_recorded_shape`,
  `check::a_use_case_with_one_image_adds_no_details`,
  `cli::squint_check_details_first_prints_the_verdict_under_a_recorded_fake`,
  `scripts/tests/test-tool-records.sh` holding the records of `resize` and `align`,
  `just docs` regenerated, and the carry-over row on locations through a homography
  narrowed with a line.

**Reworded with the task.** Item 2 said "the aligned frame bound onto the source by a
homography mapping", and the test's name says the same; it is bound onto the image it was
warped from (the candidate, as squint's frame had its root there), which is what lets a
location on it map back, and "source" there is that binding's source image. The action
states its measurements in a diagnostic, not a port, and `check` reads them by working them
out again (`docs/carry-over.md`, the row on a result that is not an artifact).

**Keeping tests lean (A2).** Alignment on pictures of at most 128 pixels, 24 pairs made
in one test; squint's vectors ported as small committed planes; no judge but a scripted
one.

### What the owner provides (A)

Nothing new: the Anthropic judge as configured for the first verdict. **Origins (A):
none new.**

## P1. The Google and OpenAI judges (large)

### Goal

`provider = "google"` and `provider = "openai"` beside `anthropic` in the kind
`service`, each passing provider conformance against squint's recorded exchanges, each
with its spend bound, so that every use case can be asked of a judge of any of the three.

1. **The wires.** Ports of squint's `gemini` (Gemini `generateContent`) and `openai`
   (Responses) adapters (`crates/squint-providers/src/gemini/`, `openai/`, the shared
   parts of `wire.rs`), by `port-module`, as task S1 ported Anthropic's: the core's
   `JudgeRequest` to the body and back to a `Turn`, its usage and a typed fault whose
   sentence is fixed words and typed fields (carry-over row on the fault's sentence),
   and whose stage is `not_sent` only when no byte left (row on whether a failed call may
   have been charged).
2. **The keys.** `key_env` as for Anthropic. Google's key goes only to its backend, in the
   `x-goog-api-key` header; OpenAI's only to its backend, as `Authorization: Bearer`;
   each only to its recipient's origin, never in a URL, a record, a diagnostic or a
   fault.
3. **The picture sent.** Each model's preparation from squint's catalogue (the long edge:
   1,536 for Gemini, 2,048 for OpenAI; the format), by the encoder S1 made; its version
   in the run (carry-over row on what an adapter said of itself).
4. **Spend.** The bound of each request from the kind's three ceilings at the model's
   price row (squint's catalogue, with the date read), the input counted as an upper bound
   by each provider's published picture formula, settled from the reply's usage, at its
   bound when nothing settles it, ADR 0026 amended for each.
5. **The models.** `gemini-3.8-flash` and `gpt-6.1-terra`, the cheapest of each in the
   study (`docs/baseline/manifest.toml`), in the config example of `docs/guides/config.md`;
   the others of the study (`gemini-4-argon`, `gpt-6.1-sol`) as price rows only.
6. **A quote that sees the request** (the rest of the row on what an adapter said of
   itself): a bound worked out from the request's shape, its text and its pictures, in
   place of the fixed ceiling, when it is lower. Overflow to F1.
7. **Evidence, by hand, once.** On the owner's yes to both recipients, the builder runs
   each of the five use cases once on each of the two new judges, on the synthetic
   pictures of A1, and adds the fields to `docs/results/m2c-use-cases.md`.

- [ ] Done when:
  `google::conformance::each_recorded_exchange_of_squint_is_read_back_to_its_turn`,
  `google::conformance::a_request_is_written_as_the_recorded_request_was`,
  `openai::conformance::each_recorded_exchange_of_squint_is_read_back_to_its_turn`,
  `openai::conformance::a_request_is_written_as_the_recorded_request_was`
  (each over squint's recorded fixtures, `fixtures/adapters/gemini-generate-content/` and
  `openai-responses/`, served by a recorded fake on a loopback listener with a test
  certificate),
  `google::a_fault_holds_nothing_of_the_body_the_headers_or_the_address`,
  `openai::a_fault_holds_nothing_of_the_body_the_headers_or_the_address`,
  `google::the_key_goes_only_to_the_approved_origin_in_its_header`,
  `openai::the_key_goes_only_to_the_approved_origin_as_a_bearer`,
  `wired::a_google_or_openai_call_to_a_recipient_the_config_does_not_name_is_refused_before_a_byte_leaves`,
  `google::the_input_count_is_at_least_the_usage_of_every_recorded_exchange`,
  `openai::the_input_count_is_at_least_the_usage_of_every_recorded_exchange`,
  `spend::judge::a_google_and_an_openai_turn_reserve_their_bound_and_settle_at_their_usage`,
  `prepare::each_models_picture_is_fitted_to_its_long_edge_and_the_run_names_the_preparation`,
  `record::a_run_on_each_new_judge_holds_no_byte_of_its_key`,
  `spend::the_example_configs_whole_call_reservation_fits_its_cap_for_each_judge`,
  `evidence::the_use_cases_record_names_a_commit_of_this_tree_and_holds_no_secret` on the
  record with both judges' lines, and the carry-over rows above closed or narrowed.

**Status (P1's pull request, 2026-11-18).** Left unticked: every clause is shown by its
named test but the last, `evidence::the_use_cases_record_names_a_commit_of_this_tree_and_holds_no_secret`
on the record with both judges' lines, which waits on the owner's yes to the two
recipients and their keys (nothing has been sent to either) and on A1's record (carry-over
row on P1's live evidence). Item 6 moved to F1 as the named overflow.

**Keeping tests lean (P1).** The recorded exchanges are read once per test; pictures of
`many-images` cut to a tenth on each side in the test, as S1 did; one loopback listener
per test.

### What the owner provides (P)

The answers to the first two items under "What the owner must answer", and, on each yes,
the key in the owner's env file at the path the builder's brief names (as
`GEMINI_API_KEY` and `OPENAI_API_KEY`), put there by the owner. No agent opens, prints or
copies that file; whizbang alone reads it, by `--env-file`; CI never uses it.

### Origins (P)

`https://generativelanguage.googleapis.com` (Google's Gemini API) and
`https://api.openai.com`, each for judging, each with the classes `source_images`,
`derived_images`, `prompts` and `evidence`. Both are new recipients: neither is approved,
and nothing is sent to either until the owner says yes. On a no, that provider's kind
lands with its conformance (which sends nothing) and its live evidence is left on a
carry-over row; the milestone's row then waits on it. No upstream, no probe. CI contacts
nothing.

## E1. Evaluation sets, matrices, the runner, `runs stats` and `runs export` (large)

### Goal

Squint's evaluation sets are in whizbang, byte for byte, checked by code; a matrix of
use cases, sets, judges and arms runs each item as a run of `check`, resumably; and
`whizbang runs stats` and `runs export` report on those runs.

1. **The sets, moved.** Every file squint tracks under `evalsets/` (the manifests, jobs,
   families, seals, corrections, receipts and the labels of the released sets) is copied
   to `evalsets/` byte for byte, with `docs/evalsets.md`. Nothing more: blind-v3 has no
   labels in squint's repo and gets none here; its labels stay in the owner's storage.
   The pictures are not committed: they are fetched by hash.
2. **The sets, read** (`squint-eval`'s `sets.rs`, by `port-module`): a set, its seal, its
   corrections and its labels read and checked; `whizbang eval sets check` runs every
   check that code can, offline; a seal that does not match is refused.
3. **The pictures, fetched.** `whizbang eval sets fetch --set <id> --store <dir>` fetches
   each picture of a set from `https://raw.githubusercontent.com` at the revision of
   `wafflery-evalsets` the set pins, through N1's download client behind the egress gate,
   checks its hash, and imports it as a picture (P0). Only on the owner's yes (third item
   under "What the owner must answer"); on a no, `--from <dir>` imports a folder the owner
   placed by hand, by the same hashes.
4. **Matrices and the runner** (`matrix.rs`, `descriptor.rs`, `runner.rs`, `arms.rs`):
   `whizbang eval matrix --matrix <file>` runs each cell (use case, set, judge, arm,
   repetition) through the executor as a run of `check`, with the cell's identity in the
   run; an interrupted matrix resumes, and a failed attempt is reported, never replaced
   by a later success (`SPEC.md` section 9, "Run resume"). Arms are `C` (`details =
   none`, `view` and `align`) and `F` (`details = first`), and an arm may name more
   tools. Squint's study matrices are ported as files, with the models of P1 named.
   **Spend.** Each run is admitted as any call of `check` is, reserving its own bound
   (the request bound times `check`'s 16 turns: about 0.10 dollars for Haiku and 0.60
   for Flash at the kind's defaults); the runner admits at most `concurrency` runs at
   once (default 2), so at most that many reservations are held. A whole matrix is never
   reserved at once: 600 Haiku decisions so reserved would hold about 58 dollars. Before
   its first run, a matrix's forecast (its decisions times the model's settled cost per
   decision from the study, `docs/baseline/manifest.toml`, times two) is set against the
   allowance left, and a matrix whose forecast passes it is refused before anything runs.
5. **`runs stats` and `runs export`** (`SPEC.md` section 3, "Metrics from ordinary
   runs"): stats over a filter (an action, a tool, a model, a backend, a use case, a
   matrix, a period): runs, outcome rates, verdict and unsure rates, retries and
   repairs, time as median and 95th percentile, cost, and cache hits once M1 writes them;
   export as one JSON object per line. Labels are never in a run; a matrix's report joins
   them from the set at the time it is read.
6. **Request ids and paid calls in volume** (carry-over rows on every publication
   writing the whole generation, and on the records of request ids): measured at 100,000
   ids before the first matrix runs, and decided as the row asks, with the ADR; the
   ledger's fold starts from the newest reset, so the names of closed epochs are not read
   at an admission, as the row's second half asks; and `whizbang store list` says which
   names are request ids (overflow to F1).
7. **Evidence, by hand, once.** On the owner's Mac: the five dev sets and the five
   adversarial sets fetched (110 items), and one matrix of arm `F` over them with
   `claude-haiku-5-5`, two repetitions (220 decisions). Arm `F` calls `align` once for
   each pair a use case relates, whatever the judge does, so the 132 decisions of the
   three use cases with pairs make at least 132 runs of `align`, each a child run of its
   `check`. Then `runs stats --tool align` and `runs export --tool align` over them: the
   counted population is the runs of the action `align`, and each must report at least
   100. The counts and rates go in `docs/results/m2c-evaluation.md` as
   aggregates by set and use case: no item's verdict beyond what its set already
   publishes.

- [x] Done when:
  `sets::every_file_squint_tracked_under_evalsets_is_here_byte_for_byte`,
  `sets::a_set_whose_seal_does_not_match_is_refused`,
  `sets::corrections_are_applied_as_a_versioned_file_beside_the_set`,
  `sets::no_set_without_published_labels_has_a_labels_file`,
  `fetch::a_picture_is_fetched_by_hash_from_the_pinned_revision_and_imported`,
  `fetch::a_picture_whose_hash_differs_is_refused_and_nothing_is_imported`,
  `fetch::a_fetch_to_an_origin_the_config_does_not_name_is_refused_before_a_byte_leaves`,
  `fetch::from_a_folder_imports_the_same_pictures_by_hash`,
  `matrix::each_cell_runs_as_a_run_of_check_with_its_cell_in_the_record`,
  `matrix::an_interrupted_matrix_resumes_and_a_failed_attempt_is_kept`,
  `matrix::a_matrix_whose_forecast_passes_the_allowance_left_is_refused_before_its_first_run`,
  `matrix::at_most_the_concurrency_of_runs_hold_a_reservation_at_once`,
  `ledger::an_admission_after_many_paid_runs_and_a_reset_reads_no_closed_epoch`,
  `matrix::the_ported_study_matrices_are_read`,
  `runs::stats_over_a_tool_report_count_outcomes_verdicts_time_and_cost`,
  `runs::export_writes_one_json_object_per_run_and_no_label`,
  `store::request_ids_at_100000_publish_within_the_measured_bound`,
  `cli::store_list_says_which_names_are_request_ids`,
  `cli::eval_matrix_runs_a_three_item_set_under_a_scripted_judge_and_runs_stats_reports_it`,
  `evidence::the_evaluation_record_names_a_commit_of_this_tree_and_holds_only_aggregates`,
  `evidence::the_evaluation_record_reports_at_least_100_runs_of_align_by_stats_and_by_export`,
  and the carry-over rows above closed.

**Reworded with the task (E1).** The box is ticked with one clause moved:
`cli::store_list_says_which_names_are_request_ids`, `whizbang store list` naming request
ids, is the named overflow to F1 (plan 2C, "Overflow"; the carry-over row on the records of
request ids). Two tests live one module deeper than named:
`store::volume::request_ids_at_100000_publish_within_the_measured_bound` (`#[ignore]`d, run by
hand, its numbers in ADR 0030, with `store::volume::request_ids_at_2000_…` in CI) and
`spend::ledger::an_admission_after_many_paid_runs_and_a_reset_reads_no_closed_epoch`. Item 7
ran on 2026-11-18 after the owner's yes (`docs/results/m2c-evaluation.md`): 220 decisions,
172 runs of `align` by `runs stats` and by `runs export`, 0.77 dollars settled. The copy of
squint's `evalsets/` is 374 files and 16,428 lines, not the 8,900 the split estimated.

**Keeping tests lean (E1).** Sets made in the test, of three to six items with pictures
of 32 by 32; the byte-for-byte test compares digests listed in a committed file, not
squint's repo; the 100,000-id measurement is `#[ignore]`d and run once, its number
recorded, with a test of 2,000 ids in CI.

### Origins (E)

`https://raw.githubusercontent.com`, for downloads only (a GET of a picture by its path
at a pinned revision of the public `wafflery-evalsets` repository; nothing of the
owner's is sent), on the owner's yes. No other origin.

**Approved by the owner on 2026-11-18**: "approved on #3", the third item under "What the
owner must answer". Used once, for the evidence of item 7.

## E2. Metrics, paired statistics, gates and the baseline gate (large)

### Goal

A gate is frozen in a file before a rung 3 or 4 run starts, `whizbang eval run --gate
<file>` runs it and decides it as `SPEC.md` section 8 says; and the baseline reproduces:
whizbang, given the recorded judge turns, produces the same sequence of judge requests,
tool calls and verdicts for every recorded decision.

0. **What is ported, and what is not.** Squint's seven modules come to about 2,600
   lines; E2 ports the parts a gate decides by and leaves the rest. Ported: from
   `metrics.rs` the measures and the paired differences (about 400), `bootstrap.rs` and
   `wilson.rs` (about 320), `gate_rules.rs` and the deciding part of `gate.rs` (about
   700), `admissions.rs` (about 100). Not ported: `summary.rs`'s report (the report
   viewer is milestone 5's), `canary.rs` (a line on the carry-over list saying why),
   `estimate.rs` (E1's forecast replaces it), and `gate.rs`'s publication of labels and
   manifests (item 2). New: `view` as squint's judge knew it (about 250), the capture
   program (about 200), the transcript writer and the replay (about 600), tests (about
   400). About 2,800 in all, so the gates are E2's planned overflow: E2 builds items 0,
   1 and 3 to 8 first, and items 2 and 9 (the gate files, `eval run` and the one gate
   run, about 800 with their tests) only as far as its 2,500 allows; the rest moves to
   F1, which takes it before any carry-over row (F1, item 1). The synthetic transcripts
   and pictures of item 3 are generated by the capture program and `convert.py`, not
   written by hand, and are counted apart as E1's copy is.
1. **Metrics and paired statistics** (`metrics.rs`, `bootstrap.rs`, `wilson.rs`):
   balanced accuracy with `unsure` and errors counted as neither; paired
   differences by family with resampling by family; the correction for several
   hypotheses; Wilson intervals; self-disagreement across repetitions.
2. **Gates** (`gate.rs`, `gate_rules.rs`, `admissions.rs`): a gate file states every item
   of `SPEC.md` section 8, "Gates", and is frozen by its digest in the run; too little
   data is "inconclusive", never a pass; `whizbang eval run --gate <file>` runs the
   gate's matrix through E1's runner and writes its receipt. **Labels stay where they
   are.** Squint's `gate.rs` wrote a held-out set's labels into `evalsets/` and rewrote
   its manifest once scored (`publish_manifest`); that is not ported. Scoring reads labels
   from a path given on the command line, writes nothing into the sets' tree, and its
   receipt holds aggregates only.
3. **A public replay fixture.** `docs/baseline/example-transcript.json` is an invented
   decision whose images are strings, so it cannot exercise preparation or tools. E2
   captures three decisions from frozen squint with an offline harness: in a scratch
   clone of squint at its tag (never pushed; the old repo is not changed), a small test
   program runs squint's own harness with a provider that answers scripted turns while
   applying the real preparation of `crates/squint-providers/src/prepare.rs` for the
   profile of `claude-haiku-5-5`, and writes the journal, the stored exchanges, the tool
   artifacts and the prepared pictures in the package's layout, which
   `scripts/baseline/convert.py` reads. Each decision calls `view` and `align` on
   synthetic pictures made for the purpose. The program is kept in this repo as
   `scripts/baseline/capture/` with how to run it; the converted transcripts and their
   pictures are committed at `fixtures/baseline/synthetic/`. Nothing in them is the
   owner's, and no provider is called.
4. **`view` as squint's judge knew it** (carry-over row on what squint's `view` had around
   its pixels, taken here because the replay compares it): the judge's call names a
   `frame` as squint's did, not whizbang's `image`, and is answered with squint's
   structured presentation (the crop's place on its source, its size, its label), each
   compared by the transcript format; whizbang's present form stays for `details = first`
   only if the ADR of A2 says so. About 250 lines, inside E2's budget by its overflow.
5. **The transcript writer.** A run of `check` written as a transcript in the format of
   `docs/baseline/transcript.md`, from what the run recorded (carry-over row on sending a
   run again from what the store holds: what a transcript needs is recorded).
6. **The choice for scaled tool images** (carry-over row on what format 1 compares for a
   tool image): this plan takes the first of the row's two choices, a transcript format 2
   that compares a scaled tool image's size and mapping in place of its prepared bytes,
   and moves `COMPARISON_VERSION`; `view` keeps one method. The number it rests on, how
   many recorded tool images are crops whose cut exceeds 1,024 pixels on a side, is
   counted by `scripts/baseline/audit.py --count-scaled-cuts` from the owner's storage,
   by the owner, and printed as one number. If it is 0, format 1 stands and nothing
   changes. Either way it is agreed with the owner before the gate runs (fourth item under
   "What the owner must answer").
7. **The baseline gate.** `whizbang eval baseline --package <dir> --transcripts <dir>
   --receipt <file>` replays each recorded decision at the core's judge interface: a
   judge that answers the recorded turns, with the real preparation of each model (A1's
   use case, A2's `view` and `align`, P1's preparations), compares each transcript with
   `whizbang_eval::baseline::compare`, and writes the receipt of `docs/baseline/README.md`
   (the package digest, the comparison version, the commit, the result) and nothing else.
   It prints counts only. The owner runs it on the Mac on E2's pushed head, before E2 is
   merged (the package is owner-only and no agent reads it); the receipt it writes holds
   the four public fields alone, and the builder commits it at
   `docs/baseline/receipt.toml` on E2's branch. If the owner's run cannot be had before
   E2 is otherwise ready, the receipt and its test move to F1, and E2 merges without
   them, saying so.
8. **The live rerun, descriptive** (`SPEC.md` section 16): arm C of `claude-haiku-5-5`
   on `source-fidelity/blind-v3` (an exposed regression set, not held out), one
   repetition, run by the owner with the same command's `--live`; its agreement with the
   recorded run is printed beside the recorded run's self-disagreement, as two numbers,
   and those go in `docs/results/m2c-baseline.md`. Overflow to F1, with item 9.
9. **One gate, run** (the evidence of items 1 and 2): arm `F` against arm `C` on the five
   dev sets with `claude-haiku-5-5` (three repetitions), `gemini-3.8-flash` and
   `gpt-6.1-terra` (one each), frozen in `evalsets/gates/details-first.toml`, its
   aggregates in `docs/results/m2c-evaluation.md`. Dev sets cannot admit anything; the
   result says whether handing the judge the details first helped, and is kept either
   way.

- [ ] Done when:
  `metrics::balanced_accuracy_counts_unsure_and_errors_as_neither`,
  `bootstrap::paired_differences_resample_by_family_with_a_fixed_seed`,
  `metrics::the_ported_vectors_of_squint_give_squints_numbers`,
  `gate::a_gate_file_missing_an_item_of_the_spec_is_refused`,
  `gate::too_little_data_is_inconclusive_never_a_pass`,
  `gate::the_frozen_gates_digest_is_in_every_run_it_makes`,
  `transcript::a_run_of_check_is_written_as_a_transcript_the_comparison_reads`,
  `view::a_judges_call_names_a_frame_and_is_answered_with_squints_presentation`,
  `baseline::format_2_compares_a_scaled_tool_images_size_and_mapping_and_not_its_bytes`,
  `baseline::each_synthetic_decision_replayed_through_the_loop_preparation_and_tools_compares_with_no_difference`,
  `gate::scoring_a_synthetic_blind_set_leaves_the_sets_tree_unchanged_and_writes_only_aggregates`,
  `baseline::a_replay_that_differs_at_one_place_fails_and_names_the_place`,
  `baseline::the_receipt_holds_exactly_the_four_fields`,
  `cli::eval_baseline_prints_counts_and_no_item`,
  `evidence::the_baseline_receipt_names_the_manifests_digest_and_a_commit_of_this_tree`
  (the receipt the owner's run wrote, with `result = "pass"`),
  `evidence::the_evaluation_record_names_a_commit_of_this_tree_and_holds_only_aggregates`
  on the gate's lines, and the carry-over rows on the baseline gate and on what format 1
  compares closed.

**Keeping tests lean (E2).** Statistics on families made in the test, 200 resamples;
the replay's tests on the three synthetic decisions of item 3 (pictures of at most 128
pixels), never the package; no live call.

**What E2's pull request built, and what moved (E2).** Built: item 1 (`whizbang_eval::metrics`,
`bootstrap`, `wilson`, with squint's resampling reproduced and held to squint's own numbers);
of item 2, the gate file and how a gate is decided (`whizbang_eval::gate`); item 6 (format 2
in the comparison, `COMPARISON_VERSION` 4, and `scripts/baseline/audit.py --count-scaled-cuts
--transcripts <folder>`, the owner's command for the fourth item under "What the owner must
answer", which prints one number). Moved to F1, item 1, each with a carry-over row: the rest
of item 2 (`eval run`, the gate's digest in its runs, scoring from a labels path), and items
3, 4, 5 and 7 (the replay and `eval baseline`), which did not fit beside the rest; items 8 and
9 with them. The box stays open: of its clauses, `metrics::balanced_accuracy_counts_unsure_and_errors_as_neither`,
`bootstrap::paired_differences_resample_by_family_with_a_fixed_seed`,
`metrics::the_ported_vectors_of_squint_give_squints_numbers`,
`gate::a_gate_file_missing_an_item_of_the_spec_is_refused`,
`gate::too_little_data_is_inconclusive_never_a_pass` and
`baseline::format_2_compares_a_scaled_tool_images_size_and_mapping_and_not_its_bytes` pass.

### What the owner provides (E)

The answers to the third, fourth and fifth items under "What the owner must answer"; for
the fifth, two commands on the Mac, each printing counts only.

## X1. Tool files, `try` and `eval tool`, and per-subject comparison (large)

### Goal

A perception tool written as one Python file is tried on one pair, scored on a set,
edited and run again with no rebuild (`SPEC.md` section 4, "Experimental tools"); every
tool the upper rungs apply to shows its mark; and per-subject comparison exists as such a
tool and is scored on rungs 1 and 2.

1. **The shape first.** The first commit is an ADR deciding the shape of a tool file
   (`SPEC.md` section 4: "the first decision of milestone 2, made before any tool is
   written"; A2's Rust tools are ports with descriptors, not tool files): its declaration
   (name, arguments, output bounds, the bindings it reads, the kind of evidence it
   returns) and its one function, which reaches images and capabilities only through the
   context it is given.
2. **Where a tool file runs: a tool worker of its own.** Not in the model's worker: that
   worker runs one call at a time, so a tool inside it asking for SAM 3 would wait on
   itself. The kind `pack` starts a second process of the pack's runtime, the tool worker
   (at most one, started on the first tool call, stopped with the kind), speaking a
   protocol of its own, `tool/1`, beside `pack/1`, which is unchanged (ADR 0020: a new
   kind of request is a new version; a new ADR states `tool/1`). Whizbang asks it to run
   a tool; the tool asks whizbang for a capability through its context, and whizbang
   answers through the executor as a nested call of the tool's run (ADR 0008), under the
   run's allowance and policy, with at most one capability request open at a time and
   each bounded by the run's time; cancelling the tool's run cancels its nested call. The
   carry-over row on a request from the worker is closed for the tool worker and says so.
   Files are listed under `[tools] paths` in config; a run pins each file's digest; an
   edited file is the next run's, with nothing rebuilt or installed. Its output is
   checked against its declared bounds before anything uses it.
3. **Nothing left running.** The tool worker is started in a process group of its own,
   and stopping it (a tool's fault, a cancellation, the kind dropped, the door's end of
   input) ends the whole group and waits for it, so a process a tool file started is
   ended with it, even one holding the worker's pipes. A tool file runs with the
   worker's privileges, as a user's adapter does; the ADR and `docs/guides/` say so
   plainly.
4. **A tool file offered to a judge.** A run that names a tool file (`--tool <name>` of
   `squint check`, or an arm of a matrix) offers it to the judge with the schema its
   declaration gives, and `check` dispatches the judge's calls of it to the tool worker;
   a file not named is never offered (`SPEC.md` section 4, "Reach").
5. **`whizbang try <tool>`** on one image or pair, no judge; **`whizbang eval tool <tool>
   --set <set>`** (rung 2): the tool's read on each item of a set, scored against the
   set's labels by E2's metrics, each item a run.
6. **Marks** (carry-over row on a tool's mark): `whizbang describe` shows the mark of each
   tool the upper rungs apply to, from its record; a tool without a record cannot be
   unverified or admitted.
7. **Per-subject comparison** (`SPEC.md` section 7: new, experimental): a tool file that
   finds a named subject in the source and the candidate with SAM 3 (by name, a phrase;
   by location, a point or a box) and reports, per subject, its presence, its area, its
   place and the agreement of its two masks after A2's alignment. Rung 1: two worked
   examples in its record. Rung 2: `eval tool` on the two sets of item 8, on the Mac, its
   separation of pass from fail recorded.
8. **How a measurement is scored on rung 2.** A tool's declaration names its primary
   measurement and which way is "changed" (larger or smaller); a tool reporting several
   regions or subjects reduces them by its declared rule (the worst one). `eval tool`
   scores that number's separation of the set's fail items from its pass items as the
   area under the ROC curve, with a 95% interval resampled by family (E2's bootstrap),
   and the balanced accuracy at the threshold chosen on one half of the families and
   read on the other: the families of every set scored are pooled and halved by a fixed
   seed, so no family is on both sides; with fewer than four families pooled the
   threshold is not chosen and the balanced accuracy is reported as inconclusive, the
   area alone standing. An item whose tool failed or answered nothing is counted as an
   error, reported apart and never as a pass or a fail. Per-subject comparison is scored
   on source fidelity alone (a source and a candidate): `source-fidelity/scenes-v1` and
   `source-fidelity/adversarial-v1`. Its subjects come from each job's requirements of
   kind `must_hold`, never its label: the phrase before " keeps " in the requirement's
   text (squint's jobs write each as "The rectangular swimming pool keeps its identity,
   …"), asked of SAM 3 as a phrase on the source and on the candidate after `align`; a
   requirement not of that form is reported as not applicable. Each subject's value is
   one minus the IoU of its two masks (0 when both are empty, 1 when one is), and the
   item's value is the largest of its subjects'. `reference-consistency` is not scored
   on it in milestone 2.
9. **`change_map` and `edge_similarity`** (squint's `builtin/compare.rs` and their
   manifests, about 450 with tests), ported as experimental tools, each with its record
   and two worked examples (rung 1), each named by `--detail` to be handed to the judge
   first; then rung 2 by `eval tool` on `source-fidelity/scenes-v1`; each
   becomes unverified when its record holds rungs 1 and 2, and only then may `details =
   first` run it by default, as `SPEC.md` section 8 lets an unverified tool be a default
   while nothing admitted replaces it.
10. **The reference page** (carry-over row on what `docs/reference/actions.md` prints):
   each action's arguments, ports and needs, with its mark. Overflow to F1.

- [ ] Done when:
  `tool_file::a_declaration_missing_its_bounds_is_refused_when_the_config_is_read`,
  `tool_file::an_output_outside_its_declared_bounds_is_refused_before_use`,
  `tool_file::a_tool_reaches_a_capability_only_through_its_context_and_under_the_runs_allowance`,
  `test_tool_worker.Capabilities.test_a_tool_file_asks_for_a_segment_and_is_answered`,
  `tool_worker::a_capability_asked_by_a_tool_is_a_nested_call_under_the_runs_allowance`,
  `tool_worker::cancelling_a_tools_run_cancels_its_nested_call`,
  `tool_worker::a_second_capability_request_while_one_is_open_is_refused`,
  `tool_worker::a_tools_fault_ends_the_group_and_a_child_holding_the_pipes_with_it`,
  `tool_worker::a_cancellation_ends_the_group_and_waits_for_it`,
  `check::a_named_tool_file_is_offered_to_the_judge_with_its_schema_and_called`,
  `check::a_tool_file_not_named_is_never_offered`,
  `cli::a_tool_file_is_tried_scored_edited_and_run_again_without_a_rebuild`
  (the digest in each run, the second run's the edited file's),
  `cli::try_runs_one_tool_on_a_pair_with_no_judge`,
  `cli::eval_tool_scores_a_tool_on_a_three_item_set`,
  `change_map::the_ported_vectors_give_squints_map_and_measurements`,
  `edge_similarity::the_ported_vectors_give_squints_ratio`,
  `check::an_experimental_tool_is_run_only_when_the_run_names_it`,
  `cli::squint_check_with_detail_change_map_hands_its_measurements_to_a_scripted_judge`,
  `eval_tool::a_measurement_that_separates_fail_from_pass_scores_an_auc_of_one_and_a_shuffled_one_near_a_half`,
  `eval_tool::the_threshold_is_chosen_on_one_half_of_the_families_and_read_on_the_other`,
  `eval_tool::fewer_than_four_families_pooled_give_the_area_alone_and_an_inconclusive_accuracy`,
  `eval_tool::an_item_whose_tool_failed_is_an_error_and_neither_pass_nor_fail`,
  `test_per_subject.Subjects.test_subjects_come_from_the_jobs_requirements_and_never_its_label`,
  `test_per_subject.Subjects.test_a_requirement_not_of_the_form_is_not_applicable`,
  `test_per_subject.Score.test_the_items_value_is_the_largest_subjects_one_minus_iou`,
  `describe::each_tool_the_upper_rungs_apply_to_shows_its_mark_from_its_record`,
  `describe::a_tool_without_a_record_cannot_be_unverified_or_admitted`,
  `test_per_subject.Compare.test_a_subject_removed_is_reported_absent_by_name_and_by_location`
  against the fake models,
  `model_tier::per_subject_comparison_answers_the_records_examples_on_the_mac`
  (`#[ignore]`, under `just test-models`),
  `evidence::the_per_subject_record_names_a_commit_of_this_tree_and_its_rung_2_numbers`,
  `scripts/tests/test-tool-records.sh` holding the records of `per_subject`,
  `change_map` and `edge_similarity` with rungs 1 and 2, and the carry-over rows above
  closed.

**Keeping tests lean (X1).** Tool files in tests are a few lines run by the pack's
fake models; the worker is started once per test file, as the pack's tests do; SAM 3
runs only under `just test-models`.

### Origins (X)

None: SAM 3 as installed, on the Mac. Nothing leaves the machine.

## M1. The stdio MCP door and the decision cache (large)

### Goal

`whizbang serve` speaks MCP over stdio to a local agent (`SPEC.md` section 1), offering
each of the five use cases and the tools, and a job asked again unchanged is answered
from the decision cache.

1. **The crate `whizbang-serve`** (`SPEC.md` section 3: MCP types stay inside it), given
   the executor by the binary. The server's library is chosen here (`SPEC.md` section 17:
   "for the server, chosen at milestone 2"): squint's `rmcp` at a pinned version, or a
   hand-written server on the frames of the protocol's one revision, in an ADR with what
   each adds to the binary. `scripts/crate-graph.txt` gains its edges.
2. **What it offers.** Each action as a tool with its schema, `check` taking the use case,
   its images by identity and their roles, and the brief; progress as notifications;
   cancellation of a call; nothing but the protocol on stdout.
3. **A process that outlives a call** (carry-over rows on what running an action does not
   do yet, on events a door shows, and on files verified once in a process): a run that
   waits gives up after a stated time; events reach the client as they are published;
   an installation's files are verified again before each `load` that follows an unload
   or a restart of the worker; a run the server started is cancelled, waited for and
   told apart from an abandoned one by a second process (carry-over row on cancelling
   in the process that made the run; overflow to F1); on end of input the server cancels what runs, waits for
   it, and stops its worker.
4. **The decision cache** (`SPEC.md` section 9; squint's `cache.rs`): the key is known
   before the run, from the complete job (use case, brief, every semantic argument, the
   bindings in order with their roles and labels), the resolved rules, checklist and
   prompt, the config that bears on it, the revisions of every enabled tool (a tool
   file's digest included), the capability identities, the image preparation, the limits
   and the harness version. A hit answers with the earlier run's verdict and records a
   run that names it; a judge's result from a backend that cannot report its identity is
   not cached. The cache is never read by `eval matrix`, `eval run`, `eval tool` or
   `eval baseline`: each repetition is a fresh trial. Off by default; `[cache]` turns it
   on.
5. **Evidence, by hand, once.** Each of the five use cases asked over stdio by a client of
   the test kit's, on the owner's Mac, with the Anthropic judge on A1's pictures; then each
   asked again and answered from the cache. The fields go in `docs/results/m2c-use-cases.md`.

- [ ] Done when:
  `serve::initialize_lists_each_action_as_a_tool_with_its_schema`,
  `serve::check_over_stdio_returns_the_verdict_of_each_use_case_under_a_scripted_judge`,
  `serve::nothing_but_the_protocol_is_written_to_stdout`,
  `serve::a_cancelled_call_ends_its_run_as_cancelled`,
  `serve::end_of_input_cancels_waits_and_stops_the_worker`,
  `serve::end_of_input_ends_the_tool_workers_group_with_a_child_holding_its_pipes`
  (with X1, whichever lands second adds it),
  `serve::a_run_waiting_past_its_stated_time_gives_up`,
  `serve::progress_reaches_the_client_as_notifications`,
  `pack::files_are_verified_again_before_a_load_after_a_restart_of_the_worker`,
  `cache::a_change_of_use_case_brief_role_order_rule_or_tool_misses`,
  `cache::an_unchanged_job_hits_and_records_a_run_naming_the_earlier_one`,
  `cache::a_tool_files_edit_misses`,
  `cache::evaluation_never_reads_the_cache`,
  `cache::a_backend_that_cannot_report_its_identity_is_not_cached`,
  `cli::serve_answers_a_client_over_stdio_and_exits_when_its_input_ends`
  (the binary started by the test, killed and waited for on every path),
  `evidence::the_use_cases_record_names_a_commit_of_this_tree_and_holds_no_secret` on
  the record with the door's lines, `scripts/crate-graph.txt` holding `whizbang-serve`,
  and the carry-over rows above closed.

**Where M1's pull request leaves the box.** Unticked. Shown by the tests named above: the
seven of `serve::` other than the tool worker's group, `pack::files_are_verified_again_before_a_load_after_a_restart_of_the_worker`,
four of the five `cache::`, `cli::serve_answers_a_client_over_stdio_and_exits_when_its_input_ends`,
and `scripts/crate-graph.txt` holding `whizbang-serve`. Not shown, each a carry-over row:
`cache::a_tool_files_edit_misses` and `serve::end_of_input_ends_the_tool_workers_group_with_a_child_holding_its_pipes`,
which need X1's tool files; and of item 3, a cancel from a second process (F1, as named).
Item 5's evidence was narrowed by the orchestrator to one use case asked over stdio and
then answered from the cache (`docs/results/m2c-use-cases.md`, held by
`evidence::the_use_cases_record_names_a_commit_of_this_tree_and_holds_no_secret`). The
carry-over row on files verified once in a process is closed; those on what running an
action does not do yet and on the events a door shows keep only what no door can do
(ADR 0031).

**Keeping tests lean (M1).** The server's tests run in process over in-memory pipes; one
test starts the binary; cache keys are computed over jobs made in the test.

### Origins (M)

None new: the door is local, on stdio; what it sends a judge goes through the same gate.

## F1. The follow-ups, and the note (large)

1. **What overflowed** from the pull requests above, each named in its builder's
   hand-back, first: E2's gates if they moved, since `SPEC.md` section 8 and the first
   slices' table owe them. Then 1D's follow-ups (item 3), then the rows of item 2; a row
   that does not fit is re-owned with a line saying why, never a spec item.
2. **The rest of the carry-over rows owned by milestone 2**, each built with a test, or
   re-owned or dropped with a line saying why: what a judge's tool hands back (rows on a
   tool's result, on deleting a run's artifacts for tools, on what an action hands back,
   on `view` decoding each time, on the reduced crop with transparency, on the fixed
   slack, and on the tool manifest); picture loading (rows on colour profiles
   that are not RGB matrices, on a JPEG whose scans are short, and on what squint's
   `image.rs` holds beside identity); the check behind a send (rows on a prepared JPEG
   known by its markers alone, and on folding case on macOS); the executor (rows on an
   executor built with a digest alone, two needs of one capability, a parent reading a
   child's names); the models directory (rows on `whizbang models verify`, and on copies
   of users' adapters never removed); the panorama side of `view` and the rest of
   squint's frames, if A2's overflow brought them.
3. **1D's follow-ups** (carry-over row on 1D's last two pull requests): a named test that
   a user's catalogue using a shipped model's name is refused whole; the worker refusing
   a load with no record when the installation's folder holds `files/`; ADR 0017 saying
   plainly that a user's adapter runs with the worker's full privileges; a test that the
   worker's `SHIPPED` and the shipped catalogue cannot drift apart. The Apache-2.0
   licence text shipped beside hosted weights is re-owned by milestone 6, when weights
   are hosted.
4. **The note.** `docs/plans/m2-note.md`, in the form of `m1d-note.md`: each clause of row
   2 with the tests that show it, the spend, what a reader should not assume, what moved
   to the carry-over list, and what milestone 3 and E start from.

- [ ] Done when:
  `models::a_users_catalogue_naming_a_shipped_model_is_refused_whole`,
  `test_worker.Load.test_a_load_with_no_record_beside_files_is_refused`,
  `test_worker.Shipped.test_the_workers_shipped_map_is_the_shipped_catalogue`,
  `models::verify_hashes_every_file_and_writes_the_check_record_only_when_whole`,
  `models::removing_a_model_removes_adapter_copies_nothing_names`,
  `pixels::a_jpeg_whose_scans_are_short_is_refused`,
  `jpeg::a_table_body_out_of_its_structure_is_refused_by_the_check`,
  `gate::a_store_under_a_folder_written_in_another_unicode_case_is_kept_out`,
  `check::a_tools_image_is_a_reference_only_its_parent_reads`,
  `view::a_run_reading_an_image_twice_decodes_it_once`,
  `view::a_reduced_crop_with_transparency_is_the_mean_weighted_by_alpha`,
  `space::a_point_on_the_edge_of_a_magnified_crop_of_a_crop_lands`,
  `scripts/tests/test-notes.sh` reading `m2-note.md`, and every carry-over row owned by
  milestone 2 closed, re-owned or dropped with a line.

**Keeping tests lean (F1).** Each follow-up's test on files made in the test; no model,
no network.

## What the owner must answer

1. I need `https://generativelanguage.googleapis.com` approved as a recipient for judging
   (the classes `source_images`, `derived_images`, `prompts`, `evidence`: test pictures,
   evaluation-set pictures and squint's questions), and `GEMINI_API_KEY` put by you in the
   env file whizbang reads by `--env-file` (its path is in the builder's brief, never in
   a tracked file): yes/no.
2. I need `https://api.openai.com` approved as a recipient for judging, with the same
   classes, and `OPENAI_API_KEY` put in the same env file by you: yes/no.
3. I need `https://raw.githubusercontent.com` approved as a download origin for the
   evaluation sets' pictures (GETs of the public `wafflery-evalsets` at a pinned
   revision, checked by hash; nothing of yours is sent): yes/no. No means you place the
   pictures in a folder by hand.
4. I need you to run `scripts/baseline/audit.py --count-scaled-cuts` against your copy of
   the baseline package (it prints one number) and, given that number, your yes to
   transcript format 2 for scaled tool images (their size and mapping compared, not
   their bytes): yes/no. A count of 0 needs no answer.
5. I need you to run the baseline gate and its descriptive live rerun on your Mac on E2's
   pushed head, before it is merged (two commands, each printing counts only; about 1
   dollar to Anthropic), and to hand the builder the receipt file it writes: yes/no.
6. I need the allowance of the evaluation store set to 40 dollars for milestone 2's live
   runs (`whizbang spend reset --allowance 40` on that store, by you): yes/no.

## Spend

All on approved recipients, on the owner's Mac, each pull request's settled spend in its
results file and the total in the note.

| Pull request | What is called | Estimate |
|---|---|---|
| A1 | Four use cases once each, Haiku, retries included | 0.50 |
| A2 | `details = first` tried on each use case, Haiku | 0.50 |
| P1 | Five use cases on each of `gemini-3.8-flash` and `gpt-6.1-terra` | 1.00 |
| E1 | Arm F over 110 items, Haiku, twice | 1.50 |
| E2 | The gate: arms C and F over 100 dev items, Haiku three times, Flash and Terra once each; the live rerun of 143 items, Haiku | 18.00 |
| X1 | Nothing paid: SAM 3 on the Mac | 0 |
| M1 | Five use cases over stdio, Haiku; the cache's hits are free | 0.20 |
| F1 | Nothing paid | 0 |
| | **Total** | **about 22 dollars, of the 100** |

The rates are the study's (`docs/baseline/manifest.toml`): about 0.0033 dollars a decision
for Haiku, 0.023 for Flash and 0.017 for Terra on arm C; arm F's opening details are
counted at twice arm C.

## Risks

1. **`align` does not reproduce squint's frames byte for byte.** The gate compares each
   aligned frame's prepared bytes. A2 ports squint's arithmetic in its order and holds it
   with squint's vectors; a difference that remains is a decision brought to the owner
   before E2 runs the gate, as E2's item 6 is, never a silent change to the comparison.
2. **A provider's wire has moved since squint recorded it.** Unknown fields are read as
   opaque, and P1's live calls prove the wire; a field that is now required is a change
   to the adapter with a recorded exchange of its own.
3. **The details mislead a judge.** An unverified measurement put in front of a judge may
   anchor it. Each is labelled with its mark, `--details none` stays one flag away, and
   E2's gate measures the difference before anything is admitted.
4. **The store under 100 runs and more.** Each publication writes the whole generation;
   E1 measures before its matrix and decides as the carry-over row asks, before volume.
5. **A tool file runs with the worker's privileges.** As a user's adapter does; X1's ADR
   and the guide say so plainly, and its reviewer reads it.

### What milestone 3, E and later owe after 2

| Item | Left for |
|---|---|
| Use cases expressed as recipes on the shared mechanism | Milestone 3 |
| Report viewer and export | Milestone 5 |
| `regions`, `classes`, `outline`; per-subject comparison on rungs 3 and 4 | Milestone E |
| A local or self-hosted judge | Milestone 8 |
| The runtime on Linux, on an external GPU host | Its carry-over row (the owner's decision of 2026-11-18) |

## Review rounds

The first outside round, on draft 1, had 11 findings: 3 blockers (a tool inside the
model's one worker asking for a model would wait on itself; the baseline receipt asked
for after the pull request that needs it; a whole matrix reserved at once could not fit
any allowance asked for) and 8 majors (dependencies of E1 and X1; shared files not named;
arm C's projection of `align`; a tool file never offered to a judge; the ledger's closed
epochs; the population of 100 runs of a tool; E1's size with the sets' copy; processes a
tool file starts). All are taken into draft 2. The second, on draft 2, had 5: 1 blocker
(squint's structured jobs had no way through `check`) and 4 majors (a replay fixture that
exercises preparation and tools; how a measurement is scored on rung 2; E2's size; labels
written into the sets' tree by squint's gate code). All are taken into draft 3. The third, on draft 3, had 3, all major
(squint's replay provider cannot make captures from scripted turns; per-subject
comparison's subjects and references; A2's size). None was a blocker. All are taken into
draft 4. The fourth, on draft 4, had 2, both
major (`view`'s form as squint's judge knew it, needed by the replay; too few families in
one set to split). Both are taken into draft 5. The fifth, on draft 5, had 1 major (E2's
size with its generated fixtures and `view`'s form). It is taken into draft 6: the
fixtures counted apart, and the gates E2's planned overflow to F1.
