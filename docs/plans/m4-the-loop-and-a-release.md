# Plan: milestone 4, the loop and a release

Draft 1, 2026-11-19, from `main` at 0c75eedd (the merge of #244, which closed milestone 3;
its note is `docs/plans/m3-note.md`). Milestone 4 is `SPEC.md` section 16, row 4, "The loop
and a release": "Remove with a check and bounded retries runs as nested calls on one budget;
the `prerelease` workflow passes and a release archive installs and runs on both targets".
Beside the row it takes every carry-over row that names milestone 4 (the table "Carry-over
rows owned by milestone 4, and who takes each", below) and one row that milestone E owns and
the loop cannot wait for: details first as `check`'s default. It ends with the note of
milestone 4.

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
- **A place** is a location the judge cited on a picture, a box or a polygon, when it said
  an item of its checklist failed or could not be settled.
- **An archive** is one compressed file a person downloads to install whizbang. **A
  target** is one of the two platforms of `SPEC.md` section 5: `aarch64-apple-darwin`
  (macOS 15 on Apple silicon; CI's runner `macos-15`) and `x86_64-unknown-linux-gnu`
  (Ubuntu 22.04 on x86-64; CI's runner `ubuntu-22.04`).
- **A candidate** is a commit tagged as a release candidate, and the archives built from it.
- **The local leg** is the part of the pre-release checks that the orchestrator runs on the
  owner's Mac, because it needs a real model or a provider's key.

## What the spec says, and where it is silent

**What it says.** Each of these binds the tasks below.

- Section 3, "Primitives, recipes and loops": actions come at three levels, and the third is
  the "Loop across tools", whose example is "remove, check, retry", called by "Agents and
  apps". "A recipe that spans tools lives with the tool that owns the goal:
  remove-then-check is rascal's." "A tool that needs an edit evaluated calls squint's
  checking recipe; it does not choose squint's tools."
- Section 3, "Nested calls": a tool reaches another only through the executor in its
  context, "which passes down a child allowance and refuses calls deeper than four levels or
  back into an action already on the stack".
- Section 3, "The execution contract", "Cancellation": "Cancelling a run cancels its
  descendants and blocks new ones. A parent becomes terminal only when its descendants are
  terminal and every dispatched request is accounted for."
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
- Section 14, "GitHub Actions": `prerelease` runs "On a release candidate tag" and contains
  "Model tier; a live call to each provider; install from the built archive on both
  targets. A release is refused unless this passed on the same commit". `release` runs "On a
  version tag" and contains "Binaries for both targets and the pack".
- Section 15: agents may "tag, cut version releases", "provided CI and both scans pass
  first"; a new recipient for data is the owner's.

**Where it is silent, or disagrees with what was built.** Each is an item under "Open
questions" with a recommended answer; none is settled silently.

1. The spec does not name the loop, say how many attempts it makes, what a retry changes,
   which of squint's use cases checks a removal, what is handed back when every attempt
   fails, or what happens when no judge is ready. Rascal decided all of these (its ADR 0023,
   "A bounded check-and-retry for remove", and `crates/rascal-mask/src/heal.rs`,
   `retry.rs`, `checker/` at the commit `docs/sources.toml` pins). Section 7 says the item
   is "Kept", so this plan takes rascal's rules as the recommended answers; the old repos
   are "reference, never authority" (`AGENTS.md`), so each is asked.
2. A recipe file (`recipe/1`, ADR 0033) is a straight sequence of steps: it cannot branch
   or repeat. The spec does not say whether a loop is a file or code.
3. Rascal's retry is aimed at the places its checker named. In whizbang a verdict handed to
   a caller is a decision and a reason (`contract::Verdict`); the places are only in the
   diagnostic `judged.json`, and a diagnostic can never be used as an output.
4. Calls nest four levels. The loop, squint's use case recipe, its step `check` and a tool
   the judge calls are four, so the loop fills every level and nothing may call the loop.
5. The spec does not say what the loop does when its reservation, which covers every
   attempt, is more than the call cap. Three attempts with one of the dearer judges reserve
   about 3.02 US dollars, and the config guide's examples cap a call at 2.00 or 2.50.
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

## How the work is split

Eight pull requests, where the owner asked for few and large. Sizes are changed lines of
source, tests and hand-written documents, by the rate milestone 3 measured (about 2,500 for
a large pull request); generated reference is counted apart, as in milestone 3.

| Pull request | What | Builds on | Size |
|---|---|---|---|
| A1 | Details first is no longer `check`'s default; it stays selectable | `main` | Small: about 700 |
| L1 | The loop: `remove_checked`, its attempts, its one reservation, its record, cancellation, and what it hands back | A1 | Large: about 2,500 (the action 800, the record's new version 400, the command and the server 300, the guide 150, tests 850) |
| P1 | What the located retry needs from below: `check` hands back the places it cited; a recipe run from a named step; a mask carried from a crop to its picture | `main` | Large: about 2,100 (places 600, the runner 450, the carry 450, tests 600) |
| L2 | The located retry: a retry aimed at the places the check named | L1, P1 | Medium: about 1,500 |
| R1 | The archive, its checksums, the install check, and both in CI's `release` job on each runner | `main` | Large: about 1,900 (scripts 500, their tests 650, the licences of what the binary holds 250, the install guide 200, CI 100, the binary's version line 200) |
| R2 | The `prerelease` and `release` workflows, the local leg's script, and the rule that refuses a release without a passed candidate | R1 | Medium: about 1,500 |
| F1 | The rows left: the lab's progress and counts, the pairs page, an executor built with a digest alone | `main` | Medium: about 1,600 |
| N1 | The live evidence, the loop's record, the candidate, the rows closed, and the note | all of the above | Medium: about 1,100, no product code beyond the record's hooks |

**Waves.** Four, each started from `main` once what it builds on has landed.

1. **A1, P1, R1 and F1** from `main`, side by side.
2. **L1** when A1 has landed; **R2** when R1 has landed.
3. **L2** when L1 and P1 have landed.
4. **N1** when the other seven have landed.

A1 is small on purpose: it lands within the first day, so L1 waits for little.

**What can run side by side, and what cannot.** The loop's strand (A1, L1, P1, L2) and the
release's strand (R1, R2) share no source file but the binary's command list, and run side
by side from start to end. F1 touches the lab and the tests' manifests only. Inside the
loop's strand, P1 runs beside A1 and L1; L2 needs both L1 and P1.

**Shared files.** `crates/whizbang-squint/src/check.rs`: A1 does not touch it (the default
it changes is in the five recipe files and in the command); P1 owns it. The recipe runner
(`crates/whizbang-core/src/recipe/runner.rs`): P1 owns it; L1 only calls it. The run record
(`crates/whizbang-core/src/contract/record.rs` and its schema): L1 owns the new version; L2
adds its fields to L1's list. `.github/workflows/ci.yml` and the `justfile`: R1 owns them in
wave 1, R2 in wave 2; F1 touches neither. The generated reference (`docs/reference/`) is
made again by `just docs` after merging `main`, never merged by hand; in each wave the
second to land merges `main` (a merge commit), runs `just docs` again and reruns its checks.

**Seams.** Each pull request names where it splits if it passes about 2,500 changed lines,
into a second pull request of its own that lands before anything that builds on it: L1, the
server's side and `runs stats` (its items 10 and 11); P1, the carry of a mask from a crop
(its item 3); R1, the licences of what the binary holds (its item 3); R2, the `release`
workflow (its item 4); F1, the executor built with a digest alone (its item 4). A split is
said in the hand-back and the owner is told the count. Nothing of the row is dropped to keep
the count.

**Review rule.** As in milestones 2 and 3: a separate reviewer reads a pull request only for
the handling of a token or key, egress (what leaves the machine, to whom, and in what form),
processes or threads left running, a user's own code or files that whizbang runs or reads,
and what ships in the binary or the release. L1 gets one (nested calls under one
reservation; the result of a removal sent to a judge). P1 gets one for the places (a
model's words that steer a paid retry are data, bounded and held, never instruction). R1
and R2 each get one (what ships; a workflow that may write a release; the token it runs
with). A1, L2 and F1 are checked by the orchestrator. Each is merged on green CI, with
`just scan-range` over `base..head`. A "Done when" is a list of tests by name.

**Test time.** The limit is 300 seconds of test time on every runner and on the owner's Mac
(`SPEC.md` section 13); the hosted macOS runner has taken 60 to 260 seconds. Milestone 4 may
add at most 25 seconds there, about 3 seconds a pull request and 8 for L1. Every test that
calls a provider, a model or the network beyond loopback is `#[ignore]`d and run by hand or
by `just test-models` on the Mac, never by CI. A loop in a test runs over the fake
segmenter, a scripted filler and a scripted judge, on synthetic pictures of 64 by 64,
through the production path. Each pull request reports its `just check` and CI test times, and stops and
says so if a run passes about 250 seconds.

**Live runs are the orchestrator's.** A builder makes no provider call and loads no real
model in CI. Every live run this plan asks for is made by the orchestrator on the owner's
Mac, from the command the builder hands back with the branch's head, and its record is
committed by the orchestrator or by the builder from what the orchestrator gives it.

**Spend.** The owner's standing allowance is 100 US dollars a milestone; milestone 4 has
spent nothing. The live runs come to about 30 dollars, itemised under "Spend".

## A1. Details first is no longer `check`'s default (small)

### Goal

"Details first" means that before the judge's first turn, `check` runs `align` (and any
other tool named with `--detail`) on each pair of images the use case relates, and opens the
conversation with what they measured (ADR 0029). Its gate rejected it twice on 2026-11-19
(`docs/results/m3-gates.md`): one judge did worse with it and one did not change. It is
still the default of `whizbang squint check` and of the five use case recipes. The
carry-over row gives it to milestone E, "first thing"; this plan takes it now, because L1's
loop calls
squint's checking recipe on every attempt and would otherwise pay for, and be judged under,
a setting its own gate refused. Nothing about details first is removed: it stays one flag
away.

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

- [ ] Done when:
  `use_case::each_shipped_recipe_measures_no_details_unless_a_run_asks`,
  `cli::squint_check_measures_no_details_by_default_and_first_when_asked`,
  `check::the_default_offers_the_judge_what_the_gates_control_arm_offered`,
  `serve::a_use_case_called_without_details_measures_none`,
  `evaluation::an_arm_states_its_own_details_whatever_the_default_is`,
  `use_case_records::each_use_case_record_holds_two_worked_examples_that_its_recipe_gives`
  (on the records made again),
  `evidence::the_record_of_check_says_details_first_is_off_and_names_its_gate`, `just docs`
  regenerated, and the carry-over row closed.

**Keeping tests lean (A1).** No new picture; each test is a scripted judge on the pictures
the use cases' tests already draw.

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
at the places the check named.

1. **The action.** `remove_checked`, of the kind `Loop` (`contract::Kind::Loop`, which the
   descriptor already has and no shipped action yet uses), in `whizbang-rascal`, because
   the goal is rascal's (`SPEC.md` section 3). It is written in Rust, not as a recipe file:
   `recipe/1` has no branch and no repeat, and this plan adds neither (open question 10). A
   new ADR, "The loop: remove, check, retry", records every decision of this task. Its
   inputs are `remove_prompt`'s (an image, a phrase, an optional mask to protect, the
   segmenter, the filler) and the judge. `remove_prompt` itself is unchanged: one pass,
   unchecked.
2. **An attempt is two nested calls.** First `remove_prompt`, the shipped recipe, through
   the executor in the loop's context: its own run, its own record, its recipe's digest.
   Then squint's checking recipe `source-fidelity`, with the result as the candidate and
   the picture the loop was given as the source, under a job the loop writes from the
   phrase by a fixed template (`job/1`, ADR 0028): the brief says what was to be removed,
   and its requirements are that the named thing is gone, that nothing else changed, and
   that the filled area looks like its surroundings. The template is versioned, and its
   digest is in the loop's record. The loop calls the recipe and nothing under it: it does
   not choose squint's tools (`SPEC.md` section 3). The check receives what it needs (its
   images, its job, its judge and its share of the allowance) as a nested call's arguments,
   not through the environment of a second program, which is what the carry-over row "A
   check cannot receive the variables it needs" asks.
3. **How many attempts.** At most `attempts`, from 1 to 3, 3 by default, the first
   included: rascal's limit (its `MAX_TRIES`), kept (open question 4).
4. **What ends the attempts.** A verdict of `pass`. The last attempt allowed. A verdict of
   `unsure` (in L1 every `unsure` ends them; L2 narrows this to an `unsure` that names no
   place: rascal found that widening after such an answer "repeated the same fill at the
   same price"). A check that could not run. A retry whose mask is the same as the one
   before it, which is not filled again. A retry whose removal could not be made or filled:
   the attempt before it is kept.
5. **What a retry changes, in L1.** After a `fail`, the next attempt runs `remove_prompt`
   again with its step `grow` given twice the `px` (the runner's own override, `set`), once
   in a run: rascal's "a `fail` that names no place widens the mask once". Nothing else
   changes between attempts: the same phrase, segmenter, filler and judge. What changed is
   written in the record (item 8).
6. **One budget.** The loop's descriptor states what every attempt can need: for each
   attempt allowed, one run of `remove_prompt` and one of `source-fidelity`. Prepare gives
   the bound of the whole loop; admission reserves it once, before any request is sent
   (`SPEC.md` section 6). Each nested call takes its share from the loop's allowance and
   reserves nothing of its own (ADR 0008, "Allowances"); what a child did not spend comes
   back to the loop when the child ends (ADR 0016, "An allowance holds a signed balance"),
   so the reservation covers every attempt at its bound and, when nothing overruns, cannot
   run out part-way.
   - **When the bound is over the call cap.** Prepare fits the attempts to the cap: the
     loop is prepared with the most attempts, up to the number asked, whose whole bound is
     within `spend.call_cap_usd`, and says how many were asked and how many the cap allows.
     When the cap covers a removal and no check, the loop is prepared as one attempt,
     unchecked, with the reason "the call cap does not cover a check". When it does not
     cover one removal, the call is refused as any call over its cap is. This is rascal's
     rule ("Tries are reserved up front within the spending limit") and the spec's lean
     toward letting work run; the other answer, to refuse the call and name the cap, is
     open question 7.
   - **When the store has less left than the bound.** Admission is refused, as for any
     call. Nothing is fitted at admission: an admitted call is exactly the call that was
     prepared.
   - **When the allowance runs out in the middle.** This needs an overrun: a request
     charged over its bound puts the runs above it in debt (ADR 0016). The next nested call
     is then refused for want of allowance (`Refusal::OverCaller`), and the refusal is in
     the loop's record. The loop starts no further call, keeps the attempts that completed,
     and ends as item 7 says, with the reason "the budget ran out after attempt N". When
     the refusal comes before the first removal has completed, the loop ends as an error of
     the type `budget`, with the refusal's own sentence.
   - **The guide gives real figures** (carry-over row "The documented per-call cap for a
     check is one cent"): `docs/guides/loop.md` states the bound of one attempt and of
     three for each judge in the catalogue, worked from the ceilings the config guide
     already gives: a check reserves 0.112768 dollars with `claude-haiku-5-5`, 0.72576 with
     `gemini-3.8-flash` and 0.96768 with `gpt-6.1-terra`, and a fill 0.04; so three attempts
     reserve about 0.46, 2.30 and 3.02 dollars, and what is settled is far less.
7. **What is handed back.** The loop completes whenever at least one removal completed, and
   commits one attempt's outputs as its own: the filled picture and the mask rebound onto
   it. Which attempt is kept is rascal's rule: the first that passed; failing that, an
   `unsure` before a `fail`, and either before an attempt whose check could not run; then
   the attempt with fewer places named; then the earlier one. With the outcome the caller
   is handed the kept attempt's verdict, when it has one, and a short result that says
   whether the kept attempt was checked, why not when it was not, how many attempts were
   asked, allowed and made, which was kept, and why the attempts ended. So **when every
   attempt fails, the loop still completes**, hands back the best attempt it made, and says
   `fail` or `unsure` with it: the caller decides what to do with a picture the judge did
   not pass (open question 6). The loop ends as an error only when no removal completed:
   the first `remove_prompt` failed, and its own error type and message are the loop's
   cause.
8. **The record.** A new version of the run record, `run/6` (by the rule of ADR 0005, with
   its schema and fixed vectors, reading `run/5`): beside the steps every run lists (each
   nested call in order, with the child's run), a loop's record holds its attempts: for
   each, its number, the run of its removal, the run of its check, the verdict or the
   reason there is none, what the retry changed to make it (the wider `grow`; L2 adds the
   crops and the masks they gave), and what its requests settled; then which attempt was
   kept and why the attempts ended; the attempts asked and allowed; and the digest of the
   job's template. A check that ended in error is recorded with its own error type and
   message, not a word of the loop's (carry-over row "Keep the cause when a check fails").
9. **Unchecked, and saying so.** `--check auto`, the default: when no judge is configured,
   or its key is not in the environment, or the call cap does not cover a check, the loop
   makes one attempt, unchecked, and says why: rascal's "A missing checker key never blocks
   the remove". `--check required`: the same call is refused at prepare, naming what is
   missing. `--check off`: one attempt, no judge asked, nothing reserved for one. A check
   that was started and ended in error leaves its attempt unchecked, and the attempts end.
   On the terminal, each attempt is said on the error stream as it starts and ends, and the
   last lines say whether the result was checked, the verdict, and the reason when it was
   not (carry-over row "Say on the terminal when a run was not checked").
10. **The doors.** `describe` lists the loop after the recipes and before the primitives
    (the description already orders by kind). `whizbang serve` offers it as a tool, and its
    answer carries the verdict and the short result of item 7. Exit codes follow `whizbang
    squint check`'s for a verdict, and the builder states them in the guide.
11. **Metrics.** `whizbang runs stats` over the loop counts its runs by how the attempts
    ended, the attempts made, and the retries (`SPEC.md` section 3, "retries and repairs").
12. **Cancellation.** The loop looks at its signal before each nested call and starts no
    other once it is set; cancelling the loop cancels the child that runs (`SPEC.md` section
    3). A cancelled loop commits no outputs. The attempts that completed before it are in
    its record, each child's outputs "recorded as produced under" the cancelled loop. Those
    pictures are names of the loop's own tree and no later call can take one (ADR 0008; plan
    m3, task Q1, item 3): open question 12 asks whether that is enough for now.
13. **Levels.** The loop is at the first level, `source-fidelity` at the second, its step
    `check` at the third, and a tool the judge calls at the fourth, the last there is. So a
    recipe that names a loop as a step is refused when it is read, with a sentence saying
    why, instead of failing deep inside a paid run (open question 10).

- [ ] Done when:
  `checked::a_first_attempt_that_passes_is_one_removal_and_one_check`,
  `checked::a_fail_widens_the_mask_once_and_the_second_attempt_is_checked`,
  `checked::an_unsure_ends_the_attempts`,
  `checked::the_attempts_never_pass_the_number_asked_nor_three`,
  `checked::when_every_attempt_fails_the_loop_completes_with_the_kept_attempt_and_its_verdict`,
  `checked::the_kept_attempt_is_the_first_pass_then_unsure_then_fewer_places_then_the_earlier`,
  `checked::a_retry_whose_mask_is_the_one_before_is_not_filled_again`,
  `checked::a_retry_that_cannot_be_filled_keeps_the_attempt_before_it`,
  `checked::a_first_removal_that_fails_is_the_loops_error_with_the_childs_own_cause`,
  `checked::a_check_that_ends_in_error_leaves_its_attempt_unchecked_with_the_checks_own_error`,
  `checked::a_judge_that_is_not_ready_gives_one_attempt_unchecked_and_the_reason`,
  `checked::check_required_is_refused_at_prepare_naming_what_is_missing`,
  `checked::check_off_asks_no_judge_and_reserves_nothing_for_one`,
  `checked::the_job_the_loop_writes_is_valid_and_names_the_phrase`,
  `checked::the_check_is_squints_recipe_and_the_loop_calls_nothing_under_it`,
  `budget::the_loop_reserves_once_for_every_attempt_and_each_child_draws_on_it`,
  `budget::what_a_child_did_not_spend_is_back_before_the_next_attempt`,
  `budget::the_attempts_are_fitted_to_the_call_cap_and_the_result_says_asked_and_allowed`,
  `budget::a_cap_that_covers_no_check_gives_one_attempt_unchecked`,
  `budget::a_child_refused_for_want_of_allowance_ends_the_attempts_and_keeps_what_completed`,
  `checked::a_cancelled_loop_cancels_its_running_child_starts_no_other_and_commits_nothing`,
  `checked::attempts_completed_before_a_cancellation_are_in_the_record`,
  `record::a_loops_record_lists_each_attempt_its_runs_its_verdict_and_what_the_retry_changed`,
  `record::run_6_reads_a_record_of_run_5`,
  `nested::the_deepest_call_of_a_checked_removal_is_at_the_fourth_level_and_is_admitted`,
  `recipe::a_recipe_that_names_a_loop_as_a_step_is_refused_when_read`,
  `describe::the_loop_is_listed_after_the_recipes_and_before_the_primitives`,
  `serve::the_loop_is_offered_as_a_tool_and_answers_with_its_verdict_and_whether_it_was_checked`,
  `cli::rascal_remove_checked_says_each_attempt_and_whether_the_result_was_checked`,
  `stats::runs_stats_counts_a_loops_attempts_and_retries`,
  `guides::the_loops_guide_states_the_bound_of_an_attempt_for_each_judge_of_the_catalogue`,
  `model_tier::remove_checked_makes_its_mask_with_sam3_under_a_scripted_judge` (`#[ignore]`,
  under `just test-models`), `just docs` regenerated, and the four carry-over rows named
  above closed with a line each. The loop's tool record is N1's, because its numbers come
  from the live step.

**Keeping tests lean (L1).** One fixture builds the loop over the fake segmenter, a scripted
filler and a scripted judge whose verdicts a test lists in order; pictures of 64 by 64; no
test waits on a clock.

### Origins (L)

None new. The loop sends what `remove_prompt` and `check` already send, to the recipients
already approved: the filler (`https://fal.run`) and the configured judge.

## P1. What the located retry needs from below (large)

### Goal

Three additions below the loop, each useful alone and none of them a loop: the places a
check cited reach its caller; a recipe can be run from a named step; and a mask made on a
crop of a picture can be carried onto the picture. L2 puts them together.

1. **`check` hands back its places.** A third kind of result beside artifacts, `findings`
   (ADR 0035, amended; a new description version): one JSON document, at most 32 KiB, that
   lists each checklist item the verdict counts against a pass, with its disposition and,
   when the judge cited one, its place: the image it is on and a box or a polygon in
   fractions of that image. It is made from what `check` already keeps in `judged.json`,
   by the same code, so the two cannot differ. A recipe whose last step declares findings
   hands them to its caller with the verdict, as it does the verdict. **A place is data
   from a model:** it is held to a count (at most eight are handed back; the rest are
   counted), to the image it names (one of the run's own), and to numbers inside the
   picture; no text of the judge's travels with it. The executor refuses findings from an
   action that does not declare them, as it does a verdict.
2. **A recipe run from a named step.** The runner gains `from`, the mirror of `until` (ADR
   0033, amended): a run names a step and gives artifacts in place of the outputs of the
   steps before it; those steps do not run, nothing is reserved for them, and the run's
   record says where it started and what it was given. It is refused before anything runs
   when the recipe has no such step, when a later step needs an output that was not given,
   or when a given artifact is not of the port's type. This is how rascal rebuilt a mask
   "through the same shaping": `remove_prompt` from its step `select`, with the instance
   masks given.
3. **A mask carried from a crop to its picture.** `view` cuts a crop bound onto its source,
   and `segment` on the crop gives masks that belong to the crop. Using one on the picture
   "needs an explicit, checked rebind" (`SPEC.md` section 3, "Identities"; ADR 0002). The
   carry is ported from rascal (`crates/rascal-ops/src/invert.rs`, `map_plane_to_parent`,
   by `port-module`): a mask on a crop becomes a mask on the picture the crop was cut from,
   empty outside the crop, refused when the crop is not of that picture. It is offered where
   rascal offered it, as `invert`'s `to`, and as whatever form L2's retry needs, which the
   builder decides in the ADR of item 2. With it the crop half of the carry-over row on
   `invert`'s `to` closes, on rascal's vector of a crop; the half about a cut canvas is
   re-owned with a line, to the first shipped recipe that inverts a mask on a cut, since
   none does (the table of rows, below).

- [ ] Done when:
  `check::the_places_handed_back_are_those_judged_json_holds`,
  `check::a_place_on_an_image_the_run_was_not_given_is_left_out_and_counted`,
  `check::no_more_than_eight_places_are_handed_back_and_no_text_of_the_judges`,
  `executor::findings_from_an_action_that_does_not_declare_them_end_the_run_as_an_error`,
  `use_case::a_use_case_recipe_hands_its_callers_the_findings_of_its_last_step`,
  `describe::the_description_lists_findings_among_an_actions_results`,
  `runner::a_run_from_a_step_runs_that_step_and_those_after_it_with_what_it_was_given`,
  `runner::a_run_from_a_step_reserves_nothing_for_the_steps_before_it`,
  `runner::a_run_from_a_step_that_lacks_an_output_a_later_step_needs_is_refused_before_anything_runs`,
  `record::a_run_from_a_step_says_where_it_started_and_what_it_was_given`,
  `carry::a_mask_on_a_crop_lands_on_its_picture_and_is_empty_outside_the_crop`,
  `carry::a_mask_on_a_crop_of_another_picture_is_refused`,
  `invert::invert_from_a_crop_mask_lands_protect_on_its_bbox` (rascal's vector), `just docs`
  regenerated, and the row on `invert`'s `to` narrowed with a line.

**Keeping tests lean (P1).** The places are read from scripted submissions; the runner's
tests use the fake segmenter; the carry is exact arithmetic on masks of 32 by 32.

## L2. The located retry (medium)

### Goal

A retry aimed at what the check saw, rascal's second kind of retry (its `retry.rs`): when a
check says `fail`, or says `unsure` and names a place, the next attempt looks for the named
thing again inside each place, adds what it finds to the mask, and fills again.

1. **From places to masks.** For each place the check named on the result, at most eight:
   the picture the loop was given is cut to the place's box, widened by five hundredths of
   its size on each side (rascal's crop pad), with `view`; `segment` is asked the same
   phrase inside the crop; each mask it gives is carried onto the picture (P1, item 3). A
   place where the segmenter finds nothing joins as itself, rasterised. Rascal also halved
   the segmenter's threshold inside a crop; whizbang's `segment` takes no threshold, so the
   phrase is asked as it is, and a threshold is added only if N1's live step shows crops
   that find nothing.
2. **The next attempt.** `remove_prompt` is run from its step `select` (P1, item 2) with
   every mask so far: the first attempt's instances and what the places added. The shaping
   and the fill are the recipe's own, unchanged. The source of every attempt is the picture
   the loop was given, never an earlier attempt's result, so no retry paints over a fill.
3. **Which retry.** A `fail` or an `unsure` that names places: the located retry. A `fail`
   that names none: L1's wider mask, once. An `unsure` that names none: the attempts end.
4. **The budget.** Each attempt allowed after the first adds to the loop's bound at most
   eight requests of the segmenter, which cost nothing on the Mac and will have a price on
   a GPU host (milestone 6).
5. **The record.** An attempt made by a located retry lists the crops it cut, as boxes on
   the picture, and the masks they gave, by identity (rascal's `Transition`).

- [ ] Done when:
  `located::a_fail_that_names_a_place_segments_inside_it_and_the_mask_grows_there`,
  `located::a_place_where_nothing_is_found_joins_as_itself`,
  `located::an_unsure_that_names_a_place_is_retried_and_one_that_names_none_ends_the_attempts`,
  `located::a_fail_that_names_no_place_widens_once`,
  `located::every_attempt_fills_the_picture_the_loop_was_given`,
  `located::the_retry_runs_remove_prompt_from_select_and_its_shaping_is_the_recipes`,
  `located::no_more_than_eight_places_are_segmented_and_the_bound_covers_them`,
  `record::a_located_retry_lists_its_crops_and_the_masks_they_gave`,
  `cli::rascal_remove_checked_retries_inside_a_named_place_under_a_scripted_judge`,
  `model_tier::a_located_retry_finds_the_phrase_inside_a_crop_with_sam3` (`#[ignore]`,
  under `just test-models`), and `just docs` regenerated.

**Keeping tests lean (L2).** L1's fixture, with a scripted judge that names a box.

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
   `README.md`, the install guide of item 5. Nothing else: the pack, the recipes, the
   rules, the constant sets and the catalogue are inside the binary already. Beside it the
   script writes a line of `SHA256SUMS`, the archive's SHA-256 and its name. The same
   sources give the same folder: entries are sorted and their times and owners are fixed.
2. **What the binary may need from the machine.** On Linux, only the system's C library at
   the version Ubuntu 22.04 ships or older, and no other shared library that is not part of
   that system: held by a test that reads the binary's own list. On the Mac, a minimum
   system of macOS 15, set when it is built and read back from the binary.
3. **The licences of what the binary holds.** A release hands other people's code to a
   third party, so the archive carries the licence text of every crate the binary links,
   generated from the lock file by a script and held to the list `cargo deny` checks: a
   crate in the binary with no text in the file fails `just check`. The model's licence is
   not here: no model is in the archive.
4. **The install check**, `scripts/install-check.sh <archive> <sums>`, does what the guide
   tells a person to do, and nothing a person could not: in an empty folder, with a cleared
   environment and a temporary home, and with no Rust toolchain on its path, it checks the
   SHA-256, unpacks, holds the list of files to item 1, and then runs the installed binary:
   - `whizbang --version` gives the version the archive is named for, and the commit;
   - `whizbang describe --json` equals the committed `docs/reference/description.json`, so
     the binary installed is the binary documented;
   - **a run through the executor, with no model:** a picture is imported into a new store,
     the recipe `mask_rect` is run on it, and `runs show` gives a completed run that lists
     its steps: the store, the runner, a recipe and the record, end to end;
   - **the server:** `whizbang serve` is started, answers `initialize`, lists its tools,
     answers one call of `mask_rect`, and ends when its input closes; the script kills and
     waits for it on every path;
   - **the pack:** `whizbang setup` installs the pack's core, offline, with the pinned `uv`
     and Python the guide names, and the worker says hello;
   - `whizbang backends status` answers, with no network;
   - **what this target cannot do is refused in its own words.** On Linux: `whizbang setup
     --extra torch` is refused before anything is downloaded, and `whizbang rascal
     remove-checked` and `remove-prompt` are refused at prepare, naming the segmenter that
     is not ready. The check for the loop is added by L1 or R1, whichever lands second.
5. **The install guide**, `docs/guides/install.md`, and its copy in the archive: download,
   check the SHA-256, unpack, put the binary on the path; what each target can do (below);
   that a Mac's browser marks a downloaded file and how to download so that it does not
   (open question 9); and that SAM 3 is fetched from its maker with the user's own sign-in
   (open question 8).
6. **In CI.** The `release` job of each runner, which already builds the release binary,
   runs `just package` and the install check on what it built. A change that breaks the
   archive fails its own pull request.

**What each archive can and cannot do.**

| | `aarch64-apple-darwin` | `x86_64-unknown-linux-gnu` |
|---|---|---|
| Every operation and recipe that uses no model; the store, runs, spend, export and import; `serve` over stdio; the evaluation and lab commands | Yes | Yes |
| Fills through fal; the judges of the three providers; `squint check`; `remove` and the repairs given a location | Yes, with keys | Yes, with keys |
| The pack's core and its worker | Yes | Yes |
| A local model: `segment` on SAM 3, and so `remove_prompt` and the loop | Yes, after `whizbang setup --extra torch` and `whizbang models get sam3` | No. The runtime is refused on Linux (`SPEC.md` section 5; the carry-over row "Linux runtime extra", deferred by the owner on 2026-11-18: production runs models on an external GPU host). They come back with a remote segmenter, milestone 6 |

**How the Linux archive's test proves "runs".** By item 4 on the `ubuntu-22.04` runner: the
installed binary executes a recipe end to end through the executor and the store, serves a
call over stdio, and installs the pack's core and hears its worker; and it refuses, in a
sentence that names the reason, each thing that needs a local model. It makes no paid call:
the paths to fal and to the judges are the same code the fast tier runs on Linux against
recorded exchanges, and no key is given to a hosted runner. That this is enough for "runs"
on Linux is open question 2.

- [ ] Done when: `scripts/tests/test-package.sh` (the archive holds exactly the four files;
  two builds of one tree give the same list and the same sums line; the sums line matches
  the archive), `scripts/tests/test-install-check.sh` (the check passes on a good archive;
  it fails on an archive whose sum is wrong, on one with a file too many, on a binary of
  another version, and on a server that does not end, which it kills),
  `scripts/tests/test-third-party-licences.sh` (a crate in the binary without its text
  fails), `cli::the_version_line_names_the_version_and_the_commit`,
  `release::the_linux_binary_needs_only_the_c_library_of_ubuntu_22_04`,
  `release::the_mac_binary_states_macos_15_as_its_minimum`,
  `scripts/tests/test-ci-jobs.sh` holding the `release` job to `just package` and the
  install check, and CI green with the install check passing on `macos-15` and on
  `ubuntu-22.04`.

**Keeping tests lean (R1).** The script tests of the check run on a tiny stand-in binary
(a shell script named `whizbang`), not on a release build; the real check runs once on each
runner in the `release` job, which has the binary already.

### Origins (R)

None for the build. The install check contacts nothing. The release build downloads what CI
already downloads: crates from crates.io, the pinned tools, the pinned Python.

## R2. The `prerelease` and `release` workflows (medium)

### Goal

A candidate is built once, checked on both targets and on the owner's Mac, and the bytes
that were checked are the bytes that are released (`SPEC.md` section 14, "GitHub Actions").

1. **Versions and tags.** The workspace's version becomes the release's (open question 11
   recommends `0.1.0`). A candidate is the tag `v<version>-rc.<n>`; a release is the tag
   `v<version>` on the same commit. Only the orchestrator tags, on `main`, after CI and
   both scans pass on that commit (`AGENTS.md`). A tag whose version is not the binary's is
   refused by both workflows.
2. **`prerelease`**, on a candidate tag. Jobs:
   - `build`, on `macos-15` and on `ubuntu-22.04`: `just package`; the archive and its sums
     line are kept as the workflow's artifacts.
   - `install`, on each: a fresh job that checks out the scripts and nothing built,
     downloads the archive, and runs R1's install check. On Linux it runs inside a bare
     Ubuntu 22.04 container, so nothing a hosted runner happens to have installed can
     stand in for something the archive forgot.
   - `candidate`: when all four pass, makes a GitHub pre-release for the tag, a draft, with
     both archives and `SHA256SUMS`. This is the only job with leave to write, and the only
     thing it may write is a release.
3. **The local leg**, `scripts/prerelease-local.sh <tag>`, the orchestrator's, on the
   owner's Mac: it downloads the Mac's archive from the candidate, checks it against
   `SHA256SUMS`, installs it into a temporary folder, and runs, with the installed binary
   and the owner's env file: the model tier (`just test-models`, and `remove-prompt` on
   SAM 3 through the installed binary); one check by each of the three providers' judges
   and one fill through fal, on synthetic pictures; and the loop once. It writes one file of
   fields (the tag, the commit, both archives' digests, and for each call the model, the
   outcome, the verdict, the tokens and what was settled; no reply text, no path), and
   attaches it to the candidate. The script tests run it against a stand-in binary and a
   stand-in for the `gh` command; it is never run by CI.
4. **`release`**, on a version tag. It builds nothing. It is refused unless, for the tag's
   commit, a `prerelease` run ended in success and the candidate holds the local leg's file
   naming that commit and those archives' digests: section 14's "A release is refused
   unless this passed on the same commit". It then publishes the candidate's own archives
   and `SHA256SUMS` under the version tag.
5. **The spec's table.** Section 14's rows for `prerelease` and `release` are brought in
   line: the model tier and the live calls are the local leg, on the owner's Mac, and the
   release holds the two archives, the pack being inside the binary. This edit changes what
   a workflow holds, so it is reviewed with this plan's review rounds and made only after
   the owner's yes to open question 1.

- [ ] Done when: `scripts/tests/test-release-workflows.sh` (each workflow runs only on its
  kind of tag; `prerelease`'s jobs call `just package` and the install check and nothing
  else builds; only `candidate` may write; `release` builds nothing),
  `scripts/tests/test-release-gate.sh` (the release is refused when no `prerelease` run
  succeeded on the commit, when the local leg's file is missing, when it names another
  commit, and when a digest differs; it passes when all agree),
  `scripts/tests/test-prerelease-local.sh` (against stand-ins: a wrong sum stops it before
  anything runs; a failed call fails the leg; the file it writes holds only the fields),
  `scripts/tests/test-tag-version.sh` (a tag of another version than the binary's is
  refused), the spec's table edited, and the first real run of `prerelease`, which is N1's
  box, not this one.

**Why the first real run is N1's.** A workflow that runs on a tag cannot run before a tag
exists, and a builder does not tag. R2 is held by tests of its scripts and of the workflow
files' shape; N1 tags the first candidate and fixes what the real run finds.

## F1. The rows left (medium)

### Goal

The carry-over rows that name milestone 4 and belong to neither strand.

1. **`whizbang lab run` says what it is doing.** Each cell is said on the error stream as it
   starts and as it ends, so a round of a hundred fills is not silent for many minutes.
2. **"Started now" counts only what a leg started.** A request the store had already
   answered is printed apart, with a test of a resumed round whose earlier attempts are
   answered as repeats.
3. **The pairs page.** Each picture of a pair is written so that its size on disk does not
   tell its arm, and a backspace takes back the last pick. Milestone E's picking rounds are
   drawn again from the kept stores after this lands.
4. **An executor built with a digest alone** (`Engine::over`) is compiled for tests alone,
   as `OneWorker::starting` is, or backed by one walk of the config, as the row allows.

- [ ] Done when: `lab::each_cell_is_said_as_it_starts_and_as_it_ends`,
  `lab::a_resumed_round_counts_only_what_it_started_and_prints_repeats_apart`,
  `pairs_page::a_pictures_size_on_disk_does_not_tell_its_arm`,
  `pairs_page::a_backspace_takes_back_the_last_pick`,
  `asked::an_executor_built_with_a_digest_alone_is_a_tests_alone` (or the test of the one
  walk, as the builder chose), `scripts/tests/test-features.sh` and
  `scripts/tests/test-crate-graph.sh` passing with the new feature, and the four rows
  closed.

## N1. The live evidence, the candidate and the note (medium; the orchestrator's runs)

### Goal

Show the row on real models, write down what the loop does and does not do, pass the first
candidate, and close the milestone. Every run here is the orchestrator's; the builder of N1
writes the files that hold what the orchestrator hands it.

1. **The loop on a drawn set.** A lab, `labs/m4-loop/`, on milestone 3's synthetic edit set
   (`labs/m3-edits/`: 24 pictures of `source-fidelity/scenes-v1`, each with a drawn red
   ball and the phrase "a red ball"), its protocol frozen before it runs. Arms:
   `remove_prompt`, one pass, as the reference; `remove_checked` with `claude-haiku-5-5` as
   the judge; `remove_checked` with `gpt-6.1-terra`; and `remove_checked` with Haiku on a
   first mask made too tight on purpose (the first attempt's `grow` set to nothing), so that
   retries really happen on a set whose masks otherwise cover the defect whole. Recorded in
   `docs/results/m4-loop.md`, aggregates only: for each arm, how the attempts ended (passed
   at first, passed after a retry, kept `unsure`, kept `fail`, unchecked), the attempts made,
   which kind of retry, and, by milestone 3's scoring program (`whizbang eval edits
   score`), the coverage of the defect and the fill's error for the kept attempt beside the
   first attempt's. It is descriptive: it admits nothing.
2. **Three cases by hand**, from the command line, the cases the carry-over list keeps for
   milestone 5's playground: a clean pass, or a retry then a pass; a vague phrase, showing
   its attempts and that it was not passed; an unset judge key, leaving the run unchecked
   and saying why. On pictures of the owner's if the owner gives some (below), else on
   synthetic ones. Their fields go in the same record.
3. **The loop's tool record**, `docs/reference/tools/remove_checked.md` (`SPEC.md` section
   8: "A tool is not finished ... without its record"): what it does, when to use it and
   when not, two worked examples from scripted runs (one of them a case it handles badly),
   rung 1 from those, rung 2 from item 1, its cost for each judge, its limits, and its
   mark: experimental. Its visual quality reads "not yet confirmed by a person", as the
   other fill recipes' does; rungs 3 and 4 are milestone E's.
4. **The first candidate.** With `main` holding everything above: the orchestrator tags
   `v<version>-rc.1`, `prerelease` runs, the local leg runs, and each thing a real run
   finds is fixed in a pull request of its own and tagged as the next candidate. When a
   candidate passes whole, its fields are recorded in `docs/results/m4-release.md`: the
   tag, the commit, the archives' digests, each job's result, and the local leg's fields.
   Whether the version itself is then published is open question 11.
5. **Google's prices.** The carry-over row on the introductory prices: if the candidate's
   local leg runs on or after 2026-11-30, the row of `gemini-4-argon` is read again from
   the provider's page and the build's row, its date and ADR 0026 changed with it. The row
   of `gemini-3.8-flash` cannot be read before 2026-12-31 and is re-owned, with a line, to
   the first live run on or after that date.
6. **The model's licence, read for hosting.** The first step of the carry-over row on
   hosting the model's files: what the model's licence asks of someone who redistributes
   its files (the copy that travels with them, the uses it forbids and how a user is told,
   the acknowledgement it asks), written into the row as a reading, not legal advice. The
   rest of that row is open question 8.
7. **The rows and the note.** Every row of the table below closed, narrowed or re-owned
   with its line; `docs/plans/m4-note.md` with the clauses of the spec's row each beside
   the tests and records that show it, the spend, what the owner decided, and what a reader
   should not assume; the tag `m4`.

- [ ] Done when:
  `evidence::the_loops_record_names_a_commit_of_this_tree_and_holds_only_aggregates`,
  `evidence::the_loops_tool_record_holds_rungs_1_and_2_and_its_mark`,
  `tool_records::recipes::the_record_of_remove_checked_is_what_it_gives`,
  `evidence::the_release_record_names_a_candidate_whose_commit_is_of_this_tree`,
  `scripts/tests/test-tool-records.sh` and `scripts/tests/test-notes.sh` passing with the
  new record and note, a `prerelease` run ended in success on the candidate's commit with
  the local leg's file attached (named in `docs/results/m4-release.md`), every row of the
  table below closed or re-owned, and the note written.

### What the owner provides (N)

- The Google key answering again. In milestone 3's last gate run, 195 of 200 runs on
  `gemini-3.8-flash` ended in the provider's 402, a limit on the key. "A live call to each
  provider" cannot pass while it stands.
- If the owner wishes: three to five pictures of their own, each with a phrase, where one
  pass leaves something behind (rascal's case was a dog's collar whose buckle stayed), in a
  folder on the Mac. Without them the three cases by hand run on synthetic pictures.

## Carry-over rows owned by milestone 4, and who takes each

| Row (by its subject in `docs/carry-over.md`) | What happens to it |
|---|---|
| Say on the terminal when a run was not checked | Taken by L1 (item 9) |
| Keep the cause when a check fails | Taken by L1 (item 8) |
| A check cannot receive the variables it needs | Taken by L1 (item 2): a nested call's arguments |
| The documented per-call cap for a check is one cent | Taken by L1 (item 6): the guide's real figures |
| `remove_prompt`'s `check` argument (the dropped row on what a manifest says, which points here) | Answered by L1: checking is the loop's, an action of its own; `remove_prompt` gains no argument |
| `invert`'s `to`: a mask on a cut canvas or a crop carried up before it is inverted | The crop half taken by P1 (item 3), which the located retry needs. The cut half re-owned to "the first shipped recipe that inverts a mask on a cut": no recipe of milestone 4 does, since every attempt of the loop fills the picture it was given |
| An executor built with a digest alone | Taken by F1 (item 4) |
| `whizbang lab run` prints nothing while a round runs | Taken by F1 (item 1) |
| `whizbang lab run`'s "started now" | Taken by F1 (item 2) |
| The pairs page: a picture's size tells its arm; backspace | Taken by F1 (item 3) |
| The Google rows of prices are introductory | Taken by N1 (item 5) for the row whose date has come; the other re-owned to the first live run on or after 2026-12-31 |
| Hosting the model's files for customers | Its first step, the reading of the licence, taken by N1 (item 6). The rest re-owned to milestone 6 if the owner says yes to open question 8: the first release fetches SAM 3 from its maker with the user's own sign-in, and milestone 6 is where a model first runs on a host that is not the user's machine |

**Rows milestone E owns that milestone 4 leans on.** They stay E's, but for the first.

| Row | How milestone 4 leans on it |
|---|---|
| Details first is still `check`'s default | Taken from E by A1, because the loop calls `check` on every attempt. Gating a fixed details first on fresh data stays E's |
| The Google arm of the gate, once the key answers | The same key must answer for the candidate's live call to Google |
| The blinded picking rounds of milestone 3 | The loop's record says of its visual quality what the five fill recipes' say: not yet confirmed by a person. F1's fix to the pairs page should land before the owner picks |
| The model-based recipes up rungs 3 and 4; a fresh confirmation set | The loop joins them: milestone 4 gives it rungs 1 and 2 and the mark experimental |
| `check`'s `tiles` and second look | The loop's check uses neither; both stay off |
| A resumed matrix or gate never runs a judge's failed cell again | Not leaned on; N1's lab has `--retry-failed` |

**Rows with no milestone that bound milestone 4.** "Linux runtime extra", deferred by the
owner on 2026-11-18: it is why the Linux archive runs no local model. Milestone 5's
acceptance check of the loop by hand in the playground: its three cases are run from the
command line in N1, and stay milestone 5's for the playground.

## Commit strategy

- One task is one branch and one pull request, built tests first, checked by the reviewer
  the review rule names, merged by the orchestrator with a merge commit once `ci` and both
  scans pass. A builder pushes once, when the whole task is done, after one full `just
  check` on the finished tree.
- An ADR is in the pull request of the task that names it. Ported behaviour names its
  source in the commit message: the repo, the path, the pinned commit.
- Every state of `main` between tasks passes `just check`, and at every one of them
  `remove_prompt` and `squint check` work as they did or as A1 says.
- **Rollback checkpoints.** The tag `m3`, which the orchestrator makes on 0c75eedd before
  wave 1 (origin's tags end at `m2`). `m4-loop`, on the merge of L2: the loop whole, against
  fakes and SAM 3, nothing released. Each candidate tag, `v<version>-rc.<n>`: a state whose
  archives exist. `m4` at the exit check. A pushed tag is never moved: a bad candidate is
  followed by the next number.
- **If a task shows a decision here is wrong,** the decision is changed in this file first,
  in its own pull request; a change of scope or architecture gets the review rounds first.
  A test is never bent to fit.

| Tasks | To undo |
|---|---|
| A1 | Revert the merge: details first is the default again. A run can also ask for it with `--details first` without any revert |
| L1, L2 | Revert L2's merge, then L1's. `remove_prompt` and `check` never depended on the loop. A store the loop wrote to holds records of `run/6`, which a build without L1 does not read: the live runs use stores made for them |
| P1 | Revert after L2. Findings are an addition to what `check` hands back, and `from` an argument no shipped recipe uses by default |
| R1 | Revert the merge: CI's `release` job is the smoke alone again |
| R2 | Revert the merge. A draft candidate on GitHub is deleted by the orchestrator; a published release is not deleted but followed by a new version |
| F1 | Revert the merge |
| N1 | Records and a note; a candidate that failed stays as its tag and is superseded |

## Spend

All on approved recipients, on the owner's Mac, by the orchestrator, each run's settled
spend in its results file and the total in the note. A line that passes twice its estimate
stops and is said.

| Step | What is called | Estimate |
|---|---|---|
| A1, P1, L1, L2, R1, R2, F1 | Nothing paid: fakes, and SAM 3 on the Mac under `just test-models` | 0 |
| N1, item 1: the reference arm | `remove_prompt`: 24 fills | 1.00 |
| N1, item 1: the loop with Haiku | 24 pictures, at most 3 fills and 3 checks each | at most 3.20 |
| N1, item 1: the loop with Terra | The same with the dearer judge | at most 4.10 |
| N1, item 1: the loop on a first mask too tight | The same as the Haiku arm; most items retry | at most 3.20 |
| N1, item 2: three cases by hand, and the owner's pictures | About ten runs of the loop | 1.50 |
| N1, item 4: the local leg of a candidate | One check by each of three judges, one fill, one loop; about 0.25 a candidate, three candidates allowed for | 0.75 |
| The owner's own tries of the loop | About thirty runs | 5.00 |
| One whole rerun of item 1 after a fix | | 11.50 |
| | **Total** | **about 30 dollars, of the 100** |

The rates are milestone 3's: Bria Eraser is 0.04 dollars a fill; a decision with no details
measured first settled about 0.0033 dollars with `claude-haiku-5-5`, 0.023 with
`gemini-3.8-flash` and 0.017 with `gpt-6.1-terra` (`docs/baseline/manifest.toml`). What a
loop reserves is far more than what it settles (L1, item 6), so the store's config for
these runs sets the call cap at 3.50 dollars, and the record says so.

## Dependencies

```
A1 ──► L1 ──┐
            ├──► L2 ──┐
P1 ─────────┘         │
R1 ──► R2 ────────────┼──► N1
F1 ───────────────────┘
```

- A1 before L1: the loop's tests and its cost are those of the default `check`.
- P1 beside A1 and L1: it changes what `check` and the runner offer, not what the loop does.
- L2 after both: it is the first thing that reads a place and runs a recipe from a step.
- R1 and R2 beside the whole loop strand: an archive of `main` at any commit is a valid
  subject for them.
- N1 last: its candidate must hold the loop, and its record runs the loop live.

## Risks

1. **The judge is not good enough to steer a paid retry.** Squint's judges agreed with the
   labels about 61% of the time, and a third of verdicts were `unsure`
   (`docs/results/m2c-evaluation.md`). A wrong `fail` buys a fill nobody needed; a wrong
   `pass` hides a leftover. The loop bounds the first (three attempts; an `unsure` with no
   place ends them) and cannot prevent the second. Its mark is experimental, N1 reports how
   often each arm retried and what the retry did, and milestone E takes it up the ladder.
2. **The synthetic set rarely fails.** Milestone 3's masks covered the drawn defect whole,
   so the loop may pass at first on nearly every item. The arm with a first mask too tight
   and the cases by hand exist to exercise retries; the owner's own pictures are the better
   evidence.
3. **The loop fills all four levels.** Nothing can call it: a user's recipe that names it
   is refused when read, and a later tool that wants a checked removal as one step of
   something larger will need the limit raised, which is a change to the spec.
4. **The reservation of three attempts is over common call caps.** Fitting the attempts to
   the cap keeps work running and can leave a caller with fewer attempts than it expected;
   the result and the terminal say how many were allowed.
5. **A workflow that runs on a tag is first run for real at the first candidate.** R1's
   install check runs on every pull request to shrink what is left to find; N1 allows for
   three candidates.
6. **The Google key's limit fails the local leg.** It is the owner's to clear; until then
   no candidate can pass a live call to each provider.
7. **A Mac's browser marks a downloaded archive, and macOS refuses to run an unsigned
   binary so marked.** Without Apple's notarisation the guide tells a user to download with
   a command that sets no mark. CI cannot show this either way.
8. **Linux "runs" without any model.** A person who reads "installs and runs on both
   targets" may expect `remove-prompt` on Linux. The archive's own guide, the refusal's
   sentence and the note say otherwise in plain words.
9. **Test time.** The loop's tests run several nested runs each; the fixture keeps them to
   64-pixel pictures and scripted models, and L1 reports its seconds.
10. **The budgets are estimates.** A pull request that passes about 2,500 changed lines
    splits at its named seam, and the owner is told the count.

## Open questions

Each is phrased so the owner can answer yes or no. The recommended answer is the one the
tasks above assume; a no changes the task named.

1. **The pre-release checks in two parts.** I need your yes to run the model tier and the
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
4. **Rascal's retry rules.** I need your yes to keep them: at most three attempts, the
   first included; after a `fail` or an `unsure` that names places, look again inside those
   places and fill again; after a `fail` that names none, widen the mask once; after an
   `unsure` that names none, stop. Recommended: yes. (L1, L2)
5. **Unchecked rather than refused.** I need your yes that, by default, the loop runs once
   unchecked and says why when no judge is ready, with `--check required` for a caller who
   would sooner be refused. Recommended: yes. (L1)
6. **When every attempt fails, the loop still completes.** I need your yes that it hands
   back its best attempt with the verdict `fail` or `unsure`, and is an error only when no
   removal was made. No means it ends as an error and hands back no picture. Recommended:
   yes. (L1)
7. **Attempts fitted to the call cap.** I need your yes that a loop whose three attempts
   would reserve more than the call cap runs with as many attempts as the cap covers, and
   says so. No means it is refused, naming the cap. Recommended: yes. (L1)
8. **Hosting SAM 3's files moves to milestone 6.** I need your yes that the first release
   fetches the model from its maker with the user's own sign-in, as today, and that hosting
   the files for customers is designed with milestone 6, after the licence is read in this
   one. Recommended: yes. (N1)
9. **No notarisation by Apple in this milestone.** I need your yes to ship the Mac's
   binary unsigned by a developer identity, with the guide's download command, and a
   carry-over row for notarisation, which needs a paid Apple developer account of yours.
   Recommended: yes. (R1)
10. **The loop is code, and nothing calls it.** I need your yes that the loop is written
    in Rust (recipe files gain no branch or repeat), that a recipe may not name a loop as a
    step, and that calls stay four levels deep. Recommended: yes. (L1)
11. **The first version.** I need your yes to name it `0.1.0`, with candidates
    `v0.1.0-rc.1` onward, and to publish `v0.1.0` on the repo's GitHub releases, marked as
    a pre-release, once a candidate passes. No to publishing means milestone 4 ends at a
    passed candidate, whose archives stay on a draft. Recommended: yes. (R2, N1)
12. **A cancelled loop's finished attempts.** I need your yes that, for now, a picture
    made by an attempt that completed before the loop was cancelled is named in the record
    and cannot be taken by a later call. No means L1 also builds a way to keep it.
    Recommended: yes. (L1)
13. **The check is `source-fidelity`, and it hands back its places.** I need your yes that
    the loop checks with squint's `source-fidelity` recipe under a job written from the
    phrase, and that `check` hands its caller the places it cited as a new kind of result,
    since a diagnostic may not be read as an output. Recommended: yes. (L1, P1)

## What the owner is told, not asked

- **Eight pull requests** of at most about 2,500 changed lines; four of them run side by
  side in the first wave.
- **Details first is turned off as the default now**, ahead of milestone E, because the loop
  calls `check`; it stays selectable with `--details first`.
- **The tag `m3`** is made by the orchestrator on 0c75eedd before the first wave.

### What milestone 5, E and later owe after 4

| Item | Left for |
|---|---|
| The loop in the playground, with its three cases by hand | Milestone 5 |
| The loop up rungs 3 and 4; its visual quality confirmed by a person; a threshold for the segmenter inside a crop, if the live step calls for it | Milestone E |
| A local model on Linux, and so `remove_prompt` and the loop there, through a remote segmenter | Milestone 6 |
| Hosting the model's files for customers | Milestone 6, if open question 8 is yes |
| Notarisation of the Mac's binary by Apple | Its carry-over row, if open question 9 is yes |
| The second Google row of introductory prices | The first live run on or after 2026-12-31 |
| `invert` of a mask on a cut canvas | The first shipped recipe that needs it |
| Holding the `whizbang` name on crates.io | "Later", as the carry-over list has it; untouched here |

## Review rounds

None yet. This plan gets at least two rounds by the independent reviewing model before
building starts (`SPEC.md` section 14, "Review before building"); the outcome of each is
recorded here.
