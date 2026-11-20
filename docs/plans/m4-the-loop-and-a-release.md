# Plan: milestone 4, the loop and a release

Draft 2, 2026-11-19, from `main` at 0c75eedd (the merge of #244, which closed milestone 3;
its note is `docs/plans/m3-note.md`), after the first outside review round ("Review rounds",
at the end). Milestone 4 is `SPEC.md` section 16, row 4, "The loop and a release": "Remove
with a check and bounded retries runs as nested calls on one budget; the `prerelease`
workflow passes and a release archive installs and runs on both targets". Beside the row it
takes every carry-over row that names milestone 4 (the table "Carry-over rows owned by
milestone 4, and who takes each", below) and one row that milestone E owns and the loop
cannot wait for: details first as `check`'s default. It ends with the note of milestone 4.

**Words this plan uses.**

- **The loop** is the action this milestone adds: it removes what a phrase names from a
  picture, asks squint whether the result is good, and tries again a bounded number of
  times. `SPEC.md` section 3 calls this level "Loop across tools" and gives "remove, check,
  retry" as its example.
- **An attempt** is one removal (a mask made and filled) with the check of its result. The
  first attempt is counted, so "three attempts" is one removal and at most two retries.
- **A retry** is every attempt after the first.
- **A nested call** is a call one action makes to another through the executor in its
  context; it is a run of its own under the run that made it (`SPEC.md` section 3, "Nested
  calls").
- **The reservation** is the amount of money the ledger sets aside for a run when it is
  admitted. **The allowance** of a run is what it may still spend of that amount. **The
  call cap** is the config's `spend.call_cap_usd`, the most one call may reserve.
- **A finding** is one item of the judge's checklist that counts against a pass, as `check`
  hands it to its caller: which rule or requirement it is about, how it was answered, and
  the place the judge cited, if it cited one. **A place** is a box or a polygon on a
  picture.
- **The plan of a call** is what preparation settles for a loop before it is admitted: how
  many attempts it may make and whether it checks.
- **An archive** is one compressed file a person downloads to install whizbang. **A
  target** is one of the two platforms of `SPEC.md` section 5: `aarch64-apple-darwin`
  (macOS 15 on Apple silicon; CI's runner `macos-15`) and `x86_64-unknown-linux-gnu`
  (Ubuntu 22.04 on x86-64; CI's runner `ubuntu-22.04`).
- **A candidate** is a commit tagged as a release candidate, and the archives built from it.
- **The local leg** is the part of the pre-release checks that the orchestrator runs on the
  owner's Mac, because it needs a real model or a provider's key. **The receipt** is the
  file the local leg writes, which the release reads.

## What the spec says, and where it is silent

**What it says.** Each of these binds the tasks below.

- Section 3, "Primitives, recipes and loops": actions come at three levels, and the third is
  the "Loop across tools", whose example is "remove, check, retry", called by "Agents and
  apps". "A recipe that spans tools lives with the tool that owns the goal:
  remove-then-check is rascal's." "A tool that needs an edit evaluated calls squint's
  checking recipe; it does not choose squint's tools."
- Section 3, "The execution contract": prepare "returns readiness for these exact arguments
  and a bounded estimate", and "Its result is bound to the digests of the inputs, the config
  and the model identities it saw"; admit "Revalidates a prepared call against those
  digests", and "A stale preparation is refused".
- Section 3, "Nested calls": a tool reaches another only through the executor in its
  context, "which passes down a child allowance and refuses calls deeper than four levels or
  back into an action already on the stack".
- Section 3, "Cancellation": "Cancelling a run cancels its descendants and blocks new ones.
  A parent becomes terminal only when its descendants are terminal and every dispatched
  request is accounted for."
- Section 3, "What a run publishes": "A child action that completed keeps its committed
  outputs even if its parent later fails; they are recorded as produced under a failed
  parent." Diagnostic evidence "can never be used as an output".
- Section 3, "What every run records": "Every recipe step, tool call and nested call, in
  order, with its arguments and a reference to its result"; for a refusal inside a run,
  "what was refused and why"; for every paid request, what was reserved and settled.
  "Metrics from ordinary runs" asks `runs stats` for "retries and repairs".
- Section 6, "Spend": "Admission is atomic. A call reserves its bound before any request is
  sent." "Nested calls draw on the parent's reservation; nothing is reserved twice."
  "Retries belong to the layer that issued the request, and each is admitted like any other
  attempt." Spend control "errs toward letting work run".
- Section 7, "Rascal": "Bounded check and retry | Kept, as nested calls | Milestone 4".
  "Doors and releases": "Release archives for both targets of section 5 | Kept | Milestone
  4".
- Section 5, "Platforms": the two targets, and that on Linux "the pack's core installs and
  the extra is refused", so no local model runs there.
- Section 13: the slow tier holds "a live call to each provider; install from a release
  archive", run "before a release".
- Section 14, "Workflows": the skill `release`, "A version release", exists from milestone
  4. "GitHub Actions": `prerelease` runs "On a release candidate tag" and contains "Model
  tier; a live call to each provider; install from the built archive on both targets. A
  release is refused unless this passed on the same commit". `release` runs "On a version
  tag" and contains "Binaries for both targets and the pack".
- Section 15: agents may "tag, cut version releases", "provided CI and both scans pass
  first"; a new recipient for data is the owner's.

**Where it is silent, or disagrees with what was built.** Each is an item under "Open
questions" or "What the owner decided"; none is settled silently.

1. The spec does not name the loop, say how many attempts it makes, what a retry changes,
   which of squint's use cases checks a removal, what is handed back when every attempt
   fails, or what happens when no judge is ready. Rascal decided all of these (its ADR 0023,
   "A bounded check-and-retry for remove", and `crates/rascal-mask/src/heal.rs`,
   `retry.rs`, `checker/` at the commit `docs/sources.toml` pins). Section 7 says the item
   is "Kept", so this plan takes rascal's rules as the recommended answers, narrowed where
   the review found them unsafe; the old repos are "reference, never authority"
   (`AGENTS.md`), so each is asked.
2. A recipe file (`recipe/1`, ADR 0033) is a straight sequence of steps: it cannot branch
   or repeat. The spec does not say whether a loop is a file or code.
3. Rascal's retry is aimed at the places its checker named. In whizbang a verdict handed to
   a caller is a decision and a reason (`contract::Verdict`); the places are only in the
   diagnostic `judged.json`, and a diagnostic can never be used as an output.
4. Calls nest four levels. The loop, squint's use case recipe, its step `check` and a tool
   the judge calls are four, so the loop fills every level and nothing may call the loop.
5. The spec does not say what the loop does when its reservation, which covers every
   attempt, is more than the call cap. Three attempts with one of the dearer judges reserve
   about 3.02 US dollars, and the config guide's examples cap a call at 2.00 or 2.50. And
   preparation as built cannot fit a call to a cap or leave out a capability that is not
   ready: an action states fixed needs, which apply by whether an argument is given and by
   nothing else (`contract/descriptor.rs`, `When`; `contract/prepare.rs`).
6. Section 14 puts the model tier and a live call to each provider inside a GitHub
   workflow. `AGENTS.md` says the model tier runs "never in CI"; no provider key is held by
   GitHub; a hosted runner cannot hold SAM 3; and every live run so far has been the
   orchestrator's on the owner's Mac.
7. Section 14 says a release contains "the pack". Since checkpoint 1C the pack travels
   inside the binary (ADR 0021; `crates/whizbang/src/pack_setup.rs`), so there is no second
   thing to release.
8. Row 4 says the archive "runs on both targets", and section 5 leaves Linux with no local
   model. The spec does not say what "runs" means there.
9. The spec does not say where archives are published, how they are named, how they are
   checked after download, or whether the Mac's binary is signed and notarised by Apple.
   The owner has decided the first and the last ("What the owner decided").

## What the owner decided

On 2026-11-19 the orchestrator put three of draft 1's open questions to the owner, who
answered "1, 2, 3 good". They are settled, and the tasks below build on them.

1. **The first version is `0.1.0`**, with candidates `v0.1.0-rc.<n>`, published on the
   repo's GitHub releases and marked there as a pre-release.
2. **Hosting SAM 3's files moves to milestone 6.** The first release fetches the model from
   its maker with the user's own sign-in, as today; the model's licence is read for what
   redistribution asks in this milestone (E1, item 6).
3. **No notarisation by Apple in this milestone.** The Mac's binary ships unsigned by a
   developer identity, the guide gives a download command that sets no mark on the file,
   and notarisation gets a carry-over row.

## How the work is split

Eight pull requests, where the owner asked for few and large. Sizes are changed lines of
source, tests and hand-written documents, by the rate milestone 3 measured (about 2,500 for
a large pull request); generated reference is counted apart, as in milestone 3.

| Pull request | What | Builds on | Size |
|---|---|---|---|
| A1 | Details first is no longer `check`'s default; and the rows left: the lab's progress and counts, the pairs page, an executor built with a digest alone | `main` | Large: about 2,300 (the default 700, the rows 1,600) |
| K1 | What the loop stands on, in the core and in `check`: a call's plan settled at preparation; a recipe run from a named step; findings as a kind of result, the same on a cache hit; the record's new version | `main` | Large: about 2,900 (the plan of a call 750, the runner 450, findings 750, the record 350, tests 600); its seam below |
| L1 | The loop: `remove_checked`, its attempts, its one reservation, which findings may widen a mask, cancellation, and what it hands back | A1, K1 | Large: about 2,500 (the action 900, the command and the server 300, the guide 150, `runs stats` 150, tests 1,000) |
| L2 | The located retry: a retry aimed at the places a finding named; a mask carried from a crop to its picture; the masks kept within a bound | L1 | Large: about 2,000 (the carry 450, the retry 700, tests 850) |
| R1 | The archive, its checksums, the bootstrap of what a user must have, the install check, and all of it in CI's `release` job on each runner | `main` | Large: about 2,200 (scripts 650, their tests 750, the licences of what the binary holds 250, the install guide 200, CI 100, the binary's version line 250) |
| R2 | The `prerelease` and `release` workflows, the local leg and its receipt, the rule that refuses a release, and the skill `release` | R1 | Large: about 2,300 (workflows 300, the local leg 450, the receipt and the gate 450, the skill and its listings 300, tests 800) |
| E1 | The evidence before the candidate: the loop run live by a committed script, three cases by hand, the loop's record, the rows left to the orchestrator | all of the above | Medium: about 1,300 (the script and its tally 400, its tests 300, the record and results 600) |
| N1 | After the candidate: the release's record and the note | E1, and a candidate that passed | Small: about 500, no code |

**Waves.** Five, each started from `main` once what it builds on has landed.

1. **A1, K1 and R1** from `main`, side by side.
2. **L1** when A1 and K1 have landed; **R2** when R1 has landed.
3. **L2** when L1 has landed.
4. **E1** when the other six have landed. The first candidate is tagged on `main` at E1's
   merge.
5. **N1** when a candidate has passed.

**What can run side by side, and what cannot.** The loop's strand (A1, K1, L1, L2) and the
release's strand (R1, R2) share no source file but the binary's command list, and run side
by side from start to end. Inside the loop's strand the order is fixed: everything L1 reads
(the plan of a call, a run from a named step, findings, the record's version) is K1's and
lands first, so no later task changes a contract another is building on.

**Shared files, each with one owner.** The run record
(`crates/whizbang-core/src/contract/record.rs` and its schema) and the kinds of result
(`contract/descriptor.rs`, `ResultKind`; the description's version): K1 alone. K1 writes the
whole of `run/6`, the fields L1 and L2 fill among them, so the version changes once.
Preparation and admission (`contract/prepare.rs`, `engine.rs`): K1. The recipe runner
(`crates/whizbang-core/src/recipe/runner.rs`): K1; L1 and L2 only call it.
`crates/whizbang-squint/src/check.rs` and `cache.rs`: K1; A1 does not touch them (the
default it changes is in the five recipe files and in the command).
`.github/workflows/ci.yml` and the `justfile`: R1 in wave 1, R2 in wave 2. `AGENTS.md`,
`.claude/skills/` and the tests that hold them: R2 alone. The generated reference
(`docs/reference/`) is made again by `just docs` after merging `main`, never merged by hand;
in each wave the second to land merges `main` (a merge commit), runs `just docs` again and
reruns its checks.

**Seams.** Each pull request names where it splits if it passes about 2,500 changed lines,
into a second pull request of its own that lands before anything that builds on it: A1, the
executor built with a digest alone (its item 10); K1, `check`'s side of findings and the
cache (its item 3, after the kind itself and the record have landed with the first); L1,
`runs stats` and the server's side (its items 10 and 11); L2, none: the carry is its first
commit and the retry is not split from its bound; R1, the licences of what the binary holds
(its item 3); R2, the skill `release` (its item 6). A split is said in the hand-back and the
owner is told the count. Nothing of the row is dropped to keep the count.

**Review rule.** Every pull request is reviewed whole by a reviewer that did not write it,
as `AGENTS.md` says: the branch and its pull request read against the spec and the task,
`just check` and the range scan run by the reviewer, the reviewed head named, each finding
a blocker or a follow-up. Nothing below narrows that. Beside it, each task names what its
reviewer looks at hardest: A1, that the new default is the arm the gate measured; K1, that
a plan cannot be changed between preparation and admission, and that a finding is data,
bounded and held, never instruction; L1, nested calls under one reservation and what is
sent to a judge; L2, that no retry paints where no finding of the target pointed; R1 and
R2, what ships in the archive, the token a workflow runs with and the one thing it may
write, and every line of the change to `AGENTS.md` and the skills; E1, that the record
holds aggregates only. Each is merged on green CI, with `just scan-range` over
`base..head`. A "Done when" is a list of tests by name.

**Test time.** The limit is 300 seconds of test time on every runner and on the owner's Mac
(`SPEC.md` section 13); the hosted macOS runner has taken 60 to 260 seconds. Milestone 4 may
add at most 25 seconds there, about 3 seconds a pull request and 8 for L1. Every test that
calls a provider, a model or the network beyond loopback is `#[ignore]`d and run by hand or
by `just test-models` on the Mac, never by CI. A loop in a test runs over the fake
segmenter, a scripted filler and a scripted judge, on synthetic pictures of 64 by 64,
through the production path. Each pull request reports its `just check` and CI test times,
and stops and says so if a run passes about 250 seconds.

**Live runs are the orchestrator's.** A builder makes no provider call and loads no real
model in CI. Every live run this plan asks for is made by the orchestrator on the owner's
Mac, by the committed script or the exact command the task names, and its record is
committed by the orchestrator or by the builder from what the orchestrator gives it.

**Spend.** The owner's standing allowance is 100 US dollars a milestone; milestone 4 has
spent nothing. The live runs come to about 22 dollars, itemised under "Spend".

## A1. Details first is no longer `check`'s default; and the rows left (large)

### Goal

"Details first" means that before the judge's first turn, `check` runs `align` (and any
other tool named with `--detail`) on each pair of images the use case relates, and opens the
conversation with what they measured (ADR 0029). Its gate rejected it twice on 2026-11-19
(`docs/results/m3-gates.md`): one judge did worse with it and one did not change. It is
still the default of `whizbang squint check` and of the five use case recipes. The
carry-over row gives it to milestone E, "first thing"; this plan takes it now, because L1's
loop calls squint's checking recipe on every attempt and would otherwise pay for, and be
judged under, a setting its own gate refused. Nothing about details first is removed: it
stays one flag away. With it, in the same pull request, go the carry-over rows that name
milestone 4 and belong to neither strand.

1. **The recipes.** In each of the five files under `crates/whizbang-squint/recipes/`, the
   step `details` says `details = "none"`. The step stays, so `--set details.details=first`
   and the command's `--details first` select it for one run. A step that measures nothing
   sends nothing and starts no tool.
2. **The command.** `whizbang squint check --details` defaults to `none`. With `none` the
   judge is offered exactly what the gate's control arm (arm C) offered, as
   `door::Details::None` gives it today, so the new default is the arm that was measured,
   not a third thing.
3. **The server.** A use case called over MCP without `details` measures none.
4. **Evaluation is unchanged.** A matrix's arm states its own details
   (`crates/whizbang/src/evaluation.rs` passes `arm.details()`), so every committed matrix
   and gate means what it meant; a test holds that an arm F cell still measures first.
5. **The records.** `check`'s record and the five use cases' records
   (`docs/reference/tools/`) say that details first is off by default, name the gate that
   rejected it, and say how to turn it on; their worked examples are made again under the
   new default. The recipes' digests change, so the decision cache misses once for each
   job, which is what the cache's key is for.
6. **The row.** The carry-over row "Details first is still `check`'s default" is closed
   with a line. Fixing details first and gating it again on fresh data stays milestone E's,
   in the row on `tiles` and the second look.
7. **`whizbang lab run` says what it is doing.** Each cell is said on the error stream as it
   starts and as it ends, so a round of a hundred fills is not silent for many minutes.
8. **"Started now" counts only what a leg started.** A request the store had already
   answered is printed apart, with a test of a resumed round whose earlier attempts are
   answered as repeats.
9. **The pairs page.** Each picture of a pair is written so that its size on disk does not
   tell its arm, and a backspace takes back the last pick. Milestone E's picking rounds are
   drawn again from the kept stores after this lands.
10. **An executor built with a digest alone** (`Engine::over`) is compiled for tests alone,
    as `OneWorker::starting` is, or backed by one walk of the config, as the row allows.

- [ ] Done when:
  `use_case::each_shipped_recipe_measures_no_details_unless_a_run_asks`,
  `cli::squint_check_measures_no_details_by_default_and_first_when_asked`,
  `check::the_default_offers_the_judge_what_the_gates_control_arm_offered`,
  `serve::a_use_case_called_without_details_measures_none`,
  `evaluation::an_arm_states_its_own_details_whatever_the_default_is`,
  `use_case_records::each_use_case_record_holds_two_worked_examples_that_its_recipe_gives`
  (on the records made again),
  `evidence::the_record_of_check_says_details_first_is_off_and_names_its_gate`,
  `lab::each_cell_is_said_as_it_starts_and_as_it_ends`,
  `lab::a_resumed_round_counts_only_what_it_started_and_prints_repeats_apart`,
  `pairs_page::a_pictures_size_on_disk_does_not_tell_its_arm`,
  `pairs_page::a_backspace_takes_back_the_last_pick`,
  `asked::an_executor_built_with_a_digest_alone_is_a_tests_alone` (or the test of the one
  walk, as the builder chose), `scripts/tests/test-features.sh` and
  `scripts/tests/test-crate-graph.sh` passing with the new feature, `just docs`
  regenerated, and the five carry-over rows closed.

**Keeping tests lean (A1).** No new picture; each test of the default is a scripted judge on
the pictures the use cases' tests already draw.

## K1. What the loop stands on (large)

### Goal

Four additions below the loop, none of them a loop, each needed before L1 can be written as
this plan describes it. The first review round found that draft 1 promised behaviour at
preparation that no action can give today, and put the loop before the things it reads.
They are one task so that the record's version, the kinds of result and the rules of
preparation each change once, in one place.

1. **A call's plan, settled at preparation.** Today an action states fixed needs, which
   apply by whether an argument is given and by nothing else, and preparation multiplies
   each need's bound by a fixed count of requests and calls the call not ready when any
   capability is missing. A loop needs more: to make fewer attempts when the call cap
   covers fewer, and to go on without a judge when none is ready. So an action may
   **plan**: given the call's arguments, the bound of one request of each capability it
   names, which of them are ready, and the config's call cap, it states the plan of this
   call, which here is how many attempts it may make and whether it checks, and why. The
   plan decides the needs preparation resolves (a call planned without a check needs no
   judge and reserves nothing for one) and the estimate.
   - **It is settled once.** The plan is part of the prepared call and of its fingerprint,
     beside the digests of the inputs, the config and the model identities.
   - **Admission holds it.** Admission prepares the call again, as it does today, and
     refuses a preparation whose plan is no longer the one that would be made (a judge's
     key that has appeared or gone, a call cap that changed) as it refuses any stale
     preparation. No plan is fitted at admission.
   - **The run reads it.** An action is handed its plan with the prepared call and never
     works it out again, so what ran is what was reserved for.
   - **A repeated request id** answers with the run its first admission made, under the
     plan that run had, as for any call.
   - An action that plans nothing is prepared exactly as today. ADR 0004 is amended, and
     the description says which actions plan.
2. **A recipe run from a named step.** The runner gains `from`, the mirror of `until` (ADR
   0033, amended): a run names a step and gives artifacts in place of the outputs of the
   steps before it; those steps do not run, nothing is reserved for them, and the run's
   record says where it started and what it was given. `from` and `until` may be given
   together, so a caller can run one stretch of a recipe: this is how the loop gets a mask
   before any fill, and fills only when it has compared it (L1, item 2). It is refused
   before anything runs when the recipe has no such step, when `from` comes after `until`,
   when a later step needs an output that was not given, or when a given artifact is not of
   the port's type.
3. **Findings, a third kind of result** beside the verdict and measurements (ADR 0035,
   amended; a new description version): one JSON document, at most 32 KiB, that lists each
   checklist item the verdict counts against a pass. For each: the obligation it is about,
   by its id (a rule of the use case, such as `SF1`, or a requirement of the job, `RQ<n>`);
   how it was answered; and, when the judge cited one, its place: the image it is on and a
   box or a polygon in fractions of that image.
   - It is made from what `check` already keeps in `judged.json`, by the same code, so the
     two cannot differ. A recipe whose last step declares findings hands them to its
     caller with the verdict.
   - **A finding is data from a model.** At most eight places are handed back and the rest
     counted; a place on an image the run was not given is left out and counted; numbers
     are held inside the picture; no text of the judge's travels with a finding. The
     executor refuses findings from an action that does not declare them, as it does a
     verdict.
   - **The same on a cache hit.** A run of `check` answered from the decision cache hands
     back the findings the earlier run handed back. They are worked out again from the
     `judged.json` the hit holds, by the same code, and held by the same rules. An entry
     written before this change that kept no `judged.json` is not answered from: the check
     runs, and the new entry replaces it. So two identical checks give a caller identical
     findings whether or not the second was answered from the cache.
4. **The record's new version, `run/6`** (by the rule of ADR 0005, with its schema and
   fixed vectors, reading `run/5`), written whole here so that it changes once: the plan of
   a call; where a recipe's run started and what it was given; the findings a run handed
   back, as counts by obligation and the places; and a loop's attempts, the list L1 and L2
   fill: for each attempt its number, the runs it made in order, the verdict or the reason
   there is none, what the retry changed to make it (a wider `grow`; the crops cut and the
   masks they gave), and what its requests settled; then which attempt was kept and why
   the attempts ended. K1 holds the list by its schema and by a fake action that fills it;
   L1 and L2 add no field.

- [ ] Done when:
  `prepare::an_action_that_plans_is_prepared_with_the_needs_and_the_estimate_of_its_plan`,
  `prepare::a_plan_fitted_to_the_call_cap_says_what_was_asked_and_what_it_allows`,
  `prepare::a_capability_the_plan_leaves_out_is_not_resolved_and_nothing_is_reserved_for_it`,
  `prepare::the_plan_is_in_the_fingerprint_of_the_prepared_call`,
  `admit::a_preparation_whose_plan_would_now_differ_is_refused_as_stale`,
  `admit::a_repeated_request_id_answers_with_the_run_and_the_plan_of_its_first_admission`,
  `executor::an_action_is_handed_the_plan_it_was_prepared_with`,
  `prepare::an_action_that_plans_nothing_is_prepared_as_before`,
  `cli::a_planned_call_prints_its_plan_before_it_runs` and
  `serve::a_planned_call_answers_with_its_plan` (each on a fake action that plans),
  `runner::a_run_from_a_step_runs_that_step_and_those_after_it_with_what_it_was_given`,
  `runner::a_run_from_one_step_until_another_runs_that_stretch_alone`,
  `runner::a_run_from_a_step_reserves_nothing_for_the_steps_before_it`,
  `runner::a_run_from_a_step_that_lacks_an_output_a_later_step_needs_is_refused_before_anything_runs`,
  `check::the_findings_handed_back_are_those_judged_json_holds_each_with_its_obligation`,
  `check::a_place_on_an_image_the_run_was_not_given_is_left_out_and_counted`,
  `check::no_more_than_eight_places_are_handed_back_and_no_text_of_the_judges`,
  `check::a_check_answered_from_the_cache_hands_back_the_findings_of_the_run_that_answered`,
  `cache::an_entry_that_kept_no_judged_document_is_not_answered_from`,
  `executor::findings_from_an_action_that_does_not_declare_them_end_the_run_as_an_error`,
  `use_case::a_use_case_recipe_hands_its_caller_the_findings_of_its_last_step`,
  `describe::the_description_lists_findings_among_an_actions_results_and_which_actions_plan`,
  `record::run_6_holds_a_plan_a_start_findings_and_a_loops_attempts`,
  `record::run_6_reads_a_record_of_run_5`, and `just docs` regenerated.

**Keeping tests lean (K1).** Planning is tested on a fake action with fake backends whose
bounds a test states; findings are read from scripted submissions; the runner's tests use
the fake segmenter.

## L1. The loop: remove, check, retry (large)

### Goal

One action that an agent or an app calls to remove what a phrase names and to know whether
the result was checked (`SPEC.md` section 3, "Primitives, recipes and loops"; section 7,
"Bounded check and retry"; section 16, row 4):

```
whizbang rascal remove-checked --image <id> --prompt "leather collar"
        [--attempts <1..3>] [--check auto|required|off] [--protect <id>]
        [--backend <segmenter>] [--filler <filler>] [--judge <judge>] --store <dir>
```

L1 builds the loop whole, with the simplest retry (a wider mask). L2 adds the retry aimed
at the places a finding named.

1. **The action.** `remove_checked`, of the kind `Loop` (`contract::Kind::Loop`, which the
   descriptor already has and no shipped action yet uses), in `whizbang-rascal`, because
   the goal is rascal's (`SPEC.md` section 3). It is written in Rust, not as a recipe file:
   `recipe/1` has no branch and no repeat, and this plan adds neither (open question 8). A
   new ADR, "The loop: remove, check, retry", records every decision of this task. Its
   inputs are `remove_prompt`'s (an image, a phrase, an optional mask to protect, the
   segmenter, the filler) and the judge. `remove_prompt` itself is unchanged: one pass,
   unchecked.
2. **An attempt, call by call.** The loop never calls `remove_prompt` whole, because the
   recipe fills before it hands anything back, and the loop must see a mask before it pays
   for a fill. It runs the shipped recipe in stretches (K1, item 2), each a nested call
   with its own run and record, and the recipe's own steps and settings do all the work:
   - **Find.** `remove_prompt` until its step `select`: `segment` finds the instances and
     `select` joins them. The loop is handed one mask, **the found mask**. (First attempt
     only; a retry makes its found mask as item 5 or L2 says.)
   - **Shape.** `remove_prompt` from `grow` until `feather`, given the found mask: the
     loop is handed **the shaped mask**, the one a fill would paint.
   - **Compare.** A retry whose shaped mask has the identity of the attempt's before it
     would repeat that attempt: it is not filled, and the attempts end.
   - **Fill.** `remove_prompt` from `fill`, given the shaped mask: the filled picture and
     the mask rebound onto it.
   - **Check.** Squint's checking recipe `source-fidelity`, with the filled picture as the
     candidate and the picture the loop was given as the source, under the job of item 3.

   The source of every attempt is the picture the loop was given, never an earlier
   attempt's result, so no retry paints over a fill.
3. **The job the check is given.** Written by the loop from the phrase, by a fixed,
   versioned template whose digest is in the record (`job/1`, ADR 0028). Its brief says
   what was to be removed. Its requirements are three, with fixed ids: `RQ1`, the named
   thing is no longer visible; `RQ2`, nothing else in the picture changed; `RQ3`, the
   filled area looks like its surroundings. The loop calls the recipe and nothing under
   it: it does not choose squint's tools (`SPEC.md` section 3). The check receives what it
   needs (its images, its job, its judge and its share of the allowance) as a nested
   call's arguments, not through the environment of a second program, which is what the
   carry-over row "A check cannot receive the variables it needs" asks.
4. **Which findings may make a mask larger.** A retry paints more of the picture, so it is
   allowed only by a finding that says the named thing is still there: a finding about
   `RQ1`, and no other. A finding is classed by the obligation it cites and by nothing in
   the judge's words.
   - A `fail` or an `unsure` whose findings are all about `RQ1`: a retry (item 5; L2).
   - A verdict with any finding about something else that must hold (a protected subject,
     `SF1`; the scene's geometry, `SF2`; `RQ2`): **the attempts end**. Such a finding says
     the removal damaged something, and painting more cannot mend that. The loop hands
     back its best attempt with the verdict (item 8).
   - A finding about `RQ3` alone, or one that is only noted (`SF4`), neither allows a
     retry nor forbids one that `RQ1` allows; alone, it ends the attempts, since the same
     mask would be filled again.
5. **What a retry changes, in L1.** After a `fail` whose finding about `RQ1` names no
   place: the shape stretch is run again from the same found mask with its step `grow`
   given twice the `px` (the runner's own override, `set`), once in a run: rascal's "a
   `fail` that names no place widens the mask once". After an `unsure` whose finding about
   `RQ1` names no place, the attempts end: rascal found that widening after such an answer
   "repeated the same fill at the same price". Until L2 lands, a finding that names a place
   is treated as one that names none. Nothing else changes between attempts: the same
   phrase, segmenter, filler and judge.
6. **What ends the attempts.** A verdict of `pass`. The last attempt the plan allows. A
   verdict item 4 says ends them. An `unsure` that names no place. A check that could not
   run. A retry whose shaped mask is the one before it. A retry whose mask could not be
   made or filled: the attempt before it is kept.
7. **One budget.** The loop plans its call (K1, item 1). Its plan is the number of attempts
   and whether it checks. For each attempt allowed the bound holds one request of the
   filler and one run of `source-fidelity`; the segmenter is asked once in the first
   attempt (L2 adds its own). Admission reserves the plan's bound once, before any request
   is sent (`SPEC.md` section 6). Each nested call takes its share from the loop's
   allowance and reserves nothing of its own (ADR 0008, "Allowances"); what a child did not
   spend comes back to the loop when the child ends (ADR 0016, "An allowance holds a signed
   balance"), so the reservation covers every attempt at its bound and, when nothing
   overruns, cannot run out part-way.
   - **How many attempts.** At most `attempts`, from 1 to 3, 3 by default, the first
     included: rascal's limit, kept (open question 4).
   - **When the bound is over the call cap.** The plan is the most attempts, up to the
     number asked, whose whole bound is within `spend.call_cap_usd`, and it says how many
     were asked and how many the cap allows. When the cap covers a removal and no check,
     the plan is one attempt, unchecked, with the reason "the call cap does not cover a
     check". When it does not cover one removal, the call is refused as any call over its
     cap is. This is rascal's rule ("Tries are reserved up front within the spending
     limit") and the spec's lean toward letting work run; the other answer, to refuse the
     call and name the cap, is open question 7.
   - **When the store has less left than the bound.** Admission is refused, as for any
     call.
   - **When the allowance runs out in the middle.** This needs an overrun: a request
     charged over its bound puts the runs above it in debt (ADR 0016). The next nested call
     is then refused for want of allowance (`Refusal::OverCaller`), and the refusal is in
     the loop's record. The loop starts no further call, keeps the attempts that completed,
     and ends as item 8 says, with the reason "the budget ran out after attempt N". When
     the refusal comes before the first fill has completed, the loop ends as an error of
     the type `budget`, with the refusal's own sentence.
   - **How many runs.** An attempt is at most four calls of the loop's own in L1, and each
     is a recipe whose steps are runs too; with the check's tool calls, three attempts stay
     under 200 runs of the 1,024 a tree may hold, and under the 256 calls one run may make.
     A test holds the count at its largest.
   - **The guide gives real figures** (carry-over row "The documented per-call cap for a
     check is one cent"): `docs/guides/loop.md` states the bound of one attempt and of
     three for each judge in the catalogue, worked from the ceilings the config guide
     already gives: a check reserves 0.112768 dollars with `claude-haiku-5-5`, 0.72576 with
     `gemini-3.8-flash` and 0.96768 with `gpt-6.1-terra`, and a fill 0.04; so three attempts
     reserve about 0.46, 2.30 and 3.02 dollars, and what is settled is far less.
8. **What is handed back.** The loop completes whenever at least one fill completed, and
   commits one attempt's outputs as its own: the filled picture and the mask rebound onto
   it. Which attempt is kept: the first that passed; failing that, an `unsure` before a
   `fail`, and either before an attempt whose check could not run; then the attempt with
   fewer findings; then the earlier one. With the outcome the caller is handed the kept
   attempt's verdict and findings, when it has them, and the loop's plan and a short result
   that says whether the kept attempt was checked, why not when it was not, how many
   attempts were made, which was kept, and why the attempts ended. So **when every attempt
   fails, the loop still completes**, hands back the best attempt it made, and says `fail`
   or `unsure` with it: the caller decides what to do with a picture the judge did not pass
   (open question 6). The loop ends as an error only when no fill completed: the cause is
   the own error type and message of the call that failed.
9. **Unchecked, and saying so.** `--check auto`, the default: when no judge is ready (none
   configured, or its key not in the environment), or the call cap does not cover a check,
   the plan is one attempt, unchecked, with the reason: rascal's "A missing checker key
   never blocks the remove". `--check required`: the same call is not ready at prepare,
   naming what is missing. `--check off`: one attempt, no judge resolved, nothing reserved
   for one. A check that was started and ended in error leaves its attempt unchecked, with
   the check's own error type and message in the record (carry-over row "Keep the cause
   when a check fails"), and the attempts end. On the terminal, the plan is printed before
   anything runs, each attempt is said on the error stream as it starts and ends, and the
   last lines say whether the result was checked, the verdict, and the reason when it was
   not (carry-over row "Say on the terminal when a run was not checked").
10. **The doors.** `describe` lists the loop after the recipes and before the primitives
    (the description already orders by kind). `whizbang serve` offers it as a tool, and its
    answer carries the verdict, the plan and the short result of item 8. Exit codes follow
    `whizbang squint check`'s for a verdict, and the builder states them in the guide.
11. **Metrics.** `whizbang runs stats` over the loop counts its runs by how the attempts
    ended, the attempts made, and the retries (`SPEC.md` section 3, "retries and repairs").
12. **Cancellation.** The loop looks at its signal before each nested call and starts no
    other once it is set; cancelling the loop cancels the child that runs (`SPEC.md` section
    3). A cancelled loop commits no outputs. The attempts that completed before it are in
    its record, each child's outputs "recorded as produced under" the cancelled loop. Those
    pictures are names of the loop's own tree and no later call can take one (ADR 0008; plan
    m3, task Q1, item 3): open question 9 asks whether that is enough for now.
13. **Levels.** The loop is at the first level, `source-fidelity` at the second, its step
    `check` at the third, and a tool the judge calls at the fourth, the last there is. So a
    recipe that names a loop as a step is refused when it is read, with a sentence saying
    why, instead of failing deep inside a paid run (open question 8).

- [ ] Done when:
  `checked::a_first_attempt_that_passes_is_one_find_one_shape_one_fill_and_one_check`,
  `checked::no_fill_is_asked_before_its_shaped_mask_is_in_hand`,
  `checked::a_fail_about_the_named_thing_with_no_place_widens_once_and_is_checked_again`,
  `checked::a_retry_whose_shaped_mask_is_the_one_before_is_not_filled`,
  `checked::a_finding_about_a_protected_subject_ends_the_attempts_and_nothing_more_is_painted`,
  `checked::a_finding_about_the_scenes_geometry_ends_the_attempts_and_nothing_more_is_painted`,
  `checked::a_finding_about_the_named_thing_beside_one_about_anything_else_that_must_hold_ends_the_attempts`,
  `checked::a_finding_is_classed_by_the_obligation_it_cites`,
  `checked::an_unsure_that_names_no_place_ends_the_attempts`,
  `checked::the_attempts_never_pass_the_plans_number`,
  `checked::when_every_attempt_fails_the_loop_completes_with_the_kept_attempt_its_verdict_and_findings`,
  `checked::the_kept_attempt_is_the_first_pass_then_unsure_then_fewer_findings_then_the_earlier`,
  `checked::a_retry_that_cannot_be_filled_keeps_the_attempt_before_it`,
  `checked::a_first_fill_that_fails_is_the_loops_error_with_the_calls_own_cause`,
  `checked::a_check_that_ends_in_error_leaves_its_attempt_unchecked_with_the_checks_own_error`,
  `checked::a_judge_that_is_not_ready_plans_one_attempt_unchecked_and_the_reason`,
  `checked::check_required_is_not_ready_at_prepare_naming_what_is_missing`,
  `checked::check_off_resolves_no_judge_and_reserves_nothing_for_one`,
  `checked::the_job_the_loop_writes_is_valid_and_holds_its_three_requirements`,
  `checked::the_check_is_squints_recipe_and_the_loop_calls_nothing_under_it`,
  `checked::every_pixel_outside_the_kept_mask_is_the_pictures_own`,
  `budget::the_loop_reserves_once_for_its_plan_and_each_child_draws_on_it`,
  `budget::what_a_child_did_not_spend_is_back_before_the_next_attempt`,
  `budget::the_plan_is_the_most_attempts_the_call_cap_covers_and_says_asked_and_allowed`,
  `budget::a_cap_that_covers_no_check_plans_one_attempt_unchecked`,
  `budget::a_judges_key_that_goes_between_prepare_and_admit_is_a_stale_preparation`,
  `budget::a_child_refused_for_want_of_allowance_ends_the_attempts_and_keeps_what_completed`,
  `budget::three_attempts_at_their_largest_stay_inside_a_trees_runs_and_a_runs_calls`,
  `checked::a_cancelled_loop_cancels_its_running_child_starts_no_other_and_commits_nothing`,
  `checked::attempts_completed_before_a_cancellation_are_in_the_record`,
  `record::a_loops_record_lists_each_attempt_its_runs_its_verdict_and_what_the_retry_changed`,
  `nested::the_deepest_call_of_a_checked_removal_is_at_the_fourth_level_and_is_admitted`,
  `recipe::a_recipe_that_names_a_loop_as_a_step_is_refused_when_read`,
  `describe::the_loop_is_listed_after_the_recipes_and_before_the_primitives`,
  `serve::the_loop_is_offered_as_a_tool_and_answers_with_its_plan_its_verdict_and_whether_it_was_checked`,
  `cli::rascal_remove_checked_prints_its_plan_each_attempt_and_whether_the_result_was_checked`,
  `stats::runs_stats_counts_a_loops_attempts_and_retries`,
  `guides::the_loops_guide_states_the_bound_of_an_attempt_for_each_judge_of_the_catalogue`,
  `model_tier::remove_checked_makes_its_mask_with_sam3_under_a_scripted_judge` (`#[ignore]`,
  under `just test-models`), `just docs` regenerated, and the four carry-over rows named
  above closed with a line each. The loop's tool record is E1's.

**Keeping tests lean (L1).** One fixture builds the loop over the fake segmenter, a scripted
filler and a scripted judge whose verdicts and findings a test lists in order; pictures of
64 by 64; no test waits on a clock. **Every kind of retry and every way the attempts end is
shown here, with fakes, and nowhere else:** the live step (E1) only reports what happened.

### Origins (L)

None new. The loop sends what `remove_prompt` and `check` already send, to the recipients
already approved: the filler (`https://fal.run`) and the configured judge.

## L2. The located retry (large)

### Goal

A retry aimed at what the check saw, rascal's second kind of retry (its `retry.rs`): when a
finding says the named thing is still there and names where, the next attempt looks for it
again inside that place, adds what it finds to the mask, and fills again.

1. **A mask carried from a crop to its picture.** `view` cuts a crop bound onto its source,
   and `segment` on the crop gives masks that belong to the crop. Using one on the picture
   "needs an explicit, checked rebind" (`SPEC.md` section 3, "Identities"; ADR 0002). The
   carry is ported from rascal (`crates/rascal-ops/src/invert.rs`, `map_plane_to_parent`,
   by `port-module`): a mask on a crop becomes a mask on the picture the crop was cut from,
   empty outside the crop, refused when the crop is not of that picture. It is offered where
   rascal offered it, as `invert`'s `to`, and to the loop in the form the ADR of L1 is
   amended to state. With it the crop half of the carry-over row on `invert`'s `to` closes,
   on rascal's vector of a crop; the half about a cut canvas is re-owned with a line, to
   the first shipped recipe that inverts a mask on a cut, since none does.
2. **Only the named thing's places.** The places used are those of findings about `RQ1`
   (L1, item 4), at most eight. A place of any other finding is never cut, segmented or
   painted, and a verdict that holds one ends the attempts before this task's code runs.
3. **From places to masks.** For each place: the picture the loop was given is cut to the
   place's box, widened by five hundredths of its size on each side (rascal's crop pad),
   with `view`; `segment` is asked the same phrase inside the crop, for at most four
   instances; each mask it gives is carried onto the picture. A place where the segmenter
   finds nothing joins as itself, rasterised: the judge said the named thing is there, and
   item 2 has held that it said so of the named thing. Rascal also halved the segmenter's
   threshold inside a crop; whizbang's `segment` takes no threshold, the phrase is asked as
   it is, and a threshold is milestone E's (below).
4. **The masks are joined as they come, so their number is bounded.** The loop holds one
   found mask, not a list. A located retry gives `select` the found mask of the attempt
   before and the new masks, at most 1 + 8 × 4 = 33 of the 64 `select` takes, by running
   `remove_prompt` from `select` until `select`; what comes back is the one found mask of
   this attempt, and the third attempt starts from it in the same way, again with at most
   33. The shape, the comparison, the fill and the check are L1's, unchanged.
5. **Which retry.** A finding about `RQ1` that names places: the located retry, after a
   `fail` or an `unsure`. A `fail` about `RQ1` that names none: L1's wider mask, once. An
   `unsure` that names none: the attempts end.
6. **The budget.** Each attempt after the first adds to the plan's bound at most eight
   requests of the segmenter, which cost nothing on the Mac and will have a price on a GPU
   host (milestone 6), and at most 8 × 3 + 1 calls of the loop's own; the test of the
   largest count (L1) is raised to hold three attempts with eight places each.
7. **The record.** An attempt made by a located retry lists the crops it cut, as boxes on
   the picture, the masks they gave, by identity, and the places that joined as themselves
   because nothing was found there: the fields K1 wrote for it.

- [ ] Done when:
  `carry::a_mask_on_a_crop_lands_on_its_picture_and_is_empty_outside_the_crop`,
  `carry::a_mask_on_a_crop_of_another_picture_is_refused`,
  `invert::invert_from_a_crop_mask_lands_protect_on_its_bbox` (rascal's vector),
  `located::a_finding_of_the_named_thing_with_a_place_segments_inside_it_and_the_mask_grows_there`,
  `located::a_place_of_a_finding_about_anything_else_is_never_cut_segmented_or_painted`,
  `located::a_place_where_nothing_is_found_joins_as_itself`,
  `located::an_unsure_that_names_a_place_is_retried_and_one_that_names_none_ends_the_attempts`,
  `located::a_fail_that_names_no_place_widens_once`,
  `located::every_attempt_fills_the_picture_the_loop_was_given`,
  `located::the_shaping_and_the_fill_of_a_retry_are_the_recipes_own_steps`,
  `located::eight_places_of_four_masks_on_the_second_and_the_third_attempt_reach_the_fill`
  (the largest number of masks, 33 given to `select` each time),
  `located::no_more_than_eight_places_are_used_and_the_plans_bound_covers_them`,
  `record::a_located_retry_lists_its_crops_the_masks_they_gave_and_the_places_that_joined_as_themselves`,
  `cli::rascal_remove_checked_retries_inside_a_named_place_under_a_scripted_judge`,
  `model_tier::a_located_retry_finds_the_phrase_inside_a_crop_with_sam3` (`#[ignore]`,
  under `just test-models`), `just docs` regenerated, and the row on `invert`'s `to`
  narrowed with a line.

**Keeping tests lean (L2).** L1's fixture, with a scripted judge that names boxes and the
obligation each is about; the carry is exact arithmetic on masks of 32 by 32.

## R1. The archive, its checksums and the install check (large)

### Goal

One command makes the archive of the machine's target, and one script proves that an
archive installs and runs, on both runners on every pull request, so that the candidate's
checks hold no surprise (`SPEC.md` section 7, "Release archives for both targets"; section
13, "install from a release archive").

1. **The archive.** `just package` (a script, `scripts/package.sh`) builds the release
   binary as `just smoke` does, with no feature of the tests and without `baseline-replay`
   (ADR 0034), and writes `whizbang-<version>-<target>.tar.gz` holding one folder with
   exactly: the `whizbang` binary; `LICENSE`; `THIRD-PARTY-LICENSES.md` (item 3); and
   `README.md`, the install guide of item 6. Nothing else: the pack, the recipes, the
   rules, the constant sets and the catalogue are inside the binary already. Beside it the
   script writes a line of `SHA256SUMS`, the archive's SHA-256 and its name. The same
   sources give the same folder: entries are sorted and their times and owners are fixed.
2. **What the binary may need from the machine.** On Linux, only the system's C library at
   the version Ubuntu 22.04 ships or older, and no other shared library that is not part of
   that system: held by a test that reads the binary's own list. On the Mac, a minimum
   system of macOS 15, set when it is built and read back from the binary. `whizbang
   --version` gives the version and the commit the binary was built from.
3. **The licences of what the binary holds.** A release hands other people's code to a
   third party, so the archive carries the licence text of every crate the binary links,
   generated from the lock file by a script and held to the list `cargo deny` checks: a
   crate in the binary with no text in the file fails `just check`. The model's licence is
   not here: no model is in the archive.
4. **The bootstrap of what a user must have**, `scripts/install-prereqs.sh <folder>`, apart
   from the install check and run before it. `whizbang setup` downloads no Python and
   needs the pinned `uv` (`docs/guides/setup.md`), so a machine with nothing on it cannot
   pass a check that runs `setup`. The bootstrap does what the setup guide tells a user to
   do, and no more: it puts the pinned `uv` (`scripts/tool-versions.txt`), fetched from its
   maker's GitHub release and held to a hash the script carries, and the pinned Python,
   installed by that `uv` with the guide's own command from GitHub's releases, into the
   folder it is given. On a bare Ubuntu 22.04 it first installs, from Ubuntu's own
   archive, the three things the fetch needs (certificates, `curl`, `tar`). These are the
   only downloads, and they are named under "Origins (R)".
5. **The install check**, `scripts/install-check.sh <archive> <sums> <prereqs folder>`,
   does what the install guide tells a person to do, and nothing a person could not. It
   contacts nothing. In an empty folder, with a cleared environment, a temporary empty
   home, and a path that holds the system's own folders and the prerequisites' folder and
   no Rust toolchain, it checks the SHA-256, unpacks, holds the list of files to item 1,
   and then runs the installed binary:
   - `whizbang --version` gives the version the archive is named for, and the commit;
   - `whizbang describe --json` equals the committed `docs/reference/description.json`, so
     the binary installed is the binary documented;
   - **a run through the executor, with no model:** a picture is imported into a new store,
     the recipe `mask_rect` is run on it, and `runs show` gives a completed run that lists
     its steps: the store, the runner, a recipe and the record, end to end;
   - **the server:** `whizbang serve` is started, answers `initialize`, lists its tools,
     answers one call of `mask_rect`, and ends when its input closes; the script kills and
     waits for it on every path;
   - **the pack:** `whizbang setup` installs the pack's core, offline, with the `uv` and
     the Python of the prerequisites' folder and nothing else, and the worker says hello;
   - `whizbang backends status` answers, with no network;
   - **what this target cannot do is refused in its own words.** On Linux: `whizbang setup
     --extra torch` is refused before anything is downloaded, and `whizbang rascal
     remove-prompt` is not ready at prepare, naming the segmenter. The same check of
     `remove-checked` is added by L1 or R1, whichever lands second.
6. **The install guide**, `docs/guides/install.md`, and its copy in the archive: what a
   user must have first (item 4's two things, with the commands); download, check the
   SHA-256, unpack, put the binary on the path; what each target can do (below); that a
   Mac's browser marks a downloaded file, that the binary is not notarised by Apple, and
   the download command that sets no mark (the owner's decision 3); and that SAM 3 is
   fetched from its maker with the user's own sign-in (the owner's decision 2).
7. **In CI.** The `release` job of each runner, which already builds the release binary,
   runs `just package`, the bootstrap into a fresh folder, and the install check on what it
   built. A change that breaks the archive fails its own pull request.

**What each archive can and cannot do.**

| | `aarch64-apple-darwin` | `x86_64-unknown-linux-gnu` |
|---|---|---|
| Every operation and recipe that uses no model; the store, runs, spend, export and import; `serve` over stdio; the evaluation and lab commands | Yes | Yes |
| Fills through fal; the judges of the three providers; `squint check`; `remove` and the repairs given a location | Yes, with keys | Yes, with keys |
| The pack's core and its worker | Yes | Yes |
| A local model: `segment` on SAM 3, and so `remove_prompt` and the loop | Yes, after `whizbang setup --extra torch` and `whizbang models get sam3` | No. The runtime is refused on Linux (`SPEC.md` section 5; the carry-over row "Linux runtime extra", deferred by the owner on 2026-11-18: production runs models on an external GPU host). They come back with a remote segmenter, milestone 6 |

**How the Linux archive's test proves "runs".** By item 5 on the `ubuntu-22.04` runner: the
installed binary executes a recipe end to end through the executor and the store, serves a
call over stdio, and installs the pack's core and hears its worker; and it refuses, in a
sentence that names the reason, each thing that needs a local model. It makes no paid call:
the paths to fal and to the judges are the same code the fast tier runs on Linux against
recorded exchanges, and no key is given to a hosted runner. That this is enough for "runs"
on Linux is open question 2.

- [ ] Done when: `scripts/tests/test-package.sh` (the archive holds exactly the four files;
  two builds of one tree give the same list and the same sums line; the sums line matches
  the archive), `scripts/tests/test-install-prereqs.sh` (against a stand-in for the fetch:
  a `uv` whose hash differs is refused; only the two things are put in the folder),
  `scripts/tests/test-install-check.sh` (the check passes on a good archive with an empty
  home and only the prerequisites' folder; it fails when that folder lacks `uv` or the
  Python, on an archive whose sum is wrong, on one with a file too many, on a binary of
  another version, and on a server that does not end, which it kills),
  `scripts/tests/test-third-party-licences.sh` (a crate in the binary without its text
  fails), `cli::the_version_line_names_the_version_and_the_commit`,
  `release::the_linux_binary_needs_only_the_c_library_of_ubuntu_22_04`,
  `release::the_mac_binary_states_macos_15_as_its_minimum`,
  `scripts/tests/test-ci-jobs.sh` holding the `release` job to `just package`, the
  bootstrap and the install check, and CI green with the install check passing on
  `macos-15` and on `ubuntu-22.04`.

**Keeping tests lean (R1).** The script tests of the check run on a tiny stand-in binary
(a shell script named `whizbang`), not on a release build; the real check runs once on each
runner in the `release` job, which has the binary already.

### Origins (R)

The build downloads what CI already downloads: crates from crates.io, the pinned tools, the
pinned Python. The bootstrap downloads `uv` from its maker's GitHub releases and the Python
from GitHub's releases, both as the setup guide already tells a user to, and, in R2's bare
container only, three packages from Ubuntu's archive and the container's image from its
registry. These last two are places CI has not fetched from before; they receive nothing of
the owner's. The install check contacts nothing.

## R2. The `prerelease` and `release` workflows (large)

### Goal

A candidate is built once, checked on both targets and on the owner's Mac, and the bytes
that were checked are the bytes that are released (`SPEC.md` section 14, "GitHub Actions"
and "Workflows").

1. **Versions and tags.** The workspace's version becomes `0.1.0` (the owner's decision
   1). A candidate is the tag `v0.1.0-rc.<n>`; a release is the tag `v0.1.0` on the same
   commit. Only the orchestrator tags, on `main`, after CI and both scans pass on that
   commit (`AGENTS.md`). A tag whose version is not the binary's is refused by both
   workflows.
2. **`prerelease`**, on a candidate tag. Jobs:
   - `build`, on `macos-15` and on `ubuntu-22.04`: `just package`; the archive and its sums
     line are kept as the workflow's artifacts.
   - `install`, on each: a fresh job that checks out the scripts and nothing built,
     downloads the archive, runs R1's bootstrap and then R1's install check. On Linux it
     runs inside a bare Ubuntu 22.04 container, so nothing a hosted runner happens to have
     installed can stand in for something the archive or the guide forgot.
   - `candidate`: when all four pass, makes a GitHub pre-release for the tag, a draft, with
     both archives and `SHA256SUMS`. This is the only job with leave to write, and the only
     thing it may write is a release.
3. **The local leg**, `scripts/prerelease-local.sh <tag>`, the orchestrator's, on the
   owner's Mac, never run by CI. It keeps two things apart that draft 1 ran together.
   - **The checkout's model tests.** `just test-models`, at the tag's commit checked out.
     They build and test the checkout, not the archive; the receipt names them so.
   - **The archive's acceptance.** The Mac's archive is downloaded from the candidate and
     held to `SHA256SUMS`; it is installed into a temporary folder; the installed binary's
     own digest is taken, and its `--version` must name the tag's version and the tag's
     commit, or nothing more runs. Then, with that binary alone and the owner's env file:
     `whizbang setup --extra torch`, which installs the pack the binary itself carries;
     `whizbang rascal remove-prompt --until feather` on SAM 3; one fill through fal; one
     `whizbang squint check` by a judge of each of the three providers; and `whizbang
     rascal remove-checked` once. Every command that loads a model runs under the heavy
     lock, the thread guard at 200 and the watch on resident memory
     (`scripts/heavy.sh`, `scripts/thread-guard.sh`, `scripts/memory-watch.sh`), as `just
     test-models` does. The pictures are synthetic, drawn by the script.
   - It writes the receipt (item 4) and attaches it to the candidate.
4. **The receipt**, `prerelease-receipt/1`, one file with a fixed shape and no field
   beyond it: the tag; the commit; the SHA-256 of each archive and of the installed
   binary; the binary's version line; and one entry for each required check, by a fixed
   name: the checkout's model tests; the archive's `setup` with the extra; its mask on SAM
   3; its fill; its check by each of the three providers, one entry each; and its loop.
   Each entry holds its outcome, the run, the backend, the model and its version, and for a
   paid call the tokens and what was settled. It holds no reply text, no path and no key.
5. **`release`**, on a version tag. It builds nothing. The gate,
   `scripts/release-gate.sh`, refuses unless all of these hold for the tag's commit: a
   `prerelease` run on that commit ended in success; the candidate is the one with the
   highest number among the version's candidates, so a receipt of an earlier candidate
   that a later one superseded is not taken; the candidate holds a receipt that is read
   whole by its schema; the receipt names this tag's candidate and this commit; **every
   required check is there exactly once and passed**; and the digests the receipt names
   are those of the archives the candidate holds, which the gate takes again from their
   bytes, and those of `SHA256SUMS`. This is section 14's "A release is refused unless
   this passed on the same commit". The workflow then publishes the candidate's own
   archives and `SHA256SUMS` under the version tag, marked as a pre-release.
6. **The skill `release`** (`SPEC.md` section 14, "Workflows": "`release` | Milestone 4 |
   A version release"), `.claude/skills/release/SKILL.md`, for the orchestrator, in the
   form the other skills have (inputs, checklist, commands, hand-back): choosing the commit
   and the next candidate number; CI and both scans on that commit before a tag; tagging;
   waiting for `prerelease`; the local leg and its receipt; what to do with a failed
   candidate (it keeps its tag and its draft; the fix is a pull request; the next number is
   tagged; a pushed tag is never moved); the version tag; checking what was published
   against `SHA256SUMS`; and the record. `AGENTS.md` lists it, and
   `scripts/tests/test-skills.sh` and `scripts/tests/test-rules.sh` are changed to hold it.
7. **The spec's table.** Section 14's rows for `prerelease` and `release` are brought in
   line: the model tier and the live calls are the local leg, on the owner's Mac, and the
   release holds the two archives, the pack being inside the binary. This edit changes what
   a workflow holds, so it is reviewed with this plan's review rounds and made only after
   the owner's yes to open question 1.

- [ ] Done when: `scripts/tests/test-release-workflows.sh` (each workflow runs only on its
  kind of tag; `prerelease`'s jobs call `just package`, the bootstrap and the install check
  and nothing else builds; only `candidate` may write; `release` builds nothing),
  `scripts/tests/test-release-gate.sh` (the release is refused when no `prerelease` run
  succeeded on the commit; when the receipt is missing; when it is malformed or holds a
  field its schema does not; when a required check is missing, is there twice, or did not
  pass; when it names another commit or another candidate; when it is of a candidate a
  later one superseded; when a digest it names is not that of the archive's bytes; and it
  passes when all agree), `scripts/tests/test-prerelease-local.sh` (against stand-ins for
  the binary and for the `gh` command: a wrong sum stops it before anything runs; a binary
  whose version line names another commit stops it; a failed call is written as failed and
  fails the leg; each model-loading command is run under the three guards; the receipt it
  writes is valid by its schema and holds only its fields),
  `scripts/tests/test-tag-version.sh` (a tag of another version than the binary's is
  refused), `scripts/tests/test-skills.sh` and `scripts/tests/test-rules.sh` passing with
  the fifth skill listed in `AGENTS.md`, and the spec's table edited. The first real run of
  `prerelease` is N1's box, not this one.

**Why the first real run is N1's.** A workflow that runs on a tag cannot run before a tag
exists, and a builder does not tag. R2 is held by tests of its scripts and of the workflow
files' shape; the orchestrator tags the first candidate after E1 and fixes what the real
run finds, and N1 records it.

## E1. The evidence before the candidate (medium; the orchestrator's runs)

### Goal

Show the loop on real models, and write down what it does and does not do, before any
candidate is tagged. The first review round found that draft 1's experiment could not be
run: the lab loads recipe files, refuses a judge and compares two arms, and the scorer
reads four fixed recipes. So this task adds no arm to the lab and no recipe to the scorer.
It adds one script and a tally, and the live step **reports what happened**; that each kind
of retry works is shown by L1's and L2's tests with fakes, not here.

1. **The script**, `scripts/loop-evidence.sh`, with its tally in Python's standard library
   (as `scripts/baseline/` is). What the orchestrator runs, exactly, once for each judge:

   ```
   scripts/heavy.sh loop-evidence scripts/loop-evidence.sh --binary <whizbang> \
       --store <a new store> --pictures <folder> --judge <judge> \
       --config <file> --env-file <file> --out docs/results/m4-loop-<judge>.toml
   ```

   For each picture in the folder, in name order, it imports the picture (`whizbang store
   import --picture`), runs `whizbang rascal remove-checked` on it with the phrase "a red
   ball" and the request id `m4-loop-<judge>-<n>`, so a run that was interrupted is run
   again without repeating a paid call, and then reads the store with `whizbang runs
   export` and writes aggregates only: how many runs; how the attempts ended (passed at
   first, passed after a retry, kept `unsure`, kept `fail`, unchecked, error); the attempts
   made; the retries by kind (wider, located) and the endings by cause (a finding about
   something else, the same mask, the budget); the places cut and how many found nothing;
   the findings by obligation; the median time; what was reserved and settled. The
   pictures are milestone 3's synthetic edit set, the 24 with a red ball, drawn again by
   `whizbang eval edits draw` from `labs/m3-edits/edits.toml`. The judges are
   `claude-haiku-5-5` and `gpt-6.1-terra`. The store's config sets the call cap at 3.50
   dollars, so three attempts are planned with either. The script is tested against a
   stand-in binary; it makes no call a test could pay for.
2. **Three cases by hand**, from the command line, the cases the carry-over list keeps for
   milestone 5's playground: a clean pass, or a retry then a pass (`whizbang rascal
   remove-checked --image <id> --prompt <phrase>`); a vague phrase, showing its attempts
   and that it was not passed (the same with a phrase that names nothing in the picture);
   an unset judge key, leaving the run unchecked and saying why (the same with an env file
   that lacks the judge's key). On pictures of the owner's if the owner gives some, else
   on three of the 24. Their fields (outcome, attempts, verdict, why the attempts ended,
   settled) go in `docs/results/m4-loop.md`, which also holds the two tallies.
3. **What the evidence is not.** It scores nothing: no attempt's fill is measured against a
   clean picture, and no first attempt is compared with a kept one, because the scorer
   cannot read a loop's runs and this plan does not change it. Milestone 3's masks covered
   the drawn ball whole, so most runs are expected to pass at first; the record says how
   many retried, whatever that number is. Whether a retry makes a result better is rung 2
   proper and rungs 3 and 4, and is milestone E's.
4. **The loop's tool record**, `docs/reference/tools/remove_checked.md` (`SPEC.md` section
   8: "A tool is not finished ... without its record"): what it does, when to use it and
   when not, two worked examples from scripted runs (one of them a case it handles badly:
   a finding about a protected subject, where it stops), its cost for each judge, its
   limits, and its mark: experimental. Rung 1 is the worked examples. Rung 2 is **shown in
   part and said so**: preservation, by
   `checked::every_pixel_outside_the_kept_mask_is_the_pictures_own`; the mask and the fill
   of each attempt are `remove_prompt`'s own steps, whose rung 2 is milestone 3's
   (`docs/results/m3-edits.md`); what the loop adds, the choice to retry and which attempt
   to keep, is reported by item 1 and not scored. Its visual quality reads "not yet
   confirmed by a person", as the other fill recipes' does.
5. **Google's prices.** The carry-over row on the introductory prices: if these runs are
   made on or after 2026-11-30, the row of `gemini-4-argon` is read again from the
   provider's page and the build's row, its date and ADR 0026 changed with it; if before,
   it is re-owned with a line to the first live run on or after that date, as the row of
   `gemini-3.8-flash` is to 2026-12-31.
6. **The model's licence, read for hosting.** The first step of the carry-over row on
   hosting the model's files: what the model's licence asks of someone who redistributes
   its files (the copy that travels with them, the uses it forbids and how a user is told,
   the acknowledgement it asks), written into the row as a reading, not legal advice. The
   rest of the row is re-owned to milestone 6 (the owner's decision 2).
7. **The rows.** Every row of the table below closed, narrowed or re-owned with its line,
   and a new row for notarisation (the owner's decision 3).

- [ ] Done when: `scripts/tests/test-loop-evidence.sh` (against a stand-in binary: each
  picture is run once under its request id; a second run of the script starts nothing
  again; the file written holds aggregates and no item),
  `evidence::the_loops_record_names_a_commit_of_this_tree_and_holds_only_aggregates` (on
  both judges' tallies and the three cases),
  `evidence::the_loops_tool_record_holds_rung_1_and_says_what_of_rung_2_is_shown`,
  `tool_records::recipes::the_record_of_remove_checked_is_what_it_gives`,
  `scripts/tests/test-tool-records.sh` passing with the new record, and every row of the
  table below closed or re-owned.

### What the owner provides (E)

- If the owner wishes: three to five pictures of their own, each with a phrase, where one
  pass leaves something behind (rascal's case was a dog's collar whose buckle stayed), in a
  folder on the Mac. Without them the three cases by hand run on synthetic pictures.

Nothing else. The Google key answers again (the orchestrator, 2026-11-19: a fourth run of
the gate is completing with it), so a live call to each provider can pass.

## N1. After the candidate: the release's record and the note (small)

### Goal

Draft 1 had one task that had to be merged before a candidate could be tagged and could not
be done until the candidate had passed. They are two now, and this is the second.

**Which commit is which.** The first candidate, `v0.1.0-rc.1`, is tagged by the
orchestrator on `main` at the merge of E1, by the skill `release`. Each thing a real run
finds is fixed in a pull request of its own, and the next candidate is tagged on `main` at
that fix's merge. The candidate that passes is on some commit C. `v0.1.0` is tagged on C,
and the release holds C's archives. N1 is written after that, lands on `main` after C, and
is in no archive of `0.1.0`: it records C from outside. The tag `m4` is made on N1's merge.

1. **The release's record**, `docs/results/m4-release.md`: each candidate tagged, with its
   commit and how it ended; for the one that passed, each job's result, the archives'
   digests, and the receipt's entries; the version tag and what was published.
2. **The note**, `docs/plans/m4-note.md`: the clauses of the spec's row each beside the
   tests and records that show it, the spend, what the owner decided, and what a reader
   should not assume (the loop is experimental and unscored; Linux runs no local model;
   the Mac's binary is not notarised).
3. **The boxes.** R2's and E1's clauses about a real run are ticked here, by this later
   pull request, as `AGENTS.md` asks of a box about something that happens after its own
   pull request.

- [ ] Done when:
  `evidence::the_release_record_names_a_candidate_whose_commit_is_of_this_tree`,
  `evidence::the_release_record_holds_every_required_check_of_the_receipt_as_passed`,
  `scripts/tests/test-notes.sh` passing with the note, a `prerelease` run ended in success
  on C with a receipt the gate accepts (both named in the record), `v0.1.0` published from
  C's archives, and the note written.

## Carry-over rows owned by milestone 4, and who takes each

| Row (by its subject in `docs/carry-over.md`) | What happens to it |
|---|---|
| Say on the terminal when a run was not checked | Taken by L1 (item 9) |
| Keep the cause when a check fails | Taken by L1 (item 9) |
| A check cannot receive the variables it needs | Taken by L1 (item 3): a nested call's arguments |
| The documented per-call cap for a check is one cent | Taken by L1 (item 7): the guide's real figures |
| `remove_prompt`'s `check` argument (the dropped row on what a manifest says, which points here) | Answered by L1: checking is the loop's, an action of its own; `remove_prompt` gains no argument |
| `invert`'s `to`: a mask on a cut canvas or a crop carried up before it is inverted | The crop half taken by L2 (item 1), which the located retry needs. The cut half re-owned to "the first shipped recipe that inverts a mask on a cut": no recipe of milestone 4 does, since every attempt of the loop fills the picture it was given |
| An executor built with a digest alone | Taken by A1 (item 10) |
| `whizbang lab run` prints nothing while a round runs | Taken by A1 (item 7) |
| `whizbang lab run`'s "started now" | Taken by A1 (item 8) |
| The pairs page: a picture's size tells its arm; backspace | Taken by A1 (item 9) |
| The Google rows of prices are introductory | Taken by E1 (item 5) for the row whose date has come by then; each other re-owned to the first live run on or after its date |
| Hosting the model's files for customers | Its first step, the reading of the licence, taken by E1 (item 6). The rest re-owned to milestone 6 by the owner's decision of 2026-11-19 |

**Rows milestone E owns that milestone 4 leans on.** They stay E's, but for the first.

| Row | How milestone 4 leans on it |
|---|---|
| Details first is still `check`'s default | Taken from E by A1, because the loop calls `check` on every attempt. Gating a fixed details first on fresh data stays E's |
| The Google arm of the gate, once the key answers | The key answers again; the same key answers the candidate's live call to Google |
| The blinded picking rounds of milestone 3 | The loop's record says of its visual quality what the five fill recipes' say: not yet confirmed by a person. A1's fix to the pairs page should land before the owner picks |
| The model-based recipes up rungs 3 and 4; a fresh confirmation set | The loop joins them: milestone 4 gives it rung 1, rung 2 in part, and the mark experimental |
| `check`'s `tiles` and second look | The loop's check uses neither; both stay off |
| A resumed matrix or gate never runs a judge's failed cell again | Not leaned on |

**Rows with no milestone that bound milestone 4.** "Linux runtime extra", deferred by the
owner on 2026-11-18: it is why the Linux archive runs no local model. Milestone 5's
acceptance check of the loop by hand in the playground: its three cases are run from the
command line in E1, and stay milestone 5's for the playground.

## Commit strategy

- One task is one branch and one pull request, built tests first, reviewed whole by a
  reviewer that did not write it, merged by the orchestrator with a merge commit once `ci`
  and both scans pass. A builder pushes once, when the whole task is done, after one full
  `just check` on the finished tree.
- An ADR is in the pull request of the task that names it. Ported behaviour names its
  source in the commit message: the repo, the path, the pinned commit.
- Every state of `main` between tasks passes `just check`, and at every one of them
  `remove_prompt` and `squint check` work as they did or as A1 says.
- **Rollback checkpoints.** The tag `m3`, on 0c75eedd, which exists before wave 1.
  `m4-loop`, on the merge of L2: the loop whole, against fakes and SAM 3, nothing released.
  Each candidate tag, `v0.1.0-rc.<n>`: a state whose archives exist. `v0.1.0`, the release.
  `m4`, on N1's merge. A pushed tag is never moved: a bad candidate is followed by the next
  number.
- **If a task shows a decision here is wrong,** the decision is changed in this file first,
  in its own pull request; a change of scope or architecture gets the review rounds first.
  A test is never bent to fit.

| Tasks | To undo |
|---|---|
| A1 | Revert the merge: details first is the default again, and the lab's rows are as they were. A run can also ask for details first with `--details first` without any revert |
| K1 | Revert after L2 and L1. A plan is something only an action that plans has; `from` is an argument no caller gives by default; findings are an addition to what `check` hands back. A store written since holds records of `run/6`, which a build without K1 does not read: the live runs use stores made for them |
| L1, L2 | Revert L2's merge, then L1's. `remove_prompt` and `check` never depended on the loop |
| R1 | Revert the merge: CI's `release` job is the smoke alone again |
| R2 | Revert the merge. A draft candidate on GitHub is deleted by the orchestrator; a published release is not deleted but followed by a new version |
| E1, N1 | Records, a script and a note; a candidate that failed stays as its tag and is superseded |

## Spend

All on approved recipients, on the owner's Mac, by the orchestrator, each run's settled
spend in its results file and the total in the note. A line that passes twice its estimate
stops and is said.

| Step | What is called | Estimate |
|---|---|---|
| A1, K1, L1, L2, R1, R2 | Nothing paid: fakes, and SAM 3 on the Mac under `just test-models` | 0 |
| E1, item 1, with Haiku | 24 pictures, at most 3 fills and 3 checks each | at most 3.20 |
| E1, item 1, with Terra | The same with the dearer judge | at most 4.10 |
| E1, item 2: three cases by hand, and the owner's pictures | About ten runs of the loop | 1.50 |
| The local leg of a candidate | One check by each of three judges, two fills, one loop; about 0.30 a candidate, three candidates allowed for | 0.90 |
| The owner's own tries of the loop | About thirty runs | 5.00 |
| One whole rerun of E1's item 1 after a fix | | 7.30 |
| | **Total** | **about 22 dollars, of the 100** |

The rates are milestone 3's: Bria Eraser is 0.04 dollars a fill; a decision with no details
measured first settled about 0.0033 dollars with `claude-haiku-5-5`, 0.023 with
`gemini-3.8-flash` and 0.017 with `gpt-6.1-terra` (`docs/baseline/manifest.toml`). What a
loop reserves is far more than what it settles (L1, item 7).

## Dependencies

```
A1 ──┐
     ├──► L1 ──► L2 ──┐
K1 ──┘                ├──► E1 ──► the candidate ──► N1
R1 ──► R2 ────────────┘
```

- A1 before L1: the loop's tests and its cost are those of the default `check`.
- K1 before L1: the loop is prepared by a plan, runs a recipe in stretches, reads findings
  and fills the record's list, and all four are K1's.
- L2 after L1: it adds one kind of retry to a loop that is whole without it.
- R1 and R2 beside the whole loop strand: an archive of `main` at any commit is a valid
  subject for them.
- E1 after everything: its script runs the loop, and its merge is the first commit a
  candidate is tagged on.
- N1 after a candidate has passed.

## Risks

1. **The judge is not good enough to steer a paid retry.** Squint's judges agreed with the
   labels about 61% of the time, and a third of verdicts were `unsure`
   (`docs/results/m2c-evaluation.md`). A wrong `fail` buys a fill nobody needed; a wrong
   `pass` hides a leftover. The loop bounds the first (three attempts; only a finding of
   the named thing allows a retry; an `unsure` with no place ends them) and cannot prevent
   the second. Its mark is experimental, and milestone E takes it up the ladder.
2. **The live step may show almost no retry.** Milestone 3's masks covered the drawn ball
   whole. That is reported as it is; the retries are shown by tests with fakes, and the
   owner's own pictures are the better live evidence.
3. **A judge cites the wrong obligation.** The loop classes a finding by the obligation it
   cites. A judge that reports a leftover under `RQ2` stops a retry that would have helped;
   one that reports damage under `RQ1` allows a retry inside that place. The second is the
   dangerous one, and is bounded: the place joins the mask only where the segmenter finds
   the phrase or, finding nothing, as the cited box itself, at most eight places, and
   `protect` is still subtracted by the recipe's own step. E1 counts findings by
   obligation, so a judge that confuses them shows.
4. **Planning at preparation is a change to the contract's core.** It touches preparation,
   the fingerprint and admission, which every call goes through. K1 holds that an action
   that plans nothing is prepared exactly as before, and its reviewer looks there hardest.
5. **The loop fills all four levels.** Nothing can call it: a user's recipe that names it
   is refused when read, and a later tool that wants a checked removal as one step of
   something larger will need the limit raised, which is a change to the spec.
6. **The reservation of three attempts is over common call caps.** Fitting the plan to the
   cap keeps work running and can leave a caller with fewer attempts than it expected; the
   plan is printed before anything runs.
7. **A workflow that runs on a tag is first run for real at the first candidate.** R1's
   bootstrap and install check run on every pull request to shrink what is left to find;
   the spend allows for three candidates.
8. **A Mac's browser marks a downloaded archive, and macOS refuses to run an unsigned
   binary so marked.** The guide tells a user to download with a command that sets no mark
   (the owner's decision 3). CI cannot show this either way.
9. **Linux "runs" without any model.** A person who reads "installs and runs on both
   targets" may expect `remove-prompt` on Linux. The archive's own guide, the refusal's
   sentence and the note say otherwise in plain words.
10. **Test time.** The loop's tests run several nested runs each, and an attempt is now
    four calls where draft 1 had two; the fixture keeps them to 64-pixel pictures and
    scripted models, and L1 reports its seconds.
11. **The budgets are estimates.** K1 is over the usual size on purpose, with its seam
    named; a pull request that passes about 2,500 changed lines splits there, and the owner
    is told the count.

## Open questions

Each is phrased so the owner can answer yes or no. The recommended answer is the one the
tasks above assume; a no changes the task named. Three of draft 1's questions are settled
and are under "What the owner decided".

1. **The pre-release checks in two parts.** I need your yes to run the model tests and the
   live call to each provider on your Mac, by the orchestrator, against the candidate's own
   archive (the local leg), and to keep GitHub's part to building and installing on both
   targets, with the spec's table edited to say so. No means provider keys and a model are
   given to GitHub's runners, which nothing so far has done and a hosted runner cannot do
   for SAM 3. Recommended: yes. (R2)
2. **What "runs" means on Linux.** I need your yes that the Linux archive passes when it
   installs, runs a recipe that needs no model end to end, serves a call over stdio,
   installs the pack's core, and refuses in plain words what needs a local model; with no
   paid call made from a Linux machine. Recommended: yes. (R1)
3. **The loop is its own action, on a phrase.** I need your yes that the loop is a new
   action, `remove_checked` (`whizbang rascal remove-checked`), over `remove_prompt` only,
   as rascal's was, and that `remove_prompt` stays one unchecked pass. No means
   `remove_prompt` itself checks and retries by default. Recommended: yes. (L1)
4. **Rascal's retry rules, narrowed.** I need your yes to: at most three attempts, the
   first included; a retry only when the judge says the named thing is still there, inside
   the places it named, or with a wider mask once when it named none; and a stop, with the
   best attempt handed back, when the judge says something else was damaged or is unsure
   without naming a place. Recommended: yes. (L1, L2)
5. **Unchecked rather than refused.** I need your yes that, by default, the loop runs once
   unchecked and says why when no judge is ready, with `--check required` for a caller who
   would sooner be refused. Recommended: yes. (L1)
6. **When every attempt fails, the loop still completes.** I need your yes that it hands
   back its best attempt with the verdict `fail` or `unsure`, and is an error only when no
   removal was made. No means it ends as an error and hands back no picture. Recommended:
   yes. (L1)
7. **Attempts fitted to the call cap.** I need your yes that a loop whose three attempts
   would reserve more than the call cap runs with as many attempts as the cap covers, and
   says so before it starts. No means it is refused, naming the cap, and K1's planning is
   needed only to leave out a judge. Recommended: yes. (K1, L1)
8. **The loop is code, and nothing calls it.** I need your yes that the loop is written in
   Rust (recipe files gain no branch or repeat), that a recipe may not name a loop as a
   step, and that calls stay four levels deep. Recommended: yes. (L1)
9. **A cancelled loop's finished attempts.** I need your yes that, for now, a picture made
   by an attempt that completed before the loop was cancelled is named in the record and
   cannot be taken by a later call. No means L1 also builds a way to keep it. Recommended:
   yes. (L1)
10. **The check is `source-fidelity`, and it hands back its findings.** I need your yes
    that the loop checks with squint's `source-fidelity` recipe under a job written from
    the phrase, and that `check` hands its caller its findings as a new kind of result,
    since a diagnostic may not be read as an output. Recommended: yes. (K1, L1)
11. **The loop's record without a score of its own.** I need your yes that milestone 4
    gives the loop rung 1 and rung 2 in part (preservation shown; each attempt's mask and
    fill credited from `remove_prompt`'s record; whether a retry helps reported as counts
    and not scored), with the scoring left to milestone E. No means a task is added that
    teaches the scorer to read a loop's runs. Recommended: yes. (E1)

## What the owner is told, not asked

- **Eight pull requests**, six of them large; three run side by side in the first wave.
- **Details first is turned off as the default now**, ahead of milestone E, because the loop
  calls `check`; it stays selectable with `--details first`.
- **The tag `m3`**, on 0c75eedd, is made by the orchestrator before the first wave.
- **CI fetches from two places it has not before**, in the bare container of `prerelease`
  alone: Ubuntu's package archive and the container image's registry. Neither receives
  anything of the owner's.

### What milestone 5, E and later owe after 4

| Item | Left for |
|---|---|
| The loop in the playground, with its three cases by hand | Milestone 5 |
| The loop's rung 2 scored (whether a retry makes a result better), and rungs 3 and 4; its visual quality confirmed by a person; a threshold for the segmenter inside a crop, if E1's count of crops that found nothing calls for it | Milestone E |
| A local model on Linux, and so `remove_prompt` and the loop there, through a remote segmenter | Milestone 6 |
| Hosting the model's files for customers | Milestone 6 (the owner's decision of 2026-11-19) |
| Notarisation of the Mac's binary by Apple | Its carry-over row (the owner's decision of 2026-11-19) |
| Google's introductory prices, each row from its date | The first live run on or after 2026-11-30 and 2026-12-31 |
| `invert` of a mask on a cut canvas | The first shipped recipe that needs it |
| The skills `add-tool` and `run-eval`, which `SPEC.md` section 14 lists from milestone 2 and the repo does not hold | Not milestone 4's; said here because R2 changes the list of skills |
| Holding the `whizbang` name on crates.io | "Later", as the carry-over list has it; untouched here |

## Review rounds

### Review round 1

On draft 1, at 85049850. Verdict: revise. Fifteen findings: 3 blockers, 11 majors, 1 minor.
Each was checked against the code before it was acted on; every one was found right, and
all are taken into draft 2. One line each:

1. **Blocker: preparation cannot fit attempts or leave out a judge.** Right: an action is a
   static descriptor and `run`, needs apply by an argument's presence alone, and a missing
   capability is "not ready". Taken: K1, item 1, a call's plan, in the fingerprint and held
   at admission, tested through the executor, the command and the server.
2. **Blocker: `remove_prompt` fills before it hands anything back.** Right: the recipe ends
   in `fill` and hands back the picture and the rebound mask, and only `until` shows a step
   between. Taken: the loop runs the recipe in stretches (find, shape, compare, fill), each
   a nested call, counted (L1, items 2 and 7); `from` is K1's and lands first.
3. **Major: L1 read what P1 made, and two tasks changed one contract.** Right. Taken: P1 is
   gone; everything L1 reads is K1's, which alone owns the record's version, the kinds of
   result, preparation and the runner; the waves are drawn again.
4. **Major: 72 masks for a `select` that takes 64.** Right (`MASKS_MAX` is 64; a segment
   call defaults to eight). Taken: the loop holds one found mask and joins as it goes, at
   most 33 to `select` on any attempt, with a test at the largest number on the second and
   the third attempt (L2, item 4).
5. **Major: any finding could enlarge the mask.** Right: `source-fidelity`'s rules cover
   protected subjects and geometry, and a place with nothing found joined as itself.
   Taken: a finding is classed by the obligation it cites; only one about the job's `RQ1`
   allows a retry; one about `SF1`, `SF2` or `RQ2` ends the attempts and nothing more is
   painted; both tested (L1, item 4; L2, item 2; open question 4 reworded; risk 3).
6. **Major: findings on a cache hit.** Right: a hit returns a verdict alone, and its
   `judged` document is optional. Taken: a hit hands back findings worked out from the
   kept document by the same code; an entry without one is not answered from (K1, item 3).
7. **Blocker: the lab and the scorer cannot run the experiment.** Right: the lab loads
   recipe files, refuses a need beyond fill and segment, and compares two arms; the scorer
   reads four fixed recipes. Taken, by the narrower way: no lab and no scorer; one
   committed script, whose command is stated, runs the loop and tallies its records (E1,
   items 1 and 3); the loop's own score is left to milestone E and asked (open question
   11).
8. **Major: a first mask "too tight" was neither defined nor injectable.** Right. Taken:
   the arm is dropped. Every kind of retry is shown by tests with fakes; the live step
   reports what happened (L1, "Keeping tests lean"; E1, item 3; risk 2).
9. **Major: `just test-models` tests the checkout, not the archive.** Right: it builds and
   runs the checkout's tests. Taken: the local leg keeps the checkout's model tests and
   the archive's acceptance apart; the second uses the installed binary alone, its commit
   and digest held first, its own pack installed, each model-loading command under the
   three guards (R2, item 3).
10. **Major: matching metadata is not a passed slow tier.** Right. Taken: a receipt with a
    fixed schema and a fixed list of required checks; the gate holds every one present
    once and passed, the candidate's identity and the archives' bytes, and refuses a
    superseded candidate's; rejection tests for each (R2, items 4 and 5).
11. **Major: nothing provided `uv` and the Python to an empty machine.** Right: `setup`
    downloads no Python and needs the pinned `uv`. Taken: a bootstrap apart from the
    offline check, by the setup guide's own downloads, then the check with an empty home
    and only that folder (R1, items 4 and 5; "Origins (R)").
12. **Major: N1 had to merge before the candidate and finish after it.** Right. Taken: E1
    before the candidate, N1 after; the candidate is tagged at E1's merge or a fix's; the
    release is that commit; the note lands later and ticks the boxes about the real run.
13. **Major: the skill `release`.** Right: `SPEC.md` section 14 lists it from milestone 4,
    and the tests hold four skills. Taken: R2, item 6, with `AGENTS.md` and both tests.
14. **Major: the review rule narrowed the review.** Right by `AGENTS.md`, "Roles and flow".
    Taken: every pull request is reviewed whole; the listed concerns are what each
    reviewer looks at hardest, in addition.
15. **Minor: the threshold was promised twice.** Right. Taken: it is milestone E's; E1
    counts the crops that found nothing.

Also changed with draft 2, from the owner and the orchestrator: three open questions are
settled ("What the owner decided"); the Google key is no longer owed; the rows left (draft
1's F1) are joined to A1, so the count stays eight.
